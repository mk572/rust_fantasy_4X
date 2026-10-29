//! The main screen's right column while a stack is selected (`Main_Screen_Draw_Unit_Window`,
//! `Main_Screen_Draw_Unit_Action_Buttons`, `Main_Screen_Draw_Movement_Bar` in `MainScr.c`): up
//! to nine unit pictures in a grid of three, the Done, Patrol, Wait and Build buttons and the
//! stack's moves. A click on a picture picks that unit, a right-click opens its sheet.

use crate::font::Style;
use crate::input::Input;
use crate::ui;
use f4x_core::game::Command;
use f4x_core::map::Position;
use f4x_core::text::Args;
use f4x_core::unit::Standby;
use macroquad::prelude::*;

use crate::frame::{FrameClick, Layout};
use crate::view::{App, HUMAN};

/// The top left of the first picture, and the step between pictures (`Unit_Window_Picture_Coords`).
const START: (f32, f32) = (247.0, 79.0);
const STEP: (f32, f32) = (23.0, 29.0);
/// Where the four buttons sit.
const DONE: (f32, f32) = (246.0, 176.0);
const PATROL: (f32, f32) = (280.0, 176.0);
const WAIT: (f32, f32) = (246.0, 186.0);
const SPECIAL: (f32, f32) = (280.0, 186.0);

/// The button a click on the column pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackButton {
    Done,
    Patrol,
    Wait,
    Special,
}

/// What the fourth button does for the selected units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Special {
    None,
    Build,
    Meld,
    Purify,
}

impl App {
    /// Which special action the selected units have (the original's `special_action_flag`).
    pub(crate) fn special_action(&self) -> Special {
        let units = self.game.units();
        let any = |ability: &str| {
            self.selection
                .iter()
                .any(|&u| units.unit_has_ability(u, ability))
        };
        let builds = self.selection.iter().any(|&u| {
            units.unit_has_ability(u, "create_outpost")
                || units.unit_stat(u, f4x_core::rules::Stat::Construction) > 0
        });
        if builds {
            Special::Build
        } else if any("meld") {
            Special::Meld
        } else if any("purify") {
            Special::Purify
        } else {
            Special::None
        }
    }

    /// The units of the selected stack, in the order the window shows them.
    fn stack_units(&self) -> Vec<hecs::Entity> {
        let Some(at) = self.selection_position() else {
            return Vec::new();
        };
        self.game
            .units()
            .units_at(at)
            .into_iter()
            .filter(|&u| self.game.units().unit_owner(u) == Some(HUMAN))
            .take(f4x_core::unit::MAX_STACK)
            .collect()
    }

    /// One unit's picture on its background, as `Draw_Unit_Picture` does: a selected unit has
    /// the background of its banner under it.
    pub(crate) fn draw_unit_picture(
        &mut self,
        lay: &Layout,
        unit: hecs::Entity,
        at: (f32, f32),
        selected: bool,
        tint: Color,
    ) {
        let Some((lbx, entry)) = self.game.units().unit_def(unit).and_then(|d| d.art.clone())
        else {
            return;
        };
        let banner = self
            .banner_index(HUMAN.0)
            .min(f4x_core::player::NEUTRAL_BANNER);
        if selected {
            self.draw_pic(
                lay,
                "mapback",
                crate::art::background_entry(banner),
                0,
                at.0,
                at.1,
            );
        }
        let tex = match self.unit_glow_realm(unit) {
            Some(realm) => self.art.unit_glow(
                &lbx,
                entry as usize,
                banner,
                realm,
                crate::map_slide::glow_step(),
            ),
            None => self.art.unit_picture(&lbx, entry as usize, banner),
        };
        if let Some(tex) = tex {
            let to = lay.point(at.0 + 1.0, at.1 + 1.0);
            draw_texture_ex(
                tex,
                to.x,
                to.y,
                tint,
                DrawTextureParams {
                    dest_size: Some(tex.size() * lay.scale),
                    ..Default::default()
                },
            );
        }
    }

    /// The damage bar, experience medals and weapon icon of one picture
    /// (`Unit_Window_Draw_Unit_Attributes`).
    fn draw_unit_attributes(&mut self, lay: &Layout, unit: hecs::Entity, at: (f32, f32)) {
        let (hits, max) = self.game.units().unit_hits(unit);
        if hits < max && hits > 0 {
            let length = (hits * 10 / max).saturating_sub(1) as f32;
            let colour = if length >= 6.0 {
                Color::new(0.2, 0.8, 0.2, 1.0)
            } else if length >= 3.0 {
                Color::new(0.9, 0.85, 0.2, 1.0)
            } else {
                Color::new(0.85, 0.15, 0.15, 1.0)
            };
            let from = lay.point(at.0 + 5.0, at.1 + 19.0);
            draw_rectangle(
                from.x,
                from.y,
                (length + 1.0) * lay.scale,
                lay.scale,
                colour,
            );
        }
        let level = self.game.units().unit_level(unit);
        if level > 0 {
            let (medal, zero) = match level {
                7.. => (53, 6),
                4..=6 => (52, 3),
                _ => (51, 0),
            };
            for i in 0..level - zero {
                self.draw_pic(
                    lay,
                    "main",
                    medal,
                    0,
                    at.0 + 2.0 + 4.0 * i as f32,
                    at.1 + 21.0,
                );
            }
        }
        let quality = usize::from(self.game.units().unit_weapon_quality(unit));
        if quality > 0 {
            let frame = (get_time() * 6.0) as usize % 4;
            self.draw_pic(lay, "main", 53 + quality, frame, at.0 + 13.0, at.1 + 19.0);
        }
    }

    /// Draw the unit window, the buttons and the moves, and say what was clicked.
    pub(crate) fn draw_unit_window(&mut self, lay: &Layout, click: &mut FrameClick, input: &Input) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let right = input.mouse_pressed(MouseButton::Right);
        let stack = self.stack_units();
        let all_picked = stack.iter().all(|u| self.selection.contains(u));
        for (i, &unit) in stack.iter().enumerate() {
            let at = (
                START.0 + (i % 3) as f32 * STEP.0,
                START.1 + (i / 3) as f32 * STEP.1,
            );
            // The slot's own background is one pixel up and left of the picture.
            self.draw_pic(lay, "main", 24 + i, 0, at.0 - 1.0, at.1 - 1.0);
            let selected = self.selection.contains(&unit);
            let patrolling = self.game.units().unit_standby(unit) == Some(Standby::Patrol);
            let tint = if patrolling {
                Color::new(0.6, 0.6, 0.6, 1.0)
            } else {
                WHITE
            };
            self.draw_unit_picture(lay, unit, at, selected, tint);
            self.draw_unit_attributes(lay, unit, at);
            let mark = if self.game.unit_building_road(unit) {
                Some("B")
            } else if self.game.unit_has_order(unit) {
                Some("G")
            } else {
                None
            };
            if let Some(mark) = mark {
                self.draw_styled(view, &Style::outlined(1), mark, at.0 + 2.0, at.1 + 2.0);
            }
            let rect = lay.rect((at.0 - 1.0, at.1 - 1.0, STEP.0 - 1.0, STEP.1 - 1.0));
            let state = ui::hit(rect, true, input);
            if state.hovered() {
                if state.clicked() {
                    click.slot = Some(unit);
                    click.pick_only = all_picked;
                }
                if right {
                    click.slot_info = Some(unit);
                }
            }
        }

        // The buttons.
        let special = self.special_action();
        let mut button = |app: &mut App, entry: usize, at: (f32, f32), which: StackButton| {
            let tex_size = app.art_picture("main", entry, 0).map(|t| t.size());
            let state = tex_size.map_or(ui::Hit::None, |s| {
                let p = lay.point(at.0, at.1);
                ui::hit(
                    Rect::new(p.x, p.y, s.x * lay.scale, s.y * lay.scale),
                    true,
                    input,
                )
            });
            app.draw_pic(lay, "main", entry, usize::from(state.held()), at.0, at.1);
            if state.clicked() {
                click.button = Some(which);
            }
        };
        button(self, 8, DONE, StackButton::Done);
        button(self, 9, PATROL, StackButton::Patrol);
        button(self, 10, WAIT, StackButton::Wait);
        match special {
            Special::None => {
                self.draw_pic(lay, "main", 15, 0, SPECIAL.0, SPECIAL.1);
            }
            Special::Build => button(self, 11, SPECIAL, StackButton::Special),
            Special::Meld => button(self, 49, SPECIAL, StackButton::Special),
            Special::Purify => button(self, 42, SPECIAL, StackButton::Special),
        }

        // "Moves:" and what the active units have left, in whole and half moves.
        let moves = self
            .selection
            .iter()
            .map(|&u| self.game.units().unit_moves_left(u))
            .min()
            .unwrap_or(0);
        if moves > 0 {
            let args = Args::new().count("n", moves / 2);
            let text = self.text_with(
                if moves % 2 == 1 {
                    "unit.moves_half"
                } else {
                    "unit.moves_whole"
                },
                &args,
            );
            self.draw_styled(view, &Style::outlined(1), &text, 246.0, 167.0);
        }
    }

    /// What a click on the unit window or its buttons does.
    pub(crate) fn unit_window_click(&mut self, click: FrameClick) {
        if let Some(unit) = click.slot {
            if click.pick_only {
                self.selection = vec![unit];
            } else if let Some(i) = self.selection.iter().position(|&u| u == unit) {
                if self.selection.len() > 1 {
                    self.selection.remove(i);
                }
            } else {
                self.selection.push(unit);
            }
        }
        if let Some(unit) = click.slot_info {
            let group = self.stack_units();
            self.open_unit_sheet_of(unit, group);
        }
        let units = self.selection.clone();
        match click.button {
            Some(StackButton::Done) => self.stand_by(units, Standby::Done),
            Some(StackButton::Patrol) => self.stand_by(units, Standby::Patrol),
            Some(StackButton::Wait) => self.select_next_idle(true),
            Some(StackButton::Special) => self.special_button(),
            None => {}
        }
    }

    /// The Done and Patrol buttons: the units are skipped, and the next stack is picked.
    pub(crate) fn stand_by(&mut self, units: Vec<hecs::Entity>, standby: Standby) {
        self.game.apply(Command::SetStandby {
            units,
            standby: Some(standby),
        });
        self.select_next_idle(false);
    }

    /// Pick the next stack with units waiting for orders after the selected one, or deselect
    /// when there is none. `keep` leaves the current stack alone if it is the only one (Wait).
    pub(crate) fn select_next_idle(&mut self, keep: bool) {
        let from = self.selection_position();
        let mut idle: Vec<Position> = Vec::new();
        for unit in self.game.units().player_units(HUMAN) {
            let Some(at) = self.game.units().unit_position(unit) else {
                continue;
            };
            if self.game.unit_is_idle(unit) && !idle.contains(&at) {
                idle.push(at);
            }
        }
        // Start after the current stack in the list, wrapping round.
        let start = from.and_then(|f| idle.iter().position(|&p| p == f));
        let candidates =
            (0..idle.len()).map(|i| idle[(start.map_or(0, |s| s + 1) + i) % idle.len()]);
        let next = candidates.into_iter().find(|&p| Some(p) != from);
        match next {
            Some(at) => {
                self.select_at(at);
                self.center_on(at);
            }
            None if keep => {}
            None => self.selection.clear(),
        }
    }

    /// Whether the human has no unit left that waits for orders (the Next Turn button shows).
    pub(crate) fn all_units_moved(&self) -> bool {
        !self
            .game
            .units()
            .player_units(HUMAN)
            .into_iter()
            .any(|u| self.game.unit_is_idle(u))
    }
}
