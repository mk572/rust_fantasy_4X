//! The learning environment for the AIs of `docs/learning-ai.md`: battle scenarios and the
//! battle environment (task 169), and later the networks and training.

pub mod act;
pub mod agent;
pub mod bank;
#[cfg(feature = "train")]
pub mod brain;
pub mod build;
pub mod env;
#[cfg(feature = "train")]
pub mod net;
pub mod obs;
pub mod policy;
pub mod reward;
pub mod scenario;
#[cfg(feature = "train")]
pub mod train;

pub use build::build;
pub use env::{BattleEnv, Outcome, Step};
pub use scenario::{generate, BattleScenario};

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::battle::Side;
    use rand::SeedableRng;
    use rand_pcg::Pcg64Mcg;
    use std::path::Path;

    fn content() -> f4x_core::content::Content {
        f4x_data::load_content_layers(&[Path::new("../../content")]).expect("content/ loads")
    }

    /// Play a scenario to the end with random commands for both sides.
    fn play_random(content: &f4x_core::content::Content, seed: u64) -> BattleEnv {
        let scenario = generate(content, seed);
        let mut env =
            BattleEnv::new(content, scenario).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let mut rng = Pcg64Mcg::seed_from_u64(seed);
        while !env.done() {
            let actions = env.actions();
            assert!(!actions.is_empty(), "seed {seed}: no action to take");
            let c = policy::random_command(&actions, &mut rng);
            env.step(c.clone())
                .unwrap_or_else(|e| panic!("seed {seed}: {c:?} was offered but {e}"));
        }
        env
    }

    #[test]
    fn random_play_finishes_generated_battles_without_a_refused_command() {
        let content = content();
        let mut winners = [0u32; 2];
        let (mut cast, mut unique, mut dear, mut worn) = (0, 0, 0, 0);
        for seed in 0..24 {
            let env = play_random(&content, seed);
            assert!(
                env.steps < env::STEP_LIMIT,
                "seed {seed} hit the step limit"
            );
            let w = env.outcome.winner.expect("a winner");
            winners[w.index()] += 1;
            cast += i32::from(env.outcome.mana_spent.iter().sum::<i32>() > 0);
            for side in 0..2 {
                let known = [&env.scenario.attacker_wizard, &env.scenario.defender_wizard][side]
                    .as_ref()
                    .map_or(0, |w| w.spells.len());
                assert!(
                    env.outcome.unique_spells[side] as usize <= known,
                    "seed {seed}: more different spells cast than known"
                );
            }
            unique += env.outcome.unique_spells.iter().sum::<u32>();
            let o = &env.outcome;
            for i in 0..2 {
                // A unit is worth at least one, so value is at least the count.
                assert!(o.sent_value[i] >= o.sent[i] as f32 - 1e-3, "seed {seed}");
                assert!(o.lost_value[i] >= o.lost[i] as f32 - 1e-3, "seed {seed}");
                dear += i32::from(o.sent_value[i] > o.sent[i] as f32 + 0.5);
                worn += i32::from(o.standing_hurt_value[i] > 0.0);
            }
        }
        assert!(winners[0] > 0 && winners[1] > 0, "{winners:?}");
        assert!(cast > 0, "no battle ever cast a spell");
        assert!(unique > 0, "no different spell was ever counted");
        assert!(dear > 0, "no unit was ever worth more than one");
        assert!(worn > 0, "no unit was ever left below half health");
    }

    #[test]
    fn a_scenario_replays_the_same_battle() {
        let content = content();
        let a = play_random(&content, 5);
        let b = play_random(&content, 5);
        assert_eq!(a.outcome, b.outcome);
        assert_eq!(a.steps, b.steps);
        let text = generate(&content, 5).to_text();
        assert_eq!(
            BattleScenario::from_text(&text).unwrap(),
            generate(&content, 5)
        );
    }

    #[test]
    fn fleeing_saves_only_the_units_that_survive_the_flight() {
        let content = content();
        let (mut sent, mut saved) = (0, 0);
        for seed in 0..60 {
            let mut env = BattleEnv::new(&content, generate(&content, seed)).unwrap();
            let side = env.to_play().unwrap();
            env.step(f4x_core::battle::BattleCommand::Flee).unwrap();
            assert!(env.outcome.fled[side.index()], "seed {seed}");
            let i = side.index();
            assert!(env.outcome.saved_by_fleeing[i] <= env.outcome.sent[i]);
            sent += env.outcome.sent[i];
            saved += env.outcome.saved_by_fleeing[i];
        }
        // Each unit is lost with one chance in two (flat settings: no safe flight).
        let share = saved as f32 / sent as f32;
        assert!((0.35..0.65).contains(&share), "saved {saved} of {sent}");
    }

    #[test]
    fn wizards_of_one_or_two_realms_fit_the_spell_slots() {
        let content = content();
        let realm_of = |id: &str| {
            let spells = &content.rules.spells;
            spells.get(spells.id_of(id).unwrap()).realm.clone()
        };
        // Every pair of realms with the arcane spells fits whole, so a generated wizard of one or
        // two realms is never cut short.
        let battle: Vec<String> = {
            content
                .rules
                .spells
                .iter()
                .filter(|(_, d)| scenario::battle_spell(d))
                .map(|(_, d)| d.realm.clone())
                .collect()
        };
        let in_realm = |r: &str| battle.iter().filter(|x| *x == r).count();
        let arcane = in_realm("arcane");
        let names = ["nature", "sorcery", "chaos", "life", "death"];
        for a in names {
            for b in names {
                let n = in_realm(a) + if a == b { 0 } else { in_realm(b) } + arcane;
                assert!(n <= obs::MAX_SPELLS, "{a} and {b} have {n} battle spells");
            }
        }
        let (mut small, mut large) = (0, 0);
        for seed in 0..3000 {
            let s = generate(&content, seed);
            for w in [&s.attacker_wizard, &s.defender_wizard]
                .into_iter()
                .flatten()
            {
                let mut realms: Vec<String> = w
                    .spells
                    .iter()
                    .map(|id| realm_of(id))
                    .filter(|r| r != "arcane")
                    .collect();
                realms.sort();
                realms.dedup();
                if realms.len() <= 2 {
                    small += 1;
                } else {
                    large += 1;
                }
            }
        }
        // Most wizards are of one or two realms, a few of more.
        assert!(
            small > 5 * large && large > 0,
            "{small} small, {large} large"
        );
    }

    #[test]
    fn the_scripted_battle_ai_plays_a_generated_battle_to_its_end() {
        let content = content();
        for seed in 100..110 {
            let mut env = BattleEnv::new(&content, generate(&content, seed)).unwrap();
            while !env.done() {
                let c = policy::scripted_command(env.battle().unwrap());
                env.step(c).unwrap();
            }
            assert!(env.outcome.winner.is_some(), "seed {seed}");
        }
        let _ = Side::Attacker;
    }

    #[test]
    fn the_observation_shows_every_active_unit_and_castable_spell() {
        use crate::obs::{Brief, ObsSpec, GRID_CHANNELS, MAX_SPELLS, MAX_UNITS};
        let content = content();
        let spec = ObsSpec::new(&content);
        let mut with_spell = 0;
        for seed in 0..12 {
            let env = BattleEnv::new(&content, generate(&content, seed)).unwrap();
            let side = env.to_play().unwrap();
            let o = spec
                .observe(&env.game, side, Brief::default())
                .expect("a battle is on");
            let b = env.battle().unwrap();
            let active = b.units.iter().filter(|u| u.is_active()).count();
            // No unit has fallen yet, so the tokens are the units on the field.
            assert_eq!(o.unit_mask.iter().filter(|&&m| m).count(), active);
            assert_eq!(o.units.len(), MAX_UNITS * spec.unit_features());
            assert_eq!(o.spells.len(), MAX_SPELLS * obs::SPELL_FEATURES);
            assert_eq!(o.grid.len(), GRID_CHANNELS * 21 * 22);
            assert_eq!(o.globals.len(), obs::GLOBALS);
            assert!(o
                .units
                .iter()
                .chain(&o.grid)
                .chain(&o.globals)
                .all(|v| v.is_finite()));
            assert_eq!(
                o.spell_mask.iter().filter(|&&m| m).count(),
                env.game
                    .battle_spells(b.player_of(side))
                    .len()
                    .min(MAX_SPELLS)
            );
            with_spell += i32::from(o.spell_mask.iter().any(|&m| m));
        }
        assert!(with_spell > 0, "no scenario gave a wizard a spell");
    }

    #[test]
    fn the_fallen_have_tokens_so_animate_dead_can_be_aimed() {
        use crate::obs::{Brief, ObsSpec};
        use f4x_core::battle::Status;
        let content = content();
        let spec = ObsSpec::new(&content);
        let spell = content.rules.spells.id_of("animate_dead").unwrap();
        // A scenario with a wizard on the side to play.
        let mut env = (0..40)
            .map(|seed| BattleEnv::new(&content, generate(&content, seed)).unwrap())
            .find(|e| {
                let b = e.battle().unwrap();
                b.caster(e.to_play().unwrap()).is_some()
            })
            .expect("a scenario with a wizard");
        let side = env.to_play().unwrap();
        let player = env.battle().unwrap().player_of(side);
        env.game
            .player_mut(player)
            .unwrap()
            .magic
            .known
            .insert(spell);
        // Skill and mana enough for it.
        env.game
            .battle
            .as_mut()
            .unwrap()
            .battle
            .set_caster(side, f4x_core::battle::BattleCaster::new(100, 1000, 10));
        let before = spec
            .observe(&env.game, side, Brief::default())
            .unwrap()
            .unit_mask
            .iter()
            .filter(|&&m| m)
            .count();
        // One of its units falls, struck down.
        let fallen = env
            .battle()
            .unwrap()
            .units
            .iter()
            .find(|u| u.side == side && u.is_active() && u.source.is_some())
            .map(|u| u.id)
            .unwrap();
        {
            let u = &mut env.game.battle.as_mut().unwrap().battle.units[fallen];
            u.status = Status::Dead;
            u.figures = 0;
            u.damage_kinds = [3, 0, 0];
        }
        let o = spec.observe(&env.game, side, Brief::default()).unwrap();
        let now = o.unit_mask.iter().filter(|&&m| m).count();
        assert_eq!(now, before, "the dead unit's token replaces its live one");
        let slot = (0..obs::MAX_UNITS)
            .find(|&t| o.unit_mask[t] && o.unit_ids[t] == fallen)
            .expect("the fallen unit has a token");
        assert_eq!(
            o.units[slot * spec.unit_features() + 9],
            0.0,
            "a fallen unit shows no figures"
        );
        // The spell may be aimed at it, and the aim is in the vocabulary.
        let casts = env.game.legal_battle_casts();
        let cast = casts.iter().find(|c| c.spell == spell);
        let aim = cast.and_then(|c| {
            c.aims
                .iter()
                .find(|a| **a == f4x_core::battle::SpellAim::Unit(fallen))
        });
        let aim = *aim.expect("animate dead may be aimed at the fallen unit");
        assert!(act::aim_index(aim, &o).is_some());
    }

    #[test]
    fn the_odds_of_generated_scenarios_follow_their_names() {
        let content = content();
        let (mut over, mut under) = (0, 0);
        for seed in 0..60 {
            let s = generate(&content, seed);
            match s.odds {
                scenario::Odds::Overwhelming if s.attackers.len() > s.defenders.len() => over += 1,
                scenario::Odds::Overwhelmed if s.attackers.len() <= s.defenders.len() => under += 1,
                _ => {}
            }
        }
        assert!(over > 0 && under > 0, "{over} {under}");
    }
}

#[cfg(test)]
mod agent_tests {
    use super::*;
    use crate::act;
    use crate::agent::{Driver, Request};
    use crate::obs::{Brief, ObsSpec};
    use f4x_core::battle::{BattleCommand, Side};
    use rand::{Rng, SeedableRng};
    use rand_pcg::Pcg64Mcg;
    use std::path::Path;

    fn pick(mask: &[bool], rng: &mut Pcg64Mcg) -> usize {
        let ok: Vec<usize> = (0..mask.len()).filter(|&i| mask[i]).collect();
        ok[rng.gen_range(0..ok.len())]
    }

    #[test]
    fn a_random_agent_playing_by_the_masks_is_never_refused() {
        let content = f4x_data::load_content_layers(&[Path::new("../../content")]).unwrap();
        let spec = ObsSpec::new(&content);
        let brief = Brief {
            may_flee: true,
            ..Brief::default()
        };
        let (mut wizard_asks, mut casts, mut flees, mut decisions) = (0, 0, 0, 0);
        for seed in 0..16 {
            let env = BattleEnv::new(&content, generate(&content, seed)).unwrap();
            let mut d = Driver::new(env, &spec, brief);
            let mut rng = Pcg64Mcg::seed_from_u64(seed);
            // Both sides are the agent, choosing at random among what the masks allow.
            let mut none = |_: &f4x_core::battle::Battle, _: Side| BattleCommand::EndTurn;
            while let Some(p) = d.advance([true, true], &mut none).unwrap() {
                decisions += 1;
                match &p.request {
                    Request::Unit { mask, slot, .. } => {
                        assert!(p.obs.unit_mask[*slot]);
                        let i = pick(mask, &mut rng);
                        d.answer_unit(&p, i)
                            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
                    }
                    Request::Wizard { options, .. } => {
                        wizard_asks += 1;
                        // Flee is rare so battles go on.
                        let mut m = options.choice.clone();
                        m[act::FLEE] = m[act::FLEE] && rng.gen_bool(0.02);
                        let c = pick(&m, &mut rng);
                        flees += usize::from(c == act::FLEE);
                        let (aim, bin) = if c >= act::SPELL {
                            casts += 1;
                            let k = c - act::SPELL;
                            let bins = [true, options.more_mana[k]];
                            (pick(&options.aims[k], &mut rng), pick(&bins, &mut rng))
                        } else {
                            (0, 0)
                        };
                        d.answer_wizard(&p, c, aim, bin)
                            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
                    }
                }
            }
            assert!(d.env.outcome.winner.is_some() || d.env.steps >= env::STEP_LIMIT);
        }
        assert!(
            decisions > 100 && wizard_asks > 0 && casts > 0,
            "{decisions} {wizard_asks} {casts}"
        );
        let _ = flees;
    }
}

#[cfg(all(test, feature = "train"))]
mod brain_tests {
    use super::*;
    use burn::prelude::*;
    use std::path::Path;

    /// A network that was never trained, saved and loaded as a checkpoint, plays a battle of
    /// the game through the core's hook and every command it gives is accepted.
    #[test]
    fn a_checkpoint_plays_battles_through_the_core_hook() {
        let content = f4x_data::load_content_layers(&[Path::new("../../content")]).unwrap();
        let spec = obs::ObsSpec::new(&content);
        let dir = std::env::temp_dir().join(format!("f4x-learn-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("net");
        net::save_checkpoint(
            net::PolicyNet::new(spec.unit_features(), &Device::flex()),
            &spec,
            &file,
        )
        .unwrap();
        let mut finished = 0;
        for seed in 0..4 {
            let mut env = BattleEnv::new(&content, generate(&content, seed)).unwrap();
            let brain = brain::LearnedBrain::load(&content, &file).unwrap();
            env.game
                .set_battle_brain(build::ATTACKER, Some(Box::new(brain)));
            let mut steps = 0;
            while !env.done() && steps < 4000 {
                let c = env.game.next_battle_command().expect("a battle is on");
                env.step(c).unwrap_or_else(|e| {
                    panic!("seed {seed}: the brain's command was refused: {e}")
                });
                steps += 1;
            }
            finished += usize::from(env.outcome.winner.is_some());
        }
        assert!(finished >= 3, "{finished} of 4 battles ended");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A checkpoint saved for other rules (an ability gone, a size changed) or with no record of
    /// what it reads is refused with a reason, not a panic.
    #[test]
    fn a_checkpoint_for_other_rules_is_refused_with_a_reason() {
        let content = f4x_data::load_content_layers(&[Path::new("../../content")]).unwrap();
        let spec = obs::ObsSpec::new(&content);
        let dir = std::env::temp_dir().join(format!("f4x-learn-shape-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("net");
        net::save_checkpoint(
            net::PolicyNet::new(spec.unit_features(), &Device::flex()),
            &spec,
            &file,
        )
        .unwrap();
        assert!(brain::LearnedBrain::load(&content, &file).is_ok());

        let shape = dir.join("net.shape.ron");
        let mut sig = spec.signature();
        sig.abilities.pop();
        sig.sizes[2].1 += 1;
        std::fs::write(&shape, ron::to_string(&sig).unwrap()).unwrap();
        let Err(why) = brain::LearnedBrain::load(&content, &file) else {
            panic!("a checkpoint of other rules was loaded");
        };
        assert!(
            why.contains("abilities added") && why.contains("MAX_SPELLS was"),
            "{why}"
        );

        std::fs::remove_file(&shape).unwrap();
        let Err(why) = brain::LearnedBrain::load(&content, &file) else {
            panic!("a checkpoint with no record was loaded");
        };
        assert!(why.contains("no record"), "{why}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
