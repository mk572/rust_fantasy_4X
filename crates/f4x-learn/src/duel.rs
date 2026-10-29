//! Two networks against each other, in pairs of battles (`docs/PLANNED.md` item 25).
//!
//! A *pair* is one scenario played twice with the same dice, the networks swapping sides in the
//! second battle. The side with more wins takes the pair. When the wins are level (one each, or
//! two battles that hit the step limit), the tie-breaks decide, in order, over the two battles:
//! more enemy units killed, more own units surviving, more hit points at the end (own minus
//! enemy), less mana spent; if all are level the pair is a draw and each side gets half. A *match*
//! is a batch of pairs; whoever has more pair points wins it, and only a level batch is settled
//! by a coin flip.
//!
//! The networks play as the game would play them (`brain.rs`): greedy, no advice, the default
//! brief (may flee, stakes one half, no mana budget).

use burn::prelude::*;
use rand::Rng;
use rand_pcg::Pcg64Mcg;
use serde::{Deserialize, Serialize};

use f4x_core::battle::{Battle, BattleCommand, Side};
use f4x_core::content::Content;

use crate::act;
use crate::agent::{Driver, Pending, Request};
use crate::bank::Bank;
use crate::env::BattleEnv;
use crate::net::PolicyNet;
use crate::obs::{Brief, ObsSpec};
use crate::policy;
use crate::scenario::Odds;
use crate::train::choose;

/// What one battle came to, by *entry* (0 or 1: the two networks of the pair, not the sides).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GameRecord {
    /// The entry that won, or `None` for a battle that hit the step limit.
    pub winner: Option<usize>,
    /// Enemy units each entry destroyed.
    pub kills: [u32; 2],
    /// Units each entry lost (died).
    pub lost: [u32; 2],
    /// Units each entry had left (those that got away count, less the ones lost on the way).
    pub survivors: [u32; 2],
    /// Hit points each entry's units had left at the end.
    pub hit_points: [i32; 2],
    pub mana: [i32; 2],
    /// Wizard decisions to cast, and to flee.
    pub casts: [u32; 2],
    pub flees: [u32; 2],
    /// Commands the battle took.
    pub steps: u32,
}

/// What settled a pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decider {
    Wins,
    Kills,
    Survivors,
    HitPoints,
    Mana,
    /// Everything level: half a point each.
    Draw,
}

/// A scenario played twice: in `games[0]` entry 0 is the attacker, in `games[1]` entry 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairRecord {
    pub seed: u64,
    /// The scenario's odds class for the attacker: 0 even, 1 overwhelming, 2 overwhelmed.
    pub odds: u8,
    pub games: [GameRecord; 2],
    pub decided_by: Decider,
    /// Points of each entry: 1 for the pair, ½ each for a draw.
    pub points: [f32; 2],
}

/// The entry that plays `side` in game `game` of a pair.
pub fn entry_of(game: usize, side: Side) -> usize {
    (game + usize::from(side == Side::Defender)) % 2
}

/// The side `entry` plays in game `game` of a pair.
pub fn side_of(game: usize, entry: usize) -> Side {
    if (entry == 0) == (game == 0) {
        Side::Attacker
    } else {
        Side::Defender
    }
}

/// Settle a pair from its two battles.
pub fn settle(games: &[GameRecord; 2]) -> (Decider, [f32; 2]) {
    let sum = |f: &dyn Fn(&GameRecord) -> [i64; 2]| {
        let (a, b) = (f(&games[0]), f(&games[1]));
        [a[0] + b[0], a[1] + b[1]]
    };
    let wins = sum(&|g| {
        let mut w = [0i64; 2];
        if let Some(e) = g.winner {
            w[e] = 1;
        }
        w
    });
    let by = |d: Decider, a: i64, b: i64| match a.cmp(&b) {
        std::cmp::Ordering::Greater => Some((d, [1.0, 0.0])),
        std::cmp::Ordering::Less => Some((d, [0.0, 1.0])),
        std::cmp::Ordering::Equal => None,
    };
    let kills = sum(&|g| [g.kills[0].into(), g.kills[1].into()]);
    let alive = sum(&|g| [g.survivors[0].into(), g.survivors[1].into()]);
    let hp = sum(&|g| [g.hit_points[0].into(), g.hit_points[1].into()]);
    let mana = sum(&|g| [g.mana[0].into(), g.mana[1].into()]);
    by(Decider::Wins, wins[0], wins[1])
        .or_else(|| by(Decider::Kills, kills[0], kills[1]))
        .or_else(|| by(Decider::Survivors, alive[0], alive[1]))
        // Own minus enemy is the same comparison as own against enemy.
        .or_else(|| by(Decider::HitPoints, hp[0], hp[1]))
        // Less mana is better.
        .or_else(|| by(Decider::Mana, mana[1], mana[0]))
        .unwrap_or((Decider::Draw, [0.5, 0.5]))
}

/// The result of a batch of pairs: points, whether the coin was needed, and the winner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MatchResult {
    pub points: [f32; 2],
    pub coin_flip: bool,
    pub winner: usize,
}

/// Total the pairs; a level batch is settled by a coin flip from `rng`.
pub fn tally(pairs: &[PairRecord], rng: &mut impl Rng) -> MatchResult {
    let mut points = [0.0f32; 2];
    for p in pairs {
        points[0] += p.points[0];
        points[1] += p.points[1];
    }
    let (winner, coin_flip) = if points[0] > points[1] {
        (0, false)
    } else if points[1] > points[0] {
        (1, false)
    } else {
        (usize::from(rng.gen_bool(0.5)), true)
    };
    MatchResult {
        points,
        coin_flip,
        winner,
    }
}

struct Slot<'a> {
    driver: Driver<'a>,
    pair: usize,
    game: usize,
    odds: u8,
    pending: Option<Pending>,
    casts: [u32; 2],
    flees: [u32; 2],
}

fn odds_number(o: Odds) -> u8 {
    match o {
        Odds::Even => 0,
        Odds::Overwhelming => 1,
        Odds::Overwhelmed => 2,
    }
}

/// How many pairs are in play at once (two battles each).
const CHUNK: usize = 16;

fn record(s: &Slot) -> GameRecord {
    let o = s.driver.env.outcome;
    let mut g = GameRecord {
        winner: o.winner.map(|w| entry_of(s.game, w)),
        steps: s.driver.env.steps,
        casts: s.casts,
        flees: s.flees,
        ..GameRecord::default()
    };
    for e in 0..2 {
        let me = side_of(s.game, e).index();
        let foe = side_of(s.game, 1 - e).index();
        g.kills[e] = o.lost[foe];
        g.lost[e] = o.lost[me];
        g.survivors[e] = if o.fled[me] {
            o.saved_by_fleeing[me]
        } else {
            o.sent[me].saturating_sub(o.lost[me])
        };
        g.hit_points[e] = o.end_hit_points[me];
        g.mana[e] = o.mana_spent[me];
    }
    g
}

/// Play `pairs` pairs between `nets` (entry 0 and entry 1) on scenarios from `bank`, with the dice
/// seeds from `first_seed` on; a battle still going after `step_limit` commands is stopped with no
/// winner. The same arguments give the same records.
#[allow(clippy::too_many_arguments)]
pub fn play_pairs(
    nets: [&PolicyNet; 2],
    spec: &ObsSpec,
    content: &Content,
    bank: &Bank,
    device: &Device,
    first_seed: u64,
    pairs: usize,
    step_limit: u32,
    rng: &mut Pcg64Mcg,
) -> Result<Vec<PairRecord>, String> {
    let mut out: Vec<PairRecord> = Vec::with_capacity(pairs);
    let mut seed = first_seed;
    let mut scripted = |b: &Battle, _s: Side| -> BattleCommand { policy::scripted_command(b) };
    while out.len() < pairs {
        let want = (pairs - out.len()).min(CHUNK);
        let mut slots: Vec<Slot> = Vec::new();
        let mut seeds: Vec<u64> = Vec::new();
        while seeds.len() < want {
            let mut scenario = bank.pick(seed);
            scenario.seed = seed;
            let this = seed;
            seed += 1;
            let (Ok(mut a), Ok(mut b)) = (
                BattleEnv::new(content, scenario.clone()),
                BattleEnv::new(content, scenario),
            ) else {
                continue;
            };
            a.step_limit = step_limit;
            b.step_limit = step_limit;
            let pair = seeds.len();
            seeds.push(this);
            for (game, env) in [a, b].into_iter().enumerate() {
                let odds = odds_number(env.scenario.odds);
                let mut driver = Driver::new(env, spec, Brief::default());
                for side in [Side::Attacker, Side::Defender] {
                    driver.set_brief(
                        side,
                        Brief {
                            may_flee: true,
                            stakes: 0.5,
                            ..Brief::default()
                        },
                    );
                }
                slots.push(Slot {
                    driver,
                    pair,
                    game,
                    odds,
                    pending: None,
                    casts: [0; 2],
                    flees: [0; 2],
                });
            }
        }
        let mut games: Vec<[Option<GameRecord>; 2]> = vec![[None, None]; want];
        let mut odds = vec![0u8; want];
        while !slots.is_empty() {
            let mut finished = Vec::new();
            for (k, s) in slots.iter_mut().enumerate() {
                if s.pending.is_some() {
                    continue;
                }
                match s.driver.advance([true, true], &mut scripted)? {
                    Some(p) => s.pending = Some(p),
                    None => finished.push(k),
                }
            }
            for &k in finished.iter().rev() {
                let s = slots.swap_remove(k);
                odds[s.pair] = s.odds;
                games[s.pair][s.game] = Some(record(&s));
            }
            for (e, net) in nets.into_iter().enumerate() {
                let idx: Vec<usize> = (0..slots.len())
                    .filter(|&k| {
                        slots[k]
                            .pending
                            .as_ref()
                            .is_some_and(|p| entry_of(slots[k].game, p.side) == e)
                    })
                    .collect();
                if idx.is_empty() {
                    continue;
                }
                let pend: Vec<&Pending> = idx
                    .iter()
                    .filter_map(|&k| slots[k].pending.as_ref())
                    .collect();
                let no_advice = vec![false; idx.len()];
                let choices = choose(net, spec, device, &pend, &no_advice, true, rng);
                for (j, &k) in idx.iter().enumerate() {
                    let s = &mut slots[k];
                    let Some(p) = s.pending.take() else {
                        continue;
                    };
                    let c = choices[j];
                    match &p.request {
                        Request::Unit { .. } => s.driver.answer_unit(&p, c.index)?,
                        Request::Wizard { .. } => {
                            s.casts[e] += u32::from(c.index >= act::SPELL);
                            s.flees[e] += u32::from(c.index == act::FLEE);
                            s.driver.answer_wizard(&p, c.index, c.aim, c.mana_bin)?;
                        }
                    }
                }
            }
        }
        for (i, g) in games.into_iter().enumerate() {
            let [Some(g0), Some(g1)] = g else {
                return Err("a battle of the pair did not finish".into());
            };
            let games = [g0, g1];
            let (decided_by, points) = settle(&games);
            out.push(PairRecord {
                seed: seeds[i],
                odds: odds[i],
                games,
                decided_by,
                points,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(winner: Option<usize>) -> GameRecord {
        GameRecord {
            winner,
            ..GameRecord::default()
        }
    }

    #[test]
    fn seats_swap_between_the_two_battles() {
        assert_eq!(side_of(0, 0), Side::Attacker);
        assert_eq!(side_of(0, 1), Side::Defender);
        assert_eq!(side_of(1, 0), Side::Defender);
        assert_eq!(side_of(1, 1), Side::Attacker);
        for g in 0..2 {
            for e in 0..2 {
                assert_eq!(entry_of(g, side_of(g, e)), e);
            }
        }
    }

    #[test]
    fn two_wins_or_a_win_and_a_draw_take_the_pair_without_tie_breaks() {
        let mut a = game(Some(0));
        a.mana = [0, 99];
        let b = game(Some(0));
        assert_eq!(settle(&[a.clone(), b]), (Decider::Wins, [1.0, 0.0]));
        // A win and a step-limit draw: the win takes it, whatever the tie-break numbers say.
        let mut d = game(None);
        d.kills = [0, 9];
        assert_eq!(settle(&[a, d]), (Decider::Wins, [1.0, 0.0]));
    }

    #[test]
    fn a_split_goes_through_the_tie_breaks_in_order() {
        let split = |f: &dyn Fn(&mut GameRecord, &mut GameRecord)| {
            let (mut a, mut b) = (game(Some(0)), game(Some(1)));
            f(&mut a, &mut b);
            settle(&[a, b])
        };
        // Kills first, over both battles.
        let r = split(&|a, b| {
            a.kills = [3, 0];
            b.kills = [0, 1];
            // Later tie-breaks point the other way and are not reached.
            a.survivors = [0, 9];
        });
        assert_eq!(r, (Decider::Kills, [1.0, 0.0]));
        // Kills level: survivors.
        let r = split(&|a, _| a.survivors = [1, 5]);
        assert_eq!(r, (Decider::Survivors, [0.0, 1.0]));
        // Then hit points.
        let r = split(&|a, b| {
            a.hit_points = [10, 4];
            b.hit_points = [3, 4];
        });
        assert_eq!(r, (Decider::HitPoints, [1.0, 0.0]));
        // Then mana: less is better.
        let r = split(&|a, _| a.mana = [30, 10]);
        assert_eq!(r, (Decider::Mana, [0.0, 1.0]));
        // All level: a draw.
        assert_eq!(split(&|_, _| {}), (Decider::Draw, [0.5, 0.5]));
    }

    #[test]
    fn two_step_limit_battles_go_to_the_tie_breaks() {
        let mut a = game(None);
        a.kills = [2, 1];
        assert_eq!(settle(&[a, game(None)]), (Decider::Kills, [1.0, 0.0]));
    }

    #[test]
    fn only_a_level_batch_needs_the_coin() {
        use rand::SeedableRng;
        let pair = |points: [f32; 2]| PairRecord {
            seed: 0,
            odds: 0,
            games: [GameRecord::default(), GameRecord::default()],
            decided_by: Decider::Draw,
            points,
        };
        let mut rng = Pcg64Mcg::seed_from_u64(1);
        let m = tally(&[pair([1.0, 0.0]), pair([0.5, 0.5])], &mut rng);
        assert_eq!((m.winner, m.coin_flip, m.points), (0, false, [1.5, 0.5]));
        let m = tally(&[pair([0.0, 1.0])], &mut rng);
        assert_eq!((m.winner, m.coin_flip), (1, false));
        let m = tally(&[pair([1.0, 0.0]), pair([0.0, 1.0])], &mut rng);
        assert!(m.coin_flip);
    }

    /// The same network on both seats plays complete pairs, the records add up, and the same
    /// arguments give the same records.
    #[test]
    fn a_network_plays_pairs_against_itself_and_the_records_repeat() {
        use rand::SeedableRng;
        let content =
            f4x_data::load_content_layers(&[std::path::Path::new("../../content")]).unwrap();
        let spec = ObsSpec::new(&content);
        let device = Device::flex();
        let net = PolicyNet::new(spec.unit_features(), &device);
        let mut bank = Bank::default();
        for seed in 0..6 {
            bank.scenarios
                .push(crate::scenario::generate(&content, seed));
        }
        let mut rng = Pcg64Mcg::seed_from_u64(3);
        let run = |rng: &mut Pcg64Mcg| {
            play_pairs(
                [&net, &net],
                &spec,
                &content,
                &bank,
                &device,
                100,
                1,
                60,
                rng,
            )
            .unwrap()
        };
        let a = run(&mut rng);
        assert_eq!(a.len(), 1);
        for p in &a {
            assert!((p.points[0] + p.points[1] - 1.0).abs() < 1e-6);
            for g in &p.games {
                // Units killed by one entry are the units the other lost.
                assert_eq!(g.kills[0], g.lost[1]);
                assert_eq!(g.kills[1], g.lost[0]);
                assert!(g.steps > 0);
            }
        }
        assert_eq!(run(&mut rng), a, "a repeat gave other records");
    }
}
