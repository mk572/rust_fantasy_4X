//! Overland movement: what a step costs each unit, moving stacks, shifting planes.
//!
//! Costs are in half-moves. A unit with moves left may always take a step, even when
//! the step costs more than it has left (it just ends with none).

use crate::city::{PlayerId, Position};
use crate::content::TerrainClass;
use crate::map::Plane;
use crate::rules::MovementType;
use crate::units::{MovesLeft, MAX_STACK};
use crate::Game;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveError {
    NoMap,
    NoUnits,
    /// A listed unit does not exist, is not on the first unit's tile, or is not the
    /// mover's own.
    BadSelection,
    NotAdjacent,
    OffMap,
    /// A unit in the stack cannot enter that terrain.
    Impassable,
    NoMovesLeft,
    StackFull,
    /// No tower here, or a unit lacks planar travel.
    CannotShiftPlane,
}

/// What a step into a tile leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Free,
    /// Hostile units or a hostile city: moving there starts a fight.
    Hostile(Vec<hecs::Entity>),
}

const FULL_MOVE: u32 = 2;

fn adjacent(width: u32, a: Position, b: Position) -> bool {
    if a.plane != b.plane || a == b {
        return false;
    }
    let dx = a.x.abs_diff(b.x);
    dx.min(width - dx) <= 1 && a.y.abs_diff(b.y) <= 1
}

impl Game {
    /// Cost in half-moves for `unit` to step from `from` to the adjacent `to`.
    /// `stack_flies` is true when a wind-walking unit carries the whole stack.
    pub fn step_cost(
        &self,
        unit: hecs::Entity,
        from: Position,
        to: Position,
        stack_flies: bool,
    ) -> Result<u32, MoveError> {
        let map = self.map.as_ref().ok_or(MoveError::NoMap)?;
        let grid = map.plane(to.plane);
        if to.y >= grid.height() || to.x >= grid.width() {
            return Err(MoveError::OffMap);
        }
        let def = self.unit_def(unit).ok_or(MoveError::BadSelection)?;
        let terrain = grid
            .get(to.x as i32, to.y as i32)
            .and_then(|t| self.content.terrains.get(t))
            .ok_or(MoveError::OffMap)?;
        let has = |a: &str| def.abilities.iter().any(|x| x == a);
        let is_water = terrain.class == TerrainClass::Ocean;

        if has("non_corporeal") && !is_water {
            return Ok(1);
        }
        if stack_flies || def.movement == MovementType::Flying || has("flying") {
            return Ok(FULL_MOVE);
        }
        match def.movement {
            MovementType::Sailing => {
                let own_city_here = self.city_at(to).is_some_and(|c| {
                    self.world
                        .get::<&crate::city::City>(c)
                        .is_ok_and(|c| Some(c.owner) == self.unit_owner(unit))
                });
                return if is_water || own_city_here {
                    Ok(FULL_MOVE)
                } else {
                    Err(MoveError::Impassable)
                };
            }
            MovementType::Swimming if is_water => return Ok(FULL_MOVE),
            _ => {}
        }
        if is_water || !terrain.walkable {
            return Err(MoveError::Impassable);
        }
        let mut cost = u32::from(terrain.move_cost).max(1) * FULL_MOVE;
        let from_road = map.plane(from.plane).has_road(from.x as i32, from.y as i32);
        if from_road && grid.has_road(to.x as i32, to.y as i32) {
            cost = 1;
        }
        if has("pathfinding") {
            cost = cost.min(1);
        }
        if has("forester") && terrain.class == TerrainClass::Forest {
            cost = cost.min(FULL_MOVE);
        }
        if has("mountaineer")
            && matches!(terrain.class, TerrainClass::Hills | TerrainClass::Mountains)
        {
            cost = cost.min(FULL_MOVE);
        }
        Ok(cost)
    }

    /// Whether any unit in `units` carries its stack through the air.
    pub fn stack_flies(&self, units: &[hecs::Entity]) -> bool {
        units
            .iter()
            .any(|&u| self.unit_has_ability(u, "wind_walking"))
    }

    /// Hostile units or city on `pos` from the point of view of `mover`.
    pub fn hostile_at(&self, pos: Position, mover: PlayerId) -> Option<Vec<hecs::Entity>> {
        let defenders: Vec<_> = self
            .units_at(pos)
            .into_iter()
            .filter(|&u| self.unit_owner(u).is_some_and(|o| o != mover))
            .collect();
        let hostile_city = self.city_at(pos).is_some_and(|c| {
            self.world
                .get::<&crate::city::City>(c)
                .is_ok_and(|c| c.owner != mover)
        });
        (!defenders.is_empty() || hostile_city).then_some(defenders)
    }

    fn check_stack(&self, units: &[hecs::Entity]) -> Result<(Position, PlayerId), MoveError> {
        let first = *units.first().ok_or(MoveError::NoUnits)?;
        let pos = self.unit_position(first).ok_or(MoveError::BadSelection)?;
        let owner = self.unit_owner(first).ok_or(MoveError::BadSelection)?;
        for &u in units {
            if self.unit_position(u) != Some(pos) || self.unit_owner(u) != Some(owner) {
                return Err(MoveError::BadSelection);
            }
        }
        Ok((pos, owner))
    }

    /// Check a one-tile move of a stack without doing it: the per-unit costs and
    /// what waits on the target tile.
    pub fn plan_step(
        &self,
        units: &[hecs::Entity],
        to: Position,
    ) -> Result<(Vec<u32>, Entry), MoveError> {
        let (from, owner) = self.check_stack(units)?;
        let map = self.map.as_ref().ok_or(MoveError::NoMap)?;
        if !adjacent(map.width(), from, to) {
            return Err(MoveError::NotAdjacent);
        }
        let flies = self.stack_flies(units);
        let mut costs = Vec::with_capacity(units.len());
        for &u in units {
            if self.unit_moves_left(u) == 0 {
                return Err(MoveError::NoMovesLeft);
            }
            costs.push(self.step_cost(u, from, to, flies)?);
        }
        if let Some(defenders) = self.hostile_at(to, owner) {
            return Ok((costs, Entry::Hostile(defenders)));
        }
        let present = self.units_at(to).len();
        if present + units.len() > MAX_STACK {
            return Err(MoveError::StackFull);
        }
        Ok((costs, Entry::Free))
    }

    /// Move a stack one tile. Returns the entry result; on `Entry::Hostile` nothing moved.
    pub fn move_stack(&mut self, units: &[hecs::Entity], to: Position) -> Result<Entry, MoveError> {
        let (costs, entry) = self.plan_step(units, to)?;
        if matches!(entry, Entry::Hostile(_)) {
            return Ok(entry);
        }
        let owner = self.unit_owner(units[0]).expect("checked");
        for (&u, cost) in units.iter().zip(costs) {
            let left = self.unit_moves_left(u).saturating_sub(cost);
            let _ = self.world.insert_one(u, MovesLeft(left));
            let _ = self.world.insert_one(u, to);
            let sight = self.unit_stat(u, crate::rules::Stat::Sight).max(1) as u32;
            self.explore(owner, to, sight);
        }
        Ok(Entry::Free)
    }

    /// Whether the stack could change plane where it stands: the tile holds a tower,
    /// or every unit can travel between planes at will.
    pub fn can_shift_plane(&self, units: &[hecs::Entity]) -> Result<Position, MoveError> {
        let (from, _) = self.check_stack(units)?;
        if self.planar_seal_active() {
            return Err(MoveError::CannotShiftPlane);
        }
        let map = self.map.as_ref().ok_or(MoveError::NoMap)?;
        let on_tower = map
            .plane(from.plane)
            .feature(from.x as i32, from.y as i32)
            .and_then(|f| self.content.features.get(f))
            .is_some_and(|f| f.kind == crate::content::FeatureKind::Tower);
        let all_travel = units
            .iter()
            .all(|&u| self.unit_has_ability(u, "planar_travel"));
        if !on_tower && !all_travel {
            return Err(MoveError::CannotShiftPlane);
        }
        let other = match from.plane {
            Plane::Arcanus => Plane::Myrror,
            Plane::Myrror => Plane::Arcanus,
        };
        let to = Position {
            plane: other,
            ..from
        };
        // Every unit must be able to stand on the other plane's tile.
        let flies = self.stack_flies(units);
        for &u in units {
            if self.unit_moves_left(u) == 0 {
                return Err(MoveError::NoMovesLeft);
            }
            self.step_cost(u, to, to, flies)
                .map_err(|_| MoveError::CannotShiftPlane)?;
        }
        Ok(to)
    }

    /// Move the stack to the same tile on the other plane, costing one full move.
    pub fn shift_plane(&mut self, units: &[hecs::Entity]) -> Result<Position, MoveError> {
        let to = self.can_shift_plane(units)?;
        let (_, owner) = self.check_stack(units)?;
        if self.hostile_at(to, owner).is_some() {
            return Err(MoveError::CannotShiftPlane);
        }
        if self.units_at(to).len() + units.len() > MAX_STACK {
            return Err(MoveError::StackFull);
        }
        for &u in units {
            let left = self.unit_moves_left(u).saturating_sub(FULL_MOVE);
            let _ = self.world.insert_one(u, MovesLeft(left));
            let _ = self.world.insert_one(u, to);
            let sight = self.unit_stat(u, crate::rules::Stat::Sight).max(1) as u32;
            self.explore(owner, to, sight);
        }
        Ok(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::PlayerId;
    use crate::map::{Grid, Map, TerrainId};
    use crate::testutil::content_with_units;
    use crate::units::MovesLeft;
    use crate::{Command, Event, Rejection};

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    /// 12x6 world. Row 2: grass x0-1, forest x2, hills x3, mountains x4, grass x5-6, ocean x7-8,
    /// grass x9-11. Everything else grassland. A tower at (1,4) on both planes.
    fn world() -> Game {
        let c = content_with_units();
        let id = |n: &str| c.terrains.id_of(n).unwrap();
        let build = || {
            let mut g = Grid::new(12, 6, id("grassland"));
            g.set(2, 2, id("forest"));
            g.set(3, 2, id("hills"));
            g.set(4, 2, id("mountains"));
            g.set(7, 2, id("ocean"));
            g.set(8, 2, id("ocean"));
            g.set(1, 4, id("grassland"));
            let tower = c
                .features
                .first_of_kind(crate::content::FeatureKind::Tower)
                .unwrap();
            g.set_feature(1, 4, Some(tower));
            g
        };
        let map = Map::new(build(), build());
        let mut game = Game::new(1);
        game.content = c;
        game.map = Some(map);
        game
    }

    fn unit(game: &mut Game, owner: PlayerId, name: &str, pos: Position) -> hecs::Entity {
        let id = game.content.rules.units.id_of(name).unwrap();
        game.spawn_unit(owner, pos, id)
    }

    fn cost(game: &Game, u: hecs::Entity, from: Position, to: Position) -> Result<u32, MoveError> {
        game.step_cost(u, from, to, false)
    }

    #[test]
    fn terrain_costs_for_a_walker() {
        let mut g = world();
        let w = unit(&mut g, P0, "walker", at(1, 2));
        assert_eq!(cost(&g, w, at(1, 2), at(1, 1)), Ok(2), "grass is one move");
        assert_eq!(cost(&g, w, at(1, 2), at(2, 2)), Ok(4), "forest is two");
        assert_eq!(
            cost(&g, w, at(3, 2), at(4, 2)),
            Ok(6),
            "mountains are three"
        );
        assert_eq!(
            cost(&g, w, at(6, 2), at(7, 2)),
            Err(MoveError::Impassable),
            "no walking on water"
        );
    }

    #[test]
    fn abilities_change_costs() {
        let mut g = world();
        let ranger = unit(&mut g, P0, "ranger", at(1, 2));
        assert_eq!(
            cost(&g, ranger, at(1, 2), at(2, 2)),
            Ok(2),
            "forester: forest costs one"
        );
        assert_eq!(
            cost(&g, ranger, at(3, 2), at(4, 2)),
            Ok(2),
            "mountaineer: mountains cost one"
        );
        let pf = unit(&mut g, P0, "pathfinder", at(1, 2));
        assert_eq!(
            cost(&g, pf, at(3, 2), at(4, 2)),
            Ok(1),
            "pathfinding: half a move anywhere"
        );
        let ghost = unit(&mut g, P0, "spirit", at(1, 2));
        assert_eq!(
            cost(&g, ghost, at(3, 2), at(4, 2)),
            Ok(1),
            "non-corporeal: half a move on land"
        );
        assert_eq!(
            cost(&g, ghost, at(6, 2), at(7, 2)),
            Err(MoveError::Impassable),
            "but not on water"
        );
        let scout = unit(&mut g, P0, "scout", at(1, 2));
        assert_eq!(
            cost(&g, scout, at(6, 2), at(7, 2)),
            Ok(2),
            "flying: one move over water"
        );
        assert_eq!(
            cost(&g, scout, at(3, 2), at(4, 2)),
            Ok(2),
            "and over mountains"
        );
    }

    #[test]
    fn boats_stay_on_water() {
        let mut g = world();
        let boat = unit(&mut g, P0, "boat", at(7, 2));
        assert_eq!(cost(&g, boat, at(7, 2), at(8, 2)), Ok(2));
        assert_eq!(
            cost(&g, boat, at(8, 2), at(9, 2)),
            Err(MoveError::Impassable)
        );
    }

    #[test]
    fn roads_halve_the_cost_between_road_tiles() {
        let mut g = world();
        let w = unit(&mut g, P0, "walker", at(0, 3));
        let map = g.map.as_mut().unwrap();
        map.plane_mut(Plane::Arcanus).set_road(0, 3, true);
        map.plane_mut(Plane::Arcanus).set_road(1, 3, true);
        assert_eq!(cost(&g, w, at(0, 3), at(1, 3)), Ok(1));
        assert_eq!(cost(&g, w, at(1, 3), at(2, 3)), Ok(2), "road ends");
    }

    #[test]
    fn moving_spends_moves_and_explores() {
        let mut g = world();
        let w = unit(&mut g, P0, "walker", at(1, 2));
        assert_eq!(g.unit_moves_left(w), 4);
        let before = g.fog.explored_count(P0, Plane::Arcanus);
        assert_eq!(g.move_stack(&[w], at(2, 2)), Ok(Entry::Free));
        assert_eq!(g.unit_position(w), Some(at(2, 2)));
        assert_eq!(g.unit_moves_left(w), 0, "forest used all four half-moves");
        assert!(g.fog.explored_count(P0, Plane::Arcanus) > before);
        assert_eq!(g.move_stack(&[w], at(2, 1)), Err(MoveError::NoMovesLeft));
    }

    #[test]
    fn a_unit_may_always_take_a_step_it_cannot_fully_afford() {
        let mut g = world();
        let slow = unit(&mut g, P0, "slow", at(3, 2));
        assert_eq!(g.unit_moves_left(slow), 2);
        assert_eq!(
            g.move_stack(&[slow], at(4, 2)),
            Ok(Entry::Free),
            "mountains cost 6, it has 2"
        );
        assert_eq!(g.unit_moves_left(slow), 0);
    }

    #[test]
    fn stacks_must_be_together_and_adjacent() {
        let mut g = world();
        let a = unit(&mut g, P0, "walker", at(1, 1));
        let b = unit(&mut g, P0, "walker", at(5, 1));
        assert_eq!(
            g.move_stack(&[a, b], at(2, 1)),
            Err(MoveError::BadSelection)
        );
        assert_eq!(g.move_stack(&[a], at(4, 1)), Err(MoveError::NotAdjacent));
        assert_eq!(g.move_stack(&[], at(1, 1)), Err(MoveError::NoUnits));
    }

    #[test]
    fn the_world_wraps_sideways() {
        let mut g = world();
        let w = unit(&mut g, P0, "walker", at(0, 1));
        assert_eq!(g.move_stack(&[w], at(11, 1)), Ok(Entry::Free));
    }

    #[test]
    fn enemies_block_and_ask_for_a_fight() {
        let mut g = world();
        let mine = unit(&mut g, P0, "walker", at(1, 1));
        let theirs = unit(&mut g, P1, "walker", at(2, 1));
        let events = g.apply(Command::MoveStack {
            units: vec![mine],
            to: at(2, 1),
        });
        assert_eq!(
            events,
            vec![Event::AttackRequested {
                attackers: vec![mine],
                from: at(1, 1),
                to: at(2, 1),
                defenders: vec![theirs]
            }]
        );
        assert_eq!(g.unit_position(mine), Some(at(1, 1)), "nobody moved");
        assert_eq!(g.unit_moves_left(mine), 4, "and nothing was spent");
    }

    #[test]
    fn a_tile_holds_nine_units() {
        let mut g = world();
        for _ in 0..9 {
            unit(&mut g, P0, "walker", at(2, 1));
        }
        let mover = unit(&mut g, P0, "walker", at(1, 1));
        assert_eq!(g.move_stack(&[mover], at(2, 1)), Err(MoveError::StackFull));
    }

    #[test]
    fn wind_walking_carries_the_whole_stack_over_water() {
        let mut g = world();
        let mage = unit(&mut g, P0, "mage", at(6, 2));
        let walker = unit(&mut g, P0, "walker", at(6, 2));
        assert_eq!(
            g.move_stack(&[walker], at(7, 2)),
            Err(MoveError::Impassable),
            "alone it cannot"
        );
        assert_eq!(g.move_stack(&[mage, walker], at(7, 2)), Ok(Entry::Free));
        assert_eq!(g.unit_position(walker), Some(at(7, 2)));
    }

    #[test]
    fn shifting_planes_needs_a_tower_or_planar_travel() {
        let mut g = world();
        let on_tower = unit(&mut g, P0, "walker", at(1, 4));
        let off_tower = unit(&mut g, P0, "walker", at(3, 4));
        let myrror = Position {
            plane: Plane::Myrror,
            ..at(1, 4)
        };
        assert_eq!(g.shift_plane(&[on_tower]), Ok(myrror));
        assert_eq!(g.unit_moves_left(on_tower), 2, "costs one move");
        assert_eq!(
            g.shift_plane(&[off_tower]),
            Err(MoveError::CannotShiftPlane)
        );
        let t = unit(&mut g, P0, "traveller", at(3, 4));
        assert!(g.shift_plane(&[t]).is_ok(), "planar travel works anywhere");
        assert_eq!(g.unit_position(t).unwrap().plane, Plane::Myrror);
    }

    #[test]
    fn a_shift_into_a_blocked_tile_is_refused() {
        let mut g = world();
        let w = unit(&mut g, P0, "walker", at(1, 4));
        unit(
            &mut g,
            P1,
            "walker",
            Position {
                plane: Plane::Myrror,
                ..at(1, 4)
            },
        );
        assert_eq!(g.shift_plane(&[w]), Err(MoveError::CannotShiftPlane));
    }

    #[test]
    fn paths_go_around_water_and_across_the_seam() {
        let mut g = world();
        // Make the water a wall: ocean down the whole of columns 7..=8.
        let ocean = g.content.terrains.id_of("ocean").unwrap();
        for y in 0..6 {
            g.map
                .as_mut()
                .unwrap()
                .plane_mut(Plane::Arcanus)
                .set(7, y, ocean);
            g.map
                .as_mut()
                .unwrap()
                .plane_mut(Plane::Arcanus)
                .set(8, y, ocean);
        }
        let w = unit(&mut g, P0, "walker", at(5, 2));
        // East is walled off, but the world wraps: west across the seam reaches x=9.
        let path = g.find_path(&[w], at(9, 2)).expect("a way round the seam");
        assert_eq!(path.last(), Some(&at(9, 2)));
        assert!(path.len() >= 8, "the long way round: {path:?}");
        let scout = unit(&mut g, P0, "scout", at(5, 2));
        assert_eq!(
            g.find_path(&[scout], at(9, 2)).unwrap().len(),
            4,
            "flying goes straight over"
        );
        // Walled both sides: no path for a walker once the seam is closed too.
        for y in 0..6 {
            g.map
                .as_mut()
                .unwrap()
                .plane_mut(Plane::Arcanus)
                .set(11, y, ocean);
            g.map
                .as_mut()
                .unwrap()
                .plane_mut(Plane::Arcanus)
                .set(0, y, ocean);
        }
        assert_eq!(g.find_path(&[w], at(9, 2)), None);
    }

    #[test]
    fn path_prefers_cheap_terrain() {
        let mut g = world();
        let w = unit(&mut g, P0, "walker", at(1, 2));
        // Straight east along row 2 runs through forest, hills and mountains; row 1 is grass.
        let path = g.find_path(&[w], at(6, 2)).unwrap();
        assert!(
            path.iter().all(|p| p.y != 2 || p.x == 6),
            "avoids the rough row: {path:?}"
        );
    }

    #[test]
    fn goto_orders_continue_over_turns() {
        let mut g = world();
        let w = unit(&mut g, P0, "walker", at(0, 1));
        let events = g.apply(Command::GoTo {
            units: vec![w],
            to: at(5, 1),
        });
        let moved = events
            .iter()
            .filter(|e| matches!(e, Event::UnitsMoved { .. }))
            .count();
        assert_eq!(moved, 2, "two grass tiles with four half-moves");
        assert_eq!(g.unit_position(w).unwrap().x, 2);
        assert!(
            g.world.get::<&crate::units::Order>(w).is_ok(),
            "order remains"
        );
        g.apply(Command::EndTurn);
        assert_eq!(g.unit_position(w).unwrap().x, 4);
        g.apply(Command::EndTurn);
        assert_eq!(g.unit_position(w), Some(at(5, 1)));
        assert!(
            g.world.get::<&crate::units::Order>(w).is_err(),
            "arrived: order cleared"
        );
    }

    #[test]
    fn end_turn_refills_moves() {
        let mut g = world();
        let w = unit(&mut g, P0, "walker", at(1, 1));
        g.world.insert_one(w, MovesLeft(0)).unwrap();
        g.apply(Command::EndTurn);
        assert_eq!(g.unit_moves_left(w), 4);
    }

    #[test]
    fn rejections_are_reported() {
        let mut g = world();
        let w = unit(&mut g, P0, "walker", at(6, 2));
        assert_eq!(
            g.apply(Command::MoveStack {
                units: vec![w],
                to: at(7, 2)
            }),
            vec![Event::Rejected(Rejection::Move(MoveError::Impassable))]
        );
    }

    #[test]
    fn stats_include_abilities_and_levels() {
        use crate::rules::Stat;
        let mut g = world();
        let hardy = unit(&mut g, P0, "hardy", at(1, 1));
        let plain = unit(&mut g, P0, "walker", at(2, 1));
        assert_eq!(g.unit_stat(hardy, Stat::Defense), 4, "tough adds 2");
        assert_eq!(g.unit_stat(plain, Stat::Defense), 2);
        assert_eq!(g.unit_stat(plain, Stat::Melee), 3);
        g.world
            .insert_one(plain, crate::units::Experience(25))
            .unwrap();
        assert_eq!(g.unit_level(plain), 1);
        assert_eq!(g.unit_stat(plain, Stat::Melee), 4, "veteran adds 1");
        assert_eq!(g.unit_hits(plain), (6, 6));
        g.world.insert_one(plain, crate::units::Hurt(2)).unwrap();
        assert_eq!(g.unit_hits(plain), (4, 6));
        let _ = TerrainId(0);
    }
}
