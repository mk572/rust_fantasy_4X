//! Where a scout goes next: the square from which it will see the most new ground, for the walk
//! it costs. A scouting order chooses with this (`order.rs`), and so does a computer player.
//!
//! Tests: in `tests/it/c9_errand.rs`.

use crate::map::Position;
use crate::player::view::PlayerView;
use f4x_def::half_int::HalfInt;

/// How many squares of walking a square of new ground is worth.
const WALK_COST: f32 = 0.35;
/// Scouting targets this close to a square to be avoided are left alone.
pub const KEEP_APART: u32 = 5;
/// How near a favoured square new ground has to be to count for more.
const FAVOUR_NEAR: u32 = 3;
/// What new ground near a favoured square is worth on top.
const FAVOUR_BONUS: f32 = 20.0;

impl PlayerView<'_> {
    /// How many squares within the sight `look` of `at` the player has not explored.
    pub fn unexplored_near(&self, at: Position, look: HalfInt) -> u32 {
        let mut n = 0;
        let reach = crate::map::fog::squares_seen(look) as i32;
        for dy in -reach..=reach {
            for dx in (-reach..=reach)
                .filter(|&dx| crate::map::fog::in_sight(dx, dy, look, self.diagonal_extra_cost()))
            {
                if let Some(p) = self.shift(at, dx, dy) {
                    if !self.explored(p) {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    /// The square the stack can walk to that is worth most to scout: new ground within sight of
    /// it, less the walk from `from`. Squares near one of `avoid` are left out; new ground near
    /// one of `favour` is worth more. `None` when nothing it can reach shows anything new.
    pub fn scout_target(
        &self,
        units: &[hecs::Entity],
        from: Position,
        avoid: &[Position],
        favour: &[Position],
    ) -> Option<Position> {
        let near = |list: &[Position], p: Position, within: u32| {
            list.iter()
                .any(|&q| self.distance(q, p).is_some_and(|d| d < within))
        };
        // A unit shows the squares within its sight of where it stands, and no more.
        let look = units.iter().map(|&u| self.unit_sight(u)).max()?;
        let mut best: Option<(f32, Position)> = None;
        for &p in &self.reachable(units) {
            if p == from || near(avoid, p, KEEP_APART) {
                continue;
            }
            let fresh = self.unexplored_near(p, look);
            if fresh == 0 {
                continue;
            }
            let walk = self.distance(from, p).unwrap_or(99) as f32;
            let mut value = fresh as f32 - walk * WALK_COST * 4.0;
            if near(favour, p, FAVOUR_NEAR + 1) {
                value += FAVOUR_BONUS;
            }
            // A strict comparison with the position as a tie-break keeps the choice
            // deterministic whatever order the set is walked in.
            let better = match best {
                None => true,
                Some((v, b)) => value > v || (value == v && (p.y, p.x) < (b.y, b.x)),
            };
            if better {
                best = Some((value, p));
            }
        }
        best.map(|(_, p)| p)
    }
}
