//! The decks of the ships in a sea battle (task 373): our own pictures of a hull and a deck for
//! each deck size, in the four views of the battle screen, embedded in the client
//! (`assets/ship/`, drawn by `scripts/make_ship_art.py`). Which squares are a deck is the
//! battle's; this finds the ships among them and says which picture lies where. A deck of a size
//! without pictures (a mod's ship) is drawn as plain planks (`battle_ground.rs`).

use crate::battle_turn::Turn;
use f4x_battle::map::layout::{self, W};
use f4x_battle::map::Pos;
use std::collections::BTreeSet;

/// Clear pixels round the deck's squares in a picture, as the script has it.
const PAD: f32 = 40.0;

/// One view of a deck: its name in the art cache, and the picture.
type View = (&'static str, &'static [u8]);

macro_rules! view {
    ($name:expr) => {
        (
            concat!("ship/", $name),
            include_bytes!(concat!("../assets/ship/deck_", $name, ".png")),
        )
    };
}

macro_rules! views {
    ($size:literal) => {
        [
            view!(concat!($size, "_0")),
            view!(concat!($size, "_1")),
            view!(concat!($size, "_2")),
            view!(concat!($size, "_3")),
        ]
    };
}

/// The four views of each deck size (width, length) that has pictures.
const PICTURES: &[((i32, i32), [View; 4])] = &[
    ((2, 7), views!("2x7")),
    ((3, 8), views!("3x8")),
    ((4, 9), views!("4x9")),
    ((5, 5), views!("5x5")),
];

/// One ship's deck on the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deck {
    pub size: (i32, i32),
    /// The battle's squares of the deck.
    pub squares: Vec<Pos>,
}

/// The ships whose decks make up `squares`: no two decks touch, not even at a corner
/// (`layout::ship_decks`).
pub fn decks(squares: &BTreeSet<Pos>) -> Vec<Deck> {
    let mut left = squares.clone();
    let mut out = Vec::new();
    while let Some(first) = left.pop_first() {
        let mut ship = vec![first];
        let mut next = 0;
        while let Some(&(x, y)) = ship.get(next) {
            next += 1;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if left.remove(&(x + dx, y + dy)) {
                        ship.push((x + dx, y + dy));
                    }
                }
            }
        }
        let extent = |of: fn(&Pos) -> i32| {
            let low = ship.iter().map(of).min().unwrap_or(0);
            ship.iter().map(of).max().unwrap_or(0) - low + 1
        };
        out.push(Deck {
            size: (extent(|p| p.0), extent(|p| p.1)),
            squares: ship,
        });
    }
    out
}

/// A deck's picture in one view.
pub struct Picture {
    /// Names the picture in the art cache.
    pub key: &'static str,
    pub png: &'static [u8],
    /// The turned cell the picture hangs from: the least x and the least y among the deck's.
    pub cell: (i32, i32),
    /// From that cell's top left to the picture's, in native pixels.
    pub offset: (f32, f32),
}

/// The picture of `deck` as the view `turn` draws it, if its size has one and its squares are
/// those of a ship of that size.
pub fn picture(deck: &Deck, turn: Turn) -> Option<Picture> {
    let (width, length) = deck.size;
    let (_, views) = PICTURES.iter().find(|(size, _)| *size == deck.size)?;
    let low = (
        deck.squares.iter().map(|p| p.0).min()?,
        deck.squares.iter().map(|p| p.1).min()?,
    );
    let shape: BTreeSet<Pos> = layout::ship_shape(width, length)
        .into_iter()
        .map(|(x, y)| (low.0 + x, low.1 + y))
        .collect();
    if shape != deck.squares.iter().copied().collect() {
        return None;
    }
    let turned: Vec<(i32, i32)> = deck
        .squares
        .iter()
        .map(|&(x, y)| turn.cg((W - 1 - x, y)))
        .collect();
    let cell = (
        turned.iter().map(|p| p.0).min()?,
        turned.iter().map(|p| p.1).min()?,
    );
    let rows = turned.iter().map(|p| p.1).max()? - cell.1 + 1;
    let &(key, png) = views.get(usize::from(turn.quarters()))?;
    Some(Picture {
        key,
        png,
        cell,
        offset: (-((rows - 1) as f32) * 16.0 - PAD, -PAD),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use macroquad::prelude::{Image, ImageFormat};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The deck sizes of the shipped units.
    fn shipped_sizes() -> Result<Vec<(i32, i32)>, Box<dyn std::error::Error>> {
        let content = crate::test_app::shipped::content();
        let mut sizes: Vec<(i32, i32)> = content
            .rules
            .units
            .defs()
            .iter()
            .filter_map(|d| d.deck)
            .map(|(w, l)| (i32::from(w), i32::from(l)))
            .collect();
        sizes.sort_unstable();
        sizes.dedup();
        if sizes.is_empty() {
            return Err("no shipped unit has a deck".into());
        }
        Ok(sizes)
    }

    #[test]
    fn the_ships_laid_out_for_a_battle_are_found_again() -> TestResult {
        let sizes = shipped_sizes()?;
        for &a in &sizes {
            for &d in &sizes {
                let laid = layout::ship_decks(&[a, a], &[d]);
                let mut expected: Vec<Deck> = laid
                    .attackers
                    .into_iter()
                    .map(|c| (a, c))
                    .chain(laid.defenders.into_iter().map(|c| (d, c)))
                    .filter_map(|(size, cells)| {
                        let mut squares = cells?;
                        squares.sort_unstable();
                        Some(Deck { size, squares })
                    })
                    .collect();
                let squares = expected.iter().flat_map(|d| d.squares.clone()).collect();
                let mut found = decks(&squares);
                for deck in &mut found {
                    deck.squares.sort_unstable();
                }
                expected.sort_by(|a, b| a.squares.cmp(&b.squares));
                found.sort_by(|a, b| a.squares.cmp(&b.squares));
                assert_eq!(found, expected, "{a:?} against {d:?}");
            }
        }
        Ok(())
    }

    /// Each picture lies under its squares: the middle of every deck square is painted, in all
    /// four views.
    #[test]
    fn every_shipped_deck_has_a_picture_under_each_of_its_squares() -> TestResult {
        for size in shipped_sizes()? {
            let squares: BTreeSet<Pos> = layout::ship_shape(size.0, size.1)
                .into_iter()
                .map(|(x, y)| (x + 4, y + 6))
                .collect();
            let deck = decks(&squares).pop().ok_or("no deck")?;
            let mut turn = Turn::NONE;
            for quarters in 0..4 {
                let found =
                    picture(&deck, turn).ok_or_else(|| format!("no picture of {size:?}"))?;
                let image = Image::from_file_with_format(found.png, Some(ImageFormat::Png))?;
                for &(x, y) in &squares {
                    let cell = turn.cg((W - 1 - x, y));
                    let at = |column: i32, row: i32| {
                        let (dx, dy) = (column - found.cell.0, row - found.cell.1);
                        (
                            ((dx - dy) * 16) as f32 - found.offset.0 + 16.0,
                            ((dx + dy) * 8) as f32 - found.offset.1 + 8.0,
                        )
                    };
                    let (px, py) = at(cell.0, cell.1);
                    assert!(
                        px >= 0.0
                            && py >= 0.0
                            && px < f32::from(image.width)
                            && py < f32::from(image.height),
                        "{size:?} view {quarters}: ({x}, {y}) is off the picture"
                    );
                    let alpha = image.get_pixel(px as u32, py as u32).a;
                    assert!(
                        alpha > 0.99,
                        "{size:?} view {quarters}: nothing under ({x}, {y})"
                    );
                }
                turn = turn.step(false);
            }
        }
        Ok(())
    }

    /// A full rectangle of a ship's width and length is not the ship's shape, where the shape
    /// has narrow ends.
    #[test]
    fn squares_that_are_no_ships_have_no_picture() -> TestResult {
        let mut tried = 0;
        for (width, length) in shipped_sizes()? {
            if layout::ship_shape(width, length).len() == (width * length) as usize {
                continue;
            }
            let squares = (0..length)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .collect();
            let deck = decks(&squares).pop().ok_or("no deck")?;
            assert!(picture(&deck, Turn::NONE).is_none(), "{width} by {length}");
            tried += 1;
        }
        assert!(tried > 0, "every shipped deck is a rectangle");
        Ok(())
    }
}
