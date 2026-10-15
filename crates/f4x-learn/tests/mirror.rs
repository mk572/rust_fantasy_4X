//! The scripted battle AI against its own starting point (`docs/ai-design.md` section 8.2):
//! the same random armies are fought twice, the smarter AI (unit orders and wizard spells, as
//! `Game::next_battle_command` plays a wizard) on each side in turn against the plain one
//! (`ai::baseline_command`, which casts nothing), and it must win more of them than it loses.

use f4x_core::battle::{ai, BattleCommand, Side};
use f4x_core::content::Content;
use f4x_learn::env::BattleEnv;
use f4x_learn::scenario::generate_raw;

/// Fight one scenario with the smarter AI on `side`; returns whether it won.
fn fight(content: &Content, seed: u64, side: Side) -> Option<bool> {
    let scenario = generate_raw(content, seed);
    let mut env = BattleEnv::new(content, scenario).ok()?;
    while !env.done() {
        let command: BattleCommand = if env.battle()?.active == side {
            env.game.next_battle_command()?
        } else {
            ai::baseline_command(env.battle()?)
        };
        env.step(command).ok()?;
    }
    env.outcome.winner.map(|w| w == side)
}

#[test]
#[ignore = "fights 300 mirrored battles; run with --ignored when the battle AI changes"]
fn the_smarter_ai_beats_the_plain_one_on_both_sides() {
    let content = f4x_data::load_content(std::path::Path::new("../../content")).unwrap();
    let (mut wins, mut losses) = (0, 0);
    for seed in 0..150u64 {
        for side in [Side::Attacker, Side::Defender] {
            match fight(&content, seed, side) {
                Some(true) => wins += 1,
                Some(false) => losses += 1,
                None => {}
            }
        }
    }
    println!("smarter AI: {wins} wins, {losses} losses");
    // Equal play gives about half each (the spread of 300 fights is about nine); a real edge
    // shows well above that.
    assert!(
        wins > losses + 20,
        "the smarter AI won {wins} and lost {losses} of the mirrored fights"
    );
}

/// A battle between two single units where the attacker's wizard knows Fire Bolt.
fn bolt_scenario(content: &Content, mana: i32) -> f4x_learn::scenario::BattleScenario {
    use f4x_learn::scenario::{Ground, Odds, UnitPick, WizardSpec};
    let unit = |name: &str| UnitPick {
        unit: name.into(),
        xp: 0,
        hurt: 0,
    };
    let _ = content;
    f4x_learn::scenario::BattleScenario {
        seed: 7,
        odds: Odds::Even,
        ground: Ground::Field,
        attackers: vec![unit("orc_warrior")],
        defenders: vec![unit("swordsmen"), unit("orc_warrior")],
        attacker_wizard: Some(WizardSpec {
            spells: vec!["fire_bolt".into()],
            mana,
            skill_points: 100,
        }),
        defender_wizard: None,
    }
}

#[test]
fn a_wizard_with_a_bolt_casts_it_on_the_first_turn() {
    let content = f4x_data::load_content(std::path::Path::new("../../content")).unwrap();
    let env = BattleEnv::new(&content, bolt_scenario(&content, 100)).unwrap();
    let bolt = content.rules.spells.id_of("fire_bolt").unwrap();
    let mut game = env.game;
    match game.next_battle_command() {
        Some(BattleCommand::Cast { spell, .. }) => assert_eq!(spell, bolt),
        other => panic!("expected a cast, got {other:?}"),
    }
}

#[test]
fn a_wizard_without_mana_to_spare_casts_nothing() {
    let content = f4x_data::load_content(std::path::Path::new("../../content")).unwrap();
    let env = BattleEnv::new(&content, bolt_scenario(&content, 0)).unwrap();
    let mut game = env.game;
    assert!(!matches!(
        game.next_battle_command(),
        Some(BattleCommand::Cast { .. })
    ));
}
