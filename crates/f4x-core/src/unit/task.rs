//! Unit tasks: what a player has set each of its units to do, the parties being assembled and
//! the squares a force gathers on. The player keeps them in its army ledger, reads them there and
//! changes them with `Command::Task`, one [`Change`] at a time; the computer player chooses them
//! (`docs/ai-unit-registry.md`). A task orders no unit about by itself: the movement helper walks
//! the units when the player calls it (`unit/task_walk.rs`). The turns an entry records are the
//! round the game gives the world with the command.
//!
//! The ledger holds tasks of its living units only: writing them leaves out the others, and a
//! unit that leaves the army loses its task as it loses its order.
//!
//! Tests: in `tests/it/c9_task.rs`, and the rules of the registry in `f4x-ai`, `roles.rs`.

use crate::map::Position;
use crate::world::serial::Serial;

/// Everything a player has set its units to do.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tasks {
    /// Each unit's task, and the parties being assembled.
    #[serde(default)]
    pub roles: Registry,
    /// The gathering against each target.
    #[serde(default)]
    pub assemblies: Vec<Assembly>,
}

/// One change of a player's tasks (`Command::Task`). Each does what the method of [`Registry`]
/// of its name does.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Change {
    Claim {
        serial: Serial,
        role: Role,
        to: Option<Position>,
    },
    ClaimAll {
        serials: Vec<Serial>,
        role: Role,
    },
    Assign {
        role: Role,
        serials: Vec<Serial>,
    },
    Release {
        serial: Serial,
        role: Role,
    },
    ReleaseAny {
        serial: Serial,
    },
    ReleaseRole {
        role: Role,
    },
    ReleaseMany {
        serials: Vec<Serial>,
        role: Role,
    },
    /// [`Registry::set_base`] for each of the units: the role, and where it sends the unit.
    SetBases {
        bases: Vec<(Serial, Role, Option<Position>)>,
    },
    SetTarget {
        serial: Serial,
        role: Role,
        to: Option<Position>,
    },
    /// Record where a unit sent against something goes (`to`, the square it attacks from) and
    /// what it is sent against.
    Stage {
        serial: Serial,
        role: Role,
        to: Option<Position>,
        objective: Position,
    },
    SetCrossing {
        serial: Serial,
        crossing: Option<Crossing>,
    },
    BoardAt {
        serial: Serial,
        ship: Serial,
        tile: Position,
    },
    Unboard {
        serial: Serial,
    },
    /// Record the ship that carries the unit, or the unit it is tied to, with no boarding square.
    Tie {
        serial: Serial,
        with: Serial,
    },
    Group {
        serials: Vec<Serial>,
        role: Role,
        party: u32,
        with: Option<Serial>,
    },
    PlanShip {
        ship: Serial,
        planned: u32,
        capacity: u32,
        to: Position,
    },
    Depart {
        ship: Serial,
        to: Position,
    },
    Track {
        serial: Serial,
        at: Position,
        ordered: bool,
    },
    ClearTrack {
        serial: Serial,
    },
    RestartPlan {
        serial: Serial,
    },
    BeginMuster {
        kind: MusterKind,
        rally: Position,
        objective: Option<Position>,
        serials: Vec<Serial>,
    },
    JoinMuster {
        party: u32,
        serials: Vec<Serial>,
    },
    EndMuster {
        party: u32,
    },
    EndMusters {
        kind: MusterKind,
    },
    /// Forget the musters that have no member left.
    TidyMusters,
    /// Keep the gathering against its target, in place of the one kept for that target.
    SetAssembly(Assembly),
    /// Forget the gatherings against these targets.
    DropAssemblies {
        targets: Vec<Position>,
    },
    /// Take these tasks in place of all that is held: what an older save kept elsewhere.
    Import(Box<Tasks>),
}

impl Tasks {
    /// Make `change` in the round `round`. `lives` says which units are in the army: a task is
    /// given to no other.
    pub(crate) fn apply(&mut self, change: Change, round: u32, lives: impl Fn(Serial) -> bool) {
        let living = |serials: Vec<Serial>| -> Vec<Serial> {
            serials.into_iter().filter(|&s| lives(s)).collect()
        };
        let roles = &mut self.roles;
        match change {
            Change::Claim { serial, role, to } => {
                if lives(serial) {
                    // A refused claim leaves the unit the role it holds: the claimer looks.
                    let _ = roles.claim(serial, role, round, to);
                }
            }
            Change::ClaimAll { serials, role } => {
                roles.claim_all(living(serials), role, round);
            }
            Change::Assign { role, serials } => roles.assign(role, living(serials), round),
            Change::Release { serial, role } => {
                roles.release(serial, role, round);
            }
            Change::ReleaseAny { serial } => roles.release_any(serial, round),
            Change::ReleaseRole { role } => roles.release_role(role, round),
            Change::ReleaseMany { serials, role } => roles.release_many(&serials, role, round),
            Change::SetBases { bases } => {
                for (serial, role, to) in bases {
                    if lives(serial) {
                        roles.set_base(serial, role, to, round);
                    }
                }
            }
            Change::SetTarget { serial, role, to } => roles.set_target(serial, role, to, round),
            Change::Stage {
                serial,
                role,
                to,
                objective,
            } => roles.stage(serial, role, to, objective, round),
            Change::SetCrossing { serial, crossing } => roles.set_crossing(serial, crossing),
            Change::BoardAt { serial, ship, tile } => roles.board_at(serial, ship, tile),
            Change::Unboard { serial } => roles.unboard(serial),
            Change::Tie { serial, with } => roles.tie(serial, with),
            Change::Group {
                serials,
                role,
                party,
                with,
            } => roles.group(&serials, role, party, with),
            Change::PlanShip {
                ship,
                planned,
                capacity,
                to,
            } => {
                if lives(ship) {
                    roles.plan_ship(ship, planned as usize, capacity as usize, round, to);
                }
            }
            Change::Depart { ship, to } => roles.depart(ship, to),
            Change::Track {
                serial,
                at,
                ordered,
            } => {
                roles.track(serial, at, ordered);
            }
            Change::ClearTrack { serial } => roles.clear_track(serial),
            Change::RestartPlan { serial } => roles.restart_plan(serial, round),
            Change::BeginMuster {
                kind,
                rally,
                objective,
                serials,
            } => {
                roles.begin_muster(kind, rally, objective, &living(serials), round);
            }
            Change::JoinMuster { party, serials } => {
                roles.join_muster(party, &living(serials), round);
            }
            Change::EndMuster { party } => roles.end_muster(party, round),
            Change::EndMusters { kind } => roles.end_musters(kind, round),
            Change::TidyMusters => roles.tidy_musters(),
            Change::SetAssembly(assembly) => {
                match self
                    .assemblies
                    .iter_mut()
                    .find(|a| a.target == assembly.target)
                {
                    Some(held) => *held = assembly,
                    None => self.assemblies.push(assembly),
                }
            }
            Change::DropAssemblies { targets } => {
                self.assemblies.retain(|a| !targets.contains(&a.target));
            }
            Change::Import(tasks) => {
                *self = *tasks;
                self.roles.keep_living(lives);
            }
        }
    }
}

/// The force gathering against one target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Assembly {
    pub target: Position,
    /// The square beside the target the force attacks from.
    pub beside: Option<Position>,
    /// Where the force gathers first.
    pub rally: Option<Position>,
}

/// What a unit is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Role {
    /// Standing in one of the wizard's cities, which is its post (`to` = the city).
    Garrison,
    /// Scouting by land.
    Scout,
    /// On its way to an open tower to change plane.
    PlanarScout,
    /// Out on a raid of a tower, node or encounter site.
    Raid,
    /// Gathering for a march.
    Muster,
    /// On the march against a city.
    March,
    /// In the party a sea scout carries.
    SeaParty,
    /// A settler on its own way to a site to found a city.
    Settle,
    /// A soldier that goes with a unit it protects (`escort.rs`); `with` is that unit.
    Escort,
    /// A soldier of the surplus dealing with what is in the zone of the cities (`perimeter.rs`):
    /// `to` = the enemy square it goes for.
    Perimeter,
    /// A spirit on its way to an open node to meld with it (`meld.rs`, `to` = the node).
    Meld,
    /// Engineers building a road, or on their way to the square where they start (`road.rs`);
    /// `to` is the city or frontier the road leads to.
    Road,
    /// A ship parked at sea as a bridge for the wizard's walkers (`bridge.rs`, task 739): `to` is
    /// its square. It stays there, and no pickup or voyage takes it.
    Bridge,
    /// A ship with no mission, no passengers and no target: free to be sent.
    TransportFree,
    /// A ship with a target (`to`) and room: on its way to the port where a landing loads, or to
    /// the sea square where stranded soldiers wait (those tied to it by `with` are the ones it
    /// came for). Set by `naval.rs` and `pickup.rs`.
    TransportSpace,
    /// A ship with the units to carry decided and tied to it (`with`), and its target (`to`),
    /// about to sail: the landing party is chosen, the rescue has everyone aboard. Set by
    /// `naval.rs` and `pickup.rs`.
    TransportReady,
    /// A ship that has sailed with its load, on its way to its target (`to`): a landing to its sea
    /// square, rescued soldiers to a port. Set by `naval.rs` and `pickup.rs`.
    Transporting,
    /// Nothing found for the unit to do: a role of its own, so no unit has none. The unit stays
    /// where it stands until the dispatcher (`dispatch.rs`) has a job for it.
    AwaitingOrders,
    /// On its way to a square by an order of its own (`to` = the end of the path).
    MoveTo,
    /// A soldier on its way to an under-defended own city (`reinforce.rs`, `to` = the city).
    Reinforce,
    /// A wounded unit that mends (`recover.rs`): `to` is where, its own square when it stays; one
    /// that goes to a healer is tied to it (`with`) and follows it.
    Rest,
}

/// What carries a unit to its target when no walk leads there: the unit keeps its role and its
/// target and waits where it stands until the transport (ships, spells, towers: task 647) takes
/// it. The unit is tagged with what it wants to do at the target, not with the means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Crossing {
    /// The target is on the same plane but no known land route leads to it: a ship.
    Sea,
    /// The target is on the other plane: a tower, a gate or a spell.
    Plane,
    /// The target is across one sea square that a ship parked as a bridge will span
    /// (`bridge.rs`): the unit waits for the bridge, not for a ferry.
    Bridge,
}

/// A unit's role as the registry holds it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Assignment {
    pub role: Role,
    /// The turn the unit took the role.
    pub since: u32,
    /// The turn its current plan began: the turn it took the role or was last given a new target.
    /// The plan clock (`plan_clock.rs`) reads it.
    #[serde(default)]
    pub planned: u32,
    /// Where the role sends it (a city to return to, a shore tile to wait on), if anywhere. A
    /// raider, a marcher and a perimeter soldier are sent to the square they attack from.
    pub to: Option<Position>,
    /// What a raider, a marcher or a perimeter soldier is sent against. Saves from before the
    /// field hold it in `to` ([`Registry::objective_of`]).
    #[serde(default)]
    pub objective: Option<Position>,
    /// The group it acts with (a raid party, a march, a ferry and its passengers, an escort and
    /// its charge): the units sharing the id. `Registry::new_party` gives a fresh one.
    #[serde(default)]
    pub party: Option<u32>,
    /// The one unit it is tied to: the settler an escort guards, the ship a passenger is aboard.
    #[serde(default)]
    pub with: Option<Serial>,
    /// What it needs to get to its target, when it cannot walk there.
    #[serde(default)]
    pub crossing: Option<Crossing>,
    /// The shore tile where it boards the ship sent for it (`pickup.rs`).
    #[serde(default)]
    pub boarding: Option<Position>,
    /// Where it stood at the last audit, and for how many audits in a row it has not moved.
    pub last_pos: Option<Position>,
    pub stuck: u32,
}

impl Assignment {
    /// The role of a unit that has nothing to do.
    fn awaiting(turn: u32) -> Assignment {
        Assignment {
            role: Role::AwaitingOrders,
            since: turn,
            planned: turn,
            to: None,
            objective: None,
            party: None,
            with: None,
            crossing: None,
            boarding: None,
            last_pos: None,
            stuck: 0,
        }
    }
}

/// What a muster gathers for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MusterKind {
    /// A raid on a site, tower or neutral city (`raid.rs`, `site.rs`).
    Raid,
    /// The campaign march on a rival's city (`military.rs`).
    Campaign,
}

/// A party being assembled: the units holding `Muster` with this party id walk to `rally`; when
/// they are there (or the time is up) the party sets off for `objective`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Muster {
    pub kind: MusterKind,
    /// The square the units gather on.
    pub rally: Position,
    /// What the party is to take or attack, once gathered.
    pub objective: Option<Position>,
    /// The turn the muster began.
    pub since: u32,
}

/// The one place a unit's role lives: at most one per unit. A role is taken with [`claim`] and
/// handed back with [`release`]; a claim of a lower-priority role than the one held is refused.
///
/// [`claim`]: Registry::claim
/// [`release`]: Registry::release
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Registry {
    map: std::collections::BTreeMap<Serial, Assignment>,
    /// The parties being assembled, by party id (`Muster`).
    #[serde(default)]
    musters: std::collections::BTreeMap<u32, Muster>,
}

impl Registry {
    /// The role the unit holds.
    pub fn role_of(&self, serial: Serial) -> Option<Role> {
        self.map.get(&serial).map(|a| a.role)
    }

    /// Whether the unit holds `role`.
    pub fn is(&self, serial: Serial, role: Role) -> bool {
        self.role_of(serial) == Some(role)
    }

    /// Whether a routine has a task for the unit (it holds a role other than the base roles:
    /// garrison, moving to, awaiting orders, the free ship). A unit that does not is free to be
    /// given one.
    pub fn has_role(&self, serial: Serial) -> bool {
        self.map.get(&serial).is_some_and(|a| !a.role.is_base())
    }

    /// Whether the unit is in the dispatcher's pool: no routine has a task for it and it is not
    /// a city's garrison (task 645). Only these units are given jobs.
    pub fn is_free(&self, serial: Serial) -> bool {
        self.map
            .get(&serial)
            .is_none_or(|a| a.role.is_base() && a.role != Role::Garrison)
    }

    /// Where the unit's role sends it, if it holds `role`.
    pub fn target_of(&self, serial: Serial, role: Role) -> Option<Position> {
        self.map
            .get(&serial)
            .filter(|a| a.role == role)
            .and_then(|a| a.to)
    }

    /// The units holding `role`, in serial order.
    pub fn members(&self, role: Role) -> Vec<Serial> {
        self.map
            .iter()
            .filter(|(_, a)| a.role == role)
            .map(|(s, _)| *s)
            .collect()
    }

    /// Whether the unit is out scouting, by land, by sea or to change plane.
    pub fn is_scouting(&self, serial: Serial) -> bool {
        self.role_of(serial)
            .is_some_and(|r| matches!(r, Role::Scout | Role::PlanarScout | Role::SeaParty))
    }

    /// The units out scouting, in serial order.
    pub fn scouting(&self) -> Vec<Serial> {
        self.map
            .iter()
            .filter(|(_, a)| matches!(a.role, Role::Scout | Role::PlanarScout | Role::SeaParty))
            .map(|(s, _)| *s)
            .collect()
    }

    /// The soldier escorting `charge`, if one is.
    pub fn escort_of(&self, charge: Serial) -> Option<Serial> {
        self.map
            .iter()
            .find(|(_, a)| a.role == Role::Escort && a.with == Some(charge))
            .map(|(s, _)| *s)
    }

    /// Give the ship the role its plan says: `TransportSpace` while `planned` is under `capacity`,
    /// `TransportReady` when it is at it, with `to` as its target. A ship that holds one of the two
    /// changes over; a free ship is taken. Not a ship already `Transporting`.
    pub fn plan_ship(
        &mut self,
        ship: Serial,
        planned: usize,
        capacity: usize,
        turn: u32,
        to: Position,
    ) -> bool {
        let role = if planned >= capacity {
            Role::TransportReady
        } else {
            Role::TransportSpace
        };
        match self.role_of(ship) {
            Some(Role::TransportSpace | Role::TransportReady) => {
                if let Some(a) = self.map.get_mut(&ship) {
                    a.role = role;
                    a.to = Some(to);
                }
                true
            }
            _ => self.claim(ship, role, turn, Some(to)).is_ok(),
        }
    }

    /// The ship sails with its load for `to`: it is `Transporting`.
    pub fn depart(&mut self, ship: Serial, to: Position) {
        if let Some(a) = self
            .map
            .get_mut(&ship)
            .filter(|a| matches!(a.role, Role::TransportSpace | Role::TransportReady))
        {
            a.role = Role::Transporting;
            a.to = Some(to);
        }
    }

    /// Change a unit's role from `from` to `role` with a new target, keeping what ties it to
    /// others (a ship that has put its passengers ashore is on its way back for the next load).
    pub fn transition(&mut self, serial: Serial, from: Role, role: Role, to: Option<Position>) {
        if let Some(a) = self.map.get_mut(&serial).filter(|a| a.role == from) {
            a.role = role;
            a.to = to;
        }
    }

    /// The scout on its way to an open tower, if any.
    pub fn planar(&self) -> Option<Serial> {
        self.members(Role::PlanarScout).first().copied()
    }

    /// Forget the units `lives` says are gone, and the musters that leaves empty.
    pub(crate) fn keep_living(&mut self, lives: impl Fn(Serial) -> bool) {
        self.map.retain(|&s, _| lives(s));
        self.tidy_musters();
    }

    /// Count the turns the unit has stood still on an order: a unit that moved, or has no order
    /// (`moving` false), starts again. Returns the count.
    pub fn track(&mut self, serial: Serial, at: Position, ordered: bool) -> u32 {
        let Some(a) = self.map.get_mut(&serial) else {
            return 0;
        };
        if a.last_pos == Some(at) && ordered {
            a.stuck += 1;
        } else {
            a.stuck = 0;
        }
        a.last_pos = Some(at);
        a.stuck
    }

    /// Forget how long the unit has stood still.
    pub fn clear_track(&mut self, serial: Serial) {
        if let Some(a) = self.map.get_mut(&serial) {
            a.stuck = 0;
            a.last_pos = None;
        }
    }

    /// How many units hold `role`.
    pub fn count(&self, role: Role) -> usize {
        self.map.values().filter(|a| a.role == role).count()
    }

    /// Whether any unit holds `role`.
    pub fn any(&self, role: Role) -> bool {
        self.map.values().any(|a| a.role == role)
    }

    /// Give `role` to each of the units; the ones that hold a better role keep it. The units that
    /// got it.
    pub fn claim_all(
        &mut self,
        serials: impl IntoIterator<Item = Serial>,
        role: Role,
        turn: u32,
    ) -> Vec<Serial> {
        serials
            .into_iter()
            .filter(|&s| self.claim(s, role, turn, None).is_ok())
            .collect()
    }

    /// The units that hold `role` become exactly `serials` (the ones with a better role excepted).
    pub fn assign(&mut self, role: Role, serials: impl IntoIterator<Item = Serial>, turn: u32) {
        self.release_role(role, turn);
        self.claim_all(serials, role, turn);
    }

    /// Hand back `role` from every unit that holds it.
    pub fn release_role(&mut self, role: Role, turn: u32) {
        self.retain(turn, |_, a| a.role != role);
    }

    /// Hand back `role` from each of the units that hold it.
    pub fn release_many(&mut self, serials: &[Serial], role: Role, turn: u32) {
        self.retain(turn, |s, a| a.role != role || !serials.contains(&s));
    }

    /// Change where the unit's role sends it, if it holds `role`.
    pub fn set_target(&mut self, serial: Serial, role: Role, to: Option<Position>, turn: u32) {
        if let Some(a) = self.map.get_mut(&serial).filter(|a| a.role == role) {
            if a.to != to {
                a.planned = turn;
                a.crossing = None;
            }
            a.to = to;
        }
    }

    /// What the unit is sent against, if it holds `role` (a raid, a march, the perimeter). A
    /// save from before the objective was recorded apart holds it in `to`.
    pub fn objective_of(&self, serial: Serial, role: Role) -> Option<Position> {
        self.map
            .get(&serial)
            .filter(|a| a.role == role)
            .and_then(|a| a.objective.or(a.to))
    }

    /// What the units holding `role` are sent against.
    pub fn objectives(&self, role: Role) -> Vec<Position> {
        self.map
            .values()
            .filter(|a| a.role == role)
            .filter_map(|a| a.objective.or(a.to))
            .collect()
    }

    /// Send the unit, if it holds `role`, to `to` against `objective`. A new objective is a new
    /// plan; a new square to attack the same one from is not.
    pub fn stage(
        &mut self,
        serial: Serial,
        role: Role,
        to: Option<Position>,
        objective: Position,
        turn: u32,
    ) {
        if let Some(a) = self.map.get_mut(&serial).filter(|a| a.role == role) {
            if a.objective.or(a.to) != Some(objective) {
                a.planned = turn;
                a.crossing = None;
            } else if a.to != to {
                a.crossing = None;
            }
            a.to = to;
            a.objective = Some(objective);
        }
    }

    /// Tag the unit with what it needs to reach its target (or clear it with `None`).
    pub fn set_crossing(&mut self, serial: Serial, crossing: Option<Crossing>) {
        if let Some(a) = self.map.get_mut(&serial) {
            a.crossing = crossing;
        }
    }

    /// Tie the unit, whatever its role, to the ship that comes for it, with the shore tile where
    /// it boards.
    pub fn board_at(&mut self, serial: Serial, ship: Serial, tile: Position) {
        if let Some(a) = self.map.get_mut(&serial) {
            a.with = Some(ship);
            a.boarding = Some(tile);
        }
    }

    /// Record the unit this one is tied to (the ship that carries it), whatever its role.
    pub fn tie(&mut self, serial: Serial, with: Serial) {
        if let Some(a) = self.map.get_mut(&serial) {
            a.with = Some(with);
        }
    }

    /// The ship no longer comes for the unit.
    pub fn unboard(&mut self, serial: Serial) {
        if let Some(a) = self.map.get_mut(&serial) {
            a.with = None;
            a.boarding = None;
        }
    }

    /// The shore tile where the unit boards its ship, if one is coming.
    pub fn boarding_of(&self, serial: Serial) -> Option<Position> {
        self.map.get(&serial).and_then(|a| a.boarding)
    }

    /// The units tied to the ship (`with`), in serial order.
    pub fn tied_with(&self, ship: Serial) -> Vec<Serial> {
        self.map
            .iter()
            .filter(|(_, a)| a.with == Some(ship))
            .map(|(s, _)| *s)
            .collect()
    }

    /// The units that wait for a ship with none coming yet, with where each wants to go.
    pub fn waiting_for_ship(&self) -> Vec<(Serial, Position)> {
        self.needing(Crossing::Sea)
            .into_iter()
            .filter(|&(s, _)| self.map.get(&s).is_some_and(|a| a.boarding.is_none()))
            .collect()
    }

    /// Where the unit's role sends it, whatever the role.
    pub fn destination_of(&self, serial: Serial) -> Option<Position> {
        self.map.get(&serial).and_then(|a| a.to)
    }

    /// What the unit needs to reach its target, if it cannot walk there.
    pub fn crossing_of(&self, serial: Serial) -> Option<Crossing> {
        self.map.get(&serial).and_then(|a| a.crossing)
    }

    /// The units that need `crossing`, with where they want to go, in serial order.
    pub fn needing(&self, crossing: Crossing) -> Vec<(Serial, Position)> {
        let mut found: Vec<_> = self
            .map
            .iter()
            .filter(|(_, a)| a.crossing == Some(crossing))
            .filter_map(|(s, a)| a.to.map(|p| (*s, p)))
            .collect();
        found.sort_by_key(|&(s, _)| s);
        found
    }

    /// The plan of the unit begins again now (it did something that counts, or its plan was asked
    /// for again).
    pub fn restart_plan(&mut self, serial: Serial, turn: u32) {
        if let Some(a) = self.map.get_mut(&serial) {
            a.planned = turn;
        }
    }

    /// The units holding `role` with the turn each one's plan began.
    pub fn plans(&self, role: Role) -> Vec<(Serial, u32, Option<Position>)> {
        self.map
            .iter()
            .filter(|(_, a)| a.role == role)
            .map(|(s, a)| (*s, a.planned, a.to))
            .collect()
    }

    /// The units holding `role` with the place each is sent to.
    pub fn targeted(&self, role: Role) -> Vec<(Serial, Position)> {
        self.map
            .iter()
            .filter(|(_, a)| a.role == role)
            .filter_map(|(s, a)| a.to.map(|p| (*s, p)))
            .collect()
    }

    /// A group id nobody holds yet.
    pub fn new_party(&self) -> u32 {
        self.map
            .values()
            .filter_map(|a| a.party)
            .chain(self.musters.keys().copied())
            .max()
            .map_or(1, |m| m + 1)
    }

    /// Begin a muster: the units (those that can take `Muster`) walk to `rally` as one party.
    /// Returns its party id, or `None` when no unit could take the role.
    pub fn begin_muster(
        &mut self,
        kind: MusterKind,
        rally: Position,
        objective: Option<Position>,
        serials: &[Serial],
        turn: u32,
    ) -> Option<u32> {
        let party = self.new_party();
        let taken = self.join_muster_party(party, rally, serials, turn);
        if taken == 0 {
            return None;
        }
        self.musters.insert(
            party,
            Muster {
                kind,
                rally,
                objective,
                since: turn,
            },
        );
        Some(party)
    }

    /// More units for a muster that stands. How many took the role.
    pub fn join_muster(&mut self, party: u32, serials: &[Serial], turn: u32) -> usize {
        let Some(rally) = self.musters.get(&party).map(|m| m.rally) else {
            return 0;
        };
        self.join_muster_party(party, rally, serials, turn)
    }

    fn join_muster_party(
        &mut self,
        party: u32,
        rally: Position,
        serials: &[Serial],
        turn: u32,
    ) -> usize {
        let mut taken = 0;
        for &s in serials {
            if self.claim(s, Role::Muster, turn, Some(rally)).is_ok() {
                if let Some(a) = self.map.get_mut(&s) {
                    a.party = Some(party);
                }
                taken += 1;
            }
        }
        taken
    }

    /// The musters that stand, with their members, in party order.
    pub fn musters(&self) -> Vec<(u32, Muster, Vec<Serial>)> {
        self.musters
            .iter()
            .map(|(&id, &m)| {
                let members = self
                    .map
                    .iter()
                    .filter(|(_, a)| a.role == Role::Muster && a.party == Some(id))
                    .map(|(s, _)| *s)
                    .collect();
                (id, m, members)
            })
            .collect()
    }

    /// Whether a muster of `kind` stands.
    pub fn any_muster(&self, kind: MusterKind) -> bool {
        self.musters.values().any(|m| m.kind == kind)
    }

    /// The muster the unit belongs to.
    pub fn muster_of(&self, serial: Serial) -> Option<(u32, Muster)> {
        let a = self.map.get(&serial).filter(|a| a.role == Role::Muster)?;
        let id = a.party?;
        self.musters.get(&id).map(|&m| (id, m))
    }

    /// End a muster: its members await orders and the record goes.
    pub fn end_muster(&mut self, party: u32, turn: u32) {
        self.musters.remove(&party);
        self.retain(turn, |_, a| {
            !(a.role == Role::Muster && a.party == Some(party))
        });
    }

    /// End every muster of `kind`.
    pub fn end_musters(&mut self, kind: MusterKind, turn: u32) {
        let ids: Vec<u32> = self
            .musters
            .iter()
            .filter(|(_, m)| m.kind == kind)
            .map(|(&id, _)| id)
            .collect();
        for id in ids {
            self.end_muster(id, turn);
        }
    }

    /// Whether the unit belongs to a muster of `kind`.
    pub fn in_muster(&self, serial: Serial, kind: MusterKind) -> bool {
        self.muster_of(serial).is_some_and(|(_, m)| m.kind == kind)
    }

    /// Forget the musters that have no member left.
    pub fn tidy_musters(&mut self) {
        let live: Vec<u32> = self
            .map
            .values()
            .filter(|a| a.role == Role::Muster)
            .filter_map(|a| a.party)
            .collect();
        self.musters.retain(|id, _| live.contains(id));
    }

    /// Put the units that hold `role` into the party, and tie each to `with` if given.
    pub fn group(&mut self, serials: &[Serial], role: Role, party: u32, with: Option<Serial>) {
        for s in serials {
            if let Some(a) = self.map.get_mut(s).filter(|a| a.role == role) {
                a.party = Some(party);
                a.with = with;
            }
        }
    }

    /// The units of a party, in serial order.
    pub fn members_of(&self, party: u32) -> Vec<Serial> {
        self.map
            .iter()
            .filter(|(_, a)| a.party == Some(party))
            .map(|(s, _)| *s)
            .collect()
    }

    /// The party the unit is in.
    pub fn party_of(&self, serial: Serial) -> Option<u32> {
        self.map.get(&serial).and_then(|a| a.party)
    }

    /// The unit this one is tied to.
    pub fn with_of(&self, serial: Serial) -> Option<Serial> {
        self.map.get(&serial).and_then(|a| a.with)
    }

    /// The places the units holding `role` are sent to.
    pub fn targets(&self, role: Role) -> Vec<Position> {
        self.map
            .values()
            .filter(|a| a.role == role)
            .filter_map(|a| a.to)
            .collect()
    }

    /// Every unit that holds a role of the registry.
    pub fn assigned(&self) -> Vec<Serial> {
        self.map
            .iter()
            .filter(|(_, a)| !a.role.is_base())
            .map(|(s, _)| *s)
            .collect()
    }

    /// Whether [`Registry::set_base`] with these would change what is kept of the unit.
    pub fn base_differs(&self, serial: Serial, role: Role, to: Option<Position>) -> bool {
        self.map
            .get(&serial)
            .is_none_or(|a| a.role.is_base() && (a.role != role || a.to != to))
    }

    /// Give the unit a base role from where it stands (`to` = its post, the city it garrisons, the
    /// end of its path), unless a routine has a task for it. The audit does this for every unit.
    pub fn set_base(&mut self, serial: Serial, role: Role, to: Option<Position>, turn: u32) {
        if !self.base_differs(serial, role, to) {
            return;
        }
        match self.map.get_mut(&serial) {
            Some(a) if a.role == role => a.to = to,
            _ => {
                let mut a = Assignment::awaiting(turn);
                a.role = role;
                a.to = to;
                self.map.insert(serial, a);
            }
        }
    }

    /// The assignment of a unit.
    pub fn get(&self, serial: Serial) -> Option<&Assignment> {
        self.map.get(&serial)
    }

    /// Give the unit `role`. A unit that holds another role of equal or higher priority keeps it
    /// and the claim is refused (the error is the role it holds); the same role is renewed.
    pub fn claim(
        &mut self,
        serial: Serial,
        role: Role,
        turn: u32,
        to: Option<Position>,
    ) -> Result<(), Role> {
        match self.map.get_mut(&serial) {
            // The same role again renews it; a target given is kept when none is.
            Some(held) if held.role == role => {
                if to.is_some() && held.to != to {
                    held.to = to;
                    held.planned = turn;
                    held.crossing = None;
                }
                Ok(())
            }
            Some(held) if held.role.priority() <= role.priority() => Err(held.role),
            _ => {
                self.map.insert(
                    serial,
                    Assignment {
                        role,
                        since: turn,
                        planned: turn,
                        to,
                        objective: None,
                        party: None,
                        with: None,
                        crossing: None,
                        boarding: None,
                        last_pos: None,
                        stuck: 0,
                    },
                );
                Ok(())
            }
        }
    }

    /// Hand back the role, if the unit holds it. Whether it did.
    pub fn release(&mut self, serial: Serial, role: Role, turn: u32) -> bool {
        if self.is(serial, role) {
            self.map.insert(serial, Assignment::awaiting(turn));
            true
        } else {
            false
        }
    }

    /// Hand back whatever role the unit holds.
    pub fn release_any(&mut self, serial: Serial, turn: u32) {
        if self.map.contains_key(&serial) {
            self.map.insert(serial, Assignment::awaiting(turn));
        }
    }

    /// Keep only the units `keep` says are still around and doing it.
    pub fn retain(&mut self, turn: u32, mut keep: impl FnMut(Serial, &Assignment) -> bool) {
        for (s, a) in self.map.iter_mut() {
            if !keep(*s, a) {
                *a = Assignment::awaiting(turn);
            }
        }
    }
}

impl Role {
    /// Which role wins when a unit is found in two lists: the lower number. A settler's task and
    /// a ship's voyage come first, the fall-backs last.
    /// A role the audit gives a unit from where it stands and what it is doing, when no routine
    /// has a task for it: the routines take these over with a claim and hand a unit back to them.
    pub fn is_base(self) -> bool {
        matches!(
            self,
            Role::MoveTo | Role::Garrison | Role::TransportFree | Role::AwaitingOrders
        )
    }

    fn priority(self) -> u8 {
        match self {
            // Nobody takes a wounded unit from its rest.
            Role::Rest => 0,
            Role::Settle => 1,
            Role::Bridge => 2,
            Role::TransportReady => 3,
            Role::Transporting => 3,
            Role::SeaParty => 4,
            Role::PlanarScout => 5,
            Role::TransportSpace => 6,
            Role::Perimeter => 8,
            Role::Reinforce => 8,
            Role::Raid => 9,
            Role::March => 10,
            Role::Muster => 11,
            Role::Escort => 12,
            Role::Meld => 13,
            Role::Road => 14,
            Role::Scout => 15,
            Role::MoveTo => 16,
            Role::Garrison => 17,
            Role::TransportFree => 18,
            Role::AwaitingOrders => 19,
        }
    }
}
