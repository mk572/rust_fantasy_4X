//! The original's fortress sites (`Generate_Home_Cities`, `~/projects/ReMoM/doc/NewGame/
//! MAPGEN-Generate_Home_Cities.md`), for the setting `original_map_generator`.
//!
//! A site is rolled anywhere on the wizard's plane, 1000 tries for all the wizards at a time;
//! it is refused on water, within the fortress distance of another fortress, within half the
//! site distance of a node, tower or lair, and where the land cannot feed the start (8 citizens,
//! 7 for the fourth wizard on). A pass that fails, or 500 refusals for food, starts again with
//! the distances one smaller: fortresses 16 down to 10, sites 8 down to 1 (halved when
//! compared). Left out: the original's draw of the AI's race after the site (races are chosen
//! first here) and its bugs in that part.

use crate::game::Game;
use crate::map::terrain::TerrainClass;
use crate::map::{Plane, Position};

/// The most passes with smaller distances before giving up (the original loops for ever).
const MAX_PASSES: u32 = 60;

/// One site for each wizard, in order, `planes[k]` being wizard `k`'s plane. `None` if no pass
/// found them all.
pub(super) fn fortress_sites(game: &mut Game, planes: &[Plane]) -> Option<Vec<Position>> {
    let (w, h) = game.map.as_ref().map(|m| (m.width(), m.height()))?;
    let sites = game.economy().site_squares();
    let (mut apart, mut clear) = (16u32, 8u32);
    for _ in 0..MAX_PASSES {
        let mut placed: Vec<Position> = Vec::new();
        let (mut tries, mut hungry) = (0, 0);
        'wizards: for (k, &plane) in planes.iter().enumerate() {
            loop {
                if tries >= 1000 {
                    break 'wizards;
                }
                tries += 1;
                let pos = Position {
                    plane,
                    x: 2 + game.roll(w - 6),
                    y: 2 + game.roll(h - 6),
                };
                let grid = game.map.as_ref()?.plane(plane);
                let wrap = |a: u32, b: u32| {
                    let d = a.abs_diff(b);
                    d.min(w - d)
                };
                let dist = |o: &Position| wrap(o.x, pos.x).max(o.y.abs_diff(pos.y));
                let water = grid
                    .get(pos.x as i32, pos.y as i32)
                    .and_then(|t| game.content.terrains.get(t))
                    .is_none_or(|d| d.class == TerrainClass::Ocean);
                if water
                    || placed.iter().any(|f| f.plane == plane && dist(f) < apart)
                    || sites
                        .iter()
                        .any(|s| s.plane == plane && dist(s) < clear / 2)
                    || game.can_found_city(pos).is_err()
                {
                    continue;
                }
                if game.economy().site_max_size(pos) < 8 - k as u32 / 3 {
                    hungry += 1;
                    if hungry > 500 {
                        break 'wizards;
                    }
                    continue;
                }
                placed.push(pos);
                break;
            }
        }
        if placed.len() == planes.len() {
            return Some(placed);
        }
        apart = (apart - 1).max(10);
        clear = (clear - 1).max(1);
    }
    None
}
