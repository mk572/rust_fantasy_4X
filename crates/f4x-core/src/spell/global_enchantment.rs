//! What a wizard's global enchantments do to the rest of the game.
//!
//! The spells are spell instances with the target `Global` (`spell/instance.rs`); the code that cares asks here. Plain stat changes come
//! from the spell's own passive rules (with a condition such as "normal units"); the rest is
//! recognised by spell id.

use crate::game::Game;
use crate::player::PlayerId;
use crate::rules::effects::{Effect, Trigger};
use crate::rules::exec::{eval, Facts};
use crate::rules::{Modifier, Source, SpellId};
use crate::spell::instance::SpellTarget;
use crate::unit::{Hurt, UnitView};
use crate::wizard::magic::MagicError;

/// Does this wizard keep the global spell with this id going?
pub(crate) fn magic_has_global(wizard: &crate::wizard::entity::Wizard<'_>, id: SpellId) -> bool {
    wizard.any_global(|s| s == id)
}

pub use f4x_def::rules::global_effect::{GlobalEffect, GLOBAL_EFFECT};

/// Does this wizard keep a global spell with this effect going?
pub(crate) fn magic_has_effect(
    wizard: &crate::wizard::entity::Wizard<'_>,
    rules: &crate::rules::Rules,
    effect: GlobalEffect,
) -> bool {
    wizard.any_global(|s| effect.is_in(rules.spells.get(s)))
}

impl Game {
    /// A lasting spell starts on the whole world, cast by `player`'s wizard; it is there once.
    pub(crate) fn add_global(&mut self, player: PlayerId, spell: SpellId) {
        self.add_enchantment(SpellTarget::Global, spell, player);
    }

    /// Turn off one of the wizard's own global enchantments.
    pub(crate) fn turn_off(&mut self, player: PlayerId, spell: SpellId) -> Result<(), MagicError> {
        if self.wizard(player).is_none() {
            return Err(MagicError::UnknownPlayer);
        }
        if self.turn_off_enchantment(SpellTarget::Global, player, spell) {
            Ok(())
        } else {
            Err(MagicError::NotAvailable)
        }
    }

    /// Every wizard's lasting spells on the world end (Disjunction).
    pub(crate) fn end_all_globals(&mut self) {
        for instance in self.spells().on(SpellTarget::Global) {
            self.despawn_spell(instance);
        }
    }

    /// A wizard's lasting spells on the world all end, for a test that starts again.
    #[cfg(test)]
    pub(crate) fn clear_globals(&mut self, player: PlayerId) {
        if let Some(caster) = self.wizard(player).map(|w| w.entity()) {
            for instance in self.spells().globals_of(caster) {
                self.despawn_spell(instance);
            }
        }
    }

    /// Does `player` keep the global spell with this id going?
    pub fn has_global(&self, player: PlayerId, id: SpellId) -> bool {
        self.wizard(player)
            .is_some_and(|m| magic_has_global(&m, id))
    }

    /// Does `player` keep a global spell with this effect going?
    pub fn has_global_effect(&self, player: PlayerId, effect: GlobalEffect) -> bool {
        self.wizard(player)
            .is_some_and(|m| magic_has_effect(&m, &self.content.rules, effect))
    }

    /// Does any wizard keep a global spell with this effect going?
    pub fn any_global_effect(&self, effect: GlobalEffect) -> bool {
        self.held_global_effect(effect).is_some()
    }

    /// The spell that gives the effect, if some wizard keeps it going.
    pub fn held_global_effect(&self, effect: GlobalEffect) -> Option<SpellId> {
        let spells = self.spells();
        self.player_ids()
            .into_iter()
            .filter_map(|p| self.wizard(p).map(|w| w.entity()))
            .flat_map(|w| spells.globals_of(w))
            .filter_map(|i| spells.def(i))
            .find(|&s| effect.is_in(self.content.rules.spells.get(s)))
    }

    /// The instance of `spell` that `player` keeps going on the whole world: what Disjunction
    /// and Spell Binding take as their target.
    pub fn global_instance(&self, player: PlayerId, spell: SpellId) -> Option<hecs::Entity> {
        let wizard = self.wizard(player)?.entity();
        let spells = self.spells();
        let held = spells.globals_of(wizard);
        held.into_iter().find(|&i| spells.def(i) == Some(spell))
    }

    /// The global enchantments of every wizard but `player`, wizard by wizard and oldest first:
    /// the list Disjunction and Spell Binding choose from.
    pub fn rival_globals(&self, player: PlayerId) -> Vec<hecs::Entity> {
        let spells = self.spells();
        self.player_ids()
            .into_iter()
            .filter(|&p| p != player)
            .filter_map(|p| self.wizard(p).map(|w| w.entity()))
            .flat_map(|w| spells.globals_of(w))
            .collect()
    }

    /// The wizard whose Time Stop is stopping everyone else's time, if any.
    pub fn time_stopper(&self) -> Option<PlayerId> {
        self.player_ids()
            .into_iter()
            .find(|&p| self.has_global_effect(p, GlobalEffect::TimeStop))
    }

    /// Does any wizard keep the global spell with this id going?
    pub fn any_global(&self, id: SpellId) -> bool {
        self.player_ids()
            .into_iter()
            .any(|p| self.has_global(p, id))
    }

    /// A wizard's fame as the rules count it: what they have earned, and ten more while Just
    /// Cause is kept going.
    pub fn fame(&self, player: PlayerId) -> i32 {
        self.units().fame(player)
    }

    /// Is anyone's Evil Omens, the caster's own included, making life and nature spells
    /// dearer?
    pub(crate) fn evil_omens_active(&self) -> bool {
        self.units().any_global_effect(GlobalEffect::EvilOmens)
    }

    /// The spell each other wizard is casting, as far as `viewer`'s Detect Magic shows it.
    /// Empty without the spell.
    pub fn detect_magic_view(&self, viewer: PlayerId) -> Vec<(PlayerId, SpellId)> {
        if !self.has_global_effect(viewer, GlobalEffect::DetectMagic) {
            return Vec::new();
        }
        self.player_ids()
            .into_iter()
            .filter(|&p| p != viewer)
            .filter_map(|p| Some((p, self.wizard(p)?.casting()?.spell)))
            .collect()
    }

    /// Explore what Awareness and Nature Awareness show `player`.
    pub(crate) fn reveal_for_globals(&mut self, player: PlayerId) {
        let Some(map) = self.map.as_ref() else {
            return;
        };
        let (w, h) = (map.width(), map.height());
        if self.has_global_effect(player, GlobalEffect::NatureAwareness) {
            for plane in crate::map::Plane::all() {
                // One square large enough to cover the whole plane.
                let at = crate::map::Position {
                    plane,
                    x: w / 2,
                    y: h / 2,
                };
                self.fog.explore(
                    player,
                    at,
                    crate::map::fog::sight_for(w.max(h)),
                    self.settings.basic.diagonal_extra_cost,
                );
            }
        } else if self.has_global_effect(player, GlobalEffect::Awareness) {
            let cities: Vec<_> = self
                .economy()
                .cities()
                .into_iter()
                .map(|(_, _, p)| p)
                .collect();
            for at in cities {
                self.fog.explore(
                    player,
                    at,
                    crate::map::fog::sight_for(1),
                    self.settings.basic.diagonal_extra_cost,
                );
            }
        }
    }

    /// Once a turn: Herb Mastery heals every unit, and the revealing spells look again.
    pub(crate) fn process_globals(&mut self) {
        let ids: Vec<PlayerId> = crate::player::ids(&self.world);
        for player in ids {
            if self.has_global_effect(player, GlobalEffect::HerbMastery) {
                for unit in self.units().player_units(player) {
                    if !self.units().unit_is_death(unit) {
                        self.put(unit, Hurt(0));
                    }
                }
            }
            self.reveal_for_globals(player);
        }
        // Cities explore what their sight reaches (an Oracle or Nature's Eye widens it).
        let cities: Vec<_> = self.economy().cities().into_iter().collect();
        for (entity, city, at) in cities {
            let sight = self.economy().city_sight(entity);
            self.fog.explore(
                city.owner,
                at,
                sight,
                self.settings.basic.diagonal_extra_cost,
            );
        }
    }
}

impl UnitView<'_> {
    /// Modifiers a unit gets from its owner's global spells: rules with a condition are
    /// checked against the unit, and Charm of Life adds a quarter of its hit points.
    pub(crate) fn global_unit_modifiers(&self, unit: hecs::Entity) -> Vec<Modifier> {
        let (Some(owner), Some(def)) = (self.unit_owner(unit), self.unit_def(unit)) else {
            return Vec::new();
        };
        let Some(magic) = self.wizard(owner) else {
            return Vec::new();
        };
        let globals = magic.global();
        if globals.is_empty() {
            return Vec::new();
        }
        let facts = Facts {
            abilities: self.unit_abilities(unit),
            realm: self.unit_realm(unit),
            // The undead are not normal units: Holy Arms and Crusade pass them by.
            kind: Some(if self.unit_is_undead(unit) {
                crate::rules::UnitKind::Summoned
            } else {
                def.kind
            }),
            ..Facts::default()
        };
        let mut mods = Vec::new();
        for &spell in &globals {
            let sdef = self.content.rules.spells.get(spell);
            for rule in &sdef.effects {
                if rule.when != Trigger::Passive || !eval(&rule.condition, &facts) {
                    continue;
                }
                if let Effect::Modify {
                    stat,
                    op,
                    value,
                    types,
                } = rule.effect
                {
                    mods.push(
                        Modifier::amount(stat, op, value, Source::Spell(spell)).of_types(types),
                    );
                }
            }
        }
        mods
    }
}
