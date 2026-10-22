//! Undoing orders within a turn.
//!
//! When undo is switched on (`Game::set_undo`), every command saves a snapshot of the game
//! first, and `Game::undo` goes back one command. Undo never gives an unfair look at hidden
//! things or a second try at luck, so a command that explored new squares, drew from the
//! random generator, or started a battle cannot be taken back, and neither can anything
//! before it. Ending the turn clears the history.

use crate::game::save::Snapshot;
use crate::game::{Command, Event, Game};

/// How many commands back one can go.
const DEPTH: usize = 32;

/// One step of history: the game as it was before the command.
pub(crate) struct UndoEntry {
    snapshot: Snapshot,
    label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoError {
    /// Undo is off, or there is nothing to take back.
    NothingToUndo,
    /// The saved state could not be rebuilt (content changed under it).
    Failed,
}

/// The command's name, for "Undo move".
fn label_of(command: &Command) -> String {
    let text = format!("{command:?}");
    text.split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or("command")
        .to_string()
}

impl Game {
    /// A refused command changed nothing, so it is not part of the record.
    fn forget_if_rejected(&mut self, events: &[Event]) {
        if events.iter().any(|e| matches!(e, Event::Rejected(_))) {
            self.forget_last_command();
        }
    }

    /// Switch undo on or off; switching it off forgets the history.
    pub fn set_undo(&mut self, on: bool) {
        self.undo_on = on;
        if !on {
            self.undo_stack.clear();
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Name of the command that `undo` would take back.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo_stack.last().map(|e| e.label.as_str())
    }

    /// Everything the player has explored, summed: it only ever grows.
    fn explored_total(&self) -> usize {
        self.players
            .iter()
            .map(|p| {
                crate::map::Plane::ALL
                    .iter()
                    .map(|&pl| self.fog.explored_count(p.id, pl))
                    .sum::<usize>()
            })
            .sum()
    }

    /// Run a command, recording how to take it back when that is allowed.
    pub fn apply(&mut self, command: Command) -> Vec<Event> {
        if self.game_over.is_some() {
            return vec![Event::Rejected(crate::game::Rejection::GameOver)];
        }
        self.record_command(&command);
        if !self.undo_on || self.battle.is_some() {
            let events = self.execute(command);
            self.forget_if_rejected(&events);
            return events;
        }
        let ends_history = matches!(
            command,
            Command::EndTurn | Command::Battle(_) | Command::EnterSite { .. }
        );
        let label = label_of(&command);
        let before = (!ends_history).then(|| self.snapshot());
        let (draws, explored) = (self.rng.draws(), self.explored_total());
        let events = self.execute(command);
        self.forget_if_rejected(&events);
        let rejected = events.iter().any(|e| matches!(e, Event::Rejected(_)));
        let irreversible = ends_history
            || self.rng.draws() != draws
            || self.explored_total() != explored
            || self.battle.is_some()
            || events
                .iter()
                .any(|e| matches!(e, Event::BattleStarted { .. }));
        if irreversible {
            self.undo_stack.clear();
        } else if !rejected {
            if let Some(snapshot) = before {
                self.undo_stack.push(UndoEntry { snapshot, label });
                if self.undo_stack.len() > DEPTH {
                    self.undo_stack.remove(0);
                }
            }
        }
        events
    }

    /// Take back the last command. Units and cities are rebuilt, so their `hecs` entities
    /// change: find them again by creation number (`entity_of_serial`).
    pub fn undo(&mut self) -> Result<String, UndoError> {
        let entry = self.undo_stack.pop().ok_or(UndoError::NothingToUndo)?;
        let rest = std::mem::take(&mut self.undo_stack);
        let recording = self.recording.take();
        let content = self.content.clone();
        match entry.snapshot.restore(content) {
            Ok(mut game) => {
                game.undo_on = true;
                game.undo_stack = rest;
                game.recording = recording;
                *self = game;
                self.forget_last_command();
                Ok(entry.label)
            }
            Err(_) => {
                self.recording = recording;
                self.undo_stack = rest;
                Err(UndoError::Failed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::content_with_units;
    use crate::map::Position;
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::player::PlayerId;

    const P0: PlayerId = PlayerId(0);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    /// A small explored world with one walker at (5, 5) and undo switched on.
    fn world() -> (Game, crate::game::serial::Serial) {
        let c = content_with_units();
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(3);
        g.content = c;
        g.map = Some(Map::new(Grid::new(20, 10, grass), Grid::new(20, 10, grass)));
        g.fog = crate::map::fog::Fog::new(20, 10);
        g.players.push(Player::new(P0, "W"));
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let u = g.spawn_unit(P0, at(5, 5), walker);
        // Everything is known already, so moving shows nothing new.
        g.fog.explore(P0, at(10, 5), 12);
        g.set_undo(true);
        let serial = g.serial_of(u);
        (g, serial)
    }

    fn walker(g: &Game, serial: crate::game::serial::Serial) -> hecs::Entity {
        g.units()
            .entity_of_serial(serial)
            .expect("the walker is there")
    }

    fn step(g: &mut Game, s: crate::game::serial::Serial, to: Position) {
        let u = walker(g, s);
        let ev = g.apply(Command::MoveStack { units: vec![u], to });
        assert!(
            !ev.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{ev:?}"
        );
    }

    #[test]
    fn undo_is_off_until_switched_on() {
        let (mut g, s) = world();
        g.set_undo(false);
        step(&mut g, s, at(6, 5));
        assert!(!g.can_undo());
        assert_eq!(g.undo(), Err(UndoError::NothingToUndo));
    }

    #[test]
    fn a_move_can_be_taken_back() {
        let (mut g, s) = world();
        let moves_before = g.units().unit_moves_left(walker(&g, s));
        step(&mut g, s, at(6, 5));
        assert_eq!(g.undo_label(), Some("MoveStack"));
        assert_eq!(g.units().unit_position(walker(&g, s)), Some(at(6, 5)));
        assert_eq!(g.undo(), Ok("MoveStack".to_string()));
        let u = walker(&g, s);
        assert_eq!(g.units().unit_position(u), Some(at(5, 5)));
        assert_eq!(
            g.units().unit_moves_left(u),
            moves_before,
            "the moves are back too"
        );
        assert!(!g.can_undo());
        assert!(g.undo_on, "undo stays on");
    }

    #[test]
    fn several_steps_go_back_one_at_a_time() {
        let (mut g, s) = world();
        step(&mut g, s, at(6, 5));
        step(&mut g, s, at(7, 5));
        g.apply(Command::SetTaxRate {
            player: P0,
            rate: 3,
        });
        assert_eq!(g.player(P0).unwrap().tax_rate, 3);
        g.undo().unwrap();
        assert_eq!(
            g.player(P0).unwrap().tax_rate,
            crate::player::START_TAX_RATE
        );
        g.undo().unwrap();
        assert_eq!(g.units().unit_position(walker(&g, s)), Some(at(6, 5)));
        g.undo().unwrap();
        assert_eq!(g.units().unit_position(walker(&g, s)), Some(at(5, 5)));
        assert_eq!(g.undo(), Err(UndoError::NothingToUndo));
    }

    #[test]
    fn ending_the_turn_clears_the_history() {
        let (mut g, s) = world();
        step(&mut g, s, at(6, 5));
        assert!(g.can_undo());
        g.apply(Command::EndTurn);
        assert!(!g.can_undo());
        assert_eq!(g.turn, 2);
    }

    #[test]
    fn refused_commands_leave_no_history() {
        let (mut g, s) = world();
        let u = walker(&g, s);
        let ev = g.apply(Command::MoveStack {
            units: vec![u],
            to: at(15, 5),
        });
        assert!(ev.iter().any(|e| matches!(e, Event::Rejected(_))));
        assert!(!g.can_undo());
    }

    #[test]
    fn a_move_that_uncovers_new_ground_cannot_be_undone() {
        let (mut g, s) = world();
        step(&mut g, s, at(6, 5));
        assert!(g.can_undo());
        // Put the walker at the edge of the known world and step into the unknown.
        let u = walker(&g, s);
        let _ = g.world.insert_one(u, at(16, 5));
        let _ = g.world.insert_one(u, crate::unit::MovesLeft(4));
        g.set_undo(true);
        step(&mut g, s, at(17, 5));
        assert!(g.can_undo(), "this step showed nothing new yet");
        step(&mut g, s, at(18, 5));
        assert!(g.can_undo());
        let _ = g.world.insert_one(walker(&g, s), at(19, 5));
        // Beyond the explored radius: x = 22 wraps to 2, which is known, so use a fresh fog.
        g.fog = crate::map::fog::Fog::new(20, 10);
        let _ = g.world.insert_one(walker(&g, s), crate::unit::MovesLeft(4));
        step(&mut g, s, at(18, 5));
        assert!(
            !g.can_undo(),
            "it showed new squares: no way back, and none before it"
        );
    }

    #[test]
    fn a_command_that_rolls_dice_wipes_the_history() {
        use crate::feature::node::tests::{spirit, world_seeded};
        let mut g = world_seeded(4);
        g.fog = crate::map::fog::Fog::new(24, 14);
        g.fog.explore(P0, at(6, 6), 30);
        g.set_undo(true);
        // Our guardian takes the node: no dice, so it can be taken back.
        let guard = spirit(&mut g, P0, at(6, 6), "guardian_spirit");
        g.apply(Command::Meld { units: vec![guard] });
        assert!(g.can_undo());
        g.undo().unwrap();
        assert_eq!(g.map.as_ref().unwrap().nodes()[0].owner, None);
        // Guard it again, then a rival tries: the one-in-four roll cannot be re-rolled.
        let guard = {
            let ids: Vec<_> = g.units().all_units();
            ids.into_iter()
                .find(|&u| {
                    g.units()
                        .unit_def(u)
                        .is_some_and(|d| d.id == "guardian_spirit")
                })
                .unwrap()
        };
        g.apply(Command::Meld { units: vec![guard] });
        assert!(g.can_undo());
        let rival = spirit(&mut g, PlayerId(1), at(6, 6), "spirit");
        g.apply(Command::Meld { units: vec![rival] });
        assert!(!g.can_undo(), "the dice were thrown");
    }

    #[test]
    fn undo_gives_back_the_same_game_in_every_detail() {
        let (mut g, s) = world();
        let before = g.snapshot();
        step(&mut g, s, at(6, 5));
        step(&mut g, s, at(7, 5));
        g.undo().unwrap();
        g.undo().unwrap();
        assert_eq!(g.snapshot(), before);
    }
}
