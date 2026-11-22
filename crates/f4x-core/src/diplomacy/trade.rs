//! Trades between wizards (tasks 504 and 597): what can change hands, what each thing is
//! worth and whether a side can hand it over.
//!
//! A trade is two bundles of [`Goods`]: what the proposer gives and what it gets. A bundle holds
//! gold, a spell, an item (of the vault or the award list), a unit (of a city, not a hero) and a
//! hero (of a city), each of them optional. A negotiation (`Proposal::round`) builds a bundle up
//! one category at a time. The rules check that every thing can be handed over (`Game::check_proposal`);
//! the worth below is for whoever decides, a human or an AI, and the same for both.
//!
//! **The baseline** is one gold = one research point = one production = one mana. A spell is its
//! research cost, an item its value in gold (the anvil's and the merchants' price) and a unit its
//! production cost. A modifier then says what the thing is worth to the one who would hold it
//! (the numbers are the constants below):
//! - a spell is worth more with more books in its realm;
//! - an item is worth more when a hero of the holder has a free slot it fits, a little more when
//!   a hero has a slot for it that is taken, and half when no hero could use it;
//! - a unit is worth more to a wizard at war, and more for every level and every part of a level
//!   it has earned (a quarter of its cost for a level, the hire price of a hero is the same
//!   rule); a hero is worth its hire cost the same way, plus the items it carries.
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

// What the goods are worth in percent (books, slots, war) is `trade` of `rules.ron`.

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
    /// A hero that stands in a city of the giver, by creation number. It keeps its level,
    /// experience and the items it carries.
    #[serde(default)]
    pub hero: Option<Serial>,
}

impl Goods {
    pub const NONE: Goods = Goods {
        gold: 0,
        spell: None,
        item: None,
        unit: None,
        hero: None,
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

    pub fn hero(hero: Serial) -> Goods {
        Goods {
            hero: Some(hero),
            ..Goods::NONE
        }
    }

    /// The same bundle with `other`'s parts added where this one has none (a counter that adds a
    /// category). A category both hold keeps this one's.
    pub fn with(self, other: Goods) -> Goods {
        Goods {
            gold: if self.gold > 0 { self.gold } else { other.gold },
            spell: self.spell.or(other.spell),
            item: self.item.or(other.item),
            unit: self.unit.or(other.unit),
            hero: self.hero.or(other.hero),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Goods::NONE
    }

    /// Nothing but gold.
    pub fn is_only_gold(&self) -> bool {
        self.gold > 0
            && self.spell.is_none()
            && self.item.is_none()
            && self.unit.is_none()
            && self.hero.is_none()
    }
}

/// The item with this creation number in the vault or on the award list of `owner`.
pub fn armory_item(view: &Game, owner: PlayerId, serial: Serial) -> Option<(hecs::Entity, Item)> {
    view.items()
        .held(|h| matches!(h, HeldBy::Vault(p, _) | HeldBy::Award(p) if p == owner))
        .into_iter()
        .find(|(e, _)| view.units().serial_of(*e) == serial)
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

/// The hero with this creation number if `owner` may give it: in its service and in one of its
/// cities.
pub fn giveable_hero(view: &Game, owner: PlayerId, serial: Serial) -> Option<hecs::Entity> {
    let hero = view.units().entity_of_serial(serial)?;
    let mine = view.units().hero_units(owner).contains(&hero);
    let pos = view.units().unit_position(hero)?;
    let in_city = view
        .economy()
        .city_at(pos)
        .and_then(|c| view.economy().city_owner(c))
        == Some(owner);
    (mine && in_city).then_some(hero)
}

/// Where a unit standing at `from` appears when it is given to `receiver`: the receiver's city
/// with room that is closest to it, on its own plane first.
pub fn delivery_square(view: &Game, receiver: PlayerId, from: Position) -> Option<Position> {
    let map = view.map()?;
    PlayerView::new(view, receiver)
        .my_cities()
        .into_iter()
        .map(|(_, p)| p)
        .min_by_key(|p| {
            let near = (p.plane == from.plane)
                .then(|| map.plane(p.plane).chebyshev((p.x, p.y), (from.x, from.y)));
            (near.is_none(), near, p.plane as u8, p.y, p.x)
        })
}

/// Whether `owner` could hand `goods` to `receiver` now (a bundle may be empty). Gold needs the
/// treasury, a spell the books of the receiver (a wizard's), an item the vault (the receiver's
/// too: every player has one), a unit a city and the receiver a square with room.
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
        // Any player with a vault may take it: a wizard or the neutral player of a race.
        if view.player(receiver).is_none() || armory_item(view, owner, item).is_none() {
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
    if let Some(hero) = goods.hero {
        if giveable_hero(view, owner, hero)
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
    if let Some(hero) = goods.hero {
        total += hero_worth(view, holder, hero);
    }
    total
}

/// A spell: its research cost, more with the books of its realm.
pub fn spell_worth(view: &Game, holder: PlayerId, spell: SpellId) -> i32 {
    let def = view.content().rules.spells.get(spell);
    let rules = &view.content().rules.config.trade;
    let books = view
        .wizard(holder)
        .map_or(0, |m| m.books_in(def.realm))
        .min(rules.spell_book_cap);
    def.research_cost as i32 * (100 + rules.spell_book_pct * books) / 100
}

/// An item of anybody's armory (the giver's, so it is looked up in all of them) as `holder`
/// would hold it.
fn armory_item_worth(view: &Game, holder: PlayerId, serial: Serial) -> i32 {
    let found = view
        .items()
        .held(|h| matches!(h, HeldBy::Vault(..) | HeldBy::Award(_)))
        .into_iter()
        .find(|(e, _)| view.units().serial_of(*e) == serial);
    found.map_or(0, |(_, item)| item_worth(view, holder, &item))
}

/// An item: its value, by whether a hero of the holder could carry it.
pub fn item_worth(view: &Game, holder: PlayerId, item: &Item) -> i32 {
    use crate::item::armory::ItemWorld;
    let items = view.items();
    let rules = &view.content().rules.config.trade;
    let mut pct = rules.item_no_user_pct;
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
                rules.item_free_slot_pct
            } else {
                rules.item_taken_slot_pct
            });
        }
    }
    item.cost as i32 * pct / 100
}

/// A unit: its production cost, more for its level and experience, and more to a wizard at war.
fn unit_worth(view: &Game, holder: PlayerId, serial: Serial) -> i32 {
    let Some(unit) = view.units().entity_of_serial(serial) else {
        return 0;
    };
    let cost = view.units().unit_def(unit).map_or(0, |d| d.cost as i32);
    let made = veteran_cost(view, unit, cost);
    unit_cost_worth(view, holder, made.max(combat_worth(view, unit)))
}

/// What a unit's combat rating (its worth in spearmen at its level, `unit_ratings.ron`) is worth
/// in gold; 0 for a unit with no row or when the rules leave it out. The rating says nothing of
/// price or upkeep: the worth in a trade is the greater of this and what the unit cost to make.
fn combat_worth(view: &Game, unit: hecs::Entity) -> i32 {
    let rules = &view.content().rules;
    let Some(def) = view.units().unit_def(unit) else {
        return 0;
    };
    let level = view.units().unit_level(unit);
    rules.combat_rating(def, level).map_or(0, |r| {
        (r * rules.config.trade.gold_per_rating as f32).round() as i32
    })
}

/// A hero: its hire cost, more for its level and experience, plus the items it carries, and more
/// to a wizard at war (the items too: they fight with it).
fn hero_worth(view: &Game, holder: PlayerId, serial: Serial) -> i32 {
    let Some(hero) = view.units().entity_of_serial(serial) else {
        return 0;
    };
    let hire = view
        .units()
        .unit_def(hero)
        .and_then(|d| d.hero.as_ref())
        .map_or(0, |h| h.hire_cost as i32);
    let items: i32 = view
        .items()
        .carried(hero)
        .iter()
        .map(|(_, i)| i.cost as i32)
        .sum();
    unit_cost_worth(
        view,
        holder,
        veteran_cost(view, hero, hire).max(combat_worth(view, hero)) + items,
    )
}

/// What a unit of this base cost is worth with the level and experience it has: a quarter of the
/// cost for every level (the hire price of a hero grows the same way) and the part of the next
/// quarter its points towards the next level make up.
pub fn veteran_cost(view: &Game, unit: hecs::Entity, base: i32) -> i32 {
    let Some(path) = view.units().unit_level_path(unit) else {
        return base;
    };
    let xp = view.units().unit_experience(unit);
    let level = view
        .units()
        .unit_def(unit)
        .map_or(0, |def| view.content().rules.xp_level(def, xp));
    let part = path.levels.get(level + 1).map_or(0, |next| {
        let from = path.levels[level].xp;
        let span = next.xp.saturating_sub(from).max(1);
        (xp.saturating_sub(from).min(span) as i32) * (base / 4) / span as i32
    });
    base + base * level as i32 / 4 + part
}

/// A unit of this production cost to `holder`.
pub fn unit_cost_worth(view: &Game, holder: PlayerId, cost: i32) -> i32 {
    let at_war = view
        .player_ids()
        .into_iter()
        .any(|p| p != holder && view.treaty(holder, p) == crate::diplomacy::Treaty::War);
    if at_war {
        cost * view.content().rules.config.trade.unit_at_war_pct / 100
    } else {
        cost
    }
}
