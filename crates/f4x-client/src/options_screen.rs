//! The new-game options step: difficulty, opponents, land size and node strength, shown
//! before the wizard is made (the original's `Newgame_Screen_0`).

use f4x_core::setup::{LAND_LARGE, LAND_MEDIUM, LAND_SMALL};
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::ui::{button, GOLD, PANEL};
use crate::view::{App, Screen};

pub(crate) const DIFFICULTIES: [&str; 5] = ["Intro", "Easy", "Average", "Hard", "Impossible"];
pub(crate) const LAND_SIZES: [(&str, u8); 3] = [
    ("Small", LAND_SMALL),
    ("Medium", LAND_MEDIUM),
    ("Large", LAND_LARGE),
];
/// The names on the original's options buttons.
const OPTION_DIFFICULTIES: [&str; 5] = ["Intro", "Easy", "Normal", "Hard", "Impossible"];
const OPPONENT_NAMES: [&str; 4] = ["One", "Two", "Three", "Four"];
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

    /// The options step on the original's picture (`Newgame_Screen_0`): four buttons on the
    /// right whose text is the current choice, a click goes on to the next one; OK and Cancel
    /// under them. Positions are the original's.
    pub(crate) fn options_screen(&mut self) {
        const ROWS: [(f32, f32); 4] = [(39.0, 43.0), (66.0, 70.0), (93.0, 96.0), (120.0, 123.0)];
        clear_background(BLACK);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        if self.draw_pic(&lay, "newgame", 0, 0, 0.0, 0.0).is_none() {
            let r = lay.rect((0.0, 0.0, 320.0, 200.0));
            draw_rectangle(r.x, r.y, r.w, r.h, PANEL);
        }
        self.draw_pic(&lay, "newgame", 1, 0, 165.0, 0.0);
        let o = self.options.clone();
        let labels = [
            OPTION_DIFFICULTIES[usize::from(o.difficulty).min(4)],
            OPPONENT_NAMES[usize::from(o.opponents.clamp(1, 4)) - 1],
            LAND_SIZES[usize::from(o.land).min(2)].0,
            MAGIC_LEVELS[usize::from(o.magic).min(2)],
        ];
        let mouse: Vec2 = mouse_position().into();
        let clicked = is_mouse_button_pressed(MouseButton::Left);
        let shadow = Color::new(0.1, 0.1, 0.12, 1.0);
        let font = Color::new(0.85, 0.85, 0.95, 1.0);
        let mut changed = None;
        for (i, ((y, text_y), label)) in ROWS.into_iter().zip(labels).enumerate() {
            let hover = lay.rect((250.0, y - 1.0, 66.0, 17.0)).contains(mouse);
            let shift = if hover { 1.0 } else { 0.0 };
            self.draw_pic(&lay, "newgame", 4 + i, 0, 251.0 + shift, y + shift);
            self.bm_text(
                view,
                3,
                shadow,
                label,
                283.0 + shift,
                text_y + 1.0 + shift,
                Align::Center,
            );
            self.bm_text(
                view,
                3,
                font,
                label,
                282.0 + shift,
                text_y + shift,
                Align::Center,
            );
            if hover && clicked {
                changed = Some(i);
            }
        }
        let o = &mut self.options;
        match changed {
            Some(0) => o.difficulty = (o.difficulty + 1) % 5,
            Some(1) => o.opponents = o.opponents.clamp(1, 4) % 4 + 1,
            Some(2) => o.land = (o.land + 1) % 3,
            Some(3) => o.magic = (o.magic + 1) % 3,
            _ => {}
        }
        let ok = self.newgame_button(&lay, 2, 252.0, 179.0);
        let cancel = self.newgame_button(&lay, 3, 171.0, 179.0);
        if cancel || is_key_pressed(KeyCode::Escape) {
            self.open_main_menu();
        } else if ok || is_key_pressed(KeyCode::Enter) {
            self.open_creation();
        }
    }

    /// A two-frame button of `newgame.lbx` (the second frame while pressed); true when clicked.
    pub(crate) fn newgame_button(&mut self, lay: &Layout, entry: usize, x: f32, y: f32) -> bool {
        let Some(tex) = self.art.picture("newgame", entry, 0).cloned() else {
            return false;
        };
        let r = lay.rect((x, y, tex.width(), tex.height()));
        let hover = r.contains(mouse_position().into());
        let frame = usize::from(hover && is_mouse_button_down(MouseButton::Left));
        self.draw_pic(lay, "newgame", entry, frame, x, y);
        hover && is_mouse_button_pressed(MouseButton::Left)
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
