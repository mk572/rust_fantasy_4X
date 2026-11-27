//! Class 9, diplomacy: meeting, proposals and their answers, treaties, war and peace, gifts.

use crate::support::worlds;
use f4x_core::diplomacy::{
    DiplomacyCommand as D, DiplomacyError, DiplomacyEvent, ProposalKind, Treaty,
};
use f4x_core::player::PlayerId;
use f4x_core::rules::UnitId;
use f4x_core::world::{Command, Event, Rejection, World};
use f4x_def::half_int::HalfInt;
use f4x_master::Game;

/// The result of a diplomacy command: the reason it was refused, if it was.
fn run(w: &mut World, command: D) -> Option<DiplomacyError> {
    w.apply(Command::Diplomacy(command))
        .iter()
        .find_map(|e| match e {
            Event::Rejected(Rejection::Diplomacy(why)) => Some(*why),
            _ => None,
        })
}

/// Every world with its wizards, that have all met.
fn met() -> Vec<(String, World, Vec<PlayerId>)> {
    worlds()
        .into_iter()
        .map(|(name, mut w)| {
            let ids = w.wizards();
            for &a in &ids {
                for &b in &ids {
                    w.make_contact(a, b);
                }
            }
            (name, w, ids)
        })
        .collect()
}

fn gold(w: &World, p: PlayerId) -> HalfInt {
    w.player(p).expect("a player").treasury().gold
}

#[test]
fn wizards_meet_once_and_nothing_is_done_before() {
    for (name, mut w) in worlds() {
        let ids = w.wizards();
        let (a, b) = (ids[0], ids[1]);
        assert!(!w.have_met(a, b), "{name}: they have met at the start");
        for command in [
            D::Greeted { player: a, with: b },
            D::DeclareWar { player: a, on: b },
            D::Warn { from: a, to: b },
            D::GiveGold {
                from: a,
                to: b,
                amount: 1,
            },
            D::Propose {
                from: a,
                to: b,
                kind: ProposalKind::Pact,
            },
        ] {
            assert_eq!(
                run(&mut w, command.clone()),
                Some(DiplomacyError::NotContacted),
                "{name}: {command:?}"
            );
        }
        assert!(w.make_contact(a, b), "{name}: first time");
        assert!(!w.make_contact(b, a), "{name}: second time, either way");
        assert!(w.have_met(a, b) && w.have_met(b, a));
        assert!(!w.make_contact(a, a), "{name}: nobody meets themselves");
        assert_eq!(w.treaty(a, b), Treaty::None);
    }
}

#[test]
fn a_pact_comes_when_it_is_answered_yes_and_not_when_it_is_answered_no() {
    for (name, mut w, ids) in met() {
        let (a, b) = (ids[0], ids[1]);
        let pact = ProposalKind::Pact;
        assert_eq!(
            run(
                &mut w,
                D::Propose {
                    from: a,
                    to: b,
                    kind: pact
                }
            ),
            None,
            "{name}"
        );
        // One offer at a time from a to b; nothing is settled until it is answered.
        assert_eq!(
            run(
                &mut w,
                D::Propose {
                    from: a,
                    to: b,
                    kind: pact
                }
            ),
            Some(DiplomacyError::AlreadyPending),
            "{name}"
        );
        assert_eq!(w.treaty(a, b), Treaty::None);
        // It is the receiver who answers.
        assert_eq!(
            run(
                &mut w,
                D::Answer {
                    player: a,
                    from: b,
                    accept: true
                }
            ),
            Some(DiplomacyError::NothingPending),
            "{name}"
        );
        assert_eq!(
            run(
                &mut w,
                D::Answer {
                    player: b,
                    from: a,
                    accept: false
                }
            ),
            None,
            "{name}"
        );
        assert_eq!(w.treaty(a, b), Treaty::None, "{name}: turned down");
        // The answer used the offer up.
        assert_eq!(
            run(
                &mut w,
                D::Answer {
                    player: b,
                    from: a,
                    accept: true
                }
            ),
            Some(DiplomacyError::NothingPending),
            "{name}"
        );
        run(
            &mut w,
            D::Propose {
                from: a,
                to: b,
                kind: pact,
            },
        );
        assert_eq!(
            run(
                &mut w,
                D::Answer {
                    player: b,
                    from: a,
                    accept: true
                }
            ),
            None,
            "{name}"
        );
        assert_eq!(w.treaty(a, b), Treaty::Pact, "{name}");
        assert_eq!(w.treaty(b, a), Treaty::Pact, "{name}: both ways");
        // What the treaty allows is asked of the game: a pact once more is not.
        assert_eq!(
            w.check_proposal(a, b, ProposalKind::Pact),
            Err(DiplomacyError::NotAllowed),
            "{name}"
        );
        assert_eq!(
            w.check_proposal(a, b, ProposalKind::Alliance),
            Ok(()),
            "{name}"
        );
        assert_eq!(
            w.check_proposal(a, b, ProposalKind::Peace),
            Err(DiplomacyError::NotAllowed),
            "{name}: peace is for war"
        );
    }
}

#[test]
fn breaking_a_treaty_is_told_and_only_a_treaty_can_be_broken() {
    for (name, mut w, ids) in met() {
        let (a, b) = (ids[0], ids[1]);
        assert_eq!(
            run(&mut w, D::BreakTreaty { player: a, with: b }),
            Some(DiplomacyError::NotAllowed),
            "{name}: nothing to break"
        );
        w.start_treaty(a, b, Treaty::Alliance);
        assert_eq!(w.treaty(b, a), Treaty::Alliance);
        let events = w.apply(Command::Diplomacy(D::BreakTreaty { player: a, with: b }));
        assert!(!events.iter().any(|e| matches!(e, Event::Rejected(_))));
        assert_eq!(w.treaty(a, b), Treaty::None, "{name}");
        assert_eq!(w.treaty(b, a), Treaty::None, "{name}");
        // The break is told in the events of the command.
        let broken = events.into_iter().any(|e| {
            matches!(
                e,
                Event::Diplomacy(f4x_core::diplomacy::DiplomacyEvent::TreatyBroken {
                    breaker, victim, treaty: Treaty::Alliance
                }) if breaker == a && victim == b
            )
        });
        assert!(broken, "{name}: no TreatyBroken");
    }
}

#[test]
fn war_is_declared_once_ends_a_treaty_and_ends_in_peace() {
    for (name, mut w, ids) in met() {
        let (a, b) = (ids[0], ids[1]);
        w.start_treaty(a, b, Treaty::Pact);
        assert_eq!(
            run(&mut w, D::DeclareWar { player: a, on: b }),
            None,
            "{name}"
        );
        assert_eq!(w.treaty(a, b), Treaty::War, "{name}");
        assert_eq!(w.treaty(b, a), Treaty::War, "{name}");
        assert_eq!(w.war_age(a, b), Some(0), "{name}");
        assert_eq!(
            run(&mut w, D::DeclareWar { player: b, on: a }),
            Some(DiplomacyError::NotAllowed),
            "{name}: already at war"
        );
        // At war only peace may be asked, and no treaty can be broken.
        assert_eq!(
            w.check_proposal(a, b, ProposalKind::Pact),
            Err(DiplomacyError::NotAllowed),
            "{name}"
        );
        assert_eq!(
            w.check_proposal(a, b, ProposalKind::Peace),
            Ok(()),
            "{name}"
        );
        assert_eq!(
            run(&mut w, D::BreakTreaty { player: a, with: b }),
            Some(DiplomacyError::NotAllowed),
            "{name}"
        );
        assert_eq!(
            run(
                &mut w,
                D::Propose {
                    from: a,
                    to: b,
                    kind: ProposalKind::Peace
                }
            ),
            None,
            "{name}"
        );
        run(
            &mut w,
            D::Answer {
                player: b,
                from: a,
                accept: true,
            },
        );
        assert_eq!(w.treaty(a, b), Treaty::None, "{name}: peace");
        assert_eq!(w.war_age(a, b), None, "{name}");
    }
}

#[test]
fn a_war_on_a_third_wizard_is_asked_only_of_one_who_knows_them_both() {
    for (name, mut w, ids) in met() {
        let (a, b) = (ids[0], ids[1]);
        let Some(&c) = ids.get(2) else {
            // With two wizards there is no third to name.
            assert_eq!(
                w.check_proposal(a, b, ProposalKind::WarOn(a)),
                Err(DiplomacyError::NotAllowed),
                "{name}"
            );
            continue;
        };
        assert_eq!(
            w.check_proposal(a, b, ProposalKind::WarOn(c)),
            Ok(()),
            "{name}"
        );
        assert_eq!(
            w.check_proposal(a, b, ProposalKind::WarOn(b)),
            Err(DiplomacyError::NotAllowed),
            "{name}: not on the one asked"
        );
        // Asked to break an alliance that is not there, or to go to war with an ally.
        assert_eq!(
            w.check_proposal(a, b, ProposalKind::BreakAllianceWith(c)),
            Err(DiplomacyError::NotAllowed),
            "{name}"
        );
        w.start_treaty(b, c, Treaty::Alliance);
        assert_eq!(
            w.check_proposal(a, b, ProposalKind::WarOn(c)),
            Err(DiplomacyError::NotAllowed),
            "{name}: b is allied with c"
        );
        assert_eq!(
            w.check_proposal(a, b, ProposalKind::BreakAllianceWith(c)),
            Ok(()),
            "{name}"
        );
        // Accepted, the one asked breaks the alliance.
        run(
            &mut w,
            D::Propose {
                from: a,
                to: b,
                kind: ProposalKind::BreakAllianceWith(c),
            },
        );
        assert_eq!(
            run(
                &mut w,
                D::Answer {
                    player: b,
                    from: a,
                    accept: true
                }
            ),
            None
        );
        assert_eq!(w.treaty(b, c), Treaty::None, "{name}");
    }
}

#[test]
fn a_gift_of_gold_moves_the_gold_and_is_told() {
    for (name, mut w, ids) in met() {
        let (a, b) = (ids[0], ids[1]);
        w.treasury_mut(a).expect("a treasury").gold = HalfInt::from_int(100);
        let (had_a, had_b) = (gold(&w, a), gold(&w, b));
        assert_eq!(
            run(
                &mut w,
                D::GiveGold {
                    from: a,
                    to: b,
                    amount: 40
                }
            ),
            None,
            "{name}"
        );
        assert_eq!(gold(&w, a), had_a - HalfInt::from_int(40), "{name}");
        assert_eq!(gold(&w, b), had_b + HalfInt::from_int(40), "{name}");
        // What the giver has not got, or nothing, or less than nothing, cannot be given.
        for amount in [101, 0, -5, i32::MAX, i32::MIN] {
            let (ga, gb) = (gold(&w, a), gold(&w, b));
            assert_eq!(
                run(
                    &mut w,
                    D::GiveGold {
                        from: a,
                        to: b,
                        amount
                    }
                ),
                Some(DiplomacyError::NotEnoughGold),
                "{name}: {amount}"
            );
            assert_eq!((gold(&w, a), gold(&w, b)), (ga, gb), "{name}: {amount}");
        }
        assert_eq!(
            run(
                &mut w,
                D::GiveGold {
                    from: a,
                    to: a,
                    amount: 1
                }
            ),
            Some(DiplomacyError::SameWizard),
            "{name}"
        );
    }
}

#[test]
fn a_demand_is_paid_when_it_is_yielded_to_and_not_when_the_receiver_cannot_pay() {
    for (name, mut w, ids) in met() {
        let (a, b) = (ids[0], ids[1]);
        w.treasury_mut(b).expect("a treasury").gold = HalfInt::from_int(50);
        let (had_a, had_b) = (gold(&w, a), gold(&w, b));
        run(
            &mut w,
            D::Propose {
                from: a,
                to: b,
                kind: ProposalKind::Demand { gold: 30 },
            },
        );
        assert_eq!(
            run(
                &mut w,
                D::Answer {
                    player: b,
                    from: a,
                    accept: true
                }
            ),
            None
        );
        assert_eq!(gold(&w, b), had_b - HalfInt::from_int(30), "{name}");
        assert_eq!(gold(&w, a), had_a + HalfInt::from_int(30), "{name}");
        // Twenty is left: a demand for more than that is answered yes, and not paid.
        let (ga, gb) = (gold(&w, a), gold(&w, b));
        run(
            &mut w,
            D::Propose {
                from: a,
                to: b,
                kind: ProposalKind::Demand { gold: 500 },
            },
        );
        run(
            &mut w,
            D::Answer {
                player: b,
                from: a,
                accept: true,
            },
        );
        assert_eq!((gold(&w, a), gold(&w, b)), (ga, gb), "{name}");
        // A demand for nothing is no demand.
        assert_eq!(
            run(
                &mut w,
                D::Propose {
                    from: a,
                    to: b,
                    kind: ProposalKind::Demand { gold: 0 }
                }
            ),
            Some(DiplomacyError::NotAllowed),
            "{name}"
        );
    }
}

#[test]
fn a_banished_wizard_starts_nothing_but_still_answers() {
    for (name, mut w, ids) in met() {
        let (a, b) = (ids[0], ids[1]);
        run(
            &mut w,
            D::Propose {
                from: b,
                to: a,
                kind: ProposalKind::Pact,
            },
        );
        // The fortress of a falls: a snapshot of the game with a banished is restored.
        let mut snap = w.snapshot();
        let save = snap
            .wizards
            .iter_mut()
            .find(|s| s.player == a.0)
            .expect("a save");
        save.banished = true;
        save.fortress = None;
        save.summon_at = None;
        save.more_circles.clear();
        w = snap.restore(crate::support::content()).expect("restores");
        assert!(
            w.wizard(a).is_some_and(|wizard| wizard.is_banished()),
            "{name}"
        );
        for command in [
            D::Propose {
                from: a,
                to: b,
                kind: ProposalKind::Alliance,
            },
            D::DeclareWar { player: a, on: b },
            D::GiveGold {
                from: a,
                to: b,
                amount: 1,
            },
            D::Warn { from: a, to: b },
        ] {
            assert_eq!(
                run(&mut w, command.clone()),
                Some(DiplomacyError::Banished),
                "{name}: {command:?}"
            );
        }
        assert_eq!(
            run(
                &mut w,
                D::Answer {
                    player: a,
                    from: b,
                    accept: true
                }
            ),
            None,
            "{name}"
        );
    }
}

#[test]
fn the_game_never_changes_a_treaty_by_itself() {
    // Taking a city costs the winner standing but starts no war (`docs/diplomacy.md` 9.2).
    for (name, w, ids) in met() {
        let mut w = Game::new(w);
        let (a, b) = (ids[0], ids[1]);
        let city = w
            .economy()
            .cities()
            .into_iter()
            .find(|c| c.1.owner == b && w.economy().citizens(c.0) > 0)
            .expect("a city with people")
            .0;
        w.conquer_city(city, a, 0);
        assert_eq!(
            w.treaty(a, b),
            Treaty::None,
            "{name}: a city taken is no war"
        );
        // Nor do the turns that follow start one: a pact is ended only by a player.
        w.start_treaty(a, b, Treaty::Pact);
        for _ in 0..6 {
            w.play_round();
        }
        assert_ne!(w.treaty(a, b), Treaty::War, "{name}: war came by itself");
    }
}

/// A race player with a city, of those in the world.
fn race_with_a_city(w: &World) -> PlayerId {
    w.player_ids()
        .into_iter()
        .find(|&p| w.race_of_player(p).is_some() && !w.economy().cities_of(p).is_empty())
        .expect("an independent race")
}

fn independence_lost(event: &Event, race: PlayerId) -> bool {
    matches!(
        event,
        Event::Diplomacy(DiplomacyEvent::IndependenceLost { player }) if *player == race
    )
}

#[test]
fn a_raiding_race_that_razes_what_it_took_loses_no_independence() {
    for (name, mut w, ids) in met() {
        let a = ids[0];
        let raiders = w
            .player_ids()
            .into_iter()
            .find(|&p| w.is_dormant(p))
            .expect("a race with nothing");
        let (city, owner) = w
            .economy()
            .cities()
            .into_iter()
            .map(|c| (c.0, c.1.owner))
            .find(|&(c, p)| w.race_of_player(p).is_some() && w.economy().citizens(c) > 0)
            .expect("an independent city");
        let (kind, at) = w
            .units()
            .all_units()
            .into_iter()
            .find_map(|u| Some((w.units().unit_def(u)?.num, w.units().unit_position(u)?)))
            .expect("a unit");
        w.spawn_unit(raiders, at, UnitId::new(kind));
        w.make_contact(a, raiders);
        w.declare_war(a, raiders);
        w.drain_diplomacy_events();
        w.conquer_city(city, raiders, 0);
        w.apply(Command::RazeCity {
            player: raiders,
            city,
        });
        assert!(w.economy().cities_of(raiders).is_empty(), "{name}: razed");
        assert_eq!(w.treaty(a, raiders), Treaty::War, "{name}");
        let told = w.drain_diplomacy_events();
        assert!(
            !told.iter().any(|e| independence_lost(e, raiders)),
            "{name}: {told:?} about {owner:?}"
        );
    }
}

#[test]
fn a_race_that_loses_its_last_city_has_no_relations_left() {
    for (name, mut w, ids) in met() {
        let (a, b) = (ids[0], ids[1]);
        let race = race_with_a_city(&w);
        w.make_contact(a, race);
        w.make_contact(b, race);
        w.start_treaty(a, race, Treaty::Pact);
        w.declare_war(b, race);
        assert_eq!(
            run(
                &mut w,
                D::Propose {
                    from: b,
                    to: race,
                    kind: ProposalKind::Peace
                }
            ),
            None,
            "{name}"
        );
        let cities: Vec<_> = w
            .economy()
            .cities()
            .into_iter()
            .filter(|c| c.1.owner == race)
            .map(|c| c.0)
            .collect();
        let (last, others) = cities.split_last().expect("a city");
        w.drain_diplomacy_events();
        for &city in others {
            w.capture_city(city, a);
        }
        let told = w.drain_diplomacy_events();
        assert!(!told.iter().any(|e| independence_lost(e, race)), "{name}");
        assert_eq!(w.treaty(a, race), Treaty::Pact, "{name}: a city is left");
        assert_eq!(w.treaty(b, race), Treaty::War, "{name}: a city is left");
        assert!(!w.diplomacy.proposals().is_empty(), "{name}");
        w.capture_city(*last, a);
        for p in [a, b] {
            assert_eq!(w.treaty(p, race), Treaty::None, "{name}");
            assert_eq!(w.diplomacy.war_age(p, race), None, "{name}");
            assert!(w.have_met(p, race), "{name}: they know each other");
        }
        assert!(
            w.diplomacy
                .proposals()
                .iter()
                .all(|p| p.from != race && p.to != race),
            "{name}"
        );
        let told = w.drain_diplomacy_events();
        let news = told.iter().filter(|e| independence_lost(e, race)).count();
        assert_eq!(news, 1, "{name}: {told:?}");
        assert!(
            told.iter().any(|e| matches!(
                e,
                Event::Diplomacy(DiplomacyEvent::TreatyChanged {
                    treaty: Treaty::None,
                    ..
                })
            )),
            "{name}: {told:?}"
        );
    }
}

#[test]
fn a_razed_or_ruined_last_city_ends_the_relations_too() {
    for (name, mut w, ids) in met() {
        let a = ids[0];
        let race = race_with_a_city(&w);
        w.make_contact(a, race);
        w.declare_war(a, race);
        let cities: Vec<_> = w
            .economy()
            .cities()
            .into_iter()
            .filter(|c| c.1.owner == race)
            .map(|c| c.0)
            .collect();
        let (kind, at) = w
            .units()
            .all_units()
            .into_iter()
            .find_map(|u| Some((w.units().unit_def(u)?.num, w.units().unit_position(u)?)))
            .expect("a unit");
        let abroad = w.spawn_unit(race, at, UnitId::new(kind));
        for city in cities {
            let events = w.destroy_city(city).expect("an independent city goes");
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, Event::WizardDefeated { .. })),
                "{name}: {events:?}"
            );
        }
        assert_eq!(w.treaty(a, race), Treaty::None, "{name}");
        // The race is not out of the game: what it has elsewhere stays.
        assert!(!w.player(race).expect("a player").is_defeated(), "{name}");
        assert!(w.units().player_units(race).contains(&abroad), "{name}");
        assert!(!w.is_dormant(race), "{name}");
    }
}

#[test]
fn a_race_without_a_city_that_loses_its_last_unit_has_no_relations_left() {
    for (name, mut w, ids) in met() {
        let a = ids[0];
        let race = w
            .player_ids()
            .into_iter()
            .find(|&p| w.is_dormant(p))
            .expect("a race with nothing");
        // It has a unit and no city, as a raiding party does.
        let (kind, at) = w
            .units()
            .all_units()
            .into_iter()
            .find_map(|u| Some((w.units().unit_def(u)?.num, w.units().unit_position(u)?)))
            .expect("a unit");
        let unit = w.spawn_unit(race, at, UnitId::new(kind));
        w.make_contact(a, race);
        w.declare_war(a, race);
        assert_eq!(w.treaty(a, race), Treaty::War, "{name}");
        w.apply(Command::DismissUnit { unit });
        assert!(w.is_dormant(race), "{name}");
        assert_eq!(w.treaty(a, race), Treaty::None, "{name}");
    }
}

#[test]
fn an_alliance_stands_though_one_ally_is_at_war_with_the_others_ally() {
    for (name, w, ids) in met().into_iter().filter(|m| m.2.len() >= 3) {
        let mut w = Game::new(w);
        let (a, b, c) = (ids[0], ids[1], ids[2]);
        w.start_treaty(a, b, Treaty::Alliance);
        w.start_treaty(b, c, Treaty::Alliance);
        w.declare_war(a, c);
        for _ in 0..3 {
            w.play_round();
        }
        assert_eq!(
            w.treaty(a, b),
            Treaty::Alliance,
            "{name}: the game split it"
        );
        assert_eq!(
            w.treaty(b, c),
            Treaty::Alliance,
            "{name}: the game split it"
        );
    }
}

#[test]
fn an_attack_on_a_treaty_partner_is_refused_until_a_player_breaks_the_treaty() {
    use crate::support::{around, text};
    use f4x_core::unit::movement::MoveError;
    let mut tried = 0;
    for (name, mut w, ids) in met() {
        let (a, b) = (ids[0], ids[1]);
        let Some(mover) = w
            .units()
            .all_units()
            .into_iter()
            .find(|&u| w.units().unit_owner(u) == Some(a))
        else {
            continue;
        };
        let at = w.units().unit_position(mover).expect("a position");
        let kind = w.units().unit_kind_id(mover).expect("a kind");
        // A unit of b on a square next to it that the mover can attack.
        let Some(to) = around(&w, at).into_iter().find(|&to| {
            let made = w.apply(Command::CreateUnit {
                owner: b,
                pos: to,
                unit: kind,
            });
            let asked = made.iter().all(|e| !matches!(e, Event::Rejected(_)))
                && w.apply(Command::MoveStack {
                    units: vec![mover],
                    to,
                })
                .iter()
                .any(|e| matches!(e, Event::AttackRequested { .. }));
            if !asked {
                for u in w.units().units_at(to) {
                    w.scenario_remove_unit(u);
                }
            }
            asked
        }) else {
            continue;
        };
        tried += 1;
        for treaty in [Treaty::Pact, Treaty::Alliance] {
            w.start_treaty(a, b, treaty);
            let before = text(&w);
            let events = w.apply(Command::MoveStack {
                units: vec![mover],
                to,
            });
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, Event::Rejected(Rejection::Move(MoveError::Treaty)))),
                "{name}: {treaty:?}: {events:?}"
            );
            assert_eq!(
                text(&w),
                before,
                "{name}: a refused attack changed the game"
            );
            assert_eq!(w.treaty(a, b), treaty, "{name}: the game broke the treaty");
        }
        // The player breaks it, and the attack is asked for.
        assert_eq!(
            run(&mut w, D::BreakTreaty { player: a, with: b }),
            None,
            "{name}"
        );
        let events = w.apply(Command::MoveStack {
            units: vec![mover],
            to,
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::AttackRequested { .. })),
            "{name}: {events:?}"
        );
    }
    assert!(tried > 0, "no world had room for the test");
}

#[test]
fn what_happened_is_told_in_events_and_no_feeling_is_kept_by_the_game() {
    use f4x_core::diplomacy::DiplomacyEvent as E;
    for (name, mut w, ids) in met() {
        let (a, b) = (ids[0], ids[1]);
        w.treasury_mut(a).expect("a treasury").gold = HalfInt::from_int(100);
        let gift = w.apply(Command::Diplomacy(D::GiveGold {
            from: a,
            to: b,
            amount: 40,
        }));
        assert!(
            gift.iter()
                .any(|e| matches!(e, Event::Diplomacy(E::GoldGiven { from, to, gold: 40 }) if *from == a && *to == b)),
            "{name}: a gift is told"
        );
        let war = w.apply(Command::Diplomacy(D::DeclareWar { player: a, on: b }));
        assert!(
            war.iter()
                .any(|e| matches!(e, Event::Diplomacy(E::WarDeclared { attacker, defender }) if *attacker == a && *defender == b)),
            "{name}: a war is told"
        );
        // No event says how a wizard feels: that is for each AI to decide.
        assert!(
            !format!("{gift:?}{war:?}").contains("RelationChanged"),
            "{name}"
        );
    }
}
