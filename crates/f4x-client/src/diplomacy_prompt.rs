//! What the rival wizards propose to the human at the start of the turn (`f4x_ai::Ai::approach_human`):
//! a pact, an alliance, peace, a war on a third wizard, a spell swap. Each waits in a yes-or-no
//! box over the map until it is answered; one that is left lapses when the turn ends.

use f4x_core::diplomacy::{DiplomacyCommand, Proposal, ProposalKind};
use f4x_core::game::Command;
use f4x_core::player::view::PlayerView;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::view::{App, Screen, HUMAN};

/// The key of the question a proposal asks.
pub(crate) fn question_key(kind: ProposalKind) -> &'static str {
    match kind {
        ProposalKind::Pact => "diplomacy.ai_proposes_pact",
        ProposalKind::Alliance => "diplomacy.ai_proposes_alliance",
        ProposalKind::Peace => "diplomacy.ai_proposes_peace",
        ProposalKind::WarOn(_) => "diplomacy.ai_proposes_war",
        ProposalKind::BreakAllianceWith(_) => "diplomacy.ai_proposes_break",
        ProposalKind::Exchange { .. } => "diplomacy.ai_proposes_exchange",
        ProposalKind::Demand { .. } => "diplomacy.ai_demands",
        // A threat is only ever made by the human.
        ProposalKind::Threat => "diplomacy.refused_threat",
    }
}

impl App {
    /// The first proposal a rival has made to the human that waits for an answer.
    fn pending_proposal(&self) -> Option<Proposal> {
        PlayerView::new(&self.game, HUMAN)
            .proposals_to_me()
            .into_iter()
            .next()
    }

    /// Whether a proposal waits and nothing else is in the way.
    pub(crate) fn proposal_waits(&self) -> bool {
        !self.game.in_battle()
            && matches!(self.screen, Screen::Map)
            && self.pending_proposal().is_some()
    }

    /// Ask the human about the first waiting proposal.
    pub(crate) fn proposal_frame(&mut self) -> bool {
        let Some(proposal) = self.pending_proposal() else {
            return false;
        };
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        let lay = crate::frame::Layout::new();
        let mut args = Args::new().text("wizard", self.diplo_name(proposal.from));
        match proposal.kind {
            ProposalKind::WarOn(third) | ProposalKind::BreakAllianceWith(third) => {
                args = args.text("third", self.diplo_name(third));
            }
            ProposalKind::Exchange { give, get } => {
                let spells = &self.game.content().rules.spells;
                let (give, get) = (spells.get(give).name.clone(), spells.get(get).name.clone());
                args = args.text("give", &give).text("get", &get);
            }
            ProposalKind::Demand { gold } => args = args.count("gold", gold),
            _ => {}
        }
        let text = self.text_with(question_key(proposal.kind), &args);
        if let Some(accept) = self.confirmation_box(&lay, &text) {
            let events = self
                .game
                .apply(Command::Diplomacy(DiplomacyCommand::Answer {
                    player: HUMAN,
                    from: proposal.from,
                    accept,
                }));
            self.note(events);
        }
        true
    }
}
