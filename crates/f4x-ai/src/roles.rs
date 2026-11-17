//! What each unit is for. Every unit has one role at a time; the audit at the end of the turn goes
//! through all of them, finds the soldiers with none (adrift: no list, no order, no city) and
//! sends them home, counts the ships and other units with none, and the ones with two (a unit in
//! two role lists), so nothing is left standing on a tower or a road with no purpose.
//!
//! The roles live in the lists of [`Ai`] (`scouts`, `raiders`, `marching`, ...), and [`derive`]
//! reads them back as one table. The design is in `docs/ai-design.md`.

use crate::view::{is_fighter, is_ship};
use crate::Ai;
use f4x_core::game::serial::Serial;
use f4x_core::game::{Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

/// What a unit is doing, as far as the AI's lists say.
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
    /// Where the role sends it (a city to return to, a shore tile to wait on), if anywhere.
    pub to: Option<Position>,
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
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Registry {
    map: std::collections::BTreeMap<f4x_core::game::serial::Serial, Assignment>,
    /// The latest turn the registry was told of (by a claim or a sync).
    #[serde(default)]
    now: u32,
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

    /// Drop the units that are not in `alive` (dead, or gone) from the registry.
    pub fn prune(&mut self, alive: &[Serial]) {
        self.map.retain(|s, _| alive.contains(s));
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
        self.release_role(role);
        self.claim_all(serials, role, turn);
    }

    /// Hand back `role` from every unit that holds it.
    pub fn release_role(&mut self, role: Role) {
        self.retain(|_, a| a.role != role);
    }

    /// Hand back `role` from each of the units that hold it.
    pub fn release_many(&mut self, serials: &[Serial], role: Role) {
        self.retain(|s, a| a.role != role || !serials.contains(&s));
    }

    /// Keep `role` only on the units `keep` says so.
    pub fn retain_role(&mut self, role: Role, mut keep: impl FnMut(Serial) -> bool) {
        self.retain(|s, a| a.role != role || keep(s));
    }

    /// Change where the unit's role sends it, if it holds `role`.
    pub fn set_target(&mut self, serial: Serial, role: Role, to: Option<Position>) {
        let now = self.now;
        if let Some(a) = self.map.get_mut(&serial).filter(|a| a.role == role) {
            if a.to != to {
                a.planned = now;
                a.crossing = None;
            }
            a.to = to;
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

    /// Tell the registry the turn (it dates the roles handed back and the plans re-targeted).
    pub fn tick(&mut self, turn: u32) {
        self.now = self.now.max(turn);
    }

    /// The plan of the unit begins again now (it did something that counts).
    pub fn restart_plan(&mut self, serial: Serial) {
        let now = self.now;
        if let Some(a) = self.map.get_mut(&serial) {
            a.planned = now;
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
    pub fn end_muster(&mut self, party: u32) {
        self.musters.remove(&party);
        self.retain(|_, a| !(a.role == Role::Muster && a.party == Some(party)));
    }

    /// End every muster of `kind`.
    pub fn end_musters(&mut self, kind: MusterKind) {
        let ids: Vec<u32> = self
            .musters
            .iter()
            .filter(|(_, m)| m.kind == kind)
            .map(|(&id, _)| id)
            .collect();
        for id in ids {
            self.end_muster(id);
        }
    }

    /// Whether the unit belongs to a muster of `kind`.
    pub fn in_muster(&self, serial: Serial, kind: MusterKind) -> bool {
        self.muster_of(serial).is_some_and(|(_, m)| m.kind == kind)
    }

    /// Forget the musters that have no member left.
    fn tidy_musters(&mut self) {
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

    /// Give the unit a base role from where it stands (`to` = its post, the city it garrisons, the
    /// end of its path), unless a routine has a task for it. The audit does this for every unit.
    pub fn set_base(&mut self, serial: Serial, role: Role, to: Option<Position>, turn: u32) {
        if self.has_role(serial) {
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
        self.now = self.now.max(turn);
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
    pub fn release(&mut self, serial: Serial, role: Role) -> bool {
        if self.is(serial, role) {
            self.map.insert(serial, Assignment::awaiting(self.now));
            true
        } else {
            false
        }
    }

    /// Hand back whatever role the unit holds.
    pub fn release_any(&mut self, serial: Serial) {
        if self.map.contains_key(&serial) {
            self.map.insert(serial, Assignment::awaiting(self.now));
        }
    }

    /// Keep only the units `keep` says are still around and doing it.
    pub fn retain(&mut self, mut keep: impl FnMut(Serial, &Assignment) -> bool) {
        let now = self.now;
        for (s, a) in self.map.iter_mut() {
            if !keep(*s, a) {
                *a = Assignment::awaiting(now);
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
            Role::Settle => 1,
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

/// What kind of unit it is, for the roles it can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// A unit that fights (melee or ranged strength, not a ship).
    Soldier,
    /// A ship.
    Ship,
    /// Settlers, engineers, spirits and the rest: no role of theirs is built yet but settling.
    Other,
}

/// A unit and its role.
#[derive(Debug, Clone)]
pub struct Held {
    pub unit: hecs::Entity,
    pub class: Class,
    pub role: Role,
}

/// Give every unit the wizard owns exactly one role in the registry, and return the table. A
/// unit a routine has a task for keeps its role. Every other unit gets the base role its place and orders say, written
/// into the registry with its target:
///
/// - a soldier in an own city that its defence needs (`garrison::post`): `Garrison`, `to` = the
///   city; the soldiers beyond that await orders;
/// - a unit with an order of its own: `MoveTo`, `to` = the end of its path;
/// - a ship with no mission and no order: `TransportFree` (with a mission it holds
///   `TransportSpace`, `TransportReady` or `Transporting`, which `naval.rs` and `pickup.rs` set);
/// - anything else: `AwaitingOrders`.
pub fn sync(ai: &mut Ai, view: &PlayerView, turn: u32) -> Vec<Held> {
    ai.roles.now = ai.roles.now.max(turn);
    let mut out = Vec::new();
    // The soldiers each city keeps as its garrison; the others standing in it await orders.
    let mut posts: std::collections::HashSet<hecs::Entity> = std::collections::HashSet::new();
    {
        let mut by_city: std::collections::BTreeMap<(u8, i32, i32), (Position, Vec<hecs::Entity>)> =
            std::collections::BTreeMap::new();
        for unit in view.my_units() {
            if is_ship(view, unit) || !is_fighter(view, unit) {
                continue;
            }
            if let Some(at) = view
                .unit_position(unit)
                .filter(|&p| view.city_at(p).is_some())
            {
                by_city
                    .entry((at.plane as u8, at.y as i32, at.x as i32))
                    .or_insert_with(|| (at, Vec::new()))
                    .1
                    .push(unit);
            }
        }
        for (at, soldiers) in by_city.into_values() {
            posts.extend(crate::garrison::post(view, at, &soldiers));
        }
    }
    for unit in view.my_units() {
        let class = if is_ship(view, unit) {
            Class::Ship
        } else if is_fighter(view, unit) {
            Class::Soldier
        } else {
            Class::Other
        };
        let serial = view.serial_of(unit);
        let ordered = view.unit_has_order(unit);
        let at = view.unit_position(unit);
        let city = at.filter(|&p| view.city_at(p).is_some());
        let target = view.unit_destination(unit);
        let (role, to) = match class {
            Class::Soldier if city.is_some() && posts.contains(&unit) => (Role::Garrison, city),
            // A ship with no mission is free; its transport roles (space, ready) are the ones
            // `naval.rs` and `pickup.rs` give it with a target.
            Class::Ship if !ordered => (Role::TransportFree, None),
            _ if ordered => (Role::MoveTo, target),
            _ => (Role::AwaitingOrders, None),
        };
        ai.roles.set_base(serial, role, to, turn);
        let role = ai.roles.role_of(serial).unwrap_or(Role::AwaitingOrders);
        out.push(Held { unit, class, role });
    }
    out
}

/// What the table shows: the soldiers awaiting orders (adrift), and the ships and other units
/// awaiting orders (no role of theirs is built yet).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub adrift: Vec<hecs::Entity>,
    pub unassigned: u32,
}

/// Count the table.
pub fn tally(held: &[Held]) -> Tally {
    let mut t = Tally::default();
    for h in held.iter().filter(|h| h.role == Role::AwaitingOrders) {
        match h.class {
            Class::Soldier => t.adrift.push(h.unit),
            _ => t.unassigned += 1,
        }
    }
    t
}

/// The start of the turn: units that appeared since the last audit (built, summoned, hired, come
/// off a ship) get their role at once, and those gone leave the registry.
pub fn register(ai: &mut Ai, game: &Game) {
    let view = PlayerView::new(game, ai.player);
    let alive: Vec<_> = view.my_units().iter().map(|&u| view.serial_of(u)).collect();
    ai.roles.prune(&alive);
    sync(ai, &view, game.turn);
}

/// The end-of-turn check: every unit the wizard owns is looked at, each turn, and has a role. A
/// unit with nothing to do awaits orders and stays where it stands: the soldiers among them are
/// counted (adrift), as are the ships and the other units (unassigned). The audit is counted in an
/// [`Event::RolesAudited`].
pub fn audit(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let counted = {
        let view = PlayerView::new(game, me);
        // The units that are gone leave the registry; the others each get their role.
        let alive: Vec<_> = view.my_units().iter().map(|&u| view.serial_of(u)).collect();
        ai.roles.prune(&alive);
        // Units with no entry yet came into being since the start of the turn.
        let unregistered = alive.iter().filter(|&&s| ai.roles.get(s).is_none()).count() as u32;
        let held = sync(ai, &view, game.turn);
        (tally(&held), unregistered)
    };
    let (counted, unregistered) = counted;
    let reset = std::mem::take(&mut ai.missions_reset);
    let waiting = ai.roles.waiting_for_ship().len() as u32;
    if counted.adrift.is_empty()
        && counted.unassigned == 0
        && unregistered == 0
        && reset == 0
        && waiting == 0
    {
        return;
    }
    events.push(Event::RolesAudited {
        player: me,
        adrift: counted.adrift.len() as u32,
        unassigned: counted.unassigned,
        unregistered,
        reset,
        waiting,
    });
    // Nothing more: a unit awaiting orders stays where it stands (task 645). The dispatcher
    // gives the waiting units their jobs.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, spawn_tower, COAST_CITY, P0};
    use f4x_core::game::Command;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A soldier standing outside a city with no list, no order and no city under it is adrift:
    /// the audit counts it and leaves it where it stands.
    #[test]
    fn a_lone_soldier_outside_a_city_is_counted_and_left_where_it_stands() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let tower = at(8, 6);
        spawn_tower(&mut g, tower, false)?;
        g.reveal_map(P0);
        let lone = spawn(&mut g, P0, tower, 104 /* spearmen */, 1);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        audit(&mut ai, &mut g, &mut events);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::RolesAudited { adrift: 1, .. })),
            "{events:?}"
        );
        let view = PlayerView::new(&g, P0);
        assert!(lone.iter().all(|&u| !view.unit_has_order(u)));
        assert!(events.iter().all(|e| !matches!(e, Event::Rejected(_))));
        let held = sync(&mut ai, &view, 1);
        let roles = held.iter().find(|h| h.unit == lone[0]).map(|h| h.role);
        assert_eq!(roles, Some(Role::AwaitingOrders));
        Ok(())
    }

    /// A unit in a role list, or in a city, or with an order, is not adrift.
    #[test]
    fn a_unit_with_a_list_a_city_or_an_order_is_not_adrift() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let home = spawn(&mut g, P0, at(COAST_CITY.0, COAST_CITY.1), 104, 1);
        let scout = spawn(&mut g, P0, at(8, 6), 104, 1);
        let mut ai = Ai::new(P0);
        let view = PlayerView::new(&g, P0);
        let _ = ai
            .roles
            .claim(view.serial_of(scout[0]), Role::Scout, 1, None);
        let held = sync(&mut ai, &view, 1);
        let counted = tally(&held);
        assert!(counted.adrift.is_empty(), "{held:?}");
        let roles_of = |u: hecs::Entity| held.iter().find(|h| h.unit == u).map(|h| h.role);
        assert_eq!(roles_of(home[0]), Some(Role::Garrison));
        assert_eq!(roles_of(scout[0]), Some(Role::Scout));
        Ok(())
    }

    /// Soldiers with no own city they can walk to stay where they stand and await orders: the
    /// audit sends them nowhere (the transport job of task 647 takes them to a ship).
    #[test]
    fn soldiers_with_no_city_in_reach_await_orders_where_they_stand() -> TestResult {
        use crate::testworld::ENEMY_CITY;
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let far = {
            let view = PlayerView::new(&g, P0);
            crate::naval::landing(&view, at(ENEMY_CITY.0, ENEMY_CITY.1))
                .ok_or("a shore tile near the far city")?
                .0
        };
        let stranded = spawn(&mut g, P0, far, 104 /* spearmen */, 2);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        audit(&mut ai, &mut g, &mut events);
        assert!(ai.roles.members(Role::Reinforce).is_empty());
        let view = PlayerView::new(&g, P0);
        let held = sync(&mut ai, &view, 1);
        for &u in &stranded {
            let roles = held.iter().find(|h| h.unit == u).map(|h| h.role);
            assert_eq!(roles, Some(Role::AwaitingOrders));
            assert!(!view.unit_has_order(u));
        }
        Ok(())
    }

    /// Every unit is looked at, not only soldiers: a ship in port is docked, a settler has its
    /// role, and a unit of another kind with nothing to do is counted as unassigned.
    #[test]
    fn ships_settlers_and_other_units_are_in_the_table_too() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let port = at(COAST_CITY.0, COAST_CITY.1);
        let ship = spawn(&mut g, P0, port, 36 /* galley */, 1)[0];
        let other = spawn(&mut g, P0, at(8, 6), 111 /* high_men_settlers */, 1)[0];
        let mut ai = Ai::new(P0);
        let view = PlayerView::new(&g, P0);
        let held = sync(&mut ai, &view, 1);
        let of = |u: hecs::Entity| held.iter().find(|h| h.unit == u).cloned();
        let ship_held = of(ship).ok_or("the ship is in the table")?;
        assert_eq!(
            (ship_held.class, ship_held.role),
            (Class::Ship, Role::TransportFree)
        );
        let other_held = of(other).ok_or("the settlers are in the table")?;
        assert_eq!(other_held.class, Class::Other);
        assert_eq!(other_held.role, Role::AwaitingOrders);
        assert_eq!(tally(&held).unassigned, 1);
        Ok(())
    }

    fn serial(n: u64) -> f4x_core::game::serial::Serial {
        f4x_core::game::serial::Serial(n)
    }

    /// The registry holds one role per unit: a lower-priority claim on a held role is refused, a
    /// better one takes over, and a release only gives back the role actually held.
    #[test]
    fn a_muster_holds_its_members_to_one_rally_and_ends_as_one() {
        let mut r = Registry::default();
        let (a, b, c) = (serial(1), serial(2), serial(3));
        let rally = Position {
            plane: f4x_core::map::Plane::Arcanus,
            x: 4,
            y: 5,
        };
        // A raider keeps its better role: only two join.
        assert_eq!(r.claim(c, Role::Raid, 1, None), Ok(()));
        let party = r
            .begin_muster(MusterKind::Raid, rally, Some(rally), &[a, b, c], 7)
            .expect("two units took the role");
        assert_eq!(r.members_of(party), vec![a, b]);
        assert_eq!(r.target_of(a, Role::Muster), Some(rally));
        assert!(r.any_muster(MusterKind::Raid) && !r.any_muster(MusterKind::Campaign));
        assert_eq!(
            r.muster_of(b).map(|(id, m)| (id, m.since)),
            Some((party, 7))
        );
        assert_eq!(r.new_party(), party + 1);
        r.end_muster(party);
        assert_eq!(r.role_of(a), Some(Role::AwaitingOrders));
        assert!(r.musters().is_empty());
        assert_eq!(r.role_of(c), Some(Role::Raid));
    }

    #[test]
    fn the_registry_holds_one_role_per_unit() {
        let mut r = Registry::default();
        let (a, b) = (serial(1), serial(2));
        assert_eq!(r.claim(a, Role::Raid, 3, None), Ok(()));
        assert_eq!(
            r.claim(a, Role::Road, 4, None),
            Err(Role::Raid),
            "a raider is not sent to build a road by a lower claim"
        );
        assert_eq!(r.role_of(a), Some(Role::Raid));
        assert_eq!(r.claim(b, Role::Road, 4, None), Ok(()));
        assert_eq!(
            r.claim(b, Role::Reinforce, 5, None),
            Ok(()),
            "a better role takes over"
        );
        assert_eq!(r.role_of(b), Some(Role::Reinforce));
        assert!(!r.release(b, Role::Road), "it does not hold that role");
        assert!(r.release(b, Role::Reinforce));
        assert!(!r.has_role(b));
        assert_eq!(r.members(Role::Raid), vec![a]);
        assert_eq!(r.assigned(), vec![a]);
    }

    /// The claim keeps a unit to one role: a better role refuses a worse claim, a worse one is
    /// taken over.
    #[test]
    fn a_claim_keeps_a_unit_to_one_role() {
        let mut ai = Ai::new(P0);
        let (raider, scout) = (serial(1), serial(2));
        let _ = ai.roles.claim(raider, Role::Raid, 1, None);
        let _ = ai.roles.claim(scout, Role::Scout, 1, None);
        assert_eq!(
            ai.roles.claim(raider, Role::Road, 5, None),
            Err(Role::Raid),
            "a raider is not sent to build a road by the lower claim"
        );
        assert_eq!(ai.roles.claim(scout, Role::Reinforce, 5, None), Ok(()));
        assert_eq!(ai.roles.role_of(scout), Some(Role::Reinforce));
        assert_eq!(ai.roles.role_of(raider), Some(Role::Raid));
    }

    /// Units are grouped in the registry: a party lists its members, and an escort knows the
    /// unit it guards.
    #[test]
    fn units_are_grouped_by_party_and_an_escort_knows_its_charge() {
        let mut r = Registry::default();
        let (settler, guard, other) = (serial(1), serial(2), serial(3));
        for s in [settler, guard, other] {
            let _ = r.claim(s, Role::Settle, 1, None);
        }
        let party = r.new_party();
        r.group(&[settler, guard], Role::Settle, party, None);
        r.group(&[guard], Role::Settle, party, Some(settler));
        assert_eq!(r.members_of(party), vec![settler, guard]);
        assert_eq!(r.with_of(guard), Some(settler));
        assert_eq!(r.party_of(other), None);
        assert_ne!(r.new_party(), party);
        // Released, the unit leaves the party.
        r.release(guard, Role::Settle);
        assert_eq!(r.members_of(party), vec![settler]);
    }

    /// No unit has an empty role: whatever has nothing to do awaits orders.
    #[test]
    fn no_unit_is_left_without_a_role() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        spawn(&mut g, P0, at(COAST_CITY.0, COAST_CITY.1), 104, 2);
        spawn(&mut g, P0, at(8, 6), 104, 1);
        spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            36, /* galley */
            1,
        );
        spawn(&mut g, P0, at(9, 6), 111 /* high_men_settlers */, 1);
        let mut ai = Ai::new(P0);
        let view = PlayerView::new(&g, P0);
        let held = sync(&mut ai, &view, 1);
        assert_eq!(
            held.len(),
            view.my_units().len(),
            "every unit is in the table"
        );
        Ok(())
    }

    /// Every unit has its role written in the registry, with its target: a garrison soldier the
    /// city, a unit with an order its destination, a ship its state.
    #[test]
    fn every_unit_has_an_explicit_role_with_its_target() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let home = spawn(&mut g, P0, city, 104, 1)[0];
        let walker = spawn(&mut g, P0, city, 104, 1)[0];
        let ship = spawn(&mut g, P0, city, 36 /* galley */, 1)[0];
        let idle = spawn(&mut g, P0, at(8, 6), 111 /* high_men_settlers */, 1)[0];
        let mut events = Vec::new();
        let mut ai = Ai::new(P0);
        ai.send(
            &mut g,
            Command::GoTo {
                units: vec![walker],
                to: at(8, 6),
            },
            &mut events,
        )
        .map_err(|e| format!("{e:?}"))?;
        let view = PlayerView::new(&g, P0);
        sync(&mut ai, &view, 1);
        let get = |u: hecs::Entity| ai.roles.get(view.serial_of(u)).cloned();
        let of = |u: hecs::Entity| get(u).map(|a| (a.role, a.to));
        assert_eq!(of(home), Some((Role::Garrison, Some(city))));
        assert_eq!(of(walker).map(|r| r.0), Some(Role::MoveTo));
        assert!(of(walker).and_then(|r| r.1).is_some());
        assert_eq!(of(ship), Some((Role::TransportFree, None)));
        assert_eq!(of(idle), Some((Role::AwaitingOrders, None)));
        for u in view.my_units() {
            assert!(ai.roles.has_role(view.serial_of(u)) || get(u).is_some());
        }
        // A task released goes back to a role at once, not to none.
        let s = view.serial_of(home);
        let _ = ai.roles.claim(s, Role::Raid, 1, None);
        ai.roles.release(s, Role::Raid);
        assert_eq!(ai.roles.role_of(s), Some(Role::AwaitingOrders));
        Ok(())
    }

    /// A task takes a free ship: space while fewer units are planned than it holds, ready when
    /// they fill it, transporting once it sails.
    #[test]
    fn a_ship_taken_by_a_task_is_space_until_its_load_is_full_then_ready() {
        let mut r = Registry::default();
        let ship = serial(1);
        let to = at(5, 5);
        assert!(r.plan_ship(ship, 1, 2, 1, to));
        assert_eq!(r.role_of(ship), Some(Role::TransportSpace));
        assert_eq!(r.target_of(ship, Role::TransportSpace), Some(to));
        assert!(r.plan_ship(ship, 2, 2, 2, to));
        assert_eq!(r.role_of(ship), Some(Role::TransportReady));
        // A smaller plan gives the ship room again.
        assert!(r.plan_ship(ship, 1, 2, 3, to));
        assert_eq!(r.role_of(ship), Some(Role::TransportSpace));
        r.depart(ship, to);
        assert_eq!(r.role_of(ship), Some(Role::Transporting));
    }

    /// Units tagged for a ship are counted by the audit, so a soak can tell how long they wait.
    #[test]
    fn units_waiting_for_a_ship_are_counted_by_the_audit() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let home = at(COAST_CITY.0, COAST_CITY.1);
        let soldiers = spawn(&mut g, P0, home, 104 /* spearmen */, 2);
        let mut ai = Ai::new(P0);
        register(&mut ai, &g);
        let view = PlayerView::new(&g, P0);
        for &u in &soldiers {
            let s = view.serial_of(u);
            let _ = ai.roles.claim(s, Role::March, 1, Some(at(30, 6)));
            ai.roles.set_crossing(s, Some(Crossing::Sea));
        }
        let mut events = Vec::new();
        audit(&mut ai, &mut g, &mut events);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::RolesAudited { waiting: 2, .. })),
            "{events:?}"
        );
        Ok(())
    }
}
