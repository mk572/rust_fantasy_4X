//! Trades between wizards (tasks 504 and 597): what can change hands, what each thing is
//! worth and whether a side can hand it over.
//!
//! A trade is two bundles of [`Goods`]: what the proposer gives and what it gets. A bundle holds
//! gold, a spell, an item (of the vault or the award list) and a unit (of a city, not a hero),
//! each of them optional. The rules check that every thing can be handed over (`Game::check_proposal`);
//! the worth below is for whoever decides, a human or an AI, and the same for both.
//!
//! **The baseline** is one gold = one research point = one production = one mana. A spell is its
//! research cost, an item its value in gold (the anvil's and the merchants' price) and a unit its
//! production cost. A modifier then says what the thing is worth to the one who would hold it
//! (the numbers are the constants below):
//! - a spell is worth more with more books in its realm;
//! - an item is worth more when a hero of the holder has a free slot it fits, a little more when
//!   a hero has a slot for it that is taken, and half when no hero could use it;
//! - a unit is worth more to a wizard at war.
//!
//! Tests: in diplomacy/tests.rs.

use serde::{Deserialize, Serialize};

use crate::game::serial::Serial;
use crate::game::Game;
use crate::item::entity::HeldBy;
use crate::item::Item;
use crate::map::Position;
use crate::player::view::PlayerView;
use crate::player::PlayerId;
use crate::rules::SpellId;

/// What one more book in the realm of a spell adds to its worth to a wizard, in percent, and
/// the books that count.
pub const SPELL_BOOK_PCT: i32 = 5;
pub const SPELL_BOOK_CAP: i32 = 8;
/// What an item is worth to a holder with a hero that has a free slot for it, with a hero whose
/// slot for it is taken and with no hero that could use it, in percent of its value.
pub const ITEM_FREE_SLOT_PCT: i32 = 150;
pub const ITEM_TAKEN_SLOT_PCT: i32 = 100;
pub const ITEM_NO_USER_PCT: i32 = 50;
/// What a unit is worth to a wizard at war, in percent of its cost.
pub const UNIT_AT_WAR_PCT: i32 = 125;

/// One side's part of a trade. Every part is optional; the bundle of a trade is never empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Goods {
    /// Gold from the treasury (0: none).
    #[serde(default)]
    pub gold: i32,
    #[serde(default)]
    pub spell: Option<SpellId>,
    /// An item of the vault or the award list, by creation number.
    #[serde(default)]
    pub item: Option<Serial>,
    /// A unit that is not a hero, standing in a city of the giver, by creation number.
    #[serde(default)]
    pub unit: Option<Serial>,
}

impl Goods {
    pub const NONE: Goods = Goods {
        gold: 0,
        spell: None,
        item: None,
        unit: None,
    };

    pub fn gold(amount: i32) -> Goods {
        Goods {
            gold: amount,
            ..Goods::NONE
        }
    }

    pub fn spell(spell: SpellId) -> Goods {
        Goods {
            spell: Some(spell),
            ..Goods::NONE
        }
    }

    pub fn item(item: Serial) -> Goods {
        Goods {
            item: Some(item),
            ..Goods::NONE
        }
    }

    pub fn unit(unit: Serial) -> Goods {
        Goods {
            unit: Some(unit),
            ..Goods::NONE
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Goods::NONE
    }

    /// Nothing but gold.
    pub fn is_only_gold(&self) -> bool {
        self.gold > 0 && self.spell.is_none() && self.item.is_none() && self.unit.is_none()
    }
}

/// The item with this creation number in the vault or on the award list of `owner`.
pub fn armory_item(view: &Game, owner: PlayerId, serial: Serial) -> Option<(hecs::Entity, Item)> {
    view.items()
        .held(|h| matches!(h, HeldBy::Vault(p, _) | HeldBy::Award(p) if p == owner))
        .into_iter()
        .find(|(e, _)| view.serial_of(*e) == serial)
}

/// The unit with this creation number if it is one `owner` may give: its own, not a hero, in
/// one of its cities.
pub fn giveable_unit(view: &Game, owner: PlayerId, serial: Serial) -> Option<hecs::Entity> {
    let unit = view.units().entity_of_serial(serial)?;
    let mine = view.units().player_units(owner).contains(&unit);
    let hero = view.units().hero_units(owner).contains(&unit);
    let pos = view.units().unit_position(unit)?;
    let in_city = view
        .economy()
        .city_at(pos)
        .and_then(|c| view.economy().city_owner(c))
        == Some(owner);
    (mine && !hero && in_city).then_some(unit)
}

/// Where a unit standing at `from` appears when it is given to `receiver`: the receiver's city
/// with room that is closest to it, on its own plane first.
pub fn delivery_square(view: &Game, receiver: PlayerId, from: Position) -> Option<Position> {
    let map = view.map()?;
    PlayerView::new(view, receiver)
        .my_cities()
        .into_iter()
        .map(|(_, p)| p)
        .filter(|p| view.units().units_at(*p).len() < view.max_tile())
        .min_by_key(|p| {
            let near = (p.plane == from.plane)
                .then(|| map.plane(p.plane).chebyshev((p.x, p.y), (from.x, from.y)));
            (near.is_none(), near, p.plane as u8, p.y, p.x)
        })
}

/// Whether `owner` could hand `goods` to `receiver` now (a bundle may be empty). Gold needs the
/// treasury, a spell the books of the receiver, an item the vault, a unit a city and the receiver
/// a square with room.
pub fn can_hand_over(view: &Game, owner: PlayerId, receiver: PlayerId, goods: Goods) -> bool {
    if goods.gold < 0 {
        return false;
    }
    if goods.gold > 0
        && view
            .player(owner)
            .is_none_or(|p| p.treasury().gold.floor() < goods.gold)
    {
        return false;
    }
    if let Some(spell) = goods.spell {
        let def = view.content().rules.spells.get(spell);
        let ok = view.wizard(owner).is_some_and(|m| m.knows(spell))
            && view
                .wizard(receiver)
                .is_some_and(|m| !m.knows(spell) && m.books_allow(def));
        if !ok {
            return false;
        }
    }
    if let Some(item) = goods.item {
        if view.wizard(receiver).is_none() || armory_item(view, owner, item).is_none() {
            return false;
        }
    }
    if let Some(unit) = goods.unit {
        if giveable_unit(view, owner, unit)
            .and_then(|u| view.units().unit_position(u))
            .and_then(|from| delivery_square(view, receiver, from))
            .is_none()
        {
            return false;
        }
    }
    true
}

/// What `goods` is worth in gold to `holder`, the one who would have it (see the module).
pub fn worth(view: &Game, holder: PlayerId, goods: Goods) -> i32 {
    let mut total = goods.gold.max(0);
    if let Some(spell) = goods.spell {
        total += spell_worth(view, holder, spell);
    }
    if let Some(item) = goods.item {
        total += armory_item_worth(view, holder, item);
    }
    if let Some(unit) = goods.unit {
        total += unit_worth(view, holder, unit);
    }
    total
}

/// A spell: its research cost, more with the books of its realm.
pub fn spell_worth(view: &Game, holder: PlayerId, spell: SpellId) -> i32 {
    let def = view.content().rules.spells.get(spell);
    let books = view
        .wizard(holder)
        .map_or(0, |m| m.books_in(def.realm))
        .min(SPELL_BOOK_CAP);
    def.research_cost as i32 * (100 + SPELL_BOOK_PCT * books) / 100
}

/// An item of anybody's armory (the giver's, so it is looked up in all of them) as `holder`
/// would hold it.
fn armory_item_worth(view: &Game, holder: PlayerId, serial: Serial) -> i32 {
    let found = view
        .items()
        .held(|h| matches!(h, HeldBy::Vault(..) | HeldBy::Award(_)))
        .into_iter()
        .find(|(e, _)| view.serial_of(*e) == serial);
    found.map_or(0, |(_, item)| item_worth(view, holder, &item))
}

/// An item: its value, by whether a hero of the holder could carry it.
pub fn item_worth(view: &Game, holder: PlayerId, item: &Item) -> i32 {
    use crate::item::armory::ItemWorld;
    let items = view.items();
    let mut pct = ITEM_NO_USER_PCT;
    for hero in view.units().hero_units(holder) {
        let Some(slots) = items.hero_slots(holder, hero) else {
            continue;
        };
        let used: Vec<u8> = items.carried(hero).iter().map(|(s, _)| *s).collect();
        for (i, kind) in slots.into_iter().enumerate() {
            if !kind.fits(item.kind) {
                continue;
            }
            let free = !used.contains(&(i as u8));
            pct = pct.max(if free {
                ITEM_FREE_SLOT_PCT
            } else {
                ITEM_TAKEN_SLOT_PCT
            });
        }
    }
    item.cost as i32 * pct / 100
}

/// A unit: its production cost, more to a wizard at war.
fn unit_worth(view: &Game, holder: PlayerId, serial: Serial) -> i32 {
    let Some(unit) = view.units().entity_of_serial(serial) else {
        return 0;
    };
    let cost = view.units().unit_def(unit).map_or(0, |d| d.cost as i32);
    unit_cost_worth(view, holder, cost)
}

/// A unit of this production cost to `holder`.
pub fn unit_cost_worth(view: &Game, holder: PlayerId, cost: i32) -> i32 {
    let at_war = view
        .player_ids()
        .into_iter()
        .any(|p| p != holder && view.treaty(holder, p) == crate::diplomacy::Treaty::War);
    if at_war {
        cost * UNIT_AT_WAR_PCT / 100
    } else {
        cost
    }
}
