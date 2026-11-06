//! The turn lifecycle: [`Game::start_round`], [`Game::start_player_turn`],
//! [`Game::end_player_turn`] and [`Game::end_round`] (task 477, `docs/turn-structure.md`).
//! `f4x-master` calls them in order; `Command::EndTurn` composes them for all players. What each step does is in its domain: city growth in
//! `city/growth.rs`, the treasury in `player/finance.rs`, magic in `wizard/magic_game.rs`, healing in
//! `unit/healing.rs`.
//!
//! Tests: in turn/tests.rs.

use crate::city::economy::{Farmers, Population};
use crate::game::{Event, Game};
use crate::player::PlayerId;
use crate::rules::SpellId;
use rand::SeedableRng;

impl Game {
    /// Every player that takes part in the lifecycle, in the fixed order: the wizards and the
    /// independent cities' players by id, then the monsters' owner (the neutral seat) last.
    pub fn turn_players(&self) -> Vec<PlayerId> {
        let mut ids = crate::player::ids(&self.world);
        if self.player_entity(crate::player::MONSTERS).is_some() {
            ids.push(crate::player::MONSTERS);
        }
        ids
    }

    /// Round start, once per round before any player acts: the round counter advances and the
    /// world's opening work runs (the event draw, the raiders' and monsters' clocks). See
    /// `docs/turn-structure.md` section 7.1 for what belongs to which phase.
    pub fn start_round(&mut self) -> Vec<Event> {
        self.turn += 1;
        let mut events = vec![Event::TurnStarted { turn: self.turn }];
        events.extend(self.process_events());
        events.extend(self.process_threats());
        events
    }

    /// The start of one player's turn: what makes their units and cities ready to act (moves
    /// refilled, offers rolled, the work of their units carried on, their standing orders
    /// followed).
    pub fn start_player_turn(&mut self, player: PlayerId) -> Vec<Event> {
        self.reopen_sales(player);
        let mut events = self.process_stasis(player);
        let stopper = self.time_stopper();
        let mut units: Vec<_> = self
            .units()
            .all_units()
            .into_iter()
            .filter(|&u| self.units().unit_owner(u) == Some(player))
            .collect();
        self.sort_by_serial(&mut units);
        for unit in units {
            // A unit in stasis cannot move, nor can any unit but the Time Stop wizard's.
            let moves = if self.unit_in_stasis(unit) || stopper.is_some_and(|s| s != player) {
                0
            } else {
                self.units().unit_max_moves(unit)
            };
            let _ = self.world.insert_one(unit, crate::unit::MovesLeft(moves));
            if self.units().unit_standby(unit) == Some(crate::unit::Standby::Done) {
                let _ = self.world.remove_one::<crate::unit::Standby>(unit);
            }
        }
        events.extend(self.determine_offers(player));
        events.extend(self.process_purify(player));
        events.extend(self.sail_marooned(player));
        events.extend(self.drown_stranded(Some(player)));
        events.extend(self.process_road_work(player));
        let mut with_orders: Vec<_> = self
            .world
            .query::<&crate::unit::Order>()
            .iter()
            .map(|(e, _)| e)
            .filter(|&e| self.units().unit_owner(e) == Some(player))
            .collect();
        self.sort_by_serial(&mut with_orders);
        events.extend(self.follow_orders(&with_orders));
        events
    }

    /// The end of one player's turn: their cities, treasury, magic, units' experience and healing.
    pub fn end_player_turn(&mut self, player: PlayerId) -> Vec<Event> {
        self.close_raze_windows(player);
        let mut events = self.process_land_corruption(player);
        // The monsters' owner has units only: no cities, no treasury, no magic.
        if player == crate::player::MONSTERS {
            self.process_experience(player);
            self.heal_units(player);
            return events;
        }
        let mut cities: Vec<_> = self
            .economy()
            .cities()
            .into_iter()
            .map(|(e, ..)| e)
            .filter(|&e| self.economy().city_owner(e) == Some(player))
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
        // still put them on the farms. The wizard whose Time Stop lasts pays no gold and eats
        // no food, only the spell's mana.
        if self.time_stopper() != Some(player) {
            events.extend(self.settle_treasury(player));
        }
        // `settle_treasury` may release units and sell off nothing else, but a city can be lost.
        cities.retain(|&c| self.economy().city_owner(c) == Some(player));

        for &city in &cities {
            events.extend(self.process_city(city));
        }
        self.process_experience(player);
        events.extend(self.process_magic(player));
        events.extend(self.process_gaias_blessing(player));
        self.heal_units(player);
        events.extend(self.process_chaos_rifts(player));
        events
    }

    /// Round end, once per round after the last player: durations and world-wide processes run
    /// once however many players there are.
    pub fn end_round(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        self.tick_temp_effects();
        self.process_globals();
        events.extend(self.process_world_enchantments());
        let mut cooling = rand_pcg::Pcg64Mcg::seed_from_u64(self.next_u64() ^ 0xC001);
        self.cool_volcanoes(&mut cooling);
        self.fade_corruption(&mut cooling);
        self.record_history();
        self.process_diplomacy();
        events
    }

    /// One city's turn: an outpost grows or is lost, the city builds, grows, and suffers from
    /// pestilence.
    fn process_city(&mut self, city: hecs::Entity) -> Vec<Event> {
        let mut events = Vec::new();
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
            return events;
        }
        if let Some(item) = self.production_impossible(city) {
            events.push(Event::ProductionImpossible { city, item });
        }
        self.autobuild_refresh(city);
        let Some(eco) = self.economy().city_economy(city) else {
            return events;
        };
        if let Some(item) = self.production_blocked(city, eco.production) {
            events.push(Event::ProductionBlocked { city, item });
        } else if let Some(item) = self.apply_production(city, eco.production) {
            events.push(Event::ProductionCompleted { city, item });
        }
        let (before, mut after) = self.apply_growth(city, eco.growth_tens);
        // Pestilence: each citizen has a tenth of a chance to die, but the last one is spared
        // (the original kills one when the city has more people than a roll of 1 to 10).
        if self
            .economy()
            .city_has_enchantment(city, SpellId::new(196) /* pestilence */)
        {
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
        events
    }
}

/// The per-player steps for every player in turn, for tests that mean "the turn change".
#[cfg(test)]
impl Game {
    pub(crate) fn determine_offers_all(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for p in self.turn_players() {
            events.extend(self.determine_offers(p));
        }
        events
    }

    pub(crate) fn process_chaos_rifts_all(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for p in self.turn_players() {
            events.extend(self.process_chaos_rifts(p));
        }
        events
    }

    pub(crate) fn process_gaias_blessing_all(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for p in self.turn_players() {
            events.extend(self.process_gaias_blessing(p));
        }
        events
    }

    pub(crate) fn process_magic_all(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for p in self.turn_players() {
            events.extend(self.process_magic(p));
        }
        events
    }

    pub(crate) fn process_purify_all(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for p in self.turn_players() {
            events.extend(self.process_purify(p));
        }
        events
    }

    pub(crate) fn process_road_work_all(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for p in self.turn_players() {
            events.extend(self.process_road_work(p));
        }
        events
    }

    pub(crate) fn sail_marooned_all(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for p in self.turn_players() {
            events.extend(self.sail_marooned(p));
        }
        events
    }

    pub(crate) fn heal_units_all(&mut self) {
        for p in self.turn_players() {
            self.heal_units(p);
        }
    }

    pub(crate) fn process_experience_all(&mut self) {
        for p in self.turn_players() {
            self.process_experience(p);
        }
    }
}

#[cfg(test)]
mod tests;
