//! The City list (task 139), the original's `CityList_Screen` (`CityList.c`): the wizard's
//! cities in a table of name, race, population, gold, production, what is being built and the
//! turns it needs, nine rows at a time on the picture from `reload.lbx`, with a small map and
//! the enchantments of the city under the mouse. A left-click opens the city (an outpost has its own screen), a right-click its production menu. U and D (or the arrows, or the wheel)
//! scroll five rows, Esc or OK leaves. Positions are the original's.

use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::view::{App, Screen, HUMAN};

/// Rows on the screen and the rows one scroll moves (`NUM_CITY_LIST`, `NUM_CITY_LIST_SCROLL`).
const ROWS: usize = 9;
const SCROLL: usize = 5;
/// The small map (`CITYLIST_REDUCED_MAP_*`).
const MAP: (f32, f32, f32, f32) = (42.0, 162.0, 49.0, 33.0);
/// Where the columns start: name, race, pop, gold, prd, producing, time.
const HEADS: [(&str, f32); 7] = [
    ("Name", 32.0),
    ("Race", 88.0),
    ("Pop", 136.0),
    ("Gold", 154.0),
    ("Prd", 176.0),
    ("Producing", 197.0),
    ("Time", 272.0),
];
/// The right edges of the number columns (pop, gold, prd, time).
const RIGHTS: [f32; 4] = [148.0, 168.0, 188.0, 287.0];

/// What the list remembers while it is open.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CityListUi {
    /// The first city shown.
    pub first: usize,
    /// The row under the mouse, kept when the mouse is elsewhere.
    pub scanned: usize,
}

/// One row of the table.
struct Row {
    city: hecs::Entity,
    name: String,
    race: String,
    pop: u32,
    gold: i32,
    production: i32,
    building: String,
    turns: Option<u32>,
    outpost: bool,
}

/// The first row after a scroll of `delta` steps up (negative) or down (positive).
pub(crate) fn scrolled(first: usize, delta: i32, count: usize) -> usize {
    let last = count.saturating_sub(ROWS);
    if delta < 0 {
        first.saturating_sub(SCROLL)
    } else if delta > 0 {
        if first + ROWS < count {
            // The original moves five rows, or one when five would pass the end.
            if first + ROWS + SCROLL < count {
                first + SCROLL
            } else {
                first + 1
            }
        } else {
            first
        }
    } else {
        first
    }
    .min(last)
}

impl App {
    pub(crate) fn open_city_list(&mut self) {
        self.city_list_ui = CityListUi::default();
        self.screen = Screen::CityList;
    }

    fn city_rows(&self) -> Vec<Row> {
        let eco = self.game.economy();
        let rules = &self.game.content().rules;
        eco.cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == HUMAN)
            .map(|(city, _, _)| {
                let e = eco.city_economy(city).unwrap_or_default();
                let prod = eco.city_production(city);
                let turns = prod.item.and_then(|item| {
                    let cost = eco.item_cost(city, item);
                    (cost > 0 && e.production > 0).then(|| {
                        cost.saturating_sub(prod.progress)
                            .div_ceil(e.production as u32)
                    })
                });
                Row {
                    city,
                    name: self.city_name(city),
                    race: self
                        .game
                        .city_race(city)
                        .map(|r| rules.races.get(r).name.clone())
                        .unwrap_or_default(),
                    pop: self.game.city_population(city).figures,
                    gold: e.gold - e.gold_upkeep,
                    production: e.production,
                    building: prod.item.map(|i| self.item_name(i)).unwrap_or_default(),
                    turns,
                    outpost: e.figures == 0,
                }
            })
            .collect()
    }

    pub(crate) fn city_list_screen(&mut self) {
        let rows = self.city_rows();
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let gold = Color::new(0.95, 0.75, 0.25, 1.0);
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        if self.draw_pic(&lay, "reload", 21, 0, 0.0, 0.0).is_none() {
            let r = lay.rect((0.0, 0.0, 320.0, 200.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.2, 0.2, 0.3, 1.0));
        }
        let count = rows.len();
        let mut ui = self.city_list_ui;
        ui.first = ui.first.min(count.saturating_sub(ROWS));

        // The title, the column heads and the reserves.
        let owner = self
            .game
            .wizard(HUMAN)
            .map_or_else(String::new, |w| w.name());
        self.bm_text_outlined(
            view,
            4,
            &format!("The Cities Of {owner}"),
            160.0,
            3.0,
            Align::Center,
        );
        for (head, x) in HEADS {
            self.bm_text(view, 1, gold, head, x, 17.0, Align::Left);
        }
        let (reserve_gold, reserve_mana) =
            self.game.player(HUMAN).map_or((0, 0), |p| (p.gold, p.mana));
        self.bm_text(
            view,
            1,
            light,
            &format!("{reserve_gold}GP"),
            242.0,
            172.0,
            Align::Center,
        );
        self.bm_text(
            view,
            1,
            light,
            &format!("{reserve_mana}MP"),
            276.0,
            172.0,
            Align::Center,
        );

        // Scrolling: the arrows, U and D, the wheel.
        let can_up = ui.first > 0;
        let can_down = ui.first + ROWS < count;
        let mut scroll = 0i32;
        if count > ROWS {
            for x in [11.0, 299.0] {
                if self.army_button(&lay, 1, x, 26.0, can_up) {
                    scroll -= 1;
                }
                if self.army_button(&lay, 2, x, 139.0, can_down) {
                    scroll += 1;
                }
            }
        }
        if is_key_pressed(KeyCode::U) {
            scroll -= 1;
        }
        if is_key_pressed(KeyCode::D) {
            scroll += 1;
        }
        scroll -= mouse_wheel().1.signum() as i32;
        ui.first = scrolled(ui.first, scroll.signum(), count);

        // The rows.
        let mouse: Vec2 = mouse_position().into();
        let shown = count.saturating_sub(ui.first).min(ROWS);
        ui.scanned = ui.scanned.min(shown.saturating_sub(1));
        let (mut left, mut right) = (None, None);
        for (i, row) in rows.iter().skip(ui.first).take(ROWS).enumerate() {
            let y = 26.0 + 14.0 * i as f32;
            let cell = lay.rect((30.0, y, 260.0, 9.0));
            if cell.contains(mouse) {
                ui.scanned = i;
                if is_mouse_button_pressed(MouseButton::Left) {
                    left = Some(ui.first + i);
                }
                if is_mouse_button_pressed(MouseButton::Right) {
                    right = Some(ui.first + i);
                }
            }
            if ui.scanned == i {
                let h = lay.rect((30.0, y + 1.0, 259.0, 9.0));
                draw_rectangle(h.x, h.y, h.w, h.h, Color::new(1.0, 1.0, 1.0, 0.25));
            }
            let ty = y + 1.0;
            self.bm_text(view, 1, light, &row.name, 31.0, ty, Align::Left);
            self.bm_text(view, 1, light, &row.race, 87.0, ty, Align::Left);
            self.bm_text(
                view,
                1,
                light,
                &row.pop.to_string(),
                RIGHTS[0],
                ty,
                Align::Right,
            );
            self.bm_text(
                view,
                1,
                light,
                &row.gold.to_string(),
                RIGHTS[1],
                ty,
                Align::Right,
            );
            self.bm_text(
                view,
                1,
                light,
                &row.production.to_string(),
                RIGHTS[2],
                ty,
                Align::Right,
            );
            self.bm_text(view, 1, light, &row.building, 196.0, ty, Align::Left);
            let time = row.turns.map_or_else(|| "-".to_string(), |t| t.to_string());
            self.bm_text(view, 1, light, &time, RIGHTS[3], ty, Align::Right);
        }

        // The city under the mouse: its name, its enchantments and where it is.
        if let Some(row) = rows.get(ui.first + ui.scanned) {
            self.bm_text(
                view,
                1,
                gold,
                &row.name.to_uppercase(),
                99.0,
                158.0,
                Align::Left,
            );
            let list: Vec<_> = self.game.city_enchantments(row.city);
            let rules_names: Vec<(String, u8)> = list
                .iter()
                .take(8)
                .map(|&(s, caster)| {
                    (
                        self.game.content().rules.spells.get(s).name.clone(),
                        caster.0,
                    )
                })
                .collect();
            let style = if rules_names.len() > 4 { 0 } else { 1 };
            for (k, (name, caster)) in rules_names.iter().enumerate() {
                let (x, y) = if k > 3 {
                    (163.0, 168.0 + 7.0 * (k - 4) as f32)
                } else {
                    (100.0, 168.0 + 7.0 * k as f32)
                };
                let colour = self.banner_color(*caster);
                self.bm_text(view, style, colour, name, x, y, Align::Left);
            }
            let at = self.city_position(row.city);
            self.draw_reduced_map(&lay, at, MAP);
        }

        let leave = self.city_list_ok(&lay)
            || is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::O);
        self.city_list_ui = ui;
        if leave {
            self.screen = Screen::Map;
        } else if let Some(i) = left {
            self.city_list_open(&rows[i], false);
        } else if let Some(i) = right {
            self.city_list_open(&rows[i], true);
        }
    }

    /// The OK button (`reload.lbx` 22, two frames); true when clicked.
    fn city_list_ok(&mut self, lay: &Layout) -> bool {
        let Some(tex) = self.art.picture("reload", 22, 0).cloned() else {
            return false;
        };
        let r = lay.rect((239.0, 182.0, tex.width(), tex.height()));
        let hover = r.contains(mouse_position().into());
        let frame = usize::from(hover && is_mouse_button_down(MouseButton::Left));
        self.draw_pic(lay, "reload", 22, frame, 239.0, 182.0);
        hover && is_mouse_button_pressed(MouseButton::Left)
    }

    /// Go to a city: its screen (an outpost's for an outpost), with the production menu open
    /// for a right-click.
    fn city_list_open(&mut self, row: &Row, production: bool) {
        let at = self.city_position(row.city);
        self.center_on(at);
        if row.outpost {
            self.open_outpost(row.city, false);
        } else {
            self.choosing = production;
            self.screen = Screen::City(row.city);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolling_moves_five_rows_or_one_near_the_end() {
        assert_eq!(scrolled(0, 1, 20), 5);
        assert_eq!(scrolled(5, 1, 20), 10);
        // Ten rows would pass the end of 16: one at a time.
        assert_eq!(scrolled(5, 1, 16), 6);
        assert_eq!(scrolled(7, 1, 16), 7);
        assert_eq!(scrolled(7, -1, 16), 2);
        assert_eq!(scrolled(3, -1, 16), 0);
        assert_eq!(scrolled(0, 1, 5), 0);
    }
}
