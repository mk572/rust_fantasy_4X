//! The size of the picture the game is laid out in, and the scale of the art drawn into it.
//!
//! Every screen is laid out on the original's grid of 320 by 200 squares (the "design size"
//! below); `Layout` maps that grid onto the window. The art can be finer than the grid: at art
//! scale 2 a picture is 640 by 400 pixels for the same 320 by 200 squares, at 4 it is 1280 by 800.
//! A picture's size on the grid is therefore its pixel size divided by the art scale
//! ([`logical_size`]); positions never change.
//!
//! The art scale is a preference (`art_scale` in `saves/preferences.txt`, task 393).

use std::sync::atomic::{AtomicU8, Ordering};

use macroquad::prelude::*;

/// The width of the design grid, in squares.
pub const DESIGN_W: f32 = 320.0;
/// The height of the design grid, in squares.
pub const DESIGN_H: f32 = 200.0;
/// The whole design grid as a rectangle `(x, y, w, h)`.
pub const SCREEN: (f32, f32, f32, f32) = (0.0, 0.0, DESIGN_W, DESIGN_H);

/// The art scales the game can be asked for.
pub const ART_SCALES: [u8; 3] = [1, 2, 4];

static ART_SCALE: AtomicU8 = AtomicU8::new(1);

/// The art scale in use: 1, 2 or 4.
pub fn art_scale() -> u8 {
    ART_SCALE.load(Ordering::Relaxed)
}

/// Use `scale` for the art; one that is not in [`ART_SCALES`] becomes 1. Returns what is in use.
pub fn set_art_scale(scale: u8) -> u8 {
    let scale = if ART_SCALES.contains(&scale) {
        scale
    } else {
        1
    };
    ART_SCALE.store(scale, Ordering::Relaxed);
    scale
}

/// The size of a picture on the design grid.
pub fn logical_size(tex: &Texture2D) -> Vec2 {
    tex.size() / f32::from(art_scale())
}

/// The width of a picture on the design grid.
pub fn logical_w(tex: &Texture2D) -> f32 {
    logical_size(tex).x
}

/// The height of a picture on the design grid.
pub fn logical_h(tex: &Texture2D) -> f32 {
    logical_size(tex).y
}

/// A part of a picture, given on the design grid, as the pixels of the texture (what a
/// `DrawTextureParams::source` wants).
pub fn pixel_rect(rect: Rect) -> Rect {
    let k = f32::from(art_scale());
    Rect::new(rect.x * k, rect.y * k, rect.w * k, rect.h * k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_known_scales_are_taken() {
        assert_eq!(set_art_scale(2), 2);
        assert_eq!(art_scale(), 2);
        assert_eq!(set_art_scale(3), 1);
        assert_eq!(set_art_scale(4), 4);
        assert_eq!(set_art_scale(0), 1);
        assert_eq!(art_scale(), 1);
    }

    #[test]
    fn a_part_of_a_picture_is_given_in_texture_pixels() {
        set_art_scale(2);
        assert_eq!(
            pixel_rect(Rect::new(1.0, 2.0, 3.0, 4.0)),
            Rect::new(2.0, 4.0, 6.0, 8.0)
        );
        set_art_scale(1);
    }
}
