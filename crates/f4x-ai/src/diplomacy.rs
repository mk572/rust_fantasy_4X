//! The AI's side of the talk: it answers the proposals made to it (`diplo_b_talk::rate`).

use f4x_core::diplomacy::{DiplomacyCommand, ProposalKind};
use f4x_core::player::view::PlayerView;
use f4x_core::world::{Command, Event};

use crate::Ai;
use f4x_master::Game;

/// Answer every proposal other wizards have made to this one. A proposal nobody answers lapses
/// as refused when the turn ends.
pub(crate) fn answer(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let pending = PlayerView::new(game, ai.player).proposals_to_me();
    for proposal in pending {
        if matches!(proposal.kind, ProposalKind::Trade { .. }) {
            match crate::trade::respond(game, &ai.minds(), ai.player, proposal) {
                crate::trade::Reply::Counter(kind) => {
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
                reply => {
                    ai.order(
                        game,
                        Command::Diplomacy(DiplomacyCommand::Answer {
                            player: ai.player,
                            from: proposal.from,
                            accept: reply == crate::trade::Reply::Accept,
                        }),
                        events,
                    );
                    continue;
                }
            }
        }
        let accept =
            crate::diplo_b_talk::rate(game, proposal.from, proposal.to, proposal.kind, &ai.minds())
                == Verdict::Accepted;
        ai.order(
            game,
            Command::Diplomacy(DiplomacyCommand::Answer {
                player: ai.player,
                from: proposal.from,
                accept,
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
