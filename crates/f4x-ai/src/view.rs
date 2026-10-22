//! How the AI weighs what it sees: strengths and who can fight. It reads the game only through
//! a [`PlayerView`], which holds what a player may know.

use f4x_core::content::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::rules::{MovementType, Stat};

/// How hard a unit hits and how much it can take, as one number.
pub fn unit_strength(view: &PlayerView, unit: hecs::Entity) -> f32 {
    let attack = view
        .unit_stat(unit, Stat::Melee)
        .max(view.unit_stat(unit, Stat::Ranged))
        .max(0) as f32;
    let defense = view.unit_stat(unit, Stat::Defense).max(0) as f32;
    let (hits, _) = view.unit_hits(unit);
    (attack * 1.5 + defense + 1.0) * hits as f32
}

pub fn stack_strength(view: &PlayerView, units: &[hecs::Entity]) -> f32 {
    units.iter().map(|&u| unit_strength(view, u)).sum()
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

/// A unit that can fight on land (settlers, ships and the like are not counted).
pub fn is_fighter(view: &PlayerView, unit: hecs::Entity) -> bool {
    !view.unit_has_ability(unit, "create_outpost")
        && !is_ship(view, unit)
        && view
            .unit_stat(unit, Stat::Melee)
            .max(view.unit_stat(unit, Stat::Ranged))
            > 0
}

/// King-move distance with the horizontal wrap; `None` across planes.
pub fn distance(view: &PlayerView, a: Position, b: Position) -> Option<u32> {
    view.distance(a, b)
}

/// The nearest square this wizard has not explored that the unit can walk to, looking outwards
/// from where it stands (the rings are walked in steps of two, as the army's scouts do). Of
/// the squares of the nearest ring that can be reached the last one in scan order is taken.
pub fn nearest_unexplored(view: &PlayerView, unit: hecs::Entity) -> Option<Position> {
    let from = view.unit_position(unit)?;
    let reachable = view.reachable(&[unit]);
    for radius in 3i32..=14 {
        let mut found = None;
        for dy in (-radius..=radius).step_by(2) {
            for dx in (-radius..=radius).step_by(2) {
                if dx.abs().max(dy.abs()) != radius {
                    continue;
                }
                if let Some(p) = view.shift(from, dx, dy) {
                    if !view.explored(p) && reachable.contains(&p) {
                        found = Some(p);
                    }
                }
            }
        }
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Fighters a city keeps at home: the fortress city holds two, every other city one.
pub fn garrison_wanted(view: &PlayerView, at: Position) -> usize {
    if view.fortress() == Some(at) {
        2
    } else {
        1
    }
}
