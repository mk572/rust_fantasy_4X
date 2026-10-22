//! Healing at the turn change: wounded units mend, slowly in the field, faster in a city, with
//! healers helping. `healing` works out who mends by how much from the unit and city rules and
//! the generator; `Game::heal_units` applies it.

use crate::city::economy::Economy;
use crate::game::rng::GameRng;
use crate::game::Game;
use crate::unit::{Hurt, UnitView};

/// The wounds each hurt unit has afterwards, in the order the units are visited. The draws from
/// `rng` are one per hurt unit that mends by the city's or the field's rate.
pub(crate) fn healing(
    units: &UnitView,
    economy: &Economy,
    rng: &mut GameRng,
) -> Vec<(hecs::Entity, u32)> {
    let all = units.all_units();
    let mut healed_by_healer: Vec<hecs::Entity> = Vec::new();
    // Stacks that contain a healer get an extra fifth of their hit points back.
    for &unit in &all {
        if units.unit_has_ability(unit, "healer") {
            if let Some(at) = units.unit_position(unit) {
                for other in units.units_at(at) {
                    if units.unit_owner(other) == units.unit_owner(unit)
                        && !healed_by_healer.contains(&other)
                    {
                        healed_by_healer.push(other);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for unit in all {
        let hurt = units.world.get::<&Hurt>(unit).map_or(0, |h| h.0);
        let (_, max) = units.unit_hits(unit);
        if hurt == 0 || units.unit_is_death(unit) {
            continue;
        }
        if units.unit_has_ability(unit, "regeneration") {
            out.push((unit, 0));
            continue;
        }
        let owner = units.unit_owner(unit);
        let city = units
            .unit_position(unit)
            .and_then(|p| economy.city_at(p))
            .filter(|&c| economy.city_owner(c) == owner);
        // Stream of Life: every unit in the city heals completely each turn.
        if city.is_some_and(|c| economy.city_has_enchantment(c, "stream_of_life")) {
            out.push((unit, 0));
            continue;
        }
        let divisor = match city {
            Some(c) if economy.city_has_building(c, "animists_guild") => 6,
            Some(_) => 10,
            None => 20,
        };
        let mut heal = max / divisor;
        if rng.roll(divisor) <= max % divisor {
            heal += 1;
        }
        if healed_by_healer.contains(&unit) {
            heal += max / 5;
        }
        out.push((unit, hurt.saturating_sub(heal.max(1))));
    }
    out
}

impl Game {
    /// Wounded units mend (see [`healing`]).
    pub(crate) fn heal_units(&mut self) {
        let units = UnitView::new(&self.world, &self.content, &self.players);
        let economy = Economy::new(&self.world, self.map.as_ref(), &self.content, &self.players);
        let mut plan = healing(&units, &economy, &mut self.rng);
        // While Time Stop lasts, only its wizard's units heal.
        if let Some(keeper) = self.time_stopper() {
            plan.retain(|&(u, _)| self.units().unit_owner(u) == Some(keeper));
        }
        for (unit, hurt) in plan {
            let _ = self.world.insert_one(unit, Hurt(hurt));
        }
    }
}

#[cfg(test)]
mod tests;
