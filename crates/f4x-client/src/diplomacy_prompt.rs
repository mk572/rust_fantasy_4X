//! What the rival wizards propose to the human at the start of the turn (`f4x_ai::Ai::approach_human`):
//! a pact, an alliance, peace, a war on a third wizard, a spell swap. Each waits in a yes-or-no
//! box over the map until it is answered; one that is left lapses when the turn ends.

use crate::input::Input;
use f4x_core::diplomacy::{DiplomacyCommand, Proposal, ProposalKind};
use f4x_core::game::Command;
use f4x_core::player::view::PlayerView;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::modal::{keep, kind, Modal};
use crate::view::{App, Screen, HUMAN};

/// The box that asks about the first proposal waiting in the game; the proposals themselves
/// are the game's state, so the box holds nothing.
pub(crate) struct ProposalAsk;

impl Modal for ProposalAsk {
    fn kind(&self) -> &'static str {
        kind::PROPOSAL
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.proposal_frame(input).then_some(ProposalAsk))
    }
}

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

    /// Open the box for a proposal that waits, over the map.
    pub(crate) fn queue_proposal(&mut self) {
        if !self.game.in_battle()
            && matches!(self.screen, Screen::Map)
            && !self.modals.has(kind::PROPOSAL)
            && self.pending_proposal().is_some()
        {
            self.modals.enqueue(ProposalAsk);
        }
    }

    /// Ask the human about the first waiting proposal; false once none waits.
    fn proposal_frame(&mut self, input: &Input) -> bool {
        let Some(proposal) = self.pending_proposal() else {
            return false;
        };
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
        if let Some(accept) = self.confirmation_box(&lay, &text, input) {
            let events = self
                .game
                .apply(Command::Diplomacy(DiplomacyCommand::Answer {
                    player: HUMAN,
                    from: proposal.from,
                    accept,
                }));
            self.note(events);
        }
        self.pending_proposal().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_app::shipped;
    use f4x_core::player::PlayerId;
    use f4x_core::testdir::TestResult;

    #[test]
    fn every_kind_of_proposal_asks_a_question_that_the_texts_have() -> TestResult {
        let spell = shipped::content()
            .rules
            .spells
            .id_of("fire_bolt")
            .ok_or("fire_bolt is not in the content")?;
        let kinds = [
            ProposalKind::Pact,
            ProposalKind::Alliance,
            ProposalKind::Peace,
            ProposalKind::WarOn(PlayerId(2)),
            ProposalKind::BreakAllianceWith(PlayerId(2)),
            ProposalKind::Exchange {
                give: spell,
                get: spell,
            },
            ProposalKind::Demand { gold: 50 },
            ProposalKind::Threat,
        ];
        for kind in kinds {
            let key = question_key(kind);
            assert!(
                shipped::texts().has(key),
                "{kind:?} asks {key}, which is missing"
            );
        }
        Ok(())
    }
}
