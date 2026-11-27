//! Standing orders other than a walk (`Command::GiveOrder`) and what they share: they move their
//! units as they are given and at the end of each of the owner's turns, they never fight, and
//! some end when an enemy comes into sight.

use crate::support::{around, content, worlds};
use f4x_core::diplomacy::Treaty;
use f4x_core::feature::entity::Placement;
use f4x_core::feature::node::Node;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::rules::{RealmId, UnitId};
use f4x_core::unit::{Errand, Order, OrderEnd, OrderError};
use f4x_core::world::save::Snapshot;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event, Rejection};
use f4x_def::half_int::HalfInt;

/// A game of the matrix with its first wizard, who has seen the whole map, and the square of the
/// wizard's city. No unit but those the test makes is ordered.
struct Scene {
    name: String,
    w: Table,
    p: PlayerId,
    home: Position,
    round: u32,
}

fn scenes() -> Vec<Scene> {
    worlds()
        .into_iter()
        .map(|(name, mut w)| {
            let p = w
                .player_ids()
                .into_iter()
                .find(|&p| w.wizard(p).is_some())
                .expect("a wizard");
            let home = w.economy().cities_of(p)[0].1;
            w.reveal_map(p);
            // What stands near the city of a fresh game would be company for every order.
            for u in w.units().all_units() {
                if w.units().unit_owner(u) != Some(p) {
                    w.cheat_remove_unit(u);
                }
            }
            Scene {
                name,
                w,
                p,
                home,
                round: 1,
            }
        })
        .collect()
}

impl Scene {
    fn unit(&mut self, owner: PlayerId, at: Position, num: u16) -> hecs::Entity {
        let unit = UnitId::new(num);
        self.w.apply(Command::CreateUnit {
            owner,
            pos: at,
            unit,
        });
        *self.w.units().all_units().last().expect("the new unit")
    }

    fn give(&mut self, units: &[hecs::Entity], errand: Errand) -> Vec<Event> {
        let events = self.w.apply(Command::GiveOrder {
            units: units.to_vec(),
            errand,
        });
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{}: {events:?}",
            self.name
        );
        events
    }

    fn refused(&mut self, units: &[hecs::Entity], errand: Errand) -> Rejection {
        let events = self.w.apply(Command::GiveOrder {
            units: units.to_vec(),
            errand,
        });
        match events.as_slice() {
            [Event::Rejected(why)] => *why,
            other => panic!("{}: not refused: {other:?}", self.name),
        }
    }

    /// End the owner's turn and begin the next.
    fn turn(&mut self) -> Vec<Event> {
        let mut events = self.w.end_player_turn(self.p);
        self.round += 1;
        events.extend(self.w.start_player_turn(self.p, self.round));
        events
    }

    fn spend(&mut self, units: &[hecs::Entity]) {
        for &u in units {
            self.w.scenario_set_moves_left(u, HalfInt::ZERO);
        }
    }

    fn at(&self, unit: hecs::Entity) -> Position {
        self.w.units().unit_position(unit).expect("on the map")
    }

    fn distance(&self, a: Position, b: Position) -> u32 {
        PlayerView::new(&self.w, self.p)
            .distance(a, b)
            .expect("one plane")
    }

    /// The squares `units` can walk to that are `far` squares or more from home, nearest first,
    /// with no city, site or unit on them.
    fn open_squares(&self, units: &[hecs::Entity], far: u32) -> Vec<Position> {
        let mut squares: Vec<Position> = self
            .w
            .reachable_squares(units)
            .into_iter()
            .filter(|&p| {
                p.plane == self.home.plane
                    && self.distance(self.home, p) >= far
                    && self.w.economy().city_at(p).is_none()
                    && self.w.economy().lair_at(p).is_none()
                    && self.w.units().units_at(p).is_empty()
            })
            .collect();
        squares.sort_by_key(|&p| (self.distance(self.home, p), p.y, p.x));
        squares
    }

    /// An empty square next to `at`.
    fn beside(&self, at: Position) -> Position {
        around(&self.w, at)
            .into_iter()
            .find(|&p| {
                self.w.units().units_at(p).is_empty() && self.w.economy().city_at(p).is_none()
            })
            .expect("an empty square beside")
    }
}

fn ended(events: &[Event], unit: hecs::Entity) -> Option<OrderEnd> {
    events.iter().find_map(|e| match e {
        Event::OrderOver { units, why, .. } if units.contains(&unit) => Some(*why),
        _ => None,
    })
}

const SETTLERS: u16 = 111; // high_men_settlers
const SPEARMEN: u16 = 104; // high_men_spearmen
const CAVALRY: u16 = 107; // high_men_cavalry
const ENGINEERS: u16 = 110; // high_men_engineers
const SPIRIT: u16 = 154; // magic_spirit

#[test]
fn a_settle_order_walks_to_the_square_and_founds_a_city_there() {
    for mut s in scenes() {
        let settler = s.unit(s.p, s.home, SETTLERS);
        let site = s
            .open_squares(&[settler], 5)
            .into_iter()
            .find(|&p| s.w.can_found_city(p).is_ok())
            .expect("a site");
        let cities = s.w.economy().cities_of(s.p).len();
        s.give(
            &[settler],
            Errand::Settle {
                to: site,
                name: None,
            },
        );
        for _ in 0..20 {
            if s.w.economy().city_at(site).is_some() {
                break;
            }
            s.turn();
        }
        let city = s.w.economy().city_at(site);
        assert!(city.is_some(), "{}: no city on the site", s.name);
        assert_eq!(s.w.economy().cities_of(s.p).len(), cities + 1, "{}", s.name);
        assert_eq!(
            s.w.units().unit_owner(settler),
            None,
            "{}: the settlers are used up",
            s.name
        );
    }
}

#[test]
fn an_order_that_ends_on_sight_ends_for_a_newly_seen_enemy_only() {
    for mut s in scenes() {
        let settler = s.unit(s.p, s.home, SETTLERS);
        let site = s
            .open_squares(&[settler], 6)
            .into_iter()
            .find(|&p| s.w.can_found_city(p).is_ok())
            .expect("a site");
        s.spend(&[settler]);
        // One that is there already is known as the order is given.
        let first = s.beside(s.home);
        s.unit(MONSTERS, first, SPEARMEN);
        s.give(
            &[settler],
            Errand::Settle {
                to: site,
                name: None,
            },
        );
        s.w.apply(Command::ClearOrders { units: Vec::new() });
        assert!(s.w.unit_has_order(settler), "{}: the one in view", s.name);
        // A second one is new.
        let second = s.beside(s.home);
        let events = s.w.apply(Command::CreateUnit {
            owner: MONSTERS,
            pos: second,
            unit: UnitId::new(SPEARMEN),
        });
        assert!(!s.w.unit_has_order(settler), "{}", s.name);
        assert_eq!(
            ended(&events, settler),
            Some(OrderEnd::EnemySeen),
            "{}",
            s.name
        );
    }
}

#[test]
fn a_walk_a_reinforcement_and_an_escort_go_on_when_an_enemy_comes_into_sight() {
    for mut s in scenes() {
        let walker = s.unit(s.p, s.home, SPEARMEN);
        let guard = s.unit(s.p, s.home, SPEARMEN);
        let far = s.open_squares(&[walker], 6)[0];
        let start = s.open_squares(&[walker], 4)[0];
        let reinforcement = s.unit(s.p, start, SPEARMEN);
        s.spend(&[walker, guard, reinforcement]);
        s.w.apply(Command::GoTo {
            units: vec![walker],
            to: far,
        });
        s.give(&[guard], Errand::Escort { charge: walker });
        s.give(&[reinforcement], Errand::Reinforce { to: s.home });
        for at in [s.beside(s.home), s.beside(start)] {
            s.unit(MONSTERS, at, SPEARMEN);
        }
        for u in [walker, guard, reinforcement] {
            assert!(s.w.unit_has_order(u), "{}: {:?}", s.name, s.w.unit_order(u));
        }
    }
}

#[test]
fn another_players_unit_ends_an_order_only_at_war() {
    for mut s in scenes() {
        let Some(other) =
            s.w.player_ids()
                .into_iter()
                .find(|&o| o != s.p && s.w.wizard(o).is_some())
        else {
            continue;
        };
        let engineer = s.unit(s.p, s.home, ENGINEERS);
        let to = s.open_squares(&[engineer], 4)[0];
        let path = s.w.find_path(&[engineer], to).expect("a way");
        s.w.apply(Command::BuildRoad {
            units: vec![engineer],
            path,
        });
        assert!(s.w.unit_building_road(engineer), "{}", s.name);
        let first = s.beside(s.at(engineer));
        s.unit(other, first, SPEARMEN);
        assert!(s.w.unit_has_order(engineer), "{}: at peace", s.name);
        s.w.make_contact(s.p, other);
        s.w.declare_war(s.p, other);
        assert_eq!(s.w.treaty(s.p, other), Treaty::War, "{}", s.name);
        s.w.apply(Command::ClearOrders { units: Vec::new() });
        assert!(!s.w.unit_has_order(engineer), "{}: at war", s.name);
    }
}

#[test]
fn a_scout_goes_from_patch_to_patch_until_nothing_is_left() {
    for mut s in scenes() {
        let scout = s.unit(s.p, s.home, CAVALRY);
        let events = s.give(
            &[scout],
            Errand::Scout {
                to: None,
                avoid: Vec::new(),
                favour: Vec::new(),
            },
        );
        assert_eq!(
            ended(&events, scout),
            Some(OrderEnd::NothingLeft),
            "{}: the whole map is known",
            s.name
        );
        assert!(!s.w.unit_has_order(scout), "{}", s.name);
    }
    for (name, mut w) in worlds() {
        let p = w
            .player_ids()
            .into_iter()
            .find(|&p| w.wizard(p).is_some())
            .expect("a wizard");
        let home = w.economy().cities_of(p)[0].1;
        w.apply(Command::CreateUnit {
            owner: p,
            pos: home,
            unit: UnitId::new(CAVALRY),
        });
        let scout = *w.units().all_units().last().expect("the scout");
        w.apply(Command::GiveOrder {
            units: vec![scout],
            errand: Errand::Scout {
                to: None,
                avoid: Vec::new(),
                favour: Vec::new(),
            },
        });
        assert_ne!(
            w.units().unit_position(scout),
            Some(home),
            "{name}: it set out at once"
        );
        assert!(
            matches!(w.unit_order(scout), Some(Order::Scout(_))),
            "{name}: {:?}",
            w.unit_order(scout)
        );
        assert!(w.unit_destination(scout).is_some(), "{name}");
    }
}

#[test]
fn a_scouting_ship_carries_its_passenger_whose_spent_moves_do_not_hold_it_up() {
    const GALLEY: u16 = 36; // galley
    let mut sailed = false;
    for (name, mut w) in worlds() {
        let p = w
            .player_ids()
            .into_iter()
            .find(|&p| w.wizard(p).is_some())
            .expect("a wizard");
        let home = w.economy().cities_of(p)[0].1;
        for u in w.units().all_units() {
            if w.units().unit_owner(u) != Some(p) {
                w.cheat_remove_unit(u);
            }
        }
        // Open sea next to the home city.
        let ocean =
            |w: &Table, at| PlayerView::new(w, p).terrain_class(at) == Some(TerrainClass::Ocean);
        let Some(sea) = around(&w, home)
            .into_iter()
            .find(|&s| ocean(&w, s) && around(&w, s).iter().filter(|&&n| ocean(&w, n)).count() > 3)
        else {
            continue;
        };
        let mut make = |num: u16| {
            w.apply(Command::CreateUnit {
                owner: p,
                pos: sea,
                unit: UnitId::new(num),
            });
            *w.units().all_units().last().expect("the new unit")
        };
        let ship = make(GALLEY);
        let rider = make(SPEARMEN);
        w.scenario_set_moves_left(rider, HalfInt::ZERO);
        let events = w.apply(Command::GiveOrder {
            units: vec![ship, rider],
            errand: Errand::Scout {
                to: None,
                avoid: Vec::new(),
                favour: Vec::new(),
            },
        });
        let at = w.units().unit_position(ship).expect("the ship");
        assert_ne!(at, sea, "{name}: the ship stayed: {events:?}");
        assert!(ocean(&w, at) || w.economy().city_at(at).is_some(), "{name}");
        assert_eq!(w.units().unit_position(rider), Some(at), "{name}");
        sailed = true;
    }
    assert!(sailed, "no world has sea beside the first city");
}

#[test]
fn a_meld_order_walks_to_the_node_and_melds() {
    for mut s in scenes() {
        let spirit = s.unit(s.p, s.home, SPIRIT);
        let at = s.open_squares(&[spirit], 4)[0];
        let feature =
            s.w.content()
                .features
                .id_numbered(20 /* sorcery_node */)
                .expect("a node feature");
        s.w.spawn_feature(Placement {
            at,
            feature,
            node: Some(Node {
                realm: RealmId::new(1),
                power: 1,
                aura: vec![(at.x, at.y)],
                guardian: false,
                warped: false,
            }),
            lair: None,
        });
        s.give(&[spirit], Errand::Meld { to: at });
        for _ in 0..80 {
            if !s.w.unit_has_order(spirit) {
                break;
            }
            s.turn();
        }
        let owner = s.w.economy().node_at(at).and_then(|n| n.owner);
        assert_eq!(owner, Some(s.p), "{}: melded", s.name);
        assert_eq!(s.w.units().unit_owner(spirit), None, "{}", s.name);
        // A second spirit has nothing to do there.
        let second = s.unit(s.p, s.home, SPIRIT);
        let why = s.refused(&[second], Errand::Meld { to: at });
        assert!(matches!(why, Rejection::Meld(_)), "{}: {why:?}", s.name);
    }
}

#[test]
fn a_reinforcement_ends_on_arrival_and_when_the_city_is_lost() {
    for mut s in scenes() {
        let start = {
            let probe = s.unit(s.p, s.home, SPEARMEN);
            let start = s.open_squares(&[probe], 4)[0];
            s.w.cheat_remove_unit(probe);
            start
        };
        let unit = s.unit(s.p, start, SPEARMEN);
        s.give(&[unit], Errand::Reinforce { to: s.home });
        for _ in 0..20 {
            if !s.w.unit_has_order(unit) {
                break;
            }
            s.turn();
        }
        assert_eq!(s.at(unit), s.home, "{}", s.name);
        assert!(!s.w.unit_has_order(unit), "{}", s.name);

        let late = s.unit(s.p, start, SPEARMEN);
        s.spend(&[late]);
        s.give(&[late], Errand::Reinforce { to: s.home });
        let city = s.w.economy().city_at(s.home).expect("the city");
        s.w.apply(Command::RazeCity { player: s.p, city });
        if s.w.economy().city_at(s.home).is_some() {
            continue;
        }
        let events = s.turn();
        assert_eq!(
            ended(&events, late),
            Some(OrderEnd::TargetGone),
            "{}",
            s.name
        );
    }
}

#[test]
fn an_escort_walks_with_its_charge_as_one_and_follows_it() {
    for mut s in scenes() {
        let settler = s.unit(s.p, s.home, SETTLERS);
        let guard = s.unit(s.p, s.home, CAVALRY);
        s.give(&[guard], Errand::Escort { charge: settler });
        let to = s.open_squares(&[settler], 5)[0];
        s.w.apply(Command::GoTo {
            units: vec![settler],
            to,
        });
        // The faster escort keeps the slower charge's pace.
        assert_eq!(s.at(guard), s.at(settler), "{}", s.name);
        for _ in 0..10 {
            s.turn();
            assert_eq!(s.at(guard), s.at(settler), "{}", s.name);
        }
        assert_eq!(s.at(settler), to, "{}", s.name);
        // Moved by hand without the escort, the charge is followed at the end of the turn.
        let next = s.beside(to);
        s.w.apply(Command::MoveStack {
            units: vec![settler],
            to: next,
        });
        if s.at(settler) == next {
            s.turn();
            assert_eq!(s.at(guard), next, "{}", s.name);
        }
        // Moved together they keep the escort.
        let back = s.at(settler);
        let on = s.beside(back);
        s.w.apply(Command::MoveStack {
            units: vec![settler, guard],
            to: on,
        });
        assert!(
            matches!(s.w.unit_order(guard), Some(Order::Escort(_))),
            "{}: {:?}",
            s.name,
            s.w.unit_order(guard)
        );
        // The charge gone, the escort asks for orders.
        s.w.cheat_remove_unit(settler);
        let events = s.turn();
        assert_eq!(
            ended(&events, guard),
            Some(OrderEnd::TargetGone),
            "{}",
            s.name
        );
    }
}

#[test]
fn an_order_that_cannot_be_given_changes_nothing() {
    for mut s in scenes() {
        let soldier = s.unit(s.p, s.home, SPEARMEN);
        let empty = s.open_squares(&[soldier], 3)[0];
        let why = s.refused(
            &[soldier],
            Errand::Settle {
                to: empty,
                name: None,
            },
        );
        assert_eq!(why, Rejection::Order(OrderError::NoSettler), "{}", s.name);
        let why = s.refused(&[soldier], Errand::Reinforce { to: empty });
        assert_eq!(why, Rejection::Order(OrderError::NotOwnCity), "{}", s.name);
        let why = s.refused(&[soldier], Errand::Escort { charge: soldier });
        assert_eq!(why, Rejection::Order(OrderError::BadCharge), "{}", s.name);
        let why = s.refused(&[soldier], Errand::Meld { to: empty });
        assert!(matches!(why, Rejection::Meld(_)), "{}: {why:?}", s.name);
        assert!(!s.w.unit_has_order(soldier), "{}", s.name);
    }
}

#[test]
fn an_order_and_what_it_has_seen_are_saved_with_the_game() {
    for mut s in scenes() {
        let engineer = s.unit(s.p, s.home, ENGINEERS);
        let to = s.open_squares(&[engineer], 4)[0];
        let first = s.beside(s.home);
        s.unit(MONSTERS, first, SPEARMEN);
        let path = s.w.find_path(&[engineer], to).expect("a way");
        s.w.apply(Command::BuildRoad {
            units: vec![engineer],
            path,
        });
        let order = s.w.unit_order(engineer).expect("building");
        let text = s.w.snapshot().to_text().expect("writes");
        let again = Snapshot::from_text(&text)
            .expect("reads")
            .restore(content())
            .expect("restores");
        let engineer = *again
            .units()
            .player_units(s.p)
            .last()
            .expect("the engineer");
        assert_eq!(again.unit_order(engineer), Some(order), "{}", s.name);
        // The enemy it knew of still does not end the order.
        let mut again = Table::new(again, f4x_core::world::rng::GameRng::new(1));
        again.apply(Command::ClearOrders { units: Vec::new() });
        assert!(again.unit_has_order(engineer), "{}", s.name);
    }
}
