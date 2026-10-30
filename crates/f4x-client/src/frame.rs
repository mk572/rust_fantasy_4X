//! The overland screen's frame, laid out like the original's 320 by 200 main screen.
//!
//! Everything is placed in the original's pixel coordinates and scaled by one factor so the
//! screen keeps its shape in any window: the map window (12 by 10 tiles at 240 by 180), the
//! menu bar above it, and the right-hand column with the minimap, gold and mana boxes, the
//! resource panels and the Next Turn button. The original pictures come from the local art
//! set; without it the same layout is drawn in flat colours.

use crate::font::Style;
use crate::input::Input;
use f4x_core::map::Position;
use f4x_core::text::Args;
use f4x_data::spell_art::Role;
use macroquad::prelude::*;

use crate::ui::{self, GOLD};
use crate::view::{App, HUMAN};

pub use crate::resolution::{DESIGN_H as ORIGINAL_H, DESIGN_W as ORIGINAL_W};

/// Where the original's elements sit, in its 320 by 200 pixels.
pub mod spot {
    /// The window onto the map.
    pub const MAP: (f32, f32, f32, f32) = (0.0, 20.0, 240.0, 180.0);
    /// The minimap's hole in the frame.
    pub const MINIMAP: (f32, f32, f32, f32) = (251.0, 21.0, 58.0, 30.0);
    /// The gold and mana boxes below it (the numbers go inside).
    /// The gold, food and mana panels (one picture, three thirds).
    pub const PANELS: (f32, f32) = (240.0, 76.0);
    /// The Next Turn button.
    pub const NEXT_TURN: (f32, f32) = (240.0, 173.0);
    /// The menu buttons: `main.lbx` entries 1 to 7 at these x positions, all at y = 4.
    pub const MENU_Y: f32 = 4.0;
    pub const MENU_X: [f32; 7] = [7.0, 47.0, 89.0, 140.0, 184.0, 226.0, 270.0];
}

/// Scale and offset that fit the original screen into the window.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub scale: f32,
    pub origin: Vec2,
}

/// How a picture button looks when it is disabled.
#[derive(Clone, Copy)]
pub(crate) enum Disabled {
    /// The picture faded to this opacity.
    Fade(f32),
    /// This frame of the picture (the original's greyed second frame).
    Frame(usize),
}

/// The window's size; the tests, which have no window, get a fixed one.
pub fn window_size() -> (f32, f32) {
    #[cfg(test)]
    return (1280.0, 720.0);
    #[cfg(not(test))]
    (screen_width(), screen_height())
}

impl Layout {
    pub fn new() -> Self {
        let (w, h) = window_size();
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

    /// Draw the picture for `role` that belongs to no one spell (a box, the sparkle).
    pub(crate) fn draw_shared_pic(
        &mut self,
        lay: &Layout,
        role: &Role,
        frame: usize,
        x: f32,
        y: f32,
    ) -> Option<Rect> {
        let (lbx, entry) = self.art.shared_picture(role)?;
        self.draw_pic(lay, &lbx, entry, frame, x, y)
    }

    /// A picture button that is always enabled (`pic_button` without a disabled look).
    pub(crate) fn pic_button_on(
        &mut self,
        lay: &Layout,
        pic: (&str, usize),
        at: (f32, f32),
        input: &Input,
    ) -> bool {
        self.pic_button(lay, pic, at, (true, Disabled::Frame(0)), input)
    }

    /// A picture button of the original: frame 0 at rest, frame 1 while the left button is held
    /// on it, and `disabled` when it is not enabled. True on the frame it is clicked.
    pub(crate) fn pic_button(
        &mut self,
        lay: &Layout,
        (lbx, entry): (&str, usize),
        (x, y): (f32, f32),
        (enabled, disabled): (bool, Disabled),
        input: &Input,
    ) -> bool {
        let Some(tex) = self.art_picture(lbx, entry, 0) else {
            return false;
        };
        let state = ui::hit(
            lay.rect((
                x,
                y,
                crate::resolution::logical_w(&tex),
                crate::resolution::logical_h(&tex),
            )),
            enabled,
            input,
        );
        let (mut frame, mut tint) = (usize::from(state.held()), WHITE);
        match (enabled, disabled) {
            (false, Disabled::Frame(f)) => frame = f,
            (false, Disabled::Fade(alpha)) => tint = Color::new(1.0, 1.0, 1.0, alpha),
            _ => {}
        }
        if self.art_picture(lbx, entry, frame).is_none() {
            frame = 0;
        }
        self.draw_pic_tinted(lay, lbx, entry, frame, x, y, tint);
        state.clicked()
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
        let size = crate::resolution::logical_size(&tex) * lay.scale;
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
        let cycle = crate::cycle::for_file(lbx);
        if let Some((mask, cycle)) =
            cycle.and_then(|c| Some((self.art.cycle_mask(lbx, entry, frame)?, c)))
        {
            let mut colour = cycle.colour(get_time());
            colour.a = tint.a;
            draw_texture_ex(
                mask,
                at.x,
                at.y,
                colour,
                DrawTextureParams {
                    dest_size: Some(size),
                    ..Default::default()
                },
            );
        }
        Some(Rect::new(at.x, at.y, size.x, size.y))
    }

    /// The frame, menu, side column and everything drawn over the map. Returns what the
    /// player clicked this frame.
    pub(crate) fn draw_frame(&mut self, input: &Input) -> FrameClick {
        let lay = Layout::new();
        let mut click = FrameClick::default();
        let s = lay.scale;

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
            let state = self
                .art_picture("main", entry, 0)
                .map_or(ui::Hit::None, |t| {
                    let at = lay.point(x, spot::MENU_Y);
                    let rect = Rect::new(
                        at.x,
                        at.y,
                        crate::resolution::logical_w(&t) * s,
                        crate::resolution::logical_h(&t) * s,
                    );
                    ui::hit(rect, true, input)
                });
            let frame = usize::from(state.held());
            if self
                .draw_pic(&lay, "main", entry, frame, x, spot::MENU_Y)
                .is_none()
            {
                let names = [
                    self.text("ui.menu_game"),
                    self.text("ui.menu_spells"),
                    self.text("ui.menu_armies"),
                    self.text("ui.menu_cities"),
                    self.text("ui.menu_magic"),
                    self.text("ui.menu_info"),
                    self.text("ui.menu_plane"),
                ];
                let r = lay.rect((x, spot::MENU_Y, 38.0, 12.0));
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.45, 0.3, 0.12, 1.0));
                text_centered(&names[i], r.x + r.w / 2.0, r.y + r.h * 0.75, 3.2 * s, GOLD);
                click.menu = click.menu.or_else(|| state.clicked().then_some(i));
            } else if state.clicked() {
                click.menu = Some(i);
            }
        }

        // Gold and mana in the boxes under the minimap (`Main_Screen_Draw_Status_Window`): the
        // figures right-aligned in white, the units after them; the narrow font when a figure
        // has five digits.
        let gold = self.game.player(HUMAN).map_or(0, |p| p.treasury().gold);
        let mana = self.game.player(HUMAN).map_or(0, |p| p.treasury().mana);
        let view = (lay.origin.x, lay.origin.y, s);
        let style = if gold > 9999 || mana > 9999 { 0 } else { 1 };
        for (value, right, unit) in [(gold, 265.0, "GP"), (mana, 303.0, "MP")] {
            self.draw_styled(
                view,
                &Style::flat(style, WHITE).shadow().right(),
                &value.to_string(),
                right,
                68.0,
            );
            self.draw_styled(
                view,
                &Style::flat(0, WHITE).shadow(),
                unit,
                right + 3.0,
                68.0,
            );
        }

        self.draw_world_event(&lay);
        if self.selection.is_empty() {
            self.draw_summary(&lay, &mut click, input);
        } else {
            self.draw_unit_window(&lay, &mut click, input);
        }

        self.draw_minimap(&lay, input);
        click
    }

    /// One income figure in the summary column (`Main_Screen_Draw_Summary_Window`): the number
    /// right-aligned at 273, the word at 278, in the outlined narrow font, colour set 4 (dark gold); a loss pulses
    /// red with the palette cycle.
    fn income_text(&mut self, view: (f32, f32, f32), value: i32, label: &str, y: f32) {
        let colour = (value < 0).then(|| crate::cycle::MAIN.colour(get_time()));
        let black = Color::new(0.0, 0.0, 0.0, 1.0);
        let number = value.to_string();
        let (ox, oy, s) = view;
        for (text, x, align) in [
            (number.as_str(), 273.0, Align::Right),
            (label, 278.0, Align::Left),
        ] {
            let width = self.art.text_width(0, text);
            let left = if align == Align::Right { x - width } else { x };
            for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (1.0, 1.0)] {
                let at = (ox + (left + dx) * s, oy + (y + dy) * s);
                self.art.draw_flat_text(0, text, at, s, black);
            }
            let at = (ox + left * s, oy + y * s);
            match colour {
                Some(pulse) => self.art.draw_flat_text(0, text, at, s, pulse),
                None => {
                    self.art.draw_bitmap_text(0, 4, text, at.0, at.1, s);
                }
            }
        }
    }

    /// The gold, food and mana panels, the locked action buttons and the Next Turn button: the
    /// column while no stack is selected.
    fn draw_summary(&mut self, lay: &Layout, click: &mut FrameClick, input: &Input) {
        let s = lay.scale;
        let view = (lay.origin.x, lay.origin.y, s);
        // Resource panels with this turn's income in each.
        let t = self.game.budget(HUMAN);
        let income = self.game.income(HUMAN);
        let net_mana = income.split.mana - income.mana_upkeep;
        let (px, py) = spot::PANELS;
        self.draw_pic(lay, "main", 34, 0, px, py);
        for (k, (value, label)) in [
            (t.net_gold(), self.text("ui.gold")),
            (t.food_surplus - t.food_upkeep, self.text("ui.food")),
            (net_mana, self.text("ui.mana")),
        ]
        .into_iter()
        .enumerate()
        {
            self.income_text(view, value, &label, 101.0 + 32.0 * k as f32);
        }

        // The locked buttons (done, patrol, wait, and build or purify).
        for (entry, at) in [
            (12, (246.0, 176.0)),
            (13, (280.0, 176.0)),
            (14, (246.0, 186.0)),
        ] {
            self.draw_pic(lay, "main", entry, 0, at.0, at.1);
        }
        self.draw_pic(lay, "main", 15, 0, 280.0, 186.0);

        // While a wizard's Time Stop runs, a note says whose it is.
        if let Some(stopper) = self.game.time_stopper() {
            let args = self.wizard_arg(Args::new(), "wizard", stopper);
            let note = self.text_with("ui.time_stop_note", &args);
            let (nx, ny) = spot::NEXT_TURN;
            let r = lay.rect((nx, ny - 14.0, 80.0, 12.0));
            text_centered(&note, r.x + r.w / 2.0, r.y + r.h * 0.7, 3.2 * s, GOLD);
        }
        // Next Turn, over them, once no unit is left that waits for orders.
        if !self.all_units_moved() {
            return;
        }
        let (nx, ny) = spot::NEXT_TURN;
        if self.draw_pic(lay, "main", 35, 0, nx, ny).is_some() {
            // The button has one picture; it sits 2 px down and right while it is held.
            let (bx, by) = (nx + 6.0, ny + 5.0);
            let state = self.art_picture("main", 58, 0).map_or(ui::Hit::None, |t| {
                let p = lay.point(bx, by);
                ui::hit(
                    Rect::new(
                        p.x,
                        p.y,
                        crate::resolution::logical_w(&t) * s,
                        crate::resolution::logical_h(&t) * s,
                    ),
                    true,
                    input,
                )
            });
            let push = if state.held() { 2.0 } else { 0.0 };
            self.draw_pic(lay, "main", 58, 0, bx + push, by + push);
            click.next_turn = state.clicked();
        } else {
            let r = lay.rect((nx + 4.0, ny + 4.0, 72.0, 19.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.45, 0.3, 0.12, 1.0));
            text_centered(
                &self.text("ui.next_turn"),
                r.x + r.w / 2.0,
                r.y + r.h * 0.7,
                5.0 * s,
                GOLD,
            );
            click.next_turn = ui::hit(r, true, input).clicked();
        }
    }

    /// The minimap as the original draws it (`Create_Reduced_Map_Picture`, `Draw_World_Window`):
    /// one pixel for each of the 58 by 30 squares round the middle of the map window, black where
    /// unexplored, in the colour of the terrain type, the cities in their banner's colour, and
    /// ticks at the corners of the box that shows the map window. Pressing or dragging on it centres
    /// the map there.
    fn draw_minimap(&mut self, lay: &Layout, input: &Input) {
        const COLS: i32 = 58;
        const ROWS: i32 = 30;
        let r = lay.rect(spot::MINIMAP);
        draw_rectangle(r.x, r.y, r.w, r.h, BLACK);
        let Some(map) = self.game.map() else {
            return;
        };
        let (w, h) = (map.width() as i32, map.height() as i32);
        let (cols, rows) = self.visible_tiles();
        let (mid_x, mid_y) = (
            (self.cam.x + cols / 2.0).round() as i32,
            (self.cam.y + rows / 2.0).round() as i32,
        );
        let (start_x, start_y) = (mid_x - COLS / 2, mid_y - ROWS / 2);
        let grid = map.plane(self.plane);
        let content = self.game.content();
        let plane_offset = if self.plane == f4x_core::map::Plane::Myrror {
            crate::art::terrain_type::MYRROR_OFFSET
        } else {
            0
        };
        let s = lay.scale;
        let pixel = |i: i32, j: i32, colour: Color| {
            let (x0, y0) = ((i as f32 * s).floor(), (j as f32 * s).floor());
            let (x1, y1) = (((i + 1) as f32 * s).floor(), ((j + 1) as f32 * s).floor());
            draw_rectangle(
                r.x + x0,
                r.y + y0,
                (x1 - x0).max(1.0),
                (y1 - y0).max(1.0),
                colour,
            );
        };
        for j in 0..ROWS {
            let y = start_y + j;
            if !(0..h).contains(&y) {
                continue;
            }
            for i in 0..COLS {
                let x = (start_x + i).rem_euclid(w);
                let at = Position {
                    plane: self.plane,
                    x: x as u32,
                    y: y as u32,
                };
                if !self.game.is_explored(HUMAN, at) {
                    continue;
                }
                let art = &self.art;
                let ground = if self.game.economy().volcano_at(at).is_some() {
                    Some(crate::terrain_pick::VOLCANO)
                } else {
                    crate::terrain_pick::ground_type(
                        grid,
                        content,
                        x,
                        y,
                        self.plane.index(),
                        &|table, mask| art.autotile_type(table, mask),
                    )
                };
                if let Some(ground) = ground {
                    pixel(i, j, art.minimap().terrain(ground + plane_offset));
                }
            }
        }
        for (_, city, at) in self.game.economy().cities() {
            let (i, j) = ((at.x as i32 - start_x).rem_euclid(w), at.y as i32 - start_y);
            if at.plane == self.plane
                && self.game.is_explored(HUMAN, at)
                && (0..COLS).contains(&i)
                && (0..ROWS).contains(&j)
            {
                let banner = self
                    .banner_index(city.owner.0)
                    .min(f4x_core::player::NEUTRAL_BANNER);
                pixel(i, j, self.art.minimap().city(banner));
            }
        }
        // The selected stack blinks.
        if let Some(at) = self.selection_position().filter(|a| a.plane == self.plane) {
            let (i, j) = ((at.x as i32 - start_x).rem_euclid(w), at.y as i32 - start_y);
            if (get_time() * 2.0) as i64 % 2 == 0
                && (0..COLS).contains(&i)
                && (0..ROWS).contains(&j)
            {
                pixel(i, j, self.art.minimap().mark());
            }
        }
        // The box round the map window: three pixels at each corner.
        let (box_w, box_h) = (cols.round() as i32, rows.round() as i32);
        let (left, top) = (COLS / 2 - box_w / 2, ROWS / 2 - box_h / 2);
        let colour = self.art.minimap().window_box();
        for (cx, cy, dx, dy) in [
            (left - 1, top - 1, 1, 1),
            (left + box_w, top - 1, -1, 1),
            (left - 1, top + box_h, 1, -1),
            (left + box_w, top + box_h, -1, -1),
        ] {
            for k in 0..3 {
                pixel(cx + dx * k, cy, colour);
                pixel(cx, cy + dy * k, colour);
            }
        }
        // Pressing the minimap centres the map there, and so does holding the button while the
        // pointer moves over it (the original's grid field answers every frame the button is down).
        let mouse: Vec2 = input.mouse_position().into();
        if ui::hit(r, true, input).held() {
            let (i, j) = (
                ((mouse.x - r.x) / s).floor() as i32,
                ((mouse.y - r.y) / s).floor() as i32,
            );
            self.cam = vec2(
                (start_x + i) as f32 - cols / 2.0,
                (start_y + j) as f32 - rows / 2.0,
            );
        }
    }
}

/// The colour of a terrain on the minimap and the Cartographer's map.
pub(crate) fn mini_terrain_color(class: f4x_core::map::terrain::TerrainClass) -> Color {
    use f4x_core::map::terrain::TerrainClass as T;
    match class {
        T::Ocean => Color::new(0.1, 0.2, 0.6, 1.0),
        T::Mountains => Color::new(0.6, 0.6, 0.6, 1.0),
        T::Desert => Color::new(0.8, 0.7, 0.4, 1.0),
        T::Tundra => Color::new(0.85, 0.9, 0.95, 1.0),
        _ => Color::new(0.2, 0.5, 0.2, 1.0),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

impl App {}

/// What was clicked on the frame this frame.
#[derive(Debug, Default, Clone, Copy)]
pub struct FrameClick {
    /// The menu button's index: Game, Spells, Armies, Cities, Magic, Info, Plane.
    pub menu: Option<usize>,
    pub next_turn: bool,
    /// A unit of the unit window that was clicked, or right-clicked.
    pub slot: Option<hecs::Entity>,
    pub slot_info: Option<hecs::Entity>,
    /// The click was on a unit while all of the stack was picked: pick only that one.
    pub pick_only: bool,
    /// An action button of the unit window.
    pub button: Option<crate::unit_window::StackButton>,
}
