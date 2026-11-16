//! What each unit is for. A unit outside a city has one role at a time; the audit at the end of
//! the turn finds the ones with none (adrift: no list, no order, no city) and sends them home,
//! and the ones with two (a unit in two role lists), so nothing is left standing on a tower or a
//! road with no purpose.
//!
//! The roles live in the lists of [`Ai`] (`scouts`, `raiders`, `marching`, ...), and [`derive`]
//! reads them back as one table. The design is in `docs/ai-design.md`.

use crate::view::{distance, is_fighter};
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
    /// A settler or the escort of a colony.
    Settle,
    /// Stranded where no own city can be reached: waits on a shore tile for a ship
    /// (`pickup.rs`).
    AwaitTransport,
    /// Has an order (on its way somewhere) but belongs to no list.
    Ordered,
}

/// A unit and the roles the lists give it.
#[derive(Debug, Clone)]
pub struct Held {
    pub unit: hecs::Entity,
    pub roles: Vec<Role>,
}

/// The roles of the wizard's soldiers (not ships, not units that cannot fight).
pub fn derive(ai: &Ai, view: &PlayerView) -> Vec<Held> {
    let mut out = Vec::new();
    for unit in view.my_units() {
        if !is_fighter(view, unit) {
            continue;
        }
        let serial = view.serial_of(unit);
        let mut roles = Vec::new();
        // A planar scout and a sea party are scouts with a way to go: one role, the narrower.
        let planar = ai.planar == Some(serial);
        let sea = ai.sea_party.contains(&serial);
        let listed: [(bool, Role); 7] = [
            (ai.scouts.contains(&serial) && !planar && !sea, Role::Scout),
            (planar, Role::PlanarScout),
            (ai.raiders.contains(&serial), Role::Raid),
            (ai.muster.contains(&serial), Role::Muster),
            (ai.marching.contains(&serial), Role::March),
            (sea, Role::SeaParty),
            (
                ai.ferries.iter().any(|f| f.group.contains(&serial)),
                Role::Voyage,
            ),
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
            if in_city {
                roles.push(Role::Garrison);
            } else if view.unit_has_order(unit) {
                roles.push(Role::Ordered);
            }
        }
        out.push(Held { unit, roles });
    }
    out
}

/// The units with no role at all, and how many have more than one.
pub fn adrift_and_doubled(held: &[Held]) -> (Vec<hecs::Entity>, u32) {
    let adrift = held
        .iter()
        .filter(|h| h.roles.is_empty())
        .map(|h| h.unit)
        .collect();
    let doubled = held.iter().filter(|h| h.roles.len() > 1).count() as u32;
    (adrift, doubled)
}

/// The end-of-turn check: every soldier has a role. A soldier with none (a lone unit, or a party
/// too small for any task, left on a tower or a road) walks to the nearest own city that can be
/// reached and has room. The audit is counted in an [`Event::RolesAudited`].
pub fn audit(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let (adrift, doubled) = {
        let view = PlayerView::new(game, me);
        adrift_and_doubled(&derive(ai, &view))
    };
    if adrift.is_empty() && doubled == 0 {
        return;
    }
    events.push(Event::RolesAudited {
        player: me,
        adrift: adrift.len() as u32,
        doubled,
    });
    for group in send_home(ai, game, &adrift, events) {
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
        let (adrift, doubled) = adrift_and_doubled(&held);
        assert!(adrift.is_empty(), "{held:?}");
        assert_eq!(doubled, 0);
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
        assert!(adrift_and_doubled(&held).0.is_empty());
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
        let (adrift, doubled) = adrift_and_doubled(&derive(&ai, &view));
        assert!(adrift.is_empty());
        assert_eq!(doubled, 1);
        Ok(())
    }
}
