//! The Custom screen of the new-game options (task 503): every gameplay setting of
//! `GameSettings` in one list, changed one by one. A plain list for now; the design waits for the
//! full set of options. The same list, read-only, is what the Game settings box shows in a
//! running game (`options_screen::GameSettingsPage`), so no setting is missing from one or the
//! other.

use crate::font::Ink;
use crate::font_ramp;
use crate::frame::Align;
use crate::frame::Layout;
use crate::input::Input;
use crate::page::Page;
use crate::text::{Args, Texts};
use crate::ui;
use crate::view::{App, Screen};
use f4x_core::game::setting::{
    BasicSettings, EventFrequency, GameSettings, MeleeVsFliers, SettingsDef, MAP_HEIGHTS,
    MAP_WIDTHS, MAX_OPPONENTS, MAX_TILE_LIMIT,
};
use f4x_def::half_int::HalfInt;
use macroquad::prelude::*;

/// How a setting is changed.
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    /// A whole number from `min` to `max` in steps of `step`, `suffix` after it (`%`).
    Num {
        min: i32,
        max: i32,
        step: i32,
        suffix: &'static str,
    },
    /// On or off.
    Flag,
    /// Half-squares (the value is in halves: 5 is 2.5), `min` to `max`.
    Half { min: i32, max: i32 },
    /// One of these words (text keys), by index.
    Choice(&'static [&'static str]),
}

/// One setting: its words, how it is changed, and where it lives in the record.
#[derive(Clone, Copy)]
pub(crate) struct Field {
    pub label: &'static str,
    pub kind: Kind,
    pub get: fn(&GameSettings) -> i32,
    pub set: fn(&mut GameSettings, i32),
}

/// A row of the list: a heading or a setting.
#[derive(Clone, Copy)]
pub(crate) enum Row {
    Heading(&'static str),
    Field(Field),
}

const MAGIC_WORDS: &[&str] = &["options.weak", "options.normal", "options.powerful"];
const MELEE_WORDS: &[&str] = &[
    "custom.melee_zero",
    "custom.melee_half",
    "custom.melee_full",
];
const FREQUENCY_WORDS: &[&str] = &[
    "custom.frequency_half",
    "custom.frequency_normal",
    "custom.frequency_double",
];

fn num(
    label: &'static str,
    (min, max, step): (i32, i32, i32),
    suffix: &'static str,
    get: fn(&GameSettings) -> i32,
    set: fn(&mut GameSettings, i32),
) -> Row {
    Row::Field(Field {
        label,
        kind: Kind::Num {
            min,
            max,
            step,
            suffix,
        },
        get,
        set,
    })
}

fn flag(
    label: &'static str,
    get: fn(&GameSettings) -> i32,
    set: fn(&mut GameSettings, i32),
) -> Row {
    Row::Field(Field {
        label,
        kind: Kind::Flag,
        get,
        set,
    })
}

fn choice(
    label: &'static str,
    words: &'static [&'static str],
    get: fn(&GameSettings) -> i32,
    set: fn(&mut GameSettings, i32),
) -> Row {
    Row::Field(Field {
        label,
        kind: Kind::Choice(words),
        get,
        set,
    })
}

/// Every gameplay setting, in the order the screen lists them. The difficulty is not here: it is
/// chosen on the options step and fills in the figures (`GameSettings::for_difficulty`).
pub(crate) fn rows() -> Vec<Row> {
    use Row::Heading;
    vec![
        Heading("custom.h_map"),
        num(
            "custom.opponents",
            (1, i32::from(MAX_OPPONENTS), 1),
            "",
            |s| s.basic.opponents.into(),
            |s, v| s.basic.opponents = v as u8,
        ),
        num(
            "custom.land",
            (1, 99, 1),
            "%",
            |s| s.basic.land_percent.into(),
            |s, v| s.basic.land_percent = v as u8,
        ),
        num(
            "custom.map_width",
            (
                i32::from(*MAP_WIDTHS.start()),
                i32::from(*MAP_WIDTHS.end()),
                5,
            ),
            "",
            |s| s.basic.map_width.into(),
            |s, v| s.basic.map_width = v as u16,
        ),
        num(
            "custom.map_height",
            (
                i32::from(*MAP_HEIGHTS.start()),
                i32::from(*MAP_HEIGHTS.end()),
                5,
            ),
            "",
            |s| s.basic.map_height.into(),
            |s, v| s.basic.map_height = v as u16,
        ),
        num(
            "custom.towers",
            (0, 40, 1),
            "",
            |s| s.basic.tower_count().into(),
            |s, v| s.basic.towers = Some(v as u8),
        ),
        num(
            "custom.neutral_cities",
            (0, 60, 1),
            "",
            |s| s.basic.neutral_city_count() as i32,
            |s, v| s.basic.neutral_cities = Some(v as u8),
        ),
        choice(
            "custom.magic",
            MAGIC_WORDS,
            |s| s.basic.magic_strength.into(),
            |s, v| s.basic.magic_strength = v as u8,
        ),
        flag(
            "custom.original_generator",
            |s| s.basic.original_map_generator.into(),
            |s, v| s.basic.original_map_generator = v != 0,
        ),
        flag(
            "custom.diagonal_extra_cost",
            |s| s.basic.diagonal_extra_cost.into(),
            |s, v| s.basic.diagonal_extra_cost = v != 0,
        ),
        Row::Field(Field {
            label: "custom.start_reveal_radius",
            kind: Kind::Half { min: 0, max: 20 },
            get: |s| s.world.start_reveal_radius.raw(),
            set: |s, v| s.world.start_reveal_radius = HalfInt::from_raw(v),
        }),
        Heading("custom.h_wizards"),
        num(
            "custom.gold_you",
            (0, 2000, 25),
            "",
            |s| s.wizards.starting_gold.human,
            |s, v| s.wizards.starting_gold.human = v,
        ),
        num(
            "custom.gold_ai",
            (0, 2000, 25),
            "",
            |s| s.wizards.starting_gold.ai,
            |s, v| s.wizards.starting_gold.ai = v,
        ),
        num(
            "custom.mana_you",
            (0, 2000, 25),
            "",
            |s| s.wizards.starting_mana.human,
            |s, v| s.wizards.starting_mana.human = v,
        ),
        num(
            "custom.mana_ai",
            (0, 2000, 25),
            "",
            |s| s.wizards.starting_mana.ai,
            |s, v| s.wizards.starting_mana.ai = v,
        ),
        num(
            "custom.picks_you",
            (5, 20, 1),
            "",
            |s| s.wizards.picks.human.into(),
            |s, v| s.wizards.picks.human = v as u8,
        ),
        num(
            "custom.picks_ai",
            (5, 20, 1),
            "",
            |s| s.wizards.picks.ai.into(),
            |s, v| s.wizards.picks.ai = v as u8,
        ),
        Heading("custom.h_ai_cities"),
        num(
            "custom.ai_growth",
            (10, 800, 5),
            "%",
            |s| s.ai.growth_percent.into(),
            |s, v| s.ai.growth_percent = v as u16,
        ),
        num(
            "custom.ai_outpost_growth",
            (10, 800, 5),
            "%",
            |s| s.ai.outpost_growth_percent.into(),
            |s, v| s.ai.outpost_growth_percent = v as u16,
        ),
        num(
            "custom.ai_production",
            (10, 800, 5),
            "%",
            |s| s.ai.production_percent.into(),
            |s, v| s.ai.production_percent = v as u16,
        ),
        num(
            "custom.ai_gold",
            (10, 800, 5),
            "%",
            |s| s.ai.gold_percent.into(),
            |s, v| s.ai.gold_percent = v as u16,
        ),
        num(
            "custom.ai_mana",
            (10, 800, 5),
            "%",
            |s| s.ai.mana_percent.into(),
            |s, v| s.ai.mana_percent = v as u16,
        ),
        num(
            "custom.ai_food",
            (10, 800, 5),
            "%",
            |s| s.ai.food_percent.into(),
            |s, v| s.ai.food_percent = v as u16,
        ),
        num(
            "custom.ai_research",
            (10, 800, 5),
            "%",
            |s| s.ai.research_percent.into(),
            |s, v| s.ai.research_percent = v as u16,
        ),
        num(
            "custom.ai_maintenance",
            (10, 800, 5),
            "%",
            |s| s.ai.maintenance_percent.into(),
            |s, v| s.ai.maintenance_percent = v as u16,
        ),
        num(
            "custom.ai_unit_upkeep",
            (10, 800, 5),
            "%",
            |s| s.ai.unit_upkeep_percent.into(),
            |s, v| s.ai.unit_upkeep_percent = v as u16,
        ),
        Heading("custom.h_ai_wizards"),
        flag(
            "custom.ai_randomise",
            |s| s.ai.wizards.randomise.into(),
            |s, v| s.ai.wizards.randomise = v != 0,
        ),
        num(
            "custom.ai_merge",
            (0, 100, 5),
            "%",
            |s| s.ai.wizards.consolidate_percent.into(),
            |s, v| s.ai.wizards.consolidate_percent = v as u8,
        ),
        num(
            "custom.ai_erasure_dice",
            (0, 10, 1),
            "",
            |s| s.ai.wizards.erasure_dice.0.into(),
            |s, v| s.ai.wizards.erasure_dice.0 = v as u8,
        ),
        num(
            "custom.ai_erasure_sides",
            (0, 20, 1),
            "",
            |s| s.ai.wizards.erasure_dice.1.into(),
            |s, v| s.ai.wizards.erasure_dice.1 = v as u8,
        ),
        flag(
            "custom.ai_myrran",
            |s| s.ai.wizards.myrran_rule.into(),
            |s, v| s.ai.wizards.myrran_rule = v != 0,
        ),
        flag(
            "custom.ai_summoning_spell",
            |s| s.ai.wizards.guaranteed_summoning_spell.into(),
            |s, v| s.ai.wizards.guaranteed_summoning_spell = v != 0,
        ),
        Heading("custom.h_world"),
        num(
            "custom.lair_guards",
            (0, 300, 5),
            "%",
            |s| s.world.lair_guard_percent.into(),
            |s, v| s.world.lair_guard_percent = v as u16,
        ),
        num(
            "custom.lair_treasure",
            (0, 500, 5),
            "%",
            |s| s.world.lair_treasure_percent.into(),
            |s, v| s.world.lair_treasure_percent = v as u16,
        ),
        num(
            "custom.city_bonus",
            (0, 6, 1),
            "",
            |s| s.world.neutral_city_size_bonus.into(),
            |s, v| s.world.neutral_city_size_bonus = v as u8,
        ),
        flag(
            "custom.raiders",
            |s| s.world.raiders_enabled.into(),
            |s, v| s.world.raiders_enabled = v != 0,
        ),
        num(
            "custom.raider_size",
            (0, 300, 5),
            "%",
            |s| s.world.raider_percent.into(),
            |s, v| s.world.raider_percent = v as u16,
        ),
        flag(
            "custom.monsters",
            |s| s.world.monsters_enabled.into(),
            |s, v| s.world.monsters_enabled = v != 0,
        ),
        Heading("custom.h_rules"),
        flag(
            "custom.flee_you",
            |s| s.rules.safe_flee.human.into(),
            |s, v| s.rules.safe_flee.human = v != 0,
        ),
        flag(
            "custom.flee_ai",
            |s| s.rules.safe_flee.ai.into(),
            |s, v| s.rules.safe_flee.ai = v != 0,
        ),
        flag(
            "custom.settlers_cost_citizen",
            |s| s.rules.settlers_cost_citizen.into(),
            |s, v| s.rules.settlers_cost_citizen = v != 0,
        ),
        choice(
            "custom.melee_vs_fliers",
            MELEE_WORDS,
            |s| match s.rules.melee_vs_fliers {
                MeleeVsFliers::Zero => 0,
                MeleeVsFliers::Half => 1,
                MeleeVsFliers::Full => 2,
            },
            |s, v| {
                s.rules.melee_vs_fliers = match v {
                    0 => MeleeVsFliers::Zero,
                    1 => MeleeVsFliers::Half,
                    _ => MeleeVsFliers::Full,
                }
            },
        ),
        num(
            "custom.max_tile",
            (
                f4x_core::game::setting::MAX_STACK as i32,
                MAX_TILE_LIMIT as i32,
                1,
            ),
            "",
            |s| s.rules.max_tile as i32,
            |s, v| s.rules.max_tile = v as usize,
        ),
        flag(
            "custom.half_upkeep",
            |s| s.rules.half_upkeep.into(),
            |s, v| s.rules.half_upkeep = v != 0,
        ),
        num(
            "custom.arcane_cap",
            (0, 20, 1),
            "",
            |s| s.rules.arcane_candidate_cap.into(),
            |s, v| s.rules.arcane_candidate_cap = v as u8,
        ),
        Heading("custom.h_events"),
        flag(
            "custom.random_events",
            |s| s.events.random_events.into(),
            |s, v| s.events.random_events = v != 0,
        ),
        choice(
            "custom.event_frequency",
            FREQUENCY_WORDS,
            |s| match s.events.frequency {
                EventFrequency::Half => 0,
                EventFrequency::Normal => 1,
                EventFrequency::Double => 2,
            },
            |s, v| {
                s.events.frequency = match v {
                    0 => EventFrequency::Half,
                    1 => EventFrequency::Normal,
                    _ => EventFrequency::Double,
                }
            },
        ),
        num(
            "custom.event_pressure",
            (10, 200, 5),
            "%",
            |s| s.events.pressure_percent.into(),
            |s, v| s.events.pressure_percent = v as u16,
        ),
        Heading("custom.h_score"),
        num(
            "custom.score",
            (10, 500, 10),
            "%",
            |s| s.scoring.score_percent.into(),
            |s, v| s.scoring.score_percent = v as u16,
        ),
    ]
}

/// The value after (or, going back, before) `v`, wrapping from the last to the first.
pub(crate) fn stepped(kind: Kind, v: i32, back: bool) -> i32 {
    let (min, max, step) = match kind {
        Kind::Num { min, max, step, .. } => (min, max, step),
        Kind::Flag => (0, 1, 1),
        Kind::Half { min, max } => (min, max, 1),
        Kind::Choice(words) => (0, words.len() as i32 - 1, 1),
    };
    let next = if back { v - step } else { v + step };
    if next > max {
        min
    } else if next < min {
        max
    } else {
        next
    }
}

/// A value in words: the number, On or Off, or the choice's word.
pub(crate) fn value_text(texts: &Texts, field: &Field, v: i32) -> String {
    let word = |key: &str| texts.get(key, &Args::new());
    match field.kind {
        Kind::Num { suffix, .. } => format!("{v}{suffix}"),
        Kind::Flag => word(if v != 0 { "custom.on" } else { "custom.off" }),
        Kind::Half { .. } => HalfInt::from_raw(v).to_string(),
        Kind::Choice(words) => word(
            words
                .get(v.max(0) as usize)
                .copied()
                .unwrap_or("custom.off"),
        ),
    }
}

/// The list of what a game is played with, a heading or a setting and its value on each line
/// (the heading's value is empty): the difficulty first, then every row of [`rows`].
pub(crate) fn lines(texts: &Texts, s: &GameSettings, def: &SettingsDef) -> Vec<(String, String)> {
    let word = |key: &str| texts.get(key, &Args::new());
    let level = word(crate::options_screen::DIFFICULTIES[usize::from(s.basic.difficulty.min(4))]);
    let custom = if s.is_level(def) {
        String::new()
    } else {
        word("options.custom")
    };
    let mut out = vec![(word("options.difficulty"), format!("{level}{custom}"))];
    for row in rows() {
        out.push(match row {
            Row::Heading(key) => (word(key), String::new()),
            Row::Field(f) => (word(f.label), value_text(texts, &f, (f.get)(s))),
        });
    }
    out
}

impl App {
    pub fn open_custom(&mut self) {
        let work = self
            .options
            .custom
            .unwrap_or_else(|| self.options.settings(&self.game.content().settings));
        self.open_page(
            Screen::Options,
            CustomPage {
                work,
                preset: self.options.preset,
                open: None,
                top: 0,
            },
        );
    }
}

/// Rows in a column, and rows in view (two columns).
const COLUMN: usize = 15;
const VISIBLE: usize = 2 * COLUMN;
/// Row height in the original's pixels.
const ROW: f32 = 9.0;
/// Height of the two boxes: the title, the rows and the buttons under them.
const BOX_HEIGHT: f32 = 29.0 + COLUMN as f32 * ROW + 8.0 + 12.0 + 14.0;

/// The rule sets the dropdown offers. Picking one fills in its settings; changing any setting
/// afterwards switches to `Custom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Preset {
    /// The original's last official patch: the game's default rules.
    #[default]
    V131,
    /// The community patch's rule changes.
    V160,
    /// Every setting by hand.
    Custom,
}

impl Preset {
    pub(crate) const ALL: [Preset; 3] = [Preset::V131, Preset::V160, Preset::Custom];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Preset::V131 => "custom.preset_v131",
            Preset::V160 => "custom.preset_v160",
            Preset::Custom => "custom.preset_custom",
        }
    }

    /// `work` with this preset's rules: the level's figures for everything but the basic
    /// choices (the map, the opponents), which stay. `Custom` changes nothing.
    pub(crate) fn applied(self, work: GameSettings, def: &SettingsDef) -> GameSettings {
        let level = GameSettings::for_difficulty(def, work.basic.difficulty);
        let base = GameSettings {
            basic: BasicSettings {
                diagonal_extra_cost: level.basic.diagonal_extra_cost,
                ..work.basic
            },
            ..level
        };
        match self {
            Preset::V131 => base,
            // The patch's changes are not defined yet: until they are, it plays as 1.31.
            Preset::V160 => base,
            Preset::Custom => work,
        }
    }
}

/// The working copy of the settings, changed row by row; OK keeps it for the new game, Back
/// drops it.
pub(crate) struct CustomPage {
    work: GameSettings,
    preset: Preset,
    /// The dropdown whose list is open.
    open: Option<Menu>,
    top: usize,
}

impl Page for CustomPage {
    /// Two of the stone boxes of the in-game settings side by side, a column of settings in each.
    fn frame(&mut self, app: &mut App, input: &Input) {
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let bw = app
            .art_picture("resource", 0, 0)
            .map_or(186.0, |t| crate::resolution::logical_w(&t));
        let width = 2.0 * bw + 4.0;
        let lay = Layout::with_width(width);
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let by = ((crate::resolution::DESIGN_H - BOX_HEIGHT) / 2.0).floor();
        let all = rows();
        app.draw_stone_box(&lay, 0.0, by, BOX_HEIGHT);
        app.draw_stone_box(&lay, bw + 4.0, by, BOX_HEIGHT);
        let gold = Ink::Set(font_ramp::NOTIFY_TEXT);
        let light = Ink::Set(font_ramp::DIALOG_TEXT);
        let (title, hint) = (app.text("custom.title"), app.text("custom.hint"));
        app.stone_text(view, 4, gold, &title, bw / 2.0, by + 6.0, Align::Center);
        app.stone_text(view, 1, light, &hint, bw / 2.0, by + 17.0, Align::Center);
        let wheel = input.mouse_wheel().1;
        let last = all.len().saturating_sub(VISIBLE);
        if wheel > 0.0 || input.key_pressed(KeyCode::Up) {
            self.top = self.top.saturating_sub(1);
        }
        if wheel < 0.0 || input.key_pressed(KeyCode::Down) {
            self.top = (self.top + 1).min(last);
        }
        if input.key_pressed(KeyCode::PageUp) {
            self.top = self.top.saturating_sub(VISIBLE);
        }
        if input.key_pressed(KeyCode::PageDown) {
            self.top = (self.top + VISIBLE).min(last);
        }
        let (left, right) = (
            input.mouse_pressed(MouseButton::Left),
            input.mouse_pressed(MouseButton::Right),
        );
        let mut changed = None;
        for (n, row) in all.iter().enumerate().skip(self.top).take(VISIBLE) {
            let at = n - self.top;
            let x = (at / COLUMN) as f32 * (bw + 4.0);
            let y = by + 29.0 + (at % COLUMN) as f32 * ROW;
            match row {
                Row::Heading(key) => {
                    let text = app.text(key);
                    app.stone_text(view, 1, gold, &text, x + 9.0, y, Align::Left);
                }
                Row::Field(f) => {
                    let r = lay.rect((x + 6.0, y - 1.0, bw - 12.0, ROW));
                    let hover = self.open.is_none() && ui::hit(r, true, input).hovered();
                    if hover {
                        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.5, 0.38, 0.26, 1.0));
                        if left || right {
                            changed = Some((*f, right));
                        }
                    }
                    let label = app.text(f.label);
                    let value = value_text(app.texts(), f, (f.get)(&self.work));
                    app.stone_text(view, 1, light, &label, x + 9.0, y, Align::Left);
                    app.stone_text(view, 1, gold, &value, x + bw - 9.0, y, Align::Right);
                }
            }
        }
        if let Some((f, back)) = changed {
            let v = stepped(f.kind, (f.get)(&self.work), back);
            (f.set)(&mut self.work, v);
            self.preset = Preset::Custom;
        }
        self.dropdown(app, input, &lay, (bw, by));
        let range = app.text_with(
            "custom.range",
            &Args::new()
                .count("from", self.top + 1)
                .count("to", (self.top + VISIBLE).min(all.len()))
                .count("of", all.len()),
        );
        let button_y = by + BOX_HEIGHT - 24.0;
        app.stone_text(
            view,
            1,
            light,
            &range,
            bw + 4.0 + bw / 2.0,
            by + 166.0,
            Align::Center,
        );
        let (ok_label, back_label) = (app.text("ui.button_ok"), app.text("options.back"));
        let back =
            app.red_button_sized(&lay, &back_label, (bw - 60.0) / 2.0, button_y, 60.0, input);
        let ok = app.red_button_sized(
            &lay,
            &ok_label,
            bw + 4.0 + (bw - 60.0) / 2.0,
            button_y,
            60.0,
            input,
        );
        if ok || input.key_pressed(KeyCode::Enter) {
            let settings = app.game.content().settings.clone();
            app.options.keep_custom(self.work, self.preset, &settings);
            app.open_options();
        } else if back || input.key_pressed(KeyCode::Escape) {
            app.open_options();
        }
    }
}

/// The two dropdowns at the top of the right box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Menu {
    Preset,
    Difficulty,
}

impl CustomPage {
    /// The dropdowns at the top of the right box: the rule set and the difficulty, each showing
    /// its choice and, when open, its list. Picking a rule set fills in its settings; picking a
    /// difficulty fills in that level's figures under the chosen rule set.
    fn dropdown(&mut self, app: &mut App, input: &Input, lay: &Layout, (bw, by): (f32, f32)) {
        const H: f32 = 11.0;
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (x, w) = (bw + 4.0 + 9.0, bw - 18.0);
        let light = Ink::Set(font_ramp::DIALOG_TEXT);
        let gold = Ink::Set(font_ramp::NOTIFY_TEXT);
        let clicked = input.mouse_pressed(MouseButton::Left);
        let draw_item = |app: &mut App, y: f32, text: &str, hover: bool| {
            let r = lay.rect((x, y, w, H));
            let fill = if hover {
                Color::new(0.5, 0.38, 0.26, 1.0)
            } else {
                Color::new(0.16, 0.11, 0.09, 1.0)
            };
            draw_rectangle(r.x, r.y, r.w, r.h, fill);
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.65, 0.5, 0.35, 1.0));
            app.stone_text(view, 1, gold, text, x + 4.0, y + 2.0, Align::Left);
        };
        let heads = [
            (
                Menu::Preset,
                by + 5.0,
                format!(
                    "{} {}",
                    app.text("custom.preset"),
                    app.text(self.preset.key())
                ),
            ),
            (
                Menu::Difficulty,
                by + 17.0,
                format!(
                    "{} {}",
                    app.text("options.difficulty"),
                    app.text(
                        crate::options_screen::DIFFICULTIES
                            [usize::from(self.work.basic.difficulty.min(4))]
                    )
                ),
            ),
        ];
        let mut toggled = None;
        for (menu, y, text) in &heads {
            let hover = ui::hit(lay.rect((x, *y, w, H)), true, input).hovered();
            draw_item(app, *y, text, hover);
            app.stone_text(view, 1, light, "v", x + w - 8.0, y + 2.0, Align::Left);
            if hover && clicked {
                toggled = Some(*menu);
            }
        }
        if let Some(menu) = toggled {
            self.open = (self.open != Some(menu)).then_some(menu);
            return;
        }
        let Some(open) = self.open else { return };
        let y = heads.iter().find(|h| h.0 == open).map_or(by, |h| h.1);
        let items: Vec<String> = match open {
            Menu::Preset => Preset::ALL.iter().map(|p| app.text(p.key())).collect(),
            Menu::Difficulty => crate::options_screen::DIFFICULTIES
                .iter()
                .map(|k| app.text(k))
                .collect(),
        };
        let mut picked = None;
        for (i, text) in items.iter().enumerate() {
            let iy = y + H * (i as f32 + 1.0);
            let hover = ui::hit(lay.rect((x, iy, w, H)), true, input).hovered();
            draw_item(app, iy, text, hover);
            if hover && clicked {
                picked = Some(i);
            }
        }
        if let Some(i) = picked {
            let def = app.game.content().settings.clone();
            match open {
                Menu::Preset => {
                    self.preset = Preset::ALL[i];
                    self.work = self.preset.applied(self.work, &def);
                }
                Menu::Difficulty => {
                    // The level's figures replace every rule, so a hand-made set is gone.
                    self.work.basic.difficulty = i as u8;
                    if self.preset == Preset::Custom {
                        self.preset = Preset::V131;
                    }
                    self.work = self.preset.applied(self.work, &def);
                }
            }
            self.open = None;
        } else if clicked {
            self.open = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn average() -> GameSettings {
        GameSettings::default()
    }

    #[test]
    fn a_step_wraps_round_and_goes_back() {
        let kind = Kind::Num {
            min: 5,
            max: 20,
            step: 5,
            suffix: "",
        };
        assert_eq!(stepped(kind, 20, false), 5);
        assert_eq!(stepped(kind, 5, true), 20);
        assert_eq!(stepped(kind, 10, false), 15);
        assert_eq!(stepped(Kind::Flag, 1, false), 0);
        assert_eq!(stepped(Kind::Choice(MELEE_WORDS), 2, false), 0);
    }

    /// Every setting of the record that the rules read is in the list: changing each row's
    /// value changes the record, and the rows together reach every field.
    #[test]
    fn every_row_changes_the_record() {
        for row in rows() {
            let Row::Field(f) = row else { continue };
            let mut s = average();
            let before = (f.get)(&s);
            let v = stepped(f.kind, before, false);
            (f.set)(&mut s, v);
            assert_ne!(s, average(), "{} changes nothing", f.label);
            assert_eq!((f.get)(&s), v, "{} does not read back", f.label);
        }
    }

    /// The list reaches every field of the record: with each row moved one step, the only
    /// field left as it was is the difficulty, which the options step chooses.
    #[test]
    fn the_rows_reach_every_field_of_the_record() {
        let before = average();
        let mut after = before;
        for row in rows() {
            if let Row::Field(f) = row {
                let v = stepped(f.kind, (f.get)(&after), false);
                (f.set)(&mut after, v);
            }
        }
        let pretty = ron::ser::PrettyConfig::default();
        let (a, b) = (
            ron::ser::to_string_pretty(&before, pretty.clone()).expect("ron"),
            ron::ser::to_string_pretty(&after, pretty).expect("ron"),
        );
        let same: Vec<_> = a
            .lines()
            .zip(b.lines())
            .filter(|(x, y)| x == y && x.contains(':') && !x.trim_end().ends_with('('))
            .map(|(x, _)| x.trim().to_string())
            .collect();
        assert_eq!(same, ["difficulty: 2,"], "fields no row changes: {same:?}");
    }

    #[test]
    fn stepping_through_a_rows_range_stays_valid() {
        for row in rows() {
            let Row::Field(f) = row else { continue };
            let mut s = average();
            let mut v = (f.get)(&s);
            for _ in 0..200 {
                v = stepped(f.kind, v, false);
                (f.set)(&mut s, v);
                assert_eq!(
                    s.validate(),
                    Vec::<String>::new(),
                    "{} at {v} is out of range",
                    f.label
                );
            }
        }
    }

    #[test]
    fn every_label_has_words() {
        let texts = crate::test_app::shipped::texts();
        for row in rows() {
            let keys: Vec<&str> = match row {
                Row::Heading(k) => vec![k],
                Row::Field(f) => {
                    let mut keys = vec![f.label];
                    if let Kind::Choice(words) = f.kind {
                        keys.extend(words);
                    }
                    keys
                }
            };
            for key in keys {
                assert!(texts.has(key), "no text for {key}");
            }
        }
    }

    #[test]
    fn a_running_games_list_has_a_line_for_every_row() {
        let def = crate::test_app::shipped::settings();
        let texts = crate::test_app::shipped::texts();
        let s = GameSettings::for_difficulty(def, 4);
        let lines = lines(texts, &s, def);
        assert_eq!(lines.len(), rows().len() + 1);
        assert_eq!(lines[0], ("Difficulty".into(), "Impossible".into()));
        let gold = lines
            .iter()
            .find(|(l, _)| l == "Starting gold, you")
            .expect("a gold line");
        assert_eq!(gold.1, "25");
        let mut custom = s;
        custom.wizards.starting_gold.human = 300;
        assert_eq!(lines_first(texts, &custom, def), "Impossible (custom)");
    }

    fn lines_first(texts: &Texts, s: &GameSettings, def: &SettingsDef) -> String {
        lines(texts, s, def).remove(0).1
    }

    #[test]
    fn a_preset_fills_in_its_rules_and_keeps_the_map() {
        let def = crate::test_app::shipped::settings();
        let level = GameSettings::for_difficulty(def, 2);
        let mut work = level;
        work.wizards.starting_gold.human = 999;
        work.rules.half_upkeep = true;
        work.basic.map_width = 75;
        work.basic.diagonal_extra_cost = true;
        let filled = Preset::V131.applied(work, def);
        assert_eq!(filled.wizards, level.wizards);
        assert_eq!(filled.rules, level.rules);
        assert!(!filled.basic.diagonal_extra_cost);
        assert_eq!(filled.basic.map_width, 75);
        assert_eq!(Preset::Custom.applied(work, def), work);
    }
}
