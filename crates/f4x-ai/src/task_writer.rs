//! How the computer player reads and changes its unit tasks. They are in its army ledger
//! (`f4x_core::unit::task`): [`Ai::tasks`] borrows them there, and each method of [`Writer`]
//! ([`Ai::write`]) is one command that changes them. There is no copy of them in the AI.
//!
//! Tests: those of the routines that give tasks, and `f4x-core`, `tests/it/c9_task.rs`.

use f4x_core::map::Position;
use f4x_core::unit::ledger::TasksRef;
use f4x_core::unit::task::{Assembly, Change, Crossing, MusterKind, Role, Tasks};
use f4x_core::world::serial::Serial;
use f4x_core::world::{Command, Event, World};
use f4x_master::Game;

use crate::Ai;

impl Ai {
    /// The unit tasks of this player. Nothing in the game changes while they are held.
    pub fn tasks<'w>(&self, world: &'w World) -> TasksRef<'w> {
        world.player_tasks(self.player)
    }

    /// The same tasks, to change them.
    pub fn write<'a>(&'a self, game: &'a mut Game) -> Writer<'a> {
        Writer { ai: self, game }
    }
}

/// Changes of the unit tasks, one command each. Each method does what the method of
/// `f4x_core::unit::task::Registry` of its name does, in the round of the game.
pub struct Writer<'a> {
    ai: &'a Ai,
    game: &'a mut Game,
}

impl Writer<'_> {
    #[track_caller]
    fn send(&mut self, change: Change) {
        // The other wizard's turn may have ended the game: nothing more is written.
        if self.game.game_over().is_some() {
            return;
        }
        let command = Command::Task {
            player: self.ai.player,
            change,
        };
        let events = self.game.apply(self.ai.player, command);
        if let Some(why) = events.iter().find_map(|e| match e {
            Event::Rejected(why) => Some(why),
            _ => None,
        }) {
            self.ai
                .note_failure(self.game, "a unit task".to_string(), format!("{why:?}"));
        }
        self.ai.task_events.borrow_mut().extend(events);
    }

    fn role_of(&self, serial: Serial) -> Option<Role> {
        self.ai.tasks(self.game).roles.role_of(serial)
    }

    /// Give the unit `role`; the error is the role it keeps.
    #[track_caller]
    pub fn claim(&mut self, serial: Serial, role: Role, to: Option<Position>) -> Result<(), Role> {
        self.send(Change::Claim { serial, role, to });
        match self.role_of(serial) {
            Some(held) if held == role => Ok(()),
            held => Err(held.unwrap_or(Role::AwaitingOrders)),
        }
    }

    /// The units that got `role`.
    #[track_caller]
    pub fn claim_all(
        &mut self,
        serials: impl IntoIterator<Item = Serial>,
        role: Role,
    ) -> Vec<Serial> {
        let serials: Vec<Serial> = serials.into_iter().collect();
        self.send(Change::ClaimAll {
            serials: serials.clone(),
            role,
        });
        serials
            .into_iter()
            .filter(|&s| self.role_of(s) == Some(role))
            .collect()
    }

    #[track_caller]
    pub fn assign(&mut self, role: Role, serials: impl IntoIterator<Item = Serial>) {
        let serials = serials.into_iter().collect();
        self.send(Change::Assign { role, serials });
    }

    /// Whether the unit held the role.
    #[track_caller]
    pub fn release(&mut self, serial: Serial, role: Role) -> bool {
        let held = self.role_of(serial) == Some(role);
        self.send(Change::Release { serial, role });
        held
    }

    #[track_caller]
    pub fn release_any(&mut self, serial: Serial) {
        self.send(Change::ReleaseAny { serial });
    }

    #[track_caller]
    pub fn release_role(&mut self, role: Role) {
        self.send(Change::ReleaseRole { role });
    }

    #[track_caller]
    pub fn release_many(&mut self, serials: &[Serial], role: Role) {
        self.send(Change::ReleaseMany {
            serials: serials.to_vec(),
            role,
        });
    }

    /// Keep `role` only on the units `keep` says so.
    #[track_caller]
    pub fn retain_role(&mut self, role: Role, mut keep: impl FnMut(Serial) -> bool) {
        let mut gone = self.ai.tasks(self.game).roles.members(role);
        gone.retain(|&s| !keep(s));
        self.release_many(&gone, role);
    }

    /// Give the units their base roles: one command with those that differ from what the ledger
    /// keeps, none when nothing does.
    #[track_caller]
    pub fn set_bases(&mut self, bases: impl IntoIterator<Item = (Serial, Role, Option<Position>)>) {
        let bases: Vec<_> = {
            let tasks = self.ai.tasks(self.game);
            bases
                .into_iter()
                .filter(|&(serial, role, to)| tasks.roles.base_differs(serial, role, to))
                .collect()
        };
        if !bases.is_empty() {
            self.send(Change::SetBases { bases });
        }
    }

    #[track_caller]
    pub fn set_target(&mut self, serial: Serial, role: Role, to: Option<Position>) {
        self.send(Change::SetTarget { serial, role, to });
    }

    /// Where a unit sent against something goes, and against what.
    #[track_caller]
    pub fn stage(&mut self, serial: Serial, role: Role, to: Option<Position>, objective: Position) {
        self.send(Change::Stage {
            serial,
            role,
            to,
            objective,
        });
    }

    #[track_caller]
    pub fn set_crossing(&mut self, serial: Serial, crossing: Option<Crossing>) {
        self.send(Change::SetCrossing { serial, crossing });
    }

    #[track_caller]
    pub fn board_at(&mut self, serial: Serial, ship: Serial, tile: Position) {
        self.send(Change::BoardAt { serial, ship, tile });
    }

    #[track_caller]
    pub fn unboard(&mut self, serial: Serial) {
        self.send(Change::Unboard { serial });
    }

    #[track_caller]
    pub fn tie(&mut self, serial: Serial, with: Serial) {
        self.send(Change::Tie { serial, with });
    }

    #[track_caller]
    pub fn group(&mut self, serials: &[Serial], role: Role, party: u32, with: Option<Serial>) {
        self.send(Change::Group {
            serials: serials.to_vec(),
            role,
            party,
            with,
        });
    }

    /// Whether the ship has the role and the target of the plan.
    #[track_caller]
    pub fn plan_ship(
        &mut self,
        ship: Serial,
        planned: usize,
        capacity: usize,
        to: Position,
    ) -> bool {
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        self.send(Change::PlanShip {
            ship,
            planned: count(planned),
            capacity: count(capacity),
            to,
        });
        let role = if planned >= capacity {
            Role::TransportReady
        } else {
            Role::TransportSpace
        };
        self.ai.tasks(self.game).roles.target_of(ship, role) == Some(to)
    }

    #[track_caller]
    pub fn depart(&mut self, ship: Serial, to: Position) {
        self.send(Change::Depart { ship, to });
    }

    /// The turns the unit has stood still on an order, this one counted.
    #[track_caller]
    pub fn track(&mut self, serial: Serial, at: Position, ordered: bool) -> u32 {
        self.send(Change::Track {
            serial,
            at,
            ordered,
        });
        self.ai
            .tasks(self.game)
            .roles
            .get(serial)
            .map_or(0, |a| a.stuck)
    }

    #[track_caller]
    pub fn clear_track(&mut self, serial: Serial) {
        self.send(Change::ClearTrack { serial });
    }

    #[track_caller]
    pub fn restart_plan(&mut self, serial: Serial) {
        self.send(Change::RestartPlan { serial });
    }

    /// The party of the new muster, `None` when no unit could join it.
    #[track_caller]
    pub fn begin_muster(
        &mut self,
        kind: MusterKind,
        rally: Position,
        objective: Option<Position>,
        serials: &[Serial],
    ) -> Option<u32> {
        let party = self.ai.tasks(self.game).roles.new_party();
        self.send(Change::BeginMuster {
            kind,
            rally,
            objective,
            serials: serials.to_vec(),
        });
        let begun = self
            .ai
            .tasks(self.game)
            .roles
            .musters()
            .iter()
            .any(|m| m.0 == party);
        begun.then_some(party)
    }

    /// How many of the units are in the muster now.
    #[track_caller]
    pub fn join_muster(&mut self, party: u32, serials: &[Serial]) -> usize {
        self.send(Change::JoinMuster {
            party,
            serials: serials.to_vec(),
        });
        let tasks = self.ai.tasks(self.game);
        serials
            .iter()
            .filter(|&&s| tasks.roles.is(s, Role::Muster) && tasks.roles.party_of(s) == Some(party))
            .count()
    }

    #[track_caller]
    pub fn end_muster(&mut self, party: u32) {
        self.send(Change::EndMuster { party });
    }

    #[track_caller]
    pub fn end_musters(&mut self, kind: MusterKind) {
        self.send(Change::EndMusters { kind });
    }

    #[track_caller]
    pub fn tidy_musters(&mut self) {
        self.send(Change::TidyMusters);
    }

    #[track_caller]
    pub fn set_assembly(&mut self, assembly: Assembly) {
        self.send(Change::SetAssembly(assembly));
    }

    #[track_caller]
    pub fn drop_assemblies(&mut self, targets: Vec<Position>) {
        self.send(Change::DropAssemblies { targets });
    }

    #[track_caller]
    pub fn import(&mut self, tasks: Tasks) {
        self.send(Change::Import(Box::new(tasks)));
    }
}
