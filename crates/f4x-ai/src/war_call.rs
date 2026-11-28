//! The call to arms (`docs/ai-diplomacy-b.md`): a wizard at war asks its ally to join. The ally
//! joins, ends the alliance, or names a price for joining; the caller pays it or not, and an ally
//! whose price is not paid chooses between the other two.
//!
//! Tests: below.

use crate::lever::Levered;
use f4x_core::diplomacy::trade as core;
use f4x_core::diplomacy::{Goods, Proposal, ProposalKind, Treaty};
use f4x_core::player::PlayerId;
use f4x_core::world::{Event, World};

use crate::diplo_b::regard;
use crate::memory::{Did, Diplomats};
use crate::statecraft::{propose, Scene};
use crate::trade::{arms, payment, spendable, will_give, Reply};
use crate::Ai;
use f4x_master::Game;

/// The reply of the ally to `call`, the call to join the war on `third`. It joins when the war
/// is worth it to it (`war_accept`); a little short of that it names its price; else, and when its
/// price was not paid, it joins for a friend it would still ally with and refuses anyone else. It
/// never joins against a wizard much stronger than it is.
pub(crate) fn respond(game: &Game, minds: &Diplomats, call: Proposal, third: PlayerId) -> Reply {
    let (judge, caller) = (call.to, call.from);
    let k = game.levers().diplomacy_b;
    let support = crate::diplo_b_talk::war_support(game, judge, caller, third, minds);
    let Some(support) = support else {
        return Reply::Reject;
    };
    if support >= k.war_accept {
        return Reply::Accept;
    }
    let short = k.war_accept - support;
    if call.round == 0 && short <= k.counter_reach {
        let need = (short * k.gold_per_point).max(game.levers().trade.least_price);
        let price = payment(game, judge, caller, need, |g| will_give(game, caller, g))
            .map(|price| ProposalKind::WarPrice { third, price })
            .filter(|&kind| game.check_proposal(judge, caller, kind).is_ok());
        if let Some(price) = price {
            return Reply::Counter(price);
        }
    }
    let scene = Scene::new(game, judge);
    let memory = minds.of(judge);
    let friendship = regard(game, judge, caller, &scene, &memory, game.round()).friendship;
    if friendship >= game.content().rules.config.diplomacy.alliance_relation {
        Reply::Accept
    } else {
        Reply::Reject
    }
}

/// Whether `caller` pays `price` for `ally` to join its war: what it will part with, out of the
/// gold it can spend, and worth no more to it than the most a computer player asks.
pub(crate) fn pays(world: &World, caller: PlayerId, ally: PlayerId, price: Goods) -> bool {
    let k = world.levers().diplomacy_b;
    !arms(world, caller, ally, price)
        && will_give(world, caller, price)
        && price.gold <= spendable(world, caller)
        && core::worth(world, caller, price) <= k.counter_reach * k.gold_per_point
}

/// Call `ally` to a war the wizard is in and the ally is not, if one is due: against the strongest
/// such enemy that is a match for the wizard (`call_enemy_pct` of its power). Whether it did.
pub(crate) fn ask(ai: &Ai, game: &mut Game, events: &mut Vec<Event>, ally: PlayerId) -> bool {
    let me = ai.player;
    let k = game.levers().diplomacy_b;
    if !ai
        .memory
        .get()
        .due(ally, Did::Call, game.round(), k.call_cool)
    {
        return false;
    }
    let enemy = {
        let scene = Scene::new(game, me);
        let weights = game.levers().diplomacy;
        let mine = scene.power(game, me).score(&weights);
        scene
            .others
            .iter()
            .filter(|&&t| game.treaty(me, t) == Treaty::War)
            .filter(|&&t| {
                game.check_proposal(me, ally, ProposalKind::WarOn(t))
                    .is_ok()
            })
            .map(|&t| (scene.power(game, t).score(&weights), t))
            .filter(|&(power, _)| power * 100 >= mine * k.call_enemy_pct)
            .max()
            .map(|(_, t)| t)
    };
    let called = enemy.is_some_and(|t| propose(ai, game, events, ally, ProposalKind::WarOn(t)));
    if called {
        ai.memory.put().did(ally, Did::Call, game.round());
    }
    called
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplo_b_stance::tests::pair;
    use crate::diplo_b_talk::{approach, rate};
    use crate::diplomacy::Verdict;
    use crate::testworld::{at, levers, spawn, P0, P1};
    use f4x_core::diplomacy::{DiplomacyCommand, DiplomacyEvent};
    use f4x_core::rules::RealmId;
    use f4x_core::world::Command;
    use f4x_def::half_int::HalfInt;

    /// The wizard a call is against.
    const P2: PlayerId = PlayerId(2);

    /// `P0` and `P1` are allies, one of them (`caller`) at war with a third wizard of `army` units;
    /// `P1` is the human and both have gold.
    fn war(caller: PlayerId, army: usize) -> Game {
        let mut w = pair(8, 8, 100);
        w.set_human(Some(P1));
        w.add_player(P2, "North");
        w.init_wizard(P2, &[(RealmId::new(3 /* life */), 3)], &[], None)
            .expect("a wizard");
        spawn(&mut w, P2, at(14, 4), 95 /* high_elf_spearmen */, army);
        for p in [P0, P1] {
            w.make_contact(p, P2);
            w.explore(p, at(14, 4), HalfInt::from_raw(3));
            w.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(10_000);
        }
        w.start_treaty(P0, P1, Treaty::Alliance);
        w.declare_war(caller, P2);
        w
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Answered {
        Broke,
        Priced,
        Joined,
    }

    /// What `P0`, feeling `friendship` for `P1`, makes of its call; with `paid` false, what it does
    /// when its price is turned down.
    fn called(friendship: i32, army: usize) -> (Game, Ai, Answered) {
        let ai = Ai::new(P0);
        let mut w = war(P1, army);
        ai.memory.put().set_friendship(&w, P0, P1, friendship);
        w.command(Command::Diplomacy(DiplomacyCommand::Propose {
            from: P1,
            to: P0,
            kind: ProposalKind::WarOn(P2),
        }));
        crate::answer_proposals(&mut w, P0, &ai.minds());
        let answered = if !w.diplomacy.proposals().is_empty() {
            Answered::Priced
        } else if w.treaty(P0, P2) == Treaty::War {
            assert_eq!(w.treaty(P0, P1), Treaty::Alliance);
            Answered::Joined
        } else {
            assert_eq!(w.treaty(P0, P1), Treaty::None);
            Answered::Broke
        };
        (w, ai, answered)
    }

    #[test]
    fn an_ally_refuses_names_a_price_or_joins_by_its_friendship() {
        let seen: Vec<Answered> = (-100..=100).step_by(5).map(|f| called(f, 8).2).collect();
        for answer in [Answered::Broke, Answered::Priced, Answered::Joined] {
            assert!(seen.contains(&answer), "never {answer:?}: {seen:?}");
        }
        assert!(
            seen.windows(2).all(|w| w[0] <= w[1]),
            "a better friend is never less willing: {seen:?}"
        );
    }

    #[test]
    fn an_ally_whose_price_is_not_paid_joins_or_ends_the_alliance_and_one_paid_joins() {
        let mut priced = 0;
        for friendship in (-100..=100).step_by(5) {
            for pay in [false, true] {
                let (mut w, ai, answered) = called(friendship, 8);
                if answered != Answered::Priced {
                    continue;
                }
                priced += 1;
                w.command(Command::Diplomacy(DiplomacyCommand::Answer {
                    player: P1,
                    from: P0,
                    accept: pay,
                }));
                crate::answer_proposals(&mut w, P0, &ai.minds());
                assert_eq!(w.diplomacy.proposals(), [], "{friendship} {pay}");
                let joined = w.treaty(P0, P2) == Treaty::War;
                let allied = w.treaty(P0, P1) == Treaty::Alliance;
                assert_eq!(joined, allied, "{friendship} {pay}: one or the other");
                assert!(joined || !pay, "{friendship}: paid and stayed out");
            }
        }
        assert!(priced > 0, "never a price");
    }

    #[test]
    fn an_ally_never_joins_against_a_wizard_much_stronger_than_it_is() {
        assert_eq!(called(100, 200).2, Answered::Broke);
    }

    #[test]
    fn a_caller_pays_a_small_price_and_not_a_huge_one() {
        let ai = Ai::new(P0);
        let k = levers().diplomacy_b;
        let most = k.counter_reach * k.gold_per_point;
        let mut w = war(P0, 8);
        let mut asked = |gold: i32| {
            let price = ProposalKind::WarPrice {
                third: P2,
                price: Goods::gold(gold),
            };
            rate(&mut w, P1, P0, price, &ai.minds())
        };
        assert_eq!(asked(1), Verdict::Accepted);
        assert_eq!(asked(most + 1), Verdict::Refused);
    }

    /// The calls to arms `P0` sends `P1` when it looks at the others, and its turn ends.
    fn calls(ai: &Ai, w: &mut Game) -> usize {
        let mut events = Vec::new();
        approach(ai, w, &mut events);
        w.end_player_turn(P0);
        events
            .iter()
            .filter(|e| {
                matches!(e, Event::Diplomacy(DiplomacyEvent::ProposalMade(p))
                    if p.from == P0 && p.to == P1 && p.kind == ProposalKind::WarOn(P2))
            })
            .count()
    }

    #[test]
    fn a_wizard_at_war_calls_its_ally_and_not_again_for_a_while() {
        let ai = Ai::new(P0);
        let mut w = war(P0, 60);
        assert_eq!(calls(&ai, &mut w), 1);
        assert_eq!(calls(&ai, &mut w), 0, "at once again");
        let later = w.round() + levers().diplomacy_b.call_cool;
        w.set_round(later);
        assert_eq!(calls(&ai, &mut w), 1, "after the cool-down");
    }

    #[test]
    fn a_wizard_calls_no_help_against_an_enemy_far_weaker_than_it_is() {
        let ai = Ai::new(P0);
        let mut w = war(P0, 1);
        assert_eq!(calls(&ai, &mut w), 0);
    }
}
