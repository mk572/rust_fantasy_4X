//! Lasting spells that cannot be combined, as the original refuses them (`Cast_Spell_Overland`
//! in `OverSpel.c`, `Questionable_Spell_Cast` in `SBookScr.c`): a spell the unit or city has
//! already, one it is not cumulative with, and one the unit's own ability makes pointless.
//!
//! Tests: in spell/stacking/tests.rs.
use crate::game::Game;
use crate::rules::exec::Target;
use crate::rules::{Duration, SpellId, SpellTarget};
use crate::wizard::magic::MagicError;
pub(crate) use f4x_def::rules::stacking::{stacking_refusal, unit_only_refusal, Carrier, Stacking};

impl Game {
    /// Whether `spell`, a lasting one, may be put on `target` by `player`.
    pub(crate) fn check_stacking(
        &self,
        player: crate::player::PlayerId,
        spell: SpellId,
        target: Option<Target>,
    ) -> Result<(), MagicError> {
        let def = self.content.rules.spells.get(spell);
        if let Some(Target::Unit(u)) = target {
            let units = self.units();
            if let Some(kind) = units.unit_def(u).map(|d| d.kind) {
                if let Some(why) =
                    unit_only_refusal(def.unit_only, kind, units.unit_counts_as_undead(u))
                {
                    return Err(MagicError::Target(why));
                }
            }
        }
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
