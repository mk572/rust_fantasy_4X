//! The input of one frame, read from the window once at the top of the loop (`Input::read`) and
//! handed down to every screen and prompt that asks. The same frame gives the same answers
//! everywhere, typed characters are drained once, and a test builds the frame by hand instead of
//! needing a window. A screen drawn under a modal is given `Input::default()`: no hover, no click.

use std::collections::HashSet;

use macroquad::prelude::{KeyCode, MouseButton};

/// What the player did in one frame.
#[derive(Clone, Debug, Default)]
pub(crate) struct Input {
    pub(crate) mouse: (f32, f32),
    pub(crate) wheel: (f32, f32),
    pub(crate) buttons_pressed: Vec<MouseButton>,
    pub(crate) buttons_down: Vec<MouseButton>,
    pub(crate) keys_pressed: HashSet<KeyCode>,
    pub(crate) keys_down: HashSet<KeyCode>,
    pub(crate) chars: Vec<char>,
}

const BUTTONS: [MouseButton; 3] = [MouseButton::Left, MouseButton::Right, MouseButton::Middle];

impl Input {
    /// Take this frame's input from the window; the typed characters are consumed.
    pub(crate) fn read() -> Input {
        use macroquad::prelude as mq;
        let mut chars = Vec::new();
        while let Some(c) = mq::get_char_pressed() {
            chars.push(c);
        }
        Input {
            mouse: mq::mouse_position(),
            wheel: mq::mouse_wheel(),
            buttons_pressed: BUTTONS
                .into_iter()
                .filter(|&b| mq::is_mouse_button_pressed(b))
                .collect(),
            buttons_down: BUTTONS
                .into_iter()
                .filter(|&b| mq::is_mouse_button_down(b))
                .collect(),
            keys_pressed: mq::get_keys_pressed(),
            keys_down: mq::get_keys_down(),
            chars,
        }
    }

    pub(crate) fn mouse_position(&self) -> (f32, f32) {
        self.mouse
    }

    pub(crate) fn mouse_wheel(&self) -> (f32, f32) {
        self.wheel
    }

    pub(crate) fn mouse_pressed(&self, button: MouseButton) -> bool {
        self.buttons_pressed.contains(&button)
    }

    pub(crate) fn mouse_down(&self, button: MouseButton) -> bool {
        self.buttons_down.contains(&button)
    }

    pub(crate) fn key_pressed(&self, key: KeyCode) -> bool {
        self.keys_pressed.contains(&key)
    }

    pub(crate) fn key_down(&self, key: KeyCode) -> bool {
        self.keys_down.contains(&key)
    }

    /// Some key went down this frame.
    pub(crate) fn any_key_pressed(&self) -> bool {
        !self.keys_pressed.is_empty()
    }

    /// The frame closes a notice: a click, Enter, Esc or Space.
    pub(crate) fn dismisses(&self) -> bool {
        self.mouse_pressed(MouseButton::Left)
            || self.mouse_pressed(MouseButton::Right)
            || self.key_pressed(KeyCode::Enter)
            || self.key_pressed(KeyCode::Escape)
            || self.key_pressed(KeyCode::Space)
    }

    /// The characters typed this frame, in order.
    pub(crate) fn typed(&self) -> impl Iterator<Item = char> + '_ {
        self.chars.iter().copied()
    }
}

/// Frames built by hand, for the tests of click flows (`docs/rust_coding_rules.md`, UI test).
#[cfg(test)]
impl Input {
    /// The frame in which this key goes down.
    pub(crate) fn key(key: KeyCode) -> Input {
        Input {
            keys_pressed: [key].into(),
            keys_down: [key].into(),
            ..Input::default()
        }
    }

    /// The frame in which this mouse button goes down at (x, y).
    pub(crate) fn click(button: MouseButton, x: f32, y: f32) -> Input {
        Input {
            mouse: (x, y),
            buttons_pressed: vec![button],
            buttons_down: vec![button],
            ..Input::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notice_closes_on_a_click_or_one_of_its_keys_only() {
        for frame in [
            Input::click(MouseButton::Left, 5.0, 5.0),
            Input::click(MouseButton::Right, 5.0, 5.0),
            Input::key(KeyCode::Enter),
            Input::key(KeyCode::Escape),
            Input::key(KeyCode::Space),
        ] {
            assert!(frame.dismisses());
        }
        assert!(!Input::default().dismisses());
        assert!(!Input::key(KeyCode::A).dismisses());
        assert!(!Input::click(MouseButton::Middle, 5.0, 5.0).dismisses());
    }

    #[test]
    fn a_built_frame_answers_every_asker_the_same() {
        let input = Input {
            mouse: (3.0, 4.0),
            buttons_pressed: vec![MouseButton::Left],
            keys_pressed: [KeyCode::Escape].into(),
            chars: vec!['a', 'b'],
            ..Input::default()
        };
        assert_eq!(input.mouse_position(), (3.0, 4.0));
        assert!(input.mouse_pressed(MouseButton::Left) && !input.mouse_pressed(MouseButton::Right));
        assert!(input.key_pressed(KeyCode::Escape) && input.any_key_pressed());
        assert_eq!(input.typed().collect::<String>(), "ab");
    }

    #[test]
    fn the_default_frame_has_no_input() {
        let input = Input::default();
        assert!(!input.key_pressed(KeyCode::Enter) && input.typed().next().is_none());
        assert_eq!(input.mouse_position(), (0.0, 0.0));
    }
}
