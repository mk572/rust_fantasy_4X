//! Resurrection: the dead hero to raise is chosen from a list before the spell is cast
//! (`Cast_Resurrection`), like a rival's global enchantment is for Disjunction. Up and Down
//! move, Enter or a click casts it (the spell's target, aimed at the hero), Escape gives up.

use f4x_core::game::Command;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::modal::Modal;
use crate::ui::GOLD;
use crate::view::{App, Screen, HUMAN};
use f4x_core::rules::exec::Target;
use f4x_core::rules::SpellId;

impl App {
    /// Start Resurrection: open the list of the dead heroes, or say there are none.
    pub(crate) fn open_resurrect(&mut self, spell: SpellId, resolve: bool) {
        self.screen = Screen::Map;
        if self.game.units().raisable_heroes().is_empty() {
            self.log.push(self.text("spell.no_dead"));
        } else {
            self.modals.open(Modal::Resurrect {
                spell,
                resolve,
                row: 0,
            });
        }
    }

    /// The list of the dead heroes over the map, the row picked at `row`.
    pub(crate) fn resurrect_prompt_frame(
        &mut self,
        spell: SpellId,
        resolve: bool,
        row: usize,
    ) -> Option<Modal> {
        let dead = self.game.units().raisable_heroes();
        let lines: Vec<String> = dead
            .iter()
            .map(|&kind| self.game.units().hero_type_name(kind).unwrap_or_default())
            .collect();
        let row = row.min(lines.len().saturating_sub(1));
        let (w, h) = (screen_width(), screen_height());
        let (top, step) = (h / 2.0 - 20.0, 30.0);
        let clicked = if is_mouse_button_pressed(MouseButton::Left) {
            let (mx, my) = mouse_position();
            (0..lines.len()).find(|&n| {
                let y = top + step * n as f32;
                (w / 2.0 - 260.0..w / 2.0 + 260.0).contains(&mx)
                    && (y - 20.0..y + 8.0).contains(&my)
            })
        } else {
            None
        };
        let mut pick = clicked.unwrap_or(row);
        if is_key_pressed(KeyCode::Up) {
            pick = pick.saturating_sub(1);
        }
        if is_key_pressed(KeyCode::Down) {
            pick = (pick + 1).min(lines.len().saturating_sub(1));
        }
        let mut left = None;
        if is_key_pressed(KeyCode::Escape) || dead.is_empty() {
            self.log.push(self.text("log.cast_cancelled"));
        } else if clicked.is_some() || is_key_pressed(KeyCode::Enter) {
            let target = Target::Hero(dead[pick]);
            let command = if resolve {
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
            };
            let events = self.game.apply(command);
            self.note(events);
        } else {
            left = Some(Modal::Resurrect {
                spell,
                resolve,
                row: pick,
            });
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
