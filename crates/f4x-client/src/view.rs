//! The overworld and city screens.

use std::collections::HashMap;

use f4x_core::city::{PlayerId, Position};
use f4x_core::content::{Content, FeatureKind, TerrainClass};
use f4x_core::map::Plane;
use f4x_core::mapgen::{generate, GenParams};
use f4x_core::{Command, Event, Game};
use macroquad::prelude::*;

use crate::art::{terrain_type as tt, Art, TILE_H, TILE_W};

const BAR_H: f32 = 28.0;
const SIDE_W: f32 = 260.0;
const HUMAN: PlayerId = PlayerId(0);

const PANEL: Color = Color::new(0.08, 0.07, 0.06, 0.92);
const GOLD: Color = Color::new(0.95, 0.8, 0.35, 1.0);

pub struct App {
    game: Game,
    art: Art,
    params: GenParams,
    seed: u64,
    plane: Plane,
    /// Top-left visible tile (fractional while panning).
    cam: Vec2,
    zoom: f32,
    /// Index into the human player's race list in `content.rules.races`.
    race: usize,
    city_race: HashMap<hecs::Entity, usize>,
    open_city: Option<hecs::Entity>,
    message: String,
}

fn flat_color(class: TerrainClass) -> Color {
    match class {
        TerrainClass::Ocean => Color::new(0.1, 0.2, 0.55, 1.0),
        TerrainClass::Grassland => Color::new(0.25, 0.55, 0.2, 1.0),
        TerrainClass::Forest => Color::new(0.1, 0.35, 0.15, 1.0),
        TerrainClass::Hills => Color::new(0.5, 0.45, 0.25, 1.0),
        TerrainClass::Mountains => Color::new(0.5, 0.5, 0.5, 1.0),
        TerrainClass::Swamp => Color::new(0.25, 0.3, 0.2, 1.0),
        TerrainClass::Desert => Color::new(0.8, 0.7, 0.4, 1.0),
        TerrainClass::Tundra => Color::new(0.8, 0.85, 0.9, 1.0),
    }
}

fn base_type(class: TerrainClass) -> usize {
    match class {
        TerrainClass::Ocean => tt::OCEAN,
        TerrainClass::Grassland => tt::GRASSLAND,
        TerrainClass::Forest => tt::FOREST,
        TerrainClass::Hills => tt::HILLS,
        TerrainClass::Mountains => tt::MOUNTAINS,
        TerrainClass::Swamp => tt::SWAMP,
        TerrainClass::Desert => tt::DESERT,
        TerrainClass::Tundra => tt::TUNDRA,
    }
}

impl App {
    pub fn new(content: Content, art: Art, seed: u64) -> Result<Self, String> {
        let params = GenParams::default();
        let game = Self::new_game(seed, &params, &content)?;
        // Start as the first race that has city names, else the first race.
        let race = content
            .rules
            .races
            .defs()
            .iter()
            .position(|r| !r.city_names.is_empty())
            .unwrap_or(0);
        let mut app = Self {
            game,
            art,
            params,
            seed,
            plane: Plane::Arcanus,
            cam: Vec2::ZERO,
            zoom: 3.0,
            race,
            city_race: HashMap::new(),
            open_city: None,
            message: String::new(),
        };
        app.place_start_city();
        Ok(app)
    }

    fn new_game(seed: u64, params: &GenParams, content: &Content) -> Result<Game, String> {
        let map = generate(seed, params, content).map_err(|e| e.to_string())?;
        let mut game = Game::new(seed);
        game.map = Some(map);
        game.content = content.clone();
        Ok(game)
    }

    /// Found the human player's first city on the best-looking spot near the middle.
    fn place_start_city(&mut self) {
        let (cx, cy) = (self.params.width / 2, self.params.height / 2);
        let mut spots: Vec<(u32, u32)> = (0..self.params.height)
            .flat_map(|y| (0..self.params.width).map(move |x| (x, y)))
            .collect();
        spots.sort_by_key(|&(x, y)| x.abs_diff(cx).pow(2) + y.abs_diff(cy).pow(2));
        for (x, y) in spots {
            let pos = Position {
                plane: Plane::Arcanus,
                x,
                y,
            };
            if self.game.can_found_city(pos).is_ok() && self.found_city(pos) {
                self.center_on(pos);
                return;
            }
        }
    }

    fn center_on(&mut self, pos: Position) {
        self.plane = pos.plane;
        let (cols, rows) = self.visible_tiles();
        self.cam = vec2(
            pos.x as f32 - cols / 2.0,
            (pos.y as f32 - rows / 2.0).max(0.0),
        );
    }

    fn race_name(&self, race: usize) -> &str {
        &self.game.content.rules.races.defs()[race].name
    }

    fn found_city(&mut self, pos: Position) -> bool {
        let race = &self.game.content.rules.races.defs()[self.race];
        let used = self.game.cities().len();
        let name = race
            .city_names
            .get(used % race.city_names.len().max(1))
            .cloned()
            .unwrap_or_else(|| format!("City {}", used + 1));
        let race_id = self
            .game
            .content
            .rules
            .races
            .id_of(&self.game.content.rules.races.defs()[self.race].id);
        let population = if used == 0 { 3 } else { 0 };
        let events = self.game.apply(Command::FoundCity {
            owner: HUMAN,
            pos,
            name: name.clone(),
            race: race_id,
            population,
        });
        for event in events {
            match event {
                Event::CityFounded { city, .. } => {
                    self.city_race.insert(city, self.race);
                    self.message = format!("Founded {name}");
                    return true;
                }
                Event::Rejected(why) => self.message = format!("Cannot found a city here: {why:?}"),
                _ => {}
            }
        }
        false
    }

    pub fn open_first_city(&mut self) {
        self.open_city = self.game.cities().first().map(|(e, ..)| *e);
    }

    fn tile_size(&self) -> Vec2 {
        vec2(TILE_W * self.zoom, TILE_H * self.zoom)
    }

    fn map_rect(&self) -> Rect {
        Rect::new(
            0.0,
            BAR_H,
            screen_width() - SIDE_W,
            screen_height() - 2.0 * BAR_H,
        )
    }

    fn visible_tiles(&self) -> (f32, f32) {
        let r = self.map_rect();
        let t = self.tile_size();
        (r.w / t.x, r.h / t.y)
    }

    fn tile_at(&self, screen: Vec2) -> Option<(i32, i32)> {
        let r = self.map_rect();
        if !r.contains(screen) {
            return None;
        }
        let t = self.tile_size();
        let x = ((screen.x - r.x) / t.x + self.cam.x).floor() as i32;
        let y = ((screen.y - r.y) / t.y + self.cam.y).floor() as i32;
        let grid = self.game.map.as_ref()?.plane(self.plane);
        (y >= 0 && y < grid.height() as i32).then(|| (grid.wrap_x(x) as i32, y))
    }

    fn clamp_camera(&mut self) {
        let (_, rows) = self.visible_tiles();
        let h = self.params.height as f32;
        self.cam.y = self.cam.y.clamp(0.0, (h - rows).max(0.0));
        let w = self.params.width as f32;
        self.cam.x = self.cam.x.rem_euclid(w);
    }

    fn new_map(&mut self, seed: u64) {
        let content = self.game.content.clone();
        match Self::new_game(seed, &self.params, &content) {
            Ok(game) => {
                self.game = game;
                self.seed = seed;
                self.city_race.clear();
                self.open_city = None;
                self.place_start_city();
                self.message = format!("New map, seed {seed}");
            }
            Err(e) => self.message = format!("Map generation failed: {e}"),
        }
    }

    /// Handle input and draw one frame. Returns false when the player quits.
    pub fn frame(&mut self) -> bool {
        if self.open_city.is_some() {
            if is_key_pressed(KeyCode::Escape) || is_mouse_button_pressed(MouseButton::Right) {
                self.open_city = None;
            }
        } else {
            if is_key_pressed(KeyCode::Escape) {
                return false;
            }
            self.overworld_input();
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        if self.open_city.is_some() {
            self.draw_city_screen();
        } else {
            self.draw_map();
            self.draw_bars();
        }
        true
    }

    fn overworld_input(&mut self) {
        let step = 12.0 * get_frame_time();
        let mut pan = Vec2::ZERO;
        for (key, dir) in [
            (KeyCode::Left, vec2(-1.0, 0.0)),
            (KeyCode::A, vec2(-1.0, 0.0)),
            (KeyCode::Right, vec2(1.0, 0.0)),
            (KeyCode::D, vec2(1.0, 0.0)),
            (KeyCode::Up, vec2(0.0, -1.0)),
            (KeyCode::W, vec2(0.0, -1.0)),
            (KeyCode::Down, vec2(0.0, 1.0)),
            (KeyCode::S, vec2(0.0, 1.0)),
        ] {
            if is_key_down(key) {
                pan += dir;
            }
        }
        self.cam += pan * step;
        if is_key_pressed(KeyCode::Tab) {
            self.plane = if self.plane == Plane::Arcanus {
                Plane::Myrror
            } else {
                Plane::Arcanus
            };
        }
        if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
            self.zoom = (self.zoom + 1.0).min(6.0);
        }
        if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract) {
            self.zoom = (self.zoom - 1.0).max(1.0);
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
            for event in self.game.apply(Command::EndTurn) {
                if let Event::TurnStarted { turn } = event {
                    self.message = format!("Turn {turn}");
                }
            }
        }
        if is_key_pressed(KeyCode::N) {
            self.new_map(self.seed + 1);
        }
        if is_key_pressed(KeyCode::R) {
            self.race = (self.race + 1) % self.game.content.rules.races.len().max(1);
            self.message = format!("Playing as {}", self.race_name(self.race));
        }
        self.clamp_camera();

        let hovered = self.tile_at(mouse_position().into());
        if let Some((x, y)) = hovered {
            let pos = Position {
                plane: self.plane,
                x: x as u32,
                y: y as u32,
            };
            if is_mouse_button_pressed(MouseButton::Left) {
                if let Some(city) = self.game.city_at(pos) {
                    self.open_city = Some(city);
                }
            }
            if is_key_pressed(KeyCode::B) {
                self.found_city(pos);
            }
        }
    }

    fn draw_map(&mut self) {
        let Some(map) = self.game.map.as_ref() else {
            return;
        };
        let grid = map.plane(self.plane);
        let r = self.map_rect();
        let t = self.tile_size();
        let (cols, rows) = self.visible_tiles();
        let (x0, y0) = (self.cam.x.floor() as i32, self.cam.y.floor() as i32);
        let off = vec2(
            (self.cam.x - x0 as f32) * t.x,
            (self.cam.y - y0 as f32) * t.y,
        );
        let plane_offset = if self.plane == Plane::Myrror {
            tt::MYRROR_OFFSET
        } else {
            0
        };
        let content = &self.game.content;
        let cities: Vec<_> = self.game.cities();

        // Clip the map to its viewport.
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        gl.quad_gl.scissor(Some((
            (r.x * screen_dpi_scale()) as i32,
            ((screen_height() - r.y - r.h) * screen_dpi_scale()) as i32,
            (r.w * screen_dpi_scale()) as i32,
            (r.h * screen_dpi_scale()) as i32,
        )));
        for j in 0..=(rows.ceil() as i32) {
            let y = y0 + j;
            if y < 0 || y >= grid.height() as i32 {
                continue;
            }
            for i in 0..=(cols.ceil() as i32) {
                let wx = grid.wrap_x(x0 + i) as i32;
                let (sx, sy) = (r.x + i as f32 * t.x - off.x, r.y + j as f32 * t.y - off.y);
                let Some(def) = grid.get(wx, y).and_then(|id| content.terrains.get(id)) else {
                    continue;
                };
                let feature = grid.feature(wx, y).and_then(|f| content.features.get(f));
                let mut type_no = base_type(def.class);
                if let Some(f) = feature.filter(|f| f.kind == FeatureKind::Node) {
                    type_no = match f.id.as_str() {
                        "sorcery_node" => tt::SORCERY_NODE,
                        "nature_node" => tt::NATURE_NODE,
                        _ => tt::CHAOS_NODE,
                    };
                }
                match self.art.terrain(type_no + plane_offset) {
                    Some(tex) => draw_texture_ex(
                        tex,
                        sx,
                        sy,
                        WHITE,
                        DrawTextureParams {
                            dest_size: Some(t),
                            ..Default::default()
                        },
                    ),
                    None => draw_rectangle(sx, sy, t.x, t.y, flat_color(def.class)),
                }
                if let Some(f) = feature {
                    draw_feature_marker(f.kind, sx, sy, t);
                }
            }
        }
        for (_, city, pos) in &cities {
            if pos.plane != self.plane {
                continue;
            }
            let mut dx = pos.x as f32 - x0 as f32;
            if dx < 0.0 {
                dx += self.params.width as f32;
            }
            let (sx, sy) = (
                r.x + dx * t.x - off.x,
                r.y + (pos.y as f32 - y0 as f32) * t.y - off.y,
            );
            draw_city_marker(sx, sy, t);
            let label = &city.name;
            draw_text(label, sx - 2.0, sy - 3.0, 16.0 + self.zoom * 2.0, BLACK);
            draw_text(label, sx - 3.0, sy - 4.0, 16.0 + self.zoom * 2.0, WHITE);
        }
        gl.flush();
        gl.quad_gl.scissor(None);
    }

    fn draw_bars(&mut self) {
        let w = screen_width();
        let h = screen_height();
        draw_rectangle(0.0, 0.0, w, BAR_H, PANEL);
        let plane = if self.plane == Plane::Arcanus {
            "Arcanus"
        } else {
            "Myrror"
        };
        draw_text(
            format!(
                "Turn {}    {}    zoom {}x    seed {}",
                self.game.turn, plane, self.zoom, self.seed
            ),
            10.0,
            19.0,
            22.0,
            GOLD,
        );
        draw_rectangle(0.0, h - BAR_H, w, BAR_H, PANEL);
        draw_text(
            "arrows/WASD scroll   Tab plane   +/- zoom   B found city   click city   Enter end turn   N new map   R race   Esc quit",
            10.0,
            h - 9.0,
            18.0,
            LIGHTGRAY,
        );

        // Side panel: what is under the cursor.
        let px = w - SIDE_W;
        draw_rectangle(px, BAR_H, SIDE_W, h - 2.0 * BAR_H, PANEL);
        let mut lines = Lines {
            x: px + 12.0,
            y: BAR_H + 26.0,
        };
        lines.line(&format!("You: {}", self.race_name(self.race)), GOLD);
        lines.line(&format!("Cities: {}", self.game.cities().len()), WHITE);
        if !self.art.available() {
            lines.line("(no art: run extract-art)", RED);
        }
        lines.y += 8.0;
        if let Some((x, ty)) = self.tile_at(mouse_position().into()) {
            let content = &self.game.content;
            let map = self.game.map.as_ref().unwrap();
            let grid = map.plane(self.plane);
            lines.line(&format!("Tile {x}, {ty}"), GOLD);
            if let Some(def) = grid.get(x, ty).and_then(|id| content.terrains.get(id)) {
                lines.line(&def.name, WHITE);
                lines.line(
                    &format!("Move cost {}   Food {}", def.move_cost, def.food),
                    LIGHTGRAY,
                );
            }
            if let Some(f) = grid.feature(x, ty).and_then(|f| content.features.get(f)) {
                lines.line(&f.name, Color::new(0.6, 0.9, 1.0, 1.0));
            }
            let pos = Position {
                plane: self.plane,
                x: x as u32,
                y: ty as u32,
            };
            if let Some(city) = self.game.city_at(pos) {
                if let Some((_, c, _)) = self.game.cities().into_iter().find(|(e, ..)| *e == city) {
                    lines.line(&format!("City of {}", c.name), GOLD);
                }
            }
        }
        if !self.message.is_empty() {
            draw_text(&self.message, px + 12.0, h - BAR_H - 14.0, 20.0, GOLD);
        }
    }

    fn draw_city_screen(&mut self) {
        let Some(city_entity) = self.open_city else {
            return;
        };
        let Some((_, city, _)) = self
            .game
            .cities()
            .into_iter()
            .find(|(e, ..)| *e == city_entity)
        else {
            self.open_city = None;
            return;
        };
        let race = self
            .city_race
            .get(&city_entity)
            .copied()
            .unwrap_or(self.race);
        let scale = ((screen_height() / 200.0).min(screen_width() / 320.0))
            .floor()
            .max(1.0);
        let (ox, oy) = (
            (screen_width() - 320.0 * scale) / 2.0,
            (screen_height() - 200.0 * scale) / 2.0,
        );
        // The original's panels are see-through cut-outs: back them with white, as the game did.
        draw_rectangle(ox, oy, 320.0 * scale, 200.0 * scale, WHITE);
        match self.art.picture("backgrnd", 6, 0) {
            Some(tex) => draw_texture_ex(
                tex,
                ox,
                oy,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(320.0 * scale, 200.0 * scale)),
                    ..Default::default()
                },
            ),
            None => draw_rectangle(ox, oy, 320.0 * scale, 200.0 * scale, PANEL),
        }
        let races = self.game.content.rules.races.defs();
        let race_def = &races[race];
        let title = format!("{} - {}", city.name, race_def.name);
        draw_text(
            &title,
            ox + 12.0 * scale,
            oy + 12.0 * scale,
            8.0 * scale,
            GOLD,
        );
        // The big white "Buildings" box of the original layout: list what this race can build.
        let rules = &self.game.content.rules;
        let buildable: Vec<_> = rules
            .buildings
            .defs()
            .iter()
            .filter(|b| b.races.is_empty() || b.races.contains(&race_def.id))
            .collect();
        draw_text(
            "Can build:",
            ox + 10.0 * scale,
            oy + 112.0 * scale,
            5.5 * scale,
            DARKBLUE,
        );
        let columns = 3;
        for (i, b) in buildable.iter().enumerate() {
            let (cx, cy) = ((i % columns) as f32, (i / columns) as f32);
            let text = format!("{} ({})", b.name, b.cost);
            draw_text(
                &text,
                ox + (10.0 + cx * 66.0) * scale,
                oy + (120.0 + cy * 6.5) * scale,
                4.6 * scale,
                BLACK,
            );
        }
        // The "Units" box on the right: the race's units.
        for (i, u) in race_def.units.iter().take(5).enumerate() {
            let name = rules
                .units
                .by_name(u)
                .map_or_else(|| u.clone(), |d| d.name.clone());
            draw_text(
                &name,
                ox + 218.0 * scale,
                oy + (112.0 + i as f32 * 5.5) * scale,
                4.4 * scale,
                BLACK,
            );
        }
        if race_def.units.len() > 5 {
            let more = format!("+{} more", race_def.units.len() - 5);
            draw_text(
                &more,
                ox + 218.0 * scale,
                oy + 140.0 * scale,
                4.0 * scale,
                DARKGRAY,
            );
        }
        draw_text(
            "Esc / right-click to close",
            ox + 10.0 * scale,
            oy + 197.0 * scale,
            4.0 * scale,
            DARKGRAY,
        );
    }
}

/// Stacked text lines in a panel.
struct Lines {
    x: f32,
    y: f32,
}

impl Lines {
    fn line(&mut self, text: &str, color: Color) {
        draw_text(text, self.x, self.y, 20.0, color);
        self.y += 24.0;
    }
}

fn draw_feature_marker(kind: FeatureKind, sx: f32, sy: f32, t: Vec2) {
    let (cx, cy) = (sx + t.x / 2.0, sy + t.y / 2.0);
    match kind {
        FeatureKind::Mineral => draw_circle(cx, cy, t.y * 0.18, GOLD),
        FeatureKind::Encounter => {
            draw_poly(cx, cy, 4, t.y * 0.3, 45.0, Color::new(0.85, 0.2, 0.2, 1.0))
        }
        FeatureKind::Tower => draw_rectangle(
            cx - t.x * 0.18,
            sy + t.y * 0.1,
            t.x * 0.36,
            t.y * 0.8,
            LIGHTGRAY,
        ),
        FeatureKind::Node => {}
    }
}

fn draw_city_marker(sx: f32, sy: f32, t: Vec2) {
    let (w, h) = (t.x * 0.8, t.y * 0.6);
    let (x, y) = (sx + t.x * 0.1, sy + t.y * 0.3);
    draw_rectangle(x, y, w, h, Color::new(0.75, 0.6, 0.4, 1.0));
    draw_triangle(
        vec2(x - 2.0, y),
        vec2(x + w + 2.0, y),
        vec2(x + w / 2.0, y - t.y * 0.3),
        Color::new(0.7, 0.2, 0.15, 1.0),
    );
    draw_rectangle_lines(x, y, w, h, 1.5, BLACK);
}
