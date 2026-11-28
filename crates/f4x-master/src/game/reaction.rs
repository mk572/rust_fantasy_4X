//! Reactions: what a player may do on another player's turn (`docs/turn-structure.md` 10.4).
//!
//! On its own turn a player may do anything the world allows; on another's only react to
//! something that happened to it. A request (a proposal, a resold item) and an attack wait in
//! the world, and the game reads them there. What may be left lapses with the turn it happened
//! in: the game keeps it as a [`Reaction`], saved with the game.
//!
//! Tests: in tests/it/reaction.rs.

use f4x_core::diplomacy::{DiplomacyCommand, DiplomacyEvent};
use f4x_core::item::armory::ItemSpot;
use f4x_core::player::offer::Offer;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::world::{Command, Event};
use serde::{Deserialize, Serialize};

use super::Game;

/// Something that happened to `player` in the turn being played, which it may answer until the
/// turn ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reaction {
    /// It met `with` for the first time: it may greet them.
    Contact { player: PlayerId, with: PlayerId },
    /// `by` took a city of it: it may declare war on them.
    CityTaken { player: PlayerId, by: PlayerId },
    /// Items fell to it: it may place, sell or scrap what lies in its armory. They go to the
    /// vault first, and nothing tells them from what lay there.
    Items { player: PlayerId },
    /// It was banished: it may resign.
    Banished { player: PlayerId },
}

/// The reactions an event opens.
fn opened_by(event: &Event) -> Vec<Reaction> {
    match *event {
        Event::Diplomacy(DiplomacyEvent::ContactMade { a, b }) => vec![
            Reaction::Contact { player: a, with: b },
            Reaction::Contact { player: b, with: a },
        ],
        Event::CityCaptured { from, to, .. } => vec![Reaction::CityTaken {
            player: from,
            by: to,
        }],
        Event::ItemsAwarded { player, .. } => vec![Reaction::Items { player }],
        Event::WizardBanished { player, .. } => vec![Reaction::Banished { player }],
        _ => Vec::new(),
    }
}

impl Game {
    /// What the players may still answer of what happened to them in this turn. Requests and
    /// attacks are not among them: the world has those.
    pub fn reactions(&self) -> &[Reaction] {
        &self.state.reactions
    }

    /// Open the reactions `events` give rise to. Returns whether there is a new one.
    pub(super) fn open_reactions(&mut self, events: &[Event]) -> bool {
        let before = self.state.reactions.len();
        for reaction in events.iter().flat_map(opened_by) {
            if !self.state.reactions.contains(&reaction) {
                self.state.reactions.push(reaction);
            }
        }
        self.state.reactions.len() > before
    }

    /// Whether `command` is one `player` may send on another player's turn. That the command is
    /// this player's to give is checked before ([`World::may_command`]).
    ///
    /// [`World::may_command`]: f4x_core::world::World::may_command
    pub(super) fn is_reaction(&self, player: PlayerId, command: &Command) -> bool {
        let open = |reaction| self.state.reactions.contains(&reaction);
        let items = || open(Reaction::Items { player });
        // A proposal of `from` that waits for the player's answer.
        let asked = |from: PlayerId| {
            let proposals = self.world.diplomacy.proposals();
            proposals.iter().any(|p| p.from == from && p.to == player)
        };
        // An item another wizard sells waits as an offer.
        let resale = |index: usize| {
            let offers = PlayerView::new(&self.world, player).offers();
            matches!(offers.get(index), Some(Offer::Resale { .. }))
        };
        match command {
            Command::Anyway(inner) => self.is_reaction(player, inner),
            // An attack that waits, and its battle, where only the side to play may give an
            // order. Whether there is such an attack is checked for every sender.
            Command::Defend { .. } | Command::Battle(_) => true,
            Command::Diplomacy(talk) => match *talk {
                DiplomacyCommand::Answer { from, .. } | DiplomacyCommand::Counter { from, .. } => {
                    asked(from)
                }
                // Paying.
                DiplomacyCommand::GiveGold { to, .. } => asked(to),
                DiplomacyCommand::DeclareWar { on, .. } => {
                    asked(on) || open(Reaction::CityTaken { player, by: on })
                }
                DiplomacyCommand::Greeted { with, .. } => open(Reaction::Contact { player, with }),
                DiplomacyCommand::Propose { .. }
                | DiplomacyCommand::BreakTreaty { .. }
                | DiplomacyCommand::GiveSpell { .. }
                | DiplomacyCommand::Warn { .. }
                | DiplomacyCommand::Overlook { .. }
                | DiplomacyCommand::Remark { .. } => false,
            },
            Command::AcceptOffer { index, .. } | Command::RejectOffer { index, .. } => {
                resale(*index)
            }
            // From the armory: what a hero carries stays where it is.
            Command::MoveItem { from, .. } => !matches!(from, ItemSpot::Hero(..)) && items(),
            Command::SellItem { .. } | Command::ScrapItem { .. } => items(),
            Command::Resign { .. } => open(Reaction::Banished { player }),
            _ => false,
        }
    }
}
