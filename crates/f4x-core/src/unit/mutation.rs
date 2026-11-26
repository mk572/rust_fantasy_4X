//! Chaos mutations (Chaos Channels, and Doom Mastery for every newly trained normal unit).
//!
//! A normal unit that is channelled becomes a creature of chaos and gains one of three
//! mutations, chosen at random from those that suit it: demon-skin armour (+3 defence, as the
//! original's code does; the help says +2), demon wings (flight, at least 2 moves) or fiery
//! breath (a strength 2 ranged attack, for units with no ranged attack of their own). It can
//! never be channelled again. The changes are permanent temporary effects of the unit
//! (`spell/temp_effect.rs`), so they are saved and read by every stat; the marker ability
//! [`MUTATED`] makes the unit a creature of chaos (`UnitView::unit_realm`).
//!
//! Tests: in spell/wizard/tests.rs.

use crate::rules::{Ability, MutationRules};
use crate::rules::{ModOp, Modifier, MovementType, Source, Stat, UnitKind};
use crate::spell::target_error::TargetError;
use crate::unit::UnitView;
use crate::world::rng::GameRng;
use crate::world::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutation {
    Armor,
    Wings,
    Breath,
}

impl Mutation {
    /// The ability that names this mutation on a unit, for the unit sheet.
    pub fn marker(self, rules: &MutationRules) -> Ability {
        Ability::new(match self {
            Mutation::Armor => rules.armor_marker,
            Mutation::Wings => rules.wings_marker,
            Mutation::Breath => rules.breath_marker,
        })
    }
}

impl UnitView<'_> {
    /// Whether the unit has been mutated by chaos.
    pub fn unit_is_mutated(&self, unit: hecs::Entity) -> bool {
        self.unit_has_ability(
            unit,
            Ability::new(self.content.rules.config.abilities.mutation.mutated),
        )
    }
}

impl World {
    /// Chaos Channels works on a normal unit or a hero that is not undead and not mutated yet.
    pub fn check_mutate(&self, unit: hecs::Entity) -> Result<(), TargetError> {
        let units = self.units();
        let def = units.unit_def(unit).ok_or(TargetError::WrongTarget)?;
        if !matches!(def.kind, UnitKind::Normal | UnitKind::Hero) {
            return Err(TargetError::NotNormalUnit);
        }
        if units.unit_counts_as_undead(unit) {
            return Err(TargetError::Undead);
        }
        if units.unit_is_mutated(unit) {
            return Err(TargetError::AlreadyMutated);
        }
        Ok(())
    }

    /// The mutations that suit the unit: wings not for a flier or a ship, breath only for a unit
    /// with no ranged attack (a weak thrown one will do).
    fn mutations_for(&self, unit: hecs::Entity) -> Vec<Mutation> {
        let units = self.units();
        let mut options = vec![Mutation::Armor];
        let rules = &self.content.rules.config.abilities.mutation;
        let flies = units
            .unit_def(unit)
            .is_some_and(|d| matches!(d.movement, MovementType::Flying | MovementType::Sailing))
            || units.unit_has_ability(unit, Ability::new(rules.wings_ability));
        if !flies {
            options.push(Mutation::Wings);
        }
        let ranged = units.unit_stat(unit, Stat::Ranged);
        if ranged <= 0
            || (units.unit_has_ability(unit, Ability::new(rules.thrown_ability))
                && ranged <= rules.thrown_most)
        {
            options.push(Mutation::Breath);
        }
        options
    }

    /// Mutate the unit with a random suitable mutation; `None` if it cannot be mutated.
    pub(crate) fn mutate(&mut self, unit: hecs::Entity, rng: &mut GameRng) -> Option<Mutation> {
        if self.check_mutate(unit).is_err() {
            return None;
        }
        let options = self.mutations_for(unit);
        let pick = options[rng.roll(options.len() as u32) as usize - 1];
        let channels =
            crate::spell::unit_effect::UnitEffect::ChaosChannels.spell_in(&self.content.rules)?;
        let source = Source::Spell(channels);
        let rules = self.content.rules.config.abilities.mutation.clone();
        let who = crate::rules::exec::Target::Unit(unit);
        match pick {
            Mutation::Armor => self.add_temp_modifier(
                who,
                Modifier::new(Stat::Defense, ModOp::Add, rules.armor_defense, source),
            ),
            Mutation::Wings => {
                self.grant_temp_ability(who, Ability::new(rules.wings_ability));
                self.add_temp_modifier(
                    who,
                    Modifier::new(Stat::Moves, ModOp::Min, rules.wings_moves, source),
                );
            }
            Mutation::Breath => {
                self.grant_temp_ability(who, Ability::new(rules.breath_ability));
                for (stat, op, value) in [
                    (Stat::Ranged, ModOp::Set, rules.breath_strength),
                    (Stat::Ammo, ModOp::Min, rules.breath_ammo),
                ] {
                    self.add_temp_modifier(who, Modifier::new(stat, op, value, source));
                }
            }
        }
        self.grant_temp_ability(who, Ability::new(rules.mutated));
        self.grant_temp_ability(who, pick.marker(&rules));
        Some(pick)
    }

    /// Doom Mastery: a newly trained unit of a wizard who keeps it going is mutated.
    pub(crate) fn doom_mastery_mutates(
        &mut self,
        unit: hecs::Entity,
        owner: crate::player::PlayerId,
    ) {
        if !self.has_global_effect(
            owner,
            crate::spell::global_enchantment::GlobalEffect::DoomMastery,
        ) || self.check_mutate(unit).is_err()
        {
            return;
        }
        let mut rng = std::mem::replace(&mut self.rng, GameRng::lent());
        self.mutate(unit, &mut rng);
        self.rng = rng;
    }
}
