//! The wizard sheet, the original's Mirror screen (`WIZVIEW.c`): one wizard's gem and name, for
//! a rival what you know of them (relations, treaties, personality, objective), then gold and
//! mana, a shelf of spell books, fame, the retorts as a running sentence, and the heroes.
//! F9 shows your own; a right-click on a rival's gem on the magic screen shows theirs. Esc or a
//! click leaves. Positions are the original's, measured from the panel's top-left.

use f4x_core::diplomacy::relation_word;
use f4x_core::player::PlayerId;
use f4x_core::wizard::magic::Retort;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::view::{App, Screen, HUMAN};

/// Where the panel's top-left sits on the 320 by 200 screen, and its size.
const PANEL: (f32, f32, f32, f32) = (75.0, 2.0, 180.0, 196.0);
/// The realms in the order of the book pictures (`magic` 33 to 50: three variants each).
const BOOK_REALMS: [&str; 5] = ["nature", "sorcery", "chaos", "life", "death"];
const FIRST_BOOK: usize = 33;
/// A book icon is eight pixels wide on the shelf.
const BOOK_STEP: f32 = 8.0;
/// The retort sentence starts at this x and wraps at this width, a line being this tall.
const RETORT_WIDTH: f32 = 155.0;
const RETORT_LINE: f32 = 7.0;

/// What the wizard sheet remembers while it is open.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WizardUi {
    pub who: PlayerId,
    /// The screen to return to.
    pub back: Screen,
}

impl Default for WizardUi {
    fn default() -> Self {
        Self {
            who: HUMAN,
            back: Screen::Map,
        }
    }
}

/// One piece of the retort sentence and where it goes, as `(text, x offset, line)`; the closing
/// full stop is the last piece. `width` measures text in pixels. The pieces are "Alchemy, ",
/// "Warlord and " and "Famous", each followed by a pixel, and a piece that would pass `limit`
/// starts the next line.
pub(crate) fn retort_pieces(
    names: &[&str],
    limit: f32,
    width: impl Fn(&str) -> f32,
) -> Vec<(String, f32, usize)> {
    let mut out = Vec::new();
    let (mut x, mut line) = (0.0, 0);
    for (i, name) in names.iter().enumerate() {
        let mut piece = (*name).to_string();
        if i + 1 != names.len() {
            piece.push_str(if i + 2 == names.len() { " and" } else { "," });
            piece.push(' ');
        }
        let w = width(&piece);
        if x + w > limit {
            x = 0.0;
            line += 1;
        }
        out.push((piece, x, line));
        x += w + 1.0;
    }
    if !names.is_empty() {
        out.push((".".to_string(), x, line));
    }
    out
}

/// The book icons of a shelf: for each realm one picture a book, cycling through the realm's three
/// pictures (the first, second, third, first...). ReMoM's code for the original steps with
/// `(n + 1) / 3`, which stays on the first picture; this is a departure (`docs/divergences.md`
/// section 7).
pub(crate) fn book_entries(books: impl Fn(&str) -> i32) -> Vec<usize> {
    BOOK_REALMS
        .iter()
        .enumerate()
        .flat_map(|(i, realm)| {
            (0..books(realm).max(0) as usize).map(move |n| FIRST_BOOK + i * 3 + n % 3)
        })
        .collect()
}

impl App {
    /// Open the sheet of `who`: yours, or a rival you have met.
    pub fn open_wizard(&mut self, who: PlayerId) {
        let known = who == HUMAN
            || (self.game.have_met(HUMAN, who)
                && self.game.player(who).is_some_and(|p| !p.neutral));
        if !known {
            return;
        }
        self.wizard_ui = WizardUi {
            who,
            back: self.screen,
        };
        self.screen = Screen::Wizard;
    }

    /// For the screenshot check: show a wizard's sheet whether or not they have been met.
    pub fn open_test_wizard(&mut self, who: u8) {
        self.wizard_ui = WizardUi {
            who: PlayerId(who),
            back: Screen::Map,
        };
        self.screen = Screen::Wizard;
    }

    pub(crate) fn wizard_screen(&mut self) {
        let ui = self.wizard_ui;
        if is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::Enter)
            || is_mouse_button_pressed(MouseButton::Left)
        {
            self.screen = ui.back;
            return;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (px, py, pw, ph) = PANEL;
        if self.draw_pic(&lay, "backgrnd", 4, 0, px, py).is_none() {
            let r = lay.rect(PANEL);
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.18, 0.2, 0.3, 1.0));
            let _ = (pw, ph);
        }
        let Some(player) = self.game.player(ui.who).as_deref().cloned() else {
            return;
        };
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let gold = Color::new(0.95, 0.75, 0.25, 1.0);
        let wizard_picture = usize::from(player.portrait) * 5 + usize::from(player.banner);
        self.draw_pic(&lay, "lilwiz", wizard_picture, 0, px + 11.0, py + 11.0);
        self.bm_text(
            view,
            4,
            gold,
            &player.name,
            px + 114.0,
            py + 11.0,
            Align::Center,
        );

        if ui.who != HUMAN {
            let relation = relation_word(self.game.relation(HUMAN, ui.who));
            let treaty = self.game.treaty(HUMAN, ui.who).name();
            let personality = self.game.personality(ui.who).name();
            let objective = self.game.objective(ui.who).map_or("", |o| o.name());
            for (i, (label, value)) in [
                ("Relations:", relation),
                ("Treaties:", treaty),
                ("Personality:", personality),
                ("Objective:", objective),
            ]
            .into_iter()
            .enumerate()
            {
                let y = py + 25.0 + 8.0 * i as f32;
                self.bm_text(
                    view,
                    1,
                    light,
                    &format!("{label} {value}"),
                    px + 59.0,
                    y,
                    Align::Left,
                );
            }
        }

        // Gold and mana, then the shelf of books.
        self.bm_text(
            view,
            1,
            light,
            &player.gold.to_string(),
            px + 31.0,
            py + 75.0,
            Align::Right,
        );
        self.bm_text(
            view,
            1,
            light,
            &player.mana.to_string(),
            px + 159.0,
            py + 75.0,
            Align::Right,
        );
        self.bm_text(view, 0, gold, "GP", px + 34.0, py + 75.0, Align::Left);
        self.bm_text(view, 0, gold, "MP", px + 162.0, py + 75.0, Align::Left);
        let books = book_entries(|r| player.magic.books_in(r));
        let left = px + 90.0 - books.len() as f32 * BOOK_STEP / 2.0;
        for (i, &entry) in books.iter().enumerate() {
            self.draw_pic(
                &lay,
                "magic",
                entry,
                0,
                left + i as f32 * BOOK_STEP,
                py + 66.0,
            );
        }
        let fame = self.game.fame(ui.who);
        if fame > 0 {
            self.bm_text(
                view,
                2,
                light,
                &fame.to_string(),
                px + 79.0,
                py + 96.0,
                Align::Right,
            );
            self.bm_text(view, 2, light, "Fame", px + 85.0, py + 96.0, Align::Left);
        }

        // The retorts as one sentence, then the heroes.
        let retorts: Vec<&str> = Retort::LISTED
            .into_iter()
            .filter(|&r| player.magic.has(r))
            .map(f4x_core::wizard::retort_name)
            .collect();
        let start_y = py + 111.0 - if retorts.len() > 5 { 2.0 } else { 0.0 };
        let pieces = {
            let art = &self.art;
            retort_pieces(&retorts, RETORT_WIDTH, |t| art.text_width(1, t))
        };
        for (text, x, line) in pieces {
            let y = start_y + RETORT_LINE * line as f32;
            self.bm_text(view, 1, light, &text, px + 13.0 + x, y, Align::Left);
        }
        self.bm_text(view, 2, gold, "Heroes", px + 74.0, py + 131.0, Align::Left);
        for (i, hero) in self.game.wizard_heroes(ui.who).iter().enumerate() {
            self.bm_text(
                view,
                1,
                light,
                hero,
                px + 13.0,
                py + 142.0 + 8.0 * i as f32,
                Align::Left,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Five pixels a letter.
    fn width(t: &str) -> f32 {
        t.chars().count() as f32 * 5.0
    }

    #[test]
    fn the_retorts_read_as_a_sentence() {
        let p = retort_pieces(&["Alchemy", "Warlord", "Famous"], 1000.0, width);
        let words: Vec<_> = p.iter().map(|(t, _, _)| t.as_str()).collect();
        assert_eq!(words, ["Alchemy, ", "Warlord and ", "Famous", "."]);
        assert!(p.iter().all(|&(_, _, line)| line == 0));
        assert_eq!(p[1].1, width("Alchemy, ") + 1.0);
    }

    #[test]
    fn one_retort_is_a_word_and_a_stop() {
        let p = retort_pieces(&["Archmage"], 1000.0, width);
        assert_eq!(p[0].0, "Archmage");
        assert_eq!(p[1].0, ".");
        assert!(retort_pieces(&[], 1000.0, width).is_empty());
    }

    #[test]
    fn a_long_sentence_wraps() {
        let p = retort_pieces(&["Alchemy", "Warlord", "Famous"], 60.0, width);
        // "Alchemy, " is 45 wide, "Warlord and " 60: it does not fit after the first.
        assert_eq!((p[0].2, p[1].2), (0, 1));
        assert_eq!(p[1].1, 0.0);
    }

    #[test]
    fn books_are_listed_realm_by_realm() {
        let books = |r: &str| match r {
            "sorcery" => 4,
            "death" => 1,
            _ => 0,
        };
        // Sorcery's three pictures in turn, then the first again; then death's first.
        assert_eq!(book_entries(books), [36, 37, 38, 36, 45]);
    }
}
