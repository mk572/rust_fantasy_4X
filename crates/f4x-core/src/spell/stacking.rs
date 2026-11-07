//! Lasting spells that cannot be combined, as the original refuses them (`Cast_Spell_Overland`
//! in `OverSpel.c`, `Questionable_Spell_Cast` in `SBookScr.c`): a spell the unit or city has
//! already, one it is not cumulative with, and one the unit's own ability makes pointless.
//!
//! Tests: in spell/stacking/tests.rs.

use crate::game::Game;
use crate::rules::exec::Target;
use crate::rules::{AbilityId, SpellDef};
use crate::rules::{Duration, SpellId, SpellTarget};
use crate::spell::unit_effect::UnitEffects;
use crate::wizard::magic::MagicError;

/// Why a lasting spell is of no use on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stacking {
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
    pub(crate) fn is_pointless(self) -> bool {
        matches!(self, Self::AlreadyEnchanted | Self::AlreadyAble)
    }
}

/// What a unit has, as far as the stacking rules ask, for the map and the battle alike.
pub(crate) struct Carrier<'a> {
    pub has_spell: &'a dyn Fn(SpellId) -> bool,
    pub has_ability: &'a dyn Fn(AbilityId) -> bool,
    /// What its spells do to it (`unit_effect` hook).
    pub effects: UnitEffects,
}

/// The one rule for a lasting spell on a unit, used on the map and in a battle: why the spell
/// is of no use on `unit` for a caster with the given global enchantments, if it is.
pub(crate) fn stacking_refusal(
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

impl Game {
    /// Whether `spell`, a lasting one, may be put on `target` by `player`.
    pub(crate) fn check_stacking(
        &self,
        player: crate::player::PlayerId,
        spell: SpellId,
        target: Option<Target>,
    ) -> Result<(), MagicError> {
        let def = self.content.rules.spells.get(spell);
        if def.duration == Duration::Instant {
            return Ok(());
        }
        let spells = self.spells();
        match (def.target, target) {
            (
                SpellTarget::OwnUnit | SpellTarget::EnemyUnit | SpellTarget::AnyUnit,
                Some(Target::Unit(u)),
            ) => {
                let own = self.units().unit_def(u).map(|d| d.abilities.as_slice());
                let carrier = Carrier {
                    has_spell: &|id| spells.unit_has(u, id),
                    has_ability: &|a| own.is_some_and(|own| own.contains(&a)),
                    effects: self.units().unit_effects(u),
                };
                match stacking_refusal(spell, def, &carrier, &|id| self.has_global(player, id)) {
                    Some(why) if !(why.is_pointless() && self.confirmed) => Err(why.into()),
                    _ => Ok(()),
                }
            }
            (SpellTarget::OwnCity | SpellTarget::EnemyCity, Some(Target::City(c)))
                if self.economy().city_has_enchantment(c, spell) && !self.confirmed =>
            {
                Err(MagicError::AlreadyEnchanted)
            }
            _ => Ok(()),
        }
    }
}

impl From<Stacking> for MagicError {
    fn from(why: Stacking) -> Self {
        match why {
            Stacking::AlreadyEnchanted => Self::AlreadyEnchanted,
            Stacking::NotCumulative => Self::NotCumulative,
            Stacking::AlreadyAble => Self::AlreadyAble,
        }
    }
}

impl crate::unit::UnitView<'_> {
    /// Whether a spell on the unit gives way to a stronger one it is not cumulative with, one of
    /// the unit's own enchantments or one its items carry (Stone Skin under Iron Skin).
    pub(crate) fn unit_spell_superseded(&self, unit: hecs::Entity, spell: SpellId) -> bool {
        let def = self.content.rules.spells.get(spell);
        def.not_with.clone().into_iter().any(|other| {
            crate::spell::instance::Spells::new(self.world).unit_has(unit, other)
                || self.items().spells(unit).contains(&other)
        })
    }
}

#[cfg(test)]
mod tests;
