//! Instant overland spells that strike at a wizard or across the whole map (task 56): Drain
//! Power, Cruel Unminding, Subversion, Great Unsummoning, Death Wish and Earthquake. Each is a
//! hook the spell runs (`spell/cast.rs`); the targets they accept are checked by `check_target`.
//!
//! Where the original's help and code disagree the code is followed, except Subversion (see
//! `docs/help-vs-implementation-conflicts.md`, rows B30 to B33).

use crate::battle::immunity::{resistance_failure, Realm};
use crate::game::rng::GameRng;
use crate::game::{Event, Game};
use crate::player::PlayerId;
use crate::rules::UnitKind;
use crate::wizard::magic::{base_casting_skill, skill_points_for, Retort};

pub const DRAIN_POWER: &str = "drain_power";
pub const CRUEL_UNMINDING: &str = "cruel_unminding";
pub const SUBVERSION: &str = "subversion";
pub const GREAT_UNSUMMONING: &str = "great_unsummoning";
pub const DEATH_WISH: &str = "death_wish";
pub const EARTHQUAKE: &str = "earthquake";
pub const BLACK_WIND: &str = "black_wind";
pub const LYCANTHROPY: &str = "lycanthropy";

/// The unit Lycanthropy turns a normal unit into.
const WEREWOLVES: &str = "werewolves";
/// Black Wind's resistance check is at this penalty.
const BLACK_WIND_MODIFIER: i32 = -1;

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
                self.remove_unit(unit, crate::hero::roster::Fate::Slain);
                slain += 1;
            }
        }
        slain
    }

    /// Black Wind on one unit (the spell reaches every unit on the square through
    /// `TargetSel::UnitsOnChosenSquare`): it rolls a resistance check against death at -1 for
    /// each of its figures, and each failure costs it one figure's hit points. Returns whether
    /// the unit died.
    pub(crate) fn black_wind_unit(&mut self, unit: hecs::Entity, rng: &mut GameRng) -> bool {
        use crate::unit::Hurt;
        let units = self.units();
        if units.unit_protected_from(unit, Some(Realm::Death)) {
            return false;
        }
        let figures = units.unit_stat(unit, crate::rules::Stat::Figures).max(1);
        let hits_each = units.unit_stat(unit, crate::rules::Stat::MaxHp).max(0) as u32;
        let resistance = units.unit_resistance_against(unit, Some(Realm::Death));
        let (left, _) = units.unit_hits(unit);
        let failed = (0..figures)
            .filter(|_| resistance_failure(resistance, BLACK_WIND_MODIFIER, rng) > 0)
            .count() as u32;
        let damage = failed * hits_each;
        if damage >= left {
            self.remove_unit(unit, crate::hero::roster::Fate::Slain);
            return true;
        }
        if damage > 0 {
            let hurt = self.world.get::<&Hurt>(unit).map_or(0, |h| h.0);
            let _ = self.world.insert_one(unit, Hurt(hurt + damage));
        }
        false
    }

    /// Whether a spell is aimed at a square and hits what stands on it, in a city or not: any
    /// spell with an effect on `TargetSel::UnitsOnChosenSquare` (Black Wind, Ice Storm, Fire
    /// Storm). Such a spell needs an enemy unit on the square to be cast.
    pub fn is_square_spell(&self, spell: crate::rules::SpellId) -> bool {
        self.content
            .rules
            .spells
            .get(spell)
            .effects
            .iter()
            .any(|e| e.target == crate::rules::effects::TargetSel::UnitsOnChosenSquare)
    }

    /// Whether Lycanthropy may be cast on the unit: a normal land unit that is not undead.
    /// (The original refuses heroes, fantastic creatures and the mutated; ships are refused
    /// here too, which its test does not do.)
    pub fn can_become_werewolf(&self, unit: hecs::Entity) -> bool {
        let units = self.units();
        let Some(def) = units.unit_def(unit) else {
            return false;
        };
        def.kind == UnitKind::Normal
            && def.realm.is_none()
            && def.movement != crate::rules::MovementType::Sailing
            && !units.unit_counts_as_undead(unit)
            && self.content.rules.units.id_of(WEREWOLVES).is_some()
    }

    /// Lycanthropy: the unit becomes Werewolves for good, starting again from no experience. It
    /// keeps the damage it has taken and its spells. Returns what it was and what it is.
    pub(crate) fn lycanthropy(
        &mut self,
        unit: hecs::Entity,
    ) -> Option<(crate::rules::UnitId, crate::rules::UnitId)> {
        if !self.can_become_werewolf(unit) {
            return None;
        }
        let to = self.content.rules.units.id_of(WEREWOLVES)?;
        let from = self.world.get::<&crate::unit::Unit>(unit).ok()?.0;
        let _ = self.world.insert_one(unit, crate::unit::Unit(to));
        let _ = self.world.remove_one::<crate::unit::Experience>(unit);
        let (left, _) = self.units().unit_hits(unit);
        if left == 0 {
            // More damage than the new body can take: it is left on its last hit point.
            let max = self
                .units()
                .unit_stat(unit, crate::rules::Stat::MaxHp)
                .max(0) as u32
                * self
                    .units()
                    .unit_stat(unit, crate::rules::Stat::Figures)
                    .max(0) as u32;
            let _ = self
                .world
                .insert_one(unit, crate::unit::Hurt(max.saturating_sub(1)));
        }
        Some((from, to))
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
        let (units_lost, buildings_lost) = self.shake_city(
            city,
            EARTHQUAKE_UNIT_PERCENT,
            EARTHQUAKE_BUILDING_PERCENT,
            rng,
        );
        Event::EarthquakeStruck {
            city,
            units_lost,
            buildings_lost,
        }
    }

    /// Tremors in a city: each unit that neither flies nor lacks a body dies with `unit_percent`,
    /// each building falls with `building_percent`. Returns the units and buildings lost.
    pub(crate) fn shake_city(
        &mut self,
        city: hecs::Entity,
        unit_percent: u32,
        building_percent: u32,
        rng: &mut GameRng,
    ) -> (u32, u32) {
        let at = self.world.get::<&crate::map::Position>(city).map(|p| *p);
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
                if rng.roll(100) <= unit_percent {
                    self.remove_unit(unit, crate::hero::roster::Fate::Slain);
                    units_lost += 1;
                }
            }
        }
        let built: Vec<_> = self
            .world
            .get::<&crate::city::economy::Buildings>(city)
            .map(|b| b.0.clone())
            .unwrap_or_default();
        let mut buildings_lost = 0;
        for b in built {
            if rng.roll(100) <= building_percent && self.destroy_building(city, b) {
                buildings_lost += 1;
            }
        }
        (units_lost, buildings_lost)
    }
}

#[cfg(test)]
mod tests;
