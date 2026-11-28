//! The swap of world maps (`docs/ai-diplomacy-b.md`): a wizard takes it unless it knows far more
//! of the world than the other, and offers it to a partner whose map would show it enough.
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

/// Whether `judge` swaps maps with `other`: the other's map shows it something new, and it has not
/// explored `map_refuse_pct` percent of what the other has, or more.
pub(crate) fn wanted(world: &World, judge: PlayerId, other: PlayerId) -> bool {
    let k = world.levers().diplomacy_b;
    let percent = usize::try_from(k.map_refuse_pct).unwrap_or(0);
    world.map_news(other, judge) > 0
        && world.explored_squares(judge) * 100 < world.explored_squares(other) * percent
}

/// Offer `other` a swap of maps, if one is due and worth it. Whether it did.
pub(crate) fn offer(ai: &Ai, game: &mut Game, events: &mut Vec<Event>, other: PlayerId) -> bool {
    let me = ai.player;
    let k = game.levers().diplomacy_b;
    let kind = ProposalKind::MapExchange;
    let due = ai
        .memory
        .get()
        .due(other, Did::Maps, game.round(), k.map_cool);
    if !due
        || game.check_proposal(me, other, kind).is_err()
        || game.map_news(other, me) < k.map_least_news
        || !wanted(game, me, other)
    {
        return false;
    }
    let made = propose(ai, game, events, other, kind);
    if made {
        ai.memory.put().did(other, Did::Maps, game.round());
    }
    made
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplo_b_stance::tests::pair;
    use crate::diplo_b_talk::approach;
    use crate::testworld::{at, levers, P0, P1};
    use f4x_core::diplomacy::{DiplomacyCommand, DiplomacyEvent, Treaty};
    use f4x_core::map::{Plane, Position};
    use f4x_core::world::Command;
    use f4x_def::half_int::HalfInt;

    /// `P0` and `P1`, bound by a pact, under the expanded diplomacy options; `P1` is the human.
    fn partners() -> Game {
        let mut w = pair(8, 8, 100);
        w.set_human(Some(P1));
        w.scenario_change_settings(|s| s.rules.expanded_diplomacy = true);
        w.start_treaty(P0, P1, Treaty::Pact);
        w
    }

    /// Let `who` explore all of Myrror, a plane neither of the two has seen yet.
    fn travelled(w: &mut Game, who: PlayerId) {
        let at = Position {
            plane: Plane::Myrror,
            ..at(0, 0)
        };
        w.explore(who, at, HalfInt::from_int(200));
    }

    /// Whether `P0` takes the swap `P1` offers it.
    fn takes(mut w: Game) -> bool {
        let ai = Ai::new(P0);
        w.command(Command::Diplomacy(DiplomacyCommand::Propose {
            from: P1,
            to: P0,
            kind: ProposalKind::MapExchange,
        }));
        assert_eq!(w.diplomacy.proposals().len(), 1, "the offer is made");
        crate::answer_proposals(&mut w, P0, &ai.minds());
        w.map_news(P1, P0) == 0
    }

    #[test]
    fn a_wizard_swaps_maps_with_one_that_knows_much_less_but_not_far_less() {
        // Both have explored Myrror: each knows about as much, the other a little new.
        let mut w = partners();
        travelled(&mut w, P0);
        travelled(&mut w, P1);
        w.explore(P1, at(25, 6), HalfInt::from_int(2));
        assert!(w.map_news(P1, P0) > 0, "something to learn");
        assert!(takes(w), "about as much");
        // Only the judge has: it knows more than three times as much.
        let mut w = partners();
        travelled(&mut w, P0);
        w.explore(P1, at(25, 6), HalfInt::from_int(2));
        let percent = levers().diplomacy_b.map_refuse_pct as usize;
        assert!(w.explored_squares(P0) * 100 >= w.explored_squares(P1) * percent);
        assert!(w.map_news(P1, P0) > 0, "something to learn");
        assert!(!takes(w), "far more");
    }

    #[test]
    fn a_wizard_turns_down_a_map_that_shows_it_nothing() {
        let mut w = partners();
        travelled(&mut w, P0);
        travelled(&mut w, P1);
        for command in [
            DiplomacyCommand::Propose {
                from: P1,
                to: P0,
                kind: ProposalKind::MapExchange,
            },
            DiplomacyCommand::Answer {
                player: P0,
                from: P1,
                accept: true,
            },
        ] {
            w.command(Command::Diplomacy(command));
        }
        assert_eq!(w.map_news(P1, P0), 0);
        assert!(!wanted(&w, P0, P1));
    }

    /// The offers of a swap `P0` makes when it looks at the others, and its turn ends.
    fn offers(ai: &Ai, w: &mut Game) -> usize {
        let mut events = Vec::new();
        approach(ai, w, &mut events);
        w.end_player_turn(P0);
        events
            .iter()
            .filter(|e| {
                matches!(e, Event::Diplomacy(DiplomacyEvent::ProposalMade(p))
                    if p.from == P0 && p.kind == ProposalKind::MapExchange)
            })
            .count()
    }

    #[test]
    fn a_wizard_offers_a_swap_worth_having_and_not_again_for_a_while() {
        let ai = Ai::new(P0);
        let mut w = partners();
        travelled(&mut w, P0);
        travelled(&mut w, P1);
        w.explore(P1, at(25, 6), HalfInt::from_int(6));
        assert!(w.map_news(P1, P0) >= levers().diplomacy_b.map_least_news);
        assert_eq!(offers(&ai, &mut w), 1);
        assert_eq!(offers(&ai, &mut w), 0, "at once again");
        let later = w.round() + levers().diplomacy_b.map_cool;
        w.set_round(later);
        assert_eq!(offers(&ai, &mut w), 1, "after the cool-down");
    }

    #[test]
    fn a_wizard_offers_no_swap_without_the_expanded_options() {
        let ai = Ai::new(P0);
        let mut w = partners();
        w.scenario_change_settings(|s| s.rules.expanded_diplomacy = false);
        travelled(&mut w, P0);
        travelled(&mut w, P1);
        w.explore(P1, at(25, 6), HalfInt::from_int(6));
        assert_eq!(offers(&ai, &mut w), 0);
    }
}
