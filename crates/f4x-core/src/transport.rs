//! Ships carrying land units.
//!
//! A ship's `Transport` stat is how many land units it holds. Land units cannot enter the
//! sea alone: they board by moving onto a sea square where a friendly ship has room, or by
//! moving with a ship they share a square (or city) with. Passengers ride at no cost and
//! follow the ship; stepping from a ship onto land is an ordinary move and unloads them. A
//! passenger left at sea without a ship drowns.

use crate::city::{PlayerId, Position};
use crate::content::TerrainClass;
use crate::rules::{MovementType, Stat};
use crate::serial::Serial;
use crate::{Event, Game};

/// The ship a land unit is aboard, by creation number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Aboard(pub Serial);

impl Game {
    /// A unit that carries others: it sails and has room.
    pub fn is_ship(&self, unit: hecs::Entity) -> bool {
        self.unit_stat(unit, Stat::Transport) > 0
            && self
                .unit_def(unit)
                .is_some_and(|d| d.movement == MovementType::Sailing)
    }

    /// Whether the unit cannot cross the sea by itself: it walks and neither flies nor is
    /// carried through the air nor is non-corporeal.
    pub fn needs_ship(&self, unit: hecs::Entity, stack_flies: bool) -> bool {
        !stack_flies
            && self.unit_def(unit).is_some_and(|d| {
                d.movement == MovementType::Walking
                    && !self
                        .unit_abilities(unit)
                        .iter()
                        .any(|a| a == "flying" || a == "non_corporeal")
            })
    }

    /// The ship `unit` is aboard, if it still exists.
    pub fn carrier_of(&self, unit: hecs::Entity) -> Option<hecs::Entity> {
        let serial = self.world.get::<&Aboard>(unit).ok()?.0;
        self.entity_of_serial(serial)
    }

    /// Units aboard `ship`.
    pub fn passengers_of(&self, ship: hecs::Entity) -> Vec<hecs::Entity> {
        let serial = self.serial_of(ship);
        let mut out: Vec<_> = self
            .world
            .query::<&Aboard>()
            .iter()
            .filter(|(_, a)| a.0 == serial)
            .map(|(e, _)| e)
            .collect();
        self.sort_by_serial(&mut out);
        out
    }

    /// Free places on a ship.
    pub fn ship_room(&self, ship: hecs::Entity) -> u32 {
        let cap = self.unit_stat(ship, Stat::Transport).max(0) as u32;
        cap.saturating_sub(self.passengers_of(ship).len() as u32)
    }

    /// `owner`'s ships standing on `at`, oldest first.
    pub fn ships_at(&self, at: Position, owner: PlayerId) -> Vec<hecs::Entity> {
        self.units_at(at)
            .into_iter()
            .filter(|&u| self.unit_owner(u) == Some(owner) && self.is_ship(u))
            .collect()
    }

    /// Whether any of `owner`'s ships on `at` has a free place.
    pub fn ship_room_at(&self, at: Position, owner: PlayerId) -> bool {
        self.ships_at(at, owner)
            .into_iter()
            .any(|s| self.ship_room(s) > 0)
    }

    pub(crate) fn is_sea(&self, at: Position) -> bool {
        self.terrain_class_at(at) == Some(TerrainClass::Ocean)
    }

    /// Land units at sea with no ship under them are lost.
    pub(crate) fn drown_stranded(&mut self) -> Vec<Event> {
        let mut lost: Vec<(PlayerId, hecs::Entity)> = Vec::new();
        for unit in self.all_units() {
            let (Some(at), Some(owner)) = (self.unit_position(unit), self.unit_owner(unit)) else {
                continue;
            };
            if !self.is_sea(at) || !self.needs_ship(unit, false) {
                continue;
            }
            // Under a ship, or held by one standing on this square.
            let held = self
                .carrier_of(unit)
                .is_some_and(|s| self.unit_position(s) == Some(at));
            if !held {
                lost.push((owner, unit));
            }
        }
        let mut events = Vec::new();
        for (owner, unit) in lost {
            let _ = self.world.despawn(unit);
            events.push(Event::UnitsDrowned {
                player: owner,
                units: 1,
            });
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Grid, Map, Plane};
    use crate::movement::{Entry, MoveError};
    use crate::rules::Registry;
    use crate::testutil::content_with_units;
    use crate::units::MovesLeft;
    use crate::Command;

    const P0: PlayerId = PlayerId(0);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    /// 14x8: row 4 is grass x0-4, sea x5-9, grass x10-13; everything else grass.
    fn world() -> Game {
        let mut c = content_with_units();
        let mut units = c.rules.units.defs().to_vec();
        for u in units.iter_mut() {
            if u.id == "boat" {
                u.stats.insert(Stat::Transport, 2);
            }
        }
        c.rules.units = Registry::new(units);
        let (grass, ocean) = (
            c.terrains.id_of("grassland").unwrap(),
            c.terrains.id_of("ocean").unwrap(),
        );
        let build = || {
            let mut g = Grid::new(14, 8, grass);
            for x in 5..10 {
                g.set(x, 4, ocean);
            }
            g
        };
        let mut g = Game::new(2);
        g.content = c;
        g.map = Some(Map::new(build(), build()));
        g
    }

    fn unit(g: &mut Game, id: &str, pos: Position) -> hecs::Entity {
        let u = g.content.rules.units.id_of(id).unwrap();
        g.spawn_unit(P0, pos, u)
    }

    #[test]
    fn land_units_cannot_enter_the_sea_alone() {
        let mut g = world();
        let w = unit(&mut g, "walker", at(4, 4));
        assert_eq!(g.move_stack(&[w], at(5, 4)), Err(MoveError::Impassable));
    }

    #[test]
    fn a_unit_boards_a_waiting_ship_and_rides_with_it() {
        let mut g = world();
        let boat = unit(&mut g, "boat", at(5, 4));
        let w = unit(&mut g, "walker", at(4, 4));
        assert_eq!(g.move_stack(&[w], at(5, 4)), Ok(Entry::Free));
        assert_eq!(g.carrier_of(w), Some(boat));
        assert_eq!(g.ship_room(boat), 1);
        let moves = g.unit_moves_left(w);
        // The ship sails alone; its passenger comes along at no cost.
        g.move_stack(&[boat], at(6, 4)).unwrap();
        assert_eq!(g.unit_position(w), Some(at(6, 4)));
        assert_eq!(g.unit_moves_left(w), moves);
        assert_eq!(g.carrier_of(w), Some(boat));
        // Selected together, they sail as one stack too.
        g.move_stack(&[boat, w], at(7, 4)).unwrap();
        assert_eq!(g.unit_position(w), Some(at(7, 4)));
        assert_eq!(g.unit_moves_left(w), moves, "riding is free");
    }

    #[test]
    fn a_ship_holds_only_so_many() {
        let mut g = world();
        let boat = unit(&mut g, "boat", at(5, 4));
        let three: Vec<_> = (0..3).map(|_| unit(&mut g, "walker", at(4, 4))).collect();
        assert_eq!(g.move_stack(&three, at(5, 4)), Err(MoveError::NoRoom));
        g.move_stack(&three[..2], at(5, 4)).unwrap();
        assert_eq!(g.ship_room(boat), 0);
        assert_eq!(
            g.move_stack(&three[2..], at(5, 4)),
            Err(MoveError::Impassable),
            "a full ship is no place to step"
        );
    }

    #[test]
    fn units_load_in_port_and_unload_onto_land() {
        let mut g = world();
        g.apply(Command::FoundCity {
            owner: P0,
            pos: at(4, 4),
            name: "Port".into(),
            race: None,
            population: 3,
        });
        let boat = unit(&mut g, "boat", at(4, 4));
        let w = unit(&mut g, "walker", at(4, 4));
        // The stack sails out of the port: the walker boards for free.
        g.move_stack(&[boat, w], at(5, 4)).unwrap();
        assert_eq!(g.carrier_of(w), Some(boat));
        g.move_stack(&[boat], at(6, 4)).unwrap();
        g.move_stack(&[boat], at(7, 4)).unwrap();
        g.move_stack(&[boat], at(8, 4)).unwrap();
        let _ = g.world.insert_one(boat, MovesLeft(8));
        g.move_stack(&[boat], at(9, 4)).unwrap();
        assert_eq!(g.unit_position(w), Some(at(9, 4)));
        // Stepping ashore is an ordinary move.
        let _ = g.world.insert_one(w, MovesLeft(2));
        g.move_stack(&[w], at(10, 4)).unwrap();
        assert_eq!(g.carrier_of(w), None, "no longer aboard");
        assert_eq!(g.unit_position(w), Some(at(10, 4)));
        assert_eq!(g.unit_moves_left(w), 0, "grassland costs a whole move");
        // The ship stays; its place is free again.
        assert_eq!(g.ship_room(boat), 2);
    }

    #[test]
    fn passengers_ride_into_a_port_and_get_off_there() {
        let mut g = world();
        g.apply(Command::FoundCity {
            owner: P0,
            pos: at(10, 3),
            name: "Far Port".into(),
            race: None,
            population: 3,
        });
        let boat = unit(&mut g, "boat", at(9, 4));
        let w = unit(&mut g, "walker", at(10, 4));
        g.move_stack(&[w], at(9, 4)).unwrap();
        assert_eq!(g.carrier_of(w), Some(boat));
        g.move_stack(&[boat, w], at(10, 3)).unwrap();
        assert_eq!(g.carrier_of(w), None, "ashore in the port");
        assert_eq!(g.unit_position(w), Some(at(10, 3)));
    }

    #[test]
    fn land_units_without_a_ship_drown() {
        let mut g = world();
        let boat = unit(&mut g, "boat", at(5, 4));
        let w = unit(&mut g, "walker", at(4, 4));
        g.move_stack(&[w], at(5, 4)).unwrap();
        let _ = g.world.despawn(boat);
        let events = g.drown_stranded();
        assert_eq!(
            events,
            vec![Event::UnitsDrowned {
                player: P0,
                units: 1
            }]
        );
        assert!(g.world.get::<&crate::units::Unit>(w).is_err());
        // Fliers and ships are never in danger.
        let flier = unit(&mut g, "scout", at(6, 4));
        let other = unit(&mut g, "boat", at(7, 4));
        assert!(g.drown_stranded().is_empty());
        assert!(g.unit_position(flier).is_some() && g.unit_position(other).is_some());
    }

    #[test]
    fn who_is_aboard_survives_a_save() {
        let mut g = world();
        let boat = unit(&mut g, "boat", at(5, 4));
        let w = unit(&mut g, "walker", at(4, 4));
        g.move_stack(&[w], at(5, 4)).unwrap();
        let content = g.content.clone();
        let back = g.snapshot().restore(content).unwrap();
        let (boat2, w2) = (
            back.entity_of_serial(g.serial_of(boat)).unwrap(),
            back.entity_of_serial(g.serial_of(w)).unwrap(),
        );
        assert_eq!(back.carrier_of(w2), Some(boat2));
        assert_eq!(back.ship_room(boat2), 1);
    }
}
