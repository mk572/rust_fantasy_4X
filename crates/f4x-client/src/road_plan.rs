//! Road-building mode (task 102): the original's topics 510-512. B with engineers selected opens
//! the mode; a click on the map picks the far end, the squares between are marked (blue where
//! a road can be built, red where it cannot) with the turns each takes, and OK builds the whole
//! path with `Command::BuildRoad`. Cancel, Escape or a right-click leave the mode.

use f4x_core::city::Position;
use f4x_core::Command;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::ui::{button, GOLD};
use crate::view::App;

/// A road being planned.
#[derive(Debug, Clone)]
pub struct RoadPlan {
    /// The engineers (the selection when the mode was opened).
    pub units: Vec<hecs::Entity>,
    /// Where they stand.
    pub start: Position,
    /// The squares to build on after `start`; empty until a far end is picked.
    pub path: Vec<Position>,
}

/// The most turns a plan may add up to; the path is cut short before it passes this.
const MAX_TURNS: u32 = 120;

/// The buttons' places in the original's pixels: OK and Cancel on the side panel.
const OK_AT: (f32, f32) = (246.0, 181.0);
const CANCEL_AT: (f32, f32) = (280.0, 181.0);
/// Both buttons are 33 by 9.
const BUTTON_SIZE: (f32, f32) = (33.0, 9.0);

/// What a plan comes to.
struct Summary {
    /// Turns in all, over the squares up to where the path is cut.
    turns: u32,
    /// A square on the path cannot hold a road.
    blocked: bool,
    /// How many squares of the path count (the path is cut at water or past the turn limit).
    shown: usize,
}

fn button_rect(lay: &Layout, (x, y): (f32, f32)) -> Rect {
    lay.rect((x, y, BUTTON_SIZE.0, BUTTON_SIZE.1))
}

impl App {
    /// Open the mode for the selected engineers.
    pub(crate) fn start_road_plan(&mut self) {
        let Some(&first) = self.selection.first() else {
            return;
        };
        let Some(start) = self.game.units().unit_position(first) else {
            return;
        };
        self.road_plan = Some(RoadPlan {
            units: self.selection.clone(),
            start,
            path: Vec::new(),
        });
        self.log
            .push("Road: click where it should end, then OK (Enter)".into());
    }

    /// Whether a click at the mouse falls on one of the mode's buttons.
    pub(crate) fn road_plan_on_button(&self) -> bool {
        let lay = Layout::new();
        let at = mouse_position().into();
        self.road_plan.is_some()
            && (button_rect(&lay, OK_AT).contains(at) || button_rect(&lay, CANCEL_AT).contains(at))
    }

    /// A map click in the mode: plan the road to that square.
    pub(crate) fn road_plan_click(&mut self, pos: Position) {
        let Some(plan) = self.road_plan.as_mut() else {
            return;
        };
        if pos == plan.start {
            plan.path.clear();
            return;
        }
        match self.game.find_path(&plan.units, pos) {
            Some(path) => plan.path = path,
            None => {
                plan.path.clear();
                self.log.push("No way there".into());
            }
        }
    }

    /// Build the planned road and leave the mode.
    pub(crate) fn road_plan_confirm(&mut self) {
        let Some(mut plan) = self.road_plan.take() else {
            return;
        };
        let sum = self.road_summary(&plan);
        // The original does nothing when there is no time to spend (nothing picked, or only
        // squares that have their road already) or the way is blocked.
        if sum.turns == 0 || sum.blocked {
            self.road_plan = Some(plan);
            return;
        }
        plan.path.truncate(sum.shown);
        let events = self.game.apply(Command::BuildRoad {
            units: plan.units,
            path: plan.path,
        });
        self.note(events);
    }

    /// Leave the mode without building.
    pub(crate) fn road_plan_cancel(&mut self) {
        if self.road_plan.take().is_some() {
            self.log.push("Road cancelled".into());
        }
    }

    /// Keys of the mode: Enter or B builds.
    pub(crate) fn road_plan_keys(&mut self) {
        if self.road_plan.is_none() {
            return;
        }
        // The engineers must still be where the plan began.
        let moved = self.road_plan.as_ref().is_some_and(|p| {
            p.units
                .first()
                .is_none_or(|&u| self.game.units().unit_position(u) != Some(p.start))
        });
        if moved {
            self.road_plan = None;
            return;
        }
        if is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::KpEnter)
            || is_key_pressed(KeyCode::O)
        {
            self.road_plan_confirm();
        }
    }

    /// Turns, whether water blocks the way, and how much of the path counts.
    fn road_summary(&self, plan: &RoadPlan) -> Summary {
        let mut turns = u32::from(self.game.road_turns(&plan.units, plan.start).unwrap_or(0));
        let mut sum = Summary {
            turns,
            blocked: false,
            shown: plan.path.len(),
        };
        for (i, &pos) in plan.path.iter().enumerate() {
            match self.game.road_turns(&plan.units, pos) {
                None => {
                    sum.blocked = true;
                    sum.shown = i + 1;
                    break;
                }
                Some(t) if turns + u32::from(t) > MAX_TURNS => {
                    sum.shown = i;
                    break;
                }
                Some(t) => turns += u32::from(t),
            }
        }
        sum.turns = turns;
        sum
    }

    /// The path as a line from the engineers' square through the squares to build on,
    /// blue (red where it is blocked), its four shades running along it. Called inside the
    /// map's clip, with the position of a square's top left on screen.
    pub(crate) fn draw_road_plan(&self, screen_of: &dyn Fn(u32, u32) -> Vec2, t: Vec2) {
        let Some(plan) = &self.road_plan else {
            return;
        };
        let sum = self.road_summary(plan);
        let shades: [Color; 4] = if sum.blocked {
            [
                Color::new(0.55, 0.0, 0.0, 1.0),
                Color::new(0.8, 0.1, 0.1, 1.0),
                Color::new(1.0, 0.3, 0.25, 1.0),
                Color::new(1.0, 0.55, 0.5, 1.0),
            ]
        } else {
            [
                Color::new(0.1, 0.15, 0.6, 1.0),
                Color::new(0.2, 0.3, 0.85, 1.0),
                Color::new(0.4, 0.55, 1.0, 1.0),
                Color::new(0.7, 0.8, 1.0, 1.0),
            ]
        };
        // The shades step along the line, and the line moves with the clock.
        let phase = 4 - (get_time() * 10.0) as usize % 4;
        let step = (t.x / 8.0).max(2.0);
        let mut n = 0;
        let centre = |p: Position| screen_of(p.x, p.y) + t / 2.0;
        let mut from = plan.start;
        for &to in &plan.path[..sum.shown] {
            let (a, b) = (centre(from), centre(to));
            from = to;
            // A step across the map's seam would draw a line over the whole map.
            if (b - a).length() > t.x * 2.0 || to.plane != self.plane {
                continue;
            }
            let pieces = ((b - a).length() / step).ceil().max(1.0) as usize;
            for k in 0..pieces {
                let (p, q) = (
                    a.lerp(b, k as f32 / pieces as f32),
                    a.lerp(b, (k + 1) as f32 / pieces as f32),
                );
                draw_line(
                    p.x,
                    p.y,
                    q.x,
                    q.y,
                    (t.x / 12.0).max(2.0),
                    shades[(n + phase) % 4],
                );
                n += 1;
            }
        }
    }

    /// The panel over the side column: the title, the turns it will take and the buttons.
    pub(crate) fn road_plan_bar(&mut self) {
        let Some(plan) = self.road_plan.clone() else {
            return;
        };
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let sum = self.road_summary(&plan);
        let (mouse, down) = (
            Vec2::from(mouse_position()),
            is_mouse_button_down(MouseButton::Left),
        );
        let pressed = is_mouse_button_pressed(MouseButton::Left);
        let (ok_rect, cancel_rect) = (button_rect(&lay, OK_AT), button_rect(&lay, CANCEL_AT));
        let can = sum.turns > 0 && !sum.blocked;

        // The panel hides Next Turn, then the buttons sit on the border at the bottom.
        let art = self.draw_pic(&lay, "main", 44, 0, 242.0, 173.0).is_some()
            && self.draw_pic(&lay, "main", 45, 0, 240.0, 76.0).is_some();
        if !art {
            // No pictures extracted: the plain buttons and a line of text.
            let text = format!(
                "Road: {} turns{}",
                sum.turns,
                if sum.blocked { " (blocked)" } else { "" }
            );
            draw_text(&text, ok_rect.x - 200.0, 30.0, 22.0, GOLD);
            let (ok, cancel) = (
                Rect::new(screen_width() / 2.0 - 110.0, 8.0, 100.0, 30.0),
                Rect::new(screen_width() / 2.0 + 10.0, 8.0, 100.0, 30.0),
            );
            if button(ok, "OK", can) && can {
                self.road_plan_confirm();
            } else if button(cancel, "Cancel", true) {
                self.road_plan_cancel();
            }
            return;
        }
        self.draw_pic(&lay, "main", 48, 0, 240.0, 173.0);
        self.bm_text_set(view, 4, 1, "Road", 280.0, 80.0, Align::Center);
        self.bm_text_set(view, 4, 1, "Building", 280.0, 90.0, Align::Center);
        let message = if sum.blocked {
            "Blocked.  Roads can not be built across large bodies of water.".to_string()
        } else if sum.turns > 0 {
            format!(
                "It will take {} turn{} to complete the construction of this road.",
                sum.turns,
                if sum.turns > 1 { "s" } else { "" }
            )
        } else {
            String::new()
        };
        for (i, line) in self.bm_wrap(1, &message, 60.0).iter().enumerate() {
            self.bm_text_set(view, 1, 0, line, 250.0, 106.0 + i as f32 * 9.0, Align::Left);
        }
        let ok_frame = usize::from(can && down && ok_rect.contains(mouse));
        self.draw_pic_tinted(
            &lay,
            "main",
            46,
            ok_frame,
            OK_AT.0,
            OK_AT.1,
            if can {
                WHITE
            } else {
                Color::new(1.0, 1.0, 1.0, 0.5)
            },
        );
        let cancel_frame = usize::from(down && cancel_rect.contains(mouse));
        self.draw_pic(&lay, "main", 41, cancel_frame, CANCEL_AT.0, CANCEL_AT.1);
        if pressed && can && ok_rect.contains(mouse) {
            self.road_plan_confirm();
        } else if pressed && cancel_rect.contains(mouse) {
            self.road_plan_cancel();
        }
    }
}
