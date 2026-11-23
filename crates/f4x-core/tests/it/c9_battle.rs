//! Class 9, the battle: how one starts, who may be in it, how it ends and what the end changes.
//!
//! The battles here are begun on the fresh worlds: a unit of the first wizard and a unit of the second
//! on neighbouring squares. No battle AI is set in this crate (`set_battle_ai` belongs to `f4x-ai`), so
//! what plays a side is the test.

use crate::support::{around, text, worlds};
use f4x_battle::battle::{BattleCommand, BattleError, Side};
use f4x_core::battle::game::BattleStartError;
use f4x_core::diplomacy::{DiplomacyCommand as D, Treaty};
use f4x_core::game::{Command, Event, Game, Rejection};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::player::PlayerId;
use f4x_def::half_int::HalfInt;
use f4x_def::rules::Stat;

/// A world with two stacks face to face: one unit of `a` and one unit of `b` on neighbouring squares.
struct Faceoff {
    name: String,
    g: Game,
    a: PlayerId,
    b: PlayerId,
    attacker: hecs::Entity,
    defender: hecs::Entity,
}

fn faceoffs() -> Vec<Faceoff> {
    let mut out = Vec::new();
    for (name, mut g) in worlds() {
        let w = g.wizards();
        let (a, b) = (w[0], w[1]);
        let Some(attacker) = g.units().all_units().into_iter().find(|&u| {
            g.units().unit_owner(u) == Some(a)
                && g.units().unit_def(u).is_some_and(|d| {
                    d.stats
                        .get(&Stat::Melee)
                        .is_some_and(|m| *m > HalfInt::ZERO)
                })
        }) else {
            continue;
        };
        let at = g.units().unit_position(attacker).expect("a position");
        let kind = g.units().unit_kind_id(attacker).expect("a kind");
        let Some(to) = around(&g, at).into_iter().find(|&to| {
            g.units().units_at(to).is_empty()
                && g.economy().city_at(to).is_none()
                && g.economy().terrain_class_at(to) == Some(TerrainClass::Grassland)
                && g.apply(Command::CreateUnit {
                    owner: b,
                    pos: to,
                    unit: kind,
                })
                .iter()
                .all(|e| !matches!(e, Event::Rejected(_)))
        }) else {
            continue;
        };
        let defender = g.units().units_at(to)[0];
        out.push(Faceoff {
            name,
            g,
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
    f.g.apply(Command::BeginBattle {
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
        let site = f.g.units().unit_position(f.defender).expect("a position");
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
        assert!(f.g.in_battle(), "{}", f.name);
        let battle = f.g.battle().expect("a battle");
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
            let before = text(&f.g);
            let events = f.g.apply(Command::BeginBattle {
                attackers,
                defenders,
            });
            assert_eq!(
                refusal(&events),
                Some(BattleStartError::BadSides),
                "{name}: {events:?}"
            );
            assert_eq!(
                text(&f.g),
                before,
                "{name}: a refused battle changed the game"
            );
        }
        for treaty in [Treaty::Pact, Treaty::Alliance] {
            f.g.make_contact(f.a, f.b);
            f.g.start_treaty(f.a, f.b, treaty);
            let before = text(&f.g);
            let events = start(&mut f);
            assert_eq!(
                refusal(&events),
                Some(BattleStartError::Treaty),
                "{name}: {treaty:?}: {events:?}"
            );
            assert_eq!(
                text(&f.g),
                before,
                "{name}: a refused battle changed the game"
            );
        }
        f.g.apply(Command::Diplomacy(D::BreakTreaty {
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
        let at = f.g.units().unit_position(f.attacker).expect("a position");
        // A unit of the defender's wizard two squares or more away from the attacker.
        let kind = f.g.units().unit_kind_id(f.attacker).expect("a kind");
        let far = around(&f.g, at)
            .into_iter()
            .flat_map(|n| around(&f.g, n))
            .find(|&q| {
                f.g.map()
                    .expect("a map")
                    .plane(q.plane)
                    .chebyshev((q.x, q.y), (at.x, at.y))
                    > 1
                    && f.g.units().units_at(q).is_empty()
                    && f.g.economy().city_at(q).is_none()
            });
        let Some(far) = far else { continue };
        let made = f.g.apply(Command::CreateUnit {
            owner: f.b,
            pos: far,
            unit: kind,
        });
        if made.iter().any(|e| matches!(e, Event::Rejected(_))) {
            continue;
        }
        let distant = f.g.units().units_at(far)[0];
        let before = text(&f.g);
        let events = f.g.apply(Command::AutoResolve {
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
        assert_eq!(text(&f.g), before, "{}", f.name);
    }
}

#[test]
fn a_battle_command_without_a_battle_is_over() {
    for mut f in faceoffs() {
        let before = text(&f.g);
        let events = f.g.apply(Command::Battle(BattleCommand::EndTurn));
        assert_eq!(battle_error(&events), Some(BattleError::Over), "{}", f.name);
        assert_eq!(text(&f.g), before, "{}", f.name);
    }
}

#[test]
fn fleeing_gives_the_battle_to_the_other_side_and_closes_it() {
    for mut f in faceoffs() {
        start(&mut f);
        let events = f.g.apply(Command::Battle(BattleCommand::Flee));
        assert_eq!(battle_error(&events), None, "{}: {events:?}", f.name);
        assert!(!f.g.in_battle(), "{}: the battle stayed open", f.name);
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
        for u in f.g.units().all_units() {
            let at = f.g.units().unit_position(u);
            let owners: Vec<_> =
                f.g.units()
                    .units_at(at.expect("a position"))
                    .into_iter()
                    .filter_map(|v| f.g.units().unit_owner(v))
                    .collect();
            assert!(
                owners.windows(2).all(|p| p[0] == p[1]),
                "{}: two wizards share a square after the battle",
                f.name
            );
        }
        // A new battle can be begun at once, if both sides still have a unit.
        assert!(!f.g.in_battle());
    }
}

#[test]
fn a_battle_played_to_the_end_tells_one_end_and_leaves_no_battle_open() {
    for mut f in faceoffs() {
        start(&mut f);
        let mut ended = Vec::new();
        for _ in 0..5_000 {
            if !f.g.in_battle() {
                break;
            }
            // The side to play attacks the nearest enemy, or ends its turn.
            let battle = f.g.battle().expect("a battle");
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
                    let ev = f.g.apply(Command::Battle(BattleCommand::Attack {
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
                ended.extend(f.g.apply(Command::Battle(BattleCommand::EndTurn)));
            }
        }
        // Whatever stalls ends by fleeing, as `auto_resolve` does.
        if f.g.in_battle() {
            ended.extend(f.g.apply(Command::Battle(BattleCommand::Flee)));
        }
        assert!(!f.g.in_battle(), "{}", f.name);
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
    for (name, mut g) in worlds() {
        let w = g.wizards();
        let (a, b) = (w[0], w[1]);
        let (width, height) = {
            let map = g.map().expect("a map");
            (map.width(), map.height())
        };
        let sea = |g: &Game, x: u32, y: u32| {
            let at = Position {
                x,
                y,
                plane: Plane::Arcanus,
            };
            (g.units().units_at(at).is_empty()
                && g.economy().city_at(at).is_none()
                && g.economy().terrain_class_at(at) == Some(TerrainClass::Ocean))
            .then_some(at)
        };
        let pair = (0..height).find_map(|y| {
            (0..width.saturating_sub(1)).find_map(|x| Some((sea(&g, x, y)?, sea(&g, x + 1, y)?)))
        });
        let Some((from, to)) = pair else { continue };
        for (owner, pos) in [(a, from), (b, to)] {
            let made = g.apply(Command::CreateUnit {
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
        let (attacker, defender) = (g.units().units_at(from)[0], g.units().units_at(to)[0]);
        let events = g.apply(Command::BeginBattle {
            attackers: vec![attacker],
            defenders: vec![defender],
        });
        assert_eq!(refusal(&events), None, "{name}: {events:?}");
        let battle = g.battle().expect("a battle");
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
    for (name, mut g) in worlds() {
        let w = g.wizards();
        let (a, b) = (w[0], w[1]);
        let (width, height) = {
            let map = g.map().expect("a map");
            (map.width(), map.height())
        };
        let sea = |g: &Game, x: u32, y: u32| {
            let at = Position {
                x,
                y,
                plane: Plane::Arcanus,
            };
            (g.units().units_at(at).is_empty()
                && g.economy().city_at(at).is_none()
                && g.economy().terrain_class_at(at) == Some(TerrainClass::Ocean))
            .then_some(at)
        };
        let pair = (0..height).find_map(|y| {
            (0..width.saturating_sub(1)).find_map(|x| Some((sea(&g, x, y)?, sea(&g, x + 1, y)?)))
        });
        let Some((from, to)) = pair else { continue };
        for (owner, pos) in [(a, from), (b, to)] {
            let made = g.apply(Command::CreateUnit {
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
        let (attacker, defender) = (g.units().units_at(from)[0], g.units().units_at(to)[0]);
        let events = g.apply(Command::BeginBattle {
            attackers: vec![attacker],
            defenders: vec![defender],
        });
        assert_eq!(refusal(&events), None, "{name}: {events:?}");
        let battle = g.battle().expect("a battle");
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
