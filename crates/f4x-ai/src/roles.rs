//! What each unit is for. Every unit has one role at a time; the audit at the end of the turn goes
//! through all of them, finds the soldiers with none (adrift: no list, no order, no city) and
//! sends them home, counts the ships and other units with none, and the ones with two (a unit in
//! two role lists), so nothing is left standing on a tower or a road with no purpose.
//!
//! The roles live in the lists of [`Ai`] (`scouts`, `raiders`, `marching`, ...), and [`derive`]
//! reads them back as one table. The design is in `docs/ai-design.md`.

use crate::view::{distance, is_fighter, is_ship};
use crate::Ai;
use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
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
    /// Aboard a ship on a voyage with a landing.
    Voyage,
    /// A settler on its own way to a site to found a city.
    Settle,
    /// The ship and the settler of a colony by sea (`colony.rs`): the settler is tied to the ship.
    Colonize,
    /// A soldier that goes with a unit it protects (`escort.rs`); `with` is that unit.
    Escort,
    /// A soldier of the surplus dealing with what is in the zone of the cities (`perimeter.rs`):
    /// `to` = the enemy square it goes for, or the city it joins when the enemy is the stronger.
    Perimeter,
    /// A spirit on its way to an open node to meld with it (`meld.rs`, `to` = the node).
    Meld,
    /// Engineers building a road, or on their way to the square where they start (`road.rs`);
    /// `to` is the city or frontier the road leads to.
    Road,
    /// A soldier walking to the nearest own city; it keeps the role until it is there.
    Return,
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
    /// Nothing found for the unit to do: a role of its own, so no unit has none. The audit
    /// looks for something better for a soldier in it (home, or a shore to wait on).
    AwaitingOrders,
    /// Stranded where no own city can be reached: waits on a shore tile for a ship
    /// (`pickup.rs`).
    AwaitTransport,
    /// On its way to a square by an order of its own (`to` = the end of the path).
    MoveTo,
}

/// A unit's role as the registry holds it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Assignment {
    pub role: Role,
    /// The turn the unit took the role.
    pub since: u32,
    /// Where the role sends it (a city to return to, a shore tile to wait on), if anywhere.
    pub to: Option<Position>,
    /// The group it acts with (a raid party, a march, a ferry and its passengers, an escort and
    /// its charge): the units sharing the id. `Registry::new_party` gives a fresh one.
    #[serde(default)]
    pub party: Option<u32>,
    /// The one unit it is tied to: the settler an escort guards, the ship a passenger is aboard.
    #[serde(default)]
    pub with: Option<Serial>,
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
            to: None,
            party: None,
            with: None,
            last_pos: None,
            stuck: 0,
        }
    }
}

/// The one place a unit's role lives: at most one per unit. A role is taken with [`claim`] and
/// handed back with [`release`]; a claim of a lower-priority role than the one held is refused.
///
/// [`claim`]: Registry::claim
/// [`release`]: Registry::release
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Registry {
    map: std::collections::BTreeMap<f4x_core::game::serial::Serial, Assignment>,
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

    /// The ship and settler of the colony voyage under way, if any: the settler is tied to the
    /// ship.
    pub fn colonists(&self) -> Option<(Serial, Serial)> {
        self.map
            .iter()
            .filter(|(_, a)| a.role == Role::Colonize)
            .find_map(|(s, a)| a.with.map(|ship| (ship, *s)))
    }

    /// The soldier escorting `charge`, if one is.
    pub fn escort_of(&self, charge: Serial) -> Option<Serial> {
        self.map
            .iter()
            .find(|(_, a)| a.role == Role::Escort && a.with == Some(charge))
            .map(|(s, _)| *s)
    }

    /// The ships on a voyage with a landing, in serial order: a ship that is `TransportReady` or
    /// `TransportSpace` and is not the one a rescue came with (those hold soldiers
    /// `AwaitTransport` tied to them).
    pub fn ferries(&self) -> Vec<Serial> {
        self.map
            .iter()
            .filter(|(_, a)| {
                matches!(
                    a.role,
                    Role::TransportReady | Role::Transporting | Role::TransportSpace
                ) && a.with.is_none()
                    && a.party.is_none()
            })
            .map(|(s, _)| *s)
            .collect()
    }

    /// The units aboard the ship on a voyage.
    pub fn passengers_of(&self, ship: Serial) -> Vec<Serial> {
        self.map
            .iter()
            .filter(|(_, a)| a.role == Role::Voyage && a.with == Some(ship))
            .map(|(s, _)| *s)
            .collect()
    }

    /// A task takes the ship: it is `TransportSpace` while the passengers planned for it are fewer
    /// than `capacity`, and `TransportReady` when they fill it, with `to` as its target. Each
    /// passenger holds `Voyage`, tied to the ship; soldiers tied by an earlier plan that are not in
    /// this one are let go. Returns whether all of them could take the role.
    pub fn sail(
        &mut self,
        ship: Serial,
        passengers: &[Serial],
        turn: u32,
        to: Position,
        capacity: usize,
    ) -> bool {
        let stale: Vec<Serial> = self
            .passengers_of(ship)
            .into_iter()
            .filter(|p| !passengers.contains(p))
            .collect();
        self.release_many(&stale, Role::Voyage);
        let mut all = self.plan_ship(ship, passengers.len(), capacity, turn, to);
        for &p in passengers {
            all &= self.claim(p, Role::Voyage, turn, None).is_ok();
            self.tie(p, Role::Voyage, ship);
        }
        all
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

    /// The voyage is over: the ship and its passengers hand the role back.
    pub fn end_voyage(&mut self, ship: Serial) {
        let aboard = self.passengers_of(ship);
        self.release_many(&aboard, Role::Voyage);
        if matches!(
            self.role_of(ship),
            Some(Role::TransportReady | Role::Transporting | Role::TransportSpace)
        ) {
            self.release_any(ship);
        }
    }

    /// Every ferry's voyage is over (the landing was given up).
    pub fn end_voyages(&mut self) {
        for ship in self.ferries() {
            self.end_voyage(ship);
        }
    }

    /// The scout on its way to an open tower, if any.
    pub fn planar(&self) -> Option<Serial> {
        self.members(Role::PlanarScout).first().copied()
    }

    /// Drop the units that are not in `alive` (dead, or gone) from the registry.
    pub fn prune(&mut self, alive: &[Serial]) {
        self.map.retain(|s, _| alive.contains(s));
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
        if let Some(a) = self.map.get_mut(&serial).filter(|a| a.role == role) {
            a.to = to;
        }
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
            .max()
            .map_or(1, |m| m + 1)
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

    /// Tie the unit, if it holds `role`, to another (a passenger to its ship).
    pub fn tie(&mut self, serial: Serial, role: Role, with: Serial) {
        if let Some(a) = self.map.get_mut(&serial).filter(|a| a.role == role) {
            a.with = Some(with);
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
        match self.map.get_mut(&serial) {
            // The same role again renews it; a target given is kept when none is.
            Some(held) if held.role == role => {
                if to.is_some() {
                    held.to = to;
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
                        to,
                        party: None,
                        with: None,
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
            self.map.insert(serial, Assignment::awaiting(0));
            true
        } else {
            false
        }
    }

    /// Hand back whatever role the unit holds.
    pub fn release_any(&mut self, serial: Serial) {
        if self.map.contains_key(&serial) {
            self.map.insert(serial, Assignment::awaiting(0));
        }
    }

    /// Keep only the units `keep` says are still around and doing it.
    pub fn retain(&mut self, mut keep: impl FnMut(Serial, &Assignment) -> bool) {
        for (s, a) in self.map.iter_mut() {
            if !keep(*s, a) {
                *a = Assignment::awaiting(a.since);
            }
        }
    }

    /// Note where each unit returning home stands at the audit: for how many in a row it has not
    /// moved. (The other roles count their own way with `track`.)
    pub fn note_positions(&mut self, mut at: impl FnMut(Serial) -> Option<Position>) {
        for (s, a) in self.map.iter_mut().filter(|(_, a)| a.role == Role::Return) {
            let here = at(*s);
            if here.is_some() && here == a.last_pos {
                a.stuck += 1;
            } else {
                a.stuck = 0;
            }
            a.last_pos = here;
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
            Role::Colonize => 0,
            Role::Settle => 1,
            Role::Voyage => 2,
            Role::TransportReady => 3,
            Role::Transporting => 3,
            Role::SeaParty => 4,
            Role::PlanarScout => 5,
            Role::TransportSpace => 6,
            Role::AwaitTransport => 7,
            Role::Perimeter => 8,
            Role::Raid => 9,
            Role::March => 10,
            Role::Muster => 11,
            Role::Escort => 12,
            Role::Meld => 13,
            Role::Road => 14,
            Role::Scout => 15,
            Role::Return => 16,
            Role::MoveTo => 17,
            Role::Garrison => 18,
            Role::TransportFree => 19,
            Role::AwaitingOrders => 20,
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
/// unit a routine has a task for keeps its role (a `Return` only while it walks and has not
/// stood still too long). Every other unit gets the base role its place and orders say, written
/// into the registry with its target:
///
/// - a soldier in an own city: `Garrison`, `to` = the city;
/// - a unit with an order of its own: `MoveTo`, `to` = the end of its path;
/// - a ship with no mission and no order: `TransportFree` (with a mission it holds
///   `TransportSpace`, `TransportReady` or `Transporting`, which `naval.rs` and `pickup.rs` set);
/// - anything else: `AwaitingOrders`.
pub fn sync(ai: &mut Ai, view: &PlayerView, turn: u32) -> Vec<Held> {
    let mut out = Vec::new();
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
        // A `Return` ends when the soldier is home, has no order, or has stood still too long.
        if ai.roles.is(serial, Role::Return) {
            let stuck = ai.roles.get(serial).map_or(0, |a| a.stuck);
            if !ordered || city.is_some() || stuck >= RETURN_PATIENCE {
                ai.roles.release(serial, Role::Return);
            }
        }
        let target = view.unit_destination(unit);
        let (role, to) = match class {
            Class::Soldier if city.is_some() => (Role::Garrison, city),
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

/// Audits in a row a soldier walking home may stand still before the role is taken from it.
const RETURN_PATIENCE: u32 = 5;

/// The end-of-turn check: every unit the wizard owns is looked at, each turn, and has a role. A
/// soldier awaiting orders (a lone unit, or a party too small for any task, left on a tower or a
/// road) walks to the nearest own city that can be reached and has room, and keeps that role
/// (`Return`) until it is there; if no city can be reached it waits on a shore for a ship. Ships
/// and the other units with none are counted. The audit is counted in an
/// [`Event::RolesAudited`].
pub fn audit(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let counted = {
        let view = PlayerView::new(game, me);
        // The units that are gone leave the registry; the others each get their role.
        let alive: Vec<_> = view.my_units().iter().map(|&u| view.serial_of(u)).collect();
        ai.roles.prune(&alive);
        let held = sync(ai, &view, game.turn);
        ai.roles.note_positions(|s| {
            held.iter()
                .find(|h| view.serial_of(h.unit) == s)
                .and_then(|h| view.unit_position(h.unit))
        });
        tally(&held)
    };
    if counted.adrift.is_empty() && counted.unassigned == 0 {
        return;
    }
    events.push(Event::RolesAudited {
        player: me,
        adrift: counted.adrift.len() as u32,
        unassigned: counted.unassigned,
    });
    for group in send_home(ai, game, &counted.adrift, events) {
        // No own city can be reached by land: wait for a ship on the shore.
        crate::pickup::wait_at_shore(ai, game, &group);
    }
}

/// Send each group of units standing on one square to the nearest own city that has room. The
/// groups no city took are returned.
fn send_home(
    ai: &mut Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    events: &mut Vec<Event>,
) -> Vec<Vec<hecs::Entity>> {
    let mut left_out = Vec::new();
    let me = ai.player;
    let mut by_square: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
    let cities: Vec<Position> = {
        let view = PlayerView::new(game, me);
        for &u in units {
            let Some(p) = view.unit_position(u) else {
                continue;
            };
            match by_square.iter_mut().find(|(q, _)| *q == p) {
                Some((_, list)) => list.push(u),
                None => by_square.push((p, vec![u])),
            }
        }
        view.my_cities().into_iter().map(|(_, p)| p).collect()
    };
    by_square.sort_by_key(|(p, _)| (p.plane as u8, p.y, p.x));
    for (pos, group) in by_square {
        let mut homes: Vec<(u32, Position)> = {
            let view = PlayerView::new(game, me);
            cities
                .iter()
                .filter(|&&c| c.plane == pos.plane)
                .filter_map(|&c| distance(&view, pos, c).map(|d| (d, c)))
                .collect()
        };
        homes.sort_by_key(|&(d, c)| (d, c.y, c.x));
        // Whether any city can be reached on foot at all (full cities are a wait, not a strand).
        let reachable = {
            let view = PlayerView::new(game, me);
            homes.iter().any(|&(_, c)| view.can_reach(&group, c))
        };
        let mut sent = false;
        for (_, home) in homes {
            let ok = {
                let view = PlayerView::new(game, me);
                Ai::room_at(game, home, group.len()) && view.can_reach(&group, home)
            };
            if ok
                && ai
                    .send(
                        game,
                        Command::GoTo {
                            units: group.clone(),
                            to: home,
                        },
                        events,
                    )
                    .is_ok()
            {
                sent = true;
                let view = PlayerView::new(game, me);
                for &u in &group {
                    let _ = ai
                        .roles
                        .claim(view.serial_of(u), Role::Return, game.turn, Some(home));
                }
                break;
            }
        }
        if !sent && !reachable {
            left_out.push(group);
        }
    }
    left_out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, spawn_tower, COAST_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A soldier standing outside a city with no list, no order and no city under it is adrift:
    /// the audit sends it home and counts it.
    #[test]
    fn a_lone_soldier_outside_a_city_is_found_and_sent_home() -> TestResult {
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
        assert!(lone.iter().all(|&u| view.unit_has_order(u)));
        // It keeps the role until it is in the city: next turn it is `Return`, not adrift.
        let held = sync(&mut ai, &view, 1);
        let roles = held.iter().find(|h| h.unit == lone[0]).map(|h| h.role);
        assert_eq!(roles, Some(Role::Return));
        assert!(tally(&held).adrift.is_empty());
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

    /// Soldiers with no own city they can walk to take the waiting role and head for the shore:
    /// they are not sent on an impossible way home every turn.
    #[test]
    fn soldiers_with_no_city_in_reach_wait_for_a_ship() -> TestResult {
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
        assert_eq!(
            ai.roles.members(Role::AwaitTransport).len(),
            2,
            "{events:?}"
        );
        let view = PlayerView::new(&g, P0);
        let held = sync(&mut ai, &view, 1);
        for &u in &stranded {
            let roles = held.iter().find(|h| h.unit == u).map(|h| h.role);
            assert_eq!(roles, Some(Role::AwaitTransport));
        }
        assert!(tally(&held).adrift.is_empty());
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
    fn the_registry_holds_one_role_per_unit() {
        let mut r = Registry::default();
        let (a, b) = (serial(1), serial(2));
        assert_eq!(r.claim(a, Role::Raid, 3, None), Ok(()));
        assert_eq!(
            r.claim(a, Role::Return, 4, None),
            Err(Role::Raid),
            "a raider is not sent home by a lower claim"
        );
        assert_eq!(r.role_of(a), Some(Role::Raid));
        assert_eq!(r.claim(b, Role::Return, 4, None), Ok(()));
        assert_eq!(
            r.claim(b, Role::AwaitTransport, 5, None),
            Ok(()),
            "a better role takes over"
        );
        assert_eq!(r.role_of(b), Some(Role::AwaitTransport));
        assert!(!r.release(b, Role::Return), "it does not hold that role");
        assert!(r.release(b, Role::AwaitTransport));
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
            ai.roles.claim(raider, Role::Return, 5, None),
            Err(Role::Raid),
            "a raider is not sent home by the lower claim"
        );
        assert_eq!(ai.roles.claim(scout, Role::AwaitTransport, 5, None), Ok(()));
        assert_eq!(ai.roles.role_of(scout), Some(Role::AwaitTransport));
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

    /// The registry counts the audits a unit has stood still, and starts again when it moves.
    #[test]
    fn the_registry_counts_the_audits_a_unit_has_not_moved() {
        let mut r = Registry::default();
        let a = serial(1);
        let _ = r.claim(a, Role::Return, 1, None);
        let here = Some(crate::testworld::at(3, 3));
        let elsewhere = Some(crate::testworld::at(4, 3));
        for _ in 0..3 {
            r.note_positions(|_| here);
        }
        assert_eq!(
            r.get(a).map(|x| x.stuck),
            Some(2),
            "the first sight is no wait"
        );
        r.note_positions(|_| elsewhere);
        assert_eq!(r.get(a).map(|x| x.stuck), Some(0));
    }

    /// A soldier given up on as stuck on its way home loses the role and is found again.
    #[test]
    fn a_soldier_that_does_not_get_home_loses_the_role_of_returning() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let lone = spawn(&mut g, P0, at(8, 6), 104, 1)[0];
        let mut ai = Ai::new(P0);
        let s = PlayerView::new(&g, P0).serial_of(lone);
        let _ = ai
            .roles
            .claim(s, Role::Return, 1, Some(at(COAST_CITY.0, COAST_CITY.1)));
        // It has no order and stands still.
        for _ in 0..=RETURN_PATIENCE + 1 {
            ai.roles.note_positions(|_| Some(at(8, 6)));
        }
        audit(&mut ai, &mut g, &mut Vec::new());
        // The old assignment was dropped; the audit found the soldier adrift and sent it again.
        assert_eq!(ai.roles.get(s).map(|a| a.since), Some(g.turn));
        Ok(())
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

    /// A task takes a free ship: space while fewer soldiers are planned than it holds, ready when
    /// they fill it, transporting once it sails.
    #[test]
    fn a_ship_taken_by_a_task_is_space_until_its_load_is_full_then_ready() {
        let mut r = Registry::default();
        let (ship, a, b) = (serial(1), serial(2), serial(3));
        let to = at(5, 5);
        r.sail(ship, &[a], 1, to, 2);
        assert_eq!(r.role_of(ship), Some(Role::TransportSpace));
        assert_eq!(r.target_of(ship, Role::TransportSpace), Some(to));
        r.sail(ship, &[a, b], 2, to, 2);
        assert_eq!(r.role_of(ship), Some(Role::TransportReady));
        assert_eq!(r.passengers_of(ship), vec![a, b]);
        // A smaller plan lets the extra soldier go and the ship has room again.
        r.sail(ship, &[a], 3, to, 2);
        assert_eq!(r.role_of(ship), Some(Role::TransportSpace));
        assert_eq!(r.passengers_of(ship), vec![a]);
        r.depart(ship, to);
        assert_eq!(r.role_of(ship), Some(Role::Transporting));
    }
}
