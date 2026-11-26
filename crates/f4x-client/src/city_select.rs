//! The city picker: a panel in the side column, in the look of the original's spell casting panel
//! (`Spell_Casting_Screen`, `SCastScr.c`: the background, the title, the prompt and the buttons),
//! with a list of the cities that may be chosen, in alphabetical order, which the original lacks.
//!
//! Whoever opens it gives the cities that are valid for what the player is doing, and gets the one
//! chosen once OK is clicked (or `None` on Cancel or Escape). A click on a city in the list selects
//! it and centres the map on it; a click on a city on the map selects it and scrolls the list to
//! it. The right mouse button on a city on the map still opens the city. The wheel and the arrows
//! scroll the list.
//!
//! Tests: below. The drawing is left to the screenshots.

use crate::action_button_art::Action;
use crate::font::Style;
use crate::frame::Layout;
use crate::input::Input;
use crate::scroll::{self, Window};
use crate::ui;
use crate::view::{App, HUMAN};
use macroquad::prelude::*;

/// What is done with the answer: the city picked, or `None` when the player gave up.
pub(crate) type Answer = Box<dyn FnOnce(&mut App, Option<hecs::Entity>)>;

/// Where the list is in the panel (in the original's 320 by 200 pixels), its rows and their height.
const LIST_X: f32 = 247.0;
const LIST_Y: f32 = 113.0;
const LIST_W: f32 = 58.0;
const ROW_H: f32 = 9.0;
const ROWS: usize = 6;
/// The scroll arrows, at the right of the list.
const ARROW_X: f32 = 306.0;
/// The prompt starts here and is wrapped to this width.
const PROMPT_AT: (f32, f32) = (249.0, 90.0);
const PROMPT_W: f32 = 60.0;
/// The buttons: Cancel on the left, OK on the right.
const CANCEL_AT: (f32, f32) = (246.0, 181.0);
const OK_AT: (f32, f32) = (280.0, 181.0);
/// The most characters of a name that fit a row.
const NAME_CHARS: usize = 11;

/// The picker while it is open.
pub(crate) struct CitySelect {
    title: String,
    prompt: String,
    /// The cities that may be chosen, by name.
    cities: Vec<hecs::Entity>,
    names: Vec<String>,
    selected: Option<usize>,
    /// The first row of the list in view.
    first: usize,
    answer: Option<Answer>,
}

/// The first row in view so that `row` is in view with the list moved as little as it can be.
fn scroll_to(row: usize, first: usize, total: usize) -> usize {
    let first = if row < first {
        row
    } else if row >= first + ROWS {
        row + 1 - ROWS
    } else {
        first
    };
    Window::new(first, ROWS, total).first
}

/// What a name looks like in a row of the list.
fn shown(name: &str) -> String {
    name.chars().take(NAME_CHARS).collect()
}

impl App {
    /// Ask the player for one of `cities` in the side panel: `title` and `prompt` say what for, and
    /// `answer` gets the city once OK is clicked, or `None` on Cancel. The list is alphabetical. With no
    /// city to choose from `answer` runs at once with `None`.
    pub(crate) fn select_city(
        &mut self,
        title: String,
        prompt: String,
        cities: Vec<hecs::Entity>,
        answer: impl FnOnce(&mut App, Option<hecs::Entity>) + 'static,
    ) {
        if cities.is_empty() {
            answer(self, None);
            return;
        }
        let mut named: Vec<(String, hecs::Entity)> = cities
            .into_iter()
            .map(|c| (self.world.city_name(c).unwrap_or_default(), c))
            .collect();
        named.sort_by_cached_key(|(name, c)| {
            (name.to_lowercase(), self.world.units().serial_of(*c))
        });
        self.selection.clear();
        self.targeting = None;
        self.city_select = Some(CitySelect {
            title,
            prompt,
            names: named.iter().map(|(n, _)| n.clone()).collect(),
            cities: named.into_iter().map(|(_, c)| c).collect(),
            selected: None,
            first: 0,
            answer: Some(Box::new(answer)),
        });
    }

    /// For the screenshot check: open the picker with every city of the game on the list.
    pub fn open_test_city_select(&mut self) {
        let cities: Vec<hecs::Entity> = self
            .world
            .economy()
            .cities()
            .into_iter()
            .map(|c| c.0)
            .collect();
        let title = self.text("spell.casting_title");
        let prompt = self.text("spell.choose_a_city");
        self.select_city(title, prompt, cities, |_, _| {});
    }

    /// Where a city stands.
    fn city_square(&self, city: hecs::Entity) -> Option<f4x_core::map::Position> {
        self.world
            .economy()
            .cities()
            .into_iter()
            .find(|c| c.0 == city)
            .map(|c| c.2)
    }

    /// A click on the map at `pos` while the picker is open: a city in the list is selected and the list
    /// scrolled to it.
    pub(crate) fn city_select_click(&mut self, pos: f4x_core::map::Position) {
        let city = self.world.economy().city_at(pos);
        let Some(sel) = self.city_select.as_mut() else {
            return;
        };
        match city.and_then(|c| sel.cities.iter().position(|&x| x == c)) {
            Some(row) => {
                sel.selected = Some(row);
                sel.first = scroll_to(row, sel.first, sel.cities.len());
            }
            None => self.log.push(self.text("spell.no_valid_target")),
        }
    }

    /// The right mouse button on the map at `pos` while the picker is open: a city that has been
    /// explored is opened, as it is without the picker. Whether the picker took the click.
    pub(crate) fn city_select_open(&mut self, pos: f4x_core::map::Position) -> bool {
        if self.city_select.is_none() {
            return false;
        }
        if let Some(city) = self.world.economy().city_at(pos) {
            if self.world.economy().city_owner(city) == Some(HUMAN) {
                self.selection.clear();
                self.open_city_or_outpost(city);
            } else if self.world.is_explored(HUMAN, pos) {
                self.open_enemy_city(city);
            }
        }
        true
    }

    /// Leave the picker without an answer.
    pub(crate) fn city_select_cancel(&mut self) {
        if let Some(mut sel) = self.city_select.take() {
            if let Some(answer) = sel.answer.take() {
                answer(self, None);
            }
        }
    }

    /// The panel over the side column, and what the player does in it.
    pub(crate) fn city_select_bar(&mut self, input: &Input) {
        let Some(mut sel) = self.city_select.take() else {
            return;
        };
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        // The panel hides Next Turn, then the buttons sit on the border at the bottom.
        let art = self.draw_pic(&lay, "main", 44, 0, 242.0, 173.0).is_some()
            && self.draw_pic(&lay, "main", 40, 0, 240.0, 76.0).is_some();
        if art {
            self.draw_pic(&lay, "main", 47, 0, 240.0, 173.0);
        } else {
            draw_rectangle(
                lay.rect((240.0, 76.0, 80.0, 124.0)).x,
                lay.rect((240.0, 76.0, 80.0, 124.0)).y,
                80.0 * lay.scale,
                124.0 * lay.scale,
                Color::new(0.08, 0.06, 0.04, 1.0),
            );
        }
        self.draw_styled(
            view,
            &Style::survey_white(4).center(),
            &sel.title,
            280.0,
            80.0,
        );
        for (i, line) in self.bm_wrap(1, &sel.prompt, PROMPT_W).iter().enumerate() {
            self.draw_styled(
                view,
                &Style::survey_ramp(),
                line,
                PROMPT_AT.0,
                PROMPT_AT.1 + i as f32 * ROW_H,
            );
        }

        // The list: the wheel and the arrows scroll it, a click selects a city and centres the map on it.
        let total = sel.cities.len();
        let mut window = Window::new(sel.first, ROWS, total);
        window = window.moved(scroll::step(input.mouse_wheel().1, false, false));
        if window.scrolls() {
            if window.can_up() && self.arrow(&lay, ("spellscr", 43), (ARROW_X, LIST_Y), input) {
                window = window.moved(-1);
            }
            let down_y = LIST_Y + ROW_H * ROWS as f32 - 9.0;
            if window.can_down() && self.arrow(&lay, ("spellscr", 44), (ARROW_X, down_y), input) {
                window = window.moved(1);
            }
        }
        sel.first = window.first;
        let mut centre = None;
        for (n, row) in window.rows().enumerate() {
            let y = LIST_Y + ROW_H * n as f32;
            let rect = lay.rect((LIST_X - 1.0, y, LIST_W + 2.0, ROW_H));
            if sel.selected == Some(row) {
                draw_rectangle(
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h,
                    Color::new(1.0, 0.85, 0.3, 0.35),
                );
            } else if ui::hit(rect, true, input) == ui::Hit::Hover {
                draw_rectangle(
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h,
                    Color::new(1.0, 1.0, 1.0, 0.12),
                );
            }
            let style = if sel.selected == Some(row) {
                Style::survey_white(1)
            } else {
                Style::survey_ramp()
            };
            self.draw_styled(view, &style, &shown(&sel.names[row]), LIST_X, y);
            if ui::hit(rect, true, input).clicked() {
                sel.selected = Some(row);
                centre = Some(sel.cities[row]);
            }
        }
        if let Some(position) = centre.and_then(|c| self.city_square(c)) {
            self.center_on(position);
        }

        let can = sel.selected.is_some();
        let ok = self
            .draw_action_button(&lay, Action::Ok, OK_AT, !can, input)
            .clicked()
            || (can && input.key_pressed(KeyCode::Enter));
        let cancel = self
            .draw_action_button(&lay, Action::Cancel, CANCEL_AT, false, input)
            .clicked()
            || input.key_pressed(KeyCode::Escape);
        let answer = if ok {
            sel.selected.map(|row| Some(sel.cities[row]))
        } else if cancel {
            Some(None)
        } else {
            None
        };
        match answer {
            Some(city) => {
                if let Some(answer) = sel.answer.take() {
                    answer(self, city);
                }
            }
            None => self.city_select = Some(sel),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_def::testdir::TestResult;

    #[test]
    fn the_list_scrolls_the_least_that_brings_a_row_into_view() {
        assert_eq!(scroll_to(0, 0, 20), 0);
        assert_eq!(scroll_to(ROWS - 1, 0, 20), 0, "already in view");
        assert_eq!(scroll_to(ROWS, 0, 20), 1);
        assert_eq!(scroll_to(3, 10, 20), 3, "above the window");
        assert_eq!(scroll_to(19, 0, 20), 20 - ROWS, "never past the end");
        assert_eq!(scroll_to(2, 0, 3), 0, "a list that fits does not scroll");
    }

    #[test]
    fn a_name_is_cut_to_the_row() {
        assert_eq!(shown("Ab"), "Ab");
        assert_eq!(shown("Abcdefghijklmnop").chars().count(), NAME_CHARS);
    }

    /// An app with the picker open on the cities of the human wizard and the answer put where the
    /// test can see it.
    type Answers = std::rc::Rc<std::cell::RefCell<Vec<Option<hecs::Entity>>>>;

    fn opened(app: &mut App) -> (Vec<hecs::Entity>, Answers) {
        let cities: Vec<hecs::Entity> = app
            .world
            .economy()
            .cities_of(HUMAN)
            .into_iter()
            .map(|c| c.0)
            .collect();
        let got = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = got.clone();
        app.select_city("T".into(), "P".into(), cities.clone(), move |_, c| {
            seen.borrow_mut().push(c)
        });
        (cities, got)
    }

    #[test]
    fn the_cities_are_listed_alphabetically() -> TestResult {
        let mut app = App::for_test();
        opened(&mut app);
        let sel = app.city_select.as_ref().ok_or("the picker did not open")?;
        let mut sorted: Vec<String> = sel.names.iter().map(|n| n.to_lowercase()).collect();
        let before = sorted.clone();
        sorted.sort();
        assert_eq!(before, sorted);
        Ok(())
    }

    #[test]
    fn a_click_on_a_city_on_the_map_selects_it_and_a_click_elsewhere_changes_nothing() -> TestResult
    {
        let mut app = App::for_test();
        let (cities, got) = opened(&mut app);
        let city = *cities.first().ok_or("no city")?;
        let at = app.city_square(city).ok_or("no square")?;
        app.city_select_click(at);
        let sel = app.city_select.as_ref().ok_or("closed")?;
        assert_eq!(sel.selected.map(|r| sel.cities[r]), Some(city));
        // Nothing is done until OK.
        assert!(got.borrow().is_empty());
        let empty = f4x_core::map::Position { x: 0, y: 0, ..at };
        app.city_select_click(empty);
        let sel = app.city_select.as_ref().ok_or("closed")?;
        assert_eq!(sel.selected.map(|r| sel.cities[r]), Some(city));
        Ok(())
    }

    #[test]
    fn cancel_answers_none_and_closes_the_picker() -> TestResult {
        let mut app = App::for_test();
        let (_, got) = opened(&mut app);
        app.city_select_cancel();
        assert!(app.city_select.is_none());
        assert_eq!(got.borrow().as_slice(), [None]);
        app.city_select_cancel();
        assert_eq!(got.borrow().len(), 1, "nothing to cancel the second time");
        Ok(())
    }

    #[test]
    fn with_no_city_to_choose_the_answer_is_none_at_once() {
        let mut app = App::for_test();
        let got = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = got.clone();
        app.select_city("T".into(), "P".into(), Vec::new(), move |_, c| {
            seen.borrow_mut().push(c)
        });
        assert!(app.city_select.is_none());
        assert_eq!(got.borrow().as_slice(), [None]);
    }
}
