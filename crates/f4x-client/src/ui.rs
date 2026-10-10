//! Small immediate-mode widgets and shared colours.

use macroquad::prelude::*;

pub const PANEL: Color = Color::new(0.08, 0.07, 0.06, 0.92);
pub const GOLD: Color = Color::new(0.95, 0.8, 0.35, 1.0);
pub const INK: Color = Color::new(0.1, 0.07, 0.04, 1.0);

/// One colour per player id, for borders and flags.
pub fn player_color(id: u8) -> Color {
    const COLORS: [Color; 6] = [
        Color::new(0.2, 0.45, 0.95, 1.0),
        Color::new(0.85, 0.2, 0.2, 1.0),
        Color::new(0.2, 0.7, 0.3, 1.0),
        Color::new(0.9, 0.8, 0.2, 1.0),
        Color::new(0.7, 0.3, 0.8, 1.0),
        Color::new(0.9, 0.55, 0.15, 1.0),
    ];
    if id == u8::MAX {
        return Color::new(0.6, 0.6, 0.6, 1.0);
    }
    COLORS[id as usize % COLORS.len()]
}

/// A clickable button; returns true on the frame it is clicked.
pub fn button(rect: Rect, label: &str, enabled: bool) -> bool {
    let hover = rect.contains(mouse_position().into());
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
    enabled && hover && is_mouse_button_pressed(MouseButton::Left)
}

/// Stacked text lines in a panel.
pub struct Lines {
    pub x: f32,
    pub y: f32,
    pub size: f32,
}

impl Lines {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y, size: 20.0 }
    }

    pub fn line(&mut self, text: &str, color: Color) {
        draw_text(text, self.x, self.y, self.size, color);
        self.y += self.size * 1.2;
    }

    pub fn gap(&mut self, pixels: f32) {
        self.y += pixels;
    }
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

/// Break `text` into lines no wider than `width` pixels at font size `size`.
pub fn wrap(text: &str, size: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && measure_text(&candidate, None, size as u16, 1.0).width > width {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    lines
}
