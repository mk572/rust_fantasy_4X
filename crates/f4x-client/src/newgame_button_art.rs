//! The new game screens' wooden OK and Cancel without their lettering:
//! the screen's blank wooden plate (`assets/button/wood_64.png` for the wide ones, `wood_30.png`, cut from
//! it, for the small OK of the creation steps; `scripts/button_text_free.py`). The label is a text key drawn by the font
//! engine in style 4 in the original's dark brown with a light edge down and to the right.

use macroquad::prelude::*;
use std::cell::RefCell;

use crate::font::{rgb, Style};
use crate::frame::Layout;
use crate::input::Input;
use crate::menu_bar_art::{draw_button, label_font};
use crate::ui;
use crate::view::App;

/// Which picture the button has.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Board {
    Wide,
    Small,
}

impl Board {
    fn picture(self) -> &'static [u8] {
        match self {
            Board::Wide => include_bytes!("../assets/button/wood_64.png"),
            Board::Small => include_bytes!("../assets/button/wood_30.png"),
        }
    }

    /// How far up and left of the original's OK picture the plate starts (it is a pixel bigger).
    fn offset(self) -> (f32, f32) {
        match self {
            Board::Wide => (1.0, 1.0),
            Board::Small => (0.0, 1.0),
        }
    }

    /// Where the label's middle and top are, from the original OK picture's top left. A pixel higher
    /// than the original's own lettering for the wide buttons, whose plate has an extra row on top.
    fn label_at(self, width: f32) -> (f32, f32) {
        match self {
            Board::Wide => (width / 2.0 + 1.0, 2.0),
            Board::Small => (width / 2.0, 2.0),
        }
    }
}

thread_local! {
    static TEXTURES: RefCell<[Option<Texture2D>; 2]> = const { RefCell::new([None, None]) };
}

fn texture(board: Board) -> Texture2D {
    TEXTURES.with(|cache| {
        cache.borrow_mut()[board as usize]
            .get_or_insert_with(|| {
                let tex = Texture2D::from_file_with_format(board.picture(), Some(ImageFormat::Png));
                tex.set_filter(FilterMode::Nearest);
                tex
            })
            .clone()
    })
}

impl App {
    /// Draw the button with the label of text key `key` with its top left at `at`; `pressed` as every
    /// button of the client looks while held (`menu_bar_art::draw_button`), `fade` (1 = opaque)
    /// dims a disabled one.
    pub(crate) fn draw_board(
        &mut self,
        lay: &Layout,
        board: Board,
        key: &str,
        at: (f32, f32),
        pressed: bool,
        fade: f32,
    ) {
        let tex = texture(board);
        let s = lay.scale;
        let size = vec2(tex.width(), tex.height());
        let (ox, oy) = board.offset();
        draw_button(
            &tex,
            lay.point(at.0 - ox, at.1 - oy),
            size * s,
            s,
            pressed,
            Color::new(1.0, 1.0, 1.0, fade),
        );
        let shift = if pressed { 1.0 } else { 0.0 };
        let ink = rgb(60, 42, 34);
        let style = Style::flat(4, Color::new(ink.r, ink.g, ink.b, fade))
            .shadow()
            .edge(Color::from_rgba(142, 113, 89, (128.0 * fade) as u8))
            .center();
        let (lx, ly) = board.label_at(size.x - ox);
        let label = self.text(key);
        self.draw_styled_in(
            &label_font(),
            (lay.origin.x, lay.origin.y, s),
            &style,
            &label,
            (at.0 + lx + shift, at.1 + ly + shift),
        );
    }

    /// The small OK of the creation steps at `at` (its click is the step's own, on a rectangle a
    /// little larger); a pixel down and right while the mouse holds it, faded while `enabled` is
    /// false.
    pub(crate) fn draw_small_ok(
        &mut self,
        lay: &Layout,
        input: &Input,
        at: (f32, f32),
        enabled: bool,
    ) {
        let held = ui::hit(lay.rect((at.0 - 1.0, at.1 - 1.0, 32.0, 16.0)), true, input).held();
        let fade = if enabled { 1.0 } else { 0.5 };
        self.draw_board(lay, Board::Small, "ui.button_ok", at, held, fade);
    }

    /// A wide wooden button; true when clicked.
    pub(crate) fn board_button(
        &mut self,
        lay: &Layout,
        key: &str,
        at: (f32, f32),
        input: &Input,
    ) -> bool {
        let size = vec2(63.0, 14.0);
        let state = ui::hit(lay.rect((at.0, at.1, size.x, size.y)), true, input);
        self.draw_board(lay, Board::Wide, key, at, state.held(), 1.0);
        state.clicked()
    }
}

#[cfg(test)]
mod tests {
    use super::Board;

    /// The wide picture is 64 by 15 (its opaque part) and the small one 30 by 15 (the PNG header
    /// holds the size at bytes 16 to 24).
    #[test]
    fn the_wooden_buttons_have_the_sizes_of_the_originals() {
        for (board, width) in [(Board::Wide, 64), (Board::Small, 30)] {
            let bytes = board.picture();
            let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
            let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
            assert_eq!((w, h), (width, 15));
        }
    }
}
