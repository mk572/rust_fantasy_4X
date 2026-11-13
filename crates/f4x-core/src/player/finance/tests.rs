use super::*;
use f4x_def::half_int::HalfInt;

fn ids(n: usize) -> Vec<hecs::Entity> {
    let mut world = hecs::World::new();
    (0..n).map(|_| world.spawn(())).collect()
}

fn troop(id: hecs::Entity, food: i32, gold: i32) -> Troop {
    Troop {
        id,
        food: HalfInt::from_int(food), // whole food in the call
        gold: HalfInt::from_int(gold),
        noble_hero: false,
        cost: 0,
        last: false,
    }
}

fn city(gold: i32, upkeep: i32, food: i32, figures: i32) -> CityYield {
    CityYield {
        gold: HalfInt::from_int(gold),
        gold_upkeep: HalfInt::from_int(upkeep),
        food: HalfInt::from_int(food),
        figures,
    }
}

#[test]
fn the_treasury_adds_cities_and_sells_spare_food() {
    let e = ids(3);
    let mut hero = troop(e[2], 0, 3);
    hero.noble_hero = true;
    let troops = [troop(e[0], 1, 2), troop(e[1], 1, 2), hero];
    // Cities make 9 food for 5 citizens and 7 for 3: 4 + 4 spare, 4 gold if no army ate.
    let t = Budget::compute(&[city(10, 2, 9, 5), city(6, 1, 7, 3)], &troops, 1);
    assert_eq!(t.city_gold, 16);
    assert_eq!(t.building_upkeep, 3);
    assert_eq!(t.noble_gold, 10);
    assert_eq!(t.food_surplus, HalfInt::from_int(8));
    assert_eq!(t.food_upkeep, HalfInt::from_int(2));
    // 4 gold from 8 spare food, less half the army's food (1).
    assert_eq!(t.food_gold, 3);
    assert_eq!(t.unit_upkeep, 6, "7 gold of troops, 1 paid by fame");
    assert_eq!(t.net_gold(), 16 - 3 + 10 + 3 - 6);
}

#[test]
fn fame_cannot_make_the_upkeep_negative() {
    let e = ids(1);
    let t = Budget::compute(&[], &[troop(e[0], 0, 2)], 30);
    assert_eq!(t.unit_upkeep, 0);
}

#[test]
fn an_army_that_eats_too_much_loses_its_newest_units_when_all_are_alike() {
    let e = ids(3);
    let troops = [troop(e[0], 1, 0), troop(e[1], 1, 0), troop(e[2], 1, 0)];
    let t = Budget {
        food_surplus: HalfInt::from_int(1),
        food_upkeep: HalfInt::from_int(3),
        ..Budget::default()
    };
    let s = settle(&t, f4x_def::half_int::HalfInt::from_int(0), 0, &troops);
    assert_eq!(s.disbanded, vec![e[2], e[1]]);
}

#[test]
fn units_go_newest_first_until_the_gold_covers_the_rest() {
    let e = ids(3);
    let troops = [troop(e[0], 0, 4), troop(e[1], 0, 4), troop(e[2], 0, 4)];
    let t = Budget {
        city_gold: f4x_def::half_int::HalfInt::from_int(5),
        unit_upkeep: f4x_def::half_int::HalfInt::from_int(12),
        ..Budget::default()
    };
    // 3 + 5 = 8 gold: two units (8) are affordable, the newest goes.
    let s = settle(&t, f4x_def::half_int::HalfInt::from_int(3), 0, &troops);
    assert_eq!(s.disbanded, vec![e[2]]);
    assert_eq!(s.gold, 0);
    // Fame pays for 4 of the 12, so all three can stay: 8 gold covers the 8 left.
    let s = settle(&t, f4x_def::half_int::HalfInt::from_int(3), 4, &troops);
    assert!(s.disbanded.is_empty());
    assert_eq!(s.gold, 0);
    let s = settle(&t, f4x_def::half_int::HalfInt::from_int(10), 4, &troops);
    assert_eq!(s.gold, 15 - 8);
}

#[test]
fn units_that_cost_nothing_are_never_let_go() {
    let e = ids(2);
    let troops = [troop(e[0], 0, 0), troop(e[1], 0, 5)];
    let t = Budget::default();
    let s = settle(&t, f4x_def::half_int::HalfInt::from_int(0), 0, &troops);
    assert_eq!(s.disbanded, vec![e[1]]);
}

#[test]
fn halves_of_food_are_added_exactly() {
    let e = ids(3);
    let half = |id| Troop {
        id,
        food: HalfInt::from_raw(1),
        gold: f4x_def::half_int::HalfInt::from_int(0),
        noble_hero: false,
        cost: 0,
        last: false,
    };
    let one = Budget::compute(&[], &[half(e[0])], 0);
    let two = Budget::compute(&[], &[half(e[0]), half(e[1])], 0);
    let three = Budget::compute(&[], &[half(e[0]), half(e[1]), half(e[2])], 0);
    assert_eq!(
        (one.food_upkeep, two.food_upkeep, three.food_upkeep),
        (
            HalfInt::from_raw(1),
            HalfInt::from_int(1),
            HalfInt::from_raw(3)
        )
    );
}

#[test]
fn the_highest_upkeep_goes_first_then_the_cheapest_to_make_then_the_newest() {
    let e = ids(4);
    let mut troops = [
        troop(e[0], 0, 2),
        troop(e[1], 0, 5),
        troop(e[2], 0, 5),
        troop(e[3], 0, 2),
    ];
    troops[1].cost = 90;
    troops[2].cost = 40;
    let order: Vec<_> = release_order(&troops, |u| u.gold)
        .into_iter()
        .map(|i| troops[i].id)
        .collect();
    // Upkeep 5 first (the cheaper to make, e[2], before e[1]), then upkeep 2, newest first.
    assert_eq!(order, vec![e[2], e[1], e[3], e[0]]);
}

#[test]
fn heroes_and_loaded_ships_are_let_go_last() {
    let e = ids(3);
    let mut troops = [troop(e[0], 0, 9), troop(e[1], 0, 2), troop(e[2], 0, 3)];
    troops[0].last = true;
    let t = Budget {
        unit_upkeep: f4x_def::half_int::HalfInt::from_int(14),
        ..Budget::default()
    };
    // 10 gold does not cover 14: the highest upkeep that is not last (e[2], then e[1]) goes first.
    let s = settle(&t, f4x_def::half_int::HalfInt::from_int(10), 0, &troops);
    assert_eq!(s.disbanded, vec![e[2], e[1]]);
}

#[test]
fn a_half_food_in_a_city_or_an_army_is_kept() {
    let e = ids(1);
    let half = Troop {
        food: HalfInt::from_raw(1),
        ..troop(e[0], 0, 0)
    };
    // A city makes 4.5 food for 3 citizens: 1.5 spare; the army eats a half.
    let mut city = city(0, 0, 0, 3);
    city.food = HalfInt::from_raw(9);
    let t = Budget::compute(&[city], &[half], 0);
    assert_eq!(t.food_surplus, HalfInt::from_raw(3));
    assert_eq!(t.food_upkeep, HalfInt::from_raw(1));
    assert_eq!(t.food_surplus - t.food_upkeep, HalfInt::from_int(1));
}

#[test]
fn spare_food_is_sold_from_the_empires_total_not_a_city_at_a_time() {
    let e = ids(1);
    // Two cities each with 1.5 food spare, an army that eats a half: 2.5 spare, 1 gold.
    let mut a = city(0, 0, 0, 3);
    a.food = HalfInt::from_raw(9);
    let mut b = a;
    b.food = HalfInt::from_raw(9);
    let half = Troop {
        food: HalfInt::from_raw(1),
        ..troop(e[0], 0, 0)
    };
    let t = Budget::compute(&[a, b], &[half], 0);
    assert_eq!(t.food_gold, 1);
}

#[test]
fn half_a_gold_of_tax_or_upkeep_is_kept_and_paid() {
    let cities = [CityYield {
        gold: HalfInt::from_raw(9),        // 4.5
        gold_upkeep: HalfInt::from_raw(3), // 1.5
        food: HalfInt::ZERO,
        figures: 0,
    }];
    let troops = [Troop {
        gold: HalfInt::from_raw(1), // 0.5
        ..troop(hecs::Entity::DANGLING, 0, 0)
    }];
    let t = Budget::compute(&cities, &troops, 0);
    assert_eq!(t.net_gold(), HalfInt::from_raw(5)); // 4.5 - 1.5 - 0.5
    let settled = settle(&t, HalfInt::from_raw(1), 0, &troops);
    assert_eq!(settled.gold, HalfInt::from_int(3)); // 0.5 + 4.5 - 1.5 - 0.5
}

#[test]
fn the_kept_budget_follows_a_completed_item_as_the_budget_does() -> f4x_def::testdir::TestResult {
    use crate::game::setup::{new_game, NewGame};
    let content = crate::game::testutil::shipped();
    let mut game = new_game(
        &content,
        &NewGame {
            seed: 7,
            all_ai: true,
            ..NewGame::with_wizards(2)
        },
    )?;
    let player = game
        .player_ids()
        .into_iter()
        .find(|&p| game.is_wizard(p))
        .ok_or("no wizard")?;
    let (city, at) = *game
        .economy()
        .cities_of(player)
        .first()
        .ok_or("the wizard has no city")?;
    assert_eq!(game.kept_budget(player, city), game.budget(player));
    // A unit is made in the city and a building stands in it: the asking after the flag agrees.
    let unit = game
        .units()
        .player_units(player)
        .first()
        .and_then(|&u| game.units().unit_def(u))
        .map(|d| d.num)
        .ok_or("the wizard has no unit")?;
    let unit = game
        .content()
        .rules
        .units
        .id_numbered(unit)
        .ok_or("no such unit")?;
    game.spawn_unit(player, at, unit);
    game.spawn_unit(player, at, unit);
    game.mark_budget_dirty(player, city);
    assert_eq!(game.kept_budget(player, city), game.budget(player));
    // Another player's figures replace the kept ones.
    let other = game
        .player_ids()
        .into_iter()
        .find(|&p| p != player && game.is_wizard(p))
        .ok_or("no second wizard")?;
    let (other_city, _) = *game
        .economy()
        .cities_of(other)
        .first()
        .ok_or("no city for the second wizard")?;
    assert_eq!(game.kept_budget(other, other_city), game.budget(other));
    assert_eq!(game.kept_budget(player, city), game.budget(player));
    Ok(())
}

/// The halving search for a city's smallest feeding number of farmers finds what the linear
/// search found, in every city of new games of several seeds, at each population from one to the
/// largest a city grows to.
#[test]
fn the_minimum_farmers_by_halving_are_those_of_the_linear_search() -> f4x_def::testdir::TestResult {
    use crate::city::economy::Population;
    use crate::game::setup::{new_game, NewGame};
    let content = crate::game::testutil::shipped();
    let mut checked = 0;
    for seed in [3, 7, 11] {
        let mut game = new_game(
            &content,
            &NewGame {
                seed,
                all_ai: true,
                ..NewGame::with_wizards(3)
            },
        )?;
        let cities: Vec<_> = game
            .economy()
            .cities()
            .into_iter()
            .map(|(e, ..)| e)
            .collect();
        let largest = content.rules.config.max_city_size;
        for city in cities {
            for figures in 0..=largest {
                game.world
                    .insert_one(city, Population { figures, tens: 0 })?;
                assert_eq!(
                    game.economy().city_min_farmers(city),
                    game.economy().city_min_farmers_linear(city),
                    "seed {seed}, a city of {figures}"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "{checked}");
    Ok(())
}

/// `city_places` is `cities()` without the names, with the filter before the sort: the same cities
/// in the same order, for every owner, in new games of several seeds.
#[test]
fn the_city_places_are_the_cities_without_their_names() -> f4x_def::testdir::TestResult {
    use crate::game::setup::{new_game, NewGame};
    let content = crate::game::testutil::shipped();
    let mut checked = 0;
    for seed in [3, 7, 11] {
        let game = new_game(
            &content,
            &NewGame {
                seed,
                all_ai: true,
                ..NewGame::with_wizards(4)
            },
        )?;
        for me in game.player_ids() {
            let old: Vec<_> = game
                .economy()
                .cities()
                .into_iter()
                .filter(|(_, c, p)| c.owner != me && p.x % 2 == 0)
                .map(|(e, c, p)| (e, c.owner, p))
                .collect();
            let new = game
                .economy()
                .city_places(|owner, p| owner != me && p.x % 2 == 0);
            assert_eq!(old, new, "seed {seed}, player {me:?}");
            checked += new.len();
        }
    }
    assert!(checked > 20, "{checked}");
    Ok(())
}
