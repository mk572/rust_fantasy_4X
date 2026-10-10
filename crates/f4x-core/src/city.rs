//! Cities: ECS entities placed on the map, with founding rules.

use crate::content::{Content, TerrainClass};
use crate::map::{Map, Plane};
use crate::rules::RaceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlayerId(pub u8);

/// Where an entity stands. Components on map objects such as cities and units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    pub plane: Plane,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct City {
    pub name: String,
    pub owner: PlayerId,
    pub race: RaceId,
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
    let blocked = grid
        .feature(pos.x as i32, pos.y as i32)
        .and_then(|f| content.features.get(f))
        .is_some_and(|f| f.kind.blocks_city());
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
    use crate::testutil::{at, found, game, P0};
    use crate::{Command, Event, Rejection};

    fn try_found(g: &mut crate::Game, pos: Position) -> Vec<Event> {
        let race = g.content.rules.races.id_of("sunfolk").unwrap();
        g.apply(Command::FoundCity {
            owner: P0,
            race,
            pos,
            name: "Test".into(),
        })
    }

    #[test]
    fn founding_creates_a_city_entity() {
        let mut g = game();
        let pos = at(Plane::Arcanus, 3, 3);
        let city = found(&mut g, P0, "sunfolk", pos);
        assert_eq!(g.city_at(pos), Some(city));
        assert_eq!(g.cities().len(), 1);
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
            rejected(try_found(&mut g, at(a, 0, 3))),
            FoundCityError::BadTerrain
        );
        assert_eq!(
            rejected(try_found(&mut g, at(a, 5, 5))),
            FoundCityError::BadTerrain
        );
        assert_eq!(
            rejected(try_found(&mut g, at(a, 8, 5))),
            FoundCityError::BlockedByFeature
        );
        assert_eq!(
            rejected(try_found(&mut g, at(a, 99, 3))),
            FoundCityError::OutOfBounds
        );
        assert_eq!(g.cities().len(), 0);
    }

    #[test]
    fn minerals_do_not_block_cities() {
        let g = game();
        assert_eq!(g.can_found_city(at(Plane::Arcanus, 12, 5)), Ok(()));
    }

    #[test]
    fn spacing_applies_per_plane_and_wraps() {
        let mut g = game();
        found(&mut g, P0, "sunfolk", at(Plane::Arcanus, 10, 6));
        assert_eq!(
            g.can_found_city(at(Plane::Arcanus, 10, 6)),
            Err(FoundCityError::CityAlreadyHere)
        );
        assert_eq!(
            g.can_found_city(at(Plane::Arcanus, 12, 8)),
            Err(FoundCityError::TooCloseToCity)
        );
        assert_eq!(g.can_found_city(at(Plane::Arcanus, 13, 6)), Ok(()));
        assert_eq!(g.can_found_city(at(Plane::Myrror, 10, 6)), Ok(()));
        found(&mut g, P0, "sunfolk", at(Plane::Arcanus, 19, 6));
        assert_eq!(
            g.can_found_city(at(Plane::Arcanus, 1, 6)),
            Err(FoundCityError::TooCloseToCity)
        );
    }

    #[test]
    fn needs_a_map() {
        let g = crate::Game::new(1);
        assert_eq!(
            g.can_found_city(at(Plane::Arcanus, 1, 1)),
            Err(FoundCityError::NoMap)
        );
    }
}
