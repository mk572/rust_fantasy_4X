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

use crate::lever::Levered;
use f4x_core::diplomacy::trade as core;
use f4x_core::diplomacy::{Goods, Proposal, ProposalKind, Treaty};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::rules::SpellId;
use f4x_core::world::World;

use crate::diplomacy::Verdict;
use crate::memory::Diplomats;
use crate::statecraft::{arms_enemy, goodwill};

/// What a trade must bring `judge` in percent of what it gives `other`: the margin.
pub(crate) fn margin(world: &World, minds: &Diplomats, judge: PlayerId, other: PlayerId) -> i32 {
    let k = world.levers().trade;
    let base = match world.treaty(judge, other) {
        Treaty::Alliance => k.margin_alliance,
        Treaty::Pact => k.margin_pact,
        _ => k.margin_none,
    };
    let relation = minds.friendship(world, judge, other);
    let kind = ProposalKind::Trade {
        give: Goods::NONE,
        get: Goods::NONE,
    };
    let eased = base
        - k.margin_per_relation * relation / 10
        - k.margin_per_goodwill * goodwill(world, minds, judge, other, kind) / 10;
    eased.min(k.margin_most).max(k.margin_least)
}

/// Whether handing `goods` to `to` arms an enemy of `from`: a spell, an item or a unit, not gold.
pub(crate) fn arms(world: &World, from: PlayerId, to: PlayerId, goods: Goods) -> bool {
    (goods.spell.is_some() || goods.item.is_some() || goods.unit.is_some())
        && arms_enemy(world, from, to)
}

/// What `judge` says to the trade `asker` proposes: `give` is what the asker hands over (the
/// judge gets it), `get` what it asks of the judge.
pub(crate) fn verdict(
    world: &World,
    minds: &Diplomats,
    judge: PlayerId,
    asker: PlayerId,
    give: Goods,
    get: Goods,
) -> Verdict {
    if !world.levers().diplomacy.on_trade {
        return Verdict::Accepted;
    }
    if arms(world, judge, asker, get) || !will_give(world, judge, get) {
        return Verdict::Refused;
    }
    let gets = core::worth(world, judge, give);
    let costs = core::worth(world, judge, get);
    if gets * 100 >= costs * margin(world, minds, judge, asker) {
        Verdict::Accepted
    } else {
        Verdict::Refused
    }
}

/// Whether `judge` is willing to part with the units and the hero of `goods`: not while it is at
/// war, not its last defender of a city and not its last hero.
pub(crate) fn will_give(world: &World, judge: PlayerId, goods: Goods) -> bool {
    let at_war = world
        .player_ids()
        .into_iter()
        .any(|p| p != judge && world.treaty(judge, p) == Treaty::War);
    let defended = |serial| {
        core::giveable_unit(world, judge, serial)
            .or_else(|| core::giveable_hero(world, judge, serial))
            .and_then(|u| world.units().unit_position(u))
            .is_some_and(|at| world.units().units_at(at).len() > 1)
    };
    let unit_ok = goods.unit.is_none_or(|u| !at_war && defended(u));
    let hero_ok = goods
        .hero
        .is_none_or(|h| !at_war && defended(h) && world.units().hero_units(judge).len() > 1);
    unit_ok && hero_ok
}

/// Every single thing `owner` could hand to `to` (a spell, an item, a unit, a hero), one bundle
/// each, in a stable order; gold is not in the list.
fn holdings(world: &World, owner: PlayerId, to: PlayerId) -> Vec<Goods> {
    let mut out = Vec::new();
    if let (Some(mine), Some(theirs)) = (world.wizard(owner), world.wizard(to)) {
        out.extend(
            world
                .content()
                .rules
                .spells
                .iter()
                .filter(|&(id, def)| mine.knows(id) && !theirs.knows(id) && theirs.books_allow(def))
                .map(|(id, _)| Goods::spell(id)),
        );
    }
    let mut serials: Vec<_> = world
        .items()
        .held(|h| {
            matches!(h,
            f4x_core::item::entity::HeldBy::Vault(p, _)
            | f4x_core::item::entity::HeldBy::Award(p) if p == owner)
        })
        .into_iter()
        .map(|(e, _)| world.units().serial_of(e))
        .collect();
    serials.sort();
    out.extend(serials.into_iter().map(Goods::item));
    let mut units: Vec<_> = world
        .units()
        .player_units(owner)
        .into_iter()
        .filter(|&u| core::giveable_unit(world, owner, world.units().serial_of(u)).is_some())
        .map(|u| world.units().serial_of(u))
        .collect();
    units.sort();
    out.extend(units.into_iter().map(Goods::unit));
    let mut heroes: Vec<_> = world
        .units()
        .hero_units(owner)
        .into_iter()
        .filter(|&h| core::giveable_hero(world, owner, world.units().serial_of(h)).is_some())
        .map(|h| world.units().serial_of(h))
        .collect();
    heroes.sort();
    out.extend(heroes.into_iter().map(Goods::hero));
    out.retain(|&g| core::can_hand_over(world, owner, to, g));
    out
}

/// What the AI answers to a trade: a deal, a refusal or a trade of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reply {
    Accept,
    Reject,
    Counter(ProposalKind),
}

/// The reply of `judge` to a trade proposal made to it (the negotiation, `max_rounds`):
/// - a full offer that clears its margin is accepted;
/// - a short one is countered by asking for one more thing from the asker, or by asking less of
///   itself, whichever it can do and the asker would take;
/// - an open question is answered by naming a price (the asker named only what it wants), or by
///   naming what it would give (the asker named only what it gives);
/// - anything else is refused, and so is every counter past the last round.
pub(crate) fn respond(
    world: &World,
    minds: &Diplomats,
    judge: PlayerId,
    proposal: Proposal,
) -> Reply {
    let ProposalKind::Trade { give, get } = proposal.kind else {
        return Reply::Reject;
    };
    let asker = proposal.from;
    if !give.is_empty()
        && !get.is_empty()
        && verdict(world, minds, judge, asker, give, get) == Verdict::Accepted
    {
        return Reply::Accept;
    }
    if proposal.round + 1 >= world.content().rules.config.diplomacy.max_rounds
        || !world.levers().diplomacy.on_trade
        || arms(world, judge, asker, get)
        || !will_give(world, judge, get)
    {
        return Reply::Reject;
    }
    let counter = if give.is_empty() {
        name_price(world, minds, judge, asker, get)
    } else if get.is_empty() {
        name_offer(world, minds, judge, asker, give)
    } else {
        counter_short(world, minds, judge, asker, give, get)
    };
    // The counter is the judge's own proposal: it gives what the asker asked for.
    counter
        .filter(|&kind| world.check_proposal(judge, asker, kind).is_ok())
        .map_or(Reply::Reject, Reply::Counter)
}

/// Whether the asker could take what the judge counters with (it gets `gives` and hands over
/// `gets`), as far as anybody can tell: by what the things are worth to it and the least margin
/// anyone asks. What it feels is its own.
fn asker_could_take(
    world: &World,
    judge: PlayerId,
    asker: PlayerId,
    gives: Goods,
    gets: Goods,
) -> bool {
    !arms(world, asker, judge, gets)
        && will_give(world, asker, gets)
        && core::worth(world, asker, gives) * 100
            >= core::worth(world, asker, gets) * world.levers().trade.margin_least
}

/// A full offer that is a little short: ask for one more category from the asker.
fn counter_short(
    world: &World,
    minds: &Diplomats,
    judge: PlayerId,
    asker: PlayerId,
    give: Goods,
    get: Goods,
) -> Option<ProposalKind> {
    let need = core::worth(world, judge, get) * margin(world, minds, judge, asker) / 100;
    let clears = |new_give: Goods| core::worth(world, judge, new_give) >= need;
    let ask = holdings(world, asker, judge)
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
            let more = need - core::worth(world, judge, give) + give.gold;
            (more > give.gold && core::can_hand_over(world, asker, judge, Goods::gold(more)))
                .then_some(Goods { gold: more, ..give })
        })
        .filter(|&g| clears(g) && asker_could_take(world, judge, asker, get, g))
        .min_by_key(|&g| core::worth(world, asker, g));
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
            core::worth(world, judge, give)
                >= core::worth(world, judge, g) * margin(world, minds, judge, asker) / 100
        })
        .filter(|&g| asker_could_take(world, judge, asker, g, give))
        .max_by_key(|&g| core::worth(world, asker, g))
        .map(|less| ProposalKind::Trade {
            give: less,
            get: give,
        })
}

/// The asker wants `get` and names nothing: name a price it can pay, the cheapest for it that
/// the judge takes (gold, one of its things, or a thing and the gold that makes it up).
fn name_price(
    world: &World,
    minds: &Diplomats,
    judge: PlayerId,
    asker: PlayerId,
    get: Goods,
) -> Option<ProposalKind> {
    let k = world.levers().trade;
    let need = (core::worth(world, judge, get) * margin(world, minds, judge, asker) / 100)
        .max(k.least_price);
    let price = payment(world, judge, asker, need, |g| {
        asker_could_take(world, judge, asker, get, g)
    })?;
    Some(ProposalKind::Trade {
        give: get,
        get: price,
    })
}

/// What `payer` could hand `payee` that is worth `need` to it, the cheapest for the payer of what
/// `takes` lets through: gold, one of its things, or a thing and the gold that makes it up.
pub(crate) fn payment(
    world: &World,
    payee: PlayerId,
    payer: PlayerId,
    need: i32,
    takes: impl Fn(Goods) -> bool,
) -> Option<Goods> {
    holdings(world, payer, payee)
        .into_iter()
        .map(|c| {
            let short = (need - core::worth(world, payee, c)).max(0);
            Goods { gold: short, ..c }
        })
        .chain(std::iter::once(Goods::gold(need)))
        .filter(|&g| core::can_hand_over(world, payer, payee, g))
        .filter(|&g| core::worth(world, payee, g) >= need)
        .filter(|&g| takes(g))
        .min_by_key(|&g| core::worth(world, payer, g))
}

/// The asker gives `give` and names nothing: name what the judge would give for it, the thing
/// worth the most to the asker that costs the judge no more than its margin leaves.
fn name_offer(
    world: &World,
    minds: &Diplomats,
    judge: PlayerId,
    asker: PlayerId,
    give: Goods,
) -> Option<ProposalKind> {
    let k = world.levers().trade;
    let budget = core::worth(world, judge, give) * 100 / margin(world, minds, judge, asker);
    let gold = budget.min(spendable(world, judge));
    let cheap = |g: Goods| {
        core::worth(world, judge, g) <= budget
            && !arms(world, judge, asker, g)
            && will_give(world, judge, g)
    };
    let pay = holdings(world, judge, asker)
        .into_iter()
        .filter(|&g| cheap(g))
        .chain((gold >= k.least_price).then(|| Goods::gold(gold)))
        .filter(|&g| core::can_hand_over(world, judge, asker, g))
        .filter(|&g| !(g.is_only_gold() && give.is_only_gold()))
        .filter(|&g| asker_could_take(world, judge, asker, g, give))
        .max_by_key(|&g| core::worth(world, asker, g))?;
    Some(ProposalKind::Trade {
        give: pay,
        get: give,
    })
}

/// The cheapest gold price at which `seller`'s thing is worth selling to `buyer`: the seller's
/// own worth with its margin (a seller is paid more than it values the thing at), and at most
/// what the buyer would pay at its margin. None when the two do not meet.
fn price(
    world: &World,
    minds: &Diplomats,
    seller: PlayerId,
    buyer: PlayerId,
    thing: Goods,
) -> Option<i32> {
    let k = world.levers().trade;
    // Rounded up, so that the price clears the seller's own margin (`meets` checks it in hundredths).
    let ask = (core::worth(world, seller, thing) * margin(world, minds, seller, buyer) + 99) / 100;
    let ask = ask.max(k.least_price);
    let most = core::worth(world, buyer, thing) * 100 / margin(world, minds, buyer, seller);
    (ask <= most).then_some(ask)
}

/// The gold `me` can spend on a purchase.
pub(crate) fn spendable(world: &World, me: PlayerId) -> i32 {
    let gold = world.player(me).map_or(0, |p| p.treasury().gold.floor());
    (gold - crate::economy::gold_reserve(world, me)).max(0)
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
fn swap(world: &World, minds: &Diplomats, me: PlayerId, other: PlayerId) -> Option<Offer> {
    let (mine, theirs) = (world.wizard(me)?, world.wizard(other)?);
    let spells = &world.content().rules.spells;
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
            core::spell_worth(world, me, get) * 100 / core::spell_worth(world, me, give).max(1);
        let theirs = core::spell_worth(world, other, give) * 100
            / core::spell_worth(world, other, get).max(1);
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
    meets(world, minds, me, other, offer)
}

/// The offer with its gain for `me`, when both `me` and `other` would accept it.
fn meets(
    world: &World,
    minds: &Diplomats,
    me: PlayerId,
    other: PlayerId,
    offer: Offer,
) -> Option<Offer> {
    let (gets, costs) = (
        core::worth(world, me, offer.get),
        core::worth(world, me, offer.give),
    );
    let ok_me = gets * 100 >= costs * margin(world, minds, me, other);
    let ok_other = verdict(world, minds, other, me, offer.give, offer.get) == Verdict::Accepted;
    (ok_me
        && ok_other
        && core::can_hand_over(world, me, other, offer.give)
        && core::can_hand_over(world, other, me, offer.get)
        && !arms(world, me, other, offer.give))
    .then_some(Offer {
        gain: gets - costs,
        ..offer
    })
}

/// Buy a spell `other` knows with gold.
fn buy_spell(world: &World, minds: &Diplomats, me: PlayerId, other: PlayerId) -> Option<Offer> {
    let (mine, theirs) = (world.wizard(me)?, world.wizard(other)?);
    let budget = spendable(world, me);
    world
        .content()
        .rules
        .spells
        .iter()
        .filter(|&(id, def)| theirs.knows(id) && !mine.knows(id) && mine.books_allow(def))
        .filter_map(|(id, _)| {
            let thing = Goods::spell(id);
            let gold = price(world, minds, other, me, thing)?;
            (gold <= budget).then(|| Offer {
                give: Goods::gold(gold),
                get: thing,
                gain: 0,
            })
        })
        .filter_map(|o| meets(world, minds, me, other, o))
        .max_by_key(|o| o.gain)
}

/// Sell something of `me`'s to `other` for gold: a spell, an item from the armory or a unit that
/// is in a city.
fn sell(world: &World, minds: &Diplomats, me: PlayerId, other: PlayerId) -> Option<Offer> {
    let budget = world.player(other).map_or(0, |p| p.treasury().gold.floor());
    let mut things: Vec<Goods> = Vec::new();
    if let (Some(mine), Some(theirs)) = (world.wizard(me), world.wizard(other)) {
        things.extend(
            world
                .content()
                .rules
                .spells
                .iter()
                .filter(|&(id, def)| mine.knows(id) && !theirs.knows(id) && theirs.books_allow(def))
                .map(|(id, _)| Goods::spell(id)),
        );
    }
    let armory = world.items().armory(me);
    let serials = world
        .items()
        .held(|h| {
            matches!(h,
            f4x_core::item::entity::HeldBy::Vault(p, _)
            | f4x_core::item::entity::HeldBy::Award(p) if p == me)
        })
        .into_iter()
        .map(|(e, _)| world.units().serial_of(e));
    if armory.items().next().is_some() {
        things.extend(serials.map(Goods::item));
    }
    things.extend(surplus_unit(world, me).map(Goods::unit));
    things
        .into_iter()
        .filter_map(|thing| {
            let gold = price(world, minds, me, other, thing)?;
            (gold <= budget).then(|| Offer {
                give: thing,
                get: Goods::gold(gold),
                gain: 0,
            })
        })
        .filter_map(|o| meets(world, minds, me, other, o))
        .max_by_key(|o| o.gain)
}

/// A unit `me` can do without: the last-made fighter of a city that holds more fighters than
/// `garrison_wanted` asks for, with `unit_spare` to spare, at peace with everybody (`will_give` asks the
/// same of the one who answers). The cheapest first, so the best stay home.
fn surplus_unit(world: &World, me: PlayerId) -> Option<f4x_core::world::serial::Serial> {
    let at_war = world
        .player_ids()
        .into_iter()
        .any(|p| p != me && world.treaty(me, p) == Treaty::War);
    if at_war {
        return None;
    }
    let view = PlayerView::new(world, me);
    view.my_cities().into_iter().find_map(|(_, city)| {
        let fighters: Vec<_> = view
            .garrison(city)
            .into_iter()
            .filter(|&u| crate::view::is_fighter(&view, u))
            .collect();
        let spare = world.levers().trade.unit_spare;
        if fighters.len() <= crate::view::garrison_wanted(&view, city) + spare {
            return None;
        }
        let serial = world.units().serial_of(*fighters.last()?);
        core::giveable_unit(world, me, serial).map(|_| serial)
    })
}

/// The trade `me` would offer `other` now: the one that gains it most of a swap, a purchase and
/// a sale. None when nothing clears the margins.
pub(crate) fn offer(
    world: &World,
    minds: &Diplomats,
    me: PlayerId,
    other: PlayerId,
) -> Option<ProposalKind> {
    if !world.levers().diplomacy.on_trade
        || world.treaty(me, other) == Treaty::War
        || !PlayerView::new(world, me).is_diplomat(other)
    {
        return None;
    }
    [
        swap(world, minds, me, other),
        buy_spell(world, minds, me, other),
        sell(world, minds, me, other),
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
    .or_else(|| open_ask(world, me, other))
}

/// An open question: the thing of `other`'s that is worth the most more to `me` than to `other`.
fn open_ask(world: &World, me: PlayerId, other: PlayerId) -> Option<ProposalKind> {
    if arms_enemy(world, other, me) {
        return None;
    }
    let thing = holdings(world, other, me)
        .into_iter()
        .filter(|&g| will_give(world, other, g))
        .map(|g| (core::worth(world, me, g) - core::worth(world, other, g), g))
        .filter(|&(surplus, _)| surplus > 0)
        .max_by_key(|&(surplus, _)| surplus)?
        .1;
    let kind = ProposalKind::Trade {
        give: Goods::NONE,
        get: thing,
    };
    world
        .check_proposal(me, other, kind)
        .is_ok()
        .then_some(kind)
}

/// The spell `me` would give `other` for nothing (a gift, a tribute): the cheapest one `other` can
/// learn. None when it would arm an enemy.
pub(crate) fn spell_to_give(
    world: &World,
    me: PlayerId,
    other: PlayerId,
) -> Option<f4x_core::rules::SpellId> {
    if arms_enemy(world, me, other) {
        return None;
    }
    let (mine, theirs) = (world.wizard(me)?, world.wizard(other)?);
    world
        .content()
        .rules
        .spells
        .iter()
        .filter(|&(id, def)| mine.knows(id) && !theirs.knows(id) && theirs.books_allow(def))
        .min_by_key(|&(id, def)| (core::spell_worth(world, other, id), def.num))
        .map(|(id, _)| id)
}

#[cfg(test)]
mod tests;
