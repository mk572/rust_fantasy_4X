//! A unit made in a battle: a creature summoned there exists only on that battlefield, so it is
//! made from its definition alone, with no overland unit behind it. What the wizards' global
//! enchantments do to it is worked out from the ids the battle holds (`unit/stats.rs`).
//!
//! Tests: in `f4x-core`'s battle/tests.rs (the same sheet as an overland unit of its kind).

use f4x_def::rules::realm::Realm;
use f4x_def::rules::stats::{compute, passive_modifiers, Modifier, Source, Stat};
use f4x_def::rules::unit_flag::{UnitFlag, UnitFlags};
use f4x_def::rules::{MovementType, Rules, UnitId};

use super::{BattleUnit, Sheet, UnitCasting};
use crate::side::Side;
use crate::weapon::ranged_kind_of;

impl BattleUnit {
    /// A fresh unit of this kind: its definition's numbers with what its own abilities and its
    /// first level add, no wounds, no items, no lasting spells. Give it the content with `bind`.
    pub fn from_def(rules: &Rules, unit: UnitId, side: Side) -> Self {
        let def = rules.units.get(unit);
        let abilities = def.abilities.clone();
        let mut modifiers: Vec<Modifier> = Vec::new();
        for &a in &abilities {
            modifiers.extend(passive_modifiers(
                &rules.abilities.get(a).effects,
                Source::Ability(a),
            ));
        }
        let mut flags = UnitFlags::NONE;
        if def.movement == MovementType::Flying {
            flags.insert(UnitFlag::Flies);
        }
        if def.innate_magical_weapon() {
            flags.insert(UnitFlag::MagicalWeapon);
        }
        let mut sheet = Sheet::plain(side, def.stats.clone());
        sheet.ranged_kind = ranged_kind_of(&abilities, rules);
        sheet.abilities = abilities;
        sheet.modifiers = modifiers;
        sheet.flags = flags;
        let mut level_mods = sheet.modifiers.clone();
        if let Some(effects) = def
            .level_path
            .map(|p| rules.level_paths.get(p))
            .and_then(|path| path.level(0))
        {
            level_mods.extend(passive_modifiers(effects, Source::Level(0)));
        }
        let figures =
            compute(Stat::Figures, sheet.number(Stat::Figures), &level_mods).max(1) as u32;
        let casting = UnitCasting::of_abilities(&sheet.abilities, rules);
        let mut built = BattleUnit::from_sheet(side, sheet, figures);
        built.unit_id = Some(unit);
        built.casting = casting;
        built.sails = def.movement == MovementType::Sailing;
        built.swims = def.movement == MovementType::Swimming;
        built.realm = def.realm.and_then(Realm::of);
        built
    }
}
