//! Resurrection: the dead hero to raise is chosen from a list before the spell is cast
//! (`Cast_Resurrection`), like a rival's global enchantment is for Disjunction. Up and Down
//! move, Enter or a click casts it (the spell's target, aimed at the hero), Escape gives up.
//!
//! Tests: below. What the keys and a click pick is `decide`, the command the pick sends is
//! `raise_command`; the drawing is left to the screenshots.

use crate::input::Input;
use f4x_core::game::Command;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::modal::{keep, Modal};
use crate::ui::{self, GOLD};
use crate::view::{App, Screen, HUMAN};
use f4x_core::rules::exec::Target;
use f4x_core::rules::SpellId;

/// The dead hero to raise, the row picked at `row`.
pub(crate) struct Resurrect {
    spell: SpellId,
    resolve: bool,
    row: usize,
}

/// What a frame of the list decides.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Stay,
    Cancel,
    Raise,
}

/// The row picked after a frame, and what the frame decides: Up and Down move it, a click on a
/// row picks and raises it, Enter raises the picked one, Escape (or no one to raise) gives up.
fn decide(row: usize, clicked: Option<usize>, dead: usize, input: &Input) -> (usize, Step) {
    let last = dead.saturating_sub(1);
    let mut pick = clicked.unwrap_or(row).min(last);
    if input.key_pressed(KeyCode::Up) {
        pick = pick.saturating_sub(1);
    }
    if input.key_pressed(KeyCode::Down) {
        pick = (pick + 1).min(last);
    }
    let step = if input.key_pressed(KeyCode::Escape) || dead == 0 {
        Step::Cancel
    } else if clicked.is_some() || input.key_pressed(KeyCode::Enter) {
        Step::Raise
    } else {
        Step::Stay
    };
    (pick, step)
}

/// The command that raises this hero: the answer to a cast that waits for its target, or a
/// new cast aimed at the hero.
fn raise_command(spell: SpellId, resolve: bool, hero: usize) -> Command {
    let target = Target::Hero(hero);
    if resolve {
        Command::ResolveCast {
            player: HUMAN,
            target,
        }
    } else {
        Command::CastSpell {
            player: HUMAN,
            spell,
            target: Some(target),
        }
    }
}

impl Modal for Resurrect {
    fn kind(&self) -> &'static str {
        "resurrect"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.resurrect_prompt_frame(*self, input))
    }
}

impl App {
    /// Start Resurrection: open the list of the dead heroes, or say there are none.
    pub(crate) fn open_resurrect(&mut self, spell: SpellId, resolve: bool) {
        self.screen = Screen::Map;
        if self.game.units().raisable_heroes().is_empty() {
            self.log.push(self.text("spell.no_dead"));
        } else {
            self.modals.open(Resurrect {
                spell,
                resolve,
                row: 0,
            });
        }
    }

    /// The list of the dead heroes over the map, the row picked at `row`.
    pub(crate) fn resurrect_prompt_frame(
        &mut self,
        resurrect: Resurrect,
        input: &Input,
    ) -> Option<Resurrect> {
        let Resurrect {
            spell,
            resolve,
            row,
        } = resurrect;
        let dead = self.game.units().raisable_heroes();
        let lines: Vec<String> = dead
            .iter()
            .map(|&kind| self.game.units().hero_type_name(kind).unwrap_or_default())
            .collect();
        let row = row.min(lines.len().saturating_sub(1));
        let (w, h) = (screen_width(), screen_height());
        let (top, step) = (h / 2.0 - 20.0, 30.0);
        let clicked = (0..lines.len()).find(|&n| {
            let y = top + step * n as f32;
            ui::hit(
                Rect::new(w / 2.0 - 260.0, y - 20.0, 520.0, 28.0),
                true,
                input,
            )
            .clicked()
        });
        let (pick, verdict) = decide(row, clicked, lines.len(), input);
        let mut left = None;
        match verdict {
            Step::Cancel => self.log.push(self.text("log.cast_cancelled")),
            Step::Raise => {
                let command = raise_command(spell, resolve, dead[pick]);
                let events = self.game.apply(command);
                self.note(events);
            }
            Step::Stay => {
                left = Some(Resurrect {
                    spell,
                    resolve,
                    row: pick,
                });
            }
        }
        let height = 110.0 + step * lines.len() as f32;
        draw_rectangle(
            w / 2.0 - 280.0,
            h / 2.0 - 80.0,
            560.0,
            height,
            Color::new(0.08, 0.06, 0.04, 0.95),
        );
        draw_rectangle_lines(w / 2.0 - 280.0, h / 2.0 - 80.0, 560.0, height, 3.0, GOLD);
        let name = self.game.content().rules.spells.get(spell).name.clone();
        crate::frame::text_centered(
            &self.text_with("spell.choose_dead", &Args::new().text("spell", &name)),
            w / 2.0,
            h / 2.0 - 45.0,
            22.0,
            WHITE,
        );
        for (n, line) in lines.iter().enumerate() {
            let color = if n == pick { GOLD } else { WHITE };
            crate::frame::text_centered(line, w / 2.0, top + step * n as f32, 22.0, color);
        }
        crate::frame::text_centered(
            &self.text("spell.global_keys"),
            w / 2.0,
            top + step * lines.len() as f32 + 10.0,
            18.0,
            WHITE,
        );
        left
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::testdir::TestResult;

    fn none() -> Input {
        Input::default()
    }

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
        assert_eq!(decide(1, None, 3, &none()), (1, Step::Stay));
    }

    #[test]
    fn enter_or_a_click_raises_the_picked_hero() {
        assert_eq!(
            decide(1, None, 3, &Input::key(KeyCode::Enter)),
            (1, Step::Raise)
        );
        assert_eq!(decide(0, Some(2), 3, &none()), (2, Step::Raise));
    }

    #[test]
    fn escape_or_an_empty_list_gives_up() {
        assert_eq!(
            decide(1, None, 3, &Input::key(KeyCode::Escape)).1,
            Step::Cancel
        );
        assert_eq!(
            decide(0, None, 0, &Input::key(KeyCode::Enter)).1,
            Step::Cancel
        );
    }

    #[test]
    fn a_pick_beyond_a_shrunk_list_is_pulled_back() {
        assert_eq!(decide(5, None, 2, &none()), (1, Step::Stay));
    }

    #[test]
    fn the_command_answers_a_waiting_cast_or_starts_one() -> TestResult {
        let app = App::for_test();
        let spell = app
            .game
            .content()
            .rules
            .spells
            .id_of("resurrection")
            .ok_or("resurrection is not in the content")?;
        assert!(matches!(
            raise_command(spell, true, 7),
            Command::ResolveCast {
                target: Target::Hero(7),
                ..
            }
        ));
        assert!(matches!(
            raise_command(spell, false, 7),
            Command::CastSpell {
                target: Some(Target::Hero(7)),
                ..
            }
        ));
        Ok(())
    }
}
