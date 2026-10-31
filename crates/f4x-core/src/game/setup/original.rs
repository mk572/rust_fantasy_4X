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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::setup::{new_game, NewGame};
    use crate::game::testutil::shipped;
    use crate::player::PlayerId;
    use crate::testdir::TestResult;

    fn original(seed: u64, wizards: u8) -> Result<Game, String> {
        let mut spec = NewGame::with_wizards(wizards);
        spec.seed = seed;
        spec.all_ai = true;
        spec.settings.basic.original_map_generator = true;
        new_game(&shipped(), &spec)
    }

    fn fortress(g: &Game, p: PlayerId) -> Option<Position> {
        g.wizard(p)?.fortress()
    }

    #[test]
    fn every_wizard_gets_a_fortress_on_land_that_can_feed_the_start() -> TestResult {
        let g = original(3, 4)?;
        for k in 0..4 {
            let p = PlayerId(k);
            let pos = fortress(&g, p).ok_or("no fortress")?;
            let class = g
                .map
                .as_ref()
                .and_then(|m| m.plane(pos.plane).get(pos.x as i32, pos.y as i32))
                .and_then(|t| g.content.terrains.get(t))
                .map(|d| d.class);
            assert_ne!(
                class,
                Some(TerrainClass::Ocean),
                "wizard {k} stands on land"
            );
            assert!(g.economy().site_max_size(pos) >= 5, "wizard {k} is fed");
        }
        Ok(())
    }

    #[test]
    fn fortresses_on_one_plane_are_far_apart_across_the_wrapping_edge() -> TestResult {
        let g = original(5, 4)?;
        let w = g.map.as_ref().ok_or("no map")?.width();
        let sites: Vec<Position> = (0..4)
            .map(|k| fortress(&g, PlayerId(k)))
            .collect::<Option<_>>()
            .ok_or("a wizard has no fortress")?;
        for (i, a) in sites.iter().enumerate() {
            for b in &sites[i + 1..] {
                if a.plane != b.plane {
                    continue;
                }
                let dx = a.x.abs_diff(b.x);
                let apart = dx.min(w - dx).max(a.y.abs_diff(b.y));
                assert!(apart >= 10, "{a:?} and {b:?} are {apart} apart");
            }
        }
        Ok(())
    }

    #[test]
    fn the_same_seed_rolls_the_same_sites_and_another_seed_others() -> TestResult {
        let sites = |seed| -> Result<Vec<Position>, String> {
            let g = original(seed, 3)?;
            Ok((0..3).filter_map(|k| fortress(&g, PlayerId(k))).collect())
        };
        assert_eq!(sites(8)?, sites(8)?);
        assert_ne!(sites(8)?, sites(9)?);
        Ok(())
    }

    #[test]
    fn a_game_with_no_map_has_no_sites() {
        let mut g = Game::new(1);
        assert_eq!(fortress_sites(&mut g, &[Plane::Arcanus]), None);
    }
}
