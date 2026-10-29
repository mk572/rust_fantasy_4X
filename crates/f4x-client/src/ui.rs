//! Small immediate-mode widgets and shared colours.

use crate::input::Input;
use macroquad::prelude::*;

pub const PANEL: Color = Color::new(0.08, 0.07, 0.06, 0.92);
pub const GOLD: Color = Color::new(0.95, 0.8, 0.35, 1.0);

/// A clickable button; returns true on the frame it is clicked.
pub fn button(rect: Rect, label: &str, enabled: bool, input: &Input) -> bool {
    let hover = rect.contains(input.mouse_position().into());
    let base = if enabled {
        if hover {
            Color::new(0.45, 0.35, 0.2, 1.0)
        } else {
            Color::new(0.3, 0.22, 0.12, 1.0)
        }
    } else {
        Color::new(0.2, 0.2, 0.2, 1.0)
    };
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, base);
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        2.0,
        Color::new(0.8, 0.65, 0.3, 1.0),
    );
    let size = (rect.h * 0.7).min(24.0);
    let dims = measure_text(label, None, size as u16, 1.0);
    let color = if enabled { WHITE } else { GRAY };
    draw_text(
        label,
        rect.x + (rect.w - dims.width) / 2.0,
        rect.y + (rect.h + dims.height) / 2.0 - 1.0,
        size,
        color,
    );
    enabled && hover && input.mouse_pressed(MouseButton::Left)
}

/// A horizontal progress bar.
pub fn bar(rect: Rect, fraction: f32, fill: Color) {
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        Color::new(0.0, 0.0, 0.0, 0.6),
    );
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w * fraction.clamp(0.0, 1.0),
        rect.h,
        fill,
    );
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        1.0,
        Color::new(0.8, 0.65, 0.3, 1.0),
    );
}
