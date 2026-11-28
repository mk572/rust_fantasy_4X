//! The AI's side of the talk: it answers the proposals made to it, accepting, refusing or countering
//! (`trade.rs`, `bargain.rs`, `war_call.rs`, `diplo_b_talk::rate`, which asks `map_swap.rs`).

use f4x_core::diplomacy::{Deal, DiplomacyCommand, ProposalKind};
use f4x_core::player::view::PlayerView;
use f4x_core::world::{Command, Event};

use crate::trade::Reply;
use crate::Ai;
use f4x_master::Game;

/// Answer every proposal other wizards have made to this one. A proposal nobody answers lapses
/// as refused when the turn ends.
pub(crate) fn answer(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let pending = PlayerView::new(game, ai.player).proposals_to_me();
    for proposal in pending {
        let reply = match (proposal.kind, Deal::of(proposal.kind)) {
            (ProposalKind::Trade { .. }, _) => {
                crate::trade::respond(game, &ai.minds(), ai.player, proposal)
            }
            (_, Some(deal)) => crate::bargain::respond(game, &ai.minds(), proposal, deal),
            (ProposalKind::WarOn(third), _) if game.is_call_to_arms(proposal) => {
                crate::war_call::respond(game, &ai.minds(), proposal, third)
            }
            (kind, None) => {
                match crate::diplo_b_talk::rate(game, proposal.from, ai.player, kind, &ai.minds()) {
                    Verdict::Accepted => Reply::Accept,
                    Verdict::Refused => Reply::Reject,
                }
            }
        };
        if let Reply::Counter(kind) = reply {
            let sent = ai.send(
                game,
                Command::Diplomacy(DiplomacyCommand::Counter {
                    player: ai.player,
                    from: proposal.from,
                    kind,
                }),
                events,
            );
            if sent.is_ok() {
                continue;
            }
        }
        ai.order(
            game,
            Command::Diplomacy(DiplomacyCommand::Answer {
                player: ai.player,
                from: proposal.from,
                accept: reply == Reply::Accept,
            }),
            events,
        );
    }
}

/// How the AI rates a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Refused,
    Accepted,
}
