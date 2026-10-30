//! A screen that owns its state (task 394). A [`Page`] is built when its screen opens, gets
//! one `frame` per frame with the whole [`App`] to draw and answer through. The `Screen` variant
//! still names which screen is up; the page is the state and the code of it, taken out of the
//! `App` for the frame as a modal is.
//!
//! A page outlives its screen being left: a screen that was opened over it (a unit sheet, the
//! console) returns with `app.screen = back`, and the page is still there. Opening the screen
//! again with `open_page` replaces it.

use std::mem::{discriminant, Discriminant};

use crate::audio::Track;
use crate::input::Input;
use crate::view::{App, Screen};

pub(crate) trait Page {
    /// The piece that plays while this screen is up, if it has one.
    fn music(&self, _app: &App) -> Option<Track> {
        None
    }

    /// The help topic of what lies at `(x, y)` (320 by 200 coordinates) on this screen that the
    /// generic help fields do not know, with its area.
    fn help_at(&self, _app: &App, _x: i32, _y: i32) -> Option<(u16, (f32, f32, f32, f32))> {
        None
    }

    /// The icon of the weapon the open unit sheet shows, which picks the melee help topic.
    fn melee_icon(&self) -> Option<usize> {
        None
    }

    /// One frame: draw the screen and take the input. Changing `app.screen` leaves it.
    fn frame(&mut self, app: &mut App, input: &Input);
}

/// The pages, by the kind of screen they belong to.
#[derive(Default)]
pub(crate) struct Pages(Vec<(Discriminant<Screen>, Box<dyn Page>)>);

impl Pages {
    pub(crate) fn get(&self, screen: &Screen) -> Option<&dyn Page> {
        let key = discriminant(screen);
        self.0.iter().find(|(k, _)| *k == key).map(|(_, p)| &**p)
    }

    fn take(&mut self, screen: &Screen) -> Option<Box<dyn Page>> {
        let key = discriminant(screen);
        let at = self.0.iter().position(|(k, _)| *k == key)?;
        Some(self.0.swap_remove(at).1)
    }

    /// Keep `page` for `screen`, replacing the one it had.
    fn put(&mut self, screen: &Screen, page: Box<dyn Page>) {
        self.take(screen);
        self.0.push((discriminant(screen), page));
    }
}

impl App {
    /// Show `screen`, run by `page`.
    pub(crate) fn open_page(&mut self, screen: Screen, page: impl Page + 'static) {
        self.pages.put(&screen, Box::new(page));
        self.screen = screen;
    }

    /// Go back to `screen`, whose page is kept if it was left under another screen (a talk, a
    /// sheet), else open it afresh with `open`.
    pub(crate) fn show_page(&mut self, screen: Screen, open: impl FnOnce(&mut App)) {
        if self.pages.get(&screen).is_some() {
            self.screen = screen;
        } else {
            open(self);
        }
    }

    /// One frame of the page of the open screen. A screen with no page (it was set without
    /// `open_page`) goes back to the map.
    pub(crate) fn page_frame(&mut self, input: &Input) {
        let shown = self.screen;
        let Some(mut page) = self.pages.take(&shown) else {
            self.screen = Screen::Map;
            return;
        };
        page.frame(self, input);
        // Unless the frame opened the same screen afresh, which put its own page.
        if self.pages.get(&shown).is_none() {
            self.pages.put(&shown, page);
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
    fn a_page_stays_for_its_screen_to_come_back_to() {
        let mut app = App::for_test();
        app.open_page(Screen::Wizard, Twice(0));
        app.page_frame(&Input::default());
        assert!(matches!(app.screen, Screen::Wizard));
        app.page_frame(&Input::default());
        assert!(matches!(app.screen, Screen::Map));
        // Back to the screen without opening it: the page is still there (without one the
        // screen would fall back to the map), and past its second frame it stays.
        app.screen = Screen::Wizard;
        app.page_frame(&Input::default());
        assert!(matches!(app.screen, Screen::Wizard));
        assert!(app.pages.get(&Screen::Wizard).is_some());
    }

    #[test]
    fn opening_a_screen_again_replaces_its_page() {
        let mut app = App::for_test();
        app.open_page(Screen::Wizard, Twice(1));
        app.open_page(Screen::Wizard, Twice(0));
        app.page_frame(&Input::default());
        // A fresh page: one frame is not its second.
        assert!(matches!(app.screen, Screen::Wizard));
    }

    #[test]
    fn a_screen_without_a_page_goes_back_to_the_map() {
        let mut app = App::for_test();
        app.screen = Screen::HallOfFame;
        app.page_frame(&Input::default());
        assert!(matches!(app.screen, Screen::Map));
    }
}
