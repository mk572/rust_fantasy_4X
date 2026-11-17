//! The enemy city screen, the original's `Enemy_City_Screen` (`City_ovr55.c`): what
//! the wizard knows of another wizard's city: its size and name, race, citizens, the units that
//! stand in it (none unseen: "No Report" for a square never scouted; invisible units are left
//! out), its enchantments and its cityscape. A click anywhere or Esc leaves. Positions are the
//! original's.
//!
//! Tests: none, because it only draws and needs a window.

use crate::font::Style;
use crate::input::Input;
use crate::page::Page;
use crate::text::Args;
use crate::ui;
use f4x_core::player::view::PlayerView;
use macroquad::prelude::*;

use crate::city_screen::Layout;
use crate::view::{App, Screen, HUMAN};

/// The window's left edge (`x_start`; the picture is drawn one pixel left of it).
const X: f32 = 60.0;

/// The citizens' icons: the farmers' first in `backgrnd` 45, the workers' in 59, the rebels'
/// in 74, one for each race.
const JOBS: &[usize] = &[45, 59, 74];

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
            let city = *city;
            // A human opens the screen of a city it has seen: the check explores it first.
            let at = self.city_position(city);
            self.game
                .explore(HUMAN, at, f4x_def::half_int::HalfInt::from_raw(5));
            self.open_enemy_city(city);
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
        if app.game.economy().city_at(at) != Some(city)
            || app.game.economy().city_owner(city) == Some(HUMAN)
        {
            app.screen = Screen::Map;
            return;
        }
        let lay = Layout::new();
        let view = App::view(lay);
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        // The picture without its headings (`scripts/city_headings_text_free.py`).
        let corner = lay.rect(X - 1.0, 0.0, 0.0, 0.0);
        crate::city_screen::draw_plain(
            crate::city_screen::Plain::Enemy,
            vec2(corner.x, corner.y),
            lay.scale,
        );

        // What a human may know of the city is the report of the view, the same the AI reads
        // (`PlayerView::city_report`); a square never scouted gives none ("No Report").
        let report = PlayerView::new(&app.game, HUMAN).city_report(city);
        let Some(report) = report else {
            let none = app.text("enemy_city.no_report");
            app.draw_styled(view, &Style::city_figures(), &none, X + 54.0, 64.0);
            if input.key_pressed(KeyCode::Escape) || input.mouse_pressed(MouseButton::Left) {
                app.screen = Screen::Map;
            }
            return;
        };

        // The title: the size and the name.
        let title = app.text_with(
            "city.title",
            &Args::new()
                .text("size", app.size_name(report.figures, report.fortress))
                .text("city", app.city_name(city)),
        );
        app.draw_styled(view, &Style::sh_set(5, 5).center(), &title, X + 106.0, 3.0);
        let (race_name, race_index) = report
            .race
            .map(|r| {
                (
                    app.names
                        .race(app.game.content().rules.races.get(r).num)
                        .to_string(),
                    r.num() as usize,
                )
            })
            .unwrap_or_default();
        app.draw_styled(view, &Style::city_figures(), &race_name, X + 6.0, 19.0);
        let heading = crate::city_screen::heading_style();
        for (key, x, y) in [
            ("enemy_city.units", X + 6.0, 43.0),
            ("city.heading_enchantments", X + 140.0, 43.0),
            ("city.heading_buildings", X + 6.0, 95.0),
        ] {
            let text = app.text(key);
            app.draw_styled(view, &heading, &text, x, y);
        }

        // The citizens, farmers then workers then rebels.
        let total = (report.farmers + report.workers + report.rebels).max(1) as f32;
        let step = 11.0f32.min(196.0 / total);
        let mut x = X + 4.0;
        for (count, base) in [report.farmers, report.workers, report.rebels]
            .into_iter()
            .zip(JOBS)
        {
            for _ in 0..count {
                app.city_pic(lay, "backgrnd", base + race_index, 0, x, 27.0);
                x += step;
            }
        }

        // The garrison.
        let layout = crate::frame::Layout::new();
        for (i, &unit) in report.garrison.iter().take(12).enumerate() {
            let (ux, uy) = (
                X + 5.0 + (i % 6) as f32 * 22.0,
                52.0 + (i / 6) as f32 * 20.0,
            );
            app.draw_army_unit(&layout, unit, ux - 1.0, uy - 1.0);
            if ui::hit(lay.rect(ux, uy, 20.0, 18.0), true, input).hovered() {
                let name = app
                    .game
                    .units()
                    .unit_def(unit)
                    .map(|d| app.names.unit(d.num).to_string());
                if let Some(name) = name {
                    app.draw_styled(view, &Style::city_figures(), &name, X + 36.0, 43.0);
                }
                if input.mouse_pressed(MouseButton::Right) {
                    app.open_unit_sheet(unit);
                    return;
                }
            }
        }

        // The enchantments, in the colour of whoever cast them.
        for (k, &(spell, caster)) in report.enchantments.iter().take(6).enumerate() {
            let name = app
                .names
                .spell(app.game.content().rules.spells.get(spell).num)
                .to_string();
            let ink = app.city_ench_ink(caster.0);
            app.draw_styled(
                view,
                &Style::inked(1, ink),
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
