//! Every spell the real content lets a wizard cast in a battle is cast once, in a small battle,
//! to see that what the game lists as legal is accepted and that the fight goes on without a
//! refused command (task 302, part 4: the real data through the engine). Slow, so it runs on
//! request: `cargo test -p f4x-learn --test it battle_spells -- --ignored`.

use f4x_core::battle::{BattleCommand, Side};
use f4x_core::rules::defs::CastContext;
use f4x_learn::build::build;
use f4x_learn::scenario::{generate_raw, Ground, WizardSpec};

/// Spells that no scenario of this test can offer, with the reason: they need a fallen unit of the
/// caster's, a hero, or a node.
const NEEDS_SETUP: [&str; 4] = ["raise_dead", "animate_dead", "recall_hero", "magic_vortex"];

#[test]
#[ignore = "slow: casts every battle spell of the content once"]
fn every_battle_spell_that_is_offered_is_accepted_and_the_fight_goes_on() {
    let content = f4x_data::load_content(std::path::Path::new("../../content")).unwrap();
    let spells: Vec<String> = content
        .rules
        .spells
        .iter()
        .filter(|(_, d)| d.contexts.contains(&CastContext::Battle))
        .map(|(_, d)| d.id.clone())
        .collect();
    assert!(spells.len() > 50, "{}", spells.len());
    let mut never_offered = Vec::new();
    for id in &spells {
        let mut cast = false;
        'seeds: for seed in 0..8u64 {
            let mut scenario = generate_raw(&content, seed);
            let wizard = Some(WizardSpec {
                spells: vec![id.clone()],
                mana: 50_000,
                skill_points: 100_000,
            });
            // Every other scenario is a siege of a walled city (Disrupt, the walls).
            if seed % 2 == 1 {
                scenario.ground = Ground::City { walls: true };
            }
            scenario.attacker_wizard = wizard.clone();
            scenario.defender_wizard = wizard;
            let Ok(mut game) = build(&content, &scenario) else {
                continue;
            };
            // The attacker's turn first, then the defender's (a wall can only be raised by it).
            for turn in 0..2 {
                let offered = game.legal_battle_casts().into_iter().find(|c| {
                    game.content().rules.spells.get(c.spell).id == *id && !c.aims.is_empty()
                });
                if let Some(c) = offered {
                    let command = BattleCommand::Cast {
                        spell: c.spell,
                        aim: c.aims[0],
                        mana: c.least_mana,
                    };
                    game.battle_command(command)
                        .unwrap_or_else(|e| panic!("{id}, seed {seed}: refused {e:?}"));
                    for _ in 0..60 {
                        if !game.in_battle() {
                            break;
                        }
                        let next = game.next_battle_command().expect("a command");
                        game.battle_command(next)
                            .unwrap_or_else(|e| panic!("{id}, seed {seed}: refused {e:?}"));
                    }
                    cast = true;
                    break 'seeds;
                }
                if turn == 0 && game.battle().is_some_and(|b| b.active == Side::Attacker) {
                    game.battle_command(BattleCommand::EndTurn).ok();
                }
            }
        }
        if !cast {
            never_offered.push(id.as_str());
        }
    }
    let unexpected: Vec<_> = never_offered
        .iter()
        .filter(|id| !NEEDS_SETUP.contains(id))
        .collect();
    assert!(
        unexpected.is_empty(),
        "spells never offered in eight scenarios: {unexpected:?}"
    );
}
