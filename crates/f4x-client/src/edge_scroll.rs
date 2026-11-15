//! Edge scrolling (patch 1.60, a preference): with the pointer on the edge of the window the
//! overland map scrolls that way, after the pointer has stayed there for a moment so that a
//! pointer passing the edge does not move the map.
//!
//! Tests: below.

use macroquad::prelude::{vec2, Vec2};

/// How close to the window's edge the pointer is on it, in window pixels.
const BAND: f32 = 2.0;
/// How long the pointer stays on the edge before the map starts to scroll, in seconds.
pub(crate) const DELAY: f32 = 0.4;

/// Which way the edge the pointer is on points: (-1, 0) the left edge, (1, 1) the bottom right
/// corner; zero when it is not on an edge, or outside the window.
pub(crate) fn direction(mouse: (f32, f32), size: (f32, f32)) -> Vec2 {
    let (x, y) = mouse;
    let (w, h) = size;
    if x < 0.0 || y < 0.0 || x >= w || y >= h {
        return Vec2::ZERO;
    }
    let axis = |at: f32, len: f32| {
        if at < BAND {
            -1.0
        } else if at >= len - BAND {
            1.0
        } else {
            0.0
        }
    };
    vec2(axis(x, w), axis(y, h))
}

/// How long the pointer has stayed on an edge.
#[derive(Debug, Default)]
pub(crate) struct EdgeScroll {
    waited: f32,
}

impl EdgeScroll {
    /// The scroll for a frame of `dt` seconds with the pointer pointing `dir`: nothing until the
    /// delay is over, then `dir` (the caller sets the speed).
    pub(crate) fn step(&mut self, dir: Vec2, dt: f32) -> Vec2 {
        if dir == Vec2::ZERO {
            self.waited = 0.0;
            return Vec2::ZERO;
        }
        self.waited += dt;
        if self.waited >= DELAY {
            dir
        } else {
            Vec2::ZERO
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pointer_on_an_edge_or_corner_points_that_way() {
        let size = (800.0, 600.0);
        assert_eq!(direction((0.0, 300.0), size), vec2(-1.0, 0.0));
        assert_eq!(direction((799.0, 300.0), size), vec2(1.0, 0.0));
        assert_eq!(direction((400.0, 0.0), size), vec2(0.0, -1.0));
        assert_eq!(direction((799.0, 599.0), size), vec2(1.0, 1.0));
        assert_eq!(direction((400.0, 300.0), size), Vec2::ZERO);
        assert_eq!(direction((-1.0, 300.0), size), Vec2::ZERO, "outside");
        assert_eq!(direction((900.0, 300.0), size), Vec2::ZERO, "outside");
    }

    #[test]
    fn the_map_scrolls_only_after_the_delay_and_leaving_the_edge_starts_it_again() {
        let mut edge = EdgeScroll::default();
        let left = vec2(-1.0, 0.0);
        assert_eq!(edge.step(left, DELAY / 2.0), Vec2::ZERO);
        assert_eq!(edge.step(left, DELAY / 2.0), left);
        assert_eq!(edge.step(left, 0.01), left);
        assert_eq!(edge.step(Vec2::ZERO, 0.01), Vec2::ZERO);
        assert_eq!(edge.step(left, DELAY / 2.0), Vec2::ZERO, "waits again");
    }
}
