//! The credits roll of the start menu: the game's name, what it is inspired by, its licence and
//! the software and art it uses, scrolling up the screen (task 396). The original rolls its
//! credits when the menu is left alone; the start menu has no entry for it (`--credits` shows it). A click or Esc
//! leaves, and so does the end of the roll.

use macroquad::prelude::*;

use crate::font::Style;
use crate::frame::{Align, Layout};
use crate::input::Input;
use crate::page::Page;
use crate::view::{App, Screen};

/// The height of the screen in the original's pixels.
const HEIGHT: f32 = 200.0;
/// The pixels the roll rises in a second.
const SPEED: f32 = 18.0;
/// The distance between two lines.
const LINE: f32 = 12.0;

/// What a line of the roll is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Title,
    Head,
    Body,
    /// Empty space.
    Gap,
}

/// The roll: a text key and how it is drawn, top to bottom.
pub(crate) const ROLL: &[(Kind, &str)] = &[
    (Kind::Title, "credits.title"),
    (Kind::Gap, ""),
    (Kind::Body, "credits.tagline"),
    (Kind::Gap, ""),
    (Kind::Head, "credits.inspired_head"),
    (Kind::Body, "credits.inspired_game"),
    (Kind::Body, "credits.inspired_by"),
    (Kind::Gap, ""),
    (Kind::Head, "credits.licence_head"),
    (Kind::Body, "credits.licence_1"),
    (Kind::Body, "credits.licence_2"),
    (Kind::Gap, ""),
    (Kind::Head, "credits.software_head"),
    (Kind::Body, "credits.software_engine"),
    (Kind::Body, "credits.software_mt32"),
    (Kind::Gap, ""),
    (Kind::Head, "credits.art_head"),
    (Kind::Body, "credits.art_1"),
    (Kind::Body, "credits.art_2"),
    (Kind::Body, "credits.art_3"),
    (Kind::Gap, ""),
    (Kind::Body, "credits.leave"),
];

/// The y of line `i` after `seconds` of rolling: the first line starts at the bottom edge.
pub(crate) fn line_y(i: usize, seconds: f64) -> f32 {
    HEIGHT + LINE * i as f32 - SPEED * seconds as f32
}

/// The roll has gone off the top.
pub(crate) fn finished(seconds: f64) -> bool {
    line_y(ROLL.len(), seconds) < 0.0
}

struct CreditsPage {
    since: f64,
}

impl App {
    pub(crate) fn open_credits(&mut self) {
        self.open_page(Screen::Credits, CreditsPage { since: get_time() });
    }
}

impl Page for CreditsPage {
    fn frame(&mut self, app: &mut App, input: &Input) {
        clear_background(BLACK);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let seconds = get_time() - self.since;
        for (i, &(kind, key)) in ROLL.iter().enumerate() {
            let y = line_y(i, seconds);
            if kind == Kind::Gap || !(-LINE..HEIGHT).contains(&y) {
                continue;
            }
            let style = match kind {
                Kind::Title => Style::nearest(5, GOLD),
                Kind::Head => Style::nearest(2, GOLD),
                _ => Style::nearest(1, WHITE),
            };
            let text = app.text(key);
            app.draw_styled(view, &style.align(Align::Center), &text, 160.0, y);
        }
        if input.key_pressed(KeyCode::Escape)
            || input.mouse_pressed(MouseButton::Left)
            || finished(seconds)
        {
            app.open_main_menu();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_roll_starts_below_the_screen_and_ends_above_it() {
        assert!(line_y(0, 0.0) >= HEIGHT);
        assert!(!finished(0.0));
        assert!(finished(60.0));
    }

    #[test]
    fn every_line_of_the_roll_has_its_text() {
        let texts = crate::test_app::shipped::texts();
        for &(kind, key) in ROLL {
            if kind != Kind::Gap {
                assert!(texts.has(key), "{key} has no text");
            }
        }
    }
}
