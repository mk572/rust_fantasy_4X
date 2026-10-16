//! The new-game steps after the options (`Newgame_Screen_1` to `_7`, `NewGame.c`), each on the
//! original's picture from `newgame.lbx`: Select Wizard, Select Picture (custom), Wizard's Name,
//! the picks (`newgame_picks.rs`), the spells (`newgame_picks.rs`), Select Race and Select
//! Banner. A ready-made wizard goes Wizard, Name, Race, Banner; a custom one Wizard (Custom),
//! Picture, Name, Picks, Spells, Race, Banner. Esc goes one step back. Positions are the
//! original's. The rules behind the choices are the core's (`WizardBuild`).

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
/// The banner of each flag on the banner step, top to bottom: green, blue, red, purple, yellow
/// (as numbers of `BANNER_NAMES`).
const FLAG_BANNERS: [u8; 5] = [1, 0, 3, 2, 4];

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
        let names: Vec<&str> = f4x_core::wizard::ALL_RETORTS
            .iter()
            .filter(|r| self.create.build.retorts.contains(r))
            .map(|&r| f4x_core::wizard::retort_name(r))
            .collect();
        match names.len() {
            0 => String::new(),
            1 => format!("{}.", names[0]),
            n => format!("{} and {}.", names[..n - 1].join(", "), names[n - 1]),
        }
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

    /// `Warn0`: the warning box with the message; any click or Enter closes it.
    pub(crate) fn ng_warn_box(&mut self, lay: &Layout) -> bool {
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
        if is_mouse_button_pressed(MouseButton::Left)
            || is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::Escape)
        {
            self.create.warn = None;
        }
        true
    }

    pub(crate) fn create_screen(&mut self) {
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
        let before = self.create.warn.clone();
        match self.create.step {
            Step::Wizard => self.step_wizard(&lay, false),
            Step::Portrait => self.step_wizard(&lay, true),
            Step::Name => self.step_name(&lay),
            Step::Picks => self.step_picks(&lay),
            Step::Spells(i) => self.step_spells(&lay, i),
            Step::Race => self.step_race(&lay),
            Step::Banner => self.step_banner(&lay),
        }
        if before.is_none() {
            self.ng_warn_box(&lay);
        }
    }

    /// Whether the steps may take input (no warning box is open).
    pub(crate) fn ng_input(&self) -> bool {
        self.create.warn.is_none()
    }

    /// Screens 1 and 2: the two columns of wizard buttons. Hovering shows the wizard; a click
    /// takes a ready-made wizard, "Custom", or (as a picture) one of the fourteen portraits.
    fn step_wizard(&mut self, lay: &Layout, picture: bool) {
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
        let title = if picture {
            "Select Picture"
        } else {
            "Select Wizard"
        };
        self.bm_text_outlined(view, 5, title, 242.0, 1.0, Align::Center);
        let mouse: Vec2 = mouse_position().into();
        let clicked = is_mouse_button_pressed(MouseButton::Left) && self.ng_input();
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
            let name = defs.get(i).map_or("Custom", |d| d.1.as_str()).to_string();
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
                    let names: Vec<&str> = f4x_core::wizard::ALL_RETORTS
                        .iter()
                        .filter(|r| build.retorts.contains(r))
                        .map(|&r| f4x_core::wizard::retort_name(r))
                        .collect();
                    if let Some(first) = names.first() {
                        let text = format!("{first}.");
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
        if is_key_pressed(KeyCode::Escape) && self.ng_input() {
            if picture {
                self.create.step = Step::Wizard;
            } else {
                self.open_options();
            }
        }
    }

    /// Screen 3: the name box (`newgame` 40) with the typed name.
    fn step_name(&mut self, lay: &Layout) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        self.draw_pic(lay, "newgame", 40, 0, 181.0, 17.0);
        self.bm_text_outlined(view, 5, "Wizard's Name", 242.0, 1.0, Align::Center);
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
            while let Some(c) = get_char_pressed() {
                if !c.is_control() && typed.chars().count() < 10 {
                    typed.push(c);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                typed.pop();
            }
        }
        self.create.build.name = typed.clone();
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
        if !self.ng_input() {
            return;
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
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
                    .unwrap_or_else(|| "You".into());
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
                self.create.step = Step::Race;
            }
        } else if is_key_pressed(KeyCode::Escape) {
            self.create.step = if self.create.custom {
                Step::Portrait
            } else {
                Step::Wizard
            };
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
    fn step_race(&mut self, lay: &Layout) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        self.draw_pic(lay, "newgame", 54, 0, 164.0, 18.0);
        self.draw_pic(lay, "newgame", 55, 0, 208.0, 34.0);
        let portrait = self.create.build.portrait.map(usize::from);
        self.ng_frame(lay, "Select Race", portrait);
        let name = self.create.build.name.clone();
        self.ng_name(lay, &name);
        self.ng_build_shelf(lay);
        let myrran = self
            .create
            .build
            .retorts
            .contains(&f4x_core::magic::Retort::Myrran);
        let (arcanian, myrran_list) = self.race_lists();
        let names: Vec<String> = self
            .game
            .content()
            .rules
            .races
            .defs()
            .iter()
            .map(|r| r.name.clone())
            .collect();
        let title = Color::new(0.85, 0.65, 0.2, 1.0);
        let bright = Color::new(1.0, 0.95, 0.6, 1.0);
        let mouse: Vec2 = mouse_position().into();
        let clicked = is_mouse_button_pressed(MouseButton::Left) && self.ng_input();
        self.bm_text(view, 3, title, "Arcanian Races:", 198.0, 25.0, Align::Left);
        self.bm_text(view, 3, title, "Myrran Races:", 202.0, 134.0, Align::Left);
        let mut chosen = None;
        let mut refused = false;
        for (list, top, allowed) in [(&arcanian, 38.0, true), (&myrran_list, 147.0, myrran)] {
            for (i, race) in list.iter().enumerate() {
                let Some(race) = *race else { continue };
                let y = top + 10.0 * i as f32;
                let hot = lay.rect((211.0, y, 59.0, 8.0)).contains(mouse) && self.ng_input();
                let ink = if !allowed {
                    GREY_INK
                } else if hot {
                    bright
                } else {
                    LIGHT
                };
                self.bm_text(
                    view,
                    2,
                    Color::new(0.0, 0.0, 0.0, 1.0),
                    &names[race],
                    221.0,
                    y + 1.0,
                    Align::Left,
                );
                self.bm_text(view, 2, ink, &names[race], 220.0, y, Align::Left);
                if hot && clicked {
                    if allowed {
                        chosen = Some(race);
                    } else {
                        refused = true;
                    }
                }
            }
        }
        if refused {
            self.create.warn =
                Some("You can not select a Myrran race unless you have the Myrran special.".into());
        }
        if let Some(race) = chosen {
            self.race = race;
            self.create.step = Step::Banner;
        } else if is_key_pressed(KeyCode::Escape) && self.ng_input() {
            self.create.step = if self.create.custom {
                Step::Picks
            } else {
                Step::Name
            };
        }
    }

    /// Screen 7: five flags; a click takes the banner and starts the game.
    fn step_banner(&mut self, lay: &Layout) {
        self.draw_pic(lay, "newgame", 46, 0, 158.0, 0.0);
        let portrait = self.create.build.portrait.map(usize::from);
        self.ng_frame(lay, "Select Banner", portrait);
        let name = self.create.build.name.clone();
        self.ng_name(lay, &name);
        self.ng_build_shelf(lay);
        let tops = [21.0, 56.0, 94.0, 129.0, 163.0];
        let bottoms = [55.0, 93.0, 128.0, 162.0, 199.0];
        let mouse: Vec2 = mouse_position().into();
        if !self.ng_input() {
            return;
        }
        for i in 0..5 {
            let r = lay.rect((175.0, tops[i], 138.0, bottoms[i] - tops[i]));
            if r.contains(mouse) && is_mouse_button_pressed(MouseButton::Left) {
                self.create.build.banner = Some(FLAG_BANNERS[i]);
                self.begin_game();
                return;
            }
        }
        if is_key_pressed(KeyCode::Escape) {
            self.create.step = Step::Race;
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
                .insert(f4x_core::magic::Retort::Alchemy);
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
            "race" => Step::Race,
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
    fn the_flags_cover_the_five_banners_once() {
        let mut seen = FLAG_BANNERS.to_vec();
        seen.sort_unstable();
        assert_eq!(seen, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn the_race_lists_name_fourteen_races_of_the_rules() {
        let all: Vec<&str> = ARCANIAN.iter().chain(MYRRAN.iter()).copied().collect();
        assert_eq!(all.len(), 14);
    }
}
