//! A recorded game plays again to the same game.

use f4x_battle::battle::BattleCommand;
use f4x_core::battle::world::BattleBrain;
use f4x_core::world::save::GAME_VERSION;
use f4x_core::world::{Command, Event, World};
use f4x_master::{Entry, Replay, ReplayError};

use crate::support::{all_farm, busy_round, content, field, games, text};

#[test]
fn a_recorded_game_plays_again_to_the_same_game() {
    for (name, mut game) in games() {
        game.start_recording();
        for _ in 0..10 {
            busy_round(&mut game, 6);
        }
        let replay = game.replay_so_far().expect("a recording");
        // Written and read again, as a file would carry it.
        let replay = Replay::from_text(&replay.to_text().expect("writes")).expect("reads");
        let (played, _) = replay.play(content(), usize::MAX).expect("plays");
        assert_eq!(text(played.world()), text(game.world()), "{name}");
    }
}

#[test]
fn balancing_the_farmers_is_replayed_and_can_be_taken_back() {
    for (name, mut game) in games() {
        all_farm(&mut game);
        let player = game.player_ids()[0];
        game.start_recording();
        game.set_undo(true);
        let before = text(game.world());
        game.command(Command::BalanceFarmers { player });
        let balanced = text(game.world());
        assert_ne!(balanced, before, "{name}: nothing was balanced");
        let replay = game.replay_so_far().expect("a record");
        let (again, _) = replay.play(content(), usize::MAX).expect("plays");
        assert_eq!(text(again.world()), balanced, "{name}");
        game.undo().expect("takes it back");
        assert_eq!(text(game.world()), before, "{name}");
    }
}

#[test]
fn a_replay_stopped_part_way_is_the_game_as_it_was_then() {
    for (name, mut game) in games() {
        game.start_recording();
        let mut states = vec![(0, text(game.world()))];
        for _ in 0..6 {
            busy_round(&mut game, 4);
            let done = game.replay_so_far().expect("a recording").entries.len();
            states.push((done, text(game.world())));
        }
        let replay = game.stop_recording().expect("a recording");
        for (count, state) in states {
            let (part, _) = replay.play(content(), count).expect("plays");
            assert_eq!(text(part.world()), state, "{name}, after {count} commands");
        }
        assert!(!game.is_recording());
    }
}

#[test]
fn a_replay_that_is_no_replay_or_names_nothing_is_refused() {
    assert!(matches!(
        Replay::from_text("nonsense"),
        Err(ReplayError::Load(_))
    ));
    let (_, mut game) = games().remove(0);
    game.start_recording();
    busy_round(&mut game, 3);
    let mut replay = game.stop_recording().expect("recorded");
    let unknown = "MoveStack(units: [999999], to: (x: 0, y: 0, plane: Arcanus))";
    let sender = game.player_ids()[0];
    replay
        .entries
        .insert(0, Entry::Command(sender, unknown.into()));
    assert!(matches!(
        replay.play(content(), usize::MAX),
        Err(ReplayError::Unresolved(0))
    ));
    let (_, untouched) = games().remove(0);
    assert!(matches!(
        untouched.replay_so_far(),
        Err(ReplayError::NotRecording)
    ));
}

#[test]
fn the_units_a_recorded_command_names_are_told_apart() {
    let (_, mut game) = games().remove(0);
    game.start_recording();
    busy_round(&mut game, 6);
    let replay = game.stop_recording().expect("a recording");
    let moves: std::collections::BTreeSet<&str> = replay
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Command(_, c) if c.starts_with("MoveStack") => Some(c.as_str()),
            _ => None,
        })
        .collect();
    assert!(moves.len() >= 3, "{moves:?}");
    let units: std::collections::BTreeSet<&str> = moves
        .iter()
        .filter_map(|c| c.split("units:[").nth(1)?.split(']').next())
        .collect();
    assert!(units.len() >= 3, "the moves name too few units: {units:?}");
}

#[test]
fn a_replay_of_a_game_version_this_game_cannot_read_is_refused() {
    let (_, mut game) = games().remove(0);
    game.start_recording();
    busy_round(&mut game, 2);
    let text = game
        .stop_recording()
        .expect("recorded")
        .to_text()
        .expect("writes");
    assert!(Replay::from_text(&text).is_ok());
    let from = format!("game_version: \"{GAME_VERSION}\"");
    assert!(text.contains(&from), "the replay does not say its version");
    for version in ["9999.0.0", "0.0.0", "banana"] {
        let edited = text.replacen(&from, &format!("game_version: \"{version}\""), 1);
        assert!(
            matches!(Replay::from_text(&edited), Err(ReplayError::Load(_))),
            "{version}"
        );
    }
}

/// A brain that ends its side's turn `waits` times and then flees: what it answers depends on how
/// often it was asked.
struct Fickle {
    waits: u32,
}

impl BattleBrain for Fickle {
    fn command(&mut self, _: &World) -> BattleCommand {
        match self.waits.checked_sub(1) {
            Some(left) => {
                self.waits = left;
                BattleCommand::EndTurn
            }
            None => BattleCommand::Flee,
        }
    }
}

/// The test games have no scripted battle AI, and a replay no brain: only the recorded orders
/// make the defender flee. Without them the attacker would, as the side to play first.
#[test]
fn an_auto_resolved_battle_is_replayed_with_its_orders_and_no_brain() {
    for answered in [false, true] {
        let mut f = field();
        f.game.world_mut().set_human(answered.then_some(f.b));
        for (player, waits) in [(f.a, 10), (f.b, 1)] {
            let brain = Box::new(Fickle { waits });
            f.game.world_mut().set_battle_brain(player, Some(brain));
        }
        f.game.start_recording();
        let attack = Command::AutoResolve {
            attackers: f.attackers.clone(),
            defenders: f.defenders.clone(),
            mana_budget: None,
        };
        let mut events = f.game.apply(f.a, attack);
        if answered {
            let defend = Command::Defend {
                attackers: f.attackers.clone(),
                fighters: f.defenders.clone(),
                auto: true,
                mana: None,
            };
            events = f.game.apply(f.b, defend);
        }
        let lost = |e: &Event| matches!(e, Event::BattleEnded { loser, .. } if *loser == f.b);
        assert!(events.iter().any(lost), "{events:?}");

        let replay = f.game.replay_so_far().expect("a recording");
        let mut replay = Replay::from_text(&replay.to_text().expect("writes")).expect("reads");
        let orders = match replay.entries.last() {
            Some(Entry::AutoBattle(_, _, orders)) => orders.clone(),
            other => panic!("no orders were recorded: {other:?}"),
        };
        assert_eq!(orders.last(), Some(&BattleCommand::Flee));
        assert!(orders.len() > 1, "{orders:?}");
        let (played, all) = replay.play(content(), usize::MAX).expect("plays");
        let fight = |events: &[Event]| -> Vec<Event> {
            let battle = events.iter().filter(|e| matches!(e, Event::Battle(_)));
            battle.cloned().collect()
        };
        assert_eq!(all.last().map(|e| fight(e)), Some(fight(&events)));
        assert_eq!(text(played.world()), text(f.game.world()));
        // A replay of the replay carries the orders on.
        let again = played.replay_so_far().expect("a recording");
        assert_eq!(again.entries, replay.entries);

        // The command alone asks again, and nobody answers as the brains did.
        if let Some(Entry::AutoBattle(player, command, _)) = replay.entries.pop() {
            replay.entries.push(Entry::Command(player, command));
        }
        let (other, _) = replay.play(content(), usize::MAX).expect("plays");
        assert_ne!(text(other.world()), text(f.game.world()));
    }
}
