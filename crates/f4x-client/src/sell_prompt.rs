//! Selling a building from the city picture (task 117). A click on a building asks "Do you wish
//! to sell back the ... for ... gold?" (`CityScr.c`, help topic 303); a building another one
//! needs is refused with a notice. Yes sends `Command::SellBuilding`.

use f4x_core::rules::BuildingId;
use f4x_core::Command;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::view::App;

/// What the box says.
pub(crate) enum SellPrompt {
    /// The question: the building, its name, the gold it brings, and the name of the item whose
    /// production the sale ends, if any.
    Ask {
        city: hecs::Entity,
        building: BuildingId,
        name: String,
        gold: u32,
        stops: Option<String>,
    },
    /// The sale is refused, with the reason.
    Refused(String),
}

impl App {
    /// A click on a building of the city picture: the question, or the refusal.
    pub(crate) fn start_sale(&mut self, city: hecs::Entity, picture_index: i16) {
        let rules = &self.game.content().rules;
        let Some(def) = usize::try_from(picture_index - 3)
            .ok()
            .and_then(|i| rules.buildings.defs().get(i))
        else {
            return;
        };
        let Some(building) = rules.buildings.id_of(&def.id) else {
            return;
        };
        let name = def.name.clone();
        let eco = self.game.economy();
        let prompt = match eco.first_dependant(city, building) {
            _ if eco.sold_this_turn(city) => {
                SellPrompt::Refused("You can only sell back one building each turn.".into())
            }
            Some(other) => SellPrompt::Refused(format!(
                "You cannot sell back the {name} because it is required by the {}.",
                rules.buildings.get(other).name
            )),
            None => {
                let Ok(gold) = eco.sale_price(city, building) else {
                    return;
                };
                let stops = eco
                    .production_needs(city, building)
                    .then(|| eco.city_production(city).item)
                    .flatten()
                    .map(|item| self.item_name(item));
                SellPrompt::Ask {
                    city,
                    building,
                    name,
                    gold,
                    stops,
                }
            }
        };
        self.sell_prompt = Some(prompt);
    }

    /// For screenshots: the question for the first building of the open city.
    pub fn open_test_sale(&mut self) {
        let crate::view::Screen::City(city) = self.screen else {
            return;
        };
        let rules = &self.game.content().rules;
        let first = self.game.city_buildings(city).into_iter().find_map(|b| {
            let name = &rules.buildings.get(b).id;
            rules.buildings.defs().iter().position(|d| &d.id == name)
        });
        if let Some(i) = first {
            self.start_sale(city, i as i16 + 3);
        }
    }

    /// Draw the box and take the answer.
    pub(crate) fn draw_sell_prompt(&mut self) {
        let Some(prompt) = self.sell_prompt.take() else {
            return;
        };
        let lay = Layout::new();
        let text = match &prompt {
            SellPrompt::Ask {
                name, gold, stops, ..
            } => {
                let mut text = format!("Do you wish to sell back the {name} for {gold} gold?");
                if let Some(item) = stops {
                    text += &format!(" This will cease production of your {item}.");
                }
                text
            }
            SellPrompt::Refused(text) => text.clone(),
        };
        match &prompt {
            SellPrompt::Ask { city, building, .. } => match self.confirmation_box(&lay, &text) {
                Some(true) => {
                    let events = self.game.apply(Command::SellBuilding {
                        city: *city,
                        building: *building,
                    });
                    self.note(events);
                }
                Some(false) => {}
                None => self.sell_prompt = Some(prompt),
            },
            SellPrompt::Refused(_) => {
                if !self.warning_box(&lay, &text) {
                    self.sell_prompt = Some(prompt);
                }
            }
        }
    }

    /// `Confirmation_Box`: the text, a Yes and a No (`resource.lbx` 0, 1, 3 and 4), 166 wide
    /// in the box font, the box in the middle of the screen. A right-click counts as a left
    /// one, a click elsewhere or Esc as No. `Some` once answered.
    pub(crate) fn confirmation_box(&mut self, lay: &Layout, text: &str) -> Option<bool> {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let line = self.art.text_height(4) + 2.0;
        let lines: Vec<(f32, String)> = self
            .bm_wrap(4, text, 166.0)
            .into_iter()
            .map(|l| (0.0, l))
            .collect();
        let paragraph = line * lines.len() as f32;
        let (x, y) = (68.0, ((200.0 - (paragraph + 34.0)) / 2.0).floor());
        self.draw_pic_rows(lay, "resource", 0, x, y, y, y + paragraph + 12.0);
        self.draw_pic(lay, "resource", 1, 0, x, y + paragraph + 10.0);
        self.draw_box_text(view, &lines, x + 9.0, y + 9.0, line);
        let mouse = Vec2::from(mouse_position());
        let down = is_mouse_button_down(MouseButton::Left);
        let yes_at = (x + 101.0, y + paragraph + 15.0);
        let no_at = (x + 18.0, y + paragraph + 15.0);
        let yes = lay.rect((yes_at.0, yes_at.1, 33.0, 17.0));
        let no = lay.rect((no_at.0, no_at.1, 33.0, 17.0));
        self.draw_pic(
            lay,
            "resource",
            3,
            usize::from(down && yes.contains(mouse)),
            yes_at.0,
            yes_at.1,
        );
        self.draw_pic(
            lay,
            "resource",
            4,
            usize::from(down && no.contains(mouse)),
            no_at.0,
            no_at.1,
        );
        let clicked = is_mouse_button_pressed(MouseButton::Left)
            || is_mouse_button_pressed(MouseButton::Right);
        if (clicked && yes.contains(mouse)) || is_key_pressed(KeyCode::Y) {
            Some(true)
        } else if clicked || is_key_pressed(KeyCode::N) || is_key_pressed(KeyCode::Escape) {
            Some(false)
        } else {
            None
        }
    }

    /// `Warn0`: the red box (`resource.lbx` 38 and 39) with the text centred; any click or key
    /// closes it. Returns whether it was closed.
    pub(crate) fn warning_box(&mut self, lay: &Layout, text: &str) -> bool {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let line = self.art.text_height(4) + 2.0;
        let lines = self.bm_wrap(4, text, 166.0);
        let paragraph = line * lines.len() as f32;
        let (x, y) = (68.0, ((200.0 - (paragraph + 33.0)) / 2.0).floor());
        self.draw_pic_rows(lay, "resource", 38, x, y, y, y + paragraph + 12.0);
        self.draw_pic(lay, "resource", 39, 0, x, y + paragraph + 10.0);
        let shadow = Color::new(0.2, 0.02, 0.02, 1.0);
        let light = Color::new(0.95, 0.9, 0.85, 1.0);
        for (i, l) in lines.iter().enumerate() {
            let at = y + 10.0 + line * i as f32;
            let cx = x + 10.0 + 83.0;
            self.bm_text(view, 4, shadow, l, cx + 1.0, at + 1.0, Align::Center);
            self.bm_text(view, 4, light, l, cx, at, Align::Center);
        }
        is_mouse_button_pressed(MouseButton::Left)
            || is_mouse_button_pressed(MouseButton::Right)
            || is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::Space)
    }
}
