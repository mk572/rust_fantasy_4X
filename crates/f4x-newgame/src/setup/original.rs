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
//!
//! Tests: below.

use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Plane, Position};
use f4x_core::world::rng::GameRng;
use f4x_core::world::World;

/// The most passes with smaller distances before giving up.
const MAX_PASSES: u32 = 60;

/// One site for each wizard, in order, `planes[k]` being wizard `k`'s plane. `None` if no pass
/// found them all.
pub(super) fn fortress_sites(
    world: &mut World,
    planes: &[Plane],
    rng: &mut GameRng,
) -> Option<Vec<Position>> {
    let (w, h) = world.map().map(|m| (m.width(), m.height()))?;
    let sites = world.economy().site_squares();
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
                    x: 2 + rng.roll(w - 6),
                    y: 2 + rng.roll(h - 6),
                };
                let grid = world.map()?.plane(plane);
                let wrap = |a: u32, b: u32| {
                    let d = a.abs_diff(b);
                    d.min(w - d)
                };
                let dist = |o: &Position| wrap(o.x, pos.x).max(o.y.abs_diff(pos.y));
                let water = grid
                    .get(pos.x as i32, pos.y as i32)
                    .and_then(|t| world.content().terrains.get(t))
                    .is_none_or(|d| d.class == TerrainClass::Ocean);
                if water
                    || placed.iter().any(|f| f.plane == plane && dist(f) < apart)
                    || sites
                        .iter()
                        .any(|s| s.plane == plane && dist(s) < clear / 2)
                    || world.can_found_city(pos).is_err()
                {
                    continue;
                }
                if world.economy().site_max_size(pos) < 8 - k as u32 / 3 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::{new_game, NewGame};
    use f4x_core::player::PlayerId;
    use f4x_def::testdir::shipped;
    use f4x_def::testdir::TestResult;

    fn original(seed: u64, wizards: u8) -> Result<World, crate::error::NewGameError> {
        let mut spec = NewGame::with_wizards(wizards);
        spec.seed = seed;
        spec.all_ai = true;
        spec.settings.basic.original_map_generator = true;
        new_game(shipped(), &spec).map(|(world, _)| world)
    }

    fn fortress(w: &World, p: PlayerId) -> Option<Position> {
        w.wizard(p)?.fortress()
    }

    #[test]
    fn every_wizard_gets_a_fortress_on_land_that_can_feed_the_start() -> TestResult {
        let w = original(3, 4)?;
        for k in 0..4 {
            let p = PlayerId(k);
            let pos = fortress(&w, p).ok_or("no fortress")?;
            let class = w
                .map()
                .and_then(|m| m.plane(pos.plane).get(pos.x as i32, pos.y as i32))
                .and_then(|t| w.content().terrains.get(t))
                .map(|d| d.class);
            assert_ne!(
                class,
                Some(TerrainClass::Ocean),
                "wizard {k} stands on land"
            );
            assert!(w.economy().site_max_size(pos) >= 5, "wizard {k} is fed");
        }
        Ok(())
    }

    #[test]
    fn fortresses_on_one_plane_are_far_apart_across_the_wrapping_edge() -> TestResult {
        let w = original(5, 4)?;
        let width = w.map().ok_or("no map")?.width();
        let sites: Vec<Position> = (0..4)
            .map(|k| fortress(&w, PlayerId(k)))
            .collect::<Option<_>>()
            .ok_or("a wizard has no fortress")?;
        for (i, a) in sites.iter().enumerate() {
            for b in &sites[i + 1..] {
                if a.plane != b.plane {
                    continue;
                }
                let dx = a.x.abs_diff(b.x);
                let apart = dx.min(width - dx).max(a.y.abs_diff(b.y));
                assert!(apart >= 10, "{a:?} and {b:?} are {apart} apart");
            }
        }
        Ok(())
    }

    #[test]
    fn the_same_seed_rolls_the_same_sites_and_another_seed_others() -> TestResult {
        let sites = |seed| -> Result<Vec<Position>, String> {
            let w = original(seed, 3)?;
            Ok((0..3).filter_map(|k| fortress(&w, PlayerId(k))).collect())
        };
        assert_eq!(sites(8)?, sites(8)?);
        assert_ne!(sites(8)?, sites(9)?);
        Ok(())
    }

    #[test]
    fn a_game_with_no_map_has_no_sites() {
        let mut rng = GameRng::new(1);
        assert_eq!(
            fortress_sites(&mut World::new(), &[Plane::Arcanus], &mut rng),
            None
        );
    }
}
