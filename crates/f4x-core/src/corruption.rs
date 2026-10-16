//! Corrupted land: made by the Corruption spell and by units with Land Corruption, cleansed
//! in three turns by units with Purify, and by Gaia's Blessing.
//!
//! A corrupted square gives its city no food, no production bonus and no mineral bonus.

use crate::city::{PlayerId, Position};
use crate::content::TerrainClass;
use crate::{Event, Game};

pub const CORRUPT_LAND: &str = "corrupt_land";

/// The chance in a hundred, each turn, that a corrupted square heals by itself.
const FADE_PERCENT: u32 = 2;

/// Turns a purifying unit works on one square.
pub const PURIFY_TURNS: u8 = 3;

/// A unit cleansing the square it stands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PurifyWork {
    pub at: Position,
    pub left: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurifyError {
    NoUnits,
    /// No unit of the stack can purify.
    CannotPurify,
    /// The square is not corrupted.
    NotCorrupted,
}

impl Game {
    pub fn is_corrupted(&self, at: Position) -> bool {
        self.map
            .as_ref()
            .is_some_and(|m| m.plane(at.plane).is_corrupted(at.x as i32, at.y as i32))
    }

    /// Corruption works on any land square that is not already corrupted.
    pub fn can_corrupt(&self, at: Position) -> bool {
        self.economy()
            .terrain_class_at(at)
            .is_some_and(|c| c != TerrainClass::Ocean)
            && !self.is_corrupted(at)
    }

    pub(crate) fn corrupt(&mut self, at: Position) -> Vec<Event> {
        if !self.can_corrupt(at) {
            return Vec::new();
        }
        if let Some(m) = self.map.as_mut() {
            m.plane_mut(at.plane).set_corrupted(at.x, at.y, true);
        }
        vec![Event::LandCorrupted { pos: at }]
    }

    fn purify_square(&mut self, at: Position) -> Vec<Event> {
        if let Some(m) = self.map.as_mut() {
            m.plane_mut(at.plane).set_corrupted(at.x, at.y, false);
        }
        vec![Event::LandPurified { pos: at }]
    }

    /// Order a stack with a purifying unit to cleanse the corrupted square it stands on.
    pub fn purify(&mut self, units: &[hecs::Entity]) -> Result<Vec<Event>, PurifyError> {
        let first = *units.first().ok_or(PurifyError::NoUnits)?;
        let at = self
            .units()
            .unit_position(first)
            .ok_or(PurifyError::NoUnits)?;
        let workers: Vec<hecs::Entity> = units
            .iter()
            .copied()
            .filter(|&u| self.units().unit_has_ability(u, "purify"))
            .collect();
        if workers.is_empty() {
            return Err(PurifyError::CannotPurify);
        }
        if !self.is_corrupted(at) {
            return Err(PurifyError::NotCorrupted);
        }
        for u in workers {
            let _ = self.world.remove_one::<crate::units::Order>(u);
            let _ = self.world.insert_one(
                u,
                PurifyWork {
                    at,
                    left: PURIFY_TURNS,
                },
            );
        }
        Ok(Vec::new())
    }

    /// One turn of cleansing: squares whose last purifier finishes become clean.
    pub(crate) fn process_purify(&mut self) -> Vec<Event> {
        let mut workers: Vec<hecs::Entity> = self
            .world
            .query::<&PurifyWork>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        self.sort_by_serial(&mut workers);
        let mut events = Vec::new();
        for u in workers {
            let Some(work) = self.world.get::<&PurifyWork>(u).ok().map(|w| *w) else {
                continue;
            };
            // Moved away, or someone else cleaned it already: the job is off.
            if self.units().unit_position(u) != Some(work.at) || !self.is_corrupted(work.at) {
                let _ = self.world.remove_one::<PurifyWork>(u);
                continue;
            }
            if work.left <= 1 {
                events.extend(self.purify_square(work.at));
                let _ = self.world.remove_one::<PurifyWork>(u);
            } else {
                let _ = self.world.insert_one(
                    u,
                    PurifyWork {
                        at: work.at,
                        left: work.left - 1,
                    },
                );
            }
        }
        events
    }

    /// Each turn every corrupted square has a small chance of healing by itself (whoever
    /// corrupted it: the spell, a unit, Call the Void or Great Wasting).
    pub(crate) fn fade_corruption(&mut self, rng: &mut dyn rand::RngCore) {
        let Some(map) = self.map.as_mut() else {
            return;
        };
        for plane in crate::map::Plane::ALL {
            let grid = map.plane_mut(plane);
            for y in 0..grid.height() {
                for x in 0..grid.width() {
                    if grid.is_corrupted(x as i32, y as i32) && rng.next_u32() % 100 < FADE_PERCENT
                    {
                        grid.set_corrupted(x, y, false);
                    }
                }
            }
        }
    }

    /// Units with Land Corruption that stood still all turn corrupt their square. Call
    /// before the moves are refilled.
    pub(crate) fn process_land_corruption(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for u in self.units().all_units() {
            if !self.units().unit_has_ability(u, "land_corruption") {
                continue;
            }
            let Some(at) = self.units().unit_position(u) else {
                continue;
            };
            if self.units().unit_moves_left(u) >= self.units().unit_max_moves(u) {
                events.extend(self.corrupt(at));
            }
        }
        events
    }

    /// Gaia's Blessing: each turn a city's deserts may turn to grassland, its volcanoes to
    /// hills (a tenth each) and its corrupted squares come clean (a fifth).
    pub(crate) fn process_gaias_blessing(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let mut cities: Vec<_> = self
            .economy()
            .cities()
            .into_iter()
            .map(|(e, ..)| e)
            .collect();
        self.sort_by_serial(&mut cities);
        for city in cities {
            let blessed = self
                .world
                .get::<&crate::economy::Enchantments>(city)
                .is_ok_and(|e| {
                    e.0.iter()
                        .any(|&(s, _)| self.content.rules.spells.get(s).id == "gaias_blessing")
                });
            if !blessed {
                continue;
            }
            let Ok(at) = self.world.get::<&Position>(city).map(|p| *p) else {
                continue;
            };
            let area: Vec<Position> = self
                .economy()
                .city_area(at)
                .into_iter()
                .map(|(p, _)| p)
                .collect();
            for p in area {
                if self.is_corrupted(p) && self.roll(5) == 1 {
                    events.extend(self.purify_square(p));
                }
                let has_volcano = self
                    .map
                    .as_ref()
                    .is_some_and(|m| m.plane(p.plane).volcano(p.x as i32, p.y as i32).is_some());
                match self.economy().terrain_class_at(p) {
                    Some(TerrainClass::Desert) if self.roll(10) == 1 => {
                        events.extend(self.set_class(p, TerrainClass::Grassland));
                    }
                    Some(TerrainClass::Mountains) if has_volcano && self.roll(10) == 1 => {
                        if let Some(m) = self.map.as_mut() {
                            m.plane_mut(p.plane).set_volcano(p.x, p.y, None);
                        }
                        events.extend(self.set_class(p, TerrainClass::Hills));
                    }
                    _ => {}
                }
            }
        }
        events
    }

    fn set_class(&mut self, at: Position, class: TerrainClass) -> Vec<Event> {
        let Some(id) = self.content.terrains.first_of_class(class, at.plane) else {
            return Vec::new();
        };
        if let Some(m) = self.map.as_mut() {
            m.plane_mut(at.plane).set(at.x, at.y, id);
        }
        vec![Event::TerrainChanged { pos: at }]
    }

    /// Warp Node: a node somebody owns turns against them, draining five power.
    pub fn can_warp_node(&self, at: Position, caster: PlayerId) -> bool {
        self.node_at(at)
            .is_some_and(|n| n.owner.is_some_and(|o| o != caster.0) && !n.warped)
    }

    pub(crate) fn warp_node(&mut self, at: Position) -> Vec<Event> {
        let Some(map) = self.map.as_mut() else {
            return Vec::new();
        };
        let Some(i) = map.node_at(at.plane, at.x, at.y) else {
            return Vec::new();
        };
        if let Some(n) = map.node_mut(i) {
            n.warped = true;
        }
        vec![Event::NodeWarped { pos: at }]
    }
}

pub const WARP_NODE: &str = "warp_node";

#[cfg(test)]
mod tests;
