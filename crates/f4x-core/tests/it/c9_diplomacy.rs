//! Class 9, diplomacy: meeting, proposals and their answers, treaties, war and peace, gifts.

use crate::support::{around, content, state, text, worlds};
use f4x_core::diplomacy::{
    Deal, DiplomacyCommand as D, DiplomacyError, DiplomacyEvent, Goods, ProposalKind, Treaty,
};
use f4x_core::map::{Plane, Position};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::rules::UnitId;
use f4x_core::world::save::Snapshot;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event, Rejection, World};
use f4x_def::half_int::HalfInt;

/// The result of a diplomacy command: the reason it was refused, if it was.
fn run(w: &mut Table, command: D) -> Option<DiplomacyError> {
    w.apply(Command::Diplomacy(command))
        .iter()
        .find_map(|e| match e {
            Event::Rejected(Rejection::Diplomacy(why)) => Some(*why),
            _ => None,
        })
}

/// Every world with its wizards, that have all met.
fn met() -> Vec<(String, Table, Vec<PlayerId>)> {
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
        w.world = snap.restore(crate::support::content()).expect("restores");
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
fn an_attack_on_a_treaty_partner_is_refused_until_a_player_breaks_the_treaty() {
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
            let before = state(&w);
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
                state(&w),
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

/// Every world in which the second wizard has a spearman next to a city of the first, the two
/// bound by a pact, with that unit.
fn neighbours() -> Vec<(String, Table, PlayerId, PlayerId, hecs::Entity)> {
    let found: Vec<_> = met()
        .into_iter()
        .filter_map(|(name, mut w, ids)| {
            let (a, b) = (ids[0], ids[1]);
            let unit = beside_city(&mut w, a, b, 104 /* high_men_spearmen */)?;
            w.start_treaty(a, b, Treaty::Pact);
            w.drain_diplomacy_events();
            Some((name, w, a, b, unit))
        })
        .collect();
    assert!(!found.is_empty(), "no world had room next to a city");
    found
}

/// A new unit of `owner` on a square next to a city of `host`.
fn beside_city(w: &mut Table, host: PlayerId, owner: PlayerId, unit: u16) -> Option<hecs::Entity> {
    let (_, city) = w.economy().cities_of(host).first().copied()?;
    let unit = w.content().rules.units.id_numbered(unit)?;
    around(w, city).into_iter().find_map(|pos| {
        w.apply(Command::CreateUnit { owner, pos, unit })
            .iter()
            .all(|e| !matches!(e, Event::Rejected(_)))
            .then(|| w.units().units_at(pos))?
            .into_iter()
            .find(|&u| w.units().unit_owner(u) == Some(owner))
    })
}

/// The warnings the start of `owner`'s turn raises.
fn warnings(w: &mut Table, owner: PlayerId) -> Vec<(PlayerId, u8)> {
    w.start_player_turn(owner, 2);
    w.drain_diplomacy_events()
        .into_iter()
        .filter_map(|e| match e {
            Event::Diplomacy(DiplomacyEvent::Trespassing {
                owner: by,
                intruder,
                warning,
            }) if by == owner => Some((intruder, warning)),
            _ => None,
        })
        .collect()
}

/// Warn `b` until `a` has to choose.
fn warn_to_the_last(w: &mut Table, a: PlayerId, b: PlayerId) {
    let last = w.content().rules.config.diplomacy.trespass_warnings;
    for n in 1..=last {
        assert_eq!(
            w.trespass_choices(a),
            [],
            "no choice before the last warning"
        );
        assert_eq!(warnings(w, a), [(b, n)]);
    }
    assert_eq!(w.trespass_choices(a), [b]);
}

#[test]
fn a_partner_next_to_a_city_is_warned_each_turn_and_the_last_warning_asks_the_owner() {
    for (name, mut w, a, b, _) in neighbours() {
        warn_to_the_last(&mut w, a, b);
        assert_eq!(
            w.treaty(a, b),
            Treaty::Pact,
            "{name}: the game decides nothing"
        );
        assert_eq!(
            w.trespass_choices(b),
            [],
            "{name}: the intruder has no choice"
        );
    }
}

#[test]
fn a_warned_partner_that_leaves_starts_again_at_the_first_warning() {
    for (name, mut w, a, b, unit) in neighbours() {
        assert_eq!(warnings(&mut w, a), [(b, 1)], "{name}");
        w.scenario_remove_unit(unit);
        assert_eq!(warnings(&mut w, a), [], "{name}");
        beside_city(&mut w, a, b, 104 /* high_men_spearmen */).expect("room as before");
        assert_eq!(warnings(&mut w, a), [(b, 1)], "{name}");
    }
}

#[test]
fn settlers_and_engineers_do_not_trespass_and_without_a_treaty_nobody_is_warned() {
    for (name, mut w, a, b, unit) in neighbours() {
        assert!(w.trespassers(a).contains(&b), "{name}");
        w.scenario_remove_unit(unit);
        for harmless in [
            110, /* high_men_engineers */
            111, /* high_men_settlers */
        ] {
            let made = beside_city(&mut w, a, b, harmless).expect("room as before");
            assert!(!w.trespassers(a).contains(&b), "{name}: unit {harmless}");
            assert_eq!(warnings(&mut w, a), [], "{name}: unit {harmless}");
            w.scenario_remove_unit(made);
        }
        beside_city(&mut w, a, b, 104 /* high_men_spearmen */).expect("room as before");
        assert_eq!(run(&mut w, D::BreakTreaty { player: a, with: b }), None);
        assert!(w.trespassers(a).contains(&b), "{name}: still seen there");
        assert_eq!(
            warnings(&mut w, a),
            [],
            "{name}: no warnings without a treaty"
        );
    }
}

#[test]
fn the_owner_breaks_the_treaty_declares_war_overlooks_it_or_lets_the_choice_lapse() {
    for (name, mut w, a, b, _) in neighbours() {
        let overlook = D::Overlook { player: a, with: b };
        assert_eq!(
            run(&mut w, overlook.clone()),
            Some(DiplomacyError::NothingPending),
            "{name}: before the last warning"
        );
        warn_to_the_last(&mut w, a, b);
        assert_eq!(run(&mut w, overlook), None, "{name}");
        assert_eq!(w.trespass_choices(a), [], "{name}: overlooked");
        assert_eq!(w.treaty(a, b), Treaty::Pact, "{name}");

        warn_to_the_last(&mut w, a, b);
        w.end_player_turn(a);
        assert_eq!(w.trespass_choices(a), [], "{name}: lapsed with the turn");

        warn_to_the_last(&mut w, a, b);
        assert_eq!(run(&mut w, D::BreakTreaty { player: a, with: b }), None);
        assert_eq!(w.treaty(a, b), Treaty::None, "{name}");
        assert_eq!(w.trespass_choices(a), [], "{name}: broken");

        w.start_treaty(a, b, Treaty::Pact);
        warn_to_the_last(&mut w, a, b);
        assert_eq!(run(&mut w, D::DeclareWar { player: a, on: b }), None);
        assert_eq!(w.treaty(a, b), Treaty::War, "{name}");
        assert_eq!(w.trespass_choices(a), [], "{name}: at war");
    }
}

#[test]
fn an_ally_next_to_a_city_is_warned_only_under_the_setting_for_it() {
    for (name, mut w, a, b, _) in neighbours() {
        w.start_treaty(a, b, Treaty::Alliance);
        for penalty in [false, true] {
            w.scenario_change_settings(|s| s.rules.allied_trespass_penalty = penalty);
            assert!(w.trespassers(a).contains(&b), "{name}: still seen there");
            let told: &[(PlayerId, u8)] = if penalty { &[(b, 1)] } else { &[] };
            assert_eq!(warnings(&mut w, a), told, "{name}: penalty {penalty}");
        }
    }
}

#[test]
fn the_warnings_are_kept_in_a_save() {
    for (name, mut w, a, b, _) in neighbours() {
        warn_to_the_last(&mut w, a, b);
        let copy = Snapshot::from_text(&text(&w))
            .and_then(|s| s.restore(content()))
            .expect("the game reads back");
        assert_eq!(copy.trespass_choices(a), [b], "{name}");
    }
}

/// A price of `gold` the proposer of the counter asks for the treaty.
fn asking(deal: Deal, gold: i32) -> ProposalKind {
    ProposalKind::Priced {
        deal,
        give: Goods::NONE,
        get: Goods::gold(gold),
    }
}

#[test]
fn a_treaty_is_countered_with_a_price_that_is_paid_when_it_is_agreed() {
    for (name, mut w, ids) in met() {
        let (a, b) = (ids[0], ids[1]);
        for (deal, before, after) in [
            (Deal::Pact, Treaty::None, Treaty::Pact),
            (Deal::Alliance, Treaty::Pact, Treaty::Alliance),
            (Deal::Peace, Treaty::War, Treaty::None),
        ] {
            if before == Treaty::War {
                w.declare_war(a, b);
            } else {
                w.start_treaty(a, b, before);
            }
            w.treasury_mut(a).expect("a treasury").gold = HalfInt::from_int(100);
            let held = (gold(&w, a), gold(&w, b));
            let propose = D::Propose {
                from: a,
                to: b,
                kind: deal.kind(),
            };
            assert_eq!(run(&mut w, propose), None, "{name}: {deal:?}");
            let counter = |kind| D::Counter {
                player: b,
                from: a,
                kind,
            };
            let other = if deal == Deal::Peace {
                Deal::Pact
            } else {
                Deal::Peace
            };
            for wrong in [
                deal.kind(),
                asking(other, 10),
                ProposalKind::Trade {
                    give: Goods::NONE,
                    get: Goods::gold(10),
                },
                asking(deal, 0),
                asking(deal, 1_000_000),
            ] {
                assert_eq!(
                    run(&mut w, counter(wrong)),
                    Some(DiplomacyError::NotAllowed),
                    "{name}: {deal:?} answered with {wrong:?}"
                );
            }
            assert_eq!(run(&mut w, counter(asking(deal, 10))), None, "{name}");
            assert_eq!(w.treaty(a, b), before, "{name}: nothing yet");
            let accept = D::Answer {
                player: a,
                from: b,
                accept: true,
            };
            assert_eq!(run(&mut w, accept), None, "{name}: {deal:?}");
            assert_eq!(w.treaty(a, b), after, "{name}: {deal:?}");
            let ten = HalfInt::from_int(10);
            assert_eq!(
                (gold(&w, a), gold(&w, b)),
                (held.0 - ten, held.1 + ten),
                "{name}: {deal:?}"
            );
        }
    }
}

#[test]
fn a_treaty_is_haggled_over_for_the_rounds_of_the_rules_and_no_more() {
    for (name, mut w, ids) in met() {
        let (a, b) = (ids[0], ids[1]);
        for p in [a, b] {
            w.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(100);
        }
        let rounds = w.content().rules.config.diplomacy.max_rounds;
        let propose = D::Propose {
            from: a,
            to: b,
            kind: ProposalKind::Pact,
        };
        assert_eq!(run(&mut w, propose), None, "{name}");
        let (mut from, mut to) = (a, b);
        for round in 1..=rounds {
            let counter = D::Counter {
                player: to,
                from,
                kind: asking(Deal::Pact, i32::from(round)),
            };
            let expected = (round == rounds).then_some(DiplomacyError::NotAllowed);
            assert_eq!(run(&mut w, counter), expected, "{name}: counter {round}");
            if expected.is_none() {
                (from, to) = (to, from);
            }
        }
        // The last price waits, saved with the game.
        let copy = Snapshot::from_text(&text(&w))
            .and_then(|s| s.restore(content()))
            .expect("the game reads back");
        assert_eq!(
            copy.diplomacy.proposals(),
            w.diplomacy.proposals(),
            "{name}"
        );
    }
}

/// Every world with a third wizard in which the first, at war with the third, is allied with the
/// second and has called it to arms.
fn called() -> Vec<(String, Table, [PlayerId; 3])> {
    let found: Vec<_> = met()
        .into_iter()
        .filter(|(_, _, ids)| ids.len() > 2)
        .map(|(name, mut w, ids)| {
            let (a, b, c) = (ids[0], ids[1], ids[2]);
            w.start_treaty(a, b, Treaty::Alliance);
            w.declare_war(a, c);
            let call = D::Propose {
                from: a,
                to: b,
                kind: ProposalKind::WarOn(c),
            };
            assert_eq!(run(&mut w, call), None, "{name}");
            w.drain_diplomacy_events();
            (name, w, [a, b, c])
        })
        .collect();
    assert!(!found.is_empty(), "no world has three wizards");
    found
}

fn answer(player: PlayerId, from: PlayerId, accept: bool) -> D {
    D::Answer {
        player,
        from,
        accept,
    }
}

/// The price `ally` asks of `caller` for joining the war on `third`.
fn name_price(ally: PlayerId, caller: PlayerId, third: PlayerId, gold: i32) -> D {
    D::Counter {
        player: ally,
        from: caller,
        kind: ProposalKind::WarPrice {
            third,
            price: Goods::gold(gold),
        },
    }
}

#[test]
fn an_ally_called_to_arms_joins_the_war_or_ends_the_alliance_by_refusing() {
    for (name, mut w, [a, b, c]) in called() {
        let call = w.diplomacy.proposals()[0];
        assert!(w.is_call_to_arms(call), "{name}");
        assert_eq!(run(&mut w, answer(b, a, true)), None, "{name}");
        assert_eq!(w.treaty(b, c), Treaty::War, "{name}");
        assert_eq!(w.treaty(a, b), Treaty::Alliance, "{name}");
    }
    for (name, mut w, [a, b, c]) in called() {
        let told = w.apply(Command::Diplomacy(answer(b, a, false)));
        assert_eq!(w.treaty(a, b), Treaty::None, "{name}: ended");
        assert_ne!(w.treaty(b, c), Treaty::War, "{name}");
        let broke = told.iter().any(|e| {
            matches!(e, Event::Diplomacy(DiplomacyEvent::TreatyBroken { breaker, victim, .. })
                if *breaker == b && *victim == a)
        });
        assert!(broke, "{name}: broken by the one that refuses");
    }
}

#[test]
fn a_request_for_a_war_that_is_no_call_to_arms_is_refused_without_harm() {
    // Allies with no war on the third, and a pact partner of one that is at war.
    for (treaty, at_war) in [(Treaty::Alliance, false), (Treaty::Pact, true)] {
        for (name, mut w, ids) in met() {
            if ids.len() < 3 {
                continue;
            }
            let (a, b, c) = (ids[0], ids[1], ids[2]);
            w.start_treaty(a, b, treaty);
            if at_war {
                w.declare_war(a, c);
            }
            let kind = ProposalKind::WarOn(c);
            assert_eq!(
                run(
                    &mut w,
                    D::Propose {
                        from: a,
                        to: b,
                        kind
                    }
                ),
                None,
                "{name}"
            );
            assert!(!w.is_call_to_arms(w.diplomacy.proposals()[0]), "{name}");
            assert_eq!(
                run(&mut w, name_price(b, a, c, 1)),
                Some(DiplomacyError::NotAllowed),
                "{name}: no price for it"
            );
            assert_eq!(run(&mut w, answer(b, a, false)), None, "{name}");
            assert_eq!(w.treaty(a, b), treaty, "{name}");
        }
    }
}

#[test]
fn a_call_to_arms_that_lapses_or_is_overtaken_ends_no_alliance() {
    for (name, mut w, [a, b, _]) in called() {
        w.end_player_turn(a);
        assert_eq!(w.diplomacy.proposals(), [], "{name}");
        assert_eq!(w.treaty(a, b), Treaty::Alliance, "{name}");
    }
    for (name, mut w, [a, b, c]) in called() {
        // The caller has made peace in the meantime.
        w.start_treaty(a, c, Treaty::None);
        assert_eq!(run(&mut w, answer(b, a, false)), None, "{name}");
        assert_eq!(w.treaty(a, b), Treaty::Alliance, "{name}");
    }
}

#[test]
fn a_price_for_joining_a_war_is_paid_when_agreed_and_the_ally_joins() {
    for (name, mut w, [a, b, c]) in called() {
        let price = gold(&w, a).floor() / 2;
        assert!(price > 0, "{name}: the caller can pay something");
        assert_eq!(run(&mut w, name_price(b, a, c, price)), None, "{name}");
        assert_eq!(
            w.treaty(b, c),
            Treaty::None,
            "{name}: not before it is paid"
        );
        let (had, got) = (gold(&w, a), gold(&w, b));
        assert_eq!(run(&mut w, answer(a, b, true)), None, "{name}");
        assert_eq!(gold(&w, a), had - HalfInt::from_int(price), "{name}");
        assert_eq!(gold(&w, b), got + HalfInt::from_int(price), "{name}");
        assert_eq!(w.treaty(b, c), Treaty::War, "{name}");
        assert_eq!(w.treaty(a, b), Treaty::Alliance, "{name}");
        assert_eq!(w.diplomacy.proposals(), [], "{name}");
    }
}

#[test]
fn a_price_not_paid_puts_the_call_again_to_be_joined_or_refused() {
    for (name, mut w, [a, b, c]) in called() {
        assert_eq!(run(&mut w, name_price(b, a, c, 1)), None, "{name}");
        assert_eq!(run(&mut w, answer(a, b, false)), None, "{name}");
        assert_eq!(w.treaty(a, b), Treaty::Alliance, "{name}: not yet");
        let again = w.diplomacy.proposals().to_vec();
        assert_eq!(again.len(), 1, "{name}");
        assert_eq!((again[0].from, again[0].to), (a, b), "{name}");
        assert!(w.is_call_to_arms(again[0]), "{name}");
        // One price to a call.
        assert!(run(&mut w, name_price(b, a, c, 1)).is_some(), "{name}");
        assert_eq!(run(&mut w, answer(b, a, false)), None, "{name}");
        assert_eq!(w.treaty(a, b), Treaty::None, "{name}");
    }
}

#[test]
fn a_price_for_a_war_is_only_ever_an_answer_to_a_call_and_is_kept_in_a_save() {
    for (name, mut w, [a, b, c]) in called() {
        let kind = ProposalKind::WarPrice {
            third: c,
            price: Goods::gold(1),
        };
        assert_eq!(
            run(
                &mut w,
                D::Propose {
                    from: b,
                    to: a,
                    kind
                }
            ),
            Some(DiplomacyError::NotAllowed),
            "{name}"
        );
        assert_eq!(run(&mut w, name_price(b, a, c, 1)), None, "{name}");
        let copy = Snapshot::from_text(&text(&w))
            .and_then(|s| s.restore(content()))
            .expect("the game reads back");
        assert_eq!(
            copy.diplomacy.proposals(),
            w.diplomacy.proposals(),
            "{name}"
        );
    }
}

/// Every world with the expanded diplomacy options on, in which the first two wizards have a
/// pact and each has explored squares the other has not, on both planes.
fn map_partners() -> Vec<(String, Table, PlayerId, PlayerId)> {
    met()
        .into_iter()
        .map(|(name, mut w, ids)| {
            let (a, b) = (ids[0], ids[1]);
            w.scenario_change_settings(|s| s.rules.expanded_diplomacy = true);
            w.start_treaty(a, b, Treaty::Pact);
            let map = w.map().expect("a map");
            let (width, height) = (map.width(), map.height());
            for (who, x) in [(a, width / 4), (b, 3 * width / 4)] {
                for plane in Plane::all() {
                    let at = Position {
                        plane,
                        x,
                        y: height / 2,
                    };
                    w.explore(who, at, HalfInt::from_raw(9));
                }
            }
            assert!(w.map_news(a, b) > 0 && w.map_news(b, a) > 0, "{name}");
            (name, w, a, b)
        })
        .collect()
}

/// Every square of both planes.
fn squares(w: &World) -> Vec<Position> {
    let map = w.map().expect("a map");
    let (width, height) = (map.width(), map.height());
    Plane::all()
        .flat_map(|plane| {
            (0..height).flat_map(move |y| (0..width).map(move |x| Position { plane, x, y }))
        })
        .collect()
}

fn swap(w: &mut Table, a: PlayerId, b: PlayerId, accept: bool) {
    let offer = D::Propose {
        from: a,
        to: b,
        kind: ProposalKind::MapExchange,
    };
    assert_eq!(run(w, offer), None);
    assert_eq!(run(w, answer(b, a, accept)), None);
}

#[test]
fn maps_are_swapped_only_by_partners_under_the_expanded_options() {
    for (name, mut w, a, b) in map_partners() {
        let kind = ProposalKind::MapExchange;
        let refused = Err(DiplomacyError::NotAllowed);
        for treaty in [Treaty::Pact, Treaty::Alliance] {
            w.start_treaty(a, b, treaty);
            assert_eq!(w.check_proposal(a, b, kind), Ok(()), "{name}: {treaty:?}");
        }
        for treaty in [Treaty::None, Treaty::War] {
            w.start_treaty(a, b, treaty);
            assert_eq!(w.check_proposal(a, b, kind), refused, "{name}: {treaty:?}");
        }
        w.start_treaty(a, b, Treaty::Pact);
        w.scenario_change_settings(|s| s.rules.expanded_diplomacy = false);
        assert_eq!(w.check_proposal(a, b, kind), refused, "{name}: setting off");
    }
}

#[test]
fn an_agreed_swap_shows_each_the_squares_the_other_has_explored() {
    for (name, mut w, a, b) in map_partners() {
        let all = squares(&w);
        let known = |w: &World, p| {
            all.iter()
                .map(|&at| w.is_explored(p, at))
                .collect::<Vec<_>>()
        };
        let (had_a, had_b) = (known(&w, a), known(&w, b));
        let both: Vec<bool> = had_a.iter().zip(&had_b).map(|(&x, &y)| x || y).collect();
        let seen: Vec<bool> = {
            let view = PlayerView::new(&w, a);
            all.iter().map(|&at| view.sees(at)).collect()
        };

        swap(&mut w, a, b, true);
        assert_eq!(known(&w, a), both, "{name}");
        assert_eq!(known(&w, b), both, "{name}");
        assert_eq!((w.map_news(a, b), w.map_news(b, a)), (0, 0), "{name}");
        // A map shows the land, not what moves on it.
        let view = PlayerView::new(&w, a);
        let now: Vec<bool> = all.iter().map(|&at| view.sees(at)).collect();
        assert_eq!(now, seen, "{name}: sight is unchanged");
        let b_cities = w.economy().cities_of(b);
        for (city, _) in b_cities {
            assert!(
                view.known_cities().iter().any(|&(c, _, _)| c == city),
                "{name}: the partner's cities are on the map"
            );
        }
    }
}

#[test]
fn a_swap_turned_down_shows_nothing_and_one_that_waits_is_kept_in_a_save() {
    for (name, mut w, a, b) in map_partners() {
        let before = (w.explored_squares(a), w.explored_squares(b));
        swap(&mut w, a, b, false);
        assert_eq!(
            (w.explored_squares(a), w.explored_squares(b)),
            before,
            "{name}"
        );
        let offer = D::Propose {
            from: a,
            to: b,
            kind: ProposalKind::MapExchange,
        };
        assert_eq!(run(&mut w, offer), None, "{name}");
        let copy = Snapshot::from_text(&text(&w))
            .and_then(|s| s.restore(content()))
            .expect("the game reads back");
        assert_eq!(
            copy.diplomacy.proposals(),
            w.diplomacy.proposals(),
            "{name}"
        );
    }
}

#[test]
fn agreed_and_waiting_trade_and_research_pacts_are_kept_in_a_save() {
    for (name, mut w, a, b) in map_partners() {
        let propose = |from, to, kind| D::Propose { from, to, kind };
        assert_eq!(run(&mut w, propose(a, b, ProposalKind::TradePact)), None);
        assert_eq!(run(&mut w, answer(b, a, true)), None);
        assert_eq!(run(&mut w, propose(a, b, ProposalKind::ResearchPact)), None);
        let copy = Snapshot::from_text(&text(&w))
            .and_then(|s| s.restore(content()))
            .expect("the game reads back");
        assert!(copy.diplomacy.has_trade_pact(b, a), "{name}");
        assert!(!copy.diplomacy.has_research_pact(a, b), "{name}");
        assert_eq!(
            copy.diplomacy.proposals(),
            w.diplomacy.proposals(),
            "{name}"
        );
    }
}
