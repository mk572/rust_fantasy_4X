//! The human side of a trade (tasks 504 and 597): the goods in words, the things the human can
//! offer or ask for, and the prices to pick from. The worth is the core's
//! (`f4x_core::diplomacy::trade`), the same as the AI's, so a price near the worth is one that is
//! likely to be taken.

use crate::text::Args;
use f4x_core::diplomacy::trade::{self, Goods};
use f4x_core::player::PlayerId;

use crate::view::{App, HUMAN};

/// What the human trades in this menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TradeSort {
    BuySpell,
    SellSpell,
    SellItem,
    SellUnit,
}

/// What a buyer offers, in percent of what the seller holds the thing to be worth.
const BUY_PERCENTS: [i32; 3] = [110, 130, 160];
/// What a seller asks, in percent of what the buyer holds the thing to be worth.
const SELL_PERCENTS: [i32; 3] = [100, 80, 60];
/// The most things a list shows.
const SHOWN: usize = 6;

impl App {
    /// A bundle of goods in words.
    pub(crate) fn goods_text(&self, goods: Goods) -> String {
        let mut parts = Vec::new();
        if goods.gold > 0 {
            parts.push(self.text_with(
                "diplomacy.goods_gold",
                &Args::new().count("gold", goods.gold),
            ));
        }
        if let Some(spell) = goods.spell {
            let name = self
                .names
                .spell(self.game.content().rules.spells.get(spell).num)
                .to_string();
            parts.push(self.text_with("diplomacy.goods_spell", &Args::new().text("name", &name)));
        }
        if let Some(serial) = goods.item {
            let found = self
                .game
                .items()
                .held(|_| true)
                .into_iter()
                .find(|(e, _)| self.game.serial_of(*e) == serial);
            if let Some((_, item)) = found {
                let name = self.names.item_name(&item, &self.texts);
                parts
                    .push(self.text_with("diplomacy.goods_item", &Args::new().text("name", &name)));
            }
        }
        if let Some(serial) = goods.unit {
            if let Some(def) = self
                .game
                .units()
                .entity_of_serial(serial)
                .and_then(|u| self.game.units().unit_def(u))
            {
                let name = self.names.unit(def.num).to_string();
                parts
                    .push(self.text_with("diplomacy.goods_unit", &Args::new().text("name", &name)));
            }
        }
        if parts.is_empty() {
            return self.text("diplomacy.goods_nothing");
        }
        parts.join(&self.text("diplomacy.goods_join"))
    }

    /// What the human could buy or sell of this sort: a label and the thing, the dearest first.
    pub(crate) fn trade_things(&self, with: PlayerId, sort: TradeSort) -> Vec<(String, Goods)> {
        let rules = &self.game.content().rules;
        let mut out: Vec<(i32, String, Goods)> = Vec::new();
        match sort {
            TradeSort::BuySpell | TradeSort::SellSpell => {
                let (owner, other) = if sort == TradeSort::BuySpell {
                    (with, HUMAN)
                } else {
                    (HUMAN, with)
                };
                let (Some(mine), Some(theirs)) = (self.game.wizard(owner), self.game.wizard(other))
                else {
                    return Vec::new();
                };
                for (id, def) in rules.spells.iter() {
                    if mine.knows(id) && !theirs.knows(id) && theirs.books_allow(def) {
                        let label = self.names.spell(def.num).to_string();
                        out.push((def.research_cost as i32, label, Goods::spell(id)));
                    }
                }
            }
            TradeSort::SellItem => {
                for (entity, item) in self.game.items().held(|h| {
                    matches!(h,
                        f4x_core::item::entity::HeldBy::Vault(p, _)
                        | f4x_core::item::entity::HeldBy::Award(p) if p == HUMAN)
                }) {
                    let label = self.names.item_name(&item, &self.texts);
                    out.push((
                        item.cost as i32,
                        label,
                        Goods::item(self.game.serial_of(entity)),
                    ));
                }
            }
            TradeSort::SellUnit => {
                for unit in self.game.units().player_units(HUMAN) {
                    let serial = self.game.serial_of(unit);
                    if trade::giveable_unit(&self.game, HUMAN, serial).is_none() {
                        continue;
                    }
                    if let Some(def) = self.game.units().unit_def(unit) {
                        out.push((
                            def.cost as i32,
                            self.names.unit(def.num).to_string(),
                            Goods::unit(serial),
                        ));
                    }
                }
            }
        }
        out.sort_by_key(|(cost, label, _)| (std::cmp::Reverse(*cost), label.clone()));
        out.into_iter()
            .take(SHOWN)
            .map(|(_, label, goods)| (label, goods))
            .collect()
    }

    /// The gold prices offered for `thing`, with a worth the other holds it to have: a buyer
    /// offers more than the seller's worth, a seller asks less than the buyer's.
    pub(crate) fn trade_prices(&self, with: PlayerId, thing: Goods, buying: bool) -> Vec<i32> {
        let (percents, holder) = if buying {
            (BUY_PERCENTS, with)
        } else {
            (SELL_PERCENTS, with)
        };
        let worth = trade::worth(&self.game, holder, thing);
        let mut prices: Vec<i32> = percents.iter().map(|p| (worth * p / 100).max(1)).collect();
        prices.dedup();
        prices
    }
}
