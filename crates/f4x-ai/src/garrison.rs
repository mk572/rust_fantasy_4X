//! How many units stand in a city. More than a stack's worth (nine) add nothing to the defence,
//! so the excess is put to use: it shores up a city that holds too few, raids a site, tower or
//! node, or scouts. The city with the wizard's summoning circle keeps a slot free for the
//! creatures a summoning spell brings (`make_room`).
//!
//! Tests: below, on the shipped content, with a second city, a full circle and a summon.

use std::collections::HashMap;

use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

use crate::view::{garrison_wanted, is_fighter, unit_strength, ReachCache};
use crate::Ai;

/// Most units the AI keeps in a city: one stack.
pub const CITY_CAP: usize = f4x_core::unit::MAX_STACK;

/// The square the wizard's summoned creatures appear on.
fn circle(view: &PlayerView) -> Option<Position> {
    view.wizard().and_then(|w| w.summon_point())
}

/// How many units the AI lets stand on `at`: [`CITY_CAP`], less one at the summoning circle
/// when the game's limit leaves no more room there.
pub fn city_cap(view: &PlayerView, at: Position) -> usize {
    let limit = view.max_tile();
    let cap = CITY_CAP.min(limit);
    if circle(view) == Some(at) {
        cap.min(limit.saturating_sub(1))
    } else {
        cap
    }
}

/// Units that may be sent away: fighters with moves left and no task.
fn movable(ai: &Ai, view: &PlayerView, units: &[hecs::Entity]) -> Vec<hecs::Entity> {
    let busy = |serial| {
        ai.scouts.contains(&serial)
            || ai.raiders.contains(&serial)
            || ai.marching.contains(&serial)
            || ai.sea_party.contains(&serial)
            || ai.planar == Some(serial)
            || ai.ferries.iter().any(|f| f.group.contains(&serial))
    };
    units
        .iter()
        .copied()
        .filter(|&u| is_fighter(view, u))
        .filter(|&u| view.unit_moves_left(u) > 0 && !view.unit_has_order(u))
        .filter(|&u| !busy(view.serial_of(u)))
        .collect()
}

/// Send the units a city holds beyond [`city_cap`] where they are of use: to the nearest city
/// with fewer defenders than it wants, else on a raid (three or more on a square), else
/// scouting. The strongest leave; the weakest stay at home.
pub fn disperse(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let (sent, rest) = {
        let view = PlayerView::new(game, me);
        let cities: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
        let mut held: HashMap<Position, Vec<hecs::Entity>> = HashMap::new();
        for u in view.my_units() {
            if let Some(p) = view.unit_position(u).filter(|p| cities.contains(p)) {
                held.entry(p).or_default().push(u);
            }
        }
        // Cities that hold fewer defenders than they want, and the room left in each.
        let mut needy: Vec<(Position, usize, usize)> = cities
            .iter()
            .filter_map(|&c| {
                let units = held.get(&c).map_or(&[][..], Vec::as_slice);
                let fighters = units.iter().filter(|&&u| is_fighter(&view, u)).count();
                let wanted = garrison_wanted(&view, c);
                let room = city_cap(&view, c).saturating_sub(units.len());
                (fighters < wanted && room > 0).then(|| (c, wanted - fighters, room))
            })
            .collect();
        let mut over: Vec<Position> = cities
            .iter()
            .copied()
            .filter(|c| held.get(c).is_some_and(|u| u.len() > city_cap(&view, *c)))
            .collect();
        over.sort_by_key(|p| (p.plane as u8, p.y, p.x));
        let mut reached = ReachCache::default();
        let mut sent: Vec<(hecs::Entity, Position)> = Vec::new();
        let mut rest: Vec<hecs::Entity> = Vec::new();
        for city in over {
            let units = &held[&city];
            let excess = units.len() - city_cap(&view, city);
            let mut leaving = movable(ai, &view, units);
            leaving.sort_by(|&a, &b| {
                unit_strength(&view, b)
                    .total_cmp(&unit_strength(&view, a))
                    .then(view.serial_of(a).cmp(&view.serial_of(b)))
            });
            leaving.truncate(excess);
            for u in leaving {
                let mut near: Vec<usize> = (0..needy.len())
                    .filter(|&i| needy[i].0 != city && needy[i].2 > 0)
                    .collect();
                near.sort_by_key(|&i| {
                    (view.distance(city, needy[i].0), needy[i].0.y, needy[i].0.x)
                });
                let home = near.into_iter().find(|&i| {
                    view.distance(city, needy[i].0).is_some()
                        && reached.can_reach(&view, &[u], needy[i].0)
                });
                match home {
                    Some(i) => {
                        sent.push((u, needy[i].0));
                        needy[i].1 = needy[i].1.saturating_sub(1);
                        needy[i].2 -= 1;
                        if needy[i].1 == 0 {
                            needy.remove(i);
                        }
                    }
                    None => rest.push(u),
                }
            }
        }
        (sent, rest)
    };
    for (unit, to) in sent {
        ai.order(
            game,
            Command::GoTo {
                units: vec![unit],
                to,
            },
            events,
        );
    }
    if rest.is_empty() {
        return;
    }
    crate::site::deploy(ai, game, &rest, events);
    if ai.frontier_closed {
        return;
    }
    // What is left scouts: the scouting gives each a patch to walk to.
    let view = PlayerView::new(game, me);
    for u in rest {
        let serial = view.serial_of(u);
        if !ai.raiders.contains(&serial) && !ai.scouts.contains(&serial) {
            ai.scouts.push(serial);
        }
    }
}

/// Could a summoning be given room now: the circle's square is full, but a fighter on it can
/// still walk out? (When it has room already this is of no matter.)
pub fn can_make_room(view: &PlayerView) -> bool {
    let Some(at) = circle(view) else {
        return false;
    };
    let units = view.garrison(at);
    units.len() < view.max_tile()
        || units
            .iter()
            .any(|&u| is_fighter(view, u) && view.unit_moves_left(u) > 0)
}

/// Make sure the summoning circle's square has a free slot for the creatures: when it is full,
/// the weakest fighter that can walk leaves for the nearest other city with room. Returns
/// whether there is room now.
pub fn make_room(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) -> bool {
    let me = ai.player;
    let (at, plan) = {
        let view = PlayerView::new(game, me);
        let Some(at) = circle(&view) else {
            return true;
        };
        let units = view.garrison(at);
        if units.len() < view.max_tile() {
            return true;
        }
        let mut leaving = movable(ai, &view, &units);
        leaving.sort_by(|&a, &b| {
            unit_strength(&view, a)
                .total_cmp(&unit_strength(&view, b))
                .then(view.serial_of(a).cmp(&view.serial_of(b)))
        });
        let mut homes: Vec<(u32, Position)> = view
            .my_cities()
            .into_iter()
            .map(|(_, p)| p)
            .filter(|&p| p != at && view.garrison(p).len() < city_cap(&view, p))
            .filter_map(|p| Some((view.distance(at, p)?, p)))
            .collect();
        homes.sort_by_key(|&(d, p)| (d, p.y, p.x));
        let mut reached = ReachCache::default();
        let plan = leaving.into_iter().find_map(|u| {
            homes
                .iter()
                .find(|&&(_, p)| reached.can_reach(&view, &[u], p))
                .map(|&(_, p)| (u, p))
        });
        (at, plan)
    };
    let Some((unit, to)) = plan else {
        return false;
    };
    // A unit that cannot leave has not made room.
    let left = ai
        .send(
            game,
            Command::GoTo {
                units: vec![unit],
                to,
            },
            events,
        )
        .is_ok();
    let view = PlayerView::new(game, me);
    left && view.garrison(at).len() < view.max_tile()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, INLAND_CITY, P0};
    use f4x_core::wizard::fortress::{Fortress, SummonAt};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const SPEARMEN: u16 = 104; // high_men_spearmen

    /// Player 0 with a city on the coast and a second one inland, and its circle on the coast.
    fn two_cities() -> Result<Game, Box<dyn std::error::Error>> {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let events = g.apply(Command::FoundCity {
            owner: P0,
            pos: at(INLAND_CITY.0, INLAND_CITY.1),
            name: "Inland".into(),
            race: g.content().rules.races.id_numbered(8),
            population: 8,
        });
        assert!(!events.iter().any(|e| matches!(e, Event::Rejected(_))));
        let coast = at(COAST_CITY.0, COAST_CITY.1);
        let mut w = g.wizard_mut(P0).ok_or("player 0 is a wizard")?;
        w.put(Some(Fortress(coast)));
        w.put(Some(SummonAt(coast)));
        Ok(g)
    }

    fn held(g: &Game, pos: Position) -> usize {
        PlayerView::new(g, P0).garrison(pos).len()
    }

    #[test]
    fn a_city_with_nine_or_fewer_keeps_them() -> TestResult {
        let mut g = two_cities()?;
        let coast = at(COAST_CITY.0, COAST_CITY.1);
        spawn(&mut g, P0, coast, SPEARMEN, 9);
        let mut ai = Ai::new(P0);
        disperse(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(held(&g, coast), 9);
        assert!(ai.scouts.is_empty());
        Ok(())
    }

    #[test]
    fn the_excess_shores_up_a_weak_city_first_and_scouts_otherwise() -> TestResult {
        let mut g = two_cities()?;
        let coast = at(COAST_CITY.0, COAST_CITY.1);
        let inland = at(INLAND_CITY.0, INLAND_CITY.1);
        spawn(&mut g, P0, coast, SPEARMEN, 12);
        assert_eq!(held(&g, inland), 0);
        let mut ai = Ai::new(P0);
        disperse(&mut ai, &mut g, &mut Vec::new());
        // One defender is what the inland city wants: one of the three extra walks there, the
        // other two are given to the scouting.
        assert_eq!(
            held(&g, coast),
            11,
            "one unit left the coast for the inland city"
        );
        assert_eq!(ai.scouts.len(), 2);
        Ok(())
    }

    #[test]
    fn a_full_circle_gets_a_slot_by_sending_a_unit_to_another_city() -> TestResult {
        let mut g = two_cities()?;
        let coast = at(COAST_CITY.0, COAST_CITY.1);
        let limit = g.max_tile();
        spawn(&mut g, P0, coast, SPEARMEN, limit);
        let ai = Ai::new(P0);
        assert_eq!(held(&g, coast), limit);
        assert!(make_room(&ai, &mut g, &mut Vec::new()));
        assert_eq!(held(&g, coast), limit - 1);
        // With room already, nothing moves.
        assert!(make_room(&ai, &mut g, &mut Vec::new()));
        assert_eq!(held(&g, coast), limit - 1);
        Ok(())
    }
}
