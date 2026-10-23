use super::*;

fn pop(figures: u32, tens: u32) -> Population {
    Population { figures, tens }
}

#[test]
fn tens_turn_into_citizens_up_to_the_cap() {
    assert_eq!(grow_population(pop(3, 90), 20, 25), pop(4, 10));
    assert_eq!(grow_population(pop(3, 90), 250, 25), pop(6, 40));
    assert_eq!(
        grow_population(pop(25, 90), 50, 25),
        pop(25, 99),
        "no growth past the cap"
    );
}

#[test]
fn a_shrinking_city_loses_citizens_but_keeps_one() {
    assert_eq!(grow_population(pop(3, 10), -30, 25), pop(2, 80));
    assert_eq!(grow_population(pop(1, 10), -30, 25), pop(1, 0));
}

#[test]
fn curses_raise_the_chance_an_outpost_shrinks() {
    assert_eq!(outpost_shrink_chance(|_| false), 5);
    assert_eq!(
        outpost_shrink_chance(|n| n == "famine" || n == "evil_presence"),
        20
    );
}

#[test]
fn an_outpost_that_reaches_ten_tens_becomes_a_city_and_keeps_them() {
    let mut rng = GameRng::new(1);
    // Certain growth (chance 100), no shrinking (chance 0).
    let mut p = pop(0, 8);
    for _ in 0..4 {
        p = outpost_step(p, 100, 0, &mut rng);
        if p.figures > 0 {
            break;
        }
    }
    assert_eq!(p.figures, 1);
    assert!(p.tens >= OUTPOST_CITY_TENS);
}

#[test]
fn an_outpost_step_draws_in_a_fixed_order() {
    let a = outpost_step(pop(0, 10), 50, 50, &mut GameRng::new(7));
    let b = outpost_step(pop(0, 10), 50, 50, &mut GameRng::new(7));
    assert_eq!(a, b);
}

#[test]
fn a_new_outpost_starts_with_three_tens_and_an_outpost_with_none_is_lost() {
    let mut g = Game::new(1);
    g.add_player(PlayerId(0), "W");
    let at = crate::map::Position {
        plane: crate::map::Plane::Arcanus,
        x: 20,
        y: 20,
    };
    let city = g.spawn_city(PlayerId(0), at, "O".into(), None, 0);
    assert_eq!(g.city_population(city), pop(0, OUTPOST_START_TENS));
    g.world.insert_one(city, pop(0, 0)).unwrap();
    assert_eq!(g.grow_outpost(city), OutpostTurn::Lost);
}
