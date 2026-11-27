//! Whether the game is over, and who won (`docs/game_end.md`). The world keeps the facts; the game
//! looks at them after each command and each step of the turn order.
//!
//! Tests: in tests/it/end.rs.

use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::world::endgame::EndKind;
use f4x_core::world::{Event, World};
use serde::{Deserialize, Serialize};

use super::Game;

/// How the game ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameOver {
    pub winner: PlayerId,
    pub kind: EndKind,
    /// The round it ended in.
    pub turn: u32,
}

/// Who has won in `world` as it stands, and how; `None` while the game goes on. Of two conditions
/// met at once the first of these counts: the human wizard has fallen, the Spell of Mastery is
/// complete, a wizard has been defeated and at most one is left.
fn verdict(world: &World) -> Option<(PlayerId, EndKind)> {
    let defeated = |p: PlayerId| world.player(p).is_some_and(|pl| pl.is_defeated());
    let beaten_by = |p: PlayerId| world.player(p).and_then(|pl| pl.defeated_by());
    if let Some(human) = world.human().filter(|&h| defeated(h)) {
        return Some((beaten_by(human).unwrap_or(MONSTERS), EndKind::Conquest));
    }
    if let Some(caster) = world.mastered_by() {
        return Some((caster, EndKind::SpellOfMastery));
    }
    let fallen = world
        .player_ids()
        .into_iter()
        .find(|&p| world.is_wizard(p) && defeated(p))?;
    match world.living_wizards().as_slice() {
        [last] => Some((*last, EndKind::Conquest)),
        [] => Some((beaten_by(fallen).unwrap_or(MONSTERS), EndKind::Conquest)),
        _ => None,
    }
}

/// Note in `over` that the game has ended in this `round`, if it has and `over` does not say so
/// yet; the event that tells of it.
pub(super) fn judge(world: &World, round: u32, over: &mut Option<GameOver>) -> Option<Event> {
    if over.is_some() {
        return None;
    }
    let (winner, kind) = verdict(world)?;
    *over = Some(GameOver {
        winner,
        kind,
        turn: round,
    });
    Some(Event::GameOver {
        winner,
        kind,
        turn: round,
    })
}

impl Game {
    /// How the game ended, once it has.
    pub fn game_over(&self) -> Option<GameOver> {
        self.state.game_over
    }

    /// Look at the world and end the game if it is decided. A command or a step does this by
    /// itself; whoever changes the world by hand ([`Game::world_mut`], a cheat) calls it
    /// afterwards. Returns the event that tells of the end, if it ended now.
    pub fn check_end(&mut self) -> Vec<Event> {
        let event = judge(&self.world, self.state.round, &mut self.state.game_over);
        if event.is_some() {
            self.forget_history();
        }
        event.into_iter().collect()
    }
}
