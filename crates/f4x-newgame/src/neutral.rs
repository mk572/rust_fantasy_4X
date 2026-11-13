//! Neutral cities of a new world: where the independent cities go, as the original's
//! `Generate_Neutral_Cities` places them.
//!
//! Each plane gets up to 15, on land away from fortresses, cities, nodes, towers and lairs.
//! A city's race is picked from the races of its plane (a high elf city only on forest), with a
//! three in four chance of the default race of its landmass. How one city starts (its player,
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
/// and of nodes, towers and lairs; `strict` asks for more room all round (the row scan).
fn neutral_site_ok(game: &Game, pos: Position, strict: bool) -> bool {
    if game.can_found_city(pos).is_err() {
        return false;
    }
    let width = game.map().map_or(1, |m| m.width());
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
        && game
            .economy()
            .cities()
            .iter()
            .any(|(_, _, c)| c.plane == pos.plane && near(c.x, c.y, STRICT_CITY_CLEARANCE))
    {
        return false;
    }
    clear_of_sites(game, pos, site)
}

/// The races neutral cities of `plane` may have, high men counted twice (the original gives
/// them two of the nine slots; the second is high elves where the land is forest).
fn neutral_races(game: &Game, plane: Plane) -> Vec<RaceId> {
    let rules = &game.content().rules;
    let mut out = Vec::new();
    for r in rules.races.defs() {
        if r.myrror != (plane == Plane::Myrror) || r.num == 7
        /* high_elf */
        {
            continue;
        }
        let Some(id) = rules.races.id_numbered(r.num) else {
            continue;
        };
        out.push(id);
        if r.num == 8
        /* high_men */
        {
            out.push(id);
        }
    }
    out
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
    let masses = game.landmasses(plane);
    let mut default_race: std::collections::HashMap<u32, RaceId> = Default::default();
    let mut made = 0;
    let target = if game.settings().basic.neutral_cities.is_some() {
        game.settings().basic.neutral_city_count()
    } else {
        neutral_city_count(w, h)
    };
    // The original rolls squares at random, 1000 tries for a plane of 60 by 40, 3 to 56
    // across and 3 to 36 down; the custom generator's land is searched row by row from the
    // equator outward, a row south and then a row north, each from west to east: a city goes
    // on the first square that will do, or a better one within two squares of it. The polar
    // rows are left out.
    let original = game.settings().basic.original_map_generator;
    let max_tries = crate::mapgen::scaled_count(MAX_TRIES, w, h).max(MAX_TRIES);
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
                    x: 2 + game.roll(w - 6),
                    y: 2 + game.roll(h - 6),
                };
                if neutral_site_ok(game, pos, false) {
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
            if neutral_site_ok(game, here, true)
                && game.economy().site_max_size(here) >= MIN_SITE_SIZE
            {
                let best = best_site_near(game, here, h);
                x = x.max(best.x + 1);
                break best;
            }
        };
        let forest = game.map().is_some_and(|m| {
            m.plane(plane)
                .get(pos.x as i32, pos.y as i32)
                .and_then(|t| game.content().terrains.get(t))
                .is_some_and(|t| t.class == TerrainClass::Forest)
        });
        let slot = game.roll(races.len() as u32) as usize - 1;
        let mut race = races[slot];
        // The second high men slot is high elves on forest.
        if forest && races[..slot].contains(&race) {
            if let Some(elf) = game.content().rules.races.id_numbered(7)
            /* high_elf */
            {
                race = elf;
            }
        }
        if game.roll(4) > 1 {
            let mass = masses
                .get((pos.y * w + pos.x) as usize)
                .copied()
                .unwrap_or(0);
            let pick = game.roll(races.len() as u32) as usize - 1;
            race = *default_race.entry(mass).or_insert(races[pick]);
        }
        game.found_neutral_city(pos, race);
        made += 1;
    }
    made
}

/// The site within two squares of `at` (itself included) where the most citizens could live;
/// `at` unless another is better.
fn best_site_near(game: &Game, at: Position, height: u32) -> Position {
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
            if pos != at && neutral_site_ok(game, pos, true) {
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
}
