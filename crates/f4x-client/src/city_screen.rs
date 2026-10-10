//! The city screen: economy, buildings, worked area and the production list, drawn on
//! the original screen layout (320x200) when the art is available.

use f4x_core::city::Position;
use f4x_core::economy::{Buildings, CityRace, Enchantments, Farmers, Population};
use f4x_core::production::ProductionItem;
use f4x_core::Command;
use macroquad::prelude::*;

use crate::art::{TILE_H, TILE_W};
use crate::ui::{bar, button, GOLD, INK};
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

    fn draw_city_header(&mut self, city: hecs::Entity, lay: Layout) {
        let name = self.city_name(city);
        let race = self
            .game
            .world
            .get::<&CityRace>(city)
            .map(|r| self.game.content.rules.races.get(r.0).name.clone())
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
        let owner = self.game.city_owner(city);
        let mut tags = String::new();
        if self.game.is_fortress_city(city) {
            tags.push_str("  [Fortress]");
        }
        let here = self.game.world.get::<&Position>(city).map(|p| *p).ok();
        if owner
            .and_then(|o| self.game.player(o))
            .and_then(|p| p.summon_at)
            == here
            && here.is_some()
        {
            tags.push_str("  [Summoning circle]");
        }
        lay.text(&format!("{name} - {race}{tags}"), 10.0, 14.0, 9.0, GOLD);
        let outpost = if eco.figures == 0 { "  (outpost)" } else { "" };
        lay.text(
            &format!(
                "Pop {} ({:+}/turn){}   max {}",
                pop.figures,
                eco.growth_tens * 10,
                outpost,
                eco.max_size
            ),
            10.0,
            24.0,
            5.5,
            WHITE,
        );
        lay.text(
            &format!(
                "Farmers {}   Workers {}   Rebels {}",
                eco.farmers, eco.workers, eco.rebels
            ),
            10.0,
            38.0,
            5.5,
            INK,
        );
        let farmers = self.game.world.get::<&Farmers>(city).map_or(0, |f| f.0);
        let has_people = eco.figures > 0;
        if button(
            lay.rect(150.0, 31.0, 10.0, 9.0),
            "-",
            has_people && farmers > 0,
        ) {
            self.game.apply(Command::SetFarmers {
                city,
                farmers: farmers.saturating_sub(1),
            });
        }
        if button(
            lay.rect(163.0, 31.0, 10.0, 9.0),
            "+",
            has_people && farmers < eco.figures,
        ) {
            self.game.apply(Command::SetFarmers {
                city,
                farmers: farmers + 1,
            });
        }
    }

    fn draw_city_resources(&mut self, city: hecs::Entity, lay: Layout) {
        let Some(eco) = self.game.city_economy(city) else {
            return;
        };
        let figures = eco.figures as i32;
        let net = eco.gold - eco.gold_upkeep;
        let lines = [
            format!(
                "Food {} / {}  ({:+})",
                eco.food,
                figures,
                eco.food - figures
            ),
            format!("Production {}", eco.production),
            format!("Gold {:+}  ({} - {})", net, eco.gold, eco.gold_upkeep),
            format!("Magic power {}", eco.mana),
            format!("Research {}", eco.research),
            format!("Units cost {}%", eco.unit_cost_percent),
        ];
        for (i, l) in lines.iter().enumerate() {
            lay.text(l, 8.0, 59.0 + i as f32 * 6.5, 5.0, INK);
        }
        let rules = &self.game.content.rules;
        if let Ok(e) = self.game.world.get::<&Enchantments>(city) {
            for (i, (spell, _)) in e.0.iter().take(6).enumerate() {
                lay.text(
                    &rules.spells.get(*spell).name,
                    140.0,
                    64.0 + i as f32 * 5.5,
                    4.5,
                    INK,
                );
            }
        }
    }

    fn draw_city_buildings(&mut self, city: hecs::Entity, lay: Layout) {
        let rules = &self.game.content.rules;
        lay.text("Built:", 10.0, 108.0, 5.0, Color::new(0.1, 0.1, 0.5, 1.0));
        let Ok(buildings) = self.game.world.get::<&Buildings>(city) else {
            return;
        };
        if buildings.0.is_empty() {
            lay.text("nothing yet", 10.0, 116.0, 4.6, GRAY);
        }
        for (i, &b) in buildings.0.iter().enumerate() {
            let (col, row) = ((i % 3) as f32, (i / 3) as f32);
            let def = rules.buildings.get(b);
            lay.text(&def.name, 10.0 + col * 66.0, 116.0 + row * 6.5, 4.6, BLACK);
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
        for (i, u) in self.game.units_at(at).into_iter().take(5).enumerate() {
            if let Some(def) = self.game.unit_def(u) {
                let (hp, max) = self.game.unit_hits(u);
                lay.text(
                    &format!("{} {hp}/{max}", def.name),
                    216.0,
                    114.0 + i as f32 * 5.5,
                    4.4,
                    BLACK,
                );
            }
        }
    }

    fn draw_city_production(&mut self, city: hecs::Entity, lay: Layout) {
        let prod = self.game.city_production(city);
        let eco = self.game.city_economy(city).unwrap_or_default();
        match prod.item {
            Some(item) => {
                lay.text(&self.item_name(item), 216.0, 153.0, 5.0, BLACK);
                let cost = self.game.item_cost(city, item);
                if cost > 0 {
                    let remaining = cost.saturating_sub(prod.progress);
                    let turns = if eco.production > 0 {
                        remaining.div_ceil(eco.production as u32)
                    } else {
                        0
                    };
                    lay.text(
                        &format!(
                            "{}/{}  ({} turns)",
                            prod.progress,
                            cost,
                            if eco.production > 0 {
                                turns.to_string()
                            } else {
                                "never".into()
                            }
                        ),
                        216.0,
                        164.0,
                        4.4,
                        BLACK,
                    );
                    bar(
                        lay.rect(216.0, 168.0, 96.0, 4.0),
                        prod.progress as f32 / cost as f32,
                        Color::new(0.2, 0.7, 0.3, 1.0),
                    );
                } else if item == ProductionItem::TradeGoods {
                    lay.text(
                        &format!(
                            "converts {} production to {} gold",
                            eco.production,
                            eco.production / 2
                        ),
                        216.0,
                        164.0,
                        4.0,
                        BLACK,
                    );
                } else {
                    lay.text("speeds growth", 216.0, 160.0, 4.4, BLACK);
                }
            }
            None => lay.text("nothing", 216.0, 153.0, 5.0, GRAY),
        }
        let gold = self.game.player(crate::view::HUMAN).map_or(0, |p| p.gold);
        let price = self.game.buy_cost(city);
        let label = price.map_or("BUY".to_string(), |p| format!("BUY {p}"));
        if button(
            lay.rect(214.0, 187.0, 40.0, 10.0),
            &label,
            price.is_some_and(|p| p as i32 <= gold),
        ) {
            let events = self.game.apply(Command::BuyProduction { city });
            self.note(events);
        }
        if button(lay.rect(258.0, 187.0, 34.0, 10.0), "CHANGE", !self.choosing) {
            self.choosing = true;
            self.choice_scroll = 0;
        }
        if button(lay.rect(296.0, 187.0, 20.0, 10.0), "OK", !self.choosing) {
            self.screen = Screen::Map;
        }
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
