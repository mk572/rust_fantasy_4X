//! The resource window: a click on the food, production, gold, power or research row of the city
//! screen opens a scroll that lists what makes the figure up, line by line, and what is taken off
//! it (the food the citizens eat, the gold the buildings cost). The lines come from
//! `Economy::city_breakdown`, which works them out from the city's own calculation, so they
//! always add up to the row. The layout is our own: the original has the window but its drawing
//! is not known. A click or Escape closes it.
//!
//! The scroll is the help scroll's paper, cut to the height the lines need, with its bottom roll
//! (`help` 0 and 1, as the combat result's scroll does it).

use crate::font::Style;
use crate::frame::{Align, Layout};
use crate::input::Input;
use crate::modal::{keep, kind, Modal};
use crate::view::App;
use f4x_core::city::economy::{Breakdown, Item, Resource};
use f4x_def::half_int::HalfInt;
use macroquad::prelude::*;

/// The left of the scroll, and its width.
const PAPER_X: f32 = 52.0;
const PAPER_W: f32 = 217.0;
/// The height of the bottom roll.
const ROLL: f32 = 23.0;
/// Where the title and the first line sit below the top of the scroll.
const TITLE_Y: f32 = 29.0;
const FIRST_LINE: f32 = 45.0;
/// The space the lines may take on the screen.
const SPACE: f32 = 200.0 - ROLL - FIRST_LINE - 6.0;
const HEADING: Color = Color::new(0.4, 0.05, 0.02, 1.0);
const LINES: Color = Color::new(0.2, 0.1, 0.04, 1.0);
const NEGATIVE: Color = Color::new(0.6, 0.08, 0.04, 1.0);

/// The window open on one resource of one city.
pub(crate) struct ResourceWindow {
    city: hecs::Entity,
    resource: Resource,
}

impl ResourceWindow {
    pub(crate) fn new(city: hecs::Entity, resource: Resource) -> Self {
        Self { city, resource }
    }
}

impl Modal for ResourceWindow {
    fn kind(&self) -> &'static str {
        kind::BREAKDOWN
    }
    fn blocks_turn(&self) -> bool {
        false
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.resource_window_frame(*self, input))
    }
}

impl App {
    /// For screenshots: the window of the first city's `resource` (food, production, gold, power
    /// or research).
    pub fn open_test_breakdown(&mut self, resource: &str) {
        let Some(city) = self.game.economy().cities().first().map(|c| c.0) else {
            return;
        };
        let resource = match resource {
            "production" => Resource::Production,
            "gold" => Resource::Gold,
            "power" => Resource::Power,
            "research" => Resource::Research,
            _ => Resource::Food,
        };
        self.modals.open(ResourceWindow::new(city, resource));
    }

    /// The name of a line.
    fn breakdown_label(&self, item: Item) -> String {
        let rules = self.game.content().rules.clone();
        match item {
            Item::Farmers => self.text("breakdown.farmers"),
            Item::Workers => self.text("breakdown.workers"),
            Item::Taxes => self.text("breakdown.taxes"),
            Item::Dwarves => self.text("breakdown.dwarves"),
            Item::Mines => self.text("breakdown.mines"),
            Item::Trade => self.text("breakdown.trade"),
            Item::Terrain => self.text("breakdown.terrain"),
            Item::WildGame => self.text("breakdown.wild_game"),
            Item::Citizens => self.text("breakdown.citizens"),
            Item::TradeGoods => self.text("breakdown.trade_goods"),
            Item::BeyondLand => self.text("breakdown.beyond_land"),
            Item::Other => self.text("breakdown.other"),
            Item::Adjustment => self.text("breakdown.adjustment"),
            Item::Race(r) => self.names.race(rules.races.get(r).num).to_string(),
            Item::Building(b) => self.names.building(rules.buildings.get(b).num).to_string(),
            Item::Spell(s) => self.names.spell(rules.spells.get(s).num).to_string(),
        }
    }

    /// The title key of a resource.
    fn breakdown_resource(&self, resource: Resource) -> String {
        match resource {
            Resource::Food => self.text("breakdown.food"),
            Resource::Production => self.text("breakdown.production"),
            Resource::Gold => self.text("breakdown.gold"),
            Resource::Power => self.text("breakdown.power"),
            Resource::Research => self.text("breakdown.research"),
        }
    }

    /// The rows of the window: label, amount and whether it is a loss; the title and the closing
    /// lines are drawn apart.
    fn breakdown_rows(&self, b: &Breakdown) -> Vec<(String, String, bool)> {
        b.lines
            .iter()
            .map(|l| {
                let loss = l.amount < HalfInt::ZERO;
                (self.breakdown_label(l.item), self.signed(l.amount), loss)
            })
            .collect()
    }

    /// An amount with its sign: `+2`, `-1.5`.
    fn signed(&self, amount: HalfInt) -> String {
        let shown = self.texts.half_amount(amount);
        if shown.starts_with('-') {
            shown
        } else {
            format!("+{shown}")
        }
    }

    /// One frame of the window: the scroll over the city screen, closed by a click or Escape.
    pub(crate) fn resource_window_frame(
        &mut self,
        window: ResourceWindow,
        input: &Input,
    ) -> Option<ResourceWindow> {
        let closing = input.key_pressed(KeyCode::Escape)
            || input.mouse_pressed(MouseButton::Left)
            || input.mouse_pressed(MouseButton::Right);
        let b = self
            .game
            .economy()
            .city_breakdown(window.city, window.resource)?;
        let rows = self.breakdown_rows(&b);
        // The total, and with an upkeep the upkeep and the net.
        let closing_lines = 1 + 2 * usize::from(b.upkeep != HalfInt::ZERO);
        let count = rows.len() + closing_lines;
        let step = (SPACE / (count as f32 + 1.0)).floor().clamp(6.0, 9.0);
        // The total and the net are set in the larger heading face and take a little more room.
        let big = if b.upkeep != HalfInt::ZERO { 8.0 } else { 3.0 };
        let height = FIRST_LINE + step * (count as f32) + big + 8.0;
        let top = ((crate::resolution::DESIGN_H - (height + ROLL)) / 2.0).floor();
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let bottom = top + height;
        let mut drawn = false;
        self.clipped((PAPER_X, top, PAPER_W, height), |app| {
            drawn = app.draw_pic(&lay, "help", 0, 0, PAPER_X, top).is_some();
        });
        if drawn {
            self.draw_pic(&lay, "help", 1, 0, PAPER_X, bottom);
        } else {
            let r = lay.rect((PAPER_X, top, PAPER_W, height + ROLL));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.55, 0.4, 0.2, 1.0));
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, Color::new(0.3, 0.2, 0.1, 1.0));
        }
        let city = self.game.city_name(window.city).unwrap_or_default();
        let title = self.text_with(
            "breakdown.title",
            &crate::text::Args::new()
                .text("resource", self.breakdown_resource(window.resource))
                .text("city", &city),
        );
        let centre = PAPER_X + PAPER_W / 2.0;
        self.draw_styled(
            view,
            &Style::nearest(4, HEADING).align(Align::Center),
            &title,
            centre,
            top + TITLE_Y,
        );
        let (left, right) = (PAPER_X + 30.0, PAPER_X + PAPER_W - 30.0);
        let mut y = top + FIRST_LINE;
        for (label, amount, loss) in &rows {
            let ink = if *loss { NEGATIVE } else { LINES };
            self.draw_styled(view, &Style::nearest(1, LINES), label, left, y);
            self.draw_styled(
                view,
                &Style::nearest(1, ink).align(Align::Right),
                amount,
                right,
                y,
            );
            y += step;
        }
        // The total the row shows; then what is taken off it and what is left.
        let heading = |app: &mut Self, label: &str, amount: &str, y: f32| {
            app.draw_styled(view, &Style::nearest(4, HEADING), label, left, y);
            app.draw_styled(
                view,
                &Style::nearest(4, HEADING).align(Align::Right),
                amount,
                right,
                y,
            );
        };
        let total = self.text("breakdown.total");
        y += 2.0;
        heading(self, &total, &self.texts.half_amount(b.total), y);
        if b.upkeep != HalfInt::ZERO {
            y += step + 3.0;
            let key = match window.resource {
                Resource::Food => self.text("breakdown.eaten"),
                _ => self.text("breakdown.upkeep"),
            };
            self.draw_styled(view, &Style::nearest(1, NEGATIVE), &key, left, y);
            self.draw_styled(
                view,
                &Style::nearest(1, NEGATIVE).align(Align::Right),
                &format!("-{}", self.texts.half_amount(b.upkeep)),
                right,
                y,
            );
            y += step + 1.0;
            let net = self.text("breakdown.net");
            heading(self, &net, &self.signed(b.total - b.upkeep), y);
        }
        (!closing).then_some(window)
    }
}
