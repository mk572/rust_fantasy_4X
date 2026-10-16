//! The new-game options step: difficulty, opponents, land size and node strength, shown
//! before the wizard is made (the original's `Newgame_Screen_0`).

use f4x_core::setup::{LAND_LARGE, LAND_MEDIUM, LAND_SMALL};
use macroquad::prelude::*;

use crate::ui::{button, GOLD, PANEL};
use crate::view::{App, Screen};

pub(crate) const DIFFICULTIES: [&str; 5] = ["Intro", "Easy", "Average", "Hard", "Impossible"];
pub(crate) const LAND_SIZES: [(&str, u8); 3] = [
    ("Small", LAND_SMALL),
    ("Medium", LAND_MEDIUM),
    ("Large", LAND_LARGE),
];
pub(crate) const MAGIC_LEVELS: [&str; 3] = ["Weak", "Normal", "Powerful"];

/// The choices made on the options step; the defaults are the game's usual settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OptionsState {
    /// Index into [`DIFFICULTIES`].
    pub difficulty: u8,
    /// Computer wizards, 1 to 4.
    pub opponents: u8,
    /// Index into [`LAND_SIZES`].
    pub land: u8,
    /// Index into [`MAGIC_LEVELS`].
    pub magic: u8,
}

impl Default for OptionsState {
    fn default() -> Self {
        Self {
            difficulty: 2,
            opponents: 1,
            land: 2,
            magic: 1,
        }
    }
}

impl OptionsState {
    pub fn land_percent(&self) -> u8 {
        LAND_SIZES[usize::from(self.land).min(2)].1
    }

    /// Wizards in the game: the player and the opponents.
    pub fn wizards(&self) -> u8 {
        self.opponents.clamp(1, 4) + 1
    }
}

impl App {
    pub fn open_options(&mut self) {
        self.screen = Screen::Options;
    }

    pub(crate) fn options_screen(&mut self) {
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, w, h, PANEL);
        let x = (w / 2.0 - 260.0).max(30.0);
        draw_text("Game options", x, 80.0, 36.0, GOLD);
        let mut y = 130.0;
        let o = &mut self.options;
        o.difficulty = choose(x, &mut y, "Difficulty", &DIFFICULTIES, o.difficulty);
        let opponents = ["1", "2", "3", "4"];
        o.opponents = 1 + choose(
            x,
            &mut y,
            "Opponents",
            &opponents,
            o.opponents.clamp(1, 4) - 1,
        );
        let lands = LAND_SIZES.map(|(name, _)| name);
        o.land = choose(x, &mut y, "Land size", &lands, o.land);
        o.magic = choose(x, &mut y, "Magic power", &MAGIC_LEVELS, o.magic);
        let pct = o.land_percent();
        draw_text(
            format!("Land covers {pct}% of each plane; magic power sets how strong nodes are."),
            x,
            y + 10.0,
            18.0,
            GRAY,
        );
        if button(Rect::new(x, y + 50.0, 150.0, 40.0), "Cancel", true)
            || is_key_pressed(KeyCode::Escape)
        {
            self.open_main_menu();
        } else if button(Rect::new(x + 170.0, y + 50.0, 150.0, 40.0), "OK", true)
            || is_key_pressed(KeyCode::Enter)
        {
            self.open_creation();
        }
    }
}

impl App {
    /// What the game is played with (task 94), opened from the Settings screen's Game button.
    /// In a game it shows the settings and changes nothing; from the start menu there is no
    /// game, so it is the options for the next one. Back returns to the Settings screen.
    pub(crate) fn game_settings_screen(&mut self) {
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, w, h, PANEL);
        let x = (w / 2.0 - 260.0).max(30.0);
        draw_text("Settings", x, 80.0, 36.0, GOLD);
        let mut y = 130.0;
        let back = if self.menu.from_menu {
            draw_text("For the next game", x, y - 6.0, 20.0, GRAY);
            y += 16.0;
            let o = &mut self.options;
            o.difficulty = choose(x, &mut y, "Difficulty", &DIFFICULTIES, o.difficulty);
            let opponents = ["1", "2", "3", "4"];
            o.opponents = 1 + choose(
                x,
                &mut y,
                "Opponents",
                &opponents,
                o.opponents.clamp(1, 4) - 1,
            );
            let lands = LAND_SIZES.map(|(name, _)| name);
            o.land = choose(x, &mut y, "Land size", &lands, o.land);
            o.magic = choose(x, &mut y, "Magic power", &MAGIC_LEVELS, o.magic);
            button(Rect::new(x, y + 20.0, 150.0, 40.0), "Back", true)
        } else {
            draw_text("This game (cannot be changed)", x, y - 6.0, 20.0, GRAY);
            y += 16.0;
            for line in settings_lines(self.game.settings()) {
                draw_text(&line, x, y + 20.0, 22.0, WHITE);
                y += 30.0;
            }
            y += 10.0;
            button(Rect::new(x, y + 20.0, 150.0, 40.0), "Back", true)
        };
        if back || is_key_pressed(KeyCode::Escape) {
            self.screen = Screen::Settings;
        }
    }
}

/// What a game was made with, one line each (the Settings screen of a running game).
pub(crate) fn settings_lines(s: &f4x_core::settings::GameSettings) -> Vec<String> {
    let b = &s.basic;
    fn name<'a>(names: &[&'a str], i: u8) -> &'a str {
        names.get(usize::from(i)).copied().unwrap_or("?")
    }
    let mut lines = vec![
        format!(
            "Difficulty: {}{}",
            name(&DIFFICULTIES, b.difficulty),
            if s.is_level() { "" } else { " (custom)" }
        ),
        format!("Opponents: {}", b.opponents),
        format!("Land: {}% of each plane", b.land_percent),
        format!("Magic power: {}", name(&MAGIC_LEVELS, b.magic_strength)),
        format!(
            "Starting gold: {} for you, {} for the computer wizards",
            s.wizards.starting_gold.human, s.wizards.starting_gold.ai
        ),
    ];
    if s.ai.production_percent != 100 || s.ai.gold_percent != 100 {
        lines.push(format!(
            "Computer wizards' cities: production {}%, gold {}%, research {}%",
            s.ai.production_percent, s.ai.gold_percent, s.ai.research_percent
        ));
    }
    lines.push(format!("Final score: {}%", s.scoring.score_percent));
    lines
}

/// A row of radio buttons; returns the chosen index.
fn choose(x: f32, y: &mut f32, label: &str, names: &[&str], current: u8) -> u8 {
    draw_text(label, x, *y + 22.0, 24.0, WHITE);
    let mut chosen = current;
    for (i, name) in names.iter().enumerate() {
        let r = Rect::new(x + 170.0 + i as f32 * 112.0, *y, 104.0, 30.0);
        let label = if i == usize::from(current) {
            format!("[{name}]")
        } else {
            (*name).to_string()
        };
        if button(r, &label, true) {
            chosen = i as u8;
        }
    }
    *y += 46.0;
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_a_normal_two_wizard_game() {
        let o = OptionsState::default();
        assert_eq!(o.wizards(), 2);
        assert_eq!(o.land_percent(), LAND_LARGE);
        assert_eq!(DIFFICULTIES[usize::from(o.difficulty)], "Average");
    }

    #[test]
    fn a_running_games_settings_read_as_words() {
        let lines = settings_lines(&f4x_core::settings::GameSettings::for_difficulty(4));
        assert_eq!(lines[0], "Difficulty: Impossible");
        assert!(lines.iter().any(|l| l.contains("25 for you, 150")));
        assert!(lines.iter().any(|l| l.contains("production 400%")));
        let mut custom = f4x_core::settings::GameSettings::default();
        custom.wizards.starting_gold.human = 300;
        assert_eq!(settings_lines(&custom)[0], "Difficulty: Average (custom)");
    }

    #[test]
    fn opponents_are_one_to_four() {
        let o = |n| OptionsState {
            opponents: n,
            ..Default::default()
        };
        assert_eq!(o(0).wizards(), 2);
        assert_eq!(o(4).wizards(), 5);
        assert_eq!(o(9).wizards(), 5);
    }
}
