//! What a city produces each turn. Base amounts follow the original game's formulas
//! (farmers and workers, the worked area, mines, trade); everything content defines
//! (buildings, race traits, city enchantments) arrives as stat modifiers.
//!
//! The rules are in [`Economy`] (task 186): a read handle over the world, the map, the content
//! and the players, made by `Game::economy()`. A city's yields are worked out from those alone,
//! so a test needs a world, a map and the content, no `Game`. `Game` keeps thin methods with
//! the old names that ask it.
//!
//! Units: population is in citizens ("figures", 1000 people each) with a fractional part
//! in tens of people; growth is in tens of people per turn; food in whole food; production
//! and gold in whole points.

use std::collections::{HashSet, VecDeque};

use hecs::World;

use crate::city::{City, PlayerId, Position};
use crate::city_spells::RELIGIOUS;
use crate::content::{Content, TerrainClass};
use crate::magic::Retort;
use crate::map::{Map, Plane};
use crate::player::Player;
use crate::rules::stats::{compute, passive_modifiers, Modifier, Source};
use crate::rules::{RaceDef, Stat};
use crate::units::{Owner, Units};
use crate::Game;

/// Citizens plus tens of people (0..100), so one citizen is 100 tens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Population {
    pub figures: u32,
    pub tens: u32,
}

impl Population {
    pub fn people(self) -> u32 {
        self.figures * 1000 + self.tens * 10
    }
}

/// Citizens set to farm. Everyone else works, unless rebelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Farmers(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Buildings(pub Vec<crate::rules::BuildingId>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CityRace(pub crate::rules::RaceId);

/// City enchantments with the player who cast each.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Enchantments(pub Vec<(crate::rules::SpellId, PlayerId)>);

/// Everything a city yields this turn.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CityEconomy {
    pub figures: u32,
    pub farmers: u32,
    pub workers: u32,
    pub rebels: u32,
    /// Largest population the surroundings and buildings feed.
    pub max_size: u32,
    pub food: i32,
    pub production: i32,
    pub gold: i32,
    pub gold_upkeep: i32,
    pub research: i32,
    pub mana: i32,
    /// Change in tens of people this turn.
    pub growth_tens: i32,
    /// Percent of full price units cost here (mines lower it, never below 50).
    pub unit_cost_percent: i32,
}

/// Offsets of the worked area: 5x5 without its corners.
pub fn area_offsets() -> impl Iterator<Item = (i32, i32)> {
    (-2i32..=2)
        .flat_map(|dy| (-2i32..=2).map(move |dx| (dx, dy)))
        .filter(|(dx, dy)| !(dx.abs() == 2 && dy.abs() == 2))
}

fn wrap_distance(width: u32, a: u32, b: u32) -> u32 {
    let d = a.abs_diff(b);
    d.min(width - d)
}

/// The economy rules' view of the game: the world, the map, the content and the players (for the
/// owner's tax rate, capital race and retorts).
#[derive(Clone, Copy)]
pub struct Economy<'a> {
    pub(crate) world: &'a World,
    pub(crate) map: Option<&'a Map>,
    pub(crate) content: &'a Content,
    pub(crate) players: &'a [Player],
}

impl<'a> Economy<'a> {
    pub fn new(
        world: &'a World,
        map: Option<&'a Map>,
        content: &'a Content,
        players: &'a [Player],
    ) -> Self {
        Economy {
            world,
            map,
            content,
            players,
        }
    }

    /// The unit rules over the same pieces.
    pub(crate) fn units(&self) -> Units<'a> {
        Units::new(self.world, self.content, self.players)
    }

    pub(crate) fn player(&self, id: PlayerId) -> Option<&'a Player> {
        self.players.iter().find(|p| p.id == id)
    }

    pub(crate) fn magic(&self, id: PlayerId) -> Option<&'a crate::magic::Magic> {
        self.player(id).map(|p| &p.magic)
    }

    /// All cities with their positions, oldest first.
    pub fn cities(&self) -> Vec<(hecs::Entity, City, Position)> {
        let mut out: Vec<_> = self
            .world
            .query::<(&City, &Position)>()
            .iter()
            .map(|(e, (c, p))| (e, c.clone(), *p))
            .collect();
        let units = self.units();
        out.sort_by_key(|(e, ..)| (units.serial_of(*e), e.id()));
        out
    }

    /// Tiles a city at `at` works, with whether another city works each too.
    pub fn city_area(&self, at: Position) -> Vec<(Position, bool)> {
        let Some(map) = self.map else {
            return Vec::new();
        };
        let (w, h) = (map.width() as i32, map.height() as i32);
        let others: Vec<Position> = self
            .cities()
            .into_iter()
            .map(|(_, _, p)| p)
            .filter(|p| p.plane == at.plane && *p != at)
            .collect();
        let works = |city: Position, x: u32, y: u32| {
            let dx = wrap_distance(map.width(), city.x, x) as i32;
            let dy = city.y.abs_diff(y) as i32;
            dx <= 2 && dy <= 2 && !(dx == 2 && dy == 2)
        };
        let mut out = Vec::new();
        for (dx, dy) in area_offsets() {
            let y = at.y as i32 + dy;
            if !(0..h).contains(&y) {
                continue;
            }
            let x = (at.x as i32 + dx).rem_euclid(w) as u32;
            let shared = others.iter().any(|&o| works(o, x, y as u32));
            out.push((
                Position {
                    plane: at.plane,
                    x,
                    y: y as u32,
                },
                shared,
            ));
        }
        out
    }

    /// Modifiers from the city's race, buildings and enchantments.
    pub fn city_modifiers(&self, city: hecs::Entity) -> Vec<Modifier> {
        let rules = &self.content.rules;
        let mut mods = Vec::new();
        if let Ok(race) = self.world.get::<&CityRace>(city) {
            let def = rules.races.get(race.0);
            mods.extend(passive_modifiers(&def.effects, Source::Race(race.0)));
        }
        if let Ok(buildings) = self.world.get::<&Buildings>(city) {
            let evil = self.evil_presence_active(city);
            // Divine and Infernal Power add half again to the power and the pacifying of the
            // religious buildings: the original adds the buildings up, then takes 150 percent.
            let owner = self.world.get::<&City>(city).ok().map(|c| c.owner);
            let devout = owner
                .and_then(|o| self.magic(o))
                .is_some_and(|m| m.has(Retort::DivinePower) || m.has(Retort::InfernalPower));
            let (mut power, mut pacify) = (0, 0);
            for &b in &buildings.0 {
                let def = rules.buildings.get(b);
                let mut found = passive_modifiers(&def.effects, Source::Building(b));
                // Evil Presence destroys the magic power of the religious buildings (the help;
                // the original's code stops their pacifying instead, B22).
                if evil && RELIGIOUS.contains(&def.id.as_str()) {
                    found.retain(|m| m.stat != Stat::CityMana);
                }
                if RELIGIOUS.contains(&def.id.as_str()) {
                    for m in &found {
                        match m.stat {
                            Stat::CityMana => power += m.value,
                            Stat::CityUnrest => pacify -= m.value,
                            _ => {}
                        }
                    }
                }
                mods.extend(found);
            }
            if devout {
                let more = |total: i32| total * 150 / 100 - total;
                if power > 0 {
                    mods.push(Modifier::new(
                        Stat::CityMana,
                        crate::rules::ModOp::Add,
                        more(power),
                        Source::Other,
                    ));
                }
                if pacify > 0 {
                    mods.push(Modifier::new(
                        Stat::CityUnrest,
                        crate::rules::ModOp::Add,
                        -more(pacify),
                        Source::Other,
                    ));
                }
            }
        }
        if let Ok(enchantments) = self.world.get::<&Enchantments>(city) {
            for &(spell, _) in &enchantments.0 {
                mods.extend(passive_modifiers(
                    &rules.spells.get(spell).effects,
                    Source::Spell(spell),
                ));
            }
        }
        mods.extend(self.units().temp_modifiers(city));
        mods
    }

    pub fn city_has_building(&self, city: hecs::Entity, building: &str) -> bool {
        let Some(id) = self.content.rules.buildings.id_of(building) else {
            return false;
        };
        self.world
            .get::<&Buildings>(city)
            .is_ok_and(|b| b.0.contains(&id))
    }

    /// Total weight of citizens in cities road-connected to `at` (other races count fully,
    /// the same race half): the trade that boosts gold.
    fn road_trade(
        &self,
        city: hecs::Entity,
        at: Position,
        race: Option<crate::rules::RaceId>,
    ) -> i32 {
        let Some(map) = self.map else {
            return 0;
        };
        let grid = map.plane(at.plane);
        if !grid.has_road(at.x as i32, at.y as i32) {
            return 0;
        }
        let (w, h) = (grid.width() as i32, grid.height() as i32);
        let mut seen = HashSet::from([(at.x, at.y)]);
        let mut queue = VecDeque::from([(at.x, at.y)]);
        while let Some((x, y)) = queue.pop_front() {
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let ny = y as i32 + dy;
                    if (dx == 0 && dy == 0) || !(0..h).contains(&ny) {
                        continue;
                    }
                    let nx = (x as i32 + dx).rem_euclid(w);
                    if grid.has_road(nx, ny) && seen.insert((nx as u32, ny as u32)) {
                        queue.push_back((nx as u32, ny as u32));
                    }
                }
            }
        }
        let mut total = 0;
        for (e, _, p) in self.cities() {
            if e == city || p.plane != at.plane || !seen.contains(&(p.x, p.y)) {
                continue;
            }
            let pop = self.world.get::<&Population>(e).map_or(0, |p| p.figures) as i32;
            let other = self.world.get::<&CityRace>(e).ok().map(|r| r.0);
            total += if other == race { pop / 2 } else { pop };
        }
        total
    }

    /// What a city at `at` could grow to on its land alone, with no buildings, race or
    /// enchantments: the estimate the original's new-game placement judges a home site by.
    pub(crate) fn site_max_size(&self, at: Position) -> u32 {
        let Some(map) = self.map else {
            return 0;
        };
        let grid = map.plane(at.plane);
        let (mut food_x2, mut wild_food) = (0i32, 0i32);
        for (p, shared) in self.city_area(at) {
            let Some(def) = grid
                .get(p.x as i32, p.y as i32)
                .and_then(|t| self.content.terrains.get(t))
            else {
                continue;
            };
            let food = i32::from(def.food) * 2;
            food_x2 += if shared { food / 2 } else { food };
            if let Some(f) = grid
                .feature(p.x as i32, p.y as i32)
                .and_then(|f| self.content.features.get(f))
            {
                wild_food += if shared { f.city.food / 2 } else { f.city.food };
            }
        }
        ((food_x2 / 4 + wild_food).max(0) as u32).min(self.content.rules.config.max_city_size)
    }

    /// Citizens needed on farms so the city feeds itself (all of them if it cannot).
    pub fn city_min_farmers(&self, city: hecs::Entity) -> u32 {
        let figures = self.world.get::<&Population>(city).map_or(0, |p| p.figures);
        (0..=figures)
            .find(|&f| {
                self.city_economy_with(city, f)
                    .is_some_and(|e| e.food >= figures as i32)
            })
            .unwrap_or(figures)
    }

    pub fn city_economy(&self, city: hecs::Entity) -> Option<CityEconomy> {
        let farmers = self.world.get::<&Farmers>(city).map_or(0, |f| f.0);
        self.city_economy_with(city, farmers)
    }

    /// Compute the city's yields as if `farmers` citizens farmed.
    pub fn city_economy_with(&self, city: hecs::Entity, farmers: u32) -> Option<CityEconomy> {
        let at = *self.world.get::<&Position>(city).ok()?;
        let owner = self.world.get::<&City>(city).ok()?.owner;
        let pop = self
            .world
            .get::<&Population>(city)
            .map(|p| *p)
            .unwrap_or_default();
        let race_id = self.world.get::<&CityRace>(city).ok().map(|r| r.0);
        let rules = &self.content.rules;
        let race: Option<&RaceDef> = race_id.map(|r| rules.races.get(r));
        let map = self.map?;
        let grid = map.plane(at.plane);
        let mods = self.city_modifiers(city);
        let player = self.player(owner);
        let figures = pop.figures;
        let farmers = farmers.min(figures);

        let has = |name: &str| self.city_has_building(city, name);
        let miners = has("miners_guild");
        let dwarf = race.is_some_and(|r| r.double_mining);

        // Worked tiles: food, production bonus, mines.
        let area = self.city_area(at);
        let (mut terrain_food_x2, mut production_percent) = (0i32, 0i32);
        let (mut wild_food, mut mine_gold, mut mine_mana, mut cost_reduction) = (0, 0, 0, 0);
        for &(p, shared) in &area {
            let Some(def) = grid
                .get(p.x as i32, p.y as i32)
                .and_then(|t| self.content.terrains.get(t))
            else {
                continue;
            };
            // Corrupted land gives nothing at all.
            let spoiled = grid.is_corrupted(p.x as i32, p.y as i32);
            let mut food = if spoiled { 0 } else { i32::from(def.food) * 2 };
            let mut prod = if spoiled { 0 } else { def.production_percent };
            if shared {
                food /= 2;
                prod /= 2;
            }
            terrain_food_x2 += food;
            production_percent += prod;
            if let Some(f) = grid
                .feature(p.x as i32, p.y as i32)
                .filter(|_| !spoiled)
                .and_then(|f| self.content.features.get(f))
            {
                let b = f.city;
                let scale = |mut v: i32, miners_factor: (i32, i32)| {
                    if dwarf {
                        v *= 2;
                    }
                    if miners {
                        v = v * miners_factor.0 / miners_factor.1;
                    }
                    if shared {
                        v /= 2;
                    }
                    v
                };
                mine_gold += scale(b.gold, (3, 2));
                mine_mana += scale(b.mana, (3, 2));
                wild_food += if shared { b.food / 2 } else { b.food };
                let mut cut = b.unit_cost_percent * if miners { 2 } else { 1 };
                if dwarf {
                    cut *= 2;
                }
                if shared {
                    cut /= 2;
                }
                cost_reduction += cut;
            }
        }
        if self.city_has_enchantment(city, "gaias_blessing") {
            terrain_food_x2 = terrain_food_x2 * 3 / 2;
        }
        let terrain_food = terrain_food_x2 / 4;

        // Rebels.
        let tax_rate = player.map_or(1, |p| p.tax_rate) as usize;
        let capital = player.and_then(|p| p.capital_race);
        let racial = match (capital, race_id) {
            (Some(c), Some(r)) => {
                rules
                    .config
                    .racial_unrest
                    .get(c.index())
                    .and_then(|row| row.get(r.index()))
                    .copied()
                    .unwrap_or(0) as i32
                    * 10
            }
            _ => 0,
        };
        let tax_unrest = rules
            .config
            .tax_unrest_percent
            .get(tax_rate)
            .copied()
            .unwrap_or(0) as i32;
        let unrest_percent = compute(Stat::CityUnrestPercent, racial + tax_unrest, &mods);
        let raw_rebels = (figures as i32 * unrest_percent / 100).max(0);
        // Pacification: negative CityUnrest modifiers calm the city; troops help if still short.
        let mut pacify = -compute(Stat::CityUnrest, 0, &mods);
        // Gaia's Blessing pacifies two (the wiki and the original's code say so; its help text
        // does not).
        if self.city_has_enchantment(city, "gaias_blessing") {
            pacify += 2;
        }
        if pacify < raw_rebels {
            let police = self
                .units()
                .units_at(at)
                .into_iter()
                .filter(|&u| self.units().unit_owner(u) == Some(owner))
                .filter(|&u| {
                    self.units().unit_def(u).is_some_and(|d| {
                        matches!(
                            d.kind,
                            crate::rules::UnitKind::Normal | crate::rules::UnitKind::Hero
                        )
                    })
                })
                .count() as i32;
            pacify += police / 2;
        }
        // Stream of Life: the citizens are so pleased that unrest disappears (the original's
        // code reads the owner's table instead of the city's, see B8).
        let rebels = if self.city_has_enchantment(city, "stream_of_life") {
            0
        } else {
            (raw_rebels - pacify).clamp(0, figures as i32) as u32
        };
        let farmers = farmers.min(figures - rebels.min(figures));
        let workers = figures - farmers - rebels;

        // Food.
        let farmer_rate = i32::from(race.map_or(2, |r| r.farmer_food));
        let farmer_rate = compute(Stat::FarmerFood, farmer_rate, &mods);
        let mut food = compute(Stat::CityFood, farmers as i32 * farmer_rate, &mods);
        if food > terrain_food {
            food = terrain_food + (food - terrain_food) / 2;
        }
        let flat = compute(Stat::CityFoodFlat, 0, &mods);
        food += flat + wild_food;
        // The city feeds as many as its land, farm work and buildings allow.
        let famine = self.city_has_enchantment(city, "famine");
        let pre_cap_adds = compute(Stat::CityFood, 0, &mods);
        let mut max_size = terrain_food;
        if famine {
            max_size /= 2;
        }
        let max_size = ((max_size + pre_cap_adds + flat + wild_food).max(0) as u32)
            .min(rules.config.max_city_size);

        // Production.
        let per_worker_x2 = i32::from(race.map_or(4, |r| r.worker_production_x2));
        let base_production = (workers as i32 * per_worker_x2 + farmers as i32 + 1) / 2;
        let mut production_mods = mods.clone();
        production_mods.push(Modifier::new(
            Stat::CityProduction,
            crate::rules::ModOp::Percent,
            production_percent,
            Source::Other,
        ));
        let production = compute(Stat::CityProduction, base_production, &production_mods);

        // Gold: taxes, trade bonus, mines, then percentage buildings.
        let mut gold = (figures as i32 - rebels as i32) * tax_rate as i32 / 2;
        if dwarf {
            gold *= 2;
        }
        gold += mine_gold;
        let coast = (-1i32..=1).any(|dy| {
            (-1i32..=1).any(|dx| {
                (dx != 0 || dy != 0)
                    && grid
                        .get(at.x as i32 + dx, at.y as i32 + dy)
                        .and_then(|t| self.content.terrains.get(t))
                        .is_some_and(|d| d.class == TerrainClass::Ocean)
            })
        });
        // Shore 10, river 20, river mouth 30 (Square_Gold_Bonus in the original).
        let river = grid.has_river(at.x as i32, at.y as i32);
        let site_bonus = (match (river, coast) {
            (false, false) => 0,
            (false, true) => 10,
            (true, false) => 20,
            (true, true) => 30,
        } + self.road_trade(city, at, race_id)
            + race.map_or(0, |r| r.trade_percent))
        .min(figures as i32 * 3);
        let mut gold_mods = mods.clone();
        gold_mods.push(Modifier::new(
            Stat::CityGold,
            crate::rules::ModOp::Percent,
            site_bonus,
            Source::Other,
        ));
        let mut gold = compute(Stat::CityGold, gold, &gold_mods);
        if let Ok(p) = self.world.get::<&crate::production::Production>(city) {
            if p.item == Some(crate::production::ProductionItem::TradeGoods) {
                gold += production / 2;
            }
        }
        let gold = gold.min(255);

        let gold_upkeep = self.city_building_upkeep(city) as i32;
        let research = compute(Stat::CityResearch, 0, &mods);
        let citizen_mana =
            figures as i32 * i32::from(race.map_or(0, |r| r.mana_per_citizen_x2)) / 2;
        let mana = compute(Stat::CityMana, citizen_mana + mine_mana, &mods).max(0);

        // Growth, with a bonus while the city builds housing.
        let housing = match self.world.get::<&crate::production::Production>(city) {
            Ok(p) if p.item == Some(crate::production::ProductionItem::Housing) => {
                let mut bonus = if figures == 1 {
                    50
                } else if figures > 1 {
                    (figures as i32 - rebels as i32 - farmers as i32) * 100 / figures as i32
                } else {
                    0
                };
                if has("sawmill") {
                    bonus += 10;
                }
                if has("builders_hall") {
                    bonus += 15;
                }
                bonus
            }
            _ => 0,
        };
        let growth_tens = growth_rate(
            self.content.rules.config.max_city_size,
            figures,
            food,
            max_size,
            &mods,
            housing,
        );

        Some(CityEconomy {
            figures,
            farmers,
            workers,
            rebels,
            max_size,
            food,
            production,
            gold,
            gold_upkeep,
            research,
            mana,
            growth_tens,
            unit_cost_percent: (100 - cost_reduction).max(50),
        })
    }

    /// Gold the buildings of a city cost to maintain (replaced buildings are free).
    pub fn city_building_upkeep(&self, city: hecs::Entity) -> u32 {
        let rules = &self.content.rules;
        let Ok(buildings) = self.world.get::<&Buildings>(city) else {
            return 0;
        };
        let replaced: HashSet<_> = buildings
            .0
            .iter()
            .filter_map(|&b| rules.buildings.get(b).replaces.as_deref())
            .filter_map(|n| rules.buildings.id_of(n))
            .collect();
        buildings
            .0
            .iter()
            .filter(|b| !replaced.contains(b))
            .map(|&b| rules.buildings.get(b).upkeep_gold)
            .sum()
    }

    /// The terrain class of a square.
    pub fn terrain_class_at(&self, at: Position) -> Option<crate::content::TerrainClass> {
        let map = self.map?;
        let id = map.plane(at.plane).get(at.x as i32, at.y as i32)?;
        self.content.terrains.get(id).map(|d| d.class)
    }

    /// The intact lair on a square, with its index.
    pub fn lair_at(&self, pos: Position) -> Option<(usize, &'a crate::map::Lair)> {
        let map = self.map?;
        let i = map.lair_at(pos.plane, pos.x, pos.y)?;
        Some((i, &map.lairs()[i]))
    }

    /// The city standing at `pos`.
    pub fn city_at(&self, pos: Position) -> Option<hecs::Entity> {
        self.cities()
            .into_iter()
            .find(|(_, _, p)| *p == pos)
            .map(|(e, ..)| e)
    }

    /// The player owning a city.
    pub fn city_owner(&self, city: hecs::Entity) -> Option<PlayerId> {
        self.world.get::<&City>(city).ok().map(|c| c.owner)
    }

    /// Whether any unit of `owner` could be a garrison here (used by display code).
    pub fn garrison(&self, at: Position) -> Vec<hecs::Entity> {
        self.units()
            .units_at(at)
            .into_iter()
            .filter(|&u| self.world.get::<&Owner>(u).is_ok())
            .collect()
    }

    /// Whether `plane` has any city (used by tests and the client).
    pub fn has_cities_on(&self, plane: Plane) -> bool {
        self.cities().iter().any(|(_, _, p)| p.plane == plane)
    }
}

impl Game {
    /// The economy rules over the world, the map, the content and the players.
    pub fn economy(&self) -> Economy<'_> {
        Economy::new(&self.world, self.map.as_ref(), &self.content, &self.players)
    }

    /// Tiles a city at `at` works, with whether another city works each too.
    pub fn city_area(&self, at: Position) -> Vec<(Position, bool)> {
        self.economy().city_area(at)
    }

    /// Modifiers from the city's race, buildings and enchantments.
    pub fn city_modifiers(&self, city: hecs::Entity) -> Vec<Modifier> {
        self.economy().city_modifiers(city)
    }

    pub fn city_has_building(&self, city: hecs::Entity, building: &str) -> bool {
        self.economy().city_has_building(city, building)
    }

    /// What a city at `at` could grow to on its land alone.
    pub(crate) fn site_max_size(&self, at: Position) -> u32 {
        self.economy().site_max_size(at)
    }

    /// Citizens needed on farms so the city feeds itself (all of them if it cannot).
    pub fn city_min_farmers(&self, city: hecs::Entity) -> u32 {
        self.economy().city_min_farmers(city)
    }

    pub fn city_economy(&self, city: hecs::Entity) -> Option<CityEconomy> {
        self.economy().city_economy(city)
    }

    /// Compute the city's yields as if `farmers` citizens farmed.
    pub fn city_economy_with(&self, city: hecs::Entity, farmers: u32) -> Option<CityEconomy> {
        self.economy().city_economy_with(city, farmers)
    }

    /// Gold the buildings of a city cost to maintain (replaced buildings are free).
    pub fn city_building_upkeep(&self, city: hecs::Entity) -> u32 {
        self.economy().city_building_upkeep(city)
    }

    /// The player owning a city.
    pub fn city_owner(&self, city: hecs::Entity) -> Option<PlayerId> {
        self.economy().city_owner(city)
    }

    /// The units on a square that could be a garrison (used by display code).
    pub fn garrison(&self, at: Position) -> Vec<hecs::Entity> {
        self.economy().garrison(at)
    }

    /// Whether `plane` has any city (used by tests and the client).
    pub fn has_cities_on(&self, plane: Plane) -> bool {
        self.economy().has_cities_on(plane)
    }
}

/// Population change for a turn, in tens of people (negative when starving or over the limit).
///
/// The original returns zero early for a city at its maximum size and for one at the largest
/// size a city can have, which also stopped such cities shrinking from starvation or from being
/// over their limit (drake178's bugs 1 to 3 in `City_Growth_Rate`). Here those cases only stop
/// growth; shrinking always applies.
fn growth_rate(
    max_city_size: u32,
    figures: u32,
    food: i32,
    max_size: u32,
    mods: &[Modifier],
    housing_percent: i32,
) -> i32 {
    if figures == 0 {
        return 0;
    }
    if max_size == 0 {
        return -(figures as i32);
    }
    if food < figures as i32 {
        return (food - figures as i32) * 5;
    }
    if figures == max_size {
        return 0;
    }
    // Half the room left, plus what the race and buildings add (a ten is ten people).
    let rate = (max_size as i32 - figures as i32 + 1) / 2;
    let rate = compute(Stat::CityGrowth, rate * 10, mods) / 10;
    let percent = compute(Stat::CityGrowthPercent, 0, mods) + housing_percent;
    let rate = rate * (100 + percent) / 100;
    // Never grow past the limit, whatever the bonuses add.
    if figures > max_size || figures >= max_city_size {
        return rate.min(0);
    }
    rate
}

#[cfg(test)]
mod growth_tests {
    use super::*;

    fn cap() -> u32 {
        Content::default().rules.config.max_city_size
    }

    fn rate(figures: u32, food: i32, max_size: u32) -> i32 {
        growth_rate(cap(), figures, food, max_size, &[], 0)
    }

    #[test]
    fn a_city_at_its_maximum_size_does_not_grow_but_can_starve() {
        assert_eq!(rate(8, 8, 8), 0);
        assert_eq!(rate(8, 6, 8), -10, "two short of food: five tens each");
    }

    #[test]
    fn a_city_over_its_limit_shrinks_even_at_the_largest_size() {
        assert!(rate(14, 14, 10) < 0);
        let cap = cap();
        assert!(
            rate(cap, cap as i32, cap - 4) < 0,
            "the original returned 0 here"
        );
        assert!(rate(cap, cap as i32, cap + 3) <= 0, "the cap stops growth");
        assert_eq!(
            rate(cap, cap as i32 - 2, cap + 3),
            -10,
            "but not starvation"
        );
    }

    #[test]
    fn growth_stops_at_zero_people_and_a_zero_limit_kills() {
        assert_eq!(rate(0, 5, 5), 0);
        assert_eq!(rate(3, 5, 0), -3);
    }
}

#[cfg(test)]
mod handle_tests {
    use super::*;
    use crate::map::Grid;
    use crate::testutil::content_with_economy;

    #[test]
    fn a_cities_yields_come_from_a_world_a_map_and_the_content_alone() {
        let content = content_with_economy();
        let grass = content.terrains.id_of("grassland").unwrap();
        let map = Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass));
        let at = Position {
            plane: Plane::Arcanus,
            x: 10,
            y: 7,
        };
        let mut world = World::new();
        let city = world.spawn((
            City {
                name: "Town".into(),
                owner: PlayerId(0),
            },
            at,
            Population {
                figures: 4,
                tens: 0,
            },
            Farmers(0),
            Buildings::default(),
            Enchantments::default(),
        ));
        let players = [Player::new(PlayerId(0), "Merlin")];
        let eco = Economy::new(&world, Some(&map), &content, &players);

        let now = eco.city_economy(city).unwrap();
        assert_eq!(now.figures, 4);
        assert_eq!(now.farmers + now.workers + now.rebels, 4);
        let farmers = eco.city_min_farmers(city);
        let fed = eco.city_economy_with(city, farmers).unwrap();
        assert!(fed.food >= 4, "the minimum farmers feed the city");
        assert_eq!(eco.city_owner(city), Some(PlayerId(0)));
        assert_eq!(eco.cities().len(), 1);
        assert!(eco.has_cities_on(Plane::Arcanus));
        assert!(!eco.has_cities_on(Plane::Myrror));
        assert_eq!(
            eco.city_area(at).len(),
            21,
            "a 5x5 area without its corners"
        );
    }
}
