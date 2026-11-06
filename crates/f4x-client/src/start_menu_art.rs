//! The start menu's words (Continue, Load Game, New Game, Hall of Fame, Quit). The original's are
//! pictures of gold lettering; here the font engine draws them from text keys in the blackletter
//! (style 4) with the colour sets `font_ramp::MENU_WORD` (the lettering) and `MENU_WORD_HOVER` (the
//! word under the mouse), a dark shadow down and to the right, and the font's kerning.

use crate::font::{rgb, Style};
use crate::font_ramp::{MENU_WORD, MENU_WORD_HOVER};
use crate::frame::Layout;
use crate::view::App;
use macroquad_bitmap_font_engine as bm;
use std::cell::RefCell;
use std::rc::Rc;

/// Style 4 with the menu bar's kerning less the pair "am" (`assets/button/kerning_word.fnt`): the
/// start menu's pictures set it open in "Game" and "Fame".
fn word_font() -> Rc<bm::BmFont> {
    thread_local! {
        static FACE: RefCell<Option<Rc<bm::BmFont>>> = const { RefCell::new(None) };
    }
    FACE.with(|face| {
        face.borrow_mut()
            .get_or_insert_with(|| {
                let fnt = format!(
                    "{}\n{}",
                    include_str!("../assets/fonts/style4.fnt"),
                    include_str!("../assets/button/kerning_word.fnt")
                );
                Rc::new(
                    bm::BmFont::from_bytes(&fnt, include_bytes!("../assets/fonts/style4.png"))
                        .expect("the start menu's font loads"),
                )
            })
            .clone()
    })
}

/// Where the middle of a word is in the original's pixels: its pictures are 82 wide from x = 123, and
/// the lettering in them is not on one centre line (each is set a pixel or two off).
fn centre(key: &str) -> f32 {
    match key {
        "menu.continue" => 163.5,
        "menu.load_game" => 161.5,
        "menu.hall_of_fame" => 163.0,
        _ => 161.0,
    }
}

impl App {
    /// Draw the word of text key `key` with its top at `y`; `hover` when the mouse is over it.
    pub(crate) fn draw_menu_word(&mut self, lay: &Layout, key: &str, y: f32, hover: bool) {
        let set = if hover { MENU_WORD_HOVER } else { MENU_WORD };
        let style = Style::inked(4, crate::font::Ink::Set(set))
            .shadow()
            .edge(rgb(8, 4, 4))
            .center();
        let label = self.text(key);
        let centre = centre(key);
        self.draw_styled_in(
            &word_font(),
            (lay.origin.x, lay.origin.y, lay.scale),
            &style,
            &label,
            (centre, y),
        );
    }
}
