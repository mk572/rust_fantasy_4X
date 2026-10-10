//! Building roads: engineers work one square at a time, then step on to the next.
//!
//! A square takes a number of turns that depends on its terrain (3 on grassland up to 8 on
//! mountains and swamp); the stack's total construction strength divides it, with a minimum
//! of one turn. Roads halve movement; enchanted roads (and every road on Myrror) cost nothing.

use crate::city::Position;
use crate::content::{FeatureKind, TerrainClass};
use crate::movement::MoveError;
use crate::rules::Stat;
use crate::units::UnitEnchantments;
use crate::{Event, Game};

/// Hook of the Enchant Road spell.
pub const ENCHANT_ROAD: &str = "enchant_road";

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
        // Magic nodes take 4 (sorcery) or 5 turns whatever lies under them.
        if grid
            .feature(at.x as i32, at.y as i32)
            .and_then(|f| self.content.features.get(f))
            .is_some_and(|f| f.kind == FeatureKind::Node)
        {
            let realm = map
                .node_at(at.plane, at.x, at.y)
                .map(|i| map.nodes()[i].realm.as_str());
            return Some(if realm == Some("sorcery") { 4 } else { 5 });
        }
        Some(match self.terrain_class_at(at)? {
            TerrainClass::Ocean => return None,
            TerrainClass::Grassland => 3,
            TerrainClass::Desert => 4,
            TerrainClass::Forest | TerrainClass::Hills | TerrainClass::Tundra => 6,
            TerrainClass::Mountains | TerrainClass::Swamp => 8,
        })
    }

    /// Total road-building strength of the units that can build (one more for each unit
    /// under the Endurance spell).
    pub fn construction_points(&self, units: &[hecs::Entity]) -> u32 {
        units
            .iter()
            .map(|&u| {
                let base = self.unit_stat(u, Stat::Construction).max(0) as u32;
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
        let here = self.unit_position(first).ok_or(RoadError::BadSelection)?;
        let owner = self.unit_owner(first).ok_or(RoadError::BadSelection)?;
        if units
            .iter()
            .any(|&u| self.unit_position(u) != Some(here) || self.unit_owner(u) != Some(owner))
        {
            return Err(RoadError::BadSelection);
        }
        let builders: Vec<hecs::Entity> = units
            .iter()
            .copied()
            .filter(|&u| self.unit_stat(u, Stat::Construction) > 0)
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
            .any(|w| !crate::movement::adjacent(width, w[0], w[1]))
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
            let _ = self.world.remove_one::<crate::units::Order>(u);
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
                    self.unit_position(w) == self.unit_position(u)
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
        let pos = self.unit_position(lead);
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
                Ok(crate::movement::Entry::Free) => {
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
mod tests {
    use super::*;
    use crate::city::PlayerId;
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::rules::Registry;
    use crate::testutil::content_with_magic;
    use crate::units::MovesLeft;
    use crate::Command;

    const P0: PlayerId = PlayerId(0);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    fn world() -> Game {
        let mut c = content_with_magic();
        let mut units = c.rules.units.defs().to_vec();
        for u in units.iter_mut() {
            match u.id.as_str() {
                "walker" => {
                    u.stats.insert(Stat::Construction, 1);
                }
                "slow" => {
                    u.stats.insert(Stat::Construction, 2);
                }
                _ => {}
            }
        }
        c.rules.units = Registry::new(units);
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(4);
        g.content = c;
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        g.players.push(Player::new(P0, "W"));
        g
    }

    fn engineer(g: &mut Game, id: &str, pos: Position) -> hecs::Entity {
        let u = g.content.rules.units.id_of(id).unwrap();
        g.spawn_unit(P0, pos, u)
    }

    fn set_terrain(g: &mut Game, pos: Position, class: TerrainClass) {
        let id = g.content.terrains.first_of_class(class, pos.plane).unwrap();
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(pos.plane)
            .set(pos.x, pos.y, id);
    }

    fn has_road(g: &Game, pos: Position) -> bool {
        g.map
            .as_ref()
            .unwrap()
            .plane(pos.plane)
            .has_road(pos.x as i32, pos.y as i32)
    }

    #[test]
    fn terrain_decides_how_long_a_road_takes() {
        let mut g = world();
        for (class, turns) in [
            (TerrainClass::Grassland, 3),
            (TerrainClass::Desert, 4),
            (TerrainClass::Forest, 6),
            (TerrainClass::Hills, 6),
            (TerrainClass::Tundra, 6),
            (TerrainClass::Mountains, 8),
            (TerrainClass::Swamp, 8),
        ] {
            set_terrain(&mut g, at(3, 3), class);
            assert_eq!(g.turns_to_build_road(at(3, 3)), Some(turns), "{class:?}");
        }
        set_terrain(&mut g, at(3, 3), TerrainClass::Ocean);
        assert_eq!(g.turns_to_build_road(at(3, 3)), None);
        set_terrain(&mut g, at(3, 3), TerrainClass::Grassland);
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set_road(3, 3, true);
        assert_eq!(
            g.turns_to_build_road(at(3, 3)),
            Some(0),
            "a road is already there"
        );
    }

    #[test]
    fn one_engineer_needs_three_turns_on_grassland() {
        let mut g = world();
        let e = engineer(&mut g, "walker", at(5, 5));
        let events = g.apply(Command::BuildRoad {
            units: vec![e],
            path: vec![],
        });
        assert!(events.is_empty(), "nothing finishes at once: {events:?}");
        assert!(!has_road(&g, at(5, 5)));
        g.process_road_work();
        g.process_road_work();
        assert!(!has_road(&g, at(5, 5)), "two turns are not enough");
        let events = g.process_road_work();
        assert!(has_road(&g, at(5, 5)));
        assert!(events.iter().any(|e| matches!(e, Event::RoadBuilt { .. })));
        assert!(g.world.get::<&RoadWork>(e).is_err(), "the work is over");
    }

    #[test]
    fn a_strong_stack_divides_the_time_but_never_below_one_turn() {
        let mut g = world();
        let a = engineer(&mut g, "walker", at(5, 5));
        let b = engineer(&mut g, "slow", at(5, 5));
        // 3 turns / (1 + 2) points = 1 turn.
        g.apply(Command::BuildRoad {
            units: vec![a, b],
            path: vec![],
        });
        let events = g.process_road_work();
        assert!(has_road(&g, at(5, 5)), "{events:?}");
        // A mountain takes 8 / 3 = 2 turns.
        set_terrain(&mut g, at(8, 8), TerrainClass::Mountains);
        let c = engineer(&mut g, "walker", at(8, 8));
        let d = engineer(&mut g, "slow", at(8, 8));
        g.apply(Command::BuildRoad {
            units: vec![c, d],
            path: vec![],
        });
        g.process_road_work();
        assert!(!has_road(&g, at(8, 8)));
        g.process_road_work();
        assert!(has_road(&g, at(8, 8)));
    }

    #[test]
    fn engineers_work_along_a_path() {
        let mut g = world();
        let e = engineer(&mut g, "slow", at(5, 5)); // 2 points: grassland takes 1 turn
        g.apply(Command::BuildRoad {
            units: vec![e],
            path: vec![at(5, 5), at(6, 5), at(7, 5)],
        });
        for _ in 0..4 {
            let _ = g.world.insert_one(e, MovesLeft(4));
            g.process_road_work();
        }
        for x in 5..=7 {
            assert!(has_road(&g, at(x, 5)), "x = {x}");
        }
        assert_eq!(g.unit_position(e), Some(at(7, 5)));
    }

    #[test]
    fn bad_orders_are_refused_and_moving_away_cancels() {
        let mut g = world();
        let spirit = engineer(&mut g, "boat", at(5, 5));
        assert_eq!(g.build_road(&[spirit], vec![]), Err(RoadError::NoBuilders));
        assert_eq!(g.build_road(&[], vec![]), Err(RoadError::BadSelection));
        let e = engineer(&mut g, "walker", at(5, 5));
        assert_eq!(
            g.build_road(&[e], vec![at(5, 5), at(9, 9)]),
            Err(RoadError::BadPath)
        );
        set_terrain(&mut g, at(6, 5), TerrainClass::Ocean);
        assert_eq!(
            g.build_road(&[e], vec![at(5, 5), at(6, 5)]),
            Err(RoadError::CannotBuildHere)
        );
        g.build_road(&[e], vec![]).unwrap();
        let _ = g.world.insert_one(e, at(5, 6));
        g.process_road_work();
        assert!(
            g.world.get::<&RoadWork>(e).is_err(),
            "moved away: cancelled"
        );
        assert!(!has_road(&g, at(5, 5)));
    }

    #[test]
    fn enchanted_roads_and_myrror_roads_are_free() {
        let mut g = world();
        let w = engineer(&mut g, "spirit", at(5, 5)); // the 2-move walker without roads
        let _ = w;
        let walker = engineer(&mut g, "pathfinder", at(5, 5));
        let _ = g.world.insert_one(walker, MovesLeft(2));
        {
            let grid = g.map.as_mut().unwrap().plane_mut(Plane::Arcanus);
            for x in 5..=7 {
                grid.set_road(x, 5, true);
            }
        }
        assert!(g.can_enchant_road(at(6, 6)));
        assert!(
            !g.can_enchant_road(at(15, 10)),
            "no road within two squares"
        );
        let events = g.enchant_roads(at(6, 6));
        assert!(!events.is_empty());
        let boot = engineer(&mut g, "hardy", at(5, 5));
        let _ = g.world.insert_one(boot, MovesLeft(2));
        g.move_stack(&[boot], at(6, 5)).unwrap();
        assert_eq!(
            g.unit_moves_left(boot),
            2,
            "an enchanted road costs nothing"
        );
        g.move_stack(&[boot], at(7, 5)).unwrap();
        assert_eq!(g.unit_moves_left(boot), 2);
        // Plain roads cost half a move; Myrror's are enchanted already.
        let plain = engineer(&mut g, "hardy", at(10, 8));
        {
            let grid = g.map.as_mut().unwrap().plane_mut(Plane::Arcanus);
            grid.set_road(10, 8, true);
            grid.set_road(11, 8, true);
        }
        let _ = g.world.insert_one(plain, MovesLeft(2));
        g.move_stack(&[plain], at(11, 8)).unwrap();
        assert_eq!(g.unit_moves_left(plain), 1);
        let on_myrror = Position {
            plane: Plane::Myrror,
            x: 10,
            y: 8,
        };
        let m = engineer(&mut g, "hardy", on_myrror);
        {
            let grid = g.map.as_mut().unwrap().plane_mut(Plane::Myrror);
            grid.set_road(10, 8, true);
            grid.set_road(11, 8, true);
        }
        let _ = g.world.insert_one(m, MovesLeft(2));
        g.move_stack(&[m], Position { x: 11, ..on_myrror }).unwrap();
        assert_eq!(g.unit_moves_left(m), 2);
    }

    #[test]
    fn road_work_and_enchanted_roads_survive_a_save() {
        let mut g = world();
        let e = engineer(&mut g, "walker", at(5, 5));
        g.build_road(&[e], vec![at(5, 5), at(6, 5)]).unwrap();
        g.process_road_work();
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set_road(2, 2, true);
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set_enchanted_road(2, 2, true);
        let content = g.content.clone();
        let back = g.snapshot().restore(content).unwrap();
        let e2 = back.entity_of_serial(g.serial_of(e)).unwrap();
        let work = back.world.get::<&RoadWork>(e2).unwrap();
        assert_eq!(
            *work,
            RoadWork {
                at: at(5, 5),
                left: 2,
                rest: vec![at(6, 5)]
            }
        );
        assert!(back
            .map
            .as_ref()
            .unwrap()
            .plane(Plane::Arcanus)
            .has_enchanted_road(2, 2));
    }
}
