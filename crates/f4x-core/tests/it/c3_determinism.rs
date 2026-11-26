//! Class 3: the same game and the same commands give the same events and the same game, however
//! and wherever they are run. Two games made from one text hold their `HashMap`s with different
//! random hashers, so a result that depends on the order of a hash-ordered walk differs between them.

use crate::support::{busy_round, content, shape, text, worlds};
use f4x_core::world::save::Snapshot;
use f4x_core::world::World;

fn copy(w: &World) -> World {
    Snapshot::from_text(&text(w))
        .expect("reads")
        .restore(content())
        .expect("restores")
}

/// The events of each round and the text of the game after it.
fn play(mut w: World, rounds: usize) -> Vec<(String, String)> {
    (0..rounds)
        .map(|_| {
            let events = shape(&busy_round(&mut w, 6));
            (events, text(&w))
        })
        .collect()
}

#[test]
fn two_copies_of_a_game_play_alike() {
    for (name, w) in worlds() {
        let (a, b) = (play(copy(&w), 25), play(copy(&w), 25));
        for (round, (x, y)) in a.iter().zip(&b).enumerate() {
            assert_eq!(x.0, y.0, "{name}, round {round}: the events differ");
            assert_eq!(x.1, y.1, "{name}, round {round}: the games differ");
        }
    }
}

#[test]
fn a_game_played_in_turns_with_another_plays_as_it_does_alone() {
    let mut worlds = worlds();
    let (name_a, a) = worlds.remove(0);
    let (name_b, b) = worlds.remove(0);
    let (alone_a, alone_b) = (play(copy(&a), 15), play(copy(&b), 15));
    let (mut a, mut b) = (copy(&a), copy(&b));
    for round in 0..15 {
        let ea = shape(&busy_round(&mut a, 6));
        let eb = shape(&busy_round(&mut b, 6));
        assert_eq!(
            (&ea, text(&a)),
            (&alone_a[round].0, alone_a[round].1.clone()),
            "{name_a}, round {round}"
        );
        assert_eq!(
            (&eb, text(&b)),
            (&alone_b[round].0, alone_b[round].1.clone()),
            "{name_b}, round {round}"
        );
    }
}

#[test]
fn a_game_plays_the_same_on_another_thread() {
    for (name, w) in worlds() {
        let here = play(copy(&w), 15);
        let start = text(&w);
        let there = std::thread::spawn(move || {
            let w = Snapshot::from_text(&start)
                .expect("reads")
                .restore(content())
                .expect("restores");
            play(w, 15)
        })
        .join()
        .expect("the thread played");
        assert_eq!(here, there, "{name}");
    }
}
