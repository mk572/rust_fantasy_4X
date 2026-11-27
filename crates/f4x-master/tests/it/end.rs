//! The end of a game: who wins, when it is told, and what a finished game still takes.

use f4x_core::player::PlayerId;
use f4x_core::world::endgame::EndKind;
use f4x_core::world::{Command, Event, Rejection};
use f4x_master::{Game, GameOver};
use f4x_newgame::setup::{new_game, NewGame};

use crate::support::{content, text};

/// A new game of three wizards, the first a person's when `human`.
fn three(human: bool) -> (Game, Vec<PlayerId>) {
    let spec = NewGame {
        all_ai: !human,
        ..NewGame::with_wizards(3)
    };
    let game = Game::from(new_game(&content(), &spec).expect("a new game"));
    let wizards: Vec<PlayerId> = game
        .player_ids()
        .into_iter()
        .filter(|&p| game.is_wizard(p))
        .collect();
    assert_eq!(wizards.len(), 3);
    (game, wizards)
}

fn told_over(events: &[Event]) -> Vec<(PlayerId, EndKind)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::GameOver { winner, kind, .. } => Some((*winner, *kind)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_game_goes_on_while_two_wizards_stand_and_the_last_one_wins_by_conquest() {
    let (mut game, w) = three(false);
    game.cheat_defeat_player(w[1], w[0]);
    assert_eq!(game.check_end(), Vec::new(), "over while two stand");
    assert_eq!(game.game_over(), None);
    game.cheat_defeat_player(w[2], w[0]);
    let events = game.check_end();
    assert_eq!(told_over(&events), [(w[0], EndKind::Conquest)]);
    assert_eq!(
        game.game_over(),
        Some(GameOver {
            winner: w[0],
            kind: EndKind::Conquest,
            turn: game.round(),
        })
    );
}

#[test]
fn the_fall_of_the_human_ends_the_game_though_rivals_still_stand() {
    let (mut game, w) = three(true);
    let human = game.human().expect("a person plays");
    let victor = *w.iter().find(|&&p| p != human).expect("a rival");
    game.cheat_defeat_player(human, victor);
    assert_eq!(told_over(&game.check_end()), [(victor, EndKind::Conquest)]);
}

#[test]
fn the_spell_of_mastery_wins_and_the_first_to_complete_it_keeps_the_win() {
    let (mut game, w) = three(false);
    game.world_mut().complete_mastery(w[2]);
    game.world_mut().complete_mastery(w[1]);
    assert_eq!(
        told_over(&game.check_end()),
        [(w[2], EndKind::SpellOfMastery)]
    );
}

#[test]
fn a_human_who_has_fallen_has_lost_even_if_the_mastery_is_complete_too() {
    let (mut game, w) = three(true);
    let human = game.human().expect("a person plays");
    let rivals: Vec<_> = w.iter().copied().filter(|&p| p != human).collect();
    game.world_mut().complete_mastery(rivals[1]);
    game.cheat_defeat_player(human, rivals[0]);
    assert_eq!(
        told_over(&game.check_end()),
        [(rivals[0], EndKind::Conquest)]
    );
}

#[test]
fn an_end_is_told_once_and_is_not_changed_by_what_happens_afterwards() {
    let (mut game, w) = three(false);
    game.set_undo(true);
    game.apply(Command::SetTaxRate {
        player: w[0],
        rate: 1,
    });
    assert!(game.can_undo());
    game.world_mut().complete_mastery(w[0]);
    assert_eq!(told_over(&game.check_end()).len(), 1);
    assert!(!game.can_undo(), "the end was taken back");
    let over = game.game_over();
    game.cheat_defeat_player(w[0], w[1]);
    game.cheat_defeat_player(w[2], w[1]);
    assert_eq!(game.check_end(), Vec::new());
    assert_eq!(game.game_over(), over);
}

#[test]
fn a_game_that_is_over_takes_no_commands_and_no_turns() {
    let (mut game, w) = three(false);
    game.world_mut().complete_mastery(w[0]);
    game.check_end();
    let before = (text(game.world()), game.round());
    for &player in &w {
        for command in [
            Command::SetTaxRate { player, rate: 1 },
            Command::BalanceFarmers { player },
        ] {
            assert_eq!(game.apply(command), [Event::Rejected(Rejection::GameOver)]);
        }
        assert_eq!(
            game.start_player_turn(player),
            [Event::Rejected(Rejection::GameOver)]
        );
    }
    assert_eq!(game.play_round(), [Event::Rejected(Rejection::GameOver)]);
    assert_eq!((text(game.world()), game.round()), before);
}
