//! The human side of a trade (tasks 504, 597 and the negotiation): the goods in words, a draft
//! of what the human gives and asks, and the things and amounts to add to it, one category at a
//! time. The worth is the core's
//! (`f4x_core::diplomacy::trade`), the same as the AI's, so a price near the worth is one that is
//! likely to be taken.

use crate::text::Args;
use f4x_core::diplomacy::trade::{self, Goods};
use f4x_core::player::PlayerId;

use crate::view::App;

/// A side of the draft: what the human gives or what it asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Give,
    Ask,
}

/// A category of goods to add.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cat {
    Gold,
    Spell,
    Item,
    Unit,
    Hero,
}

impl Cat {
    pub(crate) const ALL: [Cat; 5] = [Cat::Gold, Cat::Spell, Cat::Item, Cat::Unit, Cat::Hero];

    /// The key of the row that opens the category.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Cat::Gold => "diplomacy.trade_cat_gold",
            Cat::Spell => "diplomacy.trade_cat_spell",
            Cat::Item => "diplomacy.trade_cat_item",
            Cat::Unit => "diplomacy.trade_cat_unit",
            Cat::Hero => "diplomacy.trade_cat_hero",
        }
    }
}

/// The trade the human is putting together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Draft {
    pub give: Goods,
    pub ask: Goods,
    /// The draft answers a trade the wizard has made (a counter) and not a new offer.
    pub counter: bool,
}

impl Draft {
    pub(crate) fn side(&self, side: Side) -> Goods {
        match side {
            Side::Give => self.give,
            Side::Ask => self.ask,
        }
    }

    /// `add` goes into the side: its category replaces what the side had of it.
    pub(crate) fn add(&mut self, side: Side, add: Goods) {
        let mut base = self.side(side);
        if add.gold > 0 {
            base.gold = add.gold;
        }
        base.spell = add.spell.or(base.spell);
        base.item = add.item.or(base.item);
        base.unit = add.unit.or(base.unit);
        base.hero = add.hero.or(base.hero);
        match side {
            Side::Give => self.give = base,
            Side::Ask => self.ask = base,
        }
    }
}

/// The gold amounts a side can be given.
const GOLD_STEPS: [i32; 7] = [25, 50, 100, 200, 400, 800, 1600];
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
            if let Some(unit) = self.game.units().entity_of_serial(serial) {
                if let Some(def) = self.game.units().unit_def(unit) {
                    let name = self.names.unit(def.num).to_string();
                    let level = self.game.units().unit_level(unit) as i32;
                    parts.push(self.text_with(
                        "diplomacy.goods_unit",
                        &Args::new().text("name", &name).count("level", level + 1),
                    ));
                }
            }
        }
        if let Some(serial) = goods.hero {
            if let Some(hero) = self.game.units().entity_of_serial(serial) {
                let name = self.game.units().hero_name(hero).unwrap_or_default();
                let level = self.game.units().unit_level(hero) as i32;
                parts.push(self.text_with(
                    "diplomacy.goods_hero",
                    &Args::new().text("name", &name).count("level", level + 1),
                ));
            }
        }
        if parts.is_empty() {
            return self.text("diplomacy.goods_nothing");
        }
        parts.join(&self.text("diplomacy.goods_join"))
    }

    /// What `owner` could hand `to` of this category: a label with its worth to the one who
    /// would get it, and the thing, the dearest first.
    pub(crate) fn pick_things(
        &self,
        owner: PlayerId,
        to: PlayerId,
        cat: Cat,
    ) -> Vec<(String, Goods)> {
        let rules = &self.game.content().rules;
        let mut out: Vec<(i32, String, Goods)> = Vec::new();
        let mut put = |goods: Goods, label: String| {
            let worth = trade::worth(&self.game, to, goods);
            let label = self.text_with(
                "diplomacy.pick_label",
                &Args::new().text("name", &label).count("worth", worth),
            );
            out.push((worth, label, goods));
        };
        match cat {
            Cat::Gold => {}
            Cat::Spell => {
                if let (Some(mine), Some(theirs)) = (self.game.wizard(owner), self.game.wizard(to))
                {
                    for (id, def) in rules.spells.iter() {
                        if mine.knows(id) && !theirs.knows(id) && theirs.books_allow(def) {
                            put(Goods::spell(id), self.names.spell(def.num).to_string());
                        }
                    }
                }
            }
            Cat::Item => {
                for (entity, item) in self.game.items().held(|h| {
                    matches!(h,
                        f4x_core::item::entity::HeldBy::Vault(p, _)
                        | f4x_core::item::entity::HeldBy::Award(p) if p == owner)
                }) {
                    put(
                        Goods::item(self.game.serial_of(entity)),
                        self.names.item_name(&item, &self.texts),
                    );
                }
            }
            Cat::Unit => {
                for unit in self.game.units().player_units(owner) {
                    let serial = self.game.serial_of(unit);
                    if trade::giveable_unit(&self.game, owner, serial).is_none() {
                        continue;
                    }
                    if let Some(def) = self.game.units().unit_def(unit) {
                        let level = self.game.units().unit_level(unit) + 1;
                        put(
                            Goods::unit(serial),
                            format!("{} {level}", self.names.unit(def.num)),
                        );
                    }
                }
            }
            Cat::Hero => {
                for hero in self.game.units().hero_units(owner) {
                    let serial = self.game.serial_of(hero);
                    if trade::giveable_hero(&self.game, owner, serial).is_none() {
                        continue;
                    }
                    let name = self.game.units().hero_name(hero).unwrap_or_default();
                    let level = self.game.units().unit_level(hero) + 1;
                    put(Goods::hero(serial), format!("{name} {level}"));
                }
            }
        }
        out.retain(|(_, _, g)| trade::can_hand_over(&self.game, owner, to, *g));
        out.sort_by_key(|(worth, label, _)| (std::cmp::Reverse(*worth), label.clone()));
        out.into_iter()
            .take(SHOWN)
            .map(|(_, label, goods)| (label, goods))
            .collect()
    }

    /// The gold amounts to pick from.
    pub(crate) fn gold_steps(&self) -> [i32; 7] {
        GOLD_STEPS
    }

    /// Whether `owner` has any of the category to hand over.
    pub(crate) fn can_pick(&self, owner: PlayerId, to: PlayerId, cat: Cat) -> bool {
        match cat {
            Cat::Gold => true,
            _ => !self.pick_things(owner, to, cat).is_empty(),
        }
    }
}
