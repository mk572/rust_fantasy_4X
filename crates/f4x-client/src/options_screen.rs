//! The new-game options step: difficulty, opponents, land size and node strength, shown
//! before the wizard is made (the original's `Newgame_Screen_0`).

use crate::input::Input;
use crate::page::Page;
use crate::text::Args;
use f4x_core::game::setting::{BasicSettings, GameSettings, SettingsDef};
use macroquad::prelude::*;

use crate::font::Ink;
use crate::font_ramp;
use crate::frame::{Align, Layout};
use crate::newgame_flow::ng_ink;
use crate::ui::{self, PANEL};
use crate::view::{App, Screen};

/// The message keys of the difficulty names, easiest first.
pub(crate) const DIFFICULTIES: &[&str] = &[
    "options.intro",
    "options.easy",
    "options.average",
    "options.hard",
    "options.impossible",
    "options.extreme",
];
/// The names on the original's options buttons.
const OPTION_DIFFICULTIES: &[&str] = &[
    "options.intro",
    "options.easy",
    "options.normal",
    "options.hard",
    "options.impossible",
    "options.extreme",
];
const OPPONENT_NAMES: &[&str] = &[
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
pub(crate) const MAGIC_LEVELS: &[&str] = &["options.weak", "options.normal", "options.powerful"];

/// The choices made on the options step; they start as `rules_settings.ron` says.
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
    /// The settings of the Custom screen, when the player kept some: they replace the level's
    /// figures for the new game.
    pub custom: Option<f4x_core::game::setting::GameSettings>,
    /// The rule set the Custom screen's dropdown shows.
    pub preset: crate::custom_screen::Preset,
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
            difficulty: b.difficulty.min(f4x_def::settings::MAX_DIFFICULTY),
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
            custom: None,
            preset: Default::default(),
        }
    }

    /// The settings these choices make: the level's figures with the basic choices on top, or
    /// the Custom screen's record when the player kept one.
    pub fn settings(&self, settings: &SettingsDef) -> GameSettings {
        if let Some(custom) = self.custom {
            return custom;
        }
        let mut s = GameSettings::for_difficulty(settings, self.difficulty);
        s.basic.opponents = self.opponents.clamp(1, settings.max_opponents);
        s.basic.land_percent = self.land_percent(settings);
        s.basic.magic_strength = self.magic;
        (s.basic.map_width, s.basic.map_height) = self.map_size(settings);
        let (towers, cities) = self.map_counts(settings);
        s.basic.towers = Some(towers);
        s.basic.neutral_cities = Some(cities);
        s.basic.original_map_generator = self.original_generator;
        s
    }

    /// Keep the Custom screen's record, and show its basic choices on the options step: the
    /// opponents, the magic power and the generator as they are, the land and the map the
    /// nearest size.
    pub fn keep_custom(
        &mut self,
        custom: GameSettings,
        preset: crate::custom_screen::Preset,
        settings: &SettingsDef,
    ) {
        self.preset = preset;
        self.difficulty = custom
            .basic
            .difficulty
            .min(f4x_def::settings::MAX_DIFFICULTY);
        let b = custom.basic;
        let nearest = OptionsState::from_settings(&SettingsDef {
            new_game: b,
            ..settings.clone()
        });
        self.opponents = b.opponents.clamp(1, settings.max_opponents);
        self.magic = b.magic_strength.min(2);
        self.original_generator = b.original_map_generator;
        self.land = nearest.land;
        self.map = nearest.map;
        self.custom = Some(custom);
    }

    /// A change on the options step itself: the Custom record follows the basic choices, except
    /// a new difficulty, which fills in its figures again and drops the record.
    pub fn changed_here(&mut self, difficulty_changed: bool, settings: &SettingsDef) {
        if difficulty_changed {
            self.custom = None;
            self.preset = Default::default();
            // Extreme plays the v1.6 rules.
            if self.difficulty == f4x_def::settings::EXTREME {
                let base = self.settings(settings);
                self.preset = crate::custom_screen::Preset::V160;
                self.custom = Some(self.preset.applied(base, settings));
            }
        } else if let Some(custom) = self.custom {
            let mut fresh = self.clone();
            fresh.custom = None;
            let base = fresh.settings(settings);
            self.custom = Some(GameSettings {
                basic: BasicSettings {
                    diagonal_extra_cost: custom.basic.diagonal_extra_cost,
                    ..base.basic
                },
                ..custom
            });
        }
    }

    /// The squares across and down of the chosen map size.
    pub fn map_size(&self, settings: &SettingsDef) -> (u16, u16) {
        let sizes = &settings.map_sizes;
        let m = &sizes[usize::from(self.map).min(sizes.len() - 1)];
        (m.width, m.height)
    }

    /// The towers and the most neutral cities on each plane of the chosen map size.
    pub fn map_counts(&self, settings: &SettingsDef) -> (u8, u8) {
        let sizes = &settings.map_sizes;
        let m = &sizes[usize::from(self.map).min(sizes.len() - 1)];
        (m.towers, m.neutral_cities)
    }

    /// The land share of the chosen size.
    pub fn land_percent(&self, settings: &SettingsDef) -> u8 {
        let sizes = &settings.land_sizes;
        sizes[usize::from(self.land).min(sizes.len() - 1)].land_percent
    }
}

impl App {
    pub fn open_options(&mut self) {
        self.open_page(Screen::Options, OptionsPage);
    }
}

/// The options step on the original's picture (`Newgame_Screen_0`): four buttons on the
/// right whose text is the current choice, a click goes on to the next one; OK and Cancel
/// under them. Positions are the original's.
pub(crate) struct OptionsPage;

/// Where the Custom button is, in the dark square under the picture, above the gargoyles (the original's pixels).
const CUSTOM_AT: (f32, f32) = (50.0, 139.0);

impl Page for OptionsPage {
    fn frame(&mut self, app: &mut App, input: &Input) {
        const ROWS: &[(f32, f32)] = &[(39.0, 43.0), (66.0, 70.0), (93.0, 96.0), (120.0, 123.0)];
        clear_background(BLACK);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        if app.draw_pic(&lay, "newgame", 0, 0, 0.0, 0.0).is_none() {
            let r = lay.rect(crate::resolution::SCREEN);
            draw_rectangle(r.x, r.y, r.w, r.h, PANEL);
        }
        let o = app.options.clone();
        let settings = &app.game.content().settings;
        let (most, sizes) = (settings.max_opponents, settings.land_sizes.len() as u8);
        let land = app.text(land_key(
            &settings.land_sizes[usize::from(o.land).min(sizes as usize - 1)].id,
        ));
        let labels = [
            app.text(
                OPTION_DIFFICULTIES[usize::from(o.difficulty).min(OPTION_DIFFICULTIES.len() - 1)],
            ),
            app.text(OPPONENT_NAMES[usize::from(o.opponents.clamp(1, most)) - 1]),
            land,
            app.text(MAGIC_LEVELS[usize::from(o.magic).min(2)]),
        ];
        let names = [
            app.text("options.difficulty"),
            app.text("options.opponents"),
            app.text("options.land_size"),
            app.text("options.magic"),
        ];
        let title = app.text("options.title");
        app.draw_options_frame(&lay, names.each_ref().map(|n| &**n), &title);
        let clicked = input.mouse_pressed(MouseButton::Left);
        let mut changed = None;
        for (i, ((y, text_y), label)) in ROWS.iter().copied().zip(labels).enumerate() {
            let hover = ui::hit(lay.rect((250.0, y - 1.0, 66.0, 17.0)), true, input).hovered();
            let shift = if hover { 1.0 } else { 0.0 };
            app.draw_pic(&lay, "newgame", 4 + i, 0, 251.0 + shift, y + shift);
            app.draw_styled(
                view,
                &ng_ink(3, 31).center(),
                &label,
                283.0 + shift,
                text_y + 1.0 + shift,
            );
            app.draw_styled(
                view,
                &ng_ink(3, 187).center(),
                &label,
                282.0 + shift,
                text_y + shift,
            );
            if hover && clicked {
                changed = Some(i);
            }
        }
        // Too little land for each wizard: the game still starts, but the player is told.
        let crowded = { o.settings(&app.game.content().settings).basic.crowding() };
        if let Some(each) = crowded {
            let text = app.text_with("options.crowded", &Args::new().count("n", each));
            for (i, line) in app.bm_wrap(1, &text, 150.0).iter().enumerate() {
                app.draw_styled(view, &ng_ink(1, 201), line, 167.0, 170.0 + 8.0 * i as f32);
            }
        }
        let o = &mut app.options;
        let picked = changed;
        match changed {
            Some(0) => o.difficulty = (o.difficulty + 1) % 5,
            Some(1) => o.opponents = o.opponents.clamp(1, most) % most + 1,
            Some(2) => o.land = (o.land + 1) % sizes,
            Some(3) => o.magic = (o.magic + 1) % 3,
            _ => {}
        }
        if picked.is_some() {
            let settings = app.game.content().settings.clone();
            app.options.changed_here(picked == Some(0), &settings);
        }
        if app.options.custom.is_some() {
            let text = app.text("custom.active");
            app.draw_styled(view, &ng_ink(3, 201).center(), &text, 82.0, 156.0);
        }
        let custom = app.board_button_gold(&lay, "custom.button", CUSTOM_AT, input);
        let ok = app.board_button(&lay, "ui.button_ok", (252.0, 179.0), input);
        let cancel = app.board_button(&lay, "ui.cancel", (171.0, 179.0), input);
        if custom {
            app.open_custom();
        } else if cancel || input.key_pressed(KeyCode::Escape) {
            app.open_main_menu();
        } else if ok || input.key_pressed(KeyCode::Enter) {
            app.open_creation();
        }
    }
}

/// What the game is played with, opened from the Settings screen's Game button.
/// In a game it shows the settings and changes nothing; from the start menu there is no
/// game, so it is the options for the next one. Back returns to the Settings screen.
#[derive(Default)]
pub(crate) struct GameSettingsPage {
    /// The first row in view, when the list is longer than the box.
    top: usize,
}

/// The value after (or, going back, before) `current` of `count`, wrapping.
fn cycled(current: usize, count: usize, back: bool) -> usize {
    let count = count.max(1);
    (current + if back { count - 1 } else { 1 }) % count
}

impl Page for GameSettingsPage {
    /// A stone box like the original's confirmation boxes: from the start menu a row for each
    /// choice (left-click the value for the next one, right-click for the previous one), in a
    /// game the lines of what it is played with.
    fn frame(&mut self, app: &mut App, input: &Input) {
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let bw = app
            .art_picture("resource", 0, 0)
            .map_or(186.0, |t| crate::resolution::logical_w(&t));
        let from_menu = app.menu.from_menu;
        // What each row says and whether a click changes it: (label or line, value, clickable).
        let mut items: Vec<(String, String, bool)> = Vec::new();
        if from_menu {
            let settings = &app.game.content().settings;
            let o = &app.options;
            let land = settings
                .land_sizes
                .get(usize::from(o.land))
                .map_or(String::new(), |s| app.text(land_key(&s.id)));
            items.push((
                app.text("options.difficulty"),
                app.text(DIFFICULTIES[usize::from(o.difficulty)]),
                true,
            ));
            items.push((app.text("options.opponents"), o.opponents.to_string(), true));
            items.push((app.text("options.land_size"), land, true));
            items.push((
                app.text("options.magic_power"),
                app.text(MAGIC_LEVELS[usize::from(o.magic)]),
                true,
            ));
            let seed = app.text_with(
                "options.line_seed",
                &Args::new().text("seed", app.seed.to_string()),
            );
            items.push((seed, String::new(), false));
        } else {
            // Every setting the game was made with, read-only; the list scrolls.
            for (label, value) in crate::custom_screen::lines(
                app.texts(),
                app.game.settings(),
                &app.game.content().settings,
            ) {
                items.push((label, value, false));
            }
            let seed = app.text_with(
                "options.line_seed",
                &Args::new().text("seed", app.game.seed().to_string()),
            );
            items.push((seed, String::new(), false));
        }
        let rows_in_view = items.len().min(SETTINGS_VISIBLE);
        let last = items.len() - rows_in_view;
        let wheel = input.mouse_wheel().1;
        if wheel > 0.0 || input.key_pressed(KeyCode::Up) {
            self.top = self.top.saturating_sub(1);
        }
        if wheel < 0.0 || input.key_pressed(KeyCode::Down) {
            self.top = self.top.saturating_add(1);
        }
        if input.key_pressed(KeyCode::PageUp) {
            self.top = self.top.saturating_sub(SETTINGS_VISIBLE);
        }
        if input.key_pressed(KeyCode::PageDown) {
            self.top = self.top.saturating_add(SETTINGS_VISIBLE);
        }
        self.top = self.top.min(last);
        let total = items.len();
        let range = app.text_with(
            "custom.range",
            &Args::new()
                .count("from", self.top + 1)
                .count("to", self.top + rows_in_view)
                .count("of", total),
        );
        let items: Vec<_> = items
            .into_iter()
            .enumerate()
            .skip(self.top)
            .take(rows_in_view)
            .collect();
        let more = if total > rows_in_view { 10.0 } else { 0.0 };
        let bh = 29.0 + items.len() as f32 * SETTINGS_ROW + 8.0 + 12.0 + 14.0 + more;
        let (bx, by) = (
            ((crate::resolution::DESIGN_W - bw) / 2.0).floor(),
            ((crate::resolution::DESIGN_H - bh) / 2.0).floor(),
        );
        app.draw_stone_box(&lay, bx, by, bh);
        let gold = Ink::Set(font_ramp::NOTIFY_TEXT);
        let light = Ink::Set(font_ramp::DIALOG_TEXT);
        let (title, which) = (
            app.text("options.settings"),
            app.text(if from_menu {
                "options.next_game"
            } else {
                "options.this_game"
            }),
        );
        app.stone_text(
            view,
            4,
            gold,
            &title,
            bx + bw / 2.0,
            by + 6.0,
            Align::Center,
        );
        app.stone_text(
            view,
            1,
            light,
            &which,
            bx + bw / 2.0,
            by + 17.0,
            Align::Center,
        );
        let (left, right) = (
            input.mouse_pressed(MouseButton::Left),
            input.mouse_pressed(MouseButton::Right),
        );
        let mut changed: Option<usize> = None;
        for (n, (i, (label, value, clickable))) in items.iter().enumerate() {
            let i = *i;
            let y = by + 29.0 + n as f32 * SETTINGS_ROW;
            let r = lay.rect((bx + 6.0, y - 1.0, bw - 12.0, SETTINGS_ROW));
            let hover = *clickable && ui::hit(r, true, input).hovered();
            if hover {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.5, 0.38, 0.26, 1.0));
            }
            app.stone_text(view, 1, light, label, bx + 9.0, y, Align::Left);
            app.stone_text(view, 1, gold, value, bx + bw - 9.0, y, Align::Right);
            if hover && (left || right) {
                changed = Some(i);
            }
        }
        if let Some(i) = changed {
            let back = right;
            let lands = app.game.content().settings.land_sizes.len();
            let most = app.game.content().settings.max_opponents;
            let o = &mut app.options;
            match i {
                0 => {
                    o.difficulty = cycled(usize::from(o.difficulty), DIFFICULTIES.len(), back) as u8
                }
                1 => {
                    let at = usize::from(o.opponents.clamp(1, most)) - 1;
                    o.opponents = 1 + cycled(at, usize::from(most), back) as u8;
                }
                2 => o.land = cycled(usize::from(o.land), lands, back) as u8,
                3 => o.magic = cycled(usize::from(o.magic), MAGIC_LEVELS.len(), back) as u8,
                _ => {}
            }
        }
        if total > rows_in_view {
            app.stone_text(
                view,
                1,
                light,
                &range,
                bx + 9.0,
                by + bh - 34.0,
                Align::Left,
            );
        }
        let back_label = app.text("options.back");
        let back = app.red_button_sized(
            &lay,
            &back_label,
            bx + (bw - 60.0) / 2.0,
            by + bh - 24.0,
            60.0,
            input,
        );
        if back || input.key_pressed(KeyCode::Escape) {
            app.open_settings();
        }
    }
}

/// Row height of the game settings box, in the original's pixels.
const SETTINGS_ROW: f32 = 10.0;
/// Rows of the game settings box in view at once; a longer list scrolls.
const SETTINGS_VISIBLE: usize = 12;

/// The text key of a land size of `rules_settings.ron`, by its id; an id a language has no words for
/// shows as the id itself.
fn land_key(id: &str) -> &str {
    match id {
        "small" => "options.land_small",
        "medium" => "options.land_medium",
        "large" => "options.land_large",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped_settings() -> &'static f4x_core::game::setting::SettingsDef {
        crate::test_app::shipped::settings()
    }

    #[test]
    fn defaults_are_a_normal_two_wizard_game() {
        let settings = shipped_settings();
        let o = OptionsState::from_settings(settings);
        assert_eq!(o.opponents, 1);
        assert_eq!(o.land_percent(settings), f4x_core::game::setup::LAND_LARGE);
        assert_eq!(DIFFICULTIES[usize::from(o.difficulty)], "options.average");
    }

    #[test]
    fn extreme_on_the_options_step_plays_the_v16_rules() {
        let settings = shipped_settings();
        let mut o = OptionsState::from_settings(settings);
        o.difficulty = f4x_def::settings::EXTREME;
        o.changed_here(true, settings);
        assert_eq!(o.preset, crate::custom_screen::Preset::V160);
        assert_eq!(o.settings(settings).rules.galley_capacity, 8);
        o.difficulty = 3;
        o.changed_here(true, settings);
        assert_eq!(o.settings(settings).rules.galley_capacity, 5);
    }

    #[test]
    fn the_levels_figures_are_used_until_the_custom_screen_keeps_a_record() {
        let settings = shipped_settings();
        let mut o = OptionsState::from_settings(settings);
        o.difficulty = 4;
        assert_eq!(o.settings(settings).wizards.starting_gold.human, 25);
        let mut custom = o.settings(settings);
        custom.wizards.starting_gold.human = 300;
        custom.basic.opponents = 3;
        custom.basic.map_width = 75;
        custom.basic.map_height = 50;
        custom.basic.diagonal_extra_cost = true;
        o.keep_custom(custom, Default::default(), settings);
        let made = o.settings(settings);
        assert_eq!(made, custom);
        // The options step shows what the record says.
        assert_eq!((o.opponents, o.map_size(settings)), (3, (75, 50)));
        assert_eq!(o.difficulty, 4);
        // A change of opponents on the options step reaches the record and keeps the rest.
        o.opponents = 5;
        o.changed_here(false, settings);
        let made = o.settings(settings);
        assert_eq!(made.basic.opponents, 5);
        assert_eq!(made.wizards.starting_gold.human, 300);
        assert!(made.basic.diagonal_extra_cost);
        // A new difficulty fills in its own figures again.
        o.difficulty = 1;
        o.changed_here(true, settings);
        assert!(o.custom.is_none());
        assert_eq!(o.settings(settings).wizards.starting_gold.human, 100);
    }
}
