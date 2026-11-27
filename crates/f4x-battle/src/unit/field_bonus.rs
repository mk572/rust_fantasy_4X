//! What the battlefield enchantments ask of a unit's numbers, and how the unit takes it.
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use f4x_def::rules::battle_number::StatBonus;
use f4x_def::rules::unit_flag::UnitFlag;
use f4x_def::rules::{Ability, Rules};

use crate::attack::{Combatant, DefenderState};

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

impl FieldBonus {
    /// Add what a field spell gives; its melee and ranged numbers only when the flag says so.
    pub fn add(&mut self, bonus: StatBonus, melee: bool, ranged: bool) {
        if melee {
            self.melee += bonus.melee;
        }
        if ranged {
            self.ranged += bonus.ranged;
        }
        self.defense += bonus.defense;
        self.resistance += bonus.resistance;
        self.to_hit += bonus.to_hit;
        self.to_block += bonus.to_block;
    }
}

impl BattleUnit {
    /// Whether the creature belongs to a realm (a summoned or fantastic creature, not one of a
    /// race's units).
    pub fn is_fantastic(&self) -> bool {
        self.realm.is_some()
    }
}

/// A unit as it stands in its battle now: its own numbers with what the battlefield adds to them,
/// worked out when it is asked for (`Battle::now`), so there is nothing to take off when a spell
/// ends or the unit that gave a bonus is gone. It reads as the unit for everything else.
#[derive(Debug, Clone, Copy)]
pub struct UnitNow<'a> {
    unit: &'a BattleUnit,
    bonus: FieldBonus,
}

impl<'a> UnitNow<'a> {
    pub fn new(unit: &'a BattleUnit, bonus: FieldBonus) -> Self {
        Self { unit, bonus }
    }

    /// What the battlefield adds to this unit.
    pub fn bonus(&self) -> FieldBonus {
        self.bonus
    }

    /// The unit itself, whose own numbers (`own_melee`, ...) leave the battlefield out.
    pub fn unit(&self) -> &'a BattleUnit {
        self.unit
    }

    pub fn melee(&self) -> i32 {
        (self.unit.own_melee + self.bonus.melee).max(0)
    }

    pub fn ranged(&self) -> i32 {
        (self.unit.own_ranged + self.bonus.ranged).max(0)
    }

    pub fn defense(&self) -> i32 {
        (self.unit.own_defense + self.bonus.defense).max(0)
    }

    pub fn resistance(&self) -> i32 {
        (self.unit.own_resistance + self.bonus.resistance).max(0)
    }

    pub fn to_hit(&self) -> i32 {
        self.unit.own_to_hit + self.bonus.to_hit
    }

    pub fn to_block(&self) -> i32 {
        self.unit.own_to_block + self.bonus.to_block
    }

    /// Whether the unit has the battle flag; Metal Fires makes the weapons magical.
    pub fn flag(&self, flag: UnitFlag) -> bool {
        self.unit.flag(flag) || (flag == UnitFlag::MagicalWeapon && self.bonus.magical)
    }

    /// Invisible: attackers without immunity to illusions have a penalty to hit it.
    pub fn is_invisible(&self) -> bool {
        self.unit.has(Ability::new(33 /* invisibility */)) || self.bonus.invisible
    }

    /// Blur makes it hard to hit.
    pub fn blur(&self) -> bool {
        self.bonus.blur
    }

    pub fn defender_state(&self, rules: &Rules) -> DefenderState {
        self.unit.defender_state(rules, self.bonus)
    }

    pub fn body_state(&self) -> DefenderState {
        self.unit.body_state(self.bonus)
    }

    pub fn web_strength(&self, rules: &Rules) -> i32 {
        self.unit.web_strength(rules, self.bonus)
    }

    pub fn melee_combatant(&self, rules: &Rules, magic_exceptions: bool) -> Combatant {
        self.unit
            .melee_combatant(rules, magic_exceptions, self.bonus)
    }

    pub fn thrown_combatant(&self, rules: &Rules, magic_exceptions: bool) -> Option<Combatant> {
        self.unit
            .thrown_combatant(rules, magic_exceptions, self.bonus)
    }
}

impl std::ops::Deref for UnitNow<'_> {
    type Target = BattleUnit;

    fn deref(&self) -> &BattleUnit {
        self.unit
    }
}
