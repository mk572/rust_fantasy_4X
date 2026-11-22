use super::*;
use crate::testworld::{at, game, spawn, COAST_CITY, P0, P1};
use f4x_core::game::Event;
use f4x_core::rules::RealmId;
use f4x_def::half_int::HalfInt;

type TestResult = Result<(), Box<dyn std::error::Error>>;

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
        wizard(&mut g, p, &[(RealmId::new(3 /* life */), 3)], &[], None);
    }
    spawn(
        &mut g,
        P0,
        at(COAST_CITY.0, COAST_CITY.1),
        95, /* high_elf_spearmen */
        8,
    );
    g.make_contact(P0, P1);
    if let Some((x, y)) = rival {
        g.explore(P0, at(x, y), HalfInt::from_raw(3));
    }
    g.turn = 100;
    Ok(g)
}

#[test]
fn the_factors_ease_a_treaty_a_little_and_a_broken_one_is_remembered() -> TestResult {
    let mut g = lopsided()?;
    let mut ai = Ai::new(P0);
    let plain = affinity(&g, &ai.minds(), P0, P1);
    // The same race is a bonus (both are high men), a Charismatic retort another.
    wizard(
        &mut g,
        P1,
        &[(RealmId::new(3 /* life */), 3)],
        &[Retort::new(16) /* charismatic */],
        None,
    );
    let charismatic = affinity(&g, &ai.minds(), P0, P1);
    assert_eq!(charismatic, plain + Knobs::current().charismatic);
    // Life against Death do not get on.
    wizard(&mut g, P1, &[(RealmId::new(4 /* death */), 3)], &[], None);
    assert!(affinity(&g, &ai.minds(), P0, P1) < plain);
    wizard(&mut g, P1, &[(RealmId::new(3 /* life */), 3)], &[], None);
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
    let before = affinity(&g, &ai.minds(), P0, P1);
    let events = g.apply(Command::Diplomacy(DiplomacyCommand::BreakTreaty {
        player: P1,
        with: P0,
    }));
    ai.notice(&g, &events);
    assert_eq!(
        affinity(&g, &ai.minds(), P0, P1),
        before - Knobs::current().broken_memory
    );
    Ok(())
}

#[test]
fn no_trade_arms_an_enemy() -> TestResult {
    let mut g = lopsided()?;
    wizard(&mut g, P0, &[(RealmId::new(3 /* life */), 5)], &[], None);
    // At war with it: nothing is traded.
    g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
        player: P0,
        on: P1,
    }));
    assert!(crate::trade::offer(&g, &Ai::new(P0).minds(), P0, P1).is_none());
    assert!(arms_enemy(&g, P0, P1));
    Ok(())
}

#[test]
fn neutral_races_take_part_too() -> TestResult {
    // A neutral race player far weaker than the wizard is a target like a wizard would be: it is in the
    // scene, and its enemy value reaches the mark for a war.
    let mut g = lopsided_at(None)?;
    let race = g.content().rules.races.id_numbered(3).ok_or("a race")?;
    let neutral = f4x_core::player::race_player::race_player_id(race);
    g.add_neutral_player(neutral);
    found(&mut g, neutral, WEST_RIVAL.0, WEST_RIVAL.1);
    g.make_contact(P0, neutral);
    g.explore(P0, at(WEST_RIVAL.0, WEST_RIVAL.1), HalfInt::from_raw(3));
    let ai = Ai::new(P0);
    let scene = crate::statecraft::Scene::new(&g, P0);
    assert!(scene.others.contains(&neutral), "the race is not met");
    let reach = crate::statecraft::can_attack(&g, P0, &scene, neutral);
    assert!(reach, "no army can get there");
    let value =
        crate::diplo_b_stance::values(&g, P0, neutral, &scene, &ai.memory.get(), reach).enemy;
    assert!(value >= crate::diplo_b::Knobs::current().declare, "{value}");
    Ok(())
}

#[test]
fn the_lead_is_above_one_for_the_wizard_ahead_and_one_for_a_wizard_that_has_met_nobody(
) -> TestResult {
    let g = lopsided()?;
    assert!(lead_of(&g, P0) > 1.0, "{}", lead_of(&g, P0));
    let alone = game(COAST_CITY, 8 /* high_men */);
    assert_eq!(lead_of(&alone, P0), 1.0);
    Ok(())
}
