//! What the AI knows: its own things, and enemy things on squares it can see.

use f4x_core::city::{PlayerId, Position};
use f4x_core::map::Plane;
use f4x_core::rules::Stat;
use f4x_core::Game;

/// The squares a player can see right now, for both planes.
pub struct Sight {
    width: u32,
    seen: [Vec<bool>; 2],
}

impl Sight {
    pub fn of(game: &Game, player: PlayerId) -> Self {
        Self {
            width: game.map.as_ref().map_or(0, |m| m.width()),
            seen: [
                game.visibility(player, Plane::Arcanus),
                game.visibility(player, Plane::Myrror),
            ],
        }
    }

    pub fn sees(&self, p: Position) -> bool {
        self.seen[p.plane.index()]
            .get((p.y * self.width + p.x) as usize)
            .copied()
            .unwrap_or(false)
    }
}

/// How hard a unit hits and how much it can take, as one number.
pub fn unit_strength(game: &Game, unit: hecs::Entity) -> f32 {
    let attack = game
        .unit_stat(unit, Stat::Melee)
        .max(game.unit_stat(unit, Stat::Ranged))
        .max(0) as f32;
    let defense = game.unit_stat(unit, Stat::Defense).max(0) as f32;
    let (hits, _) = game.unit_hits(unit);
    (attack * 1.5 + defense + 1.0) * hits as f32
}

pub fn stack_strength(game: &Game, units: &[hecs::Entity]) -> f32 {
    units.iter().map(|&u| unit_strength(game, u)).sum()
}

/// A unit that can fight (settlers and the like cannot).
pub fn is_fighter(game: &Game, unit: hecs::Entity) -> bool {
    !game.unit_has_ability(unit, "create_outpost")
        && game
            .unit_stat(unit, Stat::Melee)
            .max(game.unit_stat(unit, Stat::Ranged))
            > 0
}

/// King-move distance with the horizontal wrap; `None` across planes.
pub fn distance(game: &Game, a: Position, b: Position) -> Option<u32> {
    if a.plane != b.plane {
        return None;
    }
    let map = game.map.as_ref()?;
    Some(map.plane(a.plane).chebyshev((a.x, a.y), (b.x, b.y)))
}
