//! The army ledger: which units each player has, and their standing orders
//! (`docs/ecs-structure.md` 3.12).
//!
//! Tests: in `tests/it/c9_ledger.rs`.

use std::collections::{BTreeMap, BTreeSet};

use crate::hero::{Fate, Hero};
use crate::map::Position;
use crate::player::{Owner, PlayerId, MONSTERS};
use crate::rules::UnitId;
use crate::unit::{Experience, Order, Unit};
use crate::world::serial::Serial;

/// How a unit left a player's army.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Departure {
    /// Killed in a fight, by a spell or by the road.
    Slain,
    /// Gone for good, so that nothing can bring it back (`Fate::Gone`).
    Destroyed,
    /// Dismissed, let go or disbanded.
    LetGo,
    /// Spent on what it was for: a settler on its city, a spirit on its node.
    UsedUp,
    /// Now another player's.
    ChangedSides,
}

impl From<Fate> for Departure {
    fn from(fate: Fate) -> Self {
        match fate {
            Fate::Slain => Departure::Slain,
            Fate::Released => Departure::LetGo,
            Fate::Gone => Departure::Destroyed,
        }
    }
}

/// What a unit that left was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    /// A unit that is no hero: its type and the experience it left with.
    Troop { kind: UnitId, xp: u32 },
    /// A hero: its entry in the hero list, which says what it is and what it has become since.
    Hero(hecs::Entity),
}

/// The record of a unit that left a player's army.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepartedUnit {
    pub serial: Serial,
    pub who: Who,
    pub how: Departure,
    /// The square it stood on.
    pub at: Position,
}

/// A unit's order, and the enemies it knew of when it got it.
#[derive(Debug, Clone)]
struct Standing {
    order: Order,
    /// The enemies its walking group saw as the order was given: seeing one of these again does
    /// not end it (`Order::ends_on_sight`).
    seen: BTreeSet<Serial>,
}

/// On every player entity: the units the player has, by creation number, their standing orders,
/// and the record of those that left. It points at the units and copies nothing of theirs. Only
/// [`enlist`], [`transfer`] and [`strike`] write the units, and every change of a unit's owner
/// goes through one of them. The orders are kept nowhere else.
#[derive(Debug, Clone, Default)]
pub struct ArmyLedger {
    living: BTreeMap<Serial, hecs::Entity>,
    /// Of living units only: a unit that leaves or changes sides loses its order.
    orders: BTreeMap<Serial, Standing>,
    departed: Vec<DepartedUnit>,
}

impl ArmyLedger {
    /// The units, oldest first.
    pub fn living(&self) -> impl Iterator<Item = hecs::Entity> + '_ {
        self.living.values().copied()
    }

    /// How many units the player has.
    pub fn len(&self) -> usize {
        self.living.len()
    }

    pub fn is_empty(&self) -> bool {
        self.living.is_empty()
    }

    /// The standing order of the unit with this creation number.
    pub fn order(&self, serial: Serial) -> Option<&Order> {
        self.orders.get(&serial).map(|s| &s.order)
    }

    /// The units that have a standing order, with it, oldest first.
    pub fn ordered(&self) -> impl Iterator<Item = (hecs::Entity, &Order)> + '_ {
        self.orders
            .iter()
            .filter_map(|(serial, s)| Some((*self.living.get(serial)?, &s.order)))
    }

    /// The living unit with this creation number.
    pub fn unit(&self, serial: Serial) -> Option<hecs::Entity> {
        self.living.get(&serial).copied()
    }

    /// The units that left, in the order they did.
    pub fn departed(&self) -> &[DepartedUnit] {
        &self.departed
    }
}

/// Whose a unit is and its creation number; `None` for anything that is no unit with an owner.
fn entry_of(ecs: &hecs::World, thing: hecs::Entity) -> Option<(hecs::Entity, Serial)> {
    let thing = ecs.entity(thing).ok()?;
    if !thing.has::<Unit>() {
        return None;
    }
    let owner = thing.get::<&Owner>()?.0;
    let serial = *thing.get::<&Serial>()?;
    Some((owner, serial))
}

/// Enter a unit, which has its owner and its creation number by now, in its owner's ledger.
pub(crate) fn enlist(ecs: &hecs::World, unit: hecs::Entity) {
    let entry = entry_of(ecs, unit);
    debug_assert!(entry.is_some(), "only a unit with an owner is enlisted");
    let Some((owner, serial)) = entry else {
        return;
    };
    let ledger = ecs.get::<&mut ArmyLedger>(owner);
    debug_assert!(ledger.is_ok(), "every player has a ledger");
    if let Ok(mut ledger) = ledger {
        ledger.living.insert(serial, unit);
    }
}

/// Take a unit out of its owner's ledger, before it loses its body or changes sides. Anything
/// else (a city, a feature) is in no ledger and is left alone.
pub(crate) fn strike(ecs: &hecs::World, thing: hecs::Entity) {
    let Some((owner, serial)) = entry_of(ecs, thing) else {
        return;
    };
    // A player entity that is gone took its ledger with it.
    if let Ok(mut ledger) = ecs.get::<&mut ArmyLedger>(owner) {
        ledger.living.remove(&serial);
        ledger.orders.remove(&serial);
    }
}

/// The standing order of a unit.
pub(crate) fn order_of(ecs: &hecs::World, unit: hecs::Entity) -> Option<Order> {
    let (owner, serial) = entry_of(ecs, unit)?;
    let ledger = ecs.get::<&ArmyLedger>(owner).ok()?;
    ledger.order(serial).cloned()
}

/// Whether a unit has a standing order.
pub(crate) fn has_order(ecs: &hecs::World, unit: hecs::Entity) -> bool {
    entry_of(ecs, unit).is_some_and(|(owner, serial)| {
        ecs.get::<&ArmyLedger>(owner)
            .is_ok_and(|ledger| ledger.order(serial).is_some())
    })
}

/// Give a unit a new order, in place of the one it had, with the enemies already known to it.
/// Anything that is no unit of a player's army takes none.
pub(crate) fn give_order(ecs: &hecs::World, unit: hecs::Entity, order: Order, seen: &[Serial]) {
    let Some((owner, serial)) = entry_of(ecs, unit) else {
        return;
    };
    if let Ok(mut ledger) = ecs.get::<&mut ArmyLedger>(owner) {
        if ledger.living.get(&serial) == Some(&unit) {
            let seen = seen.iter().copied().collect();
            ledger.orders.insert(serial, Standing { order, seen });
        }
    }
}

/// Note how far a unit's order has got: the order changes, what it has seen stays.
pub(crate) fn advance_order(ecs: &hecs::World, unit: hecs::Entity, order: Order) {
    let Some((owner, serial)) = entry_of(ecs, unit) else {
        return;
    };
    if let Ok(mut ledger) = ecs.get::<&mut ArmyLedger>(owner) {
        if let Some(standing) = ledger.orders.get_mut(&serial) {
            standing.order = order;
        }
    }
}

/// The enemies a unit knew of when it got its order, oldest first.
pub(crate) fn seen_of(ecs: &hecs::World, unit: hecs::Entity) -> Vec<Serial> {
    let Some((owner, serial)) = entry_of(ecs, unit) else {
        return Vec::new();
    };
    ecs.get::<&ArmyLedger>(owner)
        .ok()
        .and_then(|ledger| {
            ledger
                .orders
                .get(&serial)
                .map(|s| s.seen.iter().copied().collect())
        })
        .unwrap_or_default()
}

/// End a unit's standing order, if it has one.
pub(crate) fn end_order(ecs: &hecs::World, unit: hecs::Entity) {
    let Some((owner, serial)) = entry_of(ecs, unit) else {
        return;
    };
    if let Ok(mut ledger) = ecs.get::<&mut ArmyLedger>(owner) {
        ledger.orders.remove(&serial);
    }
}

/// Give a unit to the player entity `to`.
pub(crate) fn transfer(ecs: &mut hecs::World, unit: hecs::Entity, to: hecs::Entity) {
    strike(ecs, unit);
    crate::world::put_in(ecs, unit, Owner(to));
    enlist(ecs, unit);
}

/// Note in its owner's ledger that a unit is about to leave, before anything is taken off it. The
/// monsters keep no record: their lair guards come and go with each fight.
pub(crate) fn record(ecs: &hecs::World, unit: hecs::Entity, how: Departure) {
    let Some((owner, serial)) = entry_of(ecs, unit) else {
        return;
    };
    if ecs.get::<&PlayerId>(owner).is_ok_and(|p| *p == MONSTERS) {
        return;
    }
    let who = if ecs.get::<&Hero>(unit).is_ok() {
        Who::Hero(unit)
    } else {
        let Ok(kind) = ecs.get::<&Unit>(unit).map(|u| u.0) else {
            return;
        };
        let xp = ecs.get::<&Experience>(unit).map_or(0, |x| x.0);
        Who::Troop { kind, xp }
    };
    let at = ecs.get::<&Position>(unit).map(|p| *p);
    debug_assert!(at.is_ok(), "a unit stands somewhere");
    if let Ok(at) = at {
        keep(
            ecs,
            owner,
            DepartedUnit {
                serial,
                who,
                how,
                at,
            },
        );
    }
}

/// Add a record to the ledger of the player entity `owner`.
pub(crate) fn keep(ecs: &hecs::World, owner: hecs::Entity, departed: DepartedUnit) {
    let ledger = ecs.get::<&mut ArmyLedger>(owner);
    debug_assert!(ledger.is_ok(), "every player has a ledger");
    if let Ok(mut ledger) = ledger {
        ledger.departed.push(departed);
    }
}
