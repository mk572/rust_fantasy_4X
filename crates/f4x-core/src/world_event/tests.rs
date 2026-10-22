use super::*;

fn moon(kind: EventKind) -> Events {
    let mut events = Events::default();
    events.start(kind, None, None);
    events
}

#[test]
fn the_moon_of_a_wizards_realm_doubles_their_religious_mana_and_the_other_halves_it() {
    let good = moon(EventKind::GoodMoon);
    // A temple is 2 mana: a life wizard gets 4, a death wizard 1.
    assert_eq!(good.moon_mana(2, true, false), 2);
    assert_eq!(good.moon_mana(2, false, true), -1);
    let bad = moon(EventKind::BadMoon);
    assert_eq!(bad.moon_mana(2, false, true), 2);
    assert_eq!(bad.moon_mana(2, true, false), -1);
}

#[test]
fn the_halving_rounds_down_like_the_original() {
    let good = moon(EventKind::GoodMoon);
    assert_eq!(
        good.moon_mana(1, false, true),
        0,
        "a lone shrine loses nothing"
    );
    assert_eq!(good.moon_mana(3, false, true), -1);
}

#[test]
fn no_moon_and_no_books_change_nothing() {
    assert_eq!(Events::default().moon_mana(5, true, true), 0);
    assert_eq!(moon(EventKind::GoodMoon).moon_mana(5, false, false), 0);
    assert_eq!(moon(EventKind::ManaShort).moon_mana(5, true, true), 0);
}

#[test]
fn a_wizard_of_both_realms_gets_both_changes() {
    // Good Moon: minus half (death) and plus all (life).
    assert_eq!(moon(EventKind::GoodMoon).moon_mana(4, true, true), 2);
}

#[test]
fn a_conjunction_doubles_the_nodes_of_its_colour_and_halves_the_others() {
    let chaos = moon(EventKind::ConjunctionChaos);
    assert_eq!(chaos.node_power("chaos", 10), 20);
    assert_eq!(chaos.node_power("nature", 10), 5);
    assert_eq!(chaos.node_power("sorcery", 7), 3);
    assert_eq!(Events::default().node_power("chaos", 10), 10);
    assert_eq!(
        moon(EventKind::ConjunctionNature).node_power("nature", 6),
        12
    );
}

#[test]
fn events_survive_a_save_and_unknown_ones_are_refused() {
    let mut events = moon(EventKind::ConjunctionNature);
    let victim = hecs::World::new().reserve_entity();
    events.start(EventKind::Plague, Some(victim), Some(Serial(7)));
    events.drawn(12);
    let saved = events.to_save(|_| Some(PlayerId(1)));
    assert_eq!(Events::from_save(&saved, |_| Some(victim)), Some(events));
    let mut bad = saved;
    bad.ongoing[0].kind = "blue_sun".into();
    assert_eq!(Events::from_save(&bad, |_| Some(victim)), None);
}

#[test]
fn the_first_five_turns_have_no_events_then_the_pressure_is_the_turns_waited_by_level() {
    let mut events = Events::default();
    for turn in 1..=5 {
        assert_eq!(
            events.pressure(turn, 2, EventFrequency::Normal),
            0,
            "turn {turn}"
        );
    }
    let at =
        |events: &mut Events, turn, level| events.pressure(turn, level, EventFrequency::Normal);
    assert_eq!(at(&mut events, 40, 4), 40);
    assert_eq!(at(&mut events, 40, 2), 30);
    assert_eq!(at(&mut events, 40, 0), 20);
    events.drawn(40);
    for turn in 41..=45 {
        assert_eq!(
            at(&mut events, turn, 4),
            0,
            "five quiet turns after an event"
        );
    }
    assert_eq!(at(&mut events, 46, 4), 6, "counted from the last event");
}

#[test]
fn a_draw_fires_when_the_roll_of_512_is_not_over_the_pressure() {
    assert!(fires(100, 100));
    assert!(!fires(100, 101));
    assert!(!fires(0, 1));
}

#[test]
fn good_events_favour_the_weak_and_bad_ones_the_strong() {
    assert_eq!(victim_weights(&[100, 300], false), vec![100, 300]);
    assert_eq!(victim_weights(&[100, 300], true), vec![250, 150]);
    // A wizard of 600 or more power is never helped.
    assert_eq!(victim_weights(&[700, 200], true), vec![0, 400]);
}

#[test]
fn the_weights_are_halved_until_they_come_to_500() {
    assert_eq!(victim_weights(&[400, 400], false), vec![200, 200]);
    assert_eq!(victim_weights(&[1000, 600], false), vec![250, 150]);
}

#[test]
fn the_roll_walks_the_weights() {
    let w = [3, 0, 5];
    assert_eq!(pick_victim(&w, 1), Some(0));
    assert_eq!(pick_victim(&w, 3), Some(0));
    assert_eq!(pick_victim(&w, 4), Some(2));
    assert_eq!(pick_victim(&w, 8), Some(2));
    assert_eq!(pick_victim(&[0, 0], 1), None);
}

#[test]
fn a_plague_ends_on_a_one_in_twenty_after_three_turns() {
    assert!(!flat_end(1, 3));
    assert!(flat_end(1, 4));
    assert!(!flat_end(2, 50));
}

#[test]
fn a_world_event_ends_with_a_chance_that_climbs_with_its_age() {
    assert!(!world_end(1, 3), "never before the fourth turn");
    assert!(world_end(2, 4), "d20 <= turns - 2");
    assert!(!world_end(3, 4));
    assert!(world_end(18, 20));
    assert!(!world_end(19, 20));
}

#[test]
fn the_pirates_take_thirty_to_fifty_percent_in_tens() {
    assert_eq!(pirates_take(1000, 1), 300);
    assert_eq!(pirates_take(1000, 21), 500);
    assert_eq!(pirates_take(333, 5), 110);
}

#[test]
fn a_donation_is_a_hundred_and_five_to_six_hundred_gold() {
    assert_eq!(donation(1), 105);
    assert_eq!(donation(100), 600);
}

#[test]
fn the_table_has_eighteen_events_and_the_good_ones_are_as_the_original_marks_them() {
    assert_eq!(EventKind::ALL.len(), 18);
    assert!(EventKind::ALL
        .iter()
        .enumerate()
        .all(|(i, k)| k.number() == i as u32 + 1));
    let bad: Vec<_> = EventKind::ALL.iter().filter(|k| !k.is_good()).collect();
    assert_eq!(bad.len(), 7, "{bad:?}");
    assert!(EventKind::ManaShort.is_world() && !EventKind::Plague.is_world());
    assert!(EventKind::Plague.lasts() && !EventKind::Piracy.lasts());
}

fn text(event: u32, voice: u32, text: &str) -> EventText {
    EventText {
        event,
        voice,
        text: text.into(),
    }
}

#[test]
fn a_message_fills_in_the_names_and_counts() {
    let texts = [text(
        1,
        0,
        "Hit {city} killing {people} townsfolk and {units} unit{units_s}, destroying {buildings} building{buildings_s}.",
    )];
    let mut n = EventNotice::new(EventKind::GreatMeteor, Voice::Started);
    n.city_name = "Rome".into();
    n.lost_population = 3;
    n.lost_units = 1;
    n.lost_buildings = 0;
    assert_eq!(
        compose(&texts, &n).unwrap(),
        "Hit Rome killing 3 thousand townsfolk and 1 unit, destroying no buildings."
    );
    n.lost_units = 4;
    n.lost_buildings = 1;
    assert_eq!(
        compose(&texts, &n).unwrap(),
        "Hit Rome killing 3 thousand townsfolk and 4 units, destroying 1 building."
    );
}

#[test]
fn the_message_is_the_one_of_the_events_voice() {
    let texts = [
        text(7, 0, "yours {size} of {city}"),
        text(7, 1, "over in {city}"),
        text(7, 4, "{player}'s"),
    ];
    let mut n = EventNotice::new(EventKind::Plague, Voice::Ended);
    n.city_name = "Oslo".into();
    assert_eq!(compose(&texts, &n).unwrap(), "over in Oslo");
    n.voice = Voice::Elsewhere;
    n.player_name = "Rjak".into();
    assert_eq!(compose(&texts, &n).unwrap(), "Rjak's");
    n.voice = Voice::Happened;
    assert_eq!(compose(&texts, &n), None);
}

#[test]
fn the_frequency_halves_or_doubles_the_pressure() {
    assert_eq!(scaled_pressure(40, 2, EventFrequency::Normal), 30);
    assert_eq!(scaled_pressure(40, 2, EventFrequency::Half), 15);
    assert_eq!(scaled_pressure(40, 2, EventFrequency::Double), 60);
    // About 1.5 percent a turn after ten quiet turns on Average, as the design says.
    assert_eq!(scaled_pressure(10, 2, EventFrequency::Normal), 7);
}
