//! Trade and research pacts (`docs/ai-diplomacy-b.md`): both sides gain, so a computer player
//! takes either from a treaty partner (`diplo_b_talk::rate`) and offers what the two lack.
//!
//! Tests: below.

use f4x_core::diplomacy::ProposalKind;
use f4x_core::player::PlayerId;
use f4x_core::world::{Event, World};

use crate::lever::Levered;
use crate::memory::Did;
use crate::statecraft::propose;
use crate::Ai;
use f4x_master::Game;

/// The pact `me` would offer `other` next: research before trade.
fn lacking(world: &World, me: PlayerId, other: PlayerId) -> Option<ProposalKind> {
    [ProposalKind::ResearchPact, ProposalKind::TradePact]
        .into_iter()
        .find(|&kind| world.check_proposal(me, other, kind).is_ok())
}

/// Offer `other` a pact the two lack, if one is due. Whether it did.
pub(crate) fn offer(ai: &Ai, game: &mut Game, events: &mut Vec<Event>, other: PlayerId) -> bool {
    let cool = game.levers().diplomacy_b.pact_cool;
    let due = ai.memory.get().due(other, Did::Pacts, game.round(), cool);
    let Some(kind) = lacking(game, ai.player, other).filter(|_| due) else {
        return false;
    };
    let made = propose(ai, game, events, other, kind);
    if made {
        ai.memory.put().did(other, Did::Pacts, game.round());
    }
    made
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplo_b_stance::tests::pair;
    use crate::diplo_b_talk::approach;
    use crate::testworld::at;
    use crate::testworld::{levers, P0, P1};
    use f4x_core::diplomacy::{DiplomacyCommand, Treaty};
    use f4x_core::map::{Plane, Position};
    use f4x_core::world::Command;
    use f4x_def::half_int::HalfInt;

    /// `P0` and `P1`, bound by a pact, under the expanded diplomacy options; `P1` is the human.
    /// Both know the whole world, so neither has a map to offer.
    fn partners() -> Game {
        let mut w = pair(8, 8, 100);
        w.set_human(Some(P1));
        w.scenario_change_settings(|s| s.rules.expanded_diplomacy = true);
        w.start_treaty(P0, P1, Treaty::Pact);
        for who in [P0, P1] {
            for plane in Plane::all() {
                let from = Position { plane, ..at(0, 0) };
                w.explore(who, from, HalfInt::from_int(200));
            }
        }
        w
    }

    fn held(w: &World, kind: ProposalKind) -> bool {
        match kind {
            ProposalKind::TradePact => w.diplomacy.has_trade_pact(P0, P1),
            _ => w.diplomacy.has_research_pact(P0, P1),
        }
    }

    #[test]
    fn a_wizard_takes_a_trade_or_a_research_pact_from_a_partner() {
        for kind in [ProposalKind::TradePact, ProposalKind::ResearchPact] {
            let mut w = partners();
            let ai = Ai::new(P0);
            w.command(Command::Diplomacy(DiplomacyCommand::Propose {
                from: P1,
                to: P0,
                kind,
            }));
            crate::answer_proposals(&mut w, P0, &ai.minds());
            assert!(held(&w, kind), "{kind:?}");
        }
    }

    /// The pacts `P0` offers when it looks at the others, each accepted before its turn ends.
    fn offered(ai: &Ai, w: &mut Game) -> Vec<ProposalKind> {
        let mut events = Vec::new();
        approach(ai, w, &mut events);
        let made: Vec<ProposalKind> = w
            .diplomacy
            .proposals()
            .iter()
            .filter(|p| p.from == P0)
            .map(|p| p.kind)
            .filter(|k| matches!(k, ProposalKind::TradePact | ProposalKind::ResearchPact))
            .collect();
        for &kind in &made {
            w.command(Command::Diplomacy(DiplomacyCommand::Answer {
                player: P1,
                from: P0,
                accept: true,
            }));
            assert!(held(w, kind), "{kind:?}");
        }
        w.end_player_turn(P0);
        made
    }

    #[test]
    fn a_wizard_offers_first_research_and_after_a_while_trade() {
        let ai = Ai::new(P0);
        let mut w = partners();
        assert_eq!(offered(&ai, &mut w), [ProposalKind::ResearchPact]);
        assert_eq!(offered(&ai, &mut w), [], "at once again");
        let wait = levers().diplomacy_b.pact_cool;
        w.set_round(w.round() + wait);
        assert_eq!(offered(&ai, &mut w), [ProposalKind::TradePact]);
        w.set_round(w.round() + wait);
        assert_eq!(offered(&ai, &mut w), [], "both are held");
    }

    #[test]
    fn a_wizard_offers_no_pact_without_the_expanded_options() {
        let ai = Ai::new(P0);
        let mut w = partners();
        w.scenario_change_settings(|s| s.rules.expanded_diplomacy = false);
        assert_eq!(offered(&ai, &mut w), []);
    }
}
