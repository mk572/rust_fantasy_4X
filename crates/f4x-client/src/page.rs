//! A screen that owns its state (task 394). A [`Page`] is built when its screen opens, gets
//! one `frame` per frame with the whole [`App`] to draw and answer through, and is dropped when
//! the screen changes. The `Screen` variant still names which screen is up; the page is the
//! state and the code of it, taken out of the `App` for the frame as a modal is.

use crate::input::Input;
use crate::view::{App, Screen};

pub(crate) trait Page {
    /// One frame: draw the screen and take the input. Changing `app.screen` leaves it.
    fn frame(&mut self, app: &mut App, input: &Input);
}

impl App {
    /// Show `screen`, run by `page`.
    pub(crate) fn open_page(&mut self, screen: Screen, page: impl Page + 'static) {
        self.screen = screen;
        self.page = Some(Box::new(page));
    }

    /// One frame of the open page. A screen with no page (it was set without `open_page`)
    /// goes back to the map.
    pub(crate) fn page_frame(&mut self, input: &Input) {
        let Some(mut page) = self.page.take() else {
            self.screen = Screen::Map;
            return;
        };
        let before = std::mem::discriminant(&self.screen);
        page.frame(self, input);
        // Kept only if the screen is still the same one and no other page was opened meanwhile.
        if self.page.is_none() && std::mem::discriminant(&self.screen) == before {
            self.page = Some(page);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Counts its frames and leaves on the second.
    struct Twice(u8);

    impl Page for Twice {
        fn frame(&mut self, app: &mut App, _input: &Input) {
            self.0 += 1;
            if self.0 == 2 {
                app.screen = Screen::Map;
            }
        }
    }

    #[test]
    fn a_page_lives_while_its_screen_is_up_and_goes_when_the_screen_changes() {
        let mut app = App::for_test();
        app.open_page(Screen::Wizard, Twice(0));
        app.page_frame(&Input::default());
        assert!(app.page.is_some());
        app.page_frame(&Input::default());
        assert!(app.page.is_none());
        assert!(matches!(app.screen, Screen::Map));
    }

    #[test]
    fn a_screen_without_a_page_goes_back_to_the_map() {
        let mut app = App::for_test();
        app.screen = Screen::HallOfFame;
        app.page_frame(&Input::default());
        assert!(matches!(app.screen, Screen::Map));
    }
}
