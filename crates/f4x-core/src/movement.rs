//! Overland movement: what a step costs each unit, moving stacks, shifting planes.
//!
//! Costs are in half-moves. A unit with moves left may always take a step, even when
//! the step costs more than it has left (it just ends with none).

use crate::city::{PlayerId, Position};
use crate::content::TerrainClass;
use crate::economy::Economy;
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
    /// The ships have no free places for the land units that would board.
    NoRoom,
    /// No tower here, or a unit lacks planar travel.
    CannotShiftPlane,
    /// The stack is not in a city with an Earth Gate, or the target has none (or is not the
    /// same wizard's city on the same plane).
    NoGate,
    /// A Spell Ward on the city at the target bars a fantastic creature of the realm it guards.
    Warded,
}

/// What a step into a tile leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Free,
    /// Hostile units or a hostile city: moving there starts a fight.
    Hostile(Vec<hecs::Entity>),
    /// A guarded site (tower, node or encounter site): nothing moved; its guards must be
    /// fought first. Holds the lair's index.
    Site(usize),
}

const FULL_MOVE: u32 = 2;

pub(crate) fn adjacent(width: u32, a: Position, b: Position) -> bool {
    if a.plane != b.plane || a == b {
        return false;
    }
    let dx = a.x.abs_diff(b.x);
    dx.min(width - dx) <= 1 && a.y.abs_diff(b.y) <= 1
}

/// Abilities that one unit shares with everyone stacked with it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StackTraits {
    /// A wind-walking unit carries the whole stack through the air.
    pub flies: bool,
    /// Every unit crosses any land at road speed.
    pub pathfinding: bool,
    /// Every unit crosses forests for one move.
    pub forester: bool,
    /// Every unit crosses hills and mountains for one move.
    pub mountaineer: bool,
    /// The stack has a ship, which carries its land units over the sea.
    pub sea_lift: bool,
}

impl Economy<'_> {
    /// What a stack's units give each other: flight, pathfinding, forest and mountain walking.
    pub fn stack_traits(&self, units: &[hecs::Entity]) -> StackTraits {
        let any = |a: &str| units.iter().any(|&u| self.units().unit_has_ability(u, a));
        StackTraits {
            flies: any("wind_walking"),
            pathfinding: any("pathfinding"),
            forester: any("forester"),
            mountaineer: any("mountaineer"),
            sea_lift: units.iter().any(|&u| self.is_ship(u)),
        }
    }

    /// Cost in half-moves for `unit` to step from `from` to the adjacent `to`, with the
    /// abilities its stack shares.
    pub fn step_cost(
        &self,
        unit: hecs::Entity,
        from: Position,
        to: Position,
        traits: StackTraits,
    ) -> Result<u32, MoveError> {
        let stack_flies = traits.flies;
        let map = self.map.ok_or(MoveError::NoMap)?;
        let grid = map.plane(to.plane);
        if to.y >= grid.height() || to.x >= grid.width() {
            return Err(MoveError::OffMap);
        }
        let def = self.units().unit_def(unit).ok_or(MoveError::BadSelection)?;
        let terrain = grid
            .get(to.x as i32, to.y as i32)
            .and_then(|t| self.content.terrains.get(t))
            .ok_or(MoveError::OffMap)?;
        let abilities = self.units().unit_abilities(unit);
        let has = |a: &str| abilities.iter().any(|x| x == a);
        let is_water = terrain.class == TerrainClass::Ocean;

        if has("non_corporeal") && !is_water {
            return Ok(1);
        }
        if stack_flies
            || def.movement == MovementType::Flying
            || has("flying")
            || (is_water && has("water_walking"))
        {
            return Ok(FULL_MOVE);
        }
        match def.movement {
            MovementType::Sailing => {
                let own_city_here = self.city_at(to).is_some_and(|c| {
                    self.world
                        .get::<&crate::city::City>(c)
                        .is_ok_and(|c| Some(c.owner) == self.units().unit_owner(unit))
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
        if is_water {
            // Land units ride a ship in their stack for nothing, or board one at the target.
            if traits.sea_lift {
                return Ok(0);
            }
            let owner = self.units().unit_owner(unit);
            return if owner.is_some_and(|o| self.ship_room_at(to, o)) {
                Ok(FULL_MOVE)
            } else {
                Err(MoveError::Impassable)
            };
        }
        if !terrain.walkable {
            return Err(MoveError::Impassable);
        }
        let mut cost = u32::from(terrain.move_cost).max(1) * FULL_MOVE;
        let from_road = map.plane(from.plane).has_road(from.x as i32, from.y as i32);
        if from_road && grid.has_road(to.x as i32, to.y as i32) {
            // Roads halve the cost; enchanted roads (all roads on Myrror) make it nothing.
            let magic = |p: Position| {
                p.plane == Plane::Myrror
                    || map
                        .plane(p.plane)
                        .has_enchanted_road(p.x as i32, p.y as i32)
            };
            cost = if magic(from) && magic(to) { 0 } else { 1 };
        }
        if has("pathfinding") || traits.pathfinding {
            cost = cost.min(1);
        }
        if (has("forester") || traits.forester) && terrain.class == TerrainClass::Forest {
            cost = cost.min(FULL_MOVE);
        }
        if (has("mountaineer") || traits.mountaineer)
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
            .any(|&u| self.units().unit_has_ability(u, "wind_walking"))
    }

    /// Hostile units or city on `pos` from the point of view of `mover`.
    pub fn hostile_at(&self, pos: Position, mover: PlayerId) -> Option<Vec<hecs::Entity>> {
        let defenders: Vec<_> = self
            .units()
            .units_at(pos)
            .into_iter()
            .filter(|&u| self.units().unit_owner(u).is_some_and(|o| o != mover))
            .collect();
        let hostile_city = self.city_at(pos).is_some_and(|c| {
            self.world
                .get::<&crate::city::City>(c)
                .is_ok_and(|c| c.owner != mover)
        });
        (!defenders.is_empty() || hostile_city).then_some(defenders)
    }

    pub(crate) fn check_stack(
        &self,
        units: &[hecs::Entity],
    ) -> Result<(Position, PlayerId), MoveError> {
        let first = *units.first().ok_or(MoveError::NoUnits)?;
        let pos = self
            .units()
            .unit_position(first)
            .ok_or(MoveError::BadSelection)?;
        let owner = self
            .units()
            .unit_owner(first)
            .ok_or(MoveError::BadSelection)?;
        for &u in units {
            if self.units().unit_position(u) != Some(pos)
                || self.units().unit_owner(u) != Some(owner)
            {
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
        let map = self.map.ok_or(MoveError::NoMap)?;
        if !adjacent(map.width(), from, to) {
            return Err(MoveError::NotAdjacent);
        }
        let flies = self.stack_traits(units);
        let to_sea = self.is_sea(to);
        let mut costs = Vec::with_capacity(units.len());
        let mut boarding = 0u32;
        for &u in units {
            // Passengers ride for nothing: those already aboard a ship that moves with them,
            // and land units that board a ship of the stack as it sails out.
            let rides = self.carrier_of(u).is_some_and(|c| units.contains(&c));
            let boards = to_sea && flies.sea_lift && self.needs_ship(u, flies.flies) && !rides;
            if rides || boards {
                if boards {
                    boarding += 1;
                }
                costs.push(0);
                continue;
            }
            if self.units().unit_moves_left(u) == 0 {
                return Err(MoveError::NoMovesLeft);
            }
            costs.push(self.step_cost(u, from, to, flies)?);
        }
        if boarding > 0 {
            let room: u32 = units
                .iter()
                .filter(|&&u| self.is_ship(u))
                .map(|&s| self.ship_room(s))
                .sum();
            if boarding > room {
                return Err(MoveError::NoRoom);
            }
        }
        // A land unit stepping onto a ship waiting at sea needs a place there.
        if to_sea && !flies.sea_lift {
            let walkers = units
                .iter()
                .filter(|&&u| self.needs_ship(u, flies.flies))
                .count() as u32;
            let room: u32 = self
                .ships_at(to, owner)
                .iter()
                .map(|&s| self.ship_room(s))
                .sum();
            if walkers > room {
                return Err(MoveError::NoRoom);
            }
        }
        if let Some(city) = self.city_at(to) {
            if units.iter().any(|&u| self.ward_bars_creature(city, u)) {
                return Err(MoveError::Warded);
            }
        }
        if let Some(defenders) = self.hostile_at(to, owner) {
            return Ok((costs, Entry::Hostile(defenders)));
        }
        if owner != crate::units::NEUTRAL {
            if let Some((index, lair)) = self.lair_at(to) {
                if !lair.guards.is_empty() {
                    return Ok((costs, Entry::Site(index)));
                }
            }
        }
        let present = self.units().units_at(to).len();
        if present + units.len() > MAX_STACK {
            return Err(MoveError::StackFull);
        }
        Ok((costs, Entry::Free))
    }

    /// Whether the stack could change plane where it stands: the tile holds a tower,
    /// or every unit can travel between planes at will.
    pub fn can_shift_plane(&self, units: &[hecs::Entity]) -> Result<Position, MoveError> {
        let (from, _) = self.check_stack(units)?;
        if self.units().planar_seal_active() {
            return Err(MoveError::CannotShiftPlane);
        }
        let map = self.map.ok_or(MoveError::NoMap)?;
        let on_tower = map
            .plane(from.plane)
            .feature(from.x as i32, from.y as i32)
            .and_then(|f| self.content.features.get(f))
            .is_some_and(|f| f.kind == crate::content::FeatureKind::Tower);
        let all_travel = units
            .iter()
            .all(|&u| self.units().unit_has_ability(u, "planar_travel"));
        let (_, owner) = self.check_stack(units)?;
        let on_gate = self.has_gate(from, owner, crate::gates::ASTRAL_GATE);
        if !on_tower && !all_travel && !on_gate {
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
        // An Astral Gate only works while it does not lead into another city.
        if !on_tower && !all_travel && self.city_at(to).is_some() {
            return Err(MoveError::CannotShiftPlane);
        }
        // Every unit must be able to stand on the other plane's tile.
        let flies = self.stack_traits(units);
        for &u in units {
            if self.units().unit_moves_left(u) == 0 {
                return Err(MoveError::NoMovesLeft);
            }
            self.step_cost(u, to, to, flies)
                .map_err(|_| MoveError::CannotShiftPlane)?;
        }
        Ok(to)
    }

    /// Whether the square holds a tower.
    pub(crate) fn is_tower(&self, at: Position) -> bool {
        self.map.is_some_and(|m| {
            m.plane(at.plane)
                .feature(at.x as i32, at.y as i32)
                .and_then(|f| self.content.features.get(f))
                .is_some_and(|f| f.kind == crate::content::FeatureKind::Tower)
        })
    }
}

impl Game {
    /// A stack that ends a move in a tower becomes its owner, and its player sees the tower
    /// square on both planes (the original's `Units_In_Tower`).
    pub(crate) fn claim_tower(&mut self, units: &[hecs::Entity], at: Position) {
        let Some(owner) = units.first().and_then(|&u| self.units().unit_owner(u)) else {
            return;
        };
        if !self.economy().is_tower(at) || self.units().planar_seal_active() {
            return;
        }
        if let Some(m) = self.map.as_mut() {
            m.set_tower_owner(at.x, at.y, owner.0);
        }
        for plane in Plane::ALL {
            self.fog.explore(owner, Position { plane, ..at }, 1);
        }
    }

    /// Move a stack one tile. Returns the entry result; on `Entry::Hostile` nothing moved.
    pub fn move_stack(&mut self, units: &[hecs::Entity], to: Position) -> Result<Entry, MoveError> {
        let (costs, entry) = self.economy().plan_step(units, to)?;
        if matches!(entry, Entry::Hostile(_) | Entry::Site(_)) {
            return Ok(entry);
        }
        let owner = self.units().unit_owner(units[0]).expect("checked");
        let from = self.units().unit_position(units[0]).expect("checked");
        for (&u, cost) in units.iter().zip(costs) {
            let left = self.units().unit_moves_left(u).saturating_sub(cost);
            let _ = self.world.insert_one(u, MovesLeft(left));
            let _ = self.world.insert_one(u, to);
            let sight = self.units().unit_stat(u, crate::rules::Stat::Sight).max(1) as u32;
            self.explore(owner, to, sight);
        }
        self.claim_tower(units, to);
        // Passengers follow a ship that sails on without them being selected.
        let mut carried: Vec<hecs::Entity> = Vec::new();
        for &ship in units.iter().filter(|&&u| self.economy().is_ship(u)) {
            for p in self.economy().passengers_of(ship) {
                if !units.contains(&p) && self.units().unit_position(p) == Some(from) {
                    carried.push(p);
                }
            }
        }
        for &p in &carried {
            let _ = self.world.insert_one(p, to);
        }
        // Who is aboard what now.
        let flies = self.economy().stack_traits(units);
        crate::transport::update_aboard(
            &mut self.world,
            self.map.as_ref(),
            &self.content,
            &self.players,
            units,
            to,
            flies.flies,
        );
        Ok(Entry::Free)
    }

    /// Move the stack to the same tile on the other plane, costing one full move.
    pub fn shift_plane(&mut self, units: &[hecs::Entity]) -> Result<Position, MoveError> {
        let to = self.economy().can_shift_plane(units)?;
        let (_, owner) = self.economy().check_stack(units)?;
        if self.economy().hostile_at(to, owner).is_some() {
            return Err(MoveError::CannotShiftPlane);
        }
        if self.units().units_at(to).len() + units.len() > MAX_STACK {
            return Err(MoveError::StackFull);
        }
        for &u in units {
            let left = self.units().unit_moves_left(u).saturating_sub(FULL_MOVE);
            let _ = self.world.insert_one(u, MovesLeft(left));
            let _ = self.world.insert_one(u, to);
            let sight = self.units().unit_stat(u, crate::rules::Stat::Sight).max(1) as u32;
            self.explore(owner, to, sight);
        }
        Ok(to)
    }
}

#[cfg(test)]
mod tests;
