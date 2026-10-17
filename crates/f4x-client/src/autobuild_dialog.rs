//! The dialog that sets a city's auto-builder plan (docs/auto-builder.md), opened from the
//! city screen. Left-click a row for the next value, right-click for the previous one.

use f4x_core::autobuild::{BuildPlan, Focus, Mix};
use f4x_core::Command;
use macroquad::prelude::*;

use crate::city_screen::Layout;
use crate::view::App;

/// The plan being edited, and whether the city had one when the dialog opened.
pub(crate) struct AutoDialog {
    pub plan: BuildPlan,
    pub had_plan: bool,
}

const MIXES: [Mix; 3] = [Mix::Both, Mix::Buildings, Mix::Units];
const POPULATIONS: std::ops::RangeInclusive<u32> = 2..=12;

/// The next or previous of `all` after `current`, wrapping.
fn step<T: Copy + PartialEq>(all: &[T], current: T, back: bool) -> T {
    let at = all.iter().position(|&x| x == current).unwrap_or(0);
    let len = all.len();
    all[(at + if back { len - 1 } else { 1 }) % len]
}

/// Balanced first, then the seven focuses.
fn primaries() -> Vec<Focus> {
    std::iter::once(Focus::Balanced).chain(Focus::ALL).collect()
}

/// What the button on the city screen says.
pub(crate) fn label(plan: Option<&BuildPlan>) -> String {
    match plan {
        None => "Auto: off".to_string(),
        Some(p) => match p.secondary {
            Some(s) if p.primary != Focus::Balanced => {
                format!("Auto: {} / {}", p.primary.name(), s.name())
            }
            _ => format!("Auto: {}", p.primary.name()),
        },
    }
}

impl App {
    /// The "Auto" button of the city screen; opens the dialog when clicked.
    pub(crate) fn draw_autobuild_button(&mut self, city: hecs::Entity, lay: Layout) {
        let plan = self.game.autobuild_of(city).map(|a| a.plan);
        let own = self.game.autobuild_of(city).is_some();
        let r = lay.rect(142.0, 80.0, 64.0, 10.0);
        let hover = r.contains(mouse_position().into()) && !self.choosing;
        let fill = if hover {
            Color::new(0.4, 0.3, 0.15, 1.0)
        } else {
            Color::new(0.18, 0.14, 0.09, 1.0)
        };
        draw_rectangle(r.x, r.y, r.w, r.h, fill);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, GOLD);
        let text = label(plan.as_ref());
        let size = (5.0 * lay.scale) as u16;
        let dims = measure_text(&text, None, size, 1.0);
        draw_text(
            &text,
            r.x + (r.w - dims.width) / 2.0,
            r.y + r.h * 0.72,
            5.0 * lay.scale,
            if own { WHITE } else { LIGHTGRAY },
        );
        if hover && is_mouse_button_pressed(MouseButton::Left) {
            self.autobuild_dialog = Some(AutoDialog {
                plan: plan.unwrap_or_default(),
                had_plan: own,
            });
        }
    }

    /// Draw the dialog and handle it. Returns true while it is open.
    pub(crate) fn draw_autobuild_dialog(&mut self, city: hecs::Entity, lay: Layout) -> bool {
        let Some(mut d) = self.autobuild_dialog.take() else {
            return false;
        };
        let panel = lay.rect(40.0, 24.0, 240.0, 150.0);
        draw_rectangle(
            panel.x,
            panel.y,
            panel.w,
            panel.h,
            Color::new(0.1, 0.08, 0.05, 0.98),
        );
        draw_rectangle_lines(panel.x, panel.y, panel.w, panel.h, 3.0, GOLD);
        let text = |s: &str, x: f32, y: f32, color: Color| {
            draw_text(
                s,
                lay.rect(x, y, 0.0, 0.0).x,
                lay.rect(x, y, 0.0, 0.0).y,
                6.0 * lay.scale,
                color,
            )
        };
        text("City auto-builder", 48.0, 36.0, GOLD);
        text("Left click: next   Right click: previous", 48.0, 44.0, GRAY);
        let pointer: Vec2 = mouse_position().into();
        let left = is_mouse_button_pressed(MouseButton::Left);
        let right = is_mouse_button_pressed(MouseButton::Right);
        let balanced = d.plan.primary == Focus::Balanced;
        let secondary = match d.plan.secondary {
            Some(s) if !balanced => s.name(),
            _ => "none",
        };
        let rows: [(String, String, bool); 6] = [
            ("Builds".into(), d.plan.mix.name().into(), true),
            ("Primary focus".into(), d.plan.primary.name().into(), true),
            ("Secondary focus".into(), secondary.into(), !balanced),
            (
                "Settlers".into(),
                if d.plan.settlers { "yes" } else { "no" }.into(),
                true,
            ),
            (
                "Settlers from population".into(),
                d.plan.min_settler_pop.to_string(),
                d.plan.settlers,
            ),
            (
                "Switch off when threatened".into(),
                if d.plan.event_prompt { "yes" } else { "no" }.into(),
                true,
            ),
        ];
        let mut action: Option<usize> = None;
        for (i, (name, value, enabled)) in rows.iter().enumerate() {
            let y = 54.0 + i as f32 * 11.0;
            let r = lay.rect(46.0, y - 6.0, 228.0, 10.0);
            let hover = r.contains(pointer);
            if hover && *enabled {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.4, 0.3, 0.15, 1.0));
            }
            let color = if *enabled { WHITE } else { DARKGRAY };
            text(name, 50.0, y, color);
            let dims = measure_text(value, None, (6.0 * lay.scale) as u16, 1.0);
            draw_text(
                value,
                r.x + r.w - dims.width - 4.0,
                lay.rect(0.0, y, 0.0, 0.0).y,
                6.0 * lay.scale,
                if *enabled { GOLD } else { DARKGRAY },
            );
            if hover && *enabled && (left || right) {
                action = Some(i);
            }
        }
        if let Some(i) = action {
            let back = right;
            match i {
                0 => d.plan.mix = step(&MIXES, d.plan.mix, back),
                1 => d.plan.primary = step(&primaries(), d.plan.primary, back),
                2 => {
                    let mut all: Vec<Option<Focus>> = vec![None];
                    all.extend(
                        Focus::ALL
                            .iter()
                            .copied()
                            .filter(|&f| f != d.plan.primary)
                            .map(Some),
                    );
                    d.plan.secondary = step(&all, d.plan.secondary, back);
                }
                3 => d.plan.settlers = !d.plan.settlers,
                4 => {
                    let p = d.plan.min_settler_pop as i32 + if back { -1 } else { 1 };
                    d.plan.min_settler_pop =
                        (p.max(*POPULATIONS.start() as i32) as u32).min(*POPULATIONS.end());
                }
                _ => d.plan.event_prompt = !d.plan.event_prompt,
            }
        }
        text(&hint(&d.plan), 48.0, 126.0, Color::new(0.8, 0.8, 0.6, 1.0));
        let buttons = [("OK", 52.0), ("Switch off", 120.0), ("Cancel", 208.0)];
        let mut chosen: Option<usize> = None;
        for (k, (name, x)) in buttons.iter().enumerate() {
            let enabled = k != 1 || d.had_plan;
            let r = lay.rect(*x, 150.0, 60.0, 11.0);
            let hover = r.contains(pointer) && enabled;
            draw_rectangle(
                r.x,
                r.y,
                r.w,
                r.h,
                if hover {
                    Color::new(0.4, 0.3, 0.15, 1.0)
                } else {
                    Color::new(0.2, 0.15, 0.1, 1.0)
                },
            );
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, GOLD);
            let dims = measure_text(name, None, (6.0 * lay.scale) as u16, 1.0);
            draw_text(
                name,
                r.x + (r.w - dims.width) / 2.0,
                r.y + r.h * 0.75,
                6.0 * lay.scale,
                if enabled { WHITE } else { DARKGRAY },
            );
            if hover && left {
                chosen = Some(k);
            }
        }
        if is_key_pressed(KeyCode::Escape) {
            chosen = Some(2);
        }
        if is_key_pressed(KeyCode::Enter) {
            chosen = Some(0);
        }
        match chosen {
            Some(0) => {
                let events = self.game.apply(Command::SetAutoBuild {
                    city,
                    plan: Some(d.plan),
                });
                self.note(events);
            }
            Some(1) => {
                let events = self.game.apply(Command::SetAutoBuild { city, plan: None });
                self.note(events);
            }
            Some(_) => {}
            None => {
                self.autobuild_dialog = Some(d);
            }
        }
        true
    }
}

/// One line on what the plan will do.
fn hint(plan: &BuildPlan) -> String {
    let what = match plan.mix {
        Mix::Both => "Spends equal production on units and buildings.",
        Mix::Buildings => "Builds buildings only.",
        Mix::Units => "Builds units only.",
    };
    if plan.mix == Mix::Buildings {
        what.to_string()
    } else {
        format!("{what} Defenders come first.")
    }
}
