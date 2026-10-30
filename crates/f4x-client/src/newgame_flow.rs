//! The new-game steps after the options (`Newgame_Screen_1` to `_7`, `NewGame.c`), each on the
//! original's picture from `newgame.lbx`: Select Wizard, Select Picture (custom), Wizard's Name,
//! the picks (`newgame_picks.rs`), the spells (`newgame_picks.rs`), Select Race and Select
//! Banner. A ready-made wizard goes Wizard, Name, Race, Banner; a custom one Wizard (Custom),
//! Picture, Name, Picks, Spells, Race, Banner. Esc goes one step back. Positions are the
//! original's. The rules behind the choices are the core's (`WizardBuild`).

use crate::font::Style;
use crate::input::Input;
use crate::page::Page;
use crate::ui;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::create_screen::{CreateState, Step};
use crate::frame::Layout;
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
        self.draw_styled(view, &Style::outlined(5).center(), title, 242.0, 1.0);
        if let Some(p) = portrait {
            self.draw_pic(lay, "wizards", p, 0, 24.0, 10.0);
        }
    }

    pub(crate) fn ng_name(&mut self, lay: &Layout, name: &str) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        self.draw_styled(view, &Style::nearest(4, SHADOW).center(), name, 78.0, 120.0);
        self.draw_styled(view, &Style::nearest(4, SHADOW).center(), name, 77.0, 120.0);
        self.draw_styled(view, &Style::nearest(4, LIGHT).center(), name, 77.0, 119.0);
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
                self.draw_styled(
                    view,
                    &Style::nearest(0, GREY_INK),
                    line,
                    x + 4.0,
                    top + 5.0 + 7.0 * i as f32,
                );
            }
            return;
        };
        self.draw_styled(
            view,
            &Style::nearest(1, Color::new(1.0, 0.95, 0.6, 1.0)),
            &names[race],
            x + 4.0,
            top + 4.0,
        );
        let text = match race_summary_key(&ids[race]) {
            Some(key) => self.text(key),
            None => String::new(),
        };
        for (i, line) in self.bm_wrap(0, &text, w - 8.0).iter().enumerate() {
            self.draw_styled(
                view,
                &Style::nearest(0, LIGHT),
                line,
                x + 4.0,
                top + 15.0 + 7.0 * i as f32,
            );
        }
    }
}
impl App {
    pub fn open_creation(&mut self) {
        self.open_creation_with(CreateState::empty());
    }

    /// For the screenshot check: open the creation at a step (`CreateState::test_create_step`).
    pub fn open_creation_at(&mut self, step: &str) {
        let mut state = CreateState::empty();
        state.test_create_step(self, step);
        self.open_creation_with(state);
    }

    fn open_creation_with(&mut self, mut state: CreateState) {
        if state.build.name.is_empty() {
            state.build.name = "You".into();
        }
        self.open_page(Screen::Create, state);
    }
}

impl Page for CreateState {
    fn frame(&mut self, app: &mut App, input: &Input) {
        self.frame_of_step(app, input);
    }
}

impl CreateState {
    /// The books on the shelf at `(36, 135)`, eight pixels each, realms in the given order.
    pub(crate) fn ng_shelf(
        &mut self,
        app: &mut App,
        lay: &Layout,
        order: &[&str],
        books: &dyn Fn(&str) -> u8,
    ) {
        let mut x = 36.0;
        for realm in order {
            for k in 0..books(realm) {
                let var = self.shelf[usize::from(k).min(10)];
                app.draw_pic(
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
    pub(crate) fn ng_abilities_text(&self, app: &App) -> String {
        let names: Vec<String> = f4x_core::wizard::ALL_RETORTS
            .iter()
            .filter(|r| self.build.retorts.contains(r))
            .map(|&r| app.retort_name(r))
            .collect();
        if names.is_empty() {
            return String::new();
        }
        format!("{}{}", app.texts().join(&names), app.text("ui.full_stop"))
    }

    /// `Draw_Special_Abilities_String`: the retorts at `(12, 180)`, 138 pixels wide.
    pub(crate) fn ng_abilities(&mut self, app: &mut App, lay: &Layout) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let text = self.ng_abilities_text(app);
        let start = if self.build.retorts.len() > 5 {
            178.0
        } else {
            180.0
        };
        let lines = app.bm_wrap(0, &text, 138.0);
        let gold = Color::new(0.95, 0.7, 0.2, 1.0);
        for (i, line) in lines.iter().enumerate() {
            let y = start + i as f32 * 7.0;
            app.draw_styled(view, &Style::nearest(0, SHADOW), line, 13.0, y + 1.0);
            app.draw_styled(view, &Style::nearest(0, gold), line, 12.0, y);
        }
    }

    /// The shelf and abilities of a build that is being made.
    pub(crate) fn ng_build_shelf(&mut self, app: &mut App, lay: &Layout) {
        let books = self.build.books.clone();
        self.ng_shelf(
            app,
            lay,
            &["life", "sorcery", "nature", "death", "chaos"],
            &|r| books.get(r).copied().unwrap_or(0),
        );
        self.ng_abilities(app, lay);
    }

    /// `Warn0`: the warning box with the message; any click or Enter closes it, but not on the
    /// frame it came up (`closable` is false): the click that caused it would close it at once.
    pub(crate) fn ng_warn_box(
        &mut self,
        app: &mut App,
        lay: &Layout,
        closable: bool,
        input: &Input,
    ) -> bool {
        let Some(message) = self.warn.clone() else {
            return false;
        };
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let lines = app.bm_wrap(1, &message, 160.0);
        let (x, y) = (67.0, 40.0);
        app.draw_pic(lay, "newgame", 44, 0, x, y);
        app.draw_pic(lay, "newgame", 45, 0, x, y + 112.0);
        let top = y + 56.0 - lines.len() as f32 * 4.0;
        for (i, line) in lines.iter().enumerate() {
            app.draw_styled(
                view,
                &Style::nearest(1, LIGHT).center(),
                line,
                x + 93.0,
                top + i as f32 * 8.0,
            );
        }
        if closable
            && (input.mouse_pressed(MouseButton::Left)
                || input.key_pressed(KeyCode::Enter)
                || input.key_pressed(KeyCode::Escape))
        {
            self.warn = None;
            self.ignore_drag = true;
        }
        true
    }

    /// The step before this one, which Escape and the Back button go to: the options step from
    /// the first. A step of spells or the race goes back to the last realm that has spells to
    /// choose (what was chosen stays), else to the books of a custom wizard or the name of a
    /// ready-made one.
    pub(crate) fn create_back(&mut self, app: &mut App) {
        let before_spells = if self.custom { Step::Picks } else { Step::Name };
        let last_spells = |state: &CreateState, app: &App, below: usize| {
            (0..below)
                .rev()
                .find(|&i| {
                    !state
                        .spell_sections(app, crate::newgame_picks::REALM_ORDER[i])
                        .is_empty()
                })
                .map_or(before_spells, Step::Spells)
        };
        match self.step {
            Step::Wizard => app.open_options(),
            Step::Portrait => self.step = Step::Wizard,
            Step::Name => {
                self.step = if self.custom {
                    Step::Portrait
                } else {
                    Step::Wizard
                }
            }
            Step::Picks => self.step = Step::Name,
            Step::Spells(i) => self.step = last_spells(self, app, i),
            Step::Race => {
                self.step = last_spells(self, app, crate::newgame_picks::REALM_ORDER.len())
            }
            Step::Banner => self.step = Step::Race,
        }
    }

    /// The Back button at the foot of every step of the creation: a small box between the portrait
    /// frame and the picks count.
    fn ng_back_button(&mut self, app: &mut App, lay: &Layout, input: &Input) {
        // The race and banner steps use the corner (the race list's frame reaches down to y 198
        // at x 166 to 231), so there the button stands by OK, right of that frame.
        let left = if matches!(self.step, Step::Race | Step::Banner) {
            248.0
        } else {
            165.0
        };
        let r = lay.rect((left, 182.0, 30.0, 14.0));
        let state = ui::hit(r, self.ng_input(), input);
        let hover = state.hovered();
        let fill = if hover {
            Color::new(0.34, 0.24, 0.15, 1.0)
        } else {
            Color::new(0.24, 0.17, 0.11, 1.0)
        };
        draw_rectangle(r.x, r.y, r.w, r.h, fill);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, Color::new(0.1, 0.07, 0.05, 1.0));
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let label = app.text("create.back");
        app.draw_styled(
            view,
            &Style::nearest(3, SHADOW).center(),
            &label,
            left + 16.0,
            187.0,
        );
        app.draw_styled(
            view,
            &Style::nearest(3, if hover { GOLD_INK } else { LIGHT }).center(),
            &label,
            left + 15.0,
            186.0,
        );
        if state.clicked() {
            self.create_back(app);
        }
    }

    fn frame_of_step(&mut self, app: &mut App, input: &Input) {
        clear_background(BLACK);
        let lay = Layout::new();
        let r = lay.rect(crate::resolution::SCREEN);
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.2, 0.15, 0.1, 1.0));
        let background = if self.step == Step::Picks { 41 } else { 0 };
        app.draw_pic(&lay, "newgame", background, 0, 0.0, 0.0);
        // A warning box has the input until it is closed.
        let had_warning = self.warn.is_some();
        if !input.mouse_down(MouseButton::Left) {
            self.ignore_drag = false;
        }
        let step = self.step;
        match step {
            Step::Wizard => self.step_wizard(app, &lay, false, input),
            Step::Portrait => self.step_wizard(app, &lay, true, input),
            Step::Name => self.step_name(app, &lay, input),
            Step::Picks => self.step_picks(app, &lay, input),
            Step::Spells(i) => self.step_spells(app, &lay, i, input),
            Step::Race => self.step_race(app, &lay, input),
            Step::Banner => self.step_banner(app, &lay, input),
        }
        // A step that was left this frame (OK clicked) must not hand its click to the next step's
        // Back button, which stands where OK did.
        if self.step == step {
            self.ng_back_button(app, &lay, input);
        }
        self.ng_warn_box(app, &lay, had_warning, input);
    }

    /// Whether the steps may take input (no warning box is open).
    pub(crate) fn ng_input(&self) -> bool {
        self.warn.is_none()
    }

    /// Screens 1 and 2: the two columns of wizard buttons. Hovering shows the wizard; a click
    /// takes a ready-made wizard, "Custom", or (as a picture) one of the fourteen portraits.
    fn step_wizard(&mut self, app: &mut App, lay: &Layout, picture: bool, input: &Input) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let defs: Vec<(String, String)> = app
            .game
            .content()
            .rules
            .wizards
            .defs()
            .iter()
            .map(|w| (w.id.clone(), w.name.clone()))
            .collect();
        let intro = app.options.difficulty == 0;
        let right_count = if picture || intro { 7 } else { 8 };
        app.draw_pic(lay, "newgame", 39, 0, 165.0, 17.0);
        let title = app.text(if picture {
            "create.select_picture"
        } else {
            "create.select_wizard"
        });
        app.draw_styled(view, &Style::outlined(5).center(), &title, 242.0, 1.0);
        let clicked = input.mouse_pressed(MouseButton::Left) && self.ng_input();
        let mut chosen = None;
        let mut over = None;
        for i in 0..7 + right_count {
            let (col, row) = if i < 7 { (LEFT, i) } else { (RIGHT, i - 7) };
            let y = col.1 + ROW_STEP * row as f32;
            let hot = ui::hit(lay.rect((col.0, y, 70.0, 17.0)), self.ng_input(), input).hovered();
            if hot {
                over = Some(i);
                if clicked {
                    chosen = Some(i);
                }
            }
        }
        if let Some(i) = over {
            self.hover = i;
        }
        for i in 0..7 + right_count {
            let (x0, row) = if i < 7 { (169.0, i) } else { (245.0, i - 7) };
            let hot = over == Some(i);
            let shift = if hot { 1.0 } else { 0.0 };
            let y = 27.0 + ROW_STEP * row as f32;
            app.draw_pic(lay, "newgame", 9 + i, 0, x0 + shift, y + shift);
            let name = defs
                .get(i)
                .map_or_else(|| app.text("create.custom"), |d| d.1.clone());
            let cx = x0 + 34.0 + shift;
            app.draw_styled(
                view,
                &Style::nearest(3, SHADOW).center(),
                &name,
                cx + 1.0,
                y + 4.0 + shift + 1.0,
            );
            app.draw_styled(
                view,
                &Style::nearest(3, LIGHT).center(),
                &name,
                cx,
                y + 3.0 + shift,
            );
        }
        // The wizard under the mouse.
        let shown = self.hover;
        if shown < defs.len() {
            app.draw_pic(lay, "wizards", shown, 0, 24.0, 10.0);
            app.ng_name(lay, &defs[shown].1);
            if !picture {
                if let Some(build) = app.game.preset_build(&defs[shown].0) {
                    let books = build.books.clone();
                    // The first two realms with books, in the original's order.
                    let order = ["life", "sorcery", "nature", "death", "chaos"];
                    let shelf: Vec<&str> = order
                        .iter()
                        .copied()
                        .filter(|r| books.get(*r).copied().unwrap_or(0) > 0)
                        .collect();
                    self.ng_shelf(app, lay, &shelf, &|r| books.get(r).copied().unwrap_or(0));
                    let names: Vec<String> = f4x_core::wizard::ALL_RETORTS
                        .iter()
                        .filter(|r| build.retorts.contains(r))
                        .map(|&r| app.retort_name(r))
                        .collect();
                    if let Some(first) = names.first() {
                        let text = format!("{first}{}", app.text("ui.full_stop"));
                        app.draw_styled(view, &Style::nearest(0, SHADOW), &text, 13.0, 181.0);
                        app.draw_styled(view, &Style::nearest(0, GOLD_INK), &text, 12.0, 180.0);
                    }
                }
            }
        }
        if let Some(i) = chosen {
            if i < defs.len() {
                if picture {
                    self.build.portrait = Some(i as u8);
                    self.build.name = defs[i].1.clone();
                    self.step = Step::Name;
                } else {
                    self.pick_preset(app, Some(i));
                    self.custom = false;
                    self.step = Step::Name;
                }
            } else {
                // "Custom": a free build, to be made on the picks and spells steps.
                self.pick_preset(app, None);
                self.custom = true;
                self.step = Step::Portrait;
                self.hover = 0;
            }
        }
        if input.key_pressed(KeyCode::Escape) && self.ng_input() {
            self.create_back(app);
        }
    }

    /// Screen 3: the name box (`newgame` 40) with the typed name.
    fn step_name(&mut self, app: &mut App, lay: &Layout, input: &Input) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        app.draw_pic(lay, "newgame", 40, 0, 181.0, 17.0);
        let title = app.text("create.wizard_name");
        app.draw_styled(view, &Style::outlined(5).center(), &title, 242.0, 1.0);
        let portrait = self.build.portrait.map(usize::from);
        if let Some(p) = portrait {
            app.draw_pic(lay, "wizards", p, 0, 24.0, 10.0);
        }
        let name = self.build.name.clone();
        app.ng_name(lay, &name);
        if !self.custom {
            self.ng_build_shelf(app, lay);
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
        self.build.name = typed.clone();
        if self.custom {
            // Tab turns the wizard's gender: he, she, they.
            if self.ng_input() && input.key_pressed(KeyCode::Tab) {
                use f4x_core::text::Gender::*;
                self.build.gender = match self.build.gender {
                    Masculine => Feminine,
                    Feminine => Neuter,
                    Neuter => Masculine,
                };
            }
            let hint = app.text_with(
                "create.gender_hint",
                &Args::new().named("wizard", &typed, self.build.gender),
            );
            app.draw_styled(view, &Style::nearest(0, LIGHT), &hint, 196.0, 52.0);
        }
        let cursor = if ((get_time() * 2.0) as u32).is_multiple_of(2) {
            "_"
        } else {
            " "
        };
        app.draw_styled(
            view,
            &Style::nearest(3, LIGHT),
            &format!("{typed}{cursor}"),
            196.0,
            38.0,
        );
        // OK, where it stands on the other steps; Enter does the same.
        app.draw_pic(lay, "newgame", 42, 0, 252.0, 182.0);
        if !self.ng_input() {
            return;
        }
        let ok_clicked = ui::hit(lay.rect((251.0, 181.0, 32.0, 16.0)), true, input).clicked();
        if ok_clicked || input.key_pressed(KeyCode::Enter) || input.key_pressed(KeyCode::KpEnter) {
            if self.build.name.trim().is_empty() {
                let fallback = portrait
                    .and_then(|p| {
                        app.game
                            .content()
                            .rules
                            .wizards
                            .defs()
                            .get(p)
                            .map(|w| w.name.clone())
                    })
                    .unwrap_or_else(|| app.text("create.default_name"));
                self.build.name = fallback;
            }
            if self.custom {
                // A custom wizard starts from nothing on the picks step.
                let build = &mut self.build;
                build.books.clear();
                build.retorts.clear();
                build.spells.clear();
                self.step = Step::Picks;
            } else {
                // A ready-made wizard also chooses their starting spells, from the defaults.
                self.next_spells(app, 0);
            }
        } else if input.key_pressed(KeyCode::Escape) {
            self.create_back(app);
        }
    }

    /// Screen 6: the races, the Myrran ones only for a wizard with the Myrran retort.
    fn step_race(&mut self, app: &mut App, lay: &Layout, input: &Input) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        app.draw_pic(lay, "newgame", 54, 0, 164.0, 18.0);
        // The lists stand at the left of the right-hand half, an explanation box beside them.
        app.draw_pic(lay, "newgame", 55, 0, 166.0, 34.0);
        let portrait = self.build.portrait.map(usize::from);
        let title = app.text("create.select_race");
        app.ng_frame(lay, &title, portrait);
        let name = self.build.name.clone();
        app.ng_name(lay, &name);
        self.ng_build_shelf(app, lay);
        let myrran = self
            .build
            .retorts
            .contains(&f4x_core::wizard::magic::Retort::Myrran);
        let (arcanian, myrran_list) = app.race_lists();
        let (names, ids): (Vec<String>, Vec<String>) = app
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
        let live = self.ng_input();
        let clicked = input.mouse_pressed(MouseButton::Left) && live;
        let (arcanian_title, myrran_title) =
            (app.text("create.arcanian"), app.text("create.myrran"));
        app.draw_styled(
            view,
            &Style::nearest(3, title).center(),
            &arcanian_title,
            199.0,
            25.0,
        );
        app.draw_styled(
            view,
            &Style::nearest(3, title).center(),
            &myrran_title,
            199.0,
            134.0,
        );
        let mut chosen = None;
        let mut refused = false;
        let mut hovered = None;
        for (list, top, allowed) in [(&arcanian, 38.0, true), (&myrran_list, 147.0, myrran)] {
            for (i, race) in list.iter().enumerate() {
                let Some(race) = *race else { continue };
                let y = top + 10.0 * i as f32;
                let hot = ui::hit(lay.rect((168.0, y, 64.0, 8.0)), live, input).hovered();
                if hot {
                    hovered = Some(race);
                }
                let picked = self.race_pick == Some(race);
                let ink = if !allowed {
                    GREY_INK
                } else if hot || picked {
                    bright
                } else {
                    LIGHT
                };
                app.draw_styled(
                    view,
                    &Style::nearest(2, Color::new(0.0, 0.0, 0.0, 1.0)),
                    &names[race],
                    189.0,
                    y + 1.0,
                );
                app.draw_styled(view, &Style::nearest(2, ink), &names[race], 178.0, y);
                if picked {
                    app.draw_pic(lay, "newgame", 52, 0, 169.0, y);
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
        app.race_summary_box(lay, view, hovered.or(self.race_pick), &names, &ids);
        if refused {
            self.warn = Some(app.text("create.need_myrran"));
        }
        if let Some(race) = chosen {
            self.race_pick = Some(race);
        }
        // OK goes on, once a race is picked.
        let ok = self.race_pick.is_some();
        app.draw_pic(lay, "newgame", if ok { 42 } else { 43 }, 0, 286.0, 182.0);
        if ui::hit(lay.rect((285.0, 181.0, 32.0, 16.0)), live, input).clicked() {
            match self.race_pick {
                Some(race) => {
                    app.race = race;
                    self.step = Step::Banner;
                }
                None => self.warn = Some(app.text("create.pick_race")),
            }
        } else if input.key_pressed(KeyCode::Escape) && live {
            self.create_back(app);
        }
    }

    /// Screen 7: ten small flags in two columns, the original's five and five more colours made
    /// from them (`scripts/make_banner_flags.py`); a click on one takes the banner and starts the
    /// game.
    fn step_banner(&mut self, app: &mut App, lay: &Layout, input: &Input) {
        let portrait = self.build.portrait.map(usize::from);
        let title = app.text("create.select_banner");
        app.ng_frame(lay, &title, portrait);
        let name = self.build.name.clone();
        app.ng_name(lay, &name);
        self.ng_build_shelf(app, lay);
        let live = self.ng_input();
        let mut clicked = None;
        for column in 0..2 {
            for row in 0..5 {
                let (x, y) = (170.0 + 74.0 * column as f32, 26.0 + 34.0 * row as f32);
                let entry = row + 5 * column;
                let area = lay.rect((x - 2.0, y - 4.0, 74.0, 30.0));
                let hot = ui::hit(area, live, input).hovered();
                if app.draw_pic(lay, "banner_flags", entry, 0, x, y).is_none() {
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
                self.build.banner = Some(FLAG_BANNERS[row + 5 * column]);
                self.begin_game(app);
            }
            None => {
                if input.key_pressed(KeyCode::Escape) && live {
                    self.create_back(app);
                }
            }
        }
    }

    /// For the screenshot check: go to a step with a ready-made wizard (Merlin), or with
    /// `custom-` before the name a custom one with some books and retorts.
    pub fn test_create_step(&mut self, app: &mut App, name: &str) {
        let (custom, name) = match name.strip_prefix("custom-") {
            Some(rest) => (true, rest),
            None => (false, name),
        };
        if custom {
            self.pick_preset(app, None);
            self.custom = true;
            self.build.name = "Zed".into();
            self.build.portrait = Some(3);
            self.build.books.insert("sorcery".into(), 4);
            self.build.books.insert("chaos".into(), 3);
            self.build.books.insert("nature".into(), 1);
            self.build
                .retorts
                .insert(f4x_core::wizard::magic::Retort::Alchemy);
        } else {
            self.pick_preset(app, Some(0));
            self.custom = false;
        }
        self.hover = 4;
        self.step = match name {
            "portrait" => Step::Portrait,
            "name" => Step::Name,
            "picks" => Step::Picks,
            "spells" => {
                self.next_spells(app, 0);
                self.step
            }
            "race" => {
                self.race_pick = Some(0);
                Step::Race
            }
            "banner" => Step::Banner,
            _ => Step::Wizard,
        };
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
