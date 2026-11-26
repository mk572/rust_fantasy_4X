//! One list for choosing a city: the cities it may pick come in, the city picked (or none, on
//! cancel) goes out through a callback. The Summoning Circle, Move Fortress and Spell of Return
//! spells use it for the city they are cast on, and a summoning, when the wizard has several
//! summoning circles (`multiple_summoning_circles`), for the city the creature appears in.
//! Up and Down move, Enter or a click picks, Escape gives up.
//!
//! Tests: below. What the keys and a click decide is `decide`, which rows show is `window`; the
//! drawing is left to the screenshots.

use crate::input::Input;
use crate::modal::{keep, Modal};
use crate::ui::{self, GOLD};
use crate::view::{App, Screen, HUMAN};
use f4x_core::rules::exec::Target;
use f4x_def::rules::SpellId;
use macroquad::prelude::*;

use crate::spellbook::Targeting;
use crate::text::Args;

/// The rows in view at most.
const ROWS: usize = 9;

/// What is done with the answer: the city picked, or `None` when the player gave up.
pub(crate) type Answer = Box<dyn FnOnce(&mut App, Option<hecs::Entity>)>;

/// The cities to choose from, the row picked, and what is done with the answer.
pub(crate) struct CityPick {
    /// The question, already in words.
    prompt: String,
    cities: Vec<hecs::Entity>,
    row: usize,
    answer: Answer,
}

/// What a frame of the list decides.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Stay,
    Cancel,
    Pick,
}

/// The row picked after a frame, and what the frame decides: Up and Down move it, a click on a row
/// picks it, Enter picks the highlighted one, Escape (or no city to pick) gives up.
fn decide(row: usize, clicked: Option<usize>, cities: usize, input: &Input) -> (usize, Step) {
    let last = cities.saturating_sub(1);
    let mut pick = clicked.unwrap_or(row).min(last);
    if input.key_pressed(KeyCode::Up) {
        pick = pick.saturating_sub(1);
    }
    if input.key_pressed(KeyCode::Down) {
        pick = (pick + 1).min(last);
    }
    let step = if input.key_pressed(KeyCode::Escape) || cities == 0 {
        Step::Cancel
    } else if clicked.is_some() || input.key_pressed(KeyCode::Enter) {
        Step::Pick
    } else {
        Step::Stay
    };
    (pick, step)
}

/// The first row in view when `pick` is highlighted among `len` rows and `ROWS` fit: the highlight
/// stays in view and the list does not scroll past its end.
fn window(pick: usize, len: usize) -> usize {
    pick.saturating_sub(ROWS - 1).min(len.saturating_sub(ROWS))
}

impl Modal for CityPick {
    fn kind(&self) -> &'static str {
        "city_pick"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.city_pick_frame(*self, input))
    }
}

impl App {
    /// Ask the player for one of `cities`; `answer` gets it, or `None` on cancel. With no city to
    /// choose from `answer` runs at once with `None`.
    pub(crate) fn pick_city(
        &mut self,
        prompt: String,
        cities: Vec<hecs::Entity>,
        answer: impl FnOnce(&mut App, Option<hecs::Entity>) + 'static,
    ) {
        self.screen = Screen::Map;
        if cities.is_empty() {
            answer(self, None);
            return;
        }
        self.modals.open(CityPick {
            prompt,
            cities,
            row: 0,
            answer: Box::new(answer),
        });
    }

    /// The cities `spell` may be cast on by the human wizard when it is aimed at a city and the player
    /// chooses it in the side panel (`city_select`), or `None` when the spell is aimed in another way.
    /// Only the cities the game accepts are listed; an enemy city must have been explored.
    pub(crate) fn city_choices(&self, spell: SpellId) -> Option<Vec<hecs::Entity>> {
        use f4x_core::spell::special;
        use f4x_def::rules::SpellTarget;
        let def = self.world.content().rules.spells.get(spell);
        let summons_to_a_circle = self.world.spell_summons(spell)
            && self.world.has_several_circles(HUMAN)
            && !f4x_core::spell::hero::HeroSummon::Resurrection.is_in(def);
        let by_target = matches!(def.target, SpellTarget::OwnCity | SpellTarget::EnemyCity);
        let by_hook =
            def.has_hook(special::MOVE_BUILDING) || def.has_hook(special::SPELL_OF_RETURN);
        if !(summons_to_a_circle || by_target || by_hook) {
            return None;
        }
        let ward = f4x_core::spell::city::CityEffect::SpellWard.is_in(def);
        Some(
            self.world
                .economy()
                .cities()
                .into_iter()
                .filter(|&(c, _, at)| {
                    let mine = self.world.economy().city_owner(c) == Some(HUMAN);
                    (mine || self.world.is_explored(HUMAN, at))
                        && !(ward && self.world.fully_warded(c))
                        && self
                            .world
                            .check_target(HUMAN, spell, Some(Target::City(c)))
                            .is_ok()
                })
                .map(|(c, ..)| c)
                .collect(),
        )
    }

    /// The cities of the human wizard that hold a summoning circle.
    pub(crate) fn circle_cities(&self) -> Vec<hecs::Entity> {
        self.world
            .economy()
            .cities_of(HUMAN)
            .into_iter()
            .map(|c| c.0)
            .filter(|&c| self.world.circle_in(HUMAN, c).is_some())
            .collect()
    }

    /// Where the summoning circle of `city` stands.
    pub(crate) fn circle_position(&self, city: hecs::Entity) -> Option<f4x_core::map::Position> {
        self.world.circle_in(HUMAN, city)
    }

    /// Choose the city for `spell` in the side panel and cast it there once OK is clicked. A Spell
    /// Ward then asks for its realm.
    pub(crate) fn pick_city_for(&mut self, t: Targeting, cities: Vec<hecs::Entity>) {
        self.screen = Screen::Map;
        if cities.is_empty() {
            self.log.push(self.text("spell.no_valid_target"));
            return;
        }
        let term = self
            .names
            .spell_term(self.world.content().rules.spells.get(t.spell).num);
        let prompt = self.text_with("spell.choose_city", &Args::new().term("spell", term));
        let title = self.text("spell.casting_title");
        self.select_city(title, prompt, cities, move |app, city| match city {
            Some(city)
                if f4x_core::spell::city::CityEffect::SpellWard
                    .is_in(app.world.content().rules.spells.get(t.spell)) =>
            {
                app.modals.open(crate::view::Ward {
                    spell: t.spell,
                    city,
                    resolve: t.resolve,
                });
            }
            Some(city) => app.cast_on(t, Target::City(city)),
            None => app.log.push(app.text("log.cast_cancelled")),
        });
    }

    /// The list over the map.
    fn city_pick_frame(&mut self, pick: CityPick, input: &Input) -> Option<CityPick> {
        let CityPick {
            prompt,
            cities,
            row,
            answer,
        } = pick;
        let lines: Vec<String> = cities
            .iter()
            .map(|&c| self.world.city_name(c).unwrap_or_default())
            .collect();
        let row = row.min(lines.len().saturating_sub(1));
        let (w, h) = (screen_width(), screen_height());
        let step = 30.0;
        let shown = lines.len().min(ROWS);
        let first = window(row, lines.len());
        let top = h / 2.0 - 20.0;
        let clicked = (0..shown).find_map(|n| {
            let y = top + step * n as f32;
            ui::hit(
                Rect::new(w / 2.0 - 260.0, y - 20.0, 520.0, 28.0),
                true,
                input,
            )
            .clicked()
            .then_some(first + n)
        });
        let (pick_row, step_taken) = decide(row, clicked, lines.len(), input);
        // The frame draws the list as it was before the keys of this frame moved the highlight.
        let height = 110.0 + step * shown as f32;
        draw_rectangle(
            w / 2.0 - 280.0,
            h / 2.0 - 80.0,
            560.0,
            height,
            Color::new(0.08, 0.06, 0.04, 0.95),
        );
        draw_rectangle_lines(w / 2.0 - 280.0, h / 2.0 - 80.0, 560.0, height, 3.0, GOLD);
        crate::frame::text_centered(&prompt, w / 2.0, h / 2.0 - 45.0, 22.0, WHITE);
        for n in 0..shown {
            let color = if first + n == pick_row { GOLD } else { WHITE };
            crate::frame::text_centered(
                &lines[first + n],
                w / 2.0,
                top + step * n as f32,
                22.0,
                color,
            );
        }
        crate::frame::text_centered(
            &self.text("spell.global_keys"),
            w / 2.0,
            top + step * shown as f32 + 10.0,
            18.0,
            WHITE,
        );
        match step_taken {
            Step::Stay => Some(CityPick {
                prompt,
                cities,
                row: pick_row,
                answer,
            }),
            Step::Cancel => {
                answer(self, None);
                None
            }
            Step::Pick => {
                answer(self, Some(cities[pick_row]));
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_and_down_move_the_pick_and_stop_at_the_ends() {
        assert_eq!(
            decide(1, None, 3, &Input::key(KeyCode::Up)),
            (0, Step::Stay)
        );
        assert_eq!(
            decide(0, None, 3, &Input::key(KeyCode::Up)),
            (0, Step::Stay)
        );
        assert_eq!(
            decide(1, None, 3, &Input::key(KeyCode::Down)),
            (2, Step::Stay)
        );
        assert_eq!(
            decide(2, None, 3, &Input::key(KeyCode::Down)),
            (2, Step::Stay)
        );
    }

    #[test]
    fn enter_and_a_click_pick_and_escape_or_no_city_gives_up() {
        assert_eq!(
            decide(1, None, 3, &Input::key(KeyCode::Enter)),
            (1, Step::Pick)
        );
        assert_eq!(decide(0, Some(2), 3, &Input::default()), (2, Step::Pick));
        assert_eq!(
            decide(1, None, 3, &Input::key(KeyCode::Escape)),
            (1, Step::Cancel)
        );
        assert_eq!(decide(0, None, 0, &Input::default()), (0, Step::Cancel));
        assert_eq!(decide(1, None, 3, &Input::default()), (1, Step::Stay));
    }

    #[test]
    fn the_window_keeps_the_highlight_in_view_and_stops_at_the_end() {
        assert_eq!(window(0, 5), 0);
        assert_eq!(window(4, 5), 0);
        for len in ROWS..ROWS + 12 {
            for pick in 0..len {
                let first = window(pick, len);
                assert!(first <= pick && pick < first + ROWS, "{pick} of {len}");
                assert!(first + ROWS <= len, "{pick} of {len} scrolls past the end");
            }
        }
    }
}
