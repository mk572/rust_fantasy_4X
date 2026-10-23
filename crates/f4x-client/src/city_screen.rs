//! The city screen: economy, buildings, worked area and the production list, drawn on
//! the original screen layout (320x200) when the art is available.

use f4x_core::city::production::ProductionItem;
use f4x_core::game::Command;
use f4x_core::map::Position;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::art::{TILE_H, TILE_W};
use crate::frame::Align;
use crate::sell_prompt::SellPrompt;
use crate::ui::GOLD;
use crate::view::{App, Screen, HUMAN};

/// Virtual-to-screen mapping for the 320x200 layout.
#[derive(Clone, Copy)]
pub(crate) struct Layout {
    pub(crate) ox: f32,
    pub(crate) oy: f32,
    pub(crate) scale: f32,
}

impl Layout {
    pub(crate) fn new() -> Self {
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

    pub(crate) fn rect(self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(
            self.ox + x * self.scale,
            self.oy + y * self.scale,
            w * self.scale,
            h * self.scale,
        )
    }

    pub(crate) fn text(self, s: &str, x: f32, y: f32, size: f32, color: Color) {
        draw_text(
            s,
            self.ox + x * self.scale,
            self.oy + y * self.scale,
            size * self.scale,
            color,
        );
    }
}

/// The top left of the first unit in the garrison window.
const GARRISON: (f32, f32) = (215.0, 102.0);

impl App {
    pub(crate) fn city_screen(&mut self, city: hecs::Entity) {
        if self
            .game
            .economy()
            .city_at(self.city_position(city))
            .is_none()
        {
            self.screen = Screen::Map;
            return;
        }
        let lay = Layout::new();
        let dialog = self.autobuild_dialog.is_some();
        if dialog && is_key_pressed(KeyCode::Escape) {
            self.autobuild_dialog = None;
            return;
        }
        let selling = self.sell_prompt.is_some();
        let renaming = self.city_naming.is_some();
        if renaming && is_key_pressed(KeyCode::Escape) {
            self.city_naming = None;
            return;
        }
        let choosing = self.choosing || dialog || selling || renaming;
        // A right-click on a unit of the garrison opens its statistics sheet.
        if is_mouse_button_pressed(MouseButton::Right) && !choosing {
            let over = self
                .garrison_cells(city, lay)
                .into_iter()
                .find(|(r, _)| r.contains(mouse_position().into()));
            if let Some((_, unit)) = over {
                self.open_unit_sheet(unit);
                return;
            }
        }
        // A left-click on a unit of the garrison picks it, or lets it go.
        if is_mouse_button_pressed(MouseButton::Left) && !choosing {
            let over = self
                .garrison_cells(city, lay)
                .into_iter()
                .find(|(r, _)| r.contains(mouse_position().into()));
            if let Some((_, unit)) = over {
                let here = self.city_position(city);
                let elsewhere = self
                    .selection
                    .iter()
                    .any(|&u| self.game.units().unit_position(u) != Some(here));
                if elsewhere {
                    self.selection.clear();
                }
                match self.selection.iter().position(|&u| u == unit) {
                    Some(i) => {
                        self.selection.remove(i);
                    }
                    None => self.selection.push(unit),
                }
            }
        }
        if !selling
            && !renaming
            && (is_key_pressed(KeyCode::Escape)
                || (is_mouse_button_pressed(MouseButton::Right) && !choosing))
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
        // While the auto-builder dialog is open the screen behind it takes no input.
        let was_choosing = self.choosing;
        if dialog || selling || renaming {
            self.choosing = true;
        }
        self.draw_city_header(city, lay);
        self.draw_city_resources(city, lay);
        self.draw_city_buildings(city, lay);
        self.draw_city_area(city, lay);
        self.draw_city_garrison(city, lay);
        self.draw_city_production(city, lay);
        self.draw_autobuild_button(city, lay);
        if renaming {
            self.choosing = was_choosing;
            self.draw_rename_box(city);
        } else if dialog {
            self.choosing = was_choosing;
            self.draw_autobuild_dialog(city, lay);
        } else if selling {
            self.choosing = was_choosing;
            self.draw_sell_prompt();
        } else if self.choosing {
            self.draw_production_menu(city, lay);
        }
    }

    pub(crate) fn city_position(&self, city: hecs::Entity) -> Position {
        self.game.city_position(city).unwrap_or(Position {
            plane: f4x_core::map::Plane::Arcanus,
            x: 0,
            y: 0,
        })
    }

    pub(crate) fn view(lay: Layout) -> (f32, f32, f32) {
        (lay.ox, lay.oy, lay.scale)
    }

    /// A picture from the local art at the original's `(x, y)`; false if it is missing.
    pub(crate) fn city_pic(
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

    /// The box for a new name for the city (the original's name box); Enter keeps what is typed,
    /// an empty name keeps the old one.
    fn draw_rename_box(&mut self, city: hecs::Entity) {
        let Some(mut name) = self.city_naming.take() else {
            return;
        };
        let lay = crate::frame::Layout::new();
        if self.name_box(
            &lay,
            (60.0, 60.0),
            &self.text("city.name_prompt"),
            &mut name,
        ) {
            let events = self.game.apply(Command::RenameCity { city, name });
            self.note(events);
        } else {
            self.city_naming = Some(name);
        }
    }

    fn draw_city_header(&mut self, city: hecs::Entity, lay: Layout) {
        let name = self.city_name(city);
        let (race_name, race_index) = self
            .game
            .city_race(city)
            .map(|r| {
                (
                    self.game.content().rules.races.get(r).name.clone(),
                    r.index(),
                )
            })
            .unwrap_or_default();
        let Some(eco) = self.game.economy().city_economy(city) else {
            return;
        };
        let pop = self.game.city_population(city);
        // The title names the size: Outpost, Hamlet, Village, Town, City, Capital.
        let title = self.text_with(
            "city.title",
            &Args::new()
                .text("size", self.city_size_name(city))
                .text("city", &name),
        );
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
        // A click on the name asks for a new one.
        if is_mouse_button_pressed(MouseButton::Left)
            && !self.choosing
            && lay
                .rect(0.0, 0.0, 210.0, 17.0)
                .contains(mouse_position().into())
        {
            self.city_naming = Some(name.clone());
        }
        let orange = Color::new(0.95, 0.7, 0.2, 1.0);
        let mut race_name = race_name;
        let serial = self.game.serial_of(city);
        for (kind, mark) in [
            (f4x_core::world_event::EventKind::Plague, "city.plague"),
            (
                f4x_core::world_event::EventKind::PopulationBoom,
                "city.pop_boom",
            ),
        ] {
            if self.game.events().on_city(kind, serial) {
                race_name += &self.text(mark);
            }
        }
        self.bm_text(view, 3, orange, &race_name, 6.0, 19.0, Align::Left);
        let people = pop.figures * 1000 + pop.tens * 10;
        let grow = eco.growth_tens * 10;
        self.bm_text(
            view,
            3,
            orange,
            &self.text_with(
                "city.population",
                &Args::new()
                    .text("people", thousands(people))
                    .text("growth", format!("{grow:+}")),
            ),
            207.0,
            19.0,
            Align::Right,
        );
        // The citizens: farmers, then workers, then rebels. Clicking one changes its job.
        let farmers = self.game.city_farmers(city);
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
        let Some(eco) = self.game.economy().city_economy(city) else {
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
        self.draw_city_enchantments(city, lay);
    }

    /// The enchantments on the city, six to a page in the colour of whoever cast them, with the
    /// arrows that turn the page; a click on the player's own asks to turn it off
    /// (`City_Screen_Draw_City_Enchantments`, `City_Add_Fields_City_Enchantments`).
    fn draw_city_enchantments(&mut self, city: hecs::Entity, lay: Layout) {
        let enabled = !self.choosing;
        self.city_enchantment_list(city, lay, (139.0, 51.0), 3.0, enabled);
    }

    /// The list with its first row at `first` (the original's `xstart`, `ystart`), shared by
    /// the city and outpost screens. Returns whether a click this frame landed on a row or an
    /// arrow, so the caller does not take it for something else. The names are drawn `shift`
    /// pixels right of and below the clickable rows (the city screen's panel has a margin).
    pub(crate) fn city_enchantment_list(
        &mut self,
        city: hecs::Entity,
        lay: Layout,
        first: (f32, f32),
        shift: f32,
        enabled: bool,
    ) -> bool {
        const PAGE: usize = 6;
        let mut hit = false;
        let view = Self::view(lay);
        let list = self.game.city_enchantments(city);
        if list.len() > PAGE {
            let last = list.len() - 1;
            let up = lay.rect(first.0 + 62.0, first.1 - 1.0, 12.0, 12.0);
            hit |=
                is_mouse_button_pressed(MouseButton::Left) && up.contains(mouse_position().into());
            if self.pic_button(lay, 15, first.0 + 62.0, first.1 - 1.0, enabled) {
                self.city_spell_first = match self.city_spell_first {
                    0 => last / PAGE * PAGE,
                    n => n.saturating_sub(PAGE),
                };
            }
            let down = lay.rect(first.0 + 62.0, first.1 + 34.0, 12.0, 12.0);
            hit |= is_mouse_button_pressed(MouseButton::Left)
                && down.contains(mouse_position().into());
            if self.pic_button(lay, 16, first.0 + 62.0, first.1 + 34.0, enabled) {
                self.city_spell_first += PAGE;
                if self.city_spell_first > last {
                    self.city_spell_first = 0;
                }
            }
        } else {
            self.city_spell_first = 0;
        }
        let mouse = mouse_position().into();
        let click = is_mouse_button_pressed(MouseButton::Left) && enabled;
        for (row, &(spell, caster)) in list
            .iter()
            .enumerate()
            .skip(self.city_spell_first)
            .take(PAGE)
        {
            let i = (row - self.city_spell_first) as f32;
            let def = self.game.content().rules.spells.get(spell).clone();
            let mut name = def.name.clone();
            if def.id == "spell_ward" {
                name = self.text_with(
                    "city.spell_ward",
                    &Args::new()
                        .text("spell", &name)
                        .text("wards", self.game.economy().city_wards(city).join(", ")),
                );
            }
            let color = self.banner_color(caster.0);
            self.bm_text(
                view,
                1,
                color,
                &name,
                first.0 + shift,
                first.1 + shift + i * 7.0,
                Align::Left,
            );
            let rect = lay.rect(first.0, first.1 + i * 7.0, 60.0, 7.0);
            hit |= click && rect.contains(mouse);
            if click && caster == HUMAN && rect.contains(mouse) {
                self.sell_prompt = Some(SellPrompt::TurnOff {
                    city,
                    spell,
                    name: def.name.clone(),
                });
            }
        }
        hit
    }

    /// The title's size word: Outpost, Hamlet, Village, Town, City, or Capital for the
    /// wizard's fortress.
    pub(crate) fn city_size_name(&self, city: hecs::Entity) -> String {
        let figures = self
            .game
            .economy()
            .city_economy(city)
            .map_or(0, |e| e.figures);
        let key = if figures == 0 {
            "city.size_outpost"
        } else if self.game.economy().is_fortress_city(city) {
            "city.size_capital"
        } else {
            [
                "city.size_hamlet",
                "city.size_village",
                "city.size_town",
                "city.size_city",
                "city.size_capital",
            ][(figures.div_ceil(4).clamp(1, 5) - 1) as usize]
        };
        self.text(key)
    }

    /// What the cityscape layout needs from a city.
    fn cityscape_info(&self, city: hecs::Entity) -> crate::cityscape::CityInfo {
        use crate::cityscape as cs;
        let rules = &self.game.content().rules;
        let at = self.city_position(city);
        let pop = self.game.city_population(city).figures;
        let buildings = self
            .game
            .city_buildings(city)
            .into_iter()
            .map(|id| (3 + id.index() as i16, rules.buildings.get(id).shape))
            .collect();
        let enchanted = |name: &str| {
            self.game
                .city_enchantments(city)
                .iter()
                .any(|&(s, _)| rules.spells.get(s).id == name)
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
        let owner = self.game.economy().city_owner(city);
        if owner
            .and_then(|o| self.game.wizard(o))
            .and_then(|w| w.summon_at())
            == Some(at)
        {
            magic.push(cs::SUMMONING_CIRCLE);
        }
        let class = self.game.economy().terrain_class_at(at);
        let house_type = self
            .game
            .city_race(city)
            .map(|r| rules.races.get(r).house_type)
            .unwrap_or(0);
        cs::CityInfo {
            population: pop,
            buildings,
            magic,
            fortress: self.game.economy().is_fortress_city(city),
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
        let over = self.draw_city_buildings_at(city, lay, 4.0, 100.0);
        if let Some(i) = over.filter(|i| (3..=35).contains(i)) {
            if is_mouse_button_pressed(MouseButton::Left) && !self.choosing {
                self.start_sale(city, i);
            }
        }
    }

    /// The same picture with its top-left corner at `(x0, y0)` (the enemy city screen's).
    pub(crate) fn draw_city_buildings_at(
        &mut self,
        city: hecs::Entity,
        lay: Layout,
        x0: f32,
        y0: f32,
    ) -> Option<i16> {
        use crate::cityscape as cs;
        let view = Self::view(lay);
        let at = self.city_position(city);
        let myrror = at.plane == f4x_core::map::Plane::Myrror;
        let enchanted = |name: &str| {
            self.game
                .city_enchantments(city)
                .iter()
                .any(|&(s, _)| self.game.content().rules.spells.get(s).id == name)
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
        if let Some(map) = self.game.map() {
            let grid = map.plane(at.plane);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (x, y) = (at.x as i32 + dx, at.y as i32 + dy);
                    river |= grid.has_river(x, y);
                    let class = grid
                        .get(x, y)
                        .and_then(|t| self.game.content().terrains.get(t))
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
            .city_race(city)
            .map(|r| self.game.content().rules.races.get(r).house_type as usize)
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
        let has_walls = self
            .game
            .city_buildings(city)
            .iter()
            .any(|&id| self.game.content().rules.buildings.get(id).id == "city_walls");
        if has_walls {
            self.city_pic(lay, "cityscap", 76, 0, x0, y0 + 82.0);
        }
        gl.flush();
        gl.quad_gl.scissor(None);

        // The name of the building under the mouse.
        let name = match mouse_over {
            Some(cs::FORTRESS) => Some(self.text("city.fortress")),
            Some(cs::SUMMONING_CIRCLE) => Some(self.text("city.summoning_circle")),
            Some(i) if (3..=35).contains(&i) => self
                .game
                .content()
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
        mouse_over
    }

    /// The 5x5 worked area at original tile size, drawn as the map is (`City_Screen_Draw_Map`):
    /// terrain, resources and roads, the corners shaded, the cities on it, and a mark on the squares another city
    /// of the player also works.
    fn draw_city_area(&mut self, city: hecs::Entity, lay: Layout) {
        let at = self.city_position(city);
        let Some(map) = self.game.map() else {
            return;
        };
        let (width, height) = (map.width() as i32, map.height() as i32);
        let area: Vec<_> = self.game.economy().city_area(at);
        let tile = vec2(TILE_W * lay.scale, TILE_H * lay.scale);
        let anim = (get_time() * 4.0) as usize;
        // A click on the map shows or hides each square's food and production.
        if is_mouse_button_pressed(MouseButton::Left)
            && !self.choosing
            && lay
                .rect(215.0, 4.0, 5.0 * TILE_W, 5.0 * TILE_H)
                .contains(mouse_position().into())
        {
            self.city_area_figures = !self.city_area_figures;
        }
        let gaia = self
            .game
            .economy()
            .city_has_enchantment(city, "gaias_blessing");
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let corner = dx.abs() == 2 && dy.abs() == 2;
                let (x, y) = ((at.x as i32 + dx).rem_euclid(width), at.y as i32 + dy);
                let (cx, cy) = (
                    215.0 + (dx + 2) as f32 * TILE_W,
                    4.0 + (dy + 2) as f32 * TILE_H,
                );
                let r = lay.rect(cx, cy, TILE_W, TILE_H);
                if !(0..height).contains(&y) {
                    draw_rectangle(r.x, r.y, r.w, r.h, BLACK);
                    continue;
                }
                crate::map_draw::draw_square(
                    &self.game,
                    &mut self.art,
                    at.plane,
                    x as u32,
                    y as u32,
                    r.x,
                    r.y,
                    tile,
                    anim,
                    self.prefs.get(crate::preferences::Pref::ShowNodeOwners),
                );
                if corner {
                    // The corners are shaded (`Gradient_Fill` with a remap block).
                    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.5));
                    continue;
                }
                let square = Position {
                    plane: at.plane,
                    x: x as u32,
                    y: y as u32,
                };
                if let Some(here) = self.game.economy().city_at(square) {
                    crate::map_draw::draw_city(&self.game, &mut self.art, here, r.x, r.y, tile);
                }
                let shared = area
                    .iter()
                    .any(|(p, shared)| *shared && p.x == x as u32 && p.y == y as u32);
                if shared {
                    self.city_pic(lay, "backgrnd", 0, 0, cx + 7.0, cy + 4.0);
                }
                if self.city_area_figures {
                    let (food, production) = self.game.map().map_or((0, 0), |m| {
                        let eco = self.game.economy();
                        let half = |v: i32| if shared { v / 2 } else { v };
                        let g = m.plane(at.plane);
                        (
                            half(eco.square_food(g, x, y)),
                            half(eco.square_production(g, x, y, gaia)),
                        )
                    });
                    // Food as stacked bread, half a loaf cut across the diagonal; production
                    // as one hammer for each percent.
                    self.draw_icon_stack(lay, 40, food / 2, food % 2 == 1, cx + 1.0, cy + 1.0);
                    self.draw_icon_stack(lay, 41, production, false, cx + 1.0, cy + 9.0);
                }
            }
        }
    }

    /// The garrison's unit squares and the unit on each (at most nine).
    fn garrison_cells(&self, city: hecs::Entity, lay: Layout) -> Vec<(Rect, hecs::Entity)> {
        let at = self.city_position(city);
        self.game
            .units()
            .units_at(at)
            .into_iter()
            .take(9)
            .enumerate()
            .map(|(i, u)| {
                let (col, row) = ((i % 5) as f32, (i / 5) as f32);
                let (x, y) = (GARRISON.0 + col * 20.0, GARRISON.1 + row * 17.0);
                (lay.rect(x, y, 20.0, 18.0), u)
            })
            .collect()
    }

    /// The units in the city on their banner backgrounds (`City_Screen_Draw_Garrison_Window`),
    /// five to a row; the ones picked (by a click) have a gold frame.
    fn draw_city_garrison(&mut self, city: hecs::Entity, _lay: Layout) {
        let at = self.city_position(city);
        let lay = crate::frame::Layout::new();
        for (i, u) in self
            .game
            .units()
            .units_at(at)
            .into_iter()
            .take(9)
            .enumerate()
        {
            let spot = (
                GARRISON.0 + (i % 5) as f32 * 20.0,
                GARRISON.1 + (i / 5) as f32 * 17.0,
            );
            self.draw_unit_picture(&lay, u, spot, true, WHITE);
            if self.selection.contains(&u) {
                let frame = lay.rect((spot.0, spot.1, 20.0, 18.0));
                draw_rectangle_lines(frame.x, frame.y, frame.w, frame.h, 2.0, GOLD);
            }
        }
    }

    /// The progress as coins, ten production points to a coin and ten coins to a row: the full
    /// coin (`backgrnd` 12) for each ten made, the empty one (11) for the rest, and half a coin
    /// cut off for a partial one (`City_Screen_Draw_Production_Coins`).
    fn draw_production_coins(&mut self, lay: Layout, progress: u32, cost: u32) {
        let (mut per_row, mut space) = (10u32, 5.0f32);
        let coins = (cost / 10).max(1);
        if coins > 50 {
            (per_row, space) = (20, 2.0);
        }
        let (whole, part) = (progress / 10, progress % 10);
        for k in 0..coins {
            let x = 262.0 + (k % per_row) as f32 * space;
            let y = 151.0 + (k / per_row) as f32 * 6.0;
            if k < whole {
                self.city_pic(lay, "backgrnd", 12, 0, x, y);
            } else {
                self.city_pic(lay, "backgrnd", 11, 0, x, y);
                if k == whole && part > 0 {
                    // The left part of the full coin over the empty one.
                    if let Some(tex) = self.art.picture("backgrnd", 12, 0).cloned() {
                        let width = (part / 2) as f32;
                        let at = lay.rect(x, y, width, tex.height());
                        draw_texture_ex(
                            &tex,
                            at.x,
                            at.y,
                            WHITE,
                            DrawTextureParams {
                                dest_size: Some(vec2(at.w, tex.height() * lay.scale)),
                                source: Some(Rect::new(0.0, 0.0, width, tex.height())),
                                ..Default::default()
                            },
                        );
                    }
                }
            }
        }
    }

    /// `whole` icons of `backgrnd` entry `entry` in a row that overlaps as far as it must to fit
    /// the 18 pixels of a square, and a half icon (cut along its diagonal) after them.
    fn draw_icon_stack(
        &mut self,
        lay: Layout,
        entry: usize,
        whole: i32,
        half: bool,
        x: f32,
        y: f32,
    ) {
        let Some(tex) = self.art.picture("backgrnd", entry, 0).cloned() else {
            return;
        };
        let slots = whole.max(0) + i32::from(half);
        if slots == 0 {
            return;
        }
        let step = if slots > 1 {
            ((18.0 - tex.width()) / (slots - 1) as f32).clamp(0.5, tex.width())
        } else {
            0.0
        };
        for k in 0..slots {
            let at = lay.rect(x + k as f32 * step, y, 0.0, 0.0);
            let (w, h) = (tex.width() * lay.scale, tex.height() * lay.scale);
            if half && k == slots - 1 {
                // The half loaf: the triangle above the diagonal from the top right.
                let corner = |dx: f32, dy: f32, u: f32, v: f32| {
                    Vertex::new(at.x + dx, at.y + dy, 0.0, u, v, WHITE)
                };
                draw_mesh(&Mesh {
                    vertices: vec![
                        corner(0.0, 0.0, 0.0, 0.0),
                        corner(w, 0.0, 1.0, 0.0),
                        corner(0.0, h, 0.0, 1.0),
                    ],
                    indices: vec![0, 1, 2],
                    texture: Some(tex.clone()),
                });
            } else {
                draw_texture_ex(
                    &tex,
                    at.x,
                    at.y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(w, h)),
                        ..Default::default()
                    },
                );
            }
        }
    }

    /// A `cityscap` picture centred in the 44 by 31 pixels at (215, 147).
    fn draw_centred_building(&mut self, lay: Layout, entry: usize) {
        let Some(size) = self.art.picture("cityscap", entry, 0).map(|t| t.size()) else {
            return;
        };
        let x = 215.0 + ((44.0 - size.x) / 2.0).floor();
        let y = 147.0 + ((31.0 - size.y) / 2.0).floor();
        self.city_pic(lay, "cityscap", entry, 0, x, y);
    }

    /// What is being made, in the window's corner (`City_Screen_Draw2__WIP`): a unit's figures on
    /// a diamond, a building's picture from the cityscape centred in 44 by 31 pixels.
    fn draw_product_picture(&mut self, lay: Layout, city: hecs::Entity, item: ProductionItem) {
        match item {
            ProductionItem::Unit(id) => {
                let def = self.game.content().rules.units.get(id).clone();
                let Some(ty) = def
                    .art
                    .as_ref()
                    .and_then(|(lbx, entry)| crate::art::unit_type_number(lbx, *entry as usize))
                else {
                    return;
                };
                let ships = def
                    .stats
                    .get(&f4x_core::rules::Stat::Transport)
                    .is_some_and(|&t| t > 0);
                if ships {
                    self.city_pic(lay, "unitview", 34, 0, 219.0, 157.0);
                } else {
                    self.city_pic(lay, "cityscap", 103, 0, 218.0, 144.0);
                }
                let figures = def
                    .stats
                    .get(&f4x_core::rules::Stat::Figures)
                    .copied()
                    .unwrap_or(1);
                let banner = self.banner_index(HUMAN.0);
                self.draw_figures(
                    &crate::frame::Layout::new(),
                    ty,
                    figures.max(1) as usize,
                    banner,
                    224.0,
                    136.0,
                );
            }
            ProductionItem::Building(id) => {
                // The cityscape picture numbers the buildings from 3 in the order of the list;
                // the walls have a big picture of their own.
                let entry = if self.game.content().rules.buildings.get(id).id == "city_walls" {
                    Some(114)
                } else {
                    crate::cityscape::sprite_of(id.index() as i16 + 3).map(|(entry, _)| entry)
                };
                if let Some(entry) = entry {
                    self.draw_centred_building(lay, entry);
                }
            }
            ProductionItem::TradeGoods => self.draw_centred_building(lay, 41),
            ProductionItem::Housing => {
                let house_type = self.game.city_race(city).map_or(0, |r| {
                    self.game.content().rules.races.get(r).house_type as usize
                });
                self.draw_centred_building(lay, 42 + house_type.min(2));
            }
        }
    }

    fn draw_city_production(&mut self, city: hecs::Entity, lay: Layout) {
        let prod = self.game.economy().city_production(city);
        let eco = self.game.economy().city_economy(city).unwrap_or_default();
        let view = Self::view(lay);
        let white = WHITE;
        match prod.item {
            Some(item) => {
                let name = self.item_name(item);
                // The name wraps in 44 pixels and its last line is at y 179, as in the original.
                let lines = self.bm_wrap(1, &name, 44.0);
                let top = 179.0 - 6.0 * lines.len().saturating_sub(1) as f32;
                for (i, line) in lines.iter().enumerate() {
                    self.bm_text(
                        view,
                        1,
                        white,
                        line,
                        216.0,
                        top + 6.0 * i as f32,
                        Align::Left,
                    );
                }
                let cost = self.game.economy().item_cost(city, item);
                if cost > 0 {
                    let remaining = cost.saturating_sub(prod.progress);
                    if eco.production > 0 {
                        // "N Turns" as the original prints it: the number right-aligned at 286,
                        // the word after it.
                        let turns = remaining.div_ceil(eco.production as u32);
                        let gold = Color::new(0.95, 0.7, 0.2, 1.0);
                        self.bm_text(
                            view,
                            1,
                            gold,
                            &turns.to_string(),
                            286.0,
                            140.0,
                            Align::Right,
                        );
                        let word = self.text_with("city.turns", &Args::new().count("n", turns));
                        self.bm_text(view, 1, gold, &word, 291.0, 140.0, Align::Left);
                    }
                    self.draw_production_coins(lay, prod.progress, cost);
                } else {
                    // Trade goods and housing cover the coins with their description
                    // (`proddescr_tradegoods`, `proddescr_housing`).
                    self.city_pic(lay, "backgrnd", 13, 0, 260.0, 149.0);
                    let text = if item == ProductionItem::TradeGoods {
                        self.text("city.converts_to_gold")
                    } else {
                        self.text("city.increases_growth")
                    };
                    for (i, line) in self.bm_wrap(1, &text, 50.0).iter().enumerate() {
                        self.bm_text(
                            view,
                            1,
                            white,
                            line,
                            262.0,
                            159.0 + 6.0 * i as f32,
                            Align::Left,
                        );
                    }
                }
                self.draw_product_picture(lay, city, item);
            }
            None => {
                self.bm_text(
                    view,
                    3,
                    GRAY,
                    &self.text("city.nothing"),
                    246.0,
                    177.0,
                    Align::Center,
                );
            }
        }
        let gold = self.game.treasury(crate::view::HUMAN).map_or(0, |p| p.gold);
        let price = self.game.economy().buy_cost(city);
        let can_buy = price.is_some_and(|p| p as i32 <= gold);
        if self.pic_button(lay, 7, 214.0, 188.0, can_buy && !self.choosing) {
            if let (Some(price), Some(item)) = (price, prod.item) {
                let name = self.item_name(item);
                self.sell_prompt = Some(SellPrompt::Buy { city, name, price });
            }
        }
        if self.pic_button(lay, 8, 247.0, 188.0, !self.choosing) {
            self.choosing = true;
            self.choice_scroll = 0;
        }
        if self.pic_button(lay, 9, 286.0, 188.0, !self.choosing) {
            self.screen = Screen::Map;
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
        lay.text(&self.text("city.choose_build"), 26.0, 24.0, 6.0, GOLD);
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
        let eco = self.game.economy().city_economy(city).unwrap_or_default();
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
            let cost = self.game.economy().item_cost(city, *item);
            let detail = match item {
                ProductionItem::TradeGoods => self.text("city.detail_gold"),
                ProductionItem::Housing => self.text("city.detail_growth"),
                _ => {
                    let turns = if eco.production > 0 {
                        cost.div_ceil(eco.production as u32).to_string()
                    } else {
                        "-".into()
                    };
                    self.text_with(
                        "city.cost_turns",
                        &Args::new().count("cost", cost).text("turns", turns),
                    )
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
        let rules = &self.game.content().rules;
        let mut items = Vec::new();
        for (id, _) in rules.buildings.iter() {
            let item = ProductionItem::Building(id);
            if self.game.economy().can_build(city, item).is_ok() {
                items.push(item);
            }
        }
        for (id, _) in rules.units.iter() {
            let item = ProductionItem::Unit(id);
            if self.game.economy().can_build(city, item).is_ok() {
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
