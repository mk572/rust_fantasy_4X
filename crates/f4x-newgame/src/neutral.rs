//! Neutral cities of a new world: where the independent cities go, as the original's
//! `Generate_Neutral_Cities` places them, and which race each has.
//!
//! Each plane gets up to 15, on land away from fortresses, cities, nodes, towers and lairs, in
//! four steps: (1) the squares are chosen, with the founded cities left for the last; (2) each
//! square rolls a race from the races of its plane; (3) the races go to the squares: a race with
//! a favoured terrain (`home_terrain` in `races.ron`: High Elves forest, Dwarves mountains, ...)
//! takes the square with the most of that terrain round it, the rest take what is left, in the
//! order they were rolled; (4) the cities are founded. How one city starts (its player,
//! population, buildings and garrison) is `Game::found_neutral_city` in core.
//!
//! Tests: below.

use f4x_core::game::Game;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Plane, Position};
use f4x_core::rules::RaceId;

pub use f4x_def::settings::NEUTRAL_CITIES_PER_PLANE;

/// Neutral cities on a plane of `width` by `height`: 15 for every 60 by 40 squares.
pub fn neutral_city_count(width: u32, height: u32) -> u32 {
    crate::mapgen::scaled_count(NEUTRAL_CITIES_PER_PLANE, width, height)
}

/// The original's `Range`: the longer side plus half the shorter, with no wrap-around. Squares
/// at a range under 4 form a diamond-cornered block, smaller than a square of 7 by 7.
pub fn range(width: u32, x1: u32, y1: u32, x2: u32, y2: u32) -> u32 {
    // The map wraps east and west: the short way round counts.
    let dx = x1.abs_diff(x2);
    let (dx, dy) = (dx.min(width - dx), y1.abs_diff(y2));
    dx.max(dy) + dx.min(dy) / 2
}

/// Whether no node, tower or lair on `pos`'s plane is within `distance` (the original's
/// `Range`).
pub fn clear_of_sites(game: &Game, pos: Position, distance: u32) -> bool {
    let Some(width) = game.map().map(|m| m.width()) else {
        return false;
    };
    // Towers stand on both planes at the same place.
    !game
        .economy()
        .site_squares()
        .iter()
        .any(|s| s.plane == pos.plane && range(width, s.x, s.y, pos.x, pos.y) < distance)
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

/// Whether a neutral city may stand on `pos`: any city site, clear of the wizards' fortresses
/// and of nodes, towers and lairs; `strict` asks for more room all round (the row scan). The
/// cities of the plane are not founded yet, so `chosen` holds the squares picked so far: the
/// same spacing applies to them as to cities.
fn neutral_site_ok(game: &Game, pos: Position, strict: bool, chosen: &[Position]) -> bool {
    if game.can_found_city(pos).is_err() {
        return false;
    }
    let width = game.map().map_or(1, |m| m.width());
    let min_distance = game.content().rules.config.min_city_distance;
    if game.map().is_some_and(|m| {
        chosen.iter().any(|c| {
            c.plane == pos.plane
                && m.plane(pos.plane).chebyshev((c.x, c.y), (pos.x, pos.y)) < min_distance
        })
    }) {
        return false;
    }
    let near = |x: u32, y: u32, d: u32| range(width, x, y, pos.x, pos.y) < d;
    let (fortress, site) = if strict {
        (STRICT_FORTRESS_CLEARANCE, STRICT_SITE_CLEARANCE)
    } else {
        (FORTRESS_CLEARANCE, SITE_CLEARANCE)
    };
    if game
        .player_ids()
        .into_iter()
        .filter_map(|p| game.wizard(p)?.fortress())
        .any(|f| near(f.x, f.y, fortress))
    {
        return false;
    }
    if strict
        && (game
            .economy()
            .cities()
            .iter()
            .any(|(_, _, c)| c.plane == pos.plane && near(c.x, c.y, STRICT_CITY_CLEARANCE))
            || chosen
                .iter()
                .any(|c| c.plane == pos.plane && near(c.x, c.y, STRICT_CITY_CLEARANCE)))
    {
        return false;
    }
    clear_of_sites(game, pos, site)
}

/// The races neutral cities of `plane` may have: the plane's races, one slot each (the
/// original gave High Men two slots and made the second High Elves where the land was forest;
/// now a race with a favoured terrain is put where its terrain is, see [`assign_races`]).
fn neutral_races(game: &Game, plane: Plane) -> Vec<RaceId> {
    let rules = &game.content().rules;
    rules
        .races
        .defs()
        .iter()
        .filter(|r| r.myrror == (plane == Plane::Myrror))
        .filter_map(|r| rules.races.id_numbered(r.num))
        .collect()
}

/// How many of the squares a city on `at` works are of `class` (the 21 squares of its area:
/// five by five without the corners).
fn terrain_round(game: &Game, at: Position, class: TerrainClass) -> u32 {
    let Some(grid) = game.map().map(|m| m.plane(at.plane)) else {
        return 0;
    };
    let mut count = 0;
    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            let y = at.y as i32 + dy;
            if (dx.abs() == 2 && dy.abs() == 2) || y < 0 || y >= grid.height() as i32 {
                continue;
            }
            let x = grid.wrap_x(at.x as i32 + dx) as i32;
            if grid
                .get(x, y)
                .and_then(|t| game.content().terrains.get(t))
                .is_some_and(|t| t.class == class)
            {
                count += 1;
            }
        }
    }
    count
}

/// With the setting `mix_neutral_races`: every race that has a weight on `plane`, with it.
/// `None` when the setting is off, or no race has a weight (content without them): the plane's
/// own races are then rolled, each as likely as the next.
fn weighted_races(game: &Game, plane: Plane) -> Option<Vec<(RaceId, u32)>> {
    if !game.settings().rules.mix_neutral_races {
        return None;
    }
    let rules = &game.content().rules;
    let list: Vec<(RaceId, u32)> = rules
        .races
        .defs()
        .iter()
        .filter_map(|r| {
            let weight = match plane {
                Plane::Arcanus => r.neutral_weight.arcanus,
                Plane::Myrror => r.neutral_weight.myrror,
            };
            Some((rules.races.id_numbered(r.num)?, weight)).filter(|&(_, w)| w > 0)
        })
        .collect();
    (!list.is_empty()).then_some(list)
}

/// The race a roll of 1 to the sum of the weights lands on.
fn pick_weighted<T: Copy>(weights: &[(T, u32)], roll: u32) -> T {
    let mut left = roll;
    for &(item, weight) in weights {
        if left <= weight {
            return item;
        }
        left -= weight;
    }
    weights
        .last()
        .map(|&(item, _)| item)
        .expect("a weight to roll on")
}

/// Step 3: which of the rolled races goes to which site. A race with a favoured terrain, in the
/// order the races were rolled, takes the free site with the most of it (the first of equals);
/// a race whose terrain is nowhere left, or that has none, takes what is left, again in the
/// order rolled. `score(site, class)` is how much of `class` the site has. Returns the race of
/// each site.
fn assign_races<T: Copy>(
    rolled: &[(T, Vec<TerrainClass>)],
    sites: usize,
    score: impl Fn(usize, TerrainClass) -> u32,
) -> Vec<T> {
    let mut at: Vec<Option<T>> = vec![None; sites];
    let mut placed = vec![false; rolled.len()];
    for (i, (race, terrains)) in rolled.iter().enumerate() {
        if terrains.is_empty() {
            continue;
        }
        let best = (0..sites)
            .filter(|&s| at[s].is_none())
            .map(|s| (terrains.iter().map(|&c| score(s, c)).sum::<u32>(), s))
            .filter(|&(points, _)| points > 0)
            .max_by_key(|&(points, s)| (points, std::cmp::Reverse(s)));
        if let Some((_, site)) = best {
            at[site] = Some(*race);
            placed[i] = true;
        }
    }
    let mut rest = rolled
        .iter()
        .enumerate()
        .filter(|(i, _)| !placed[*i])
        .map(|(_, (race, _))| *race);
    at.into_iter()
        .map(|slot| slot.or_else(|| rest.next()))
        .map(|race| race.expect("a race for every site: as many were rolled as there are sites"))
        .collect()
}

/// Step 1: the squares of the plane's neutral cities, `target` of them at most (fewer when the
/// land runs out).
fn choose_sites(game: &mut Game, plane: Plane, target: u32) -> Vec<Position> {
    let Some(map) = game.map() else {
        return Vec::new();
    };
    let (w, h) = (map.width(), map.height());
    let mut chosen: Vec<Position> = Vec::new();
    // The original rolls squares at random, 1000 tries for a plane of 60 by 40, 3 to 56
    // across and 3 to 36 down; the custom generator's land is searched row by row from the
    // equator outward, a row south and then a row north, each from west to east: a city goes
    // on the first square that will do, or a better one within two squares of it. The polar
    // rows are left out.
    let original = game.settings().basic.original_map_generator;
    let max_tries = crate::mapgen::scaled_count(MAX_TRIES, w, h).max(MAX_TRIES);
    let rows = scan_rows(h);
    let (mut row, mut x, mut tries) = (0, 2, 0);
    while (chosen.len() as u32) < target {
        let pos = loop {
            if original {
                tries += 1;
                if tries >= max_tries {
                    return chosen;
                }
                let pos = Position {
                    plane,
                    x: 2 + game.roll(w - 6),
                    y: 2 + game.roll(h - 6),
                };
                if neutral_site_ok(game, pos, false, &chosen) {
                    break pos;
                }
                continue;
            }
            let Some(&y) = rows.get(row) else {
                return chosen;
            };
            if x >= w - 2 {
                (row, x) = (row + 1, 2);
                continue;
            }
            let here = Position { plane, x, y };
            x += 1;
            if neutral_site_ok(game, here, true, &chosen)
                && game.economy().site_max_size(here) >= MIN_SITE_SIZE
            {
                let best = best_site_near(game, here, h, &chosen);
                x = x.max(best.x + 1);
                break best;
            }
        };
        chosen.push(pos);
    }
    chosen
}

/// Place a plane's neutral cities. Returns how many were made.
pub fn generate_neutral_cities(game: &mut Game, plane: Plane) -> u32 {
    let Some(map) = game.map() else {
        return 0;
    };
    let (w, h) = (map.width(), map.height());
    if w < 8 || h < 8 {
        return 0;
    }
    let races = neutral_races(game, plane);
    if races.is_empty() {
        return 0;
    }
    let target = if game.settings().basic.neutral_cities.is_some() {
        game.settings().basic.neutral_city_count()
    } else {
        neutral_city_count(w, h)
    };
    // Step 1: the squares. Step 2: a race rolled for each. Step 3: the races to the squares by
    // their favoured terrain. Step 4: the cities.
    let sites = choose_sites(game, plane, target);
    let weights = weighted_races(game, plane);
    let rolled: Vec<(RaceId, Vec<TerrainClass>)> = sites
        .iter()
        .map(|_| {
            let race = match &weights {
                Some(w) => pick_weighted(w, game.roll(w.iter().map(|&(_, n)| n).sum())),
                None => races[game.roll(races.len() as u32) as usize - 1],
            };
            let home = game.content().rules.races.get(race).home_terrain.clone();
            (race, home)
        })
        .collect();
    let assigned = assign_races(&rolled, sites.len(), |site, class| {
        terrain_round(game, sites[site], class)
    });
    for (&pos, &race) in sites.iter().zip(&assigned) {
        game.found_neutral_city(pos, race);
    }
    sites.len() as u32
}

/// The site within two squares of `at` (itself included) where the most citizens could live;
/// `at` unless another is better.
fn best_site_near(game: &Game, at: Position, height: u32, chosen: &[Position]) -> Position {
    let mut best = (game.economy().site_max_size(at), at);
    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            let (x, y) = (at.x as i32 + dx, at.y as i32 + dy);
            if x < 2 || y < i32::from(POLAR_ROWS) || y >= height as i32 - i32::from(POLAR_ROWS) {
                continue;
            }
            let pos = Position {
                x: x as u32,
                y: y as u32,
                ..at
            };
            if pos != at && neutral_site_ok(game, pos, true, chosen) {
                let size = game.economy().site_max_size(pos);
                if size > best.0 {
                    best = (size, pos);
                }
            }
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::{new_game, NewGame};
    use f4x_def::testdir::shipped;
    use f4x_def::testdir::TestResult;

    fn world(seed: u64) -> Result<Game, String> {
        new_game(
            shipped(),
            &NewGame {
                seed,
                all_ai: true,
                ..Default::default()
            },
        )
    }

    #[test]
    fn a_plane_of_sixty_by_forty_gets_fifteen_cities_and_a_bigger_one_more() {
        assert_eq!(neutral_city_count(60, 40), NEUTRAL_CITIES_PER_PLANE);
        assert!(neutral_city_count(120, 80) > neutral_city_count(60, 40));
        assert!(neutral_city_count(30, 20) < neutral_city_count(60, 40));
    }

    #[test]
    fn the_scan_starts_at_the_equator_and_leaves_out_the_poles() {
        let rows = scan_rows(40);
        assert_eq!(rows[..3], [20, 21, 19], "the equator, one below, one above");
        let polar = u32::from(POLAR_ROWS);
        assert!(rows.iter().all(|y| (polar..40 - polar).contains(y)));
        let mut sorted = rows.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), rows.len(), "no row twice");
        assert_eq!(rows.len() as u32, 40 - 2 * polar, "every other row once");
    }

    #[test]
    fn the_range_between_squares_is_the_short_way_round_a_wrapping_map() {
        assert_eq!(range(60, 5, 5, 5, 5), 0);
        assert_eq!(range(60, 0, 0, 3, 0), 3);
        // The seam: 2 and 58 are four apart, not fifty-six.
        assert_eq!(range(60, 2, 7, 58, 7), 4);
        // Diagonals count the longer side and half the shorter one.
        assert_eq!(range(60, 0, 0, 4, 2), 5);
    }

    #[test]
    fn a_new_world_has_independent_cities_one_player_for_each_race_and_a_garrison() -> TestResult {
        let g = world(7)?;
        let neutral: Vec<_> = g
            .economy()
            .cities()
            .into_iter()
            .filter(|(_, info, _)| g.is_neutral_player(info.owner))
            .collect();
        let (w, h) = g.map().map(|m| (m.width(), m.height())).ok_or("no map")?;
        let most = 2 * neutral_city_count(w, h) as usize;
        assert!(
            !neutral.is_empty(),
            "a world starts with independent cities"
        );
        assert!(neutral.len() <= most, "{} of at most {most}", neutral.len());
        let mut owners: Vec<_> = neutral.iter().map(|(_, info, _)| info.owner).collect();
        owners.sort();
        owners.dedup();
        let mut races: Vec<_> = neutral
            .iter()
            .filter_map(|(e, ..)| g.city_race(*e))
            .map(f4x_core::player::race_player::race_player_id)
            .collect();
        races.sort();
        races.dedup();
        assert_eq!(owners, races, "the cities of a race are one player");
        assert!(
            !owners.contains(&f4x_core::player::MONSTERS),
            "not the monsters' player"
        );
        let mut squares: Vec<_> = neutral.iter().map(|(_, _, p)| *p).collect();
        squares.sort_by_key(|p| (p.plane as u8, p.x, p.y));
        squares.dedup();
        assert_eq!(squares.len(), neutral.len(), "one city to a square");
        for (_, info, pos) in &neutral {
            let guards = g.units().units_at(*pos);
            assert!(
                guards
                    .iter()
                    .all(|&u| g.units().unit_owner(u) == Some(info.owner)),
                "{} has only its own soldiers",
                info.name
            );
        }
        assert!(
            neutral
                .iter()
                .any(|(_, _, pos)| !g.units().units_at(*pos).is_empty()),
            "some city has a garrison"
        );
        Ok(())
    }

    #[test]
    fn the_same_seed_makes_the_same_independent_cities() -> TestResult {
        let places = |seed| -> Result<Vec<(String, Position)>, String> {
            Ok(world(seed)?
                .economy()
                .cities()
                .into_iter()
                .map(|(_, info, p)| (info.name, p))
                .collect())
        };
        assert_eq!(places(11)?, places(11)?);
        assert_ne!(places(11)?, places(12)?);
        Ok(())
    }

    /// The roads a new world starts with (`roads_at_start`, `f4x_core::map::start_road`): every
    /// city stands on a road; two neutral cities on one landmass within ten squares are joined
    /// when the straight line between them is dry; Myrror's are enchanted; and with the setting
    /// off there is no road at all.
    #[test]
    fn a_new_world_has_roads_between_near_neutral_cities_when_the_setting_is_on() -> TestResult {
        use f4x_core::map::Plane;
        let make = |seed, on: bool| {
            let mut settings = f4x_def::settings::GameSettings::default();
            settings.rules.roads_at_start = on;
            new_game(
                shipped(),
                &NewGame {
                    seed,
                    all_ai: true,
                    settings,
                    ..Default::default()
                },
            )
        };
        let off = make(7, false)?;
        for plane in Plane::all() {
            let grid = off.map().ok_or("no map")?.plane(plane);
            assert!(
                grid.roads().iter().all(|&r| !r),
                "no road with the setting off"
            );
        }
        let g = make(7, true)?;
        let map = g.map().ok_or("no map")?;
        let cities = g.economy().cities();
        for (_, _, p) in &cities {
            assert!(map.plane(p.plane).has_road(p.x as i32, p.y as i32), "{p:?}");
        }
        let myrror = map.plane(Plane::Myrror);
        let arcanus = map.plane(Plane::Arcanus);
        let mut roads = 0;
        for plane in Plane::all() {
            let grid = map.plane(plane);
            roads += grid.roads().iter().filter(|&&r| r).count();
            let on_city = |x: i32, y: i32| {
                cities
                    .iter()
                    .any(|(_, _, p)| p.plane == plane && (p.x as i32, p.y as i32) == (x, y))
            };
            for y in 0..grid.height() as i32 {
                for x in 0..grid.width() as i32 {
                    if plane == Plane::Arcanus {
                        assert!(!arcanus.has_enchanted_road(x, y), "Arcanus roads are plain");
                    }
                    if plane == Plane::Myrror && myrror.has_road(x, y) && !on_city(x, y) {
                        assert!(myrror.has_enchanted_road(x, y), "({x},{y}) on Myrror");
                    }
                }
            }
        }
        assert!(
            roads > cities.len(),
            "roads beyond the squares under the cities"
        );
        // Every pair of near neutral cities on one landmass with a dry straight line is joined;
        // over a few worlds there are such pairs.
        let mut joined = 0;
        for seed in 1..=6 {
            let g = make(seed, true)?;
            let map = g.map().ok_or("no map")?;
            let cities = g.economy().cities();
            for plane in Plane::all() {
                let land = g.landmasses(plane);
                let w = map.width();
                let near: Vec<_> = cities
                    .iter()
                    .filter(|(_, i, p)| p.plane == plane && g.is_neutral_player(i.owner))
                    .map(|(_, _, p)| *p)
                    .collect();
                for (i, a) in near.iter().enumerate() {
                    for b in &near[i + 1..] {
                        let mass = |p: &Position| land[(p.y * w + p.x) as usize];
                        if mass(a) != mass(b) || range(w, a.x, a.y, b.x, b.y) > 10 {
                            continue;
                        }
                        let line = f4x_core::map::start_road::path_wrap(
                            a.x as i32, a.y as i32, b.x as i32, b.y as i32, w as i32,
                        );
                        let steps = &line[..line.len() - 1];
                        let dry = steps.iter().all(|&(x, y)| {
                            g.economy().terrain_class_at(Position {
                                plane,
                                x: x as u32,
                                y: y as u32,
                            }) != Some(f4x_core::map::terrain::TerrainClass::Ocean)
                        });
                        if dry {
                            joined += 1;
                            assert!(steps.iter().all(|&(x, y)| map.plane(plane).has_road(x, y)));
                        }
                    }
                }
            }
        }
        assert!(
            joined > 0,
            "some neutral cities are near enough to be joined"
        );
        Ok(())
    }

    #[test]
    fn no_independent_city_starts_with_more_people_than_its_land_holds() -> TestResult {
        // The hardest difficulty gives the biggest cities (a tenth of them up to ten people).
        for seed in 1..8u64 {
            let mut spec = NewGame {
                seed,
                all_ai: true,
                ..Default::default()
            };
            spec.settings.basic.difficulty = 5;
            let g = new_game(shipped(), &spec)?;
            for (e, info, pos) in g.economy().cities() {
                if g.is_neutral_player(info.owner) {
                    let people = g.city_population(e).figures;
                    let room = g.economy().site_max_size(pos);
                    assert!(
                        people <= room,
                        "seed {seed}: {} has {people} of {room}",
                        info.name
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn a_race_goes_to_the_site_with_most_of_its_terrain_and_the_rest_fill_up_in_order() {
        use TerrainClass::{Forest, Hills};
        // Sites 0 to 3 have this much forest; site 2 has the most.
        let forest = [1, 3, 9, 3];
        let score =
            |site: usize, class: TerrainClass| if class == Forest { forest[site] } else { 0 };
        let rolled = [
            ("orc", vec![]),
            ("elf", vec![Forest]),
            ("dwarf", vec![Hills]),
            ("fey", vec![Forest]),
        ];
        let at = assign_races(&rolled, 4, score);
        assert_eq!(at[2], "elf", "the first race to want forest takes the most");
        assert_eq!(
            at[1], "fey",
            "the next takes the best of what is left, the first of equals"
        );
        // No hills anywhere: the dwarf takes what is left, after the orc, in the order rolled.
        assert_eq!(at, ["orc", "fey", "elf", "dwarf"]);
    }

    #[test]
    fn a_race_with_a_terrain_of_its_own_is_found_among_it_more_than_chance_would_have_it(
    ) -> TestResult {
        // For every neutral city: whether its own race's terrain is near, and the share of the
        // plane's races whose terrain is near (what a random race would have had).
        let (mut among, mut chance) = (0.0, 0.0);
        let mut total = 0.0;
        for seed in 1..7u64 {
            let g = world(seed)?;
            for (e, info, pos) in g.economy().cities() {
                let Some(race) = g.city_race(e).filter(|_| g.is_neutral_player(info.owner)) else {
                    continue;
                };
                let near = |r| {
                    let home = &g.content().rules.races.get(r).home_terrain;
                    home.iter().any(|&c| terrain_round(&g, pos, c) > 0)
                };
                let races = neutral_races(&g, pos.plane);
                total += 1.0;
                among += f64::from(u8::from(near(race)));
                chance += races.iter().filter(|&&r| near(r)).count() as f64 / races.len() as f64;
            }
        }
        assert!(
            among > chance * 1.25,
            "{among} of {total} have their terrain near; chance gives {chance:.1}"
        );
        Ok(())
    }

    #[test]
    fn a_roll_lands_on_the_race_whose_share_of_the_weights_it_is_in() {
        let weights = [("a", 2), ("b", 3), ("c", 1)];
        let picks: Vec<_> = (1..=6).map(|r| pick_weighted(&weights, r)).collect();
        assert_eq!(picks, ["a", "a", "b", "b", "b", "c"]);
    }

    #[test]
    fn every_race_has_ten_in_all_and_each_plane_seventy() -> TestResult {
        let g = world(1)?;
        let races = g.content().rules.races.defs();
        for r in races {
            assert_eq!(
                r.neutral_weight.arcanus + r.neutral_weight.myrror,
                10,
                "{}",
                r.id
            );
        }
        let plane = |f: fn(&f4x_def::rules::PlaneWeight) -> u32| {
            races.iter().map(|r| f(&r.neutral_weight)).sum::<u32>()
        };
        assert_eq!(plane(|w| w.arcanus), 70);
        assert_eq!(plane(|w| w.myrror), 70);
        Ok(())
    }

    #[test]
    fn races_of_both_planes_appear_on_each_only_with_the_mix_setting() -> TestResult {
        let foreign = |mix: bool| -> Result<u32, String> {
            let mut found = 0;
            for seed in 1..6u64 {
                let mut spec = NewGame {
                    seed,
                    all_ai: true,
                    ..Default::default()
                };
                spec.settings.rules.mix_neutral_races = mix;
                let g = new_game(shipped(), &spec)?;
                for (e, info, pos) in g.economy().cities() {
                    let Some(race) = g.city_race(e).filter(|_| g.is_neutral_player(info.owner))
                    else {
                        continue;
                    };
                    let native =
                        g.content().rules.races.get(race).myrror == (pos.plane == Plane::Myrror);
                    found += u32::from(!native);
                }
            }
            Ok(found)
        };
        assert_eq!(foreign(false)?, 0, "each plane keeps to its own races");
        assert!(foreign(true)? > 0, "with the setting the races mix");
        Ok(())
    }
}
