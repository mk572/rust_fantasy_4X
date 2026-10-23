//! Gold and food that run short (task 292): the preview a player (human or AI) asks for before
//! ending the turn, and the command that lets chosen units go.
//!
//! The turn change settles the treasury by itself (`finance::settle`: food first, newest first,
//! then gold). A player who would rather choose asks [`Game::treasury_shortfall`] and answers
//! with `Command::ReleaseTroops`; whatever is released goes at once, so the turn change finds
//! the books balanced or lets go only of what is still missing. Units released for food no
//! longer count for gold. A hero who leaves goes back to the roster and can be hired again, and
//! the items come home to the armory; what does not fit is destroyed for mana, the owner's
//! choice (`Game::item_overflow`). A ship let go with passengers aboard turns neutral and sails
//! for the nearest free shore (`transport::Marooned`).

use crate::item::armory::{Armory, ItemError, ItemSpot, MAX_AWARDS, VAULT_SLOTS};
use crate::item::{HeroItems, Item};
use crate::player::finance::{gold_after_income, settle};
use crate::player::{PlayerId, MONSTERS};
use crate::unit::movement::MoveError;
use crate::unit::transport::{Aboard, Marooned};

use crate::game::{Event, Game, LossCause, Rejection, Shortage};

/// One unit the treasury pays for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TroopLine {
    pub unit: hecs::Entity,
    pub label: String,
    pub gold: i32,
    pub food: i32,
    pub hero: bool,
    /// What the unit cost to make (the AI lets go of the cheap ones first).
    pub cost: u32,
    /// A ship: letting it go with passengers aboard sends it sailing for the shore.
    pub ship: bool,
    /// Units aboard (for a ship).
    pub aboard: u32,
}

/// The gold and food the next turn change cannot pay for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreasuryShortfall {
    pub food_upkeep: i32,
    pub food_surplus: i32,
    /// The gold on hand once the turn's income and the buildings' upkeep are counted.
    pub gold: i32,
    /// Fame pays this much of the troops' gold.
    pub fame: i32,
    /// The troops, oldest first; the turn change lets go from the end.
    pub troops: Vec<TroopLine>,
    /// What the turn change would let go by itself.
    pub default_release: Vec<hecs::Entity>,
}

impl TreasuryShortfall {
    /// The food and the gold still missing if `released` go. A unit let go for food does not cost
    /// gold either.
    pub fn missing(&self, released: &[hecs::Entity]) -> (i32, i32) {
        let gone = |l: &&TroopLine| released.contains(&l.unit);
        let freed_food: i32 = self.troops.iter().filter(gone).map(|l| l.food).sum();
        let food = (self.food_upkeep - freed_food - self.food_surplus).max(0);
        let gold_upkeep: i32 = self
            .troops
            .iter()
            .filter(|l| !gone(l))
            .map(|l| l.gold)
            .sum();
        let gold = ((gold_upkeep - self.fame).max(0) - self.gold).max(0);
        (food, gold)
    }
}

/// An item that would not fit the armory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemLine {
    pub spot: ItemSpot,
    pub label: String,
    /// The mana the anvil gives for it.
    pub mana: i32,
}

/// Releasing heroes brings their items to an armory that is too full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemOverflow {
    /// How many items must go.
    pub excess: usize,
    /// Every item that could go: the armory's and the released heroes' own.
    pub lines: Vec<ItemLine>,
    /// The cheapest `excess` of them, which go if the player chooses nothing.
    pub default_scrap: Vec<ItemSpot>,
}

impl Game {
    /// The gold or food the wizard cannot pay at the next turn change, or `None` when both are
    /// covered.
    pub fn treasury_shortfall(&self, player: PlayerId) -> Option<TreasuryShortfall> {
        if self.time_stopper() == Some(player) {
            return None;
        }
        let t = self.budget(player);
        let troops = self.troops(player);
        let fame = self.fame(player);
        let gold = self.treasury(player)?.gold;
        let lines = troops
            .iter()
            .map(|tr| {
                let units = self.units();
                let eco = self.economy();
                TroopLine {
                    unit: tr.id,
                    label: units
                        .unit_def(tr.id)
                        .map_or_else(|| "?".to_string(), |d| d.name.clone()),
                    gold: tr.gold,
                    food: tr.food,
                    hero: units.hero_kind(tr.id).is_some(),
                    cost: units.unit_def(tr.id).map_or(0, |d| d.cost),
                    ship: eco.is_ship(tr.id),
                    aboard: eco.passengers_of(tr.id).len() as u32,
                }
            })
            .collect();
        let short = TreasuryShortfall {
            food_upkeep: t.food_upkeep,
            food_surplus: t.food_surplus,
            gold: gold_after_income(&t, gold),
            fame,
            troops: lines,
            default_release: settle(&t, gold, fame, &troops).disbanded,
        };
        (short.missing(&[]) != (0, 0)).then_some(short)
    }

    fn hero_slot_items(&self, hero: hecs::Entity) -> Vec<(usize, Item)> {
        self.world
            .get::<&HeroItems>(hero)
            .map(|h| {
                h.0.iter()
                    .enumerate()
                    .filter_map(|(i, it)| it.clone().map(|it| (i, it)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The items that will not fit if `released` go: `None` when they all do.
    pub fn item_overflow(
        &self,
        player: PlayerId,
        released: &[hecs::Entity],
    ) -> Option<ItemOverflow> {
        let armory = self.player_component::<Armory>(player)?;
        let mut lines = Vec::new();
        fn line(lines: &mut Vec<ItemLine>, spot: ItemSpot, item: &Item) {
            lines.push(ItemLine {
                spot,
                label: item.name.clone(),
                mana: item.anvil_mana(),
            })
        }
        for (i, item) in armory.vault.iter().enumerate() {
            if let Some(item) = item {
                line(&mut lines, ItemSpot::Vault(i), item);
            }
        }
        for (i, item) in armory.awards.iter().enumerate() {
            line(&mut lines, ItemSpot::Award(i), item);
        }
        for &hero in released {
            for (slot, item) in self.hero_slot_items(hero) {
                line(&mut lines, ItemSpot::Hero(hero, slot), &item);
            }
        }
        let room = VAULT_SLOTS + MAX_AWARDS;
        let excess = lines.len().saturating_sub(room);
        if excess == 0 {
            return None;
        }
        let mut cheapest: Vec<usize> = (0..lines.len()).collect();
        cheapest.sort_by_key(|&i| (lines[i].mana, i));
        let default_scrap = cheapest[..excess].iter().map(|&i| lines[i].spot).collect();
        Some(ItemOverflow {
            excess,
            lines,
            default_scrap,
        })
    }

    /// Let units go and destroy the listed items for mana (see the module docs).
    pub fn release_troops(
        &mut self,
        player: PlayerId,
        units: &[hecs::Entity],
        scrap: &[ItemSpot],
    ) -> Result<Vec<Event>, Rejection> {
        let mut chosen: Vec<hecs::Entity> = Vec::new();
        for &u in units {
            if self.units().unit_owner(u) != Some(player) {
                return Err(Rejection::Move(MoveError::BadSelection));
            }
            if !chosen.contains(&u) {
                chosen.push(u);
            }
        }
        let mut spots: Vec<ItemSpot> = Vec::new();
        for &spot in scrap {
            let present = match spot {
                ItemSpot::Vault(i) => self
                    .player_component::<Armory>(player)
                    .is_some_and(|a| a.vault.get(i).is_some_and(Option::is_some)),
                ItemSpot::Award(i) => self
                    .player_component::<Armory>(player)
                    .is_some_and(|a| i < a.awards.len()),
                ItemSpot::Hero(h, slot) => {
                    self.units().unit_owner(h) == Some(player)
                        && self.hero_slot_items(h).iter().any(|(s, _)| *s == slot)
                }
            };
            if !present {
                return Err(Rejection::Item(ItemError::Empty));
            }
            if !spots.contains(&spot) {
                spots.push(spot);
            }
        }
        let before = self.treasury_shortfall(player).map(|s| s.missing(&[]));
        let (food_short, gold_short) = before.map_or((false, false), |(f, g)| (f > 0, g > 0));
        let mut events = Vec::new();
        if food_short {
            events.push(Event::Shortage {
                player,
                what: Shortage::Food,
            });
        }
        if gold_short {
            events.push(Event::Shortage {
                player,
                what: Shortage::Gold,
            });
        }
        // Items first, the armory's later awards before earlier ones so the numbers hold.
        spots.sort_by_key(|s| match s {
            ItemSpot::Award(i) => (1, usize::MAX - i),
            ItemSpot::Vault(i) => (0, *i),
            ItemSpot::Hero(..) => (2, 0),
        });
        for spot in spots {
            let item = match spot {
                ItemSpot::Hero(h, slot) => self
                    .hero_slot_items(h)
                    .into_iter()
                    .find(|(s, _)| *s == slot)
                    .map(|(_, it)| it),
                ItemSpot::Vault(i) => self
                    .player_component::<Armory>(player)
                    .and_then(|a| a.vault[i].clone()),
                ItemSpot::Award(i) => self
                    .player_component::<Armory>(player)
                    .and_then(|a| a.awards.get(i).cloned()),
            };
            if let Some(item) = item {
                self.set_item_at(player, spot, None);
                if let Some(t) = self.treasury_mut(player) {
                    t.mana += item.anvil_mana();
                }
            }
        }
        for &u in &chosen {
            let food = self.units().unit_food_upkeep(u) > 0;
            let cause = if food_short && food {
                LossCause::Food
            } else {
                LossCause::Gold
            };
            events.extend(self.release_one(player, u, cause));
        }
        if !chosen.is_empty() {
            events.push(Event::UnitsDisbanded {
                player,
                units: chosen.len() as u32,
            });
        }
        Ok(events)
    }

    /// Let one unit go. A hero returns to the roster and its items to the armory (what does not
    /// fit is destroyed for mana); a ship with passengers aboard at sea turns neutral and sails
    /// for the shore, one in port unloads and is released; anything else just goes.
    pub(crate) fn release_one(
        &mut self,
        player: PlayerId,
        unit: hecs::Entity,
        cause: LossCause,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        let Some(kind) = self.units().unit_kind_id(unit) else {
            return events;
        };
        events.push(Event::UnitLost {
            player,
            unit: kind,
            cause,
        });
        if self.units().hero_kind(unit).is_some() {
            events.push(Event::HeroLeft { player, hero: kind });
            let items = self.hero_slot_items(unit);
            self.remove_unit(unit, crate::hero::Fate::Released);
            for (_, item) in items {
                let mana = item.anvil_mana();
                if self.give_item(player, item) == crate::item::armory::Placed::Lost {
                    if let Some(t) = self.treasury_mut(player) {
                        t.mana += mana;
                    }
                }
            }
            return events;
        }
        if self.economy().is_ship(unit) {
            let aboard = self.economy().passengers_of(unit);
            let at_sea = self
                .units()
                .unit_position(unit)
                .is_some_and(|at| self.economy().is_sea(at));
            if !aboard.is_empty() && at_sea {
                self.set_owner(unit, MONSTERS);
                let _ = self.world.insert_one(unit, Marooned);
                return events;
            }
            for p in aboard {
                let _ = self.world.remove_one::<Aboard>(p);
            }
        }
        self.remove_unit(unit, crate::hero::Fate::Released);
        events
    }
}

#[cfg(test)]
mod tests;
