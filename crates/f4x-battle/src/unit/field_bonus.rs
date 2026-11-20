//! What the battlefield enchantments ask of a unit's numbers, and how the unit takes it.
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use f4x_def::rules::Ability;

use super::BattleUnit;
/// What the battlefield spells ask of a unit's numbers (`BattleUnit::field`). It is only a
/// request: the unit's numbers are worked out with it on top of its sheet and its lasting spells
/// (`unit/stats.rs`), so there is nothing to take off again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct FieldBonus {
    pub melee: i32,
    pub ranged: i32,
    pub defense: i32,
    pub resistance: i32,
    pub to_hit: i32,
    pub to_block: i32,
    /// Metal Fires makes its weapons magical.
    pub magical: bool,
    /// Mass Invisibility makes it invisible.
    pub invisible: bool,
    /// Blur makes it hard to hit.
    pub blur: bool,
}

impl BattleUnit {
    /// Whether the creature belongs to a realm (a summoned or fantastic creature, not one of a
    /// race's units).
    pub fn is_fantastic(&self) -> bool {
        self.realm.is_some()
    }

    /// Invisible: attackers without immunity to illusions have a penalty to hit it.
    pub fn is_invisible(&self) -> bool {
        self.has(Ability::new(33 /* invisibility */)) || self.field.invisible
    }
}
