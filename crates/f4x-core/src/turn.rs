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
mod tests {
    use super::*;
    use crate::city::PlayerId;
    use crate::economy::Population;
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::production::{ProductionError, ProductionItem};
    use crate::rules::{ModOp, Stat};
    use crate::testutil::content_with_economy;
    use crate::units::Hurt;
    use crate::Command;

    const P0: PlayerId = PlayerId(0);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    /// 24x14 grassland with the sea at x=0. One player with 500 gold, tax rate 2.
    fn world() -> Game {
        let c = content_with_economy();
        let grass = c.terrains.id_of("grassland").unwrap();
        let ocean = c.terrains.id_of("ocean").unwrap();
        let build = || {
            let mut g = Grid::new(24, 14, grass);
            for y in 0..14 {
                g.set(0, y, ocean);
            }
            g
        };
        let map = Map::new(build(), build());
        let mut game = Game::new(7);
        game.content = c;
        // No unrest from taxes up to rate 2, so income sums stay simple.
        game.content.rules.config.tax_unrest_percent = vec![0, 0, 0, 30, 45, 60, 75];
        game.map = Some(map);
        let mut p = Player::new(P0, "Merlin");
        p.gold = 500;
        p.tax_rate = 2;
        game.players.push(p);
        game
    }

    fn race(g: &Game, id: &str) -> Option<crate::rules::RaceId> {
        g.content.rules.races.id_of(id)
    }

    fn found(g: &mut Game, pos: Position, race_id: &str, pop: u32) -> hecs::Entity {
        let race = race(g, race_id);
        g.apply(Command::FoundCity {
            owner: P0,
            pos,
            name: "Town".into(),
            race,
            population: pop,
        });
        g.city_at(pos).unwrap()
    }

    fn building(g: &Game, id: &str) -> crate::rules::BuildingId {
        g.content.rules.buildings.id_of(id).unwrap()
    }

    /// Stop the city building housing (which speeds growth) so growth tests see the base rate.
    fn idle(g: &mut Game, city: hecs::Entity) {
        g.world.insert_one(city, Production::default()).unwrap();
    }

    fn add_building(g: &mut Game, city: hecs::Entity, id: &str) {
        let b = building(g, id);
        g.world.get::<&mut Buildings>(city).unwrap().0.push(b);
    }

    #[test]
    fn the_worked_area_is_five_by_five_without_corners() {
        let g = world();
        assert_eq!(g.city_area(at(10, 6)).len(), 21);
        // Near the top edge the rows above the map are dropped, and x wraps.
        assert_eq!(g.city_area(at(1, 0)).len(), 13);
        assert!(
            g.city_area(at(1, 6)).iter().any(|(p, _)| p.x == 23),
            "wraps across the seam"
        );
    }

    #[test]
    fn a_grassland_city_can_grow_to_the_size_cap() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 3);
        let eco = g.city_economy(city).unwrap();
        // 21 tiles x 3 half-food = 31 people fed, capped at 25.
        assert_eq!(eco.max_size, 25);
        assert!(eco.food >= 3);
    }

    #[test]
    fn farmers_and_workers_share_the_citizens() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        // Elves farm 3 food each: two farmers feed four citizens, with food to spare.
        assert_eq!(g.city_min_farmers(city), 2);
        let eco = g.city_economy_with(city, 2).unwrap();
        assert_eq!((eco.farmers, eco.workers, eco.rebels), (2, 2, 0));
        assert_eq!(eco.food, 6);
        // 2 workers x 2 production + 2 farmers x half, rounded: (2*4 + 2 + 1)/2 = 5
        assert_eq!(eco.production, 5);
        // Dwarves work harder (3 each) but farm 2 food.
        g.player_mut(P0).unwrap().tax_rate = 0;
        let dwarf = found(&mut g, at(18, 6), "dwarf", 4);
        assert_eq!(g.city_min_farmers(dwarf), 2);
        assert_eq!(
            g.city_economy_with(dwarf, 2).unwrap().production,
            (2 * 6 + 2 + 1) / 2
        );
    }

    #[test]
    fn gold_comes_from_taxes_and_the_sea_and_markets() {
        let mut g = world();
        let inland = found(&mut g, at(10, 6), "elf", 4);
        // 4 citizens x tax rate 2 / 2 = 4 gold.
        assert_eq!(g.city_economy(inland).unwrap().gold, 4);
        add_building(&mut g, inland, "marketplace");
        assert_eq!(g.city_economy(inland).unwrap().gold, 6, "+50%");
        // A coastal city (sea at x=0) gets +10% trade; 10 gold base needs more citizens.
        let coastal = found(&mut g, at(1, 6), "elf", 10);
        assert_eq!(g.city_economy(coastal).unwrap().gold, 11, "10 * 1.10");
        // Dwarves double the tax.
        let dwarf = found(&mut g, at(18, 6), "dwarf", 4);
        assert_eq!(g.city_economy(dwarf).unwrap().gold, 8);
    }

    #[test]
    fn a_lake_counts_as_a_shore_for_trade() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 10);
        assert_eq!(g.city_economy(city).unwrap().gold, 10);
        let ocean = g
            .content
            .terrains
            .first_of_class(crate::content::TerrainClass::Ocean, Plane::Arcanus)
            .unwrap();
        let grid = g.map.as_mut().unwrap().plane_mut(Plane::Arcanus);
        grid.set(11, 6, ocean);
        grid.set_lake(11, 6, true);
        assert_eq!(
            g.city_economy(city).unwrap().gold,
            11,
            "+10% beside the water"
        );
    }

    #[test]
    fn rivers_pay_twenty_percent_and_river_mouths_thirty() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 10);
        assert_eq!(g.city_economy(city).unwrap().gold, 10);
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set_river(10, 6, true);
        assert_eq!(g.city_economy(city).unwrap().gold, 12, "river");
        let mouth = found(&mut g, at(1, 6), "elf", 10);
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set_river(1, 6, true);
        assert_eq!(g.city_economy(mouth).unwrap().gold, 13, "river mouth");
    }

    /// Add a building with the given stat modifiers to the rules and to a city.
    fn add_custom_building(
        g: &mut Game,
        city: hecs::Entity,
        id: &str,
        mods: &[(Stat, ModOp, i32)],
    ) {
        use crate::rules::effects::{Condition, Effect, EffectRule, TargetSel, Trigger};
        let mut def = g.content.rules.buildings.defs()[0].clone();
        def.id = id.into();
        def.requires.clear();
        def.effects = mods
            .iter()
            .map(|&(stat, op, value)| EffectRule {
                when: Trigger::Passive,
                condition: Condition::Always,
                target: TargetSel::This,
                effect: Effect::Modify { stat, op, value },
            })
            .collect();
        let mut defs = g.content.rules.buildings.defs().to_vec();
        defs.push(def);
        let new = crate::rules::BuildingId::new((defs.len() - 1) as u16);
        g.content.rules.buildings = crate::rules::Registry::new(defs);
        g.world.get::<&mut Buildings>(city).unwrap().0.push(new);
    }

    #[test]
    fn the_animists_guild_adds_food_for_every_farmer() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 6);
        let food = |g: &Game, farmers| g.city_economy_with(city, farmers).unwrap().food;
        let (two, four) = (food(&g, 2), food(&g, 4));
        add_custom_building(
            &mut g,
            city,
            "animists",
            &[(Stat::FarmerFood, ModOp::Add, 1)],
        );
        assert_eq!(food(&g, 2), two + 2, "one more for each of two farmers");
        assert_eq!(
            food(&g, 4),
            four + 4,
            "and for each of four, not a flat bonus"
        );
    }

    #[test]
    fn the_foresters_guilds_food_raises_the_size_limit_too() {
        let mut g = world();
        // Barren land: nothing to feed anyone but the guild.
        let desert = g
            .content
            .terrains
            .first_of_class(crate::content::TerrainClass::Desert, Plane::Arcanus)
            .unwrap();
        for x in 0..24 {
            for y in 0..14 {
                g.map
                    .as_mut()
                    .unwrap()
                    .plane_mut(Plane::Arcanus)
                    .set(x, y, desert);
            }
        }
        let city = found(&mut g, at(10, 6), "elf", 1);
        let size = |g: &Game| g.city_economy(city).unwrap().max_size;
        assert_eq!(size(&g), 0);
        add_custom_building(
            &mut g,
            city,
            "foresters",
            &[(Stat::CityFood, ModOp::Add, 2)],
        );
        assert_eq!(size(&g), 2, "+2 food feeds two more people (B14)");
    }

    #[test]
    fn mines_add_gold_mana_and_cheaper_units() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        let gold_ore = g.content.features.id_of("gold").unwrap();
        let adamant = g.content.features.id_of("adamantium_ore").unwrap();
        let iron = g.content.features.id_of("iron").unwrap();
        let plane = g.map.as_mut().unwrap().plane_mut(Plane::Arcanus);
        plane.set_feature(10, 8, Some(gold_ore));
        plane.set_feature(11, 6, Some(adamant));
        plane.set_feature(9, 6, Some(iron));
        let eco = g.city_economy(city).unwrap();
        assert_eq!(eco.gold, 4 + 3, "taxes plus the gold ore");
        assert_eq!(
            eco.mana,
            2 + 2,
            "half a mana per elf citizen plus adamantium"
        );
        assert_eq!(eco.unit_cost_percent, 95, "iron saves 5%");
        // A miners' guild improves the yields; a second city sharing a tile halves it.
        add_building(&mut g, city, "miners_guild");
        assert_eq!(
            g.city_economy(city).unwrap().gold,
            4 + 4,
            "3 x 1.5 rounds down to 4"
        );
        assert_eq!(g.city_economy(city).unwrap().unit_cost_percent, 90);
        // A second city sharing the gold ore's tile halves what each gets from it.
        let before = g.city_economy(city).unwrap().gold;
        found(&mut g, at(10, 10), "elf", 1);
        assert!(g
            .city_area(at(10, 6))
            .iter()
            .any(|(p, shared)| *p == at(10, 8) && *shared));
        assert!(
            g.city_economy(city).unwrap().gold < before,
            "the shared ore pays less"
        );
    }

    #[test]
    fn a_shared_tile_feeds_each_city_half() {
        let mut g = world();
        let a = found(&mut g, at(8, 6), "elf", 3);
        let before = g.city_economy_with(a, 0).unwrap().max_size;
        found(&mut g, at(12, 6), "elf", 3);
        let shared = g.city_area(at(8, 6)).iter().filter(|(_, s)| *s).count();
        assert_eq!(
            shared, 3,
            "the middle column, without the corners of the left city's area"
        );
        let after = g.city_economy_with(a, 0).unwrap().max_size;
        assert!(after <= before, "sharing never helps: {before} -> {after}");
    }

    #[test]
    fn taxes_and_race_cause_unrest_and_temples_calm_it() {
        let mut g = world();
        g.player_mut(P0).unwrap().tax_rate = 4; // 45% unrest
        let city = found(&mut g, at(10, 6), "elf", 10);
        // Capital race is elf, so no racial unrest: 10 * 45% = 4 rebels.
        assert_eq!(g.city_economy(city).unwrap().rebels, 4);
        add_building(&mut g, city, "temple");
        assert_eq!(g.city_economy(city).unwrap().rebels, 3);
        // A dwarf city under an elf capital: racial table says 2 -> +20%.
        let dwarf = found(&mut g, at(18, 6), "dwarf", 10);
        assert_eq!(
            g.city_economy(dwarf).unwrap().rebels,
            6,
            "10 * (45+20)% = 6"
        );
    }

    #[test]
    fn stream_of_life_ends_unrest_and_heals_the_units_in_the_city() {
        let mut g = world();
        g.player_mut(P0).unwrap().tax_rate = 4;
        let city = found(&mut g, at(10, 6), "elf", 10);
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let home = g.spawn_unit(P0, at(10, 6), spear);
        let field = g.spawn_unit(P0, at(14, 3), spear);
        for u in [home, field] {
            g.world.insert_one(u, Hurt(5)).unwrap();
        }
        assert!(g.city_economy(city).unwrap().rebels > 0);
        let mut spells = g.content.rules.spells.defs().to_vec();
        let mut def = spells
            .iter()
            .find(|s| s.id == "gaias_blessing")
            .unwrap()
            .clone();
        def.id = "stream_of_life".into();
        spells.push(def);
        g.content.rules.spells = crate::rules::Registry::new(spells);
        let stream = g.content.rules.spells.id_of("stream_of_life").unwrap();
        g.world
            .get::<&mut Enchantments>(city)
            .unwrap()
            .0
            .push((stream, P0));
        assert_eq!(g.city_economy(city).unwrap().rebels, 0, "no unrest");
        g.apply(Command::EndTurn);
        let hurt = |u| g.world.get::<&Hurt>(u).unwrap().0;
        assert_eq!(hurt(home), 0, "healed completely in one turn");
        assert!(hurt(field) > 0, "units outside the city are not");
    }

    #[test]
    fn undead_units_are_not_healed_eat_nothing_and_are_death_creatures() {
        let mut g = world();
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let living = g.spawn_unit(P0, at(10, 6), spear);
        let raised = g.spawn_unit(P0, at(10, 7), spear);
        for u in [living, raised] {
            g.world.insert_one(u, Hurt(5)).unwrap();
        }
        assert!(g.make_undead(raised));
        g.heal_units();
        let hurt = |u| g.world.get::<&Hurt>(u).unwrap().0;
        assert!(hurt(living) < 5, "the living heal");
        assert_eq!(hurt(raised), 5, "the undead do not");
        assert_eq!(g.unit_food_upkeep(living), 1);
        assert_eq!(g.unit_food_upkeep(raised), 0);
        assert_eq!(g.unit_gold_upkeep(raised), 0);
        assert_eq!(g.unit_realm(raised).as_deref(), Some("death"));
        assert_eq!(g.unit_realm(living), None);
        assert!(g.unit_is_death(raised) && !g.unit_is_death(living));
        for a in [
            "death_immunity",
            "cold_immunity",
            "poison_immunity",
            "illusion_immunity",
        ] {
            assert!(g.unit_has_ability(raised, a), "{a}");
            assert!(!g.unit_has_ability(living, a), "{a}");
        }
    }

    #[test]
    fn troops_in_a_city_keep_order() {
        let mut g = world();
        g.player_mut(P0).unwrap().tax_rate = 4;
        let city = found(&mut g, at(10, 6), "elf", 10);
        let spear = g.content.rules.units.id_of("spear").unwrap();
        for _ in 0..4 {
            g.spawn_unit(P0, at(10, 6), spear);
        }
        // 4 rebels, minus 4 troops / 2 = 2 calmed.
        assert_eq!(g.city_economy(city).unwrap().rebels, 2);
    }

    #[test]
    fn growth_follows_the_room_to_grow_and_starvation_shrinks() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 3);
        idle(&mut g, city);
        let eco = g.city_economy(city).unwrap();
        // (25 - 3 + 1) / 2 = 11 tens; granary adds 20 people = 2 tens.
        assert_eq!(eco.growth_tens, 11);
        add_building(&mut g, city, "granary");
        assert_eq!(g.city_economy(city).unwrap().growth_tens, 13);
        // No farmers and too little food: shrinks five tens per missing food.
        let starving = g.city_economy_with(city, 0).unwrap();
        assert!(starving.growth_tens < 0);
    }

    #[test]
    fn turns_grow_the_population_and_carry_the_surplus() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 3);
        idle(&mut g, city);
        g.world
            .insert_one(
                city,
                Population {
                    figures: 3,
                    tens: 95,
                },
            )
            .unwrap();
        g.apply(Command::EndTurn);
        let pop = *g.world.get::<&Population>(city).unwrap();
        assert_eq!(
            (pop.figures, pop.tens),
            (4, 6),
            "95 + 11 = 106: one citizen and 6 left over"
        );
    }

    #[test]
    fn production_builds_units_and_buildings() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        let barracks = building(&g, "barracks");
        assert_eq!(
            g.apply(Command::SetProduction {
                city,
                item: ProductionItem::Building(barracks)
            }),
            vec![]
        );
        // 5 production per turn, 30 to build: done on turn 6.
        for _ in 0..5 {
            assert!(g
                .apply(Command::EndTurn)
                .iter()
                .all(|e| !matches!(e, Event::ProductionCompleted { .. })));
        }
        let events = g.apply(Command::EndTurn);
        assert!(events.contains(&Event::ProductionCompleted {
            city,
            item: ProductionItem::Building(barracks)
        }));
        assert!(g.has_building(city, barracks));
        assert_eq!(
            g.city_production(city).item,
            Some(ProductionItem::Housing),
            "falls back to housing"
        );
        // Now spearmen need barracks; fighters guild trains them as Regulars.
        add_building(&mut g, city, "fighters_guild");
        let spear = g.content.rules.units.id_of("spear").unwrap();
        g.apply(Command::SetProduction {
            city,
            item: ProductionItem::Unit(spear),
        });
        for _ in 0..4 {
            g.apply(Command::EndTurn);
        }
        let units = g.units_at(at(10, 6));
        assert_eq!(units.len(), 1, "20 production at 5 a turn: one spearman");
        assert_eq!(
            g.world
                .get::<&crate::units::Experience>(units[0])
                .unwrap()
                .0,
            20,
            "fighters guild: regular"
        );
    }

    #[test]
    fn requirements_and_races_limit_what_can_be_built() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let guild = building(&g, "fighters_guild");
        let forge = building(&g, "forge");
        let barracks = building(&g, "barracks");
        assert_eq!(
            g.can_build(city, ProductionItem::Unit(spear)),
            Err(ProductionError::MissingBuilding)
        );
        assert_eq!(
            g.can_build(city, ProductionItem::Building(guild)),
            Err(ProductionError::MissingBuilding)
        );
        assert_eq!(
            g.can_build(city, ProductionItem::Building(forge)),
            Err(ProductionError::WrongRace)
        );
        add_building(&mut g, city, "barracks");
        assert_eq!(g.can_build(city, ProductionItem::Unit(spear)), Ok(()));
        assert_eq!(
            g.can_build(city, ProductionItem::Building(barracks)),
            Err(ProductionError::AlreadyBuilt)
        );
        let galley = g.content.rules.units.id_of("galley").unwrap();
        add_building(&mut g, city, "shipyard");
        assert_eq!(
            g.can_build(city, ProductionItem::Unit(galley)),
            Err(ProductionError::NotCoastal)
        );
        let port = found(&mut g, at(1, 6), "elf", 3);
        add_building(&mut g, port, "shipyard");
        assert_eq!(g.can_build(port, ProductionItem::Unit(galley)), Ok(()));
    }

    /// A city that has just finished the barracks, with the Vizier's list set.
    fn vizier_city(on: bool) -> (Game, hecs::Entity) {
        let mut g = world();
        g.content.rules.config.autobuild_order = ["barracks", "fighters_guild", "granary", "nope"]
            .map(String::from)
            .to_vec();
        let city = found(&mut g, at(10, 6), "elf", 4);
        g.apply(Command::SetGrandVizier { player: P0, on });
        let barracks = building(&g, "barracks");
        g.set_production(city, ProductionItem::Building(barracks))
            .unwrap();
        (g, city)
    }

    #[test]
    fn without_the_vizier_a_finished_building_leaves_the_city_on_housing() {
        let (mut g, city) = vizier_city(false);
        g.apply_production(city, 1000);
        assert_eq!(g.city_production(city).item, Some(ProductionItem::Housing));
    }

    #[test]
    fn the_vizier_picks_the_next_building_in_the_list() {
        let (mut g, city) = vizier_city(true);
        g.apply_production(city, 1000);
        let guild = building(&g, "fighters_guild");
        assert_eq!(
            g.city_production(city).item,
            Some(ProductionItem::Building(guild)),
            "barracks done, so the guild that needs it comes next"
        );
        g.apply_production(city, 1000);
        let granary = building(&g, "granary");
        assert_eq!(
            g.city_production(city).item,
            Some(ProductionItem::Building(granary))
        );
        g.apply_production(city, 1000);
        assert_eq!(
            g.city_production(city).item,
            Some(ProductionItem::Housing),
            "nothing left on the list"
        );
    }

    #[test]
    fn the_vizier_skips_what_the_treasury_cannot_keep() {
        let (mut g, city) = vizier_city(true);
        g.player_mut(P0).unwrap().tax_rate = 0;
        let net = g.treasury(P0).net_gold();
        add_building(&mut g, city, "barracks");
        assert!(
            net < 3,
            "the test needs a poor treasury, net is {net} gold a turn"
        );
        assert_eq!(
            g.vizier_choice(city, P0),
            Some(ProductionItem::Building(building(&g, "granary"))),
            "the guild costs 3 a turn and is skipped"
        );
    }

    #[test]
    fn turning_the_vizier_on_gives_idle_cities_a_building() {
        let (mut g, city) = vizier_city(false);
        g.set_production(city, ProductionItem::Housing).unwrap();
        g.apply(Command::SetGrandVizier {
            player: P0,
            on: true,
        });
        let barracks = building(&g, "barracks");
        assert_eq!(
            g.city_production(city).item,
            Some(ProductionItem::Building(barracks))
        );
        // A city already building something is left alone.
        let other = found(&mut g, at(16, 6), "elf", 4);
        let granary = building(&g, "granary");
        g.set_production(other, ProductionItem::Building(granary))
            .unwrap();
        g.apply(Command::SetGrandVizier {
            player: P0,
            on: true,
        });
        assert_eq!(
            g.city_production(other).item,
            Some(ProductionItem::Building(granary))
        );
    }

    #[test]
    fn the_vizier_leaves_outposts_alone() {
        let (mut g, _) = vizier_city(true);
        let outpost = found(&mut g, at(4, 6), "elf", 0);
        assert_eq!(g.vizier_choice(outpost, P0), None);
    }

    #[test]
    fn buying_costs_more_the_less_is_done() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        let hall = building(&g, "builders_hall"); // 60
        g.set_production(city, ProductionItem::Building(hall))
            .unwrap();
        assert_eq!(g.buy_cost(city), Some(240), "nothing done: 4 x 60");
        g.world
            .insert_one(
                city,
                Production {
                    item: Some(ProductionItem::Building(hall)),
                    progress: 10,
                },
            )
            .unwrap();
        assert_eq!(
            g.buy_cost(city),
            Some(150),
            "remaining 50, accumulated 10 < 25: 3 x 50"
        );
        g.world
            .insert_one(
                city,
                Production {
                    item: Some(ProductionItem::Building(hall)),
                    progress: 40,
                },
            )
            .unwrap();
        assert_eq!(
            g.buy_cost(city),
            Some(40),
            "remaining 20, accumulated 40: 2 x 20"
        );
        let gold = g.player(P0).unwrap().gold;
        g.apply(Command::BuyProduction { city });
        assert_eq!(g.player(P0).unwrap().gold, gold - 40);
        let events = g.apply(Command::EndTurn);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ProductionCompleted { .. })),
            "done next turn"
        );
    }

    #[test]
    fn buying_needs_the_gold() {
        let mut g = world();
        g.player_mut(P0).unwrap().gold = 10;
        let city = found(&mut g, at(10, 6), "elf", 4);
        let hall = building(&g, "builders_hall");
        g.set_production(city, ProductionItem::Building(hall))
            .unwrap();
        assert_eq!(
            g.apply(Command::BuyProduction { city }),
            vec![Event::Rejected(crate::Rejection::Production(
                ProductionError::NotEnoughGold
            ))]
        );
    }

    #[test]
    fn selling_a_building_refunds_it_unless_others_need_it() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        add_building(&mut g, city, "barracks");
        add_building(&mut g, city, "fighters_guild");
        let barracks = building(&g, "barracks");
        assert_eq!(
            g.sell_building(city, barracks),
            Err(ProductionError::MissingBuilding)
        );
        let guild = building(&g, "fighters_guild");
        let gold = g.player(P0).unwrap().gold;
        assert_eq!(g.sell_building(city, guild), Ok(200));
        assert_eq!(g.player(P0).unwrap().gold, gold + 200);
        assert_eq!(g.sell_building(city, guild), Err(ProductionError::NotBuilt));
    }

    #[test]
    fn selling_is_blocked_through_a_chain_with_a_missing_link() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        // The war college needs the fighters guild, which needs the barracks; the guild is
        // not built, but the barracks still cannot go while the college stands.
        add_building(&mut g, city, "barracks");
        add_building(&mut g, city, "war_college");
        let barracks = building(&g, "barracks");
        assert_eq!(
            g.sell_building(city, barracks),
            Err(ProductionError::MissingBuilding)
        );
        let college = building(&g, "war_college");
        assert_eq!(g.sell_building(city, college), Ok(500));
        assert_eq!(g.sell_building(city, barracks), Ok(30));
    }

    fn put_ore(g: &mut Game, ore: &str, x: u32, y: u32) {
        let id = g.content.features.id_of(ore).unwrap();
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set_feature(x, y, Some(id));
    }

    #[test]
    fn weapon_quality_follows_the_alchemists_guild_and_the_ore() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        put_ore(&mut g, "mithril_ore", 11, 6);
        assert_eq!(g.city_weapon_quality(city), 0, "ore alone does nothing");
        add_building(&mut g, city, "alchemists_guild");
        assert_eq!(g.city_weapon_quality(city), 2, "mithril");
        put_ore(&mut g, "adamantium_ore", 9, 6);
        assert_eq!(g.city_weapon_quality(city), 3, "adamantium beats mithril");
        for x in [9, 11] {
            g.map
                .as_mut()
                .unwrap()
                .plane_mut(Plane::Arcanus)
                .set_corrupted(x, 6, true);
        }
        assert_eq!(g.city_weapon_quality(city), 1, "corrupted ore is useless");
    }

    #[test]
    fn units_are_trained_with_the_cities_weapons_and_alchemy_gives_magic() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        add_building(&mut g, city, "barracks");
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let train = |g: &mut Game| {
            g.set_production(city, ProductionItem::Unit(spear)).unwrap();
            g.apply_production(city, 100);
            g.units_at(at(10, 6)).into_iter().last().unwrap()
        };
        let plain = train(&mut g);
        assert_eq!(g.unit_weapon_quality(plain), 0);
        g.magic_mut(P0)
            .unwrap()
            .retorts
            .insert(crate::magic::Retort::Alchemy);
        let magic = train(&mut g);
        assert_eq!(g.unit_weapon_quality(magic), 1, "Alchemy: magic weapons");
        put_ore(&mut g, "mithril_ore", 11, 6);
        add_building(&mut g, city, "alchemists_guild");
        let mithril = train(&mut g);
        assert_eq!(g.unit_weapon_quality(mithril), 2);
    }

    #[test]
    fn replaced_buildings_cost_nothing_to_keep() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        add_building(&mut g, city, "barracks"); // upkeep 0 anyway
        add_building(&mut g, city, "smithy"); // 1
        add_building(&mut g, city, "armory"); // 2, replaces barracks
        assert_eq!(g.city_building_upkeep(city), 3);
        add_building(&mut g, city, "library"); // 1
        assert_eq!(g.city_building_upkeep(city), 4);
    }

    #[test]
    fn the_treasury_collects_taxes_and_pays_for_troops() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        let spear = g.content.rules.units.id_of("spear").unwrap();
        g.spawn_unit(P0, at(10, 6), spear);
        g.spawn_unit(P0, at(10, 6), spear);
        let eco = g.city_economy(city).unwrap();
        let before = g.player(P0).unwrap().gold;
        g.apply(Command::EndTurn);
        // Income 4 gold; food: 6 produced - 4 eaten = 2 spare, the 2 soldiers eat it all -> no food gold.
        // Upkeep: two spearmen at 1 gold each.
        assert_eq!(eco.gold, 4);
        assert_eq!(g.player(P0).unwrap().gold, before + 4 - 2);
    }

    #[test]
    fn spare_food_is_sold_for_gold() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        let eco = g.city_economy(city).unwrap();
        assert_eq!(eco.food, 6);
        let before = g.player(P0).unwrap().gold;
        g.apply(Command::EndTurn);
        // 6 food - 4 citizens = 2 spare, sold at two for one: +1, plus 4 tax.
        assert_eq!(g.player(P0).unwrap().gold, before + 4 + 1);
    }

    #[test]
    fn an_army_that_cannot_be_fed_or_paid_is_disbanded() {
        let mut g = world();
        found(&mut g, at(10, 6), "elf", 4);
        let spear = g.content.rules.units.id_of("spear").unwrap();
        // Spare food is 2: three soldiers eat 3, so one goes.
        for _ in 0..3 {
            g.spawn_unit(P0, at(11, 6), spear);
        }
        let events = g.apply(Command::EndTurn);
        assert!(events.contains(&Event::UnitsDisbanded {
            player: P0,
            units: 1
        }));
        assert_eq!(g.player_units(P0).len(), 2);
        // Broke: upkeep above the treasury sends the rest home.
        g.player_mut(P0).unwrap().gold = 0;
        g.player_mut(P0).unwrap().tax_rate = 0;
        let events = g.apply(Command::EndTurn);
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::UnitsDisbanded { .. })));
    }

    #[test]
    fn wounded_units_heal_faster_in_cities() {
        let mut g = world();
        found(&mut g, at(10, 6), "elf", 4);
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let field = g.spawn_unit(P0, at(14, 3), spear);
        let home = g.spawn_unit(P0, at(10, 6), spear);
        // Spearman: 3 figures x 2 hp = 6 hit points. Give both 4 damage.
        g.world.insert_one(field, Hurt(4)).unwrap();
        g.world.insert_one(home, Hurt(4)).unwrap();
        g.apply(Command::EndTurn);
        let hurt = |u| g.world.get::<&Hurt>(u).unwrap().0;
        assert!(
            hurt(home) <= hurt(field),
            "city {} vs field {}",
            hurt(home),
            hurt(field)
        );
        assert!(hurt(field) < 4, "even in the field something mends");
    }

    #[test]
    fn regenerating_units_mend_completely_each_turn() {
        let mut g = world();
        found(&mut g, at(10, 6), "elf", 4);
        let mut units = g.content.rules.units.defs().to_vec();
        for u in units.iter_mut() {
            if u.id == "spear" {
                u.abilities.push("regeneration".into());
            }
        }
        g.content.rules.units = crate::rules::Registry::new(units);
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let troll = g.spawn_unit(P0, at(14, 3), spear);
        g.world.insert_one(troll, Hurt(5)).unwrap();
        g.apply(Command::EndTurn);
        assert_eq!(g.world.get::<&Hurt>(troll).unwrap().0, 0);
    }

    #[test]
    fn wild_game_raises_an_outposts_growth_chance_by_its_food_and_its_bonus() {
        let mut g = world();
        // The test land feeds more than the usual largest city, which would hide the food.
        g.content.rules.config.max_city_size = 100;
        let outpost = found(&mut g, at(10, 6), "elf", 0);
        let plain = g.outpost_growth_chance(outpost);
        let game = g.content.features.id_of("game").unwrap();
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set_feature(10, 8, Some(game));
        // The special's own +10 and the 2 food it adds to what the land can feed.
        assert_eq!(g.outpost_growth_chance(outpost), plain + 10 + 2);
    }

    #[test]
    fn difficulty_does_not_change_outpost_growth_for_anyone() {
        let mut g = world();
        let outpost = found(&mut g, at(10, 6), "elf", 0);
        let normal = g.outpost_growth_chance(outpost);
        for difficulty in [0, 4] {
            g.difficulty = difficulty;
            assert_eq!(g.outpost_growth_chance(outpost), normal);
        }
    }

    #[test]
    fn outposts_grow_into_cities_in_time() {
        let mut g = world();
        let outpost = found(&mut g, at(10, 6), "elf", 0);
        for _ in 0..300 {
            g.apply(Command::EndTurn);
            if g.world.get::<&Population>(outpost).unwrap().figures > 0 {
                break;
            }
        }
        assert_eq!(
            g.world.get::<&Population>(outpost).unwrap().figures,
            1,
            "grew to a hamlet"
        );
        assert!(g.world.get::<&Farmers>(outpost).unwrap().0 <= 1);
    }

    #[test]
    fn farmers_can_be_set_but_not_below_what_feeds_the_city() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        g.apply(Command::SetFarmers { city, farmers: 0 });
        assert_eq!(
            g.world.get::<&Farmers>(city).unwrap().0,
            2,
            "raised to the minimum"
        );
        g.apply(Command::SetFarmers { city, farmers: 4 });
        assert_eq!(g.world.get::<&Farmers>(city).unwrap().0, 4);
        g.apply(Command::SetFarmers { city, farmers: 99 });
        assert_eq!(
            g.world.get::<&Farmers>(city).unwrap().0,
            4,
            "never more than the citizens"
        );
    }

    #[test]
    fn balancing_farmers_feeds_the_army() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 6);
        let spear = g.content.rules.units.id_of("spear").unwrap();
        for _ in 0..3 {
            g.spawn_unit(P0, at(10, 6), spear);
        }
        g.balance_farmers(P0);
        let t = g.treasury(P0);
        assert!(t.food_surplus >= t.food_upkeep, "{t:?}");
        assert_eq!(t.food_upkeep, 3);
        // Elves farm 3 food each: 2 farmers feed 6 citizens' food-wise only if 6 food; plus 3 for troops.
        let farmers = g.world.get::<&Farmers>(city).unwrap().0;
        assert_eq!(
            farmers, 3,
            "three farmers: 9 food for 6 citizens and 3 soldiers"
        );
        // Not enough land or people: everyone farms and the rest cannot be helped.
        for _ in 0..20 {
            g.spawn_unit(P0, at(11, 6), spear);
        }
        g.balance_farmers(P0);
        assert_eq!(g.world.get::<&Farmers>(city).unwrap().0, 6);
    }

    #[test]
    fn settlers_found_outposts_and_are_used_up() {
        let mut g = world();
        let settler = g.content.rules.units.id_of("settler").unwrap();
        let unit = g.spawn_unit(P0, at(10, 6), settler);
        let events = g.apply(Command::SettleOutpost {
            unit,
            name: "Newtown".into(),
        });
        assert!(
            matches!(events[..], [Event::CityFounded { .. }]),
            "{events:?}"
        );
        let city = g.city_at(at(10, 6)).unwrap();
        assert_eq!(
            g.world.get::<&Population>(city).unwrap().figures,
            0,
            "an outpost"
        );
        assert!(
            g.world.get::<&crate::economy::CityRace>(city).is_ok(),
            "of the settlers' race"
        );
        assert!(g.unit_position(unit).is_none(), "the settlers are gone");
        // Too close to the new outpost.
        let again = g.spawn_unit(P0, at(12, 6), settler);
        assert_eq!(
            g.apply(Command::SettleOutpost {
                unit: again,
                name: "Nope".into()
            }),
            vec![Event::Rejected(crate::Rejection::FoundCity(
                crate::city::FoundCityError::TooCloseToCity
            ))]
        );
        assert!(
            g.unit_position(again).is_some(),
            "refused settlers stay alive"
        );
        // Soldiers cannot found cities.
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let soldier = g.spawn_unit(P0, at(18, 6), spear);
        assert!(matches!(
            g.apply(Command::SettleOutpost {
                unit: soldier,
                name: "x".into()
            })[..],
            [Event::Rejected(_)]
        ));
    }

    #[test]
    fn tax_rate_is_clamped_to_the_table() {
        let mut g = world();
        g.apply(Command::SetTaxRate {
            player: P0,
            rate: 99,
        });
        assert_eq!(g.player(P0).unwrap().tax_rate, 6);
    }

    #[test]
    fn famine_and_gaias_blessing_change_the_land() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), "elf", 4);
        let base = g.city_economy_with(city, 2).unwrap();
        let famine = g.content.rules.spells.id_of("famine").unwrap();
        g.world
            .get::<&mut Enchantments>(city)
            .unwrap()
            .0
            .push((famine, PlayerId(1)));
        let hungry = g.city_economy_with(city, 2).unwrap();
        assert!(hungry.food < base.food);
        let gaia = g.content.rules.spells.id_of("gaias_blessing").unwrap();
        g.world.get::<&mut Enchantments>(city).unwrap().0.clear();
        g.world
            .get::<&mut Enchantments>(city)
            .unwrap()
            .0
            .push((gaia, P0));
        // Gaia's blessing raises the food the land can carry by half.
        let blessed = g.city_economy_with(city, 2).unwrap();
        assert!(blessed.max_size >= base.max_size);
    }

    #[test]
    fn gaias_blessing_pacifies_two_rebels() {
        let mut g = world();
        g.player_mut(P0).unwrap().tax_rate = 4; // 45% unrest: 4 rebels of 10
        let city = found(&mut g, at(10, 6), "elf", 10);
        assert_eq!(g.city_economy(city).unwrap().rebels, 4);
        let gaia = g.content.rules.spells.id_of("gaias_blessing").unwrap();
        g.world
            .get::<&mut Enchantments>(city)
            .unwrap()
            .0
            .push((gaia, P0));
        assert_eq!(g.city_economy(city).unwrap().rebels, 2);
    }
}
