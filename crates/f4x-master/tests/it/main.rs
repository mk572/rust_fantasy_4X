//! The master on real games: the order of the seats, human controllers that wait, command
//! validation, defeated seats and resuming from a saved state. Replays and undo are in the files
//! of their names.

mod convert;
mod end;
mod replay;
mod save;
mod step;
mod support;
mod turn;
mod undo;

use std::cell::RefCell;
use std::rc::Rc;

use f4x_core::player::PlayerId;
use f4x_core::world::save::ContentCheck;
use f4x_core::world::{Command, Event, World};
use f4x_master::{
    turn_players, Controller, Game, Master, Phase, Refused, SaveFile, Seat, Status, Stopped, Until,
};
use f4x_newgame::setup::{new_game, NewGame};

type Log = Rc<RefCell<Vec<(u32, PlayerId)>>>;

/// Writes down (round, first player) each time it plays, and finishes after `waits` calls that
/// said `Waiting` (a human who needs input).
struct Recorder {
    log: Log,
    waits: u32,
    left: u32,
    /// Defeat the first player at their turn (by the next one in the list).
    defeat_by: Option<PlayerId>,
}

impl Recorder {
    fn seat(log: &Log, player: PlayerId, waits: u32) -> Seat<'static> {
        Seat::new(
            player,
            Box::new(Recorder {
                log: log.clone(),
                waits,
                left: waits,
                defeat_by: None,
            }),
        )
    }
}

impl Controller for Recorder {
    fn play(&mut self, game: &mut Game, player: PlayerId, events: &mut Vec<Event>) -> Status {
        if self.left > 0 {
            self.left -= 1;
            return Status::Waiting;
        }
        self.left = self.waits;
        self.log.borrow_mut().push((game.round(), player));
        if let Some(winner) = self.defeat_by {
            events.extend(game.cheat_defeat_player(player, winner));
        }
        Status::Done
    }
}

fn game(wizards: u8) -> Game {
    Game::new(
        new_game(
            f4x_def::testdir::shipped(),
            &NewGame {
                all_ai: true,
                ..NewGame::with_wizards(wizards)
            },
        )
        .expect("a new game"),
    )
}

fn seats(w: &World, log: &Log) -> Vec<Seat<'static>> {
    w.player_ids()
        .into_iter()
        .filter(|&p| !w.is_neutral_player(p))
        .map(|p| Recorder::seat(log, p, 0))
        .collect()
}

/// The wizard whose turn comes first.
fn first_turn(w: &World) -> Option<PlayerId> {
    turn_players(w).into_iter().find(|&p| w.is_wizard(p))
}

#[test]
fn each_round_every_seat_plays_once_in_order() {
    let mut w = game(3);
    let log: Log = Log::default();
    let mut m = Master::new(seats(&w, &log));
    let start = w.round();
    m.run(&mut w, Until::Rounds(3));
    let ids: Vec<PlayerId> = m.seats().iter().filter_map(Seat::player).collect();
    let played = log.borrow().clone();
    let expect: Vec<(u32, PlayerId)> = (0..3)
        .flat_map(|r| ids.iter().map(move |&p| (start + r, p)))
        .collect();
    assert_eq!(played, expect);
    assert_eq!(
        w.round(),
        start + 3,
        "the round counter advances once per round"
    );
}

#[test]
fn a_waiting_controller_pauses_the_master_and_play_resumes() {
    let mut w = game(2);
    let log: Log = Log::default();
    let ids = w.player_ids();
    let human = Recorder::seat(&log, ids[0], 2);
    let ai = Recorder::seat(&log, ids[1], 0);
    let mut m = Master::new(vec![human, ai]);
    for _ in 0..2 {
        let p = m.run(&mut w, Until::Waiting);
        assert_eq!(p.stopped, Stopped::Waiting(ids[0]));
        assert!(log.borrow().is_empty(), "the human has not finished");
    }
    let p = m.run(&mut w, Until::Rounds(1));
    assert_eq!(p.stopped, Stopped::Ready, "the round ended");
    assert_eq!(log.borrow().len(), 2, "the human and then the AI played");
}

#[test]
fn only_the_acting_player_may_command() {
    let mut w = game(2);
    let log: Log = Log::default();
    let ids = w.player_ids();
    let mut m = Master::new(seats(&w, &log));
    assert_eq!(
        m.apply(
            &mut w,
            ids[0],
            Command::SetTaxRate {
                player: ids[0],
                rate: 1
            }
        ),
        Err(Refused::NobodyActing)
    );
    // Step to the first seat's turn.
    while m.active_player().is_none() {
        m.step(&mut w);
    }
    assert_eq!(m.active_player(), Some(ids[0]));
    assert!(m
        .apply(
            &mut w,
            ids[0],
            Command::SetTaxRate {
                player: ids[0],
                rate: 1
            }
        )
        .is_ok());
    assert_eq!(
        m.apply(
            &mut w,
            ids[1],
            Command::SetTaxRate {
                player: ids[1],
                rate: 1
            }
        ),
        Err(Refused::NotYourTurn {
            active: ids[0],
            actor: ids[1]
        })
    );
    assert_eq!(
        m.apply(
            &mut w,
            ids[0],
            Command::SetTaxRate {
                player: ids[1],
                rate: 1
            }
        ),
        Err(Refused::ForAnotherPlayer {
            actor: ids[0],
            named: ids[1]
        })
    );
}

#[test]
fn a_defeated_player_gets_no_further_turn() {
    let mut w = game(3);
    let log: Log = Log::default();
    let ids: Vec<PlayerId> = seats(&w, &log).iter().filter_map(Seat::player).collect();
    let mut seats = seats(&w, &log);
    // The second wizard is beaten on its first turn.
    seats[1] = Seat::new(
        ids[1],
        Box::new(Recorder {
            log: log.clone(),
            waits: 0,
            left: 0,
            defeat_by: Some(ids[0]),
        }),
    );
    let mut m = Master::new(seats);
    let start = w.round();
    m.run(&mut w, Until::Rounds(3));
    let by_second = log.borrow().iter().filter(|&&(_, p)| p == ids[1]).count();
    assert_eq!(by_second, 1, "it played only until it was beaten");
    assert!(log
        .borrow()
        .iter()
        .any(|&(r, p)| p == ids[2] && r == start + 2));
}

#[test]
fn a_game_resumes_from_a_saved_state_without_repeating_a_step() {
    let log: Log = Log::default();
    let mut straight = game(2);
    let mut a = Master::new(seats(&straight, &log));
    a.run(&mut straight, Until::Rounds(3));

    let log2: Log = Log::default();
    let mut w = game(2);
    let mut b = Master::new(seats(&w, &log2));
    b.run(&mut w, Until::Rounds(1));
    // Stop in the middle of round two, as a save would.
    for _ in 0..3 {
        b.step(&mut w);
    }
    let saved = b.state();
    assert!(matches!(
        saved.phase,
        Phase::TurnStart(_) | Phase::Acting(_) | Phase::TurnEnd(_)
    ));
    let mut c = Master::resume(seats(&w, &log2), saved);
    c.run(&mut w, Until::Rounds(2));
    assert_eq!(
        *log.borrow(),
        *log2.borrow(),
        "the same turns, none twice or missed"
    );
    assert_eq!(w.round(), straight.round());
    assert_eq!(w.next_u64(), straight.next_u64(), "the same random draws");
}

/// A game written out as a save and read back.
fn saved_and_loaded(game: &Game) -> Game {
    let text = game.save_file(Vec::new()).to_text().expect("a save");
    SaveFile::from_text(&text)
        .and_then(|s| s.restore(support::content(), ContentCheck::Strict))
        .expect("the save loads")
        .game
}

#[test]
fn a_game_saved_at_the_start_of_a_round_plays_on_as_the_game_that_went_on() {
    let log: Log = Log::default();
    let mut straight = game(3);
    Master::new(seats(&straight, &log)).run(&mut straight, Until::Rounds(4));

    let log2: Log = Log::default();
    let mut w = game(3);
    let mut m = Master::new(seats(&w, &log2)).checkpoints();
    // Round one is played; the master stops before round two begins, and the game is saved.
    let progress = m.run(&mut w, Until::Rounds(3));
    assert_eq!(progress.stopped, Stopped::RoundStart);
    assert_eq!(m.state().phase, Phase::RoundStart);
    let mut loaded = saved_and_loaded(&w);
    assert_eq!(loaded.round(), w.round(), "the round has not begun yet");

    let mut resumed = Master::from_game(seats(&loaded, &log2), &loaded).expect("the flow fits");
    assert_eq!(resumed.state().phase, Phase::RoundStart);
    resumed.run(&mut loaded, Until::Rounds(4));
    assert_eq!(
        *log.borrow(),
        *log2.borrow(),
        "the same turns, none twice or missed"
    );
    assert_eq!(loaded.round(), straight.round());
    assert_eq!(
        loaded.next_u64(),
        straight.next_u64(),
        "the same random draws"
    );
}

#[test]
fn the_game_knows_where_it_is_in_its_round_while_a_seat_waits() {
    let log: Log = Log::default();
    let mut w = game(2);
    let ids: Vec<PlayerId> = w
        .player_ids()
        .into_iter()
        .filter(|&p| !w.is_neutral_player(p))
        .collect();
    let seats = vec![
        Recorder::seat(&log, ids[0], 1),
        Recorder::seat(&log, ids[1], 0),
    ];
    let mut m = Master::new(seats);
    assert_eq!(
        m.run(&mut w, Until::Waiting).stopped,
        Stopped::Waiting(ids[0])
    );
    let loaded = saved_and_loaded(&w);
    let again = Master::from_game(
        vec![
            Recorder::seat(&log, ids[0], 1),
            Recorder::seat(&log, ids[1], 0),
        ],
        &loaded,
    )
    .expect("the flow fits");
    assert_eq!(again.state().phase, Phase::Acting(ids[0]));
}

#[test]
fn a_game_no_master_has_driven_is_resumed_on_the_first_seats_turn() {
    let log: Log = Log::default();
    let w = game(2);
    assert_eq!(w.phase(), None);
    let m = Master::from_game(seats(&w, &log), &w).expect("no phase is fine");
    assert_eq!(Some(m.state().phase), first_turn(&w).map(Phase::Acting));
}

#[test]
fn a_game_saved_at_another_table_is_refused() {
    let log: Log = Log::default();
    let mut w = game(3);
    let mut three = seats(&w, &log);
    // The third wizard waits, so the game stops on its turn.
    let last = three[2].player().expect("a wizard's seat");
    three[2] = Recorder::seat(&log, last, 1);
    assert_eq!(
        Master::new(three).run(&mut w, Until::Waiting).stopped,
        Stopped::Waiting(last)
    );
    let mut two = seats(&w, &log);
    two.truncate(2);
    assert_eq!(
        Master::from_game(two, &w).err(),
        Some(f4x_master::FlowError::NoSeat { player: last })
    );
}

/// Counts the events it is told of, whoever played the step that made them.
struct Listener {
    told: Rc<RefCell<usize>>,
}

impl Controller for Listener {
    fn play(&mut self, _: &mut Game, _: PlayerId, _: &mut Vec<Event>) -> Status {
        Status::Done
    }

    fn notice(&mut self, _: &Game, events: &[Event]) {
        *self.told.borrow_mut() += events.len();
    }
}

#[test]
fn every_seat_is_told_the_events_of_each_step_whoever_made_them() {
    let g0 = game(2);
    let mut w = g0;
    let told: Vec<Rc<RefCell<usize>>> = (0..2).map(|_| Rc::default()).collect();
    let seats: Vec<Seat> = w
        .player_ids()
        .into_iter()
        .filter(|&p| !w.is_neutral_player(p))
        .zip(&told)
        .map(|(p, t)| Seat::new(p, Box::new(Listener { told: t.clone() })))
        .collect();
    let mut master = Master::new(seats);
    let made = master.run(&mut w, Until::Rounds(2)).events.len();
    assert!(made > 0, "two rounds make events");
    for t in &told {
        assert_eq!(*t.borrow(), made, "each seat hears all of them");
    }
    // A host that changes the game itself tells the seats what its commands did.
    master.tell(
        &w,
        &[Event::BattleEnded {
            winner: PlayerId(0),
            loser: PlayerId(1),
        }],
    );
    for t in &told {
        assert_eq!(*t.borrow(), made + 1);
    }
}
