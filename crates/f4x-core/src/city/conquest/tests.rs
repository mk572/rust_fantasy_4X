use super::*;

#[test]
fn a_city_is_a_hamlet_up_to_four_citizens_and_a_metropolis_from_seventeen() {
    assert_eq!(city_size_class(0), 0);
    assert_eq!(city_size_class(1), 1);
    assert_eq!(city_size_class(4), 1);
    assert_eq!(city_size_class(5), 2);
    assert_eq!(city_size_class(16), 4);
    assert_eq!(city_size_class(17), 5);
    assert_eq!(city_size_class(40), 5);
}

#[test]
fn the_loser_loses_the_size_and_the_winner_gains_the_size_less_two() {
    assert_eq!(fame_change(1), (1, 0));
    assert_eq!(fame_change(2), (2, 0));
    assert_eq!(fame_change(5), (5, 3));
}

#[test]
fn the_loot_is_the_citys_share_of_the_losers_gold() {
    assert_eq!(loot_share(200, 5, 10), 100);
    assert_eq!(loot_share(200, 10, 10), 200, "an only city loses it all");
    assert_eq!(loot_share(201, 1, 3), 67);
    assert_eq!(loot_share(-50, 5, 10), 0, "debts are not looted");
    assert_eq!(loot_share(200, 0, 0), 0);
}

#[test]
fn razing_pays_a_tenth_of_each_building_and_costs_the_size_plus_one() {
    assert_eq!(razing_loot([100, 45, 9]), 10 + 4);
    assert_eq!(razing_loot([]), 0);
    assert_eq!(razing_fame_loss(3), 4);
}

fn b(n: u16) -> BuildingId {
    BuildingId::new(n)
}

#[test]
fn the_toll_spares_one_citizen_and_what_other_buildings_need() {
    let mut rng = GameRng::new(11);
    // Every roll succeeds at the top chances (damage 100: 50 and 75 percent) only some of the
    // time, so count against a twin generator.
    let mut twin = GameRng::new(11);
    let built = [b(0), b(1), b(2)];
    let toll = fighting_toll(5, 100, &built, &mut rng, |building| building != b(1));
    let lost = (0..4).filter(|_| twin.roll(100) <= 50).count() as u32;
    let wrecked = built
        .iter()
        .filter(|&&x| twin.roll(100) <= 75 && x != b(1))
        .count() as u32;
    assert_eq!(toll.citizens_lost, lost);
    assert_eq!(toll.buildings_lost, wrecked);
    assert!(toll.citizens_lost <= 4, "one citizen always lives");
}

#[test]
fn a_city_with_no_damage_loses_one_citizen_in_ten_at_most() {
    let mut rng = GameRng::new(3);
    let mut lost = 0;
    for _ in 0..500 {
        lost += fighting_toll(2, 0, &[], &mut rng, |_| true).citizens_lost;
    }
    assert!(
        (20..=90).contains(&lost),
        "about ten percent of 500: {lost}"
    );
}
