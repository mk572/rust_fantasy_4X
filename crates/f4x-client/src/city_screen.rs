//! The city screen: economy, buildings, worked area and the production list, drawn on
//! the original screen layout (320x200) when the art is available.

use f4x_core::city::Position;
use f4x_core::economy::{Buildings, CityRace, Enchantments, Farmers, Population};
use f4x_core::production::ProductionItem;
use f4x_core::Command;
use macroquad::prelude::*;

use crate::art::{TILE_H, TILE_W};
use crate::frame::Align;
use crate::ui::GOLD;
use crate::view::{base_type, App, Screen};

/// Virtual-to-screen mapping for the 320x200 layout.
#[derive(Clone, Copy)]
struct Layout {
    ox: f32,
    oy: f32,
    scale: f32,
}

impl Layout {
    fn new() -> Self {
        let scale = (screen_height() / 200.0)
            .min(screen_width() / 320.0)
            .floor()
            .max(1.0);
        Self {
            ox: (screen_width() - 320.0 * scale) / 2.0,
            oy: (screen_height() - 200.0 * scale) / 2.0,
            scale,
        }
    }

    fn rect(self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(
            self.ox + x * self.scale,
            self.oy + y * self.scale,
            w * self.scale,
            h * self.scale,
        )
    }

    fn text(self, s: &str, x: f32, y: f32, size: f32, color: Color) {
        draw_text(
            s,
            self.ox + x * self.scale,
            self.oy + y * self.scale,
            size * self.scale,
            color,
        );
    }
}

impl App {
    pub(crate) fn city_screen(&mut self, city: hecs::Entity) {
        if self.game.city_at(self.city_position(city)).is_none() {
            self.screen = Screen::Map;
            return;
        }
        let lay = Layout::new();
        let choosing = self.choosing;
        if is_key_pressed(KeyCode::Escape)
            || (is_mouse_button_pressed(MouseButton::Right) && !choosing)
        {
            if choosing {
                self.choosing = false;
            } else {
                self.screen = Screen::Map;
            }
            return;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        // The original's panels are see-through cut-outs: back them with white, as the game did.
        let full = lay.rect(0.0, 0.0, 320.0, 200.0);
        draw_rectangle(full.x, full.y, full.w, full.h, WHITE);
        match self.art.picture("backgrnd", 6, 0) {
            Some(tex) => draw_texture_ex(
                tex,
                full.x,
                full.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(full.w, full.h)),
                    ..Default::default()
                },
            ),
            None => draw_rectangle(
                full.x,
                full.y,
                full.w,
                full.h,
                Color::new(0.3, 0.25, 0.2, 1.0),
            ),
        }
        self.draw_city_header(city, lay);
        self.draw_city_resources(city, lay);
        self.draw_city_buildings(city, lay);
        self.draw_city_area(city, lay);
        self.draw_city_garrison(city, lay);
        self.draw_city_production(city, lay);
        if self.choosing {
            self.draw_production_menu(city, lay);
        }
    }

    pub(crate) fn city_position(&self, city: hecs::Entity) -> Position {
        self.game
            .world
            .get::<&Position>(city)
            .map(|p| *p)
            .unwrap_or(Position {
                plane: f4x_core::map::Plane::Arcanus,
                x: 0,
                y: 0,
            })
    }

    fn view(lay: Layout) -> (f32, f32, f32) {
        (lay.ox, lay.oy, lay.scale)
    }

    /// A picture from the local art at the original's `(x, y)`; false if it is missing.
    fn city_pic(
        &mut self,
        lay: Layout,
        lbx: &str,
        entry: usize,
        frame: usize,
        x: f32,
        y: f32,
    ) -> bool {
        let Some(tex) = self.art.picture(lbx, entry, frame).cloned() else {
            return false;
        };
        let at = lay.rect(x, y, 0.0, 0.0);
        draw_texture_ex(
            &tex,
            at.x,
            at.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(tex.size() * lay.scale),
                ..Default::default()
            },
        );
        true
    }

    fn draw_city_header(&mut self, city: hecs::Entity, lay: Layout) {
        let name = self.city_name(city);
        let (race_name, race_index) = self
            .game
            .world
            .get::<&CityRace>(city)
            .map(|r| {
                (
                    self.game.content.rules.races.get(r.0).name.clone(),
                    r.0.index(),
                )
            })
            .unwrap_or_default();
        let Some(eco) = self.game.city_economy(city) else {
            return;
        };
        let pop = self
            .game
            .world
            .get::<&Population>(city)
            .map(|p| *p)
            .unwrap_or_default();
        // The title names the size: Outpost, Hamlet, Village, Town, City, Capital.
        let size = if eco.figures == 0 {
            "Outpost"
        } else {
            ["Hamlet", "Village", "Town", "City", "Capital"]
                [(eco.figures.div_ceil(4).clamp(1, 5) - 1) as usize]
        };
        let mut title = format!("{size} of {name}");
        if self.game.is_fortress_city(city) && size != "Capital" {
            title = format!("Capital of {name}");
        }
        let view = Self::view(lay);
        self.bm_text(
            view,
            5,
            Color::new(0.95, 0.85, 0.5, 1.0),
            &title,
            105.0,
            0.0,
            Align::Center,
        );
        let orange = Color::new(0.95, 0.7, 0.2, 1.0);
        self.bm_text(view, 3, orange, &race_name, 6.0, 19.0, Align::Left);
        let people = pop.figures * 1000 + pop.tens * 10;
        let grow = eco.growth_tens * 10;
        self.bm_text(
            view,
            3,
            orange,
            &format!("Population: {} ({:+})", thousands(people), grow),
            207.0,
            19.0,
            Align::Right,
        );
        // The citizens: farmers, then workers, then rebels. Clicking one changes its job.
        let farmers = self.game.world.get::<&Farmers>(city).map_or(0, |f| f.0);
        let total = (eco.farmers + eco.workers + eco.rebels).max(1) as f32;
        let step = (11.0f32).min(196.0 / total);
        let mut x = 7.0;
        let mouse: Vec2 = mouse_position().into();
        let clicked = is_mouse_button_pressed(MouseButton::Left) && !self.choosing;
        let jobs = [(eco.farmers, 45usize), (eco.workers, 59), (eco.rebels, 74)];
        for (job, (count, base)) in jobs.into_iter().enumerate() {
            for _ in 0..count {
                let entry = base + race_index;
                if !self.city_pic(lay, "backgrnd", entry, 0, x, 26.0) {
                    let r = lay.rect(x, 27.0, 8.0, 12.0);
                    draw_rectangle(r.x, r.y, r.w, r.h, [GREEN, ORANGE, RED][job]);
                }
                let hit = lay.rect(x, 26.0, step, 15.0);
                if clicked && hit.contains(mouse) && eco.figures > 0 {
                    match job {
                        0 => {
                            self.game.apply(Command::SetFarmers {
                                city,
                                farmers: farmers.saturating_sub(1),
                            });
                        }
                        1 => {
                            self.game.apply(Command::SetFarmers {
                                city,
                                farmers: farmers + 1,
                            });
                        }
                        _ => {}
                    }
                }
                x += step;
            }
        }
    }

    /// `count` small icons of `entry` from `backgrnd.lbx` (tens as bigger ones), starting at
    /// `(x, y)`; returns the x after the last icon.
    fn icon_row(
        &mut self,
        lay: Layout,
        small: usize,
        big: usize,
        count: i32,
        x: f32,
        y: f32,
    ) -> f32 {
        let mut at = x;
        if count <= 0 {
            return at;
        }
        let (tens, ones) = (count / 10, count % 10);
        for _ in 0..tens {
            if !self.city_pic(lay, "backgrnd", big, 0, at, y - 4.0) {
                at += 0.0;
            }
            at += 9.0;
        }
        for _ in 0..ones {
            self.city_pic(lay, "backgrnd", small, 0, at, y);
            at += 8.0;
        }
        at
    }

    fn draw_city_resources(&mut self, city: hecs::Entity, lay: Layout) {
        let Some(eco) = self.game.city_economy(city) else {
            return;
        };
        let view = Self::view(lay);
        let figures = eco.figures as i32;
        let net_gold = eco.gold - eco.gold_upkeep;
        // Food, production, gold (income, then upkeep), magic power, research.
        let rows = [
            (
                40usize,
                34usize,
                eco.food,
                format!("{:+}", eco.food - figures),
            ),
            (41, 35, eco.production, String::new()),
            (42, 36, eco.gold, format!("{net_gold:+}")),
            (43, 37, eco.mana, String::new()),
            (44, 38, eco.research, String::new()),
        ];
        let ink = Color::new(0.12, 0.1, 0.08, 1.0);
        for (i, (small, big, count, note)) in rows.into_iter().enumerate() {
            let y = 54.0 + i as f32 * 8.6;
            let mut end = self.icon_row(lay, small, big, count, 8.0, y);
            if i == 2 && eco.gold_upkeep > 0 {
                end = self.icon_row(lay, 73, 73, eco.gold_upkeep, end + 6.0, y);
            }
            let _ = end;
            if !note.is_empty() {
                let color = if note.starts_with('-') {
                    Color::new(0.7, 0.1, 0.05, 1.0)
                } else {
                    ink
                };
                self.bm_text(view, 1, color, &note, 134.0, y, Align::Right);
            }
        }
        // Enchantments, in the colour of whoever cast them.
        let list: Vec<_> = self
            .game
            .world
            .get::<&Enchantments>(city)
            .map(|e| e.0.clone())
            .unwrap_or_default();
        // The fortress and the summoning circle head the list, in gold.
        let here = self.game.world.get::<&Position>(city).map(|p| *p).ok();
        let owner = self.game.city_owner(city);
        let mut tags = Vec::new();
        if self.game.is_fortress_city(city) {
            tags.push("Fortress");
        }
        if here.is_some()
            && owner
                .and_then(|o| self.game.player(o))
                .and_then(|p| p.summon_at)
                == here
        {
            tags.push("Summon Circle");
        }
        let mut line = 0usize;
        for tag in tags {
            self.bm_text(
                view,
                1,
                GOLD,
                tag,
                142.0,
                54.0 + line as f32 * 6.5,
                Align::Left,
            );
            line += 1;
        }
        for (spell, caster) in list.iter().take(6usize.saturating_sub(line)) {
            let def = self.game.content.rules.spells.get(*spell);
            let mut name = def.name.clone();
            if def.id == "spell_ward" {
                name = format!("{name} ({})", self.game.city_wards(city).join(", "));
            }
            let color = self.banner_color(caster.0);
            self.bm_text(
                view,
                1,
                color,
                &name,
                142.0,
                54.0 + line as f32 * 6.5,
                Align::Left,
            );
            line += 1;
        }
    }

    /// What the cityscape layout needs from a city.
    fn cityscape_info(&self, city: hecs::Entity) -> crate::cityscape::CityInfo {
        use crate::cityscape as cs;
        let rules = &self.game.content.rules;
        let at = self.city_position(city);
        let pop = self
            .game
            .world
            .get::<&Population>(city)
            .map(|p| p.figures)
            .unwrap_or(0);
        let buildings = self
            .game
            .world
            .get::<&Buildings>(city)
            .map(|b| {
                b.0.iter()
                    .map(|&id| (3 + id.index() as i16, rules.buildings.get(id).shape))
                    .collect()
            })
            .unwrap_or_default();
        let enchanted = |name: &str| {
            self.game
                .world
                .get::<&Enchantments>(city)
                .is_ok_and(|e| e.0.iter().any(|&(s, _)| rules.spells.get(s).id == name))
        };
        let mut magic = Vec::new();
        for (name, idx) in [
            ("earth_gate", cs::EARTH_GATE),
            ("stream_of_life", cs::STREAM_OF_LIFE),
            ("astral_gate", cs::ASTRAL_GATE),
            ("dark_rituals", cs::DARK_RITUALS),
            ("altar_of_battle", cs::ALTAR_OF_BATTLE),
        ] {
            if enchanted(name) {
                magic.push(idx);
            }
        }
        let owner = self.game.city_owner(city);
        if owner
            .and_then(|o| self.game.player(o))
            .and_then(|p| p.summon_at)
            == Some(at)
        {
            magic.push(cs::SUMMONING_CIRCLE);
        }
        let class = self.game.terrain_class_at(at);
        let house_type = self
            .game
            .world
            .get::<&CityRace>(city)
            .map(|r| rules.races.get(r.0).house_type)
            .unwrap_or(0);
        cs::CityInfo {
            population: pop,
            buildings,
            magic,
            fortress: self.game.is_fortress_city(city),
            many_trees: class == Some(f4x_core::content::TerrainClass::Forest) || house_type == 1,
            many_rocks: matches!(
                class,
                Some(
                    f4x_core::content::TerrainClass::Mountains
                        | f4x_core::content::TerrainClass::Hills
                )
            ),
            seed: 0x2895 + u64::from(at.x) * u64::from(at.y),
        }
    }

    /// The city's picture: ground, water, hills, roads, then every cell from the back to the
    /// front (houses, trees, rocks, buildings), and the walls along the bottom.
    fn draw_city_buildings(&mut self, city: hecs::Entity, lay: Layout) {
        use crate::cityscape as cs;
        let view = Self::view(lay);
        let (x0, y0) = (4.0f32, 100.0f32);
        let at = self.city_position(city);
        let myrror = at.plane == f4x_core::map::Plane::Myrror;
        let enchanted = |name: &str| {
            self.game.world.get::<&Enchantments>(city).is_ok_and(|e| {
                e.0.iter()
                    .any(|&(s, _)| self.game.content.rules.spells.get(s).id == name)
            })
        };
        let frame = if enchanted("flying_fortress") {
            1
        } else if enchanted("cursed_lands") {
            0
        } else if enchanted("famine") {
            2
        } else if enchanted("gaias_blessing") {
            3
        } else {
            4
        };
        // Clip to the window.
        let r = lay.rect(x0, y0, 205.0, 96.0);
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        let dpi = screen_dpi_scale();
        let _ = dpi;
        if !self.city_pic(lay, "cityscap", if myrror { 8 } else { 0 }, frame, x0, y0) {
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.35, 0.5, 0.3, 1.0));
        }
        // Water and high ground around the city.
        let (mut river, mut ocean, mut mountain, mut hills) = (false, false, false, false);
        if let Some(map) = self.game.map.as_ref() {
            let grid = map.plane(at.plane);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (x, y) = (at.x as i32 + dx, at.y as i32 + dy);
                    river |= grid.has_river(x, y);
                    let class = grid
                        .get(x, y)
                        .and_then(|t| self.game.content.terrains.get(t))
                        .map(|d| d.class);
                    ocean |= class == Some(f4x_core::content::TerrainClass::Ocean);
                    if dy <= 0 && !(dy == 0 && dx != 0) {
                        mountain |= class == Some(f4x_core::content::TerrainClass::Mountains);
                        hills |= class == Some(f4x_core::content::TerrainClass::Hills);
                    }
                }
            }
        }
        if river {
            self.city_pic(lay, "cityscap", if myrror { 115 } else { 3 }, 0, x0, y0);
        } else if ocean {
            self.city_pic(lay, "cityscap", if myrror { 116 } else { 4 }, 0, x0, y0);
        }
        if mountain {
            self.city_pic(lay, "cityscap", if myrror { 9 } else { 2 }, 0, x0, y0);
        } else if hills {
            self.city_pic(lay, "cityscap", if myrror { 10 } else { 1 }, 0, x0, y0);
        }
        self.city_pic(lay, "cityscap", 5, 0, x0 + 2.0, y0 + 16.0);

        let info = self.cityscape_info(city);
        let grid = cs::make_layout(&info);
        let house_type = self
            .game
            .world
            .get::<&CityRace>(city)
            .map(|r| self.game.content.rules.races.get(r.0).house_type as usize)
            .unwrap_or(0);
        // Variants of houses, trees and rocks are drawn from the layout's own seed so they
        // stay put from frame to frame.
        let mut mouse_over: Option<i16> = None;
        let mouse: Vec2 = mouse_position().into();
        for (row, cells) in grid.iter().enumerate().skip(1) {
            for (col, &cell) in cells.iter().enumerate() {
                let (cx, cy) = cs::cell_position(row, col);
                let (sx, sy) = (x0 + cx, y0 + cy);
                let pick = |n: usize, salt: usize| (row * 31 + col * 17 + salt) % n;
                match cell {
                    cs::HOUSE if sx < x0 + 195.0 => {
                        self.city_pic(
                            lay,
                            "cityscap",
                            25 + house_type * 5 + pick(5, 1),
                            0,
                            sx,
                            sy - 19.0,
                        );
                    }
                    cs::TREE => {
                        let shift = pick(3, 2) as f32 - 1.0;
                        self.city_pic(
                            lay,
                            "cityscap",
                            19 + pick(3, 3),
                            0,
                            sx + shift,
                            sy - 6.0 - shift,
                        );
                    }
                    cs::ROCK => {
                        let shift = pick(3, 4) as f32 - 1.0;
                        self.city_pic(lay, "cityscap", 22 + pick(3, 5), 0, sx + shift, sy - 6.0);
                    }
                    idx if cs::sprite_of(idx).is_some() => {
                        let (entry, up) = cs::sprite_of(idx).expect("checked");
                        self.city_pic(lay, "cityscap", entry, 0, sx, sy - up);
                        let rect = lay.rect(sx, sy - up, 40.0, up + 1.0);
                        if rect.contains(mouse) {
                            mouse_over = Some(idx);
                        }
                    }
                    _ => {}
                }
            }
        }
        // Walls along the front.
        let has_walls = self.game.world.get::<&Buildings>(city).is_ok_and(|b| {
            b.0.iter()
                .any(|&id| self.game.content.rules.buildings.get(id).id == "city_walls")
        });
        if has_walls {
            self.city_pic(lay, "cityscap", 76, 0, x0, y0 + 82.0);
        }
        gl.flush();
        gl.quad_gl.scissor(None);

        // The name of the building under the mouse.
        let name = match mouse_over {
            Some(cs::FORTRESS) => Some("Fortress".to_string()),
            Some(cs::SUMMONING_CIRCLE) => Some("Summoning Circle".to_string()),
            Some(i) if (3..=35).contains(&i) => self
                .game
                .content
                .rules
                .buildings
                .defs()
                .get((i - 3) as usize)
                .map(|b| b.name.clone()),
            _ => None,
        };
        if let Some(n) = name {
            self.bm_text(view, 3, WHITE, &n, x0 + 102.0, y0 + 84.0, Align::Center);
        }
    }

    /// The 5x5 worked area at original tile size, with shared tiles dimmed.
    fn draw_city_area(&mut self, city: hecs::Entity, lay: Layout) {
        let at = self.city_position(city);
        let Some(map) = self.game.map.as_ref() else {
            return;
        };
        let grid = map.plane(at.plane);
        let plane_offset = if at.plane == f4x_core::map::Plane::Myrror {
            crate::art::terrain_type::MYRROR_OFFSET
        } else {
            0
        };
        let area: Vec<_> = self.game.city_area(at);
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                if dx.abs() == 2 && dy.abs() == 2 {
                    continue;
                }
                let r = lay.rect(
                    215.0 + (dx + 2) as f32 * TILE_W,
                    5.0 + (dy + 2) as f32 * TILE_H,
                    TILE_W,
                    TILE_H,
                );
                let x = grid.wrap_x(at.x as i32 + dx);
                let y = at.y as i32 + dy;
                let Some(def) = grid
                    .get(x as i32, y)
                    .and_then(|t| self.game.content.terrains.get(t))
                else {
                    draw_rectangle(r.x, r.y, r.w, r.h, BLACK);
                    continue;
                };
                match self.art.terrain(base_type(def.class) + plane_offset) {
                    Some(tex) => draw_texture_ex(
                        tex,
                        r.x,
                        r.y,
                        WHITE,
                        DrawTextureParams {
                            dest_size: Some(vec2(r.w, r.h)),
                            ..Default::default()
                        },
                    ),
                    None => draw_rectangle(r.x, r.y, r.w, r.h, DARKGREEN),
                }
                let shared = area.iter().any(|(p, s)| p.x == x && p.y as i32 == y && *s);
                if shared {
                    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.35));
                }
                if let Some(f) = grid
                    .feature(x as i32, y)
                    .and_then(|f| self.game.content.features.get(f))
                {
                    draw_circle(r.x + r.w / 2.0, r.y + r.h / 2.0, r.h * 0.18, GOLD);
                    let _ = f;
                }
                if dx == 0 && dy == 0 {
                    draw_rectangle(
                        r.x + r.w * 0.15,
                        r.y + r.h * 0.3,
                        r.w * 0.7,
                        r.h * 0.55,
                        Color::new(0.75, 0.6, 0.4, 1.0),
                    );
                    draw_triangle(
                        vec2(r.x + r.w * 0.1, r.y + r.h * 0.3),
                        vec2(r.x + r.w * 0.9, r.y + r.h * 0.3),
                        vec2(r.x + r.w * 0.5, r.y + r.h * 0.05),
                        Color::new(0.7, 0.2, 0.15, 1.0),
                    );
                }
            }
        }
    }

    fn draw_city_garrison(&mut self, city: hecs::Entity, lay: Layout) {
        let at = self.city_position(city);
        let view = Self::view(lay);
        let owner = self.game.city_owner(city).map_or(0, |o| o.0);
        for (i, u) in self.game.units_at(at).into_iter().take(9).enumerate() {
            let (col, row) = ((i % 5) as f32, (i / 5) as f32);
            let (x, y) = (216.0 + col * 19.0, 105.0 + row * 17.0);
            let frame = lay.rect(x - 1.0, y - 1.0, 18.0, 16.0);
            draw_rectangle(frame.x, frame.y, frame.w, frame.h, {
                let c = self.banner_color(owner);
                Color::new(c.r * 0.5, c.g * 0.3, c.b * 0.3, 1.0)
            });
            let art = self.game.unit_def(u).and_then(|d| d.art.clone());
            let drawn = art.is_some_and(|(lbx, entry)| {
                self.art
                    .picture(&lbx, entry as usize, 0)
                    .cloned()
                    .map(|tex| {
                        draw_texture_ex(
                            &tex,
                            frame.x + lay.scale,
                            frame.y + lay.scale,
                            WHITE,
                            DrawTextureParams {
                                dest_size: Some(vec2(16.0 * lay.scale, 14.0 * lay.scale)),
                                ..Default::default()
                            },
                        )
                    })
                    .is_some()
            });
            if !drawn {
                if let Some(def) = self.game.unit_def(u) {
                    let short: String = def.name.chars().take(4).collect();
                    self.bm_text(view, 1, WHITE, &short, x, y + 4.0, Align::Left);
                }
            }
            let (hp, max) = self.game.unit_hits(u);
            if hp < max {
                let bar_r = lay.rect(x, y + 13.0, 16.0 * hp as f32 / max as f32, 1.0);
                draw_rectangle(bar_r.x, bar_r.y, bar_r.w, bar_r.h, RED);
            }
        }
    }

    fn draw_city_production(&mut self, city: hecs::Entity, lay: Layout) {
        let prod = self.game.city_production(city);
        let eco = self.game.city_economy(city).unwrap_or_default();
        let view = Self::view(lay);
        let white = WHITE;
        match prod.item {
            Some(item) => {
                let name = self.item_name(item);
                self.bm_text(view, 3, white, &name, 246.0, 177.0, Align::Center);
                let cost = self.game.item_cost(city, item);
                if cost > 0 {
                    let remaining = cost.saturating_sub(prod.progress);
                    if eco.production > 0 {
                        let turns = remaining.div_ceil(eco.production as u32);
                        let label = if turns == 1 {
                            "1 Turn".to_string()
                        } else {
                            format!("{turns} Turns")
                        };
                        self.bm_text(
                            view,
                            3,
                            Color::new(0.95, 0.7, 0.2, 1.0),
                            &label,
                            313.0,
                            142.0,
                            Align::Right,
                        );
                    }
                    // Progress as rows of production icons, ten to a row.
                    let done = (prod.progress as i32 * 30 / cost as i32).min(30);
                    for k in 0..30 {
                        let (x, y) = (262.0 + (k % 10) as f32 * 5.0, 151.0 + (k / 10) as f32 * 7.0);
                        let tint = if k < done {
                            WHITE
                        } else {
                            Color::new(1.0, 1.0, 1.0, 0.25)
                        };
                        if let Some(tex) = self.art.picture("backgrnd", 41, 0).cloned() {
                            let at = lay.rect(x, y, 0.0, 0.0);
                            draw_texture_ex(
                                &tex,
                                at.x,
                                at.y,
                                tint,
                                DrawTextureParams {
                                    dest_size: Some(tex.size() * lay.scale),
                                    ..Default::default()
                                },
                            );
                        }
                    }
                    self.bm_text(
                        view,
                        1,
                        white,
                        &format!("{}/{}", prod.progress, cost),
                        313.0,
                        168.0,
                        Align::Right,
                    );
                } else if item == ProductionItem::TradeGoods {
                    self.bm_text(
                        view,
                        1,
                        white,
                        &format!("{} gold", eco.production / 2),
                        262.0,
                        152.0,
                        Align::Left,
                    );
                } else {
                    self.bm_text(view, 1, white, "speeds", 262.0, 152.0, Align::Left);
                    self.bm_text(view, 1, white, "growth", 262.0, 159.0, Align::Left);
                }
                // What is being made: the unit's picture.
                if let ProductionItem::Unit(id) = item {
                    let art = self.game.content.rules.units.get(id).art.clone();
                    if let Some((lbx, entry)) = art {
                        self.city_pic(lay, &lbx, entry as usize, 0, 217.0, 152.0);
                    }
                }
            }
            None => {
                self.bm_text(view, 3, GRAY, "Nothing", 246.0, 177.0, Align::Center);
            }
        }
        let gold = self.game.player(crate::view::HUMAN).map_or(0, |p| p.gold);
        let price = self.game.buy_cost(city);
        let can_buy = price.is_some_and(|p| p as i32 <= gold);
        if self.pic_button(lay, 7, 214.0, 190.0, can_buy && !self.choosing) {
            let events = self.game.apply(Command::BuyProduction { city });
            self.note(events);
        }
        if self.pic_button(lay, 8, 247.0, 190.0, !self.choosing) {
            self.choosing = true;
            self.choice_scroll = 0;
        }
        if self.pic_button(lay, 9, 286.0, 190.0, !self.choosing) {
            self.screen = Screen::Map;
        }
        if let Some(p) = price {
            self.bm_text(
                view,
                1,
                if can_buy { WHITE } else { GRAY },
                &format!("{p} gold"),
                229.0,
                181.0,
                Align::Center,
            );
        }
    }

    /// One of the original's two-frame buttons from `backgrnd.lbx`; true when clicked.
    fn pic_button(&mut self, lay: Layout, entry: usize, x: f32, y: f32, enabled: bool) -> bool {
        let Some(tex) = self.art.picture("backgrnd", entry, 0).cloned() else {
            return false;
        };
        let r = lay.rect(x, y, tex.width(), tex.height());
        let hover = r.contains(mouse_position().into());
        let down = enabled && hover && is_mouse_button_down(MouseButton::Left);
        let frame = usize::from(down);
        let tex = self
            .art
            .picture("backgrnd", entry, frame)
            .cloned()
            .unwrap_or(tex);
        draw_texture_ex(
            &tex,
            r.x,
            r.y,
            if enabled {
                WHITE
            } else {
                Color::new(1.0, 1.0, 1.0, 0.45)
            },
            DrawTextureParams {
                dest_size: Some(vec2(r.w, r.h)),
                ..Default::default()
            },
        );
        enabled && hover && is_mouse_button_pressed(MouseButton::Left)
    }

    fn draw_production_menu(&mut self, city: hecs::Entity, lay: Layout) {
        let panel = lay.rect(20.0, 14.0, 280.0, 170.0);
        draw_rectangle(
            panel.x,
            panel.y,
            panel.w,
            panel.h,
            Color::new(0.1, 0.08, 0.05, 0.97),
        );
        draw_rectangle_lines(panel.x, panel.y, panel.w, panel.h, 3.0, GOLD);
        lay.text(
            "Choose what to build   (wheel scrolls, right-click cancels)",
            26.0,
            24.0,
            6.0,
            GOLD,
        );
        let items = self.buildable_items(city);
        let per_page = 18usize;
        let columns = 2usize;
        let page = per_page * columns;
        let (_, wheel) = mouse_wheel();
        if wheel != 0.0 {
            let max = items.len().saturating_sub(page);
            self.choice_scroll = (self.choice_scroll as i32 - wheel.signum() as i32 * 2)
                .clamp(0, max as i32) as usize;
        }
        let eco = self.game.city_economy(city).unwrap_or_default();
        if is_mouse_button_pressed(MouseButton::Right) {
            self.choosing = false;
            return;
        }
        for (slot, item) in items.iter().skip(self.choice_scroll).take(page).enumerate() {
            let (col, row) = ((slot / per_page) as f32, (slot % per_page) as f32);
            let r = lay.rect(26.0 + col * 138.0, 30.0 + row * 8.2, 134.0, 7.6);
            let hover = r.contains(mouse_position().into());
            if hover {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.4, 0.3, 0.15, 1.0));
            }
            let cost = self.game.item_cost(city, *item);
            let detail = match item {
                ProductionItem::TradeGoods => "gold".to_string(),
                ProductionItem::Housing => "growth".to_string(),
                _ => {
                    let turns = if eco.production > 0 {
                        cost.div_ceil(eco.production as u32).to_string()
                    } else {
                        "-".into()
                    };
                    format!("{cost}  {turns}t")
                }
            };
            lay.text(
                &self.item_name(*item),
                27.0 + col * 138.0,
                35.5 + row * 8.2,
                5.0,
                WHITE,
            );
            let dims = measure_text(&detail, None, (5.0 * lay.scale) as u16, 1.0);
            draw_text(
                &detail,
                r.x + r.w - dims.width - 4.0,
                r.y + r.h - 4.0,
                5.0 * lay.scale,
                Color::new(0.8, 0.8, 0.6, 1.0),
            );
            if hover && is_mouse_button_pressed(MouseButton::Left) {
                let events = self
                    .game
                    .apply(Command::SetProduction { city, item: *item });
                self.note(events);
                self.choosing = false;
            }
        }
    }

    /// Everything the city may start building: buildings, then units, then the two specials.
    fn buildable_items(&self, city: hecs::Entity) -> Vec<ProductionItem> {
        let rules = &self.game.content.rules;
        let mut items = Vec::new();
        for (id, _) in rules.buildings.iter() {
            let item = ProductionItem::Building(id);
            if self.game.can_build(city, item).is_ok() {
                items.push(item);
            }
        }
        for (id, _) in rules.units.iter() {
            let item = ProductionItem::Unit(id);
            if self.game.can_build(city, item).is_ok() {
                items.push(item);
            }
        }
        items.push(ProductionItem::TradeGoods);
        items.push(ProductionItem::Housing);
        items
    }
}

/// 17930 as "17,930".
fn thousands(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
