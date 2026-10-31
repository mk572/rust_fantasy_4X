//! Questions the city screen asks in the original's confirmation box: selling a building from
//! the city picture and turning off one of the player's spells on the city.
//! Selling A click on a building asks "Do you wish
//! to sell back the ... for ... gold?" (`CityScr.c`, help topic 303); a building another one
//! needs is refused with a notice. Yes sends `Command::SellBuilding`.
//!
//! Tests: below. What a click or key answers is `Buttons::answer`, what the answer sends is
//! `settle_sell`, which question a click on a building asks is `start_sale`; the drawing of the
//! boxes is left to the screenshots.

use crate::font::Style;
use crate::input::Input;
use f4x_core::game::Command;
use f4x_core::rules::BuildingId;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::ui::{self, Hit};
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

impl Buttons {
    /// What the frame answers, given the mouse on the right and the left button: the right one
    /// answers `Some(true)`, the left one `Some(false)`. Either mouse button answers on a
    /// button, a key answers as its button, Esc and (if `elsewhere`) a click elsewhere answer
    /// `escape`.
    fn answer(&self, right: Hit, left: Hit, input: &Input) -> Option<bool> {
        let right_click = input.mouse_pressed(MouseButton::Right);
        let clicked = input.mouse_pressed(MouseButton::Left) || right_click;
        if right.clicked() || (right_click && right.hovered()) || input.key_pressed(self.right_key)
        {
            Some(true)
        } else if left.clicked()
            || (right_click && left.hovered())
            || input.key_pressed(self.left_key)
        {
            Some(false)
        } else if input.key_pressed(KeyCode::Escape) || (clicked && self.elsewhere) {
            Some(self.escape)
        } else {
            None
        }
    }
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
        let answer = match &prompt {
            SellPrompt::Refused(_) => self.warning_box(&lay, &text, input).then_some(false),
            _ => self.confirmation_box(&lay, &text, input),
        };
        panel.sell = self.settle_sell(prompt, answer);
    }

    /// What an answer does to the question: Yes sends the command the question was about, No
    /// and a closed notice send nothing. The question stays (`Some`) until it is answered.
    pub(crate) fn settle_sell(
        &mut self,
        prompt: SellPrompt,
        answer: Option<bool>,
    ) -> Option<SellPrompt> {
        let Some(yes) = answer else {
            return Some(prompt);
        };
        let command = match &prompt {
            SellPrompt::Ask { city, building, .. } => Some(Command::SellBuilding {
                city: *city,
                building: *building,
            }),
            SellPrompt::Buy { city, .. } => Some(Command::BuyProduction { city: *city }),
            SellPrompt::TurnOff { city, spell, .. } => Some(Command::TurnOffCitySpell {
                player: crate::view::HUMAN,
                city: *city,
                spell: *spell,
            }),
            SellPrompt::Refused(_) => None,
        };
        if let Some(command) = command.filter(|_| yes) {
            let events = self.game.apply(command);
            self.note(events);
        }
        None
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
        buttons.answer(right, left, input)
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
        input.dismisses()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::HUMAN;
    use f4x_core::testdir::TestResult;

    fn input_at(button: MouseButton, x: f32, y: f32) -> Input {
        Input::click(button, x, y)
    }

    #[test]
    fn a_yes_no_box_is_answered_by_its_buttons_and_keys() {
        let none = Input::default();
        assert_eq!(YES_NO.answer(Hit::None, Hit::None, &none), None);
        assert_eq!(
            YES_NO.answer(Hit::Hover, Hit::None, &none),
            None,
            "a hover answers nothing"
        );
        assert_eq!(
            YES_NO.answer(
                Hit::Clicked,
                Hit::None,
                &input_at(MouseButton::Left, 0.0, 0.0)
            ),
            Some(true)
        );
        assert_eq!(
            YES_NO.answer(
                Hit::None,
                Hit::Clicked,
                &input_at(MouseButton::Left, 0.0, 0.0)
            ),
            Some(false)
        );
        assert_eq!(
            YES_NO.answer(Hit::None, Hit::None, &Input::key(KeyCode::Y)),
            Some(true)
        );
        assert_eq!(
            YES_NO.answer(Hit::None, Hit::None, &Input::key(KeyCode::N)),
            Some(false)
        );
    }

    #[test]
    fn a_right_click_on_a_button_answers_as_a_left_click() {
        let right = input_at(MouseButton::Right, 0.0, 0.0);
        assert_eq!(YES_NO.answer(Hit::Hover, Hit::None, &right), Some(true));
        assert_eq!(YES_NO.answer(Hit::None, Hit::Hover, &right), Some(false));
    }

    #[test]
    fn a_click_elsewhere_or_esc_answers_no_in_a_yes_no_box() {
        let elsewhere = input_at(MouseButton::Left, 0.0, 0.0);
        assert_eq!(YES_NO.answer(Hit::None, Hit::None, &elsewhere), Some(false));
        let esc = Input::key(KeyCode::Escape);
        assert_eq!(YES_NO.answer(Hit::None, Hit::None, &esc), Some(false));
        let other_key = Input::key(KeyCode::A);
        assert_eq!(YES_NO.answer(Hit::None, Hit::None, &other_key), None);
    }

    #[test]
    fn the_raze_box_ignores_a_click_elsewhere_and_esc_spares_the_city() {
        let elsewhere = input_at(MouseButton::Left, 0.0, 0.0);
        assert_eq!(RAZE_OR_SPARE.answer(Hit::None, Hit::None, &elsewhere), None);
        let esc = Input::key(KeyCode::Escape);
        assert_eq!(RAZE_OR_SPARE.answer(Hit::None, Hit::None, &esc), Some(true));
        assert_eq!(
            RAZE_OR_SPARE.answer(Hit::None, Hit::None, &Input::key(KeyCode::R)),
            Some(false),
            "R razes"
        );
        assert_eq!(
            RAZE_OR_SPARE.answer(Hit::None, Hit::None, &Input::key(KeyCode::N)),
            Some(true),
            "N spares"
        );
    }

    /// The human's first city, and a building of it that nothing built needs.
    fn city_and_free_building(app: &App) -> Result<(hecs::Entity, BuildingId), &'static str> {
        let (city, _, _) = *app
            .game
            .economy()
            .cities()
            .iter()
            .find(|c| c.1.owner == HUMAN)
            .ok_or("the human has no city")?;
        let eco = app.game.economy();
        let building = app
            .game
            .city_buildings(city)
            .into_iter()
            .find(|&b| eco.first_dependant(city, b).is_none() && eco.sale_price(city, b).is_ok())
            .ok_or("no building can be sold")?;
        Ok((city, building))
    }

    fn gold(app: &App) -> Result<i32, &'static str> {
        Ok(app.game.player(HUMAN).ok_or("no human")?.treasury().gold)
    }

    fn ask(app: &App, city: hecs::Entity, building: BuildingId) -> SellPrompt {
        let name = app
            .game
            .content()
            .rules
            .buildings
            .get(building)
            .name
            .clone();
        SellPrompt::Ask {
            city,
            building,
            name,
            gold: 0,
            stops: None,
        }
    }

    #[test]
    fn yes_sells_the_building_and_pays_its_price() -> TestResult {
        let mut app = App::for_test();
        let (city, building) = city_and_free_building(&app)?;
        let price = app
            .game
            .economy()
            .sale_price(city, building)
            .map_err(|e| format!("{e:?}"))?;
        let before = gold(&app)?;
        let left = app.settle_sell(ask(&app, city, building), Some(true));
        assert!(left.is_none(), "an answered question closes");
        assert!(!app.game.city_buildings(city).contains(&building));
        assert_eq!(gold(&app)?, before + price as i32);
        Ok(())
    }

    #[test]
    fn no_and_no_answer_sell_nothing() -> TestResult {
        let mut app = App::for_test();
        let (city, building) = city_and_free_building(&app)?;
        let before = gold(&app)?;
        assert!(app
            .settle_sell(ask(&app, city, building), Some(false))
            .is_none());
        assert!(
            app.settle_sell(ask(&app, city, building), None).is_some(),
            "no answer keeps the question"
        );
        assert!(app.game.city_buildings(city).contains(&building));
        assert_eq!(gold(&app)?, before);
        Ok(())
    }

    #[test]
    fn a_notice_closes_when_answered_and_sends_nothing() -> TestResult {
        let mut app = App::for_test();
        let (city, _) = city_and_free_building(&app)?;
        let before = app.game.city_buildings(city);
        let notice = SellPrompt::Refused("no".into());
        assert!(app.settle_sell(notice.clone(), None).is_some());
        assert!(app.settle_sell(notice, Some(false)).is_none());
        assert_eq!(app.game.city_buildings(city), before);
        Ok(())
    }

    /// The human's first city with a smithy and a marketplace (which needs the smithy).
    fn city_with_smithy_and_market(
        app: &mut App,
    ) -> Result<(hecs::Entity, BuildingId, BuildingId), &'static str> {
        let (city, _, _) = *app
            .game
            .economy()
            .cities()
            .iter()
            .find(|c| c.1.owner == HUMAN)
            .ok_or("the human has no city")?;
        let buildings = &app.game.content().rules.buildings;
        let smithy = buildings.id_of("smithy").ok_or("no smithy")?;
        let market = buildings.id_of("marketplace").ok_or("no marketplace")?;
        app.game.scenario_set_building(city, smithy, true);
        app.game.scenario_set_building(city, market, true);
        Ok((city, smithy, market))
    }

    /// The index a click on the picture sends for this building.
    fn picture_index(app: &App, building: BuildingId) -> Result<i16, &'static str> {
        let buildings = &app.game.content().rules.buildings;
        let at = buildings
            .defs()
            .iter()
            .position(|d| buildings.id_of(&d.id) == Some(building))
            .ok_or("the building is not in the table")?;
        Ok(at as i16 + 3)
    }

    #[test]
    fn a_click_on_a_building_asks_and_a_second_sale_the_same_turn_is_refused() -> TestResult {
        let mut app = App::for_test();
        let (city, smithy, market) = city_with_smithy_and_market(&mut app)?;
        let mut panel = CityPanel::default();
        app.start_sale(&mut panel, city, picture_index(&app, market)?);
        let Some(SellPrompt::Ask { building, gold, .. }) = panel.sell.take() else {
            return Err("the marketplace is not offered for sale".into());
        };
        assert_eq!(building, market);
        assert!(gold > 0);
        app.settle_sell(
            SellPrompt::Ask {
                city,
                building,
                name: String::new(),
                gold,
                stops: None,
            },
            Some(true),
        );
        app.start_sale(&mut panel, city, picture_index(&app, smithy)?);
        let Some(SellPrompt::Refused(text)) = panel.sell else {
            return Err("a second sale in the turn is not refused".into());
        };
        assert_eq!(text, app.text("sell.one_a_turn"));
        Ok(())
    }

    #[test]
    fn a_building_another_one_needs_is_refused_with_a_notice_naming_it() -> TestResult {
        let mut app = App::for_test();
        let (city, smithy, market) = city_with_smithy_and_market(&mut app)?;
        let mut panel = CityPanel::default();
        app.start_sale(&mut panel, city, picture_index(&app, smithy)?);
        let Some(SellPrompt::Refused(text)) = panel.sell else {
            return Err("the smithy is not refused".into());
        };
        let name = &app.game.content().rules.buildings.get(market).name;
        assert!(
            text.contains(name.as_str()),
            "{text:?} does not name {name}"
        );
        Ok(())
    }

    #[test]
    fn a_click_off_the_table_asks_nothing() -> TestResult {
        let mut app = App::for_test();
        let (city, _, _) = city_with_smithy_and_market(&mut app)?;
        let mut panel = CityPanel::default();
        app.start_sale(&mut panel, city, 0);
        assert!(panel.sell.is_none());
        Ok(())
    }
}
