//! The Outpost screen (task 141), the original's `Outpost_Screen` (`Outpost.c`): a small window
//! for an outpost with its reduced cityscape, the houses it has grown (bright) and has still to
//! grow (dark), the garrison and the enchantments on it. A newly founded outpost is named in the
//! box of `Change_City_Name_Popup` first; its header reads "New Outpost Founded" until the
//! screen is left. A click anywhere or Esc leaves, a click on a garrison unit selects the stack
//! on the map, a right-click on one opens its sheet. Positions are the original's.

use f4x_core::Command;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::view::{App, Screen, HUMAN};

/// The window's top-left corner.
const WINDOW: (f32, f32) = (30.0, 50.0);
/// The reduced cityscape's window (`Outpost_Cityscape`): 72 by 66 at +185, +30.
const SCAPE: (f32, f32, f32, f32) = (215.0, 80.0, 72.0, 66.0);
/// Where the name box is (`Change_City_Name_Popup`).
const NAME_BOX: (f32, f32) = (85.0, 100.0);

/// What the screen remembers while it is open.
#[derive(Debug, Clone, Default)]
pub(crate) struct OutpostUi {
    /// A freshly founded outpost: "New Outpost Founded", no garrison shown.
    pub new: bool,
    /// The name box is up; `name` is what has been typed.
    pub naming: bool,
    pub name: String,
}

/// Houses lit on the row of ten: one for each hundred people.
pub(crate) fn houses(tens: u32) -> usize {
    (tens / 10).min(10) as usize
}

impl App {
    /// Show an outpost; `new` for one just founded, which is named first.
    pub(crate) fn open_outpost(&mut self, city: hecs::Entity, new: bool) {
        self.outpost_ui = OutpostUi {
            new,
            naming: new,
            name: if new {
                self.city_name(city)
            } else {
                String::new()
            },
        };
        self.screen = Screen::Outpost(city);
    }

    /// For the screenshot check: show the wizard's first city as an outpost.
    pub fn open_first_outpost(&mut self, new: bool) {
        if let Some((city, ..)) = self.game.economy().cities().first() {
            self.open_outpost(*city, new);
        }
    }

    /// The square of garrison slot `slot`.
    fn outpost_slot(slot: usize) -> (f32, f32, f32, f32) {
        (
            WINDOW.0 + 6.0 + (slot % 5) as f32 * 20.0,
            WINDOW.1 + 53.0 + (slot / 5) as f32 * 24.0,
            18.0,
            16.0,
        )
    }

    pub(crate) fn outpost_screen(&mut self, city: hecs::Entity) {
        let at = self.city_position(city);
        if self.game.economy().city_at(at) != Some(city) {
            self.screen = Screen::Map;
            return;
        }
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (wx, wy) = WINDOW;
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let ui = self.outpost_ui.clone();

        // The map stays under the window, as the original's does.
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        let r = lay.rect((wx, wy, 250.0, 100.0));
        if self.draw_pic(&lay, "backgrnd", 32, 0, wx, wy).is_none() {
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.25, 0.2, 0.3, 1.0));
        }
        self.draw_outpost_scape(&lay, city);

        // The houses: bright for those grown, dark for the rest.
        let pop = self.game.city_population(city);
        let house_type = self
            .game
            .city_race(city)
            .map_or(0, |r| {
                self.game.content().rules.races.get(r).house_type as usize
            })
            .min(2);
        for i in 0..10 {
            let entry = if i < houses(pop.tens) { 34 } else { 37 } + house_type;
            self.draw_pic(
                &lay,
                "backgrnd",
                entry,
                0,
                wx + 8.0 + i as f32 * 14.0,
                wy + 31.0,
            );
        }

        // The header and the race.
        let title = if ui.new {
            "New Outpost Founded".to_string()
        } else {
            format!("Outpost Of {}", self.city_name(city))
        };
        self.bm_text_outlined(view, 5, &title, wx + 124.0, wy + 6.0, Align::Center);
        let race = self
            .game
            .city_race(city)
            .map(|r| self.game.content().rules.races.get(r).name.clone())
            .unwrap_or_default();
        self.bm_text(
            view,
            1,
            Color::new(0.95, 0.7, 0.2, 1.0),
            &race,
            wx + 6.0,
            wy + 23.0,
            Align::Left,
        );

        // The garrison (a new outpost shows none) and the enchantments.
        let units: Vec<_> = if ui.new {
            Vec::new()
        } else {
            self.game
                .units()
                .units_at(at)
                .into_iter()
                .take(10)
                .collect()
        };
        let mouse: Vec2 = mouse_position().into();
        let (mut left, mut right) = (None, None);
        for (slot, &unit) in units.iter().enumerate() {
            let (x, y, w, h) = Self::outpost_slot(slot);
            self.draw_army_unit(&lay, unit, x - 1.0, y - 1.0);
            if lay.rect((x, y, w, h)).contains(mouse) {
                if is_mouse_button_pressed(MouseButton::Left) {
                    left = Some(unit);
                }
                if is_mouse_button_pressed(MouseButton::Right) {
                    right = Some(unit);
                }
            }
        }
        for (k, &(spell, caster)) in self.game.city_enchantments(city).iter().take(6).enumerate() {
            let name = self.game.content().rules.spells.get(spell).name.clone();
            let colour = self.banner_color(caster.0);
            self.bm_text(
                view,
                1,
                colour,
                &name,
                wx + 114.0,
                wy + 54.0 + 6.5 * k as f32,
                Align::Left,
            );
        }

        if ui.naming {
            self.outpost_name_box(&lay, city);
            return;
        }
        if let Some(unit) = right {
            self.open_unit_sheet(unit);
        } else if is_key_pressed(KeyCode::Escape) || is_mouse_button_pressed(MouseButton::Left) {
            if left.is_some() && self.game.economy().city_owner(city) == Some(HUMAN) {
                self.select_at(at);
            }
            self.screen = Screen::Map;
        }
        let _ = light;
    }

    /// The reduced cityscape: the ground, hills or mountains to the north, a house.
    fn draw_outpost_scape(&mut self, lay: &Layout, city: hecs::Entity) {
        let at = self.city_position(city);
        let myrror = at.plane == f4x_core::map::Plane::Myrror;
        let (mut mountain, mut hills) = (false, false);
        if let Some(map) = self.game.map() {
            let grid = map.plane(at.plane);
            for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (0, 0)] {
                let class = grid
                    .get(at.x as i32 + dx, at.y as i32 + dy)
                    .and_then(|t| self.game.content().terrains.get(t))
                    .map(|d| d.class);
                mountain |= class == Some(f4x_core::content::TerrainClass::Mountains);
                hills |= class == Some(f4x_core::content::TerrainClass::Hills);
            }
        }
        let flying = self
            .game
            .city_enchantments(city)
            .iter()
            .any(|&(s, _)| self.game.content().rules.spells.get(s).id == "flying_fortress");
        let house_type = self.game.city_race(city).map_or(0, |r| {
            self.game.content().rules.races.get(r).house_type as usize
        });
        let (x, y) = (SCAPE.0 - 65.0, SCAPE.1);
        let lay = *lay;
        self.clipped(SCAPE, |app| {
            app.draw_pic(&lay, "cityscap", if myrror { 8 } else { 0 }, 4, x, y);
            if !flying {
                if mountain {
                    app.draw_pic(&lay, "cityscap", if myrror { 9 } else { 2 }, 0, x, y);
                } else if hills {
                    app.draw_pic(&lay, "cityscap", if myrror { 10 } else { 1 }, 0, x, y);
                }
            }
            app.draw_pic(
                &lay,
                "cityscap",
                25 + house_type * 5,
                0,
                SCAPE.0 + 30.0,
                SCAPE.1 + 30.0,
            );
        });
        let f = lay.rect(SCAPE);
        draw_rectangle_lines(f.x, f.y, f.w, f.h, 1.0, BLACK);
    }

    /// `Change_City_Name_Popup`: the box with "New Outpost" and a field for the name. Enter
    /// keeps what is typed (an empty name keeps the one the game chose).
    fn outpost_name_box(&mut self, lay: &Layout, city: hecs::Entity) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (x, y) = NAME_BOX;
        let gold = Color::new(0.95, 0.75, 0.25, 1.0);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        self.draw_pic(lay, "backgrnd", 33, 0, x, y);
        self.bm_text(view, 4, gold, "New Outpost", x + 10.0, y + 6.0, Align::Left);
        while let Some(c) = get_char_pressed() {
            if !c.is_control() && self.outpost_ui.name.chars().count() < f4x_core::city::NAME_LEN {
                self.outpost_ui.name.push(c);
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            self.outpost_ui.name.pop();
        }
        let shown = format!("{}_", self.outpost_ui.name);
        self.bm_text(view, 4, light, &shown, x + 16.0, y + 21.0, Align::Left);
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
            let name = std::mem::take(&mut self.outpost_ui.name);
            let events = self.game.apply(Command::RenameCity { city, name });
            self.note(events);
            self.outpost_ui.naming = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_house_is_lit_for_each_hundred_people_and_never_more_than_ten() {
        assert_eq!(houses(0), 0);
        assert_eq!(houses(9), 0);
        assert_eq!(houses(30), 3);
        assert_eq!(houses(99), 9);
        assert_eq!(houses(400), 10);
    }
}
