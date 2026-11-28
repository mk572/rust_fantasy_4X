//! The price of a treaty (`docs/ai-diplomacy-b.md`): what a wizard answers to a pact, an alliance or
//! a peace, with or without a price. How many points it is short of accepting
//! (`diplo_b_talk::shortfall`) is worth gold: short of it, it names a price; with points to spare, it
//! pays up to their worth. Too far short, it refuses whatever is offered.
//!
//! Tests: in `diplo_b_talk.rs`.

use crate::lever::Levered;
use f4x_core::diplomacy::trade as core;
use f4x_core::diplomacy::{Deal, Goods, Proposal, ProposalKind};

use crate::memory::Diplomats;
use crate::trade::{arms, payment, spendable, will_give, Reply};
use f4x_master::Game;

/// The reply of the receiver of `proposal`, which is about `deal`.
pub(crate) fn respond(game: &Game, minds: &Diplomats, proposal: Proposal, deal: Deal) -> Reply {
    let (judge, asker) = (proposal.to, proposal.from);
    let k = game.levers().diplomacy_b;
    let short = crate::diplo_b_talk::shortfall(game, asker, judge, deal, minds);
    if short > k.counter_reach {
        return Reply::Reject;
    }
    // What the judge must still get, in gold: what is below 0 it can pay.
    let required = short * k.gold_per_point;
    let (offered, asked) = match proposal.kind {
        ProposalKind::Priced { give, get, .. } => (give, get),
        _ => (Goods::NONE, Goods::NONE),
    };
    let parts_with = !arms(game, judge, asker, asked) && will_give(game, judge, asked);
    let net = core::worth(game, judge, offered) - core::worth(game, judge, asked);
    if parts_with && net >= required {
        return Reply::Accept;
    }
    if proposal.round + 1 >= game.content().rules.config.diplomacy.max_rounds {
        return Reply::Reject;
    }
    let least = game.levers().trade.least_price;
    let price = if required > 0 {
        payment(game, judge, asker, required.max(least), |g| {
            will_give(game, asker, g)
        })
        .map(|get| (Goods::NONE, get))
    } else {
        let gold = (-required).min(spendable(game, judge));
        (gold >= least).then(|| (Goods::gold(gold), Goods::NONE))
    };
    price
        .map(|(give, get)| ProposalKind::Priced { deal, give, get })
        .filter(|&kind| game.check_proposal(judge, asker, kind).is_ok())
        .map_or(Reply::Reject, Reply::Counter)
}
