//! The wizard creation screen: pick a ready-made wizard or build one from eleven picks,
//! choose starting spells and a race, then begin.

use f4x_core::magic::Retort;
use f4x_core::rules::SpellId;
use f4x_core::wizard::{
    retort_available, retort_cost, retort_name, WizardBuild, ALL_RETORTS, MAX_BOOKS, MAX_RETORTS,
    PICKS,
};
use macroquad::prelude::*;

use crate::ui::{button, wrap, Lines, GOLD, PANEL};
use crate::view::{App, Screen};

/// What the player has set up so far.
pub(crate) struct CreateState {
    /// Index into the rules' ready-made wizards; `None` once edited or for "Custom".
    pub preset: Option<usize>,
    pub build: WizardBuild,
    /// The name field is taking key presses.
    pub typing: bool,
}

impl CreateState {
    pub fn empty() -> Self {
        Self {
            preset: None,
            build: WizardBuild::default(),
            typing: false,
        }
    }
}

const ROW: f32 = 24.0;

impl App {
    /// Realm ids that hold spell books, in the rules' order (arcane has none).
    fn book_realms(&self) -> Vec<String> {
        self.game
            .content
            .rules
            .realms
            .defs()
            .iter()
            .filter(|r| r.id != "arcane")
            .map(|r| r.id.clone())
            .collect()
    }

    pub fn open_creation(&mut self) {
        let mut state = CreateState::empty();
        state.build.name = "You".into();
        self.create = state;
        if !self.game.content.rules.wizards.is_empty() {
            self.pick_preset(Some(0));
        } else {
            let realms = self.book_realms();
            for r in realms.iter().take(2) {
                self.create.build.books.insert(r.clone(), 5);
            }
            self.create.build.retorts.insert(Retort::Alchemy);
            self.refill_spells();
        }
        self.screen = Screen::Create;
    }

    fn pick_preset(&mut self, index: Option<usize>) {
        match index {
            Some(i) => {
                let id = self.game.content.rules.wizards.defs()[i].id.clone();
                if let Some(build) = self.game.preset_build(&id) {
                    self.create.build = build;
                    self.create.preset = Some(i);
                }
            }
            None => {
                // "Custom" keeps what is there but under the full pick rules.
                self.create.build.preset = false;
                self.create.preset = None;
            }
        }
        self.create.build.spells.clear();
        self.refill_spells();
    }

    /// Keep the chosen starting spells within what the books allow, filling any gaps with
    /// the realm's defaults so the list always shows what the wizard would start with.
    fn refill_spells(&mut self) {
        let realms = self.book_realms();
        let mut spells: Vec<SpellId> = Vec::new();
        for realm in &realms {
            let want = self.create.build.spell_choices(realm);
            let mut chosen: Vec<SpellId> = self
                .create
                .build
                .spells
                .iter()
                .copied()
                .filter(|&s| self.game.content.rules.spells.get(s).realm == *realm)
                .collect();
            chosen.truncate(want);
            for s in self.game.default_starting_spells(realm) {
                if chosen.len() >= want {
                    break;
                }
                if !chosen.contains(&s) {
                    chosen.push(s);
                }
            }
            for s in self.game.starting_spell_pool(realm) {
                if chosen.len() >= want {
                    break;
                }
                if !chosen.contains(&s) {
                    chosen.push(s);
                }
            }
            spells.extend(chosen);
        }
        self.create.build.spells = spells;
    }

    /// A manual change: the wizard is no longer a ready-made one.
    fn edited(&mut self) {
        self.create.preset = None;
        self.create.build.preset = false;
        let books = self.create.build.books.clone();
        self.create
            .build
            .retorts
            .retain(|&r| retort_available(r, &books));
        self.refill_spells();
    }

    pub(crate) fn create_screen(&mut self) {
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, w, h, PANEL);
        if self.create.typing {
            while let Some(c) = get_char_pressed() {
                if !c.is_control() && self.create.build.name.chars().count() < 16 {
                    self.create.build.name.push(c);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                self.create.build.name.pop();
            }
            if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Escape) {
                self.create.typing = false;
            }
        }
        let col = ((w - 100.0) / 3.0).max(280.0);
        let (x0, x1, x2) = (30.0, 50.0 + col, 70.0 + 2.0 * col);
        self.create_wizard_column(x0, col, h);
        self.create_picks_column(x1, col);
        self.create_spells_column(x2, col, h);
    }

    fn create_wizard_column(&mut self, x: f32, width: f32, h: f32) {
        let mut lines = Lines::new(x, 36.0);
        lines.size = 24.0;
        lines.line("Create your wizard", GOLD);
        lines.gap(6.0);
        let names: Vec<String> = self
            .game
            .content
            .rules
            .wizards
            .defs()
            .iter()
            .map(|w| w.name.clone())
            .collect();
        let mut y = lines.y;
        let half = (width - 8.0) / 2.0;
        let mut clicked = None;
        let mut custom = false;
        for (i, name) in names.iter().enumerate() {
            let r = Rect::new(
                x + (i % 2) as f32 * (half + 8.0),
                y + (i / 2) as f32 * 30.0,
                half,
                26.0,
            );
            let label = if self.create.preset == Some(i) {
                format!("> {name}")
            } else {
                name.clone()
            };
            if button(r, &label, true) {
                clicked = Some(i);
            }
        }
        y += names.len().div_ceil(2) as f32 * 30.0 + 4.0;
        if button(Rect::new(x, y, width, 26.0), "Custom wizard", true) {
            custom = true;
        }
        y += 40.0;
        if let Some(i) = clicked {
            self.pick_preset(Some(i));
        }
        if custom {
            self.pick_preset(None);
        }
        // The name.
        draw_text("Name", x, y + 16.0, 20.0, GOLD);
        let field = Rect::new(x + 70.0, y, width - 70.0, 26.0);
        draw_rectangle(
            field.x,
            field.y,
            field.w,
            field.h,
            Color::new(0.0, 0.0, 0.0, 0.6),
        );
        draw_rectangle_lines(
            field.x,
            field.y,
            field.w,
            field.h,
            2.0,
            if self.create.typing { GOLD } else { GRAY },
        );
        let shown = if self.create.typing {
            format!("{}_", self.create.build.name)
        } else {
            self.create.build.name.clone()
        };
        draw_text(&shown, field.x + 6.0, field.y + 19.0, 22.0, WHITE);
        if is_mouse_button_pressed(MouseButton::Left) {
            self.create.typing = field.contains(mouse_position().into());
        }
        y += 40.0;
        // What the preset is about.
        if let Some(i) = self.create.preset {
            let text = self.game.content.rules.wizards.defs()[i]
                .description
                .clone();
            for line in wrap(&text, 18.0, width) {
                draw_text(&line, x, y + 14.0, 18.0, LIGHTGRAY);
                y += 20.0;
            }
        }
        // The race.
        y = y.max(h - 170.0);
        draw_text("Race", x, y + 16.0, 20.0, GOLD);
        let n = self.game.content.rules.races.len().max(1);
        if button(Rect::new(x + 70.0, y, 30.0, 26.0), "<", true) {
            self.race = (self.race + n - 1) % n;
        }
        draw_text(self.race_name(self.race), x + 108.0, y + 19.0, 22.0, WHITE);
        if button(Rect::new(x + width - 30.0, y, 30.0, 26.0), ">", true) {
            self.race = (self.race + 1) % n;
        }
        // The banner: five colours to choose from.
        y += 36.0;
        draw_text("Banner", x, y + 18.0, 20.0, GOLD);
        let chosen = self.create.build.banner.unwrap_or(0);
        for banner in 0..f4x_core::player::BANNER_CHOICES {
            let r = Rect::new(x + 70.0 + f32::from(banner) * 34.0, y, 28.0, 26.0);
            draw_rectangle(r.x, r.y, r.w, r.h, self.art.banner_color(banner));
            let picked = banner == chosen;
            draw_rectangle_lines(
                r.x,
                r.y,
                r.w,
                r.h,
                if picked { 3.0 } else { 1.0 },
                if picked { WHITE } else { BLACK },
            );
            if is_mouse_button_pressed(MouseButton::Left) && r.contains(mouse_position().into()) {
                self.create.build.banner = Some(banner);
            }
        }
        // Begin.
        let ok =
            self.create.build.preset || self.game.check_wizard_build(&self.create.build).is_ok();
        if button(Rect::new(x, h - 80.0, width, 40.0), "Begin the game", ok) {
            self.begin_game();
        }
        if !ok {
            let left = self.create.build.picks_left();
            let msg = if left > 0 {
                format!("Spend all {PICKS} picks ({left} left)")
            } else if left < 0 {
                format!("{} picks too many", -left)
            } else {
                "A retort needs more books".to_string()
            };
            draw_text(&msg, x, h - 30.0, 18.0, Color::new(0.95, 0.5, 0.4, 1.0));
        }
    }

    fn create_picks_column(&mut self, x: f32, width: f32) {
        let left = self.create.build.picks_left();
        draw_text("Spell books", x, 36.0, 24.0, GOLD);
        draw_text(
            format!("Picks left: {left}"),
            x + width - 130.0,
            36.0,
            20.0,
            if left == 0 { GREEN } else { WHITE },
        );
        let realms = self.book_realms();
        let mut y = 52.0;
        let mut change: Option<(String, i16)> = None;
        for realm in &realms {
            let n = self.create.build.books.get(realm).copied().unwrap_or(0);
            let color = self
                .game
                .content
                .rules
                .realms
                .by_name(realm)
                .map_or(WHITE, |r| {
                    Color::from_rgba(r.color.0, r.color.1, r.color.2, 255)
                });
            let name = self
                .game
                .content
                .rules
                .realms
                .by_name(realm)
                .map_or(realm.as_str(), |r| r.name.as_str());
            draw_text(name, x, y + 18.0, 22.0, color);
            if button(Rect::new(x + 110.0, y, 26.0, 24.0), "-", n > 0) {
                change = Some((realm.clone(), -1));
            }
            for b in 0..n {
                draw_rectangle(x + 144.0 + f32::from(b) * 11.0, y + 3.0, 9.0, 18.0, color);
            }
            let plus_x = x + 144.0 + f32::from(MAX_BOOKS) * 11.0 + 6.0;
            if button(
                Rect::new(plus_x, y, 26.0, 24.0),
                "+",
                n < MAX_BOOKS && left > 0,
            ) {
                change = Some((realm.clone(), 1));
            }
            y += ROW + 6.0;
        }
        if let Some((realm, d)) = change {
            let n = self.create.build.books.get(&realm).copied().unwrap_or(0);
            let new = (i16::from(n) + d).clamp(0, i16::from(MAX_BOOKS)) as u8;
            if new == 0 {
                self.create.build.books.remove(&realm);
            } else {
                self.create.build.books.insert(realm, new);
            }
            self.edited();
        }
        y += 14.0;
        draw_text(
            format!(
                "Retorts ({} of {MAX_RETORTS})",
                self.create.build.retorts.len()
            ),
            x,
            y + 16.0,
            24.0,
            GOLD,
        );
        y += 30.0;
        let mut toggle = None;
        for &r in &ALL_RETORTS {
            let have = self.create.build.retorts.contains(&r);
            let open = retort_available(r, &self.create.build.books);
            let afford =
                i16::from(retort_cost(r)) <= left && self.create.build.retorts.len() < MAX_RETORTS;
            let usable = have || (open && afford);
            let rect = Rect::new(x, y, width, ROW);
            let hover = rect.contains(mouse_position().into());
            if hover && usable {
                draw_rectangle(
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h,
                    Color::new(0.3, 0.22, 0.12, 0.7),
                );
            }
            draw_rectangle_lines(x + 2.0, y + 4.0, 14.0, 14.0, 1.5, GRAY);
            if have {
                draw_rectangle(x + 5.0, y + 7.0, 8.0, 8.0, GREEN);
            }
            let color = if have {
                GREEN
            } else if usable {
                WHITE
            } else {
                Color::new(0.5, 0.5, 0.5, 1.0)
            };
            draw_text(retort_name(r), x + 26.0, y + 18.0, 20.0, color);
            draw_text(
                format!("{}", retort_cost(r)),
                x + width - 18.0,
                y + 18.0,
                20.0,
                color,
            );
            if hover && usable && is_mouse_button_pressed(MouseButton::Left) {
                toggle = Some(r);
            }
            y += ROW;
        }
        if let Some(r) = toggle {
            if !self.create.build.retorts.remove(&r) {
                self.create.build.retorts.insert(r);
            }
            self.edited();
        }
    }

    fn create_spells_column(&mut self, x: f32, width: f32, h: f32) {
        draw_text("Starting spells", x, 36.0, 24.0, GOLD);
        let realms = self.book_realms();
        let mut y = 52.0;
        let mut toggle: Option<SpellId> = None;
        for realm in &realms {
            let need = self.create.build.spell_choices(realm);
            if need == 0 {
                continue;
            }
            let have = self
                .create
                .build
                .spells
                .iter()
                .filter(|&&s| self.game.content.rules.spells.get(s).realm == *realm)
                .count();
            let color = self
                .game
                .content
                .rules
                .realms
                .by_name(realm)
                .map_or(WHITE, |r| {
                    Color::from_rgba(r.color.0, r.color.1, r.color.2, 255)
                });
            let title = self
                .game
                .content
                .rules
                .realms
                .by_name(realm)
                .map_or(realm.as_str(), |r| r.name.as_str());
            draw_text(
                format!("{title}: {have} of {need}"),
                x,
                y + 16.0,
                20.0,
                color,
            );
            y += 22.0;
            // Two spell names per row keeps ten commons per realm compact.
            let pool = self.game.starting_spell_pool(realm);
            let half = width / 2.0;
            for (i, &s) in pool.iter().enumerate() {
                let r = Rect::new(
                    x + (i % 2) as f32 * half,
                    y + (i / 2) as f32 * 20.0,
                    half - 4.0,
                    20.0,
                );
                let picked = self.create.build.spells.contains(&s);
                let hover = r.contains(mouse_position().into());
                if hover {
                    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.3, 0.22, 0.12, 0.7));
                }
                let name = self.game.content.rules.spells.get(s).name.clone();
                draw_text(
                    &name,
                    r.x + 4.0,
                    r.y + 15.0,
                    17.0,
                    if picked { GREEN } else { LIGHTGRAY },
                );
                if hover && is_mouse_button_pressed(MouseButton::Left) && (picked || have < need) {
                    toggle = Some(s);
                }
            }
            y += pool.len().div_ceil(2) as f32 * 20.0 + 10.0;
            if y > h - 40.0 {
                break;
            }
        }
        if let Some(s) = toggle {
            if let Some(i) = self.create.build.spells.iter().position(|&x| x == s) {
                self.create.build.spells.remove(i);
            } else {
                self.create.build.spells.push(s);
            }
            // Choosing by hand is a change; the ready-made loadout otherwise stays.
            self.create.build.preset = false;
            self.create.preset = None;
        }
    }

    fn begin_game(&mut self) {
        let mut build = self.create.build.clone();
        build.name = if build.name.trim().is_empty() {
            "You".into()
        } else {
            build.name.trim().to_string()
        };
        self.human_build = Some(build);
        self.new_map(self.seed);
    }
}
