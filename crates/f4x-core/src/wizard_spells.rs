//! Instant overland spells that strike at a wizard or across the whole map (task 56): Drain
//! Power, Cruel Unminding, Subversion, Great Unsummoning, Death Wish and Earthquake. Each is a
//! hook the spell runs (`cast.rs`); the targets they accept are checked by `check_target`.
//!
//! Where the original's help and code disagree the code is followed, except Subversion (see
//! `docs/help-vs-implementation-conflicts.md`, rows B30 to B33).

use crate::battle::immunity::{resistance_failure, Realm};
use crate::city::PlayerId;
use crate::magic::{base_casting_skill, skill_points_for, Retort};
use crate::rng::GameRng;
use crate::rules::UnitKind;
use crate::{Event, Game};

pub const DRAIN_POWER: &str = "drain_power";
pub const CRUEL_UNMINDING: &str = "cruel_unminding";
pub const SUBVERSION: &str = "subversion";
pub const GREAT_UNSUMMONING: &str = "great_unsummoning";
pub const DEATH_WISH: &str = "death_wish";
pub const EARTHQUAKE: &str = "earthquake";

/// Drain Power takes the sum of this many rolls of a twenty-sided die (`Apply_Drain_Power`).
const DRAIN_ROLLS: u32 = 10;
const DRAIN_DIE: u32 = 20;
/// Great Unsummoning's penalty to the creatures' resistance (a roll against resistance minus 2).
const UNSUMMONING_MODIFIER: i32 = -2;
/// Earthquake: the chance in a hundred that a building falls, and that a unit that cannot fly or
/// slip through the ground dies (the original's code rolls one in four for units).
const EARTHQUAKE_BUILDING_PERCENT: u32 = 15;
const EARTHQUAKE_UNIT_PERCENT: u32 = 25;

impl Game {
    /// Drain Power: the victim loses 10 to 200 mana (not more than they have); returns what
    /// was taken.
    pub(crate) fn drain_power(&mut self, victim: PlayerId, rng: &mut GameRng) -> i32 {
        let wanted: i32 = (0..DRAIN_ROLLS).map(|_| rng.roll(DRAIN_DIE) as i32).sum();
        let Some(p) = self.player_mut(victim) else {
            return 0;
        };
        let taken = wanted.min(p.mana.max(0));
        p.mana -= taken;
        taken
    }

    /// Cruel Unminding: the victim permanently loses 1 to 10 percent of their casting skill (at
    /// least a point). Returns the skill lost.
    pub(crate) fn cruel_unminding(&mut self, victim: PlayerId, rng: &mut GameRng) -> i32 {
        let Some(magic) = self.magic_mut(victim) else {
            return 0;
        };
        let archmage = magic.has(Retort::Archmage);
        let skill = magic.casting_skill();
        let lost = (skill * rng.roll(10) as i32 / 100).max(1);
        // An archmage's ten extra points are not part of what the spell eats into.
        let target = (skill - lost - if archmage { 10 } else { 0 }).max(0);
        magic.skill_points = magic.skill_points.min(skill_points_for(target, false));
        let now = base_casting_skill(magic.skill_points, archmage);
        magic.skill_left = magic.skill_left.min(now);
        skill - now
    }

    /// The units on the map that the spell may touch, with the realm each is judged by.
    fn all_units_with_owner(&self) -> Vec<(hecs::Entity, PlayerId)> {
        let units = self.units();
        self.players
            .iter()
            .flat_map(|p| units.player_units(p.id).into_iter().map(|u| (u, p.id)))
            .collect()
    }

    /// Slay the units `chosen` picks that fail a resistance check against `realm`. Returns how
    /// many died.
    fn slay_units(
        &mut self,
        realm: Realm,
        modifier: i32,
        rng: &mut GameRng,
        chosen: impl Fn(&Game, hecs::Entity, PlayerId) -> bool,
    ) -> u32 {
        let mut slain = 0;
        for (unit, owner) in self.all_units_with_owner() {
            if !chosen(self, unit, owner) {
                continue;
            }
            if self.units().unit_protected_from(unit, Some(realm)) {
                continue;
            }
            let resistance = self.units().unit_resistance_against(unit, Some(realm));
            if resistance_failure(resistance, modifier, rng) > 0 {
                let _ = self.world.despawn(unit);
                slain += 1;
            }
        }
        slain
    }

    /// Great Unsummoning: every fantastic creature on both planes, the caster's too, must resist
    /// at -2 or be banished.
    pub(crate) fn great_unsummoning(&mut self, rng: &mut GameRng) -> u32 {
        self.slay_units(Realm::Sorcery, UNSUMMONING_MODIFIER, rng, |g, u, _| {
            g.units().unit_realm(u).is_some()
        })
    }

    /// Death Wish: every normal unit of the other wizards must resist or die.
    pub(crate) fn death_wish(&mut self, caster: PlayerId, rng: &mut GameRng) -> u32 {
        self.slay_units(Realm::Death, 0, rng, |g, u, owner| {
            owner != caster
                && g.units().unit_realm(u).is_none()
                && g.units()
                    .unit_def(u)
                    .is_some_and(|d| d.kind == UnitKind::Normal)
        })
    }

    /// Earthquake: every unit in the city that neither flies nor has no body has a one in four
    /// chance of dying, and each building a 15 percent chance of falling (unless another needs
    /// it). Returns the units and buildings lost.
    pub(crate) fn earthquake(&mut self, city: hecs::Entity, rng: &mut GameRng) -> Event {
        let at = self.world.get::<&crate::city::Position>(city).map(|p| *p);
        let mut units_lost = 0;
        if let Ok(at) = at {
            for unit in self.units().units_at(at) {
                let units = self.units();
                let flies = units
                    .unit_def(unit)
                    .is_some_and(|d| d.movement == crate::rules::MovementType::Flying);
                if flies
                    || units.unit_has_ability(unit, "flying")
                    || units.unit_has_ability(unit, "non_corporeal")
                {
                    continue;
                }
                if rng.roll(100) <= EARTHQUAKE_UNIT_PERCENT {
                    let _ = self.world.despawn(unit);
                    units_lost += 1;
                }
            }
        }
        let built: Vec<_> = self
            .world
            .get::<&crate::economy::Buildings>(city)
            .map(|b| b.0.clone())
            .unwrap_or_default();
        let mut buildings_lost = 0;
        for b in built {
            if rng.roll(100) <= EARTHQUAKE_BUILDING_PERCENT && self.destroy_building(city, b) {
                buildings_lost += 1;
            }
        }
        Event::EarthquakeStruck {
            city,
            units_lost,
            buildings_lost,
        }
    }
}

#[cfg(test)]
mod tests;
