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

use crate::game::rng::GameRng;
use crate::game::Game;
use crate::rules::{ModOp, Modifier, MovementType, Source, Stat, UnitKind};
use crate::spell::target_error::TargetError;
use crate::unit::UnitView;

/// Hook of Chaos Channels.
pub const CHAOS_CHANNELS: &str = "chaos_channels";
/// The ability that marks a unit as mutated.
pub const MUTATED: &str = "chaos_channeled";
/// The ability that says which mutation a unit has, for the unit sheet to name it.
pub const MUTATION_MARKERS: [(Mutation, &str); 3] = [
    (Mutation::Armor, "mutation_armor"),
    (Mutation::Wings, "mutation_wings"),
    (Mutation::Breath, "mutation_breath"),
];

/// Defence the demon-skin armour adds.
const ARMOR_DEFENSE: i32 = 3;
/// Moves (whole) a unit with demon wings has at least.
const WINGS_MOVES: i32 = 2;
/// Strength of the fire breath.
const BREATH_STRENGTH: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutation {
    Armor,
    Wings,
    Breath,
}

impl UnitView<'_> {
    /// Whether the unit has been mutated by chaos.
    pub fn unit_is_mutated(&self, unit: hecs::Entity) -> bool {
        self.unit_has_ability(unit, MUTATED)
    }
}

impl Game {
    /// Chaos Channels works on a normal unit that is not undead and not mutated yet.
    pub fn check_mutate(&self, unit: hecs::Entity) -> Result<(), TargetError> {
        let units = self.units();
        let def = units.unit_def(unit).ok_or(TargetError::WrongTarget)?;
        if def.kind != UnitKind::Normal {
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
        let flies = units
            .unit_def(unit)
            .is_some_and(|d| matches!(d.movement, MovementType::Flying | MovementType::Sailing))
            || units.unit_has_ability(unit, "flying");
        if !flies {
            options.push(Mutation::Wings);
        }
        let ranged = units.unit_stat(unit, Stat::Ranged);
        if ranged <= 0 || (units.unit_has_ability(unit, "thrown") && ranged <= 3) {
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
        let source = Source::Spell(self.content.rules.spells.id_of(CHAOS_CHANNELS)?);
        let who = crate::rules::exec::Target::Unit(unit);
        match pick {
            Mutation::Armor => self.add_temp_modifier(
                who,
                Modifier::new(Stat::Defense, ModOp::Add, ARMOR_DEFENSE, source),
                None,
            ),
            Mutation::Wings => {
                self.grant_temp_ability(who, "flying", None);
                self.add_temp_modifier(
                    who,
                    Modifier::new(Stat::Moves, ModOp::Min, WINGS_MOVES, source),
                    None,
                );
            }
            Mutation::Breath => {
                self.grant_temp_ability(who, "fire_breath", None);
                for (stat, op, value) in [
                    (Stat::Ranged, ModOp::Set, BREATH_STRENGTH),
                    (Stat::Ammo, ModOp::Min, 1),
                ] {
                    self.add_temp_modifier(who, Modifier::new(stat, op, value, source), None);
                }
            }
        }
        self.grant_temp_ability(who, MUTATED, None);
        if let Some((_, marker)) = MUTATION_MARKERS.iter().find(|(m, _)| *m == pick) {
            self.grant_temp_ability(who, marker, None);
        }
        Some(pick)
    }

    /// Doom Mastery: a newly trained unit of a wizard who keeps it going is mutated.
    pub(crate) fn doom_mastery_mutates(
        &mut self,
        unit: hecs::Entity,
        owner: crate::player::PlayerId,
    ) {
        if !self.has_global(owner, "doom_mastery") || self.check_mutate(unit).is_err() {
            return;
        }
        let mut rng = std::mem::replace(&mut self.rng, GameRng::lent());
        self.mutate(unit, &mut rng);
        self.rng = rng;
    }
}
