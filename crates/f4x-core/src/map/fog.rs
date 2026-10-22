//! What each player has explored, as the original keeps it: four corner bits per square.
//! Visibility is derived on demand from where units and cities stand; exploration is
//! remembered, for every player (the AI's too) and plane apart.

use std::collections::BTreeMap;

use crate::map::{Plane, Position};
use crate::player::PlayerId;

const BOTTOM_LEFT: u8 = 1;
const TOP_LEFT: u8 = 2;
const TOP_RIGHT: u8 = 4;
const BOTTOM_RIGHT: u8 = 8;
const ALL: u8 = 15;

/// The three neighbours (by `dx, dy`) that meet an explored square at each of its corners.
const AT_CORNER: [(u8, [(i32, i32); 3]); 4] = [
    (BOTTOM_LEFT, [(-1, 0), (-1, 1), (0, 1)]),
    (TOP_LEFT, [(-1, 0), (-1, -1), (0, -1)]),
    (TOP_RIGHT, [(0, -1), (1, -1), (1, 0)]),
    (BOTTOM_RIGHT, [(1, 0), (1, 1), (0, 1)]),
];

/// Sight, like movement, is counted in half-squares: a step along a row or column costs 2, a
/// diagonal step 3, and a square is in sight when its cost is at most the sight. So sight 3 is
/// the tile and the eight round it, sight 5 a 5 by 5 block without its corners.
///
/// The sight that shows `squares` squares along a row or column and the diagonal step after
/// it (the original's scouting level).
pub const fn sight_for(squares: u32) -> u32 {
    2 * squares + 1
}

/// The most squares a `sight` shows along a row or column.
pub const fn squares_seen(sight: u32) -> u32 {
    sight / 2
}

/// Whether a square `dx, dy` away is within a `sight`, in half-squares.
pub fn in_sight(dx: i32, dy: i32, sight: i32) -> bool {
    let (long, short) = (dx.abs().max(dy.abs()), dx.abs().min(dy.abs()));
    2 * long + short <= sight
}

/// Whether a square `dx, dy` away is within `radius`, where a step along a row or column counts 1
/// and a diagonal step 1.5 (so radius 2.5 is a 5 by 5 block without its corners).
pub fn in_radius(dx: i32, dy: i32, radius: f32) -> bool {
    let (long, short) = (dx.abs().max(dy.abs()), dx.abs().min(dy.abs()));
    (2 * long + short) as f32 <= 2.0 * radius
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fog {
    width: u32,
    height: u32,
    explored: BTreeMap<PlayerId, [Vec<bool>; 2]>,
}

impl Fog {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            explored: BTreeMap::new(),
        }
    }

    /// Dimensions and each player's explored flags (Arcanus, Myrror), for saving.
    #[allow(clippy::type_complexity)]
    pub fn to_parts(&self) -> (u32, u32, Vec<(PlayerId, [Vec<bool>; 2])>) {
        let players = self.explored.iter().map(|(p, l)| (*p, l.clone())).collect();
        (self.width, self.height, players)
    }

    /// Rebuild from saved parts. Returns `None` if a layer has the wrong length.
    pub fn from_parts(
        width: u32,
        height: u32,
        players: Vec<(PlayerId, [Vec<bool>; 2])>,
    ) -> Option<Self> {
        let len = (width as usize).checked_mul(height as usize)?;
        if players
            .iter()
            .any(|(_, l)| l[0].len() != len || l[1].len() != len)
        {
            return None;
        }
        Some(Self {
            width,
            height,
            explored: players.into_iter().collect(),
        })
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0
    }

    fn layer(&mut self, player: PlayerId, plane: Plane) -> &mut Vec<bool> {
        let len = (self.width * self.height) as usize;
        &mut self
            .explored
            .entry(player)
            .or_insert_with(|| [vec![false; len], vec![false; len]])[plane.index()]
    }

    /// Mark every tile within `sight` (half-squares, see `in_sight`, wrapping horizontally) as
    /// explored.
    pub fn explore(&mut self, player: PlayerId, at: Position, sight: u32) {
        if self.width == 0 {
            return;
        }
        let (w, h) = (self.width as i32, self.height as i32);
        let layer = self.layer(player, at.plane);
        let r = squares_seen(sight) as i32;
        for dy in -r..=r {
            let y = at.y as i32 + dy;
            if !(0..h).contains(&y) {
                continue;
            }
            for dx in (-r..=r).filter(|&dx| in_sight(dx, dy, sight as i32)) {
                let x = (at.x as i32 + dx).rem_euclid(w);
                layer[(y * w + x) as usize] = true;
            }
        }
    }

    /// Mark every tile within `radius` (see [`in_radius`]) as explored.
    pub fn explore_within(&mut self, player: PlayerId, at: Position, radius: f32) {
        if self.width == 0 {
            return;
        }
        let (w, h) = (self.width as i32, self.height as i32);
        let layer = self.layer(player, at.plane);
        let r = radius.ceil() as i32;
        for dy in -r..=r {
            let y = at.y as i32 + dy;
            if !(0..h).contains(&y) {
                continue;
            }
            for dx in (-r..=r).filter(|&dx| in_radius(dx, dy, radius)) {
                let x = (at.x as i32 + dx).rem_euclid(w);
                layer[(y * w + x) as usize] = true;
            }
        }
    }

    /// Forget everything `player` has explored.
    pub fn forget(&mut self, player: PlayerId) {
        self.explored.remove(&player);
    }

    /// Mark a `width` by `height` block of tiles, centred as nearly as it can on `at`
    /// (wrapping horizontally, clipped vertically), as explored.
    pub fn explore_block(&mut self, player: PlayerId, at: Position, width: u32, height: u32) {
        if self.width == 0 {
            return;
        }
        let (w, h) = (self.width as i32, self.height as i32);
        let layer = self.layer(player, at.plane);
        let (left, top) = (
            at.x as i32 - width as i32 / 2,
            at.y as i32 - height as i32 / 2,
        );
        for dy in 0..height as i32 {
            let y = top + dy;
            if !(0..h).contains(&y) {
                continue;
            }
            for dx in 0..width as i32 {
                layer[(y * w + (left + dx).rem_euclid(w)) as usize] = true;
            }
        }
    }

    pub fn is_explored(&self, player: PlayerId, plane: Plane, x: u32, y: u32) -> bool {
        self.explored
            .get(&player)
            .is_some_and(|p| p[plane.index()][(y * self.width + x) as usize])
    }

    /// How an explored square is drawn: the corners that are not shaded (bottom left 1, top
    /// left 2, top right 4, bottom right 8). A corner is shaded when a neighbour that meets
    /// it is unexplored, so the border of the known land is drawn on its last squares. 15
    /// when no corner is shaded; 0 for a square that is not explored.
    pub fn corners(&self, player: PlayerId, plane: Plane, x: u32, y: u32) -> u8 {
        if !self.is_explored(player, plane, x, y) {
            return 0;
        }
        let (w, h) = (self.width as i32, self.height as i32);
        let unexplored = |dx: i32, dy: i32| {
            let ny = y as i32 + dy;
            (0..h).contains(&ny)
                && !self.is_explored(
                    player,
                    plane,
                    (x as i32 + dx).rem_euclid(w) as u32,
                    ny as u32,
                )
        };
        let lit = AT_CORNER
            .iter()
            .filter(|(_, near)| !near.iter().any(|&(dx, dy)| unexplored(dx, dy)))
            .fold(0, |bits, (corner, _)| bits | corner);
        // A square with every corner shaded (alone in the dark) is drawn plain.
        if lit == 0 {
            ALL
        } else {
            lit
        }
    }

    pub fn explored_count(&self, player: PlayerId, plane: Plane) -> usize {
        self.explored
            .get(&player)
            .map_or(0, |p| p[plane.index()].iter().filter(|&&b| b).count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    #[test]
    fn explores_a_square_and_wraps() {
        let mut fog = Fog::new(10, 8);
        let p = PlayerId(0);
        fog.explore(p, pos(0, 0), 3);
        // rows -1 is off the map; rows 0..=1, columns 9, 0, 1
        assert_eq!(fog.explored_count(p, Plane::Arcanus), 6);
        assert!(fog.is_explored(p, Plane::Arcanus, 9, 1));
        assert!(!fog.is_explored(p, Plane::Arcanus, 2, 0));
        assert!(!fog.is_explored(p, Plane::Myrror, 0, 0));
        assert!(!fog.is_explored(PlayerId(1), Plane::Arcanus, 0, 0));
    }

    #[test]
    fn the_border_is_drawn_on_the_explored_squares_beside_unexplored_ones() {
        let mut fog = Fog::new(10, 8);
        let p = PlayerId(0);
        fog.explore(p, pos(4, 4), 3);
        let corners = |x, y| fog.corners(p, Plane::Arcanus, x, y);
        // The middle is clear; the edge squares are shaded on the side that faces the dark.
        assert_eq!(corners(4, 4), ALL);
        assert_eq!(corners(5, 4), BOTTOM_LEFT | TOP_LEFT);
        assert_eq!(corners(4, 3), BOTTOM_LEFT | BOTTOM_RIGHT);
        // A corner square keeps only the corner that faces the block.
        assert_eq!(corners(5, 3), BOTTOM_LEFT);
        assert_eq!(corners(3, 5), TOP_RIGHT);
        // Unexplored squares have no corners.
        assert_eq!(corners(6, 4), 0);
        // Across the wrap.
        let mut fog = Fog::new(10, 8);
        fog.explore(p, pos(0, 4), 3);
        assert_eq!(
            fog.corners(p, Plane::Arcanus, 9, 4),
            TOP_RIGHT | BOTTOM_RIGHT
        );
    }

    #[test]
    fn sight_is_in_half_squares_and_a_diagonal_step_costs_three() {
        let shape = |sight| {
            (-4..=4)
                .flat_map(|dy| (-4..=4).map(move |dx| (dx, dy)))
                .filter(|&(dx, dy)| in_sight(dx, dy, sight))
                .count()
        };
        assert_eq!(shape(1), 1);
        assert_eq!(shape(2), 5, "one square along a row or column only");
        assert_eq!(shape(3), 9, "the tile and the eight round it");
        // A 5 by 5 block without its four corners.
        assert_eq!(shape(5), 21);
        assert!(in_sight(2, 1, 5) && !in_sight(2, 2, 5));
        assert_eq!(shape(7), 37);
        assert_eq!(sight_for(2), 5);
        assert_eq!(squares_seen(sight_for(2)), 2);
    }

    #[test]
    fn every_player_has_an_exploration_of_their_own() {
        let mut fog = Fog::new(10, 8);
        fog.explore(PlayerId(0), pos(2, 2), 5);
        fog.explore(PlayerId(1), pos(7, 4), 3);
        assert!(fog.is_explored(PlayerId(0), Plane::Arcanus, 2, 2));
        assert!(!fog.is_explored(PlayerId(1), Plane::Arcanus, 2, 2));
        assert!(fog.is_explored(PlayerId(1), Plane::Arcanus, 7, 4));
        assert!(!fog.is_explored(PlayerId(0), Plane::Arcanus, 7, 4));
        assert_eq!(fog.explored_count(PlayerId(1), Plane::Arcanus), 9);
    }
}
