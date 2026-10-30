//! Neutral cities: the independent cities a new world starts with, owned by the neutral player
//! (`player::MONSTERS`), as the original's `Generate_Neutral_Cities` makes them.
//!
//! Each plane gets up to 15, on land away from fortresses, cities, nodes, towers and lairs.
//! A city's race is picked from the races of its plane (a high elf city only on forest), with a
//! three in four chance of the default race of its landmass. Its population grows with the
//! difficulty, like lair guards (design.md section 8), and it starts with the buildings a
//! population brings and a garrison of the best soldiers its race can recruit. After that a
//! neutral city plays by the ordinary rules: it grows and can be taken in battle. Its decisions
//! (farmers, taxes, what it builds) are an AI's, not the turn's: `f4x_ai::play_neutrals`, which a
//! host calls before ending a turn. Every neutral city is its own player (the `Neutral` component),
//! with its own treasury: its taxes pay for its buildings and its garrison, and a garrison it
//! cannot pay for is disbanded. The player goes when the city is taken. Monsters, lair guards and
//! the undead raised for them stay with the shared `player::MONSTERS`.

use std::collections::VecDeque;

use crate::city::economy::{Buildings, Farmers};
use crate::city::production::{Production, ProductionItem};
use crate::map::terrain::TerrainClass;
use crate::map::{Plane, Position};
use crate::player::PlayerId;

use crate::game::Game;
use crate::rules::{RaceId, UnitKind};

/// Neutral cities placed on each plane.
pub const NEUTRAL_CITIES_PER_PLANE: u32 = 15;

/// Neutral cities on a plane of `width` by `height`: 15 for every 60 by 40 squares.
pub fn neutral_city_count(width: u32, height: u32) -> u32 {
    crate::map::mapgen::scaled_count(NEUTRAL_CITIES_PER_PLANE, width, height)
}

/// The fewest citizens the land round a neutral city must be able to feed: its start population
/// is up to 5, and a city that starves loses its garrison.
const MIN_SITE_SIZE: u32 = 5;

/// Rows at each pole that get no neutral city: tundra and ice.
const POLAR_ROWS: u8 = 4;

/// The rows a plane of `height` rows is searched in: the equator row, then the one below, the
/// one above, two below and so on, to the polar rows.
fn scan_rows(height: u32) -> Vec<u32> {
    let polar = u32::from(POLAR_ROWS);
    let equator = height / 2;
    let mut rows = vec![equator];
    for d in 1..height {
        for y in [equator + d, equator.saturating_sub(d)] {
            if (polar..height - polar).contains(&y) && !rows.contains(&y) {
                rows.push(y);
            }
        }
    }
    rows
}

/// Squares between a neutral city and a node, tower or lair.
const SITE_CLEARANCE: u32 = 4;

/// The row scan of the custom generator keeps a little more room: squares between two cities,
/// between a city and a fortress, and between a city and a node, tower or lair, so that the 15
/// cities of a plane do not crowd it. The original's random placement keeps the original's.
const STRICT_CITY_CLEARANCE: u32 = 6;
const STRICT_FORTRESS_CLEARANCE: u32 = 6;
const STRICT_SITE_CLEARANCE: u32 = 5;

/// Placement attempts of the original's random placement on a plane of 60 by 40 squares (its
/// 1000); a bigger map gets more, a smaller one the same.
const MAX_TRIES: u32 = 1000;

/// Squares between a neutral city and a wizard's fortress.
const FORTRESS_CLEARANCE: u32 = 5;

/// What a city's population gives it, in the order the original hands them out: the building
/// comes with `population - 2` at or above the number.
const STARTING_BUILDINGS: [(u32, &str); 10] = [
    (0, "barracks"),
    (1, "smithy"),
    (2, "builders_hall"),
    (3, "armory"),
    (5, "granary"),
    (6, "stables"),
    (8, "city_walls"),
    (10, "fighters_guild"),
    (11, "armorers_guild"),
    (12, "shrine"),
];

/// The first number given to the players of neutral cities, well clear of the wizards.
const FIRST_CITY_PLAYER: u8 = 100;

/// The original's `Range`: the longer side plus half the shorter, with no wrap-around. Squares
/// at a range under 4 form a diamond-cornered block, smaller than a square of 7 by 7.
fn range(width: u32, x1: u32, y1: u32, x2: u32, y2: u32) -> u32 {
    // The map wraps east and west: the short way round counts.
    let dx = x1.abs_diff(x2);
    let (dx, dy) = (dx.min(width - dx), y1.abs_diff(y2));
    dx.max(dy) + dx.min(dy) / 2
}

impl Game {
    /// A new player for one neutral city, with the lowest number not yet taken.
    pub(crate) fn new_city_player(&mut self) -> PlayerId {
        let id = (FIRST_CITY_PLAYER..u8::MAX)
            .map(PlayerId)
            .find(|id| !self.has_player(*id))
            .unwrap_or(PlayerId(u8::MAX - 1));
        self.add_neutral_player(id);
        id
    }

    /// Whether a neutral city may stand on `pos`: any city site, clear of the wizards' fortresses
    /// and of nodes, towers and lairs; `strict` asks for more room all round (the row scan).
    fn neutral_site_ok(&self, pos: Position, strict: bool) -> bool {
        if self.can_found_city(pos).is_err() {
            return false;
        }
        let width = self.map.as_ref().map_or(1, |m| m.width());
        let near = |x: u32, y: u32, d: u32| range(width, x, y, pos.x, pos.y) < d;
        let (fortress, site) = if strict {
            (STRICT_FORTRESS_CLEARANCE, STRICT_SITE_CLEARANCE)
        } else {
            (FORTRESS_CLEARANCE, SITE_CLEARANCE)
        };
        if crate::wizard::entity::all(&self.world)
            .iter()
            .filter_map(|w| w.fortress())
            .any(|f| near(f.x, f.y, fortress))
        {
            return false;
        }
        if strict
            && self
                .economy()
                .cities()
                .iter()
                .any(|(_, _, c)| c.plane == pos.plane && near(c.x, c.y, STRICT_CITY_CLEARANCE))
        {
            return false;
        }
        self.clear_of_sites(pos, site)
    }

    /// Whether no node, tower or lair on `pos`'s plane is within `distance` (the original's
    /// `Range`).
    pub(crate) fn clear_of_sites(&self, pos: Position, distance: u32) -> bool {
        let Some(width) = self.map.as_ref().map(|m| m.width()) else {
            return false;
        };
        // Towers stand on both planes at the same place.
        !self
            .economy()
            .site_squares()
            .iter()
            .any(|s| s.plane == pos.plane && range(width, s.x, s.y, pos.x, pos.y) < distance)
    }

    /// The landmass a square is on (0 for sea); two squares are on the same land when the
    /// numbers match. Walks the whole plane, so ask for events, not for every square.
    pub fn landmass_of(&self, at: Position) -> u32 {
        let Some(map) = self.map.as_ref() else {
            return 0;
        };
        let width = map.plane(at.plane).width();
        self.landmasses(at.plane)
            .get((at.y * width + at.x) as usize)
            .copied()
            .unwrap_or(0)
    }

    /// A number for every land square telling which landmass it belongs to (0 for sea).
    pub(crate) fn landmasses(&self, plane: Plane) -> Vec<u32> {
        let Some(map) = self.map.as_ref() else {
            return Vec::new();
        };
        let grid = map.plane(plane);
        let (w, h) = (grid.width() as i32, grid.height() as i32);
        let land = |x: i32, y: i32| {
            grid.get(x, y)
                .and_then(|t| self.content.terrains.get(t))
                .is_some_and(|t| t.class != TerrainClass::Ocean)
        };
        let mut ids = vec![0u32; (w * h) as usize];
        let mut next = 0;
        for y in 0..h {
            for x in 0..w {
                if ids[(y * w + x) as usize] != 0 || !land(x, y) {
                    continue;
                }
                next += 1;
                ids[(y * w + x) as usize] = next;
                let mut queue = VecDeque::from([(x, y)]);
                while let Some((cx, cy)) = queue.pop_front() {
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let (nx, ny) = ((cx + dx).rem_euclid(w), cy + dy);
                            if !(0..h).contains(&ny) || !land(nx, ny) {
                                continue;
                            }
                            let slot = &mut ids[(ny * w + nx) as usize];
                            if *slot == 0 {
                                *slot = next;
                                queue.push_back((nx, ny));
                            }
                        }
                    }
                }
            }
        }
        ids
    }

    /// The races neutral cities of `plane` may have, high men counted twice (the original gives
    /// them two of the nine slots; the second is high elves where the land is forest).
    fn neutral_races(&self, plane: Plane) -> Vec<RaceId> {
        let rules = &self.content.rules;
        let mut out = Vec::new();
        for r in rules.races.defs() {
            if r.city_names.is_empty() || r.myrror != (plane == Plane::Myrror) || r.id == "high_elf"
            {
                continue;
            }
            let Some(id) = rules.races.id_of(&r.id) else {
                continue;
            };
            out.push(id);
            if r.id == "high_men" {
                out.push(id);
            }
        }
        out
    }

    fn unused_city_name(&self, race: RaceId) -> String {
        let def = self.content.rules.races.get(race);
        let used: Vec<String> = self
            .economy()
            .cities()
            .into_iter()
            .map(|(_, c, _)| c.name.clone())
            .collect();
        def.city_names
            .iter()
            .find(|n| !used.contains(n))
            .cloned()
            .unwrap_or_else(|| format!("{} {}", def.name, used.len() + 1))
    }

    /// Place a plane's neutral cities. Returns how many were made.
    pub fn generate_neutral_cities(&mut self, plane: Plane) -> u32 {
        let Some(map) = self.map.as_ref() else {
            return 0;
        };
        let (w, h) = (map.width(), map.height());
        if w < 8 || h < 8 {
            return 0;
        }
        let races = self.neutral_races(plane);
        if races.is_empty() {
            return 0;
        }
        let masses = self.landmasses(plane);
        let mut default_race: std::collections::HashMap<u32, RaceId> = Default::default();
        let mut made = 0;
        let target = neutral_city_count(w, h);
        // The original rolls squares at random, 1000 tries for a plane of 60 by 40, 3 to 56
        // across and 3 to 36 down; the custom generator's land is searched row by row from the
        // equator outward, a row south and then a row north, each from west to east: a city goes
        // on the first square that will do, or a better one within two squares of it. The polar
        // rows are left out.
        let original = self.settings.basic.original_map_generator;
        let max_tries = crate::map::mapgen::scaled_count(MAX_TRIES, w, h).max(MAX_TRIES);
        let rows = scan_rows(h);
        let (mut row, mut x, mut tries) = (0, 2, 0);
        while made < target {
            let pos = loop {
                if original {
                    tries += 1;
                    if tries >= max_tries {
                        return made;
                    }
                    let pos = Position {
                        plane,
                        x: 2 + self.roll(w - 6),
                        y: 2 + self.roll(h - 6),
                    };
                    if self.neutral_site_ok(pos, false) {
                        break pos;
                    }
                    continue;
                }
                let Some(&y) = rows.get(row) else {
                    return made;
                };
                if x >= w - 2 {
                    (row, x) = (row + 1, 2);
                    continue;
                }
                let here = Position { plane, x, y };
                x += 1;
                if self.neutral_site_ok(here, true)
                    && self.economy().site_max_size(here) >= MIN_SITE_SIZE
                {
                    let best = self.best_site_near(here, h);
                    x = x.max(best.x + 1);
                    break best;
                }
            };
            let forest = self.map.as_ref().is_some_and(|m| {
                m.plane(plane)
                    .get(pos.x as i32, pos.y as i32)
                    .and_then(|t| self.content.terrains.get(t))
                    .is_some_and(|t| t.class == TerrainClass::Forest)
            });
            let slot = self.roll(races.len() as u32) as usize - 1;
            let mut race = races[slot];
            // The second high men slot is high elves on forest.
            if forest && races[..slot].contains(&race) {
                if let Some(elf) = self.content.rules.races.id_of("high_elf") {
                    race = elf;
                }
            }
            if self.roll(4) > 1 {
                let mass = masses
                    .get((pos.y * w + pos.x) as usize)
                    .copied()
                    .unwrap_or(0);
                let pick = self.roll(races.len() as u32) as usize - 1;
                race = *default_race.entry(mass).or_insert(races[pick]);
            }
            self.make_neutral_city(pos, race);
            made += 1;
        }
        made
    }

    /// The site within two squares of `at` (itself included) where the most citizens could live;
    /// `at` unless another is better.
    fn best_site_near(&self, at: Position, height: u32) -> Position {
        let mut best = (self.economy().site_max_size(at), at);
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let (x, y) = (at.x as i32 + dx, at.y as i32 + dy);
                if x < 2 || y < i32::from(POLAR_ROWS) || y >= height as i32 - i32::from(POLAR_ROWS)
                {
                    continue;
                }
                let pos = Position {
                    x: x as u32,
                    y: y as u32,
                    ..at
                };
                if pos != at && self.neutral_site_ok(pos, true) {
                    let size = self.economy().site_max_size(pos);
                    if size > best.0 {
                        best = (size, pos);
                    }
                }
            }
        }
        best.1
    }

    /// One neutral city with its population, buildings and garrison.
    fn make_neutral_city(&mut self, pos: Position, race: RaceId) -> hecs::Entity {
        let d = u32::from(self.settings.basic.difficulty);
        let bonus = u32::from(self.settings.world.neutral_city_size_bonus);
        let mut population = 1 + bonus + self.roll(4);
        if d > 2 && self.roll(5) == 1 {
            population = bonus + self.roll(10);
        }
        let name = self.unused_city_name(race);
        let owner = self.new_city_player();
        let city = self.spawn_city(owner, pos, name, Some(race), population);

        let rules = &self.content.rules;
        let level = population.saturating_sub(2);
        let race_def = rules.races.get(race);
        let mut built: Vec<crate::rules::BuildingId> = STARTING_BUILDINGS
            .iter()
            .filter(|&&(from, _)| level >= from)
            .filter_map(|&(_, id)| rules.buildings.id_of(id))
            .filter(|&b| {
                let def = rules.buildings.get(b);
                def.races.is_empty() || def.races.contains(&race_def.id)
            })
            .collect();
        // A building whose requirements fell away goes too.
        loop {
            let before = built.len();
            let keep: Vec<_> = built
                .iter()
                .copied()
                .filter(|&b| {
                    rules.buildings.get(b).requires.iter().all(|r| {
                        rules
                            .buildings
                            .id_of(r)
                            .is_some_and(|id| built.contains(&id))
                    })
                })
                .collect();
            built = keep;
            if built.len() == before {
                break;
            }
        }
        let _ = self.world.insert_one(city, Buildings(built));

        // Taxes first: the rate decides how many citizens rebel and so how much food there is.
        self.balance_taxes(owner);
        let (melee, ranged) = self.best_garrison_units(city);
        let count = (population / 4).min(crate::unit::MAX_STACK as u32);
        // A race without a ranged soldier doubles its melee ones (and the original's dark elves,
        // with no melee ones, get half a garrison).
        let wanted = (0..count)
            .filter_map(|_| melee)
            .chain((0..count).filter_map(|_| ranged.or(melee)));
        // The city pays its garrison like anyone, in food and in gold (it has no free upkeep), so
        // it keeps only as many soldiers as its land and taxes support.
        for u in wanted {
            let soldier = self.spawn_unit(owner, pos, u);
            self.balance_farmers(owner);
            self.balance_taxes(owner);
            let t = self.budget(owner);
            if t.food_upkeep > t.food_surplus || t.net_gold() < 0 {
                self.despawn_with_spells(soldier);
                break;
            }
        }
        self.balance_taxes(owner);
        let farmers = self.economy().city_min_farmers(city);
        let _ = self.world.insert_one(city, Farmers(farmers));
        let item = self.autobuild_choice(city, owner);
        let _ = self.world.insert_one(
            city,
            Production {
                item: Some(item.unwrap_or(ProductionItem::Housing)),
                progress: 0,
            },
        );
        city
    }

    /// The last (best) melee soldier and the last ranged soldier of the city's race that it has
    /// the buildings for.
    fn best_garrison_units(
        &self,
        city: hecs::Entity,
    ) -> (Option<crate::rules::UnitId>, Option<crate::rules::UnitId>) {
        let rules = &self.content.rules;
        let Ok(race) = self
            .world
            .get::<&crate::city::economy::CityRace>(city)
            .map(|r| r.0)
        else {
            return (None, None);
        };
        let (mut melee, mut ranged) = (None, None);
        for name in &rules.races.get(race).units {
            let Some(id) = rules.units.id_of(name) else {
                continue;
            };
            let def = rules.units.get(id);
            let plain = def.kind == UnitKind::Normal
                && def.cost > 0
                && def.movement != crate::rules::MovementType::Sailing
                && !def.abilities.iter().any(|a| a == "create_outpost");
            if !plain
                || self
                    .economy()
                    .can_build(city, ProductionItem::Unit(id))
                    .is_err()
            {
                continue;
            }
            let shoots = def
                .stats
                .get(&crate::rules::Stat::Ranged)
                .copied()
                .unwrap_or(0)
                > 0
                && !def.abilities.iter().any(|a| a == "thrown");
            if shoots {
                ranged = Some(id);
            } else {
                melee = Some(id);
            }
        }
        (melee, ranged)
    }
}
