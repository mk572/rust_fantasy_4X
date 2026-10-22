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

use crate::city::{City, CityInfo};
use crate::content::{Content, TerrainClass};
use crate::map::{Map, Plane, Position};
use crate::player::{Owner, PlayerId, PlayerRef};

use crate::rules::stats::{compute, passive_modifiers, Modifier, Source};
use crate::rules::{RaceDef, Stat};
use crate::spell::city::RELIGIOUS;
use crate::unit::UnitView;

use crate::game::Game;
use crate::wizard::magic::Retort;

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
    /// The world events going on; none unless [`Economy::with_events`] says otherwise.
    pub(crate) events: &'a crate::world_event::Events,
}

impl<'a> Economy<'a> {
    pub fn new(world: &'a World, map: Option<&'a Map>, content: &'a Content) -> Self {
        Economy {
            world,
            map,
            content,
            events: &crate::world_event::NONE,
        }
    }

    /// The same view with the world events that are going on.
    pub fn with_events(mut self, events: &'a crate::world_event::Events) -> Self {
        self.events = events;
        self
    }

    /// The unit rules over the same pieces.
    pub(crate) fn units(&self) -> UnitView<'a> {
        UnitView::new(self.world, self.content)
    }

    pub(crate) fn player(&self, id: PlayerId) -> Option<PlayerRef<'a>> {
        crate::player::find(self.world, id)
    }

    pub(crate) fn wizard(&self, id: PlayerId) -> Option<crate::wizard::entity::Wizard<'a>> {
        crate::wizard::entity::find(self.world, id)
    }

    /// All cities with their positions, oldest first.
    pub fn cities(&self) -> Vec<(hecs::Entity, CityInfo, Position)> {
        let mut out: Vec<_> = self
            .world
            .query::<(&City, &Position, &Owner)>()
            .iter()
            .filter_map(|(e, (c, p, o))| {
                let owner = crate::player::id_of(self.world, o.0)?;
                Some((
                    e,
                    CityInfo {
                        name: c.name.clone(),
                        owner,
                    },
                    *p,
                ))
            })
            .collect();
        let units = self.units();
        out.sort_by_cached_key(|(e, ..)| (units.serial_of(*e), e.id()));
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
            let owner = self.city_owner(city);
            let devout = owner
                .and_then(|o| self.wizard(o))
                .is_some_and(|m| m.has(Retort::DivinePower) || m.has(Retort::InfernalPower));
            let (mut power, mut pacify) = (0, 0);
            // While any wizard keeps Armageddon going, no building calms a city.
            let armageddon = crate::player::ids(self.world)
                .into_iter()
                .filter_map(|id| self.wizard(id))
                .any(|w| {
                    crate::spell::global_enchantment::magic_has_global(
                        &w,
                        self.content,
                        "armageddon",
                    )
                });
            for &b in &buildings.0 {
                let def = rules.buildings.get(b);
                let mut found = passive_modifiers(&def.effects, Source::Building(b));
                if armageddon {
                    found.retain(|m| m.stat != Stat::CityUnrest);
                }
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
            // Divine and Infernal Power take 150 percent of the religious buildings' power,
            // Dark Rituals doubles it, and the two together make 300 percent (`CITYCALC.c`).
            let rituals = self.city_has_enchantment(city, "dark_rituals");
            let percent = if devout { 150 } else { 100 } * if rituals { 2 } else { 1 };
            let religious = power * percent / 100;
            if religious > power {
                mods.push(Modifier::new(
                    Stat::CityMana,
                    crate::rules::ModOp::Add,
                    religious - power,
                    Source::Other,
                ));
            }
            if devout && pacify > 0 {
                mods.push(Modifier::new(
                    Stat::CityUnrest,
                    crate::rules::ModOp::Add,
                    -(pacify * 150 / 100 - pacify),
                    Source::Other,
                ));
            }
            // A moon in the sky changes what the religious buildings give.
            let books = |realm| {
                owner
                    .and_then(|o| self.wizard(o))
                    .is_some_and(|m| m.books_in(realm) > 0)
            };
            let moon = self
                .events
                .moon_mana(religious, books("life"), books("death"));
            if moon != 0 {
                mods.push(Modifier::new(
                    Stat::CityMana,
                    crate::rules::ModOp::Add,
                    moon,
                    Source::Other,
                ));
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
        // A population boom doubles the growth, as Stream of Life does.
        if self.events.on_city(
            crate::world_event::EventKind::PopulationBoom,
            self.units().serial_of(city),
        ) {
            mods.push(Modifier::new(
                Stat::CityGrowthPercent,
                crate::rules::ModOp::Add,
                100,
                Source::Other,
            ));
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

    /// What one square gives a city that works it, in half-food units (the original's
    /// `Square_Food_x2`): the terrain's own food; a river square gives two food whatever it is
    /// laid on; water next to land (the shore) gives half a food, open sea none; a magic node
    /// takes over its square and gives what the node says.
    pub fn square_food(&self, grid: &crate::map::Grid, x: i32, y: i32) -> i32 {
        let Some(def) = grid.get(x, y).and_then(|t| self.content.terrains.get(t)) else {
            return 0;
        };
        if let Some(land) = grid
            .feature(x, y)
            .and_then(|f| self.content.features.get(f))
            .and_then(|f| f.city.land_food)
        {
            return i32::from(land);
        }
        let food = i32::from(def.food);
        if def.class == TerrainClass::Ocean {
            let land = |dx: i32, dy: i32| {
                grid.get(x + dx, y + dy)
                    .and_then(|t| self.content.terrains.get(t))
                    .is_some_and(|d| d.class != TerrainClass::Ocean)
            };
            let mut shore = 0;
            let mut river_in = false;
            for (dx, dy) in [
                (-1, -1),
                (0, -1),
                (1, -1),
                (-1, 0),
                (1, 0),
                (-1, 1),
                (0, 1),
                (1, 1),
            ] {
                if !land(dx, dy) {
                    continue;
                }
                if (dx == 0 || dy == 0) && grid.has_river(x + dx, y + dy) {
                    river_in = true;
                } else {
                    shore += 1;
                }
            }
            // Where a river runs into a lake, or into the sea at a shore with at most three
            // other land squares around it, the water gives two food. A shore hemmed in by
            // four or more takes the plain shore's half food (the original's tile sets).
            if river_in && (grid.is_lake(x, y) || shore <= 3) {
                return food.max(4);
            }
            return food.max(i32::from(shore > 0 || river_in));
        }
        if grid.has_river(x, y) {
            return food.max(4);
        }
        food
    }

    /// What one square adds to the production of a city that works it, in percent (the
    /// original's `Square_Production_Bonus`): the terrain's own, or the node's on a magic node. A
    /// river square adds nothing, being a river and not the land it crosses. Gaia's Blessing
    /// doubles forests and nature nodes.
    pub fn square_production(&self, grid: &crate::map::Grid, x: i32, y: i32, gaia: bool) -> i32 {
        let Some(def) = grid.get(x, y).and_then(|t| self.content.terrains.get(t)) else {
            return 0;
        };
        let node = grid
            .feature(x, y)
            .and_then(|f| self.content.features.get(f))
            .filter(|f| f.city.land_production.is_some());
        let base = match node {
            Some(f) => f.city.land_production.unwrap_or(0),
            None if grid.has_river(x, y) => 0,
            None => def.production_percent,
        };
        let doubled = match node {
            Some(f) => f.id == "nature_node",
            None => def.class == TerrainClass::Forest,
        };
        if gaia && doubled {
            base * 2
        } else {
            base
        }
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
            let food = self.square_food(grid, p.x as i32, p.y as i32) * 2;
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

    /// Magic power from the corrupted squares in the areas of `owner`'s cities, one for each
    /// square (counted once however many of the cities work it): what Great Wasting gives
    /// its wizard. Nothing for anyone not keeping the spell. It joins the power base like a
    /// volcano's, outside the cities' own mana.
    pub fn blight_power(&self, owner: PlayerId) -> i32 {
        let kept = self.wizard(owner).is_some_and(|m| {
            crate::spell::global_enchantment::magic_has_global(&m, self.content, "great_wasting")
        });
        let Some(map) = self.map.filter(|_| kept) else {
            return 0;
        };
        let mut squares: Vec<Position> = Vec::new();
        for (_, city, at) in self.cities() {
            if city.owner != owner {
                continue;
            }
            for (p, _) in self.city_area(at) {
                if map.plane(p.plane).is_corrupted(p.x as i32, p.y as i32) && !squares.contains(&p)
                {
                    squares.push(p);
                }
            }
        }
        squares.len() as i32
    }

    /// Compute the city's yields as if `farmers` citizens farmed.
    pub fn city_economy_with(&self, city: hecs::Entity, farmers: u32) -> Option<CityEconomy> {
        let at = *self.world.get::<&Position>(city).ok()?;
        let owner = self.city_owner(city)?;
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
        let figures = pop.figures;
        let farmers = farmers.min(figures);

        let has = |name: &str| self.city_has_building(city, name);
        let miners = has("miners_guild");
        let dwarf = race.is_some_and(|r| r.double_mining);

        // Worked tiles: food, production bonus, mines.
        let area = self.city_area(at);
        // A wizard who keeps Great Wasting going is proof against the blight: their corrupted
        // squares still yield and add no unrest (and add power, see `blight_power`).
        let blight_proof = self.wizard(owner).is_some_and(|m| {
            crate::spell::global_enchantment::magic_has_global(&m, self.content, "great_wasting")
        });
        let gaia = self.city_has_enchantment(city, "gaias_blessing");
        let mut blighted = 0i32;
        let (mut terrain_food_x2, mut production_percent) = (0i32, 0i32);
        let (mut wild_food, mut mine_gold, mut mine_mana, mut cost_reduction) = (0, 0, 0, 0);
        for &(p, shared) in &area {
            if grid
                .get(p.x as i32, p.y as i32)
                .and_then(|t| self.content.terrains.get(t))
                .is_none()
            {
                continue;
            }
            // Corrupted land gives nothing at all.
            let corrupted = grid.is_corrupted(p.x as i32, p.y as i32);
            blighted += i32::from(corrupted);
            let spoiled = corrupted && !blight_proof;
            let mut food = if spoiled {
                0
            } else {
                self.square_food(grid, p.x as i32, p.y as i32) * 2
            };
            let mut prod = if spoiled {
                0
            } else {
                self.square_production(grid, p.x as i32, p.y as i32, gaia)
            };
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
        let tax_rate = crate::player::component::<crate::player::Tax>(self.world, owner)
            .map_or(1, |t| t.0) as usize;
        let capital = crate::wizard::entity::find(self.world, owner).and_then(|w| w.capital_race());
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
        // Each corrupted square in the city's area stirs one more citizen to unrest.
        let blight_unrest = if blight_proof { 0 } else { blighted };
        let raw_rebels = (figures as i32 * unrest_percent / 100 + blight_unrest).max(0);
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
        if let Ok(p) = self.world.get::<&crate::city::production::Production>(city) {
            if p.item == Some(crate::city::production::ProductionItem::TradeGoods) {
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
        let housing = match self.world.get::<&crate::city::production::Production>(city) {
            Ok(p) if p.item == Some(crate::city::production::ProductionItem::Housing) => {
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

        // A computer wizard's difficulty bonuses come last, on the finished figures, as in the
        // original's `Do_City_Calculations`.
        let bonus = crate::player::component::<crate::player::Bonus>(self.world, owner)
            .map_or_else(crate::player::Bonus::default, |b| *b);
        let of = crate::player::Bonus::of;
        let growth_tens = if growth_tens > 0 {
            of(bonus.growth, growth_tens)
        } else {
            growth_tens
        };
        Some(CityEconomy {
            figures,
            farmers,
            workers,
            rebels,
            max_size,
            food: of(bonus.food, food),
            production: of(bonus.production, production),
            gold: of(bonus.gold, gold),
            gold_upkeep: of(bonus.maintenance, gold_upkeep),
            research: of(bonus.research, research),
            mana: of(bonus.mana, mana),
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
    pub fn lair_at(&self, pos: Position) -> Option<(usize, &'a crate::feature::lair::Lair)> {
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
        crate::player::id_of(self.world, self.city_owner_entity(city)?)
    }

    /// The player entity owning a city.
    pub fn city_owner_entity(&self, city: hecs::Entity) -> Option<hecs::Entity> {
        self.world.get::<&Owner>(city).ok().map(|o| o.0)
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
        Economy::new(&self.world, self.map.as_ref(), &self.content).with_events(&self.events)
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
    use crate::content::{CityBonus, FeatureDef, FeatureKind};
    use crate::game::testutil::content_with_economy;
    use crate::map::Grid;

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
        let owner = world.spawn(crate::player::bundle(
            PlayerId(0),
            crate::player::Player::new(),
        ));
        let city = world.spawn((
            City {
                name: "Town".into(),
            },
            crate::player::Owner(owner),
            at,
            Population {
                figures: 4,
                tens: 0,
            },
            Farmers(0),
            Buildings::default(),
            Enchantments::default(),
        ));
        world.spawn(crate::player::bundle(
            PlayerId(0),
            crate::player::Player::new(),
        ));
        let eco = Economy::new(&world, Some(&map), &content);

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

    #[test]
    fn a_computer_wizards_bonuses_multiply_its_cities_and_nobody_elses() {
        let content = content_with_economy();
        let grass = content.terrains.id_of("grassland").unwrap();
        let map = Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass));
        let at = Position {
            plane: Plane::Arcanus,
            x: 10,
            y: 7,
        };
        let mut world = World::new();
        let owner = world.spawn(crate::player::bundle(
            PlayerId(0),
            crate::player::Player::new(),
        ));
        let city = world.spawn((
            City {
                name: "Town".into(),
            },
            crate::player::Owner(owner),
            at,
            Population {
                figures: 10,
                tens: 0,
            },
            Farmers(8),
            Buildings::default(),
            Enchantments::default(),
        ));
        let player = owner;
        let before = Economy::new(&world, Some(&map), &content)
            .city_economy(city)
            .unwrap();
        *world.get::<&mut crate::player::Bonus>(player).unwrap() = crate::player::Bonus {
            food: 200,
            production: 150,
            gold: 200,
            research: 150,
            mana: 200,
            maintenance: 50,
            growth: 200,
            ..crate::player::Bonus::default()
        };
        let after = Economy::new(&world, Some(&map), &content)
            .city_economy(city)
            .unwrap();
        assert!(before.food > 0 && before.production > 0 && before.gold > 0);
        assert!(before.growth_tens > 0, "a growing city: {before:?}");
        assert_eq!(after.food, before.food * 2);
        assert_eq!(after.production, before.production * 3 / 2);
        assert_eq!(after.gold, before.gold * 2);
        assert_eq!(after.research, before.research * 3 / 2);
        assert_eq!(after.mana, before.mana * 2);
        assert_eq!(after.gold_upkeep, before.gold_upkeep / 2);
        assert_eq!(after.growth_tens, before.growth_tens * 2);
        assert_eq!(
            after.figures, before.figures,
            "the people are not multiplied"
        );
    }

    #[test]
    fn a_squares_food_follows_the_original() {
        let mut content = content_with_economy();
        let grass = content.terrains.id_of("grassland").unwrap();
        let swamp = content.terrains.id_of("swamp").unwrap();
        let ocean = content.terrains.id_of("ocean").unwrap();
        let mut defs = content.features.defs().to_vec();
        defs.push(FeatureDef {
            id: "node_test".into(),
            name: "node".into(),
            kind: FeatureKind::Node,
            city: CityBonus {
                land_food: Some(4),
                ..Default::default()
            },
            spawn: vec![],
        });
        content.features = crate::content::Features::new(defs);
        let node = content.features.id_of("node_test").unwrap();
        let mut grid = Grid::new(24, 14, grass);
        for x in 0..8 {
            for y in 0..4 {
                grid.set(x, y, ocean);
            }
        }
        grid.set(5, 5, swamp);
        grid.set(7, 7, grass);
        grid.set_river(7, 7, true);
        grid.set_feature(9, 9, Some(node));
        let map = Map::new(grid.clone(), grid.clone());
        let world = World::new();
        let eco = Economy::new(&world, Some(&map), &content);
        let g = map.plane(Plane::Arcanus);
        assert_eq!(eco.square_food(g, 12, 12), 3, "grassland: one and a half");
        assert_eq!(eco.square_food(g, 5, 5), 1, "swamp: half a food");
        assert_eq!(eco.square_food(g, 7, 7), 4, "a river: two food");
        assert_eq!(
            eco.square_food(g, 9, 9),
            4,
            "a node square gives the node's food"
        );
        assert_eq!(eco.square_food(g, 2, 2), 0, "open sea");
        assert_eq!(
            eco.square_food(g, 2, 3),
            1,
            "the shore (land one step south)"
        );
        assert_eq!(eco.square_food(g, 7, 3), 1, "the shore, diagonally");
    }

    #[test]
    fn rivers_into_lakes_and_the_sea_give_two_food_and_nodes_set_production() {
        let mut content = content_with_economy();
        let grass = content.terrains.id_of("grassland").unwrap();
        let ocean = content.terrains.id_of("ocean").unwrap();
        let forest = content.terrains.id_of("forest").unwrap();
        let mut defs = content.features.defs().to_vec();
        for (id, production) in [("nature_node", 3), ("chaos_node", 5)] {
            defs.push(FeatureDef {
                id: id.into(),
                name: id.into(),
                kind: FeatureKind::Node,
                city: CityBonus {
                    land_production: Some(production),
                    ..Default::default()
                },
                spawn: vec![],
            });
        }
        content.features = crate::content::Features::new(defs);
        let nature = content.features.id_of("nature_node").unwrap();
        let chaos = content.features.id_of("chaos_node").unwrap();
        let mut grid = Grid::new(24, 14, grass);
        // A lake with a river running into it from the north.
        grid.set(5, 5, ocean);
        grid.set_lake(5, 5, true);
        grid.set_river(5, 4, true);
        // A lake with no river.
        grid.set(9, 5, ocean);
        grid.set_lake(9, 5, true);
        // The sea along the top rows, a river ending at (15, 2), a river whose mouth is hemmed in.
        for x in 10..24 {
            for y in 0..2 {
                grid.set(x, y, ocean);
            }
        }
        grid.set_river(15, 2, true);
        grid.set(12, 7, forest);
        grid.set(13, 7, forest);
        grid.set_river(13, 7, true);
        grid.set_feature(16, 7, Some(nature));
        grid.set_feature(17, 7, Some(chaos));
        grid.set(18, 7, forest);
        let map = Map::new(grid.clone(), grid.clone());
        let world = World::new();
        let eco = Economy::new(&world, Some(&map), &content);
        let g = map.plane(Plane::Arcanus);
        assert_eq!(eco.square_food(g, 5, 5), 4, "a lake fed by a river");
        assert_eq!(eco.square_food(g, 9, 5), 1, "a lake alone is a shore");
        assert_eq!(eco.square_food(g, 15, 1), 4, "the sea where a river ends");
        assert_eq!(eco.square_food(g, 14, 1), 1, "the shore beside it");
        assert_eq!(eco.square_production(g, 12, 7, false), 3, "forest");
        assert_eq!(
            eco.square_production(g, 12, 7, true),
            6,
            "forest, Gaia's Blessing"
        );
        assert_eq!(
            eco.square_production(g, 13, 7, false),
            0,
            "a river is not the forest"
        );
        assert_eq!(eco.square_production(g, 16, 7, false), 3, "nature node");
        assert_eq!(
            eco.square_production(g, 16, 7, true),
            6,
            "nature node, Gaia's Blessing"
        );
        assert_eq!(
            eco.square_production(g, 17, 7, true),
            5,
            "chaos node, not doubled"
        );
        assert_eq!(eco.square_production(g, 18, 7, true), 6, "forest");
    }
}
