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
    /// Standing in one of the wizard's cities.
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
    /// A ship on its way to fetch stranded soldiers (`pickup.rs`).
    Pickup,
    /// A soldier that goes with a unit it protects (`escort.rs`); `with` is that unit.
    Escort,
    /// A soldier walking to the nearest own city; it keeps the role until it is there.
    Return,
    /// A ship in a city with no voyage.
    Docked,
    /// Nothing found for the unit to do: a role of its own, so no unit has none. The audit
    /// looks for something better for a soldier in it (home, or a shore to wait on).
    AwaitingOrders,
    /// Stranded where no own city can be reached: waits on a shore tile for a ship
    /// (`pickup.rs`).
    AwaitTransport,
    /// Has an order (on its way somewhere) but belongs to no list.
    Ordered,
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

    /// Whether the unit holds any role of the registry.
    pub fn has_role(&self, serial: Serial) -> bool {
        self.map.contains_key(&serial)
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

    /// The ships on a voyage with a landing, in serial order.
    pub fn ferries(&self) -> Vec<Serial> {
        self.map
            .iter()
            .filter(|(_, a)| a.role == Role::Voyage && a.with.is_none())
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

    /// A ship sets out with `passengers`: the ship and each passenger hold `Voyage`, the
    /// passengers tied to the ship. Returns whether all of them could take the role.
    pub fn sail(&mut self, ship: Serial, passengers: &[Serial], turn: u32) -> bool {
        let party = self.new_party();
        let mut all = self.claim(ship, Role::Voyage, turn, None).is_ok();
        self.group(&[ship], Role::Voyage, party, None);
        for &p in passengers {
            all &= self.claim(p, Role::Voyage, turn, None).is_ok();
            self.group(&[p], Role::Voyage, party, Some(ship));
        }
        all
    }

    /// The voyage is over: the ship and its passengers hand the role back.
    pub fn end_voyage(&mut self, ship: Serial) {
        let aboard = self.passengers_of(ship);
        self.release_many(&aboard, Role::Voyage);
        self.release(ship, Role::Voyage);
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
        self.map.retain(|_, a| a.role != role);
    }

    /// Hand back `role` from each of the units that hold it.
    pub fn release_many(&mut self, serials: &[Serial], role: Role) {
        self.map
            .retain(|s, a| a.role != role || !serials.contains(s));
    }

    /// Keep `role` only on the units `keep` says so.
    pub fn retain_role(&mut self, role: Role, mut keep: impl FnMut(Serial) -> bool) {
        self.map.retain(|s, a| a.role != role || keep(*s));
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
        self.map.keys().copied().collect()
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
            self.map.remove(&serial);
            true
        } else {
            false
        }
    }

    /// Hand back whatever role the unit holds.
    pub fn release_any(&mut self, serial: Serial) {
        self.map.remove(&serial);
    }

    /// Keep only the units `keep` says are still around and doing it.
    pub fn retain(&mut self, mut keep: impl FnMut(Serial, &Assignment) -> bool) {
        self.map.retain(|s, a| keep(*s, a));
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
    fn priority(self) -> u8 {
        match self {
            Role::Colonize => 0,
            Role::Settle => 1,
            Role::Voyage => 2,
            Role::SeaParty => 3,
            Role::PlanarScout => 4,
            Role::Pickup => 5,
            Role::AwaitTransport => 6,
            Role::Raid => 7,
            Role::March => 8,
            Role::Muster => 9,
            Role::Escort => 10,
            Role::Scout => 11,
            Role::Return => 12,
            Role::Ordered => 13,
            Role::Garrison => 14,
            Role::Docked => 15,
            Role::AwaitingOrders => 16,
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

/// The role of every unit the wizard owns, soldiers, ships and the rest: the one the registry
/// holds, or else what the unit's place and orders say (garrison, docked, on its way), or else
/// `AwaitingOrders`.
pub fn derive(ai: &Ai, view: &PlayerView) -> Vec<Held> {
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
        let in_city = view
            .unit_position(unit)
            .is_some_and(|p| view.city_at(p).is_some());
        // A soldier on its way home keeps `Return` only while it has an order; a stale one is
        // not a role (the audit drops it).
        let role = match ai.roles.role_of(serial) {
            Some(r) if r != Role::Return || ordered => r,
            _ => match class {
                Class::Soldier if in_city => Role::Garrison,
                Class::Ship if in_city => Role::Docked,
                _ if ordered => Role::Ordered,
                _ => Role::AwaitingOrders,
            },
        };
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
        let held = derive(ai, &view);
        // The units that are gone have no role; soldiers that reached a city are no longer on
        // their way home, and one that has not moved for a few audits is given up (it is found
        // awaiting orders again).
        let alive: Vec<_> = held.iter().map(|h| view.serial_of(h.unit)).collect();
        ai.roles.prune(&alive);
        ai.roles.retain(|s, a| {
            if a.role != Role::Return {
                return true;
            }
            let outside = held.iter().any(|h| {
                view.serial_of(h.unit) == s
                    && view
                        .unit_position(h.unit)
                        .is_some_and(|p| view.city_at(p).is_none())
            });
            outside && a.stuck < RETURN_PATIENCE
        });
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
        let held = derive(&ai, &view);
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
        let held = derive(&ai, &view);
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
        let held = derive(&ai, &view);
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
        let ai = Ai::new(P0);
        let view = PlayerView::new(&g, P0);
        let held = derive(&ai, &view);
        let of = |u: hecs::Entity| held.iter().find(|h| h.unit == u).cloned();
        let ship_held = of(ship).ok_or("the ship is in the table")?;
        assert_eq!(
            (ship_held.class, ship_held.role),
            (Class::Ship, Role::Docked)
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
        let ai = Ai::new(P0);
        let view = PlayerView::new(&g, P0);
        let held = derive(&ai, &view);
        assert_eq!(
            held.len(),
            view.my_units().len(),
            "every unit is in the table"
        );
        Ok(())
    }
}
