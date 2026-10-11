//! The learning environment for the AIs of `docs/learning-ai.md`: battle scenarios and the
//! battle environment (task 169), and later the networks and training.

pub mod build;
pub mod env;
pub mod obs;
pub mod policy;
pub mod scenario;

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
