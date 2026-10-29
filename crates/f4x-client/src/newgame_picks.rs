//! The custom wizard's two making steps (`Newgame_Screen_4` and `_5`, `NewGame.c`): the picks
//! (drag in a realm's row to set its books, click a retort to take or drop it, eleven picks in
//! all) and, for each realm with two or more books, its starting spells (a click takes or drops
//! one; the commons are chosen up to `books - 1`, and with eleven books two uncommon and one
//! rare). The pick rules are the core's (`WizardBuild`, `retort_available`, `retort_cost`);
//! this file is the pictures and the clicks. Positions are the original's.

use crate::font::Style;
use crate::input::Input;
use crate::ui;
use f4x_core::content::Content;
use f4x_core::rules::Rarity;
use f4x_core::text::Args;
use f4x_core::wizard::magic::Retort;
use f4x_core::wizard::{retort_available, retort_cost, ALL_RETORTS, MAX_RETORTS};
use macroquad::prelude::*;

use crate::create_screen::Step;
use crate::frame::Layout;
use crate::newgame_flow::{book_base, GOLD_INK, GREY_INK, LIGHT, SHADOW};
use crate::view::App;

/// The realms in the order of the rows and of `TBL_Realm_Order`.
pub(crate) const REALM_ORDER: [&str; 5] = ["life", "death", "chaos", "nature", "sorcery"];
/// The realms' ink on the spells step.
fn realm_ink(realm: &str) -> Color {
    match realm {
        "life" => Color::new(0.95, 0.95, 0.9, 1.0),
        "death" => Color::new(0.7, 0.5, 0.85, 1.0),
        "chaos" => Color::new(0.95, 0.4, 0.3, 1.0),
        "nature" => Color::new(0.4, 0.85, 0.4, 1.0),
        _ => Color::new(0.45, 0.6, 0.98, 1.0),
    }
}

/// What a retort needs, in the words of the original's refusal ("To select X you need: ...").
pub(crate) fn prerequisite_text(content: &Content, retort: Retort) -> String {
    let texts = &content.texts;
    let pick = |n: u8, realm: &str| {
        let name = content
            .rules
            .realms
            .by_name(realm)
            .map_or_else(|| realm.to_string(), |r| r.name.clone());
        texts.get(
            "picks.in_realm",
            &Args::new().count("n", n).text("realm", name),
        )
    };
    {
        match retort {
            Retort::SageMaster => texts.get("picks.sage_master", &Args::new()),
            Retort::Runemaster => texts.get("picks.runemaster", &Args::new()),
            Retort::Archmage | Retort::ManaFocusing => texts.get("picks.any_realm", &Args::new()),
            Retort::ChaosMastery => pick(4, "chaos"),
            Retort::NatureMastery => pick(4, "nature"),
            Retort::SorceryMastery => pick(4, "sorcery"),
            Retort::InfernalPower => pick(4, "death"),
            Retort::DivinePower => pick(4, "life"),
            Retort::NodeMastery => texts.get(
                "picks.node_mastery",
                &Args::new()
                    .text("chaos", pick(1, "chaos"))
                    .text("sorcery", pick(1, "sorcery"))
                    .text("nature", pick(1, "nature")),
            ),
            _ => String::new(),
        }
    }
}

impl App {
    fn books_of(&self, realm: &str) -> u8 {
        self.create.build.books.get(realm).copied().unwrap_or(0)
    }

    /// Set the number of books of a realm, within the picks left and the Life and Death rule.
    fn set_books(&mut self, realm: &str, wanted: u8) {
        let current = i32::from(self.books_of(realm));
        let left = i32::from(self.create.build.picks_left());
        let n = i32::from(wanted).min(current + left).clamp(0, 11) as u8;
        if n == self.books_of(realm) {
            return;
        }
        let rival = match realm {
            "life" => "death",
            "death" => "life",
            _ => "",
        };
        if n > 0 && !rival.is_empty() && self.books_of(rival) > 0 {
            self.create.warn = Some(self.text("picks.life_and_death"));
            return;
        }
        if n == 0 {
            self.create.build.books.remove(realm);
        } else {
            self.create.build.books.insert(realm.to_string(), n);
        }
        // A retort whose books are gone is dropped, with the reason.
        let books = self.create.build.books.clone();
        let lost: Vec<Retort> = self
            .create
            .build
            .retorts
            .iter()
            .copied()
            .filter(|&r| !retort_available(r, &books))
            .collect();
        for r in lost {
            self.create.build.retorts.remove(&r);
            let args = Args::new()
                .text("retort", self.retort_name(r))
                .text("needs", prerequisite_text(self.game.content(), r));
            self.create.warn = Some(self.text_with("picks.to_select", &args));
        }
    }

    /// Take or drop a retort, with the original's refusals.
    fn click_retort(&mut self, retort: Retort) {
        if self.create.build.retorts.remove(&retort) {
            return;
        }
        let left = i32::from(self.create.build.picks_left());
        let cost = i32::from(retort_cost(retort));
        let message = if left <= 0 {
            Some(self.text("picks.all_made"))
        } else if self.create.build.retorts.len() >= MAX_RETORTS {
            Some(self.text("picks.too_many_retorts"))
        } else if !retort_available(retort, &self.create.build.books) {
            let args = Args::new()
                .text("retort", self.retort_name(retort))
                .text("needs", prerequisite_text(self.game.content(), retort));
            Some(self.text_with("picks.to_select", &args))
        } else if cost > left {
            Some(self.text_with("picks.not_enough", &Args::new().count("cost", cost)))
        } else {
            None
        };
        match message {
            Some(m) => self.create.warn = Some(m),
            None => {
                self.create.build.retorts.insert(retort);
            }
        }
    }

    /// Screen 4: the books and the retorts.
    pub(crate) fn step_picks(&mut self, lay: &Layout, input: &Input) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let portrait = self.create.build.portrait.map(usize::from);
        if let Some(p) = portrait {
            self.draw_pic(lay, "wizards", p, 0, 24.0, 10.0);
        }
        let mouse: Vec2 = input.mouse_position().into();
        let live = self.ng_input();
        let dragging = live && !self.create.ignore_drag;
        let left = self.create.build.picks_left();

        // The rows of books: a drag sets the count (eight pixels a book).
        let mut slide = None;
        for (i, realm) in REALM_ORDER.iter().enumerate() {
            let y = 47.0 + 26.0 * i as f32;
            if ui::hit(lay.rect((190.0, y, 104.0, 22.0)), dragging, input).held() {
                let x = (mouse.x - lay.origin.x) / lay.scale - 190.0;
                slide = Some((*realm, (x / 8.0).floor().max(0.0) as u8));
            }
        }
        if let Some((realm, n)) = slide {
            self.set_books(realm, n);
        }
        for (i, realm) in REALM_ORDER.iter().enumerate() {
            for k in 0..self.books_of(realm) {
                let var = self.create.rows[usize::from(k).min(10)];
                let entry = 24 + book_base(realm) + usize::from(var);
                self.draw_pic(
                    lay,
                    "newgame",
                    entry,
                    0,
                    197.0 + 8.0 * f32::from(k),
                    49.0 + 26.0 * i as f32,
                );
            }
        }
        // The shelf under the portrait: life, death, chaos, nature, sorcery.
        let books = self.create.build.books.clone();
        self.ng_shelf(lay, &REALM_ORDER, &|r| books.get(r).copied().unwrap_or(0));

        // The retorts: three columns of six.
        let mut clicked = None;
        for (i, &retort) in ALL_RETORTS.iter().enumerate() {
            let (col, row) = (i / 6, i % 6);
            let x = [171.0, 210.0, 260.0][col];
            let y = 5.0 + 7.0 * row as f32;
            let known = self.create.build.retorts.contains(&retort);
            let open = retort_available(retort, &self.create.build.books)
                && left >= i16::from(retort_cost(retort))
                && self.create.build.retorts.len() < MAX_RETORTS;
            let ink = if known {
                Color::new(1.0, 0.95, 0.6, 1.0)
            } else if open {
                LIGHT
            } else {
                GREY_INK
            };
            let name = self.retort_name(retort);
            self.draw_styled(view, &Style::nearest(0, SHADOW), &name, x + 1.0, y + 1.0);
            self.draw_styled(view, &Style::nearest(0, ink), &name, x, y);
            if known {
                let mark = [165.0, 204.0, 254.0][col];
                self.draw_pic(lay, "newgame", 52, 0, mark, y + 1.0);
            }
            let (x0, x1) = [(164.0, 202.0), (203.0, 252.0), (253.0, 319.0)][col];
            if ui::hit(lay.rect((x0, y, x1 - x0, 7.0)), live, input).clicked() {
                clicked = Some(retort);
            }
        }
        if let Some(r) = clicked {
            self.click_retort(r);
        }

        // The picks left, the abilities, the name and the OK button.
        let left = self.create.build.picks_left();
        let count = self.text_with("picks.left", &Args::new().count("n", left));
        // ReMoM prints the light text first and the shadow over it, which buries the words;
        // the shadow goes underneath.
        self.draw_styled(
            view,
            &Style::nearest(3, SHADOW).center(),
            &count,
            222.0,
            186.0,
        );
        self.draw_styled(
            view,
            &Style::nearest(3, LIGHT).center(),
            &count,
            221.0,
            185.0,
        );
        let name = self.create.build.name.clone();
        self.ng_name(lay, &name);
        self.ng_abilities(lay);
        let ok = left == 0;
        self.draw_pic(lay, "newgame", if ok { 42 } else { 43 }, 0, 252.0, 182.0);
        if ui::hit(lay.rect((251.0, 181.0, 32.0, 16.0)), live, input).clicked() {
            if ok {
                self.next_spells(0);
            } else {
                self.create.warn = Some(self.text("picks.make_all"));
            }
        } else if live && input.key_pressed(KeyCode::Escape) {
            self.create_back();
        }
    }

    /// The sections of a realm's spell step: `(rarity, choices)` for each that is shown.
    pub(crate) fn spell_sections(&self, realm: &str) -> Vec<(Rarity, usize)> {
        let build = &self.create.build;
        let mut out = Vec::new();
        let common = build.spell_choices_of(
            &self.game.content().settings.spell_library,
            realm,
            Rarity::Common,
        );
        if (1..10).contains(&common) {
            out.push((Rarity::Common, common));
        }
        for rarity in [Rarity::Uncommon, Rarity::Rare] {
            let n =
                build.spell_choices_of(&self.game.content().settings.spell_library, realm, rarity);
            if n > 0 {
                out.push((rarity, n));
            }
        }
        out
    }

    /// Go to the spells of the next realm from `from` that has any to choose, or on to the race.
    pub(crate) fn next_spells(&mut self, from: usize) {
        for (i, &realm) in REALM_ORDER.iter().enumerate().skip(from) {
            if !self.spell_sections(realm).is_empty() {
                // The original starts each realm with its default spells chosen.
                let sections = self.spell_sections(realm);
                self.create
                    .build
                    .spells
                    .retain(|&s| self.game.content().rules.spells.get(s).realm != realm);
                for (rarity, n) in sections {
                    let defaults =
                        f4x_core::wizard::magic::default_spells(self.game.content(), realm, rarity);
                    self.create
                        .build
                        .spells
                        .extend(defaults.into_iter().take(n));
                }
                self.create.step = Step::Spells(i);
                return;
            }
        }
        self.create.step = Step::Race;
    }

    /// The box under the spell lists with the help text of the spell the pointer is over, so the
    /// player knows what they pick; a hint when it is over none.
    fn spell_help_box(
        &mut self,
        lay: &Layout,
        view: (f32, f32, f32),
        top: f32,
        hovered: Option<f4x_core::rules::SpellId>,
    ) {
        let (x, w) = (167.0, 148.0);
        let bottom = 178.0;
        if bottom - top < 24.0 {
            return;
        }
        let r = lay.rect((x, top + 2.0, w, bottom - top - 2.0));
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.1, 0.07, 0.05, 0.75));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.35, 0.25, 0.15, 1.0));
        let Some(id) = hovered else {
            let hint = self.text("picks.spell_hint");
            let lines = self.bm_wrap(0, &hint, w - 8.0);
            for (i, line) in lines.iter().enumerate() {
                self.draw_styled(
                    view,
                    &Style::nearest(0, GREY_INK),
                    line,
                    x + 4.0,
                    top + 6.0 + 7.0 * i as f32,
                );
            }
            return;
        };
        let def = self.game.content().rules.spells.get(id);
        let name = def.name.clone();
        let text = self
            .help
            .topic_for(&f4x_core::help::spell_subject(&def.id))
            .map_or_else(|| self.text("picks.no_spell_help"), |t| t.text.clone());
        self.draw_styled(
            view,
            &Style::nearest(1, Color::new(1.0, 0.95, 0.6, 1.0)),
            &name,
            x + 4.0,
            top + 5.0,
        );
        let rows = (((bottom - top - 2.0) - 17.0) / 7.0).floor().max(1.0) as usize;
        let mut lines = self.bm_wrap(0, &text, w - 8.0);
        if lines.len() > rows {
            lines.truncate(rows);
            if let Some(last) = lines.last_mut() {
                last.push_str("...");
            }
        }
        for (i, line) in lines.iter().enumerate() {
            self.draw_styled(
                view,
                &Style::nearest(0, LIGHT),
                line,
                x + 4.0,
                top + 15.0 + 7.0 * i as f32,
            );
        }
    }

    /// Screen 5: the spells of one realm.
    pub(crate) fn step_spells(&mut self, lay: &Layout, index: usize, input: &Input) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let Some(&realm) = REALM_ORDER.get(index) else {
            self.create.step = Step::Race;
            return;
        };
        let sections = self.spell_sections(realm);
        self.draw_pic(lay, "newgame", 47, 0, 181.0, 18.0);
        self.draw_pic(lay, "newgame", 51, 0, 196.0, 180.0);
        let portrait = self.create.build.portrait.map(usize::from);
        if let Some(p) = portrait {
            self.draw_pic(lay, "wizards", p, 0, 24.0, 10.0);
        }
        let realm_name = self
            .game
            .content()
            .rules
            .realms
            .by_name(realm)
            .map_or_else(|| realm.to_string(), |r| r.name.clone());
        let ink = realm_ink(realm);
        let title = self.text_with(
            "picks.select_spells",
            &Args::new().text("realm", &realm_name),
        );
        self.draw_styled(
            view,
            &Style::nearest(4, SHADOW).center(),
            &title,
            242.0,
            7.0,
        );
        self.draw_styled(view, &Style::nearest(4, ink).center(), &title, 241.0, 6.0);
        let books = self.create.build.books.clone();
        self.ng_shelf(lay, &REALM_ORDER, &|r| books.get(r).copied().unwrap_or(0));
        let live = self.ng_input();
        let pressed = input.mouse_pressed(MouseButton::Left) && live;
        let mut remaining = 0usize;
        let mut toggle = None;
        let mut refused = false;
        let mut hovered = None;
        for (s, &(rarity, want)) in sections.iter().enumerate() {
            let top = 39.0 + 51.0 * s as f32;
            self.draw_pic(lay, "newgame", 48 + s, 0, 167.0, 37.0 + 51.0 * s as f32);
            let label = self.text(match rarity {
                Rarity::Common => "picks.common",
                Rarity::Uncommon => "picks.uncommon",
                Rarity::Rare => "picks.rare",
                Rarity::VeryRare => "picks.very_rare",
            });
            self.draw_styled(
                view,
                &Style::nearest(3, ink),
                &self.text_with(
                    "picks.rarity_row",
                    &Args::new().text("rarity", label).count("n", want),
                ),
                167.0,
                26.0 + 51.0 * s as f32,
            );
            // The commons are listed in the original's order (the realm's default list), the rest in the
            // order of the rules.
            let pool = if rarity == Rarity::Common {
                f4x_core::wizard::magic::default_spells(self.game.content(), realm, rarity)
            } else {
                f4x_core::wizard::magic::spells_of(self.game.content(), realm, rarity)
            };
            let chosen = pool
                .iter()
                .filter(|id| self.create.build.spells.contains(id))
                .count();
            remaining += want.saturating_sub(chosen);
            for (k, id) in pool.iter().take(10).enumerate() {
                let (col, row) = (k / 5, k % 5);
                let x = 175.0 + 74.0 * col as f32;
                let y = top + 7.0 * row as f32 + 1.0;
                let name = self.game.content().rules.spells.get(*id).name.clone();
                let selected = self.create.build.spells.contains(id);
                self.draw_styled(view, &Style::nearest(0, SHADOW), &name, x + 1.0, y + 1.0);
                let colour = if selected {
                    Color::new(1.0, 0.95, 0.6, 1.0)
                } else {
                    GREY_INK
                };
                self.draw_styled(
                    view,
                    &Style::nearest(0, if selected { colour } else { LIGHT }),
                    &name,
                    x,
                    y,
                );
                if selected {
                    self.draw_pic(lay, "newgame", 52, 0, 169.0 + 74.0 * col as f32, y);
                }
                let cell = lay.rect((170.0 + 74.0 * col as f32, top + 7.0 * row as f32, 70.0, 6.0));
                let state = ui::hit(cell, true, input);
                if state.hovered() {
                    hovered = Some(*id);
                }
                if pressed && state.hovered() {
                    if selected {
                        toggle = Some((*id, false));
                    } else if chosen < want {
                        toggle = Some((*id, true));
                    } else {
                        refused = true;
                    }
                }
            }
        }
        self.spell_help_box(lay, view, 39.0 + 51.0 * sections.len() as f32, hovered);
        if let Some((id, add)) = toggle {
            if add {
                self.create.build.spells.push(id);
            } else {
                self.create.build.spells.retain(|&s| s != id);
            }
        }
        if refused {
            self.create.warn = Some(self.text("picks.none_left_here"));
        }
        let count = self.text_with("picks.left", &Args::new().count("n", remaining));
        // ReMoM prints the light text first and the shadow over it, which buries the words;
        // the shadow goes underneath.
        self.draw_styled(
            view,
            &Style::nearest(3, SHADOW).center(),
            &count,
            222.0,
            186.0,
        );
        self.draw_styled(
            view,
            &Style::nearest(3, LIGHT).center(),
            &count,
            221.0,
            185.0,
        );
        let name = self.create.build.name.clone();
        self.ng_name(lay, &name);
        self.ng_abilities(lay);
        let ok = remaining == 0;
        self.draw_pic(lay, "newgame", if ok { 42 } else { 43 }, 0, 252.0, 182.0);
        if ui::hit(lay.rect((251.0, 181.0, 32.0, 16.0)), live, input).clicked() {
            if ok {
                self.next_spells(index + 1);
            } else {
                self.create.warn = Some(self.text("picks.make_all"));
            }
        } else if live && input.key_pressed(KeyCode::Escape) {
            self.create_back();
        }
        let _ = GOLD_INK;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retort_that_is_refused_names_what_it_needs() {
        let content = f4x_data::load_content(std::path::Path::new("../../content")).unwrap();
        assert_eq!(
            prerequisite_text(&content, Retort::ChaosMastery),
            "4 picks in Chaos Magic"
        );
        assert_eq!(
            prerequisite_text(&content, Retort::SageMaster),
            "1 pick in any 2 Realms of Magic"
        );
        assert!(
            prerequisite_text(&content, Retort::NodeMastery).contains("1 pick in Sorcery Magic")
        );
        assert!(prerequisite_text(&content, Retort::Alchemy).is_empty());
    }
}
