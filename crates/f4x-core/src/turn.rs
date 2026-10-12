//! The turn change: `process_turn` runs the steps in order (the cities, the treasury, the
//! wizards' magic, healing). What each step does is in its domain: city growth in
//! `city_growth.rs`, the treasury in `finance.rs`, magic in `magic_game.rs`, healing in
//! `healing.rs`.

use crate::city::PlayerId;
use crate::economy::{Farmers, Population};
use crate::{Event, Game};

impl Game {
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
}

#[cfg(test)]
mod tests;
