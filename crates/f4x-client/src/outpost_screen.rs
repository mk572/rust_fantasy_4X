//! The Outpost screen (task 141), the original's `Outpost_Screen` (`Outpost.c`): a small window
//! for an outpost with its reduced cityscape, the houses it has grown (bright) and has still to
//! grow (dark), the garrison and the enchantments on it. A newly founded outpost is named in the
//! box of `Change_City_Name_Popup` first; its header reads "New Outpost Founded" until the
//! screen is left. A click anywhere or Esc leaves, a click on a garrison unit selects the stack
//! on the map, a right-click on one opens its sheet. Positions are the original's.

use crate::audio::{music, Track};
use crate::font::Style;
use crate::input::Input;
use crate::modal::{keep, Modal};
use crate::page::Page;
use crate::ui;
use f4x_core::game::Command;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::view::{App, Screen, HUMAN};

/// The window's top-left corner.
const WINDOW: (f32, f32) = (30.0, 50.0);
/// The reduced cityscape's window (`Outpost_Cityscape`): 72 by 66 at +185, +30.
const SCAPE: (f32, f32, f32, f32) = (215.0, 80.0, 72.0, 66.0);
/// Where the name box is (`Change_City_Name_Popup`).
const NAME_BOX: (f32, f32) = (85.0, 100.0);

/// What the screen remembers while it is open.
#[derive(Debug, Clone)]
pub(crate) struct OutpostUi {
    pub city: hecs::Entity,
    /// A freshly founded outpost: "New Outpost Founded", no garrison shown.
    pub new: bool,
    /// The name box is up; `name` is what has been typed.
    pub naming: bool,
    pub name: String,
    /// The question box and the enchantment list, as the city screen has them.
    pub panel: crate::sell_prompt::CityPanel,
}

/// Houses lit on the row of ten: one for each hundred people.
pub(crate) fn houses(tens: u32) -> usize {
    (tens / 10).min(10) as usize
}

/// Turn 1: the box that lets the player rename the capital, with the name typed so far.
pub(crate) struct CapitalName {
    city: hecs::Entity,
    name: String,
}

impl Modal for CapitalName {
    fn kind(&self) -> &'static str {
        "capital_name"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.capital_name_frame(*self, input))
    }
}

impl App {
    /// Show an outpost; `new` for one just founded, which is named first.
    pub(crate) fn open_outpost(&mut self, city: hecs::Entity, new: bool) {
        let ui = OutpostUi {
            city,
            new,
            naming: new,
            name: if new {
                self.city_name(city)
            } else {
                String::new()
            },
            panel: Default::default(),
        };
        self.open_page(Screen::Outpost, ui);
    }

    /// For the screenshot check: show the wizard's first city as an outpost.
    pub fn open_first_outpost(&mut self, new: bool) {
        if let Some((city, ..)) = self.game.economy().cities().first() {
            let city = *city;
            if !new {
                // Eight city spells of the player's own, to see the list page and turn off.
                let spells: Vec<_> = self
                    .game
                    .content()
                    .rules
                    .spells
                    .defs()
                    .iter()
                    .filter(|s| s.target == f4x_core::rules::SpellTarget::OwnCity)
                    .take(8)
                    .filter_map(|s| self.game.content().rules.spells.id_of(&s.id))
                    .collect();
                for spell in spells {
                    self.game.attach_enchantment(
                        f4x_core::rules::exec::Target::City(city),
                        spell,
                        HUMAN,
                    );
                }
            }
            self.open_outpost(city, new);
        }
    }

    /// The square of garrison slot `slot`: 5 to a row. The original's `Outpost_Garrison_Picture_Coords`
    /// divides x by 5 as well as y (a typo: slots 0 to 4 would share one spot).
    fn outpost_slot(slot: usize) -> (f32, f32, f32, f32) {
        (
            WINDOW.0 + 6.0 + (slot % 5) as f32 * 20.0,
            WINDOW.1 + 53.0 + (slot / 5) as f32 * 24.0,
            18.0,
            16.0,
        )
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
                mountain |= class == Some(f4x_core::map::terrain::TerrainClass::Mountains);
                hills |= class == Some(f4x_core::map::terrain::TerrainClass::Hills);
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

    /// The original's name box (`outpost_name_background`, `backgrnd` 33) with `title` at `at`:
    /// types into `name`, and returns true when Enter ends it.
    pub(crate) fn name_box(
        &mut self,
        lay: &Layout,
        (x, y): (f32, f32),
        title: &str,
        name: &mut String,
        input: &Input,
    ) -> bool {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let gold = Color::new(0.95, 0.75, 0.25, 1.0);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        self.draw_pic(lay, "backgrnd", 33, 0, x, y);
        self.draw_styled(view, &Style::nearest(4, gold), title, x + 10.0, y + 6.0);
        for c in input.typed() {
            if !c.is_control() && name.chars().count() < f4x_core::city::NAME_LEN {
                name.push(c);
            }
        }
        if input.key_pressed(KeyCode::Backspace) {
            name.pop();
        }
        self.draw_styled(
            view,
            &Style::nearest(4, light),
            &format!("{name}_"),
            x + 16.0,
            y + 21.0,
        );
        input.key_pressed(KeyCode::Enter) || input.key_pressed(KeyCode::KpEnter)
    }

    /// Turn 1: the box over the screen that lets the player rename the capital
    /// (`Change_Home_City_Name_Popup`, the box at (60,30) titled "Name Starting City").
    pub(crate) fn capital_name_frame(
        &mut self,
        CapitalName { city, mut name }: CapitalName,
        input: &Input,
    ) -> Option<CapitalName> {
        let lay = Layout::new();
        let prompt = self.text("outpost.name_starting");
        if self.name_box(&lay, (60.0, 30.0), &prompt, &mut name, input) {
            let events = self.game.apply(Command::RenameCity { city, name });
            self.note(events);
            None
        } else {
            Some(CapitalName { city, name })
        }
    }

    /// Offer the naming box for the human capital, if there is one.
    pub(crate) fn offer_capital_naming(&mut self) {
        let capital = self
            .game
            .economy()
            .cities()
            .into_iter()
            .find(|(e, c, _)| c.owner == HUMAN && self.game.economy().is_fortress_city(*e));
        if let Some((city, ..)) = capital {
            let name = self.city_name(city);
            self.modals.open(CapitalName { city, name });
        }
    }
}

impl Page for OutpostUi {
    fn music(&self, _app: &App) -> Option<Track> {
        self.new.then(|| Track::piece(music::NEW_OUTPOST))
    }

    fn frame(&mut self, app: &mut App, input: &Input) {
        let city = self.city;
        let at = app.city_position(city);
        if app.game.economy().city_at(at) != Some(city) {
            app.screen = Screen::Map;
            return;
        }
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (wx, wy) = WINDOW;
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let ui = self.clone();

        // The map stays under the window, as the original's does.
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        app.draw_map();
        let r = lay.rect((wx, wy, 250.0, 100.0));
        if app.draw_pic(&lay, "backgrnd", 32, 0, wx, wy).is_none() {
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.25, 0.2, 0.3, 1.0));
        }
        app.draw_outpost_scape(&lay, city);

        // The houses: bright for those grown, dark for the rest.
        let pop = app.game.city_population(city);
        let house_type = app
            .game
            .city_race(city)
            .map_or(0, |r| {
                app.game.content().rules.races.get(r).house_type as usize
            })
            .min(2);
        for i in 0..10 {
            let entry = if i < houses(pop.tens) { 34 } else { 37 } + house_type;
            app.draw_pic(
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
            app.text("outpost.founded")
        } else {
            app.text_with("outpost.of", &Args::new().text("city", app.city_name(city)))
        };
        app.draw_styled(
            view,
            &Style::outlined(5).center(),
            &title,
            wx + 124.0,
            wy + 6.0,
        );
        let race = app
            .game
            .city_race(city)
            .map(|r| app.game.content().rules.races.get(r).name.clone())
            .unwrap_or_default();
        app.draw_styled(
            view,
            &Style::nearest(1, Color::new(0.95, 0.7, 0.2, 1.0)),
            &race,
            wx + 6.0,
            wy + 23.0,
        );

        // The garrison (a new outpost shows none) and the enchantments.
        let units: Vec<_> = if ui.new {
            Vec::new()
        } else {
            app.game.units().units_at(at).into_iter().take(10).collect()
        };
        let (mut left, mut right) = (None, None);
        for (slot, &unit) in units.iter().enumerate() {
            let (x, y, w, h) = App::outpost_slot(slot);
            app.draw_army_unit(&lay, unit, x - 1.0, y - 1.0);
            let state = ui::hit(lay.rect((x, y, w, h)), true, input);
            if state.hovered() {
                if state.clicked() {
                    left = Some(unit);
                }
                if input.mouse_pressed(MouseButton::Right) {
                    right = Some(unit);
                }
            }
        }
        // The enchantments page and turn off as the city screen's do; a click on one is not the
        // click that leaves. A question opened by a click waits a frame before it draws.
        let asking = self.panel.sell.is_some();
        let city_lay = crate::city_screen::Layout::new();
        let on_list = app.city_enchantment_list(
            &mut self.panel,
            city,
            city_lay,
            (wx + 114.0, wy + 54.0, 0.0),
            !asking,
            input,
        );
        if asking {
            app.draw_sell_prompt(&mut self.panel, input);
            return;
        }
        if ui.naming {
            self.name_box(app, &lay, input);
            return;
        }
        if let Some(unit) = right {
            app.open_unit_sheet(unit);
        } else if input.key_pressed(KeyCode::Escape)
            || (input.mouse_pressed(MouseButton::Left) && !on_list)
        {
            if left.is_some() && app.game.economy().city_owner(city) == Some(HUMAN) {
                app.select_at(at);
            }
            app.screen = Screen::Map;
        }
        let _ = light;
    }
}

impl OutpostUi {
    /// `Change_City_Name_Popup`: the box with "New Outpost" and a field for the name. Enter
    /// keeps what is typed (an empty name keeps the one the game chose).
    fn name_box(&mut self, app: &mut App, lay: &Layout, input: &Input) {
        let city = self.city;
        let mut name = std::mem::take(&mut self.name);
        let prompt = app.text("outpost.name_prompt");
        let done = app.name_box(lay, NAME_BOX, &prompt, &mut name, input);
        self.name = name;
        if done {
            let name = std::mem::take(&mut self.name);
            let events = app.game.apply(Command::RenameCity { city, name });
            app.note(events);
            self.naming = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_freshly_founded_outpost_plays_its_piece() {
        let ui = |new| OutpostUi {
            city: hecs::Entity::DANGLING,
            new,
            naming: new,
            name: String::new(),
            panel: Default::default(),
        };
        let app = App::for_test();
        assert!(ui(true).music(&app).is_some());
        assert!(ui(false).music(&app).is_none());
    }

    #[test]
    fn a_house_is_lit_for_each_hundred_people_and_never_more_than_ten() {
        assert_eq!(houses(0), 0);
        assert_eq!(houses(9), 0);
        assert_eq!(houses(30), 3);
        assert_eq!(houses(99), 9);
        assert_eq!(houses(400), 10);
    }
}
