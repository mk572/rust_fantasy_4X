//! Cities: ECS entities placed on the map, with founding rules.

pub mod autobuild;
pub mod conquest;
pub mod economy;
pub mod gate;
pub(crate) mod growth;
pub mod neutral;
pub mod nightshade;
pub mod production;

use crate::feature::def::FeatureKind;
use crate::map::terrain::TerrainClass;
use crate::map::{Plane, Position};
use crate::player::PlayerId;

/// The longest a city's name can be.
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
    /// Another wizard keeps Time Stop going: only its units may act (`SETTLE.c`).
    TimeStopped,
}

impl FoundCityError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::NoMap => "refuse_map.found_city_error_no_map",
            Self::OutOfBounds => "refuse_map.found_city_error_out_of_bounds",
            Self::BadTerrain => "refuse_map.found_city_error_bad_terrain",
            Self::BlockedByFeature => "refuse_map.found_city_error_blocked_by_feature",
            Self::CityAlreadyHere => "refuse_map.found_city_error_city_already_here",
            Self::TooCloseToCity => "refuse_map.found_city_error_too_close_to_city",
            Self::TimeStopped => "refuse_map.found_city_error_time_stopped",
        }
    }
}

/// Check the map-level rules for founding a city; `existing` are current city positions. It is
/// called when the ground and the features are fine (see [`check_site_ground`]), so a caller
/// reads the cities only then.
pub fn check_site(
    economy: &economy::Economy,
    pos: Position,
    existing: impl FnOnce() -> Vec<Position>,
) -> Result<(), FoundCityError> {
    check_site_ground(economy, pos)?;
    check_site_spacing(economy, pos, existing())
}

/// The part of [`check_site`] that needs no city: the terrain, the features and the lair.
fn check_site_ground(economy: &economy::Economy, pos: Position) -> Result<(), FoundCityError> {
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
    Ok(())
}

/// The part of [`check_site`] that needs the cities: no city on the square or too close.
fn check_site_spacing(
    economy: &economy::Economy,
    pos: Position,
    existing: Vec<Position>,
) -> Result<(), FoundCityError> {
    let grid = economy.map.ok_or(FoundCityError::NoMap)?.plane(pos.plane);
    for other in existing.into_iter().filter(|o| o.plane == pos.plane) {
        match grid.chebyshev((other.x, other.y), (pos.x, pos.y)) {
            0 => return Err(FoundCityError::CityAlreadyHere),
            d if d < economy.min_city_distance => return Err(FoundCityError::TooCloseToCity),
            _ => {}
        }
    }
    Ok(())
}
