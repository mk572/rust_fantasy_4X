//! Lasting spells that cannot be combined, as the original refuses them (`Cast_Spell_Overland`
//! in `OverSpel.c`, `Questionable_Spell_Cast` in `SBookScr.c`): a spell the unit or city has
//! already, one it is not cumulative with, and one the unit's own ability makes pointless.
//!
//! Tests: in spell/stacking/tests.rs.

use crate::game::Game;
use crate::rules::exec::Target;
use crate::rules::{Duration, SpellId, SpellTarget};
use crate::wizard::magic::MagicError;

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
                if spells.unit_has(u, spell) {
                    return Err(MagicError::AlreadyEnchanted);
                }
                let rival = def.not_with.iter().filter_map(|id| {
                    self.content
                        .rules
                        .spells
                        .id_of(id)
                        .map(|s| (s, id.as_str()))
                });
                for (other, id) in rival {
                    if spells.unit_has(u, other) || self.has_global(player, id) {
                        return Err(MagicError::NotCumulative);
                    }
                }
                let own = self.units().unit_def(u).map(|d| d.abilities.as_slice());
                if own.is_some_and(|own| def.already_able.iter().any(|a| own.contains(a))) {
                    return Err(MagicError::AlreadyAble);
                }
                Ok(())
            }
            (SpellTarget::OwnCity | SpellTarget::EnemyCity, Some(Target::City(c)))
                if self.economy().city_has_enchantment(c, &def.id) =>
            {
                Err(MagicError::AlreadyEnchanted)
            }
            _ => Ok(()),
        }
    }
}

impl crate::unit::UnitView<'_> {
    /// Whether a spell on the unit gives way to a stronger one it is not cumulative with, one of
    /// the unit's own enchantments or one its items carry (Stone Skin under Iron Skin).
    pub(crate) fn unit_spell_superseded(&self, unit: hecs::Entity, spell: SpellId) -> bool {
        let def = self.content.rules.spells.get(spell);
        def.not_with.iter().any(|id| {
            self.content.rules.spells.id_of(id).is_some_and(|other| {
                crate::spell::instance::Spells::new(self.world).unit_has(unit, other)
                    || self.items().spells(unit).contains(&other)
            })
        })
    }
}

#[cfg(test)]
mod tests;
