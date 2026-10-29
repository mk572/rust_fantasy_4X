//! The new-game options step: difficulty, opponents, land size and node strength, shown
//! before the wizard is made (the original's `Newgame_Screen_0`).

use crate::input::Input;
use f4x_core::game::setting::SettingsDef;
use f4x_core::text::{Args, Texts};
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::ui::{button, GOLD, PANEL};
use crate::view::{App, Screen};

/// The message keys of the difficulty names, easiest first.
pub(crate) const DIFFICULTIES: [&str; 5] = [
    "options.intro",
    "options.easy",
    "options.average",
    "options.hard",
    "options.impossible",
];
/// The names on the original's options buttons.
const OPTION_DIFFICULTIES: [&str; 5] = [
    "options.intro",
    "options.easy",
    "options.normal",
    "options.hard",
    "options.impossible",
];
const OPPONENT_NAMES: [&str; 9] = [
    "options.one",
    "options.two",
    "options.three",
    "options.four",
    "options.five",
    "options.six",
    "options.seven",
    "options.eight",
    "options.nine",
];
pub(crate) const MAGIC_LEVELS: [&str; 3] = ["options.weak", "options.normal", "options.powerful"];

/// The choices made on the options step; they start as `settings.ron` says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OptionsState {
    /// Index into [`DIFFICULTIES`].
    pub difficulty: u8,
    /// Computer wizards, 1 to 4.
    pub opponents: u8,
    /// Index into `settings.land_sizes`.
    pub land: u8,
    /// Index into [`MAGIC_LEVELS`].
    pub magic: u8,
    /// Index into `settings.map_sizes`.
    pub map: u8,
    /// Make the land with the original's generator.
    pub original_generator: bool,
}

impl OptionsState {
    /// The choices of `settings.new_game`; a land share that is none of the three sizes takes
    /// the nearest size.
    pub fn from_settings(settings: &SettingsDef) -> Self {
        let b = settings.new_game;
        let land = (0..settings.land_sizes.len())
            .min_by_key(|&i| settings.land_sizes[i].land_percent.abs_diff(b.land_percent))
            .unwrap_or(0);
        Self {
            difficulty: b.difficulty.min(4),
            opponents: b.opponents.clamp(1, settings.max_opponents),
            land: land as u8,
            magic: b.magic_strength.min(2),
            map: (0..settings.map_sizes.len())
                .min_by_key(|&i| {
                    let m = &settings.map_sizes[i];
                    m.width.abs_diff(b.map_width) + m.height.abs_diff(b.map_height)
                })
                .unwrap_or(0) as u8,
            original_generator: b.original_map_generator,
        }
    }

    /// The squares across and down of the chosen map size.
    pub fn map_size(&self, settings: &SettingsDef) -> (u16, u16) {
        let sizes = &settings.map_sizes;
        let m = &sizes[usize::from(self.map).min(sizes.len() - 1)];
        (m.width, m.height)
    }

    /// The land share of the chosen size.
    pub fn land_percent(&self, settings: &SettingsDef) -> u8 {
        let sizes = &settings.land_sizes;
        sizes[usize::from(self.land).min(sizes.len() - 1)].land_percent
    }
}

impl App {
    pub fn open_options(&mut self) {
        self.screen = Screen::Options;
    }

    /// The options step on the original's picture (`Newgame_Screen_0`): four buttons on the
    /// right whose text is the current choice, a click goes on to the next one; OK and Cancel
    /// under them. Positions are the original's.
    pub(crate) fn options_screen(&mut self, input: &Input) {
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
        let settings = &self.game.content().settings;
        let (most, sizes) = (settings.max_opponents, settings.land_sizes.len() as u8);
        let land = settings.land_sizes[usize::from(o.land).min(sizes as usize - 1)]
            .name
            .clone();
        let labels = [
            self.text(OPTION_DIFFICULTIES[usize::from(o.difficulty).min(4)]),
            self.text(OPPONENT_NAMES[usize::from(o.opponents.clamp(1, most)) - 1]),
            land,
            self.text(MAGIC_LEVELS[usize::from(o.magic).min(2)]),
        ];
        let mouse: Vec2 = input.mouse_position().into();
        let clicked = input.mouse_pressed(MouseButton::Left);
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
                &label,
                283.0 + shift,
                text_y + 1.0 + shift,
                Align::Center,
            );
            self.bm_text(
                view,
                3,
                font,
                &label,
                282.0 + shift,
                text_y + shift,
                Align::Center,
            );
            if hover && clicked {
                changed = Some(i);
            }
        }
        // The original's picture has four buttons only: the map size and the map generator are
        // two more rows of text under them, the name on the left as the four have it and the
        // choice on the right.
        let gold = Color::new(0.95, 0.75, 0.2, 1.0);
        let (w, h) = o.map_size(&self.game.content().settings);
        let rows = [
            (
                143.0,
                self.text("options.map_size").to_string(),
                format!("{w} x {h}"),
            ),
            (
                156.0,
                self.text("options.generator").to_string(),
                self.text(if o.original_generator {
                    "options.generator_original"
                } else {
                    "options.generator_custom"
                })
                .to_string(),
            ),
        ];
        for (i, (y, name, value)) in rows.into_iter().enumerate() {
            let hover = lay.rect((166.0, y - 1.0, 150.0, 12.0)).contains(mouse);
            let shift = if hover { 1.0 } else { 0.0 };
            let ink = if hover {
                Color::new(1.0, 1.0, 0.7, 1.0)
            } else {
                font
            };
            self.bm_text(view, 3, shadow, &name, 168.0, y + 1.0, Align::Left);
            self.bm_text(view, 3, gold, &name, 167.0, y, Align::Left);
            self.bm_text(
                view,
                3,
                shadow,
                &value,
                283.0 + shift,
                y + 1.0 + shift,
                Align::Center,
            );
            self.bm_text(
                view,
                3,
                ink,
                &value,
                282.0 + shift,
                y + shift,
                Align::Center,
            );
            if hover && clicked {
                changed = Some(4 + i);
            }
        }
        // Too little land for each wizard: the game still starts, but the player is told.
        let crowded = {
            let (w, h) = o.map_size(&self.game.content().settings);
            let basic = f4x_core::game::setting::BasicSettings {
                opponents: o.opponents,
                map_width: w,
                map_height: h,
                ..Default::default()
            };
            basic.crowding()
        };
        if let Some(each) = crowded {
            let text = self.text_with("options.crowded", &Args::new().count("n", each));
            let red = Color::new(1.0, 0.45, 0.4, 1.0);
            for (i, line) in self.bm_wrap(1, &text, 150.0).iter().enumerate() {
                self.bm_text(
                    view,
                    1,
                    red,
                    line,
                    167.0,
                    170.0 + 8.0 * i as f32,
                    Align::Left,
                );
            }
        }
        let o = &mut self.options;
        let map_sizes = self.game.content().settings.map_sizes.len() as u8;
        match changed {
            Some(4) => o.map = (o.map + 1) % map_sizes,
            Some(5) => o.original_generator = !o.original_generator,
            Some(0) => o.difficulty = (o.difficulty + 1) % 5,
            Some(1) => o.opponents = o.opponents.clamp(1, most) % most + 1,
            Some(2) => o.land = (o.land + 1) % sizes,
            Some(3) => o.magic = (o.magic + 1) % 3,
            _ => {}
        }
        let ok = self.newgame_button(&lay, 2, 252.0, 179.0, input);
        let cancel = self.newgame_button(&lay, 3, 171.0, 179.0, input);
        if cancel || input.key_pressed(KeyCode::Escape) {
            self.open_main_menu();
        } else if ok || input.key_pressed(KeyCode::Enter) {
            self.open_creation();
        }
    }

    /// A two-frame button of `newgame.lbx` (the second frame while pressed); true when clicked.
    pub(crate) fn newgame_button(
        &mut self,
        lay: &Layout,
        entry: usize,
        x: f32,
        y: f32,
        input: &Input,
    ) -> bool {
        self.pic_button_on(lay, ("newgame", entry), (x, y), input)
    }
}

impl App {
    /// What the game is played with (task 94), opened from the Settings screen's Game button.
    /// In a game it shows the settings and changes nothing; from the start menu there is no
    /// game, so it is the options for the next one. Back returns to the Settings screen.
    pub(crate) fn game_settings_screen(&mut self, input: &Input) {
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, w, h, PANEL);
        let x = (w / 2.0 - 260.0).max(30.0);
        draw_text(self.text("options.settings"), x, 80.0, 36.0, GOLD);
        let mut y = 130.0;
        let back = if self.menu.from_menu {
            draw_text(self.text("options.next_game"), x, y - 6.0, 20.0, GRAY);
            y += 16.0;
            let difficulties = DIFFICULTIES.map(|k| self.text(k));
            let magic_levels = MAGIC_LEVELS.map(|k| self.text(k));
            let generator_label = self.text("options.generator");
            let (difficulty, opponents_label, land_label, magic_label, map_label, back_label) = (
                self.text("options.difficulty"),
                self.text("options.opponents"),
                self.text("options.land_size"),
                self.text("options.magic_power"),
                self.text("options.map_size"),
                self.text("options.back"),
            );
            let o = &mut self.options;
            let settings = &self.game.content().settings;
            o.difficulty = choose(x, &mut y, &difficulty, &difficulties, o.difficulty, input);
            let opponents: Vec<String> = (1..=settings.max_opponents)
                .map(|n| n.to_string())
                .collect();
            o.opponents = 1 + choose(
                x,
                &mut y,
                &opponents_label,
                &opponents,
                o.opponents.clamp(1, settings.max_opponents) - 1,
                input,
            );
            let lands: Vec<String> = settings.land_sizes.iter().map(|s| s.name.clone()).collect();
            o.land = choose(x, &mut y, &land_label, &lands, o.land, input);
            o.magic = choose(x, &mut y, &magic_label, &magic_levels, o.magic, input);
            let maps: Vec<String> = settings.map_sizes.iter().map(|s| s.name.clone()).collect();
            o.map = choose(x, &mut y, &map_label, &maps, o.map, input);
            let generators = [
                self.texts().get("options.generator_custom", &Args::new()),
                self.texts().get("options.generator_original", &Args::new()),
            ];
            let o = &mut self.options;
            o.original_generator = choose(
                x,
                &mut y,
                &generator_label,
                &generators,
                u8::from(o.original_generator),
                input,
            ) == 1;
            let seed = self.text_with(
                "options.line_seed",
                &Args::new().text("seed", self.seed.to_string()),
            );
            draw_text(&seed, x, y + 20.0, 22.0, WHITE);
            y += 30.0;
            button(
                Rect::new(x, y + 20.0, 150.0, 40.0),
                &back_label,
                true,
                input,
            )
        } else {
            draw_text(self.text("options.this_game"), x, y - 6.0, 20.0, GRAY);
            y += 16.0;
            let lines = settings_lines(
                self.texts(),
                self.game.settings(),
                &self.game.content().settings,
            );
            for line in lines {
                draw_text(&line, x, y + 20.0, 22.0, WHITE);
                y += 30.0;
            }
            let seed = self.texts().get(
                "options.line_seed",
                &Args::new().text("seed", self.game.seed().to_string()),
            );
            draw_text(&seed, x, y + 20.0, 22.0, WHITE);
            y += 40.0;
            button(
                Rect::new(x, y + 20.0, 150.0, 40.0),
                &self.text("options.back"),
                true,
                input,
            )
        };
        if back || input.key_pressed(KeyCode::Escape) {
            self.screen = Screen::Settings;
        }
    }
}

/// What a game was made with, one line each (the Settings screen of a running game).
pub(crate) fn settings_lines(
    texts: &Texts,
    s: &f4x_core::game::setting::GameSettings,
    def: &f4x_core::game::setting::SettingsDef,
) -> Vec<String> {
    let b = &s.basic;
    let name = |names: &[&str], i: u8| {
        names
            .get(usize::from(i))
            .map_or_else(|| "?".to_string(), |k| texts.get(k, &Args::new()))
    };
    let custom = if s.is_level(def) {
        String::new()
    } else {
        texts.get("options.custom", &Args::new())
    };
    let mut lines = vec![
        texts.get(
            "options.line_difficulty",
            &Args::new()
                .text("name", name(&DIFFICULTIES, b.difficulty))
                .text("custom", custom),
        ),
        texts.get(
            "options.line_opponents",
            &Args::new().count("n", b.opponents),
        ),
        texts.get("options.line_land", &Args::new().count("n", b.land_percent)),
        texts.get(
            "options.line_map",
            &Args::new()
                .count("w", i32::from(b.map_width))
                .count("h", i32::from(b.map_height)),
        ),
        texts.get(
            "options.line_generator",
            &Args::new().text(
                "name",
                texts.get(
                    if b.original_map_generator {
                        "options.generator_original"
                    } else {
                        "options.generator_custom"
                    },
                    &Args::new(),
                ),
            ),
        ),
        texts.get(
            "options.line_magic",
            &Args::new().text("name", name(&MAGIC_LEVELS, b.magic_strength)),
        ),
        texts.get(
            "options.line_gold",
            &Args::new()
                .count("you", s.wizards.starting_gold.human)
                .count("ai", s.wizards.starting_gold.ai),
        ),
    ];
    if s.ai.production_percent != 100 || s.ai.gold_percent != 100 {
        lines.push(
            texts.get(
                "options.line_ai_cities",
                &Args::new()
                    .count("production", s.ai.production_percent)
                    .count("gold", s.ai.gold_percent)
                    .count("research", s.ai.research_percent),
            ),
        );
    }
    lines.push(texts.get(
        "options.line_score",
        &Args::new().count("n", s.scoring.score_percent),
    ));
    lines
}

/// A row of radio buttons; returns the chosen index.
fn choose(x: f32, y: &mut f32, label: &str, names: &[String], current: u8, input: &Input) -> u8 {
    draw_text(label, x, *y + 22.0, 24.0, WHITE);
    let mut chosen = current;
    for (i, name) in names.iter().enumerate() {
        let r = Rect::new(x + 170.0 + i as f32 * 112.0, *y, 104.0, 30.0);
        let label = if i == usize::from(current) {
            format!("[{name}]")
        } else {
            name.clone()
        };
        if button(r, &label, true, input) {
            chosen = i as u8;
        }
    }
    *y += 46.0;
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped_settings() -> f4x_core::game::setting::SettingsDef {
        f4x_data::load_settings(&[std::path::Path::new("../../content")]).unwrap()
    }

    #[test]
    fn defaults_are_a_normal_two_wizard_game() {
        let settings = shipped_settings();
        let o = OptionsState::from_settings(&settings);
        assert_eq!(o.opponents, 1);
        assert_eq!(o.land_percent(&settings), f4x_core::game::setup::LAND_LARGE);
        assert_eq!(DIFFICULTIES[usize::from(o.difficulty)], "options.average");
    }

    #[test]
    fn a_running_games_settings_read_as_words() {
        let settings = shipped_settings();
        let level = |d| f4x_core::game::setting::GameSettings::for_difficulty(&settings, d);
        let texts = f4x_data::load_texts(&[std::path::Path::new("../../content")], "en").unwrap();
        let lines = settings_lines(&texts, &level(4), &settings);
        assert_eq!(lines[0], "Difficulty: Impossible");
        assert!(lines.iter().any(|l| l.contains("25 for you, 150")));
        assert!(lines.iter().any(|l| l.contains("production 400%")));
        let mut custom = level(2);
        custom.wizards.starting_gold.human = 300;
        assert_eq!(
            settings_lines(&texts, &custom, &settings)[0],
            "Difficulty: Average (custom)"
        );
    }
}
