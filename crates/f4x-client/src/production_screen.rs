//! The production screen (`Production_Screen`, ProdScr.c): a full screen with the buildings the
//! city may build in a column on the left, the units on the right, and between them the
//! original's panel for the one picked (`Thing_View_Draw` called as `uvt_Prod`): a building's
//! picture, maintenance, what it allows and its description; a unit's sheet with its cost.
//! A click on another item picks it, a click on the picked one, or OK, starts it; Cancel and
//! Esc leave. Positions are the original's.

use crate::font::Style;
use crate::input::Input;
use crate::ui;
use f4x_core::city::production::ProductionItem;
use f4x_core::game::Command;
use macroquad::prelude::*;

use crate::city_screen::CityPage;
use crate::frame::Layout;
use crate::view::{App, Screen, HUMAN};

/// Rows a column shows (the original's lists are cut to fit).
const ROWS: usize = 12;
const ROW_HEIGHT: f32 = 14.0;
/// The panel's left edge, and the units column's.
const PANEL_X: f32 = 74.0;
const UNITS_X: f32 = 240.0;
/// A city holds at most this many units.
const GARRISON_LIMIT: usize = 9;

/// What the production screen remembers while it is open.
#[derive(Debug, Clone, Default)]
pub(crate) struct ProductionUi {
    pub selected: Option<ProductionItem>,
    /// The first row shown of the buildings and of the units.
    pub scroll: [usize; 2],
    /// A warning box waits for a click.
    pub warning: Option<String>,
    /// Opened this frame: the click that opened it must not answer it.
    pub fresh: bool,
    /// The abilities page of the unit sheet shown beside the list.
    pub panel: crate::unit_screen::SheetPanel,
}

impl App {
    /// The first city's screen with the production menu open (the `--choose` screenshot).
    pub(crate) fn open_test_production(&mut self) {
        let Screen::City(city) = self.screen else {
            return;
        };
        let mut page = CityPage::new(city);
        self.open_production(&mut page);
        self.open_page(Screen::City(city), page);
    }

    /// Open the production screen over `page`'s city with its current production picked.
    pub(crate) fn open_production(&mut self, page: &mut CityPage) {
        page.prod = ProductionUi {
            selected: self.game.economy().city_production(page.city).item,
            fresh: true,
            ..Default::default()
        };
        self.choosing = true;
    }

    /// Everything the city may start, as the left column (trade goods, housing, buildings)
    /// and the right (units).
    fn production_columns(&self, city: hecs::Entity) -> (Vec<ProductionItem>, Vec<ProductionItem>) {
        let items = self.buildable_items(city);
        let (units, mut buildings): (Vec<_>, Vec<_>) = items
            .into_iter()
            .partition(|i| matches!(i, ProductionItem::Unit(_)));
        // The two specials lead the buildings, as buildings 1 and 2 do in the original.
        buildings.sort_by_key(|i| match i {
            ProductionItem::TradeGoods => 0,
            ProductionItem::Housing => 1,
            _ => 2,
        });
        (buildings, units)
    }

    pub(crate) fn draw_production_screen(&mut self, page: &mut CityPage, input: &Input) {
        let city = page.city;
        let lay = Layout::new();
        clear_background(Color::new(0.03, 0.09, 0.09, 1.0));
        let (buildings, units) = self.production_columns(city);
        let fresh = std::mem::take(&mut page.prod.fresh);
        let full = self.game.units().units_at(self.city_position(city)).len() >= GARRISON_LIMIT;
        let selected = page
            .prod
            .selected
            .filter(|s| buildings.contains(s) || units.contains(s))
            .or_else(|| buildings.first().copied());
        let view = (lay.origin.x, lay.origin.y, lay.scale);

        // The middle panel first: the tabs overlap its edges.
        if let Some(item) = selected {
            self.draw_production_panel(city, item, &mut page.prod.panel, &lay, input);
        }

        let (_, wheel) = input.mouse_wheel();
        if wheel != 0.0 {
            let column = usize::from(input.mouse_position().0 > lay.origin.x + 120.0 * lay.scale);
            let len = [buildings.len(), units.len()][column];
            let max = len.saturating_sub(ROWS);
            let scroll = &mut page.prod.scroll[column];
            *scroll = (*scroll as i32 - wheel.signum() as i32).clamp(0, max as i32) as usize;
        }

        let waiting = page.prod.warning.is_some() || fresh;
        let mut clicked = None;
        for (column, list) in [&buildings, &units].into_iter().enumerate() {
            let x = if column == 0 { 0.0 } else { UNITS_X };
            let first = page.prod.scroll[column].min(list.len().saturating_sub(1));
            for (row, item) in list.iter().skip(first).take(ROWS).enumerate() {
                let y = 4.0 + ROW_HEIGHT * row as f32;
                self.draw_pic(&lay, "unitview", 31 + column, 0, x, y);
                let picked = selected == Some(*item);
                let blocked = column == 1 && full;
                let color = if blocked {
                    Color::new(0.55, 0.55, 0.55, 1.0)
                } else if picked {
                    Color::new(0.85, 1.0, 0.98, 1.0)
                } else {
                    Color::new(0.35, 0.78, 0.76, 1.0)
                };
                let name = self.item_name(*item);
                self.draw_styled(
                    view,
                    &Style::nearest(2, color),
                    &name,
                    x + 2.0 + column as f32,
                    y + 1.0,
                );
                let tab = lay.rect((x, y, 74.0, 11.0));
                if !waiting && ui::hit(tab, true, input).clicked() {
                    clicked = Some((*item, blocked));
                }
            }
        }

        let city_lay = crate::city_screen::Layout::new();
        self.draw_autobuild_button(page, city_lay, (2.0, 181.0, 70.0), input);
        let (cancel, ok) = (self.text("ui.cancel"), self.text("ui.ok"));
        let cancel = self.red_button(&lay, &cancel, 99.0, 181.0, input);
        let ok = self.red_button(&lay, &ok, 172.0, 181.0, input);

        if let Some(text) = page.prod.warning.clone() {
            if self.warning_box(&lay, &text, input) {
                page.prod.warning = None;
            }
            return;
        }
        if fresh {
            return;
        }
        if input.key_pressed(KeyCode::Escape) || cancel {
            self.choosing = false;
        } else if ok || input.key_pressed(KeyCode::Enter) {
            self.start_production(city, selected);
        } else if let Some((item, blocked)) = clicked {
            if blocked {
                page.prod.warning = Some(self.text("production.garrison_full"));
            } else if selected == Some(item) {
                self.start_production(city, Some(item));
            } else {
                page.prod.selected = Some(item);
                page.prod.panel.first = 0;
            }
        }
    }

    fn start_production(&mut self, city: hecs::Entity, item: Option<ProductionItem>) {
        if let Some(item) = item {
            let events = self.game.apply(Command::SetProduction { city, item });
            self.note(events);
        }
        self.choosing = false;
    }

    /// `Thing_View_Draw` for the production screen.
    fn draw_production_panel(
        &mut self,
        city: hecs::Entity,
        item: ProductionItem,
        sheet_panel: &mut crate::unit_screen::SheetPanel,
        lay: &Layout,
        input: &Input,
    ) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let white = WHITE;
        let (px, py) = (PANEL_X, 0.0);
        let cost = self.game.economy().item_cost(city, item);
        match item {
            ProductionItem::Unit(id) => {
                let def = self.game.content().rules.units.get(id).clone();
                let sheet = self.offer_sheet(&def, None, HUMAN.0);
                self.draw_production_sheet(lay, &sheet, (px, py), sheet_panel, input);
                let mut price = cost.to_string();
                if cost != def.cost {
                    price = format!("{cost}({})", def.cost);
                }
                let word = self.text("production.cost");
                self.draw_styled(view, &Style::nearest(1, white), &word, px + 55.0, py + 34.0);
                self.draw_styled(
                    view,
                    &Style::nearest(1, white),
                    &price,
                    px + 77.0,
                    py + 34.0,
                );
            }
            _ => self.draw_building_panel(city, item, cost, lay),
        }
    }

    fn draw_building_panel(
        &mut self,
        city: hecs::Entity,
        item: ProductionItem,
        cost: u32,
        lay: &Layout,
    ) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let white = WHITE;
        let gold = Color::new(0.95, 0.75, 0.25, 1.0);
        let (px, py) = (PANEL_X, 0.0);
        self.draw_pic(lay, "unitview", 0, 0, px, py);
        // The picture, centred in 44 by 36 pixels.
        let rules = &self.game.content().rules;
        let entry = match item {
            ProductionItem::Building(id) => {
                if rules.buildings.get(id).id == "city_walls" {
                    Some(114)
                } else {
                    crate::cityscape::sprite_of(id.index() as i16 + 3).map(|(entry, _)| entry)
                }
            }
            ProductionItem::TradeGoods => Some(41),
            _ => Some(
                42 + self
                    .game
                    .city_race(city)
                    .map_or(0, |r| rules.races.get(r).house_type as usize)
                    .min(2),
            ),
        };
        if let Some(size) = entry.and_then(|e| self.art.picture("cityscap", e, 0).map(|t| t.size()))
        {
            let x = px + 6.0 + ((44.0 - size.x) / 2.0).floor();
            let y = py + 4.0 + ((36.0 - size.y) / 2.0).floor();
            if let Some(entry) = entry {
                self.draw_pic(lay, "cityscap", entry, 0, x, y);
            }
        }
        let name = self.item_name(item);
        let name_x = px + 4.0 + 51.0;
        if self.art.text_width(4, &name) > 100.0 {
            for (i, line) in self.bm_wrap(4, &name, 100.0).iter().enumerate() {
                self.draw_styled(
                    view,
                    &Style::nearest(4, gold),
                    line,
                    name_x,
                    py + 7.0 + 8.0 * i as f32,
                );
            }
        } else {
            self.draw_styled(view, &Style::nearest(4, gold), &name, name_x, py + 11.0);
        }
        let rules = &self.game.content().rules;
        let (upkeep, id) = match item {
            ProductionItem::Building(b) => (rules.buildings.get(b).upkeep_gold, Some(b)),
            _ => (0, None),
        };
        if id.is_some() {
            let word = self.text("production.cost");
            self.draw_styled(view, &Style::nearest(1, white), &word, px + 55.0, py + 34.0);
            self.draw_styled(
                view,
                &Style::nearest(1, white),
                &cost.to_string(),
                px + 77.0,
                py + 34.0,
            );
        }
        let word = self.text("production.maintenance");
        self.draw_styled(view, &Style::nearest(4, gold), &word, px + 12.0, py + 47.0);
        if upkeep == 0 {
            self.draw_styled(
                view,
                &Style::nearest(2, white).right(),
                "0",
                px + 96.0,
                py + 48.0,
            );
        } else {
            for i in 0..upkeep {
                self.draw_pic(
                    lay,
                    "backgrnd",
                    42,
                    0,
                    px + 93.0 + 9.0 * i as f32,
                    py + 49.0,
                );
            }
        }
        let allows = self.production_allows(city, id);
        let word = self.text("production.allows");
        self.draw_styled(view, &Style::nearest(4, gold), &word, px + 12.0, py + 58.0);
        if !allows.is_empty() {
            let and = self.text("production.and");
            let mut text = allows[..allows.len() - 1].join(", ");
            if allows.len() > 1 {
                text.push_str(&format!(" {and} "));
            }
            text.push_str(&allows[allows.len() - 1]);
            text.push('.');
            for (i, line) in self.bm_wrap(2, &text, 100.0).iter().enumerate() {
                self.draw_styled(
                    view,
                    &Style::nearest(2, white),
                    line,
                    px + 64.0,
                    py + 59.0 + 7.0 * i as f32,
                );
            }
        }
        let description = match item {
            ProductionItem::Building(b) => Some(f4x_core::city::description::building_description(
                self.game.content(),
                b,
            )),
            ProductionItem::TradeGoods => Some(self.text("city.converts_to_gold")),
            _ => Some(self.text("city.increases_growth")),
        };
        if let Some(text) = description {
            for (i, line) in self.bm_wrap(4, &text, 147.0).iter().enumerate() {
                self.draw_styled(
                    view,
                    &Style::nearest(4, gold),
                    line,
                    px + 13.0,
                    py + 107.0 + 9.0 * i as f32,
                );
            }
        }
    }

    /// The buildings this one lets the city build (`Building_Allows_List`): the ones that need
    /// it, are not built yet and are open to the city's race.
    fn production_allows(
        &self,
        city: hecs::Entity,
        building: Option<f4x_core::rules::BuildingId>,
    ) -> Vec<String> {
        let Some(building) = building else {
            return Vec::new();
        };
        let rules = &self.game.content().rules;
        let this = &rules.buildings.get(building).id;
        let race = self
            .game
            .city_race(city)
            .map(|r| rules.races.get(r).id.clone());
        let built = self.game.city_buildings(city);
        rules
            .buildings
            .iter()
            .filter(|(id, def)| {
                def.requires.iter().any(|r| r == this)
                    && !built.contains(id)
                    && (def.races.is_empty()
                        || race.as_ref().is_some_and(|r| def.races.contains(r)))
            })
            .map(|(_, def)| def.name.clone())
            .collect()
    }
}
