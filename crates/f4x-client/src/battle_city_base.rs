//! The city's base picture turned with the battle view (task 732, `docs/combat.md`).
//!
//! `cmbtcity` 0 is one picture of 126x64 pixels laid over a block of 4x4 squares; its roads run
//! along the squares' edges, and two squares have a road through the middle. The picture cannot be
//! turned as a whole (an isometric picture does not turn by a quarter), so it is cut into its 16
//! squares and each square of a turned view gets the piece that has the turned square's road edges
//! and middle: the roads of the squares are the same strips whichever way they run, so every turn
//! has an exact twin, found among the 16 pieces and their mirror images.
//!
//! The block's middle is the pixel (64, 31) (one up and right of the picture's middle), which puts
//! every road pixel inside a square. Without the extracted pictures nothing is drawn turned.

/// The block's squares on a side.
const SIDE: i32 = 4;
/// The pixel the block is centred on, in the picture's own pixels.
const CENTRE: (i32, i32) = (64, 31);
/// The picture's size in its own pixels.
const SIZE: (i32, i32) = (126, 64);
/// The depth of an edge's band, as a part of the square, and how many parts an edge is read in.
const BAND: f64 = 0.25;
const PARTS: usize = 4;
/// How a piece is shown: as it is, mirrored left to right, top to bottom, or both.
const MIRRORS: [(bool, bool); 4] = [(false, false), (true, false), (false, true), (true, true)];

/// A square of the block: its column and row, 0 to 3.
type Square = (i32, i32);

/// Which of the four edges (north-east, south-east, south-west, north-west, in that order, each
/// in four parts) and whether the middle of a square has a road: bit `edge * 4 + part`, bit 16.
type Shape = u32;
const MIDDLE: Shape = 1 << 16;

/// The middle pixel of a square.
fn centre_of((i, j): Square) -> (i32, i32) {
    (CENTRE.0 + (i - j) * 16, CENTRE.1 + (i + j - SIDE + 1) * 8)
}

/// The square a pixel is in, or `None` outside the block.
fn square_at(x: i32, y: i32) -> Option<Square> {
    // Twice the pixel's middle, from the block's middle.
    let (dx, dy) = (2 * x + 1 - 2 * CENTRE.0, 2 * y + 1 - 2 * CENTRE.1);
    let (u, v) = (dx + 2 * dy + 128, 2 * dy - dx + 128);
    let (i, j) = (u.div_euclid(64), v.div_euclid(64));
    ((0..SIDE).contains(&i) && (0..SIDE).contains(&j) && u >= 0 && v >= 0).then_some((i, j))
}

/// A pixel's place in its square, 0 to 1 across (`u`, towards the south-east) and down (`v`,
/// towards the south-west), from its offset to the square's middle pixel.
fn place_in_square(offset: (i32, i32)) -> (f64, f64) {
    let (rx, ry) = (f64::from(offset.0) + 0.5, f64::from(offset.1) + 0.5);
    ((rx + 2.0 * ry) / 32.0 + 0.5, (2.0 * ry - rx) / 32.0 + 0.5)
}

/// The road pixels of a square, as offsets to its middle pixel.
fn shape_of(road: impl Iterator<Item = (i32, i32)>) -> Shape {
    let mut shape = 0;
    let part_bit = |edge: usize, along: f64| -> Shape {
        1 << (edge * PARTS + ((along * PARTS as f64) as usize).min(PARTS - 1))
    };
    for offset in road {
        let (u, v) = place_in_square(offset);
        if v < BAND {
            shape |= part_bit(0, u);
        }
        if u > 1.0 - BAND {
            shape |= part_bit(1, v);
        }
        if v > 1.0 - BAND {
            shape |= part_bit(2, 1.0 - u);
        }
        if u < BAND {
            shape |= part_bit(3, 1.0 - v);
        }
        if (u - 0.5).abs() < 0.2 && (v - 0.5).abs() < 0.2 {
            shape |= MIDDLE;
        }
    }
    shape
}

/// The shape after `quarters` clockwise quarter turns: each edge is the next one.
fn turned_shape(shape: Shape, quarters: u8) -> Shape {
    let edges = shape & !MIDDLE;
    let by = usize::from(quarters) * PARTS;
    let ring = (edges << by | edges >> (16 - by)) & 0xFFFF;
    ring | (shape & MIDDLE)
}

/// An offset to a square's middle pixel, mirrored.
fn mirrored(offset: (i32, i32), (across, down): (bool, bool)) -> (i32, i32) {
    (
        if across { -1 - offset.0 } else { offset.0 },
        if down { -1 - offset.1 } else { offset.1 },
    )
}

/// The square a square turns into when the view turns by `quarters`, going back: the square of the
/// unturned picture that lands on `square`.
fn square_before(mut square: Square, quarters: u8) -> Square {
    for _ in 0..quarters {
        square = (square.1, SIDE - 1 - square.0);
    }
    square
}

/// The piece for each square of the turned block: which square of the picture it is cut from and
/// how it is mirrored (index into `MIRRORS`).
pub(crate) fn pieces(
    road: &dyn Fn(i32, i32) -> bool,
    quarters: u8,
) -> Vec<(Square, Square, usize)> {
    let squares: Vec<Square> = (0..SIDE)
        .flat_map(|i| (0..SIDE).map(move |j| (i, j)))
        .collect();
    let roads_of = |square: Square| -> Vec<(i32, i32)> {
        let (cx, cy) = centre_of(square);
        (-16..16)
            .flat_map(|dx| (-8..8).map(move |dy| (dx, dy)))
            .filter(|&(dx, dy)| square_at(cx + dx, cy + dy) == Some(square))
            .filter(|&(dx, dy)| road(cx + dx, cy + dy))
            .collect()
    };
    let roads: Vec<(Square, Vec<(i32, i32)>)> = squares.iter().map(|&s| (s, roads_of(s))).collect();
    let library: Vec<(Square, usize, Shape)> = roads
        .iter()
        .flat_map(|(square, offsets)| {
            MIRRORS.iter().enumerate().map(move |(m, &mirror)| {
                let shape = shape_of(offsets.iter().map(|&o| mirrored(o, mirror)));
                (*square, m, shape)
            })
        })
        .collect();
    squares
        .iter()
        .map(|&target| {
            let before = square_before(target, quarters);
            let own = &roads
                .iter()
                .find(|(s, _)| *s == before)
                .expect("a square")
                .1;
            let wanted = turned_shape(shape_of(own.iter().copied()), quarters);
            let (from, mirror, _) = *library
                .iter()
                .min_by_key(|(from, m, shape)| {
                    ((shape ^ wanted).count_ones(), *from != before, *m != 0)
                })
                .expect("a library");
            (target, from, mirror)
        })
        .collect()
}

/// The base picture (RGBA bytes of `width` x `height`, at `scale` times the original's size)
/// turned by `quarters` clockwise quarter turns, or `None` when it is not the picture this is
/// made for.
pub(crate) fn turned(
    bytes: &[u8],
    width: usize,
    height: usize,
    scale: usize,
    quarters: u8,
) -> Option<Vec<u8>> {
    let (w, h) = (SIZE.0 as usize * scale, SIZE.1 as usize * scale);
    if (width, height) != (w, h) || bytes.len() != w * h * 4 || scale == 0 {
        return None;
    }
    if quarters % 4 == 2 {
        return Some(half_turned(bytes, w, h, scale));
    }
    let alpha = |x: i32, y: i32| {
        (0..SIZE.0).contains(&x)
            && (0..SIZE.1).contains(&y)
            && bytes[((y as usize * scale) * w + x as usize * scale) * 4 + 3] > 0
    };
    let chosen = pieces(&alpha, quarters % 4);
    let mut out = vec![0u8; bytes.len()];
    let s = scale as i32;
    for y in 0..SIZE.1 {
        for x in 0..SIZE.0 {
            let Some(target) = square_at(x, y) else {
                continue;
            };
            let &(_, from, mirror) = chosen.iter().find(|(t, _, _)| *t == target)?;
            let (across, down) = MIRRORS[mirror];
            let (tx, ty) = centre_of(target);
            let (sx, sy) = centre_of(from);
            let (ox, oy) = mirrored((x - tx, y - ty), (across, down));
            let (src_x, src_y) = (sx + ox, sy + oy);
            if !(0..SIZE.0).contains(&src_x) || !(0..SIZE.1).contains(&src_y) {
                continue;
            }
            for sub_y in 0..s {
                for sub_x in 0..s {
                    let from_x = src_x * s + if across { s - 1 - sub_x } else { sub_x };
                    let from_y = src_y * s + if down { s - 1 - sub_y } else { sub_y };
                    let at = (((y * s + sub_y) as usize) * w + (x * s + sub_x) as usize) * 4;
                    let source = ((from_y as usize) * w + from_x as usize) * 4;
                    out[at..at + 4].copy_from_slice(&bytes[source..source + 4]);
                }
            }
        }
    }
    Some(out)
}

/// The picture turned by half a turn: a pixel map about the block's middle, so exact (the
/// squares' pieces are only the same to a pixel, as the lattice is not painted symmetric).
fn half_turned(bytes: &[u8], w: usize, h: usize, scale: usize) -> Vec<u8> {
    // The pixel (x, y) goes to (2 * CENTRE.0 - 1 - x, 2 * CENTRE.1 - 1 - y), at the art scale.
    let (mx, my) = (
        2 * CENTRE.0 as usize * scale - 1,
        2 * CENTRE.1 as usize * scale - 1,
    );
    let mut out = vec![0u8; bytes.len()];
    for y in 0..h {
        for x in 0..w {
            if x <= mx && y <= my && mx - x < w && my - y < h {
                let at = ((my - y) * w + (mx - x)) * 4;
                out[at..at + 4].copy_from_slice(&bytes[(y * w + x) * 4..][..4]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use macroquad::prelude::{Image, ImageFormat};

    /// The extracted base picture, when the pictures are there.
    fn base() -> Option<(Vec<u8>, usize, usize)> {
        let path = "../../assets-local/screens/combat/city/cmbtcity_000_00.png";
        let file = std::fs::read(path).ok()?;
        let image = Image::from_file_with_format(&file, Some(ImageFormat::Png)).ok()?;
        Some((
            image.bytes,
            usize::from(image.width),
            usize::from(image.height),
        ))
    }

    fn road_pixels(bytes: &[u8]) -> usize {
        bytes.chunks(4).filter(|p| p[3] > 0).count()
    }

    #[test]
    fn the_block_has_sixteen_squares_that_tile_the_roads() {
        let Some((bytes, w, h)) = base() else { return };
        let mut seen = std::collections::HashSet::new();
        let mut outside = 0;
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                if bytes[(y as usize * w + x as usize) * 4 + 3] == 0 {
                    continue;
                }
                match square_at(x, y) {
                    Some(s) => {
                        seen.insert(s);
                    }
                    None => outside += 1,
                }
            }
        }
        assert_eq!(outside, 0, "every road pixel is inside a square");
        assert_eq!(seen.len(), 16);
    }

    #[test]
    fn without_a_turn_every_square_keeps_its_own_piece() {
        let Some((bytes, w, h)) = base() else { return };
        let same = turned(&bytes, w, h, 1, 0).unwrap();
        let alpha = |b: &[u8]| b.chunks(4).map(|p| p[3] > 0).collect::<Vec<_>>();
        assert_eq!(alpha(&same), alpha(&bytes));
    }

    #[test]
    fn in_every_turn_each_square_has_a_piece_with_the_turned_edges_and_middle() {
        let Some((bytes, w, h)) = base() else { return };
        let alpha = |x: i32, y: i32| {
            (0..w as i32).contains(&x)
                && (0..h as i32).contains(&y)
                && bytes[(y as usize * w + x as usize) * 4 + 3] > 0
        };
        for quarters in 0..4u8 {
            let chosen = pieces(&alpha, quarters);
            assert_eq!(chosen.len(), 16);
            let mut inexact = 0;
            for &(target, from, mirror) in &chosen {
                let shape = |square: Square, m: usize| {
                    let (cx, cy) = centre_of(square);
                    shape_of(
                        (-16..16)
                            .flat_map(|dx| (-8..8).map(move |dy| (dx, dy)))
                            .filter(|&(dx, dy)| square_at(cx + dx, cy + dy) == Some(square))
                            .filter(|&(dx, dy)| alpha(cx + dx, cy + dy))
                            .map(|o| mirrored(o, MIRRORS[m])),
                    )
                };
                let wanted = turned_shape(shape(square_before(target, quarters), 0), quarters);
                // One square of a quarter turn has a road stub one part short of its twin.
                let off = (shape(from, mirror) ^ wanted).count_ones();
                assert!(
                    off <= 1,
                    "turn {quarters}, square {target:?}: {off} parts differ"
                );
                inexact += usize::from(off > 0);
            }
            assert!(
                inexact <= 1,
                "turn {quarters}: {inexact} squares are not exact"
            );
        }
    }

    #[test]
    fn a_turn_keeps_the_roads_inside_the_block_and_draws_each_turn_differently() {
        let Some((bytes, w, h)) = base() else { return };
        let mut looks = Vec::new();
        for quarters in 0..4 {
            let out = turned(&bytes, w, h, 1, quarters).unwrap();
            for (n, p) in out.chunks(4).enumerate() {
                if p[3] > 0 {
                    let (x, y) = ((n % w) as i32, (n / w) as i32);
                    assert!(square_at(x, y).is_some());
                }
            }
            assert!(road_pixels(&out) > road_pixels(&bytes) / 2);
            looks.push(out.chunks(4).map(|p| p[3]).collect::<Vec<_>>());
        }
        for a in 0..4 {
            for b in a + 1..4 {
                assert_ne!(looks[a], looks[b], "turns {a} and {b}");
            }
        }
    }

    #[test]
    fn half_a_turn_is_the_picture_turned_about_the_blocks_middle_pixel_for_pixel() {
        let Some((bytes, w, h)) = base() else { return };
        let out = turned(&bytes, w, h, 1, 2).unwrap();
        let alpha = |b: &[u8], x: i32, y: i32| b[(y as usize * w + x as usize) * 4 + 3] > 0;
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let (tx, ty) = (2 * CENTRE.0 - 1 - x, 2 * CENTRE.1 - 1 - y);
                if (0..w as i32).contains(&tx) && (0..h as i32).contains(&ty) {
                    assert_eq!(alpha(&bytes, x, y), alpha(&out, tx, ty));
                }
            }
        }
    }

    #[test]
    fn a_picture_at_a_finer_scale_is_turned_the_same_way() {
        let Some((bytes, w, h)) = base() else { return };
        let fine: Vec<u8> = (0..h * 2)
            .flat_map(|y| (0..w * 2).flat_map(move |x| (0..4).map(move |c| (x / 2, y / 2, c))))
            .map(|(x, y, c)| bytes[(y * w + x) * 4 + c])
            .collect();
        for quarters in 1..4 {
            let small = turned(&bytes, w, h, 1, quarters).unwrap();
            let big = turned(&fine, w * 2, h * 2, 2, quarters).unwrap();
            for y in 0..h {
                for x in 0..w {
                    assert_eq!(
                        big[((y * 2) * w * 2 + x * 2) * 4 + 3] > 0,
                        small[(y * w + x) * 4 + 3] > 0
                    );
                }
            }
        }
    }
}
