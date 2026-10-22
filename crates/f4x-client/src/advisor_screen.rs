//! The Historian and Astrologer screens (task 71): `Powergraph_Screen` and `Status_Screen` in
//! `AdvsrScr.c`. Both are full pictures from `reload.lbx` (entry 0 and 1) with the wizards'
//! figures drawn over them; Esc or a click leaves.

use f4x_core::wizard::astrology::HISTORY_TURNS;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::view::{App, Screen, HUMAN};

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Where the Astrologer's picture is drawn.
const STATUS_AT: (f32, f32) = (20.0, 11.0);

impl App {
    /// The wizards the player knows of, and the player: the ones the two screens list.
    fn known_wizards(&self) -> Vec<f4x_core::player::PlayerId> {
        self.game
            .player_ids()
            .into_iter()
            .filter(|&id| self.game.is_wizard(id) && (id == HUMAN || self.game.have_met(HUMAN, id)))
            .collect()
    }

    fn leave_on_click_or_escape(&mut self) {
        if is_key_pressed(KeyCode::Escape)
            || is_mouse_button_pressed(MouseButton::Left)
            || is_mouse_button_pressed(MouseButton::Right)
        {
            self.screen = Screen::Map;
        }
    }

    /// "History Of Wizards Power": a line for each wizard met, one point to the turn.
    pub(crate) fn history_screen(&mut self) {
        clear_background(BLACK);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        if self.draw_pic(&lay, "reload", 0, 0, 0.0, 0.0).is_none() {
            let r = lay.rect((0.0, 0.0, 320.0, 200.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.08, 0.07, 0.06, 1.0));
        }
        let gold = Color::new(0.95, 0.85, 0.5, 1.0);
        self.bm_text(
            view,
            4,
            gold,
            "History Of Wizards Power",
            160.0,
            10.0,
            Align::Center,
        );
        let turn = self.game.turn as usize;
        let date = format!("{} {}", MONTHS[turn % 12], 1400 + turn / 12);
        self.bm_text(view, 2, WHITE, &date, 309.0, 11.0, Align::Right);

        // The time line, with a tick every year and the year written every fifth.
        let thick = lay.scale;
        let line = |a: (f32, f32), b: (f32, f32), c: Color| {
            let (p, q) = (lay.point(a.0, a.1), lay.point(b.0, b.1));
            draw_line(p.x, p.y, q.x, q.y, thick, c);
        };
        let grey = Color::new(0.6, 0.6, 0.65, 1.0);
        line((16.0, 183.0), (303.0, 183.0), grey);
        let kept = self
            .known_wizards()
            .iter()
            .map(|&id| self.game.player(id).map_or(0, |p| p.history.len()))
            .max()
            .unwrap_or(0)
            .min(HISTORY_TURNS);
        let first_turn = turn.saturating_sub(kept);
        for i in 0..kept {
            let t = first_turn + i;
            if t % 12 == 0 {
                let x = 16.0 + i as f32;
                line((x, 182.0), (x, 184.0), grey);
                let year = 1400 + t / 12;
                if year % 5 == 0 {
                    self.bm_text(
                        view,
                        1,
                        WHITE,
                        &year.to_string(),
                        x + 6.0,
                        186.0,
                        Align::Left,
                    );
                }
            }
        }

        // Each wizard's power, a point a turn, from the left edge.
        let wizards = self.known_wizards();
        for (row, &id) in wizards.iter().enumerate() {
            let Some((banner, history, name)) = self
                .game
                .wizard(id)
                .zip(self.game.player(id))
                .map(|(w, p)| (w.banner(), p.history.clone(), w.name()))
            else {
                continue;
            };
            let colour = self.art.banner_color(banner);
            let height = |v: u8| (181.0 - f32::from(v)).max(22.0);
            let mut last: Option<(f32, f32)> = None;
            for (i, &v) in history.iter().enumerate() {
                let at = (16.0 + i as f32, height(v));
                if let Some(prev) = last {
                    line(prev, at, colour);
                }
                last = Some(at);
            }
            self.bm_text(
                view,
                3,
                colour,
                &name,
                10.0,
                23.0 + row as f32 * 9.0,
                Align::Left,
            );
        }
        self.leave_on_click_or_escape();
    }

    /// "Current Status Of Wizards": bars of army strength, magic power and spell research.
    pub(crate) fn astrologer_screen(&mut self) {
        clear_background(BLACK);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (sx, sy) = STATUS_AT;
        if self.draw_pic(&lay, "reload", 1, 0, sx, sy).is_none() {
            let r = lay.rect((sx, sy, 277.0, 181.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.08, 0.07, 0.06, 1.0));
        }
        let gold = Color::new(0.95, 0.85, 0.5, 1.0);
        self.bm_text(
            view,
            4,
            gold,
            "Current Status Of Wizards",
            sx + 138.0,
            sy + 10.0,
            Align::Center,
        );
        for (i, text) in ["Army Strength", "Magic Power", "Spell Research"]
            .iter()
            .enumerate()
        {
            self.bm_text(
                view,
                2,
                WHITE,
                text,
                sx + 138.0,
                sy + 23.0 + 49.0 * i as f32,
                Align::Center,
            );
        }

        let table = self.game.astrology_table();
        let wizards = self.known_wizards();
        for (row, &id) in wizards.iter().enumerate() {
            let Some(&(_, a, _)) = table.iter().find(|(p, ..)| *p == id) else {
                continue;
            };
            let Some((name, banner)) = self.game.wizard(id).map(|w| (w.name(), w.banner())) else {
                continue;
            };
            let colour = self.art.banner_color(banner);
            for (section, score) in [a.strength, a.power, a.research].into_iter().enumerate() {
                let y = sy + 35.0 + 49.0 * section as f32 + row as f32 * 7.0;
                self.bm_text(view, 0, WHITE, &name, sx + 14.0, y, Align::Left);
                let bar = lay.rect((sx + 64.0, y + 1.0, score.clamp(0, 200) as f32 + 1.0, 2.0));
                draw_rectangle(bar.x, bar.y, bar.w, bar.h, colour);
                let shade = lay.rect((sx + 64.0, y + 3.0, score.clamp(0, 200) as f32 + 1.0, 1.0));
                draw_rectangle(
                    shade.x,
                    shade.y,
                    shade.w,
                    shade.h,
                    Color::new(0.0, 0.0, 0.0, 0.5),
                );
            }
        }
        self.leave_on_click_or_escape();
    }
}

/// Where the Cartographer's map starts, and the size of a square on it.
const CART_AT: (f32, f32) = (16.0, 27.0);
const CART_SQUARE: f32 = 4.0;

impl App {
    /// The Cartographer (F2): the whole of one plane, four pixels to the square, with the
    /// nodes, lairs and towers seen, the units and the cities. N flips the plane, a click on
    /// the map takes the main map there, C, Esc or the Close corner leave.
    pub(crate) fn cartographer_screen(&mut self) {
        use f4x_core::content::FeatureKind;
        use f4x_core::map::Plane;

        clear_background(BLACK);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        if self.draw_pic(&lay, "reload", 2, 0, 0.0, 0.0).is_none() {
            let r = lay.rect((0.0, 0.0, 320.0, 200.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.4, 0.3, 0.15, 1.0));
        }
        let plane = self.cart_plane;
        let (ox, oy) = CART_AT;
        let Some(map) = self.game.map() else {
            self.screen = Screen::Map;
            return;
        };
        let (w, h) = (map.width(), map.height());
        let grid = map.plane(plane);
        let content = self.game.content();
        let px = |x: u32, y: u32| (ox + x as f32 * CART_SQUARE, oy + y as f32 * CART_SQUARE);
        let square = |x: f32, y: f32, size: f32, c: Color| {
            let r = lay.rect((x, y, size, size));
            draw_rectangle(r.x, r.y, r.w, r.h, c);
        };
        let site = Color::new(0.8, 0.8, 0.8, 1.0);
        for y in 0..h {
            for x in 0..w {
                let at = f4x_core::map::Position { plane, x, y };
                if !self.game.is_explored(HUMAN, at) {
                    continue;
                }
                let Some(class) = grid
                    .get(x as i32, y as i32)
                    .and_then(|t| content.terrains.get(t))
                    .map(|d| d.class)
                else {
                    continue;
                };
                let (sx, sy) = px(x, y);
                square(sx, sy, CART_SQUARE, crate::frame::mini_terrain_color(class));
                let kind = grid
                    .feature(x as i32, y as i32)
                    .and_then(|f| content.features.get(f))
                    .map(|f| f.kind);
                let marked = match kind {
                    Some(FeatureKind::Node | FeatureKind::Tower) => true,
                    Some(FeatureKind::Encounter) => self.game.economy().lair_at(at).is_some(),
                    _ => false,
                };
                if marked {
                    square(sx + 2.0, sy + 1.0, 1.0, site);
                }
            }
        }
        // Units: the player's, and those of others where they can be seen.
        let seen = self.game.economy().visibility(HUMAN, plane);
        for u in self.game.units().all_units() {
            let (Some(at), Some(owner)) = (
                self.game.units().unit_position(u),
                self.game.units().unit_owner(u),
            ) else {
                continue;
            };
            let visible = seen
                .get((at.y * w + at.x) as usize)
                .copied()
                .unwrap_or(false);
            if at.plane != plane
                || !(owner == HUMAN || (visible && !self.game.is_neutral_player(owner)))
            {
                continue;
            }
            let banner = self.game.wizard(owner).map_or(5, |w| w.banner());
            let (sx, sy) = px(at.x, at.y);
            square(sx + 2.0, sy + 1.0, 1.0, self.art.banner_color(banner));
        }
        // Cities: a banner-coloured block with a black shadow, brown for independent ones.
        let mut hovered = None;
        let mouse: Vec2 = mouse_position().into();
        for (_, city, at) in self.game.economy().cities() {
            if at.plane != plane || !self.game.is_explored(HUMAN, at) {
                continue;
            }
            let (sx, sy) = px(at.x, at.y);
            let colour = if self.game.is_neutral_player(city.owner) {
                Color::new(0.5, 0.3, 0.1, 1.0)
            } else {
                self.art
                    .banner_color(self.game.wizard(city.owner).map_or(5, |w| w.banner()))
            };
            square(sx + 1.0, sy + 1.0, 3.0, BLACK);
            square(sx, sy, 3.0, colour);
            if lay.rect((sx, sy, CART_SQUARE, CART_SQUARE)).contains(mouse) {
                hovered = Some((sx, sy, city.name.clone()));
            }
        }
        if let Some((sx, sy, name)) = hovered {
            self.bm_text(view, 0, WHITE, &name, sx + 2.0, sy - 6.0, Align::Center);
        }

        let title = format!(
            "{} Plane",
            if plane == Plane::Arcanus {
                "Arcanus"
            } else {
                "Myrror"
            }
        );
        self.bm_text(
            view,
            5,
            Color::new(0.2, 0.1, 0.05, 1.0),
            &title,
            160.0,
            12.0,
            Align::Center,
        );
        // The legend: a flag and name for each wizard met, then the independents.
        let mut row = 0.0;
        for id in self.known_wizards() {
            let Some((banner, name)) = self.game.wizard(id).map(|w| (w.banner(), w.name())) else {
                continue;
            };
            self.draw_pic(
                &lay,
                "reload",
                3 + usize::from(banner),
                0,
                260.0,
                80.0 + row * 9.0,
            );
            self.bm_text(
                view,
                1,
                Color::new(0.2, 0.1, 0.05, 1.0),
                &name,
                269.0,
                82.0 + row * 9.0,
                Align::Left,
            );
            row += 1.0;
        }
        self.draw_pic(&lay, "reload", 8, 0, 260.0, 80.0 + row * 9.0);
        self.bm_text(
            view,
            1,
            Color::new(0.2, 0.1, 0.05, 1.0),
            "Neutral",
            269.0,
            82.0 + row * 9.0,
            Align::Left,
        );

        // Keys and clicks.
        let pressed = is_mouse_button_pressed(MouseButton::Left);
        let in_map = lay
            .rect((ox, oy, w as f32 * CART_SQUARE, h as f32 * CART_SQUARE))
            .contains(mouse);
        if is_key_pressed(KeyCode::N)
            || (pressed && lay.rect((285.0, 180.0, 29.0, 19.0)).contains(mouse))
        {
            self.cart_plane = if plane == Plane::Arcanus {
                Plane::Myrror
            } else {
                Plane::Arcanus
            };
        }
        if is_key_pressed(KeyCode::C)
            || is_key_pressed(KeyCode::Escape)
            || is_mouse_button_pressed(MouseButton::Right)
            || (pressed && lay.rect((265.0, 143.0, 34.0, 26.0)).contains(mouse))
        {
            self.screen = Screen::Map;
        } else if pressed && in_map {
            let p = lay.point(ox, oy);
            let sq = CART_SQUARE * lay.scale;
            let (tx, ty) = (
                ((mouse.x - p.x) / sq).floor(),
                ((mouse.y - p.y) / sq).floor(),
            );
            let (cols, rows) = self.visible_tiles();
            self.plane = plane;
            self.cam = vec2(tx - cols / 2.0, ty - rows / 2.0);
            self.screen = Screen::Map;
        }
    }
}
