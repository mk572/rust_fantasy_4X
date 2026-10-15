//! The screen shown when the game ends: who won and how, and the human wizard's score.

use f4x_core::endgame::EndKind;
use macroquad::prelude::*;

use crate::ui::{button, Lines, GOLD, PANEL};
use crate::view::{App, Screen, HUMAN};

/// The headline and the reason, for the human player's point of view.
pub fn headline(won: bool, kind: EndKind, winner: &str) -> (String, String) {
    match (won, kind) {
        (true, EndKind::SpellOfMastery) => (
            "Victory!".into(),
            "You have finished the Spell of Mastery and rule all of Arcanus and Myrror.".into(),
        ),
        (true, EndKind::Conquest) => (
            "Victory!".into(),
            "Every rival wizard has been defeated.".into(),
        ),
        (false, EndKind::SpellOfMastery) => (
            "Defeat".into(),
            format!("{winner} has finished the Spell of Mastery."),
        ),
        (false, EndKind::Conquest) => (
            "Defeat".into(),
            format!("You have lost your last city to {winner}."),
        ),
    }
}

impl App {
    pub(crate) fn game_over_screen(&mut self) -> bool {
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, w, h, PANEL);
        let Some(over) = self.game.game_over else {
            self.screen = Screen::Map;
            return true;
        };
        let winner = self
            .game
            .player(over.winner)
            .map_or("A rival".to_string(), |p| p.name.clone());
        let (title, reason) = headline(over.winner == HUMAN, over.kind, &winner);
        let score = self.game.score(HUMAN);
        let x = (w / 2.0 - 220.0).max(20.0);
        let mut lines = Lines::new(x, 70.0);
        lines.size = 44.0;
        lines.line(&title, GOLD);
        lines.size = 22.0;
        lines.line(&reason, WHITE);
        lines.line(&format!("Turn {}", over.turn), GRAY);
        lines.gap(18.0);
        lines.line("Your score", GOLD);
        lines.size = 20.0;
        for (label, value) in [
            ("Spells known", score.spells),
            ("Townsfolk", score.townsfolk),
            ("Wizards defeated", score.conquests),
            ("Fame", score.fame),
            ("Time bonus", score.time),
            ("Spell of Mastery", score.mastery),
        ] {
            lines.line(&format!("{label:<20}{value:>8}"), WHITE);
        }
        if score.percent != 100 {
            lines.line(&format!("{:<20}{:>7}%", "Difficulty", score.percent), WHITE);
        }
        lines.line(&format!("{:<20}{:>8}", "Total", score.total()), GOLD);
        let y = lines.y + 24.0;
        if button(Rect::new(x, y, 200.0, 36.0), "New game", true) {
            let seed = self.seed.wrapping_add(1);
            self.new_map(seed);
        }
        if button(
            Rect::new(x + 215.0, y, 200.0, 36.0),
            "Look at the map",
            true,
        ) {
            self.game_over_seen = true;
            self.screen = Screen::Map;
        }
        if button(Rect::new(x + 430.0, y, 100.0, 36.0), "Quit", true) {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_headline_says_who_won_and_how() {
        let (t, r) = headline(true, EndKind::SpellOfMastery, "Rival");
        assert_eq!(t, "Victory!");
        assert!(r.contains("Spell of Mastery"));
        let (t, r) = headline(false, EndKind::Conquest, "Rival");
        assert_eq!(t, "Defeat");
        assert!(r.contains("Rival"));
    }
}
