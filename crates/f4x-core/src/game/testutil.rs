//! Content for unit tests: [`shipped`] is the game's own `content/`.
//!
//! Tests: none, because this is test support.

use f4x_def::content::Content;

/// The game's own `content/` folder, loaded by the real loader (`f4x_def::testdir::shipped`), as
/// a copy for a `Game`, which owns its content.
pub fn shipped() -> Content {
    crate::battle::game::set_battle_ai(battle_policy);
    f4x_def::testdir::shipped().clone()
}

/// The battle player of core's own tests: every unit hits what is next to it, shoots what it can
/// or walks at the nearest enemy, with no walls and no spells. It stands in for the scripted
/// battle AI (`f4x_ai::battle`), which core cannot use, wherever a test only needs a fight to
/// get played to the end.
pub fn battle_policy(game: &crate::game::Game) -> Option<crate::battle::BattleCommand> {
    use crate::battle::map::distance;
    use crate::battle::{BattleCommand, RangedKind};
    let battle = game.battle()?;
    let side = battle.active;
    for unit in battle.active_units(side) {
        if unit.moves_left == 0 {
            continue;
        }
        let Some(enemy) = battle
            .active_units(side.other())
            .filter(|e| battle.seen_by(side, e.id))
            .min_by_key(|e| (distance(unit.cell, e.cell), e.id))
        else {
            return Some(BattleCommand::EndTurn);
        };
        if unit.can_attack() {
            if distance(unit.cell, enemy.cell) == 1 {
                return Some(BattleCommand::Attack {
                    unit: unit.id,
                    target: enemy.id,
                });
            }
            let shoots = matches!(unit.ranged_kind, RangedKind::Missile | RangedKind::Magic)
                && unit.own_ranged > 0
                && unit.ammo > 0;
            if shoots && !battle.blocked_by_darkness(unit.id, enemy.id) {
                return Some(BattleCommand::Shoot {
                    unit: unit.id,
                    target: enemy.id,
                });
            }
        }
        if unit.moves_left > 0 {
            let here = distance(unit.cell, enemy.cell);
            let best = battle
                .reachable(unit.id)
                .into_iter()
                .min_by_key(|&(p, cost)| (distance(p, enemy.cell), cost, p));
            if let Some((to, _)) = best {
                if distance(to, enemy.cell) < here {
                    return Some(BattleCommand::Move { unit: unit.id, to });
                }
            }
        }
        return Some(BattleCommand::Done { unit: unit.id });
    }
    Some(BattleCommand::EndTurn)
}

/// Where the saved fresh games of [`fresh`] are (`crates/f4x-core/tests/saves`).
pub fn saves_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/saves")
}

/// The file of the fresh game with `wizards` computer wizards on `seed`.
pub fn fresh_file(wizards: u8, seed: u64) -> std::path::PathBuf {
    saves_dir().join(format!("fresh_w{wizards}_s{seed}.save"))
}

/// A new game of `wizards` computer wizards on `seed`, on the shipped content, loaded from its
/// save: the world `f4x_newgame::setup::new_game` made, with a real map, cities, units and wizards.
/// Core cannot make one itself (the crate that does sits above it); the test `fixtures` of
/// `f4x-newgame` keeps these files equal to what it makes, and rewrites them with
/// `F4X_REFRESH_FIXTURES=1 cargo test -p f4x-newgame --test it fixtures`.
pub fn fresh(wizards: u8, seed: u64) -> Result<crate::game::Game, String> {
    let path = fresh_file(wizards, seed);
    crate::game::save::file::load_game(&path, shipped())
        .map(|(game, _)| game)
        .map_err(|e| {
            format!(
                "{}: {e} (rewrite the fresh games with F4X_REFRESH_FIXTURES=1 cargo test -p f4x-newgame --test it fixtures)",
                path.display()
            )
        })
}
