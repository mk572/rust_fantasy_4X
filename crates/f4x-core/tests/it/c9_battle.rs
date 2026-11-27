//! Class 9, the battle: how one starts, who may be in it, how it ends and what the end changes.
//!
//! The battles here are begun on the fresh worlds: a unit of the first wizard and a unit of the second
//! on neighbouring squares. No battle AI is set in this crate (`set_battle_ai` belongs to `f4x-ai`), so
//! what plays a side is the test.

use crate::support::{around, state, worlds};
use f4x_battle::battle::{BattleCommand, BattleError, BattleEvent, Side};
use f4x_battle::battlemagic::caster::SpellAim;
use f4x_battle::battlemagic::error::SpellError;
use f4x_core::battle::world::BattleStartError;
use f4x_core::diplomacy::{DiplomacyCommand as D, Treaty};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::player::PlayerId;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event, Rejection, World};
use f4x_def::half_int::HalfInt;
use f4x_def::rules::{CastContext, Stat};

/// A world with two stacks face to face: one unit of `a` and one unit of `b` on neighbouring squares.
struct Faceoff {
    name: String,
    w: Table,
    a: PlayerId,
    b: PlayerId,
    attacker: hecs::Entity,
    defender: hecs::Entity,
}

fn faceoffs() -> Vec<Faceoff> {
    let mut out = Vec::new();
    for (name, mut w) in worlds() {
        let ids = w.wizards();
        let (a, b) = (ids[0], ids[1]);
        let Some(attacker) = w.units().all_units().into_iter().find(|&u| {
            w.units().unit_owner(u) == Some(a)
                && w.units().unit_def(u).is_some_and(|d| {
                    d.stats
                        .get(&Stat::Melee)
                        .is_some_and(|m| *m > HalfInt::ZERO)
                })
        }) else {
            continue;
        };
        let at = w.units().unit_position(attacker).expect("a position");
        let kind = w.units().unit_kind_id(attacker).expect("a kind");
        let Some(to) = around(&w, at).into_iter().find(|&to| {
            w.units().units_at(to).is_empty()
                && w.economy().city_at(to).is_none()
                && w.economy().terrain_class_at(to) == Some(TerrainClass::Grassland)
                && w.apply(Command::CreateUnit {
                    owner: b,
                    pos: to,
                    unit: kind,
                })
                .iter()
                .all(|e| !matches!(e, Event::Rejected(_)))
        }) else {
            continue;
        };
        let defender = w.units().units_at(to)[0];
        out.push(Faceoff {
            name,
            w,
            a,
            b,
            attacker,
            defender,
        });
    }
    assert!(!out.is_empty(), "no world had room for a battle");
    out
}

fn start(f: &mut Faceoff) -> Vec<Event> {
    f.w.apply(Command::BeginBattle {
        attackers: vec![f.attacker],
        defenders: vec![f.defender],
    })
}

fn refusal(events: &[Event]) -> Option<BattleStartError> {
    events.iter().find_map(|e| match e {
        Event::Rejected(Rejection::BattleStart(why)) => Some(*why),
        _ => None,
    })
}

fn battle_error(events: &[Event]) -> Option<BattleError> {
    events.iter().find_map(|e| match e {
        Event::Rejected(Rejection::Battle(why)) => Some(*why),
        _ => None,
    })
}

#[test]
fn a_battle_starts_with_both_sides_and_the_site_of_the_defenders() {
    for mut f in faceoffs() {
        let site = f.w.units().unit_position(f.defender).expect("a position");
        let events = start(&mut f);
        assert_eq!(refusal(&events), None, "{}: {events:?}", f.name);
        assert!(
            events.iter().any(
                |e| matches!(e, Event::BattleStarted { attacker, defender, site: s }
                if *attacker == f.a && *defender == f.b && *s == site)
            ),
            "{}: the start is told: {events:?}",
            f.name
        );
        assert!(f.w.in_battle(), "{}", f.name);
        let battle = f.w.battle().expect("a battle");
        assert_eq!(
            (battle.attacker, battle.defender, battle.site),
            (f.a, f.b, Some(site)),
            "{}",
            f.name
        );
        assert_eq!(battle.active, Side::Attacker, "{}: who begins", f.name);
        assert_eq!(battle.winner, None, "{}", f.name);
        for side in [Side::Attacker, Side::Defender] {
            assert!(
                battle.units.iter().any(|u| u.side == side),
                "{}: {side:?} has no unit",
                f.name
            );
        }
    }
}

#[test]
fn a_battle_is_refused_for_bad_sides_a_battle_in_progress_and_a_treaty() {
    for mut f in faceoffs() {
        let name = f.name.clone();
        for (attackers, defenders) in [
            (vec![], vec![f.defender]),
            (vec![f.attacker], vec![]),
            (vec![f.attacker], vec![f.attacker]),
            (vec![f.attacker, f.defender], vec![f.defender]),
        ] {
            let before = state(&f.w);
            let events = f.w.apply(Command::BeginBattle {
                attackers,
                defenders,
            });
            assert_eq!(
                refusal(&events),
                Some(BattleStartError::BadSides),
                "{name}: {events:?}"
            );
            assert_eq!(
                state(&f.w),
                before,
                "{name}: a refused battle changed the game"
            );
        }
        for treaty in [Treaty::Pact, Treaty::Alliance] {
            f.w.make_contact(f.a, f.b);
            f.w.start_treaty(f.a, f.b, treaty);
            let before = state(&f.w);
            let events = start(&mut f);
            assert_eq!(
                refusal(&events),
                Some(BattleStartError::Treaty),
                "{name}: {treaty:?}: {events:?}"
            );
            assert_eq!(
                state(&f.w),
                before,
                "{name}: a refused battle changed the game"
            );
        }
        f.w.apply(Command::Diplomacy(D::BreakTreaty {
            player: f.a,
            with: f.b,
        }));
        assert_eq!(refusal(&start(&mut f)), None, "{name}: after the break");
        let again = start(&mut f);
        assert_eq!(
            refusal(&again),
            Some(BattleStartError::InProgress),
            "{name}: {again:?}"
        );
    }
}

#[test]
fn an_auto_resolved_attack_must_come_from_a_neighbouring_square() {
    for mut f in faceoffs() {
        let at = f.w.units().unit_position(f.attacker).expect("a position");
        // A unit of the defender's wizard two squares or more away from the attacker.
        let kind = f.w.units().unit_kind_id(f.attacker).expect("a kind");
        let far = around(&f.w, at)
            .into_iter()
            .flat_map(|n| around(&f.w, n))
            .find(|&q| {
                f.w.map()
                    .expect("a map")
                    .plane(q.plane)
                    .chebyshev((q.x, q.y), (at.x, at.y))
                    > 1
                    && f.w.units().units_at(q).is_empty()
                    && f.w.economy().city_at(q).is_none()
            });
        let Some(far) = far else { continue };
        let made = f.w.apply(Command::CreateUnit {
            owner: f.b,
            pos: far,
            unit: kind,
        });
        if made.iter().any(|e| matches!(e, Event::Rejected(_))) {
            continue;
        }
        let distant = f.w.units().units_at(far)[0];
        let before = state(&f.w);
        let events = f.w.apply(Command::AutoResolve {
            attackers: vec![f.attacker],
            defenders: vec![distant],
            mana_budget: None,
        });
        assert_eq!(
            refusal(&events),
            Some(BattleStartError::NotAdjacent),
            "{}: {events:?}",
            f.name
        );
        assert_eq!(state(&f.w), before, "{}", f.name);
    }
}

#[test]
fn a_battle_command_without_a_battle_is_over() {
    for mut f in faceoffs() {
        let before = state(&f.w);
        let events = f.w.apply(Command::Battle(BattleCommand::EndTurn));
        assert_eq!(battle_error(&events), Some(BattleError::Over), "{}", f.name);
        assert_eq!(state(&f.w), before, "{}", f.name);
    }
}

#[test]
fn fleeing_gives_the_battle_to_the_other_side_and_closes_it() {
    for mut f in faceoffs() {
        start(&mut f);
        let events = f.w.apply(Command::Battle(BattleCommand::Flee));
        assert_eq!(battle_error(&events), None, "{}: {events:?}", f.name);
        assert!(!f.w.in_battle(), "{}: the battle stayed open", f.name);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::BattleEnded { winner, loser }
                if *winner == f.b && *loser == f.a)),
            "{}: the defender wins: {events:?}",
            f.name
        );
        // The fleeing wizard's army is gone from the square or has moved: no unit of both
        // wizards is left on one square.
        for u in f.w.units().all_units() {
            let at = f.w.units().unit_position(u);
            let owners: Vec<_> =
                f.w.units()
                    .units_at(at.expect("a position"))
                    .into_iter()
                    .filter_map(|v| f.w.units().unit_owner(v))
                    .collect();
            assert!(
                owners.windows(2).all(|p| p[0] == p[1]),
                "{}: two wizards share a square after the battle",
                f.name
            );
        }
        // A new battle can be begun at once, if both sides still have a unit.
        assert!(!f.w.in_battle());
    }
}

#[test]
fn a_battle_played_to_the_end_tells_one_end_and_leaves_no_battle_open() {
    for mut f in faceoffs() {
        start(&mut f);
        let mut ended = Vec::new();
        for _ in 0..5_000 {
            if !f.w.in_battle() {
                break;
            }
            // The side to play attacks the nearest enemy, or ends its turn.
            let battle = f.w.battle().expect("a battle");
            let me = battle.active;
            let mine: Vec<usize> = (0..battle.units.len())
                .filter(|&i| battle.units[i].side == me && battle.units[i].is_active())
                .collect();
            let foes: Vec<usize> = (0..battle.units.len())
                .filter(|&i| battle.units[i].side != me && battle.units[i].is_active())
                .collect();
            let mut did = false;
            'units: for &u in &mine {
                for &t in &foes {
                    let ev = f.w.apply(Command::Battle(BattleCommand::Attack {
                        unit: u,
                        target: t,
                    }));
                    if battle_error(&ev).is_none() {
                        ended.extend(ev);
                        did = true;
                        break 'units;
                    }
                }
            }
            if !did {
                ended.extend(f.w.apply(Command::Battle(BattleCommand::EndTurn)));
            }
        }
        // Whatever stalls ends by fleeing, as `auto_resolve` does.
        if f.w.in_battle() {
            ended.extend(f.w.apply(Command::Battle(BattleCommand::Flee)));
        }
        assert!(!f.w.in_battle(), "{}", f.name);
        assert_eq!(
            ended
                .iter()
                .filter(|e| matches!(e, Event::BattleEnded { .. }))
                .count(),
            1,
            "{}: one end is told",
            f.name
        );
    }
}

#[test]
fn fliers_fight_on_the_sea_without_a_ship() {
    use f4x_core::map::Position;
    use f4x_def::map::Plane;
    use f4x_def::rules::MovementType;
    let content = crate::support::content();
    let flier = content
        .rules
        .units
        .iter()
        .find(|(_, d)| {
            d.movement == MovementType::Flying
                && d.stats
                    .get(&Stat::Melee)
                    .is_some_and(|m| *m > HalfInt::ZERO)
        })
        .map(|(id, _)| id)
        .expect("a flying unit with a melee attack");
    let mut tried = 0;
    for (name, mut w) in worlds() {
        let ids = w.wizards();
        let (a, b) = (ids[0], ids[1]);
        let (width, height) = {
            let map = w.map().expect("a map");
            (map.width(), map.height())
        };
        let sea = |w: &World, x: u32, y: u32| {
            let at = Position {
                x,
                y,
                plane: Plane::Arcanus,
            };
            (w.units().units_at(at).is_empty()
                && w.economy().city_at(at).is_none()
                && w.economy().terrain_class_at(at) == Some(TerrainClass::Ocean))
            .then_some(at)
        };
        let pair = (0..height).find_map(|y| {
            (0..width.saturating_sub(1)).find_map(|x| Some((sea(&w, x, y)?, sea(&w, x + 1, y)?)))
        });
        let Some((from, to)) = pair else { continue };
        for (owner, pos) in [(a, from), (b, to)] {
            let made = w.apply(Command::CreateUnit {
                owner,
                pos,
                unit: flier,
            });
            assert!(
                made.iter().all(|e| !matches!(e, Event::Rejected(_))),
                "{name}: {made:?}"
            );
        }
        tried += 1;
        let (attacker, defender) = (w.units().units_at(from)[0], w.units().units_at(to)[0]);
        let events = w.apply(Command::BeginBattle {
            attackers: vec![attacker],
            defenders: vec![defender],
        });
        assert_eq!(refusal(&events), None, "{name}: {events:?}");
        let battle = w.battle().expect("a battle");
        assert_eq!(battle.winner, None, "{name}: settled at the start");
        for side in [Side::Attacker, Side::Defender] {
            assert!(
                battle.units.iter().any(|u| u.side == side),
                "{name}: no flier of {side:?} fights"
            );
        }
    }
    assert!(tried > 0, "no world had two neighbouring sea squares");
}

#[test]
fn ships_fight_on_the_sea() {
    use f4x_core::map::Position;
    use f4x_def::map::Plane;
    use f4x_def::rules::MovementType;
    let content = crate::support::content();
    let ship = content
        .rules
        .units
        .iter()
        .find(|(_, d)| {
            d.movement == MovementType::Sailing
                && d.stats
                    .get(&Stat::Melee)
                    .is_some_and(|m| *m > HalfInt::ZERO)
        })
        .map(|(id, _)| id)
        .expect("a ship with a melee attack");
    let mut tried = 0;
    for (name, mut w) in worlds() {
        let ids = w.wizards();
        let (a, b) = (ids[0], ids[1]);
        let (width, height) = {
            let map = w.map().expect("a map");
            (map.width(), map.height())
        };
        let sea = |w: &World, x: u32, y: u32| {
            let at = Position {
                x,
                y,
                plane: Plane::Arcanus,
            };
            (w.units().units_at(at).is_empty()
                && w.economy().city_at(at).is_none()
                && w.economy().terrain_class_at(at) == Some(TerrainClass::Ocean))
            .then_some(at)
        };
        let pair = (0..height).find_map(|y| {
            (0..width.saturating_sub(1)).find_map(|x| Some((sea(&w, x, y)?, sea(&w, x + 1, y)?)))
        });
        let Some((from, to)) = pair else { continue };
        for (owner, pos) in [(a, from), (b, to)] {
            let made = w.apply(Command::CreateUnit {
                owner,
                pos,
                unit: ship,
            });
            assert!(
                made.iter().all(|e| !matches!(e, Event::Rejected(_))),
                "{name}: {made:?}"
            );
        }
        tried += 1;
        let (attacker, defender) = (w.units().units_at(from)[0], w.units().units_at(to)[0]);
        let events = w.apply(Command::BeginBattle {
            attackers: vec![attacker],
            defenders: vec![defender],
        });
        assert_eq!(refusal(&events), None, "{name}: {events:?}");
        let battle = w.battle().expect("a battle");
        assert_eq!(battle.winner, None, "{name}: settled at the start");
        for side in [Side::Attacker, Side::Defender] {
            assert!(
                battle.units.iter().any(|u| u.side == side),
                "{name}: no ship of {side:?} fights"
            );
        }
    }
    assert!(tried > 0, "no world had two neighbouring sea squares");
}

/// Every unit of `side` in the battle on the table falls, and the next command lets the battle see it.
fn wipe_out(w: &mut Table, side: Side) -> Vec<Event> {
    w.scenario_edit_battle(|b| {
        for u in b.units.iter_mut().filter(|u| u.side == side) {
            u.kill();
            u.status = f4x_battle::unit::Status::Dead;
        }
    });
    w.apply(Command::Battle(BattleCommand::EndTurn))
}

fn told(events: &[Event], wanted: impl Fn(&Event) -> bool) -> bool {
    events.iter().any(wanted)
}

#[test]
fn the_winning_attacker_takes_the_square_and_the_fallen_are_gone() {
    for mut f in faceoffs() {
        let site = f.w.units().unit_position(f.defender).expect("a position");
        let before = f.w.units().all_units().len();
        start(&mut f);
        let events = wipe_out(&mut f.w, Side::Defender);
        assert!(!f.w.in_battle(), "{}: {events:?}", f.name);
        assert!(
            told(
                &events,
                |e| matches!(e, Event::BattleEnded { winner, loser }
                if *winner == f.a && *loser == f.b)
            ),
            "{}: {events:?}",
            f.name
        );
        assert!(
            told(
                &events,
                |e| matches!(e, Event::BattleCasualties { player, units: 1 }
                if *player == f.b)
            ),
            "{}: the loss is told: {events:?}",
            f.name
        );
        assert_eq!(f.w.units().all_units().len(), before - 1, "{}", f.name);
        assert_eq!(
            f.w.units().unit_position(f.attacker),
            Some(site),
            "{}: the winner did not move in",
            f.name
        );
        assert!(
            told(
                &events,
                |e| matches!(e, Event::UnitsMoved { to, .. } if *to == site)
            ),
            "{}: {events:?}",
            f.name
        );
    }
}

#[test]
fn the_winning_defender_stays_and_the_fallen_attacker_is_gone() {
    for mut f in faceoffs() {
        let (home, site) = (
            f.w.units().unit_position(f.attacker).expect("a position"),
            f.w.units().unit_position(f.defender).expect("a position"),
        );
        start(&mut f);
        let events = wipe_out(&mut f.w, Side::Attacker);
        assert!(!f.w.in_battle(), "{}", f.name);
        assert!(
            told(
                &events,
                |e| matches!(e, Event::BattleEnded { winner, .. } if *winner == f.b)
            ),
            "{}: {events:?}",
            f.name
        );
        assert_eq!(
            f.w.units().unit_position(f.defender),
            Some(site),
            "{}",
            f.name
        );
        assert!(
            f.w.units().unit_position(f.attacker).is_none(),
            "{}: the fallen attacker is still in the game at {home:?}",
            f.name
        );
        assert!(
            !told(&events, |e| matches!(e, Event::UnitsMoved { .. })),
            "{}: a defence that held moved: {events:?}",
            f.name
        );
    }
}

#[test]
fn a_survivor_keeps_the_damage_it_took_in_the_fight() {
    for mut f in faceoffs() {
        start(&mut f);
        let lost = f.w.battle().expect("a battle").units[f
            .w
            .battle()
            .expect("a battle")
            .units
            .iter()
            .position(|u| u.side == Side::Attacker)
            .expect("a unit")]
        .lost();
        assert_eq!(lost, 0, "{}: a unit begins whole", f.name);
        f.w.scenario_edit_battle(|b| {
            let i = b
                .units
                .iter()
                .position(|u| u.side == Side::Attacker)
                .expect("a unit");
            b.units[i].set_lost(1);
        });
        wipe_out(&mut f.w, Side::Defender);
        let (now, max) = f.w.units().unit_hits(f.attacker);
        assert_eq!(now + 1, max, "{}: one hit point of damage stays", f.name);
    }
}

#[test]
fn units_that_flee_may_be_lost_unless_the_flight_is_safe() {
    for safe in [true, false] {
        let (mut tried, mut lost_all) = (0, 0);
        for mut f in faceoffs() {
            let at = f.w.units().unit_position(f.attacker).expect("a position");
            let kind = f.w.units().unit_kind_id(f.attacker).expect("a kind");
            // A stack of six on the attacker's square.
            let mut stack = vec![f.attacker];
            for _ in 0..5 {
                f.w.apply(Command::CreateUnit {
                    owner: f.a,
                    pos: at,
                    unit: kind,
                });
            }
            stack.extend(
                f.w.units()
                    .units_at(at)
                    .into_iter()
                    .filter(|&u| u != f.attacker && f.w.units().unit_owner(u) == Some(f.a)),
            );
            let n = stack.len();
            f.w.scenario_set_safe_flee(safe);
            let events = f.w.apply(Command::BeginBattle {
                attackers: stack.clone(),
                defenders: vec![f.defender],
            });
            assert_eq!(refusal(&events), None, "{}", f.name);
            let events = f.w.apply(Command::Battle(BattleCommand::Flee));
            assert!(!f.w.in_battle(), "{}", f.name);
            let alive = stack
                .iter()
                .filter(|&&u| f.w.units().unit_position(u).is_some())
                .count();
            let reported: u32 = events
                .iter()
                .filter_map(|e| match e {
                    Event::UnitsLostFleeing { player, units } if *player == f.a => Some(*units),
                    _ => None,
                })
                .sum();
            assert_eq!(
                alive + reported as usize,
                n,
                "{}: safe {safe}: {events:?}",
                f.name
            );
            if safe {
                assert_eq!(reported, 0, "{}: a safe flight lost units", f.name);
            }
            tried += 1;
            lost_all += reported;
        }
        assert!(tried > 0);
        assert!(
            safe || lost_all > 0,
            "no unit was lost in any flight over {tried} worlds"
        );
    }
}

#[test]
fn an_attacker_that_wins_at_a_city_takes_it() {
    let mut tried = 0;
    for mut f in faceoffs() {
        let race = {
            let c = f.w.economy().cities_of(f.b)[0].0;
            f.w.economy().city_race(c)
        };
        let at = f.w.units().unit_position(f.attacker).expect("a position");
        let kind = f.w.units().unit_kind_id(f.attacker).expect("a kind");
        // A city of b on a square next to the attacker, with a unit of b in it.
        let Some(site) = around(&f.w, at).into_iter().find(|&q| {
            f.w.units().units_at(q).is_empty()
                && f.w.economy().city_at(q).is_none()
                && f.w.economy().terrain_class_at(q) == Some(TerrainClass::Grassland)
        }) else {
            continue;
        };
        let city = f.w.scenario_found_city(f.b, site, "Fallen", race, 3);
        f.w.apply(Command::CreateUnit {
            owner: f.b,
            pos: site,
            unit: kind,
        });
        let guard = f.w.units().units_at(site)[0];
        tried += 1;
        let events = f.w.apply(Command::BeginBattle {
            attackers: vec![f.attacker],
            defenders: vec![guard],
        });
        assert_eq!(refusal(&events), None, "{}: {events:?}", f.name);
        let events = wipe_out(&mut f.w, Side::Defender);
        assert_eq!(
            f.w.economy().city_owner(city),
            Some(f.a),
            "{}: {events:?}",
            f.name
        );
        assert!(
            told(
                &events,
                |e| matches!(e, Event::CityCaptured { from, to, .. }
                if *from == f.b && *to == f.a)
            ),
            "{}: {events:?}",
            f.name
        );
        assert_eq!(
            f.w.units().unit_position(f.attacker),
            Some(site),
            "{}",
            f.name
        );
    }
    assert!(tried > 0, "no world had room for a city");
}

#[test]
fn a_city_that_holds_stays_with_its_owner() {
    let mut tried = 0;
    for mut f in faceoffs() {
        let race = {
            let c = f.w.economy().cities_of(f.b)[0].0;
            f.w.economy().city_race(c)
        };
        let at = f.w.units().unit_position(f.attacker).expect("a position");
        let kind = f.w.units().unit_kind_id(f.attacker).expect("a kind");
        let Some(site) = around(&f.w, at).into_iter().find(|&q| {
            f.w.units().units_at(q).is_empty()
                && f.w.economy().city_at(q).is_none()
                && f.w.economy().terrain_class_at(q) == Some(TerrainClass::Grassland)
        }) else {
            continue;
        };
        let city = f.w.scenario_found_city(f.b, site, "Held", race, 3);
        f.w.apply(Command::CreateUnit {
            owner: f.b,
            pos: site,
            unit: kind,
        });
        let guard = f.w.units().units_at(site)[0];
        tried += 1;
        f.w.apply(Command::BeginBattle {
            attackers: vec![f.attacker],
            defenders: vec![guard],
        });
        let events = wipe_out(&mut f.w, Side::Attacker);
        assert_eq!(
            f.w.economy().city_owner(city),
            Some(f.b),
            "{}: {events:?}",
            f.name
        );
        assert!(
            !told(&events, |e| matches!(e, Event::CityCaptured { .. })),
            "{}",
            f.name
        );
    }
    assert!(tried > 0, "no world had room for a city");
}

#[test]
fn an_auto_resolved_battle_is_begun_and_ended_and_leaves_nothing_open() {
    for mut f in faceoffs() {
        let events = f.w.apply(Command::AutoResolve {
            attackers: vec![f.attacker],
            defenders: vec![f.defender],
            mana_budget: None,
        });
        assert_eq!(refusal(&events), None, "{}: {events:?}", f.name);
        assert!(!f.w.in_battle(), "{}", f.name);
        assert!(
            told(&events, |e| matches!(e, Event::BattleStarted { .. })),
            "{}",
            f.name
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, Event::BattleEnded { .. }))
                .count(),
            1,
            "{}: {events:?}",
            f.name
        );
    }
}

#[test]
fn the_forecast_weighs_the_strongest_units_and_is_even_against_itself() {
    use f4x_def::rules::forecast::Forecast;
    for f in faceoffs() {
        let (own, enemy) = (&[f.attacker][..], &[f.defender][..]);
        assert!(
            f.w.side_worth(own) > 0.0,
            "{}: a spearman is worth nothing",
            f.name
        );
        assert_eq!(f.w.side_worth(&[]), 0.0, "{}", f.name);
        // A side of one against a side of the same kind is even.
        assert_eq!(
            f.w.combat_forecast(own, enemy),
            Some(Forecast::Even),
            "{}",
            f.name
        );
        // More of the same is stronger, and the other way round weaker.
        let two = [f.attacker, f.attacker];
        assert!(f.w.side_worth(&two) >= f.w.side_worth(own), "{}", f.name);
        // Nothing against nothing says nothing.
        assert_eq!(f.w.combat_forecast(&[], &[]), None, "{}", f.name);
        assert_eq!(
            f.w.combat_forecast(own, &[]),
            Some(Forecast::OtherOutmatched),
            "{}",
            f.name
        );
        assert_eq!(
            f.w.combat_forecast(&[], enemy),
            Some(Forecast::CompletelyOutmatched),
            "{}",
            f.name
        );
    }
}

/// The battle units of a side, by id.
fn ids(w: &World, side: Side) -> Vec<usize> {
    w.battle()
        .expect("a battle")
        .units
        .iter()
        .filter(|u| u.side == side)
        .map(|u| u.id)
        .collect()
}

fn mana(w: &World, p: PlayerId) -> HalfInt {
    w.player(p).expect("a player").treasury().mana
}

/// A face-off whose attacking wizard knows Fire Bolt and Heroism and has mana to spend.
fn casters() -> Vec<Faceoff> {
    faceoffs()
        .into_iter()
        .map(|mut f| {
            for spell in [83 /* fire_bolt */, 130 /* heroism */] {
                let id =
                    f.w.content()
                        .rules
                        .spells
                        .id_numbered(spell)
                        .expect("a spell");
                assert!(f.w.cheat_learn_spell(f.a, id));
            }
            f.w.treasury_mut(f.a).expect("a treasury").mana = HalfInt::from_int(200);
            start(&mut f);
            f
        })
        .collect()
}

fn spell(w: &World, number: u16) -> f4x_core::rules::SpellId {
    w.content()
        .rules
        .spells
        .id_numbered(number)
        .expect("a spell")
}

fn cast(w: &mut Table, number: u16, aim: SpellAim, mana: u32) -> Vec<Event> {
    let spell = spell(w, number);
    w.apply(Command::Battle(BattleCommand::Cast { spell, aim, mana }))
}

#[test]
fn a_battle_spell_is_paid_from_the_reserve_and_once_a_turn() {
    for mut f in casters() {
        let foe = ids(&f.w, Side::Defender)[0];
        let before = mana(&f.w, f.a);
        let events = cast(&mut f.w, 83 /* fire_bolt */, SpellAim::Unit(foe), 5);
        assert_eq!(battle_error(&events), None, "{}: {events:?}", f.name);
        let paid = events
            .iter()
            .find_map(|e| match e {
                Event::Battle(BattleEvent::SpellCast { mana, .. }) => Some(*mana),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{}: the cast is not told: {events:?}", f.name));
        assert!(paid > HalfInt::ZERO, "{}: a cast was free", f.name);
        assert_eq!(mana(&f.w, f.a), before - paid, "{}: the reserve", f.name);
        // A second spell in the same turn is refused, and costs nothing.
        let left = mana(&f.w, f.a);
        let again = cast(&mut f.w, 83, SpellAim::Unit(foe), 5);
        assert_eq!(
            battle_error(&again),
            Some(BattleError::Spell(SpellError::AlreadyCast)),
            "{}: {again:?}",
            f.name
        );
        assert_eq!(mana(&f.w, f.a), left, "{}", f.name);
    }
}

#[test]
fn a_battle_spell_on_the_wrong_target_or_not_known_is_refused_and_costs_nothing() {
    for mut f in casters() {
        let (mine, foe) = (ids(&f.w, Side::Attacker)[0], ids(&f.w, Side::Defender)[0]);
        let before = (mana(&f.w, f.a), state(&f.w));
        // Fire Bolt names an enemy unit, Heroism an own one.
        for (number, aim) in [
            (83, SpellAim::Unit(mine)),
            (130, SpellAim::Unit(foe)),
            (
                83,
                SpellAim::Tile(f.w.battle().expect("a battle").units[mine].cell),
            ),
        ] {
            let events = cast(&mut f.w, number, aim, 5);
            assert!(
                matches!(battle_error(&events), Some(BattleError::Spell(_))),
                "{}: spell {number}: {events:?}",
                f.name
            );
        }
        // A spell of the content the wizard does not know.
        let unknown =
            f.w.content()
                .rules
                .spells
                .iter()
                .find(|(id, d)| {
                    d.contexts.contains(&CastContext::Battle)
                        && !f.w.wizard(f.a).expect("a wizard").known().contains(id)
                })
                .map(|(_, d)| d.num)
                .expect("a battle spell the wizard does not know");
        let events = cast(&mut f.w, unknown, SpellAim::Unit(foe), 5);
        assert_eq!(
            battle_error(&events),
            Some(BattleError::Spell(SpellError::NotKnown)),
            "{}: {events:?}",
            f.name
        );
        assert_eq!(
            (mana(&f.w, f.a), state(&f.w)),
            before,
            "{}: a refused cast changed the game",
            f.name
        );
        // And the wizard can still cast after all that.
        let events = cast(&mut f.w, 83 /* fire_bolt */, SpellAim::Unit(foe), 5);
        assert_eq!(battle_error(&events), None, "{}: {events:?}", f.name);
    }
}

#[test]
fn the_wizard_of_the_side_to_play_casts_and_the_other_waits() {
    for mut f in casters() {
        // The defender's wizard knows no spell here, but the turn is the point: it is the
        // attacker's, and after it ends the defender's, and the attacker's cast is not allowed.
        let mine = ids(&f.w, Side::Attacker)[0];
        f.w.apply(Command::Battle(BattleCommand::EndTurn));
        assert_eq!(
            f.w.battle().expect("a battle").active,
            Side::Defender,
            "{}",
            f.name
        );
        let before = mana(&f.w, f.a);
        let events = cast(&mut f.w, 130, SpellAim::Unit(mine), 0);
        assert!(
            matches!(battle_error(&events), Some(BattleError::Spell(_))),
            "{}: the wizard cast on the other side's turn: {events:?}",
            f.name
        );
        assert_eq!(mana(&f.w, f.a), before, "{}", f.name);
    }
}

/// `n` more units of the kind of `f.defender`, on its square, owned by `owner`.
fn crowd(f: &mut Faceoff, owner: PlayerId, n: usize) -> Vec<hecs::Entity> {
    let at = f.w.units().unit_position(f.defender).expect("a position");
    let kind = f.w.units().unit_kind_id(f.defender).expect("a kind");
    for _ in 0..n {
        f.w.apply(Command::CreateUnit {
            owner,
            pos: at,
            unit: kind,
        });
    }
    f.w.units()
        .units_at(at)
        .into_iter()
        .filter(|&u| f.w.units().unit_owner(u) == Some(owner))
        .collect()
}

#[test]
fn the_winner_gains_experience_and_fame_for_the_kills_and_the_loser_loses_fame() {
    for mut f in faceoffs() {
        let renown = f.w.content().rules.config.renown.clone();
        let owner = f.b;
        let stack = crowd(&mut f, owner, renown.fame_kills);
        assert!(stack.len() >= renown.fame_kills, "{}", f.name);
        let (xp, fame_a, fame_b) = (
            f.w.units().unit_experience(f.attacker),
            f.w.fame(f.a),
            f.w.fame(f.b),
        );
        let events = f.w.apply(Command::BeginBattle {
            attackers: vec![f.attacker],
            defenders: stack.clone(),
        });
        assert_eq!(refusal(&events), None, "{}: {events:?}", f.name);
        let events = wipe_out(&mut f.w, Side::Defender);
        assert_eq!(
            f.w.units().unit_experience(f.attacker),
            xp + stack.len() as u32 * renown.xp_per_kill,
            "{}: experience for each kill",
            f.name
        );
        assert_eq!(f.w.fame(f.a), fame_a + 1, "{}: the winner's fame", f.name);
        // Global fame never falls below 0 (wiki article "Fame"), and a wizard starts with none.
        assert_eq!(
            f.w.fame(f.b),
            (fame_b - 1).max(0),
            "{}: the loser's fame",
            f.name
        );
        assert!(
            told(
                &events,
                |e| matches!(e, Event::BattleSpoils { player, fame: 1, loser_fame: -1, .. }
                if *player == f.a)
            ),
            "{}: {events:?}",
            f.name
        );
    }
}

#[test]
fn a_win_over_fewer_than_the_kills_for_fame_gives_experience_and_no_fame() {
    for mut f in faceoffs() {
        let renown = f.w.content().rules.config.renown.clone();
        let (xp, fame_a) = (f.w.units().unit_experience(f.attacker), f.w.fame(f.a));
        start(&mut f);
        wipe_out(&mut f.w, Side::Defender);
        assert!(
            renown.fame_kills > 1,
            "the content wants a single kill to give fame"
        );
        assert_eq!(
            f.w.units().unit_experience(f.attacker),
            xp + renown.xp_per_kill,
            "{}",
            f.name
        );
        assert_eq!(f.w.fame(f.a), fame_a, "{}", f.name);
    }
}

#[test]
fn monsters_that_win_get_nothing_and_a_city_they_beat_is_wrecked_not_taken() {
    use f4x_core::player::MONSTERS;
    let mut tried = 0;
    for mut f in faceoffs() {
        let race = {
            let c = f.w.economy().cities_of(f.b)[0].0;
            f.w.economy().city_race(c)
        };
        let at = f.w.units().unit_position(f.attacker).expect("a position");
        let kind = f.w.units().unit_kind_id(f.attacker).expect("a kind");
        let Some(site) = around(&f.w, at).into_iter().find(|&q| {
            f.w.units().units_at(q).is_empty()
                && f.w.economy().city_at(q).is_none()
                && f.w.economy().terrain_class_at(q) == Some(TerrainClass::Grassland)
        }) else {
            continue;
        };
        let city = f.w.scenario_found_city(f.b, site, "Prey", race, 3);
        for (owner, pos) in [(f.b, site), (MONSTERS, at)] {
            f.w.apply(Command::CreateUnit {
                owner,
                pos,
                unit: kind,
            });
        }
        let guard = f.w.units().units_at(site)[0];
        let beast =
            f.w.units()
                .units_at(at)
                .into_iter()
                .find(|&u| f.w.units().unit_owner(u) == Some(MONSTERS))
                .expect("a monster");
        tried += 1;
        let events = f.w.apply(Command::BeginBattle {
            attackers: vec![beast],
            defenders: vec![guard],
        });
        assert_eq!(refusal(&events), None, "{}: {events:?}", f.name);
        let events = wipe_out(&mut f.w, Side::Defender);
        assert!(
            !told(&events, |e| matches!(e, Event::BattleSpoils { .. })),
            "{}",
            f.name
        );
        assert!(
            !told(&events, |e| matches!(e, Event::CityCaptured { .. })),
            "{}",
            f.name
        );
        assert!(
            told(
                &events,
                |e| matches!(e, Event::CityRampaged { owner, .. } if *owner == f.b)
            ),
            "{}: {events:?}",
            f.name
        );
        assert!(
            f.w.economy().city_owner(city).is_none_or(|o| o == f.b),
            "{}: the monsters own the city",
            f.name
        );
        assert!(
            f.w.units().unit_position(beast).is_none_or(|p| p != site),
            "{}: the monsters moved into the city",
            f.name
        );
    }
    assert!(tried > 0, "no world had room for a city");
}

#[test]
fn defenders_beyond_the_nine_sit_the_fight_out_and_flee_when_it_is_lost() {
    for mut f in faceoffs() {
        let side = f.w.max_battle_side();
        let site = f.w.units().unit_position(f.defender).expect("a position");
        let owner = f.b;
        let stack = crowd(&mut f, owner, side + 3);
        assert!(stack.len() > side, "{}", f.name);
        // The attacker names the whole stack: only `side` of them fight.
        f.w.apply(Command::BeginBattle {
            attackers: vec![f.attacker],
            defenders: stack.clone(),
        });
        let fighting = ids(&f.w, Side::Defender).len();
        assert_eq!(fighting, side, "{}: the strongest {side} fight", f.name);
        wipe_out(&mut f.w, Side::Defender);
        // The attacker won and took the square: those who sat it out ran or were lost.
        let left_on_site =
            f.w.units()
                .units_at(site)
                .into_iter()
                .filter(|&u| f.w.units().unit_owner(u) == Some(f.b))
                .count();
        assert_eq!(
            left_on_site, 0,
            "{}: defenders stayed on a lost square",
            f.name
        );
        assert_eq!(
            f.w.units().unit_position(f.attacker),
            Some(site),
            "{}: the winner did not take the square",
            f.name
        );
    }
}
