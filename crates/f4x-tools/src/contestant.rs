//! The scripted strategic AI in its two versions: the live crate (`f4x-ai`, being worked on) and
//! the frozen reference (`f4x-ai-ref`, made by `scripts/fork_ai.py`). The soak plays either, or
//! both in one game, through the small surface the two share.

use f4x_core::game::{Event, Game};
use f4x_core::player::PlayerId;

/// Which copy of the AI plays a wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// `f4x-ai`, the AI under work.
    New,
    /// `f4x-ai-ref`, the frozen copy to measure against.
    Reference,
}

/// A command the game refused, as the soak's refused-order log keeps it.
pub struct Refused {
    pub turn: u32,
    pub player: PlayerId,
    pub source: String,
    pub command: String,
    pub reason: String,
}

/// What the soak needs from an AI player, whichever crate it comes from.
pub trait Contestant {
    fn player(&self) -> PlayerId;
    fn play_turn(&mut self, game: &mut Game) -> Vec<Event>;
    fn record_refusals(&mut self, on: bool);
    fn take_refusals(&self) -> Vec<Refused>;
    /// The AI's own state for the soak's detail lines: the tail of the player's line.
    fn describe(&self) -> String;
}

macro_rules! contestant {
    ($krate:ident) => {
        impl Contestant for $krate::Ai {
            fn player(&self) -> PlayerId {
                self.player
            }
            fn play_turn(&mut self, game: &mut Game) -> Vec<Event> {
                $krate::Ai::play_turn(self, game)
            }
            fn record_refusals(&mut self, on: bool) {
                $krate::Ai::record_refusals(self, on);
            }
            fn take_refusals(&self) -> Vec<Refused> {
                $krate::Ai::take_refusals(self)
                    .into_iter()
                    .map(|r| Refused {
                        turn: r.turn,
                        player: r.player,
                        source: r.source,
                        command: r.command,
                        reason: r.reason,
                    })
                    .collect()
            }
            fn describe(&self) -> String {
                format!(
                    "{}, scouts {:?}, campaign {:?}, landing {:?}",
                    if self.frontier_closed() {
                        " (frontier closed)"
                    } else {
                        ""
                    },
                    self.scout_targets()
                        .into_iter()
                        .map(|(_, p)| (p.x, p.y))
                        .collect::<Vec<_>>(),
                    self.campaign().map(|c| (c.x, c.y)),
                    self.landing()
                        .map(|(t, port, f)| ((t.x, t.y), (port.x, port.y), f))
                )
            }
        }
    };
}

contestant!(f4x_ai);
contestant!(f4x_ai_ref);

/// A player of the given version.
pub fn make(version: Version, player: PlayerId) -> Box<dyn Contestant> {
    match version {
        Version::New => Box::new(f4x_ai::Ai::new(player)),
        Version::Reference => Box::new(f4x_ai_ref::Ai::new(player)),
    }
}

/// The independent cities and roaming monsters play their turn, by the given version.
pub fn play_neutrals(version: Version, game: &mut Game) -> Vec<Event> {
    match version {
        Version::New => f4x_ai::play_neutrals(game),
        Version::Reference => f4x_ai_ref::play_neutrals(game),
    }
}

/// Every contestant plays its turn, then the neutrals, then the turn ends for everyone (the same
/// order as `f4x_ai::play_round`). Returns the events of each contestant's turn, in order, and
/// then those of the neutrals and the turn change.
pub fn play_round(
    game: &mut Game,
    players: &mut [Box<dyn Contestant>],
    neutrals: Version,
) -> (Vec<(PlayerId, Vec<Event>)>, Vec<Event>) {
    let turns = players
        .iter_mut()
        .map(|p| (p.player(), p.play_turn(game)))
        .collect();
    let mut rest = play_neutrals(neutrals, game);
    if game.game_over.is_none() {
        rest.extend(game.apply(f4x_core::game::Command::EndTurn));
    }
    (turns, rest)
}
