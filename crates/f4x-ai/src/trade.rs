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
use f4x_core::diplomacy::{Goods, ProposalKind, Treaty};
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
            reserve: k("TRADE_RESERVE", 100),
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
    if arms(game, judge, asker, get) {
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
