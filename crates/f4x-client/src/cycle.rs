//! The original's palette cycling (`Cycle_Palette_Color` in `Fonts.c`): one palette entry, index
//! 198, slides back and forth between two colours, so everything drawn in it pulses at once.
//! `extract-art` writes the pixels that use the index as a `_cycle` mask beside each picture;
//! the client tints the mask with [`Cycle::colour`].

use macroquad::prelude::*;

/// How often the original calls `Cycle_Palette_Color`, a call a frame.
const CALLS_PER_SECOND: f64 = 20.0;

/// The two colours (6-bit VGA values) and the step of one call.
#[derive(Debug, Clone, Copy)]
pub struct Cycle {
    min: [i32; 3],
    max: [i32; 3],
    step: i32,
}

/// Dark to bright red: the main screen (`MainScr.c`), where a negative income pulses.
pub const MAIN: Cycle = Cycle {
    min: [40, 0, 0],
    max: [63, 0, 0],
    step: 3,
};

/// The small spell book (`SmlBook_Draw`): the spell being cast, in pale greys.
pub const SMALL_BOOK: Cycle = Cycle {
    min: [41, 33, 29],
    max: [48, 43, 43],
    step: 3,
};

/// The big spell book (`BigBook_Draw`): the spell being researched, in pale blues.
pub const BIG_BOOK: Cycle = Cycle {
    min: [29, 41, 63],
    max: [44, 56, 63],
    step: 2,
};

/// The cycle the original runs while it shows a picture of this `.lbx` file: the start menu's and
/// pictures pulse; the other screens that carry index 198 (the load screen, whose palette
/// `LoadScr.c` loads without a cycle and only restarts the cycle on leaving it, the intro, the
/// wizards' portraits, the end animations) keep its base colour.
pub fn for_file(lbx: &str) -> Option<Cycle> {
    match lbx {
        "mainscrn" => Some(MAIN),
        _ => None,
    }
}

impl Cycle {
    /// The colour `time` seconds in: the primary channel (the one that changes most) goes up
    /// and down by `step` a call, the others follow it in proportion.
    pub fn colour(self, time: f64) -> Color {
        let span = |c: usize| (self.max[c] - self.min[c]).abs();
        let primary = (0..3).max_by_key(|&c| span(c)).unwrap_or(0);
        let length = f64::from(span(primary).max(1));
        let travelled = (time * CALLS_PER_SECOND * f64::from(self.step)).rem_euclid(2.0 * length);
        let along = if travelled < length {
            travelled
        } else {
            2.0 * length - travelled
        } / length;
        let channel = |c: usize| {
            let v = f64::from(self.min[c]) + f64::from(self.max[c] - self.min[c]) * along;
            (v / 63.0) as f32
        };
        Color::new(channel(0), channel(1), channel(2), 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_colour_moves_between_the_two_ends_and_back() {
        let start = MAIN.colour(0.0);
        assert!((start.r - 40.0 / 63.0).abs() < 1e-6 && start.g == 0.0);
        // 23 units at 60 a second: the top is reached after 23/60 s, then it comes back.
        let top = MAIN.colour(23.0 / 60.0);
        assert!((top.r - 1.0).abs() < 1e-6);
        let back = MAIN.colour(46.0 / 60.0);
        assert!((back.r - start.r).abs() < 1e-6);
        assert!(MAIN.colour(0.2).r >= start.r);
    }
}
