//! The turn change: `process_turn` runs the steps in order (the cities, the treasury, the
//! wizards' magic, healing). What each step does is in its domain: city growth in
//! `city/growth.rs`, the treasury in `player/finance.rs`, magic in `wizard/magic_game.rs`, healing in
//! `unit/healing.rs`.

use crate::city::economy::{Farmers, Population};
use crate::game::{Event, Game};
use crate::player::PlayerId;
use rand::SeedableRng;

impl Game {
    pub(crate) fn process_turn(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let mut cities: Vec<_> = self
            .economy()
            .cities()
            .into_iter()
            .map(|(e, ..)| e)
            .collect();
        self.sort_by_serial(&mut cities);

        // Keep enough hands on the farms to feed everyone.
        for &city in &cities {
            if self.world.get::<&Population>(city).map_or(0, |p| p.figures) == 0 {
                continue;
            }
            let min = self.economy().city_min_farmers(city);
            let farmers = self.world.get::<&Farmers>(city).map_or(0, |f| f.0);
            if farmers < min {
                let _ = self.world.insert_one(city, Farmers(min));
            }
        }

        // Upkeep is paid on the cities as they stand, before they grow, as in the original: a
        // city that grows this turn makes its new mouths felt next turn, when the player can
        // still put them on the farms.
        let players: Vec<PlayerId> = crate::player::ids(&self.world);
        // The wizard whose Time Stop lasts pays no gold and eats no food, only the spell's mana.
        let stopper = self.time_stopper();
        for player in players {
            if stopper != Some(player) {
                events.extend(self.settle_treasury(player));
            }
        }

        events.extend(self.process_events());

        for &city in &cities {
            let figures = self.world.get::<&Population>(city).map_or(0, |p| p.figures);
            if figures == 0 {
                match self.grow_outpost(city) {
                    crate::city::growth::OutpostTurn::BecameCity => {
                        let farmers = self.economy().city_min_farmers(city);
                        let _ = self.world.insert_one(city, Farmers(farmers));
                        events.push(Event::CityGrew { city, figures: 1 });
                    }
                    crate::city::growth::OutpostTurn::Lost => {
                        if let Ok(lost) = self.destroy_city(city) {
                            events.extend(lost);
                        }
                    }
                    crate::city::growth::OutpostTurn::Stays => {}
                }
                continue;
            }
            self.autobuild_refresh(city);
            let Some(eco) = self.economy().city_economy(city) else {
                continue;
            };
            if let Some(item) = self.apply_production(city, eco.production) {
                events.push(Event::ProductionCompleted { city, item });
            }
            let (before, mut after) = self.apply_growth(city, eco.growth_tens);
            // Pestilence: each citizen has a tenth of a chance to die, but the last one is spared
            // (the original kills one when the city has more people than a roll of 1 to 10).
            if self.economy().city_has_enchantment(city, "pestilence") {
                let dead = (1..after).filter(|_| self.rng.roll(10) == 1).count() as u32;
                after -= dead;
                let pop = Population {
                    figures: after,
                    ..self.city_population(city)
                };
                let _ = self.world.insert_one(city, pop);
            }
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
        self.tick_temp_effects();
        self.process_globals();
        events.extend(self.process_world_enchantments());
        let mut cooling = rand_pcg::Pcg64Mcg::seed_from_u64(self.next_u64() ^ 0xC001);
        self.cool_volcanoes(&mut cooling);
        self.fade_corruption(&mut cooling);
        self.process_experience();
        events.extend(self.process_magic());
        events.extend(self.process_gaias_blessing());
        self.heal_units();
        events.extend(self.process_chaos_rifts());
        events.extend(self.process_threats());
        events.extend(self.determine_offers());
        events
    }
}

#[cfg(test)]
mod tests;
