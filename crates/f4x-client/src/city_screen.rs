//! The city screen: economy, buildings, worked area and the production list, drawn on
//! the original screen layout (320x200) when the art is available.

use crate::font::Style;
use crate::frame::Disabled;
use crate::input::Input;
use crate::page::Page;
use crate::ui;
use f4x_core::city::production::ProductionItem;
use f4x_core::game::Command;
use f4x_core::map::Position;
use f4x_core::rules::BuildingId;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::art::{TILE_H, TILE_W};
use crate::sell_prompt::{CityPanel, SellPrompt};
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
        use crate::resolution::{DESIGN_H, DESIGN_W};
        let scale = (screen_height() / DESIGN_H)
            .min(screen_width() / DESIGN_W)
            .floor()
            .max(1.0);
        Self {
            ox: (screen_width() - DESIGN_W * scale) / 2.0,
            oy: (screen_height() - DESIGN_H * scale) / 2.0,
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
}

/// The top left of the first unit in the garrison window.
const GARRISON: (f32, f32) = (215.0, 102.0);

/// The city screen of one city.
pub(crate) struct CityPage {
    pub(crate) city: hecs::Entity,
    /// The new name being typed in the rename box.
    pub(crate) naming: Option<String>,
    /// The auto-builder dialog while it is open.
    pub(crate) autobuild: Option<crate::autobuild_dialog::AutoDialog>,
    /// Whether the worked-area map shows each square's food and production.
    pub(crate) area_figures: bool,
    /// The question box and the enchantment list, as the outpost screen has them.
    pub(crate) panel: CityPanel,
    /// The production menu while it is open.
    pub(crate) prod: crate::production_screen::ProductionUi,
}

impl CityPage {
    pub(crate) fn new(city: hecs::Entity) -> CityPage {
        CityPage {
            city,
            naming: None,
            autobuild: None,
            area_figures: false,
            panel: CityPanel::default(),
            prod: Default::default(),
        }
    }
}

impl Page for CityPage {
    fn frame(&mut self, app: &mut App, input: &Input) {
        app.city_screen(self, input);
    }
}

impl App {
    /// Open the screen of `city`.
    pub(crate) fn open_city_screen(&mut self, city: hecs::Entity) {
        self.open_page(Screen::City(city), CityPage::new(city));
    }

    pub(crate) fn city_screen(&mut self, page: &mut CityPage, input: &Input) {
        let city = page.city;
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
        let dialog = page.autobuild.is_some();
        if dialog && input.key_pressed(KeyCode::Escape) {
            page.autobuild = None;
            return;
        }
        let selling = page.panel.sell.is_some();
        let renaming = page.naming.is_some();
        if renaming && input.key_pressed(KeyCode::Escape) {
            page.naming = None;
            return;
        }
        let choosing = self.choosing || dialog || selling || renaming;
        // A right-click on a unit of the garrison opens its statistics sheet.
        if input.mouse_pressed(MouseButton::Right) && !choosing {
            let over = self
                .garrison_cells(city, lay)
                .into_iter()
                .find(|(r, _)| ui::hit(*r, true, input).hovered());
            if let Some((_, unit)) = over {
                self.open_unit_sheet(unit);
                return;
            }
        }
        // A left-click on a unit of the garrison picks it, or lets it go.
        if input.mouse_pressed(MouseButton::Left) && !choosing {
            let over = self
                .garrison_cells(city, lay)
                .into_iter()
                .find(|(r, _)| ui::hit(*r, true, input).hovered());
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
            && (input.key_pressed(KeyCode::Escape)
                || (input.mouse_pressed(MouseButton::Right) && !choosing))
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
        let full = lay.rect(
            0.0,
            0.0,
            crate::resolution::DESIGN_W,
            crate::resolution::DESIGN_H,
        );
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
        self.draw_city_header(page, lay, input);
        self.draw_city_resources(&mut page.panel, city, lay, input);
        self.draw_city_buildings(&mut page.panel, city, lay, input);
        self.draw_city_area(page, lay, input);
        self.draw_city_garrison(city, lay);
        self.draw_city_production(page, lay, input);
        if renaming {
            self.choosing = was_choosing;
            self.draw_rename_box(page, input);
        } else if dialog {
            self.choosing = was_choosing;
            self.draw_autobuild_dialog(page, lay, input);
        } else if selling {
            self.choosing = was_choosing;
            self.draw_sell_prompt(&mut page.panel, input);
        } else if self.choosing {
            self.draw_production_screen(page, input);
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
                dest_size: Some(crate::resolution::logical_size(&tex) * lay.scale),
                ..Default::default()
            },
        );
        true
    }

    /// [`App::city_pic`] with some of its pixels hidden, as the original fades in a building
    /// that was just finished (`Vanish_Bitmap`): `percent` of them show.
    #[allow(clippy::too_many_arguments)]
    fn city_pic_fading(
        &mut self,
        lay: Layout,
        entry: usize,
        frame: usize,
        x: f32,
        y: f32,
        percent: u32,
        draw: u64,
    ) {
        let Some(mut img) = self.art.pixels("cityscap", entry, frame).cloned() else {
            return;
        };
        let width = usize::from(img.width);
        for i in 0..img.bytes.len() / 4 {
            if crate::cityscape::vanished((i % width) as u32, (i / width) as u32, draw, percent) {
                img.bytes[i * 4 + 3] = 0;
            }
        }
        let faded = Texture2D::from_image(&img);
        faded.set_filter(FilterMode::Nearest);
        let at = lay.rect(x, y, 0.0, 0.0);
        draw_texture_ex(
            &faded,
            at.x,
            at.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(crate::resolution::logical_size(&faded) * lay.scale),
                ..Default::default()
            },
        );
    }

    /// For screenshots: dress the first city with these, separated by commas: a spell's id
    /// (a lasting spell on the city), `ward:REALM` (a Spell Ward), `sold` (sell its first
    /// building, which leaves rubble) or `built:ID` (the building fades in).
    pub fn open_test_city_fx(&mut self, list: &str) {
        use f4x_core::rules::exec::Target;
        let Some(city) = self.game.economy().cities().first().map(|c| c.0) else {
            return;
        };
        let rules = &self.game.content().rules;
        let spell_ward = rules.spells.id_of("spell_ward");
        let mut spells = Vec::new();
        let mut wards = Vec::new();
        let (mut sell, mut built) = (false, None);
        for token in list.split(',').filter(|t| !t.is_empty()) {
            if token == "sold" {
                sell = true;
            } else if let Some(id) = token.strip_prefix("built:") {
                built = rules.buildings.id_of(id);
            } else if let Some(realm) = token.strip_prefix("ward:") {
                match rules.realms.id_of(realm) {
                    Some(r) => wards.push(r),
                    None => eprintln!("unknown realm {realm}"),
                }
            } else {
                match rules.spells.id_of(token) {
                    Some(s) => spells.push(s),
                    None => eprintln!("unknown spell {token}"),
                }
            }
        }
        for spell in spells {
            self.game
                .attach_enchantment(Target::City(city), spell, HUMAN);
        }
        if let (Some(spell), false) = (spell_ward, wards.is_empty()) {
            self.game
                .attach_enchantment(Target::City(city), spell, HUMAN);
            for realm in wards {
                self.game.add_spell_ward(city, realm);
            }
        }
        if sell {
            let first = self.game.city_buildings(city).first().copied();
            if let Some(b) = first {
                if let Err(e) = self.game.sell_building(city, b) {
                    eprintln!("cannot sell: {e:?}");
                }
            }
        }
        if let Some(building) = built {
            self.build_in = Some(crate::built_popup::BuildIn {
                city,
                building,
                started: None,
            });
        }
    }

    /// The box for a new name for the city (the original's name box); Enter keeps what is typed,
    /// an empty name keeps the old one.
    fn draw_rename_box(&mut self, page: &mut CityPage, input: &Input) {
        let city = page.city;
        let Some(mut name) = page.naming.take() else {
            return;
        };
        let lay = crate::frame::Layout::new();
        if self.name_box(
            &lay,
            (60.0, 60.0),
            &self.text("city.name_prompt"),
            &mut name,
            input,
        ) {
            let events = self.game.apply(Command::RenameCity { city, name });
            self.note(events);
        } else {
            page.naming = Some(name);
        }
    }

    fn draw_city_header(&mut self, page: &mut CityPage, lay: Layout, input: &Input) {
        let city = page.city;
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
        self.draw_styled(
            view,
            &Style::nearest(5, Color::new(0.95, 0.85, 0.5, 1.0)).center(),
            &title,
            105.0,
            0.0,
        );
        // A click on the name asks for a new one.
        if ui::hit(lay.rect(0.0, 0.0, 210.0, 17.0), !self.choosing, input).clicked() {
            page.naming = Some(name.clone());
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
        self.draw_styled(view, &Style::nearest(3, orange), &race_name, 6.0, 19.0);
        let people = pop.figures * 1000 + pop.tens * 10;
        let grow = eco.growth_tens * 10;
        self.draw_styled(
            view,
            &Style::nearest(3, orange).right(),
            &self.text_with(
                "city.population",
                &Args::new()
                    .text("people", thousands(people))
                    .text("growth", format!("{grow:+}")),
            ),
            207.0,
            19.0,
        );
        // The citizens: farmers, then workers, then rebels. Clicking one changes its job.
        let farmers = self.game.city_farmers(city);
        let total = (eco.farmers + eco.workers + eco.rebels).max(1) as f32;
        let step = (11.0f32).min(196.0 / total);
        let mut x = 7.0;
        let clicked = input.mouse_pressed(MouseButton::Left) && !self.choosing;
        let jobs = [(eco.farmers, 45usize), (eco.workers, 59), (eco.rebels, 74)];
        for (job, (count, base)) in jobs.into_iter().enumerate() {
            for _ in 0..count {
                let entry = base + race_index;
                if !self.city_pic(lay, "backgrnd", entry, 0, x, 26.0) {
                    let r = lay.rect(x, 27.0, 8.0, 12.0);
                    draw_rectangle(r.x, r.y, r.w, r.h, [GREEN, ORANGE, RED][job]);
                }
                let hit = lay.rect(x, 26.0, step, 15.0);
                if clicked && ui::hit(hit, true, input).hovered() && eco.figures > 0 {
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

    fn draw_city_resources(
        &mut self,
        panel: &mut CityPanel,
        city: hecs::Entity,
        lay: Layout,
        input: &Input,
    ) {
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
                self.draw_styled(view, &Style::nearest(1, color).right(), &note, 134.0, y);
            }
        }
        self.draw_city_enchantments(panel, city, lay, input);
    }

    /// The enchantments on the city, six to a page in the colour of whoever cast them, with the
    /// arrows that turn the page; a click on the player's own asks to turn it off
    /// (`City_Screen_Draw_City_Enchantments`, `City_Add_Fields_City_Enchantments`).
    fn draw_city_enchantments(
        &mut self,
        panel: &mut CityPanel,
        city: hecs::Entity,
        lay: Layout,
        input: &Input,
    ) {
        let enabled = !self.choosing;
        self.city_enchantment_list(panel, city, lay, (139.0, 51.0, 3.0), enabled, input);
    }

    /// The list with its first row at `(x, y, shift)` (the original's `xstart`, `ystart`), shared by
    /// the city and outpost screens. Returns whether a click this frame landed on a row or an
    /// arrow, so the caller does not take it for something else. The names are drawn `shift`
    /// pixels right of and below the clickable rows, the third number (the city screen's panel has a margin).
    pub(crate) fn city_enchantment_list(
        &mut self,
        panel: &mut CityPanel,
        city: hecs::Entity,
        lay: Layout,
        (first_x, first_y, shift): (f32, f32, f32),
        enabled: bool,
        input: &Input,
    ) -> bool {
        const PAGE: usize = 6;
        let mut hit = false;
        let view = Self::view(lay);
        let list = self.game.city_enchantments(city);
        // The temporary effects follow the spells, in plain text and not clickable (task 152).
        let effects = self.temp_effect_lines(city);
        let total = list.len() + effects.len();
        if total > PAGE {
            let last = total - 1;
            let up = lay.rect(first_x + 62.0, first_y - 1.0, 12.0, 12.0);
            hit |= ui::hit(up, true, input).clicked();
            if self.backgrnd_button(lay, 15, first_x + 62.0, first_y - 1.0, enabled, input) {
                panel.spell_first = match panel.spell_first {
                    0 => last / PAGE * PAGE,
                    n => n.saturating_sub(PAGE),
                };
            }
            let down = lay.rect(first_x + 62.0, first_y + 34.0, 12.0, 12.0);
            hit |= ui::hit(down, true, input).clicked();
            if self.backgrnd_button(lay, 16, first_x + 62.0, first_y + 34.0, enabled, input) {
                panel.spell_first += PAGE;
                if panel.spell_first > last {
                    panel.spell_first = 0;
                }
            }
        } else {
            panel.spell_first = 0;
        }
        let click = input.mouse_pressed(MouseButton::Left) && enabled;
        for (row, line) in effects.iter().enumerate() {
            let row = list.len() + row;
            if row < panel.spell_first || row >= panel.spell_first + PAGE {
                continue;
            }
            let i = (row - panel.spell_first) as f32;
            self.draw_styled(
                view,
                &Style::nearest(1, Color::new(0.85, 0.85, 0.8, 1.0)),
                line,
                first_x + shift,
                first_y + shift + i * 7.0,
            );
        }
        for (row, &(spell, caster)) in list.iter().enumerate().skip(panel.spell_first).take(PAGE) {
            let i = (row - panel.spell_first) as f32;
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
            self.draw_styled(
                view,
                &Style::nearest(1, color),
                &name,
                first_x + shift,
                first_y + shift + i * 7.0,
            );
            let rect = lay.rect(first_x, first_y + i * 7.0, 60.0, 7.0);
            let on_row = ui::hit(rect, true, input).hovered();
            hit |= click && on_row;
            if click && caster == HUMAN && on_row {
                panel.sell = Some(SellPrompt::TurnOff {
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
        // A building sold this turn keeps its place as rubble.
        let buildings = self
            .game
            .city_buildings(city)
            .into_iter()
            .chain(self.game.economy().sold_building(city))
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
            many_trees: class == Some(f4x_core::map::terrain::TerrainClass::Forest)
                || house_type == 1,
            many_rocks: matches!(
                class,
                Some(
                    f4x_core::map::terrain::TerrainClass::Mountains
                        | f4x_core::map::terrain::TerrainClass::Hills
                )
            ),
            seed: 0x2895 + u64::from(at.x) * u64::from(at.y),
        }
    }

    /// The city's picture: ground, water, hills, roads, then every cell from the back to the
    /// front (houses, trees, rocks, buildings), and the walls along the bottom.
    fn draw_city_buildings(
        &mut self,
        panel: &mut CityPanel,
        city: hecs::Entity,
        lay: Layout,
        input: &Input,
    ) {
        let over = self.draw_city_buildings_at(city, lay, 4.0, 100.0, input);
        if let Some(i) = over.filter(|i| (3..=35).contains(i)) {
            if input.mouse_pressed(MouseButton::Left) && !self.choosing {
                self.start_sale(panel, city, i);
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
        input: &Input,
    ) -> Option<i16> {
        use crate::cityscape as cs;
        let view = Self::view(lay);
        let at = self.city_position(city);
        let myrror = at.plane == f4x_core::map::Plane::Myrror;
        let spells: Vec<String> = {
            let rules = &self.game.content().rules;
            self.game
                .city_enchantments(city)
                .iter()
                .map(|&(s, _)| rules.spells.get(s).id.clone())
                .collect()
        };
        let enchanted = |name: &str| spells.iter().any(|s| s == name);
        // The original counts its animations in draws of the screen.
        let draws = (get_time() * cs::DRAWS_PER_SECOND) as u64;
        let building_counter = cs::anim_counter(draws, 60);
        let water_counter = cs::anim_counter(draws, 84);
        let wall_counter = cs::anim_counter(draws, 60);
        // A building just finished fades in (ten steps), and one sold this turn is rubble.
        let mut fading: Option<(BuildingId, u32)> = None;
        if let Some(mut b) = self.build_in {
            if b.city == city {
                let now = get_time();
                let steps = ((now - *b.started.get_or_insert(now)) * cs::DRAWS_PER_SECOND) as u32;
                if steps < 10 {
                    fading = Some((b.building, steps * 10));
                    self.build_in = Some(b);
                } else {
                    self.build_in = None;
                }
            } else {
                self.build_in = None;
            }
        }
        let fades = |idx: i16| {
            fading
                .filter(|&(b, _)| 3 + b.index() as i16 == idx)
                .map(|(_, percent)| percent)
        };
        let flying_fortress = enchanted("flying_fortress");
        let frame = if flying_fortress {
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
                    ocean |= class == Some(f4x_core::map::terrain::TerrainClass::Ocean);
                    if dy <= 0 && !(dy == 0 && dx != 0) {
                        mountain |= class == Some(f4x_core::map::terrain::TerrainClass::Mountains);
                        hills |= class == Some(f4x_core::map::terrain::TerrainClass::Hills);
                    }
                }
            }
        }
        // The water is drawn over the ground more and more often, each time with the next
        // frame of its picture, so it seems to flow.
        let water = if river {
            Some(if myrror { 115 } else { 3 })
        } else if ocean {
            Some(if myrror { 116 } else { 4 })
        } else {
            None
        };
        if let Some(entry) = water {
            let frames = self.art.frames("cityscap", entry);
            for layer in 0..cs::water_layers(water_counter) {
                self.city_pic(
                    lay,
                    "cityscap",
                    entry,
                    cs::looped_frame(layer, frames),
                    x0,
                    y0,
                );
            }
        }
        // The sky or the land behind the city, unless it flies.
        if !flying_fortress {
            let sky = if enchanted("cloud_of_shadow") {
                Some(if myrror { 111 } else { 91 })
            } else if enchanted("heavenly_light") {
                Some(if myrror { 113 } else { 93 })
            } else if enchanted("chaos_rift") {
                Some(if myrror { 112 } else { 92 })
            } else if mountain {
                Some(if myrror { 9 } else { 2 })
            } else if hills {
                Some(if myrror { 10 } else { 1 })
            } else {
                None
            };
            if let Some(entry) = sky {
                self.city_pic(lay, "cityscap", entry, 0, x0, y0);
            }
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
        let mut over_rubble = false;
        let sold = self.game.economy().sold_building(city);
        let rubble_of = sold.map(|b| {
            (
                3 + b.index() as i16,
                self.game.content().rules.buildings.get(b).shape,
            )
        });
        let evil_presence = enchanted("evil_presence");
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
                        let rect = lay.rect(sx, sy - up, 40.0, up + 1.0);
                        // A building sold this turn stays as rubble of its size.
                        if let Some((_, shape)) = rubble_of.filter(|&(i, _)| i == idx) {
                            let rubble = cs::rubble_picture(shape);
                            let frames = self.art.frames("cityscap", rubble);
                            let frame = cs::looped_frame(building_counter, frames);
                            self.city_pic(lay, "cityscap", rubble, frame, sx, sy - up);
                            over_rubble |= ui::hit(rect, true, input).hovered();
                            continue;
                        }
                        let frames = self.art.frames("cityscap", entry);
                        let frame = cs::looped_frame(building_counter, frames);
                        let fade = fades(idx);
                        match fade {
                            Some(percent) => {
                                self.city_pic_fading(lay, entry, frame, sx, sy - up, percent, draws)
                            }
                            None => {
                                self.city_pic(lay, "cityscap", entry, frame, sx, sy - up);
                            }
                        }
                        // Evil Presence darkens the religious buildings.
                        if evil_presence && cs::EVIL_PRESENCE_BUILDINGS.contains(&idx) {
                            let pic = cs::EVIL_PRESENCE_PICTURE;
                            let frames = self.art.frames("cityscap", pic);
                            let frame = cs::looped_frame(building_counter, frames);
                            match fade {
                                Some(percent) => self.city_pic_fading(
                                    lay,
                                    pic,
                                    frame,
                                    sx,
                                    sy - up,
                                    percent,
                                    draws,
                                ),
                                None => {
                                    self.city_pic(lay, "cityscap", pic, frame, sx, sy - up);
                                }
                            }
                        }
                        if ui::hit(rect, true, input).hovered() {
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
            let walls_fade = fading
                .filter(|&(b, _)| self.game.content().rules.buildings.get(b).id == "city_walls")
                .map(|(_, percent)| percent);
            match walls_fade {
                Some(percent) => self.city_pic_fading(lay, 76, 0, x0, y0 + 82.0, percent, draws),
                None => {
                    self.city_pic(lay, "cityscap", 76, 0, x0, y0 + 82.0);
                }
            }
        }
        // The wards and spells that guard the city, along the bottom, and its magic walls.
        let wards = self.game.economy().city_wards(city);
        for (realm, entry, left) in cs::REALM_WARDS {
            if wards.iter().any(|w| w == realm) {
                self.city_pic(lay, "cityscap", entry, 0, x0 + left, y0 + cs::WARD_ROW);
            }
        }
        for (spell, entry, left) in cs::CITY_BLESSINGS {
            if enchanted(spell) {
                self.city_pic(lay, "cityscap", entry, 0, x0 + left, y0 + cs::WARD_ROW);
            }
        }
        // Wall of Darkness is drawn with the air wall's picture, as in the original; Wall of
        // Stone is not drawn (it makes the city walls).
        for (spell, entry) in [("wall_of_darkness", 79), ("wall_of_fire", 77)] {
            if enchanted(spell) {
                let frames = self.art.frames("cityscap", entry);
                let frame = cs::looped_frame(wall_counter, frames);
                self.city_pic(lay, "cityscap", entry, frame, x0, y0 + 84.0);
            }
        }
        gl.flush();
        gl.quad_gl.scissor(None);

        // The name of the building under the mouse.
        self.help_ui.building = None;
        let name = match mouse_over {
            Some(cs::FORTRESS) => Some(self.text("city.fortress")),
            Some(cs::SUMMONING_CIRCLE) => Some(self.text("city.summoning_circle")),
            None if over_rubble => Some(self.text("city.destroyed")),
            Some(i) if (3..=35).contains(&i) => self
                .game
                .content()
                .rules
                .buildings
                .iter()
                .nth((i - 3) as usize)
                .map(|(id, b)| {
                    // A right-click on it opens its description.
                    let content = self.game.content();
                    let text = f4x_core::city::description::building_description(content, id);
                    self.help_ui.building = Some((b.name.clone(), text));
                    b.name.clone()
                }),
            _ => None,
        };
        if let Some(n) = name {
            self.draw_styled(
                view,
                &Style::nearest(3, WHITE).center(),
                &n,
                x0 + 102.0,
                y0 + 84.0,
            );
        }
        mouse_over
    }

    /// The 5x5 worked area at original tile size, drawn as the map is (`City_Screen_Draw_Map`):
    /// terrain, resources and roads, the corners shaded, the cities on it, and a mark on the squares another city
    /// of the player also works.
    fn draw_city_area(&mut self, page: &mut CityPage, lay: Layout, input: &Input) {
        let city = page.city;
        let at = self.city_position(city);
        let Some(map) = self.game.map() else {
            return;
        };
        let (width, height) = (map.width() as i32, map.height() as i32);
        let area: Vec<_> = self.game.economy().city_area(at);
        let tile = vec2(TILE_W * lay.scale, TILE_H * lay.scale);
        let anim = (get_time() * 4.0) as usize;
        // A click on the map shows or hides each square's food and production.
        let map = lay.rect(215.0, 4.0, 5.0 * TILE_W, 5.0 * TILE_H);
        if ui::hit(map, !self.choosing, input).clicked() {
            page.area_figures = !page.area_figures;
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
                if page.area_figures {
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
                        let at = lay.rect(x, y, width, crate::resolution::logical_h(&tex));
                        draw_texture_ex(
                            &tex,
                            at.x,
                            at.y,
                            WHITE,
                            DrawTextureParams {
                                dest_size: Some(vec2(
                                    at.w,
                                    crate::resolution::logical_h(&tex) * lay.scale,
                                )),
                                source: Some(crate::resolution::pixel_rect(Rect::new(
                                    0.0,
                                    0.0,
                                    width,
                                    crate::resolution::logical_h(&tex),
                                ))),
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
            ((18.0 - crate::resolution::logical_w(&tex)) / (slots - 1) as f32)
                .clamp(0.5, crate::resolution::logical_w(&tex))
        } else {
            0.0
        };
        for k in 0..slots {
            let at = lay.rect(x + k as f32 * step, y, 0.0, 0.0);
            let (w, h) = (
                crate::resolution::logical_w(&tex) * lay.scale,
                crate::resolution::logical_h(&tex) * lay.scale,
            );
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
        let Some(size) = self
            .art
            .picture("cityscap", entry, 0)
            .map(crate::resolution::logical_size)
        else {
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

    fn draw_city_production(&mut self, page: &mut CityPage, lay: Layout, input: &Input) {
        let city = page.city;
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
                    self.draw_styled(
                        view,
                        &Style::nearest(1, white),
                        line,
                        216.0,
                        top + 6.0 * i as f32,
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
                        self.draw_styled(
                            view,
                            &Style::nearest(1, gold).right(),
                            &turns.to_string(),
                            286.0,
                            140.0,
                        );
                        let word = self.text_with("city.turns", &Args::new().count("n", turns));
                        self.draw_styled(view, &Style::nearest(1, gold), &word, 291.0, 140.0);
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
                        self.draw_styled(
                            view,
                            &Style::nearest(1, white),
                            line,
                            262.0,
                            159.0 + 6.0 * i as f32,
                        );
                    }
                }
                self.draw_product_picture(lay, city, item);
            }
            None => {
                self.draw_styled(
                    view,
                    &Style::nearest(3, GRAY).center(),
                    &self.text("city.nothing"),
                    246.0,
                    177.0,
                );
            }
        }
        let gold = self
            .game
            .player(crate::view::HUMAN)
            .map_or(0, |p| p.treasury().gold);
        let price = self.game.economy().buy_cost(city);
        // Clickable whenever the item can be bought; too little gold is answered with a box.
        if self.backgrnd_button(
            lay,
            7,
            214.0,
            188.0,
            price.is_some() && !self.choosing,
            input,
        ) {
            if let (Some(price), Some(item)) = (price, prod.item) {
                page.panel.sell = Some(if price as i32 <= gold {
                    let name = self.item_name(item);
                    SellPrompt::Buy { city, name, price }
                } else {
                    SellPrompt::Refused(self.text("sell.not_enough_gold"))
                });
            }
        }
        if self.backgrnd_button(lay, 8, 247.0, 188.0, !self.choosing, input) {
            self.open_production(page);
        }
        if self.backgrnd_button(lay, 9, 286.0, 188.0, !self.choosing, input) {
            self.screen = Screen::Map;
        }
    }

    /// One of the original's two-frame buttons from `backgrnd.lbx`; true when clicked.
    fn backgrnd_button(
        &mut self,
        lay: Layout,
        entry: usize,
        x: f32,
        y: f32,
        enabled: bool,
        input: &Input,
    ) -> bool {
        let lay = crate::frame::Layout {
            scale: lay.scale,
            origin: vec2(lay.ox, lay.oy),
        };
        let look = (enabled, Disabled::Fade(0.45));
        self.pic_button(&lay, ("backgrnd", entry), (x, y), look, input)
    }

    /// Everything the city may start building: buildings, then units, then the two specials.
    pub(crate) fn buildable_items(&self, city: hecs::Entity) -> Vec<ProductionItem> {
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
