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
