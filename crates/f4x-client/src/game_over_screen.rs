//! The score screen shown when the game ends (task 134), the original's `End_Of_Game_Score`
//! (`SCORE.c`): the wizard's name, then a row of icons for each part of the score (a book for
//! each spell known, a townsfolk for every two thousand citizens, the banished wizards' portraits,
//! fame, time, the Spell of Mastery), the final score and a bar of `score / 40` up to 200. A click
//! or Esc goes on: the score is entered in the Hall of Fame (task 88) and the table is shown.

use crate::font::Style;
use crate::input::Input;
use f4x_core::game::endgame::EndKind;
use f4x_core::text::{Args, Texts};
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::hall_of_fame::{self, Entry};
use crate::view::{App, Screen, HUMAN};

/// The headline and the reason, for the human player's point of view.
pub fn headline(texts: &Texts, won: bool, kind: EndKind, winner: Args) -> (String, String) {
    let (title, reason) = match (won, kind) {
        (true, EndKind::SpellOfMastery) => ("end.victory", "end.won_mastery"),
        (true, EndKind::Conquest) => ("end.victory", "end.won_conquest"),
        (false, EndKind::SpellOfMastery) => ("end.defeat", "end.lost_mastery"),
        (false, EndKind::Conquest) => ("end.defeat", "end.lost_conquest"),
    };
    (texts.get(title, &Args::new()), texts.get(reason, &winner))
}

/// Where the left edge of the rows is.
const X: f32 = 47.0;
/// The bar's width in pixels at most (`score / 40`).
const BAR_MAX: i32 = 200;

/// Width of the bar for a score.
pub(crate) fn bar_width(score: i32) -> i32 {
    (score / 40).clamp(0, BAR_MAX)
}

impl App {
    /// For the screenshot check: end the game as a win by the Spell of Mastery now.
    pub fn open_test_game_over(&mut self) {
        self.game.game_over = Some(f4x_core::game::endgame::GameOver {
            winner: HUMAN,
            kind: EndKind::SpellOfMastery,
            turn: self.game.turn,
        });
    }

    pub(crate) fn game_over_screen(&mut self, input: &Input) -> bool {
        clear_background(BLACK);
        let Some(over) = self.game.game_over else {
            self.screen = Screen::Map;
            return true;
        };
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        if self.draw_pic(&lay, "halofam", 0, 0, 0.0, 0.0).is_none() {
            let r = lay.rect((0.0, 0.0, 320.0, 200.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.1, 0.08, 0.06, 1.0));
        }
        let ink = Color::new(0.95, 0.8, 0.45, 1.0);
        let wizard = self.game.wizard(HUMAN);
        let name = wizard.map_or_else(String::new, |w| w.name());
        let race = wizard.and_then(|w| w.capital_race());
        let race_icon = race.map_or(0, |r| 15 + r.index());
        self.draw_styled(view, &Style::outlined(3).center(), &name, 160.0, 3.0);
        let score = self.game.score(HUMAN);

        // One point for each spell known, a book each (fifty at most).
        self.draw_styled(
            view,
            &Style::nearest(2, ink),
            &self.text_with(
                "end.spells",
                &Args::new()
                    .count("n", score.spells)
                    .count("points", score.spells),
            ),
            X,
            15.0,
        );
        let (mut x, mut y) = (X, 25.0);
        for i in 0..score.spells.min(50) {
            self.draw_pic(&lay, "halofam", 30, 0, x, y);
            x += 8.0;
            if i == 24 {
                (x, y) = (X + 4.0, y + 3.0);
            }
        }
        // Half a point for each thousand citizens.
        self.draw_styled(
            view,
            &Style::nearest(2, ink),
            &self.text_with(
                "end.townsfolk",
                &Args::new()
                    .count("n", score.people)
                    .count("points", score.townsfolk),
            ),
            X,
            37.0,
        );
        let (mut x, mut y) = (X, 46.0);
        for i in 0..score.townsfolk.min(100) {
            self.draw_pic(&lay, "halofam", race_icon, 0, x, y);
            x += 9.0;
            if i > 0 && (i + 1) % 25 == 0 {
                x = if matches!((i + 1) / 25, 1 | 3) {
                    X + 4.0
                } else {
                    X
                };
                y += 5.0;
            }
        }
        // Fifty points for each wizard banished.
        let banished = self.game.banished_wizards(HUMAN);
        let count = banished.len();
        let args = Args::new()
            .count("n", count)
            .count("points", score.conquests);
        self.draw_styled(
            view,
            &Style::nearest(2, ink),
            &self.text_with("end.banished", &args),
            X,
            78.0,
        );
        for (i, id) in banished.iter().take(4).enumerate() {
            let portrait = self
                .game
                .wizard(*id)
                .map_or(0, |w| usize::from(w.portrait()));
            self.draw_pic(&lay, "halofam", 1 + portrait, 0, X + 35.0 * i as f32, 87.0);
        }
        // Fame, time and the Spell of Mastery.
        self.draw_pic(&lay, "halofam", 31, 0, X, 123.0);
        let fame = self.game.fame(HUMAN);
        self.draw_styled(
            view,
            &Style::nearest(2, ink),
            &self.text_with(
                "end.fame",
                &Args::new().count("n", fame).count("points", score.fame),
            ),
            X + 18.0,
            128.0,
        );
        self.draw_pic(&lay, "halofam", 29, 0, X, 142.0);
        let sign = if score.time >= 0 { "+" } else { "" };
        let time = self.text_with(
            "end.year",
            &Args::new()
                .count("year", over.turn / 12 + 1400)
                .text("points", format!("{sign}{}", score.time)),
        );
        self.draw_styled(view, &Style::nearest(2, ink), &time, X + 18.0, 147.0);
        self.draw_pic(&lay, "halofam", 32, 0, X, 161.0);
        self.draw_styled(
            view,
            &Style::nearest(2, ink),
            &self.text_with("end.mastery", &Args::new().count("points", score.mastery)),
            X + 18.0,
            166.0,
        );
        let total = score.total();
        self.draw_styled(
            view,
            &Style::nearest(2, ink),
            &self.text_with("end.final_score", &Args::new().count("n", total)),
            X,
            180.0,
        );
        // The bar: a frame, and green for each forty points.
        let frame = lay.rect((47.0, 189.0, 205.0, 9.0));
        draw_rectangle_lines(
            frame.x,
            frame.y,
            frame.w,
            frame.h,
            1.0,
            Color::new(0.5, 0.4, 0.3, 1.0),
        );
        let fill = lay.rect((49.0, 191.0, bar_width(total) as f32 + 1.0, 5.0));
        draw_rectangle(
            fill.x,
            fill.y,
            fill.w,
            fill.h,
            Color::new(0.3, 0.75, 0.2, 1.0),
        );

        let leave = input.key_pressed(KeyCode::Escape)
            || input.key_pressed(KeyCode::Enter)
            || input.mouse_pressed(MouseButton::Left);
        if leave {
            // The score goes in the table once, then the table is shown.
            let mut table = hall_of_fame::load(std::path::Path::new(hall_of_fame::FILE));
            let race = race.map_or_else(String::new, |r| {
                self.game.content().rules.races.get(r).name.clone()
            });
            if hall_of_fame::insert(
                &mut table,
                Entry {
                    name,
                    race,
                    score: total,
                },
            )
            .is_some()
            {
                let _ = hall_of_fame::save(std::path::Path::new(hall_of_fame::FILE), &table);
            }
            // Until the win and lose animations exist, the log says who won and how.
            let winner = self.wizard_arg(Args::new(), "winner", over.winner);
            let (title, reason) = headline(self.texts(), over.winner == HUMAN, over.kind, winner);
            self.log.push(self.text_with(
                "end.log",
                &Args::new().text("title", title).text("reason", reason),
            ));
            self.game_over_seen = true;
            self.open_hall_of_fame();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_grows_a_pixel_for_forty_points_up_to_two_hundred() {
        assert_eq!(bar_width(-50), 0);
        assert_eq!(bar_width(400), 10);
        assert_eq!(bar_width(20000), 200);
    }

    #[test]
    fn the_headline_says_who_won_and_how() {
        let texts = f4x_data::load_texts(&[std::path::Path::new("../../content")], "en").unwrap();
        let rival = || Args::new().text("winner", "Rival");
        let (t, r) = headline(&texts, true, EndKind::SpellOfMastery, rival());
        assert_eq!(t, "Victory!");
        assert!(r.contains("Spell of Mastery"));
        let (t, r) = headline(&texts, false, EndKind::Conquest, rival());
        assert_eq!(t, "Defeat");
        assert!(r.contains("Rival"));
    }
}
