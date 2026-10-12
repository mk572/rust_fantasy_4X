//! The overland screen's frame, laid out like the original's 320 by 200 main screen.
//!
//! Everything is placed in the original's pixel coordinates and scaled by one factor so the
//! screen keeps its shape in any window: the map window (12 by 10 tiles at 240 by 180), the
//! menu bar above it, and the right-hand column with the minimap, gold and mana boxes, the
//! resource panels and the Next Turn button. The original pictures come from the local art
//! set; without it the same layout is drawn in flat colours.

use f4x_core::city::{PlayerId, Position};
use macroquad::prelude::*;

use crate::ui::GOLD;
use crate::view::{App, HUMAN};

pub const ORIGINAL_W: f32 = 320.0;
pub const ORIGINAL_H: f32 = 200.0;

/// Where the original's elements sit, in its 320 by 200 pixels.
pub mod spot {
    /// The window onto the map.
    pub const MAP: (f32, f32, f32, f32) = (0.0, 20.0, 240.0, 180.0);
    /// The minimap's hole in the frame.
    pub const MINIMAP: (f32, f32, f32, f32) = (251.0, 21.0, 58.0, 30.0);
    /// The gold and mana boxes below it (the numbers go inside).
    pub const GOLD_BOX: (f32, f32, f32, f32) = (243.0, 54.0, 36.0, 16.0);
    pub const MANA_BOX: (f32, f32, f32, f32) = (281.0, 54.0, 36.0, 16.0);
    /// The gold, food and mana panels (one picture, three thirds).
    pub const PANELS: (f32, f32) = (240.0, 76.0);
    /// The Next Turn button.
    pub const NEXT_TURN: (f32, f32) = (240.0, 173.0);
    /// The menu buttons: `main.lbx` entries 1 to 7 at these x positions, all at y = 4.
    pub const MENU_Y: f32 = 4.0;
    pub const MENU_X: [f32; 7] = [7.0, 48.0, 91.0, 143.0, 188.0, 231.0, 276.0];
}

/// Scale and offset that fit the original screen into the window.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub scale: f32,
    pub origin: Vec2,
}

impl Layout {
    pub fn new() -> Self {
        let (w, h) = (screen_width(), screen_height());
        let scale = (w / ORIGINAL_W).min(h / ORIGINAL_H).max(0.5);
        Self {
            scale,
            origin: vec2(
                (w - ORIGINAL_W * scale) / 2.0,
                (h - ORIGINAL_H * scale) / 2.0,
            ),
        }
    }

    /// A rectangle given in the original's pixels, in window pixels.
    pub fn rect(&self, (x, y, w, h): (f32, f32, f32, f32)) -> Rect {
        Rect::new(
            self.origin.x + x * self.scale,
            self.origin.y + y * self.scale,
            w * self.scale,
            h * self.scale,
        )
    }

    pub fn point(&self, x: f32, y: f32) -> Vec2 {
        vec2(
            self.origin.x + x * self.scale,
            self.origin.y + y * self.scale,
        )
    }
}

/// Text centred on `cx`, with a dark shadow so it reads on any background.
pub fn text_centered(text: &str, cx: f32, baseline: f32, size: f32, color: Color) {
    let w = measure_text(text, None, size as u16, 1.0).width;
    let x = cx - w / 2.0;
    draw_text(
        text,
        x + size * 0.06,
        baseline + size * 0.06,
        size,
        Color::new(0.0, 0.0, 0.0, 0.8),
    );
    draw_text(text, x, baseline, size, color);
}

impl App {
    pub(crate) fn art_picture(
        &mut self,
        lbx: &str,
        entry: usize,
        frame: usize,
    ) -> Option<Texture2D> {
        self.art.picture(lbx, entry, frame).cloned()
    }

    /// Draw a picture whose top-left is at the original's `(x, y)`.
    pub(crate) fn draw_pic(
        &mut self,
        lay: &Layout,
        lbx: &str,
        entry: usize,
        frame: usize,
        x: f32,
        y: f32,
    ) -> Option<Rect> {
        self.draw_pic_tinted(lay, lbx, entry, frame, x, y, WHITE)
    }

    /// `draw_pic` multiplied by a colour (a faded one shows a disabled control).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_pic_tinted(
        &mut self,
        lay: &Layout,
        lbx: &str,
        entry: usize,
        frame: usize,
        x: f32,
        y: f32,
        tint: Color,
    ) -> Option<Rect> {
        let tex = self.art_picture(lbx, entry, frame)?;
        let size = tex.size() * lay.scale;
        let at = lay.point(x, y);
        draw_texture_ex(
            &tex,
            at.x,
            at.y,
            tint,
            DrawTextureParams {
                dest_size: Some(size),
                ..Default::default()
            },
        );
        Some(Rect::new(at.x, at.y, size.x, size.y))
    }

    /// The frame, menu, side column and everything drawn over the map. Returns what the
    /// player clicked this frame.
    pub(crate) fn draw_frame(&mut self) -> FrameClick {
        let lay = Layout::new();
        let mut click = FrameClick::default();
        let s = lay.scale;
        let mouse: Vec2 = mouse_position().into();
        let pressed = is_mouse_button_pressed(MouseButton::Left);

        // The frame itself; flat panels when the art is missing.
        if self.draw_pic(&lay, "main", 0, 0, 0.0, 0.0).is_none() {
            let dark = Color::new(0.1, 0.09, 0.08, 1.0);
            draw_rectangle(0.0, 0.0, screen_width(), screen_height(), dark);
            let r = lay.rect(spot::MAP);
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.03, 0.03, 0.05, 1.0));
        }

        // The menu bar: Game, Spells, Armies, Cities, Magic, Info, Plane.
        for (i, &x) in spot::MENU_X.iter().enumerate() {
            let entry = i + 1;
            let hover = {
                let tex = self.art_picture("main", entry, 0);
                tex.map(|t| {
                    let at = lay.point(x, spot::MENU_Y);
                    Rect::new(at.x, at.y, t.width() * s, t.height() * s).contains(mouse)
                })
                .unwrap_or(false)
            };
            let frame = usize::from(hover && is_mouse_button_down(MouseButton::Left));
            if self
                .draw_pic(&lay, "main", entry, frame, x, spot::MENU_Y)
                .is_none()
            {
                let names = [
                    "Game", "Spells", "Armies", "Cities", "Magic", "Info", "Plane",
                ];
                let r = lay.rect((x, spot::MENU_Y, 38.0, 12.0));
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.45, 0.3, 0.12, 1.0));
                text_centered(names[i], r.x + r.w / 2.0, r.y + r.h * 0.75, 3.2 * s, GOLD);
                click.menu = click.menu.or_else(|| (hover && pressed).then_some(i));
            } else if hover && pressed {
                click.menu = Some(i);
            }
        }

        // Gold and mana boxes.
        let gold = self.game.player(HUMAN).map_or(0, |p| p.gold);
        let mana = self.game.player(HUMAN).map_or(0, |p| p.mana);
        let view = (lay.origin.x, lay.origin.y, s);
        let (g, m) = (spot::GOLD_BOX, spot::MANA_BOX);
        self.bm_text_outlined(
            view,
            1,
            &format!("{gold} GP"),
            g.0 + g.2 * 0.62,
            g.1 + g.3 * 0.72,
            Align::Center,
        );
        self.bm_text_outlined(
            view,
            1,
            &format!("{mana} MP"),
            m.0 + m.2 * 0.62,
            m.1 + m.3 * 0.72,
            Align::Center,
        );

        // Resource panels with this turn's income in each.
        let t = self.game.treasury(HUMAN);
        let income = self.game.income(HUMAN);
        let net_mana = income.split.mana - income.mana_upkeep;
        let (px, py) = spot::PANELS;
        self.draw_pic(&lay, "main", 34, 0, px, py);
        for (k, (value, label)) in [
            (t.net_gold(), "Gold"),
            (t.food_surplus - t.food_upkeep, "Food"),
            (net_mana, "Mana"),
        ]
        .into_iter()
        .enumerate()
        {
            self.bm_text_outlined(
                view,
                2,
                &format!("{value} {label}"),
                px + 40.0,
                py + 24.0 + k as f32 * 32.0,
                Align::Center,
            );
        }

        // Next Turn.
        let (nx, ny) = spot::NEXT_TURN;
        if let Some(r) = self.draw_pic(&lay, "main", 35, 0, nx, ny) {
            click.next_turn = pressed && r.contains(mouse);
        } else {
            let r = lay.rect((nx + 4.0, ny + 4.0, 72.0, 19.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.45, 0.3, 0.12, 1.0));
            text_centered("Next Turn", r.x + r.w / 2.0, r.y + r.h * 0.7, 5.0 * s, GOLD);
            click.next_turn = pressed && r.contains(mouse);
        }

        self.draw_minimap(&lay);
        click
    }

    /// A small map of everything explored, with cities in their owners' colours and the
    /// window onto the big map outlined.
    fn draw_minimap(&mut self, lay: &Layout) {
        let r = lay.rect(spot::MINIMAP);
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.05, 1.0));
        let Some(map) = self.game.map() else {
            return;
        };
        let (w, h) = (map.width() as f32, map.height() as f32);
        let (cw, ch) = (r.w / w, r.h / h);
        let grid = map.plane(self.plane);
        let content = &self.game.content();
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                if !self.game.is_explored(
                    HUMAN,
                    Position {
                        plane: self.plane,
                        x,
                        y,
                    },
                ) {
                    continue;
                }
                let class = grid
                    .get(x as i32, y as i32)
                    .and_then(|t| content.terrains.get(t))
                    .map(|d| d.class);
                let c = match class {
                    Some(f4x_core::content::TerrainClass::Ocean) => Color::new(0.1, 0.2, 0.6, 1.0),
                    Some(f4x_core::content::TerrainClass::Mountains) => {
                        Color::new(0.6, 0.6, 0.6, 1.0)
                    }
                    Some(f4x_core::content::TerrainClass::Desert) => Color::new(0.8, 0.7, 0.4, 1.0),
                    Some(f4x_core::content::TerrainClass::Tundra) => {
                        Color::new(0.85, 0.9, 0.95, 1.0)
                    }
                    Some(_) => Color::new(0.2, 0.5, 0.2, 1.0),
                    None => continue,
                };
                draw_rectangle(
                    r.x + x as f32 * cw,
                    r.y + y as f32 * ch,
                    cw.ceil(),
                    ch.ceil(),
                    c,
                );
            }
        }
        for (_, city, at) in self.game.economy().cities() {
            if at.plane == self.plane && self.game.is_explored(HUMAN, at) {
                let PlayerId(owner) = city.owner;
                draw_rectangle(
                    r.x + at.x as f32 * cw - cw,
                    r.y + at.y as f32 * ch - ch,
                    cw * 2.5,
                    ch * 2.5,
                    self.banner_color(owner),
                );
            }
        }
        // The window onto the big map.
        let (cols, rows) = self.visible_tiles();
        draw_rectangle_lines(
            r.x + self.cam.x * cw,
            r.y + self.cam.y * ch,
            (cols * cw).min(r.w),
            (rows * ch).min(r.h),
            1.5,
            WHITE,
        );
        // Clicking the minimap centres the map there.
        if is_mouse_button_pressed(MouseButton::Left) && r.contains(mouse_position().into()) {
            let (mx, my) = mouse_position();
            let (tx, ty) = ((mx - r.x) / cw, (my - r.y) / ch);
            self.cam = vec2(tx - cols / 2.0, ty - rows / 2.0);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

impl App {
    /// Bitmap text in white with a one-pixel black outline, as the original draws its
    /// gold, mana and income figures.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn bm_text_outlined(
        &mut self,
        view: (f32, f32, f32),
        style: usize,
        text: &str,
        x: f32,
        y: f32,
        align: Align,
    ) {
        let black = Color::new(0.0, 0.0, 0.0, 1.0);
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (1.0, 1.0)] {
            self.bm_text(view, style, black, text, x + dx, y + dy, align);
        }
        self.bm_text(view, style, WHITE, text, x, y, align);
    }

    /// Like [`App::bm_text`] but in one of the font's sixteen colour sets by number (the
    /// original's `Set_Font_Style` picks the set directly).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn bm_text_set(
        &mut self,
        view: (f32, f32, f32),
        style: usize,
        set: usize,
        text: &str,
        x: f32,
        y: f32,
        align: Align,
    ) -> f32 {
        let (ox, oy, s) = view;
        let w = self.art.text_width(style, text);
        let ax = match align {
            Align::Left => x,
            Align::Center => x - w / 2.0,
            Align::Right => x - w,
        };
        self.art
            .draw_bitmap_text(style, set, text, ox + ax * s, oy + y * s, s);
        w
    }

    /// Text in one of the original's bitmap fonts, placed at `(x, y)` in the original's
    /// pixels (top-left, centre or right edge by `align`) under a view of `(origin x, origin y,
    /// scale)`. Falls back to the plain font when the original fonts were not extracted.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn bm_text(
        &mut self,
        view: (f32, f32, f32),
        style: usize,
        color: Color,
        text: &str,
        x: f32,
        y: f32,
        align: Align,
    ) -> f32 {
        let (ox, oy, s) = view;
        if self.art.has_fonts() {
            let set = self.art.nearest_font_set(style, color);
            let w = self.art.text_width(style, text);
            let ax = match align {
                Align::Left => x,
                Align::Center => x - w / 2.0,
                Align::Right => x - w,
            };
            self.art
                .draw_bitmap_text(style, set, text, ox + ax * s, oy + y * s, s);
            w
        } else {
            let size = (self.art.text_height(style) + 1.0) * s;
            let w = measure_text(text, None, size as u16, 1.0).width / s;
            let ax = match align {
                Align::Left => x,
                Align::Center => x - w / 2.0,
                Align::Right => x - w,
            };
            draw_text(
                text,
                ox + ax * s,
                oy + (y + self.art.text_height(style)) * s,
                size,
                color,
            );
            w
        }
    }
}

/// What was clicked on the frame this frame.
#[derive(Debug, Default, Clone, Copy)]
pub struct FrameClick {
    /// The menu button's index: Game, Spells, Armies, Cities, Magic, Info, Plane.
    pub menu: Option<usize>,
    pub next_turn: bool,
}
