//! Content for unit tests: [`shipped`] is the game's own `content/`, and [`num`] names the numbers
//! of the terrains and map features tests use.
//!
//! Tests: none, because this is test support.

use f4x_def::content::Content;

/// The numbers (`num` in `content/`) of the terrains and map features that tests name. The number
/// is the one source of truth, as in a save; the constant's name only tells the reader which
/// English-named terrain or feature the shipped content has under it today. If a name is
/// changed or translated, the number still holds.
pub mod num {
    pub mod terrain {
        pub const CHAOS_NODE: u16 = 1;
        pub const FOREST: u16 = 3;
        pub const GRASSLAND: u16 = 4;
        pub const HILLS: u16 = 5;
        pub const MOUNTAINS: u16 = 6;
        pub const NATURE_NODE: u16 = 7;
        pub const OCEAN: u16 = 8;
        pub const SORCERY_NODE: u16 = 9;
        pub const SWAMP: u16 = 10;
    }

    pub mod feature {
        pub const ADAMANTIUM_ORE: u16 = 2;
        pub const COAL: u16 = 5;
        pub const GEM: u16 = 9;
        pub const GOLD_ORE: u16 = 10;
        pub const IRON_ORE: u16 = 11;
        pub const MITHRIL_ORE: u16 = 12;
        pub const MONSTER_LAIR: u16 = 13;
        pub const NIGHTSHADE: u16 = 16;
        pub const SILVER_ORE: u16 = 19;
        pub const SORCERY_NODE: u16 = 20;
        pub const TOWER: u16 = 21;
        pub const WILD_GAME: u16 = 22;
    }
}

/// The shipped `crates/f4x-def/content/rules_settings.ron`, for tests that play a difficulty level.
pub fn settings() -> crate::game::setting::SettingsDef {
    f4x_def::testdir::shipped().settings.clone()
}

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
        if unit.attacked && unit.moves_left == 0 {
            continue;
        }
        let Some(enemy) = battle
            .active_units(side.other())
            .filter(|e| battle.seen_by(side, e.id))
            .min_by_key(|e| (distance(unit.cell, e.cell), e.id))
        else {
            return Some(BattleCommand::EndTurn);
        };
        if !unit.attacked {
            if distance(unit.cell, enemy.cell) == 1 {
                return Some(BattleCommand::Attack {
                    unit: unit.id,
                    target: enemy.id,
                });
            }
            let shoots = matches!(unit.ranged_kind, RangedKind::Missile | RangedKind::Magic)
                && unit.ranged > 0
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

/// The number of the ability the content file calls `alias`, for tests that name abilities in
/// lists and loops (the game code itself refers to abilities by number).
pub fn ability(alias: u16) -> crate::rules::Ability {
    shipped()
        .rules
        .abilities
        .id_numbered(alias)
        .unwrap_or_else(|| panic!("no ability {alias:?} in the shipped content"))
}

/// The number of the realm the content file calls `alias`, for tests that name realms (the game
/// code itself refers to realms by number).
pub fn realm(alias: u16) -> crate::rules::RealmId {
    shipped()
        .rules
        .realms
        .id_numbered(alias)
        .unwrap_or_else(|| panic!("no realm {alias:?} in the shipped content"))
}

/// The number of the realm the content file calls `alias`, for tests that set a definition's
/// realm by hand.
pub fn realm_ref(alias: u16) -> crate::rules::RealmId {
    realm(alias)
}

/// The number of the ability the content file calls `alias`, for tests that name abilities in a
/// condition or an effect.
pub fn ability_ref(alias: u16) -> crate::rules::Ability {
    ability(alias)
}

/// [`ability`] for a list of aliases.
pub fn abilities(aliases: &[u16]) -> Vec<crate::rules::Ability> {
    aliases.iter().map(|&a| ability(a)).collect()
}
