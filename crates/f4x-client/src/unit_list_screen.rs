//! The list of another wizard's stack (`Unit_List_Window`, ReMoM `UnitList.c`): a right-click on
//! a square holding units that are not yours opens it over the map. A window of the stack's
//! units, 19 pixels each, with the owner's name over them (or "Raiders" and "Rampaging
//! Monsters" for the neutral ones) and the unit's melee, range, armour, hits and moves. It is
//! read only: a click or Escape closes it.

use crate::font::Style;
use crate::input::Input;
use crate::modal::{keep, Modal};
use f4x_core::map::Position;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::view::App;

/// The window's left edge, its width, and the height of a unit's row.
const LEFT: f32 = 45.0;
const WIDTH: f32 = 238.0;
const ROW: f32 = 19.0;
/// The head of the window above the first row, and where the middle of the window sits.
const HEAD: f32 = 24.0;
const MIDDLE: f32 = 97.0;
/// The units a stack holds at most.
const MAX_STACK: usize = 9;

/// The top of the window for `count` units: centred on the original's screen row 97.
pub(crate) fn window_top(count: usize) -> f32 {
    MIDDLE - ((HEAD + count as f32 * ROW) / 2.0).floor()
}

/// The units of a square the player cannot command, listed over the screen.
pub(crate) struct UnitList(Vec<hecs::Entity>);

impl Modal for UnitList {
    fn kind(&self) -> &'static str {
        "unit_list"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.unit_list_frame(*self, input))
    }
}

impl App {
    /// Open the list for the units of `pos` that the human can see, if any are not his own.
    pub(crate) fn open_unit_list(&mut self, pos: Position) -> bool {
        let units: Vec<_> = self
            .game
            .units()
            .units_at(pos)
            .into_iter()
            .filter(|&u| self.game.economy().unit_visible_to(u, crate::view::HUMAN))
            .take(MAX_STACK)
            .collect();
        if units.is_empty() {
            return false;
        }
        self.modals.open(UnitList(units));
        true
    }

    /// For the screenshot check: the stack of the nearest foreign unit, if there is one.
    pub fn open_first_unit_list(&mut self) {
        let at = self
            .game
            .units()
            .all_units()
            .into_iter()
            .filter(|&u| self.game.units().unit_owner(u) != Some(crate::view::HUMAN))
            .find_map(|u| self.game.units().unit_position(u));
        if let Some(at) = at {
            self.reveal_all();
            self.center_on(at);
            self.open_unit_list(at);
        }
    }

    /// One frame of the window over the screen below; what is left of it, `None` once it is left.
    pub(crate) fn unit_list_frame(
        &mut self,
        UnitList(mut units): UnitList,
        input: &Input,
    ) -> Option<UnitList> {
        units.retain(|&u| self.game.units().unit_position(u).is_some());
        let leave = input.key_pressed(KeyCode::Escape)
            || input.mouse_pressed(MouseButton::Left)
            || input.mouse_pressed(MouseButton::Right)
            || units.is_empty();
        if units.is_empty() {
            return None;
        }
        self.draw_unit_list(&units);
        (!leave).then_some(UnitList(units))
    }

    fn draw_unit_list(&mut self, units: &[hecs::Entity]) {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let top = window_top(units.len());
        let height = HEAD + units.len() as f32 * ROW;
        let bottom = top + height;
        // The background reaches down to `bottom`, and the border picture closes it.
        self.clipped(lay_area(top, height), |app| {
            app.draw_pic(&lay, "unitview", 28, 0, LEFT, top);
        });
        self.draw_pic(&lay, "unitview", 29, 0, LEFT, bottom);

        let owner = self.game.units().unit_owner(units[0]);
        let neutral = owner == Some(f4x_core::player::MONSTERS);
        let title = if neutral {
            let fantastic = self
                .game
                .units()
                .unit_def(units[0])
                .is_some_and(|d| d.kind == f4x_core::rules::UnitKind::Fantastic);
            if fantastic {
                self.text("unit_list.monsters")
            } else {
                self.text("unit_list.raiders")
            }
        } else {
            let name = owner
                .and_then(|o| self.game.wizard(o))
                .map_or_else(String::new, |w| w.name());
            self.text_with("unit_list.units_of", &Args::new().text("owner", name))
        };
        let banner = self.banner_color(owner.map_or(f4x_core::player::MONSTERS.0, |o| o.0));
        self.draw_styled(
            view,
            &Style::nearest(4, banner).center(),
            &title,
            LEFT + 118.0,
            top + 7.0,
        );

        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        for (i, &unit) in units.iter().enumerate() {
            let y = top + HEAD + ROW * i as f32;
            self.draw_army_unit(&lay, unit, LEFT + 8.0, y);
            let Some(sheet) = self.overland_sheet(unit) else {
                continue;
            };
            self.draw_styled(
                view,
                &Style::nearest(2, light),
                &sheet.name,
                LEFT + 32.0,
                y + 5.0,
            );
            let ranged = sheet.ranged.unwrap_or_default();
            let stats = [
                (sheet.melee.value, sheet.melee.icon),
                (ranged.value, ranged.icon),
                (sheet.defense.value, sheet.defense.icon),
                (sheet.hits.value, sheet.hits.icon),
                (sheet.moves as i32, sheet.move_icon),
            ];
            for (k, (value, icon)) in stats.into_iter().enumerate() {
                if value <= 0 {
                    continue;
                }
                let x = LEFT + 140.0 + 20.0 * k as f32;
                self.draw_styled(
                    view,
                    &Style::nearest(1, light).right(),
                    &value.to_string(),
                    x,
                    y + 7.0,
                );
                self.draw_pic(&lay, "unitview", 13 + icon, 0, x + 1.0, y + 5.0);
            }
        }
    }
}

/// The window's area on the original's screen.
fn lay_area(top: f32, height: f32) -> (f32, f32, f32, f32) {
    (LEFT, top, WIDTH, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_centred_on_row_97() {
        // The original's own examples: 76 for one unit, 57 for three, 47 for four.
        assert_eq!(window_top(1) as i32, 76);
        assert_eq!(window_top(3) as i32, 57);
        assert_eq!(window_top(4) as i32, 47);
    }
}
