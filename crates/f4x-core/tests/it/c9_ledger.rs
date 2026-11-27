//! Class 9, the army ledger: the units a player is said to have are those that name the player as
//! their owner, however they came, changed sides or left.

use crate::c9_unit::ships_by_a_full_shore;
use crate::support::{around, content, worlds};
use f4x_core::hero::HeroState;
use f4x_core::map::Position;
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::rules::UnitId;
use f4x_core::unit::ledger::{DepartedUnit, Departure, Who};
use f4x_core::world::save::Snapshot;
use f4x_core::world::serial::Serial;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, World};
use f4x_def::half_int::HalfInt;

fn players(w: &World) -> Vec<PlayerId> {
    let mut players = w.player_ids();
    players.push(MONSTERS);
    players
}

/// Every player's list and count against what asking each unit whose it is gives.
fn check(w: &World, when: &str) {
    for p in players(w) {
        let owned: Vec<hecs::Entity> = w
            .units()
            .all_units()
            .into_iter()
            .filter(|&u| w.units().unit_owner(u) == Some(p))
            .collect();
        assert_eq!(w.units().player_units(p), owned, "{when}: {p:?}");
        assert_eq!(w.units().player_unit_count(p), owned.len(), "{when}: {p:?}");
    }
}

fn home(w: &World, player: PlayerId) -> Position {
    w.economy().cities_of(player)[0].1
}

fn create_at(w: &mut Table, owner: PlayerId, pos: Position, unit: UnitId) -> hecs::Entity {
    w.apply(Command::CreateUnit { owner, pos, unit });
    *w.units().all_units().last().expect("the new unit")
}

fn create(w: &mut Table, owner: PlayerId) -> hecs::Entity {
    let pos = home(w, owner);
    create_at(w, owner, pos, UnitId::new(104 /* high_men_spearmen */))
}

/// The last record in a player's ledger.
fn last(w: &World, player: PlayerId) -> DepartedUnit {
    *w.units()
        .departed_units(player)
        .last()
        .expect("a record of a unit that left")
}

/// Hire the first hero type that is free.
fn hire(w: &mut Table, player: PlayerId) -> (usize, hecs::Entity) {
    let kind = w
        .units()
        .hero_states()
        .iter()
        .position(|&s| s == HeroState::Free)
        .expect("a free hero");
    let hero = w.cheat_hire_hero(player, kind, None).expect("hired");
    (kind, hero)
}

#[test]
fn a_new_unit_joins_its_owners_list_and_one_that_leaves_comes_off_it() {
    for (name, mut w) in worlds() {
        check(&w, &name);
        let p = w.wizards()[0];
        let dismissed = create(&mut w, p);
        let slain = create(&mut w, p);
        let list = w.units().player_units(p);
        assert_eq!(list[list.len() - 2..], [dismissed, slain], "{name}");
        check(&w, &name);
        w.apply(Command::DismissUnit { unit: dismissed });
        assert!(!w.units().player_units(p).contains(&dismissed), "{name}");
        assert!(w.cheat_slay_unit(slain), "{name}");
        assert!(!w.units().player_units(p).contains(&slain), "{name}");
        check(&w, &name);
    }
}

#[test]
fn a_unit_that_changes_sides_moves_to_the_other_list() {
    let mut tried = 0;
    for (name, mut w) in worlds() {
        let Some(case) = ships_by_a_full_shore(&mut w) else {
            continue;
        };
        // A loaded ship let go at sea turns neutral.
        w.apply(Command::ReleaseTroops {
            player: case.owner,
            units: vec![case.ship],
            scrap: Vec::new(),
        });
        assert!(
            !w.units().player_units(case.owner).contains(&case.ship),
            "{name}"
        );
        assert!(
            w.units().player_units(MONSTERS).contains(&case.ship),
            "{name}"
        );
        check(&w, &name);
        tried += 1;
    }
    assert!(tried > 0, "no world had a shore to try");
}

#[test]
fn a_defeated_wizard_has_an_empty_list() {
    for (name, mut w) in worlds() {
        let wizards = w.wizards();
        let (loser, winner) = (wizards[0], wizards[1]);
        create(&mut w, loser);
        w.cheat_defeat_player(loser, winner);
        assert_eq!(w.units().player_units(loser), Vec::new(), "{name}");
        check(&w, &name);
    }
}

#[test]
fn a_hero_that_leaves_stays_in_the_hero_list_without_a_body_and_can_serve_again() {
    for (name, mut w) in worlds() {
        let wizards = w.wizards();
        let (first, second) = (wizards[0], wizards[1]);
        let (kind, hero) = hire(&mut w, first);
        assert_eq!(w.units().hero_entity(kind), Some(hero), "{name}");
        assert!(w.units().player_units(first).contains(&hero), "{name}");
        for slain in [false, true] {
            if slain {
                w.cheat_slay_unit(hero);
            } else {
                w.apply(Command::DismissUnit { unit: hero });
            }
            let state = if slain {
                HeroState::Dead
            } else {
                HeroState::Free
            };
            assert_eq!(w.units().hero_entity(kind), Some(hero), "{name}");
            assert_eq!(w.units().hero_state(hero), Some(state), "{name}");
            assert_eq!(w.units().unit_position(hero), None, "{name}");
            assert_eq!(w.units().unit_owner(hero), None, "{name}");
            assert!(w.units().unit_def(hero).is_none(), "{name}");
            assert!(!w.units().player_units(first).contains(&hero), "{name}");
            check(&w, &name);
            if !slain {
                assert_eq!(w.cheat_hire_hero(first, kind, None), Ok(hero), "{name}");
            }
        }
        assert_eq!(w.cheat_hire_hero(second, kind, None), Ok(hero), "{name}");
        assert!(w.units().player_units(second).contains(&hero), "{name}");
        check(&w, &name);
    }
}

#[test]
fn a_unit_that_leaves_is_recorded_with_what_it_was_where_it_stood_and_how_it_left() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        for how in [Departure::LetGo, Departure::Slain] {
            let unit = create(&mut w, p);
            w.scenario_set_experience(unit, 7);
            let expected = DepartedUnit {
                serial: w.units().serial_of(unit),
                who: Who::Troop {
                    kind: w.units().unit_kind_id(unit).expect("a kind"),
                    xp: 7,
                },
                how,
                at: w.units().unit_position(unit).expect("a square"),
            };
            let before = w.units().departed_units(p).len();
            if how == Departure::Slain {
                w.cheat_slay_unit(unit);
            } else {
                w.apply(Command::DismissUnit { unit });
            }
            assert_eq!(w.units().departed_units(p).len(), before + 1, "{name}");
            assert_eq!(last(&w, p), expected, "{name}");
        }
    }
}

#[test]
fn a_settler_that_founds_a_city_is_recorded_as_used_up() {
    let mut tried = 0;
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (width, height) = {
            let map = w.map().expect("a map");
            (map.width(), map.height())
        };
        let squares = (0..height).flat_map(|y| {
            (0..width).map(move |x| Position {
                x,
                y,
                plane: f4x_core::map::Plane::Arcanus,
            })
        });
        let Some(site) = squares
            .into_iter()
            .find(|&at| w.can_found_city(at).is_ok() && w.units().units_at(at).is_empty())
        else {
            continue;
        };
        let settler = create_at(&mut w, p, site, UnitId::new(111 /* high_men_settlers */));
        let serial = w.units().serial_of(settler);
        w.apply(Command::SettleOutpost {
            unit: settler,
            name: "Another".to_owned(),
        });
        assert!(w.economy().city_at(site).is_some(), "{name}");
        let record = last(&w, p);
        assert_eq!(
            (record.serial, record.how, record.at),
            (serial, Departure::UsedUp, site),
            "{name}"
        );
        check(&w, &name);
        tried += 1;
    }
    assert!(tried > 0, "no world had room for a city");
}

#[test]
fn a_spirit_that_melds_with_a_node_is_recorded_as_used_up() {
    let mut tried = 0;
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let Some(node) = w.economy().nodes().into_iter().find(|n| !n.warped) else {
            continue;
        };
        let spirit = create_at(&mut w, p, node.pos, UnitId::new(154 /* magic_spirit */));
        let serial = w.units().serial_of(spirit);
        w.apply(Command::Meld {
            units: vec![spirit],
        });
        let record = last(&w, p);
        assert_eq!(
            (record.serial, record.how),
            (serial, Departure::UsedUp),
            "{name}"
        );
        check(&w, &name);
        tried += 1;
    }
    assert!(tried > 0, "no world had a node");
}

#[test]
fn a_unit_that_changes_sides_is_recorded_by_the_old_owner_only() {
    let mut tried = 0;
    for (name, mut w) in worlds() {
        let Some(case) = ships_by_a_full_shore(&mut w) else {
            continue;
        };
        let serial = w.units().serial_of(case.ship);
        w.apply(Command::ReleaseTroops {
            player: case.owner,
            units: vec![case.ship],
            scrap: Vec::new(),
        });
        let record = last(&w, case.owner);
        assert_eq!(
            (record.serial, record.how),
            (serial, Departure::ChangedSides),
            "{name}"
        );
        // The monsters keep no records, not even of what they lose.
        w.cheat_slay_unit(case.ship);
        assert_eq!(w.units().departed_units(MONSTERS), Vec::new(), "{name}");
        tried += 1;
    }
    assert!(tried > 0, "no world had a shore to try");
}

#[test]
fn a_heros_record_points_at_its_entry_in_the_hero_list() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (_, hero) = hire(&mut w, p);
        let serial = w.units().serial_of(hero);
        w.cheat_slay_unit(hero);
        let record = last(&w, p);
        assert_eq!(
            (record.serial, record.who, record.how),
            (serial, Who::Hero(hero), Departure::Slain),
            "{name}"
        );
    }
}

#[test]
fn a_loaded_game_lists_the_same_units() -> Result<(), Box<dyn std::error::Error>> {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        create(&mut w, p);
        let (_, hero) = hire(&mut w, p);
        let gone = create(&mut w, p);
        w.cheat_slay_unit(gone);
        w.cheat_slay_unit(hero);
        let lists = |w: &World| -> Vec<Vec<Serial>> {
            players(w)
                .into_iter()
                .map(|p| {
                    let units = w.units().player_units(p);
                    units.iter().map(|&u| w.units().serial_of(u)).collect()
                })
                .collect()
        };
        let back = Snapshot::from_text(&w.snapshot().to_text()?)?.restore(content())?;
        assert_eq!(lists(&back), lists(&w), "{name}");
        check(&back, &name);
        // The records come back as they were; a hero's points at the loaded hero entry.
        for player in players(&w) {
            let records = |w: &World| -> Vec<_> {
                w.units()
                    .departed_units(player)
                    .into_iter()
                    .map(|d| {
                        let who = match d.who {
                            Who::Hero(h) => Err(w.units().hero_kind(h)),
                            Who::Troop { kind, xp } => Ok((kind, xp)),
                        };
                        (d.serial, who, d.how, d.at)
                    })
                    .collect()
            };
            assert_eq!(records(&back), records(&w), "{name}: {player:?}");
        }
        assert_eq!(w.units().departed_units(p).len(), 2, "{name}");
    }
    Ok(())
}

#[test]
fn an_order_is_saved_with_the_game_and_ends_when_the_unit_changes_sides(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tried = 0;
    for (name, mut w) in worlds() {
        let Some(case) = ships_by_a_full_shore(&mut w) else {
            continue;
        };
        // A ship with no moves left keeps the order it is given.
        w.scenario_set_moves_left(case.ship, HalfInt::ZERO);
        let here = w.units().unit_position(case.ship).expect("a square");
        let to = around(&w, here).into_iter().find(|&to| {
            w.apply(Command::GoTo {
                units: vec![case.ship],
                to,
            });
            w.unit_has_order(case.ship)
        });
        let Some(to) = to else { continue };
        let serial = w.units().serial_of(case.ship);
        let back = Snapshot::from_text(&w.snapshot().to_text()?)?.restore(content())?;
        let loaded = back.units().entity_of_serial(serial).ok_or("the ship")?;
        assert_eq!(back.unit_destination(loaded), Some(to), "{name}");
        // A loaded ship let go at sea turns neutral: the order was its old owner's.
        w.apply(Command::ReleaseTroops {
            player: case.owner,
            units: vec![case.ship],
            scrap: Vec::new(),
        });
        assert!(!w.unit_has_order(case.ship), "{name}");
        tried += 1;
    }
    assert!(tried > 0, "no world had a ship with a square to go to");
    Ok(())
}
