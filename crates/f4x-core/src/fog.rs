//! What each player has explored. Visibility is derived on demand from where their
//! units and cities stand; exploration is remembered.

use std::collections::BTreeMap;

use crate::city::{PlayerId, Position};
use crate::map::Plane;

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

    /// Dimensions and each player's explored flags, for saving.
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

    /// Mark every tile within `radius` (square, wrapping horizontally) as explored.
    pub fn explore(&mut self, player: PlayerId, at: Position, radius: u32) {
        if self.width == 0 {
            return;
        }
        let (w, h) = (self.width as i32, self.height as i32);
        let layer = self.layer(player, at.plane);
        let r = radius as i32;
        for dy in -r..=r {
            let y = at.y as i32 + dy;
            if !(0..h).contains(&y) {
                continue;
            }
            for dx in -r..=r {
                let x = (at.x as i32 + dx).rem_euclid(w);
                layer[(y * w + x) as usize] = true;
            }
        }
    }

    pub fn is_explored(&self, player: PlayerId, plane: Plane, x: u32, y: u32) -> bool {
        self.explored
            .get(&player)
            .is_some_and(|p| p[plane.index()][(y * self.width + x) as usize])
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
        fog.explore(p, pos(0, 0), 1);
        // rows -1 is off the map; rows 0..=1, columns 9, 0, 1
        assert_eq!(fog.explored_count(p, Plane::Arcanus), 6);
        assert!(fog.is_explored(p, Plane::Arcanus, 9, 1));
        assert!(!fog.is_explored(p, Plane::Arcanus, 2, 0));
        assert!(!fog.is_explored(p, Plane::Myrror, 0, 0));
        assert!(!fog.is_explored(PlayerId(1), Plane::Arcanus, 0, 0));
    }
}
