use super::*;
use crate::testworld::{at, game, spawn, COAST_CITY, ENEMY_CITY, P0, P1};
use f4x_core::diplomacy::DiplomacyEvent;
use f4x_core::game::Event;
use f4x_core::rules::RealmId;
use f4x_def::half_int::HalfInt;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const P2: PlayerId = PlayerId(2);
/// The spearmen of the shipped content.
const SPEARMEN: u16 = 95;
const LIFE: u16 = 3;
const DEATH: u16 = 4;

/// Give a wizard its books, retorts and start spells.
fn wizard(
    g: &mut Game,
    p: PlayerId,
    books: &[(RealmId, u8)],
    retorts: &[Retort],
    spells: Option<&[f4x_def::rules::SpellId]>,
) {
    g.init_wizard(p, books, retorts, spells)
        .expect("init_wizard succeeds");
}

fn found(g: &mut Game, owner: PlayerId, x: u32, y: u32) {
    let race = g.content().rules.races.id_numbered(8);
    let events = g.apply(Command::FoundCity {
        owner,
        pos: at(x, y),
        name: format!("City{x}x{y}"),
        race,
        population: 5,
    });
    assert!(
        !events.iter().any(|e| matches!(e, Event::Rejected(_))),
        "{events:?}"
    );
}

/// The west wizard (P0) holds four cities and an army of ten on the west island; the east
/// wizard (P1) holds one city there and one on the east island and no army. They have met and
/// P0 has seen the nearer city of P1. It is turn 100.
fn lopsided() -> Result<Game, Box<dyn std::error::Error>> {
    lopsided_at(Some(WEST_RIVAL))
}

/// Where the east wizard has a city on the west island.
const WEST_RIVAL: (u32, u32) = (3, 8);

/// [`lopsided`], with the east wizard's city on the west island at `rival` or without one.
fn lopsided_at(rival: Option<(u32, u32)>) -> Result<Game, Box<dyn std::error::Error>> {
    let mut g = game(COAST_CITY, 8 /* high_men */);
    for (x, y) in [(6, 3), (10, 10)] {
        found(&mut g, P0, x, y);
    }
    if let Some((x, y)) = rival {
        found(&mut g, P1, x, y);
    }
    for p in [P0, P1] {
        wizard(&mut g, p, &[(RealmId::new(LIFE), 3)], &[], None);
    }
    spawn(&mut g, P0, at(COAST_CITY.0, COAST_CITY.1), SPEARMEN, 8);
    g.make_contact(P0, P1);
    if let Some((x, y)) = rival {
        g.explore(P0, at(x, y), HalfInt::from_raw(3));
    }
    g.turn = 100;
    Ok(g)
}

/// The west wizard (P0) holds one city and one spearman; the east wizard (P1) holds a city on
/// the west island and one on the east island, with an army of thirty. They have met and P0 has
/// seen the nearer city. It is turn 100 and P0 has 1000 gold.
fn strong_neighbour() -> Result<Game, Box<dyn std::error::Error>> {
    let mut g = game(COAST_CITY, 8 /* high_men */);
    found(&mut g, P1, WEST_RIVAL.0, WEST_RIVAL.1);
    for p in [P0, P1] {
        wizard(&mut g, p, &[(RealmId::new(LIFE), 3)], &[], None);
    }
    spawn(&mut g, P0, at(COAST_CITY.0, COAST_CITY.1), SPEARMEN, 1);
    spawn(&mut g, P1, at(WEST_RIVAL.0, WEST_RIVAL.1), SPEARMEN, 30);
    g.treasury_mut(P0).ok_or("treasury")?.gold = HalfInt::from_int(1000);
    g.make_contact(P0, P1);
    g.explore(P0, at(WEST_RIVAL.0, WEST_RIVAL.1), HalfInt::from_raw(3));
    g.turn = 100;
    Ok(g)
}

fn remarks(events: &[Event], why: Remark) -> usize {
    events
        .iter()
        .filter(
            |e| matches!(e, Event::Diplomacy(DiplomacyEvent::Remarked { why: w, .. }) if *w == why),
        )
        .count()
}

/// Run the statecraft of `player` for `turns` turns, keeping the events.
fn run_turns(g: &mut Game, ai: &mut Ai, turns: u32) -> Vec<Event> {
    let mut events = Vec::new();
    for _ in 0..turns {
        run(ai, g, &mut events);
        g.turn += 1;
    }
    events
}

#[test]
fn a_wizard_far_ahead_declares_war_and_says_why() -> TestResult {
    let mut g = lopsided()?;
    let mut ai = Ai::new(P0);
    let events = run_turns(&mut g, &mut ai, 400);
    assert_eq!(g.treaty(P0, P1), Treaty::War, "no war in 400 turns");
    assert_eq!(ai.quarrel, Some(P1));
    assert_eq!(remarks(&events, Remark::DominantWar), 1);
    Ok(())
}

#[test]
fn no_war_when_the_edge_is_small_or_the_turn_early() -> TestResult {
    // An army as large as its own.
    let mut g = lopsided()?;
    spawn(&mut g, P1, at(WEST_RIVAL.0, WEST_RIVAL.1), SPEARMEN, 8);
    let mut ai = Ai::new(P0);
    run_turns(&mut g, &mut ai, 400);
    assert_ne!(g.treaty(P0, P1), Treaty::War);
    // The same world before the war may begin.
    let mut g = lopsided()?;
    g.turn = 10;
    let mut ai = Ai::new(P0);
    for _ in 0..30 {
        run(&mut ai, &mut g, &mut Vec::new());
    }
    assert_ne!(g.treaty(P0, P1), Treaty::War, "turn {}", g.turn);
    Ok(())
}

#[test]
fn no_war_on_two_fronts() -> TestResult {
    let mut g = lopsided()?;
    g.add_player(P2, "Third");
    found(&mut g, P2, 32, 3);
    wizard(&mut g, P2, &[(RealmId::new(LIFE), 3)], &[], None);
    g.make_contact(P0, P2);
    g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
        player: P2,
        on: P0,
    }));
    assert_eq!(g.treaty(P0, P2), Treaty::War);
    let mut ai = Ai::new(P0);
    run_turns(&mut g, &mut ai, 400);
    assert_ne!(g.treaty(P0, P1), Treaty::War);
    Ok(())
}

#[test]
fn a_much_stronger_neighbour_is_appeased_and_not_attacked() -> TestResult {
    let mut g = strong_neighbour()?;
    let mut ai = Ai::new(P0);
    let events = run_turns(&mut g, &mut ai, 120);
    assert!(ai.is_appeasing(P1));
    assert_ne!(g.treaty(P0, P1), Treaty::War);
    let gave = events.iter().any(|e| {
        matches!(
            e,
            Event::Diplomacy(DiplomacyEvent::GoldGiven { from, to, .. }) if *from == P0 && *to == P1
        )
    });
    assert!(
        gave || g.treaty(P0, P1) == Treaty::Pact,
        "no gift and no pact in 120 turns"
    );
    assert!(remarks(&events, Remark::Appeasing) >= 1);
    Ok(())
}

#[test]
fn a_young_war_is_not_ended_by_the_loser_but_an_old_one_is() -> TestResult {
    let mut g = lopsided()?;
    g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
        player: P0,
        on: P1,
    }));
    assert_eq!(g.war_age(P0, P1), Some(0));
    // P0 is far ahead and the war has just begun: it will not make peace.
    assert!(!peace_ok(&g, P0, P1));
    g.turn += Knobs::current().spent_after;
    assert!(peace_ok(&g, P0, P1), "the gain is spent after a while");
    // The weaker side is always glad of peace (it has seen the other's cities).
    g.turn = 101;
    for (x, y) in [COAST_CITY, (6, 3), (10, 10)] {
        g.explore(P1, at(x, y), HalfInt::from_raw(3));
    }
    assert!(peace_ok(&g, P1, P0));
    Ok(())
}

#[test]
fn a_losing_wizard_offers_peace_and_says_why() -> TestResult {
    let mut g = strong_neighbour()?;
    g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
        player: P0,
        on: P1,
    }));
    let mut ai = Ai::new(P0);
    let events = run_turns(&mut g, &mut ai, 200);
    assert!(
        remarks(&events, Remark::WarWeary) >= 1,
        "no offer of peace in 200 turns"
    );
    Ok(())
}

#[test]
fn the_weaker_wizards_ally_against_the_one_that_leads() -> TestResult {
    let mut g = lopsided()?;
    // P1 and P0 are of a size; P2 is far ahead of both and has met them.
    spawn(&mut g, P1, at(WEST_RIVAL.0, WEST_RIVAL.1), SPEARMEN, 8);
    g.add_player(P2, "Leader");
    for (x, y) in [(32, 3), (32, 9), (28, 10)] {
        found(&mut g, P2, x, y);
    }
    wizard(&mut g, P2, &[(RealmId::new(LIFE), 3)], &[], None);
    spawn(&mut g, P2, at(32, 3), SPEARMEN, 60);
    g.make_contact(P0, P2);
    g.make_contact(P1, P2);
    for (x, y) in [(32, 3), (32, 9), (28, 10)] {
        g.explore(P0, at(x, y), HalfInt::from_raw(3));
        g.explore(P1, at(x, y), HalfInt::from_raw(3));
    }
    g.explore(P1, at(COAST_CITY.0, COAST_CITY.1), HalfInt::from_raw(3));
    assert_eq!(leader_of(&g, P0), Some(P2));
    assert_eq!(leader_of(&g, P2), None);
    let mut ai = Ai::new(P0);
    let events = run_turns(&mut g, &mut ai, 300);
    assert!(
        matches!(g.treaty(P0, P1), Treaty::Pact | Treaty::Alliance),
        "no treaty in 300 turns: {:?}",
        g.treaty(P0, P1)
    );
    assert!(remarks(&events, Remark::Coalition) >= 1);
    Ok(())
}

#[test]
fn an_alliance_against_a_leader_ends_when_the_leader_no_longer_leads() -> TestResult {
    for end in [0, 1] {
        let mut g = lopsided()?;
        spawn(&mut g, P1, at(WEST_RIVAL.0, WEST_RIVAL.1), SPEARMEN, 8);
        g.add_player(P2, "Leader");
        found(&mut g, P2, 32, 3);
        wizard(&mut g, P2, &[(RealmId::new(LIFE), 3)], &[], None);
        g.make_contact(P0, P2);
        g.apply(Command::Diplomacy(DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Alliance,
        }));
        g.apply(Command::Diplomacy(DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        }));
        assert_eq!(g.treaty(P0, P1), Treaty::Alliance);
        // The alliance was asked for against P2; nobody leads now (P2 has no army).
        assert_eq!(leader_of(&g, P0), None);
        let mut ai = Ai::new(P0);
        ai.statecraft.coalitions.push((P1, P2, g.turn));
        let k = Knobs {
            coalition_end: end,
            ..Knobs::current()
        };
        end_coalitions(&mut ai, &mut g, &mut Vec::new(), &k);
        assert!(ai.statecraft.coalitions.is_empty());
        let expected = if end == 1 {
            Treaty::None
        } else {
            Treaty::Alliance
        };
        assert_eq!(g.treaty(P0, P1), expected, "coalition_end {end}");
    }
    Ok(())
}

#[test]
fn a_wizard_far_ahead_attacks_a_neighbour_on_another_island_by_sea() -> TestResult {
    let mut g = lopsided_at(None)?;
    // P1 holds only the east island; P0 has seen its city and a coast beside it.
    g.explore(P0, at(ENEMY_CITY.0, ENEMY_CITY.1), HalfInt::from_raw(4));
    let mut ai = Ai::new(P0);
    let events = run_turns(&mut g, &mut ai, 400);
    assert_eq!(g.treaty(P0, P1), Treaty::War, "no war in 400 turns");
    assert_eq!(remarks(&events, Remark::DominantWar), 1);
    Ok(())
}

#[test]
fn the_factors_ease_a_treaty_a_little_and_a_broken_one_is_remembered() -> TestResult {
    let mut g = lopsided()?;
    let plain = affinity(&g, P0, P1);
    // The same race is a bonus (both are high men), a Charismatic retort another.
    wizard(
        &mut g,
        P1,
        &[(RealmId::new(LIFE), 3)],
        &[Retort::new(16) /* charismatic */],
        None,
    );
    let charismatic = affinity(&g, P0, P1);
    assert_eq!(charismatic, plain + Knobs::current().charismatic);
    // Life against Death do not get on.
    wizard(&mut g, P1, &[(RealmId::new(DEATH), 3)], &[], None);
    assert!(affinity(&g, P0, P1) < plain);
    wizard(&mut g, P1, &[(RealmId::new(LIFE), 3)], &[], None);
    // A treaty the other broke with it counts against it.
    g.apply(Command::Diplomacy(DiplomacyCommand::Propose {
        from: P0,
        to: P1,
        kind: ProposalKind::Pact,
    }));
    g.apply(Command::Diplomacy(DiplomacyCommand::Answer {
        player: P1,
        from: P0,
        accept: true,
    }));
    assert_eq!(g.treaty(P0, P1), Treaty::Pact);
    let before = affinity(&g, P0, P1);
    g.apply(Command::Diplomacy(DiplomacyCommand::BreakTreaty {
        player: P1,
        with: P0,
    }));
    assert_eq!(
        affinity(&g, P0, P1),
        before - Knobs::current().broken_memory
    );
    Ok(())
}

#[test]
fn no_trade_arms_an_enemy() -> TestResult {
    let mut g = lopsided()?;
    wizard(&mut g, P0, &[(RealmId::new(LIFE), 5)], &[], None);
    // At war with it: nothing is traded.
    g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
        player: P0,
        on: P1,
    }));
    assert!(crate::trade::offer(&g, P0, P1).is_none());
    assert!(arms_enemy(&g, P0, P1));
    Ok(())
}

#[test]
fn neutral_races_take_part_too() -> TestResult {
    // A neutral race player far weaker than the wizard is attacked like a wizard would be.
    let mut g = lopsided_at(None)?;
    let race = g.content().rules.races.id_numbered(3).ok_or("a race")?;
    let neutral = f4x_core::player::race_player::race_player_id(race);
    g.add_neutral_player(neutral);
    found(&mut g, neutral, WEST_RIVAL.0, WEST_RIVAL.1);
    g.make_contact(P0, neutral);
    g.explore(P0, at(WEST_RIVAL.0, WEST_RIVAL.1), HalfInt::from_raw(3));
    let mut ai = Ai::new(P0);
    run_turns(&mut g, &mut ai, 400);
    assert_eq!(
        g.treaty(P0, neutral),
        Treaty::War,
        "no war on the neutral race"
    );
    Ok(())
}
