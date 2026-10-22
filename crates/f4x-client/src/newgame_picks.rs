//! The custom wizard's two making steps (`Newgame_Screen_4` and `_5`, `NewGame.c`): the picks
//! (drag in a realm's row to set its books, click a retort to take or drop it, eleven picks in
//! all) and, for each realm with two or more books, its starting spells (a click takes or drops
//! one; the commons are chosen up to `books - 1`, and with eleven books two uncommon and one
//! rare). The pick rules are the core's (`WizardBuild`, `retort_available`, `retort_cost`);
//! this file is the pictures and the clicks. Positions are the original's.

use f4x_core::rules::Rarity;
use f4x_core::wizard::magic::Retort;
use f4x_core::wizard::{retort_available, retort_cost, retort_name, ALL_RETORTS, MAX_RETORTS};
use macroquad::prelude::*;

use crate::create_screen::Step;
use crate::frame::{Align, Layout};
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
pub(crate) fn prerequisite_text(retort: Retort) -> String {
    let pick =
        |n: u8, realm: &str| format!("{n} pick{} in {realm} Magic", if n > 1 { "s" } else { "" });
    match retort {
        Retort::SageMaster => "1 pick in any 2 Realms of Magic".into(),
        Retort::Runemaster => "2 picks in any 3 Realms of Magic".into(),
        Retort::Archmage | Retort::ManaFocusing => "4 picks in any Realm of Magic".into(),
        Retort::ChaosMastery => pick(4, "Chaos"),
        Retort::NatureMastery => pick(4, "Nature"),
        Retort::SorceryMastery => pick(4, "Sorcery"),
        Retort::InfernalPower => pick(4, "Death"),
        Retort::DivinePower => pick(4, "Life"),
        Retort::NodeMastery => format!(
            "{}, {}, {}",
            pick(1, "Chaos"),
            pick(1, "Sorcery"),
            pick(1, "Nature")
        ),
        _ => String::new(),
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
            self.create.warn = Some("You can not select both Life and Death magic".into());
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
            self.create.warn = Some(format!(
                "To select {} you need:   {}",
                retort_name(r),
                prerequisite_text(r)
            ));
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
            Some("You have already made all your picks".to_string())
        } else if self.create.build.retorts.len() >= MAX_RETORTS {
            Some("You may not select more than 6 special abilities".to_string())
        } else if !retort_available(retort, &self.create.build.books) {
            Some(format!(
                "To select {} you need:   {}",
                retort_name(retort),
                prerequisite_text(retort)
            ))
        } else if cost > left {
            Some(format!(
                "You don't have enough picks left to make this selection. You need {cost} picks"
            ))
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
    pub(crate) fn step_picks(&mut self, lay: &Layout) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let portrait = self.create.build.portrait.map(usize::from);
        if let Some(p) = portrait {
            self.draw_pic(lay, "wizards", p, 0, 24.0, 10.0);
        }
        let mouse: Vec2 = mouse_position().into();
        let input = self.ng_input();
        let down = is_mouse_button_down(MouseButton::Left) && input;
        let pressed = is_mouse_button_pressed(MouseButton::Left) && input;
        let left = self.create.build.picks_left();

        // The rows of books: a drag sets the count (eight pixels a book).
        let mut slide = None;
        for (i, realm) in REALM_ORDER.iter().enumerate() {
            let y = 47.0 + 26.0 * i as f32;
            if down && lay.rect((190.0, y, 104.0, 22.0)).contains(mouse) {
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
            let name = retort_name(retort);
            self.bm_text(view, 0, SHADOW, name, x + 1.0, y + 1.0, Align::Left);
            self.bm_text(view, 0, ink, name, x, y, Align::Left);
            if known {
                let mark = [165.0, 204.0, 254.0][col];
                self.draw_pic(lay, "newgame", 52, 0, mark, y + 1.0);
            }
            let (x0, x1) = [(164.0, 202.0), (203.0, 252.0), (253.0, 319.0)][col];
            if pressed && lay.rect((x0, y, x1 - x0, 7.0)).contains(mouse) {
                clicked = Some(retort);
            }
        }
        if let Some(r) = clicked {
            self.click_retort(r);
        }

        // The picks left, the abilities, the name and the OK button.
        let left = self.create.build.picks_left();
        let count = format!("{left} picks");
        // ReMoM prints the light text first and the shadow over it, which buries the words;
        // the shadow goes underneath.
        self.bm_text(view, 3, SHADOW, &count, 222.0, 186.0, Align::Center);
        self.bm_text(view, 3, LIGHT, &count, 221.0, 185.0, Align::Center);
        let name = self.create.build.name.clone();
        self.ng_name(lay, &name);
        self.ng_abilities(lay);
        let ok = left == 0;
        self.draw_pic(lay, "newgame", if ok { 42 } else { 43 }, 0, 252.0, 182.0);
        if input
            && is_mouse_button_pressed(MouseButton::Left)
            && lay.rect((251.0, 181.0, 32.0, 16.0)).contains(mouse)
        {
            if ok {
                self.next_spells(0);
            } else {
                self.create.warn =
                    Some("You need to make all your picks before you can continue".into());
            }
        } else if input && is_key_pressed(KeyCode::Escape) {
            self.create.step = Step::Name;
        }
    }

    /// The sections of a realm's spell step: `(rarity, choices)` for each that is shown.
    fn spell_sections(&self, realm: &str) -> Vec<(Rarity, usize)> {
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

    /// Screen 5: the spells of one realm.
    pub(crate) fn step_spells(&mut self, lay: &Layout, index: usize) {
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
        self.bm_text(
            view,
            4,
            SHADOW,
            &format!("Select {realm_name} Spells"),
            242.0,
            7.0,
            Align::Center,
        );
        self.bm_text(
            view,
            4,
            ink,
            &format!("Select {realm_name} Spells"),
            241.0,
            6.0,
            Align::Center,
        );
        let books = self.create.build.books.clone();
        self.ng_shelf(lay, &REALM_ORDER, &|r| books.get(r).copied().unwrap_or(0));
        let mouse: Vec2 = mouse_position().into();
        let input = self.ng_input();
        let pressed = is_mouse_button_pressed(MouseButton::Left) && input;
        let mut remaining = 0usize;
        let mut toggle = None;
        let mut refused = false;
        for (s, &(rarity, want)) in sections.iter().enumerate() {
            let top = 39.0 + 51.0 * s as f32;
            self.draw_pic(lay, "newgame", 48 + s, 0, 167.0, 37.0 + 51.0 * s as f32);
            let label = match rarity {
                Rarity::Common => "Common",
                Rarity::Uncommon => "Uncommon",
                Rarity::Rare => "Rare",
                Rarity::VeryRare => "Very Rare",
            };
            self.bm_text(
                view,
                3,
                ink,
                &format!("{label}: {want}"),
                167.0,
                26.0 + 51.0 * s as f32,
                Align::Left,
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
                self.bm_text(view, 0, SHADOW, &name, x + 1.0, y + 1.0, Align::Left);
                let colour = if selected {
                    Color::new(1.0, 0.95, 0.6, 1.0)
                } else {
                    GREY_INK
                };
                self.bm_text(
                    view,
                    0,
                    if selected { colour } else { LIGHT },
                    &name,
                    x,
                    y,
                    Align::Left,
                );
                if selected {
                    self.draw_pic(lay, "newgame", 52, 0, 169.0 + 74.0 * col as f32, y);
                }
                let cell = lay.rect((170.0 + 74.0 * col as f32, top + 7.0 * row as f32, 70.0, 6.0));
                if pressed && cell.contains(mouse) {
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
        if let Some((id, add)) = toggle {
            if add {
                self.create.build.spells.push(id);
            } else {
                self.create.build.spells.retain(|&s| s != id);
            }
        }
        if refused {
            self.create.warn = Some(
                "You have no picks left in this area, to deselect click on a selected item".into(),
            );
        }
        let count = format!("{remaining} picks");
        // ReMoM prints the light text first and the shadow over it, which buries the words;
        // the shadow goes underneath.
        self.bm_text(view, 3, SHADOW, &count, 222.0, 186.0, Align::Center);
        self.bm_text(view, 3, LIGHT, &count, 221.0, 185.0, Align::Center);
        let name = self.create.build.name.clone();
        self.ng_name(lay, &name);
        self.ng_abilities(lay);
        let ok = remaining == 0;
        self.draw_pic(lay, "newgame", if ok { 42 } else { 43 }, 0, 252.0, 182.0);
        if input
            && is_mouse_button_pressed(MouseButton::Left)
            && lay.rect((251.0, 181.0, 32.0, 16.0)).contains(mouse)
        {
            if ok {
                self.next_spells(index + 1);
            } else {
                self.create.warn =
                    Some("You need to make all your picks before you can continue".into());
            }
        } else if input && is_key_pressed(KeyCode::Escape) {
            self.create.step = Step::Picks;
        }
        let _ = GOLD_INK;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retort_that_is_refused_names_what_it_needs() {
        assert_eq!(
            prerequisite_text(Retort::ChaosMastery),
            "4 picks in Chaos Magic"
        );
        assert_eq!(
            prerequisite_text(Retort::SageMaster),
            "1 pick in any 2 Realms of Magic"
        );
        assert!(prerequisite_text(Retort::NodeMastery).contains("1 pick in Sorcery Magic"));
        assert!(prerequisite_text(Retort::Alchemy).is_empty());
    }
}
