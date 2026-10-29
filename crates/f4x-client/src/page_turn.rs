//! The page turn of the spell books (`SmlBook_PageTurn`, `BigBook_PageTurn`, `Spellbook.c`): four
//! stages in which the next page's spread shows under the turning leaf, the leaf's paper is one
//! of the four `book.lbx` frames, and the leaf's text is drawn narrower and bent along a curve
//! (`Scale_Bitmap` and `Shear_Bitmap_Y`). The leaf is composed in an off-screen picture and
//! drawn a column at a time, each lifted by the curve.

use crate::input::Input;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::view::App;

/// How long one of the four stages shows. The original flips the pages as fast as it can; this
/// is slow enough to see.
const STAGE_SECONDS: f64 = 0.07;

/// A turn in progress.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BookTurn {
    /// The big book (the Apprentice's library) rather than the small one.
    pub big: bool,
    pub forward: bool,
    started: f64,
}

/// One stretch of a curve: the leaf's column `x0` is lifted by `y0`, column `x1` by `y1`, and
/// those between by steps along the line (`Shear_Bitmap_Y`).
type Segment = (i32, f32, i32, f32);

/// What a stage draws: the leaf's width in percent, where its left edge goes and the curve.
struct Stage {
    percent: f32,
    at: (f32, f32),
    curve: &'static [Segment],
}

const SMALL_STAGES: [Stage; 4] = [
    Stage {
        percent: 85.0,
        at: (164.0, -18.0),
        curve: &[
            (0, 0.0, 3, -4.0),
            (4, -5.0, 9, -10.0),
            (10, -11.0, 21, -16.0),
            (22, -16.0, 32, -19.0),
            (33, -19.0, 40, -19.0),
            (41, -18.0, 62, -13.0),
            (63, -12.0, 101, 5.0),
        ],
    },
    Stage {
        percent: 55.0,
        at: (164.0, -20.0),
        curve: &[
            (0, 0.0, 5, -8.0),
            (6, -11.0, 16, -21.0),
            (17, -22.0, 26, -27.0),
            (27, -27.0, 38, -29.0),
            (39, -29.0, 49, -27.0),
            (50, -26.0, 61, -21.0),
            (62, -20.0, 70, -12.0),
        ],
    },
    Stage {
        percent: 55.0,
        at: (87.0, -18.0),
        curve: &[
            (0, -16.0, 9, -25.0),
            (10, -25.0, 17, -29.0),
            (18, -29.0, 29, -32.0),
            (30, -33.0, 40, -30.0),
            (41, -29.0, 52, -25.0),
            (53, -24.0, 63, -14.0),
            (64, -12.0, 70, 0.0),
        ],
    },
    Stage {
        percent: 85.0,
        at: (51.0, -12.0),
        curve: &[
            (0, 0.0, 31, -16.0),
            (32, -16.0, 47, -22.0),
            (48, -22.0, 68, -27.0),
            (69, -27.0, 82, -23.0),
            (83, -23.0, 92, -18.0),
            (93, -18.0, 101, -10.0),
        ],
    },
];

const BIG_STAGES: [Stage; 4] = [
    Stage {
        percent: 90.0,
        at: (165.0, -24.0),
        curve: &[
            (0, 0.0, 4, -6.0),
            (5, -7.0, 12, -12.0),
            (13, -13.0, 29, -19.0),
            (30, -19.0, 41, -19.0),
            (42, -19.0, 67, -14.0),
            (68, -13.0, 84, -7.0),
            (85, -6.0, 115, 8.0),
        ],
    },
    Stage {
        percent: 60.0,
        at: (165.0, -26.0),
        curve: &[
            (0, -4.0, 5, -9.0),
            (6, -10.0, 14, -18.0),
            (15, -19.0, 32, -26.0),
            (33, -26.0, 42, -28.0),
            (43, -28.0, 56, -25.0),
            (57, -25.0, 69, -18.0),
            (70, -18.0, 76, -12.0),
        ],
    },
    Stage {
        percent: 60.0,
        at: (80.0, -20.0),
        curve: &[
            (0, -20.0, 6, -25.0),
            (7, -25.0, 16, -30.0),
            (17, -30.0, 24, -32.0),
            (25, -32.0, 32, -33.0),
            (33, -33.0, 47, -29.0),
            (48, -29.0, 61, -21.0),
            (62, -20.0, 70, -12.0),
            (71, -11.0, 75, 0.0),
        ],
    },
    Stage {
        percent: 90.0,
        at: (39.0, -22.0),
        curve: &[
            (0, 0.0, 36, -18.0),
            (37, -18.0, 64, -26.0),
            (65, -26.0, 73, -27.0),
            (74, -27.0, 84, -26.0),
            (85, -25.0, 92, -23.0),
            (93, -22.0, 102, -18.0),
            (103, -17.0, 115, -6.0),
        ],
    },
];

/// How far column `x` of the leaf is lifted.
fn lift(curve: &[Segment], x: i32) -> f32 {
    curve
        .iter()
        .find(|&&(x0, _, x1, _)| (x0..=x1).contains(&x))
        .map_or(0.0, |&(x0, y0, x1, y1)| {
            if x1 == x0 {
                y0
            } else {
                y0 + (y1 - y0) * (x - x0) as f32 / (x1 - x0) as f32
            }
        })
}

impl App {
    /// Begin turning a page of the small or big book, if the art is there to show it.
    pub(crate) fn start_book_turn(&mut self, big: bool, forward: bool) {
        let entry = usize::from(big);
        if self.art_picture("book", entry, 0).is_some() {
            self.book_turn = Some(BookTurn {
                big,
                forward,
                started: get_time(),
            });
        }
    }

    /// Move the book to the next or the previous spread.
    fn move_book(&mut self, big: bool, forward: bool) {
        let left = if big {
            &mut self.big_ui.left
        } else {
            &mut self.book_ui.left
        };
        *left = if forward {
            *left + 2
        } else {
            left.saturating_sub(2)
        };
    }

    /// For screenshots: a turn already `seconds` in.
    pub fn open_test_page_turn(&mut self, big: bool, forward: bool, seconds: f64) {
        let left = if big {
            &mut self.big_ui.left
        } else {
            &mut self.book_ui.left
        };
        *left = if forward { 0 } else { 2 };
        self.start_book_turn(big, forward);
        if let Some(turn) = self.book_turn.as_mut() {
            turn.started = get_time() - seconds;
        }
    }

    /// Turn a page: with the animation if the art is there, else at once.
    pub(crate) fn turn_book_page(&mut self, big: bool, forward: bool, input: &Input) {
        self.start_book_turn(big, forward);
        if self.book_turn.is_none() {
            self.move_book(big, forward);
        } else {
            self.book_turn_frame(input);
        }
    }

    /// Draw the turn in progress. Returns true once it is over (the book is on its new spread
    /// and the turn is cleared), or when none is.
    pub(crate) fn book_turn_frame(&mut self, input: &Input) -> bool {
        let Some(turn) = self.book_turn else {
            return true;
        };
        let stage = ((get_time() - turn.started) / STAGE_SECONDS) as usize;
        if stage >= 4 {
            self.book_turn = None;
            self.move_book(turn.big, turn.forward);
            return true;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (left, pages) = if turn.big {
            (self.big_ui.left, self.big_ui.pages.len())
        } else {
            (self.book_ui.left, self.book_ui.pages.len())
        };
        // The spread under the leaf: the page that stays and the one that is uncovered.
        let (under_left, under_right) = if turn.forward {
            (left, left + 3)
        } else {
            (left.saturating_sub(2), left + 1)
        };
        // The leaf: the page that lifts, then the one that lands on the other side.
        let leaf_page = match (turn.forward, stage < 2) {
            (true, true) => left + 1,
            (true, false) => left + 2,
            (false, true) => left,
            (false, false) => left.saturating_sub(1),
        };
        if turn.big {
            let corners = (under_left != 0, under_right + 2 < pages);
            self.draw_big_spread(&lay, view, under_left, under_right, corners, None);
        } else {
            let corners = (under_left != 0, true);
            self.draw_book_spread(&lay, view, under_left, under_right, corners, input);
        }
        let frame = if turn.forward { stage } else { 3 - stage };
        self.draw_pic(&lay, "book", usize::from(turn.big), frame, 0.0, 0.0);
        let table = if turn.big { &BIG_STAGES } else { &SMALL_STAGES };
        let shape = &table[frame];
        self.draw_leaf(&lay, turn.big, leaf_page, shape);
        false
    }

    /// The leaf's page composed off screen, then drawn narrowed to `shape.percent` and bent along
    /// its curve, a column at a time.
    fn draw_leaf(&mut self, lay: &Layout, big: bool, page_i: usize, shape: &Stage) {
        let (w, h) = if big { (129.0, 200.0) } else { (122.0, 175.0) };
        let k = lay.scale;
        let (pw, ph) = ((w * k).ceil() as u32, (h * k).ceil() as u32);
        let target = render_target(pw, ph);
        target.texture.set_filter(FilterMode::Nearest);
        set_camera(&Camera2D {
            zoom: vec2(2.0 / pw as f32, 2.0 / ph as f32),
            target: vec2(pw as f32 / 2.0, ph as f32 / 2.0),
            render_target: Some(target.clone()),
            ..Default::default()
        });
        clear_background(Color::new(0.0, 0.0, 0.0, 0.0));
        let page_lay = Layout {
            scale: k,
            origin: Vec2::ZERO,
        };
        let page_view = (0.0, 0.0, k);
        let right = page_i % 2 == 1;
        if big {
            let title = (if right { 58.0 } else { 66.0 }, 30.0);
            self.draw_big_page(&page_lay, page_view, page_i, right, 0.0, title, 20.0, None);
        } else {
            self.draw_book_page(&page_lay, page_view, page_i, right, 0.0, (62.0, 30.0), 17.0);
        }
        set_default_camera();
        let columns = (w * shape.percent / 100.0).floor() as i32;
        let step = 100.0 / shape.percent;
        self.clipped((0.0, 0.0, 320.0, 200.0), |_| {
            for c in 0..columns {
                let at = lay.point(shape.at.0 + c as f32, shape.at.1 + lift(shape.curve, c));
                draw_texture_ex(
                    &target.texture,
                    at.x,
                    at.y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(k, h * k)),
                        source: Some(Rect::new(c as f32 * step * k, 0.0, step * k, h * k)),
                        ..Default::default()
                    },
                );
            }
        });
    }
}
