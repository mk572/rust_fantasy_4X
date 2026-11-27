//! What the turn order does to the world: [`World::start_round`], [`World::start_player_turn`],
//! [`World::end_player_turn`] and [`World::end_round`] (task 477, `docs/turn-structure.md`).
//! The order is the game's (`f4x-master`), which calls them through [`World::advance`]. What each step does is in its domain: city growth in
//! `city/growth.rs`, the treasury in `player/finance.rs`, magic in `wizard/magic_world.rs`, healing in
//! `unit/healing.rs`.
//!
//! Tests: in tests/it (`c7_turn.rs`, `c9_turn.rs`), and through the turn order in
//! `f4x-master/tests/it`.

use crate::city::economy::{Farmers, Population};
use crate::player::PlayerId;
use crate::world::rng::GameRng;
use crate::world::{Event, World};
use f4x_def::half_int::HalfInt;
use rand::RngCore as _;
use rand::SeedableRng;

impl World {
    /// Round start, once per round before any player acts: `round` begins and the world's
    /// opening work runs (the event draw, the raiders' and monsters' clocks). See
    /// `docs/turn-structure.md` section 7.1 for what belongs to which phase.
    pub fn start_round(&mut self, round: u32, rng: &mut GameRng) -> Vec<Event> {
        self.diplomacy.age_wars(1);
        let mut events = vec![Event::TurnStarted { turn: round }];
        events.extend(self.process_events(round, rng));
        events.extend(self.process_threats(round, rng));
        events
    }

    /// The start of one player's turn: what changes the land and the people runs first (the
    /// research that finished, Gaia's Blessing and Chaos Rift, outposts, growth, pestilence,
    /// experience, healing), so they act on the state they will see; then moves are refilled
    /// (a unit finished at the last turn end moves now), offers rolled, the work of their units
    /// carried on and their standing orders followed. The first round has none of it: the game
    /// begins with the first turn.
    pub fn start_player_turn(
        &mut self,
        player: PlayerId,
        round: u32,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        self.reopen_sales(player);
        let mut events = self.process_stasis(player, rng);
        if round > 1 {
            events.extend(self.run_growth(player, rng));
        }
        let stopper = self.time_stopper();
        // The player's own units, oldest first (not every unit of the game, sorted).
        let units = self.units().player_units(player);
        for unit in units {
            // A unit in stasis cannot move, nor can any unit but the Time Stop wizard's.
            let moves = if self.unit_in_stasis(unit) || stopper.is_some_and(|s| s != player) {
                HalfInt::ZERO
            } else {
                self.units().unit_max_moves(unit)
            };
            self.put(unit, crate::unit::MovesLeft(moves));
            if self.units().unit_standby(unit) == Some(crate::unit::Standby::Done) {
                self.strip::<crate::unit::Standby>(unit);
            }
        }
        events.extend(self.determine_offers(player, round, rng));
        events.extend(self.process_purify(player));
        self.unload_in_ports(player);
        events.extend(self.sail_marooned(player));
        events.extend(self.drown_stranded(Some(player), rng));
        events.extend(self.process_road_work(player));
        let mut with_orders: Vec<_> = self
            .ecs
            .query::<&crate::unit::Order>()
            .iter()
            .map(|(e, _)| e)
            .filter(|&e| self.units().unit_owner(e) == Some(player))
            .collect();
        self.units().sort_by_serial(&mut with_orders);
        events.extend(self.follow_orders(&with_orders, rng));
        events
    }

    /// The start-of-turn changes to the land and the people, in the order that lets what
    /// changes food and buildings act before they are counted: the research that finished,
    /// what spells do to the land and the cities (Gaia's Blessing, Chaos Rift), then outposts,
    /// growth and pestilence, and the units' experience and healing.
    fn run_growth(&mut self, player: PlayerId, rng: &mut GameRng) -> Vec<Event> {
        // The monsters' owner has units only: no cities, no treasury, no magic.
        if player == crate::player::MONSTERS {
            self.process_experience(player);
            self.heal_units(player, rng);
            self.heal_lairs();
            return Vec::new();
        }
        // While Time Stop lasts nothing grows, nothing is learned and the land does not change
        // (the wiki's article on Time Stop); the units of the wizard who keeps it heal as ever.
        if self.time_stopper().is_some() {
            self.process_experience(player);
            self.heal_units(player, rng);
            return Vec::new();
        }
        let mut events = self.process_research(player, rng);
        events.extend(self.process_gaias_blessing(player, rng));
        events.extend(self.process_chaos_rifts(player, rng));
        for city in self.player_cities(player) {
            events.extend(self.grow_city(city, rng));
        }
        self.process_experience(player);
        self.heal_units(player, rng);
        events
    }

    /// The end of one player's turn: income, bills and production, so a defender finished now
    /// defends against the rivals' turns and a shortage costs its units now. Raze windows shut;
    /// farmers are kept on the farms; the treasury settles (gold and food income, upkeep); the
    /// wizard's magic runs (mana income, research points, mana upkeep, casting); the cities build.
    /// First of all, what was proposed in the turn and not answered lapses.
    pub fn end_player_turn(&mut self, player: PlayerId, rng: &mut GameRng) -> Vec<Event> {
        self.diplomacy.lapse_proposals();
        self.close_raze_windows(player);
        let mut events = Vec::new();
        // The monsters' owner has units only: no cities, no treasury, no magic.
        if player == crate::player::MONSTERS {
            return events;
        }
        // While another wizard keeps Time Stop going, this wizard's turn does not happen: no
        // income, no upkeep, no mana, no production.
        let stopper = self.time_stopper();
        if stopper.is_some_and(|keeper| keeper != player) {
            return events;
        }
        // Keep enough hands on the farms to feed everyone.
        for city in self.player_cities(player) {
            if self.ecs.get::<&Population>(city).map_or(0, |p| p.figures) == 0 {
                continue;
            }
            let min = self.economy().city_min_farmers(city);
            let farmers = self.ecs.get::<&Farmers>(city).map_or(0, |f| f.0);
            if farmers < min {
                self.put(city, Farmers(min));
            }
        }
        // The wizard whose Time Stop lasts pays no gold and eats no food, only the spell's mana.
        if self.time_stopper() != Some(player) {
            events.extend(self.settle_treasury(player));
        }
        events.extend(self.process_magic(player, rng));
        // `settle_treasury` may release units and a city can be lost: build in what is left. The
        // wizard who keeps Time Stop builds nothing until it ends.
        if stopper.is_none() {
            self.net_gold_memo.set(crate::world::NetGoldMemo::Empty);
            *self.budget_parts.borrow_mut() = None;
            for city in self.player_cities(player) {
                let before = self.economy().city_production(city).item;
                let done = self.produce_city(city, rng);
                // A completed item changes the upkeep, and a unit the population; the item
                // made (coin, say) changes the city's gold.
                if before != self.economy().city_production(city).item
                    || done
                        .iter()
                        .any(|e| matches!(e, Event::ProductionCompleted { .. }))
                {
                    self.net_gold_memo.set(crate::world::NetGoldMemo::Empty);
                    self.mark_budget_dirty(player, city);
                }
                events.extend(done);
            }
            self.net_gold_memo.set(crate::world::NetGoldMemo::Off);
            *self.budget_parts.borrow_mut() = None;
        }
        events
    }

    /// The player's cities in a fixed order.
    fn player_cities(&self, player: PlayerId) -> Vec<hecs::Entity> {
        // Oldest first, by the serial numbers: what `cities_of` gives.
        self.economy()
            .cities_of(player)
            .into_iter()
            .map(|(e, _)| e)
            .collect()
    }

    /// Round end, once per round after the last player: durations and world-wide processes run
    /// once however many players there are.
    pub fn end_round(&mut self, rng: &mut GameRng) -> Vec<Event> {
        let mut events = Vec::new();
        self.process_globals();
        events.extend(self.process_world_enchantments(rng));
        let mut cooling = rand_pcg::Pcg64Mcg::seed_from_u64(rng.next_u64() ^ 0xC001);
        self.cool_volcanoes(&mut cooling);
        self.fade_corruption(&mut cooling);
        self.record_history();
        self.process_diplomacy();
        events
    }

    /// One city's building at the end of the turn: what it makes is finished (or the player is
    /// told it cannot be).
    fn produce_city(&mut self, city: hecs::Entity, rng: &mut GameRng) -> Vec<Event> {
        let mut events = Vec::new();
        if self.ecs.get::<&Population>(city).map_or(0, |p| p.figures) == 0 {
            return events;
        }
        if let Some(item) = self.production_impossible(city) {
            events.push(Event::ProductionImpossible { city, item });
        }
        self.autobuild_refresh(city, rng);
        let Some(eco) = self.economy().city_economy(city) else {
            return events;
        };
        // Housing ends when the city cannot grow any further: there is nothing left to house.
        if self.economy().city_production(city).item
            == Some(crate::city::production::ProductionItem::Housing)
            && eco.figures >= eco.max_size
        {
            let mut p = self.economy().city_production(city);
            p.item = None;
            self.put(city, p);
            events.push(Event::ProductionCompleted {
                city,
                item: crate::city::production::ProductionItem::Housing,
            });
            return events;
        }
        if let Some(item) = self.production_blocked(city, eco.production) {
            events.push(Event::ProductionBlocked { city, item });
        } else if let Some(item) = self.apply_production(city, eco.production, rng) {
            events.push(Event::ProductionCompleted { city, item });
        }
        events
    }

    /// One city's change at the start of the turn: an outpost grows or is lost, a city grows,
    /// and suffers from pestilence.
    fn grow_city(&mut self, city: hecs::Entity, rng: &mut GameRng) -> Vec<Event> {
        let mut events = Vec::new();
        let figures = self.ecs.get::<&Population>(city).map_or(0, |p| p.figures);
        if figures == 0 {
            match self.grow_outpost(city, rng) {
                crate::city::growth::OutpostTurn::BecameCity => {
                    let farmers = self.economy().city_min_farmers(city);
                    self.put(city, Farmers(farmers));
                    events.push(Event::CityGrew { city, figures: 1 });
                }
                crate::city::growth::OutpostTurn::Lost => {
                    events.extend(self.remove_city(city, rng));
                }
                crate::city::growth::OutpostTurn::Stays => {}
            }
            return events;
        }
        let Some(eco) = self.economy().city_economy(city) else {
            return events;
        };
        let (before, mut after) = self.apply_growth(city, eco.growth_tens);
        // Pestilence: each citizen has a tenth of a chance to die, but the last one is spared.
        if self
            .economy()
            .city_has_effect(city, crate::spell::city::CityEffect::Pestilence)
        {
            let dead = (1..after).filter(|_| rng.roll(10) == 1).count() as u32;
            after -= dead;
            let pop = Population {
                figures: after,
                ..self.city_population(city)
            };
            self.put(city, pop);
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
