//! Cities: ECS entities placed on the map, with founding rules.

pub mod autobuild;
pub mod conquest;
pub mod economy;
pub mod gate;
pub(crate) mod growth;
pub mod neutral;
pub mod nightshade;
pub mod production;

use crate::content::{FeatureKind, TerrainClass};
use crate::map::{Plane, Position};
use crate::player::PlayerId;

/// The longest a city's name can be (the original's name field holds twelve characters).
pub const NAME_LEN: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct City {
    pub name: String,
}

/// A city as the callers outside the world see it: its name and the id of its owner (the owner
/// itself is the city's [`crate::player::Owner`] component).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CityInfo {
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

impl FoundCityError {
    /// The key of its sentence in `content/text` (task 348).
    pub fn key(self) -> &'static str {
        match self {
            Self::NoMap => "refuse_map.found_city_error_no_map",
            Self::OutOfBounds => "refuse_map.found_city_error_out_of_bounds",
            Self::BadTerrain => "refuse_map.found_city_error_bad_terrain",
            Self::BlockedByFeature => "refuse_map.found_city_error_blocked_by_feature",
            Self::CityAlreadyHere => "refuse_map.found_city_error_city_already_here",
            Self::TooCloseToCity => "refuse_map.found_city_error_too_close_to_city",
        }
    }
}

/// Check the map-level rules for founding a city; `existing` are current city positions.
pub fn check_site(
    economy: &economy::Economy,
    pos: Position,
    existing: impl IntoIterator<Item = Position>,
) -> Result<(), FoundCityError> {
    let content = economy.content;
    let map = economy.map.ok_or(FoundCityError::NoMap)?;
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
    let kind_at = |plane: Plane| economy.feature_kind_at(Position { plane, ..pos });
    let tower_here = [Plane::Arcanus, Plane::Myrror]
        .into_iter()
        .any(|pl| kind_at(pl) == Some(FeatureKind::Tower));
    let blocked = tower_here
        || kind_at(pos.plane) == Some(FeatureKind::Node)
        || economy.lair_at(pos).is_some();
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
    use crate::game::testutil::content;
    use crate::game::{Command, Event, Game, Rejection};
    use crate::map::Grid;

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
            g.set_mineral(12, 5, c.features.id_of("iron"));
            g
        };
        let mut game = Game::new(1);
        game.map = Some(crate::map::Map::new(build(), build()));
        game.content = c;
        let at = |plane, x, y| Position { plane, x, y };
        // A tower stands on both planes, a node on one.
        game.put_feature(at(Plane::Arcanus, 14, 8), "tower");
        game.put_feature(at(Plane::Arcanus, 16, 8), "node");
        game.put_lair(
            at(Plane::Arcanus, 8, 5),
            "lair",
            crate::feature::lair::Lair {
                intact: true,
                guards: Vec::new(),
                gold: 0,
                mana: 0,
                items: Vec::new(),
                prisoner: false,
                special: 0,
                fought_by: Vec::new(),
            },
        );
        for id in (0..3).map(PlayerId) {
            game.add_player(id, "");
        }
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
        // Cleared: the encounter site is gone, so the square is free.
        let lair = g.economy().lair_at(at(a, 8, 5)).unwrap().entity;
        g.clear_site(lair);
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
