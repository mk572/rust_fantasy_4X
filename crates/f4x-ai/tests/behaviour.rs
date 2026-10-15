//! The scripted AI playing real content: what it does over a few dozen turns, and that it can
//! win with the Spell of Mastery (`docs/ai-design.md`). Slow, so ignored by default: run them with
//! `cargo test -p f4x-ai --test behaviour -- --ignored` when the AI changes.

use f4x_ai::{play_round, Ai};
use f4x_core::city::PlayerId;
use f4x_core::content::Content;
use f4x_core::endgame::EndKind;
use f4x_core::setup::{new_game, NewGame};
use f4x_core::Game;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn content() -> Content {
    f4x_data::load_content(std::path::Path::new("../../content")).expect("the rules data loads")
}

fn game(seed: u64) -> Game {
    new_game(
        &content(),
        &NewGame {
            seed,
            all_ai: true,
            ..NewGame::default()
        },
    )
    .expect("a new game")
}

fn play(game: &mut Game, turns: u32) {
    let mut ais = vec![Ai::new(P0), Ai::new(P1)];
    for _ in 0..turns {
        if game.game_over.is_some() {
            break;
        }
        play_round(game, &mut ais);
    }
}

#[test]
#[ignore = "plays whole games; run with --ignored when the AI changes"]
fn scouts_find_far_more_of_the_map_than_the_garrison_walks() {
    let mut g = game(1);
    play(&mut g, 60);
    let seen = g.explored_squares(P0);
    assert!(seen > 200, "explored only {seen} squares in 60 turns");
}

#[test]
#[ignore = "plays whole games; run with --ignored when the AI changes"]
fn cities_stop_making_settlers_once_there_are_enough_cities() {
    let mut g = game(1);
    play(&mut g, 120);
    let cities = g
        .economy()
        .cities()
        .iter()
        .filter(|(_, c, _)| c.owner == P0)
        .count();
    assert!(cities <= 10, "{cities} cities: settlers kept coming");
}

#[test]
#[ignore = "plays whole games; run with --ignored when the AI changes"]
fn a_wizard_that_knows_the_spell_of_mastery_casts_it_and_wins() {
    let mut g = game(1);
    let mastery = g
        .content()
        .rules
        .spells
        .id_of("spell_of_mastery")
        .expect("the spell exists");
    let p = g.player_mut(P0).unwrap();
    p.magic.known.insert(mastery);
    p.magic.skill_points = 5000;
    p.mana = 20_000;
    play(&mut g, 80);
    let over = g.game_over.expect("the game ended");
    assert_eq!(over.winner, P0);
    assert_eq!(over.kind, EndKind::SpellOfMastery);
}

#[test]
#[ignore = "plays whole games; run with --ignored when the AI changes"]
fn a_wizard_that_knows_earth_lore_casts_it_and_sees_far_more() {
    let explored_after = |lore: bool| {
        let mut g = game(1);
        if lore {
            let spell = g.content().rules.spells.id_of("earth_lore").unwrap();
            let p = g.player_mut(P0).unwrap();
            p.magic.known.insert(spell);
            p.magic.skill_points = 400;
            p.mana = 2000;
        }
        play(&mut g, 6);
        g.explored_squares(P0)
    };
    let (with, without) = (explored_after(true), explored_after(false));
    assert!(
        with > without + 60,
        "Earth Lore showed {with} squares against {without} without it"
    );
}

#[test]
#[ignore = "plays whole games; run with --ignored when the AI changes"]
fn a_wizard_alone_on_an_island_builds_a_ship_and_finds_the_world() {
    // Seed 2 puts the first wizard on a small island: walking shows it 55 squares in all.
    let mut g = game(2);
    play(&mut g, 110);
    let seen = g.explored_squares(P0);
    assert!(seen > 300, "explored only {seen} squares in 110 turns");
}
