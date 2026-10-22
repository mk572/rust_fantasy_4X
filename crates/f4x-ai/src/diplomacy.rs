//! The AI's side of the talk. The core keeps the relations and treaties and only checks that a
//! proposal is legal; whether to accept it is decided here, with the original's rating of a
//! proposal (`Game::rate_proposal`, docs/diplomacy.md section 4) as the starting logic.

use f4x_core::diplomacy::{DiplomacyCommand, Verdict};
use f4x_core::game::{Command, Event, Game};
use f4x_core::player::view::PlayerView;

use crate::Ai;

/// Answer every proposal other wizards have made to this one. A proposal nobody answers lapses
/// as refused when the turn ends.
pub(crate) fn answer(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let pending = PlayerView::new(game, ai.player).proposals_to_me();
    for proposal in pending {
        let accept =
            game.rate_proposal(proposal.from, proposal.to, proposal.kind) == Verdict::Accepted;
        ai.send(
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

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::diplomacy::{DiplomacyEvent, ProposalKind};
    use f4x_core::player::Player;
    use f4x_core::player::PlayerId;

    const HUMAN: PlayerId = PlayerId(0);
    const AI: PlayerId = PlayerId(1);

    fn game() -> Game {
        let mut g = Game::new(5);
        for id in [HUMAN, AI] {
            g.add_player(Player::new(id, "W"));
        }
        g.human = Some(HUMAN);
        g.make_contact(HUMAN, AI);
        g
    }

    #[test]
    fn a_proposal_made_to_the_ai_is_answered() {
        let mut g = game();
        g.apply(Command::Diplomacy(DiplomacyCommand::Propose {
            from: HUMAN,
            to: AI,
            kind: ProposalKind::Pact,
        }));
        let mut events = Vec::new();
        answer(&Ai::new(AI), &mut g, &mut events);
        assert!(g.diplomacy.proposals().is_empty());
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Diplomacy(DiplomacyEvent::ProposalAnswered { lapsed: false, .. })
        )));
    }

    #[test]
    fn proposals_to_others_are_left_alone() {
        let mut g = game();
        g.add_player(Player::new(PlayerId(2), "W"));
        g.make_contact(HUMAN, PlayerId(2));
        g.apply(Command::Diplomacy(DiplomacyCommand::Propose {
            from: HUMAN,
            to: PlayerId(2),
            kind: ProposalKind::Pact,
        }));
        let mut events = Vec::new();
        answer(&Ai::new(AI), &mut g, &mut events);
        assert_eq!(g.diplomacy.proposals().len(), 1);
        assert!(events.is_empty());
    }

    #[test]
    fn a_friendly_wizard_accepts_a_pact_and_a_hostile_one_does_not() {
        let mut yes = 0;
        let mut no = 0;
        for seed in 0..40 {
            let mut g = Game::new(seed);
            for id in [HUMAN, AI] {
                g.add_player(Player::new(id, "W"));
            }
            g.human = Some(HUMAN);
            g.make_contact(HUMAN, AI);
            // Friends: the human has done well by the AI, and the AI is easy-going.
            for _ in 0..8 {
                g.change_relations(8, HUMAN, AI, f4x_core::diplomacy::Action::Friendship, None);
            }
            g.diplomacy
                .set_personality(AI, f4x_core::diplomacy::Personality::Peaceful);
            if g.rate_proposal(HUMAN, AI, ProposalKind::Pact) == Verdict::Accepted {
                yes += 1;
            }
            let mut h = Game::new(seed);
            for id in [HUMAN, AI] {
                h.add_player(Player::new(id, "W"));
            }
            h.human = Some(HUMAN);
            h.make_contact(HUMAN, AI);
            for _ in 0..8 {
                h.change_relations(
                    -8,
                    HUMAN,
                    AI,
                    f4x_core::diplomacy::Action::AttackedCity,
                    None,
                );
            }
            if h.rate_proposal(HUMAN, AI, ProposalKind::Pact) == Verdict::Accepted {
                no += 1;
            }
        }
        assert_eq!(
            no, 0,
            "a hostile wizard refuses (relation is below the pact threshold)"
        );
        assert!(yes > 0, "a friendly one sometimes agrees");
    }
}
