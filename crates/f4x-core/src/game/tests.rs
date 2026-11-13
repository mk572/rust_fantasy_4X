use super::*;

#[test]
fn end_turn_advances_turn() {
    let mut g = Game::new(1);
    assert_eq!(
        g.apply(Command::EndTurn),
        vec![Event::TurnStarted { turn: 2 }]
    );
}

#[test]
fn same_seed_same_rng_stream() {
    let (mut a, mut b) = (Game::new(42), Game::new(42));
    assert_eq!((a.next_u64(), a.next_u64()), (b.next_u64(), b.next_u64()));
}

#[derive(Debug, PartialEq)]
struct Mark(u32);

#[test]
fn put_gives_and_replaces_a_component_and_strip_takes_it_off() {
    let mut g = Game::new(1);
    let e = g.world.spawn(());
    g.put(e, Mark(1));
    g.put(e, Mark(2));
    assert_eq!(g.world.get::<&Mark>(e).map(|m| m.0), Ok(2));
    g.strip::<Mark>(e);
    assert!(g.world.get::<&Mark>(e).is_err());
    // A component that is not there is fine.
    g.strip::<Mark>(e);
    g.put_bundle(e, (Mark(3), 7u8));
    assert_eq!(g.world.get::<&u8>(e).map(|m| *m), Ok(7));
}

// The helpers exist so that an entity that is gone is noticed; a build without debug assertions
// goes on, as the ignored result did.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "no such entity to give a component to")]
fn put_on_an_entity_that_is_gone_stops_a_debug_build() {
    let mut g = Game::new(1);
    let e = g.world.spawn(());
    g.world.despawn(e).ok();
    g.put(e, Mark(1));
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "no such entity to take a component off")]
fn strip_on_an_entity_that_is_gone_stops_a_debug_build() {
    let mut g = Game::new(1);
    let e = g.world.spawn((Mark(1),));
    g.world.despawn(e).ok();
    g.strip::<Mark>(e);
}
