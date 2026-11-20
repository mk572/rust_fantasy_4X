//! What the game hands a battle about a unit, and about the wizards who own the sides. It is all
//! source data: the definition's numbers, the modifiers of the unit's own abilities, spells, items
//! and temporary effects, its experience, and the ids of the wizards' global enchantments and
//! retorts. No level, no global enchantment and no number worked out from them is in it: those are
//! worked out from the sheet whenever they are asked for (`unit/stats.rs`).
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use std::collections::BTreeMap;

use f4x_def::half_int::HalfInt;
use f4x_def::hero::HeroAbilities;
use f4x_def::rules::damage_type::RangedKind;
use f4x_def::rules::stats::{Modifier, Stat};
use f4x_def::rules::unit_flag::UnitFlags;
use f4x_def::rules::{Ability, Id, RetortDef, SpellId};

use crate::side::Side;

/// What the game hands over about one unit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Sheet {
    /// The unit definition's own numbers (`units.ron`), by stat.
    pub base: BTreeMap<Stat, HalfInt>,
    /// What its own abilities, lasting spells, items and temporary effects do to its stats: not
    /// its level, a hero's abilities or its owner's global enchantments, which are worked out here.
    pub modifiers: Vec<Modifier>,
    /// Its abilities before the battle's lasting spells: its own, a hero's, and the ones items and
    /// spells give it.
    pub abilities: Vec<Ability>,
    /// Of those, the ones an item or a lasting spell gives (`SpecialAttacks::of_unit`).
    pub granted: Vec<Ability>,
    /// The ids of the lasting spells it has overland.
    pub enchantments: Vec<SpellId>,
    /// What it is by nature: flies, has a magical weapon.
    pub flags: UnitFlags,
    pub ranged_kind: RangedKind,
    /// Its experience: its level is worked out from it.
    pub xp: u32,
    pub hero: Option<HeroSheet>,
    /// The side of the wizard it belongs to, whatever side it fights on.
    pub home: Side,
}

/// What a hero is, besides its unit: the rolled abilities and the casting skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HeroSheet {
    pub abilities: HeroAbilities,
    pub casting_skill: u8,
    /// The spell skill its items add to its mana.
    pub item_spell_skill: i32,
}

impl Sheet {
    /// A sheet with nothing on it but the numbers.
    pub fn plain(home: Side, base: BTreeMap<Stat, HalfInt>) -> Self {
        Self {
            base,
            modifiers: Vec::new(),
            abilities: Vec::new(),
            granted: Vec::new(),
            enchantments: Vec::new(),
            flags: UnitFlags::NONE,
            ranged_kind: RangedKind::None,
            xp: 0,
            hero: None,
            home,
        }
    }

    /// The definition's number of a stat, whole.
    pub fn number(&self, stat: Stat) -> i32 {
        self.base.get(&stat).map_or(0, |v| v.floor())
    }
}

/// What a battle knows of the wizard of one side: the ids of its global enchantments and retorts.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WizardSheet {
    pub globals: Vec<SpellId>,
    pub retorts: Vec<Id<RetortDef>>,
}
