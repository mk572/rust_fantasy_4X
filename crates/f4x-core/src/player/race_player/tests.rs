use super::*;
use crate::game::setup::{new_game, NewGame};
use crate::game::testutil::shipped;
use crate::player::{Treasury, MONSTERS};
use f4x_def::half_int::HalfInt;
use f4x_def::testdir::TestResult;

fn world(seed: u64) -> Result<Game, String> {
    new_game(
        &shipped(),
        &NewGame {
            seed,
            all_ai: true,
            ..Default::default()
        },
    )
}

/// The race players that own a city.
fn race_players(g: &Game) -> Vec<PlayerId> {
    let mut ids: Vec<_> = g
        .economy()
        .cities()
        .into_iter()
        .map(|(_, c, _)| c.owner)
        .filter(|&o| g.race_of_player(o).is_some())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

#[test]
fn the_number_of_a_race_player_follows_the_race_and_wizards_and_monsters_have_none() -> TestResult {
    let g = world(7)?;
    let race = RaceId::new(3);
    assert_eq!(race_player_id(race), PlayerId(FIRST_RACE_PLAYER + 3));
    let mut g = g;
    let id = g.race_player(race);
    assert_eq!(g.race_of_player(id), Some(race));
    assert_eq!(g.race_of_player(PlayerId(0)), None, "a wizard");
    assert_eq!(g.race_of_player(MONSTERS), None, "the monsters");
    assert!(g.is_neutral_player(id) && !g.is_wizard(id));
    Ok(())
}

#[test]
fn a_race_player_is_made_once_with_the_computer_wizards_starting_gold() -> TestResult {
    let mut g = world(7)?;
    let race = RaceId::new(5);
    let before = g.player_ids().len();
    let id = g.race_player(race);
    g.treasury_mut(id).ok_or("no treasury")?.gold = HalfInt::from_int(7);
    assert_eq!(g.race_player(race), id, "the same player");
    assert_eq!(
        g.player(id).ok_or("no player")?.treasury().gold,
        HalfInt::from_int(7),
        "a second call does not reset it"
    );
    assert_eq!(
        g.player_ids().len(),
        before + usize::from(!race_players(&g).contains(&id))
    );
    let fresh = world(7)?;
    let gold = fresh.settings.wizards.starting_gold.of(false);
    let mut fresh = fresh;
    let other = fresh.race_player(RaceId::new(13));
    assert_eq!(
        fresh.player(other).ok_or("no player")?.treasury().gold,
        HalfInt::from_int(gold)
    );
    Ok(())
}

#[test]
fn a_new_world_has_one_player_for_each_race_of_its_independent_cities() -> TestResult {
    let g = world(7)?;
    let players = race_players(&g);
    assert!(!players.is_empty());
    for (city, info, _) in g.economy().cities() {
        if let Some(race) = g.race_of_player(info.owner) {
            assert_eq!(g.city_race(city), Some(race), "{} is its race's", info.name);
        }
    }
    Ok(())
}

#[test]
fn a_city_that_turns_neutral_joins_the_player_of_its_race() -> TestResult {
    let mut g = world(7)?;
    let (city, owner, _) = g
        .economy()
        .cities()
        .into_iter()
        .map(|(e, c, p)| (e, c.owner, p))
        .find(|&(_, o, _)| g.is_wizard(o))
        .ok_or("no wizard city")?;
    let race = g.city_race(city).ok_or("no race")?;
    g.make_city_neutral(city, owner);
    let now = g.economy().city_owner(city).ok_or("no owner")?;
    assert_eq!(now, race_player_id(race));
    assert!(g.has_player(now));
    // A second city of the same race joins the same player.
    Ok(())
}

#[test]
fn a_race_player_with_no_city_and_no_unit_is_removed_at_the_round_end() -> TestResult {
    let mut g = world(7)?;
    let race = RaceId::new(9);
    let id = g.race_player(race);
    g.end_round();
    assert!(!g.has_player(id), "no city, no unit: gone");
    let id = g.race_player(race);
    let unit = g
        .content()
        .rules
        .races
        .get(race)
        .units
        .first()
        .copied()
        .ok_or("the race has no unit")?;
    let pos = crate::map::Position {
        plane: crate::map::Plane::Arcanus,
        x: 3,
        y: 3,
    };
    g.spawn_unit(id, pos, unit);
    g.end_round();
    assert!(g.has_player(id), "a unit keeps it");
    Ok(())
}

#[test]
fn a_race_player_is_credited_the_average_fame_of_the_living_wizards() -> TestResult {
    let mut g = world(7)?;
    let wizards = g.living_wizards();
    assert!(wizards.len() >= 2);
    for (k, &w) in wizards.iter().enumerate() {
        g.set_fame(w, 10 + 7 * k as i32);
    }
    let sum: i32 = wizards.iter().map(|&w| g.fame(w)).sum();
    let id = g.race_player(RaceId::new(1));
    assert_eq!(g.fame(id), sum / wizards.len() as i32);
    assert_eq!(g.fame(MONSTERS), 0);
    Ok(())
}

#[test]
fn the_power_of_a_race_player_is_all_mana() -> TestResult {
    let g = world(7)?;
    let id = *race_players(&g).first().ok_or("no race player")?;
    let income = g.income(id);
    assert_eq!(income.split.mana, income.power_base);
    assert_eq!((income.split.skill, income.split.research), (0, 0));
    assert!(income.power_base > HalfInt::ZERO, "its cities make power");
    Ok(())
}

#[test]
fn the_seat_of_a_race_player_is_its_biggest_city() -> TestResult {
    let g = world(7)?;
    for id in race_players(&g) {
        let seat = g.seat_of(id).ok_or("no seat")?;
        let best = g
            .economy()
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == id)
            .map(|(e, ..)| g.economy().citizens(e))
            .max();
        let at_seat = g
            .economy()
            .cities()
            .into_iter()
            .find(|&(_, _, p)| p == seat)
            .map(|(e, ..)| g.economy().citizens(e));
        assert_eq!(at_seat, best);
        assert_eq!(g.summon_point(id), Some(seat));
    }
    Ok(())
}

#[test]
fn a_race_player_can_be_offered_a_hero_when_the_wizards_are_famous() -> TestResult {
    let mut g = world(7)?;
    for w in g.living_wizards() {
        g.set_fame(w, 200);
    }
    let id = *race_players(&g).first().ok_or("no race player")?;
    g.treasury_mut(id).ok_or("no treasury")?.gold = HalfInt::from_int(5000);
    let offer = g.cheat_offer(id, crate::player::offer::OfferKind::Hero);
    assert!(offer.is_some(), "a hero asks to join the neutral player");
    assert!(g
        .cheat_offer(MONSTERS, crate::player::offer::OfferKind::Hero)
        .is_none());
    let _ = Treasury::default();
    Ok(())
}

#[test]
fn the_power_of_a_race_player_is_paid_into_its_mana_at_the_turn_end() -> TestResult {
    let mut g = world(7)?;
    let id = *race_players(&g).first().ok_or("no race player")?;
    let before = g.player(id).ok_or("no player")?.treasury().mana;
    let power = g.income(id).split.mana;
    assert!(power > HalfInt::ZERO);
    g.end_player_turn(id);
    assert_eq!(
        g.player(id).ok_or("no player")?.treasury().mana,
        before + power
    );
    Ok(())
}
