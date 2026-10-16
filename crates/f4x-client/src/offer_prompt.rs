//! The offers made at the turn change (task 220): a hero for hire, a company of mercenaries and
//! the merchant's item, as a box over the map. The box stays until the player answers, and the
//! next offer comes after it. Plain boxes for now; the original's pictures are task 143.

use f4x_core::offers::Offer;
use f4x_core::Command;
use macroquad::prelude::*;

use crate::ui::{button, wrap, GOLD};
use crate::view::{App, HUMAN};

/// What the box remembers between frames.
#[derive(Default)]
pub struct OfferUi {
    /// The offer the name below was made for; a new offer starts a new name.
    shown: Option<Offer>,
    /// The name typed for a hero.
    name: String,
}

/// The longest name a hero can be given.
const NAME_LEN: usize = 16;

impl App {
    /// For screenshots and checks: give the human wizard the fame, gold and turn an offer
    /// needs and make the offer (`hero`, `mercenaries` or `item`).
    pub fn open_test_offer(&mut self, kind: &str) {
        use f4x_core::offers::OfferKind;
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
        let hero = matches!(offer, Offer::Hero { .. });
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
            self.offer_ui.shown = Some(offer.clone());
        }
        let affordable = self
            .game
            .player(HUMAN)
            .is_some_and(|p| p.gold >= offer.price());
        if hero {
            while let Some(c) = get_char_pressed() {
                if !c.is_control() && self.offer_ui.name.chars().count() < NAME_LEN {
                    self.offer_ui.name.push(c);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                self.offer_ui.name.pop();
            }
        }
        // Enter takes the offer and Escape turns it down; for the others Y and N do the same.
        let accept_key = is_key_pressed(KeyCode::Enter) || (!hero && is_key_pressed(KeyCode::Y));
        let reject_key = is_key_pressed(KeyCode::Escape) || (!hero && is_key_pressed(KeyCode::N));

        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();

        let (w, h) = (screen_width(), screen_height());
        let (bw, bh) = (620.0, 330.0);
        let (x, y) = ((w - bw) / 2.0, (h - bh) / 2.0);
        draw_rectangle(x, y, bw, bh, Color::new(0.08, 0.06, 0.04, 0.96));
        draw_rectangle_lines(x, y, bw, bh, 3.0, GOLD);
        crate::frame::text_centered(&self.offer_title(&offer), w / 2.0, y + 36.0, 26.0, GOLD);
        let mut at = y + 72.0;
        for line in self.offer_lines(&offer) {
            for part in wrap(&line, 20.0, bw - 50.0) {
                draw_text(&part, x + 25.0, at, 20.0, WHITE);
                at += 24.0;
            }
        }
        if hero {
            let typed = format!("Name: {}_", self.offer_ui.name);
            draw_text(&typed, x + 25.0, y + bh - 112.0, 24.0, GOLD);
        }
        let gold = self.game.player(HUMAN).map_or(0, |p| p.gold);
        let cost = format!("{} gold   (you have {gold})", offer.price());
        draw_text(
            &cost,
            x + 25.0,
            y + bh - 78.0,
            20.0,
            if affordable { WHITE } else { RED },
        );
        let accept = button(
            Rect::new(x + 25.0, y + bh - 56.0, 280.0, 38.0),
            if hero { "Hire (Enter)" } else { "Accept (Y)" },
            affordable,
        );
        let reject = button(
            Rect::new(x + bw - 305.0, y + bh - 56.0, 280.0, 38.0),
            if hero { "Refuse (Esc)" } else { "Decline (N)" },
            true,
        );
        if (accept || (accept_key && affordable)) && !reject {
            let name = hero.then(|| self.offer_ui.name.clone());
            let events = self.game.apply(Command::AcceptOffer {
                player: HUMAN,
                index: 0,
                name,
            });
            self.note(events);
        } else if reject || reject_key {
            let events = self.game.apply(Command::RejectOffer {
                player: HUMAN,
                index: 0,
            });
            self.note(events);
        }
        true
    }

    fn offer_title(&self, offer: &Offer) -> String {
        match offer {
            Offer::Hero { .. } => "A hero offers his service".to_string(),
            Offer::Mercenaries { .. } => "Mercenaries offer their service".to_string(),
            Offer::Item { .. } => "A merchant offers a magic item".to_string(),
        }
    }

    /// The text of the box: what is offered and what it can do.
    fn offer_lines(&self, offer: &Offer) -> Vec<String> {
        let rules = &self.game.content().rules;
        match offer {
            Offer::Hero { kind, .. } => {
                let unit = rules
                    .hero_templates
                    .get(*kind)
                    .and_then(|t| rules.units.by_name(&t.unit));
                let name = unit.map_or("A hero", |u| u.name.as_str());
                let level = self
                    .game
                    .roster(HUMAN)
                    .zip(f4x_core::roster::unit_of(rules, *kind))
                    .and_then(|(r, u)| {
                        r.get(*kind)
                            .map(|rec| f4x_core::roster::level_of(rules, u, rec.xp) + 1)
                    })
                    .unwrap_or(1);
                vec![
                    format!("{name}, level {level}, wishes to join your army."),
                    "Give the hero a name, then hire him.".to_string(),
                ]
            }
            Offer::Mercenaries {
                unit, count, level, ..
            } => {
                let name = rules
                    .units
                    .by_name(unit)
                    .map_or(unit.as_str(), |u| u.name.as_str());
                vec![format!(
                    "A company of {count} {name} (level {}) offers to join your army at your fortress.",
                    level + 1
                )]
            }
            Offer::Item { item, .. } => {
                let b = item.bonus;
                let mut bonuses = Vec::new();
                for (label, v) in [
                    ("attack", b.attack),
                    ("to hit", b.to_hit),
                    ("defence", b.defense),
                    ("resistance", b.resistance),
                    ("spell skill", b.spell_skill),
                ] {
                    if v != 0 {
                        bonuses.push(format!("{v:+} {label}"));
                    }
                }
                if b.moves != 0 {
                    bonuses.push(format!("{:+} movement", b.moves / 2));
                }
                for id in &item.powers {
                    bonuses.push(
                        rules
                            .item_powers
                            .by_name(id)
                            .map_or(id.clone(), |p| p.name.clone()),
                    );
                }
                let mut lines = vec![format!("{} ({:?})", item.name, item.kind)];
                if !bonuses.is_empty() {
                    lines.push(bonuses.join(", "));
                }
                lines.push(format!("Worth {} gold.", item.cost));
                lines
            }
        }
    }
}
