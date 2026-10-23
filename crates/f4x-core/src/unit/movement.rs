//! Overland movement: what a step costs each unit, moving stacks, shifting planes.
//!
//! Costs are in half-moves. A unit with moves left may always take a step, even when
//! the step costs more than it has left (it just ends with none).

use crate::city::economy::Economy;
use crate::map::terrain::TerrainClass;
use crate::map::{Plane, Position};
use crate::player::PlayerId;

use crate::game::Game;
use crate::rules::MovementType;
use crate::unit::{MovesLeft, MAX_STACK};

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

impl MoveError {
    /// The key of its sentence in `content/text` (task 348).
    pub fn key(self) -> &'static str {
        match self {
            Self::NoMap => "refuse_map.move_error_no_map",
            Self::NoUnits => "refuse_map.move_error_no_units",
            Self::BadSelection => "refuse_map.move_error_bad_selection",
            Self::NotAdjacent => "refuse_map.move_error_not_adjacent",
            Self::OffMap => "refuse_map.move_error_off_map",
            Self::Impassable => "refuse_map.move_error_impassable",
            Self::NoMovesLeft => "refuse_map.move_error_no_moves_left",
            Self::StackFull => "refuse_map.move_error_stack_full",
            Self::NoRoom => "refuse_map.move_error_no_room",
            Self::CannotShiftPlane => "refuse_map.move_error_cannot_shift_plane",
            Self::NoGate => "refuse_map.move_error_no_gate",
            Self::Warded => "refuse_map.move_error_warded",
        }
    }
}

/// What a step into a tile leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Free,
    /// Hostile units or a hostile city: moving there starts a fight.
    Hostile(Vec<hecs::Entity>),
    /// A guarded site (tower, node or encounter site): nothing moved; its guards must be
    /// fought first. Holds the lair's index.
    Site(hecs::Entity),
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

/// A unit's side of a step's cost: see [`Economy::step_profile`].
pub struct StepProfile<'a> {
    unit: hecs::Entity,
    def: &'a crate::rules::UnitDef,
    abilities: Vec<String>,
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
    /// abilities its stack shares. With the setting `diagonal_extra_cost` (which also sets how far units see) a diagonal step costs
    /// one and a half times a straight one (3 half-moves on open ground), rounded up.
    pub fn step_cost(
        &self,
        unit: hecs::Entity,
        from: Position,
        to: Position,
        traits: StackTraits,
    ) -> Result<u32, MoveError> {
        self.step_cost_with(&self.step_profile(unit)?, from, to, traits)
    }

    /// What a unit's step costs depend on besides the squares: its definition and its abilities
    /// (its own, its enchantments' and its items'). A search that costs many steps for one unit
    /// takes it once and calls [`Economy::step_cost_with`].
    pub fn step_profile(&self, unit: hecs::Entity) -> Result<StepProfile<'_>, MoveError> {
        let def = self.units().unit_def(unit).ok_or(MoveError::BadSelection)?;
        Ok(StepProfile {
            unit,
            def,
            abilities: self.units().unit_abilities(unit),
        })
    }

    /// The profiles of a stack's units, or `None` when one of them cannot take a step at all.
    pub fn step_profiles(&self, units: &[hecs::Entity]) -> Option<Vec<StepProfile<'_>>> {
        units.iter().map(|&u| self.step_profile(u).ok()).collect()
    }

    /// The cost of a step for the stack: that of its slowest unit; `None` when one cannot take it.
    pub fn stack_step_cost(
        &self,
        profiles: &[StepProfile],
        from: Position,
        to: Position,
        traits: StackTraits,
    ) -> Option<u32> {
        let mut worst = 0;
        for p in profiles {
            worst = worst.max(self.step_cost_with(p, from, to, traits).ok()?);
        }
        Some(worst)
    }

    /// [`Economy::step_cost`] for a unit whose profile is already at hand.
    pub fn step_cost_with(
        &self,
        profile: &StepProfile,
        from: Position,
        to: Position,
        traits: StackTraits,
    ) -> Result<u32, MoveError> {
        let straight = self.straight_step_cost(profile, from, to, traits)?;
        Ok(if self.diagonal_extra && from.x != to.x && from.y != to.y {
            (straight * 3).div_ceil(2)
        } else {
            straight
        })
    }

    fn straight_step_cost(
        &self,
        profile: &StepProfile,
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
        let StepProfile {
            unit,
            def,
            abilities,
        } = profile;
        let unit = *unit;
        let terrain = grid
            .get(to.x as i32, to.y as i32)
            .and_then(|t| self.content.terrains.get(t))
            .ok_or(MoveError::OffMap)?;
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
                let own_city_here = self
                    .city_at(to)
                    .is_some_and(|c| self.city_owner(c) == self.units().unit_owner(unit));
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
        if (has("forester") || traits.forester) && terrain.class.is_forest() {
            cost = cost.min(FULL_MOVE);
        }
        if (has("mountaineer") || traits.mountaineer)
            && (terrain.class == TerrainClass::Hills || terrain.class.is_mountain())
        {
            cost = cost.min(FULL_MOVE);
        }
        Ok(cost)
    }

    /// The unit of the stack that carries it through the air (Wind Walking), if any.
    pub fn wind_walker(&self, units: &[hecs::Entity]) -> Option<hecs::Entity> {
        units
            .iter()
            .copied()
            .find(|&u| self.units().unit_has_ability(u, "wind_walking"))
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
        let hostile_city = self
            .city_at(pos)
            .is_some_and(|c| self.city_owner(c).is_some_and(|o| o != mover));
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
        let walker = self.wind_walker(units);
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
            // A stack with a Wind Walker moves as far as the Wind Walker can.
            let left = walker.map_or_else(
                || self.units().unit_moves_left(u),
                |w| self.units().unit_moves_left(w),
            );
            if left == 0 {
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
        if owner != crate::player::MONSTERS {
            if let Some(lair) = self.lair_at(to) {
                if !lair.guards.is_empty() {
                    return Ok((costs, Entry::Site(lair.entity)));
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
        self.map.ok_or(MoveError::NoMap)?;
        let on_tower = self.is_tower(from);
        let all_travel = units
            .iter()
            .all(|&u| self.units().unit_has_ability(u, "planar_travel"));
        let (_, owner) = self.check_stack(units)?;
        let on_gate = self.has_gate(from, owner, crate::city::gate::ASTRAL_GATE);
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
        self.feature_kind_at(at) == Some(crate::feature::def::FeatureKind::Tower)
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
        if let (Some(tower), Some(player)) = (
            self.economy().feature_entity_at(at),
            self.player_entity(owner),
        ) {
            let _ = self.world.insert_one(tower, crate::player::Owner(player));
        }
        for plane in Plane::ALL {
            self.fog.explore(
                owner,
                Position { plane, ..at },
                crate::map::fog::sight_for(1),
                self.settings.basic.diagonal_extra_cost,
            );
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
            let sight = self.units().unit_sight(u) as u32;
            self.explore(owner, to, sight);
        }
        // The Wind Walker's allowance is the whole stack's: the others have what it has left.
        if let Some(w) = self.economy().wind_walker(units) {
            let left = self.units().unit_moves_left(w);
            for &u in units.iter().filter(|&&u| u != w) {
                let _ = self.world.insert_one(u, MovesLeft(left));
            }
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
        crate::unit::transport::update_aboard(
            &mut self.world,
            self.map.as_ref(),
            &self.content,
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
            let sight = self.units().unit_sight(u) as u32;
            self.explore(owner, to, sight);
        }
        Ok(to)
    }
}

#[cfg(test)]
mod tests;
