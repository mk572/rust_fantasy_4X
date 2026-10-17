//! The random events as the player sees them (task 68): the box that announces an event, the
//! name of the running world event under the minimap and the plague and boom marks on the city
//! screen. The box is the original's `Notify2` (`GENDRAW.c`): the green strip with the mascot of
//! the wizard's strongest realm on the left, the text, and the event's picture in a frame at
//! the right end. A click or a key closes it, and the next one follows.

use f4x_core::events::{compose, EventKind};
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::view::{App, HUMAN};

/// Where the mascot, the text and the box go for each realm (chaos, death, life, nature,
/// sorcery): the mascot's offset and the text's left (`notify2_mascot_x1`, `_y1`, `_text_x1`).
const MASCOT: [(f32, f32); 5] = [
    (-2.0, 2.0),
    (0.0, -20.0),
    (-22.0, -4.0),
    (8.0, -12.0),
    (2.0, -3.0),
];
const TEXT_X: [f32; 5] = [70.0, 73.0, 62.0, 68.0, 70.0];
const REALMS: [&str; 5] = ["chaos", "death", "life", "nature", "sorcery"];
/// The widest a line of the message goes.
const WIDTH: f32 = 175.0;
/// The top of the box (`Notify2(160, 70, ...)`).
const TOP: f32 = 70.0;

impl App {
    /// For screenshots and checks: make an event happen to the human wizard (`great_meteor`,
    /// `gift`, `plague`...). The reason it cannot is printed.
    pub fn open_test_event(&mut self, kind: &str) {
        let Some(kind) = EventKind::from_id(kind) else {
            eprintln!(
                "unknown event {kind}; one of {}",
                EventKind::ALL.map(|k| k.id()).join(", ")
            );
            return;
        };
        match self.game.cheat_event(kind, HUMAN) {
            Ok(events) => {
                self.note(events);
            }
            Err(why) => eprintln!("{why}"),
        }
    }

    /// The realm whose mascot stands in the box: the human's strongest book, life first and a
    /// later realm only when it has more.
    fn mascot_realm(&self) -> usize {
        let books = |i: usize| self.game.magic(HUMAN).map_or(0, |m| m.books_in(REALMS[i]));
        let mut best = 2;
        for next in [0, 1, 3, 4] {
            if books(best) < books(next) {
                best = next;
            }
        }
        best
    }

    /// The oldest event the player has not been told of, over the map until it is closed.
    /// Returns whether the box took the frame.
    pub(crate) fn event_popup_frame(&mut self) -> bool {
        let Some(notice) = self.event_notices.front().cloned() else {
            return false;
        };
        let Some(text) = compose(&self.event_texts, &notice) else {
            self.event_notices.pop_front();
            return true;
        };
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let realm = self.mascot_realm();
        let line = self.art.text_height(4) + 2.0;
        let lines = self.bm_wrap(4, &text, WIDTH);
        let widest = lines
            .iter()
            .map(|l| self.art.text_width(4, l))
            .fold(0.0f32, f32::max)
            .ceil();
        let (mascot_x, mascot_y) = MASCOT[realm];
        let x = if mascot_x < 0.0 { 6.0 - mascot_x } else { 6.0 };
        let right = TEXT_X[realm] + x + widest;
        self.clipped((0.0, 0.0, x + right + 12.0, 200.0), |app| {
            app.draw_pic(&lay, "resource", 40, 0, x, TOP);
            app.draw_pic(
                &lay,
                "resource",
                51 + realm,
                0,
                x + mascot_x - 1.0,
                TOP + mascot_y,
            );
        });
        let black = lay.rect((right + 2.0, TOP, 46.0, 56.0));
        draw_rectangle(black.x, black.y, black.w, black.h, BLACK);
        self.draw_pic(&lay, "resource", 42, 0, right + 1.0, TOP);
        self.draw_pic(
            &lay,
            "events",
            notice.kind.number() as usize - 1,
            0,
            right + 7.0,
            TOP + 8.0,
        );
        let lines: Vec<(f32, String)> = lines.into_iter().map(|l| (0.0, l)).collect();
        self.draw_box_text(view, &lines, x + TEXT_X[realm], TOP + 8.0, line);
        let close = is_mouse_button_pressed(MouseButton::Left)
            || is_mouse_button_pressed(MouseButton::Right)
            || is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::Space);
        if close {
            self.event_notices.pop_front();
        }
        true
    }

    /// The red box that announces the items of fallen heroes before the item screen
    /// (`Player_Process_Item_Pool`: message 67 of `MESSAGE.LBX`). `None` without the message;
    /// otherwise whether the box was closed this frame.
    pub(crate) fn hero_died_warning(&mut self) -> Option<bool> {
        let text = self
            .messages
            .iter()
            .find(|m| m.index == f4x_core::help::message::HERO_DIED)?
            .text
            .clone();
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        let lay = Layout::new();
        Some(self.warning_box(&lay, &text))
    }

    /// The name of the world event that is going on, centred under the minimap
    /// (`Main_Screen_Draw_Status_Window`).
    pub(crate) fn draw_world_event(&mut self, lay: &Layout) {
        let Some(kind) = self.game.events().world_event() else {
            return;
        };
        let (name, colour, y) = match kind {
            EventKind::ConjunctionSorcery => ("Conjunction", "event_sorcery", 157.0),
            EventKind::ConjunctionChaos => ("Conjunction", "event_chaos", 158.0),
            EventKind::ConjunctionNature => ("Conjunction", "event_nature", 158.0),
            EventKind::BadMoon => ("Bad Moon", "event_bad_moon", 158.0),
            EventKind::GoodMoon => ("Good Moon", "event_good_moon", 158.0),
            _ => ("Mana Short", "event_mana_short", 158.0),
        };
        let colour = self.art.minimap().named(colour, WHITE);
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let black = Color::new(0.0, 0.0, 0.0, 1.0);
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (1.0, 1.0)] {
            self.bm_text(view, 0, black, name, 280.0 + dx, y + dy, Align::Center);
        }
        self.bm_text(view, 0, colour, name, 280.0, y, Align::Center);
    }
}
