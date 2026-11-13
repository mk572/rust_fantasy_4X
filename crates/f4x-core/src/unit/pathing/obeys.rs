//! What a path or a walk over the grid must satisfy by the rules of the game, for the tests:
//! every step is one the movement rule allows, nothing hostile is walked through, and paths and
//! walks agree about what can be reached.

use crate::city::economy::Economy;
use crate::game::Game;
use crate::map::Position;

impl Game {
    /// Why `path` is no path for the stack to `to` by the rules, if it is not.
    pub(super) fn path_breaks_a_rule(
        &self,
        units: &[hecs::Entity],
        to: Position,
        path: &[Position],
    ) -> Option<String> {
        let first = *units.first()?;
        let start = self.units().unit_position(first)?;
        let owner = self.units().unit_owner(first)?;
        let traits = self.economy().stack_traits(units);
        let economy = self.economy();
        let room = economy.ship_room_squares(owner);
        let economy = economy.with_ship_room(owner, &room);
        let profiles = Economy::distinct_step_profiles(economy.step_profiles(units)?);
        if path.last() != Some(&to) {
            return Some(format!("the path does not end at {to:?}"));
        }
        let mut at = start;
        for (i, &next) in path.iter().enumerate() {
            if !super::neighbours(self, at).contains(&next) {
                return Some(format!("{next:?} is no square next to {at:?}"));
            }
            if economy
                .stack_step_cost(&profiles, at, next, traits)
                .is_none()
            {
                return Some(format!("the rule refuses the step {at:?} to {next:?}"));
            }
            if i + 1 < path.len() && self.stops_walk(owner, next) {
                return Some(format!("the path goes through the stop at {next:?}"));
            }
            at = next;
        }
        None
    }

    /// Whether the stack is stopped on `at`: something hostile stands there, or a guarded site.
    fn stops_walk(&self, owner: crate::player::PlayerId, at: Position) -> bool {
        let economy = self.economy();
        economy.hostile_squares(owner).contains(&at)
            || (owner != crate::player::MONSTERS
                && economy.lair_at(at).is_some_and(|l| !l.guards.is_empty()))
    }

    /// Why the path, the walk and `reaches_any` of the stack disagree about `to`, if they do:
    /// a square the walk reaches has a path, a square with a path (not a stop, which a walk never
    /// enters) is reached by the walk, and a path to a square that is no stop obeys the rules.
    pub(super) fn searches_disagree(&self, units: &[hecs::Entity], to: Position) -> Option<String> {
        let first = *units.first()?;
        let start = self.units().unit_position(first)?;
        let owner = self.units().unit_owner(first)?;
        if start == to {
            return None;
        }
        let path = self.grid_path(units, to, false);
        let (_, walked) = self.grid_walk(units, &[to]);
        if walked != self.grid_reaches_any(units, &[to]) {
            return Some(format!("the walk and reaches_any differ about {to:?}"));
        }
        if let Some(path) = &path {
            if let Some(why) = self.path_breaks_a_rule(units, to, path) {
                return Some(why);
            }
        }
        if self.stops_walk(owner, to) {
            return walked.then(|| format!("a walk entered the stop at {to:?}"));
        }
        (walked != path.is_some()).then(|| {
            format!(
                "{to:?}: the walk says {walked}, the path says {}",
                path.is_some()
            )
        })
    }
}
