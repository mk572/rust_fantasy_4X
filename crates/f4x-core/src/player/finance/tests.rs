use super::*;

fn ids(n: usize) -> Vec<hecs::Entity> {
    let mut world = hecs::World::new();
    (0..n).map(|_| world.spawn(())).collect()
}

fn troop(id: hecs::Entity, food: i32, gold: i32) -> Troop {
    Troop {
        id,
        food,
        gold,
        noble_hero: false,
    }
}

fn city(gold: i32, upkeep: i32, food: i32, figures: i32) -> CityYield {
    CityYield {
        gold,
        gold_upkeep: upkeep,
        food,
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
    assert_eq!(t.food_surplus, 8);
    assert_eq!(t.food_upkeep, 2);
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
fn an_army_that_eats_too_much_loses_its_newest_units() {
    let e = ids(3);
    let troops = [troop(e[0], 1, 0), troop(e[1], 1, 0), troop(e[2], 1, 0)];
    let t = Budget {
        food_surplus: 1,
        food_upkeep: 3,
        ..Budget::default()
    };
    let s = settle(&t, 0, 0, &troops);
    assert_eq!(s.disbanded, vec![e[2], e[1]]);
}

#[test]
fn units_go_newest_first_until_the_gold_covers_the_rest() {
    let e = ids(3);
    let troops = [troop(e[0], 0, 4), troop(e[1], 0, 4), troop(e[2], 0, 4)];
    let t = Budget {
        city_gold: 5,
        unit_upkeep: 12,
        ..Budget::default()
    };
    // 3 + 5 = 8 gold: two units (8) are affordable, the newest goes.
    let s = settle(&t, 3, 0, &troops);
    assert_eq!(s.disbanded, vec![e[2]]);
    assert_eq!(s.gold, 0);
    // Fame pays for 4 of the 12, so all three can stay: 8 gold covers the 8 left.
    let s = settle(&t, 3, 4, &troops);
    assert!(s.disbanded.is_empty());
    assert_eq!(s.gold, 0);
    let s = settle(&t, 10, 4, &troops);
    assert_eq!(s.gold, 15 - 8);
}

#[test]
fn units_that_cost_nothing_are_never_let_go() {
    let e = ids(2);
    let troops = [troop(e[0], 0, 0), troop(e[1], 0, 5)];
    let t = Budget::default();
    let s = settle(&t, 0, 0, &troops);
    assert_eq!(s.disbanded, vec![e[1]]);
}
