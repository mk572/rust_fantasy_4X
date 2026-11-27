//! The unit window's Order button: a list of the orders for the selected units, and for an order
//! with a target the choice of the square on the map (`docs/game-flow.md` 3.2). Escape or a
//! right-click leaves either without an order.
//!
//! Tests: below.

use crate::city_pick::Step;
use crate::input::Input;
use crate::modal::{keep, Modal};
use crate::view::{App, Screen, HUMAN};
use f4x_core::map::Position;
use f4x_core::rules::Stat;
use f4x_core::unit::{Errand, Standby};
use f4x_core::world::{Command, Event};
use f4x_def::rules::Ability;
use macroquad::prelude::*;

/// An order of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Choice {
    /// Walk to a square picked on the map.
    GoTo,
    Scout,
    Meld,
    Settle,
    Road,
    Escort,
    Reinforce,
    Patrol,
    /// Take back what the units were told to do.
    Cancel,
}

impl Choice {
    /// The text key of the line.
    fn key(self) -> &'static str {
        match self {
            Choice::GoTo => "order.go_to",
            Choice::Scout => "order.scout",
            Choice::Meld => "order.meld",
            Choice::Settle => "order.settle",
            Choice::Road => "order.road",
            Choice::Escort => "order.escort",
            Choice::Reinforce => "order.reinforce",
            Choice::Patrol => "order.patrol",
            Choice::Cancel => "order.cancel",
        }
    }
}

/// The list of orders and the line highlighted.
pub(crate) struct OrderMenu {
    choices: Vec<Choice>,
    row: usize,
}

/// Which of several units on the square picked is to be escorted.
pub(crate) struct ChargeMenu {
    charges: Vec<hecs::Entity>,
    row: usize,
}

/// What the square being picked is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Aim {
    Walk,
    Meld,
    Settle,
    Escort,
    Reinforce,
}

impl Aim {
    /// The text key of what the player is asked to click.
    fn prompt(self) -> &'static str {
        match self {
            Aim::Walk => "order.pick_square",
            Aim::Meld => "order.pick_node",
            Aim::Settle => "order.pick_site",
            Aim::Escort => "order.pick_charge",
            Aim::Reinforce => "order.pick_city",
        }
    }
}

/// A square being picked for an order: it holds for the stack and the turn it was asked in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OrderTarget {
    aim: Aim,
    round: u32,
    from: Position,
}

impl Modal for OrderMenu {
    fn kind(&self) -> &'static str {
        "order_menu"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.order_menu_frame(*self, input))
    }
}

impl Modal for ChargeMenu {
    fn kind(&self) -> &'static str {
        "charge_menu"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.charge_menu_frame(*self, input))
    }
}

impl App {
    /// The orders the selected units can be given.
    fn order_choices(&self) -> Vec<Choice> {
        let units = self.game.units();
        let any = |test: &dyn Fn(hecs::Entity) -> bool| self.selection.iter().any(|&u| test(u));
        let mut choices = vec![Choice::GoTo, Choice::Scout];
        if any(&|u| units.unit_has_ability(u, Ability::new(42 /* meld */))) {
            choices.push(Choice::Meld);
        }
        if any(&|u| units.unit_is_settler(u)) {
            choices.push(Choice::Settle);
        }
        if any(&|u| units.unit_stat(u, Stat::Construction) > 0) {
            choices.push(Choice::Road);
        }
        if units
            .player_units(HUMAN)
            .iter()
            .any(|u| !self.selection.contains(u))
        {
            choices.push(Choice::Escort);
        }
        if !self.game.economy().cities_of(HUMAN).is_empty() {
            choices.push(Choice::Reinforce);
        }
        choices.push(Choice::Patrol);
        if any(&|u| self.unit_told(u)) {
            choices.push(Choice::Cancel);
        }
        choices
    }

    /// Whether the unit has been told something: an order or standby.
    fn unit_told(&self, unit: hecs::Entity) -> bool {
        self.game.unit_has_order(unit) || self.game.units().unit_standby(unit).is_some()
    }

    /// Take back what the units were told, so that they await orders again.
    pub(crate) fn wake(&mut self, units: Vec<hecs::Entity>) {
        let units: Vec<_> = units.into_iter().filter(|&u| self.unit_told(u)).collect();
        if units.is_empty() {
            return;
        }
        self.game.apply(
            HUMAN,
            Command::ClearOrders {
                units: units.clone(),
            },
        );
        self.game.apply(
            HUMAN,
            Command::SetStandby {
                units,
                standby: None,
            },
        );
    }

    /// The Order button.
    pub(crate) fn open_order_menu(&mut self) {
        self.order_target = None;
        if self.selection.is_empty() {
            return;
        }
        let choices = self.order_choices();
        self.modals.open(OrderMenu { choices, row: 0 });
    }

    fn order_menu_frame(&mut self, mut menu: OrderMenu, input: &Input) -> Option<OrderMenu> {
        let title = self.text("order.title");
        let lines: Vec<String> = menu.choices.iter().map(|c| self.text(c.key())).collect();
        let (row, step) = self.pick_list(&title, &lines, menu.row, input);
        if input.mouse_pressed(MouseButton::Right) {
            return None;
        }
        match step {
            Step::Stay => {
                menu.row = row;
                Some(menu)
            }
            Step::Cancel => None,
            Step::Pick => {
                self.choose_order(menu.choices[row]);
                None
            }
        }
    }

    fn charge_menu_frame(&mut self, mut menu: ChargeMenu, input: &Input) -> Option<ChargeMenu> {
        let title = self.text("order.which_charge");
        let lines: Vec<String> = menu
            .charges
            .iter()
            .map(|&u| self.army_unit_name(u))
            .collect();
        let (row, step) = self.pick_list(&title, &lines, menu.row, input);
        if input.mouse_pressed(MouseButton::Right) {
            return None;
        }
        match step {
            Step::Stay => {
                menu.row = row;
                Some(menu)
            }
            Step::Cancel => None,
            Step::Pick => {
                let charge = menu.charges[row];
                self.give_errand(Errand::Escort { charge });
                None
            }
        }
    }

    /// A line of the menu was picked.
    pub(crate) fn choose_order(&mut self, choice: Choice) {
        let units = self.selection.clone();
        match choice {
            Choice::GoTo => self.pick_target(Aim::Walk),
            Choice::Meld => self.pick_target(Aim::Meld),
            Choice::Settle => self.pick_target(Aim::Settle),
            Choice::Escort => self.pick_target(Aim::Escort),
            Choice::Reinforce => self.pick_target(Aim::Reinforce),
            Choice::Scout => self.give_errand(Errand::Scout {
                to: None,
                avoid: Vec::new(),
                favour: Vec::new(),
            }),
            // The road is planned square by square (`road_plan.rs`).
            Choice::Road => {
                self.screen = Screen::Map;
                self.start_road_plan();
            }
            Choice::Patrol => self.stand_by(units, Standby::Patrol),
            Choice::Cancel => self.wake(units),
        }
    }

    /// Ask for the square of an order.
    fn pick_target(&mut self, aim: Aim) {
        let Some(from) = self.selection_position() else {
            return;
        };
        self.screen = Screen::Map;
        self.order_target = Some(OrderTarget {
            aim,
            round: self.game.round(),
            from,
        });
        self.log.push(self.text(aim.prompt()));
    }

    /// A click on `pos` while a square is being picked: whether it was taken as the target. A
    /// choice made for another stack or in an earlier turn has lapsed.
    pub(crate) fn order_target_click(&mut self, pos: Position) -> bool {
        let Some(target) = self.order_target.take() else {
            return false;
        };
        if target.round != self.game.round() || self.selection_position() != Some(target.from) {
            return false;
        }
        // The whole selection gets the order, so that it stays together: with a spent unit among
        // them all set out at the end of the turn.
        match target.aim {
            Aim::Walk => {
                self.send_go_to(self.selection.clone(), pos);
                self.leave_if_told();
            }
            Aim::Meld => self.give_errand(Errand::Meld { to: pos }),
            Aim::Settle => self.give_errand(Errand::Settle {
                to: pos,
                name: None,
            }),
            Aim::Reinforce => self.give_errand(Errand::Reinforce { to: pos }),
            Aim::Escort => self.escort_at(pos),
        }
        true
    }

    /// Escort the unit on `pos` that is not among the selected, asking which when there are
    /// several.
    fn escort_at(&mut self, pos: Position) {
        let charges: Vec<_> = self
            .own_units_at(pos)
            .into_iter()
            .filter(|u| !self.selection.contains(u))
            .collect();
        match charges.as_slice() {
            [] => self.log.push(self.text("order.nobody_to_escort")),
            &[charge] => self.give_errand(Errand::Escort { charge }),
            _ => self.modals.open(ChargeMenu { charges, row: 0 }),
        }
    }

    /// Give the selected units an order.
    fn give_errand(&mut self, errand: Errand) {
        let events = self.game.apply(
            HUMAN,
            Command::GiveOrder {
                units: self.selection.clone(),
                errand,
            },
        );
        let founded = events.iter().find_map(|e| match e {
            Event::CityFounded { city, .. } => Some(*city),
            _ => None,
        });
        self.note(events);
        match founded {
            Some(city) => self.open_outpost(city, true),
            None => self.leave_if_told(),
        }
    }

    /// A stack that has its order is not asked again: the next one is.
    fn leave_if_told(&mut self) {
        if self.selection.iter().all(|&u| self.game.unit_has_order(u)) {
            self.select_next_idle(false);
        }
    }

    /// Escape or a right-click while a square is being picked: whether one was.
    pub(crate) fn cancel_order_target(&mut self) -> bool {
        let picking = self.order_target.take().is_some();
        if picking {
            self.log.push(self.text("order.not_given"));
        }
        picking
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::FrameClick;
    use f4x_def::testdir::TestResult;

    /// The human's first stack selected, and a square it can walk to that is further than the
    /// stack gets this turn.
    fn selected() -> Result<(App, Position), Box<dyn std::error::Error>> {
        let mut app = App::for_test();
        let first = *app
            .game
            .units()
            .player_units(HUMAN)
            .first()
            .ok_or("the human has no unit")?;
        let at = app.game.units().unit_position(first).ok_or("a position")?;
        app.select_at(at);
        // With no moves left nothing walks or waits for orders, so nothing is animated and the
        // camera stays: both need a window.
        for u in app.game.units().player_units(HUMAN) {
            app.game
                .scenario_set_moves_left(u, f4x_def::half_int::HalfInt::ZERO);
        }
        let map = app.game.map().ok_or("a map")?;
        let squares: Vec<Position> = (0..map.height())
            .flat_map(|y| (0..map.width()).map(move |x| Position { x, y, ..at }))
            .filter(|&p| map.plane(p.plane).chebyshev((p.x, p.y), (at.x, at.y)) == 8)
            .collect();
        let far = squares
            .into_iter()
            .find(|&p| app.game.find_path(&app.selection, p).is_some())
            .ok_or("no square to walk to")?;
        Ok((app, far))
    }

    #[test]
    fn go_to_picked_from_the_menu_sends_the_selection_to_the_square_clicked() -> TestResult {
        let (mut app, far) = selected()?;
        let units = app.selection.clone();
        app.choose_order(Choice::GoTo);
        assert!(app.order_target_click(far));
        for u in units {
            assert_eq!(app.game.unit_destination(u), Some(far));
        }
        assert!(!app.order_target_click(far), "the pick is over");
        Ok(())
    }

    #[test]
    fn a_pick_given_up_or_left_for_another_stack_orders_nothing() -> TestResult {
        let (mut app, far) = selected()?;
        let units = app.selection.clone();
        app.choose_order(Choice::GoTo);
        assert!(app.cancel_order_target());
        assert!(!app.order_target_click(far));
        app.choose_order(Choice::GoTo);
        app.selection.clear();
        assert!(!app.order_target_click(far));
        assert!(units.iter().all(|&u| !app.game.unit_has_order(u)));
        Ok(())
    }

    #[test]
    fn cancel_is_offered_to_units_with_an_order_and_takes_it_back() -> TestResult {
        let (mut app, far) = selected()?;
        let units = app.selection.clone();
        assert!(!app.order_choices().contains(&Choice::Cancel));
        app.choose_order(Choice::GoTo);
        assert!(app.order_target_click(far));
        app.selection = units.clone();
        assert_eq!(app.order_choices().last(), Some(&Choice::Cancel));
        app.choose_order(Choice::Cancel);
        assert!(units.iter().all(|&u| !app.game.unit_has_order(u)));
        Ok(())
    }

    #[test]
    fn a_click_on_a_units_picture_wakes_that_unit_alone() -> TestResult {
        let (mut app, far) = selected()?;
        let units = app.selection.clone();
        let &[first, second, ..] = units.as_slice() else {
            return Err("the human's first stack has one unit".into());
        };
        let click = |unit, pick_only| FrameClick {
            slot: Some(unit),
            pick_only,
            ..FrameClick::default()
        };
        for standby in [Standby::Done, Standby::Patrol] {
            app.selection = units.clone();
            app.stand_by(units.clone(), standby);
            app.selection = units.clone();
            // The first click on a stack picked whole picks the unit clicked.
            app.unit_window_click(click(first, true));
            assert_eq!(app.game.units().unit_standby(first), None);
            assert_eq!(app.game.units().unit_standby(second), Some(standby));
        }
        app.selection = units.clone();
        app.choose_order(Choice::GoTo);
        assert!(app.order_target_click(far));
        app.selection = units.clone();
        // Taking a unit out of the selection leaves its order.
        app.unit_window_click(click(first, false));
        assert!(app.game.unit_has_order(first));
        // Putting it back in wakes it.
        app.unit_window_click(click(first, false));
        assert!(!app.game.unit_has_order(first));
        assert!(app.game.unit_has_order(second));
        Ok(())
    }

    #[test]
    fn the_menu_offers_what_the_units_can_do() -> TestResult {
        let (mut app, _) = selected()?;
        let first = app.selection[0];
        let at = app.game.units().unit_position(first).ok_or("a position")?;
        for (unit, choice) in [
            (111 /* high_men_settlers */, Choice::Settle),
            (110 /* high_men_engineers */, Choice::Road),
            (154 /* magic_spirit */, Choice::Meld),
        ] {
            assert!(!app.order_choices().contains(&choice), "{unit}");
            let id = app
                .game
                .content()
                .rules
                .units
                .id_numbered(unit)
                .ok_or(format!("no unit {unit}"))?;
            app.selection = vec![app.game.spawn_unit(HUMAN, at, id)];
            assert!(app.order_choices().contains(&choice), "{unit}");
            app.selection = vec![first];
        }
        Ok(())
    }

    #[test]
    fn escort_is_given_for_the_unit_on_the_square_clicked() -> TestResult {
        let (mut app, _) = selected()?;
        let units = app.selection.clone();
        let &[first, second] = units.as_slice() else {
            return Err("the human's first stack is not two units".into());
        };
        let at = app.game.units().unit_position(first).ok_or("a position")?;
        app.selection = vec![first];
        app.choose_order(Choice::Escort);
        assert!(app.order_target_click(at));
        assert!(app.game.unit_has_order(first));
        assert!(!app.game.unit_has_order(second));
        Ok(())
    }

    #[test]
    fn patrol_picked_from_the_menu_ends_the_order() -> TestResult {
        let (mut app, far) = selected()?;
        let units = app.selection.clone();
        app.choose_order(Choice::GoTo);
        assert!(app.order_target_click(far));
        app.selection = units.clone();
        app.choose_order(Choice::Patrol);
        assert!(units.iter().all(|&u| !app.game.unit_has_order(u)));
        Ok(())
    }
}
