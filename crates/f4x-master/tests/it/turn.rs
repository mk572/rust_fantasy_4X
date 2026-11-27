//! Whose turn comes when: every player has one of its own, the wizards first, then the
//! independent race players, then the monsters' player.

use std::cell::RefCell;
use std::rc::Rc;

use f4x_core::map::{Plane, Position};
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::world::save::ContentCheck;
use f4x_core::world::Event;
use f4x_master::{
    Controller, Entry, FlowError, Game, Master, Phase, SaveFile, Seat, Status, Step, Stopped, Until,
};

use crate::support::{content, games};

/// A controller that does what the test says in each turn.
struct Doer<F>(F);

impl<F: FnMut(&mut Game, PlayerId) -> Status> Controller for Doer<F> {
    fn play(&mut self, game: &mut Game, player: PlayerId, _: &mut Vec<Event>) -> Status {
        (self.0)(game, player)
    }
}

type Log = Rc<RefCell<Vec<PlayerId>>>;

/// A seat's controller that writes down whose turn it plays, and waits in the turn of
/// `wait_at`.
fn noting<'a>(log: &Log, wait_at: Option<PlayerId>) -> Box<dyn Controller + 'a> {
    let log = log.clone();
    Box::new(Doer(move |game: &mut Game, player| {
        assert_eq!(game.phase(), Some(Phase::Acting(player)));
        log.borrow_mut().push(player);
        if wait_at == Some(player) {
            Status::Waiting
        } else {
            Status::Done
        }
    }))
}

/// A seat for each wizard and one for the others.
fn table<'a>(game: &Game, log: &Log, wait_at: Option<PlayerId>) -> Vec<Seat<'a>> {
    let mut seats: Vec<Seat> = game
        .player_ids()
        .into_iter()
        .filter(|&p| game.is_wizard(p))
        .map(|p| Seat::new(p, noting(log, wait_at)))
        .collect();
    seats.push(Seat::non_wizards(noting(log, wait_at)));
    seats
}

/// A fresh game, with the race players that have something to play with.
fn game() -> (Game, Vec<PlayerId>) {
    let (_, game) = games().remove(0);
    let races: Vec<PlayerId> = race_players(&game)
        .into_iter()
        .filter(|&p| !game.is_dormant(p))
        .collect();
    assert!(races.len() > 1, "the game has too few race players");
    (game, races)
}

fn race_players(game: &Game) -> Vec<PlayerId> {
    game.player_ids()
        .into_iter()
        .filter(|&p| game.race_of_player(p).is_some())
        .collect()
}

/// A square a city can be founded on, with nothing on it.
fn free_site(game: &Game) -> Position {
    let grid = game.map().expect("a map").plane(Plane::Arcanus);
    let (width, height) = (grid.width(), grid.height());
    let plane = Plane::Arcanus;
    (0..height)
        .flat_map(|y| (0..width).map(move |x| Position { plane, x, y }))
        .find(|&at| {
            game.map().expect("a map").feature_entity(at).is_none()
                && game.economy().site_max_size(at) > 0
                && game.economy().city_at(at).is_none()
                && game.units().units_at(at).is_empty()
        })
        .expect("a free site")
}

/// The players that take a turn in `game` as it stands.
fn playing(game: &Game) -> Vec<PlayerId> {
    let list = game.turn_players().iter().copied();
    list.filter(|&p| game.takes_turn(p)).collect()
}

#[test]
fn the_wizards_play_first_then_each_race_player_by_number_then_the_monsters() {
    let (game, _) = game();
    let order = game.turn_players();
    let wizards = order.iter().take_while(|&&p| game.is_wizard(p)).count();
    assert!(wizards > 0);
    let races = race_players(&game);
    assert_eq!(
        races.len(),
        game.content().rules.races.iter().count(),
        "every race has its player"
    );
    assert_eq!(order[wizards..order.len() - 1], races[..]);
    assert!(races.windows(2).all(|w| w[0].0 < w[1].0));
    assert_eq!(order.last(), Some(&MONSTERS));
}

#[test]
fn every_player_has_a_turn_of_its_own_started_and_ended_before_the_next_begins() {
    let (mut game, _) = game();
    let log = Log::default();
    let order = playing(&game);
    game.start_recording();
    Master::new(table(&game, &log, None)).run(&mut game, Until::Rounds(1));
    assert_eq!(*log.borrow(), order);
    let steps: Vec<Step> = game
        .stop_recording()
        .expect("recorded")
        .entries
        .into_iter()
        .filter_map(|e| match e {
            Entry::Step(step) => Some(step),
            Entry::Command(_) => None,
        })
        .collect();
    let mut expected: Vec<Step> = order
        .iter()
        .flat_map(|&p| [Step::StartTurn(p), Step::EndTurn(p)])
        .collect();
    expected.extend([Step::EndRound, Step::StartRound]);
    assert_eq!(steps, expected);
}

#[test]
fn a_race_with_no_city_and_no_unit_is_skipped_and_keeps_what_it_has() {
    let (mut game, _) = game();
    let list = game.turn_players().to_vec();
    let idle = race_players(&game)
        .into_iter()
        .find(|&p| game.is_dormant(p))
        .expect("a race with nothing");
    let gold = game.player(idle).expect("a player").treasury().gold;
    let log = Log::default();
    Master::new(table(&game, &log, None)).run(&mut game, Until::Rounds(2));
    assert!(!log.borrow().contains(&idle));
    assert_eq!(
        game.player(idle).expect("still there").treasury().gold,
        gold
    );
    assert_eq!(game.turn_players(), list, "the list does not change");
}

#[test]
fn a_race_plays_in_its_place_once_it_has_a_city() {
    let (mut game, _) = game();
    // The last race with nothing.
    let late = race_players(&game)
        .into_iter()
        .rev()
        .find(|&p| game.is_dormant(p))
        .expect("a race with nothing");
    let race = game.race_of_player(late).expect("a race");
    let site = free_site(&game);
    let log = Log::default();
    let mut seats = table(&game, &log, None);
    let first = seats[0].player().expect("a wizard's seat");
    let heard = log.clone();
    seats[0] = Seat::new(
        first,
        Box::new(Doer(move |game: &mut Game, player| {
            if game.is_dormant(late) {
                game.found_neutral_city(site, race);
            }
            heard.borrow_mut().push(player);
            Status::Done
        })),
    );
    Master::new(seats).run(&mut game, Until::Rounds(1));
    assert_eq!(log.borrow().iter().filter(|&&p| p == late).count(), 1);
    assert_eq!(*log.borrow(), playing(&game));
}

#[test]
fn a_game_saved_in_a_race_players_turn_goes_on_with_that_turn() {
    let (mut game, races) = game();
    let order = playing(&game);
    let waiting = races[1];
    let log = Log::default();
    let mut master = Master::new(table(&game, &log, Some(waiting)));
    let progress = master.run(&mut game, Until::Rounds(1));
    assert_eq!(progress.stopped, Stopped::Waiting(waiting));
    assert_eq!(master.active_player(), Some(waiting));
    drop(master);

    let text = game.save_file(Vec::new()).to_text().expect("saves");
    let mut loaded = SaveFile::from_text(&text)
        .expect("reads")
        .restore(content(), ContentCheck::Strict)
        .expect("restores")
        .game;
    let log = Log::default();
    let mut master =
        Master::from_game(table(&loaded, &log, None), &loaded).expect("the table fits");
    assert_eq!(master.state().phase, Phase::Acting(waiting));
    let progress = master.run(&mut loaded, Until::Rounds(1));
    assert!(
        !progress
            .events
            .iter()
            .any(|e| matches!(e, Event::Rejected(_))),
        "{:?}",
        progress.events
    );
    // The turn that was waiting and those after it; none of those before it again.
    let at = order.iter().position(|&p| p == waiting).expect("in order");
    assert_eq!(log.borrow()[..], order[at..]);
}

#[test]
fn a_game_saved_in_a_race_players_turn_needs_a_seat_for_it() {
    let (mut game, races) = game();
    let log = Log::default();
    Master::new(table(&game, &log, Some(races[0]))).run(&mut game, Until::Waiting);
    let mut wizards_only = table(&game, &log, None);
    wizards_only.retain(|s| s.player().is_some());
    assert_eq!(
        Master::from_game(wizards_only, &game).err(),
        Some(FlowError::NoSeat { player: races[0] })
    );
}
