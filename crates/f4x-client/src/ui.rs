//! Small immediate-mode widgets and shared colours.

use crate::input::Input;
use macroquad::prelude::*;

pub const PANEL: Color = Color::new(0.08, 0.07, 0.06, 0.92);
pub const GOLD: Color = Color::new(0.95, 0.8, 0.35, 1.0);

/// A clickable button; returns true on the frame it is clicked.
pub fn button(rect: Rect, label: &str, enabled: bool, input: &Input) -> bool {
    let state = hit(rect, enabled, input);
    let base = if enabled {
        if state.hovered() {
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
    state.clicked()
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

/// What the mouse does to a button or hot-zone in one frame. The one place that tests the mouse
/// against a rectangle for buttons; screens ask `hit` instead of repeating the test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    /// Not over it, or the control is disabled.
    None,
    /// Over it, no button down.
    Hover,
    /// Over it with the left button held (the picture shows its second frame).
    Pressed,
    /// Over it on the frame the left button went down.
    Clicked,
}

impl Hit {
    pub(crate) fn clicked(self) -> bool {
        self == Hit::Clicked
    }

    /// Over the control (an enabled one), whether or not a button is down.
    pub(crate) fn hovered(self) -> bool {
        self != Hit::None
    }

    /// The left button is held on it, which includes the frame it went down.
    pub(crate) fn held(self) -> bool {
        matches!(self, Hit::Pressed | Hit::Clicked)
    }
}

/// The mouse against `rect` (in screen pixels; a screen in the original's 320x200 coordinates
/// maps its rectangle through `Layout` first). A disabled control is always `Hit::None`.
pub(crate) fn hit(rect: Rect, enabled: bool, input: &Input) -> Hit {
    if !enabled || !rect.contains(input.mouse_position().into()) {
        Hit::None
    } else if input.mouse_pressed(MouseButton::Left) {
        Hit::Clicked
    } else if input.mouse_down(MouseButton::Left) {
        Hit::Pressed
    } else {
        Hit::Hover
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECT: Rect = Rect {
        x: 10.0,
        y: 10.0,
        w: 20.0,
        h: 10.0,
    };

    #[test]
    fn a_control_answers_hover_press_and_click() {
        let hover = Input {
            mouse: (15.0, 15.0),
            ..Input::default()
        };
        let held = Input {
            buttons_down: vec![MouseButton::Left],
            ..hover.clone()
        };
        assert_eq!(hit(RECT, true, &hover), Hit::Hover);
        assert_eq!(hit(RECT, true, &held), Hit::Pressed);
        assert_eq!(
            hit(RECT, true, &Input::click(MouseButton::Left, 15.0, 15.0)),
            Hit::Clicked
        );
    }

    #[test]
    fn a_disabled_control_or_one_missed_answers_none() {
        let click = Input::click(MouseButton::Left, 15.0, 15.0);
        assert_eq!(hit(RECT, false, &click), Hit::None);
        let miss = Input::click(MouseButton::Left, 50.0, 15.0);
        assert_eq!(hit(RECT, true, &miss), Hit::None);
        assert_eq!(hit(RECT, true, &Input::default()), Hit::None);
    }

    #[test]
    fn only_the_left_button_clicks() {
        let right = Input::click(MouseButton::Right, 15.0, 15.0);
        assert_eq!(hit(RECT, true, &right), Hit::Hover);
        assert!(Hit::Clicked.held() && Hit::Pressed.held() && !Hit::Hover.held());
        assert!(Hit::Hover.hovered() && !Hit::None.hovered());
    }
}
