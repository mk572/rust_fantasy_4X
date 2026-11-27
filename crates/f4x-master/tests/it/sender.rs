//! Every command has a sender, and the game takes from a player only what is theirs to give
//! (`docs/turn-structure.md` 10.4).

use f4x_battle::battle::BattleCommand;
use f4x_core::diplomacy::DiplomacyCommand;
use f4x_core::map::Position;
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::world::{Command, Event, Rejection};
use f4x_def::rules::{UnitId, UnitKind};
use f4x_master::{Entry, Game};

use crate::support::{content, games, text};

/// A game with two wizards, the first two in the order of the turns.
fn two_wizards() -> (Game, PlayerId, PlayerId) {
    let (_, game) = games().remove(0);
    let mut wizards = game
        .turn_players()
        .iter()
        .copied()
        .filter(|&p| game.is_wizard(p));
    let (a, b) = (wizards.next(), wizards.next());
    let (a, b) = (a.expect("a wizard"), b.expect("a second wizard"));
    (game, a, b)
}

/// Where the first unit of `player` stands.
fn place_of(game: &Game, player: PlayerId) -> Position {
    let unit = game.units().player_units(player)[0];
    game.units().unit_position(unit).expect("a position")
}

/// A unit of a race, which every player can have.
fn some_unit(game: &Game) -> UnitId {
    let units = &game.content().rules.units;
    let normal = units
        .defs()
        .iter()
        .find(|d| d.kind == UnitKind::Normal && d.race.is_some());
    UnitId::new(normal.expect("a unit of a race").num)
}

/// The game refuses `command` from `sender` for `why`, and nothing is changed or recorded.
#[track_caller]
fn refuses(game: &mut Game, sender: PlayerId, command: Command, why: Rejection) {
    let before = text(game.world());
    game.start_recording();
    let events = game.apply(sender, command.clone());
    assert_eq!(events, vec![Event::Rejected(why)], "{command:?}");
    assert_eq!(text(game.world()), before, "{command:?}");
    let replay = game.stop_recording().expect("a recording");
    assert!(replay.entries.is_empty(), "{command:?} was recorded");
}

#[test]
fn a_command_for_another_player_is_refused() {
    let (mut game, me, other) = two_wizards();
    let theirs = game.units().player_units(other)[0];
    let at = place_of(&game, other);
    let commands = [
        Command::SetTaxRate {
            player: other,
            rate: 1,
        },
        Command::MoveStack {
            units: vec![theirs],
            to: at,
        },
        Command::DismissUnit { unit: theirs },
        Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: other,
            on: me,
        }),
        Command::AutoResolve {
            attackers: vec![theirs],
            defenders: game.units().player_units(me),
            mana_budget: None,
        },
        Command::Anyway(Box::new(Command::Resign { player: other })),
    ];
    for command in commands {
        refuses(&mut game, me, command, Rejection::NotYourCommand);
    }
}

#[test]
fn a_stack_with_another_players_unit_in_it_is_refused() {
    let (mut game, me, other) = two_wizards();
    let mine = game.units().player_units(me)[0];
    let theirs = game.units().player_units(other)[0];
    let at = place_of(&game, me);
    let command = Command::SetStandby {
        units: vec![mine, theirs],
        standby: Some(f4x_core::unit::Standby::Done),
    };
    refuses(&mut game, me, command, Rejection::NotYourCommand);
    let command = Command::GoTo {
        units: vec![mine, theirs],
        to: at,
    };
    refuses(&mut game, me, command, Rejection::NotYourCommand);
}

#[test]
fn a_player_cannot_set_the_world_up() {
    let (mut game, me, _) = two_wizards();
    let pos = place_of(&game, me);
    let unit = some_unit(&game);
    let create = Command::CreateUnit {
        owner: me,
        pos,
        unit,
    };
    let commands = [
        create.clone(),
        Command::Anyway(Box::new(create)),
        Command::FoundCity {
            owner: me,
            pos,
            name: "Free".into(),
            race: None,
            population: 3,
        },
        Command::PlaceFortress {
            player: me,
            pos,
            name: "Free".into(),
            race: None,
            population: 3,
        },
    ];
    for command in commands {
        refuses(&mut game, me, command, Rejection::SetupOnly);
    }
}

#[test]
fn a_sender_the_game_does_not_have_is_refused() {
    let (mut game, me, _) = two_wizards();
    let nobody = (0..u8::MAX)
        .map(PlayerId)
        .find(|&p| p != MONSTERS && game.player(p).is_none())
        .expect("a number no player has");
    let command = Command::SetTaxRate {
        player: me,
        rate: 1,
    };
    refuses(&mut game, nobody, command, Rejection::NoSuchPlayer);
}

#[test]
fn a_battle_order_is_the_side_to_plays() {
    let (mut game, me, other) = two_wizards();
    let mine = game.units().player_units(me)[0];
    let pos = place_of(&game, me);
    let unit = some_unit(&game);
    let defender = game.world_mut().spawn_unit(other, pos, unit);
    game.apply(
        me,
        Command::BeginBattle {
            attackers: vec![mine],
            defenders: vec![defender],
        },
    );
    let battle = game.battle().expect("a battle");
    let to_play = battle.player_of(battle.active);
    let waiting = if to_play == me { other } else { me };
    let order = Command::Battle(BattleCommand::EndTurn);
    refuses(&mut game, waiting, order.clone(), Rejection::NotYourCommand);
    let events = game.apply(to_play, order);
    assert!(
        !events.iter().any(|e| matches!(e, Event::Rejected(_))),
        "{events:?}"
    );
}

#[test]
fn a_replay_keeps_who_sent_each_command() {
    let (mut game, me, _) = two_wizards();
    game.start_recording();
    let rate = 1;
    game.apply(me, Command::SetTaxRate { player: me, rate });
    let replay = game.stop_recording().expect("a recording");
    assert!(
        matches!(replay.entries.as_slice(), [Entry::Command(p, _)] if *p == me),
        "{:?}",
        replay.entries
    );
    let (played, events) = replay.play(content(), usize::MAX).expect("plays");
    assert!(
        !events[0].iter().any(|e| matches!(e, Event::Rejected(_))),
        "{events:?}"
    );
    assert_eq!(text(played.world()), text(game.world()));
}
