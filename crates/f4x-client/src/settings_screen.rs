//! The Settings screen: the original's `Settings_Screen`, a page of check boxes in
//! four groups (Sound and Music, Messages, Combat, Display) reached from the save slots.
//! Pictures: `load` entries 11 (background), 4 (OK) and 13 to 20 (check boxes). A click on a
//! row flips its switch; OK, O or Escape leave, and the preferences are written either way.
//!
//! Tests: below. Which switch a click flips is `clicked_switch`, what a press on a slider sets is
//! `volume_at`, what leaves the screen is `leaves`; the drawing is left to the screenshots.

use crate::font::Style;
use crate::input::Input;
use crate::page::Page;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::preferences::{Pref, FILE};
use crate::ui::{self, button, GOLD};
use crate::view::{App, Screen};

/// Where a switch sits, in the original's pixels: the check box's left edge and top.
/// Left column x=31, right column x=192; the label starts 16 pixels right of the box and
/// 2 below its top. The gaps between groups are the original's.
const ROWS: [(Pref, f32, f32); 19] = [
    (Pref::SoundEffects, 31.0, 52.0),
    (Pref::BackgroundMusic, 31.0, 63.0),
    (Pref::EventMusic, 31.0, 74.0),
    (Pref::CitySpellEvents, 31.0, 101.0),
    (Pref::OverlandSpellEvents, 31.0, 112.0),
    (Pref::SummoningEvents, 31.0, 123.0),
    (Pref::EndOfTurnSummary, 31.0, 134.0),
    (Pref::RazeCity, 31.0, 145.0),
    (Pref::PlayIntro, 31.0, 154.0),
    (Pref::RandomEvents, 31.0, 163.0),
    (Pref::EndOfTurnWait, 31.0, 174.0),
    (Pref::StrategicCombatOnly, 192.0, 52.0),
    (Pref::AutoUnitInformation, 192.0, 63.0),
    (Pref::EnemyMoves, 192.0, 101.0),
    (Pref::EnemySpells, 192.0, 112.0),
    (Pref::SpellAnimations, 192.0, 123.0),
    (Pref::ExpandingHelp, 192.0, 134.0),
    (Pref::LeftClickMoves, 192.0, 145.0),
    (Pref::DimBehindBoxes, 192.0, 156.0),
];

/// The group headings: text and place.
const HEADINGS: [(&str, f32, f32); 4] = [
    ("settings.sound", 31.0, 42.0),
    ("settings.messages", 31.0, 92.0),
    ("settings.combat", 192.0, 42.0),
    ("settings.display", 192.0, 92.0),
];

/// The volume sliders (not in the original): label x, bar x and width, and the row's top.
const SLIDERS: [(&str, f32, f32, f32); 2] = [
    ("settings.music_volume", 100.0, 126.0, 40.0),
    ("settings.effects_volume", 172.0, 209.0, 46.0),
];
const SLIDER_Y: f32 = 187.0;

/// OK's click area: left, top, right, bottom.
const OK_AREA: (f32, f32, f32, f32) = (265.0, 175.0, 306.0, 189.0);
/// The row's click width, from the box's left edge.
const ROW_WIDTH: f32 = 144.0;
const ROW_HEIGHT: f32 = 9.0;

/// The switch whose row the left button went down on this frame.
fn clicked_switch(lay: &Layout, input: &Input) -> Option<Pref> {
    ROWS.iter()
        .find(|&&(_, x, y)| ui::hit(lay.rect((x, y, ROW_WIDTH, ROW_HEIGHT)), true, input).clicked())
        .map(|&(pref, _, _)| pref)
}

/// The volume (0 to 100) a press at `mouse_x` sets on a bar that starts at `bar_x` and is
/// `bar_w` wide, all in window pixels.
fn volume_at(mouse_x: f32, bar_x: f32, bar_w: f32) -> u8 {
    (((mouse_x - bar_x) / bar_w) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8
}

/// OK (clicked), O, Enter or Escape leave the screen.
fn leaves(ok_clicked: bool, input: &Input) -> bool {
    ok_clicked
        || input.key_pressed(KeyCode::O)
        || input.key_pressed(KeyCode::Enter)
        || input.key_pressed(KeyCode::Escape)
}

/// The preferences screen keeps nothing between frames: the preferences are the `App`'s.
struct SettingsPage;

impl Page for SettingsPage {
    fn frame(&mut self, app: &mut App, input: &Input) {
        clear_background(BLACK);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        if app.draw_pic(&lay, "load", 11, 0, 0.0, 0.0).is_none() {
            let r = lay.rect(crate::resolution::SCREEN);
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.08, 0.07, 0.06, 1.0));
            let title = app.text("options.settings");
            app.draw_styled(view, &Style::nearest(5, GOLD).center(), &title, 160.0, 20.0);
        }
        let mouse: Vec2 = input.mouse_position().into();

        for (i, &(pref, x, y)) in ROWS.iter().enumerate() {
            let on = app.prefs.get(pref);
            // The three box pictures take turns down the page.
            let n = i % 3;
            let entry = if on { 13 + n } else { 16 + n };
            if app.draw_pic(&lay, "load", entry, 0, x, y).is_none() {
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
            let label = app.text(pref.label());
            app.draw_styled(view, &Style::nearest(2, WHITE), &label, x + 16.0, y + 2.0);
        }
        if let Some(pref) = clicked_switch(&lay, input) {
            app.prefs.toggle(pref);
        }
        for &(key, x, y) in &HEADINGS {
            let text = app.text(key);
            app.draw_styled(view, &Style::nearest(3, GOLD), &text, x, y);
        }

        let (l, t, r, b) = OK_AREA;
        let ok = lay.rect((l, t, r - l, b - t));
        let ok_hit = ui::hit(ok, true, input);
        let hover = ok_hit.hovered();
        let (dx, dy) = if hover {
            (267.0, 177.0)
        } else {
            (266.0, 176.0)
        };
        if app.draw_pic(&lay, "load", 4, 0, dx, dy).is_none() {
            let near = lay.rect((l, t, 41.0, 14.0));
            if button(near, "OK", true, input) {
                app.leave_settings();
            }
        }
        app.volume_sliders(&lay, view, mouse, input);
        // The game's own settings, which the original keeps on the new-game screen.
        let game = lay.rect((31.0, 186.0, 60.0, 10.0));
        if button(game, &app.text("settings.game"), true, input) {
            app.save_preferences();
            app.open_page(
                Screen::GameSettings,
                crate::options_screen::GameSettingsPage,
            );
            return;
        }
        if leaves(ok_hit.clicked(), input) {
            app.leave_settings();
        }
    }
}

impl App {
    pub(crate) fn open_settings(&mut self) {
        self.open_page(Screen::Settings, SettingsPage);
    }

    /// The two volume sliders in the bottom row: a press or a drag on a bar sets that volume.
    fn volume_sliders(&mut self, lay: &Layout, view: (f32, f32, f32), mouse: Vec2, input: &Input) {
        for (i, &(key, label_x, bar_x, bar_w)) in SLIDERS.iter().enumerate() {
            let label = self.text(key);
            self.draw_styled(
                view,
                &Style::nearest(2, WHITE),
                &label,
                label_x,
                SLIDER_Y + 1.0,
            );
            let bar = lay.rect((bar_x, SLIDER_Y + 2.0, bar_w, 5.0));
            let hit = lay.rect((bar_x - 2.0, SLIDER_Y - 1.0, bar_w + 4.0, 11.0));
            let volume = if i == 0 {
                &mut self.prefs.music_volume
            } else {
                &mut self.prefs.effects_volume
            };
            if ui::hit(hit, true, input).held() {
                *volume = volume_at(mouse.x, bar.x, bar.w);
            }
            let fraction = f32::from(*volume) / 100.0;
            draw_rectangle(bar.x, bar.y, bar.w, bar.h, Color::new(0.0, 0.0, 0.0, 0.7));
            draw_rectangle(bar.x, bar.y, bar.w * fraction, bar.h, GOLD);
            draw_rectangle_lines(
                bar.x,
                bar.y,
                bar.w,
                bar.h,
                1.0,
                Color::new(0.8, 0.65, 0.3, 1.0),
            );
        }
    }

    fn leave_settings(&mut self) {
        self.save_preferences();
        self.show_page(Screen::Slots, |app| {
            app.open_page(Screen::Slots, crate::menu_screen::SlotsPage)
        });
    }

    fn save_preferences(&mut self) {
        if let Err(e) = self.prefs.save(std::path::Path::new(FILE)) {
            let args = Args::new().text("why", e.to_string());
            self.log.push(self.text_with("settings.save_failed", &args));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_on_each_row_picks_that_switch_and_no_other() {
        let lay = Layout::new();
        for &(pref, x, y) in &ROWS {
            let at = lay.rect((x + 5.0, y + ROW_HEIGHT / 2.0, 0.0, 0.0));
            let input = Input::click(MouseButton::Left, at.x, at.y);
            assert_eq!(clicked_switch(&lay, &input), Some(pref), "{pref:?}");
        }
    }

    #[test]
    fn a_click_beside_the_rows_or_a_right_click_flips_nothing() {
        let lay = Layout::new();
        let off = lay.rect((300.0, 100.0, 0.0, 0.0));
        assert_eq!(
            clicked_switch(&lay, &Input::click(MouseButton::Left, off.x, off.y)),
            None
        );
        let on_row = lay.rect((ROWS[0].1 + 5.0, ROWS[0].2 + 4.0, 0.0, 0.0));
        assert_eq!(
            clicked_switch(&lay, &Input::click(MouseButton::Right, on_row.x, on_row.y)),
            None
        );
    }

    #[test]
    fn the_rows_do_not_overlap() {
        for (i, a) in ROWS.iter().enumerate() {
            for b in &ROWS[i + 1..] {
                let apart = a.1 + ROW_WIDTH <= b.1
                    || b.1 + ROW_WIDTH <= a.1
                    || a.2 + ROW_HEIGHT <= b.2
                    || b.2 + ROW_HEIGHT <= a.2;
                assert!(apart, "{:?} and {:?}", a.0, b.0);
            }
        }
    }

    #[test]
    fn a_press_on_a_bar_sets_the_volume_by_how_far_along_it_is() {
        assert_eq!(volume_at(100.0, 100.0, 40.0), 0);
        assert_eq!(volume_at(120.0, 100.0, 40.0), 50);
        assert_eq!(volume_at(140.0, 100.0, 40.0), 100);
        assert_eq!(volume_at(60.0, 100.0, 40.0), 0, "left of the bar");
        assert_eq!(volume_at(500.0, 100.0, 40.0), 100, "right of the bar");
    }

    #[test]
    fn ok_o_enter_and_escape_leave_and_other_keys_do_not() {
        assert!(leaves(true, &Input::default()));
        for key in [KeyCode::O, KeyCode::Enter, KeyCode::Escape] {
            assert!(leaves(false, &Input::key(key)), "{key:?}");
        }
        assert!(!leaves(false, &Input::key(KeyCode::A)));
        assert!(!leaves(false, &Input::default()));
    }
}
