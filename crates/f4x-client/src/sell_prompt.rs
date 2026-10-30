//! Questions the city screen asks in the original's confirmation box: selling a building from
//! the city picture (task 117) and turning off one of the player's spells on the city (task 119).
//! Selling A click on a building asks "Do you wish
//! to sell back the ... for ... gold?" (`CityScr.c`, help topic 303); a building another one
//! needs is refused with a notice. Yes sends `Command::SellBuilding`.

use crate::font::Style;
use crate::input::Input;
use f4x_core::game::Command;
use f4x_core::rules::BuildingId;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::ui;
use crate::view::App;

/// What the box says.
#[derive(Debug, Clone)]
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
    /// "Do you wish to spend N Gold by purchasing a ...?", for the city's production.
    Buy {
        city: hecs::Entity,
        name: String,
        price: u32,
    },
    /// "Do you wish to turn off the ... spell?", for a spell of the player's on the city.
    TurnOff {
        city: hecs::Entity,
        spell: f4x_core::rules::SpellId,
        name: String,
    },
}

/// The two buttons of a question box and what answers it besides a click on them.
pub(crate) struct Buttons {
    right: (&'static str, usize),
    left: (&'static str, usize),
    right_key: KeyCode,
    left_key: KeyCode,
    /// Whether a click elsewhere answers (as the left button).
    elsewhere: bool,
    /// What Esc answers.
    escape: bool,
}

/// Yes (right) and No (left): `resource.lbx` 3 and 4.
const YES_NO: Buttons = Buttons {
    right: ("resource", 3),
    left: ("resource", 4),
    right_key: KeyCode::Y,
    left_key: KeyCode::N,
    elsewhere: true,
    escape: false,
};

/// `Raze_City_Prompt`: Raze (left, R) and the button that spares the city (right, N), from
/// `compix.lbx` 82 and 81. Esc spares it; a click elsewhere does nothing.
pub(crate) const RAZE_OR_SPARE: Buttons = Buttons {
    right: ("compix", 81),
    left: ("compix", 82),
    right_key: KeyCode::N,
    left_key: KeyCode::R,
    elsewhere: false,
    escape: true,
};

/// What the city screen and the outpost screen share: the question box over them and the
/// first row of the enchantment list.
#[derive(Debug, Clone, Default)]
pub(crate) struct CityPanel {
    pub(crate) sell: Option<SellPrompt>,
    /// The first enchantment shown on the list.
    pub(crate) spell_first: usize,
}

impl App {
    /// A click on a building of the city picture: the question, or the refusal.
    pub(crate) fn start_sale(
        &mut self,
        panel: &mut CityPanel,
        city: hecs::Entity,
        picture_index: i16,
    ) {
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
            _ if eco.sold_this_turn(city) => SellPrompt::Refused(self.text("sell.one_a_turn")),
            Some(other) => SellPrompt::Refused(
                self.text_with(
                    "sell.required",
                    &Args::new()
                        .text("building", &name)
                        .text("other", &rules.buildings.get(other).name),
                ),
            ),
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
        panel.sell = Some(prompt);
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
            let mut page = crate::city_screen::CityPage::new(city);
            self.start_sale(&mut page.panel, city, i as i16 + 3);
            self.open_page(crate::view::Screen::City(city), page);
        }
    }

    /// Draw the box and take the answer.
    pub(crate) fn draw_sell_prompt(&mut self, panel: &mut CityPanel, input: &Input) {
        let Some(prompt) = panel.sell.take() else {
            return;
        };
        let lay = Layout::new();
        let text = match &prompt {
            SellPrompt::Ask {
                name, gold, stops, ..
            } => {
                let args = Args::new().text("building", name).count("gold", *gold);
                let mut text = self.text_with("sell.ask", &args);
                if let Some(item) = stops {
                    let args = Args::new().text("item", item);
                    text += &self.text_with("sell.stops", &args);
                }
                text
            }
            SellPrompt::Refused(text) => text.clone(),
            SellPrompt::Buy { name, price, .. } => {
                let args = Args::new().count("price", *price).text("building", name);
                self.text_with("sell.buy", &args)
            }
            SellPrompt::TurnOff { name, .. } => {
                self.text_with("sell.turn_off", &Args::new().text("spell", name))
            }
        };
        match &prompt {
            SellPrompt::Ask { city, building, .. } => {
                match self.confirmation_box(&lay, &text, input) {
                    Some(true) => {
                        let events = self.game.apply(Command::SellBuilding {
                            city: *city,
                            building: *building,
                        });
                        self.note(events);
                    }
                    Some(false) => {}
                    None => panel.sell = Some(prompt),
                }
            }
            SellPrompt::Buy { city, .. } => match self.confirmation_box(&lay, &text, input) {
                Some(true) => {
                    let events = self.game.apply(Command::BuyProduction { city: *city });
                    self.note(events);
                }
                Some(false) => {}
                None => panel.sell = Some(prompt),
            },
            SellPrompt::TurnOff { city, spell, .. } => {
                match self.confirmation_box(&lay, &text, input) {
                    Some(true) => {
                        let events = self.game.apply(Command::TurnOffCitySpell {
                            player: crate::view::HUMAN,
                            city: *city,
                            spell: *spell,
                        });
                        self.note(events);
                    }
                    Some(false) => {}
                    None => panel.sell = Some(prompt),
                }
            }
            SellPrompt::Refused(_) => {
                if !self.warning_box(&lay, &text, input) {
                    panel.sell = Some(prompt);
                }
            }
        }
    }

    /// `Confirmation_Box`: the text, a Yes and a No (`resource.lbx` 0, 1, 3 and 4), 166 wide
    /// in the box font, the box in the middle of the screen. A right-click counts as a left
    /// one, a click elsewhere or Esc as No. `Some` once answered.
    pub(crate) fn confirmation_box(
        &mut self,
        lay: &Layout,
        text: &str,
        input: &Input,
    ) -> Option<bool> {
        self.question_box(lay, text, &YES_NO, input)
    }

    /// The box of `confirmation_box` with other buttons: the right one answers `Some(true)`,
    /// the left one `Some(false)`.
    pub(crate) fn question_box(
        &mut self,
        lay: &Layout,
        text: &str,
        buttons: &Buttons,
        input: &Input,
    ) -> Option<bool> {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let line = self.art.text_height(4) + 2.0;
        let lines: Vec<(f32, String)> = self
            .bm_wrap(4, text, 166.0)
            .into_iter()
            .map(|l| (0.0, l))
            .collect();
        let paragraph = line * lines.len() as f32;
        let (x, y) = (
            68.0,
            ((crate::resolution::DESIGN_H - (paragraph + 34.0)) / 2.0).floor(),
        );
        self.draw_pic_rows(lay, "resource", 0, x, y, y, y + paragraph + 12.0);
        self.draw_pic(lay, "resource", 1, 0, x, y + paragraph + 10.0);
        self.draw_box_text(view, &lines, x + 9.0, y + 9.0, line);
        let right_at = (x + 101.0, y + paragraph + 15.0);
        let left_at = (x + 18.0, y + paragraph + 15.0);
        let right = ui::hit(lay.rect((right_at.0, right_at.1, 33.0, 17.0)), true, input);
        let left = ui::hit(lay.rect((left_at.0, left_at.1, 33.0, 17.0)), true, input);
        let (lbx, entry) = buttons.right;
        self.draw_pic(
            lay,
            lbx,
            entry,
            usize::from(right.held()),
            right_at.0,
            right_at.1,
        );
        let (lbx, entry) = buttons.left;
        self.draw_pic(
            lay,
            lbx,
            entry,
            usize::from(left.held()),
            left_at.0,
            left_at.1,
        );
        // Either mouse button answers a question box.
        let right_click = input.mouse_pressed(MouseButton::Right);
        let clicked = input.mouse_pressed(MouseButton::Left) || right_click;
        if (right.clicked() || (right_click && right.hovered()))
            || input.key_pressed(buttons.right_key)
        {
            Some(true)
        } else if (left.clicked() || (right_click && left.hovered()))
            || input.key_pressed(buttons.left_key)
        {
            Some(false)
        } else if input.key_pressed(KeyCode::Escape) || (clicked && buttons.elsewhere) {
            Some(buttons.escape)
        } else {
            None
        }
    }

    /// `Warn0`: the red box (`resource.lbx` 38 and 39) with the text centred; any click or key
    /// closes it. Returns whether it was closed.
    pub(crate) fn warning_box(&mut self, lay: &Layout, text: &str, input: &Input) -> bool {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let line = self.art.text_height(4) + 2.0;
        let lines = self.bm_wrap(4, text, 166.0);
        let paragraph = line * lines.len() as f32;
        let (x, y) = (
            68.0,
            ((crate::resolution::DESIGN_H - (paragraph + 33.0)) / 2.0).floor(),
        );
        self.draw_pic_rows(lay, "resource", 38, x, y, y, y + paragraph + 12.0);
        self.draw_pic(lay, "resource", 39, 0, x, y + paragraph + 10.0);
        let shadow = Color::new(0.2, 0.02, 0.02, 1.0);
        let light = Color::new(0.95, 0.9, 0.85, 1.0);
        for (i, l) in lines.iter().enumerate() {
            let at = y + 10.0 + line * i as f32;
            let cx = x + 10.0 + 83.0;
            self.draw_styled(
                view,
                &Style::nearest(4, shadow).center(),
                l,
                cx + 1.0,
                at + 1.0,
            );
            self.draw_styled(view, &Style::nearest(4, light).center(), l, cx, at);
        }
        input.mouse_pressed(MouseButton::Left)
            || input.mouse_pressed(MouseButton::Right)
            || input.key_pressed(KeyCode::Enter)
            || input.key_pressed(KeyCode::Escape)
            || input.key_pressed(KeyCode::Space)
    }
}
