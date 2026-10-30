//! The enemy city screen (task 141), the original's `Enemy_City_Screen` (`City_ovr55.c`): what
//! the wizard knows of another wizard's city: its size and name, race, citizens, the units that
//! stand in it (none unseen: "No Report" for a square never scouted; invisible units are left
//! out), its enchantments and its cityscape. A click anywhere or Esc leaves. Positions are the
//! original's.

use crate::font::Style;
use crate::input::Input;
use crate::page::Page;
use crate::ui;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::city_screen::Layout;
use crate::view::{App, Screen, HUMAN};

/// The window's left edge (`x_start`; the picture is drawn one pixel left of it).
const X: f32 = 60.0;

/// The citizens' icons: the farmers' first in `backgrnd` 45, the workers' in 59, the rebels'
/// in 74, one for each race.
const JOBS: [usize; 3] = [45, 59, 74];

impl App {
    pub(crate) fn open_enemy_city(&mut self, city: hecs::Entity) {
        self.open_page(Screen::EnemyCity, EnemyCityPage { city });
    }

    /// For the screenshot check: show the first city of another wizard, or the first city.
    pub fn open_first_enemy_city(&mut self) {
        let cities = self.game.economy().cities();
        let pick = cities
            .iter()
            .find(|(_, c, _)| c.owner != HUMAN)
            .or_else(|| cities.first());
        if let Some((city, ..)) = pick {
            self.open_enemy_city(*city);
        }
    }
}

/// The city being looked at.
struct EnemyCityPage {
    city: hecs::Entity,
}

impl Page for EnemyCityPage {
    fn frame(&mut self, app: &mut App, input: &Input) {
        let city = self.city;
        let at = app.city_position(city);
        if app.game.economy().city_at(at) != Some(city) {
            app.screen = Screen::Map;
            return;
        }
        let lay = Layout::new();
        let view = App::view(lay);
        let orange = Color::new(0.95, 0.7, 0.2, 1.0);
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        if app.city_pic(lay, "reload", 26, 0, X - 1.0, 0.0) {
        } else {
            let r = lay.rect(X - 1.0, 0.0, 212.0, crate::resolution::DESIGN_H);
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.3, 0.25, 0.2, 1.0));
        }

        // The title: the size and the name.
        let eco = app.game.economy().city_economy(city).unwrap_or_default();
        let title = app.text_with(
            "city.title",
            &Args::new()
                .text("size", app.city_size_name(city))
                .text("city", app.city_name(city)),
        );
        app.draw_styled(view, &Style::outlined(5).center(), &title, X + 106.0, 3.0);
        let (race_name, race_index) = app
            .game
            .city_race(city)
            .map(|r| {
                (
                    app.game.content().rules.races.get(r).name.clone(),
                    r.index(),
                )
            })
            .unwrap_or_default();
        app.draw_styled(view, &Style::nearest(1, orange), &race_name, X + 6.0, 19.0);
        let units = app.text("enemy_city.units");
        app.draw_styled(view, &Style::nearest(1, orange), &units, X + 6.0, 43.0);

        // The citizens, farmers then workers then rebels.
        let total = (eco.farmers + eco.workers + eco.rebels).max(1) as f32;
        let step = 11.0f32.min(196.0 / total);
        let mut x = X + 4.0;
        for (count, base) in [eco.farmers, eco.workers, eco.rebels].into_iter().zip(JOBS) {
            for _ in 0..count {
                app.city_pic(lay, "backgrnd", base + race_index, 0, x, 27.0);
                x += step;
            }
        }

        // The garrison, if the square has been scouted.
        if app.game.is_explored(HUMAN, at) {
            let owner = app.game.economy().city_owner(city);
            let units: Vec<_> = app
                .game
                .units()
                .units_at(at)
                .into_iter()
                .filter(|&u| app.game.units().unit_owner(u) == owner)
                .filter(|&u| !app.game.units().unit_has_ability(u, "invisibility"))
                .collect();
            let layout = crate::frame::Layout::new();
            for (i, &unit) in units.iter().take(12).enumerate() {
                let (ux, uy) = (
                    X + 5.0 + (i % 6) as f32 * 22.0,
                    52.0 + (i / 6) as f32 * 20.0,
                );
                app.draw_army_unit(&layout, unit, ux - 1.0, uy - 1.0);
                if ui::hit(lay.rect(ux, uy, 20.0, 18.0), true, input).hovered() {
                    let name = app.game.units().unit_def(unit).map(|d| d.name.clone());
                    if let Some(name) = name {
                        app.draw_styled(view, &Style::nearest(1, orange), &name, X + 36.0, 43.0);
                    }
                    if input.mouse_pressed(MouseButton::Right) {
                        app.open_unit_sheet(unit);
                        return;
                    }
                }
            }
        } else {
            let none = app.text("enemy_city.no_report");
            app.draw_styled(view, &Style::nearest(1, orange), &none, X + 54.0, 64.0);
        }

        // The enchantments, in the colour of whoever cast them.
        for (k, &(spell, caster)) in app.game.city_enchantments(city).iter().take(6).enumerate() {
            let name = app.game.content().rules.spells.get(spell).name.clone();
            let colour = app.banner_color(caster.0);
            app.draw_styled(
                view,
                &Style::nearest(1, colour),
                &name,
                X + 139.0,
                51.0 + 6.5 * k as f32,
            );
        }
        app.draw_city_buildings_at(city, lay, X + 4.0, 101.0, input);

        if input.key_pressed(KeyCode::Escape) || input.mouse_pressed(MouseButton::Left) {
            app.screen = Screen::Map;
        }
    }
}
