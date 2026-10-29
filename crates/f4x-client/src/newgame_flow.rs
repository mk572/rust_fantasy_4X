//! The new-game steps after the options (`Newgame_Screen_1` to `_7`, `NewGame.c`), each on the
//! original's picture from `newgame.lbx`: Select Wizard, Select Picture (custom), Wizard's Name,
//! the picks (`newgame_picks.rs`), the spells (`newgame_picks.rs`), Select Race and Select
//! Banner. A ready-made wizard goes Wizard, Name, Race, Banner; a custom one Wizard (Custom),
//! Picture, Name, Picks, Spells, Race, Banner. Esc goes one step back. Positions are the
//! original's. The rules behind the choices are the core's (`WizardBuild`).

use crate::input::Input;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::create_screen::{CreateState, Step};
use crate::frame::{Align, Layout};
use crate::view::{App, Screen};

/// Left-column buttons of the wizard lists: `x`, first `y`, step; the right column follows.
const LEFT: (f32, f32) = (168.0, 26.0);
const RIGHT: (f32, f32) = (244.0, 26.0);
const ROW_STEP: f32 = 22.0;
/// The wizards' own order on the lists is the rules' order (the original's).
pub(crate) const LIGHT: Color = Color::new(0.93, 0.88, 0.68, 1.0);
pub(crate) const SHADOW: Color = Color::new(0.1, 0.07, 0.06, 1.0);
pub(crate) const GOLD_INK: Color = Color::new(0.95, 0.75, 0.25, 1.0);
pub(crate) const GREY_INK: Color = Color::new(0.5, 0.45, 0.4, 1.0);

/// The Arcanian and Myrran races of the race step, by id, in the original's order.
const ARCANIAN: [&str; 9] = [
    "barbarian",
    "gnoll",
    "halfling",
    "high_elf",
    "high_men",
    "klackon",
    "lizardman",
    "nomad",
    "orc",
];
const MYRRAN: [&str; 5] = ["beastmen", "dark_elf", "draconian", "dwarven", "troll"];
/// The banner of each flag on the banner step, top to bottom: green, blue, red, purple, yellow,
/// then (the second column) pink, brown, turquoise, dark blue, orange (as numbers of
/// `BANNER_NAMES`).
const FLAG_BANNERS: [u8; 10] = [1, 0, 3, 2, 4, 5, 6, 7, 8, 9];

/// The book picture (of `newgame.lbx` 24 to 38) of a realm: white, blue, green, black, red.
pub(crate) fn book_base(realm: &str) -> usize {
    match realm {
        "life" => 0,
        "sorcery" => 3,
        "nature" => 6,
        "death" => 9,
        _ => 12,
    }
}

/// Fresh random heights of the books on the shelf and the rows (`Randomize_Book_Heights`).
pub(crate) fn random_books() -> [u8; 11] {
    let mut v = [0u8; 11];
    for b in &mut v {
        *b = macroquad::rand::gen_range(0, 3);
    }
    v
}

impl CreateState {
    pub fn empty() -> Self {
        Self {
            preset: None,
            build: Default::default(),
            step: Step::Wizard,
            custom: false,
            hover: 0,
            shelf: random_books(),
            rows: random_books(),
            warn: None,
            ignore_drag: false,
            race_pick: None,
        }
    }
}

impl App {
    /// The wizard's name under the portrait (`(77, 119)`), the portrait, and the title.
    pub(crate) fn ng_frame(&mut self, lay: &Layout, title: &str, portrait: Option<usize>) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        self.bm_text_outlined(view, 5, title, 242.0, 1.0, Align::Center);
        if let Some(p) = portrait {
            self.draw_pic(lay, "wizards", p, 0, 24.0, 10.0);
        }
    }

    pub(crate) fn ng_name(&mut self, lay: &Layout, name: &str) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        self.bm_text(view, 4, SHADOW, name, 78.0, 120.0, Align::Center);
        self.bm_text(view, 4, SHADOW, name, 77.0, 120.0, Align::Center);
        self.bm_text(view, 4, LIGHT, name, 77.0, 119.0, Align::Center);
    }

    /// The books on the shelf at `(36, 135)`, eight pixels each, realms in the given order.
    pub(crate) fn ng_shelf(&mut self, lay: &Layout, order: &[&str], books: &dyn Fn(&str) -> u8) {
        let mut x = 36.0;
        for realm in order {
            for k in 0..books(realm) {
                let var = self.create.shelf[usize::from(k).min(10)];
                self.draw_pic(
                    lay,
                    "newgame",
                    24 + book_base(realm) + usize::from(var),
                    0,
                    x,
                    135.0,
                );
                x += 8.0;
            }
        }
    }

    /// The retorts of the build as the original writes them: "Alchemy, Warlord and Famous.".
    pub(crate) fn ng_abilities_text(&self) -> String {
        let names: Vec<String> = f4x_core::wizard::ALL_RETORTS
            .iter()
            .filter(|r| self.create.build.retorts.contains(r))
            .map(|&r| self.retort_name(r))
            .collect();
        if names.is_empty() {
            return String::new();
        }
        format!("{}{}", self.texts().join(&names), self.text("ui.full_stop"))
    }

    /// `Draw_Special_Abilities_String`: the retorts at `(12, 180)`, 138 pixels wide.
    pub(crate) fn ng_abilities(&mut self, lay: &Layout) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let text = self.ng_abilities_text();
        let start = if self.create.build.retorts.len() > 5 {
            178.0
        } else {
            180.0
        };
        let lines = self.bm_wrap(0, &text, 138.0);
        let gold = Color::new(0.95, 0.7, 0.2, 1.0);
        for (i, line) in lines.iter().enumerate() {
            let y = start + i as f32 * 7.0;
            self.bm_text(view, 0, SHADOW, line, 13.0, y + 1.0, Align::Left);
            self.bm_text(view, 0, gold, line, 12.0, y, Align::Left);
        }
    }

    /// The shelf and abilities of a build that is being made.
    pub(crate) fn ng_build_shelf(&mut self, lay: &Layout) {
        let books = self.create.build.books.clone();
        self.ng_shelf(
            lay,
            &["life", "sorcery", "nature", "death", "chaos"],
            &|r| books.get(r).copied().unwrap_or(0),
        );
        self.ng_abilities(lay);
    }

    /// `Warn0`: the warning box with the message; any click or Enter closes it, but not on the
    /// frame it came up (`closable` is false): the click that caused it would close it at once.
    pub(crate) fn ng_warn_box(&mut self, lay: &Layout, closable: bool, input: &Input) -> bool {
        let Some(message) = self.create.warn.clone() else {
            return false;
        };
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let lines = self.bm_wrap(1, &message, 160.0);
        let (x, y) = (67.0, 40.0);
        self.draw_pic(lay, "newgame", 44, 0, x, y);
        self.draw_pic(lay, "newgame", 45, 0, x, y + 112.0);
        let top = y + 56.0 - lines.len() as f32 * 4.0;
        for (i, line) in lines.iter().enumerate() {
            self.bm_text(
                view,
                1,
                LIGHT,
                line,
                x + 93.0,
                top + i as f32 * 8.0,
                Align::Center,
            );
        }
        if closable
            && (input.mouse_pressed(MouseButton::Left)
                || input.key_pressed(KeyCode::Enter)
                || input.key_pressed(KeyCode::Escape))
        {
            self.create.warn = None;
            self.create.ignore_drag = true;
        }
        true
    }

    /// The step before this one, which Escape and the Back button go to: the options step from
    /// the first. A step of spells or the race goes back to the last realm that has spells to
    /// choose (what was chosen stays), else to the books of a custom wizard or the name of a
    /// ready-made one.
    pub(crate) fn create_back(&mut self) {
        let before_spells = if self.create.custom {
            Step::Picks
        } else {
            Step::Name
        };
        let last_spells = |app: &App, below: usize| {
            (0..below)
                .rev()
                .find(|&i| {
                    !app.spell_sections(crate::newgame_picks::REALM_ORDER[i])
                        .is_empty()
                })
                .map_or(before_spells, Step::Spells)
        };
        match self.create.step {
            Step::Wizard => self.open_options(),
            Step::Portrait => self.create.step = Step::Wizard,
            Step::Name => {
                self.create.step = if self.create.custom {
                    Step::Portrait
                } else {
                    Step::Wizard
                }
            }
            Step::Picks => self.create.step = Step::Name,
            Step::Spells(i) => self.create.step = last_spells(self, i),
            Step::Race => {
                self.create.step = last_spells(self, crate::newgame_picks::REALM_ORDER.len())
            }
            Step::Banner => self.create.step = Step::Race,
        }
    }

    /// The Back button at the foot of every step of the creation: a small box between the portrait
    /// frame and the picks count.
    fn ng_back_button(&mut self, lay: &Layout, input: &Input) {
        // The race and banner steps use the corner (the race list's frame reaches down to y 198
        // at x 166 to 231), so there the button stands by OK, right of that frame.
        let left = if matches!(self.create.step, Step::Race | Step::Banner) {
            248.0
        } else {
            165.0
        };
        let r = lay.rect((left, 182.0, 30.0, 14.0));
        let hover = r.contains(input.mouse_position().into());
        let fill = if hover {
            Color::new(0.34, 0.24, 0.15, 1.0)
        } else {
            Color::new(0.24, 0.17, 0.11, 1.0)
        };
        draw_rectangle(r.x, r.y, r.w, r.h, fill);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, Color::new(0.1, 0.07, 0.05, 1.0));
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let label = self.text("create.back");
        self.bm_text(view, 3, SHADOW, &label, left + 16.0, 187.0, Align::Center);
        self.bm_text(
            view,
            3,
            if hover { GOLD_INK } else { LIGHT },
            &label,
            left + 15.0,
            186.0,
            Align::Center,
        );
        if hover && self.ng_input() && input.mouse_pressed(MouseButton::Left) {
            self.create_back();
        }
    }

    pub(crate) fn create_screen(&mut self, input: &Input) {
        clear_background(BLACK);
        let lay = Layout::new();
        let r = lay.rect((0.0, 0.0, 320.0, 200.0));
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.2, 0.15, 0.1, 1.0));
        let background = if self.create.step == Step::Picks {
            41
        } else {
            0
        };
        self.draw_pic(&lay, "newgame", background, 0, 0.0, 0.0);
        // A warning box has the input until it is closed.
        let had_warning = self.create.warn.is_some();
        if !input.mouse_down(MouseButton::Left) {
            self.create.ignore_drag = false;
        }
        let step = self.create.step;
        match step {
            Step::Wizard => self.step_wizard(&lay, false, input),
            Step::Portrait => self.step_wizard(&lay, true, input),
            Step::Name => self.step_name(&lay, input),
            Step::Picks => self.step_picks(&lay, input),
            Step::Spells(i) => self.step_spells(&lay, i, input),
            Step::Race => self.step_race(&lay, input),
            Step::Banner => self.step_banner(&lay, input),
        }
        // A step that was left this frame (OK clicked) must not hand its click to the next step's
        // Back button, which stands where OK did.
        if self.create.step == step {
            self.ng_back_button(&lay, input);
        }
        self.ng_warn_box(&lay, had_warning, input);
    }

    /// Whether the steps may take input (no warning box is open).
    pub(crate) fn ng_input(&self) -> bool {
        self.create.warn.is_none()
    }

    /// Screens 1 and 2: the two columns of wizard buttons. Hovering shows the wizard; a click
    /// takes a ready-made wizard, "Custom", or (as a picture) one of the fourteen portraits.
    fn step_wizard(&mut self, lay: &Layout, picture: bool, input: &Input) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let defs: Vec<(String, String)> = self
            .game
            .content()
            .rules
            .wizards
            .defs()
            .iter()
            .map(|w| (w.id.clone(), w.name.clone()))
            .collect();
        let intro = self.options.difficulty == 0;
        let right_count = if picture || intro { 7 } else { 8 };
        self.draw_pic(lay, "newgame", 39, 0, 165.0, 17.0);
        let title = self.text(if picture {
            "create.select_picture"
        } else {
            "create.select_wizard"
        });
        self.bm_text_outlined(view, 5, &title, 242.0, 1.0, Align::Center);
        let mouse: Vec2 = input.mouse_position().into();
        let clicked = input.mouse_pressed(MouseButton::Left) && self.ng_input();
        let mut chosen = None;
        let mut over = None;
        for i in 0..7 + right_count {
            let (col, row) = if i < 7 { (LEFT, i) } else { (RIGHT, i - 7) };
            let y = col.1 + ROW_STEP * row as f32;
            let hot = lay.rect((col.0, y, 70.0, 17.0)).contains(mouse) && self.ng_input();
            if hot {
                over = Some(i);
                if clicked {
                    chosen = Some(i);
                }
            }
        }
        if let Some(i) = over {
            self.create.hover = i;
        }
        for i in 0..7 + right_count {
            let (x0, row) = if i < 7 { (169.0, i) } else { (245.0, i - 7) };
            let hot = over == Some(i);
            let shift = if hot { 1.0 } else { 0.0 };
            let y = 27.0 + ROW_STEP * row as f32;
            self.draw_pic(lay, "newgame", 9 + i, 0, x0 + shift, y + shift);
            let name = defs
                .get(i)
                .map_or_else(|| self.text("create.custom"), |d| d.1.clone());
            let cx = x0 + 34.0 + shift;
            self.bm_text(
                view,
                3,
                SHADOW,
                &name,
                cx + 1.0,
                y + 4.0 + shift + 1.0,
                Align::Center,
            );
            self.bm_text(view, 3, LIGHT, &name, cx, y + 3.0 + shift, Align::Center);
        }
        // The wizard under the mouse.
        let shown = self.create.hover;
        if shown < defs.len() {
            self.draw_pic(lay, "wizards", shown, 0, 24.0, 10.0);
            self.ng_name(lay, &defs[shown].1);
            if !picture {
                if let Some(build) = self.game.preset_build(&defs[shown].0) {
                    let books = build.books.clone();
                    // The first two realms with books, in the original's order.
                    let order = ["life", "sorcery", "nature", "death", "chaos"];
                    let shelf: Vec<&str> = order
                        .iter()
                        .copied()
                        .filter(|r| books.get(*r).copied().unwrap_or(0) > 0)
                        .collect();
                    self.ng_shelf(lay, &shelf, &|r| books.get(r).copied().unwrap_or(0));
                    let names: Vec<String> = f4x_core::wizard::ALL_RETORTS
                        .iter()
                        .filter(|r| build.retorts.contains(r))
                        .map(|&r| self.retort_name(r))
                        .collect();
                    if let Some(first) = names.first() {
                        let text = format!("{first}{}", self.text("ui.full_stop"));
                        self.bm_text(view, 0, SHADOW, &text, 13.0, 181.0, Align::Left);
                        self.bm_text(view, 0, GOLD_INK, &text, 12.0, 180.0, Align::Left);
                    }
                }
            }
        }
        if let Some(i) = chosen {
            if i < defs.len() {
                if picture {
                    self.create.build.portrait = Some(i as u8);
                    self.create.build.name = defs[i].1.clone();
                    self.create.step = Step::Name;
                } else {
                    self.pick_preset(Some(i));
                    self.create.custom = false;
                    self.create.step = Step::Name;
                }
            } else {
                // "Custom": a free build, to be made on the picks and spells steps.
                self.pick_preset(None);
                self.create.custom = true;
                self.create.step = Step::Portrait;
                self.create.hover = 0;
            }
        }
        if input.key_pressed(KeyCode::Escape) && self.ng_input() {
            self.create_back();
        }
    }

    /// Screen 3: the name box (`newgame` 40) with the typed name.
    fn step_name(&mut self, lay: &Layout, input: &Input) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        self.draw_pic(lay, "newgame", 40, 0, 181.0, 17.0);
        let title = self.text("create.wizard_name");
        self.bm_text_outlined(view, 5, &title, 242.0, 1.0, Align::Center);
        let portrait = self.create.build.portrait.map(usize::from);
        if let Some(p) = portrait {
            self.draw_pic(lay, "wizards", p, 0, 24.0, 10.0);
        }
        let name = self.create.build.name.clone();
        self.ng_name(lay, &name);
        if !self.create.custom {
            self.ng_build_shelf(lay);
        }
        let mut typed = name;
        if self.ng_input() {
            for c in input.typed() {
                if !c.is_control() && typed.chars().count() < 10 {
                    typed.push(c);
                }
            }
            if input.key_pressed(KeyCode::Backspace) {
                typed.pop();
            }
        }
        self.create.build.name = typed.clone();
        if self.create.custom {
            // Tab turns the wizard's gender: he, she, they.
            if self.ng_input() && input.key_pressed(KeyCode::Tab) {
                use f4x_core::text::Gender::*;
                self.create.build.gender = match self.create.build.gender {
                    Masculine => Feminine,
                    Feminine => Neuter,
                    Neuter => Masculine,
                };
            }
            let hint = self.text_with(
                "create.gender_hint",
                &Args::new().named("wizard", &typed, self.create.build.gender),
            );
            self.bm_text(view, 0, LIGHT, &hint, 196.0, 52.0, Align::Left);
        }
        let cursor = if ((get_time() * 2.0) as u32).is_multiple_of(2) {
            "_"
        } else {
            " "
        };
        self.bm_text(
            view,
            3,
            LIGHT,
            &format!("{typed}{cursor}"),
            196.0,
            38.0,
            Align::Left,
        );
        // OK, where it stands on the other steps; Enter does the same.
        self.draw_pic(lay, "newgame", 42, 0, 252.0, 182.0);
        if !self.ng_input() {
            return;
        }
        let ok_clicked = input.mouse_pressed(MouseButton::Left)
            && lay
                .rect((251.0, 181.0, 32.0, 16.0))
                .contains(input.mouse_position().into());
        if ok_clicked || input.key_pressed(KeyCode::Enter) || input.key_pressed(KeyCode::KpEnter) {
            if self.create.build.name.trim().is_empty() {
                let fallback = portrait
                    .and_then(|p| {
                        self.game
                            .content()
                            .rules
                            .wizards
                            .defs()
                            .get(p)
                            .map(|w| w.name.clone())
                    })
                    .unwrap_or_else(|| self.text("create.default_name"));
                self.create.build.name = fallback;
            }
            if self.create.custom {
                // A custom wizard starts from nothing on the picks step.
                let build = &mut self.create.build;
                build.books.clear();
                build.retorts.clear();
                build.spells.clear();
                self.create.step = Step::Picks;
            } else {
                // A ready-made wizard also chooses their starting spells, from the defaults.
                self.next_spells(0);
            }
        } else if input.key_pressed(KeyCode::Escape) {
            self.create_back();
        }
    }

    /// The race indices of the Arcanian and Myrran lists.
    fn race_lists(&self) -> (Vec<Option<usize>>, Vec<Option<usize>>) {
        let races = self.game.content().rules.races.defs();
        let find = |ids: &[&str]| -> Vec<Option<usize>> {
            ids.iter()
                .map(|id| races.iter().position(|r| r.id == *id))
                .collect()
        };
        (find(&ARCANIAN), find(&MYRRAN))
    }

    /// Screen 6: the races, the Myrran ones only for a wizard with the Myrran retort.
    fn step_race(&mut self, lay: &Layout, input: &Input) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        self.draw_pic(lay, "newgame", 54, 0, 164.0, 18.0);
        // The lists stand at the left of the right-hand half, an explanation box beside them.
        self.draw_pic(lay, "newgame", 55, 0, 166.0, 34.0);
        let portrait = self.create.build.portrait.map(usize::from);
        let title = self.text("create.select_race");
        self.ng_frame(lay, &title, portrait);
        let name = self.create.build.name.clone();
        self.ng_name(lay, &name);
        self.ng_build_shelf(lay);
        let myrran = self
            .create
            .build
            .retorts
            .contains(&f4x_core::wizard::magic::Retort::Myrran);
        let (arcanian, myrran_list) = self.race_lists();
        let (names, ids): (Vec<String>, Vec<String>) = self
            .game
            .content()
            .rules
            .races
            .defs()
            .iter()
            .map(|r| (r.name.clone(), r.id.clone()))
            .unzip();
        let title = Color::new(0.85, 0.65, 0.2, 1.0);
        let bright = Color::new(1.0, 0.95, 0.6, 1.0);
        let mouse: Vec2 = input.mouse_position().into();
        let live = self.ng_input();
        let clicked = input.mouse_pressed(MouseButton::Left) && live;
        let (arcanian_title, myrran_title) =
            (self.text("create.arcanian"), self.text("create.myrran"));
        self.bm_text(view, 3, title, &arcanian_title, 199.0, 25.0, Align::Center);
        self.bm_text(view, 3, title, &myrran_title, 199.0, 134.0, Align::Center);
        let mut chosen = None;
        let mut refused = false;
        let mut hovered = None;
        for (list, top, allowed) in [(&arcanian, 38.0, true), (&myrran_list, 147.0, myrran)] {
            for (i, race) in list.iter().enumerate() {
                let Some(race) = *race else { continue };
                let y = top + 10.0 * i as f32;
                let hot = lay.rect((168.0, y, 64.0, 8.0)).contains(mouse) && live;
                if hot {
                    hovered = Some(race);
                }
                let picked = self.create.race_pick == Some(race);
                let ink = if !allowed {
                    GREY_INK
                } else if hot || picked {
                    bright
                } else {
                    LIGHT
                };
                self.bm_text(
                    view,
                    2,
                    Color::new(0.0, 0.0, 0.0, 1.0),
                    &names[race],
                    189.0,
                    y + 1.0,
                    Align::Left,
                );
                self.bm_text(view, 2, ink, &names[race], 178.0, y, Align::Left);
                if picked {
                    self.draw_pic(lay, "newgame", 52, 0, 169.0, y);
                }
                if hot && clicked {
                    if allowed {
                        chosen = Some(race);
                    } else {
                        refused = true;
                    }
                }
            }
        }
        // What the race is like, for the one the pointer is over (or else the one picked).
        self.race_summary_box(lay, view, hovered.or(self.create.race_pick), &names, &ids);
        if refused {
            self.create.warn = Some(self.text("create.need_myrran"));
        }
        if let Some(race) = chosen {
            self.create.race_pick = Some(race);
        }
        // OK goes on, once a race is picked.
        let ok = self.create.race_pick.is_some();
        self.draw_pic(lay, "newgame", if ok { 42 } else { 43 }, 0, 286.0, 182.0);
        if live
            && input.mouse_pressed(MouseButton::Left)
            && lay.rect((285.0, 181.0, 32.0, 16.0)).contains(mouse)
        {
            match self.create.race_pick {
                Some(race) => {
                    self.race = race;
                    self.create.step = Step::Banner;
                }
                None => self.create.warn = Some(self.text("create.pick_race")),
            }
        } else if input.key_pressed(KeyCode::Escape) && live {
            self.create_back();
        }
    }

    /// The box beside the race lists: the race's name and a short summary of what it is good at.
    fn race_summary_box(
        &mut self,
        lay: &Layout,
        view: (f32, f32, f32),
        race: Option<usize>,
        names: &[String],
        ids: &[String],
    ) {
        let (x, top, w, h) = (236.0, 34.0, 80.0, 143.0);
        let r = lay.rect((x, top, w, h));
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.1, 0.07, 0.05, 0.75));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.35, 0.25, 0.15, 1.0));
        let Some(race) = race else {
            let hint = self.text("create.race_hint");
            for (i, line) in self.bm_wrap(0, &hint, w - 8.0).iter().enumerate() {
                self.bm_text(
                    view,
                    0,
                    GREY_INK,
                    line,
                    x + 4.0,
                    top + 5.0 + 7.0 * i as f32,
                    Align::Left,
                );
            }
            return;
        };
        self.bm_text(
            view,
            1,
            Color::new(1.0, 0.95, 0.6, 1.0),
            &names[race],
            x + 4.0,
            top + 4.0,
            Align::Left,
        );
        let text = match race_summary_key(&ids[race]) {
            Some(key) => self.text(key),
            None => String::new(),
        };
        for (i, line) in self.bm_wrap(0, &text, w - 8.0).iter().enumerate() {
            self.bm_text(
                view,
                0,
                LIGHT,
                line,
                x + 4.0,
                top + 15.0 + 7.0 * i as f32,
                Align::Left,
            );
        }
    }

    /// Screen 7: ten small flags in two columns, the original's five and five more colours made
    /// from them (`scripts/make_banner_flags.py`); a click on one takes the banner and starts the
    /// game.
    fn step_banner(&mut self, lay: &Layout, input: &Input) {
        let portrait = self.create.build.portrait.map(usize::from);
        let title = self.text("create.select_banner");
        self.ng_frame(lay, &title, portrait);
        let name = self.create.build.name.clone();
        self.ng_name(lay, &name);
        self.ng_build_shelf(lay);
        let mouse: Vec2 = input.mouse_position().into();
        let live = self.ng_input();
        let mut clicked = None;
        for column in 0..2 {
            for row in 0..5 {
                let (x, y) = (170.0 + 74.0 * column as f32, 26.0 + 34.0 * row as f32);
                let entry = row + 5 * column;
                let area = lay.rect((x - 2.0, y - 4.0, 74.0, 30.0));
                let hot = live && area.contains(mouse);
                if self.draw_pic(lay, "banner_flags", entry, 0, x, y).is_none() {
                    draw_rectangle(
                        area.x,
                        area.y,
                        area.w,
                        area.h,
                        Color::new(0.4, 0.3, 0.2, 1.0),
                    );
                }
                if hot {
                    draw_rectangle_lines(
                        area.x,
                        area.y,
                        area.w,
                        area.h,
                        2.0,
                        Color::new(1.0, 0.95, 0.6, 1.0),
                    );
                    if input.mouse_pressed(MouseButton::Left) {
                        clicked = Some((column, row));
                    }
                }
            }
        }
        match clicked {
            Some((column, row)) => {
                self.create.build.banner = Some(FLAG_BANNERS[row + 5 * column]);
                self.begin_game();
            }
            None => {
                if input.key_pressed(KeyCode::Escape) && live {
                    self.create_back();
                }
            }
        }
    }

    /// For the screenshot check: go to a step with a ready-made wizard (Merlin), or with
    /// `custom-` before the name a custom one with some books and retorts.
    pub fn test_create_step(&mut self, name: &str) {
        let (custom, name) = match name.strip_prefix("custom-") {
            Some(rest) => (true, rest),
            None => (false, name),
        };
        if custom {
            self.pick_preset(None);
            self.create.custom = true;
            self.create.build.name = "Zed".into();
            self.create.build.portrait = Some(3);
            self.create.build.books.insert("sorcery".into(), 4);
            self.create.build.books.insert("chaos".into(), 3);
            self.create.build.books.insert("nature".into(), 1);
            self.create
                .build
                .retorts
                .insert(f4x_core::wizard::magic::Retort::Alchemy);
        } else {
            self.pick_preset(Some(0));
            self.create.custom = false;
        }
        self.create.hover = 4;
        self.create.step = match name {
            "portrait" => Step::Portrait,
            "name" => Step::Name,
            "picks" => Step::Picks,
            "spells" => {
                self.next_spells(0);
                self.create.step
            }
            "race" => {
                self.create.race_pick = Some(0);
                Step::Race
            }
            "banner" => Step::Banner,
            _ => Step::Wizard,
        };
    }

    pub fn open_creation(&mut self) {
        let mut state = CreateState::empty();
        state.build.name = "You".into();
        self.create = state;
        self.screen = Screen::Create;
    }
}

/// The text key of a race's summary on the race step (kept as literals so that the text check can
/// see that each key is used).
fn race_summary_key(id: &str) -> Option<&'static str> {
    Some(match id {
        "barbarian" => "create.race.barbarian",
        "beastmen" => "create.race.beastmen",
        "dark_elf" => "create.race.dark_elf",
        "draconian" => "create.race.draconian",
        "dwarven" => "create.race.dwarven",
        "gnoll" => "create.race.gnoll",
        "halfling" => "create.race.halfling",
        "high_elf" => "create.race.high_elf",
        "high_men" => "create.race.high_men",
        "klackon" => "create.race.klackon",
        "lizardman" => "create.race.lizardman",
        "nomad" => "create.race.nomad",
        "orc" => "create.race.orc",
        "troll" => "create.race.troll",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_realm_has_its_own_colour_of_book() {
        assert_eq!(book_base("life"), 0);
        assert_eq!(book_base("sorcery"), 3);
        assert_eq!(book_base("nature"), 6);
        assert_eq!(book_base("death"), 9);
        assert_eq!(book_base("chaos"), 12);
    }

    #[test]
    fn every_race_has_a_summary_for_the_race_step() {
        let content = f4x_data::load_content(std::path::Path::new("../../content")).unwrap();
        let texts = f4x_data::load_texts(&[std::path::Path::new("../../content")], "en").unwrap();
        for race in content.rules.races.defs() {
            let key =
                race_summary_key(&race.id).unwrap_or_else(|| panic!("no key for {}", race.id));
            assert!(texts.has(key), "no summary {key}");
        }
    }

    #[test]
    fn the_flags_cover_the_ten_banners_once() {
        let mut seen = FLAG_BANNERS.to_vec();
        seen.sort_unstable();
        assert_eq!(seen, (0..10).collect::<Vec<u8>>());
    }

    #[test]
    fn the_race_lists_name_fourteen_races_of_the_rules() {
        let all: Vec<&str> = ARCANIAN.iter().chain(MYRRAN.iter()).copied().collect();
        assert_eq!(all.len(), 14);
    }
}
