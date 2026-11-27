//! The small questions about units and squares that the unit handling asks.
//!
//! Tests: through the routines that ask them.

use crate::map::terrain::TerrainClass;
use crate::map::Position;
use crate::player::view::PlayerView;
use crate::rules::Stat;
use f4x_def::rules::MovementType;

/// The answers of `can_reach` for one stretch of the AI's turn in which nothing moves: stacks that
/// stand on one square and move alike (the same kinds of unit) get one path search between them,
/// where a search for each of hundreds of units would cost most of the turn.
#[derive(Default)]
pub struct ReachCache {
    squares: std::collections::HashMap<(crate::unit::pathing::ReachSignature, Position), bool>,
    /// Whether any square round a site can be reached, by the site.
    around: std::collections::HashMap<(crate::unit::pathing::ReachSignature, Position), bool>,
}

impl ReachCache {
    /// How many searches the cache has made (one for each stack signature and target).
    #[cfg(test)]
    pub fn searches(&self) -> usize {
        self.squares.len() + self.around.len()
    }

    pub fn can_reach(&mut self, view: &PlayerView, units: &[hecs::Entity], to: Position) -> bool {
        match view.reach_signature(units) {
            Some(key) => *self
                .squares
                .entry((key, to))
                .or_insert_with(|| view.can_reach(units, to)),
            None => view.can_reach(units, to),
        }
    }

    /// Whether the stack can walk to any of `squares`, which are the free squares round `site`.
    pub fn can_reach_round(
        &mut self,
        view: &PlayerView,
        units: &[hecs::Entity],
        site: Position,
        squares: &[Position],
    ) -> bool {
        match view.reach_signature(units) {
            Some(key) => *self
                .around
                .entry((key, site))
                .or_insert_with(|| view.reaches_any(units, squares)),
            None => view.reaches_any(units, squares),
        }
    }
}

/// What a unit adds to a battle, as one number: the one measure of the rules engine
/// (`f4x_battle::power`), at full health. Hurt units are kept out of the army by the
/// callers (`recover::needs_rest`), not weighed less here.
pub fn unit_strength(view: &PlayerView, unit: hecs::Entity) -> f32 {
    view.unit_power(unit)
}

/// A ship: it sails and carries land units.
pub fn is_ship(view: &PlayerView, unit: hecs::Entity) -> bool {
    view.unit_stat(unit, Stat::Transport) > 0
        && view
            .unit_def(unit)
            .is_some_and(|d| d.movement == MovementType::Sailing)
}

/// Does the square touch the sea?
pub fn is_coastal(view: &PlayerView, at: Position) -> bool {
    (-1..=1).any(|dy| {
        (-1..=1).any(|dx| {
            view.shift(at, dx, dy)
                .is_some_and(|p| view.terrain_class(p) == Some(TerrainClass::Ocean))
        })
    })
}

/// A unit that can fight on land (settlers, engineers, ships and the like are not counted):
/// `UnitDef::is_fighter` of its definition.
pub fn is_fighter(view: &PlayerView, unit: hecs::Entity) -> bool {
    view.unit_def(unit).is_some_and(|d| d.is_fighter())
}

/// King-move distance with the horizontal wrap; `None` across planes.
pub fn distance(view: &PlayerView, a: Position, b: Position) -> Option<u32> {
    view.distance(a, b)
}

const NEIGHBOURS: &[(i32, i32)] = &[
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

pub fn neighbours(view: &PlayerView, at: Position) -> Vec<Position> {
    NEIGHBOURS
        .iter()
        .filter_map(|&(dx, dy)| view.shift(at, dx, dy))
        .collect()
}

/// A landing for `city`: the land square nearest to it (by steps over land) that touches the sea,
/// and a sea square beside that. The troops walk the rest of the way. `None` if no coast is
/// known on its land mass.
pub fn landing(view: &PlayerView, city: Position) -> Option<(Position, Position)> {
    use crate::map::terrain::TerrainClass;
    use std::collections::{HashSet, VecDeque};
    let mut seen: HashSet<Position> = HashSet::new();
    let mut queue: VecDeque<Position> = VecDeque::new();
    seen.insert(city);
    queue.push_back(city);
    while let Some(here) = queue.pop_front() {
        if seen.len() > 900 {
            break;
        }
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let Some(next) = view.shift(here, dx, dy) else {
                    continue;
                };
                if !seen.insert(next) {
                    continue;
                }
                if matches!(view.terrain_class(next), Some(TerrainClass::Ocean) | None) {
                    continue;
                }
                queue.push_back(next);
                if view.hostile_at(next).is_some() || view.has_lair(next) || view.in_node(next) {
                    continue;
                }
                for sy in -1i32..=1 {
                    for sx in -1i32..=1 {
                        let Some(sea) = view.shift(next, sx, sy) else {
                            continue;
                        };
                        if view.terrain_class(sea) == Some(TerrainClass::Ocean)
                            && view.hostile_at(sea).is_none()
                        {
                            return Some((next, sea));
                        }
                    }
                }
            }
        }
    }
    None
}
