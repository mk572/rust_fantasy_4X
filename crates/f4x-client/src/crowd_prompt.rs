//! The box that asks what to do when a square holds more units than it may (a summoning, a hire,
//! a finished unit or a freed prisoner found it full). The player picks a unit of the square, the
//! cheapest to begin with, and either moves it to a free square next to it or disbands it; this
//! goes on until the square is back at the limit. Moving is offered only for a unit with moves
//! left and a square to go to; the squares are the eight directions.
//!
//! Tests: below. The units' order is `cheapest_first`, the compass cell of a square is
//! `direction`, and `App::resolve_crowd` sends the answer; the drawing is left to the screenshots.

use crate::font::Style;
use crate::frame::Layout;
use crate::input::Input;
use crate::modal::{kind, Modal};
use crate::text::Args;
use crate::ui;
use crate::view::{App, Screen, HUMAN};
use f4x_core::game::Command;
use f4x_core::map::Position;
use macroquad::prelude::*;

const LEFT: f32 = 12.0;
const WIDTH: f32 = 296.0;
const TOP: f32 = 8.0;
const HEAD: f32 = 26.0;
const ROW: f32 = 19.0;
const VISIBLE: usize = 6;
const BOTTOM: f32 = TOP + HEAD + ROW * VISIBLE as f32 + 28.0;

/// The question: the crowded square, its units cheapest first, the one chosen, the first row
/// shown, and whether the square to move to is being chosen.
pub(crate) struct CrowdAsk {
    square: Position,
    units: Vec<hecs::Entity>,
    chosen: usize,
    first: usize,
    moving: bool,
}

/// What the player decided in a frame.
#[derive(Debug, PartialEq, Eq)]
enum Answer {
    None,
    Disband,
    Move,
    Go(Position),
    Back,
}

/// The units by their price, the cheapest first; the oldest of equal ones first.
pub(crate) fn cheapest_first(units: &mut [hecs::Entity], price: impl Fn(hecs::Entity) -> u32) {
    units.sort_by_key(|&u| price(u));
}

/// Which of the eight directions (as column and row offsets, -1 to 1) leads from `from` to the
/// neighbouring square `to` on a map `width` squares wide (the map wraps).
pub(crate) fn direction(from: Position, to: Position, width: u32) -> (i32, i32) {
    let dx = if to.x == (from.x + 1) % width {
        1
    } else if to.x == (from.x + width - 1) % width {
        -1
    } else {
        0
    };
    let dy = (to.y as i32 - from.y as i32).signum();
    (dx, dy)
}

impl CrowdAsk {
    /// The question for the human's crowded square.
    pub(crate) fn new(app: &App, square: Position) -> Self {
        let mut units = app.own_units_at(square);
        cheapest_first(&mut units, |u| {
            app.game.units().unit_def(u).map_or(0, |d| d.cost)
        });
        CrowdAsk {
            square,
            units,
            chosen: 0,
            first: 0,
            moving: false,
        }
    }
}

impl Modal for CrowdAsk {
    fn kind(&self) -> &'static str {
        kind::CROWD
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        let mut ask = *self;
        // Answered (or the square emptied some other way).
        if !app.game.crowded_squares(HUMAN).contains(&ask.square) {
            return None;
        }
        let answer = app.crowd_frame(&mut ask, input);
        let unit = ask.units.get(ask.chosen).copied();
        match (answer, unit) {
            (Answer::Disband, Some(u)) => app.resolve_crowd(Command::DismissUnit { unit: u }),
            (Answer::Go(to), Some(u)) => {
                app.resolve_crowd(Command::MoveStack { units: vec![u], to })
            }
            (Answer::Move, _) => ask.moving = true,
            (Answer::Back, _) => ask.moving = false,
            _ => {}
        }
        if answer_resolved(&ask, app) {
            // The square may still be over the limit: ask again with the units left.
            let square = ask.square;
            if app.game.crowded_squares(HUMAN).contains(&square) {
                return Some(Box::new(CrowdAsk::new(app, square)));
            }
            return None;
        }
        Some(Box::new(ask))
    }
}

/// Whether the units listed are no longer the square's (one was moved or disbanded).
fn answer_resolved(ask: &CrowdAsk, app: &App) -> bool {
    app.own_units_at(ask.square).len() != ask.units.len()
}

impl App {
    /// Send the answer and show what it did.
    fn resolve_crowd(&mut self, command: Command) {
        let events = self.game.apply(command);
        self.note(events);
    }

    /// Open the question for the human's first crowded square, over the map.
    pub(crate) fn queue_crowd(&mut self) {
        if self.game.in_battle()
            || !matches!(self.screen, Screen::Map)
            || self.modals.has(kind::CROWD)
        {
            return;
        }
        if let Some(&square) = self.game.crowded_squares(HUMAN).first() {
            let ask = CrowdAsk::new(self, square);
            self.modals.enqueue(ask);
        }
    }

    /// One frame of the box.
    fn crowd_frame(&mut self, ask: &mut CrowdAsk, input: &Input) -> Answer {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let panel = lay.rect((LEFT, TOP, WIDTH, BOTTOM - TOP));
        draw_rectangle(
            panel.x,
            panel.y,
            panel.w,
            panel.h,
            Color::new(0.1, 0.08, 0.07, 0.97),
        );
        draw_rectangle_lines(
            panel.x,
            panel.y,
            panel.w,
            panel.h,
            2.0 * lay.scale,
            Color::new(0.75, 0.6, 0.3, 1.0),
        );
        let title = self.text("crowd.title");
        self.draw_styled(
            view,
            &Style::nearest(4, light).center(),
            &title,
            LEFT + WIDTH / 2.0,
            TOP + 5.0,
        );
        let Some(&unit) = ask.units.get(ask.chosen) else {
            return Answer::None;
        };
        if ask.moving {
            return self.crowd_squares(ask, unit, input);
        }
        let over = ask.units.len().saturating_sub(self.game.max_tile());
        let line = self.text_with("crowd.pick", &Args::new().count("n", over));
        self.draw_styled(
            view,
            &Style::nearest(1, light),
            &line,
            LEFT + 8.0,
            TOP + 17.0,
        );
        // The rows: one unit is chosen.
        let last_first = ask.units.len().saturating_sub(VISIBLE);
        let wheel = input.mouse_wheel().1;
        if wheel > 0.0 || input.key_pressed(KeyCode::PageUp) {
            ask.first = ask.first.saturating_sub(1);
        }
        if wheel < 0.0 || input.key_pressed(KeyCode::PageDown) {
            ask.first = (ask.first + 1).min(last_first);
        }
        if input.key_pressed(KeyCode::Up) {
            ask.chosen = ask.chosen.saturating_sub(1);
        }
        if input.key_pressed(KeyCode::Down) {
            ask.chosen = (ask.chosen + 1).min(ask.units.len() - 1);
        }
        // Keep the chosen row in view.
        if ask.chosen < ask.first {
            ask.first = ask.chosen;
        } else if ask.chosen >= ask.first + VISIBLE {
            ask.first = ask.chosen + 1 - VISIBLE;
        }
        let body = TOP + HEAD;
        for (i, &u) in ask.units.iter().enumerate().skip(ask.first).take(VISIBLE) {
            let y = body + ROW * (i - ask.first) as f32;
            let row = lay.rect((LEFT + 4.0, y, WIDTH - 14.0, ROW - 1.0));
            let state = ui::hit(row, true, input);
            if i == ask.chosen {
                draw_rectangle(row.x, row.y, row.w, row.h, Color::new(0.3, 0.22, 0.1, 0.8));
            } else if state.hovered() {
                draw_rectangle(row.x, row.y, row.w, row.h, Color::new(1.0, 1.0, 1.0, 0.1));
            }
            if state.clicked() {
                ask.chosen = i;
            }
            self.draw_army_unit(&lay, u, LEFT + 8.0, y);
            let name = self.army_unit_name(u);
            self.draw_styled(view, &Style::nearest(2, light), &name, LEFT + 32.0, y + 5.0);
            let price = self.game.units().unit_def(u).map_or(0, |d| d.cost);
            let cost = self.text_with("crowd.cost", &Args::new().count("n", price));
            self.draw_styled(
                view,
                &Style::nearest(1, light).right(),
                &cost,
                LEFT + WIDTH - 16.0,
                y + 7.0,
            );
        }
        // The buttons: the disbanding on the left, the moving on the right.
        let by = BOTTOM - 20.0;
        let can_move = !self.game.crowd_moves(unit).is_empty();
        let disband_label = self.text("crowd.disband");
        let move_label = self.text("crowd.move");
        let disband = self.red_button(&lay, &disband_label, LEFT + 10.0, by, input);
        let go = self.red_button_face_sized(
            &lay,
            &move_label,
            (LEFT + WIDTH - 60.0, by),
            can_move,
            49.0,
            input,
        );
        if disband {
            Answer::Disband
        } else if go && can_move || input.key_pressed(KeyCode::Enter) && can_move {
            Answer::Move
        } else {
            Answer::None
        }
    }

    /// The squares round the unit as a compass: a button for each square it can step to.
    fn crowd_squares(&mut self, ask: &CrowdAsk, unit: hecs::Entity, input: &Input) -> Answer {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let name = self.army_unit_name(unit);
        let line = self.text_with("crowd.where", &Args::new().text("name", name));
        self.draw_styled(
            view,
            &Style::nearest(1, light),
            &line,
            LEFT + 8.0,
            TOP + 17.0,
        );
        let width = self.game.map().map_or(1, |m| m.width());
        let moves = self.game.crowd_moves(unit);
        let (cx, cy) = (LEFT + WIDTH / 2.0 - 25.0, TOP + HEAD + 4.0);
        let keys = [
            ["crowd.dir_nw", "crowd.dir_n", "crowd.dir_ne"],
            ["crowd.dir_w", "", "crowd.dir_e"],
            ["crowd.dir_sw", "crowd.dir_s", "crowd.dir_se"],
        ];
        let mut answer = Answer::None;
        for (r, row) in keys.iter().enumerate() {
            for (c, key) in row.iter().enumerate() {
                let (x, y) = (cx + 18.0 * c as f32, cy + 19.0 * r as f32);
                if key.is_empty() {
                    self.draw_army_unit(&lay, unit, x - 1.0, y);
                    continue;
                }
                let target = moves
                    .iter()
                    .copied()
                    .find(|&to| direction(ask.square, to, width) == (c as i32 - 1, r as i32 - 1));
                let label = self.text(key);
                if self.red_button_face_sized(
                    &lay,
                    &label,
                    (x, y + 3.0),
                    target.is_some(),
                    17.0,
                    input,
                ) {
                    if let Some(to) = target {
                        answer = Answer::Go(to);
                    }
                }
            }
        }
        let back_label = self.text("crowd.back");
        if self.red_button(&lay, &back_label, LEFT + 10.0, BOTTOM - 20.0, input)
            || input.key_pressed(KeyCode::Escape)
        {
            return Answer::Back;
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::map::Plane;
    use f4x_def::testdir::TestResult;

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    #[test]
    fn the_cheapest_unit_comes_first_and_equal_ones_keep_their_order() {
        let mut world = hecs::World::new();
        let (a, b, c) = (world.spawn(()), world.spawn(()), world.spawn(()));
        let mut units = [a, b, c];
        let price = |u: hecs::Entity| if u == b { 10 } else { 30 };
        cheapest_first(&mut units, price);
        assert_eq!(units, [b, a, c]);
    }

    #[test]
    fn a_neighbouring_square_has_one_of_eight_directions_also_across_the_edge_of_the_map() {
        let from = at(0, 3);
        assert_eq!(direction(from, at(1, 3), 12), (1, 0));
        assert_eq!(direction(from, at(11, 2), 12), (-1, -1), "the map wraps");
        assert_eq!(direction(from, at(0, 4), 12), (0, 1));
        assert_eq!(direction(from, at(1, 2), 12), (1, -1));
    }

    #[test]
    fn the_box_is_queued_for_a_crowded_square_and_closes_when_it_is_cleared() -> TestResult {
        let mut app = App::for_test();
        let pos = app
            .game
            .units()
            .player_units(HUMAN)
            .first()
            .and_then(|&u| app.game.units().unit_position(u))
            .ok_or("the human has no unit")?;
        let kind_id = app
            .game
            .content()
            .rules
            .units
            .id_numbered(39 /* barbarian_spearmen */)
            .ok_or("a unit")?;
        app.queue_crowd();
        assert!(!app.modals.has(kind::CROWD), "no crowd, no box");
        let limit = app.game.max_tile();
        while app.own_units_at(pos).len() <= limit {
            app.game.spawn_unit(HUMAN, pos, kind_id);
        }
        app.queue_crowd();
        assert!(app.modals.has(kind::CROWD));
        let ask = CrowdAsk::new(&app, pos);
        let price = |u: hecs::Entity| app.game.units().unit_def(u).map_or(0, |d| d.cost);
        assert!(ask.units.windows(2).all(|w| price(w[0]) <= price(w[1])));
        // Disbanding the chosen unit puts the square back at the limit.
        let unit = ask.units[0];
        app.resolve_crowd(Command::DismissUnit { unit });
        assert!(app.game.crowded_squares(HUMAN).is_empty());
        Ok(())
    }
}
