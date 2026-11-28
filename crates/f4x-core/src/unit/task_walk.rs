//! The movement helper: the movement part of the unit tasks (`unit/task.rs`). A player calls it
//! with `Command::WalkTasks` (these units set out for where their task sends them) and with
//! `Command::CheckTasks` (look the tasks over). It reads the targets from the army ledger, checks
//! them, ends the walks and tasks that have no target any more and moves the units that can move.
//! It changes nothing else about a task, chooses no target and calls no attack
//! (`docs/ai-unit-registry.md`).
//!
//! A raider, a marcher, a perimeter soldier and a muster go by the shorter way, through Earth
//! Gates, towers and gates ([`crate::unit::travel`]); the others walk.
//!
//! Tests: in `tests/it/c9_task_walk.rs`.

use crate::map::Position;
use crate::player::view::PlayerView;
use crate::player::PlayerId;
use crate::unit::movement::MoveError;
use crate::unit::task::{Change, Crossing, Role};
use crate::unit::travel::Travel;
use crate::world::rng::GameRng;
use crate::world::serial::Serial;
use crate::world::{Event, Rejection, World};
use f4x_def::rules::unit_flag::UnitFlag;
use f4x_def::rules::{MovementType, Stat};

/// What `Command::CheckTasks` looks over.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Check {
    /// The missions: a raid, scout, meld, march, settle, road or reinforcement whose target is no
    /// target any more is given up; a unit that lost its walk is sent on.
    Missions {
        /// Settlers whose site the player itself holds unfit (it knows more of a site than the
        /// rules ask).
        unfit_sites: Vec<Serial>,
    },
    /// The escorts that fell behind go to where their charges stand now.
    Escorts,
}

/// The tasks whose target is looked over, in this order.
const MISSIONS: &[Role] = &[
    Role::Raid,
    Role::Scout,
    Role::Meld,
    Role::March,
    Role::Settle,
    Role::Road,
    Role::Reinforce,
];

/// Why a unit sent on was not.
enum NotSent {
    /// No walk leads there.
    NoWay,
    /// Refused for another reason (no moves left, a full square).
    Refused,
}

/// Whether the task's walk goes through the doors.
fn takes_doors(role: Role) -> bool {
    matches!(
        role,
        Role::Raid | Role::March | Role::Perimeter | Role::Muster
    )
}

impl World {
    /// Where the unit's task sends it now: the square of the task; for an escort where its charge
    /// stands, and for a unit resting with a healer where the healer stands.
    pub fn task_destination(&self, unit: hecs::Entity) -> Option<Position> {
        let owner = self.units().unit_owner(unit)?;
        let serial = self.units().serial_of(unit);
        let view = PlayerView::new(self, owner);
        let tasks = view.tasks();
        let task = tasks.roles.get(serial)?;
        let of_tied = || {
            let tied = task.with?;
            view.my_units()
                .into_iter()
                .find(|&u| view.serial_of(u) == tied)
                .and_then(|u| view.unit_position(u))
        };
        match task.role {
            Role::Escort => of_tied(),
            Role::Rest => of_tied().or(task.to),
            _ => task.to,
        }
    }

    /// `Command::WalkTasks`.
    pub(crate) fn walk_tasks(
        &mut self,
        units: &[hecs::Entity],
        door: Option<Position>,
        round: u32,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        let bad = || vec![Event::Rejected(Rejection::Move(MoveError::BadSelection))];
        let Some(&lead) = units.first() else {
            return bad();
        };
        let (Some(owner), Some(to)) = (self.units().unit_owner(lead), self.task_destination(lead))
        else {
            return bad();
        };
        let serial = self.units().serial_of(lead);
        let Some(role) = self.player_tasks(owner).roles.role_of(serial) else {
            return bad();
        };
        if takes_doors(role) {
            self.travel_step(owner, units, to, door, round, rng)
        } else if self.units().unit_position(lead) == Some(to) {
            Vec::new()
        } else {
            self.go_to(units, to, rng)
        }
    }

    /// One step of the shorter way to `to`, through the doors.
    fn travel_step(
        &mut self,
        owner: PlayerId,
        units: &[hecs::Entity],
        to: Position,
        door: Option<Position>,
        round: u32,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        let step = {
            let mut travel = Travel::new(self, owner, door);
            travel.step(&PlayerView::new(self, owner), units, to)
        };
        match step {
            Ok(None) => Vec::new(),
            Ok(Some(step)) => {
                // A square with no room for the party is left alone (it may be free later).
                if self.economy().army_at(step.to()).len() + units.len() > self.max_tile() {
                    return vec![Event::Rejected(Rejection::Move(MoveError::StackFull))];
                }
                self.execute_command(step.command(units.to_vec()), round, rng)
            }
            Err(_) => vec![Event::Rejected(Rejection::NoPath(
                self.why_no_path(units, to),
            ))],
        }
    }

    /// `Command::CheckTasks`.
    pub(crate) fn check_tasks(
        &mut self,
        player: PlayerId,
        check: &Check,
        door: Option<Position>,
        round: u32,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        match check {
            Check::Missions { unfit_sites } => {
                let (mut given_up, lost) = self.look_over_missions(player, unfit_sites);
                for (unit, role) in lost {
                    // A unit with planar travel of its own changes plane where it stands.
                    self.shift_by_itself(player, unit, round, rng, &mut events);
                    if self.send_on(unit, role, door, round, rng, &mut events) {
                        continue;
                    }
                    given_up.push((unit, role));
                }
                let mut stopped = Vec::new();
                for (unit, role) in given_up {
                    let serial = self.units().serial_of(unit);
                    self.change_task(player, Change::Release { serial, role }, round);
                    stopped.push(unit);
                }
                for unit in stopped {
                    crate::unit::ledger::end_order(&self.ecs, unit);
                }
            }
            Check::Escorts => {
                for unit in self.escorts_behind(player) {
                    // An escort the game will not send keeps its task.
                    self.send_on(unit, Role::Escort, door, round, rng, &mut events);
                }
            }
        }
        events
    }

    fn change_task(&mut self, player: PlayerId, change: Change, round: u32) {
        if let Some(owner) = self.player_entity(player) {
            crate::unit::ledger::change_tasks(&self.ecs, owner, change, round);
        }
    }

    /// The missions that are over, and the units that lost their walk.
    #[allow(clippy::type_complexity)]
    fn look_over_missions(
        &self,
        player: PlayerId,
        unfit_sites: &[Serial],
    ) -> (Vec<(hecs::Entity, Role)>, Vec<(hecs::Entity, Role)>) {
        let mut over = Vec::new();
        let mut lost = Vec::new();
        let view = PlayerView::new(self, player);
        let tasks = view.tasks();
        for unit in view.my_units() {
            let serial = view.serial_of(unit);
            let Some(task) = tasks.roles.get(serial) else {
                continue;
            };
            let role = task.role;
            if !MISSIONS.contains(&role) {
                continue;
            }
            let attacks = matches!(role, Role::Raid | Role::March);
            // What the mission is about: what is attacked, else where it sends the unit.
            let target = if attacks {
                tasks.roles.objective_of(serial, role)
            } else {
                task.to
            };
            let Some(target) = target else {
                continue;
            };
            let here = view.unit_position(unit);
            // A force fights from a square beside its objective, so that counts as arrived.
            let arrived = here.is_some_and(|h| {
                Some(h) == task.to || (attacks && view.distance(h, target).is_some_and(|d| d <= 1))
            });
            // A ship that has sailed to its scouting target has explored it by arriving, and its
            // mission is not over: its party goes ashore and the next target is chosen.
            let arrived_ship = role == Role::Scout
                && arrived
                && view.unit_stat(unit, Stat::Transport) > 0
                && view
                    .unit_def(unit)
                    .is_some_and(|d| d.movement == MovementType::Sailing);
            let valid = match role {
                Role::Raid => {
                    view.has_lair(target)
                        && view
                            .towers()
                            .iter()
                            .all(|&(t, intact)| t != target || intact)
                }
                // A city on the site, or ground that takes none, ends the plan.
                Role::Settle => {
                    view.city_at(target).is_none()
                        && view.can_found_city(target)
                        && !unfit_sites.contains(&serial)
                }
                Role::Scout => !view.explored(target),
                Role::Meld => view.nodes().iter().any(|n| {
                    n.pos == target && !n.warped && n.owner != Some(player) && !view.has_lair(n.pos)
                }),
                Role::Reinforce => view.my_cities().into_iter().any(|(_, p)| p == target),
                _ => true,
            };
            if !arrived_ship && !valid {
                over.push((unit, role));
            } else if !view.unit_has_order(unit)
                && !view.unit_builds_road(unit)
                && !arrived
                && task.crossing.is_none()
            {
                // A unit can lose its walk for many reasons (a strike moved it, a battle, a
                // refused step): the ledger still knows where it goes.
                lost.push((unit, role));
            }
        }
        (over, lost)
    }

    /// The escorts with no walk that do not stand with their charges.
    fn escorts_behind(&self, player: PlayerId) -> Vec<hecs::Entity> {
        let view = PlayerView::new(self, player);
        let tasks = view.tasks();
        tasks
            .roles
            .members(Role::Escort)
            .into_iter()
            .filter(|&e| tasks.roles.crossing_of(e).is_none())
            .filter_map(|e| {
                let unit = view
                    .my_units()
                    .into_iter()
                    .find(|&u| view.serial_of(u) == e)?;
                let there = self.task_destination(unit)?;
                let here = view.unit_position(unit)?;
                (there != here && !view.unit_has_order(unit)).then_some(unit)
            })
            .collect()
    }

    /// A unit on the other plane from its target that can travel between the planes by itself
    /// shifts now (`UnitFlag::PlanarTravel`, its own, its enchantments' or its items'), so that
    /// its walk is an ordinary one. Nothing for any other unit, or while the game will not let it
    /// shift (Planar Seal, no moves left).
    fn shift_by_itself(
        &mut self,
        player: PlayerId,
        unit: hecs::Entity,
        round: u32,
        rng: &mut GameRng,
        events: &mut Vec<Event>,
    ) {
        let Some(target) = self.task_destination(unit) else {
            return;
        };
        let shifts = {
            let view = PlayerView::new(self, player);
            view.unit_position(unit)
                .is_some_and(|p| p.plane != target.plane)
                && view.unit_has_flag(unit, UnitFlag::PlanarTravel)
                && view.can_shift_plane(&[unit])
        };
        if shifts {
            let command = crate::world::Command::ShiftPlane { units: vec![unit] };
            events.extend(self.execute_command(command, round, rng));
        }
    }

    /// Send a unit on to where its task goes. A unit no walk leads there for is tagged with what
    /// it needs ([`Crossing`]: another plane, or the sea when the target is on this plane with no
    /// known land route) and keeps its task. `false`: refused for another reason.
    fn send_on(
        &mut self,
        unit: hecs::Entity,
        role: Role,
        door: Option<Position>,
        round: u32,
        rng: &mut GameRng,
        events: &mut Vec<Event>,
    ) -> bool {
        let (Some(owner), Some(target)) =
            (self.units().unit_owner(unit), self.task_destination(unit))
        else {
            return false;
        };
        let out = if takes_doors(role) {
            self.travel_step(owner, &[unit], target, door, round, rng)
        } else {
            self.go_to(&[unit], target, rng)
        };
        let no_way = |why: &MoveError| *why == MoveError::Impassable;
        let failed = out.iter().find_map(|e| match e {
            Event::Rejected(Rejection::NoPath(_)) => Some(NotSent::NoWay),
            Event::Rejected(Rejection::Move(why)) if no_way(why) => Some(NotSent::NoWay),
            Event::OrderFailed { player, why, .. } if *player == owner => Some(if no_way(why) {
                NotSent::NoWay
            } else {
                NotSent::Refused
            }),
            Event::Rejected(_) => Some(NotSent::Refused),
            _ => None,
        });
        // What the walk did is told; why it was refused is answered by the task.
        events.extend(out.into_iter().filter(|e| !matches!(e, Event::Rejected(_))));
        let Some(failed) = failed else {
            return true;
        };
        let serial = self.units().serial_of(unit);
        let view = PlayerView::new(self, owner);
        let crossing = match view.unit_position(unit) {
            Some(here) if here.plane != target.plane => Some(Crossing::Plane),
            // Only a walker waits for a ship; a flier or a swimmer crosses the water itself.
            Some(_) if matches!(failed, NotSent::NoWay) && view.is_walker(unit) => {
                Some(Crossing::Sea)
            }
            _ => None,
        };
        let Some(crossing) = crossing else {
            return false;
        };
        let change = Change::SetCrossing {
            serial,
            crossing: Some(crossing),
        };
        self.change_task(owner, change, round);
        true
    }
}
