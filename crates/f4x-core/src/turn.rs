//! Turn processing: city growth and production, the treasury, unit upkeep and healing.

use crate::city::{City, PlayerId, Position};
use crate::economy::{Buildings, CityRace, Enchantments, Farmers, Population};
use crate::production::{Production, ProductionItem};
use crate::rules::UnitKind;
use crate::units::{Hurt, Owner};
use crate::{Event, Game};

/// A player's projected gold and food for the coming turn change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Treasury {
    /// Taxes, trade and mines from all cities.
    pub city_gold: i32,
    pub building_upkeep: i32,
    pub noble_gold: i32,
    /// Gold from selling food the cities grow beyond what citizens and troops eat.
    pub food_gold: i32,
    /// Gold paid for troops, after fame.
    pub unit_upkeep: i32,
    pub food_surplus: i32,
    pub food_upkeep: i32,
}

impl Treasury {
    /// Net change in gold.
    pub fn net_gold(&self) -> i32 {
        self.city_gold - self.building_upkeep + self.noble_gold + self.food_gold - self.unit_upkeep
    }
}

impl Game {
    /// Uniform random integer in `1..=n`.
    pub fn roll(&mut self, n: u32) -> u32 {
        self.rng.roll(n)
    }

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

    /// Divide a city's population into whole citizens and tens, applying `delta_tens`.
    fn apply_growth(&mut self, city: hecs::Entity, delta_tens: i32) -> (u32, u32) {
        let cap = self.content.rules.config.max_city_size;
        let mut pop = self
            .world
            .get::<&Population>(city)
            .map(|p| *p)
            .unwrap_or_default();
        let before = pop.figures;
        let mut tens = pop.tens as i32 + delta_tens;
        let mut figures = pop.figures as i32;
        while tens >= 100 && figures < cap as i32 {
            tens -= 100;
            figures += 1;
        }
        while tens < 0 {
            if figures > 1 {
                figures -= 1;
                tens += 100;
            } else {
                tens = 0;
            }
        }
        pop.figures = figures as u32;
        pop.tens = tens.clamp(0, 99) as u32;
        let _ = self.world.insert_one(city, pop);
        (before, pop.figures)
    }

    /// The chance (percent) that an outpost gains people this turn: the land's food, the race,
    /// the minerals and wild game in reach, and Gaia's Blessing and Stream of Life. The
    /// original added a difficulty modifier here and ignored wild game's food; we do neither
    /// (no modifier for anyone, design.md section 8).
    pub(crate) fn outpost_growth_chance(&self, city: hecs::Entity) -> i32 {
        let Some(at) = self.world.get::<&Position>(city).ok().map(|p| *p) else {
            return 0;
        };
        let race = self.world.get::<&CityRace>(city).ok().map(|r| r.0);
        let mut grow = race.map_or(10, |r| {
            i32::from(self.content.rules.races.get(r).outpost_growth)
        });
        if let Some(grid) = self.map.as_ref().map(|m| m.plane(at.plane)) {
            // Food from the land, wild game included (it counts in the city's food).
            grow += self
                .city_economy_with(city, 0)
                .map_or(0, |e| e.max_size as i32);
            for (p, _) in self.city_area(at) {
                if let Some(f) = grid
                    .feature(p.x as i32, p.y as i32)
                    .and_then(|f| self.content.features.get(f))
                {
                    grow += f.city.outpost_growth;
                }
            }
        }
        let enchanted = |name: &str| self.city_has_enchantment(city, name);
        if enchanted("gaias_blessing") {
            grow += 20;
        }
        if enchanted("stream_of_life") {
            grow += 10;
        }
        grow
    }

    /// Outposts grow a few tens of people at a time, by chance.
    fn grow_outpost(&mut self, city: hecs::Entity) -> bool {
        if self.world.get::<&Position>(city).is_err() {
            return false;
        }
        let grow = self.outpost_growth_chance(city);
        let mut shrink = 5;
        for (name, extra) in [
            ("evil_presence", 5),
            ("pestilence", 10),
            ("famine", 10),
            ("chaos_rift", 10),
        ] {
            if self.city_has_enchantment(city, name) {
                shrink += extra;
            }
        }
        let mut pop = self
            .world
            .get::<&Population>(city)
            .map(|p| *p)
            .unwrap_or_default();
        if self.roll(100) as i32 <= grow {
            pop.tens += self.roll(3);
        }
        if self.roll(100) as i32 <= shrink {
            pop.tens = pop.tens.saturating_sub(self.roll(2));
        }
        if pop.tens >= 100 {
            pop.figures = 1;
            pop.tens -= 100;
        }
        let _ = self.world.insert_one(city, pop);
        pop.figures > 0
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
        events.extend(self.process_magic());
        events.extend(self.process_gaias_blessing());
        self.heal_units();
        events.extend(self.process_chaos_rifts());
        events
    }

    /// Set the tax rate to the lowest that leaves the treasury no worse off each turn, or to the
    /// best there is when none does. How an independent city is run; wizards choose their own.
    pub(crate) fn balance_taxes(&mut self, player: PlayerId) {
        let rates = self.content.rules.config.tax_unrest_percent.len() as u8;
        let Some(start) = self.player(player).map(|p| p.tax_rate) else {
            return;
        };
        let mut best = (i32::MIN, start);
        for rate in 0..rates {
            if let Some(p) = self.player_mut(player) {
                p.tax_rate = rate;
            }
            let net = self.treasury(player).net_gold();
            if net >= 0 {
                best = (net, rate);
                break;
            }
            if net > best.0 {
                best = (net, rate);
            }
        }
        if let Some(p) = self.player_mut(player) {
            p.tax_rate = best.1;
        }
    }

    /// Put just enough citizens on farms that the player's cities feed their citizens
    /// and their army, preferring the cities that farm best. Does nothing where there
    /// is not enough land: those cities farm with everyone.
    pub fn balance_farmers(&mut self, player: PlayerId) {
        let mut mine: Vec<hecs::Entity> = self
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == player)
            .map(|(e, ..)| e)
            .collect();
        self.sort_by_serial(&mut mine);
        for &city in &mine {
            let min = self.city_min_farmers(city);
            let _ = self.world.insert_one(city, Farmers(min));
        }
        let upkeep: i32 = self
            .player_units(player)
            .iter()
            .map(|&u| self.unit_food_upkeep(u))
            .sum();
        // Each extra farmer yields food beyond what he eats; add them where it pays most.
        loop {
            let surplus = self.treasury(player).food_surplus;
            if surplus >= upkeep {
                break;
            }
            let best = mine
                .iter()
                .copied()
                .filter_map(|c| {
                    let farmers = self.world.get::<&Farmers>(c).map_or(0, |f| f.0);
                    let figures = self.world.get::<&Population>(c).map_or(0, |p| p.figures);
                    if farmers >= figures {
                        return None;
                    }
                    let now = self.city_economy_with(c, farmers)?;
                    let more = self.city_economy_with(c, farmers + 1)?;
                    Some((more.food - now.food, c, farmers))
                })
                .filter(|(gain, ..)| *gain > 0)
                .max_by_key(|(gain, c, _)| (*gain, std::cmp::Reverse(self.serial_of(*c))));
            let Some((_, city, farmers)) = best else {
                break;
            };
            let _ = self.world.insert_one(city, Farmers(farmers + 1));
        }
    }

    /// What a player will take in and pay out at the next turn change.
    pub fn treasury(&self, player: PlayerId) -> Treasury {
        let mut t = Treasury::default();
        let mut sold_food_gold = 0;
        for (city, c, _) in self.cities() {
            if c.owner != player {
                continue;
            }
            let Some(eco) = self.city_economy(city) else {
                continue;
            };
            t.city_gold += eco.gold;
            t.building_upkeep += eco.gold_upkeep;
            let excess = (eco.food - eco.figures as i32).max(0);
            sold_food_gold += excess / 2;
            t.food_surplus += excess;
        }
        let units = self.player_units(player);
        for &unit in &units {
            if self.unit_has_ability(unit, "noble")
                && self
                    .unit_def(unit)
                    .is_some_and(|d| d.kind == UnitKind::Hero)
            {
                t.noble_gold += 10;
            }
        }
        t.food_upkeep = units.iter().map(|&u| self.unit_food_upkeep(u)).sum();
        // Spare food is sold at two for one gold once the army has eaten.
        t.food_gold = (sold_food_gold - t.food_upkeep / 2).max(0);
        let fame = self.player(player).map_or(0, |p| p.fame);
        t.unit_upkeep =
            (units.iter().map(|&u| self.unit_gold_upkeep(u)).sum::<i32>() - fame).max(0);
        t
    }

    /// Income, food and gold upkeep for one player; units go when they cannot be paid for.
    fn settle_treasury(&mut self, player: PlayerId) -> Vec<Event> {
        let mut events = Vec::new();
        let t = self.treasury(player);
        let excess_food = t.food_surplus;
        let mut food_upkeep = t.food_upkeep;
        let units = self.player_units(player);
        let Some(p) = self.player_mut(player) else {
            return events;
        };
        p.gold = (p.gold + t.city_gold - t.building_upkeep + t.noble_gold + t.food_gold).max(0);

        // An army that eats more than the cities grow is disbanded, newest first.
        let mut disbanded = Vec::new();
        for &unit in units.iter().rev() {
            if food_upkeep <= excess_food {
                break;
            }
            if self.unit_food_upkeep(unit) > 0 {
                food_upkeep -= 1;
                disbanded.push(unit);
            }
        }
        let fame = self.player(player).map_or(0, |p| p.fame);
        let gold_of = |game: &Game, skip: &[hecs::Entity]| -> i32 {
            let total: i32 = game
                .player_units(player)
                .iter()
                .filter(|u| !skip.contains(u))
                .map(|&u| game.unit_gold_upkeep(u))
                .sum();
            (total - fame).max(0)
        };
        let reserve = self.player(player).map_or(0, |p| p.gold);
        let mut upkeep = gold_of(self, &disbanded);
        for &unit in units.iter().rev() {
            if upkeep <= reserve {
                break;
            }
            if !disbanded.contains(&unit) && self.unit_gold_upkeep(unit) > 0 {
                disbanded.push(unit);
                upkeep = gold_of(self, &disbanded);
            }
        }
        for unit in &disbanded {
            let _ = self.world.despawn(*unit);
        }
        if !disbanded.is_empty() {
            events.push(Event::UnitsDisbanded {
                player,
                units: disbanded.len() as u32,
            });
        }
        if let Some(p) = self.player_mut(player) {
            p.gold = (p.gold - upkeep).max(0);
        }
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

    /// Found a city (or outpost, with no population) with all its economic components.
    pub(crate) fn spawn_city(
        &mut self,
        owner: PlayerId,
        pos: Position,
        name: String,
        race: Option<crate::rules::RaceId>,
        figures: u32,
    ) -> hecs::Entity {
        let serial = self.take_serial();
        let city = self.world.spawn((
            serial,
            City { name, owner },
            pos,
            Population { figures, tens: 0 },
            Farmers(0),
            Buildings::default(),
            Enchantments::default(),
            Production {
                item: Some(ProductionItem::Housing),
                progress: 0,
            },
        ));
        if let Some(r) = race {
            let _ = self.world.insert_one(city, CityRace(r));
            if let Some(p) = self.player_mut(owner) {
                p.capital_race.get_or_insert(r);
            }
        }
        let farmers = self.city_min_farmers(city);
        let _ = self.world.insert_one(city, Farmers(farmers));
        if !self.is_neutral_player(owner) {
            self.explore(owner, pos, 2);
        }
        city
    }
}

#[cfg(test)]
mod tests;
