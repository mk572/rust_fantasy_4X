//! A turn waits for the answers to its proposals (`docs/turn-structure.md` 10.4): the master asks
//! the controller of whoever a proposal waits for before the turn ends.

use std::cell::RefCell;
use std::rc::Rc;

use f4x_core::diplomacy::{DiplomacyCommand, ProposalKind};
use f4x_core::player::PlayerId;
use f4x_core::world::{Command, Event, Rejection};
use f4x_master::{Controller, Game, Master, Phase, Seat, Status, Stopped, Until};

use crate::support::games;

/// What the two controllers of a test did, in order.
type Log = Rc<RefCell<Vec<String>>>;

/// How a controller answers a proposal it is asked about.
#[derive(Clone, Copy, PartialEq)]
enum Answer {
    /// It needs more input: the turn waits.
    Wait,
    Leave,
    Refuse,
}

/// Proposes peace to `to` on its turns, when it has someone to propose to.
struct Talker {
    log: Log,
    to: Option<PlayerId>,
    answer: Rc<RefCell<Answer>>,
}

fn send(game: &mut Game, player: PlayerId, talk: DiplomacyCommand) -> Vec<Event> {
    let events = game.apply(player, Command::Diplomacy(talk));
    let refused: Vec<&Rejection> = events
        .iter()
        .filter_map(|e| match e {
            Event::Rejected(why) => Some(why),
            _ => None,
        })
        .collect();
    assert!(refused.is_empty(), "{refused:?}");
    events
}

impl Controller for Talker {
    fn play(&mut self, game: &mut Game, player: PlayerId, events: &mut Vec<Event>) -> Status {
        self.log.borrow_mut().push(format!("play {}", player.0));
        if let Some(to) = self.to {
            let talk = DiplomacyCommand::Propose {
                from: player,
                to,
                kind: ProposalKind::Pact,
            };
            events.extend(send(game, player, talk));
        }
        Status::Done
    }

    fn react(&mut self, game: &mut Game, player: PlayerId, events: &mut Vec<Event>) -> Status {
        self.log.borrow_mut().push(format!("react {}", player.0));
        match *self.answer.borrow() {
            Answer::Wait => Status::Waiting,
            Answer::Leave => Status::Done,
            Answer::Refuse => {
                let from = game.diplomacy.proposals().iter().find(|p| p.to == player);
                let from = from.expect("asked without a proposal").from;
                let talk = DiplomacyCommand::Answer {
                    player,
                    from,
                    accept: false,
                };
                events.extend(send(game, player, talk));
                Status::Done
            }
        }
    }
}

/// A game of two wizards who have met, with the first proposing to the second, and its master at
/// the start of the first one's turn.
struct Table {
    game: Game,
    master: Master<'static>,
    log: Log,
    answer: Rc<RefCell<Answer>>,
    proposer: PlayerId,
    receiver: PlayerId,
}

fn table(answer: Answer) -> Table {
    seated(answer, true)
}

/// [`table`], where the first wizard proposes nothing when `proposes` is false.
fn seated(answer: Answer, proposes: bool) -> Table {
    let (_, mut game) = games().remove(0);
    let wizards: Vec<PlayerId> = game
        .turn_players()
        .iter()
        .copied()
        .filter(|&p| game.is_wizard(p))
        .collect();
    let (proposer, receiver) = (wizards[0], wizards[1]);
    game.world_mut().make_contact(proposer, receiver);
    let log = Log::default();
    let answer = Rc::new(RefCell::new(answer));
    let seats = wizards
        .iter()
        .map(|&p| {
            let talker = Talker {
                log: log.clone(),
                to: (proposes && p == proposer).then_some(receiver),
                answer: answer.clone(),
            };
            Seat::new(p, Box::new(talker))
        })
        .collect();
    Table {
        game,
        master: Master::new(seats),
        log,
        answer,
        proposer,
        receiver,
    }
}

#[test]
fn a_turn_does_not_end_while_its_proposal_waits_for_an_answer() {
    let mut t = table(Answer::Wait);
    let held = Phase::TurnEnd(t.proposer);
    for _ in 0..3 {
        let progress = t.master.run(&mut t.game, Until::Waiting);
        assert_eq!(progress.stopped, Stopped::Waiting(t.receiver));
        assert_eq!(t.master.state().phase, held);
        assert_eq!(t.game.phase(), Some(held), "saved where it waits");
    }
    let played = format!("play {}", t.proposer.0);
    let plays = t.log.borrow().iter().filter(|&l| *l == played).count();
    assert_eq!(plays, 1, "the turn is not played again: {:?}", t.log);

    *t.answer.borrow_mut() = Answer::Refuse;
    let progress = t.master.step(&mut t.game);
    assert_eq!(progress.stopped, Stopped::Ready);
    assert_eq!(t.game.diplomacy.proposals(), []);
    assert_eq!(t.master.state().phase, Phase::TurnStart(t.receiver));
}

#[test]
fn a_proposal_that_is_left_does_not_hold_the_turn() {
    let mut t = table(Answer::Leave);
    // The turn's start, its play and its end.
    for _ in 0..3 {
        t.master.step(&mut t.game);
    }
    assert_eq!(t.master.state().phase, Phase::TurnStart(t.receiver));
    assert_eq!(t.game.diplomacy.proposals(), [], "it lapsed with the turn");
    let asked = format!("react {}", t.receiver.0);
    assert_eq!(
        t.log.borrow().iter().filter(|&l| *l == asked).count(),
        1,
        "{:?}",
        t.log
    );
}

#[test]
fn a_turn_with_no_proposal_asks_nobody() {
    let mut t = seated(Answer::Wait, false);
    for _ in 0..3 {
        t.master.step(&mut t.game);
    }
    assert_eq!(t.master.state().phase, Phase::TurnStart(t.receiver));
    assert_eq!(*t.log.borrow(), [format!("play {}", t.proposer.0)]);
}
