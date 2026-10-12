//! The turn change: `process_turn` runs the steps in order (the cities, the treasury, the
//! wizards' magic, healing). What each step does is in its domain: city growth in
//! `city_growth.rs`, the treasury in `finance.rs`, magic in `magic_game.rs`. Unit upkeep and
//! healing stay here until the unit rules move (task 185).

use crate::city::PlayerId;
use crate::economy::{Farmers, Population};
use crate::rules::UnitKind;
use crate::units::{Hurt, Owner};
use crate::{Event, Game};

impl Game {
    /// Food a unit eats per turn.
    pub fn unit_food_upkeep(&self, unit: hecs::Entity) -> i32 {
        if self.unit_is_undead(unit) {
            return 0;
        }
        match self.unit_def(unit).map(|d| d.kind) {
            Some(UnitKind::Normal) => 1,
            _ => 0,
        }
    }

    /// Gold a unit costs per turn (heroes with the noble trait cost nothing).
    pub fn unit_gold_upkeep(&self, unit: hecs::Entity) -> i32 {
        let Some(def) = self.unit_def(unit) else {
            return 0;
        };
        if self.unit_is_undead(unit) {
            return 0;
        }
        match def.kind {
            UnitKind::Normal => def.upkeep.gold as i32,
            UnitKind::Hero if !self.unit_has_ability(unit, "noble") => def.upkeep.gold as i32,
            _ => 0,
        }
    }

    pub fn player_units(&self, player: PlayerId) -> Vec<hecs::Entity> {
        self.all_units()
            .into_iter()
            .filter(|&u| self.unit_owner(u) == Some(player))
            .collect()
    }

    pub(crate) fn process_turn(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let mut cities: Vec<_> = self.cities().into_iter().map(|(e, ..)| e).collect();
        self.sort_by_serial(&mut cities);

        // Keep enough hands on the farms to feed everyone.
        for &city in &cities {
            if self.world.get::<&Population>(city).map_or(0, |p| p.figures) == 0 {
                continue;
            }
            let min = self.city_min_farmers(city);
            let farmers = self.world.get::<&Farmers>(city).map_or(0, |f| f.0);
            if farmers < min {
                let _ = self.world.insert_one(city, Farmers(min));
            }
        }

        // Upkeep is paid on the cities as they stand, before they grow, as in the original: a
        // city that grows this turn makes its new mouths felt next turn, when the player can
        // still put them on the farms.
        let players: Vec<PlayerId> = self.players.iter().map(|p| p.id).collect();
        for &player in &players {
            // Nobody sets an independent city's farmers, so it puts enough on the farms to
            // feed its garrison, as a careful ruler would.
            if self.is_neutral_player(player) {
                self.balance_farmers(player);
                self.balance_taxes(player);
            }
        }
        for player in players {
            events.extend(self.settle_treasury(player));
        }

        for &city in &cities {
            let figures = self.world.get::<&Population>(city).map_or(0, |p| p.figures);
            if figures == 0 {
                if self.grow_outpost(city) {
                    let farmers = self.city_min_farmers(city);
                    let _ = self.world.insert_one(city, Farmers(farmers));
                    events.push(Event::CityGrew { city, figures: 1 });
                }
                continue;
            }
            let Some(eco) = self.city_economy(city) else {
                continue;
            };
            if let Some(item) = self.apply_production(city, eco.production) {
                events.push(Event::ProductionCompleted { city, item });
            }
            let (before, after) = self.apply_growth(city, eco.growth_tens);
            if after > before {
                events.push(Event::CityGrew {
                    city,
                    figures: after,
                });
            } else if after < before {
                events.push(Event::CityShrank {
                    city,
                    figures: after,
                });
            }
        }
        events.extend(self.tick_timers());
        self.tick_temp_effects();
        self.process_globals();
        self.process_experience();
        events.extend(self.process_magic());
        events.extend(self.process_gaias_blessing());
        self.heal_units();
        events.extend(self.process_chaos_rifts());
        events
    }

    /// Wounded units mend: slowly in the field, faster in a city, with healers helping.
    fn heal_units(&mut self) {
        let rules_units: Vec<_> = self.all_units();
        let mut healed_by_healer: Vec<hecs::Entity> = Vec::new();
        // Stacks that contain a healer get an extra fifth of their hit points back.
        for &unit in &rules_units {
            if self.unit_has_ability(unit, "healer") {
                if let Some(at) = self.unit_position(unit) {
                    for other in self.units_at(at) {
                        if self.unit_owner(other) == self.unit_owner(unit)
                            && !healed_by_healer.contains(&other)
                        {
                            healed_by_healer.push(other);
                        }
                    }
                }
            }
        }
        for unit in rules_units {
            let hurt = self.world.get::<&Hurt>(unit).map_or(0, |h| h.0);
            let (_, max) = self.unit_hits(unit);
            if hurt == 0 || self.unit_is_death(unit) {
                continue;
            }
            if self.unit_has_ability(unit, "regeneration") {
                let _ = self.world.insert_one(unit, Hurt(0));
                continue;
            }
            let at = self.unit_position(unit);
            let owner = self.world.get::<&Owner>(unit).ok().map(|o| o.0);
            let city = at
                .and_then(|p| self.city_at(p))
                .filter(|&c| self.city_owner(c) == owner);
            // Stream of Life: every unit in the city heals completely each turn.
            if city.is_some_and(|c| self.city_has_enchantment(c, "stream_of_life")) {
                let _ = self.world.insert_one(unit, Hurt(0));
                continue;
            }
            let divisor = match city {
                Some(c) if self.city_has_building(c, "animists_guild") => 6,
                Some(_) => 10,
                None => 20,
            };
            let mut heal = max / divisor;
            if self.roll(divisor) <= max % divisor {
                heal += 1;
            }
            if healed_by_healer.contains(&unit) {
                heal += max / 5;
            }
            let _ = self
                .world
                .insert_one(unit, Hurt(hurt.saturating_sub(heal.max(1))));
        }
    }

    /// Whether the unit counts towards a city's garrison for keeping order.
    pub fn is_garrison(&self, unit: hecs::Entity) -> bool {
        self.unit_def(unit)
            .is_some_and(|d| matches!(d.kind, UnitKind::Normal | UnitKind::Hero))
    }
}

#[cfg(test)]
mod tests;
