//! The trades the AI answers and offers (tasks 504 and 597), by worth.
//!
//! The worth of a thing is the core's (`f4x_core::diplomacy::trade`): one gold, one research
//! point, one production and one mana are the same, and a modifier says what the thing is worth
//! to the one who would hold it (a spell by the books of its realm, an item by the heroes that
//! could carry it, a unit by the war its holder fights). The AI judges a trade as the one who
//! gives and gets: it accepts when what it gets, at its own worth, is at least the *margin* of
//! what it gives. The margin is lower for friends: a pact, an alliance and a good relation each
//! ease it, so that two allies accept a fair trade and a stranger wants a profit. A thing that
//! would arm an enemy is never given ([`crate::statecraft::arms_enemy`]).
//!
//! What it offers: a spell swap, a spell for gold either way, and an item or a unit for gold,
//! each by the price the other would accept ([`price`]), picking the offer that gains it most.

use f4x_core::diplomacy::trade as core;
use f4x_core::diplomacy::{Goods, Proposal, ProposalKind, Treaty, MAX_ROUNDS};
use f4x_core::game::Game;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::rules::SpellId;

use crate::diplomacy::Verdict;
use crate::statecraft::{arms_enemy, goodwill};

/// The numbers of a trade, in one place (`AIK_DIPLO_<NAME>` for a soak).
pub(crate) struct Knobs {
    /// What a trade must bring, in percent of what it costs, between strangers, with a pact and
    /// with an ally.
    pub margin_none: i32,
    pub margin_pact: i32,
    pub margin_alliance: i32,
    /// Percent of margin less for every ten points of relation above zero (more below), and for
    /// every ten points of goodwill (`statecraft::goodwill`).
    pub margin_per_relation: i32,
    pub margin_per_goodwill: i32,
    /// The least and most the margin can be.
    pub margin_least: i32,
    pub margin_most: i32,
    /// Gold the AI keeps back when it buys.
    pub reserve: i32,
    /// The gold it will not sell a spell or an item for less than.
    pub least_price: i32,
    /// The switch of the whole part (1 on, 0 off), for a soak.
    pub on: bool,
}

impl Knobs {
    pub(crate) fn current() -> Self {
        let k =
            |name: &str, default: i32| crate::knob(&format!("DIPLO_{name}"), default as f32) as i32;
        Self {
            margin_none: k("TRADE_MARGIN_NONE", 98),
            margin_pact: k("TRADE_MARGIN_PACT", 95),
            margin_alliance: k("TRADE_MARGIN_ALLIANCE", 90),
            margin_per_relation: k("TRADE_MARGIN_PER_RELATION", 1),
            margin_per_goodwill: k("TRADE_MARGIN_PER_GOODWILL", 1),
            margin_least: k("TRADE_MARGIN_LEAST", 85),
            margin_most: k("TRADE_MARGIN_MOST", 140),
            reserve: k("TRADE_RESERVE", 40),
            least_price: k("TRADE_LEAST_PRICE", 40),
            on: k("ON_TRADE", 1) != 0,
        }
    }
}

/// What a trade must bring `judge` in percent of what it gives `other`: the margin.
pub(crate) fn margin(game: &Game, judge: PlayerId, other: PlayerId) -> i32 {
    let k = Knobs::current();
    let base = match game.treaty(judge, other) {
        Treaty::Alliance => k.margin_alliance,
        Treaty::Pact => k.margin_pact,
        _ => k.margin_none,
    };
    let relation = game.relation(judge, other);
    let kind = ProposalKind::Trade {
        give: Goods::NONE,
        get: Goods::NONE,
    };
    let eased = base
        - k.margin_per_relation * relation / 10
        - k.margin_per_goodwill * goodwill(game, judge, other, kind) / 10;
    eased.clamp(k.margin_least, k.margin_most)
}

/// Whether handing `goods` to `to` arms an enemy of `from`: a spell, an item or a unit, not gold.
fn arms(game: &Game, from: PlayerId, to: PlayerId, goods: Goods) -> bool {
    (goods.spell.is_some() || goods.item.is_some() || goods.unit.is_some())
        && arms_enemy(game, from, to)
}

/// What `judge` says to the trade `asker` proposes: `give` is what the asker hands over (the
/// judge gets it), `get` what it asks of the judge.
pub(crate) fn verdict(
    game: &Game,
    judge: PlayerId,
    asker: PlayerId,
    give: Goods,
    get: Goods,
) -> Verdict {
    if !Knobs::current().on {
        return Verdict::Accepted;
    }
    if arms(game, judge, asker, get) || !will_give(game, judge, get) {
        return Verdict::Refused;
    }
    let gets = core::worth(game, judge, give);
    let costs = core::worth(game, judge, get);
    if gets * 100 >= costs * margin(game, judge, asker) {
        Verdict::Accepted
    } else {
        Verdict::Refused
    }
}

/// Whether `judge` is willing to part with the units and the hero of `goods`: not while it is at
/// war, not its last defender of a city and not its last hero.
fn will_give(game: &Game, judge: PlayerId, goods: Goods) -> bool {
    let at_war = game
        .player_ids()
        .into_iter()
        .any(|p| p != judge && game.treaty(judge, p) == Treaty::War);
    let defended = |serial| {
        core::giveable_unit(game, judge, serial)
            .or_else(|| core::giveable_hero(game, judge, serial))
            .and_then(|u| game.units().unit_position(u))
            .is_some_and(|at| game.units().units_at(at).len() > 1)
    };
    let unit_ok = goods.unit.is_none_or(|u| !at_war && defended(u));
    let hero_ok = goods
        .hero
        .is_none_or(|h| !at_war && defended(h) && game.units().hero_units(judge).len() > 1);
    unit_ok && hero_ok
}

/// Every single thing `owner` could hand to `to` (a spell, an item, a unit, a hero), one bundle
/// each, in a stable order; gold is not in the list.
fn holdings(game: &Game, owner: PlayerId, to: PlayerId) -> Vec<Goods> {
    let mut out = Vec::new();
    if let (Some(mine), Some(theirs)) = (game.wizard(owner), game.wizard(to)) {
        out.extend(
            game.content()
                .rules
                .spells
                .iter()
                .filter(|&(id, def)| mine.knows(id) && !theirs.knows(id) && theirs.books_allow(def))
                .map(|(id, _)| Goods::spell(id)),
        );
    }
    let mut serials: Vec<_> = game
        .items()
        .held(|h| {
            matches!(h,
            f4x_core::item::entity::HeldBy::Vault(p, _)
            | f4x_core::item::entity::HeldBy::Award(p) if p == owner)
        })
        .into_iter()
        .map(|(e, _)| game.serial_of(e))
        .collect();
    serials.sort();
    out.extend(serials.into_iter().map(Goods::item));
    let mut units: Vec<_> = game
        .units()
        .player_units(owner)
        .into_iter()
        .filter(|&u| core::giveable_unit(game, owner, game.serial_of(u)).is_some())
        .map(|u| game.serial_of(u))
        .collect();
    units.sort();
    out.extend(units.into_iter().map(Goods::unit));
    let mut heroes: Vec<_> = game
        .units()
        .hero_units(owner)
        .into_iter()
        .filter(|&h| core::giveable_hero(game, owner, game.serial_of(h)).is_some())
        .map(|h| game.serial_of(h))
        .collect();
    heroes.sort();
    out.extend(heroes.into_iter().map(Goods::hero));
    out.retain(|&g| core::can_hand_over(game, owner, to, g));
    out
}

/// What the AI answers to a trade: a deal, a refusal or a trade of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reply {
    Accept,
    Reject,
    Counter(ProposalKind),
}

/// The reply of `judge` to a trade proposal made to it (the negotiation, `MAX_ROUNDS`):
/// - a full offer that clears its margin is accepted;
/// - a short one is countered by asking for one more thing from the asker, or by asking less of
///   itself, whichever it can do and the asker would take;
/// - an open question is answered by naming a price (the asker named only what it wants), or by
///   naming what it would give (the asker named only what it gives);
/// - anything else is refused, and so is every counter past the last round.
pub(crate) fn respond(game: &Game, judge: PlayerId, proposal: Proposal) -> Reply {
    let ProposalKind::Trade { give, get } = proposal.kind else {
        return Reply::Reject;
    };
    let asker = proposal.from;
    if !give.is_empty()
        && !get.is_empty()
        && verdict(game, judge, asker, give, get) == Verdict::Accepted
    {
        return Reply::Accept;
    }
    if proposal.round + 1 >= MAX_ROUNDS
        || !Knobs::current().on
        || arms(game, judge, asker, get)
        || !will_give(game, judge, get)
    {
        return Reply::Reject;
    }
    let counter = if give.is_empty() {
        name_price(game, judge, asker, get)
    } else if get.is_empty() {
        name_offer(game, judge, asker, give)
    } else {
        counter_short(game, judge, asker, give, get)
    };
    // The counter is the judge's own proposal: it gives what the asker asked for.
    counter
        .filter(|&kind| game.check_proposal(judge, asker, kind).is_ok())
        .map_or(Reply::Reject, Reply::Counter)
}

/// Whether the asker, if an AI, would take what the judge counters with; a human decides alone.
fn asker_would_take(
    game: &Game,
    judge: PlayerId,
    asker: PlayerId,
    gives: Goods,
    gets: Goods,
) -> bool {
    Some(asker) == game.human() || verdict(game, asker, judge, gives, gets) == Verdict::Accepted
}

/// A full offer that is a little short: ask for one more category from the asker.
fn counter_short(
    game: &Game,
    judge: PlayerId,
    asker: PlayerId,
    give: Goods,
    get: Goods,
) -> Option<ProposalKind> {
    let need = core::worth(game, judge, get) * margin(game, judge, asker) / 100;
    let clears = |new_give: Goods| core::worth(game, judge, new_give) >= need;
    let ask = holdings(game, asker, judge)
        .into_iter()
        .filter(|c| {
            (c.spell.is_none() || give.spell.is_none())
                && (c.item.is_none() || give.item.is_none())
                && (c.unit.is_none() || give.unit.is_none())
                && (c.hero.is_none() || give.hero.is_none())
        })
        .map(|c| give.with(c))
        .chain({
            // The same bundle with more gold.
            let more = need - core::worth(game, judge, give) + give.gold;
            (more > give.gold && core::can_hand_over(game, asker, judge, Goods::gold(more)))
                .then_some(Goods { gold: more, ..give })
        })
        .filter(|&g| clears(g) && asker_would_take(game, judge, asker, get, g))
        .min_by_key(|&g| core::worth(game, asker, g));
    if let Some(new_give) = ask {
        return Some(ProposalKind::Trade {
            give: get,
            get: new_give,
        });
    }
    // Or ask less of itself: drop a part of what the asker wants.
    let parts = [
        Goods { gold: 0, ..get },
        Goods { spell: None, ..get },
        Goods { item: None, ..get },
        Goods { unit: None, ..get },
        Goods { hero: None, ..get },
    ];
    parts
        .into_iter()
        .filter(|&g| g != get && !g.is_empty())
        .filter(|&g| {
            core::worth(game, judge, give)
                >= core::worth(game, judge, g) * margin(game, judge, asker) / 100
        })
        .filter(|&g| asker_would_take(game, judge, asker, g, give))
        .max_by_key(|&g| core::worth(game, asker, g))
        .map(|less| ProposalKind::Trade {
            give: less,
            get: give,
        })
}

/// The asker wants `get` and names nothing: name a price it can pay, the cheapest for it that
/// the judge takes (gold, one of its things, or a thing and the gold that makes it up).
fn name_price(game: &Game, judge: PlayerId, asker: PlayerId, get: Goods) -> Option<ProposalKind> {
    let k = Knobs::current();
    let need =
        (core::worth(game, judge, get) * margin(game, judge, asker) / 100).max(k.least_price);
    let paid_in = holdings(game, asker, judge)
        .into_iter()
        .map(|c| {
            let short = (need - core::worth(game, judge, c)).max(0);
            Goods { gold: short, ..c }
        })
        .chain(std::iter::once(Goods::gold(need)));
    let price = paid_in
        .filter(|&g| core::can_hand_over(game, asker, judge, g))
        .filter(|&g| core::worth(game, judge, g) >= need)
        .filter(|&g| asker_would_take(game, judge, asker, get, g))
        .min_by_key(|&g| core::worth(game, asker, g))?;
    Some(ProposalKind::Trade {
        give: get,
        get: price,
    })
}

/// The asker gives `give` and names nothing: name what the judge would give for it, the thing
/// worth the most to the asker that costs the judge no more than its margin leaves.
fn name_offer(game: &Game, judge: PlayerId, asker: PlayerId, give: Goods) -> Option<ProposalKind> {
    let k = Knobs::current();
    let budget = core::worth(game, judge, give) * 100 / margin(game, judge, asker);
    let gold = budget.min(spendable(game, judge));
    let cheap = |g: Goods| {
        core::worth(game, judge, g) <= budget
            && !arms(game, judge, asker, g)
            && will_give(game, judge, g)
    };
    let pay = holdings(game, judge, asker)
        .into_iter()
        .filter(|&g| cheap(g))
        .chain((gold >= k.least_price).then(|| Goods::gold(gold)))
        .filter(|&g| core::can_hand_over(game, judge, asker, g))
        .filter(|&g| !(g.is_only_gold() && give.is_only_gold()))
        .filter(|&g| asker_would_take(game, judge, asker, g, give))
        .max_by_key(|&g| core::worth(game, asker, g))?;
    Some(ProposalKind::Trade {
        give: pay,
        get: give,
    })
}

/// The cheapest gold price at which `seller`'s thing is worth selling to `buyer`: the seller's
/// own worth with its margin (a seller is paid more than it values the thing at), and at most
/// what the buyer would pay at its margin. None when the two do not meet.
fn price(game: &Game, seller: PlayerId, buyer: PlayerId, thing: Goods) -> Option<i32> {
    let k = Knobs::current();
    let ask = core::worth(game, seller, thing) * margin(game, seller, buyer) / 100;
    let ask = ask.max(k.least_price);
    let most = core::worth(game, buyer, thing) * 100 / margin(game, buyer, seller);
    (ask <= most).then_some(ask)
}

/// The gold `me` can spend on a purchase.
fn spendable(game: &Game, me: PlayerId) -> i32 {
    let gold = game.player(me).map_or(0, |p| p.treasury().gold.floor());
    (gold - Knobs::current().reserve).max(0)
}

/// One offer and what it brings the one who makes it, in gold.
struct Offer {
    give: Goods,
    get: Goods,
    gain: i32,
}

/// A spell swap: of every pair of a spell `me` could give and one it could get, the one that
/// leaves the less content of the two the most content (each side judges the worth to itself);
/// the swap must clear both margins.
fn swap(game: &Game, me: PlayerId, other: PlayerId) -> Option<Offer> {
    let (mine, theirs) = (game.wizard(me)?, game.wizard(other)?);
    let spells = &game.content().rules.spells;
    let gives: Vec<_> = spells
        .iter()
        .filter(|&(id, def)| mine.knows(id) && !theirs.knows(id) && theirs.books_allow(def))
        .map(|(id, def)| (id, def.num))
        .collect();
    let gets: Vec<_> = spells
        .iter()
        .filter(|&(id, def)| theirs.knows(id) && !mine.knows(id) && mine.books_allow(def))
        .map(|(id, def)| (id, def.num))
        .collect();
    // How well a pair does for the less content side, in percent of what that side gives up.
    let content = |give: SpellId, get: SpellId| {
        let mine =
            core::spell_worth(game, me, get) * 100 / core::spell_worth(game, me, give).max(1);
        let theirs =
            core::spell_worth(game, other, give) * 100 / core::spell_worth(game, other, get).max(1);
        mine.min(theirs)
    };
    let (give, get) = gives
        .iter()
        .flat_map(|&g| gets.iter().map(move |&t| (g, t)))
        .max_by_key(|&((give, gn), (get, tn))| (content(give, get), std::cmp::Reverse((gn, tn))))
        .map(|((give, _), (get, _))| (give, get))?;
    let offer = Offer {
        give: Goods::spell(give),
        get: Goods::spell(get),
        gain: 0,
    };
    meets(game, me, other, offer)
}

/// The offer with its gain for `me`, when both `me` and `other` would accept it.
fn meets(game: &Game, me: PlayerId, other: PlayerId, offer: Offer) -> Option<Offer> {
    let (gets, costs) = (
        core::worth(game, me, offer.get),
        core::worth(game, me, offer.give),
    );
    let ok_me = gets * 100 >= costs * margin(game, me, other);
    let ok_other = verdict(game, other, me, offer.give, offer.get) == Verdict::Accepted;
    (ok_me
        && ok_other
        && core::can_hand_over(game, me, other, offer.give)
        && core::can_hand_over(game, other, me, offer.get)
        && !arms(game, me, other, offer.give))
    .then_some(Offer {
        gain: gets - costs,
        ..offer
    })
}

/// Buy a spell `other` knows with gold.
fn buy_spell(game: &Game, me: PlayerId, other: PlayerId) -> Option<Offer> {
    let (mine, theirs) = (game.wizard(me)?, game.wizard(other)?);
    let budget = spendable(game, me);
    game.content()
        .rules
        .spells
        .iter()
        .filter(|&(id, def)| theirs.knows(id) && !mine.knows(id) && mine.books_allow(def))
        .filter_map(|(id, _)| {
            let thing = Goods::spell(id);
            let gold = price(game, other, me, thing)?;
            (gold <= budget).then(|| Offer {
                give: Goods::gold(gold),
                get: thing,
                gain: 0,
            })
        })
        .filter_map(|o| meets(game, me, other, o))
        .max_by_key(|o| o.gain)
}

/// Sell something of `me`'s to `other` for gold: a spell, an item from the armory or a unit that
/// is in a city.
fn sell(game: &Game, me: PlayerId, other: PlayerId) -> Option<Offer> {
    let budget = spendable(game, other) + Knobs::current().reserve;
    let mut things: Vec<Goods> = Vec::new();
    if let (Some(mine), Some(theirs)) = (game.wizard(me), game.wizard(other)) {
        things.extend(
            game.content()
                .rules
                .spells
                .iter()
                .filter(|&(id, def)| mine.knows(id) && !theirs.knows(id) && theirs.books_allow(def))
                .map(|(id, _)| Goods::spell(id)),
        );
    }
    let armory = game.items().armory(me);
    let serials = game
        .items()
        .held(|h| {
            matches!(h,
            f4x_core::item::entity::HeldBy::Vault(p, _)
            | f4x_core::item::entity::HeldBy::Award(p) if p == me)
        })
        .into_iter()
        .map(|(e, _)| game.serial_of(e));
    if armory.items().next().is_some() {
        things.extend(serials.map(Goods::item));
    }
    things
        .into_iter()
        .filter_map(|thing| {
            let gold = price(game, me, other, thing)?;
            (gold <= budget).then(|| Offer {
                give: thing,
                get: Goods::gold(gold),
                gain: 0,
            })
        })
        .filter_map(|o| meets(game, me, other, o))
        .max_by_key(|o| o.gain)
}

/// The trade `me` would offer `other` now: the one that gains it most of a swap, a purchase and
/// a sale. None when nothing clears the margins.
pub(crate) fn offer(game: &Game, me: PlayerId, other: PlayerId) -> Option<ProposalKind> {
    if !Knobs::current().on
        || game.treaty(me, other) == Treaty::War
        || !PlayerView::new(game, me).is_diplomat(other)
    {
        return None;
    }
    [
        swap(game, me, other),
        buy_spell(game, me, other),
        sell(game, me, other),
    ]
    .into_iter()
    .flatten()
    .max_by_key(|o| o.gain)
    .map(|o| ProposalKind::Trade {
        give: o.give,
        get: o.get,
    })
    // Nothing clears both margins as it stands: ask what the other wants for the thing it would
    // like most, and let the negotiation find a price (`respond`).
    .or_else(|| open_ask(game, me, other))
}

/// An open question: the thing of `other`'s that is worth the most more to `me` than to `other`.
fn open_ask(game: &Game, me: PlayerId, other: PlayerId) -> Option<ProposalKind> {
    if arms_enemy(game, other, me) {
        return None;
    }
    let thing = holdings(game, other, me)
        .into_iter()
        .filter(|&g| will_give(game, other, g))
        .map(|g| (core::worth(game, me, g) - core::worth(game, other, g), g))
        .filter(|&(surplus, _)| surplus > 0)
        .max_by_key(|&(surplus, _)| surplus)?
        .1;
    let kind = ProposalKind::Trade {
        give: Goods::NONE,
        get: thing,
    };
    game.check_proposal(me, other, kind).is_ok().then_some(kind)
}

/// The spell `me` would give `other` for nothing (a gift, a tribute): the cheapest one `other` can
/// learn. None when it would arm an enemy.
pub(crate) fn spell_to_give(
    game: &Game,
    me: PlayerId,
    other: PlayerId,
) -> Option<f4x_core::rules::SpellId> {
    if arms_enemy(game, me, other) {
        return None;
    }
    let (mine, theirs) = (game.wizard(me)?, game.wizard(other)?);
    game.content()
        .rules
        .spells
        .iter()
        .filter(|&(id, def)| mine.knows(id) && !theirs.knows(id) && theirs.books_allow(def))
        .min_by_key(|&(id, def)| (core::spell_worth(game, other, id), def.num))
        .map(|(id, _)| id)
}

#[cfg(test)]
mod tests;
