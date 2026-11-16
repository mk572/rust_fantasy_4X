//! What each unit is for. Every unit has one role at a time; the audit at the end of the turn goes
//! through all of them, finds the soldiers with none (adrift: no list, no order, no city) and
//! sends them home, counts the ships and other units with none, and the ones with two (a unit in
//! two role lists), so nothing is left standing on a tower or a road with no purpose.
//!
//! The roles live in the lists of [`Ai`] (`scouts`, `raiders`, `marching`, ...), and [`derive`]
//! reads them back as one table. The design is in `docs/ai-design.md`.

use crate::view::{distance, is_fighter, is_ship};
use crate::Ai;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

/// What a unit is doing, as far as the AI's lists say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// A settler, or the settler and ship of a colony.
    Settle,
    /// A ship on its way to fetch stranded soldiers (`pickup.rs`).
    Pickup,
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

impl Role {
    /// Which role wins when a unit is found in two lists: the lower number. A settler's task and
    /// a ship's voyage come first, the fall-backs last.
    fn priority(self) -> u8 {
        match self {
            Role::Settle => 0,
            Role::Voyage => 1,
            Role::SeaParty => 2,
            Role::PlanarScout => 3,
            Role::Pickup => 4,
            Role::AwaitTransport => 5,
            Role::Raid => 6,
            Role::March => 7,
            Role::Muster => 8,
            Role::Scout => 9,
            Role::Return => 10,
            Role::Ordered => 11,
            Role::Garrison => 12,
            Role::Docked => 13,
            Role::AwaitingOrders => 14,
        }
    }
}

/// Take the unit out of the list that gives it `role`: the losing role of a unit found in two.
fn release(ai: &mut Ai, serial: f4x_core::game::serial::Serial, role: Role) {
    match role {
        Role::Scout => {
            ai.scouts.retain(|s| *s != serial);
            ai.scout_targets.remove(&serial);
            ai.scout_last.remove(&serial);
        }
        Role::PlanarScout => {
            if ai.planar == Some(serial) {
                ai.planar = None;
            }
        }
        Role::Raid => {
            ai.raiders.retain(|s| *s != serial);
            ai.raid_targets.remove(&serial);
        }
        Role::Muster => ai.muster.retain(|s| *s != serial),
        Role::March => ai.marching.retain(|s| *s != serial),
        Role::SeaParty => ai.sea_party.retain(|s| *s != serial),
        Role::Voyage => {
            for f in &mut ai.ferries {
                f.group.retain(|s| *s != serial);
            }
        }
        Role::AwaitTransport => {
            ai.waiting.remove(&serial);
        }
        Role::Return => {
            ai.returning.remove(&serial);
        }
        // Derived from the world, or held by the unit's own task: nothing to take it out of.
        Role::Settle
        | Role::Pickup
        | Role::Ordered
        | Role::Garrison
        | Role::Docked
        | Role::AwaitingOrders => {}
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

/// A unit and the roles the lists give it.
#[derive(Debug, Clone)]
pub struct Held {
    pub unit: hecs::Entity,
    pub class: Class,
    pub roles: Vec<Role>,
}

/// The roles of every unit the wizard owns, soldiers, ships and the rest.
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
        let mut roles = Vec::new();
        // A planar scout and a sea party are scouts with a way to go: one role, the narrower.
        let planar = ai.planar == Some(serial);
        let sea = ai.sea_party.contains(&serial);
        let listed: [(bool, Role); 9] = [
            (ai.scouts.contains(&serial) && !planar && !sea, Role::Scout),
            (planar, Role::PlanarScout),
            (ai.raiders.contains(&serial), Role::Raid),
            (ai.muster.contains(&serial), Role::Muster),
            (ai.marching.contains(&serial), Role::March),
            (sea, Role::SeaParty),
            (
                ai.ferries
                    .iter()
                    .any(|f| f.ship == serial || f.group.contains(&serial)),
                Role::Voyage,
            ),
            (ai.pickups.iter().any(|p| p.ship == serial), Role::Pickup),
            (ai.settler_targets.contains_key(&serial), Role::Settle),
        ];
        roles.extend(listed.iter().filter(|(on, _)| *on).map(|&(_, r)| r));
        if ai.waiting.contains_key(&serial) {
            roles.push(Role::AwaitTransport);
        }
        if ai
            .colonists
            .is_some_and(|(a, b)| a == serial || b == serial)
        {
            roles.push(Role::Settle);
        }
        if roles.is_empty() {
            let in_city = view
                .unit_position(unit)
                .is_some_and(|p| view.city_at(p).is_some());
            let ordered = view.unit_has_order(unit);
            match class {
                Class::Soldier if in_city => roles.push(Role::Garrison),
                Class::Ship if in_city => roles.push(Role::Docked),
                _ if ordered && ai.returning.contains_key(&serial) => roles.push(Role::Return),
                _ if ordered => roles.push(Role::Ordered),
                _ => roles.push(Role::AwaitingOrders),
            }
        }
        roles.sort_by_key(|r| r.priority());
        out.push(Held { unit, class, roles });
    }
    out
}

/// What the table shows: the soldiers awaiting orders (adrift), the ships and other units
/// awaiting orders (no role of theirs is built yet), and the units found with more than one role.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub adrift: Vec<hecs::Entity>,
    pub unassigned: u32,
    pub doubled: u32,
}

/// Count the table.
pub fn tally(held: &[Held]) -> Tally {
    let mut t = Tally::default();
    for h in held {
        match (h.roles.as_slice(), h.class) {
            ([Role::AwaitingOrders], Class::Soldier) => t.adrift.push(h.unit),
            ([Role::AwaitingOrders], _) => t.unassigned += 1,
            ([_], _) => {}
            _ => t.doubled += 1,
        }
    }
    t
}

/// The end-of-turn check: every unit the wizard owns is looked at, each turn, and has a role. A
/// soldier with none (a lone unit, or a party too small for any task, left on a tower or a road)
/// walks to the nearest own city that can be reached and has room, and keeps that role (`Return`)
/// until it is there; if no city can be reached it waits on a shore for a ship. Ships and the
/// other units with no role are counted. The audit is counted in an [`Event::RolesAudited`].
pub fn audit(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let counted = {
        let view = PlayerView::new(game, me);
        // Soldiers that reached a city, or are gone, are no longer on their way home.
        let held = derive(ai, &view);
        ai.returning.retain(|s, _| {
            held.iter().any(|h| {
                view.serial_of(h.unit) == *s
                    && view
                        .unit_position(h.unit)
                        .is_some_and(|p| view.city_at(p).is_none())
            })
        });
        // Roles are exclusive: a unit in two lists keeps the one that comes first and is taken
        // out of the others.
        let losers: Vec<(hecs::Entity, Vec<Role>)> = held
            .iter()
            .filter(|h| h.roles.len() > 1)
            .map(|h| (h.unit, h.roles[1..].to_vec()))
            .collect();
        let found = tally(&held);
        (found, losers)
    };
    let (counted, losers) = counted;
    {
        let serials: Vec<(f4x_core::game::serial::Serial, Vec<Role>)> = {
            let view = PlayerView::new(game, me);
            losers
                .iter()
                .map(|(u, roles)| (view.serial_of(*u), roles.clone()))
                .collect()
        };
        for (serial, roles) in serials {
            for role in roles {
                release(ai, serial, role);
            }
        }
    }
    if counted.adrift.is_empty() && counted.unassigned == 0 && counted.doubled == 0 {
        return;
    }
    events.push(Event::RolesAudited {
        player: me,
        adrift: counted.adrift.len() as u32,
        doubled: counted.doubled,
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
                    ai.returning.insert(view.serial_of(u), (home, game.turn));
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
        let roles = held
            .iter()
            .find(|h| h.unit == lone[0])
            .map(|h| h.roles.clone());
        assert_eq!(roles, Some(vec![Role::Return]));
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
        ai.scouts.push(view.serial_of(scout[0]));
        let held = derive(&ai, &view);
        let counted = tally(&held);
        assert!(counted.adrift.is_empty(), "{held:?}");
        assert_eq!(counted.doubled, 0);
        let roles_of = |u: hecs::Entity| held.iter().find(|h| h.unit == u).map(|h| h.roles.clone());
        assert_eq!(roles_of(home[0]), Some(vec![Role::Garrison]));
        assert_eq!(roles_of(scout[0]), Some(vec![Role::Scout]));
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
        assert_eq!(ai.waiting.len(), 2, "{events:?}");
        let view = PlayerView::new(&g, P0);
        let held = derive(&ai, &view);
        for &u in &stranded {
            let roles = held.iter().find(|h| h.unit == u).map(|h| h.roles.clone());
            assert_eq!(roles, Some(vec![Role::AwaitTransport]));
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
            (ship_held.class, ship_held.roles),
            (Class::Ship, vec![Role::Docked])
        );
        let other_held = of(other).ok_or("the settlers are in the table")?;
        assert_eq!(other_held.class, Class::Other);
        assert_eq!(other_held.roles, vec![Role::AwaitingOrders]);
        assert_eq!(tally(&held).unassigned, 1);
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
        assert!(held.iter().all(|h| !h.roles.is_empty()), "{held:?}");
        Ok(())
    }

    /// Roles are exclusive: a unit found in two lists keeps the higher-priority role and is
    /// taken out of the other list.
    #[test]
    fn a_unit_found_in_two_lists_keeps_one_and_leaves_the_other() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let unit = spawn(&mut g, P0, at(8, 6), 104, 1)[0];
        let mut ai = Ai::new(P0);
        let serial = PlayerView::new(&g, P0).serial_of(unit);
        ai.raiders.push(serial);
        ai.marching.push(serial);
        audit(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(
            (ai.raiders.contains(&serial), ai.marching.contains(&serial)),
            (true, false),
            "the raid comes before the march"
        );
        let view = PlayerView::new(&g, P0);
        let held = derive(&ai, &view);
        let roles = held
            .iter()
            .find(|h| h.unit == unit)
            .map(|h| h.roles.clone());
        assert_eq!(roles, Some(vec![Role::Raid]));
        assert_eq!(tally(&held).doubled, 0);
        Ok(())
    }

    /// A unit in two role lists is counted as doubled.
    #[test]
    fn a_unit_in_two_lists_is_doubled() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let unit = spawn(&mut g, P0, at(8, 6), 104, 1);
        let mut ai = Ai::new(P0);
        let view = PlayerView::new(&g, P0);
        let serial = view.serial_of(unit[0]);
        ai.raiders.push(serial);
        ai.marching.push(serial);
        let counted = tally(&derive(&ai, &view));
        assert!(counted.adrift.is_empty());
        assert_eq!(counted.doubled, 1);
        Ok(())
    }
}
