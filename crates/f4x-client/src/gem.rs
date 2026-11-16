//! The wizards' gems (task 570): a wizard's portrait in a diamond of its banner colour, put together
//! when it is first drawn, so a new portrait needs one picture, not one for each colour.
//!
//! The parts: the five diamonds of the original (`assets/gem/frame.sheet.png`, a sprite sheet of
//! `f4x_extract::pack`; no portrait in them), the five newer colours made from them by turning the
//! hue (the turns of `make_banner_colours.py`), and the portrait of the wizard (33 x 35, rounded
//! corners; `art/wizards/<id>/face_00.png`, the original's `magic` 18 to 31 for the ready-made
//! wizards).
//!
//! The portrait goes on the diamond at the place the wizard's table gives (`WizardPictures::face_origin`).
//! The original blends the portrait's outer three rows and columns into the frame; so does this: the
//! weights ([`RAMP`]) are what the original's gems show, measured over six wizards and five colours
//! (`docs/extract-art.md`), the pixels that are only partly shown take the hue of the diamond under
//! them (the original's grey border would show as a line), and the middle third of the top and the
//! bottom is shown fuller ([`MIDDLE`]), so the top of the head is not covered by the frame.
//!
//! Tests: the pack, the turned frames and the composition on a made-up portrait, in this file.

use std::sync::OnceLock;

use f4x_extract::pack::Sheet;
use macroquad::prelude::{Image, ImageFormat};

/// The size of a gem.
pub const WIDTH: usize = 41;
pub const HEIGHT: usize = 43;
/// The size of a portrait.
pub const FACE_WIDTH: usize = 33;
pub const FACE_HEIGHT: usize = 35;

/// The pack of the five original diamonds, in the order of the banners.
const FRAME_PACK: &[u8] = include_bytes!("../assets/gem/frame.sheet.png");
const ORIGINAL_FRAMES: usize = 5;

/// The newer banner colours (5 to 9): the original colour each is made from, the turn in degrees
/// and the brightness (`make_banner_colours.py`: pink, brown, turquoise, dark blue, orange).
const TURNS: [(usize, f32, f32); 5] = [
    (1, 210.0, 1.0),
    (0, 150.0, 0.78),
    (3, 180.0, 1.0),
    (2, 285.0, 0.8),
    (4, -18.0, 1.0),
];

/// How much of the portrait shows 0, 1 and 2 pixels from its edge (from 3 pixels in, all of it),
/// at the left, right, top and bottom edge: what the original's gems show.
const RAMP_SIDE: [f32; 3] = [0.1, 0.45, 0.8];
const RAMP_TOP: [f32; 3] = [0.0, 0.6, 0.85];
const RAMP_BOTTOM: [f32; 3] = [0.1, 0.4, 0.7];
/// The same three rows in the middle third of the top and the bottom (the top of the head), eased
/// in over [`EASE`] pixels on each side of the middle third.
const MIDDLE: [f32; 3] = [0.5, 1.0, 1.0];
const EASE: f32 = 3.0;

/// The five original diamonds, decoded once.
fn originals() -> &'static [Image] {
    static FRAMES: OnceLock<Vec<Image>> = OnceLock::new();
    FRAMES.get_or_init(|| {
        let sheet = Image::from_file_with_format(FRAME_PACK, Some(ImageFormat::Png)).ok();
        let grid = Sheet::parse(FRAME_PACK).ok();
        let (Some(sheet), Some(grid)) = (sheet, grid) else {
            return Vec::new();
        };
        (0..ORIGINAL_FRAMES)
            .filter_map(|i| {
                let (x, y, w, h) = grid.rect(i)?;
                Some(crop(&sheet, x, y, w, h))
            })
            .collect()
    })
}

fn crop(image: &Image, x: usize, y: usize, w: usize, h: usize) -> Image {
    let stride = usize::from(image.width) * 4;
    let mut bytes = Vec::with_capacity(w * h * 4);
    for row in y..y + h {
        let from = row * stride + x * 4;
        bytes.extend_from_slice(&image.bytes[from..from + w * 4]);
    }
    Image {
        bytes,
        width: w as u16,
        height: h as u16,
    }
}

/// An image that was enlarged by `by` (each pixel `by` x `by`), back at its own size.
pub fn shrunk(image: &Image, by: usize) -> Image {
    if by <= 1 {
        return image.clone();
    }
    let (w, h) = (
        usize::from(image.width) / by,
        usize::from(image.height) / by,
    );
    let stride = usize::from(image.width) * 4;
    let mut bytes = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let from = y * by * stride + x * by * 4;
            bytes.extend_from_slice(&image.bytes[from..from + 4]);
        }
    }
    Image {
        bytes,
        width: w as u16,
        height: h as u16,
    }
}

/// The diamond of a banner colour: the original's for 0 to 4, the original's turned in hue for 5 to
/// 9, `None` for the neutral banner (it has no gem) or when the pack is missing.
pub fn frame(banner: u8) -> Option<Image> {
    let banner = usize::from(banner);
    if banner < ORIGINAL_FRAMES {
        return originals().get(banner).cloned();
    }
    let (from, degrees, value) = *TURNS.get(banner - ORIGINAL_FRAMES)?;
    let mut image = originals().get(from)?.clone();
    for pixel in image.bytes.as_chunks_mut::<4>().0.iter_mut() {
        if pixel[3] != 0 {
            let [r, g, b] = turn([pixel[0], pixel[1], pixel[2]], degrees, value);
            pixel[..3].copy_from_slice(&[r, g, b]);
        }
    }
    Some(image)
}

/// A colour with its hue turned by `degrees` and its brightness times `value` (at most full).
fn turn(rgb: [u8; 3], degrees: f32, value: f32) -> [u8; 3] {
    let (h, s, v) = rgb_to_hsv(rgb);
    hsv_to_rgb((
        (h + degrees / 360.0).rem_euclid(1.0),
        s,
        (v * value).min(1.0),
    ))
}

fn rgb_to_hsv([r, g, b]: [u8; 3]) -> (f32, f32, f32) {
    let (r, g, b) = (
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    );
    let top = r.max(g).max(b);
    let range = top - r.min(g).min(b);
    if range == 0.0 {
        return (0.0, 0.0, top);
    }
    let hue = if top == r {
        ((g - b) / range).rem_euclid(6.0)
    } else if top == g {
        (b - r) / range + 2.0
    } else {
        (r - g) / range + 4.0
    };
    (hue / 6.0, range / top, top)
}

fn hsv_to_rgb((h, s, v): (f32, f32, f32)) -> [u8; 3] {
    let sector = (h * 6.0).floor();
    let f = h * 6.0 - sector;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match sector as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [r, g, b].map(|c| (c * 255.0).round() as u8)
}

/// How much of the portrait shows at `distance` pixels from an edge with this ramp; the middle third
/// of the top and the bottom is fuller (`across`: the pixel's column).
fn ramp(ramp: &[f32; 3], distance: usize, across: Option<usize>) -> f32 {
    let Some(&weight) = ramp.get(distance) else {
        return 1.0;
    };
    match across {
        Some(x) => {
            let third = FACE_WIDTH as f32 / 3.0;
            let x = x as f32;
            let outside = (third - x)
                .max(x - (FACE_WIDTH as f32 - 1.0 - third))
                .max(0.0);
            let middle = 1.0 - (outside / EASE).min(1.0);
            weight + (MIDDLE[distance] - weight) * middle
        }
        None => weight,
    }
}

/// How much of the portrait shows at its pixel `(x, y)`: the least of the four edges, so the
/// corners show less than the sides.
fn shown(x: usize, y: usize) -> f32 {
    ramp(&RAMP_SIDE, x, None)
        .min(ramp(&RAMP_SIDE, FACE_WIDTH - 1 - x, None))
        .min(ramp(&RAMP_TOP, y, Some(x)))
        .min(ramp(&RAMP_BOTTOM, FACE_HEIGHT - 1 - y, Some(x)))
}

fn mix(under: u8, over: u8, weight: f32) -> u8 {
    (f32::from(under) * (1.0 - weight) + f32::from(over) * weight).round() as u8
}

/// The diamond with the portrait on it, its top left corner `at` pixels from the diamond's. A
/// portrait pixel that is transparent leaves the diamond as it is; one that is only partly shown is
/// turned to the diamond's hue by the share that is not shown, so no grey edge is left.
pub fn compose(frame: &Image, face: &Image, at: (usize, usize)) -> Image {
    let mut gem = frame.clone();
    let (frame_w, frame_h) = (usize::from(frame.width), usize::from(frame.height));
    let face_w = usize::from(face.width);
    for y in 0..usize::from(face.height) {
        for x in 0..face_w {
            let from = (y * face_w + x) * 4;
            let pixel = &face.bytes[from..from + 4];
            let (gx, gy) = (x + at.0, y + at.1);
            if pixel[3] == 0 || gx >= frame_w || gy >= frame_h {
                continue;
            }
            let to = (gy * frame_w + gx) * 4;
            let weight =
                shown(x.min(FACE_WIDTH - 1), y.min(FACE_HEIGHT - 1)) * f32::from(pixel[3]) / 255.0;
            let under = [gem.bytes[to], gem.bytes[to + 1], gem.bytes[to + 2]];
            // The portrait's pixel with the hue of the diamond's, by the share that is covered.
            let (_, saturation, value) = rgb_to_hsv([pixel[0], pixel[1], pixel[2]]);
            let (hue, _, _) = rgb_to_hsv(under);
            let turned = hsv_to_rgb((hue, saturation, value));
            for c in 0..3 {
                let own = mix(pixel[c], turned[c], 1.0 - weight);
                gem.bytes[to + c] = mix(under[c], own, weight);
            }
            gem.bytes[to + 3] = 255;
        }
    }
    gem
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A portrait of one colour with the corners cut, like the original's.
    fn portrait(colour: [u8; 3]) -> Image {
        let mut bytes = Vec::new();
        for y in 0..FACE_HEIGHT {
            for x in 0..FACE_WIDTH {
                let corner = (x + y < 2)
                    || (FACE_WIDTH - 1 - x + y < 2)
                    || (x + FACE_HEIGHT - 1 - y < 2)
                    || (FACE_WIDTH - 1 - x + FACE_HEIGHT - 1 - y < 2);
                let alpha = if corner { 0 } else { 255 };
                bytes.extend_from_slice(&[colour[0], colour[1], colour[2], alpha]);
            }
        }
        Image {
            bytes,
            width: FACE_WIDTH as u16,
            height: FACE_HEIGHT as u16,
        }
    }

    fn at(image: &Image, x: usize, y: usize) -> [u8; 4] {
        let from = (y * usize::from(image.width) + x) * 4;
        [
            image.bytes[from],
            image.bytes[from + 1],
            image.bytes[from + 2],
            image.bytes[from + 3],
        ]
    }

    #[test]
    fn the_pack_has_the_five_diamonds_and_the_turned_ones_differ_in_hue_only() {
        for banner in 0..10 {
            let frame = frame(banner).unwrap_or_else(|| panic!("banner {banner} has a frame"));
            assert_eq!(
                (usize::from(frame.width), usize::from(frame.height)),
                (WIDTH, HEIGHT)
            );
            // The corners are cut: the gem is a diamond with rounded corners.
            assert_eq!(at(&frame, 0, 0)[3], 0);
            assert_eq!(at(&frame, 20, 21)[3], 255);
        }
        assert!(frame(10).is_none(), "the neutral banner has no gem");
        // Pink is the green one turned: the same shape, other colours.
        let (green, pink) = (frame(1).map(|f| f.bytes), frame(5).map(|f| f.bytes));
        assert_ne!(green, pink);
        let alpha = |bytes: &Option<Vec<u8>>| {
            bytes.as_ref().map(|b| {
                b.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| p[3])
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(alpha(&green), alpha(&pink));
    }

    #[test]
    fn a_turn_of_the_hue_keeps_grey_and_black() {
        assert_eq!(turn([0, 0, 0], 210.0, 1.0), [0, 0, 0]);
        assert_eq!(turn([128, 128, 128], 150.0, 1.0), [128, 128, 128]);
        // Red turned by 120 degrees is green; the brightness only ever goes down to its factor.
        assert_eq!(turn([255, 0, 0], 120.0, 1.0), [0, 255, 0]);
        assert_eq!(turn([255, 0, 0], 0.0, 0.5), [128, 0, 0]);
    }

    #[test]
    fn the_inside_of_the_portrait_is_shown_whole_and_the_frame_outside_it_is_left_alone(
    ) -> Result<(), String> {
        let blue = frame(0).ok_or("a frame")?;
        let face = portrait([200, 120, 40]);
        let gem = compose(&blue, &face, (4, 5));
        // Inside, three or more pixels from the edge: the portrait's own colour.
        assert_eq!(at(&gem, 4 + 16, 5 + 17), [200, 120, 40, 255]);
        assert_eq!(at(&gem, 4 + 3, 5 + 3), [200, 120, 40, 255]);
        // Outside the portrait: the diamond, pixel for pixel.
        for (x, y) in [
            (0, 10),
            (2, 2),
            (WIDTH - 1, 20),
            (20, HEIGHT - 1),
            (3, 20),
            (4 + FACE_WIDTH, 20),
        ] {
            assert_eq!(at(&gem, x, y), at(&blue, x, y), "({x}, {y})");
        }
        // The portrait's cut corners leave the diamond.
        assert_eq!(at(&gem, 4, 5), at(&blue, 4, 5));
        Ok(())
    }

    #[test]
    fn the_edge_is_blended_in_the_diamonds_hue_and_the_top_of_the_head_is_fuller_in_the_middle(
    ) -> Result<(), String> {
        let blue = frame(0).ok_or("a frame")?;
        let face = portrait([90, 82, 78]);
        let gem = compose(&blue, &face, (4, 5));
        // The first column of the portrait shows a tenth of it; the colour is the diamond's with a
        // trace of the portrait, and no grey: the red and green stay near the diamond's.
        let (edge, under) = (at(&gem, 4, 5 + 17), at(&blue, 4, 5 + 17));
        for c in 0..3 {
            assert!(
                (i32::from(edge[c]) - i32::from(under[c])).abs() < 30,
                "channel {c}: {edge:?} {under:?}"
            );
        }
        // The second row at the top: 0.6 at the sides, the whole in the middle third.
        let side = at(&gem, 4 + 6, 5 + 1);
        let middle = at(&gem, 4 + 16, 5 + 1);
        let under_side = at(&blue, 4 + 6, 5 + 1);
        assert_ne!(side, under_side);
        assert_eq!(&middle[..3], &[90, 82, 78]);
        // The bottom row's middle is not covered as much as its sides.
        let (bottom_side, bottom_middle) = (at(&gem, 4 + 6, 5 + 34), at(&gem, 4 + 16, 5 + 34));
        let distance = |a: [u8; 4], b: [u8; 4]| {
            (0..3)
                .map(|c| (i32::from(a[c]) - i32::from(b[c])).abs())
                .sum::<i32>()
        };
        assert!(
            distance(bottom_middle, [90, 82, 78, 255]) < distance(bottom_side, [90, 82, 78, 255]),
            "{bottom_middle:?} {bottom_side:?}"
        );
        Ok(())
    }

    #[test]
    fn a_portrait_that_reaches_past_the_gem_is_cut_and_a_half_transparent_pixel_shows_less(
    ) -> Result<(), String> {
        let blue = frame(0).ok_or("a frame")?;
        let face = portrait([10, 200, 10]);
        // Moved so that it hangs out over the right and the bottom: no panic, the part inside is drawn.
        let gem = compose(&blue, &face, (20, 20));
        assert_eq!(at(&gem, 30, 30), [10, 200, 10, 255]);
        let mut soft = portrait([10, 200, 10]);
        let from = (17 * FACE_WIDTH + 16) * 4;
        soft.bytes[from + 3] = 128;
        let gem = compose(&blue, &soft, (4, 5));
        let (half, whole) = (at(&gem, 4 + 16, 5 + 17), at(&blue, 4 + 16, 5 + 17));
        assert_ne!(half[..3], [10, 200, 10]);
        assert_ne!(half[..3], whole[..3]);
        Ok(())
    }
}
