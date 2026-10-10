//! The city screen: economy, buildings, worked area and the production list, drawn on
//! the original screen layout (320x200) when the art is available.

use f4x_core::city::Position;
use f4x_core::economy::{Buildings, CityRace, Enchantments, Farmers, Population};
use f4x_core::production::ProductionItem;
use f4x_core::Command;
use macroquad::prelude::*;

use crate::art::{TILE_H, TILE_W};
use crate::frame::Align;
use crate::ui::{player_color, GOLD};
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
                [(((eco.figures + 3) / 4).clamp(1, 5) - 1) as usize]
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
        let people = pop.figures * 1000 + u32::from(pop.tens) * 10;
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
        // City tags (fortress, summoning circle) under the citizens.
        let here = self.game.world.get::<&Position>(city).map(|p| *p).ok();
        let owner = self.game.city_owner(city);
        let mut tags = Vec::new();
        if self.game.is_fortress_city(city) {
            tags.push("Fortress");
        }
        if owner
            .and_then(|o| self.game.player(o))
            .and_then(|p| p.summon_at)
            == here
            && here.is_some()
        {
            tags.push("Summoning circle");
        }
        if !tags.is_empty() {
            self.bm_text(view, 1, WHITE, &tags.join("  "), 207.0, 42.0, Align::Right);
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
        for (i, (spell, caster)) in list.iter().take(6).enumerate() {
            let name = self.game.content.rules.spells.get(*spell).name.clone();
            self.bm_text(
                view,
                1,
                player_color(caster.0),
                &name,
                142.0,
                54.0 + i as f32 * 6.5,
                Align::Left,
            );
        }
    }

    /// The building area: a backdrop and the buildings as a list until the original's
    /// cityscape is ported (task 114).
    fn draw_city_buildings(&mut self, city: hecs::Entity, lay: Layout) {
        let view = Self::view(lay);
        if !self.city_pic(lay, "cityscap", 0, 0, 4.0, 100.0) {
            let r = lay.rect(4.0, 100.0, 205.0, 90.0);
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.35, 0.5, 0.3, 1.0));
        }
        let Ok(buildings) = self.game.world.get::<&Buildings>(city) else {
            return;
        };
        let names: Vec<String> = buildings
            .0
            .iter()
            .map(|&b| self.game.content.rules.buildings.get(b).name.clone())
            .collect();
        drop(buildings);
        if names.is_empty() {
            self.bm_text(view, 1, WHITE, "No buildings yet", 12.0, 106.0, Align::Left);
        }
        for (i, name) in names.iter().enumerate() {
            let (col, row) = ((i % 3) as f32, (i / 3) as f32);
            self.bm_text(
                view,
                1,
                WHITE,
                name,
                8.0 + col * 68.0,
                104.0 + row * 8.0,
                Align::Left,
            );
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
                let c = player_color(owner);
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
                self.bm_text(view, 3, white, &name, 264.0, 178.0, Align::Center);
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
                        &format!("{} gold a turn", eco.production / 2),
                        262.0,
                        152.0,
                        Align::Left,
                    );
                } else {
                    self.bm_text(view, 1, white, "speeds growth", 262.0, 152.0, Align::Left);
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
                self.bm_text(view, 3, GRAY, "Nothing", 264.0, 178.0, Align::Center);
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
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
