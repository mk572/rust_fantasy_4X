//! The offers made at the turn change (task 220): a hero for hire, a company of mercenaries and
//! the merchant's item, laid out as the original's `HIRE.c` does (`Hire_Hero_Popup`,
//! `Hire_Merc_Popup`, `Merchant_Popup`) over the map. The box stays until the player answers,
//! and the next offer comes after it. Without the local art the pictures are missing and the
//! text and buttons are still drawn.

use f4x_core::game::Command;
use f4x_core::player::offer::Offer;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::unit_sheet::Source;
use crate::view::{App, HUMAN};

/// What the box remembers between frames.
#[derive(Default)]
pub struct OfferUi {
    /// The offer the name below was made for; a new offer starts a new name.
    shown: Option<Offer>,
    /// The name typed for a hero.
    name: String,
    /// The hero was hired and is being named.
    naming: bool,
}

/// The longest name a hero can be given.
const NAME_LEN: usize = 16;
/// Where the hire pop-ups put the unit's sheet (`Hire_Hero_Popup`: `window_x`, `window_y`).
const WINDOW: (f32, f32) = (25.0, 17.0);

impl App {
    /// For screenshots and checks: give the human wizard the fame, gold and turn an offer
    /// needs and make the offer (`hero`, `mercenaries` or `item`).
    pub fn open_test_offer(&mut self, kind: &str) {
        use f4x_core::player::offer::OfferKind;
        let what = match kind {
            "hero" => OfferKind::Hero,
            "mercenaries" => OfferKind::Mercenaries,
            _ => OfferKind::Item,
        };
        if let Some(p) = self.game.player_mut(HUMAN) {
            p.fame = 100;
            p.gold = 5000;
        }
        self.game.turn = self.game.turn.max(40);
        // A hero is drawn at random from those fame allows: try until one comes.
        let made = (0..200).any(|_| self.game.cheat_offer(HUMAN, what).is_some());
        if !made {
            eprintln!("nothing to offer");
        }
    }

    /// The first unanswered offer of the human wizard, shown over the map until answered.
    /// Returns whether the box took the frame.
    pub(crate) fn offer_prompt_frame(&mut self) -> bool {
        let Some(offer) = self.game.offers(HUMAN).first().cloned() else {
            self.offer_ui.shown = None;
            return false;
        };
        if self.offer_ui.shown.as_ref() != Some(&offer) {
            self.offer_ui.name = match &offer {
                Offer::Hero { kind, .. } => self
                    .game
                    .roster(HUMAN)
                    .and_then(|r| r.get(*kind))
                    .map(|r| r.name.clone())
                    .unwrap_or_default(),
                _ => String::new(),
            };
            self.offer_ui.naming = false;
            self.offer_ui.shown = Some(offer.clone());
            self.sheet_ui.first = 0;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        let lay = Layout::new();
        let affordable = self
            .game
            .player(HUMAN)
            .is_some_and(|p| p.gold >= offer.price());
        let (accept, reject) = match &offer {
            Offer::Item { item, price } => self.merchant_box(&lay, item, *price, affordable),
            _ => self.hire_box(&lay, &offer, affordable),
        };
        if accept {
            let hero = matches!(offer, Offer::Hero { .. });
            if hero && !self.offer_ui.naming {
                // A hero is named first (`Setup_Input_Box_Popup`), then the hire is made.
                self.offer_ui.naming = true;
            } else {
                let name = hero.then(|| self.offer_ui.name.clone());
                let events = self.game.apply(Command::AcceptOffer {
                    player: HUMAN,
                    index: 0,
                    name,
                });
                self.note(events);
            }
        } else if reject {
            if self.offer_ui.naming {
                self.offer_ui.naming = false;
            } else {
                let events = self.game.apply(Command::RejectOffer {
                    player: HUMAN,
                    index: 0,
                });
                self.note(events);
            }
        }
        true
    }

    /// `Merchant_Popup`: the banner, the merchant's words, the item's picture and the Buy and
    /// Reject buttons. Returns (buy, reject).
    fn merchant_box(
        &mut self,
        lay: &Layout,
        item: &f4x_core::item::Item,
        price: i32,
        affordable: bool,
    ) -> (bool, bool) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (x, y) = (5.0, 15.0);
        if self.draw_pic(lay, "hire", 2, 0, x, y).is_none() {
            let r = lay.rect((x, y, 312.0, 161.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.1, 0.07, 0.04, 0.96));
        }
        let text = format!(
            "A merchant arrives and offers a magic {} for sale.  The price is only {price} gold pieces.",
            item.name
        );
        let gold = Color::new(0.95, 0.75, 0.25, 1.0);
        let mut at = y + 9.0;
        for line in self.bm_wrap(4, &text, 182.0) {
            self.bm_text(view, 4, gold, &line, x + 57.0, at, Align::Left);
            at += self.art.text_height(4) + 2.0;
        }
        self.draw_item_card(lay, item, x + 14.0, y + 65.0);
        let buy = self.red_button(lay, "Buy", x + 252.0, y + 121.0);
        let reject = self.red_button(lay, "Reject", x + 252.0, y + 140.0);
        let buy =
            (buy || is_key_pressed(KeyCode::Y) || is_key_pressed(KeyCode::Enter)) && affordable;
        let reject = reject || is_key_pressed(KeyCode::N) || is_key_pressed(KeyCode::Escape);
        (buy && !reject, reject)
    }

    /// `Hire_Hero_Popup` and `Hire_Merc_Popup`: the unit's sheet with the banner above it, the
    /// price in the title and the Hire and Reject buttons; a hero is then named in a box over
    /// the sheet. Returns (hire, reject).
    fn hire_box(&mut self, lay: &Layout, offer: &Offer, affordable: bool) -> (bool, bool) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (wx, wy) = WINDOW;
        let gold = Color::new(0.95, 0.75, 0.25, 1.0);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let rules = self.game.content().rules.clone();
        let (def, level) = match offer {
            Offer::Hero { kind, .. } => {
                let unit = rules
                    .hero_templates
                    .get(*kind)
                    .and_then(|t| rules.units.by_name(&t.unit));
                let level = self
                    .game
                    .roster(HUMAN)
                    .zip(f4x_core::hero::roster::unit_of(&rules, *kind))
                    .and_then(|(r, u)| {
                        r.get(*kind)
                            .map(|rec| f4x_core::hero::roster::level_of(&rules, u, rec.xp))
                    });
                (unit, level)
            }
            Offer::Mercenaries { unit, level, .. } => {
                (rules.units.by_name(unit), Some(usize::from(*level)))
            }
            Offer::Item { .. } => (None, None),
        };
        if let Some(def) = def {
            let sheet = self.offer_sheet(def, level, HUMAN.0);
            debug_assert_eq!(sheet.source, Source::Offer);
            self.draw_sheet_panel(lay, &sheet, (wx, wy), true);
        }
        self.draw_pic(lay, "hire", 0, 0, 0.0, 0.0);
        let price = offer.price();
        let title = match offer {
            Offer::Hero { .. } => format!("Hero for Hire: {price} gold"),
            Offer::Mercenaries { count, .. } if *count == 1 => {
                format!("Mercenary for Hire: {price} gold")
            }
            Offer::Mercenaries { count, .. } => {
                format!("{count} Mercenaries for Hire: {price} gold")
            }
            Offer::Item { .. } => String::new(),
        };
        self.bm_text(view, 4, gold, &title, wx + 103.0, wy - 14.0, Align::Center);

        let naming = self.offer_ui.naming;
        if naming {
            // `Hire_Hero_Popup_Draw`: the name box (`backgrnd` 33) with its prompt and the name.
            self.draw_pic(lay, "backgrnd", 33, 0, wx + 29.0, wy + 55.0);
            self.bm_text(
                view,
                4,
                gold,
                "Name Your Hero",
                wx + 104.0,
                wy + 58.0,
                Align::Center,
            );
            while let Some(c) = get_char_pressed() {
                if !c.is_control() && self.offer_ui.name.chars().count() < NAME_LEN {
                    self.offer_ui.name.push(c);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                self.offer_ui.name.pop();
            }
            let shown = format!("{}_", self.offer_ui.name);
            self.bm_text(view, 4, light, &shown, wx + 45.0, wy + 75.0, Align::Left);
        }
        if !affordable {
            self.bm_text(
                view,
                1,
                Color::new(1.0, 0.4, 0.3, 1.0),
                "Not enough gold",
                wx + 221.0,
                wy + 128.0,
                Align::Left,
            );
        }
        let label = if matches!(offer, Offer::Hero { .. }) && naming {
            "Accept"
        } else {
            "Hire"
        };
        let hire = self.red_button(lay, label, wx + 221.0, wy + 143.0);
        let reject = self.red_button(lay, "Reject", wx + 221.0, wy + 162.0);
        // H hires (the original's hotkey); while naming Enter ends the name.
        let key = is_key_pressed(KeyCode::Enter) || (!naming && is_key_pressed(KeyCode::H));
        let hire = (hire || key) && affordable;
        let reject = reject || is_key_pressed(KeyCode::Escape);
        (hire && !reject, reject)
    }

    /// The item card of `Item_View_Prepare` (`itemisc` 25, 220 by 83) with the item's picture,
    /// its name and up to five lines of what it gives, each with a bullet. The original adds
    /// the text of each special power after its name; those texts are not extracted.
    pub(crate) fn draw_item_card(
        &mut self,
        lay: &Layout,
        item: &f4x_core::item::Item,
        x: f32,
        y: f32,
    ) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let gold = Color::new(0.95, 0.75, 0.25, 1.0);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        self.draw_pic(lay, "itemisc", 25, 0, x, y);
        self.draw_pic(lay, "items", usize::from(item.icon), 0, x + 9.0, y + 6.0);
        self.bm_text(view, 4, gold, &item.name, x + 31.0, y + 12.0, Align::Left);
        let rules = self.game.content().rules.clone();
        let b = item.bonus;
        let mut lines: Vec<String> = Vec::new();
        for (v, what) in [
            (b.attack, "Attack"),
            (b.to_hit, "To Hit"),
            (b.defense, "Defense"),
            (b.moves / 2, "Movement"),
            (b.resistance, "Resistance"),
            (b.spell_skill, "Spell Skill"),
        ] {
            if v > 0 {
                lines.push(format!("+{v} {what}"));
            }
        }
        if b.spell_save > 0 {
            lines.push(format!("-{} Spell Save", b.spell_save));
        }
        if let Some((spell, count)) = &item.charges {
            let name = rules
                .spells
                .by_name(spell)
                .map_or(spell.as_str(), |d| d.name.as_str());
            lines.push(format!("{name} x {count}"));
        }
        for id in &item.powers {
            lines.push(
                rules
                    .item_powers
                    .by_name(id)
                    .map_or(id.clone(), |d| d.name.clone()),
            );
        }
        lines.truncate(5);
        let (space, top) = if lines.len() == 5 {
            (8.0, 34.0)
        } else {
            (11.0, 33.0)
        };
        for (i, line) in lines.iter().enumerate() {
            let at = y + top + i as f32 * space;
            self.draw_pic(lay, "itemisc", 26, 0, x + 11.0, y + 34.0 + i as f32 * space);
            self.bm_text(view, 2, light, line, x + 20.0, at - 1.0, Align::Left);
        }
    }

    /// Words wrapped to `width` pixels in one of the original's bitmap fonts.
    pub(crate) fn bm_wrap(&self, style: usize, text: &str, width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in text.split_whitespace() {
            let attempt = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && self.art.text_width(style, &attempt) > width {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = attempt;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }
}
