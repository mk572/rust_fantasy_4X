//! The box that announces a hero's new level (`Hero_LevelUp_Popup` in ReMoM `LVLMAKE.c`): the
//! hero's portrait, "X has made a level." and what the level gives, over the map, with the
//! fanfare (an event piece). The original checks at each turn change whether the level its
//! experience gives is higher than the one it last noted; so does this, from the level each of the
//! human's heroes had at the last turn. A click or a key closes it.

use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::unit_sheet::special_picture;
use crate::view::{App, HUMAN};

/// Where the box starts (`lvlmake_base_x`, `lvlmake_base_y`) and its width.
const BASE: (f32, f32) = (31.0, 41.0);
const WIDTH: f32 = 251.0;
/// The portrait's frame in the box: a hero's portrait fills it.
const FRAME: (f32, f32) = (33.0, 32.0);
/// The most lines of what the hero gets.
const MOST: usize = 8;
/// Height of a row of two lines and of the part above them.
const ROW: f32 = 17.0;
const HEAD: f32 = 49.0;

impl App {
    /// Note each hero's level at a new turn, and queue a box for each that has gone up. The first
    /// look at a game only notes the levels.
    pub(crate) fn check_hero_levels(&mut self) {
        if self.game.turn == self.levels_turn {
            return;
        }
        let first = self.levels_turn == u32::MAX;
        self.levels_turn = self.game.turn;
        for hero in self.game.units().hero_units(HUMAN) {
            let level = self.game.units().unit_xp_level(hero);
            let before = self.hero_levels.insert(hero, level);
            if !first && before.is_some_and(|b| level > b) {
                self.level_ups.push_back(hero);
            }
        }
    }

    /// For screenshots and checks: announce the first unit of the human's as if it had made a level.
    pub fn open_test_level_up(&mut self) {
        let unit = self
            .game
            .units()
            .hero_units(HUMAN)
            .first()
            .copied()
            .or_else(|| self.game.units().player_units(HUMAN).first().copied());
        if let Some(unit) = unit {
            self.level_ups.push_back(unit);
        }
    }

    /// A unit that is no hero has no portrait: its map sprite, a half larger, centred in the
    /// portrait's frame.
    fn draw_sprite_in_frame(&mut self, lay: &Layout, unit: hecs::Entity, at: (f32, f32)) {
        let Some((lbx, entry)) = self.game.units().unit_def(unit).and_then(|d| d.art.clone())
        else {
            return;
        };
        let banner = self.banner_index(HUMAN.0).min(5);
        let Some(tex) = self.art.unit_picture(&lbx, entry as usize, banner) else {
            return;
        };
        let size = tex.size() * 1.5;
        let to = lay.point(
            at.0 + (FRAME.0 - size.x) / 2.0,
            at.1 + (FRAME.1 - size.y) / 2.0,
        );
        draw_texture_ex(
            tex,
            to.x,
            to.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(size * lay.scale),
                ..Default::default()
            },
        );
    }

    /// The oldest announcement not yet read. Returns whether the box took the frame.
    pub(crate) fn level_up_frame(&mut self) -> bool {
        let Some(&unit) = self.level_ups.front() else {
            return false;
        };
        let Some(sheet) = self.overland_sheet(unit) else {
            self.level_ups.pop_front();
            return true;
        };
        let level = self.game.units().unit_xp_level(unit);
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (bx, by) = BASE;
        let lines: Vec<_> = sheet
            .specials
            .iter()
            .filter(|s| s.spell.is_none())
            .take(MOST)
            .collect();
        let height = HEAD + (lines.len() as f32 / 2.0).ceil() * ROW;
        self.clipped((bx, by, WIDTH, height), |app| {
            app.draw_pic(&lay, "reload", 23, 0, bx, by);
        });
        self.draw_pic(&lay, "reload", 24, 0, bx, by + height);
        let portrait = self
            .game
            .units()
            .unit_def(unit)
            .and_then(|d| d.hero.as_ref().map(|h| h.portrait as usize));
        if let Some(p) = portrait {
            self.draw_pic(&lay, "portrait", p, 0, bx + 10.0, by + 10.0);
        } else {
            self.draw_sprite_in_frame(&lay, unit, (bx + 10.0, by + 10.0));
        }
        let gold = Color::new(0.95, 0.75, 0.25, 1.0);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let title = self.text_with(
            "levelup.title",
            &Args::new().text("name", sheet.name.clone()),
        );
        self.bm_text(view, 4, gold, &title, bx + 48.0, by + 9.0, Align::Left);
        let mut gains = vec![
            ("levelup.hit_points", bx + 47.0, by + 24.0),
            ("levelup.resistance", bx + 47.0, by + 34.0),
            ("levelup.attack", bx + 139.0, by + 24.0),
        ];
        // The levels the original gives a point of defense at: its 1, 3, 5 and 7th, counting from 1.
        let defense = (1..=7).contains(&level) && (level - 1).is_multiple_of(2);
        if defense {
            gains.push(("levelup.defense", bx + 139.0, by + 34.0));
        }
        for (key, x, y) in gains {
            self.draw_pic(&lay, "itemisc", 26, 0, x, y + 1.0);
            let text = self.text_with(key, &Args::new().count("n", 1));
            self.bm_text(view, 2, light, &text, x + 8.0, y, Align::Left);
        }
        let (mut x, mut y) = (bx + 13.0, by + 48.0);
        for (i, line) in lines.iter().enumerate() {
            if let Some(icon) = line.icon {
                let (lbx, entry) = special_picture(icon);
                self.draw_pic(&lay, lbx, entry, 0, x, y);
            }
            self.bm_text(view, 2, light, &line.name, x + 19.0, y + 5.0, Align::Left);
            // Two to a row: the first on the right of the first row, then left and right in turn.
            x = bx + 13.0;
            if i % 2 == 0 {
                x += 115.0;
            } else {
                y += ROW;
            }
        }
        let close = is_mouse_button_pressed(MouseButton::Left)
            || is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::Space);
        if close {
            self.level_ups.pop_front();
        }
        true
    }
}

// Eight lines are four rows: 49 + 4 * 17 = 117, inside the 128 pixel picture.
const _: () = assert!(HEAD + 4.0 * ROW <= 128.0);
