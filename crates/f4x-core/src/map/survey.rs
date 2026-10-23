//! The Surveyor's numbers for a square (task 71, `Compute_Base_Values_For_Map_Square` and
//! `Map_Square_Survey` in the original): why a city cannot be founded there, and what one
//! would get from the land around it.

use crate::city::FoundCityError;
use crate::game::Game;
use crate::map::terrain::TerrainClass;
use crate::map::Position;
use crate::player::PlayerId;

/// What a city founded on a square would have to work with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiteSurvey {
    /// Why no city can be founded here, if so.
    pub blocked: Option<FoundCityError>,
    /// The size the land alone lets it grow to.
    pub max_size: u32,
    /// Percent added to production by the land worked.
    pub production_percent: i32,
    /// Percent added to gold: shore 10, river 20, river mouth 30.
    pub gold_percent: i32,
    /// Some of the land around has not been explored, and is left out.
    pub unexplored: bool,
}

impl Game {
    /// The survey of `at` for `player`; land the player has not explored counts for nothing.
    pub fn site_survey(&self, player: PlayerId, at: Position) -> SiteSurvey {
        let blocked = self.can_found_city(at).err();
        let mut survey = SiteSurvey {
            blocked,
            max_size: 0,
            production_percent: 0,
            gold_percent: 0,
            unexplored: false,
        };
        let Some(map) = self.map.as_ref() else {
            return survey;
        };
        let grid = map.plane(at.plane);
        let (mut food_x2, mut wild_food) = (0i32, 0i32);
        for (p, shared) in self.economy().city_area(at) {
            if !self.is_explored(player, p) {
                survey.unexplored = true;
                continue;
            }
            let Some(def) = grid
                .get(p.x as i32, p.y as i32)
                .and_then(|t| self.content.terrains.get(t))
            else {
                continue;
            };
            let spoiled = grid.is_corrupted(p.x as i32, p.y as i32);
            let half = |v: i32| if shared { v / 2 } else { v };
            if !spoiled {
                food_x2 += half(i32::from(def.food) * 2);
                survey.production_percent += half(def.production_percent);
            }
            if let Some(f) = grid
                .mineral(p.x as i32, p.y as i32)
                .filter(|_| !spoiled)
                .and_then(|f| self.content.features.get(f))
            {
                wild_food += half(f.city.food);
            }
        }
        survey.max_size =
            ((food_x2 / 4 + wild_food).max(0) as u32).min(self.content.rules.config.max_city_size);
        let coast = (-1i32..=1).any(|dy| {
            (-1i32..=1).any(|dx| {
                (dx != 0 || dy != 0)
                    && grid
                        .get(at.x as i32 + dx, at.y as i32 + dy)
                        .and_then(|t| self.content.terrains.get(t))
                        .is_some_and(|d| d.class == TerrainClass::Ocean)
            })
        });
        survey.gold_percent = match (grid.has_river(at.x as i32, at.y as i32), coast) {
            (false, false) => 0,
            (false, true) => 10,
            (true, false) => 20,
            (true, true) => 30,
        };
        survey
    }
}

#[cfg(test)]
mod tests;
