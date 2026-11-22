//! Class 9, diplomacy: meeting, proposals and their answers, treaties, war and peace, gifts.

use crate::support::worlds;
use f4x_core::diplomacy::{DiplomacyCommand as D, DiplomacyError, ProposalKind, Treaty};
use f4x_core::game::{Command, Event, Game, Rejection};
use f4x_core::player::PlayerId;
use f4x_def::half_int::HalfInt;

/// The result of a diplomacy command: the reason it was refused, if it was.
fn run(g: &mut Game, command: D) -> Option<DiplomacyError> {
    g.apply(Command::Diplomacy(command))
        .iter()
        .find_map(|e| match e {
            Event::Rejected(Rejection::Diplomacy(why)) => Some(*why),
            _ => None,
        })
}

/// Every world with its wizards, that have all met.
fn met() -> Vec<(String, Game, Vec<PlayerId>)> {
    worlds()
        .into_iter()
        .map(|(name, mut g)| {
            let w = g.wizards();
            for &a in &w {
                for &b in &w {
                    g.make_contact(a, b);
                }
            }
            (name, g, w)
        })
        .collect()
}

fn gold(g: &Game, p: PlayerId) -> HalfInt {
    g.player(p).expect("a player").treasury().gold
}

#[test]
fn wizards_meet_once_and_nothing_is_done_before() {
    for (name, mut g) in worlds() {
        let w = g.wizards();
        let (a, b) = (w[0], w[1]);
        assert!(!g.have_met(a, b), "{name}: they have met at the start");
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
                run(&mut g, command.clone()),
                Some(DiplomacyError::NotContacted),
                "{name}: {command:?}"
            );
        }
        assert!(g.make_contact(a, b), "{name}: first time");
        assert!(!g.make_contact(b, a), "{name}: second time, either way");
        assert!(g.have_met(a, b) && g.have_met(b, a));
        assert!(!g.make_contact(a, a), "{name}: nobody meets themselves");
        assert_eq!(g.treaty(a, b), Treaty::None);
    }
}

#[test]
fn a_pact_comes_when_it_is_answered_yes_and_not_when_it_is_answered_no() {
    for (name, mut g, w) in met() {
        let (a, b) = (w[0], w[1]);
        let pact = ProposalKind::Pact;
        assert_eq!(
            run(
                &mut g,
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
                &mut g,
                D::Propose {
                    from: a,
                    to: b,
                    kind: pact
                }
            ),
            Some(DiplomacyError::AlreadyPending),
            "{name}"
        );
        assert_eq!(g.treaty(a, b), Treaty::None);
        // It is the receiver who answers.
        assert_eq!(
            run(
                &mut g,
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
                &mut g,
                D::Answer {
                    player: b,
                    from: a,
                    accept: false
                }
            ),
            None,
            "{name}"
        );
        assert_eq!(g.treaty(a, b), Treaty::None, "{name}: turned down");
        // The answer used the offer up.
        assert_eq!(
            run(
                &mut g,
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
            &mut g,
            D::Propose {
                from: a,
                to: b,
                kind: pact,
            },
        );
        assert_eq!(
            run(
                &mut g,
                D::Answer {
                    player: b,
                    from: a,
                    accept: true
                }
            ),
            None,
            "{name}"
        );
        assert_eq!(g.treaty(a, b), Treaty::Pact, "{name}");
        assert_eq!(g.treaty(b, a), Treaty::Pact, "{name}: both ways");
        // What the treaty allows is asked of the game: a pact once more is not.
        assert_eq!(
            g.check_proposal(a, b, ProposalKind::Pact),
            Err(DiplomacyError::NotAllowed),
            "{name}"
        );
        assert_eq!(
            g.check_proposal(a, b, ProposalKind::Alliance),
            Ok(()),
            "{name}"
        );
        assert_eq!(
            g.check_proposal(a, b, ProposalKind::Peace),
            Err(DiplomacyError::NotAllowed),
            "{name}: peace is for war"
        );
    }
}

#[test]
fn breaking_a_treaty_is_told_and_only_a_treaty_can_be_broken() {
    for (name, mut g, w) in met() {
        let (a, b) = (w[0], w[1]);
        assert_eq!(
            run(&mut g, D::BreakTreaty { player: a, with: b }),
            Some(DiplomacyError::NotAllowed),
            "{name}: nothing to break"
        );
        g.start_treaty(a, b, Treaty::Alliance);
        assert_eq!(g.treaty(b, a), Treaty::Alliance);
        let events = g.apply(Command::Diplomacy(D::BreakTreaty { player: a, with: b }));
        assert!(!events.iter().any(|e| matches!(e, Event::Rejected(_))));
        assert_eq!(g.treaty(a, b), Treaty::None, "{name}");
        assert_eq!(g.treaty(b, a), Treaty::None, "{name}");
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
    for (name, mut g, w) in met() {
        let (a, b) = (w[0], w[1]);
        g.start_treaty(a, b, Treaty::Pact);
        assert_eq!(
            run(&mut g, D::DeclareWar { player: a, on: b }),
            None,
            "{name}"
        );
        assert_eq!(g.treaty(a, b), Treaty::War, "{name}");
        assert_eq!(g.treaty(b, a), Treaty::War, "{name}");
        assert_eq!(g.war_age(a, b), Some(0), "{name}");
        assert_eq!(
            run(&mut g, D::DeclareWar { player: b, on: a }),
            Some(DiplomacyError::NotAllowed),
            "{name}: already at war"
        );
        // At war only peace may be asked, and no treaty can be broken.
        assert_eq!(
            g.check_proposal(a, b, ProposalKind::Pact),
            Err(DiplomacyError::NotAllowed),
            "{name}"
        );
        assert_eq!(
            g.check_proposal(a, b, ProposalKind::Peace),
            Ok(()),
            "{name}"
        );
        assert_eq!(
            run(&mut g, D::BreakTreaty { player: a, with: b }),
            Some(DiplomacyError::NotAllowed),
            "{name}"
        );
        assert_eq!(
            run(
                &mut g,
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
            &mut g,
            D::Answer {
                player: b,
                from: a,
                accept: true,
            },
        );
        assert_eq!(g.treaty(a, b), Treaty::None, "{name}: peace");
        assert_eq!(g.war_age(a, b), None, "{name}");
    }
}

#[test]
fn a_war_on_a_third_wizard_is_asked_only_of_one_who_knows_them_both() {
    for (name, mut g, w) in met() {
        let (a, b) = (w[0], w[1]);
        let Some(&c) = w.get(2) else {
            // With two wizards there is no third to name.
            assert_eq!(
                g.check_proposal(a, b, ProposalKind::WarOn(a)),
                Err(DiplomacyError::NotAllowed),
                "{name}"
            );
            continue;
        };
        assert_eq!(
            g.check_proposal(a, b, ProposalKind::WarOn(c)),
            Ok(()),
            "{name}"
        );
        assert_eq!(
            g.check_proposal(a, b, ProposalKind::WarOn(b)),
            Err(DiplomacyError::NotAllowed),
            "{name}: not on the one asked"
        );
        // Asked to break an alliance that is not there, or to go to war with an ally.
        assert_eq!(
            g.check_proposal(a, b, ProposalKind::BreakAllianceWith(c)),
            Err(DiplomacyError::NotAllowed),
            "{name}"
        );
        g.start_treaty(b, c, Treaty::Alliance);
        assert_eq!(
            g.check_proposal(a, b, ProposalKind::WarOn(c)),
            Err(DiplomacyError::NotAllowed),
            "{name}: b is allied with c"
        );
        assert_eq!(
            g.check_proposal(a, b, ProposalKind::BreakAllianceWith(c)),
            Ok(()),
            "{name}"
        );
        // Accepted, the one asked breaks the alliance.
        run(
            &mut g,
            D::Propose {
                from: a,
                to: b,
                kind: ProposalKind::BreakAllianceWith(c),
            },
        );
        assert_eq!(
            run(
                &mut g,
                D::Answer {
                    player: b,
                    from: a,
                    accept: true
                }
            ),
            None
        );
        assert_eq!(g.treaty(b, c), Treaty::None, "{name}");
    }
}

#[test]
fn a_gift_of_gold_moves_the_gold_and_is_told() {
    for (name, mut g, w) in met() {
        let (a, b) = (w[0], w[1]);
        g.treasury_mut(a).expect("a treasury").gold = HalfInt::from_int(100);
        let (had_a, had_b) = (gold(&g, a), gold(&g, b));
        assert_eq!(
            run(
                &mut g,
                D::GiveGold {
                    from: a,
                    to: b,
                    amount: 40
                }
            ),
            None,
            "{name}"
        );
        assert_eq!(gold(&g, a), had_a - HalfInt::from_int(40), "{name}");
        assert_eq!(gold(&g, b), had_b + HalfInt::from_int(40), "{name}");
        // What the giver has not got, or nothing, or less than nothing, cannot be given.
        for amount in [101, 0, -5, i32::MAX, i32::MIN] {
            let (ga, gb) = (gold(&g, a), gold(&g, b));
            assert_eq!(
                run(
                    &mut g,
                    D::GiveGold {
                        from: a,
                        to: b,
                        amount
                    }
                ),
                Some(DiplomacyError::NotEnoughGold),
                "{name}: {amount}"
            );
            assert_eq!((gold(&g, a), gold(&g, b)), (ga, gb), "{name}: {amount}");
        }
        assert_eq!(
            run(
                &mut g,
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
    for (name, mut g, w) in met() {
        let (a, b) = (w[0], w[1]);
        g.treasury_mut(b).expect("a treasury").gold = HalfInt::from_int(50);
        let (had_a, had_b) = (gold(&g, a), gold(&g, b));
        run(
            &mut g,
            D::Propose {
                from: a,
                to: b,
                kind: ProposalKind::Demand { gold: 30 },
            },
        );
        assert_eq!(
            run(
                &mut g,
                D::Answer {
                    player: b,
                    from: a,
                    accept: true
                }
            ),
            None
        );
        assert_eq!(gold(&g, b), had_b - HalfInt::from_int(30), "{name}");
        assert_eq!(gold(&g, a), had_a + HalfInt::from_int(30), "{name}");
        // Twenty is left: a demand for more than that is answered yes, and not paid.
        let (ga, gb) = (gold(&g, a), gold(&g, b));
        run(
            &mut g,
            D::Propose {
                from: a,
                to: b,
                kind: ProposalKind::Demand { gold: 500 },
            },
        );
        run(
            &mut g,
            D::Answer {
                player: b,
                from: a,
                accept: true,
            },
        );
        assert_eq!((gold(&g, a), gold(&g, b)), (ga, gb), "{name}");
        // A demand for nothing is no demand.
        assert_eq!(
            run(
                &mut g,
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
    for (name, mut g, w) in met() {
        let (a, b) = (w[0], w[1]);
        run(
            &mut g,
            D::Propose {
                from: b,
                to: a,
                kind: ProposalKind::Pact,
            },
        );
        // The fortress of a falls: a snapshot of the game with a banished is restored.
        let mut snap = g.snapshot();
        let save = snap
            .wizards
            .iter_mut()
            .find(|s| s.player == a.0)
            .expect("a save");
        save.banished = true;
        save.fortress = None;
        save.summon_at = None;
        save.more_circles.clear();
        g = snap.restore(crate::support::content()).expect("restores");
        assert!(g.wizard(a).is_some_and(|w| w.is_banished()), "{name}");
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
                run(&mut g, command.clone()),
                Some(DiplomacyError::Banished),
                "{name}: {command:?}"
            );
        }
        assert_eq!(
            run(
                &mut g,
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
    use f4x_core::game::Command;
    // Taking a city costs the winner standing but starts no war (`docs/diplomacy.md` 9.2).
    for (name, mut g, w) in met() {
        let (a, b) = (w[0], w[1]);
        let city = g
            .economy()
            .cities()
            .into_iter()
            .find(|c| c.1.owner == b && g.economy().citizens(c.0) > 0)
            .expect("a city with people")
            .0;
        g.conquer_city(city, a, 0);
        assert_eq!(
            g.treaty(a, b),
            Treaty::None,
            "{name}: a city taken is no war"
        );
        // Nor do the turns that follow start one: a pact is ended only by a player.
        g.start_treaty(a, b, Treaty::Pact);
        for _ in 0..6 {
            g.apply(Command::EndTurn);
        }
        assert_ne!(g.treaty(a, b), Treaty::War, "{name}: war came by itself");
    }
}

#[test]
fn an_alliance_stands_though_one_ally_is_at_war_with_the_others_ally() {
    use f4x_core::game::Command;
    for (name, mut g, w) in met().into_iter().filter(|m| m.2.len() >= 3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        g.start_treaty(a, b, Treaty::Alliance);
        g.start_treaty(b, c, Treaty::Alliance);
        g.declare_war(a, c);
        for _ in 0..3 {
            g.apply(Command::EndTurn);
        }
        assert_eq!(
            g.treaty(a, b),
            Treaty::Alliance,
            "{name}: the game split it"
        );
        assert_eq!(
            g.treaty(b, c),
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
    for (name, mut g, w) in met() {
        let (a, b) = (w[0], w[1]);
        let Some(mover) = g
            .units()
            .all_units()
            .into_iter()
            .find(|&u| g.units().unit_owner(u) == Some(a))
        else {
            continue;
        };
        let at = g.units().unit_position(mover).expect("a position");
        let kind = g.units().unit_kind_id(mover).expect("a kind");
        // A unit of b on a square next to it that the mover can attack.
        let Some(to) = around(&g, at).into_iter().find(|&to| {
            let made = g.apply(Command::CreateUnit {
                owner: b,
                pos: to,
                unit: kind,
            });
            let asked = made.iter().all(|e| !matches!(e, Event::Rejected(_)))
                && g.apply(Command::MoveStack {
                    units: vec![mover],
                    to,
                })
                .iter()
                .any(|e| matches!(e, Event::AttackRequested { .. }));
            if !asked {
                for u in g.units().units_at(to) {
                    g.scenario_remove_unit(u);
                }
            }
            asked
        }) else {
            continue;
        };
        tried += 1;
        for treaty in [Treaty::Pact, Treaty::Alliance] {
            g.start_treaty(a, b, treaty);
            let before = text(&g);
            let events = g.apply(Command::MoveStack {
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
                text(&g),
                before,
                "{name}: a refused attack changed the game"
            );
            assert_eq!(g.treaty(a, b), treaty, "{name}: the game broke the treaty");
        }
        // The player breaks it, and the attack is asked for.
        assert_eq!(
            run(&mut g, D::BreakTreaty { player: a, with: b }),
            None,
            "{name}"
        );
        let events = g.apply(Command::MoveStack {
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
    for (name, mut g, w) in met() {
        let (a, b) = (w[0], w[1]);
        g.treasury_mut(a).expect("a treasury").gold = HalfInt::from_int(100);
        let gift = g.apply(Command::Diplomacy(D::GiveGold {
            from: a,
            to: b,
            amount: 40,
        }));
        assert!(
            gift.iter()
                .any(|e| matches!(e, Event::Diplomacy(E::GoldGiven { from, to, gold: 40 }) if *from == a && *to == b)),
            "{name}: a gift is told"
        );
        let war = g.apply(Command::Diplomacy(D::DeclareWar { player: a, on: b }));
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
