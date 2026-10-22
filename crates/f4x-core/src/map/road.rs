//! Building roads: engineers work one square at a time, then step on to the next.
//!
//! A square takes a number of turns that depends on its terrain (3 on grassland up to 8 on
//! mountains and swamp, two more for a river's bridge); the stack's total construction strength divides it, with a minimum
//! of one turn. Roads halve movement; enchanted roads (and every road on Myrror) cost nothing.

use crate::content::TerrainClass;
use crate::game::{Event, Game};
use crate::map::Position;
use crate::rules::Stat;
use crate::unit::movement::MoveError;
use crate::unit::UnitEnchantments;

/// Hook of the Enchant Road spell.
pub const ENCHANT_ROAD: &str = "enchant_road";

/// Turns a river adds to the road on its square: the bridge.
pub const RIVER_BRIDGE_TURNS: u8 = 2;

/// A stack of engineers at work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoadWork {
    /// The square being worked on.
    pub at: Position,
    /// Turns left on this square; 0 means it is done and the unit is to step on.
    pub left: u8,
    /// The squares still to do after this one, in order.
    pub rest: Vec<Position>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoadError {
    /// No units, or not one player's stack on one square.
    BadSelection,
    /// None of the units can build roads.
    NoBuilders,
    /// A road cannot be built on this square (water).
    CannotBuildHere,
    /// The path is not a chain of neighbouring squares.
    BadPath,
}

impl Game {
    /// Turns one worker needs for a road on `at`: 0 if it already has one, `None` on water.
    pub fn turns_to_build_road(&self, at: Position) -> Option<u8> {
        let map = self.map.as_ref()?;
        let grid = map.plane(at.plane);
        if grid.has_road(at.x as i32, at.y as i32) {
            return Some(0);
        }
        // A node is a feature on ordinary ground here, so its square takes the ground's time
        // (the original's node is a terrain of its own: 4 sorcery, 5 nature and chaos).
        let ground = match self.economy().terrain_class_at(at)? {
            TerrainClass::Ocean => return None,
            TerrainClass::Grassland => 3,
            TerrainClass::Desert => 4,
            TerrainClass::Forest | TerrainClass::Hills | TerrainClass::Tundra => 6,
            TerrainClass::Mountains | TerrainClass::Swamp => 8,
        };
        // A river needs a bridge: two turns more (the original's river square takes 5).
        let bridge = if grid.has_river(at.x as i32, at.y as i32) {
            RIVER_BRIDGE_TURNS
        } else {
            0
        };
        Some(ground + bridge)
    }

    /// Total road-building strength of the units that can build (one more for each unit
    /// under the Endurance spell).
    pub fn construction_points(&self, units: &[hecs::Entity]) -> u32 {
        units
            .iter()
            .map(|&u| {
                let base = self.units().unit_stat(u, Stat::Construction).max(0) as u32;
                if base == 0 {
                    return 0;
                }
                let endurance = self.world.get::<&UnitEnchantments>(u).is_ok_and(|l| {
                    l.0.iter()
                        .any(|&(s, _)| self.content.rules.spells.get(s).id == "endurance")
                });
                base + u32::from(endurance)
            })
            .sum()
    }

    /// Turns the stack's engineers need for a road on `at`: 0 if it has one, `None` where none
    /// can be built. For the client's road-building plan.
    pub fn road_turns(&self, units: &[hecs::Entity], at: Position) -> Option<u8> {
        self.turns_to_build_road(at)?;
        Some(self.turns_for(at, self.construction_points(units)))
    }

    fn turns_for(&self, at: Position, points: u32) -> u8 {
        let turns = u32::from(self.turns_to_build_road(at).unwrap_or(0));
        if turns == 0 {
            return 0;
        }
        (turns / points.max(1)).max(1) as u8
    }

    /// Order the stack's engineers to build a road along `path` (the first square is where
    /// they stand; it is added if missing).
    pub fn build_road(
        &mut self,
        units: &[hecs::Entity],
        mut path: Vec<Position>,
    ) -> Result<Vec<Event>, RoadError> {
        let first = *units.first().ok_or(RoadError::BadSelection)?;
        let here = self
            .units()
            .unit_position(first)
            .ok_or(RoadError::BadSelection)?;
        let owner = self
            .units()
            .unit_owner(first)
            .ok_or(RoadError::BadSelection)?;
        if units.iter().any(|&u| {
            self.units().unit_position(u) != Some(here) || self.units().unit_owner(u) != Some(owner)
        }) {
            return Err(RoadError::BadSelection);
        }
        let builders: Vec<hecs::Entity> = units
            .iter()
            .copied()
            .filter(|&u| self.units().unit_stat(u, Stat::Construction) > 0)
            .collect();
        if builders.is_empty() {
            return Err(RoadError::NoBuilders);
        }
        if path.first() != Some(&here) {
            path.insert(0, here);
        }
        let width = self.map.as_ref().ok_or(RoadError::BadPath)?.width();
        if path
            .windows(2)
            .any(|w| !crate::unit::movement::adjacent(width, w[0], w[1]))
        {
            return Err(RoadError::BadPath);
        }
        if path.iter().any(|&p| self.turns_to_build_road(p).is_none()) {
            return Err(RoadError::CannotBuildHere);
        }
        let points = self.construction_points(&builders);
        let left = self.turns_for(here, points);
        let work = RoadWork {
            at: here,
            left,
            rest: path[1..].to_vec(),
        };
        for &u in &builders {
            let _ = self.world.remove_one::<crate::unit::Order>(u);
            let _ = self.world.insert_one(u, work.clone());
        }
        // A square that already has its road needs no work.
        Ok(self.process_road_work_for(&builders))
    }

    /// One turn of road work for every engineer stack.
    pub(crate) fn process_road_work(&mut self) -> Vec<Event> {
        let mut workers: Vec<hecs::Entity> = self
            .world
            .query::<&RoadWork>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        self.sort_by_serial(&mut workers);
        let mut events = Vec::new();
        // Stacks work together: handle each group of workers sharing a square once.
        let mut done: Vec<hecs::Entity> = Vec::new();
        for &u in &workers {
            if done.contains(&u) {
                continue;
            }
            let group: Vec<hecs::Entity> = workers
                .iter()
                .copied()
                .filter(|&w| {
                    self.units().unit_position(w) == self.units().unit_position(u)
                        && self.world.get::<&RoadWork>(w).ok().map(|x| (*x).clone())
                            == self.world.get::<&RoadWork>(u).ok().map(|x| (*x).clone())
                })
                .collect();
            done.extend(group.iter().copied());
            events.extend(self.work_group(&group, true));
        }
        events
    }

    /// Advance a group that has just been given work, without spending a turn.
    fn process_road_work_for(&mut self, group: &[hecs::Entity]) -> Vec<Event> {
        self.work_group(group, false)
    }

    fn work_group(&mut self, group: &[hecs::Entity], spend_turn: bool) -> Vec<Event> {
        let mut events = Vec::new();
        let Some(&lead) = group.first() else {
            return events;
        };
        let Some(mut work) = self.world.get::<&RoadWork>(lead).ok().map(|w| (*w).clone()) else {
            return events;
        };
        let pos = self.units().unit_position(lead);
        if pos != Some(work.at) {
            // Moved away by hand: the job is off.
            for &u in group {
                let _ = self.world.remove_one::<RoadWork>(u);
            }
            return events;
        }
        let points = self.construction_points(group);
        let mut turn_left = spend_turn;
        loop {
            if work.left > 0 {
                if !turn_left {
                    break;
                }
                turn_left = false;
                work.left -= 1;
                if work.left > 0 {
                    break;
                }
            }
            // This square is done (or was already a road).
            if !self.map.as_ref().is_some_and(|m| {
                m.plane(work.at.plane)
                    .has_road(work.at.x as i32, work.at.y as i32)
            }) {
                if let Some(m) = self.map.as_mut() {
                    m.plane_mut(work.at.plane)
                        .set_road(work.at.x, work.at.y, true);
                }
                events.push(Event::RoadBuilt { pos: work.at });
            }
            let Some(&next) = work.rest.first() else {
                for &u in group {
                    let _ = self.world.remove_one::<RoadWork>(u);
                }
                return events;
            };
            match self.move_stack(group, next) {
                Ok(crate::unit::movement::Entry::Free) => {
                    events.push(Event::UnitsMoved {
                        units: group.to_vec(),
                        from: work.at,
                        to: next,
                    });
                    work.at = next;
                    work.rest.remove(0);
                    work.left = self.turns_for(next, points);
                }
                Err(MoveError::NoMovesLeft) => {
                    // Step on next turn.
                    work.left = 0;
                    break;
                }
                _ => {
                    for &u in group {
                        let _ = self.world.remove_one::<RoadWork>(u);
                    }
                    return events;
                }
            }
        }
        for &u in group {
            let _ = self.world.insert_one(u, work.clone());
        }
        events
    }

    /// Whether Enchant Road has anything to work on around `at`.
    pub fn can_enchant_road(&self, at: Position) -> bool {
        self.roads_within(at, 2).next().is_some()
    }

    fn roads_within(&self, at: Position, radius: i32) -> impl Iterator<Item = (u32, u32)> + '_ {
        let map = self.map.as_ref();
        (-radius..=radius)
            .flat_map(move |dy| (-radius..=radius).map(move |dx| (dx, dy)))
            .filter_map(move |(dx, dy)| {
                let grid = map?.plane(at.plane);
                let y = at.y as i32 + dy;
                let x = grid.wrap_x(at.x as i32 + dx);
                (grid.has_road(x as i32, y)).then_some((x, y as u32))
            })
    }

    /// Enchant every road within two squares of `at`.
    pub(crate) fn enchant_roads(&mut self, at: Position) -> Vec<Event> {
        let tiles: Vec<_> = self.roads_within(at, 2).collect();
        if tiles.is_empty() {
            return Vec::new();
        }
        if let Some(m) = self.map.as_mut() {
            for (x, y) in tiles {
                m.plane_mut(at.plane).set_enchanted_road(x, y, true);
            }
        }
        vec![Event::RoadsEnchanted { pos: at }]
    }
}

#[cfg(test)]
mod tests;
