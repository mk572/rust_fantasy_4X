//! Ship bridges (task 739): a ship parked on a sea square between two land masses, over which
//! the walkers of the wizard are routed as over a land square (`Bridge` in core). A strait of one
//! sea square needs no ferry: the units wait for the bridge and walk over it.
//!
//! A bridge ship holds the role `Role::Bridge` with its square as `to`; it is free of every
//! other job (no pickup or voyage takes it), sails to its square once and stays there, and is
//! released when nobody has asked for the bridge for [`KEEP`] turns. The units that wait for
//! it are tagged `Crossing::Bridge`: the pickup leaves them alone, and the tag is cleared the
//! turn a walk leads to their target.
//!
//! Which units are served: those tagged `Crossing::Sea` whose target lies on another land mass
//! than the one they stand on, with an explored sea square between the two (one that touches
//! both) and a free ship to send there. The others keep waiting for a ferry (`pickup.rs`).

use crate::roles::{Crossing, Role};
use crate::view::is_ship;
use crate::Ai;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::world::serial::Serial;
use f4x_core::world::{Command, Event};
use f4x_master::Game;
use std::collections::{HashMap, HashSet, VecDeque};

/// How many turns a bridge stays without anyone asking for it.
const KEEP: u32 = 40;

/// The explored land squares joined to `from` by land (not the sea), on its plane.
fn land_region(view: &PlayerView, from: Position) -> HashSet<Position> {
    let is_land = |p: Position| {
        view.terrain_class(p)
            .is_some_and(|t| t != TerrainClass::Ocean)
    };
    let mut seen = HashSet::new();
    if !is_land(from) {
        return seen;
    }
    let mut queue = VecDeque::from([from]);
    seen.insert(from);
    while let Some(at) = queue.pop_front() {
        for next in crate::military::neighbours(view, at) {
            if is_land(next) && seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    seen
}

/// The sea square nearest to `from` that touches both the land `from` stands on and the land of
/// `target`: the place for a bridge between the two. `None` when they are one land mass, or
/// no explored sea square touches both.
pub(crate) fn strait_between(
    view: &PlayerView,
    from: Position,
    target: Position,
) -> Option<Position> {
    if from.plane != target.plane {
        return None;
    }
    let here = land_region(view, from);
    if here.is_empty() || here.contains(&target) {
        return None;
    }
    let there = land_region(view, target);
    if there.is_empty() {
        return None;
    }
    let mut best: Option<(u32, Position)> = None;
    for &shore in &here {
        for sea in crate::military::neighbours(view, shore) {
            if view.terrain_class(sea) != Some(TerrainClass::Ocean) {
                continue;
            }
            let both = crate::military::neighbours(view, sea)
                .iter()
                .any(|p| there.contains(p));
            if !both {
                continue;
            }
            let d = crate::view::distance(view, from, sea).unwrap_or(u32::MAX);
            if best.is_none_or(|(b, p)| (d, sea.y, sea.x) < (b, p.y, p.x)) {
                best = Some((d, sea));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// One turn of the bridges: the ships on post, the units that wait for one, the ships still to
/// be sent.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    keep_posts(ai, game, events);
    serve(ai, game, events);
}

/// A ship of the wizard as the post-keeping looks at it.
struct Ship {
    unit: hecs::Entity,
    serial: Serial,
    at: Option<Position>,
    /// The square of its `Role::Bridge`, if it holds one.
    post: Option<Position>,
    /// Whether the game has it parked as a bridge.
    parked: bool,
    ordered: bool,
}

/// The bridge ships: sail to the square, park there, and go back to the fleet when nobody has
/// asked for the bridge for a while; a ship that is a bridge for the game but holds no role of
/// it (a role dropped, a saved game) is let off its post.
fn keep_posts(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let turn = game.round();
    let ships: Vec<Ship> = {
        let view = PlayerView::new(game, me);
        view.my_units()
            .into_iter()
            .filter(|&u| is_ship(&view, u))
            .map(|u| {
                let s = view.serial_of(u);
                Ship {
                    unit: u,
                    serial: s,
                    at: view.unit_position(u),
                    post: ai.roles.target_of(s, Role::Bridge),
                    parked: game.economy().is_bridge(u),
                    ordered: view.unit_has_order(u),
                }
            })
            .collect()
    };
    for Ship {
        unit: ship,
        serial,
        at,
        post,
        parked,
        ordered,
    } in ships
    {
        let held = ai.roles.is(serial, Role::Bridge);
        if !held {
            if parked {
                ai.order(
                    game,
                    Command::SetBridge {
                        unit: ship,
                        on: false,
                    },
                    events,
                );
            }
            continue;
        }
        let asked = ai.roles.get(serial).map_or(turn, |a| a.planned);
        let Some(post) = post else {
            ai.roles.release_any(serial);
            continue;
        };
        if turn.saturating_sub(asked) > KEEP {
            ai.roles.release_any(serial);
            if parked {
                ai.order(
                    game,
                    Command::SetBridge {
                        unit: ship,
                        on: false,
                    },
                    events,
                );
            }
            continue;
        }
        if at == Some(post) {
            if !parked {
                ai.order(
                    game,
                    Command::SetBridge {
                        unit: ship,
                        on: true,
                    },
                    events,
                );
            }
        } else if !ordered {
            let _ = ai.send(
                game,
                Command::GoTo {
                    units: vec![ship],
                    to: post,
                },
                events,
            );
        }
    }
}

/// The units tagged for the sea whose target a bridge could reach get one: a ship is sent to the
/// strait (or the one already there is asked for again) and the units wait for it as
/// `Crossing::Bridge`. The units waiting for a bridge that now leads to their target are let
/// walk (their tag is cleared); those whose strait is gone go back to waiting for a ferry.
fn serve(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let turn = game.round();
    let units: HashMap<Serial, hecs::Entity> = {
        let view = PlayerView::new(game, me);
        view.my_units()
            .into_iter()
            .map(|u| (view.serial_of(u), u))
            .collect()
    };
    // The units that wait for a bridge: let walk when a way leads over, else check the strait.
    for (s, to) in ai.roles.needing(Crossing::Bridge) {
        let Some(&u) = units.get(&s) else {
            continue;
        };
        let (walks, strait) = {
            let view = PlayerView::new(game, me);
            let from = view.unit_position(u);
            (
                game.find_path(&[u], to).is_some(),
                from.and_then(|f| strait_between(&view, f, to)),
            )
        };
        if walks {
            ai.roles.set_crossing(s, None);
        } else if !strait.is_some_and(|p| posted_at(ai, p)) {
            ai.roles.set_crossing(s, Some(Crossing::Sea));
        } else if let Some(p) = strait {
            renew(ai, p, turn);
        }
    }
    // The units that wait for a ship and could do with a bridge.
    for (s, to) in ai.roles.waiting_for_ship() {
        let Some(&u) = units.get(&s) else {
            continue;
        };
        let strait = {
            let view = PlayerView::new(game, me);
            view.unit_position(u)
                .and_then(|f| strait_between(&view, f, to))
        };
        let Some(strait) = strait else {
            continue;
        };
        if posted_at(ai, strait) || send_a_ship(ai, game, strait, events) {
            renew(ai, strait, turn);
            ai.roles.set_crossing(s, Some(Crossing::Bridge));
        }
    }
}

/// Whether a ship holds the post of a bridge at `square`.
fn posted_at(ai: &Ai, square: Position) -> bool {
    ai.roles
        .members(Role::Bridge)
        .into_iter()
        .any(|s| ai.roles.target_of(s, Role::Bridge) == Some(square))
}

/// Somebody asked for the bridge at `square` again: it is kept for another [`KEEP`] turns.
fn renew(ai: &mut Ai, square: Position, turn: u32) {
    for s in ai.roles.members(Role::Bridge) {
        if ai.roles.target_of(s, Role::Bridge) == Some(square) {
            ai.roles.touch(s, turn);
        }
    }
}

/// Send the free ship that is nearest to `square` there as its bridge. Whether one was sent.
fn send_a_ship(ai: &mut Ai, game: &mut Game, square: Position, events: &mut Vec<Event>) -> bool {
    let me = ai.player;
    let ship = {
        let view = PlayerView::new(game, me);
        let mut free: Vec<(u32, hecs::Entity)> = view
            .my_units()
            .into_iter()
            .filter(|&u| is_ship(&view, u))
            .filter(|&u| ai.roles.is(view.serial_of(u), Role::TransportFree))
            .filter(|&u| view.passengers_of(u).is_empty())
            .filter_map(|u| {
                let d = crate::view::distance(&view, view.unit_position(u)?, square)?;
                Some((d, u))
            })
            .collect();
        free.sort_by_key(|&(d, u)| (d, view.serial_of(u)));
        free.first().map(|&(_, u)| u)
    };
    let Some(ship) = ship else {
        return false;
    };
    if !Ai::room_at(game, square, 1) {
        return false;
    }
    let serial = PlayerView::new(game, me).serial_of(ship);
    let turn = game.round();
    if ai
        .roles
        .claim(serial, Role::Bridge, turn, Some(square))
        .is_err()
    {
        return false;
    }
    let at = PlayerView::new(game, me).unit_position(ship);
    if at != Some(square)
        && ai
            .send(
                game,
                Command::GoTo {
                    units: vec![ship],
                    to: square,
                },
                events,
            )
            .is_err()
    {
        ai.roles.release_any(serial);
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game_with_strait, spawn, COAST_CITY, P0};
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The sea column between the two halves of the land (`game_with_strait`).
    const STRAIT_X: u32 = 27;

    /// A wizard whose walker waits at the west for a target in the east, the way the dispatcher
    /// leaves it: tagged for the sea, with a free ship in the open sea in the north.
    fn waiting() -> Result<(Game, Ai, hecs::Entity, hecs::Entity), Box<dyn std::error::Error>> {
        let mut w = game_with_strait(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let walker = spawn(&mut w, P0, at(9, 6), 104 /* spearmen */, 1)[0];
        let ship = spawn(&mut w, P0, at(25, 1), 36 /* galley */, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.round());
        let s = PlayerView::new(&w, P0).serial_of(walker);
        let _ = ai
            .roles
            .claim(s, Role::Reinforce, w.round(), Some(at(30, 6)));
        ai.roles.set_crossing(s, Some(Crossing::Sea));
        Ok((w, ai, walker, ship))
    }

    /// The ship steps to `post` along its own path (the test spawns it within its moves).
    fn sail(w: &mut Game, ship: hecs::Entity, post: Position) -> TestResult {
        let _ = w.apply(Command::ClearOrders { units: vec![ship] });
        if PlayerView::new(w, P0).unit_position(ship) == Some(post) {
            return Ok(());
        }
        for step in w.find_path(&[ship], post).ok_or("a way to the post")? {
            w.move_stack(&[ship], step).map_err(|e| format!("{e:?}"))?;
        }
        Ok(())
    }

    #[test]
    fn the_strait_is_the_sea_square_that_touches_both_land_masses() -> TestResult {
        let (w, _, walker, _) = waiting()?;
        let view = PlayerView::new(&w, P0);
        let from = view.unit_position(walker).ok_or("a position")?;
        let strait = strait_between(&view, from, at(30, 6)).ok_or("a strait")?;
        assert_eq!(strait.x, STRAIT_X);
        assert_eq!(strait_between(&view, from, at(5, 5)), None, "one land mass");
        Ok(())
    }

    #[test]
    fn a_free_ship_is_sent_to_the_strait_and_parks_there_as_a_bridge() -> TestResult {
        let (mut w, mut ai, walker, ship) = waiting()?;
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        let view = PlayerView::new(&w, P0);
        let (s, ship_serial) = (view.serial_of(walker), view.serial_of(ship));
        assert!(ai.roles.is(ship_serial, Role::Bridge));
        assert_eq!(ai.roles.crossing_of(s), Some(Crossing::Bridge));
        assert!(
            ai.roles.waiting_for_ship().is_empty(),
            "no ferry is sent for it"
        );
        let post = ai
            .roles
            .target_of(ship_serial, Role::Bridge)
            .ok_or("a post")?;
        assert_eq!(post.x, STRAIT_X);
        // The ship arrives; the next turn parks it, and the walker has a way over.
        sail(&mut w, ship, post)?;
        run(&mut ai, &mut w, &mut events);
        assert!(w.economy().is_bridge(ship));
        assert!(w.find_path(&[walker], at(30, 6)).is_some());
        // Next turn the tag is cleared: its job walks it on.
        run(&mut ai, &mut w, &mut events);
        assert_eq!(ai.roles.crossing_of(s), None);
        Ok(())
    }

    /// The audit of the turn counts the ship that holds a bridge and the unit that waits for it,
    /// for the soak's counters.
    #[test]
    fn the_audit_counts_the_bridge_ship_and_the_unit_waiting_for_it() -> TestResult {
        let (mut w, mut ai, _, _) = waiting()?;
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        crate::roles::audit(&mut ai, &mut w, &mut events);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::RolesAudited {
                    bridges: 1,
                    waiting_bridge: 1,
                    ..
                }
            )),
            "{events:?}"
        );
        Ok(())
    }

    #[test]
    fn with_no_free_ship_the_unit_keeps_waiting_for_a_ferry() -> TestResult {
        let (mut w, mut ai, walker, ship) = waiting()?;
        let ship_serial = PlayerView::new(&w, P0).serial_of(ship);
        let _ = ai.roles.claim(ship_serial, Role::Scout, w.round(), None);
        run(&mut ai, &mut w, &mut Vec::new());
        let s = PlayerView::new(&w, P0).serial_of(walker);
        assert_eq!(ai.roles.crossing_of(s), Some(Crossing::Sea));
        assert!(!ai.roles.any(Role::Bridge));
        Ok(())
    }

    #[test]
    fn a_bridge_nobody_asks_for_goes_back_to_the_fleet() -> TestResult {
        let (mut w, mut ai, walker, ship) = waiting()?;
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        let ship_serial = PlayerView::new(&w, P0).serial_of(ship);
        let post = ai
            .roles
            .target_of(ship_serial, Role::Bridge)
            .ok_or("a post")?;
        sail(&mut w, ship, post)?;
        run(&mut ai, &mut w, &mut events);
        assert!(w.economy().is_bridge(ship));
        // The unit is gone: nobody asks any more.
        w.apply(Command::DismissUnit { unit: walker });
        w.set_round(w.round() + KEEP + 2);
        run(&mut ai, &mut w, &mut events);
        assert!(!ai.roles.is(ship_serial, Role::Bridge));
        run(&mut ai, &mut w, &mut events);
        assert!(!w.economy().is_bridge(ship));
        Ok(())
    }
}
