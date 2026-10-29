//! The offers made at the turn change (task 220): a hero for hire, a company of mercenaries and
//! the merchant's item, laid out as the original's `HIRE.c` does (`Hire_Hero_Popup`,
//! `Hire_Merc_Popup`, `Merchant_Popup`) over the map. The box stays until the player answers,
//! and the next offer comes after it. Without the local art the pictures are missing and the
//! text and buttons are still drawn.

use crate::input::Input;
use f4x_core::game::Command;
use f4x_core::player::offer::Offer;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::modal::{keep, kind, Modal};
use crate::unit_sheet::Source;
use crate::view::{App, Screen, HUMAN};

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

/// The card line of each special power: its name and what it does (`ITEM_ViewTexts`).
const POWER_LINES: [(&str, &str); 32] = [
    ("vampiric", "item_power.vampiric"),
    ("lightning", "item_power.lightning"),
    ("cloak_of_fear", "item_power.cloak_of_fear"),
    ("destruction", "item_power.destruction"),
    ("wraithform", "item_power.wraithform"),
    ("regeneration", "item_power.regeneration"),
    ("pathfinding", "item_power.pathfinding"),
    ("water_walking", "item_power.water_walking"),
    ("resist_elements", "item_power.resist_elements"),
    ("elemental_armor", "item_power.elemental_armor"),
    ("chaos", "item_power.chaos"),
    ("endurance", "item_power.endurance"),
    ("haste", "item_power.haste"),
    ("invisibility", "item_power.invisibility"),
    ("death", "item_power.death"),
    ("flight", "item_power.flight"),
    ("resist_magic", "item_power.resist_magic"),
    ("magic_immunity", "item_power.magic_immunity"),
    ("power_drain", "item_power.power_drain"),
    ("holy_avenger", "item_power.holy_avenger"),
    ("true_sight", "item_power.true_sight"),
    ("phantasmal", "item_power.phantasmal"),
    ("flaming", "item_power.flaming"),
    ("bless", "item_power.bless"),
    ("lion_heart", "item_power.lion_heart"),
    ("stoning", "item_power.stoning"),
    ("planar_travel", "item_power.planar_travel"),
    ("merging", "item_power.merging"),
    ("righteousness", "item_power.righteousness"),
    ("invulnerability", "item_power.invulnerability"),
    ("guardian_wind", "item_power.guardian_wind"),
    ("giant_strength", "item_power.giant_strength"),
];

/// The first offer made to the human wizard; the offers themselves live in the game.
pub(crate) struct OfferAsk;

impl Modal for OfferAsk {
    fn kind(&self) -> &'static str {
        kind::OFFER
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.offer_prompt_frame(input).then_some(OfferAsk))
    }
}

impl App {
    /// Open the box for the offers that wait, over the map.
    pub(crate) fn queue_offer(&mut self) {
        if !self.game.in_battle()
            && matches!(self.screen, Screen::Map)
            && !self.modals.has(kind::OFFER)
            && !self.game.offers(HUMAN).is_empty()
        {
            self.modals.enqueue(OfferAsk);
        }
    }

    /// For screenshots and checks: give the human wizard the fame, gold and turn an offer
    /// needs and make the offer (`hero`, `mercenaries` or `item`).
    pub fn open_test_offer(&mut self, kind: &str) {
        use f4x_core::player::offer::OfferKind;
        let what = match kind {
            "hero" | "prisoner" => OfferKind::Hero,
            "mercenaries" => OfferKind::Mercenaries,
            _ => OfferKind::Item,
        };
        self.game.set_fame(HUMAN, 100);
        if let Some(t) = self.game.treasury_mut(HUMAN) {
            t.gold = 5000;
        }
        self.game.turn = self.game.turn.max(40);
        // A hero is drawn at random from those fame allows: try until one comes.
        let made = (0..200).any(|_| self.game.cheat_offer(HUMAN, what).is_some());
        if !made {
            eprintln!("nothing to offer");
        }
        if kind == "prisoner" {
            // The same hero, free: what a cleared lair's prisoner is.
            if let Some(offers) = self
                .game
                .player_component_mut::<f4x_core::player::Offers>(HUMAN)
            {
                if let Some(Offer::Hero { price, .. }) = offers.0.first_mut() {
                    *price = 0;
                }
            }
        }
    }

    /// The first unanswered offer of the human wizard, shown over the map until answered.
    /// Returns whether the box took the frame.
    pub(crate) fn offer_prompt_frame(&mut self, input: &Input) -> bool {
        let Some(offer) = self.game.offers(HUMAN).first().cloned() else {
            self.offer_ui.shown = None;
            return false;
        };
        if self.offer_ui.shown.as_ref() != Some(&offer) {
            self.offer_ui.name = match &offer {
                Offer::Hero { kind, .. } => {
                    self.game.units().hero_type_name(*kind).unwrap_or_default()
                }
                _ => String::new(),
            };
            self.offer_ui.naming = false;
            self.offer_ui.shown = Some(offer.clone());
            self.sheet_ui.first = 0;
        }
        let lay = Layout::new();
        let affordable = self
            .game
            .player(HUMAN)
            .is_some_and(|p| p.treasury().gold >= offer.price());
        let (accept, reject) = match &offer {
            Offer::Item { item, price } => {
                self.merchant_box(&lay, item, *price, "offer.merchant", affordable, input)
            }
            Offer::Resale { item, price, .. } => {
                self.merchant_box(&lay, item, *price, "offer.resale", affordable, input)
            }
            _ => self.hire_box(&lay, &offer, affordable, input),
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
                let mut events = self.game.apply(Command::RejectOffer {
                    player: HUMAN,
                    index: 0,
                });
                // A resold item goes on to the next wizard, who may be a computer one.
                events.extend(f4x_ai::answer_resales(&mut self.game));
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
        key: &str,
        affordable: bool,
        input: &Input,
    ) -> (bool, bool) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (x, y) = (5.0, 15.0);
        if self.draw_pic(lay, "hire", 2, 0, x, y).is_none() {
            let r = lay.rect((x, y, 312.0, 161.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.1, 0.07, 0.04, 0.96));
        }
        let name = item.display_name(self.game.content());
        let text = self.text_with(key, &Args::new().text("item", &name).count("price", price));
        let gold = Color::new(0.95, 0.75, 0.25, 1.0);
        let mut at = y + 9.0;
        for line in self.bm_wrap(4, &text, 182.0) {
            self.bm_text(view, 4, gold, &line, x + 57.0, at, Align::Left);
            at += self.art.text_height(4) + 2.0;
        }
        self.draw_item_card(lay, item, x + 14.0, y + 65.0);
        let (buy_label, reject_label) = (self.text("ui.buy"), self.text("ui.reject"));
        let buy = self.red_button(lay, &buy_label, x + 252.0, y + 121.0, input);
        let reject = self.red_button(lay, &reject_label, x + 252.0, y + 140.0, input);
        let buy = (buy || input.key_pressed(KeyCode::Y) || input.key_pressed(KeyCode::Enter))
            && affordable;
        let reject = reject || input.key_pressed(KeyCode::N) || input.key_pressed(KeyCode::Escape);
        (buy && !reject, reject)
    }

    /// `Hire_Hero_Popup` and `Hire_Merc_Popup`: the unit's sheet with the banner above it, the
    /// price in the title and the Hire and Reject buttons; a hero is then named in a box over
    /// the sheet. Returns (hire, reject).
    fn hire_box(
        &mut self,
        lay: &Layout,
        offer: &Offer,
        affordable: bool,
        input: &Input,
    ) -> (bool, bool) {
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
                let level = self.game.units().hero_kept_level(*kind);
                (unit, Some(level))
            }
            Offer::Mercenaries { unit, level, .. } => {
                (rules.units.by_name(unit), Some(usize::from(*level)))
            }
            Offer::Item { .. } | Offer::Resale { .. } => (None, None),
        };
        if let Some(def) = def {
            let sheet = self.offer_sheet(def, level, HUMAN.0);
            debug_assert_eq!(sheet.source, Source::Offer);
            self.draw_sheet_panel(lay, &sheet, (wx, wy), true, input);
        }
        self.draw_pic(lay, "hire", 0, 0, 0.0, 0.0);
        let price = offer.price();
        let title = match offer {
            // A hero that costs nothing is a prisoner set free (`GAME_HeroHireType` 2).
            Offer::Hero { price: 0, .. } => self.text("offer.prisoner"),
            Offer::Hero { .. } => self.text_with("offer.hero", &Args::new().count("price", price)),
            Offer::Mercenaries { count, .. } => self.text_with(
                "offer.mercenaries",
                &Args::new().count("n", *count).count("price", price),
            ),
            Offer::Item { .. } | Offer::Resale { .. } => String::new(),
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
                &self.text("offer.name_hero"),
                wx + 104.0,
                wy + 58.0,
                Align::Center,
            );
            for c in input.typed() {
                if !c.is_control() && self.offer_ui.name.chars().count() < NAME_LEN {
                    self.offer_ui.name.push(c);
                }
            }
            if input.key_pressed(KeyCode::Backspace) {
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
                &self.text("offer.not_enough_gold"),
                wx + 221.0,
                wy + 128.0,
                Align::Left,
            );
        }
        let free_hero = matches!(offer, Offer::Hero { price: 0, .. });
        let label = self.text(
            if matches!(offer, Offer::Hero { .. }) && (naming || free_hero) {
                "ui.accept"
            } else {
                "ui.hire"
            },
        );
        let reject_label = self.text("ui.reject");
        let hire = self.red_button(lay, &label, wx + 221.0, wy + 143.0, input);
        let reject = self.red_button(lay, &reject_label, wx + 221.0, wy + 162.0, input);
        // H hires (the original's hotkey); while naming Enter ends the name.
        let key = input.key_pressed(KeyCode::Enter)
            || (!naming && input.key_pressed(if free_hero { KeyCode::A } else { KeyCode::H }));
        let hire = (hire || key) && affordable;
        let reject = reject || input.key_pressed(KeyCode::Escape);
        (hire && !reject, reject)
    }

    /// The item card of `Item_View_Prepare` (`itemisc` 25, 220 by 83) with the item's picture,
    /// its name and up to five lines of what it gives, each with a bullet. A special power's line
    /// says what it does after its name (our words; the original's are `ITEM_ViewTexts`).
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
        let name = item.display_name(self.game.content());
        self.bm_text(view, 4, gold, &name, x + 31.0, y + 12.0, Align::Left);
        let rules = self.game.content().rules.clone();
        let b = item.bonus;
        let mut lines: Vec<String> = Vec::new();
        for (v, what) in [
            (b.attack, "stat.attack"),
            (b.to_hit, "stat.to_hit"),
            (b.defense, "stat.defense"),
            (b.moves / 2, "stat.movement"),
            (b.resistance, "stat.resistance"),
            (b.spell_skill, "stat.spell_skill"),
        ] {
            if v > 0 {
                let args = Args::new().count("n", v).text("stat", self.text(what));
                lines.push(self.text_with("item.bonus", &args));
            }
        }
        if b.spell_save > 0 {
            let args = Args::new().count("n", b.spell_save);
            lines.push(self.text_with("item.spell_save", &args));
        }
        if let Some((spell, count)) = &item.charges {
            let name = rules
                .spells
                .by_name(spell)
                .map_or(spell.as_str(), |d| d.name.as_str());
            let args = Args::new().text("spell", name).count("n", *count);
            lines.push(self.text_with("item.charges", &args));
        }
        for id in &item.powers {
            let line = POWER_LINES
                .iter()
                .find(|(power, _)| power == id)
                .map(|(_, key)| self.text(key));
            lines.push(line.unwrap_or_else(|| {
                rules
                    .item_powers
                    .by_name(id)
                    .map_or(id.clone(), |d| d.name.clone())
            }));
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
}
