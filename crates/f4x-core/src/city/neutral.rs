//! Neutral cities: the independent cities a new world starts with, owned by the neutral player
//! (`player::MONSTERS`). Where they go (`Generate_Neutral_Cities`) is `f4x_newgame::neutral`; this
//! is how one starts: its race's player, its population, which grows with the
//! difficulty, like lair guards (design.md section 8), and it starts with the buildings a
//! population brings and a garrison of the best soldiers its race can recruit. After that a
//! neutral city plays by the ordinary rules: it grows and can be taken in battle. Its decisions
//! (farmers, taxes, what it builds) are an AI's, not the turn's: `f4x_ai::NeutralSeat`, which a
//! host calls before ending a turn. Every neutral city is its own player (the `Neutral` component),
//! with its own treasury: its taxes pay for its buildings and its garrison, and a garrison it
//! cannot pay for is disbanded. The player goes when the city is taken. Monsters, lair guards and
//! the undead raised for them stay with the shared `player::MONSTERS`.
//!
//! Tests: in `f4x_newgame::neutral`, which places the cities of a new world.

use crate::rules::{Ability, BuildingId};
use f4x_def::half_int::HalfInt;
use std::collections::VecDeque;

use crate::city::economy::{Buildings, Farmers};
use crate::city::production::{Production, ProductionItem};
use crate::map::terrain::TerrainClass;
use crate::map::{Plane, Position};

use crate::game::Game;
use crate::rules::{RaceId, UnitKind};

/// What a city's population gives it, in the order the original hands them out: the building
/// comes with `population - 2` at or above the number.
const STARTING_BUILDINGS: &[(u32, BuildingId)] = &[
    (0, BuildingId::new(3)),   // barracks
    (1, BuildingId::new(8)),   // smithy
    (2, BuildingId::new(32)),  // builders_hall
    (3, BuildingId::new(4)),   // armory
    (5, BuildingId::new(29)),  // granary
    (6, BuildingId::new(9)),   // stables
    (8, BuildingId::new(35)),  // city_walls
    (10, BuildingId::new(5)),  // fighters_guild
    (11, BuildingId::new(6)),  // armorers_guild
    (12, BuildingId::new(22)), // shrine
];

impl Game {
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
    pub fn landmasses(&self, plane: Plane) -> Vec<u32> {
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

    /// The first name of the race's list that no city has, or a numbered one past its end.
    fn unused_city_name(&self, race: RaceId) -> String {
        let used: Vec<String> = self
            .economy()
            .cities()
            .into_iter()
            .map(|(_, c, _)| c.name.clone())
            .collect();
        // The race's list in order, the first name nobody has; past its end, a numbered name.
        let def = self.content.rules.races.get(race);
        let listed = self.names.cities.get(&def.num);
        listed
            .into_iter()
            .flatten()
            .find(|n| !used.contains(n))
            .cloned()
            .unwrap_or_else(|| {
                (used.len()..)
                    .map(|i| format!("{}_city_{i:02}", def.id))
                    .find(|n| !used.contains(n))
                    .unwrap_or_default()
            })
    }

    /// One neutral city with its population, buildings and garrison, for the race's player.
    pub fn found_neutral_city(&mut self, pos: Position, race: RaceId) -> hecs::Entity {
        let d = u32::from(self.difficulty());
        let bonus = u32::from(self.settings.world.neutral_city_size_bonus);
        let mut population = 1 + bonus + self.roll(4);
        if d > 2 && self.roll(5) == 1 {
            population = bonus + self.roll(10);
        }
        let name = self.unused_city_name(race);
        let owner = self.race_player(race);
        let city = self.spawn_city(owner, pos, name, Some(race), population);

        let rules = &self.content.rules;
        let level = population.saturating_sub(2);
        let mut built: Vec<crate::rules::BuildingId> = STARTING_BUILDINGS
            .iter()
            .filter(|&&(from, _)| level >= from)
            .map(|&(_, id)| id)
            .filter(|&b| {
                let def = rules.buildings.get(b);
                def.buildable_by(race)
            })
            .collect();
        // A building whose requirements fell away goes too.
        loop {
            let before = built.len();
            let keep: Vec<_> = built
                .iter()
                .copied()
                .filter(|&b| {
                    rules
                        .buildings
                        .get(b)
                        .requires
                        .iter()
                        .all(|r| built.contains(r))
                })
                .collect();
            built = keep;
            if built.len() == before {
                break;
            }
        }
        self.put(city, Buildings(built));

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
            if t.food_upkeep > t.food_surplus || t.net_gold() < HalfInt::ZERO {
                self.despawn_with_spells(soldier);
                break;
            }
        }
        self.balance_taxes(owner);
        let farmers = self.economy().city_min_farmers(city);
        self.put(city, Farmers(farmers));
        let item = self.autobuild_choice(city, owner);
        self.put(
            city,
            Production {
                item: Some(item.unwrap_or(ProductionItem::Housing)),
                progress: HalfInt::ZERO,
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
        for &id in &rules.races.get(race).units {
            let def = rules.units.get(id);
            let plain = def.kind == UnitKind::Normal
                && def.cost > 0
                && def.movement != crate::rules::MovementType::Sailing
                && !def.abilities.contains(&Ability::new(12));
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
                .map_or(0, |v| v.floor())
                > 0
                && !def.abilities.contains(&Ability::new(69));
            if shoots {
                ranged = Some(id);
            } else {
                melee = Some(id);
            }
        }
        (melee, ranged)
    }
}
