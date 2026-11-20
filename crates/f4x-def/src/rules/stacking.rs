//! Lasting spells that cannot be combined, as the original refuses them (`Cast_Spell_Overland`
//! in `OverSpel.c`, `Questionable_Spell_Cast` in `SBookScr.c`): a spell the unit has already, one it
//! is not cumulative with, and one the unit's own ability makes pointless. The rule is the same on
//! the map and in a battle.
//!
//! Tests: in `f4x-core`'s spell/stacking/tests.rs.

use super::target_error::TargetError;
use super::unit_effect::UnitEffects;
use super::{AbilityId, SpellDef, SpellId, UnitKind, UnitOnly};

/// Why a lasting spell is of no use on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stacking {
    /// The unit has the spell, or already has everything the spell does to a unit.
    AlreadyEnchanted,
    /// The spell is not cumulative with one the unit has or the wizard keeps.
    NotCumulative,
    /// The unit has the ability the spell gives, by its nature.
    AlreadyAble,
}

impl Stacking {
    /// Whether the caster may cast it anyway after a confirmation: the spell is pointless, not
    /// contradictory. The AI never confirms.
    pub fn is_pointless(self) -> bool {
        matches!(self, Self::AlreadyEnchanted | Self::AlreadyAble)
    }
}

/// What a unit has, as far as the stacking rules ask, for the map and the battle alike.
pub struct Carrier<'a> {
    pub has_spell: &'a dyn Fn(SpellId) -> bool,
    pub has_ability: &'a dyn Fn(AbilityId) -> bool,
    /// What its spells do to it (`unit_effect` hook).
    pub effects: UnitEffects,
}

/// The one rule for a lasting spell on a unit, used on the map and in a battle: why the spell
/// is of no use on `unit` for a caster with the given global enchantments, if it is.
pub fn stacking_refusal(
    spell: SpellId,
    def: &SpellDef,
    unit: &Carrier<'_>,
    caster_has: &dyn Fn(SpellId) -> bool,
) -> Option<Stacking> {
    let gives = UnitEffects::of(def);
    if (unit.has_spell)(spell) || (!gives.is_empty() && unit.effects.covers(gives)) {
        return Some(Stacking::AlreadyEnchanted);
    }
    if def
        .not_with
        .iter()
        .copied()
        .any(|other| (unit.has_spell)(other) || caster_has(other))
    {
        return Some(Stacking::NotCumulative);
    }
    if def
        .already_able
        .iter()
        .copied()
        .any(|a| (unit.has_ability)(a))
    {
        return Some(Stacking::AlreadyAble);
    }
    None
}

/// Whether a unit of this kind may take a spell that is for `only`. A unit is a valid target when
/// any one thing it is passes: a hero passes as a hero though it is also a creature of a realm.
/// Undead pass as neither.
pub fn unit_only_refusal(only: UnitOnly, kind: UnitKind, undead: bool) -> Option<TargetError> {
    match only {
        UnitOnly::Any => None,
        UnitOnly::NormalOrHero => {
            let passes = !undead && matches!(kind, UnitKind::Normal | UnitKind::Hero);
            (!passes).then_some(TargetError::NotNormalOrHero)
        }
    }
}
