//! The grids of the fast pathing for stacks: what each square costs a unit to enter (its
//! movement kind and abilities, what its stack shares), one grid for a unit and the slowest of
//! them for a stack. The rules of a step stay in `unit/movement.rs`; this only asks them once for
//! every square.
//!
//! Tests: at the bottom of this file, on the real maps of a few seeds against the step costs.

use std::cell::RefCell;
use std::rc::Rc;

use f4x_def::half_int::HalfInt;

use crate::city::economy::Economy;
use crate::fast::FastSet;
use crate::game::Game;
use crate::map::{Plane, Position};
use crate::path_grid::{Costs, PathGrid, Road, Searcher, Square};
use crate::player::PlayerId;
use crate::unit::movement::{MoveError, StackTraits, StepEntry, StepProfile, ROAD_MOVE};

impl Economy<'_> {
    /// The grid of one unit on `plane`, with what its stack gives it (`traits`). It holds the
    /// squares as they are now: the cities, the ships' room and the roads are read once.
    pub(crate) fn unit_grid(
        &self,
        profile: &StepProfile,
        traits: StackTraits,
        plane: Plane,
    ) -> Result<PathGrid, MoveError> {
        let map = self.map.ok_or(MoveError::NoMap)?;
        let ground = map.plane(plane);
        let (width, height) = (ground.width(), ground.height());
        let squares = width as usize * height as usize;
        let mut roads = Vec::with_capacity(squares);
        let mut costs = Costs {
            enter: Vec::with_capacity(squares),
            by_road: Vec::with_capacity(squares),
            by_enchanted_road: Vec::with_capacity(squares),
        };
        for y in 0..height {
            for x in 0..width {
                roads.push(match (ground.has_road(x as i32, y as i32), plane) {
                    (false, _) => Road::None,
                    (true, Plane::Myrror) => Road::Enchanted,
                    (true, _) if ground.has_enchanted_road(x as i32, y as i32) => Road::Enchanted,
                    (true, _) => Road::Plain,
                });
                let (enter, by_road, by_enchanted_road) =
                    match self.step_entry(profile, Position { plane, x, y }, traits) {
                        Ok(StepEntry::Flat(cost)) => (Some(cost), Some(cost), Some(cost)),
                        Ok(StepEntry::Land(cost)) => {
                            (Some(cost), Some(ROAD_MOVE), Some(HalfInt::ZERO))
                        }
                        Err(_) => (None, None, None),
                    };
                costs.enter.push(enter);
                costs.by_road.push(by_road);
                costs.by_enchanted_road.push(by_enchanted_road);
            }
        }
        PathGrid::new(width, height, costs, roads, self.diagonal_extra)
            .map_err(|_| MoveError::OffMap)
    }
}

thread_local! {
    static SEARCHER: RefCell<Searcher> = RefCell::new(Searcher::new());
}

/// Run a search with the thread's own tables, or fresh ones when a search is already running.
fn with_searcher<R>(search: impl FnOnce(&mut Searcher) -> R) -> R {
    SEARCHER.with(|cell| match cell.try_borrow_mut() {
        Ok(mut searcher) => search(&mut searcher),
        Err(_) => search(&mut Searcher::new()),
    })
}

/// What decides a stack's grid: whose it is, the plane, what the units can do and what the stack
/// gives them. Where the stack stands does not matter.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct GridKey {
    owner: PlayerId,
    plane: Plane,
    profiles: Vec<(u16, Vec<crate::rules::Ability>)>,
    traits: StackTraits,
}

/// A grid kept for the turn, with the squares of the owner's ships' room it was made for: the
/// room changes when a ship moves or loads, which a walk does.
pub(crate) struct CachedGrid {
    grid: Rc<PathGrid>,
    room: FastSet<Position>,
}

impl Game {
    /// The grid of a stack on the plane it stands on, at the pace of its slowest unit: the cost
    /// of a step is the worst of its units' costs, and a square one cannot enter is closed to the
    /// stack. `None` for no units, no map, or a unit that cannot move at all. Kept for the turn
    /// of a computer wizard (see `unit/kept.rs`).
    pub fn stack_grid(&self, units: &[hecs::Entity]) -> Option<Rc<PathGrid>> {
        let first = *units.first()?;
        let start = self.units().unit_position(first)?;
        let owner = self.units().unit_owner(first)?;
        let traits = self.economy().stack_traits(units);
        let economy = self.economy();
        let room = economy.ship_room_squares_for(owner, units);
        let cities = economy.own_city_squares(owner);
        let economy = economy
            .with_ship_room(owner, &room)
            .with_own_cities(owner, &cities);
        let profiles = Economy::distinct_step_profiles(economy.step_profiles(units)?);
        let mut signatures: Vec<_> = profiles.iter().map(|p| p.signature()).collect();
        signatures.sort();
        signatures.dedup();
        let key = GridKey {
            owner,
            plane: start.plane,
            profiles: signatures,
            traits,
        };
        let make = || -> Option<PathGrid> {
            let grids: Vec<PathGrid> = profiles
                .iter()
                .map(|p| economy.unit_grid(p, traits, start.plane).ok())
                .collect::<Option<_>>()?;
            // One kind of unit: its own grid is the stack's.
            match <[PathGrid; 1]>::try_from(grids) {
                Ok([only]) => Some(only),
                Err(grids) => PathGrid::slowest(&grids.iter().collect::<Vec<_>>()).ok(),
            }
        };
        if let Some(kept) = crate::unit::kept::grid(&self.world, &key) {
            if kept.room == room {
                if crate::unit::kept::checking() {
                    assert!(
                        make().is_some_and(|fresh| fresh == *kept.grid),
                        "a kept grid is stale"
                    );
                }
                return Some(kept.grid.clone());
            }
        }
        let grid = Rc::new(make()?);
        let kept = Rc::new(CachedGrid {
            grid: grid.clone(),
            room,
        });
        crate::unit::kept::keep_grid(&self.world, key, &kept);
        Some(grid)
    }

    /// The sea squares a party of `units` may step onto but not walk on over: the squares of a
    /// computer player's ships that are no bridge (`transport::Bridge`). Its walkers are routed
    /// over the bridges only; a ship's square can still end a path, where they board (the pickup
    /// of the AI). Empty for the human, whose walkers use any ship, and for a stack that needs
    /// no ship.
    pub(crate) fn ship_stops(&self, owner: PlayerId, units: &[hecs::Entity]) -> FastSet<Position> {
        let economy = self.economy();
        let traits = economy.stack_traits(units);
        if self.human() == Some(owner)
            || traits.flies
            || traits.sea_lift
            || !units.iter().any(|&u| economy.needs_ship(u, traits.flies))
        {
            return FastSet::default();
        }
        let mut stops = economy.ship_room_squares_for(owner, units);
        for bridge in economy.bridge_squares(owner) {
            stops.remove(&bridge);
        }
        stops
    }

    /// Whether the stack is stopped on `square`: hostile units or a city, or a guarded site.
    fn stops_at<'a>(
        economy: &'a Economy<'a>,
        hostile: &'a FastSet<Position>,
        ships: &'a FastSet<Position>,
        owner: PlayerId,
        plane: Plane,
        width: u32,
    ) -> impl Fn(Square) -> bool + 'a {
        move |s: Square| {
            let p = Position {
                plane,
                x: s % width,
                y: s / width,
            };
            hostile.contains(&p)
                || ships.contains(&p)
                || (owner != crate::player::MONSTERS
                    && economy.lair_at(p).is_some_and(|l| !l.guards.is_empty()))
        }
    }

    /// [`Game::find_path`]'s search over the stack's grid: the cheapest path for the stack to
    /// `to` (not through hostile squares or guarded sites; with `avoid_full`, not through squares
    /// with no room for it), without the start.
    pub(super) fn grid_path(
        &self,
        units: &[hecs::Entity],
        to: Position,
        avoid_full: bool,
    ) -> Option<Vec<Position>> {
        let first = *units.first()?;
        let start = self.units().unit_position(first)?;
        let owner = self.units().unit_owner(first)?;
        if start.plane != to.plane || start == to {
            return None;
        }
        let grid = self.stack_grid(units)?;
        let width = grid.width();
        let economy = self.economy();
        let hostile = economy.hostile_squares(owner);
        let full = if avoid_full {
            economy.full_squares(units.len())
        } else {
            Default::default()
        };
        let at = |p: Position| p.y * width + p.x;
        let square = |s: Square| Position {
            plane: start.plane,
            x: s % width,
            y: s / width,
        };
        let ships = self.ship_stops(owner, units);
        let stops = Self::stops_at(&economy, &hostile, &ships, owner, start.plane, width);
        let path = with_searcher(|searcher| {
            searcher.path(&grid, at(start), at(to), stops, |s| {
                full.contains(&square(s))
            })
        })?;
        Some(path.into_iter().map(square).collect())
    }

    /// The walk of [`Game::reachable_squares`] and [`Game::unreached_region`] over the grid:
    /// what the stack can walk to (until one of `targets`, when there are any), and whether a
    /// target was among it.
    pub(super) fn grid_walk(
        &self,
        units: &[hecs::Entity],
        targets: &[Position],
    ) -> (FastSet<Position>, bool) {
        let mut seen = FastSet::default();
        let Some(&first) = units.first() else {
            return (seen, false);
        };
        let (Some(start), Some(owner), Some(grid)) = (
            self.units().unit_position(first),
            self.units().unit_owner(first),
            self.stack_grid(units),
        ) else {
            return (seen, false);
        };
        let width = grid.width();
        let economy = self.economy();
        let hostile = economy.hostile_squares(owner);
        let ships = self.ship_stops(owner, units);
        let stops = Self::stops_at(&economy, &hostile, &ships, owner, start.plane, width);
        let targets: Vec<Square> = targets
            .iter()
            .filter(|t| t.plane == start.plane)
            .map(|t| t.y * width + t.x)
            .collect();
        let found = with_searcher(|searcher| {
            let found = searcher.walk(&grid, start.y * width + start.x, &targets, stops);
            seen.extend(searcher.settled().iter().map(|&s| Position {
                plane: start.plane,
                x: s % width,
                y: s / width,
            }));
            found
        });
        (seen, found)
    }

    /// Whether the stack can walk to any of `targets`, without collecting what it walks over.
    pub(super) fn grid_reaches_any(&self, units: &[hecs::Entity], targets: &[Position]) -> bool {
        let Some(&first) = units.first() else {
            return false;
        };
        let (Some(start), Some(owner), Some(grid)) = (
            self.units().unit_position(first),
            self.units().unit_owner(first),
            self.stack_grid(units),
        ) else {
            return false;
        };
        let width = grid.width();
        let economy = self.economy();
        let hostile = economy.hostile_squares(owner);
        let ships = self.ship_stops(owner, units);
        let stops = Self::stops_at(&economy, &hostile, &ships, owner, start.plane, width);
        let targets: Vec<Square> = targets
            .iter()
            .filter(|t| t.plane == start.plane)
            .map(|t| t.y * width + t.x)
            .collect();
        with_searcher(|searcher| {
            searcher.reaches_any(&grid, start.y * width + start.x, &targets, stops)
        })
    }
}
