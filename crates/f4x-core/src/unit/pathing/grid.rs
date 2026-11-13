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
use crate::path_grid::{Costs, PathGrid, Road, Searcher, Square, BLOCKED};
use crate::player::PlayerId;
use crate::unit::movement::{MoveError, StackTraits, StepEntry, StepProfile, ROAD_MOVE};

/// A cost in the half moves of a grid; never the value of a blocked square.
fn half_moves(cost: HalfInt) -> u8 {
    u8::try_from(cost.raw().max(0)).map_or(BLOCKED - 1, |c| c.min(BLOCKED - 1))
}

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
                        Ok(StepEntry::Flat(cost)) => {
                            let c = half_moves(cost);
                            (c, c, c)
                        }
                        Ok(StepEntry::Land(cost)) => (
                            half_moves(cost),
                            half_moves(ROAD_MOVE),
                            half_moves(HalfInt::ZERO),
                        ),
                        Err(_) => (BLOCKED, BLOCKED, BLOCKED),
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

/// Set `F4X_CHECK_GRID` to search the old way as well and compare (slow).
fn checking() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("F4X_CHECK_GRID").is_some())
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
        let room = economy.ship_room_squares(owner);
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

    /// Whether the stack is stopped on `square`: hostile units or a city, or a guarded site.
    fn stops_at<'a>(
        economy: &'a Economy<'a>,
        hostile: &'a FastSet<Position>,
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
        let stops = Self::stops_at(&economy, &hostile, owner, start.plane, width);
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
        let stops = Self::stops_at(&economy, &hostile, owner, start.plane, width);
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
        let stops = Self::stops_at(&economy, &hostile, owner, start.plane, width);
        let targets: Vec<Square> = targets
            .iter()
            .filter(|t| t.plane == start.plane)
            .map(|t| t.y * width + t.x)
            .collect();
        with_searcher(|searcher| {
            searcher.reaches_any(&grid, start.y * width + start.x, &targets, stops)
        })
    }

    /// What the stack pays for a path from where it stands, in half moves; `None` when a step is
    /// one the rules refuse.
    pub(super) fn path_cost(&self, units: &[hecs::Entity], path: &[Position]) -> Option<u32> {
        let first = *units.first()?;
        let mut at = self.units().unit_position(first)?;
        let owner = self.units().unit_owner(first)?;
        let traits = self.economy().stack_traits(units);
        let economy = self.economy();
        let room = economy.ship_room_squares(owner);
        let economy = economy.with_ship_room(owner, &room);
        let profiles = Economy::distinct_step_profiles(economy.step_profiles(units)?);
        let mut sum = 0;
        for &next in path {
            sum += economy.stack_step_cost(&profiles, at, next, traits)?.raw() as u32;
            at = next;
        }
        Some(sum)
    }

    /// [`Game::grid_path`], and with `F4X_CHECK_GRID` the old search over the squares as well:
    /// both must find a path or neither, and the grid's must cost no more (the old search guesses
    /// a road step at half a move even where enchanted roads make it free).
    pub(super) fn checked_grid_path(
        &self,
        units: &[hecs::Entity],
        to: Position,
        avoid_full: bool,
    ) -> Option<Vec<Position>> {
        let path = self.grid_path(units, to, avoid_full);
        if checking() {
            let old = self.search_path_squares(units, to, avoid_full);
            assert_eq!(old.is_some(), path.is_some(), "a path to {to:?}");
            if let (Some(old), Some(new)) = (&old, &path) {
                let (old_cost, new_cost) = (self.path_cost(units, old), self.path_cost(units, new));
                assert!(
                    new_cost.is_some() && new_cost <= old_cost,
                    "the path to {to:?} costs {new_cost:?} over the grid and {old_cost:?} over the squares"
                );
            }
        }
        path
    }

    /// [`Game::grid_walk`], and with `F4X_CHECK_GRID` the old walk as well: the same squares.
    pub(super) fn checked_grid_walk(
        &self,
        units: &[hecs::Entity],
        targets: &[Position],
    ) -> (FastSet<Position>, bool) {
        let walked = self.grid_walk(units, targets);
        if checking() {
            let old = self.walk_squares(units, targets);
            assert_eq!(old.1, walked.1, "a walk with targets {targets:?}");
            // A walk that stops at a target has gone over the squares that came before it, which
            // equal ways put in either order.
            if !walked.1 {
                assert_eq!(old.0, walked.0, "a walk with targets {targets:?}");
            }
        }
        walked
    }

    /// [`Game::grid_reaches_any`], and with `F4X_CHECK_GRID` the old walk as well.
    pub(super) fn checked_grid_reaches_any(
        &self,
        units: &[hecs::Entity],
        targets: &[Position],
    ) -> bool {
        let reached = self.grid_reaches_any(units, targets);
        if checking() {
            assert_eq!(
                self.walk_squares(units, targets).1,
                reached,
                "reaching {targets:?}"
            );
        }
        reached
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::setup::{new_game, NewGame};
    use crate::game::testutil::shipped;
    use crate::rules::exec::Target;
    use crate::rules::{Ability, MovementType, UnitDef, UnitId};

    /// A unit of a stack: its kind, and an ability it is given besides its own.
    type Member = (UnitId, Option<u16>);
    use f4x_def::testdir::TestResult;

    /// A unit of the shipped rules with the property, by its number.
    fn unit_where(
        game: &Game,
        what: &str,
        has: impl Fn(&UnitDef) -> bool,
    ) -> Result<crate::rules::UnitId, String> {
        let def = game
            .content
            .rules
            .units
            .defs()
            .iter()
            .find(|d| has(d))
            .ok_or(format!("the shipped rules have no {what}"))?;
        game.content
            .rules
            .units
            .id_numbered(def.num)
            .ok_or(format!("{what} has no id"))
    }

    fn with_ability(n: u16) -> impl Fn(&UnitDef) -> bool {
        move |d: &UnitDef| d.abilities.contains(&Ability::new(n))
    }

    /// Roads over much of both planes, some of them enchanted: a new game has none yet.
    fn lay_roads(game: &mut Game, seed: u64) -> TestResult {
        let map = game.map.as_mut().ok_or("no map")?;
        for plane in [Plane::Arcanus, Plane::Myrror] {
            let ground = map.plane_mut(plane);
            let (width, height) = (ground.width(), ground.height());
            let mut enchanted = vec![false; (width * height) as usize];
            for y in 0..height {
                for x in 0..width {
                    let road = !(u64::from(x) * 7 + u64::from(y) * 13 + seed).is_multiple_of(3);
                    ground.set_road(x, y, road);
                    enchanted[(y * width + x) as usize] = (x + 2 * y) % 5 == 0;
                }
            }
            *ground = ground.clone().with_enchanted_roads(enchanted);
        }
        Ok(())
    }

    /// A step's cost in half moves, or `None` where the stack cannot take it.
    fn rule_cost(
        economy: &Economy,
        profiles: &[StepProfile],
        traits: StackTraits,
        from: Position,
        to: Position,
    ) -> Option<u32> {
        economy
            .stack_step_cost(profiles, from, to, traits)
            .map(|c| c.raw() as u32)
    }

    #[test]
    fn the_grid_of_a_stack_gives_every_step_the_cost_the_rules_give() -> TestResult {
        let content = shipped();
        let mut steps_checked = 0;
        for (seed, diagonal) in [(3u64, false), (11, true)] {
            let mut game = new_game(
                &content,
                &NewGame {
                    seed,
                    all_ai: true,
                    ..NewGame::with_wizards(3)
                },
            )?;
            game.settings.basic.diagonal_extra_cost = diagonal;
            lay_roads(&mut game, seed)?;
            let owner = game
                .player_ids()
                .into_iter()
                .find(|&p| game.is_wizard(p))
                .ok_or("no wizard")?;
            let first = *game.units().player_units(owner).first().ok_or("no unit")?;
            let start = game.units().unit_position(first).ok_or("no position")?;
            let walker = unit_where(&game, "plain walker", |d| {
                d.movement == MovementType::Walking && d.abilities.is_empty()
            })?;
            let flyer = unit_where(&game, "flying unit", |d| d.movement == MovementType::Flying)?;
            let swimmer = unit_where(&game, "swimmer", |d| d.movement == MovementType::Swimming)?;
            let ship = unit_where(&game, "ship", |d| d.movement == MovementType::Sailing)?;
            let forester = unit_where(&game, "forest walker", with_ability(25))?;
            let mountaineer = unit_where(&game, "mountain walker", with_ability(45))?;
            let non_corporeal = unit_where(&game, "non-corporeal unit", with_ability(49))?;
            // What no unit has of its own comes from an enchantment or an item.
            let pathfinder = (walker, Some(50));
            let water_walker = (walker, Some(71));
            let wind_walker = (walker, Some(74));
            let plain = |unit| (unit, None);
            let stacks: Vec<(&str, Vec<Member>)> = vec![
                ("walker", vec![plain(walker)]),
                ("flyer", vec![plain(flyer)]),
                ("swimmer", vec![plain(swimmer)]),
                ("ship", vec![plain(ship)]),
                ("wraith-form ship", vec![(ship, Some(49))]),
                (
                    "walker and wraith-form ship",
                    vec![plain(walker), (ship, Some(49))],
                ),
                ("forester", vec![plain(forester)]),
                ("mountaineer", vec![plain(mountaineer)]),
                ("non-corporeal", vec![plain(non_corporeal)]),
                ("pathfinder", vec![pathfinder]),
                ("water walker", vec![water_walker]),
                ("wind walker", vec![wind_walker]),
                ("walker and flyer", vec![plain(walker), plain(flyer)]),
                ("walker and pathfinder", vec![plain(walker), pathfinder]),
                (
                    "walker, forester, mountaineer",
                    vec![plain(walker), plain(forester), plain(mountaineer)],
                ),
                ("walker and ship", vec![plain(walker), plain(ship)]),
                ("ship and flyer", vec![plain(ship), plain(flyer)]),
                (
                    "non-corporeal and walker",
                    vec![plain(non_corporeal), plain(walker)],
                ),
                ("swimmer and walker", vec![plain(swimmer), plain(walker)]),
                ("walker and wind walker", vec![plain(walker), wind_walker]),
                ("walker and water walker", vec![plain(walker), water_walker]),
            ];
            for (name, kinds, plane) in stacks.into_iter().flat_map(|(name, kinds)| {
                [Plane::Arcanus, Plane::Myrror].map(|plane| (name, kinds.clone(), plane))
            }) {
                let here = Position { plane, ..start };
                let mut units = Vec::new();
                for (kind, ability) in kinds {
                    let unit = game.spawn_unit(owner, here, kind);
                    if let Some(n) = ability {
                        game.grant_temp_ability(Target::Unit(unit), Ability::new(n), None);
                    }
                    units.push(unit);
                }
                let grid = game
                    .stack_grid(&units)
                    .ok_or(format!("seed {seed}: no grid for {name}"))?;
                let traits = game.economy().stack_traits(&units);
                let economy = game.economy();
                let room = economy.ship_room_squares(owner);
                let economy = economy.with_ship_room(owner, &room);
                let profiles = Economy::distinct_step_profiles(
                    economy.step_profiles(&units).ok_or("no profiles")?,
                );
                let (width, height) = (grid.width(), grid.height());
                for y in 0..height {
                    for x in 0..width {
                        let from = Position {
                            plane: here.plane,
                            x,
                            y,
                        };
                        for to in super::super::neighbours(&game, from) {
                            assert_eq!(
                                grid.step_cost(grid.square(x, y), grid.square(to.x, to.y)),
                                rule_cost(&economy, &profiles, traits, from, to),
                                "seed {seed}, {name}: step {from:?} to {to:?}"
                            );
                            steps_checked += 1;
                        }
                    }
                }
            }
        }
        assert!(
            steps_checked > 100_000,
            "only {steps_checked} steps checked"
        );
        Ok(())
    }

    #[test]
    fn the_path_over_the_grid_costs_what_the_search_over_the_squares_costs() -> TestResult {
        let content = shipped();
        let (mut found, mut cheaper) = (0, 0);
        for (seed, diagonal) in [(3u64, false), (11, true)] {
            let mut game = new_game(
                &content,
                &NewGame {
                    seed,
                    all_ai: true,
                    ..NewGame::with_wizards(3)
                },
            )?;
            game.settings.basic.diagonal_extra_cost = diagonal;
            lay_roads(&mut game, seed)?;
            let owner = game
                .player_ids()
                .into_iter()
                .find(|&p| game.is_wizard(p))
                .ok_or("no wizard")?;
            let start = {
                let first = *game.units().player_units(owner).first().ok_or("no unit")?;
                game.units().unit_position(first).ok_or("no position")?
            };
            let kinds = [
                unit_where(&game, "plain walker", |d| {
                    d.movement == MovementType::Walking && d.abilities.is_empty()
                })?,
                unit_where(&game, "flying unit", |d| d.movement == MovementType::Flying)?,
                unit_where(&game, "ship", |d| d.movement == MovementType::Sailing)?,
                unit_where(&game, "forest walker", with_ability(25))?,
            ];
            let [walker, flyer, ship, forester] = kinds;
            let mut stacks: Vec<Vec<hecs::Entity>> = Vec::new();
            for members in [
                vec![walker],
                vec![walker, forester],
                vec![walker, ship],
                vec![flyer, walker],
            ] {
                stacks.push(
                    members
                        .into_iter()
                        .map(|k| game.spawn_unit(owner, start, k))
                        .collect(),
                );
            }
            let guide = game.spawn_unit(owner, start, walker);
            game.grant_temp_ability(Target::Unit(guide), Ability::new(50), None);
            stacks.push(vec![game.spawn_unit(owner, start, walker), guide]);
            let real = game.units().player_units(owner);
            stacks.push(
                real.into_iter()
                    .filter(|&u| game.units().unit_position(u) == Some(start))
                    .collect(),
            );
            let (width, height) = {
                let map = game.map.as_ref().ok_or("no map")?;
                (map.width(), map.height())
            };
            for stack in &stacks {
                for y in (0..height).step_by(3) {
                    for x in (0..width).step_by(4) {
                        let to = Position { x, y, ..start };
                        let old = game.search_path_squares(stack, to, false);
                        let new = game.grid_path(stack, to, false);
                        assert_eq!(
                            old.is_some(),
                            new.is_some(),
                            "seed {seed}: a path to {to:?}"
                        );
                        let (Some(old), Some(new)) = (old, new) else {
                            continue;
                        };
                        let (old_cost, new_cost) = (
                            game.path_cost(stack, &old)
                                .ok_or("an old path step is refused")?,
                            game.path_cost(stack, &new)
                                .ok_or("a new path step is refused")?,
                        );
                        // The search over the squares guesses a road step at half a move even
                        // where the roads are enchanted, so it may be a little dear there.
                        assert!(
                            new_cost <= old_cost,
                            "seed {seed}: to {to:?} {new_cost} against {old_cost}"
                        );
                        found += 1;
                        cheaper += usize::from(new_cost < old_cost);
                        assert_eq!(new.last(), Some(&to));
                    }
                }
            }
        }
        assert!(found > 200, "only {found} paths found");
        println!("{found} paths, {cheaper} cheaper over the grid");
        Ok(())
    }

    /// Run with `--release -- --ignored --nocapture`.
    #[test]
    #[ignore = "a timing, not a check"]
    fn timing_of_making_a_grid_and_of_the_two_searches() -> TestResult {
        use std::time::Instant;
        let content = shipped();
        let mut game = new_game(
            &content,
            &NewGame {
                seed: 3,
                all_ai: true,
                ..NewGame::with_wizards(3)
            },
        )?;
        game.settings.basic.diagonal_extra_cost = true;
        lay_roads(&mut game, 3)?;
        let owner = game
            .player_ids()
            .into_iter()
            .find(|&p| game.is_wizard(p))
            .ok_or("no wizard")?;
        let first = *game.units().player_units(owner).first().ok_or("no unit")?;
        let start = game.units().unit_position(first).ok_or("no position")?;
        let walker = unit_where(&game, "plain walker", |d| {
            d.movement == MovementType::Walking && d.abilities.is_empty()
        })?;
        let forester = unit_where(&game, "forest walker", with_ability(25))?;
        let stack: Vec<hecs::Entity> = [walker, forester]
            .into_iter()
            .map(|k| game.spawn_unit(owner, start, k))
            .collect();
        let goals: Vec<Position> = (0..300u32)
            .map(|i| Position {
                x: (i * 7) % 60,
                y: (i * 5) % 40,
                ..start
            })
            .collect();
        let time = |label: &str, work: &mut dyn FnMut()| {
            let began = Instant::now();
            work();
            println!("{label}: {:?}", began.elapsed());
        };
        time("a grid for the stack, 50 times", &mut || {
            for _ in 0..50 {
                assert!(game.stack_grid(&stack).is_some());
            }
        });
        time("search over the squares, 300 goals", &mut || {
            for &g in &goals {
                let _ = game.search_path_squares(&stack, g, false);
            }
        });
        time("grid made and searched, 300 goals", &mut || {
            for &g in &goals {
                let _ = game.grid_path(&stack, g, false);
            }
        });
        Ok(())
    }
}
