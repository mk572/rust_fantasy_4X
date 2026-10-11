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
        let mut cast = 0;
        for seed in 0..24 {
            let env = play_random(&content, seed);
            assert!(
                env.steps < env::STEP_LIMIT,
                "seed {seed} hit the step limit"
            );
            let w = env.outcome.winner.expect("a winner");
            winners[w.index()] += 1;
            cast += i32::from(env.outcome.mana_spent.iter().sum::<i32>() > 0);
        }
        assert!(winners[0] > 0 && winners[1] > 0, "{winners:?}");
        assert!(cast > 0, "no battle ever cast a spell");
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
    use burn::module::Module;
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
        net::PolicyNet::new(spec.unit_features(), &Device::flex())
            .save_file(&file)
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
}
