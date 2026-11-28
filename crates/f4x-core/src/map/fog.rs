//! What each player has explored: four corner bits per square. Visibility is derived on demand from
//! where units and cities stand; exploration is remembered, for every player (the AI's too) and
//! plane apart.

use std::collections::BTreeMap;

use crate::map::{Plane, Position};
use crate::player::PlayerId;
use f4x_def::half_int::HalfInt;

const BOTTOM_LEFT: u8 = 1;
const TOP_LEFT: u8 = 2;
const TOP_RIGHT: u8 = 4;
const BOTTOM_RIGHT: u8 = 8;
const ALL: u8 = 15;

/// The three neighbours (by `dx, dy`) that meet an explored square at each of its corners.
const AT_CORNER: &[(u8, [(i32, i32); 3])] = &[
    (BOTTOM_LEFT, [(-1, 0), (-1, 1), (0, 1)]),
    (TOP_LEFT, [(-1, 0), (-1, -1), (0, -1)]),
    (TOP_RIGHT, [(0, -1), (1, -1), (1, 0)]),
    (BOTTOM_RIGHT, [(1, 0), (1, 1), (0, 1)]),
];

/// Sight is a distance in squares, counted like movement: a step along a row or column costs 1, a
/// diagonal step 1.5 with the setting `diagonal_extra_cost` and 1 without it, and a square is in
/// sight when its cost is at most the sight. So sight 1.5 is the tile and the eight round it,
/// sight 2.5 a 5 by 5 block, without its corners when a diagonal step costs more.
///
/// The sight that shows `squares` squares along a row or column and the diagonal step after
/// it (the original's scouting level).
pub const fn sight_for(squares: u32) -> HalfInt {
    HalfInt::from_int(squares as i32).plus_half()
}

/// The most squares a `sight` shows along a row or column.
pub const fn squares_seen(sight: HalfInt) -> u32 {
    let squares = sight.floor();
    if squares < 0 {
        0
    } else {
        squares as u32
    }
}

/// What the squares `dx, dy` away cost to see: 1 for each step along the longer axis, and half
/// more for each diagonal step when it costs extra (`diagonal_extra_cost`).
fn sight_cost(dx: i32, dy: i32, diagonal_extra: bool) -> HalfInt {
    let (long, short) = (dx.abs().max(dy.abs()), dx.abs().min(dy.abs()));
    HalfInt::from_int(long) + HalfInt::HALF * if diagonal_extra { short } else { 0 }
}

/// Whether a square `dx, dy` away is within a `sight`.
pub fn in_sight(dx: i32, dy: i32, sight: HalfInt, diagonal_extra: bool) -> bool {
    sight_cost(dx, dy, diagonal_extra) <= sight
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

    /// Mark every tile within `sight` (see `in_sight`, wrapping horizontally) as
    /// explored.
    pub fn explore(
        &mut self,
        player: PlayerId,
        at: Position,
        sight: HalfInt,
        diagonal_extra: bool,
    ) {
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
            for dx in (-r..=r).filter(|&dx| in_sight(dx, dy, sight, diagonal_extra)) {
                let x = (at.x as i32 + dx).rem_euclid(w);
                layer[(y * w + x) as usize] = true;
            }
        }
    }

    /// Forget everything `player` has explored.
    pub fn forget(&mut self, player: PlayerId) {
        self.explored.remove(&player);
    }

    /// Each of the two learns every square the other has explored, on both planes.
    pub fn share(&mut self, a: PlayerId, b: PlayerId) {
        if self.width == 0 {
            return;
        }
        for plane in Plane::all() {
            let of_a = self.layer(a, plane).clone();
            let of_b = self.layer(b, plane);
            for (square, known) in of_b.iter_mut().zip(&of_a) {
                *square |= *known;
            }
            let both = of_b.clone();
            *self.layer(a, plane) = both;
        }
    }

    /// How many squares `from` has explored that `to` has not, on both planes.
    pub fn news(&self, from: PlayerId, to: PlayerId) -> usize {
        let Some(of_from) = self.explored.get(&from) else {
            return 0;
        };
        let of_to = self.explored.get(&to);
        Plane::all()
            .map(|plane| {
                let i = plane.index();
                of_from[i]
                    .iter()
                    .enumerate()
                    .filter(|&(square, &known)| known && !of_to.is_some_and(|l| l[i][square]))
                    .count()
            })
            .sum()
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
