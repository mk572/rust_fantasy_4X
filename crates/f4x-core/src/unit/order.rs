//! Standing orders: what a unit has been told to do, and how the world carries it out
//! (`docs/game-flow.md` 3.2). An order is kept in its owner's army ledger. It moves its unit as
//! it is given and again at the end of each of the owner's turns; a road is worked on at the
//! start of the turn (`map/road.rs`). It does only what the owner could do by commands.
//!
//! Tests: in `tests/it/c9_order.rs` and `tests/it/c9_errand.rs`.

use crate::feature::node::MeldError;
use crate::map::road::RoadWork;
use crate::map::Position;
use crate::player::view::PlayerView;
use crate::player::{PlayerId, MONSTERS};
use crate::rules::Ability;
use crate::unit::ledger::{self, ArmyLedger};
use crate::unit::movement::{adjacent, Entry, MoveError};
use crate::unit::Standby;
use crate::world::rng::GameRng;
use crate::world::serial::Serial;
use crate::world::{Event, Rejection, World};

/// A square an order sends its unit to, and the way there (the first entry is the next square).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Trip {
    pub to: Position,
    pub path: Vec<Position>,
}

/// An order to explore: one patch of new ground after another.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Scouting {
    /// The patch the units are walking to now; `None` until one is chosen.
    pub trip: Option<Trip>,
    /// Squares whose surroundings are left alone.
    pub avoid: Vec<Position>,
    /// New ground near these squares is worth more.
    pub favour: Vec<Position>,
}

/// What a unit has been told to do.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Order {
    /// Walk this path (the first entry is the next square).
    GoTo(Vec<Position>),
    Scout(Scouting),
    /// Walk to a magic node and meld with it.
    Meld(Trip),
    /// Walk to a square and found a city there; without a name the race's next one is taken.
    Settle {
        trip: Trip,
        name: Option<String>,
    },
    /// Walk to a city of one's own.
    Reinforce(Trip),
    /// Go with a unit of one's own, wherever it goes.
    Escort(Serial),
    /// Build a road, one square after another.
    Road(RoadWork),
}

impl Order {
    /// Whether the order ends when its unit sees an enemy it had not seen when it got it.
    /// A walk the player chose, and the orders that are for going where the enemy is, go on.
    pub fn ends_on_sight(&self) -> bool {
        match self {
            Order::Scout(_) | Order::Meld(_) | Order::Settle { .. } | Order::Road(_) => true,
            Order::GoTo(_) | Order::Reinforce(_) | Order::Escort(_) => false,
        }
    }

    /// The order after a step along its path.
    fn stepped(mut self) -> Self {
        let path = match &mut self {
            Order::GoTo(path) => Some(path),
            Order::Meld(trip) | Order::Reinforce(trip) | Order::Settle { trip, .. } => {
                Some(&mut trip.path)
            }
            Order::Scout(scouting) => scouting.trip.as_mut().map(|t| &mut t.path),
            Order::Escort(_) | Order::Road(_) => None,
        };
        if let Some(path) = path {
            if !path.is_empty() {
                path.remove(0);
            }
        }
        self
    }

    /// The same order with another way to its square.
    fn with_trip(self, new: Trip) -> Self {
        match self {
            Order::Meld(_) => Order::Meld(new),
            Order::Reinforce(_) => Order::Reinforce(new),
            Order::Settle { name, .. } => Order::Settle { trip: new, name },
            Order::Scout(scouting) => Order::Scout(Scouting {
                trip: Some(new),
                ..scouting
            }),
            other @ (Order::GoTo(_) | Order::Escort(_) | Order::Road(_)) => other,
        }
    }
}

/// An order to be given (`Command::GiveOrder`). A walk is `Command::GoTo` and a road is
/// `Command::BuildRoad`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Errand {
    /// Explore, beginning with the patch at `to` if one is named.
    Scout {
        to: Option<Position>,
        avoid: Vec<Position>,
        favour: Vec<Position>,
    },
    /// Walk to the node at `to` and meld with it.
    Meld { to: Position },
    /// Walk to `to` and found a city there.
    Settle { to: Position, name: Option<String> },
    /// Walk to the own city at `to`.
    Reinforce { to: Position },
    /// Go with the own unit `charge`.
    Escort {
        #[serde(with = "crate::world::stable_ref::one")]
        charge: hecs::Entity,
    },
}

impl Errand {
    /// The square the order is for, when it names one.
    pub fn square(&self) -> Option<Position> {
        match self {
            Errand::Scout { to, .. } => *to,
            Errand::Meld { to } | Errand::Settle { to, .. } | Errand::Reinforce { to } => Some(*to),
            Errand::Escort { .. } => None,
        }
    }
}

/// Why an order cannot be given, besides what the commands it stands for refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderError {
    /// None of the units can found a city.
    NoSettler,
    /// The player has not seen the square.
    Unexplored,
    NotOwnCity,
    /// The unit to escort is not another unit of the player's.
    BadCharge,
}

impl OrderError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::NoSettler => "refuse_map.order_error_no_settler",
            Self::Unexplored => "refuse_map.order_error_unexplored",
            Self::NotOwnCity => "refuse_map.order_error_not_own_city",
            Self::BadCharge => "refuse_map.order_error_bad_charge",
        }
    }
}

/// Why an order ended before it was done, when no step was refused (`Event::OrderFailed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderEnd {
    /// A unit saw monsters, or a unit of a player at war with its owner, that it had not seen.
    EnemySeen,
    /// What the order was for is gone, or can no longer be done.
    TargetGone,
    /// No way leads there.
    NoWay,
    /// A scout has nothing left to explore that it can reach.
    NothingLeft,
}

impl OrderEnd {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::EnemySeen => "order.end_enemy_seen",
            Self::TargetGone => "order.end_target_gone",
            Self::NoWay => "order.end_no_way",
            Self::NothingLeft => "order.end_nothing_left",
        }
    }
}

/// When a standing order is walked: it says what a walk may do on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Walk {
    /// As it is given, which is the player's own move: a go-to may ask for a battle.
    Given,
    /// At the end of the owner's turn: the units never attack by themselves.
    TurnEnd,
}

/// What a group does next for its order.
enum Leg {
    Step(Position),
    /// Nothing for now: the order stands.
    Wait,
    /// The order is over.
    Done,
    Over(OrderEnd),
    Meld,
    Settle(Option<String>),
}

impl World {
    /// `Command::GoTo`.
    pub(crate) fn go_to(
        &mut self,
        units: &[hecs::Entity],
        to: Position,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        match self.order_path(units, to) {
            Some(path) => {
                self.set_order(units, Order::GoTo(path));
                self.follow_orders(units, Walk::Given, rng)
            }
            None => vec![Event::Rejected(Rejection::NoPath(
                self.why_no_path(units, to),
            ))],
        }
    }

    /// `Command::GiveOrder`: the units, which stand on one square, get the order and set out.
    pub(crate) fn give_errand(
        &mut self,
        units: &[hecs::Entity],
        errand: Errand,
        rng: &mut GameRng,
    ) -> Result<Vec<Event>, Rejection> {
        let bad = Rejection::Move(MoveError::BadSelection);
        let &first = units.first().ok_or(bad)?;
        let here = self.units().unit_position(first).ok_or(bad)?;
        let owner = self.units().unit_owner(first).ok_or(bad)?;
        if units.iter().any(|&u| {
            self.units().unit_position(u) != Some(here) || self.units().unit_owner(u) != Some(owner)
        }) {
            return Err(bad);
        }
        let order = match errand {
            Errand::Scout { to, avoid, favour } => {
                let trip = match to {
                    Some(to) => Some(self.trip_to(units, here, to)?),
                    None => None,
                };
                Order::Scout(Scouting {
                    trip,
                    avoid,
                    favour,
                })
            }
            Errand::Meld { to } => {
                self.check_meld_order(owner, units, to)
                    .map_err(Rejection::Meld)?;
                Order::Meld(self.trip_to(units, here, to)?)
            }
            Errand::Settle { to, name } => {
                if !units.iter().any(|&u| self.units().unit_is_settler(u)) {
                    return Err(Rejection::Order(OrderError::NoSettler));
                }
                if self.time_stopper().is_some_and(|keeper| keeper != owner) {
                    return Err(Rejection::FoundCity(
                        crate::city::FoundCityError::TimeStopped,
                    ));
                }
                if !PlayerView::new(self, owner).explored(to) {
                    return Err(Rejection::Order(OrderError::Unexplored));
                }
                self.can_found_city(to).map_err(Rejection::FoundCity)?;
                Order::Settle {
                    trip: self.trip_to(units, here, to)?,
                    name,
                }
            }
            Errand::Reinforce { to } => {
                if !self.own_city_at(owner, to) {
                    return Err(Rejection::Order(OrderError::NotOwnCity));
                }
                Order::Reinforce(self.trip_to(units, here, to)?)
            }
            Errand::Escort { charge } => {
                let there = self
                    .units()
                    .unit_position(charge)
                    .filter(|_| {
                        !units.contains(&charge) && self.units().unit_owner(charge) == Some(owner)
                    })
                    .ok_or(Rejection::Order(OrderError::BadCharge))?;
                if !self.stands_with(here, there) && self.find_path(units, there).is_none() {
                    return Err(Rejection::NoPath(self.why_no_path(units, there)));
                }
                Order::Escort(self.units().serial_of(charge))
            }
        };
        self.set_order(units, order);
        Ok(self.follow_orders(units, Walk::Given, rng))
    }

    /// The path of an order, from the tower square of the destination's plane when the stack
    /// stands in a tower (`start_from_tower_on`).
    fn order_path(&mut self, units: &[hecs::Entity], to: Position) -> Option<Vec<Position>> {
        self.start_from_tower_on(units, to);
        self.find_path(units, to)
    }

    /// The way from `here` to `to` for an errand: none to walk when the units stand there.
    fn trip_to(
        &mut self,
        units: &[hecs::Entity],
        here: Position,
        to: Position,
    ) -> Result<Trip, Rejection> {
        if here == to {
            return Ok(Trip {
                to,
                path: Vec::new(),
            });
        }
        match self.order_path(units, to) {
            Some(path) => Ok(Trip { to, path }),
            None => Err(Rejection::NoPath(self.why_no_path(units, to))),
        }
    }

    /// Whether two squares are one for the units standing on them (a tower is one square on both
    /// planes).
    fn stands_with(&self, a: Position, b: Position) -> bool {
        a == b || self.economy().tower_mirror(a) == Some(b)
    }

    fn own_city_at(&self, owner: PlayerId, at: Position) -> bool {
        self.economy()
            .city_at(at)
            .is_some_and(|c| self.economy().city_owner(c) == Some(owner))
    }

    /// Whether the units may be told to meld with the node at `to`, as far as `owner` knows.
    fn check_meld_order(
        &self,
        owner: PlayerId,
        units: &[hecs::Entity],
        to: Position,
    ) -> Result<(), MeldError> {
        let melds = |u: &hecs::Entity| {
            self.units()
                .unit_has_ability(*u, Ability::new(42 /* meld */))
        };
        if !units.iter().any(melds) {
            return Err(MeldError::NoMelder);
        }
        let node = PlayerView::new(self, owner)
            .nodes()
            .into_iter()
            .find(|n| n.pos == to)
            .ok_or(MeldError::NoNode)?;
        if node.warped {
            return Err(MeldError::Warped);
        }
        // One's own node takes a Guardian Spirit and nothing else.
        let guardian = units.iter().any(|&u| {
            self.units()
                .unit_def(u)
                .is_some_and(|d| d.num == 176 /* guardian_spirit */)
        });
        if node.owner == Some(owner) && (node.guardian || !guardian) {
            return Err(MeldError::AlreadyGuarded);
        }
        Ok(())
    }

    /// Give the units an order, in place of whatever they had been told. An order that ends on
    /// sight starts with the enemies they see now, which do not end it.
    pub(crate) fn set_order(&mut self, units: &[hecs::Entity], order: Order) {
        let seen = if order.ends_on_sight() {
            self.in_sight_of(units, |me, them| self.is_enemy(me, them))
        } else {
            Vec::new()
        };
        for &u in units {
            self.strip::<Standby>(u);
            ledger::give_order(&self.ecs, u, order.clone(), &seen);
        }
    }

    /// Monsters, and players at war with `me`.
    fn is_enemy(&self, me: PlayerId, them: PlayerId) -> bool {
        them == MONSTERS || self.treaty(me, them) == crate::diplomacy::Treaty::War
    }

    /// An order that ends on sight ends for the units that walk together when one of them sees an
    /// enemy none of them knew of.
    pub(crate) fn end_orders_on_sight(&mut self, events: &mut Vec<Event>) {
        let mut watchers: Vec<hecs::Entity> = self
            .ecs
            .query::<&ArmyLedger>()
            .iter()
            .flat_map(|(_, ledger)| {
                ledger
                    .ordered()
                    .filter(|(_, order)| order.ends_on_sight())
                    .map(|(unit, _)| unit)
                    .collect::<Vec<_>>()
            })
            .collect();
        if watchers.is_empty() {
            return;
        }
        self.units().sort_by_serial(&mut watchers);
        let sighted = self.serials_seen_by(&watchers, |me, them| self.is_enemy(me, them));
        for (watcher, enemies) in sighted {
            // Ended with its group already.
            let Some(order) = ledger::order_of(&self.ecs, watcher) else {
                continue;
            };
            let known = ledger::seen_of(&self.ecs, watcher);
            if enemies.iter().all(|enemy| known.contains(enemy)) {
                continue;
            }
            let (Some(player), Some(at)) = (
                self.units().unit_owner(watcher),
                self.units().unit_position(watcher),
            ) else {
                continue;
            };
            let group: Vec<hecs::Entity> = self
                .units()
                .player_units(player)
                .into_iter()
                .filter(|&u| {
                    self.units().unit_position(u) == Some(at)
                        && ledger::order_of(&self.ecs, u).as_ref() == Some(&order)
                })
                .collect();
            for &u in &group {
                ledger::end_order(&self.ecs, u);
            }
            events.push(Event::OrderOver {
                player,
                units: group,
                at,
                why: OrderEnd::EnemySeen,
            });
        }
    }

    /// End the orders of units that are moved by hand or by a spell. An escort whose charge moves
    /// with it keeps its order.
    pub(crate) fn end_orders_of_moved(&mut self, units: &[hecs::Entity]) {
        let moved: Vec<Serial> = units.iter().map(|&u| self.units().serial_of(u)).collect();
        for &u in units {
            if matches!(ledger::order_of(&self.ecs, u),
                Some(Order::Escort(charge)) if moved.contains(&charge))
            {
                continue;
            }
            ledger::end_order(&self.ecs, u);
        }
    }

    /// The units that move when `group` walks from `from`: the group, and the escorts of its
    /// units that stand with it.
    pub(crate) fn with_escorts(&self, group: &[hecs::Entity], from: Position) -> Vec<hecs::Entity> {
        let mut moving = group.to_vec();
        let Some(owner) = group
            .first()
            .and_then(|&u| self.units().unit_owner(u))
            .and_then(|p| self.player_entity(p))
        else {
            return moving;
        };
        let charges: Vec<Serial> = group.iter().map(|&u| self.units().serial_of(u)).collect();
        let Ok(ledger) = self.ecs.get::<&ArmyLedger>(owner) else {
            return moving;
        };
        for (unit, order) in ledger.ordered() {
            if matches!(order, Order::Escort(charge) if charges.contains(charge))
                && !moving.contains(&unit)
                && self.units().unit_position(unit) == Some(from)
            {
                moving.push(unit);
            }
        }
        moving
    }

    /// The units of `player` with a standing order walk on, oldest first: what the end of the
    /// turn does.
    pub(crate) fn walk_orders(&mut self, player: PlayerId, rng: &mut GameRng) -> Vec<Event> {
        let ordered: Vec<hecs::Entity> = self
            .player_entity(player)
            .and_then(|p| self.ecs.get::<&ArmyLedger>(p).ok())
            .map(|ledger| ledger.ordered().map(|(unit, _)| unit).collect())
            .unwrap_or_default();
        self.follow_orders(&ordered, Walk::TurnEnd, rng)
    }

    /// Advance each listed unit along its standing order as far as its moves allow.
    fn follow_orders(
        &mut self,
        units: &[hecs::Entity],
        walk: Walk,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        // Units sharing a tile and an order walk together.
        let mut groups: Vec<(Order, Vec<hecs::Entity>)> = Vec::new();
        for &u in units {
            let Some(pos) = self.units().unit_position(u) else {
                continue;
            };
            let Some(order) = ledger::order_of(&self.ecs, u) else {
                continue;
            };
            // A road is worked on at the start of the turn.
            if matches!(order, Order::Road(_)) {
                continue;
            }
            match groups
                .iter_mut()
                .find(|(o, g)| *o == order && self.units().unit_position(g[0]) == Some(pos))
            {
                Some((_, g)) => g.push(u),
                None => groups.push((order, vec![u])),
            }
        }
        // Escorts go last, so that those left behind follow where their charges end up.
        groups.sort_by_key(|(order, _)| matches!(order, Order::Escort(_)));
        // A group whose next square is full waits for the others to move (they may be the ones
        // standing on it) and tries again; what is still blocked when nothing more moves has its
        // order end with the reason, as before. So a column of big stacks walking one way clears
        // from its head, whatever the order of their serial numbers.
        let mut waiting: Vec<Vec<hecs::Entity>> = groups.into_iter().map(|(_, g)| g).collect();
        loop {
            let mut blocked = Vec::new();
            let mut moved = false;
            for group in waiting {
                let (went, stuck) = self.follow_group(&group, walk, false, &mut events, rng);
                moved |= went;
                if stuck {
                    blocked.push(group);
                }
            }
            if blocked.is_empty() || !moved {
                for group in blocked {
                    self.follow_group(&group, walk, true, &mut events, rng);
                }
                break;
            }
            waiting = blocked;
        }
        events
    }

    /// One group of units that walk together, as far as its moves and its order go. Returns
    /// whether it moved, and whether it stopped for lack of room on the next square
    /// (`StackFull`, `NoRoom`): unless `last`, such a group keeps its order for another try;
    /// with `last` its order ends with the reason (an escort waits instead).
    fn follow_group(
        &mut self,
        group: &[hecs::Entity],
        walk: Walk,
        last: bool,
        events: &mut Vec<Event>,
        rng: &mut GameRng,
    ) -> (bool, bool) {
        let (mut went, mut stuck) = (false, false);
        // A scout takes one new patch for each step it makes, so that it cannot choose for ever.
        let mut chose = false;
        // The lead unit of a group may have been lost to a battle that an earlier group's
        // move brought on: then the group has no order to follow any more.
        while let Some(order) = ledger::order_of(&self.ecs, group[0]) {
            // A stack that settled in a vacant lair on the last step is gone from the map.
            let Some(from) = self.units().unit_position(group[0]) else {
                self.end_group(group);
                break;
            };
            let owner = self.units().unit_owner(group[0]);
            let escort = matches!(order, Order::Escort(_));
            let may_fight = walk == Walk::Given && matches!(order, Order::GoTo(_));
            let leg = match owner {
                Some(owner) => self.next_leg(owner, group, from, order, &mut chose),
                None => Leg::Done,
            };
            let next = match leg {
                Leg::Step(next) => next,
                Leg::Wait => break,
                Leg::Done => {
                    self.end_group(group);
                    break;
                }
                Leg::Over(why) => {
                    self.end_group(group);
                    if let Some(player) = owner {
                        events.push(Event::OrderOver {
                            player,
                            units: group.to_vec(),
                            at: from,
                            why,
                        });
                    }
                    break;
                }
                Leg::Meld => {
                    self.end_group(group);
                    match self.meld(group, rng) {
                        Ok(done) => events.extend(done),
                        Err(e) => events.push(Event::Rejected(Rejection::Meld(e))),
                    }
                    break;
                }
                Leg::Settle(name) => {
                    self.end_group(group);
                    if let Some(&settler) = group.iter().find(|&&u| self.units().unit_is_settler(u))
                    {
                        let name = name.unwrap_or_else(|| self.outpost_name(settler));
                        events.extend(self.settle_outpost(settler, name));
                    }
                    break;
                }
            };
            let mut moving = self.with_escorts(group, from);
            let mut step = self.order_step(&moving, next, may_fight);
            // An escort that cannot go where its charge goes is left behind.
            if matches!(step, Err(MoveError::Impassable)) && moving.len() > group.len() {
                moving = group.to_vec();
                step = self.order_step(&moving, next, may_fight);
            }
            match step {
                Ok(Entry::Free) => {
                    went = true;
                    chose = false;
                    for &u in group {
                        if let Some(order) = ledger::order_of(&self.ecs, u) {
                            ledger::advance_order(&self.ecs, u, order.stepped());
                        }
                    }
                    events.push(Event::UnitsMoved {
                        units: moving.clone(),
                        from,
                        to: next,
                    });
                    events.extend(self.collect_site(&moving, next, rng));
                    // A walk of several steps may pass what no longer shows at its end.
                    self.end_patrols_on_sight();
                    self.end_orders_on_sight(events);
                }
                Ok(Entry::Site(lair)) => {
                    went = true;
                    events.push(Event::SiteEncounter {
                        units: moving,
                        from,
                        to: next,
                        lair,
                    });
                    self.end_group(group);
                    break;
                }
                Ok(Entry::Hostile(defenders)) => {
                    went = true;
                    events.extend(self.hostile_entry(&moving, from, next, defenders, rng));
                    self.end_group(group);
                    break;
                }
                Err(MoveError::NoMovesLeft) => break,
                Err(MoveError::StackFull | MoveError::NoRoom) if !last => {
                    stuck = true;
                    break;
                }
                Err(MoveError::StackFull | MoveError::NoRoom) if escort => break,
                Err(why) => {
                    // The order is over, and the caller is told why.
                    if let Some(player) = owner {
                        events.push(Event::OrderFailed {
                            player,
                            units: group.to_vec(),
                            at: from,
                            to: next,
                            why,
                        });
                    }
                    self.end_group(group);
                    break;
                }
            }
        }
        (went, stuck)
    }

    fn end_group(&mut self, group: &[hecs::Entity]) {
        for &u in group {
            ledger::end_order(&self.ecs, u);
        }
    }

    /// One step of a walk for an order: unless the player is making the move, it starts no fight.
    fn order_step(
        &mut self,
        units: &[hecs::Entity],
        to: Position,
        may_fight: bool,
    ) -> Result<Entry, MoveError> {
        match self.move_stack(units, to) {
            Ok(Entry::Hostile(_) | Entry::Site(_)) if !may_fight => Err(MoveError::WouldFight),
            step => step,
        }
    }

    /// What the group, which stands on `from`, does next for `order`.
    fn next_leg(
        &mut self,
        owner: PlayerId,
        group: &[hecs::Entity],
        from: Position,
        order: Order,
        chose: &mut bool,
    ) -> Leg {
        match &order {
            Order::GoTo(path) => path.first().map_or(Leg::Done, |&next| Leg::Step(next)),
            Order::Road(_) => Leg::Wait,
            Order::Meld(trip) => {
                if self.check_meld_order(owner, group, trip.to).is_err() {
                    Leg::Over(OrderEnd::TargetGone)
                } else if from == trip.to {
                    Leg::Meld
                } else {
                    let trip = trip.clone();
                    self.trip_leg(group, from, order, trip)
                }
            }
            Order::Settle { trip, name } => {
                if !PlayerView::new(self, owner).can_found_city(trip.to)
                    || !group.iter().any(|&u| self.units().unit_is_settler(u))
                {
                    Leg::Over(OrderEnd::TargetGone)
                } else if from == trip.to {
                    Leg::Settle(name.clone())
                } else {
                    let trip = trip.clone();
                    self.trip_leg(group, from, order, trip)
                }
            }
            Order::Reinforce(trip) => {
                if !self.own_city_at(owner, trip.to) {
                    Leg::Over(OrderEnd::TargetGone)
                } else if self.stands_with(from, trip.to) {
                    Leg::Done
                } else {
                    let trip = trip.clone();
                    self.trip_leg(group, from, order, trip)
                }
            }
            Order::Escort(charge) => {
                let there = self
                    .player_entity(owner)
                    .and_then(|p| self.ecs.get::<&ArmyLedger>(p).ok()?.unit(*charge))
                    .and_then(|u| self.units().unit_position(u));
                match there {
                    None => Leg::Over(OrderEnd::TargetGone),
                    Some(there) if self.stands_with(from, there) => Leg::Wait,
                    Some(there) => self
                        .find_path(group, there)
                        .and_then(|path| path.first().copied())
                        .map_or(Leg::Over(OrderEnd::NoWay), Leg::Step),
                }
            }
            Order::Scout(scouting) => {
                let trip = match &scouting.trip {
                    Some(trip) if from != trip.to && self.patch_is_new(owner, group, trip.to) => {
                        trip.clone()
                    }
                    // A spent unit chooses when it can walk again.
                    _ if *chose || self.walker_is_spent(group) => return Leg::Wait,
                    _ => {
                        let Some(to) = self.next_patch(owner, group, from, scouting) else {
                            return Leg::Over(OrderEnd::NothingLeft);
                        };
                        let Some(path) = self.order_path(group, to) else {
                            return Leg::Over(OrderEnd::NoWay);
                        };
                        *chose = true;
                        Trip { to, path }
                    }
                };
                self.trip_leg(group, from, order, trip)
            }
        }
    }

    /// The next step of an errand's trip, found again when the units are no longer where the way
    /// begins; the order is brought up to date.
    fn trip_leg(
        &mut self,
        group: &[hecs::Entity],
        from: Position,
        order: Order,
        mut trip: Trip,
    ) -> Leg {
        let Some(width) = self.map().map(|m| m.width()) else {
            return Leg::Over(OrderEnd::NoWay);
        };
        if !trip
            .path
            .first()
            .is_some_and(|&next| adjacent(width, from, next))
        {
            match self.order_path(group, trip.to) {
                Some(path) => trip.path = path,
                None => return Leg::Over(OrderEnd::NoWay),
            }
        }
        let Some(&next) = trip.path.first() else {
            return Leg::Over(OrderEnd::NoWay);
        };
        let order = order.with_trip(trip);
        for &u in group {
            ledger::advance_order(&self.ecs, u, order.clone());
        }
        Leg::Step(next)
    }

    /// Whether one of the units that walk for themselves has no moves left: those that ride a
    /// ship of the group do not.
    fn walker_is_spent(&self, group: &[hecs::Entity]) -> bool {
        let economy = self.economy();
        let sails = group.iter().any(|&u| economy.is_ship(u));
        let flies = economy.stack_traits(group).flies;
        group.iter().any(|&u| {
            !(sails && economy.needs_ship(u, flies)) && self.units().unit_moves_left(u) == 0
        })
    }

    /// Whether the patch at `to` still has ground the units have not explored.
    fn patch_is_new(&self, owner: PlayerId, group: &[hecs::Entity], to: Position) -> bool {
        PlayerView::new(self, owner).unexplored_near(to, self.group_sight(group)) > 0
    }

    fn group_sight(&self, group: &[hecs::Entity]) -> f4x_def::half_int::HalfInt {
        group
            .iter()
            .map(|&u| self.units().unit_sight(u))
            .max()
            .unwrap_or(f4x_def::half_int::HalfInt::ZERO)
    }

    /// The patch a scouting group walks to next: away from those its owner's other scouts are
    /// walking to.
    fn next_patch(
        &self,
        owner: PlayerId,
        group: &[hecs::Entity],
        from: Position,
        scouting: &Scouting,
    ) -> Option<Position> {
        let mut avoid = scouting.avoid.clone();
        if let Some(ledger) = self
            .player_entity(owner)
            .and_then(|p| self.ecs.get::<&ArmyLedger>(p).ok())
        {
            avoid.extend(ledger.ordered().filter_map(|(unit, order)| match order {
                Order::Scout(other) if !group.contains(&unit) => other.trip.as_ref().map(|t| t.to),
                _ => None,
            }));
        }
        PlayerView::new(self, owner).scout_target(group, from, &avoid, &scouting.favour)
    }

    /// The name of a city a settler founds when none is given.
    fn outpost_name(&self, settler: hecs::Entity) -> String {
        let used = self.economy().cities().len();
        self.units()
            .unit_def(settler)
            .and_then(|d| d.race.as_ref())
            .copied()
            .map(|r| self.city_name_for(r, used))
            .unwrap_or_else(|| format!("Outpost {}", used + 1))
    }
}
