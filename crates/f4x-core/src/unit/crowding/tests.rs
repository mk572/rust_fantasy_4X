use super::*;
use crate::game::testutil::{num, shipped};
use crate::game::{Command, Event};
use crate::map::{Grid, Map, Plane};
use f4x_def::half_int::HalfInt;
use f4x_def::testdir::TestResult;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

/// A grassland world with two players, the first the human wizard.
fn world() -> Game {
    let c = shipped();
    let grass = c
        .terrains
        .id_numbered(num::terrain::GRASSLAND)
        .expect("grassland");
    let mut game = Game::new(1);
    game.content = c;
    game.map = Some(Map::new(Grid::new(12, 6, grass), Grid::new(12, 6, grass)));
    for id in [P0, P1] {
        game.add_player(id, "");
    }
    game.set_human(Some(P0));
    game
}

fn spearman(game: &mut Game, owner: PlayerId, pos: Position) -> hecs::Entity {
    let id = game
        .content
        .rules
        .units
        .id_numbered(104 /* high_men_spearmen */)
        .expect("a unit");
    game.spawn_unit(owner, pos, id)
}

#[test]
fn only_the_human_wizard_may_overfill_a_square() {
    let g = world();
    assert!(g.crowds_allowed(P0));
    assert!(!g.crowds_allowed(P1));
}

#[test]
fn a_square_is_crowded_once_it_holds_more_than_the_limit() {
    let mut g = world();
    let limit = g.max_tile();
    for _ in 0..limit {
        spearman(&mut g, P0, at(5, 3));
    }
    assert!(
        g.crowded_squares(P0).is_empty(),
        "a full square is not crowded"
    );
    spearman(&mut g, P0, at(5, 3));
    assert_eq!(g.crowded_squares(P0), vec![at(5, 3)]);
    assert!(g.crowded_squares(P1).is_empty());
}

#[test]
fn a_unit_may_step_to_every_free_neighbour_and_not_to_a_full_one_or_without_moves() {
    let mut g = world();
    let u = spearman(&mut g, P0, at(5, 3));
    assert_eq!(g.crowd_moves(u).len(), 8);
    for _ in 0..g.max_tile() {
        spearman(&mut g, P0, at(6, 3));
    }
    let moves = g.crowd_moves(u);
    assert_eq!(moves.len(), 7);
    assert!(!moves.contains(&at(6, 3)));
    let _ = g.world.insert_one(u, crate::unit::MovesLeft(HalfInt::ZERO));
    assert!(g.crowd_moves(u).is_empty());
}

#[test]
fn moving_or_disbanding_a_unit_ends_the_crowd() -> TestResult {
    let mut g = world();
    let limit = g.max_tile();
    let units: Vec<_> = (0..=limit)
        .map(|_| spearman(&mut g, P0, at(5, 3)))
        .collect();
    let to = *g
        .crowd_moves(units[0])
        .first()
        .ok_or("no square to go to")?;
    let events = g.apply(Command::MoveStack {
        units: vec![units[0]],
        to,
    });
    assert!(
        !events.iter().any(|e| matches!(e, Event::Rejected(_))),
        "{events:?}"
    );
    assert!(g.crowded_squares(P0).is_empty());
    // Again, and disbanding.
    spearman(&mut g, P0, at(5, 3));
    assert_eq!(g.crowded_squares(P0), vec![at(5, 3)]);
    let events = g.apply(Command::DismissUnit { unit: units[1] });
    assert!(
        !events.iter().any(|e| matches!(e, Event::Rejected(_))),
        "{events:?}"
    );
    assert!(g.crowded_squares(P0).is_empty());
    Ok(())
}
