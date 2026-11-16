//! Soldiers stranded where no own city can be reached on foot (a landing party that lost its
//! ship, a stack left on another land mass) are not left standing. They walk to a shore tile and
//! wait there (`Role::AwaitTransport`), a free ship sails to the sea square beside it, they step
//! aboard with an ordinary move, and the ship carries them to the nearest own port.
//!
//! The roles are held in `Ai::waiting` (unit, shore tile) and the voyages in `Ai::pickups`.

use crate::view::is_ship;
use crate::Ai;
use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

/// A ship on its way to fetch the soldiers waiting at `land`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Pickup {
    pub ship: Serial,
    /// The shore tile where the soldiers wait.
    pub land: Position,
    /// The sea square beside it where the ship lies.
    pub sea: Position,
    /// The soldiers it came for.
    pub group: Vec<Serial>,
}

/// A sea square beside `land` that nobody hostile holds.
fn sea_beside(view: &PlayerView, land: Position) -> Option<Position> {
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            let Some(sea) = view.shift(land, dx, dy) else {
                continue;
            };
            if view.terrain_class(sea) == Some(TerrainClass::Ocean)
                && view.hostile_at(sea).is_none()
            {
                return Some(sea);
            }
        }
    }
    None
}

/// Put a group of stranded soldiers on the waiting list and send them to the shore. Whether a
/// shore is known on their land mass.
pub fn wait_at_shore(ai: &mut Ai, game: &mut Game, units: &[hecs::Entity]) -> bool {
    let me = ai.player;
    let (tile, serials) = {
        let view = PlayerView::new(game, me);
        let Some(&first) = units.first() else {
            return false;
        };
        let Some(pos) = view.unit_position(first) else {
            return false;
        };
        let Some((land, _)) = crate::naval::landing(&view, pos) else {
            return false;
        };
        (
            land,
            units
                .iter()
                .map(|&u| view.serial_of(u))
                .collect::<Vec<Serial>>(),
        )
    };
    for s in serials {
        ai.waiting.insert(s, tile);
    }
    true
}

/// One turn of the waiting soldiers and the ships that fetch them.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    // Forget soldiers that are gone or have reached a city, and voyages whose ship is lost.
    {
        let view = PlayerView::new(game, me);
        let alive: Vec<(Serial, Option<Position>)> = view
            .my_units()
            .into_iter()
            .map(|u| (view.serial_of(u), view.unit_position(u)))
            .collect();
        ai.waiting.retain(|s, _| {
            alive
                .iter()
                .find(|(a, _)| a == s)
                .is_some_and(|(_, p)| p.is_some_and(|p| view.city_at(p).is_none()))
        });
        ai.pickups
            .retain(|p| alive.iter().any(|(a, _)| *a == p.ship));
        for p in &mut ai.pickups {
            p.group.retain(|g| alive.iter().any(|(a, _)| a == g));
        }
        ai.pickups.retain(|p| !p.group.is_empty());
    }
    if ai.waiting.is_empty() && ai.pickups.is_empty() {
        return;
    }
    walk_to_shore(ai, game, events);
    assign_ships(ai, game);
    for pickup in ai.pickups.clone() {
        advance(ai, game, &pickup, events);
    }
}

/// The waiting soldiers that stand elsewhere and have no order walk to their shore tile.
fn walk_to_shore(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let mut sends: Vec<(Position, Position, Vec<hecs::Entity>)> = Vec::new();
    {
        let view = PlayerView::new(game, me);
        let aboard: Vec<Serial> = ai.pickups.iter().flat_map(|p| p.group.clone()).collect();
        for u in view.my_units() {
            let serial = view.serial_of(u);
            let Some(&tile) = ai.waiting.get(&serial) else {
                continue;
            };
            let Some(pos) = view.unit_position(u) else {
                continue;
            };
            if pos == tile
                || view.unit_has_order(u)
                || (aboard.contains(&serial) && is_aboard(&view, u))
            {
                continue;
            }
            match sends.iter_mut().find(|(p, t, _)| *p == pos && *t == tile) {
                Some((_, _, list)) => list.push(u),
                None => sends.push((pos, tile, vec![u])),
            }
        }
    }
    sends.sort_by_key(|(p, t, _)| (p.plane as u8, p.y, p.x, t.y, t.x));
    for (_, tile, units) in sends {
        let _ = ai.send(game, Command::GoTo { units, to: tile }, events);
    }
}

/// Whether the unit rides a ship at its square.
fn is_aboard(view: &PlayerView, unit: hecs::Entity) -> bool {
    view.unit_position(unit).is_some_and(|p| {
        view.my_units().into_iter().any(|s| {
            is_ship(view, s)
                && view.unit_position(s) == Some(p)
                && view.passengers_of(s).contains(&unit)
        })
    })
}

/// Every group waiting on its shore tile with no ship on the way is given the nearest free ship.
fn assign_ships(ai: &mut Ai, game: &mut Game) {
    let me = ai.player;
    let mut new: Vec<Pickup> = Vec::new();
    {
        let view = PlayerView::new(game, me);
        let busy: Vec<Serial> = ai
            .ferries
            .iter()
            .map(|f| f.ship)
            .chain(ai.pickups.iter().map(|p| p.ship))
            .collect();
        let mut free: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| is_ship(&view, u))
            .filter(|&u| {
                let s = view.serial_of(u);
                !busy.contains(&s) && !ai.scouts.contains(&s) && view.passengers_of(u).is_empty()
            })
            .collect();
        view.sort_by_serial(&mut free);
        let taken: Vec<Serial> = ai.pickups.iter().flat_map(|p| p.group.clone()).collect();
        let mut tiles: Vec<Position> = ai.waiting.values().copied().collect();
        tiles.sort_by_key(|p| (p.plane as u8, p.y, p.x));
        tiles.dedup();
        for tile in tiles {
            let group: Vec<hecs::Entity> = view
                .my_units()
                .into_iter()
                .filter(|&u| {
                    let s = view.serial_of(u);
                    ai.waiting.get(&s) == Some(&tile)
                        && view.unit_position(u) == Some(tile)
                        && !taken.contains(&s)
                })
                .collect();
            if group.is_empty() {
                continue;
            }
            let Some(sea) = sea_beside(&view, tile) else {
                continue;
            };
            let best = free
                .iter()
                .copied()
                .filter(|&s| view.unit_position(s).is_some_and(|p| p.plane == tile.plane))
                .min_by_key(|&s| {
                    view.unit_position(s)
                        .and_then(|p| view.distance(p, sea))
                        .unwrap_or(u32::MAX)
                });
            let Some(ship) = best else {
                continue;
            };
            free.retain(|&s| s != ship);
            let room = game.economy().ship_room(ship) as usize;
            let group: Vec<Serial> = group
                .iter()
                .take(room.max(1))
                .map(|&u| view.serial_of(u))
                .collect();
            new.push(Pickup {
                ship: view.serial_of(ship),
                land: tile,
                sea,
                group,
            });
        }
    }
    ai.pickups.extend(new);
}

/// One step of a voyage: sail to the sea square, take the soldiers aboard, sail to a port.
fn advance(ai: &mut Ai, game: &mut Game, pickup: &Pickup, events: &mut Vec<Event>) {
    let me = ai.player;
    let (ship, ship_pos, aboard, on_land, port, idle) = {
        let view = PlayerView::new(game, me);
        let Some(ship) = view
            .my_units()
            .into_iter()
            .find(|&u| view.serial_of(u) == pickup.ship)
        else {
            return;
        };
        let ship_pos = view.unit_position(ship);
        let group: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| pickup.group.contains(&view.serial_of(u)))
            .collect();
        let aboard: Vec<hecs::Entity> = group
            .iter()
            .copied()
            .filter(|&u| view.unit_position(u) == ship_pos && ship_pos != Some(pickup.land))
            .collect();
        let on_land: Vec<hecs::Entity> = group
            .iter()
            .copied()
            .filter(|&u| view.unit_position(u) == Some(pickup.land))
            .collect();
        let port = ship_pos.and_then(|here| {
            view.my_cities()
                .into_iter()
                .map(|(_, p)| p)
                .filter(|p| p.plane == here.plane)
                .filter(|&p| view.can_reach(&[ship], p))
                .min_by_key(|&p| (view.distance(here, p).unwrap_or(u32::MAX), p.y, p.x))
        });
        (
            ship,
            ship_pos,
            (aboard, group.len()),
            on_land,
            port,
            !view.unit_has_order(ship),
        )
    };
    let Some(ship_pos) = ship_pos else {
        return;
    };
    let (aboard, total) = aboard;
    let everyone = aboard.len() == total;
    let in_port = PlayerView::new(game, me).city_at(ship_pos).is_some();
    if everyone && in_port {
        // Delivered: the soldiers are in a city, which is where they belong.
        for s in &pickup.group {
            ai.waiting.remove(s);
        }
        ai.pickups.retain(|p| p.ship != pickup.ship);
        return;
    }
    if everyone {
        if let (true, Some(port)) = (idle, port) {
            let _ = ai.send(
                game,
                Command::GoTo {
                    units: vec![ship],
                    to: port,
                },
                events,
            );
        }
        return;
    }
    if ship_pos == pickup.sea {
        if !on_land.is_empty() {
            let _ = ai.send(
                game,
                Command::GoTo {
                    units: on_land,
                    to: pickup.sea,
                },
                events,
            );
        }
        return;
    }
    if idle {
        let _ = ai.send(
            game,
            Command::GoTo {
                units: vec![ship],
                to: pickup.sea,
            },
            events,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, ENEMY_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn shore(g: &Game) -> Result<Position, Box<dyn std::error::Error>> {
        let view = PlayerView::new(g, P0);
        let (land, _) = crate::naval::landing(&view, at(ENEMY_CITY.0, ENEMY_CITY.1))
            .ok_or("a shore tile near the far city")?;
        Ok(land)
    }

    /// Soldiers stranded on a shore far from any own city, with a free ship in port: the ship
    /// comes, takes them aboard and brings them to the port.
    #[test]
    fn stranded_soldiers_are_fetched_by_a_ship_and_brought_to_a_port() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let land = shore(&g)?;
        let soldiers = spawn(&mut g, P0, land, 104 /* spearmen */, 3);
        let port = at(COAST_CITY.0, COAST_CITY.1);
        let ship = spawn(&mut g, P0, port, 36 /* galley */, 1)[0];
        g.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        assert!(wait_at_shore(&mut ai, &mut g, &soldiers));
        assert_eq!(ai.waiting.len(), 3);
        let mut events = Vec::new();
        for _ in 0..60 {
            run(&mut ai, &mut g, &mut events);
            g.apply(f4x_core::game::Command::EndTurn);
            g.balance_farmers(P0);
            if ai.waiting.is_empty() {
                break;
            }
        }
        let view = PlayerView::new(&g, P0);
        assert!(ai.waiting.is_empty(), "{:?}", ai.waiting);
        assert!(ai.pickups.is_empty(), "{:?}", ai.pickups);
        for &u in &soldiers {
            assert_eq!(view.unit_position(u), Some(port), "the soldiers are home");
        }
        assert!(view.unit_position(ship).is_some());
        Ok(())
    }

    /// With no ship the soldiers stay on the shore and wait; nothing is sent.
    #[test]
    fn without_a_free_ship_the_soldiers_wait_on_the_shore() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let land = shore(&g)?;
        let soldiers = spawn(&mut g, P0, land, 104, 2);
        g.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        wait_at_shore(&mut ai, &mut g, &soldiers);
        let mut events = Vec::new();
        for _ in 0..3 {
            run(&mut ai, &mut g, &mut events);
            g.apply(f4x_core::game::Command::EndTurn);
            g.balance_farmers(P0);
        }
        assert_eq!(ai.waiting.len(), 2);
        assert!(ai.pickups.is_empty());
        let view = PlayerView::new(&g, P0);
        for &u in &soldiers {
            let tile = ai.waiting.get(&view.serial_of(u)).copied();
            assert_eq!(view.unit_position(u), tile, "each waits on its shore tile");
        }
        Ok(())
    }
}
