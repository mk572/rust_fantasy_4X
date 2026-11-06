//! The master on real games: the order of the seats, human controllers that wait, command
//! validation, defeated seats and resuming from a saved state.

use std::cell::RefCell;
use std::rc::Rc;

use f4x_core::game::setup::{new_game, NewGame};
use f4x_core::game::{Command, Event, Game};
use f4x_core::player::PlayerId;
use f4x_master::{Controller, Master, Phase, Refused, Seat, Status, Stopped, Until};

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
    fn seat(log: &Log, player: PlayerId, waits: u32) -> Seat {
        Seat::new(
            vec![player],
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
    fn play(&mut self, game: &mut Game, players: &[PlayerId], events: &mut Vec<Event>) -> Status {
        if self.left > 0 {
            self.left -= 1;
            return Status::Waiting;
        }
        self.left = self.waits;
        self.log.borrow_mut().push((game.turn, players[0]));
        if let Some(winner) = self.defeat_by {
            events.extend(game.cheat_defeat_player(players[0], winner));
        }
        Status::Done
    }
}

fn game(wizards: u8) -> Game {
    new_game(
        f4x_def::testdir::shipped(),
        &NewGame {
            all_ai: true,
            ..NewGame::with_wizards(wizards)
        },
    )
    .expect("a new game")
}

fn seats(g: &Game, log: &Log) -> Vec<Seat> {
    g.player_ids()
        .into_iter()
        .filter(|&p| !g.is_neutral_player(p))
        .map(|p| Recorder::seat(log, p, 0))
        .collect()
}

#[test]
fn each_round_every_seat_plays_once_in_order() {
    let mut g = game(3);
    let log: Log = Log::default();
    let mut m = Master::new(seats(&g, &log));
    let start = g.turn;
    m.run(&mut g, Until::Rounds(3));
    let ids: Vec<PlayerId> = m.seats().iter().map(|s| s.players()[0]).collect();
    let played = log.borrow().clone();
    let expect: Vec<(u32, PlayerId)> = (0..3)
        .flat_map(|r| ids.iter().map(move |&p| (start + r, p)))
        .collect();
    assert_eq!(played, expect);
    assert_eq!(
        g.turn,
        start + 2,
        "the round counter advances once per round (the first round had begun)"
    );
}

#[test]
fn a_waiting_controller_pauses_the_master_and_play_resumes() {
    let mut g = game(2);
    let log: Log = Log::default();
    let ids = g.player_ids();
    let human = Recorder::seat(&log, ids[0], 2);
    let ai = Recorder::seat(&log, ids[1], 0);
    let mut m = Master::new(vec![human, ai]);
    for _ in 0..2 {
        let p = m.run(&mut g, Until::Waiting);
        assert_eq!(p.stopped, Stopped::Waiting(0));
        assert!(log.borrow().is_empty(), "the human has not finished");
    }
    let p = m.run(&mut g, Until::Rounds(1));
    assert_eq!(p.stopped, Stopped::Ready, "the round ended");
    assert_eq!(log.borrow().len(), 2, "the human and then the AI played");
}

#[test]
fn only_the_acting_player_may_command() {
    let mut g = game(2);
    let log: Log = Log::default();
    let ids = g.player_ids();
    let mut m = Master::new(seats(&g, &log));
    assert_eq!(
        m.apply(
            &mut g,
            ids[0],
            Command::SetTaxRate {
                player: ids[0],
                rate: 1
            }
        ),
        Err(Refused::NobodyActing)
    );
    // Step to the first seat's turn.
    while m.active_seat().is_none() {
        m.step(&mut g);
    }
    assert_eq!(m.active_player(), Some(ids[0]));
    assert!(m
        .apply(
            &mut g,
            ids[0],
            Command::SetTaxRate {
                player: ids[0],
                rate: 1
            }
        )
        .is_ok());
    assert_eq!(
        m.apply(
            &mut g,
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
            &mut g,
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
    assert_eq!(
        m.apply(&mut g, ids[0], Command::EndTurn),
        Err(Refused::UseLifecycle)
    );
}

#[test]
fn a_defeated_player_gets_no_further_turn() {
    let mut g = game(3);
    let log: Log = Log::default();
    let ids: Vec<PlayerId> = seats(&g, &log).iter().map(|s| s.players()[0]).collect();
    let mut seats = seats(&g, &log);
    // The second wizard is beaten on its first turn.
    seats[1] = Seat::new(
        vec![ids[1]],
        Box::new(Recorder {
            log: log.clone(),
            waits: 0,
            left: 0,
            defeat_by: Some(ids[0]),
        }),
    );
    let mut m = Master::new(seats);
    let start = g.turn;
    m.run(&mut g, Until::Rounds(3));
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
    let mut g = game(2);
    let mut b = Master::new(seats(&g, &log2));
    b.run(&mut g, Until::Rounds(1));
    // Stop in the middle of round two, as a save would.
    for _ in 0..3 {
        b.step(&mut g);
    }
    let saved = b.state();
    assert!(matches!(
        saved.phase,
        Phase::TurnStart(_) | Phase::Acting(_) | Phase::TurnEnd(_)
    ));
    let mut c = Master::resume(seats(&g, &log2), saved);
    c.run(&mut g, Until::Rounds(2));
    assert_eq!(
        *log.borrow(),
        *log2.borrow(),
        "the same turns, none twice or missed"
    );
    assert_eq!(g.turn, straight.turn);
    assert_eq!(g.next_u64(), straight.next_u64(), "the same random draws");
}
