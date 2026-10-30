//! The two things the neutral world does to the players: independent cities that go raiding
//! (`threat/raider.rs`) and monsters that leave their lairs to settle elsewhere and to attack
//! cities (`threat/monster.rs`). The original's `Make_Raiders` and `Make_Monsters` (`AIDATA.c`)
//! start both from a clock that grows by a random 0 to the difficulty each turn; here the
//! raiders and the monsters keep one clock each, and what the raised troops and the roaming
//! monsters do afterwards is the neutral players' AI (`f4x_ai::play_neutrals`).
//!
//! Differences from the original are in `docs/divergences.md`.
//!
//! Tests: in threat/tests.rs.

mod monster;
mod raider;

use serde::{Deserialize, Serialize};

use crate::game::{Event, Game};
use crate::map::terrain::TerrainClass;
use crate::map::{Plane, Position};

/// The clocks of the two threats. Each grows by a roll of 1 to the difficulty plus one every turn
/// and starts its threat when it reaches its limit (30 for the raiders, 50 less 5 for each
/// difficulty level for the monsters), then starts again from 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Threat {
    pub raider_clock: u32,
    pub monster_clock: u32,
}

/// The raider clock's limit.
pub(crate) const RAIDER_LIMIT: u32 = 30;

/// The turn from which monsters leave their lairs.
pub(crate) const MONSTER_FIRST_TURN: u32 = 50;

impl Game {
    /// Once a turn, at the turn change.
    pub(crate) fn process_threats(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        if self.settings.world.raiders_enabled {
            events.extend(self.make_raiders());
        }
        if self.settings.world.monsters_enabled {
            events.extend(self.make_monsters());
        }
        events
    }

    /// The clocks, for tools.
    pub fn threat(&self) -> Threat {
        self.threat
    }

    /// Whether the player is an independent city's player at war (see [`crate::player::Aggressive`]).
    pub fn is_aggressive_player(&self, id: crate::player::PlayerId) -> bool {
        self.player_entity(id).is_some_and(|e| {
            self.world
                .satisfies::<&crate::player::Aggressive>(e)
                .unwrap_or(false)
        })
    }

    /// Put an independent city's player at war or at peace (scenarios and tests; the game puts it
    /// at war when the city raises raiders).
    pub fn scenario_set_aggressive(&mut self, id: crate::player::PlayerId, on: bool) {
        let Some(e) = self.player_entity(id) else {
            return;
        };
        if on {
            let _ = self.world.insert_one(e, crate::player::Aggressive);
        } else {
            let _ = self.world.remove_one::<crate::player::Aggressive>(e);
        }
    }

    /// Make a unit one its owner pays nothing for, or not (scenarios and tests).
    pub fn scenario_set_unpaid(&mut self, unit: hecs::Entity, on: bool) {
        if on {
            let _ = self.world.insert_one(unit, crate::unit::Unpaid);
        } else {
            let _ = self.world.remove_one::<crate::unit::Unpaid>(unit);
        }
    }

    /// The landmass a square is on, from [`Game::all_landmasses`]; 0 for sea.
    fn landmass_at(&self, lands: &[Vec<u32>; 2], at: Position) -> u32 {
        let Some(map) = self.map.as_ref() else {
            return 0;
        };
        let width = map.plane(at.plane).width();
        lands[at.plane.index()]
            .get((at.y * width + at.x) as usize)
            .copied()
            .unwrap_or(0)
    }

    /// The first free land square round `at`, reading the three rows from the top (the
    /// original's `Adjacent_Free_Square`): no unit, no city and no site on it.
    pub(crate) fn adjacent_free_square(&self, at: Position) -> Option<Position> {
        let map = self.map.as_ref()?;
        let grid = map.plane(at.plane);
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let y = at.y as i32 + dy;
                if (dx, dy) == (0, 0) || !(0..grid.height() as i32).contains(&y) {
                    continue;
                }
                let sq = Position {
                    plane: at.plane,
                    x: grid.wrap_x(at.x as i32 + dx),
                    y: y as u32,
                };
                let land = self
                    .economy()
                    .terrain_class_at(sq)
                    .is_some_and(|c| c != TerrainClass::Ocean);
                if land
                    && self.units().units_at(sq).is_empty()
                    && self.economy().city_at(sq).is_none()
                    && self.economy().feature_entity_at(sq).is_none()
                {
                    return Some(sq);
                }
            }
        }
        None
    }

    /// The landmasses of both planes, Arcanus first.
    fn all_landmasses(&self) -> [Vec<u32>; 2] {
        [Plane::Arcanus, Plane::Myrror].map(|p| self.landmasses(p))
    }
}

#[cfg(test)]
mod tests;
