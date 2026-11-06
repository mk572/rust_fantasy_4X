//! The new-game options step: difficulty, opponents, land size and node strength, shown
//! before the wizard is made (the original's `Newgame_Screen_0`).

use crate::input::Input;
use crate::page::Page;
use f4x_core::game::setting::SettingsDef;
use f4x_def::text::{Args, Texts};
use macroquad::prelude::*;

use crate::font::Ink;
use crate::font_ramp;
use crate::frame::{Align, Layout};
use crate::newgame_flow::ng_ink;
use crate::ui::{self, PANEL};
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
                .count("h", i32::from(b.map_height))
                .count("towers", i32::from(b.tower_count()))
                .count("cities", b.neutral_city_count() as i32),
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

/// The options step on the original's picture (`Newgame_Screen_0`): four buttons on the
/// right whose text is the current choice, a click goes on to the next one; OK and Cancel
/// under them. Positions are the original's.
pub(crate) struct OptionsPage;

impl Page for OptionsPage {
    fn frame(&mut self, app: &mut App, input: &Input) {
        const ROWS: [(f32, f32); 4] = [(39.0, 43.0), (66.0, 70.0), (93.0, 96.0), (120.0, 123.0)];
        clear_background(BLACK);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        if app.draw_pic(&lay, "newgame", 0, 0, 0.0, 0.0).is_none() {
            let r = lay.rect(crate::resolution::SCREEN);
            draw_rectangle(r.x, r.y, r.w, r.h, PANEL);
        }
        app.draw_pic(&lay, "newgame", 1, 0, 165.0, 0.0);
        let o = app.options.clone();
        let settings = &app.game.content().settings;
        let (most, sizes) = (settings.max_opponents, settings.land_sizes.len() as u8);
        let land = app.text(land_key(
            &settings.land_sizes[usize::from(o.land).min(sizes as usize - 1)].id,
        ));
        let labels = [
            app.text(OPTION_DIFFICULTIES[usize::from(o.difficulty).min(4)]),
            app.text(OPPONENT_NAMES[usize::from(o.opponents.clamp(1, most)) - 1]),
            land,
            app.text(MAGIC_LEVELS[usize::from(o.magic).min(2)]),
        ];
        let clicked = input.mouse_pressed(MouseButton::Left);
        let mut changed = None;
        for (i, ((y, text_y), label)) in ROWS.into_iter().zip(labels).enumerate() {
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
        // The original's picture has four buttons only: the map size and the map generator are
        // two more rows of text under them, the name on the left as the four have it and the
        // choice on the right.
        let (w, h) = o.map_size(&app.game.content().settings);
        let rows = [
            (
                143.0,
                app.text("options.map_size").to_string(),
                format!("{w} x {h}"),
            ),
            (
                156.0,
                app.text("options.generator").to_string(),
                app.text(if o.original_generator {
                    "options.generator_original"
                } else {
                    "options.generator_custom"
                })
                .to_string(),
            ),
        ];
        for (i, (y, name, value)) in rows.into_iter().enumerate() {
            let hover = ui::hit(lay.rect((166.0, y - 1.0, 150.0, 12.0)), true, input).hovered();
            let shift = if hover { 1.0 } else { 0.0 };
            let ink = if hover { 178 } else { 187 };
            app.draw_styled(view, &ng_ink(3, 31), &name, 168.0, y + 1.0);
            app.draw_styled(view, &ng_ink(3, 178), &name, 167.0, y);
            app.draw_styled(
                view,
                &ng_ink(3, 31).center(),
                &value,
                283.0 + shift,
                y + 1.0 + shift,
            );
            app.draw_styled(
                view,
                &ng_ink(3, ink).center(),
                &value,
                282.0 + shift,
                y + shift,
            );
            if hover && clicked {
                changed = Some(4 + i);
            }
        }
        // Too little land for each wizard: the game still starts, but the player is told.
        let crowded = {
            let (w, h) = o.map_size(&app.game.content().settings);
            let basic = f4x_core::game::setting::BasicSettings {
                opponents: o.opponents,
                map_width: w,
                map_height: h,
                ..Default::default()
            };
            basic.crowding()
        };
        if let Some(each) = crowded {
            let text = app.text_with("options.crowded", &Args::new().count("n", each));
            for (i, line) in app.bm_wrap(1, &text, 150.0).iter().enumerate() {
                app.draw_styled(view, &ng_ink(1, 201), line, 167.0, 170.0 + 8.0 * i as f32);
            }
        }
        let o = &mut app.options;
        let map_sizes = app.game.content().settings.map_sizes.len() as u8;
        match changed {
            Some(4) => o.map = (o.map + 1) % map_sizes,
            Some(5) => o.original_generator = !o.original_generator,
            Some(0) => o.difficulty = (o.difficulty + 1) % 5,
            Some(1) => o.opponents = o.opponents.clamp(1, most) % most + 1,
            Some(2) => o.land = (o.land + 1) % sizes,
            Some(3) => o.magic = (o.magic + 1) % 3,
            _ => {}
        }
        let ok = app.newgame_button(&lay, 2, 252.0, 179.0, input);
        let cancel = app.newgame_button(&lay, 3, 171.0, 179.0, input);
        if cancel || input.key_pressed(KeyCode::Escape) {
            app.open_main_menu();
        } else if ok || input.key_pressed(KeyCode::Enter) {
            app.open_creation();
        }
    }
}

/// What the game is played with, opened from the Settings screen's Game button.
/// In a game it shows the settings and changes nothing; from the start menu there is no
/// game, so it is the options for the next one. Back returns to the Settings screen.
pub(crate) struct GameSettingsPage;

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
            let map = settings
                .map_sizes
                .get(usize::from(o.map))
                .map_or(String::new(), |s| app.text(map_key(&s.id)));
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
            items.push((app.text("options.map_size"), map, true));
            items.push((
                app.text("options.generator"),
                app.text(if o.original_generator {
                    "options.generator_original"
                } else {
                    "options.generator_custom"
                }),
                true,
            ));
            let seed = app.text_with(
                "options.line_seed",
                &Args::new().text("seed", app.seed.to_string()),
            );
            items.push((seed, String::new(), false));
        } else {
            let mut lines = settings_lines(
                app.texts(),
                app.game.settings(),
                &app.game.content().settings,
            );
            lines.push(app.texts().get(
                "options.line_seed",
                &Args::new().text("seed", app.game.seed().to_string()),
            ));
            for line in lines {
                for part in app.bm_wrap(1, &line, bw - 20.0) {
                    items.push((part, String::new(), false));
                }
            }
        }
        let bh = 29.0 + items.len() as f32 * SETTINGS_ROW + 8.0 + 12.0 + 14.0;
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
        for (i, (label, value, clickable)) in items.iter().enumerate() {
            let y = by + 29.0 + i as f32 * SETTINGS_ROW;
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
            let (lands, maps) = {
                let s = &app.game.content().settings;
                (s.land_sizes.len(), s.map_sizes.len())
            };
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
                4 => o.map = cycled(usize::from(o.map), maps, back) as u8,
                _ => o.original_generator = !o.original_generator,
            }
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

/// The text key of a map size of `rules_settings.ron`, by its id.
fn map_key(id: &str) -> &str {
    match id {
        "small" => "options.map_small",
        "medium" => "options.map_medium",
        "large" => "options.map_large",
        "huge" => "options.map_huge",
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
    fn a_running_games_settings_read_as_words() {
        let settings = shipped_settings();
        let level = |d| f4x_core::game::setting::GameSettings::for_difficulty(settings, d);
        let texts = crate::test_app::shipped::texts();
        let lines = settings_lines(texts, &level(4), settings);
        assert_eq!(lines[0], "Difficulty: Impossible");
        assert!(lines.iter().any(|l| l.contains("25 for you, 150")));
        assert!(lines.iter().any(|l| l.contains("production 400%")));
        let mut custom = level(2);
        custom.wizards.starting_gold.human = 300;
        assert_eq!(
            settings_lines(texts, &custom, settings)[0],
            "Difficulty: Average (custom)"
        );
    }
}
