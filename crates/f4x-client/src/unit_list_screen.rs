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
/// The rows the window shows at once; a larger stack scrolls.
const VISIBLE: usize = 8;

/// The top of the window for `count` units: centred on the original's screen row 97.
pub(crate) fn window_top(count: usize) -> f32 {
    MIDDLE - ((HEAD + count as f32 * ROW) / 2.0).floor()
}

/// The units of a square the player cannot command, listed over the screen.
pub(crate) struct UnitList(Vec<hecs::Entity>, usize);

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
            .collect();
        if units.is_empty() {
            return false;
        }
        self.modals.open(UnitList(units, 0));
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
            // A crowded square, to see the list scroll.
            let owner = self.game.units().units_at(at).first().and_then(|&u| {
                Some((
                    self.game.units().unit_owner(u)?,
                    self.game.units().unit_def(u)?.id.clone(),
                ))
            });
            if let Some((owner, id)) = owner {
                if let Some(kind) = self.game.content().rules.units.id_of(&id) {
                    for _ in 0..12 {
                        self.game.spawn_unit(owner, at, kind);
                    }
                }
            }
            self.reveal_all();
            self.center_on(at);
            self.open_unit_list(at);
        }
    }

    /// One frame of the window over the screen below; what is left of it, `None` once it is left.
    pub(crate) fn unit_list_frame(
        &mut self,
        UnitList(mut units, mut first): UnitList,
        input: &Input,
    ) -> Option<UnitList> {
        units.retain(|&u| self.game.units().unit_position(u).is_some());
        if units.is_empty() {
            return None;
        }
        // A bigger stack scrolls: the wheel, the arrow keys and the page keys. Escape and a click
        // close the window as before.
        let last = units.len().saturating_sub(VISIBLE);
        let wheel = input.mouse_wheel().1;
        if wheel > 0.0 || input.key_pressed(KeyCode::Up) {
            first = first.saturating_sub(1);
        }
        if wheel < 0.0 || input.key_pressed(KeyCode::Down) {
            first += 1;
        }
        if input.key_pressed(KeyCode::PageUp) {
            first = first.saturating_sub(VISIBLE);
        }
        if input.key_pressed(KeyCode::PageDown) {
            first += VISIBLE;
        }
        first = first.min(last);
        let leave = input.key_pressed(KeyCode::Escape)
            || input.mouse_pressed(MouseButton::Left)
            || input.mouse_pressed(MouseButton::Right);
        self.draw_unit_list(&units, first);
        (!leave).then_some(UnitList(units, first))
    }

    fn draw_unit_list(&mut self, all: &[hecs::Entity], first: usize) {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let units = &all[first..(first + VISIBLE).min(all.len())];
        let top = window_top(units.len());
        let height = HEAD + units.len() as f32 * ROW;
        let bottom = top + height;
        // The background reaches down to `bottom`, and the border picture closes it.
        self.clipped(lay_area(top, height), |app| {
            app.draw_pic(&lay, "unitview", 28, 0, LEFT, top);
        });
        self.draw_pic(&lay, "unitview", 29, 0, LEFT, bottom);

        // Two arrows in the title bar say whether more units wait above or below.
        if all.len() > VISIBLE {
            let lit = Color::new(0.95, 0.93, 0.85, 1.0);
            let dim = Color::new(0.95, 0.93, 0.85, 0.3);
            let x = LEFT + WIDTH - 22.0;
            let up = lay.rect((x, top + 5.0, 8.0, 6.0));
            let down = lay.rect((x, top + 14.0, 8.0, 6.0));
            let colour = |on: bool| if on { lit } else { dim };
            draw_triangle(
                vec2(up.x, up.y + up.h),
                vec2(up.x + up.w, up.y + up.h),
                vec2(up.x + up.w / 2.0, up.y),
                colour(first > 0),
            );
            draw_triangle(
                vec2(down.x, down.y),
                vec2(down.x + down.w, down.y),
                vec2(down.x + down.w / 2.0, down.y + down.h),
                colour(first + VISIBLE < all.len()),
            );
        }

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
