//! Cities: ECS entities placed on the map, with founding rules.

use serde::{Deserialize, Serialize};

use crate::content::{Content, FeatureKind, TerrainClass};
use crate::map::{Map, Plane};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerId(pub u8);

/// Where an entity stands. Components on map objects such as cities and units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    pub plane: Plane,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct City {
    pub name: String,
    pub owner: PlayerId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundCityError {
    NoMap,
    OutOfBounds,
    /// Water, or terrain the content marks as not cityworthy.
    BadTerrain,
    /// A tower, node or encounter site occupies the tile.
    BlockedByFeature,
    CityAlreadyHere,
    TooCloseToCity,
}

/// Check the map-level rules for founding a city; `existing` are current city positions.
pub fn check_site(
    map: Option<&Map>,
    content: &Content,
    pos: Position,
    existing: impl IntoIterator<Item = Position>,
) -> Result<(), FoundCityError> {
    let map = map.ok_or(FoundCityError::NoMap)?;
    let grid = map.plane(pos.plane);
    if pos.x >= grid.width() || pos.y >= grid.height() {
        return Err(FoundCityError::OutOfBounds);
    }
    let terrain = grid
        .get(pos.x as i32, pos.y as i32)
        .and_then(|t| content.terrains.get(t))
        .ok_or(FoundCityError::BadTerrain)?;
    if terrain.class == TerrainClass::Ocean || !terrain.can_found_city {
        return Err(FoundCityError::BadTerrain);
    }
    // As the original's `Map_Square_Survey`: a tower (on either plane) or a node always blocks,
    // an encounter site only while its lair is intact (B17).
    let kind_at = |plane: Plane| {
        let g = map.plane(plane);
        g.feature(pos.x as i32, pos.y as i32)
            .and_then(|f| content.features.get(f))
            .map(|f| f.kind)
    };
    let tower_here = [Plane::Arcanus, Plane::Myrror]
        .into_iter()
        .any(|pl| kind_at(pl) == Some(FeatureKind::Tower));
    let blocked = tower_here
        || kind_at(pos.plane) == Some(FeatureKind::Node)
        || map.lair_at(pos.plane, pos.x, pos.y).is_some();
    if blocked {
        return Err(FoundCityError::BlockedByFeature);
    }
    for other in existing.into_iter().filter(|o| o.plane == pos.plane) {
        match grid.chebyshev((other.x, other.y), (pos.x, pos.y)) {
            0 => return Err(FoundCityError::CityAlreadyHere),
            d if d < content.rules.config.min_city_distance => {
                return Err(FoundCityError::TooCloseToCity)
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Grid;
    use crate::testutil::content;
    use crate::{Command, Event, Game, Rejection};

    /// 20x12 grassland world with ocean at x=0, a mountain at (5,5), a lair at (8,5)
    /// and a mineral at (12,5) (a city may sit on a mineral).
    fn game() -> Game {
        let c = content();
        let id = |n: &str| c.terrains.id_of(n).unwrap();
        let build = || {
            let mut g = Grid::new(20, 12, id("grassland"));
            for y in 0..12 {
                g.set(0, y, id("ocean"));
            }
            g.set(5, 5, id("mountains"));
            g.set_feature(8, 5, c.features.id_of("lair"));
            g.set_feature(12, 5, c.features.id_of("iron"));
            g
        };
        let mut game = Game::new(1);
        let mut map = crate::map::Map::new(build(), build());
        // A tower stands on both planes, a node on one.
        for plane in [Plane::Arcanus, Plane::Myrror] {
            map.plane_mut(plane)
                .set_feature(14, 8, c.features.id_of("tower"));
        }
        map.plane_mut(Plane::Arcanus)
            .set_feature(16, 8, c.features.id_of("node"));
        map.set_lairs(vec![crate::map::Lair {
            plane: Plane::Arcanus,
            x: 8,
            y: 5,
            site: "lair".into(),
            intact: true,
            guards: Vec::new(),
            gold: 0,
            mana: 0,
            items: Vec::new(),
            prisoner: false,
            special: 0,
        }]);
        game.map = Some(map);
        game.content = c;
        game
    }

    fn at(plane: Plane, x: u32, y: u32) -> Position {
        Position { plane, x, y }
    }

    fn found(game: &mut Game, pos: Position) -> Vec<Event> {
        game.apply(Command::FoundCity {
            owner: PlayerId(0),
            pos,
            name: "Test".into(),
            race: None,
            population: 1,
        })
    }

    #[test]
    fn founding_creates_a_city_entity() {
        let mut g = game();
        let pos = at(Plane::Arcanus, 3, 3);
        let events = found(&mut g, pos);
        assert!(matches!(events[..], [Event::CityFounded { pos: p, .. }] if p == pos));
        assert!(g.economy().city_at(pos).is_some());
        assert_eq!(g.economy().cities().len(), 1);
    }

    #[test]
    fn rejects_bad_sites() {
        let mut g = game();
        let rejected = |e: Vec<Event>| match e[..] {
            [Event::Rejected(Rejection::FoundCity(err))] => err,
            ref other => panic!("expected rejection, got {other:?}"),
        };
        let a = Plane::Arcanus;
        assert_eq!(
            rejected(found(&mut g, at(a, 0, 3))),
            FoundCityError::BadTerrain
        );
        assert_eq!(
            rejected(found(&mut g, at(a, 5, 5))),
            FoundCityError::BadTerrain
        );
        assert_eq!(
            rejected(found(&mut g, at(a, 8, 5))),
            FoundCityError::BlockedByFeature
        );
        assert_eq!(
            rejected(found(&mut g, at(a, 99, 3))),
            FoundCityError::OutOfBounds
        );
        assert_eq!(g.economy().cities().len(), 0);
    }

    #[test]
    fn only_intact_sites_block_but_towers_and_nodes_always_do() {
        let mut g = game();
        let a = Plane::Arcanus;
        assert_eq!(
            g.can_found_city(at(a, 8, 5)),
            Err(FoundCityError::BlockedByFeature)
        );
        // Cleared: the lair is no longer intact, so the square is free (the feature stays here
        // only because the test did not run a real assault).
        g.map.as_mut().unwrap().lair_mut(0).unwrap().intact = false;
        g.map.as_mut().unwrap().plane_mut(a).set_feature(8, 5, None);
        assert_eq!(g.can_found_city(at(a, 8, 5)), Ok(()));
        // A tower blocks on both planes, even with no intact lair record.
        for plane in [a, Plane::Myrror] {
            assert_eq!(
                g.can_found_city(at(plane, 14, 8)),
                Err(FoundCityError::BlockedByFeature)
            );
        }
        assert_eq!(
            g.can_found_city(at(a, 16, 8)),
            Err(FoundCityError::BlockedByFeature)
        );
        // The node is on Arcanus only.
        assert_eq!(g.can_found_city(at(Plane::Myrror, 16, 8)), Ok(()));
    }

    #[test]
    fn minerals_do_not_block_cities() {
        let g = game();
        assert_eq!(g.can_found_city(at(Plane::Arcanus, 12, 5)), Ok(()));
    }

    #[test]
    fn spacing_applies_per_plane_and_wraps() {
        let mut g = game();
        found(&mut g, at(Plane::Arcanus, 10, 6));
        assert_eq!(
            g.can_found_city(at(Plane::Arcanus, 10, 6)),
            Err(FoundCityError::CityAlreadyHere)
        );
        assert_eq!(
            g.can_found_city(at(Plane::Arcanus, 12, 8)),
            Err(FoundCityError::TooCloseToCity)
        );
        // The original needs at least four squares between cities.
        assert_eq!(
            g.can_found_city(at(Plane::Arcanus, 13, 6)),
            Err(FoundCityError::TooCloseToCity)
        );
        assert_eq!(g.can_found_city(at(Plane::Arcanus, 14, 6)), Ok(()));
        // Same coordinates on the other plane are independent.
        assert_eq!(g.can_found_city(at(Plane::Myrror, 10, 6)), Ok(()));
        // x wraps: (19,6) and (1,6) are 2 apart across the seam.
        found(&mut g, at(Plane::Arcanus, 19, 6));
        assert_eq!(
            g.can_found_city(at(Plane::Arcanus, 1, 6)),
            Err(FoundCityError::TooCloseToCity)
        );
    }

    #[test]
    fn needs_a_map() {
        let g = Game::new(1);
        assert_eq!(
            g.can_found_city(at(Plane::Arcanus, 1, 1)),
            Err(FoundCityError::NoMap)
        );
    }
}
