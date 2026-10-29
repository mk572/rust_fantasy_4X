//! Playing with the network, and learning from it: rollouts, behaviour cloning and PPO
//! (`docs/learning-ai.md` 6.1, 7 and 9).

use burn::module::Module;
use burn::optim::{AdamConfig, GradientsParams, ModuleOptimizer};
use burn::prelude::*;
use burn::tensor::activation::softmax;
use rand::Rng;
use rand_pcg::Pcg64Mcg;

use f4x_core::battle::{Battle, BattleCommand, Side};
use f4x_core::content::Content;

use crate::act::{self, AIM_VOCAB, UNIT_VOCAB, WIZARD_VOCAB};
use crate::agent::{Driver, Pending, Request};
use crate::bank::Bank;
use crate::env::BattleEnv;
use crate::net::{log_probs, matrix, Inputs, PolicyNet};
use crate::obs::{Brief, ObsSpec, MAX_SPELLS, MAX_UNITS};
use crate::policy;
use crate::reward::{reward, reward_with_budget};
use crate::scenario::Odds;

// ---- choices and samples ------------------------------------------------------------

/// What the network chose for one decision.
#[derive(Debug, Clone, Copy)]
pub struct Choice {
    /// A unit order, or the wizard's choice.
    pub index: usize,
    pub aim: usize,
    pub mana_bin: usize,
    /// Log-probability of the whole choice, and the value of the state.
    pub logp: f32,
    pub value: f32,
}

/// A decision taken in a rollout, with what PPO needs.
#[derive(Debug, Clone)]
pub struct Sample {
    pub pending: Pending,
    pub choice: Choice,
    /// Whether the advice was shown to the network.
    pub advice_on: bool,
    pub advantage: f32,
    pub ret: f32,
}

fn one_hot(n: usize, at: Option<usize>) -> Vec<f32> {
    let mut v = vec![0.0; n];
    if let Some(i) = at {
        v[i] = 1.0;
    }
    v
}

fn mask_row(m: &[bool]) -> Vec<f32> {
    m.iter().map(|&b| f32::from(b)).collect()
}

/// A categorical draw (or the best, when `greedy`) from log-probabilities, with the log-prob.
fn draw(logp: &[f32], greedy: bool, rng: &mut Pcg64Mcg) -> (usize, f32) {
    if greedy {
        // An empty slice has nothing to draw; index 0 with the lowest log-prob.
        return logp
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map_or((0, f32::NEG_INFINITY), |(i, &lp)| (i, lp));
    }
    let mut u: f32 = rng.gen();
    let mut last = 0;
    for (i, &lp) in logp.iter().enumerate() {
        let p = lp.exp();
        if p > 0.0 {
            last = i;
            if u < p {
                return (i, lp);
            }
            u -= p;
        }
    }
    (last, logp[last])
}

fn to_f32s(t: Tensor<2>) -> Vec<Vec<f32>> {
    let dims = t.dims();
    let v = t.into_data().try_to_vec::<f32>().unwrap_or_default();
    v.chunks(dims[1]).map(|c| c.to_vec()).collect()
}

// ---- inference ----------------------------------------------------------------------

/// Choose for a batch of pending decisions. `advice_on[i]` says whether the advice of decision
/// `i` is shown. Inference runs on the network's plain (no gradient) copy.
pub fn choose(
    net: &PolicyNet,
    spec: &ObsSpec,
    device: &Device,
    pendings: &[&Pending],
    advice_on: &[bool],
    greedy: bool,
    rng: &mut Pcg64Mcg,
) -> Vec<Choice> {
    let mut out = vec![
        Choice {
            index: 0,
            aim: 0,
            mana_bin: 0,
            logp: 0.0,
            value: 0.0
        };
        pendings.len()
    ];
    let units: Vec<usize> = (0..pendings.len())
        .filter(|&i| matches!(pendings[i].request, Request::Unit { .. }))
        .collect();
    let wizards: Vec<usize> = (0..pendings.len())
        .filter(|&i| matches!(pendings[i].request, Request::Wizard { .. }))
        .collect();
    if !units.is_empty() {
        let (x, actor, advice, mask) = unit_inputs(spec, device, pendings, &units, advice_on);
        let t = net.trunk(&x);
        let lp = to_f32s(log_probs(net.unit_logits(&t, actor, advice), mask));
        let values = t_values(net, &t);
        for (row, &i) in units.iter().enumerate() {
            let (index, logp) = draw(&lp[row], greedy, rng);
            out[i] = Choice {
                index,
                aim: 0,
                mana_bin: 0,
                logp,
                value: values[row],
            };
        }
    }
    if !wizards.is_empty() {
        let obs: Vec<&crate::obs::Observation> =
            wizards.iter().map(|&i| &pendings[i].obs).collect();
        let x = Inputs::new(&obs, spec.unit_features(), device);
        let t = net.trunk(&x);
        let choice_mask = matrix(
            wizards
                .iter()
                .map(|&i| match &pendings[i].request {
                    Request::Wizard { options, .. } => mask_row(&options.choice),
                    _ => unreachable!(),
                })
                .collect(),
            WIZARD_VOCAB,
            device,
        );
        let mut clp = to_f32s(log_probs(net.wizard_logits(&t), choice_mask));
        if !greedy {
            for (row, &i) in clp.iter_mut().zip(&wizards) {
                if let Request::Wizard { options, .. } = &pendings[i].request {
                    mix_in_uniform(row, &options.choice);
                }
            }
        }
        let values = t_values(net, &t);
        let picks: Vec<(usize, f32)> = clp.iter().map(|r| draw(r, greedy, rng)).collect();
        // The aim and mana of those that cast.
        let spell_rows: Vec<Vec<f32>> = picks
            .iter()
            .map(|&(c, _)| one_hot(MAX_SPELLS, (c >= act::SPELL).then(|| c - act::SPELL)))
            .collect();
        let spell = matrix(spell_rows, MAX_SPELLS, device);
        let aim_mask = matrix(
            wizards
                .iter()
                .zip(&picks)
                .map(|(&i, &(c, _))| match &pendings[i].request {
                    Request::Wizard { options, .. } if c >= act::SPELL => {
                        mask_row(&options.aims[c - act::SPELL])
                    }
                    _ => {
                        let mut m = vec![0.0; AIM_VOCAB];
                        m[0] = 1.0;
                        m
                    }
                })
                .collect(),
            AIM_VOCAB,
            device,
        );
        let mana_mask = matrix(
            wizards
                .iter()
                .zip(&picks)
                .map(|(&i, &(c, _))| match &pendings[i].request {
                    Request::Wizard { options, .. } if c >= act::SPELL => {
                        vec![1.0, f32::from(options.more_mana[c - act::SPELL])]
                    }
                    _ => vec![1.0, 0.0],
                })
                .collect(),
            2,
            device,
        );
        let alp = to_f32s(log_probs(net.aim_logits(&t, spell.clone()), aim_mask));
        let mlp = to_f32s(log_probs(net.mana_logits(&t, spell), mana_mask));
        for (row, &i) in wizards.iter().enumerate() {
            let (c, clp) = picks[row];
            let (mut aim, mut bin, mut lp) = (0, 0, clp);
            if c >= act::SPELL {
                let (a, alp) = draw(&alp[row], greedy, rng);
                let (m, mlp) = draw(&mlp[row], greedy, rng);
                (aim, bin, lp) = (a, m, clp + alp + mlp);
            }
            out[i] = Choice {
                index: c,
                aim,
                mana_bin: bin,
                logp: lp,
                value: values[row],
            };
        }
    }
    out
}

/// The chance, in a wizard's choice, of taking an allowed option at random whatever the
/// network says: it keeps fleeing and casting tried while the network is still learning when
/// they pay (`docs/learning-ai.md` 6.2). It is part of the policy that is trained, so the
/// log-probabilities stay exact.
pub const WIZARD_EPS: f32 = 0.1;

fn mix_in_uniform(logp: &mut [f32], allowed: &[bool]) {
    let n = allowed.iter().filter(|&&a| a).count().max(1) as f32;
    for (lp, &ok) in logp.iter_mut().zip(allowed) {
        *lp = if ok {
            ((1.0 - WIZARD_EPS) * lp.exp() + WIZARD_EPS / n).ln()
        } else {
            -1.0e9
        };
    }
}

fn t_values(net: &PolicyNet, t: &crate::net::Trunk) -> Vec<f32> {
    net.value(t)
        .into_data()
        .try_to_vec::<f32>()
        .unwrap_or_default()
}

/// The tensors for a set of unit decisions.
fn unit_inputs(
    spec: &ObsSpec,
    device: &Device,
    pendings: &[&Pending],
    rows: &[usize],
    advice_on: &[bool],
) -> (Inputs, Tensor<2>, Tensor<2>, Tensor<2>) {
    let obs: Vec<&crate::obs::Observation> = rows.iter().map(|&i| &pendings[i].obs).collect();
    let x = Inputs::new(&obs, spec.unit_features(), device);
    let mut actor = Vec::new();
    let mut advice = Vec::new();
    let mut mask = Vec::new();
    for &i in rows {
        let Request::Unit {
            slot,
            mask: m,
            advice: a,
            ..
        } = &pendings[i].request
        else {
            unreachable!()
        };
        actor.push(one_hot(MAX_UNITS, Some(*slot)));
        advice.push(one_hot(UNIT_VOCAB, a.filter(|_| advice_on[i])));
        mask.push(mask_row(m));
    }
    (
        x,
        matrix(actor, MAX_UNITS, device),
        matrix(advice, UNIT_VOCAB, device),
        matrix(mask, UNIT_VOCAB, device),
    )
}

/// A brief such as a strategic AI might give: how much the units matter against winning, how
/// much mana may be spent (none given, none at all, or a few spells' worth), and whether the
/// wizard may flee.
pub fn random_brief(rng: &mut Pcg64Mcg) -> Brief {
    let mana_budget = match rng.gen_range(0..10) {
        0..=4 => None,
        5 => Some(0),
        _ => Some([8, 20, 40, 80, 200][rng.gen_range(0..5)]),
    };
    Brief {
        mana_budget,
        stakes: [0.0, 0.25, 0.5, 0.75, 1.0][rng.gen_range(0..5)],
        may_flee: rng.gen_bool(0.85),
    }
}

// ---- rollouts -----------------------------------------------------------------------

/// Who plays against the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opponent {
    /// The same network plays both sides, and both sides learn.
    SelfPlay,
    /// The scripted battle AI plays the other side.
    Scripted,
}

pub struct RolloutConfig {
    pub envs: usize,
    pub opponent: Opponent,
    /// The chance that the advice is shown during an episode.
    pub advice_chance: f64,
    pub stakes: f32,
    pub gamma: f32,
    pub lambda: f32,
    pub step_cost: f32,
    pub greedy: bool,
    /// Against the scripted AI, always play the attackers (as an evaluation does).
    pub attacker_only: bool,
    /// Give every side a random brief (stakes, mana budget, whether it may flee).
    pub random_briefs: bool,
    /// Added to a side's reward for each different spell its wizard cast in the battle.
    pub spell_bonus: f32,
}

impl Default for RolloutConfig {
    fn default() -> Self {
        Self {
            envs: 16,
            opponent: Opponent::SelfPlay,
            advice_chance: 0.5,
            stakes: 0.5,
            gamma: 0.995,
            lambda: 0.95,
            step_cost: 0.0005,
            greedy: false,
            attacker_only: false,
            random_briefs: false,
            spell_bonus: crate::reward::SPELL_BONUS,
        }
    }
}

/// What a batch of finished battles came to, for logging.
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub battles: usize,
    /// Mean reward of the agent's sides.
    pub reward: f32,
    /// Wins of the agent against the scripted AI (when it plays it).
    pub wins: usize,
    pub games: usize,
    pub decisions: usize,
    /// The wizard's decisions: all, those to flee, those to cast.
    pub wizard_asks: usize,
    pub flees: usize,
    pub casts: usize,
    /// Mana the agent's wizards spent from their reserves, summed over the battles.
    pub mana_spent: i64,
    /// How often each spell (by its id) was cast.
    pub by_spell: std::collections::BTreeMap<String, usize>,
    /// By odds: (battles, wins, reward sum) for the agent's side as the attacker.
    pub by_odds: [(usize, usize, f32); 3],
}

struct Slot<'a> {
    driver: Driver<'a>,
    briefs: [Brief; 2],
    agents: [bool; 2],
    advice_on: bool,
    odds: usize,
    pending: Option<Pending>,
    steps: [Vec<Sample>; 2],
}

fn odds_slot(o: Odds) -> usize {
    match o {
        Odds::Even => 0,
        Odds::Overwhelming => 1,
        Odds::Overwhelmed => 2,
    }
}

/// Play `battles` battles with the network and return the samples for learning and the stats.
#[allow(clippy::too_many_arguments)]
pub fn rollout(
    net: &PolicyNet,
    spec: &ObsSpec,
    content: &Content,
    bank: &Bank,
    device: &Device,
    cfg: &RolloutConfig,
    first_seed: u64,
    battles: usize,
    rng: &mut Pcg64Mcg,
) -> Result<(Vec<Sample>, Stats), String> {
    let mut samples = Vec::new();
    let mut stats = Stats::default();
    let mut next_seed = first_seed;
    let mut slots: Vec<Slot> = Vec::new();
    let mut started = 0usize;
    let mut scripted = |b: &Battle, _s: Side| -> BattleCommand { policy::scripted_command(b) };
    loop {
        // Fill free slots with new battles.
        while slots.len() < cfg.envs && started < battles {
            let mut scenario = bank.pick(next_seed);
            scenario.seed = next_seed;
            next_seed += 1;
            let Ok(env) = BattleEnv::new(content, scenario) else {
                continue;
            };
            started += 1;
            let odds = odds_slot(env.scenario.odds);
            let agents = match cfg.opponent {
                Opponent::SelfPlay => [true, true],
                Opponent::Scripted => {
                    if cfg.attacker_only || rng.gen_bool(0.5) {
                        [true, false]
                    } else {
                        [false, true]
                    }
                }
            };
            let mut driver = Driver::new(env, spec, Brief::default());
            let mut briefs = [Brief::default(); 2];
            for side in [Side::Attacker, Side::Defender] {
                let brief = if cfg.random_briefs {
                    random_brief(rng)
                } else {
                    Brief {
                        may_flee: true,
                        stakes: cfg.stakes,
                        ..Brief::default()
                    }
                };
                briefs[side.index()] = brief;
                driver.set_brief(side, brief);
            }
            slots.push(Slot {
                driver,
                briefs,
                agents,
                advice_on: rng.gen_bool(cfg.advice_chance),
                odds,
                pending: None,
                steps: [Vec::new(), Vec::new()],
            });
        }
        if slots.is_empty() {
            break;
        }
        // Advance every slot to its next decision, or finish it.
        let mut finished = Vec::new();
        for (k, s) in slots.iter_mut().enumerate() {
            if s.pending.is_some() {
                continue;
            }
            match s.driver.advance(s.agents, &mut scripted)? {
                Some(p) => s.pending = Some(p),
                None => finished.push(k),
            }
        }
        for &k in finished.iter().rev() {
            let s = slots.swap_remove(k);
            finish(s, cfg, &mut samples, &mut stats);
        }
        // One batched choice for all the pending decisions.
        let idx: Vec<usize> = (0..slots.len())
            .filter(|&k| slots[k].pending.is_some())
            .collect();
        if idx.is_empty() {
            continue;
        }
        let pend: Vec<&Pending> = idx
            .iter()
            .filter_map(|&k| slots[k].pending.as_ref())
            .collect();
        let on: Vec<bool> = idx.iter().map(|&k| slots[k].advice_on).collect();
        let choices = choose(net, spec, device, &pend, &on, cfg.greedy, rng);
        for (j, &k) in idx.iter().enumerate() {
            let s = &mut slots[k];
            let Some(p) = s.pending.take() else {
                continue;
            };
            let c = choices[j];
            match &p.request {
                Request::Unit { .. } => s.driver.answer_unit(&p, c.index)?,
                Request::Wizard { .. } => s.driver.answer_wizard(&p, c.index, c.aim, c.mana_bin)?,
            }
            stats.decisions += 1;
            if matches!(p.request, Request::Wizard { .. }) {
                stats.wizard_asks += 1;
                stats.flees += usize::from(c.index == act::FLEE);
                stats.casts += usize::from(c.index >= act::SPELL);
                if let Some(&spell) = c
                    .index
                    .checked_sub(act::SPELL)
                    .and_then(|k| p.obs.spell_ids.get(k))
                {
                    *stats
                        .by_spell
                        .entry(content.rules.spells.get(spell).id.clone())
                        .or_default() += 1;
                }
            }
            let side = p.side.index();
            let advice_on = s.advice_on;
            s.steps[side].push(Sample {
                pending: p,
                choice: c,
                advice_on,
                advantage: 0.0,
                ret: 0.0,
            });
        }
    }
    stats.battles = started;
    if stats.games > 0 {
        stats.reward /= stats.games as f32;
    }
    Ok((samples, stats))
}

/// A battle has ended: score each agent side, work out advantages, and keep the samples.
fn finish(mut s: Slot, cfg: &RolloutConfig, samples: &mut Vec<Sample>, stats: &mut Stats) {
    let outcome = s.driver.env.outcome;
    for side in [Side::Attacker, Side::Defender] {
        if !s.agents[side.index()] {
            continue;
        }
        let brief = s.briefs[side.index()];
        let r = reward_with_budget(&outcome, side, brief.stakes, brief.mana_budget)
            + cfg.spell_bonus * outcome.unique_spells[side.index()] as f32;
        stats.games += 1;
        stats.reward += r;
        stats.mana_spent += i64::from(outcome.mana_spent[side.index()]);
        if cfg.opponent == Opponent::Scripted {
            stats.wins += usize::from(outcome.winner == Some(side));
        }
        if side == Side::Attacker {
            let e = &mut stats.by_odds[s.odds];
            e.0 += 1;
            e.1 += usize::from(outcome.winner == Some(side));
            e.2 += r;
        }
        let steps = std::mem::take(&mut s.steps[side.index()]);
        let n = steps.len();
        // Generalised advantage estimation; the only reward but the small step cost comes at
        // the end.
        let mut gae = 0.0f32;
        let mut done: Vec<Sample> = Vec::with_capacity(n);
        for (t, mut smp) in steps.into_iter().enumerate().rev() {
            let last = t + 1 == n;
            let reward_t = -cfg.step_cost + if last { r } else { 0.0 };
            let next_v = if last {
                0.0
            } else {
                done.last().map_or(0.0, |d: &Sample| d.choice.value)
            };
            let delta = reward_t + cfg.gamma * next_v - smp.choice.value;
            gae = delta + cfg.gamma * cfg.lambda * if last { 0.0 } else { gae };
            smp.advantage = gae;
            smp.ret = gae + smp.choice.value;
            done.push(smp);
        }
        samples.extend(done);
    }
}

// ---- learning -----------------------------------------------------------------------

pub struct Learner {
    pub net: PolicyNet,
    pub opt: ModuleOptimizer,
    pub device: Device,
}

#[derive(Debug, Clone, Copy)]
pub struct PpoConfig {
    pub lr: f64,
    pub clip: f32,
    pub value_coef: f32,
    pub entropy_coef: f32,
    pub epochs: usize,
    pub minibatch: usize,
}

impl Default for PpoConfig {
    fn default() -> Self {
        Self {
            lr: 3e-4,
            clip: 0.2,
            value_coef: 0.5,
            entropy_coef: 0.01,
            epochs: 3,
            minibatch: 128,
        }
    }
}

impl Learner {
    pub fn new(spec: &ObsSpec, device: &Device) -> Self {
        let device = device.clone().autodiff();
        Self {
            net: PolicyNet::new(spec.unit_features(), &device),
            opt: AdamConfig::new().init(),
            device,
        }
    }

    /// The network without gradients, for rollouts.
    pub fn plain(&self) -> PolicyNet {
        self.net.valid()
    }

    fn step(&mut self, lr: f64, loss: Tensor<1>) {
        let grads = GradientsParams::from_grads(loss.backward(), &self.net);
        let net = std::mem::replace(&mut self.net, PolicyNet::placeholder());
        self.net = self.opt.step(lr, net, grads);
    }

    /// One epoch of behaviour cloning on the unit decisions that have advice: learn to give
    /// the order the scripted AI gives, without being shown it. Returns the mean loss and the
    /// share of orders matched.
    pub fn clone_scripted(
        &mut self,
        spec: &ObsSpec,
        samples: &[&Sample],
        lr: f64,
        batch: usize,
    ) -> (f32, f32) {
        let (mut total, mut hit, mut count) = (0.0f32, 0usize, 0usize);
        for chunk in samples.chunks(batch) {
            let rows: Vec<&Pending> = chunk.iter().map(|s| &s.pending).collect();
            let all: Vec<usize> = (0..rows.len()).collect();
            let off = vec![false; rows.len()];
            let (x, actor, advice, mask) = unit_inputs(spec, &self.device, &rows, &all, &off);
            let labels: Vec<f32> = chunk.iter().map(|s| s.choice.index as f32).collect();
            let t = self.net.trunk(&x);
            let lp = log_probs(self.net.unit_logits(&t, actor, advice), mask);
            let idx = Tensor::<2, Int>::from_data(
                TensorData::new(
                    labels.iter().map(|&l| l as i64).collect::<Vec<_>>(),
                    [chunk.len(), 1],
                ),
                &self.device,
            );
            let picked = lp.clone().gather(1, idx);
            let ce = picked.mean().neg();
            let returns = vec_t(chunk.iter().map(|s| s.ret).collect(), &self.device);
            let value_loss = (self.net.value(&t) - returns).square().mean();
            let loss = ce.clone() + value_loss.mul_scalar(0.5);
            let value: f32 = ce.into_scalar::<f32>();
            total += value * chunk.len() as f32;
            let best: Vec<i64> = lp.argmax(1).into_data().iter::<i64>().collect();
            hit += best
                .iter()
                .zip(&labels)
                .filter(|(b, l)| **b as usize == **l as usize)
                .count();
            count += chunk.len();
            self.step(lr, loss);
        }
        (
            total / count.max(1) as f32,
            hit as f32 / count.max(1) as f32,
        )
    }

    /// A PPO update over the samples. Returns the mean policy loss, value loss and entropy.
    pub fn ppo(
        &mut self,
        spec: &ObsSpec,
        samples: &[Sample],
        cfg: &PpoConfig,
        rng: &mut Pcg64Mcg,
    ) -> (f32, f32, f32) {
        let mut adv: Vec<f32> = samples.iter().map(|s| s.advantage).collect();
        let mean = adv.iter().sum::<f32>() / adv.len().max(1) as f32;
        let var = adv.iter().map(|a| (a - mean).powi(2)).sum::<f32>() / adv.len().max(1) as f32;
        let sd = var.sqrt().max(1e-6);
        for a in &mut adv {
            *a = (*a - mean) / sd;
        }
        let (mut pl, mut vl, mut en, mut nb) = (0.0f32, 0.0f32, 0.0f32, 0usize);
        for _ in 0..cfg.epochs {
            let mut order: Vec<usize> = (0..samples.len()).collect();
            for i in (1..order.len()).rev() {
                order.swap(i, rng.gen_range(0..=i));
            }
            for chunk in order.chunks(cfg.minibatch) {
                let units: Vec<usize> = chunk
                    .iter()
                    .copied()
                    .filter(|&i| matches!(samples[i].pending.request, Request::Unit { .. }))
                    .collect();
                let wizards: Vec<usize> = chunk
                    .iter()
                    .copied()
                    .filter(|&i| matches!(samples[i].pending.request, Request::Wizard { .. }))
                    .collect();
                let mut terms: Vec<Tensor<1>> = Vec::new();
                let mut n_rows = 0usize;
                for group in [&units, &wizards] {
                    if group.is_empty() {
                        continue;
                    }
                    let (logp, ent, value) = self.evaluate(spec, samples, group);
                    let old = vec_t(
                        group.iter().map(|&i| samples[i].choice.logp).collect(),
                        &self.device,
                    );
                    let a = vec_t(group.iter().map(|&i| adv[i]).collect(), &self.device);
                    let ret = vec_t(
                        group.iter().map(|&i| samples[i].ret).collect(),
                        &self.device,
                    );
                    let ratio = (logp - old).exp();
                    let clipped = ratio.clone().clamp(1.0 - cfg.clip, 1.0 + cfg.clip);
                    let surr = (ratio * a.clone()).min_pair(clipped * a);
                    let policy_loss = surr.mean().neg();
                    let value_loss = (value - ret).square().mean();
                    let entropy = ent.mean();
                    pl += policy_loss.clone().into_scalar::<f32>();
                    vl += value_loss.clone().into_scalar::<f32>();
                    en += entropy.clone().into_scalar::<f32>();
                    nb += 1;
                    let w = group.len() as f32;
                    terms.push(
                        (policy_loss + value_loss.mul_scalar(cfg.value_coef)
                            - entropy.mul_scalar(cfg.entropy_coef))
                        .mul_scalar(w),
                    );
                    n_rows += group.len();
                }
                if terms.is_empty() {
                    continue;
                }
                let mut loss = terms.remove(0);
                for t in terms {
                    loss = loss + t;
                }
                self.step(cfg.lr, loss.div_scalar(n_rows as f32));
            }
        }
        let nb = nb.max(1) as f32;
        (pl / nb, vl / nb, en / nb)
    }

    /// The new log-probability, entropy and value of the samples `rows` (all of one kind).
    fn evaluate(
        &self,
        spec: &ObsSpec,
        samples: &[Sample],
        rows: &[usize],
    ) -> (Tensor<1>, Tensor<1>, Tensor<1>) {
        let pend: Vec<&Pending> = samples.iter().map(|s| &s.pending).collect();
        let on: Vec<bool> = samples.iter().map(|s| s.advice_on).collect();
        let dev = &self.device;
        if matches!(samples[rows[0]].pending.request, Request::Unit { .. }) {
            let (x, actor, advice, mask) = unit_inputs(spec, dev, &pend, rows, &on);
            let t = self.net.trunk(&x);
            let lp = log_probs(self.net.unit_logits(&t, actor, advice), mask);
            let idx = int_col(rows.iter().map(|&i| samples[i].choice.index).collect(), dev);
            let logp = lp.clone().gather(1, idx).squeeze_dim::<1>(1);
            let ent = (lp.clone().exp() * lp).sum_dim(1).squeeze_dim::<1>(1).neg();
            return (logp, ent, self.net.value(&t));
        }
        let obs: Vec<&crate::obs::Observation> = rows.iter().map(|&i| &pend[i].obs).collect();
        let x = Inputs::new(&obs, spec.unit_features(), dev);
        let t = self.net.trunk(&x);
        let mut cmask = Vec::new();
        let mut spell = Vec::new();
        let mut amask = Vec::new();
        let mut mmask = Vec::new();
        let mut cast = Vec::new();
        for &i in rows {
            let s = &samples[i];
            let Request::Wizard { options, .. } = &s.pending.request else {
                unreachable!()
            };
            cmask.push(mask_row(&options.choice));
            let k = (s.choice.index >= act::SPELL).then(|| s.choice.index - act::SPELL);
            spell.push(one_hot(MAX_SPELLS, k));
            cast.push(f32::from(k.is_some()));
            amask.push(k.map_or_else(
                || one_hot(AIM_VOCAB, Some(0)),
                |k| mask_row(&options.aims[k]),
            ));
            mmask.push(k.map_or_else(
                || vec![1.0, 0.0],
                |k| vec![1.0, f32::from(options.more_mana[k])],
            ));
        }
        let spell = matrix(spell, MAX_SPELLS, dev);
        let cmask = matrix(cmask, WIZARD_VOCAB, dev);
        let allowed = cmask.clone().sum_dim(1).clamp_min(1.0);
        let clp = log_probs(self.net.wizard_logits(&t), cmask.clone());
        // The same mixture as in `choose`.
        let mixed = clp.exp().mul_scalar(1.0 - WIZARD_EPS)
            + cmask.clone() * (allowed.recip().mul_scalar(WIZARD_EPS));
        let clp = mixed.clamp_min(1e-12).log() + cmask.sub_scalar(1.0).mul_scalar(1.0e9);
        let alp = log_probs(
            self.net.aim_logits(&t, spell.clone()),
            matrix(amask, AIM_VOCAB, dev),
        );
        let mlp = log_probs(self.net.mana_logits(&t, spell), matrix(mmask, 2, dev));
        let cast = vec_t(cast, dev);
        let pick = |lp: Tensor<2>, v: Vec<usize>| lp.gather(1, int_col(v, dev)).squeeze_dim::<1>(1);
        let logp = pick(
            clp.clone(),
            rows.iter().map(|&i| samples[i].choice.index).collect(),
        ) + (pick(alp, rows.iter().map(|&i| samples[i].choice.aim).collect())
            + pick(
                mlp,
                rows.iter().map(|&i| samples[i].choice.mana_bin).collect(),
            ))
            * cast;
        let ent = (clp.clone().exp() * clp)
            .sum_dim(1)
            .squeeze_dim::<1>(1)
            .neg();
        (logp, ent, self.net.value(&t))
    }
}

fn vec_t(v: Vec<f32>, device: &Device) -> Tensor<1> {
    let n = v.len();
    Tensor::from_data(TensorData::new(v, [n]), device)
}

fn int_col(v: Vec<usize>, device: &Device) -> Tensor<2, Int> {
    let n = v.len();
    Tensor::from_data(
        TensorData::new(v.into_iter().map(|x| x as i64).collect::<Vec<_>>(), [n, 1]),
        device,
    )
}

/// Not used by learning, kept so a softmax is at hand for probes.
pub fn probabilities(logits: Tensor<2>) -> Tensor<2> {
    softmax(logits, 1)
}

// ---- scripted data and evaluation ---------------------------------------------------

/// Play `battles` generated battles with the scripted AI on both sides and keep its unit
/// orders as samples to learn from (the wizard is asked and skips).
pub fn scripted_samples(
    spec: &ObsSpec,
    content: &Content,
    bank: &Bank,
    first_seed: u64,
    battles: usize,
) -> Result<Vec<Sample>, String> {
    let mut out: Vec<Sample> = Vec::new();
    let mut seed = first_seed;
    let mut played = 0;
    let mut none = |_: &Battle, _: Side| BattleCommand::EndTurn;
    while played < battles {
        let mut scenario = bank.pick(seed);
        scenario.seed = seed;
        seed += 1;
        let Ok(env) = BattleEnv::new(content, scenario) else {
            continue;
        };
        played += 1;
        let mut d = Driver::new(env, spec, Brief::default());
        let from = out.len();
        while let Some(p) = d.advance([true, true], &mut none)? {
            match &p.request {
                Request::Unit { advice, .. } => {
                    let Some(index) = *advice else {
                        return Err("the scripted order has no place in the vocabulary".into());
                    };
                    d.answer_unit(&p, index)?;
                    out.push(Sample {
                        choice: Choice {
                            index,
                            aim: 0,
                            mana_bin: 0,
                            logp: 0.0,
                            value: 0.0,
                        },
                        pending: p,
                        advice_on: false,
                        advantage: 0.0,
                        ret: 0.0,
                    });
                }
                Request::Wizard { .. } => d.answer_wizard(&p, act::SKIP, 0, 0)?,
            }
        }
        // What each side got: the return every decision of it is worth.
        let o = d.env.outcome;
        for smp in &mut out[from..] {
            smp.ret = reward(&o, smp.pending.side, 0.5);
        }
    }
    Ok(out)
}

/// How a network does against the scripted AI on fixed scenarios.
#[derive(Debug, Clone, Default)]
pub struct Evaluation {
    pub games: usize,
    pub wins: usize,
    pub reward: f32,
    /// (games, wins, reward) by odds: even, overwhelming, overwhelmed, with the network as the
    /// attacker.
    pub by_odds: [(usize, usize, f32); 3],
    /// The wizard's decisions, flees and casts, the mana spent, and each spell's casts.
    pub wizard_asks: usize,
    pub flees: usize,
    pub casts: usize,
    pub mana_spent: i64,
    pub by_spell: std::collections::BTreeMap<String, usize>,
}

#[allow(clippy::too_many_arguments)]
pub fn evaluate(
    net: &PolicyNet,
    spec: &ObsSpec,
    content: &Content,
    bank: &Bank,
    device: &Device,
    first_seed: u64,
    battles: usize,
    advice: bool,
    rng: &mut Pcg64Mcg,
) -> Result<Evaluation, String> {
    let cfg = RolloutConfig {
        envs: 16,
        opponent: Opponent::Scripted,
        advice_chance: if advice { 1.0 } else { 0.0 },
        greedy: true,
        attacker_only: true,
        ..RolloutConfig::default()
    };
    let (_, st) = rollout(
        net, spec, content, bank, device, &cfg, first_seed, battles, rng,
    )?;
    Ok(Evaluation {
        games: st.games,
        wins: st.wins,
        reward: st.reward,
        by_odds: st.by_odds,
        wizard_asks: st.wizard_asks,
        flees: st.flees,
        casts: st.casts,
        mana_spent: st.mana_spent,
        by_spell: st.by_spell,
    })
}

/// How fixed players do against the scripted AI on the same scenarios as [`evaluate`], as the
/// attacker: the scripted AI itself (`random` false: it plays as it advises) and a player that
/// picks uniformly among the allowed orders (`random` true; it never casts or flees).
pub fn baseline(
    spec: &ObsSpec,
    content: &Content,
    bank: &Bank,
    first_seed: u64,
    battles: usize,
    random: bool,
    rng: &mut Pcg64Mcg,
) -> Result<Evaluation, String> {
    let mut ev = Evaluation::default();
    let (mut seed, mut played) = (first_seed, 0);
    let mut scripted = |b: &Battle, _s: Side| policy::scripted_command(b);
    while played < battles {
        let mut scenario = bank.pick(seed);
        scenario.seed = seed;
        seed += 1;
        let Ok(env) = BattleEnv::new(content, scenario) else {
            continue;
        };
        played += 1;
        let odds = odds_slot(env.scenario.odds);
        let mut d = Driver::new(env, spec, Brief::default());
        while let Some(p) = d.advance([true, false], &mut scripted)? {
            match &p.request {
                Request::Unit { advice, mask, .. } => {
                    let index = if random {
                        let ok: Vec<usize> = (0..mask.len()).filter(|&i| mask[i]).collect();
                        ok[rng.gen_range(0..ok.len())]
                    } else {
                        advice.ok_or("no advice")?
                    };
                    d.answer_unit(&p, index)?;
                }
                Request::Wizard { .. } => d.answer_wizard(&p, act::SKIP, 0, 0)?,
            }
        }
        let o = d.env.outcome;
        let r = reward(&o, Side::Attacker, 0.5);
        ev.games += 1;
        ev.wins += usize::from(o.winner == Some(Side::Attacker));
        ev.reward += r;
        let e = &mut ev.by_odds[odds];
        e.0 += 1;
        e.1 += usize::from(o.winner == Some(Side::Attacker));
        e.2 += r;
    }
    ev.reward /= ev.games.max(1) as f32;
    Ok(ev)
}
