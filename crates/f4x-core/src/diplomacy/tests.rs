use super::*;
use crate::game::save::Snapshot;
use crate::game::testutil::{num, shipped};
use crate::map::{Grid, Map, Plane, Position};
use crate::testdir::TestResult;

use crate::game::{Command, Rejection};

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
const P2: PlayerId = PlayerId(2);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

/// Three wizards, the first of them the human, nobody met yet.
fn world() -> Game {
    let c = shipped();
    let grass = c
        .terrains
        .id_numbered(num::terrain::GRASSLAND)
        .expect("id_of succeeds");
    let mut g = Game::new(11);
    g.content = c;
    g.map = Some(Map::new(Grid::new(30, 20, grass), Grid::new(30, 20, grass)));
    for id in [P0, P1, P2] {
        g.add_player(id, "W");
    }
    g.set_human(Some(P0));
    for id in [P0, P1, P2] {
        g.init_wizard(id, &[("sorcery", 2)], &[], None)
            .expect("init_wizard succeeds");
    }
    g
}

/// The turn as diplomacy has it (the rest of a game turn needs cities and armies): what
/// diplomacy does at its end, and the events it raised.
fn diplomacy_turn(g: &mut Game) -> Vec<Event> {
    g.process_diplomacy();
    g.turn += 1;
    g.drain_diplomacy_events()
}

fn met(g: &mut Game, a: PlayerId, b: PlayerId) {
    g.make_contact(a, b);
    g.drain_diplomacy_events();
}

fn set_pair(g: &mut Game, a: PlayerId, b: PlayerId, f: impl FnOnce(&mut Pair)) {
    g.with_diplomacy(|d, ctx| f(d.pair_mut(ctx.view, a, b)));
}

fn set_rel(g: &mut Game, a: PlayerId, b: PlayerId, v: i32) {
    set_pair(g, a, b, |p| p.visible = v);
}

fn command(g: &mut Game, c: DiplomacyCommand) -> Vec<Event> {
    g.apply(Command::Diplomacy(c))
}

fn city(g: &mut Game, owner: PlayerId, pos: Position) -> hecs::Entity {
    let race = g.content.rules.races.id_of("high_elf");
    g.apply(Command::FoundCity {
        owner,
        pos,
        name: "Town".into(),
        race,
        population: 3,
    });
    g.economy().city_at(pos).expect("city_at succeeds")
}

fn rejected(events: &[Event]) -> Option<DiplomacyError> {
    events.iter().find_map(|e| match e {
        Event::Rejected(Rejection::Diplomacy(e)) => Some(*e),
        _ => None,
    })
}

#[test]
fn the_scale_runs_from_hate_to_harmony() {
    assert_eq!(relation_key(-100), "relation.hate");
    assert_eq!(relation_key(-81), "relation.hate");
    assert_eq!(relation_key(-80), "relation.troubled");
    assert_eq!(relation_key(-60), "relation.tense");
    assert_eq!(relation_key(0), "relation.neutral");
    assert_eq!(relation_key(99), "relation.friendly");
    assert_eq!(relation_key(100), "relation.harmony");
    assert_eq!(relation_level(0), 5);
}

#[test]
fn start_relations_come_from_the_books() -> TestResult {
    let mut g = world();
    // Life against death: -(life + death) * 5, no shared realm.
    g.init_wizard(P0, &[("life", 3)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    g.init_wizard(P1, &[("death", 2)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.relation(P0, P1), -25);
    // Shared realms add two for each book of the smaller; chaos costs two for the larger.
    g.init_wizard(P0, &[("nature", 3), ("chaos", 1)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    g.init_wizard(P1, &[("nature", 2), ("chaos", 2)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.relation(P0, P1), 2);
    // Never below -90.
    g.init_wizard(P0, &[("life", 5)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    g.init_wizard(P1, &[("death", 5)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.relation(P0, P1), -50);
    g.init_wizard(P0, &[("life", 11)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    g.init_wizard(P1, &[("death", 11)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.relation(P0, P1), -90);
    Ok(())
}

#[test]
fn a_wizard_whose_city_is_seen_is_met() -> TestResult {
    let mut g = world();
    city(&mut g, P1, at(10, 10));
    assert!(!g.have_met(P0, P1));
    g.explore(P0, at(6, 10), 5);
    assert!(!g.have_met(P0, P1), "out of sight");
    g.explore(P0, at(8, 10), 5);
    assert!(g.have_met(P0, P1));
    assert!(g.have_met(P1, P0));
    assert_eq!(
        g.diplomacy.stance(P0, P1).ok_or("stance is missing")?.stage,
        Stage::Greeted
    );
    assert!(!g.have_met(P0, P2));
    Ok(())
}

#[test]
fn the_event_of_a_meeting_comes_back_with_the_command() -> TestResult {
    let mut g = world();
    city(&mut g, P1, at(10, 10));
    let events = g.apply(Command::CreateUnit {
        owner: P0,
        pos: at(9, 10),
        unit: g
            .content
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("high_men_cavalry is not in the content")?,
    });
    // The unit looks round where it appears.
    assert!(
        events.contains(&Event::Diplomacy(DiplomacyEvent::ContactMade {
            a: P0,
            b: P1
        }))
    );
    assert!(
        g.drain_diplomacy_events().is_empty(),
        "returned with the command, not kept"
    );
    Ok(())
}

#[test]
fn the_turn_meets_those_whose_units_one_can_see() -> TestResult {
    let mut g = world();
    city(&mut g, P0, at(10, 10));
    let walker = g
        .content
        .rules
        .units
        .id_of("dwarf")
        .ok_or("dwarf is not in the content")?;
    g.treasury_mut(P1).ok_or("treasury_mut is missing")?.gold = 1000;
    g.spawn_unit(P1, at(12, 10), walker);
    // Forget that they have been shown to each other already.
    set_pair(&mut g, P0, P1, |p| p.contacted = false);
    assert!(!g.have_met(P0, P1));
    let events = g.apply(Command::EndTurn);
    assert!(g.have_met(P0, P1), "{events:?}");
    assert!(
        events.contains(&Event::Diplomacy(DiplomacyEvent::ContactMade {
            a: P0,
            b: P1
        }))
    );
    Ok(())
}

#[test]
fn nature_awareness_meets_everyone() -> TestResult {
    let mut g = world();
    city(&mut g, P0, at(2, 2));
    city(&mut g, P1, at(25, 15));
    city(&mut g, P2, at(15, 8));
    let spell = g
        .content
        .rules
        .spells
        .id_of("nature_awareness")
        .ok_or("nature_awareness is not in the content")?;
    // The spell's upkeep is paid from the reserve.
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.mana = 1000;
    g.add_global(P0, spell);
    g.apply(Command::EndTurn);
    assert!(g.have_met(P0, P1) && g.have_met(P0, P2));
    Ok(())
}

#[test]
fn good_deeds_count_less_the_better_the_relation() {
    let mut g = world();
    met(&mut g, P0, P1);
    met(&mut g, P2, P1);
    g.set_human(Some(P0));
    // P1 is the defender (the human never is): attacker P2.
    set_rel(&mut g, P1, P2, 0);
    g.change_relations(8, P2, P1, Action::Friendship, None);
    assert_eq!(g.relation(P1, P2), 8);
    // At +50 a gain is halved... divided by 50 / 25 + 1 = 3.
    set_rel(&mut g, P1, P2, 50);
    g.change_relations(9, P2, P1, Action::Friendship, None);
    assert_eq!(g.relation(P1, P2), 53);
    // From a negative relation it is doubled, but not past +10.
    set_rel(&mut g, P1, P2, -30);
    g.change_relations(8, P2, P1, Action::Friendship, None);
    assert_eq!(g.relation(P1, P2), -14);
    set_rel(&mut g, P1, P2, -3);
    g.change_relations(20, P2, P1, Action::Friendship, None);
    assert_eq!(g.relation(P1, P2), 10);
}

#[test]
fn bad_deeds_count_double_to_a_friend_and_less_to_an_enemy() {
    let mut g = world();
    met(&mut g, P2, P1);
    set_rel(&mut g, P1, P2, 20);
    g.change_relations(-10, P2, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), 0);
    set_rel(&mut g, P1, P2, -50);
    g.change_relations(-10, P2, P1, Action::AttackedCity, None);
    // Divided by 50 / 25 + 1 = 3.
    assert_eq!(g.relation(P1, P2), -53);
    set_rel(&mut g, P1, P2, -100);
    g.change_relations(-50, P2, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), -100);
}

#[test]
fn temper_and_charm_change_what_a_deed_is_worth() -> TestResult {
    let mut g = world();
    met(&mut g, P2, P1);
    set_rel(&mut g, P1, P2, 0);
    g.wizard_mut(P1)
        .ok_or("wizard_mut is missing")?
        .put(Some(Personality::Lawful));
    g.change_relations(-5, P2, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), -10);
    set_rel(&mut g, P1, P2, 0);
    g.wizard_mut(P1)
        .ok_or("wizard_mut is missing")?
        .put(Some(Personality::Maniacal));
    g.change_relations(-4, P2, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), -6);
    g.change_relations(4, P2, P1, Action::Friendship, None);
    // -6 is negative, so the gain doubles to 8, then Maniacal takes three quarters: 6.
    assert_eq!(g.relation(P1, P2), 0);
    // A charismatic attacker halves its penalties and doubles its gains.
    g.wizard_mut(P1)
        .ok_or("wizard_mut is missing")?
        .put(Some(Personality::Chaotic));
    g.wizard_mut(P2)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Retorts>()
        .0
        .insert(Retort::Charismatic);
    set_rel(&mut g, P1, P2, 0);
    g.change_relations(-10, P2, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), -5);
    set_rel(&mut g, P1, P2, 0);
    g.change_relations(4, P2, P1, Action::Friendship, None);
    assert_eq!(g.relation(P1, P2), 8);
    Ok(())
}

#[test]
fn the_human_is_never_the_one_who_changes() {
    let mut g = world();
    met(&mut g, P0, P1);
    let before = g.relation(P0, P1);
    g.change_relations(-40, P1, P0, Action::AttackedCity, None);
    assert_eq!(g.relation(P0, P1), before);
    g.change_relations(-40, P0, P1, Action::AttackedCity, None);
    assert!(g.relation(P0, P1) < before);
}

#[test]
fn strangers_do_not_react_and_neither_does_a_monster() {
    let mut g = world();
    let before = g.relation(P1, P2);
    g.change_relations(-40, P2, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), before);
    met(&mut g, P2, P1);
    g.change_relations(-40, crate::player::MONSTERS, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), before);
}

#[test]
fn a_relation_stays_below_65_without_an_alliance_and_below_minus_25_at_war() {
    let mut g = world();
    met(&mut g, P2, P1);
    set_rel(&mut g, P1, P2, 60);
    g.change_relations(100, P2, P1, Action::Friendship, None);
    assert_eq!(g.relation(P1, P2), 65);
    g.start_treaty(P1, P2, Treaty::Alliance);
    g.change_relations(100, P2, P1, Action::Friendship, None);
    assert!(g.relation(P1, P2) > 65);
    g.declare_war(P1, P2);
    set_pair(&mut g, P1, P2, |p| p.visible = 40);
    g.change_relations(5, P2, P1, Action::Friendship, None);
    assert!(g.relation(P1, P2) <= -25);
}

#[test]
fn what_a_human_does_in_a_war_does_not_worsen_it_but_wears_down_peace() -> TestResult {
    let mut g = world();
    met(&mut g, P0, P1);
    g.declare_war(P1, P0);
    let (rel, patience) = (
        g.relation(P0, P1),
        g.diplomacy
            .stance(P1, P0)
            .ok_or("stance is missing")?
            .peace_patience,
    );
    g.change_relations(-40, P0, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P0, P1), rel);
    // The war is no worse, but peace is harder: a quarter of the deed (after the
    // diminishing returns of an already bad relation) wears on the patience.
    let wear = -40 / (-rel / 25 + 1) / 4;
    assert!(wear < 0);
    assert_eq!(
        g.diplomacy
            .stance(P1, P0)
            .ok_or("stance is missing")?
            .peace_patience,
        patience + wear
    );
    // Other things (a talk-type action) still count.
    g.change_relations(-10, P0, P1, Action::Trespass, None);
    assert!(g.relation(P0, P1) < rel || rel == -100);
    Ok(())
}

#[test]
fn the_last_strongest_act_of_the_human_is_remembered() -> TestResult {
    let mut g = world();
    met(&mut g, P0, P1);
    set_rel(&mut g, P0, P1, 0);
    g.change_relations(-5, P0, P1, Action::BadCitySpell, Some("famine"));
    g.change_relations(-20, P0, P1, Action::AttackedCity, None);
    g.change_relations(-3, P0, P1, Action::BadCitySpell, Some("famine"));
    let last = g
        .diplomacy
        .stance(P1, P0)
        .ok_or("stance is missing")?
        .last_action
        .clone()
        .ok_or("clone is missing")?;
    assert_eq!(last.action, Action::AttackedCity);
    assert!(last.strength <= -20);
    Ok(())
}

#[test]
fn declaring_war_sets_the_state_the_original_sets() -> TestResult {
    let mut g = world();
    met(&mut g, P1, P2);
    let default = g.diplomacy.pair(P1, P2).ok_or("pair is missing")?.default;
    g.declare_war(P1, P2);
    assert_eq!(g.treaty(P1, P2), Treaty::War);
    assert_eq!(g.treaty(P2, P1), Treaty::War);
    let rel = g.relation(P2, P1);
    assert!((-100..=-76).contains(&rel), "{rel}");
    assert_eq!(
        g.diplomacy.pair(P1, P2).ok_or("pair is missing")?.default,
        default - 5
    );
    for (a, b) in [(P1, P2), (P2, P1)] {
        let s = g.diplomacy.stance(a, b).ok_or("stance is missing")?;
        assert_eq!(
            (s.treaty_patience, s.exchange_patience, s.peace_patience),
            (-200, -200, -130)
        );
        assert_eq!(s.hostility, 3);
    }
    // Once only.
    let events = g.drain_diplomacy_events();
    let wars = events
        .iter()
        .filter(|e| matches!(e, Event::Diplomacy(DiplomacyEvent::WarDeclared { .. })))
        .count();
    assert_eq!(wars, 1);
    g.declare_war(P1, P2);
    assert!(g.drain_diplomacy_events().is_empty());
    Ok(())
}

#[test]
fn breaking_a_treaty_costs_trust_with_everyone() -> TestResult {
    let mut g = world();
    met(&mut g, P1, P2);
    met(&mut g, P0, P1);
    met(&mut g, P0, P2);
    g.start_treaty(P1, P2, Treaty::Alliance);
    let default = g.diplomacy.pair(P1, P2).ok_or("pair is missing")?.default;
    g.break_treaties(P1, P2);
    assert_eq!(g.treaty(P1, P2), Treaty::None);
    // The oath breaker's own core reaction falls (the original raises it by mistake).
    assert_eq!(
        g.diplomacy
            .stance(P1, P2)
            .ok_or("stance is missing")?
            .hidden,
        -20
    );
    // Everyone else thinks less of the victim.
    assert_eq!(
        g.diplomacy
            .stance(P0, P2)
            .ok_or("stance is missing")?
            .hidden,
        -5
    );
    assert_eq!(
        g.diplomacy.pair(P1, P2).ok_or("pair is missing")?.default,
        default - 20
    );
    assert_eq!(
        g.diplomacy
            .stance(P2, P1)
            .ok_or("stance is missing")?
            .broken,
        Some(Treaty::Alliance)
    );
    assert_eq!(
        g.diplomacy
            .stance(P1, P2)
            .ok_or("stance is missing")?
            .treaty_patience,
        -200
    );
    assert_eq!(
        g.diplomacy
            .stance(P1, P2)
            .ok_or("stance is missing")?
            .peace_patience,
        -200
    );
    // A pact costs half as much, a Lawful breaker twice.
    g.wizard_mut(P1)
        .ok_or("wizard_mut is missing")?
        .put(Some(Personality::Lawful));
    g.start_treaty(P1, P2, Treaty::Pact);
    g.break_treaties(P1, P2);
    assert_eq!(
        g.diplomacy
            .stance(P1, P2)
            .ok_or("stance is missing")?
            .hidden,
        -40
    );
    Ok(())
}

#[test]
fn ending_no_treaty_costs_the_breaker_nothing_of_its_own() {
    let mut g = world();
    met(&mut g, P1, P2);
    g.break_treaties(P1, P2);
    assert_eq!(g.diplomacy.stance(P1, P2).map_or(0, |s| s.hidden), 0);
    assert_eq!(g.diplomacy.stance(P2, P1).and_then(|s| s.broken), None);
    // The others' record of the victim still falls, as in the original.
    assert_eq!(g.diplomacy.stance(P0, P2).map_or(0, |s| s.hidden), -5);
}

#[test]
fn a_pact_is_proposed_and_accepted() {
    let mut g = world();
    met(&mut g, P0, P1);
    let events = command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Pact,
        },
    );
    assert!(rejected(&events).is_none(), "{events:?}");
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Diplomacy(DiplomacyEvent::ProposalMade(p)) if p.kind == ProposalKind::Pact
    )));
    assert_eq!(g.diplomacy.proposals().len(), 1);
    // One at a time.
    let again = command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Alliance,
        },
    );
    assert_eq!(rejected(&again), Some(DiplomacyError::AlreadyPending));
    let events = command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert!(rejected(&events).is_none(), "{events:?}");
    assert_eq!(g.treaty(P0, P1), Treaty::Pact);
    assert!(g.diplomacy.proposals().is_empty());
}

#[test]
fn a_refused_proposal_changes_nothing() {
    let mut g = world();
    met(&mut g, P0, P1);
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Alliance,
        },
    );
    let events = command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: false,
        },
    );
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Diplomacy(DiplomacyEvent::ProposalAnswered {
            accepted: false,
            lapsed: false,
            ..
        })
    )));
    assert_eq!(g.treaty(P0, P1), Treaty::None);
    let none = command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert_eq!(rejected(&none), Some(DiplomacyError::NothingPending));
}

#[test]
fn what_the_treaty_allows_decides_what_can_be_proposed() {
    let mut g = world();
    met(&mut g, P0, P1);
    met(&mut g, P0, P2);
    met(&mut g, P1, P2);
    let ok = |g: &Game, from, to, kind| g.check_proposal(from, to, kind).is_ok();
    assert!(ok(&g, P0, P1, ProposalKind::Pact));
    assert!(ok(&g, P0, P1, ProposalKind::Alliance));
    assert!(!ok(&g, P0, P1, ProposalKind::Peace));
    assert!(ok(&g, P0, P1, ProposalKind::WarOn(P2)));
    assert!(!ok(&g, P0, P1, ProposalKind::WarOn(P0)));
    assert!(
        !ok(&g, P0, P1, ProposalKind::BreakAllianceWith(P2)),
        "no alliance to break"
    );
    g.start_treaty(P1, P2, Treaty::Alliance);
    assert!(ok(&g, P0, P1, ProposalKind::BreakAllianceWith(P2)));
    assert!(
        ok(&g, P0, P1, ProposalKind::WarOn(P2)),
        "allies may still be sent to war"
    );
    g.declare_war(P1, P2);
    assert!(!ok(&g, P0, P1, ProposalKind::WarOn(P2)), "already at war");
}

#[test]
fn a_pact_cannot_be_offered_twice_nor_in_a_war() {
    let mut g = world();
    met(&mut g, P0, P1);
    g.start_treaty(P0, P1, Treaty::Pact);
    assert!(g.check_proposal(P0, P1, ProposalKind::Pact).is_err());
    assert!(g.check_proposal(P0, P1, ProposalKind::Alliance).is_ok());
    g.declare_war(P1, P0);
    assert!(g.check_proposal(P0, P1, ProposalKind::Pact).is_err());
    assert!(g.check_proposal(P0, P1, ProposalKind::Alliance).is_err());
    assert!(g.check_proposal(P0, P1, ProposalKind::Peace).is_ok());
    assert_eq!(
        g.check_proposal(P0, P2, ProposalKind::Pact),
        Err(DiplomacyError::NotContacted)
    );
    assert_eq!(
        g.check_proposal(P0, P0, ProposalKind::Pact),
        Err(DiplomacyError::SameWizard)
    );
}

#[test]
fn peace_ends_a_war_and_each_side_is_left_alone_a_while() -> TestResult {
    let mut g = world();
    met(&mut g, P0, P1);
    g.declare_war(P1, P0);
    g.drain_diplomacy_events();
    let before = g.relation(P0, P1);
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Peace,
        },
    );
    command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert_eq!(g.treaty(P0, P1), Treaty::None);
    assert_eq!(g.relation(P0, P1), (before + 40).min(100));
    let peace = g
        .diplomacy
        .stance(P1, P0)
        .ok_or("stance is missing")?
        .peace_duration;
    assert!((8..=13).contains(&peace), "{peace}");
    let back = g
        .diplomacy
        .stance(P0, P1)
        .ok_or("stance is missing")?
        .peace_duration;
    assert!((8..=13).contains(&back), "{back}");
    Ok(())
}

#[test]
fn peace_takes_off_the_spells_each_cast_on_the_other_s_cities_but_not_the_owners_own() -> TestResult
{
    let mut g = world();
    met(&mut g, P0, P1);
    let mine = g.spawn_city(P0, at(5, 5), "A".into(), None, 3);
    let theirs = g.spawn_city(P1, at(20, 5), "B".into(), None, 3);
    let curse = g
        .content
        .rules
        .spells
        .id_of("famine")
        .ok_or("famine is not in the content")?;
    let ward = g
        .content
        .rules
        .spells
        .id_of("wall_of_fire")
        .ok_or("wall_of_fire is not in the content")?;
    g.add_city_enchantment(mine, curse, P1);
    g.add_city_enchantment(mine, ward, P0);
    g.add_city_enchantment(theirs, curse, P0);
    g.declare_war(P1, P0);
    g.drain_diplomacy_events();
    g.declare_peace(P0, P1);
    assert_eq!(g.spells().city_enchantments(mine), vec![(ward, P0)]);
    assert!(g.spells().city_enchantments(theirs).is_empty());
    Ok(())
}

#[test]
fn a_war_on_a_third_wizard_is_declared_by_the_one_asked() {
    let mut g = world();
    for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
        met(&mut g, a, b);
    }
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::WarOn(P2),
        },
    );
    command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert_eq!(g.treaty(P1, P2), Treaty::War);
    assert_eq!(g.treaty(P0, P2), Treaty::None);
}

#[test]
fn asking_an_ally_to_break_with_another_makes_it_do_so() -> TestResult {
    let mut g = world();
    for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
        met(&mut g, a, b);
    }
    g.start_treaty(P1, P2, Treaty::Alliance);
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::BreakAllianceWith(P2),
        },
    );
    command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert_eq!(g.treaty(P1, P2), Treaty::None);
    assert_eq!(
        g.diplomacy
            .stance(P1, P2)
            .ok_or("stance is missing")?
            .hidden,
        -20
    );
    Ok(())
}

#[test]
fn an_unanswered_proposal_lapses_at_the_end_of_the_turn() {
    let mut g = world();
    met(&mut g, P0, P1);
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Pact,
        },
    );
    let events = diplomacy_turn(&mut g);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Diplomacy(DiplomacyEvent::ProposalAnswered {
            accepted: false,
            lapsed: true,
            ..
        })
    )));
    assert!(g.diplomacy.proposals().is_empty());
    assert_eq!(g.treaty(P0, P1), Treaty::None);
}

#[test]
fn a_pact_that_stopped_being_possible_is_not_signed() {
    let mut g = world();
    met(&mut g, P0, P1);
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Pact,
        },
    );
    g.declare_war(P1, P0);
    command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert_eq!(g.treaty(P0, P1), Treaty::War);
}

#[test]
fn breaking_a_pact_by_command_needs_one() -> TestResult {
    let mut g = world();
    met(&mut g, P0, P1);
    let none = command(
        &mut g,
        DiplomacyCommand::BreakTreaty {
            player: P0,
            with: P1,
        },
    );
    assert_eq!(rejected(&none), Some(DiplomacyError::NotAllowed));
    g.start_treaty(P0, P1, Treaty::Pact);
    command(
        &mut g,
        DiplomacyCommand::BreakTreaty {
            player: P0,
            with: P1,
        },
    );
    assert_eq!(g.treaty(P0, P1), Treaty::None);
    assert_eq!(
        g.diplomacy
            .stance(P0, P1)
            .ok_or("stance is missing")?
            .hidden,
        -10
    );
    Ok(())
}

#[test]
fn a_wizard_can_declare_war_only_on_one_it_has_met() {
    let mut g = world();
    let events = command(&mut g, DiplomacyCommand::DeclareWar { player: P1, on: P2 });
    assert_eq!(rejected(&events), Some(DiplomacyError::NotContacted));
    met(&mut g, P1, P2);
    command(&mut g, DiplomacyCommand::DeclareWar { player: P1, on: P2 });
    assert_eq!(g.treaty(P1, P2), Treaty::War);
}

#[test]
fn gold_is_a_gift_that_warms() -> TestResult {
    let mut g = world();
    met(&mut g, P0, P1);
    set_rel(&mut g, P0, P1, 0);
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold = 300;
    g.treasury_mut(P1).ok_or("treasury_mut is missing")?.gold = 100;
    let events = command(
        &mut g,
        DiplomacyCommand::GiveGold {
            from: P0,
            to: P1,
            amount: 100,
        },
    );
    assert!(rejected(&events).is_none(), "{events:?}");
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().gold,
        200
    );
    assert_eq!(
        g.player(P1).ok_or("player is missing")?.treasury().gold,
        200
    );
    assert!(g.relation(P0, P1) >= 0);
    let broke = command(
        &mut g,
        DiplomacyCommand::GiveGold {
            from: P0,
            to: P1,
            amount: 999,
        },
    );
    assert_eq!(rejected(&broke), Some(DiplomacyError::NotEnoughGold));
    let nobody = command(
        &mut g,
        DiplomacyCommand::GiveGold {
            from: P0,
            to: P2,
            amount: 10,
        },
    );
    assert_eq!(rejected(&nobody), Some(DiplomacyError::NotContacted));
    Ok(())
}

#[test]
fn a_spell_given_is_known_and_wins_trust() -> TestResult {
    let mut g = world();
    met(&mut g, P0, P1);
    let famine = g
        .content
        .rules
        .spells
        .id_of("famine")
        .ok_or("famine is not in the content")?;
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(famine);
    g.wizard_mut(P1)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .remove(&famine);
    set_rel(&mut g, P0, P1, 0);
    let events = command(
        &mut g,
        DiplomacyCommand::GiveSpell {
            from: P0,
            to: P1,
            spell: famine,
        },
    );
    assert!(rejected(&events).is_none(), "{events:?}");
    assert!(g.wizard(P1).ok_or("wizard is missing")?.knows(famine));
    assert!(g.relation(P0, P1) > 0);
    let hidden = g
        .diplomacy
        .stance(P1, P0)
        .ok_or("stance is missing")?
        .hidden;
    assert!((2..=9).contains(&hidden), "{hidden}");
    // It is not worth giving twice.
    let again = command(
        &mut g,
        DiplomacyCommand::GiveSpell {
            from: P0,
            to: P1,
            spell: famine,
        },
    );
    assert_eq!(rejected(&again), Some(DiplomacyError::SpellUnavailable));
    Ok(())
}

#[test]
fn spells_are_exchanged_one_for_one() -> TestResult {
    let mut g = world();
    met(&mut g, P0, P1);
    let famine = g
        .content
        .rules
        .spells
        .id_of("famine")
        .ok_or("famine is not in the content")?;
    let gaia = g
        .content
        .rules
        .spells
        .id_of("gaias_blessing")
        .ok_or("gaias_blessing is not in the content")?;
    for s in [famine, gaia] {
        for p in [P0, P1] {
            g.wizard_mut(p)
                .ok_or("wizard_mut is missing")?
                .get_mut::<crate::wizard::sheet::KnownSpells>()
                .known
                .remove(&s);
        }
    }
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(famine);
    g.wizard_mut(P1)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(gaia);
    let kind = ProposalKind::Exchange {
        give: famine,
        get: gaia,
    };
    assert!(g.check_proposal(P0, P1, kind).is_ok());
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind,
        },
    );
    command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert!(g.wizard(P0).ok_or("wizard is missing")?.knows(gaia));
    assert!(g.wizard(P1).ok_or("wizard is missing")?.knows(famine));
    // Nothing left to swap.
    assert!(g.check_proposal(P0, P1, kind).is_err());
    Ok(())
}

#[test]
fn answering_wears_down_the_askers_patience_whatever_the_answer() -> TestResult {
    for accept in [false, true] {
        let mut g = world();
        met(&mut g, P0, P1);
        g.diplomacy.stance_mut(P0, P1).treaty_patience = 100;
        let events = command(
            &mut g,
            DiplomacyCommand::Propose {
                from: P0,
                to: P1,
                kind: ProposalKind::Pact,
            },
        );
        assert!(rejected(&events).is_none(), "{events:?}");
        command(
            &mut g,
            DiplomacyCommand::Answer {
                player: P1,
                from: P0,
                accept,
            },
        );
        let s = g.diplomacy.stance(P0, P1).ok_or("stance is missing")?;
        // 10 and 20 to 49 are taken, whatever the answer.
        assert!(
            (100 - 10 - 49..=100 - 10 - 20).contains(&s.treaty_patience),
            "{accept}: {}",
            s.treaty_patience
        );
        assert_eq!((s.exchange_patience, s.peace_patience), (-10, -10));
    }
    Ok(())
}

#[test]
fn a_spell_swap_is_not_a_test_of_patience_and_wears_none_when_answered_no() -> TestResult {
    let mut g = world();
    met(&mut g, P0, P1);
    let before = g.diplomacy.stance(P0, P1).map_or(0, |s| s.treaty_patience);
    let spell = g
        .content
        .rules
        .spells
        .id_of("spell_of_mastery")
        .ok_or("spell_of_mastery is not in the content")?;
    g.apply(Command::Diplomacy(DiplomacyCommand::Propose {
        from: P0,
        to: P1,
        kind: ProposalKind::Exchange {
            give: spell,
            get: spell,
        },
    }));
    g.apply(Command::Diplomacy(DiplomacyCommand::Answer {
        player: P1,
        from: P0,
        accept: false,
    }));
    assert_eq!(
        g.diplomacy.stance(P0, P1).map_or(0, |s| s.treaty_patience),
        before
    );
    Ok(())
}

#[test]
fn peace_runs_down_and_patience_comes_back() -> TestResult {
    let mut g = world();
    met(&mut g, P0, P1);
    g.diplomacy.stance_mut(P1, P0).peace_duration = 3;
    g.diplomacy.stance_mut(P1, P0).treaty_patience = -100;
    g.diplomacy.stance_mut(P1, P0).peace_patience = 100;
    diplomacy_turn(&mut g);
    let s = g.diplomacy.stance(P1, P0).ok_or("stance is missing")?;
    assert_eq!(s.peace_duration, 2);
    // +10 at the end of the turn, then up to twice 1..5 while below zero (the first)
    // and below fifty (the second).
    assert!(
        (-100 + 10 + 2..=-100 + 10 + 10).contains(&s.treaty_patience),
        "{}",
        s.treaty_patience
    );
    assert_eq!(s.peace_patience, 100, "above 100 nothing is added");
    Ok(())
}

#[test]
fn a_pact_makes_friends_over_time() {
    let mut g = world();
    met(&mut g, P1, P2);
    set_rel(&mut g, P1, P2, 0);
    set_pair(&mut g, P1, P2, |p| p.default = 0);
    g.start_treaty(P1, P2, Treaty::Alliance);
    for _ in 0..40 {
        diplomacy_turn(&mut g);
    }
    assert!(g.relation(P1, P2) > 10, "{}", g.relation(P1, P2));
    assert!(g.relation(P1, P2) <= 100);
}

#[test]
fn without_cause_relations_drift_back_to_the_start() -> TestResult {
    let mut g = world();
    met(&mut g, P1, P2);
    let default = g.diplomacy.pair(P1, P2).ok_or("pair is missing")?.default;
    set_rel(&mut g, P1, P2, default - 30);
    for _ in 0..60 {
        diplomacy_turn(&mut g);
    }
    assert_eq!(g.relation(P1, P2), default);
    // From above it comes down every tenth turn only.
    set_rel(&mut g, P1, P2, default + 20);
    for _ in 0..200 {
        diplomacy_turn(&mut g);
    }
    assert_eq!(g.relation(P1, P2), default);
    Ok(())
}

#[test]
fn a_recent_deed_holds_the_drift_for_a_turn() -> TestResult {
    let mut g = world();
    met(&mut g, P0, P1);
    let default = g.diplomacy.pair(P0, P1).ok_or("pair is missing")?.default;
    set_rel(&mut g, P0, P1, default + 30);
    g.diplomacy.stance_mut(P1, P0).last_action = Some(LastAction {
        action: Action::Friendship,
        strength: 5,
        spell: None,
    });
    diplomacy_turn(&mut g);
    assert_eq!(g.relation(P0, P1), default + 30);
    assert!(g
        .diplomacy
        .stance(P1, P0)
        .ok_or("stance is missing")?
        .last_action
        .is_none());
    Ok(())
}

#[test]
fn enchantments_offend_the_wizards_they_offend() -> TestResult {
    let mut g = world();
    for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
        met(&mut g, a, b);
    }
    g.init_wizard(P1, &[("life", 2)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    g.init_wizard(P2, &[("chaos", 2)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    for (a, b) in [(P1, P0), (P2, P0)] {
        set_rel(&mut g, a, b, 0);
    }
    // Eternal Night offends both; Evil Omens only a Life or Nature wizard.
    g.enchantment_relations(P0, "eternal_night", 1);
    assert_eq!(g.relation(P1, P0), -12);
    assert_eq!(g.relation(P2, P0), -12);
    set_rel(&mut g, P1, P0, 0);
    set_rel(&mut g, P2, P0, 0);
    g.enchantment_relations(P0, "evil_omens", 1);
    assert_eq!(g.relation(P1, P0), -20);
    assert_eq!(g.relation(P2, P0), 0, "a chaos wizard does not mind");
    // The per-turn share is a tenth, rounded to zero.
    set_rel(&mut g, P1, P0, 0);
    g.enchantment_relations(P0, "eternal_night", 10);
    assert_eq!(g.relation(P1, P0), -1);
    // Nothing for a spell nobody minds.
    set_rel(&mut g, P1, P0, 0);
    g.enchantment_relations(P0, "water_walking", 1);
    assert_eq!(g.relation(P1, P0), 0);
    Ok(())
}

#[test]
fn a_curse_on_a_city_is_noticed_by_its_owner() -> TestResult {
    let mut g = world();
    met(&mut g, P0, P1);
    set_rel(&mut g, P0, P1, 0);
    let famine = g
        .content
        .rules
        .spells
        .id_of("famine")
        .ok_or("famine is not in the content")?;
    g.cast_relations(P0, famine, Some(P1));
    assert_eq!(g.relation(P0, P1), -5);
    // On one's own city it is nobody's business.
    set_rel(&mut g, P0, P1, 0);
    g.cast_relations(P0, famine, None);
    assert_eq!(g.relation(P0, P1), 0);
    Ok(())
}

#[test]
fn casting_the_spell_of_mastery_turns_the_others_against_the_caster() -> TestResult {
    let mut g = world();
    for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
        met(&mut g, a, b);
    }
    for b in [P1, P2] {
        set_rel(&mut g, P0, b, 0);
    }
    let mastery = g
        .content
        .rules
        .spells
        .id_of("spell_of_mastery")
        .ok_or("spell_of_mastery is not in the content")?;
    g.begin_cast_relations(P0, mastery);
    // The rules only sour the relation; a war on the caster is the AI's decision
    // (`f4x-ai`, `diplomacy::mastery_scare`).
    assert_eq!(g.treaty(P0, P1), Treaty::None);
    assert_eq!(g.relation(P0, P1), -50);
    Ok(())
}

#[test]
fn a_spell_of_mastery_being_cast_sours_relations_a_little_each_turn() -> TestResult {
    let mut g = world();
    met(&mut g, P1, P2);
    // Begun, the Spell of Mastery keeps bothering them a little each turn (half of fifty).
    let mastery = g
        .content
        .rules
        .spells
        .id_of("spell_of_mastery")
        .ok_or("spell_of_mastery is not in the content")?;
    g.begin_casting(P1, mastery, 100);
    set_rel(&mut g, P1, P2, 0);
    g.with_diplomacy(|d, ctx| d.enchantments_in_force(ctx));
    // Half of fifty is twenty-five.
    assert_eq!(g.relation(P1, P2), -25);
    Ok(())
}

#[test]
fn attacking_a_partner_breaks_the_treaty_and_costs_relations() -> TestResult {
    let mut g = world();
    met(&mut g, P1, P2);
    g.start_treaty(P1, P2, Treaty::Pact);
    set_rel(&mut g, P1, P2, 30);
    g.attack_relations(P1, P2);
    assert_eq!(g.treaty(P1, P2), Treaty::None);
    assert!(g.relation(P1, P2) < 30 - 40 + 1, "{}", g.relation(P1, P2));
    assert_eq!(
        g.diplomacy
            .stance(P1, P2)
            .ok_or("stance is missing")?
            .hidden,
        -10
    );
    // With no treaty there is nothing to break and nothing to lose.
    let before = g.relation(P1, P2);
    g.attack_relations(P1, P2);
    assert_eq!(g.relation(P1, P2), before);
    Ok(())
}

#[test]
fn a_battle_against_a_partner_breaks_the_pact() -> TestResult {
    let mut g = world();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a = g.spawn_unit(P1, at(5, 5), walker);
    let d = g.spawn_unit(P2, at(6, 5), walker);
    met(&mut g, P1, P2);
    g.start_treaty(P1, P2, Treaty::Pact);
    g.drain_diplomacy_events();
    let events = g.apply(Command::BeginBattle {
        attackers: vec![a],
        defenders: vec![d],
    });
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::BattleStarted { .. })),
        "{events:?}"
    );
    assert_eq!(g.treaty(P1, P2), Treaty::None);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Diplomacy(DiplomacyEvent::TreatyBroken { breaker, victim, treaty: Treaty::Pact })
            if *breaker == P1 && *victim == P2
    )));
    Ok(())
}

#[test]
fn the_losses_of_a_defending_wizard_cost_the_attacker_its_goodwill() -> TestResult {
    use crate::battle::{Side, Status};
    let mut g = world();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a = g.spawn_unit(P2, at(5, 5), walker);
    let d: Vec<_> = (0..3).map(|_| g.spawn_unit(P1, at(6, 5), walker)).collect();
    met(&mut g, P1, P2);
    g.apply(Command::BeginBattle {
        attackers: vec![a],
        defenders: d,
    });
    let before = g.relation(P1, P2);
    {
        let b = &mut g.battle.as_mut().ok_or("as_mut is missing")?.battle;
        for u in b.units.iter_mut().filter(|u| u.side == Side::Defender) {
            u.status = Status::Dead;
        }
        b.winner = Some(Side::Attacker);
    }
    g.finish_battle();
    // Three units are worth 1 to 20 each; the relation falls by at least that, twice over when
    // it was positive.
    assert!(
        g.relation(P1, P2) < before,
        "{} {}",
        before,
        g.relation(P1, P2)
    );
    Ok(())
}

#[test]
fn a_battle_against_a_stranger_makes_them_known_to_each_other() -> TestResult {
    let mut g = world();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a = g.spawn_unit(P1, at(5, 5), walker);
    let d = g.spawn_unit(P2, at(6, 5), walker);
    set_pair(&mut g, P1, P2, |p| p.contacted = false);
    g.apply(Command::BeginBattle {
        attackers: vec![a],
        defenders: vec![d],
    });
    assert!(g.have_met(P1, P2));
    assert_eq!(g.treaty(P1, P2), Treaty::None);
    Ok(())
}

#[test]
fn attacking_a_wizard_one_has_not_met_meets_it() {
    let mut g = world();
    g.attack_relations(P1, P2);
    assert!(g.have_met(P1, P2));
}

#[test]
fn taking_a_city_is_a_war() {
    let mut g = world();
    let target = city(&mut g, P1, at(10, 10));
    city(&mut g, P1, at(20, 5));
    let events = g.conquer_city(target, P2, 0);
    assert!(!events.is_empty());
    assert_eq!(g.treaty(P1, P2), Treaty::War);
    assert!(g.relation(P1, P2) <= -76);
    let events = g.drain_diplomacy_events();
    assert!(
        events.contains(&Event::Diplomacy(DiplomacyEvent::WarDeclared {
            attacker: P2,
            defender: P1,
        }))
    );
}

#[test]
fn taking_an_independent_city_is_no_diplomacy() -> TestResult {
    let mut g = world();
    g.add_neutral_player(PlayerId(9));
    g.treasury_mut(PlayerId(9))
        .ok_or("treasury_mut is missing")?
        .gold = 0;
    let target = city(&mut g, PlayerId(9), at(10, 10));
    g.conquer_city(target, P1, 0);
    assert!(g.drain_diplomacy_events().is_empty());
    assert!(!g.have_met(P1, PlayerId(9)));
    Ok(())
}

#[test]
fn a_city_that_holds_costs_the_attacker_twenty_or_sixty() {
    let mut g = world();
    met(&mut g, P1, P2);
    set_rel(&mut g, P1, P2, 0);
    g.city_held_relations(P2, P1, false);
    assert_eq!(g.relation(P1, P2), -20);
    set_rel(&mut g, P1, P2, 0);
    g.city_held_relations(P2, P1, true);
    assert_eq!(g.relation(P1, P2), -60);
}

#[test]
fn a_fallen_fortress_is_held_against_the_winner_by_everyone_else() {
    let mut g = world();
    for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
        met(&mut g, a, b);
    }
    set_rel(&mut g, P1, P2, 0);
    g.conquest_relations(P2, P0, false);
    assert!(g.relation(P1, P2) < 0);
    assert!(g.have_met(P0, P1), "a banished wizard is still known");
    // A wizard destroyed is forgotten, and costs more.
    let banished = g.relation(P1, P2);
    set_rel(&mut g, P1, P2, 0);
    g.conquest_relations(P2, P0, true);
    assert!(g.relation(P1, P2) < banished.min(0));
    assert!(!g.have_met(P0, P1));
    assert!(!g.have_met(P0, P2));
    assert_eq!(g.treaty(P0, P1), Treaty::None);
}

#[test]
fn diplomacy_survives_a_save() -> TestResult {
    let mut g = world();
    for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
        met(&mut g, a, b);
    }
    g.start_treaty(P1, P2, Treaty::Alliance);
    g.declare_war(P0, P1);
    g.wizard_mut(P2)
        .ok_or("wizard_mut is missing")?
        .put(Some(Personality::Lawful));
    g.diplomacy.stance_mut(P2, P0).last_action = Some(LastAction {
        action: Action::AttackedCity,
        strength: -20,
        spell: Some("famine".into()),
    });
    g.diplomacy.proposals.push(Proposal {
        from: P0,
        to: P2,
        kind: ProposalKind::WarOn(P1),
    });
    let famine = g
        .content
        .rules
        .spells
        .id_of("famine")
        .ok_or("famine is not in the content")?;
    let gaia = g
        .content
        .rules
        .spells
        .id_of("gaias_blessing")
        .ok_or("gaias_blessing is not in the content")?;
    g.diplomacy.proposals.push(Proposal {
        from: P1,
        to: P2,
        kind: ProposalKind::Exchange {
            give: famine,
            get: gaia,
        },
    });
    g.drain_diplomacy_events();
    let snap: Snapshot = g.snapshot();
    let text = ron::to_string(&snap)?;
    let back: Snapshot = ron::from_str(&text)?;
    assert_eq!(back, snap);
    let h = back.restore(shipped())?;
    assert_eq!(h.diplomacy, g.diplomacy);
    assert_eq!(h.treaty(P1, P2), Treaty::Alliance);
    assert_eq!(h.treaty(P0, P1), Treaty::War);
    assert_eq!(h.personality(P2), Personality::Lawful);
    Ok(())
}

#[test]
fn an_old_save_has_no_diplomacy() -> TestResult {
    let g = world();
    let mut snap = g.snapshot();
    snap.diplomacy = DiplomacySave::default();
    let h = snap.restore(shipped())?;
    assert!(!h.have_met(P0, P1));
    assert_eq!(h.relation(P0, P1), g.relation(P0, P1));
    Ok(())
}

#[test]
fn the_same_game_replays_the_same_diplomacy() {
    let play = || {
        let mut g = world();
        city(&mut g, P0, at(10, 10));
        city(&mut g, P1, at(14, 10));
        met(&mut g, P1, P2);
        g.declare_war(P1, P2);
        for _ in 0..30 {
            g.apply(Command::EndTurn);
        }
        g.diplomacy.clone()
    };
    assert_eq!(play(), play());
}

fn propose_and_answer(
    g: &mut Game,
    from: PlayerId,
    to: PlayerId,
    kind: ProposalKind,
    accept: bool,
) {
    let made = command(g, DiplomacyCommand::Propose { from, to, kind });
    assert!(rejected(&made).is_none(), "{made:?}");
    let events = command(
        g,
        DiplomacyCommand::Answer {
            player: to,
            from,
            accept,
        },
    );
    assert!(rejected(&events).is_none(), "{events:?}");
}

#[test]
fn a_threat_that_works_buys_the_threatener_some_peace_and_costs_goodwill() -> TestResult {
    let mut g = world();
    met(&mut g, P0, P1);
    set_rel(&mut g, P0, P1, 20);
    propose_and_answer(&mut g, P0, P1, ProposalKind::Threat, true);
    let s = g.diplomacy.stance(P1, P0).ok_or("stance is missing")?;
    assert!((1..=15).contains(&s.peace_duration), "{}", s.peace_duration);
    assert!(g.relation(P0, P1) <= 20);
    assert_eq!(
        g.diplomacy
            .stance(P0, P1)
            .ok_or("stance is missing")?
            .treaty_patience,
        -120
    );
    // A refused threat gives no peace.
    let mut g = world();
    met(&mut g, P0, P1);
    propose_and_answer(&mut g, P0, P1, ProposalKind::Threat, false);
    assert_eq!(
        g.diplomacy.stance(P1, P0).map_or(0, |s| s.peace_duration),
        0
    );
    // Nobody threatens a wizard it is at war with.
    g.declare_war(P0, P1);
    let events = command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Threat,
        },
    );
    assert_eq!(rejected(&events), Some(DiplomacyError::NotAllowed));
    Ok(())
}

#[test]
fn a_demand_is_paid_when_accepted_and_resented_when_refused() -> TestResult {
    let mut g = world();
    met(&mut g, P1, P0);
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold = 200;
    g.treasury_mut(P1).ok_or("treasury_mut is missing")?.gold = 10;
    set_rel(&mut g, P0, P1, 0);
    propose_and_answer(&mut g, P1, P0, ProposalKind::Demand { gold: 75 }, true);
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().gold,
        125
    );
    assert_eq!(g.player(P1).ok_or("player is missing")?.treasury().gold, 85);
    let before = g.relation(P0, P1);
    propose_and_answer(&mut g, P1, P0, ProposalKind::Demand { gold: 50 }, false);
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().gold,
        125
    );
    // The human is never changed, so the AI's own side of a refusal shows in its patience only
    // when the demand comes from the human; here the human refuses and the AI's feelings fall.
    assert!(g.relation(P0, P1) <= before);
    // More than the human has is not paid even when answered yes.
    propose_and_answer(&mut g, P1, P0, ProposalKind::Demand { gold: 500 }, true);
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().gold,
        125
    );
    Ok(())
}

#[test]
fn a_warning_is_counted_and_told() -> TestResult {
    let mut g = world();
    met(&mut g, P1, P0);
    let events = command(&mut g, DiplomacyCommand::Warn { from: P1, to: P0 });
    assert!(events.contains(&Event::Diplomacy(DiplomacyEvent::Warned {
        from: P1,
        to: P0
    })));
    command(&mut g, DiplomacyCommand::Warn { from: P1, to: P0 });
    assert_eq!(
        g.diplomacy
            .stance(P1, P0)
            .ok_or("stance is missing")?
            .warnings,
        2
    );
    // Strangers warn no one.
    let events = command(&mut g, DiplomacyCommand::Warn { from: P1, to: P2 });
    assert_eq!(rejected(&events), Some(DiplomacyError::NotContacted));
    Ok(())
}

#[test]
fn an_alliance_does_not_outlast_one_ally_fighting_the_others_ally() {
    let mut g = world();
    for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
        met(&mut g, a, b);
    }
    g.start_treaty(P1, P2, Treaty::Alliance);
    g.start_treaty(P0, P2, Treaty::Alliance);
    g.declare_war(P0, P1);
    diplomacy_turn(&mut g);
    // P2 stands with both sides' ally; the alliance of P2 with one of the two quarrellers goes.
    let allied = [Treaty::Alliance].contains(&g.treaty(P1, P2)) as u8
        + [Treaty::Alliance].contains(&g.treaty(P0, P2)) as u8;
    assert!(allied < 2, "{:?} {:?}", g.treaty(P1, P2), g.treaty(P0, P2));
}

#[test]
fn units_next_to_a_partners_city_are_warned_off_and_then_the_treaty_ends() -> TestResult {
    let mut g = world();
    met(&mut g, P1, P2);
    g.start_treaty(P1, P2, Treaty::Pact);
    city(&mut g, P1, at(10, 10));
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    g.spawn_unit(P2, at(11, 11), walker);
    let first = diplomacy_turn(&mut g);
    assert!(
        first.contains(&Event::Diplomacy(DiplomacyEvent::Trespassing {
            owner: P1,
            intruder: P2
        }))
    );
    assert_eq!(g.treaty(P1, P2), Treaty::Pact);
    let second = diplomacy_turn(&mut g);
    assert!(!second
        .iter()
        .any(|e| matches!(e, Event::Diplomacy(DiplomacyEvent::Trespassing { .. }))));
    assert_eq!(g.treaty(P1, P2), Treaty::Pact);
    diplomacy_turn(&mut g);
    assert_eq!(g.treaty(P1, P2), Treaty::None);
    Ok(())
}

#[test]
fn leaving_the_city_resets_the_warnings_and_wars_and_distance_are_ignored() -> TestResult {
    let mut g = world();
    met(&mut g, P1, P2);
    g.start_treaty(P1, P2, Treaty::Pact);
    city(&mut g, P1, at(10, 10));
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let unit = g.spawn_unit(P2, at(11, 11), walker);
    diplomacy_turn(&mut g);
    assert_eq!(
        g.diplomacy
            .stance(P1, P2)
            .ok_or("stance is missing")?
            .trespass,
        1
    );
    g.apply(Command::DismissUnit { unit });
    g.spawn_unit(P2, at(14, 10), walker);
    diplomacy_turn(&mut g);
    assert_eq!(
        g.diplomacy
            .stance(P1, P2)
            .ok_or("stance is missing")?
            .trespass,
        0
    );
    assert_eq!(g.treaty(P1, P2), Treaty::Pact);
    Ok(())
}

#[test]
fn the_cities_allowed_grow_with_the_map_and_the_land() {
    let mut g = world();
    g.settings.basic.map_width = 60;
    g.settings.basic.map_height = 40;
    g.settings.basic.land_percent = crate::game::setting::LAND_MEDIUM;
    assert_eq!(city_allowance(&g), 6);
    g.settings.basic.land_percent = crate::game::setting::LAND_LARGE;
    assert_eq!(city_allowance(&g), 9);
    g.settings.basic.map_width = 90;
    g.settings.basic.map_height = 60;
    assert_eq!(city_allowance(&g), 20);
}
