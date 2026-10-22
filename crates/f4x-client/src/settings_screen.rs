//! The Settings screen (task 89): the original's `Settings_Screen`, a page of check boxes in
//! four groups (Sound and Music, Messages, Combat, Display) reached from the save slots.
//! Pictures: `load` entries 11 (background), 4 (OK) and 13 to 20 (check boxes). A click on a
//! row flips its switch; OK, O or Escape leave, and the preferences are written either way.

use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::preferences::{Pref, FILE};
use crate::ui::{button, GOLD};
use crate::view::{App, Screen};

/// Where a switch sits, in the original's pixels: the check box's left edge and top.
/// Left column x=31, right column x=192; the label starts 16 pixels right of the box and
/// 2 below its top. The gaps between groups are the original's.
const ROWS: [(Pref, f32, f32); 20] = [
    (Pref::SoundEffects, 31.0, 52.0),
    (Pref::BackgroundMusic, 31.0, 63.0),
    (Pref::EventMusic, 31.0, 74.0),
    (Pref::CitySpellEvents, 31.0, 101.0),
    (Pref::OverlandSpellEvents, 31.0, 112.0),
    (Pref::SummoningEvents, 31.0, 123.0),
    (Pref::EndOfTurnSummary, 31.0, 134.0),
    (Pref::RazeCity, 31.0, 145.0),
    (Pref::RandomEvents, 31.0, 163.0),
    (Pref::EndOfTurnWait, 31.0, 174.0),
    (Pref::StrategicCombatOnly, 192.0, 52.0),
    (Pref::AutoUnitInformation, 192.0, 63.0),
    (Pref::MovementAnimations, 192.0, 74.0),
    (Pref::EnemyMoves, 192.0, 101.0),
    (Pref::EnemySpells, 192.0, 112.0),
    (Pref::SpellBookOrdering, 192.0, 123.0),
    (Pref::SpellAnimations, 192.0, 134.0),
    (Pref::ShowNodeOwners, 192.0, 145.0),
    (Pref::ExpandingHelp, 192.0, 156.0),
    (Pref::LeftClickMoves, 192.0, 167.0),
];

/// The group headings: text and place.
const HEADINGS: [(&str, f32, f32); 4] = [
    ("Sound and Music", 31.0, 42.0),
    ("Messages", 31.0, 92.0),
    ("Combat", 192.0, 42.0),
    ("Display", 192.0, 92.0),
];

/// OK's click area: left, top, right, bottom.
const OK_AREA: (f32, f32, f32, f32) = (265.0, 175.0, 306.0, 189.0);
/// The row's click width, from the box's left edge.
const ROW_WIDTH: f32 = 144.0;
const ROW_HEIGHT: f32 = 9.0;

impl App {
    pub(crate) fn settings_screen(&mut self) {
        clear_background(BLACK);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        if self.draw_pic(&lay, "load", 11, 0, 0.0, 0.0).is_none() {
            let r = lay.rect((0.0, 0.0, 320.0, 200.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.08, 0.07, 0.06, 1.0));
            self.bm_text(view, 5, GOLD, "Settings", 160.0, 20.0, Align::Center);
        }
        let mouse: Vec2 = mouse_position().into();
        let pressed = is_mouse_button_pressed(MouseButton::Left);

        for (i, &(pref, x, y)) in ROWS.iter().enumerate() {
            let on = self.prefs.get(pref);
            // The three box pictures take turns down the page; Movement Animations alone
            // uses the first, as in the original.
            let n = if pref == Pref::MovementAnimations {
                0
            } else {
                i % 3
            };
            let entry = if on { 13 + n } else { 16 + n };
            if self.draw_pic(&lay, "load", entry, 0, x, y).is_none() {
                let r = lay.rect((x, y, 9.0, 9.0));
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, GOLD);
                if on {
                    draw_rectangle(
                        r.x + r.w * 0.25,
                        r.y + r.h * 0.25,
                        r.w * 0.5,
                        r.h * 0.5,
                        GOLD,
                    );
                }
            }
            self.bm_text(view, 2, WHITE, pref.label(), x + 16.0, y + 2.0, Align::Left);
            if pressed && lay.rect((x, y, ROW_WIDTH, ROW_HEIGHT)).contains(mouse) {
                self.prefs.toggle(pref);
                // The spell book list starts over on its first page.
                if pref == Pref::SpellBookOrdering {
                    self.book_ui = Default::default();
                }
            }
        }
        for &(text, x, y) in &HEADINGS {
            self.bm_text(view, 3, GOLD, text, x, y, Align::Left);
        }

        let (l, t, r, b) = OK_AREA;
        let ok = lay.rect((l, t, r - l, b - t));
        let hover = ok.contains(mouse);
        let (dx, dy) = if hover {
            (267.0, 177.0)
        } else {
            (266.0, 176.0)
        };
        if self.draw_pic(&lay, "load", 4, 0, dx, dy).is_none() {
            let near = lay.rect((l, t, 41.0, 14.0));
            if button(near, "OK", true) {
                self.leave_settings();
            }
        }
        // The game's own settings (task 94), which the original keeps on the new-game screen.
        let game = lay.rect((31.0, 186.0, 60.0, 10.0));
        if button(game, "Game...", true) {
            self.save_preferences();
            self.screen = Screen::GameSettings;
            return;
        }
        if (pressed && hover)
            || is_key_pressed(KeyCode::O)
            || is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::Escape)
        {
            self.leave_settings();
        }
    }

    fn leave_settings(&mut self) {
        self.save_preferences();
        self.screen = Screen::Slots;
    }

    fn save_preferences(&mut self) {
        if let Err(e) = self.prefs.save(std::path::Path::new(FILE)) {
            self.log.push(format!("Could not save the settings: {e}"));
        }
    }
}
