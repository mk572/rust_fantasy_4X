//! An attack on the human waits for its answer (`docs/turn-structure.md` 10.4): the game holds it,
//! the defender says who fights and how, and the master asks before the attacker's turn ends.

use std::cell::RefCell;
use std::rc::Rc;

use f4x_core::battle::world::BattleStartError;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_core::world::save::ContentCheck;
use f4x_core::world::{Command, Event, Rejection};
use f4x_def::rules::{UnitId, UnitKind};
use f4x_master::{Controller, Game, Master, Phase, SaveFile, Seat, Status, Stopped, Until};

use crate::support::{content, games, text};

/// Two soldiers of `a` beside two of `b`, who is the human.
struct Field {
    game: Game,
    a: PlayerId,
    b: PlayerId,
    attackers: Vec<hecs::Entity>,
    defenders: Vec<hecs::Entity>,
}

fn field() -> Field {
    let (_, mut game) = games().remove(0);
    let wizards: Vec<PlayerId> = game
        .turn_players()
        .iter()
        .copied()
        .filter(|&p| game.is_wizard(p))
        .collect();
    let (a, b) = (wizards[0], wizards[1]);
    game.world_mut().set_human(Some(b));
    let map = game.map().expect("a map");
    let free = |p: Position| {
        game.units().units_at(p).is_empty()
            && game.economy().terrain_class_at(p) == Some(TerrainClass::Grassland)
            && game.economy().city_at(p).is_none()
    };
    let plane = game
        .units()
        .player_units(a)
        .first()
        .and_then(|&u| game.units().unit_position(u))
        .expect("a unit")
        .plane;
    let (here, there) = (1..map.height() - 1)
        .flat_map(|y| (1..map.width() - 1).map(move |x| Position { x, y, plane }))
        .map(|p| (p, Position { x: p.x + 1, ..p }))
        .find(|&(p, q)| free(p) && free(q))
        .expect("two squares of grassland");
    let normal = game.content().rules.units.defs().iter();
    let kind = normal
        .filter(|d| d.kind == UnitKind::Normal && d.race.is_some())
        .map(|d| UnitId::new(d.num))
        .next()
        .expect("a unit of a race");
    let mut stack = |owner, pos| -> Vec<hecs::Entity> {
        (0..2)
            .map(|_| game.world_mut().spawn_unit(owner, pos, kind))
            .collect()
    };
    let attackers = stack(a, here);
    let defenders = stack(b, there);
    Field {
        game,
        a,
        b,
        attackers,
        defenders,
    }
}

impl Field {
    fn attack(&self) -> Command {
        Command::AutoResolve {
            attackers: self.attackers.clone(),
            defenders: self.defenders.clone(),
            mana_budget: None,
        }
    }

    fn defend(&self, fighters: &[hecs::Entity], auto: bool) -> Command {
        Command::Defend {
            attackers: self.attackers.clone(),
            fighters: fighters.to_vec(),
            auto,
            mana: None,
        }
    }
}

#[track_caller]
fn takes(game: &mut Game, player: PlayerId, command: Command) -> Vec<Event> {
    let events = game.apply(player, command.clone());
    let refused: Vec<&Event> = events
        .iter()
        .filter(|e| matches!(e, Event::Rejected(_)))
        .collect();
    assert!(refused.is_empty(), "{command:?}: {refused:?}");
    events
}

fn fought(events: &[Event]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, Event::BattleStarted { .. }))
}

fn no_such_attack() -> [Event; 1] {
    [Event::Rejected(Rejection::BattleStart(
        BattleStartError::NoSuchAttack,
    ))]
}

#[test]
fn an_attack_on_the_human_is_held_and_its_attackers_stay() {
    let mut f = field();
    let command = f.attack();
    let events = takes(&mut f.game, f.a, command);
    assert_eq!(events, [], "nothing was fought");
    let attack = f.game.waiting_attack().expect("an attack");
    assert_eq!(
        (attack.attackers, attack.defenders, attack.defender),
        (f.attackers.clone(), f.defenders.clone(), f.b)
    );
    for &u in &f.attackers {
        assert_eq!(f.game.units().unit_moves_left(u), 0);
    }
}

#[test]
fn an_attack_on_a_computer_player_is_fought_as_it_is_sent() {
    let mut f = field();
    f.game.world_mut().set_human(None);
    let command = f.attack();
    let events = takes(&mut f.game, f.a, command);
    assert!(fought(&events), "{events:?}");
    assert_eq!(f.game.waiting_attack(), None);
}

#[test]
fn an_attack_from_afar_is_refused() {
    let mut f = field();
    let far = f.game.units().unit_position(f.attackers[0]).expect("at");
    let far = Position {
        x: far.x - 1,
        ..far
    };
    for &u in &f.attackers.clone() {
        f.game.world_mut().cheat_move_unit(u, far);
    }
    let command = f.attack();
    let events = f.game.apply(f.a, command);
    let why = Rejection::BattleStart(BattleStartError::NotAdjacent);
    assert_eq!(events, [Event::Rejected(why)]);
    assert_eq!(f.game.waiting_attack(), None);
}

#[test]
fn only_the_defender_answers_and_only_with_the_stack_attacked() {
    let mut f = field();
    let nothing_waits = f.defend(&f.defenders, true);
    assert_eq!(f.game.apply(f.b, nothing_waits.clone()), no_such_attack());
    let command = f.attack();
    takes(&mut f.game, f.a, command);
    let before = text(f.game.world());
    let events = f.game.apply(f.a, f.defend(&f.defenders, true));
    assert_eq!(events, [Event::Rejected(Rejection::NotYourCommand)]);
    assert_eq!(f.game.apply(f.b, f.defend(&[], true)), no_such_attack());
    let elsewhere = f.game.units().player_units(f.b)[0];
    assert!(!f.defenders.contains(&elsewhere), "a unit elsewhere");
    let command = f.defend(&[elsewhere], true);
    assert_eq!(f.game.apply(f.b, command), no_such_attack());
    assert_eq!(text(f.game.world()), before);
    assert!(f.game.waiting_attack().is_some());
}

#[test]
fn the_defender_may_leave_the_fight_to_the_computer() {
    let mut f = field();
    let command = f.attack();
    takes(&mut f.game, f.a, command);
    let command = f.defend(&f.defenders[..1], true);
    let events = takes(&mut f.game, f.b, command.clone());
    assert!(fought(&events), "{events:?}");
    assert!(!f.game.in_battle());
    assert_eq!(f.game.waiting_attack(), None);
    assert_eq!(f.game.apply(f.b, command), no_such_attack());
}

#[test]
fn the_defender_may_fight_in_person() {
    let mut f = field();
    let command = f.attack();
    takes(&mut f.game, f.a, command);
    let command = f.defend(&f.defenders, false);
    takes(&mut f.game, f.b, command);
    assert!(f.game.in_battle());
    assert_eq!(f.game.waiting_attack(), None);
}

#[test]
fn a_held_attack_is_saved_with_the_game() {
    let mut f = field();
    let command = f.attack();
    takes(&mut f.game, f.a, command);
    let saved = f.game.save_file(Vec::new()).to_text().expect("a save");
    let loaded = SaveFile::from_text(&saved)
        .and_then(|s| s.restore(content(), ContentCheck::Strict))
        .expect("the save loads")
        .game;
    let serials = |game: &Game| {
        let attack = game.waiting_attack().expect("an attack");
        let units = game.units();
        let of = |stack: &[hecs::Entity]| -> Vec<_> {
            stack.iter().map(|&u| units.serial_of(u)).collect()
        };
        (
            of(&attack.attackers),
            of(&attack.defenders),
            attack.defender,
        )
    };
    assert_eq!(serials(&loaded), serials(&f.game));
}

/// How a controller answers the attack it is asked about.
#[derive(Clone, Copy, PartialEq)]
enum Answer {
    Wait,
    Leave,
}

/// Sends its command on its turn.
struct Sides {
    send: Option<Command>,
    answer: Rc<RefCell<Answer>>,
}

impl Controller for Sides {
    fn play(&mut self, game: &mut Game, player: PlayerId, events: &mut Vec<Event>) -> Status {
        if let Some(command) = self.send.take() {
            events.extend(takes(game, player, command));
        }
        Status::Done
    }

    fn react(&mut self, _: &mut Game, _: PlayerId, _: &mut Vec<Event>) -> Status {
        match *self.answer.borrow() {
            Answer::Wait => Status::Waiting,
            Answer::Leave => Status::Done,
        }
    }
}

fn master(f: &Field, answer: &Rc<RefCell<Answer>>) -> Master<'static> {
    let seat = |player, send| {
        let sides = Sides {
            send,
            answer: answer.clone(),
        };
        Seat::new(player, Box::new(sides))
    };
    Master::new(vec![seat(f.a, Some(f.attack())), seat(f.b, None)])
}

#[test]
fn the_attackers_turn_does_not_end_until_the_defender_has_answered_and_fought() {
    let mut f = field();
    let answer = Rc::new(RefCell::new(Answer::Wait));
    let mut master = master(&f, &answer);
    let held = Phase::TurnEnd(f.a);
    let progress = master.run(&mut f.game, Until::Waiting);
    assert_eq!(progress.stopped, Stopped::Waiting(f.b));
    assert_eq!(master.state().phase, held);

    // In person: the turn waits for the battle too.
    let command = f.defend(&f.defenders, false);
    takes(&mut f.game, f.b, command);
    let progress = master.step(&mut f.game);
    assert_eq!(progress.stopped, Stopped::Waiting(f.b));
    assert_eq!(master.state().phase, held);

    f.game.scenario_edit_battle(|battle| {
        for u in battle.units.iter_mut() {
            if u.side == f4x_battle::side::Side::Attacker {
                u.kill();
                u.status = f4x_battle::unit::Status::Dead;
            }
        }
    });
    let battle = f.game.battle().expect("a battle");
    let to_play = battle.player_of(battle.active);
    let order = Command::Battle(f4x_battle::battle::BattleCommand::EndTurn);
    takes(&mut f.game, to_play, order);
    assert!(!f.game.in_battle());
    let progress = master.step(&mut f.game);
    assert_eq!(progress.stopped, Stopped::Ready);
    assert_eq!(master.state().phase, Phase::TurnStart(f.b));
}

#[test]
fn an_attack_that_is_left_is_fought_by_the_computer() {
    let mut f = field();
    let answer = Rc::new(RefCell::new(Answer::Leave));
    let mut master = master(&f, &answer);
    // The turn's start, its play and its end.
    let mut events = Vec::new();
    for _ in 0..3 {
        events.extend(master.step(&mut f.game).events);
    }
    assert!(fought(&events), "{events:?}");
    assert!(!f.game.in_battle());
    assert_eq!(f.game.waiting_attack(), None);
    assert_eq!(master.state().phase, Phase::TurnStart(f.b));
}
