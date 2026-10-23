//! Offers each turn (task 220): a wandering merchant's magic item, a company of mercenaries and
//! a hero for hire, as `Determine_Offer` (`NEXTTURN.c`) and `Generate_Mercenaries`
//! (`CITYCALC.c`) make them. At the turn change every wizard with a fortress rolls the three
//! offers; what is offered waits in [`Offers`] until the wizard accepts or rejects it
//! (the pop-ups of task 143 show it, the computer wizards answer through `f4x-ai`). An offer
//! not answered by the next turn change lapses.
//!
//! The chances, the prices and the AI's answers are plain functions, so they are tested without
//! a game. Where the original is wrong this follows its evident intent (`docs/divergences.md`,
//! section 11): the computer wizards meet the merchant too, every wizard's chances use their own fame, the price is tested after the
//! Charismatic discount, a mercenary company can be of level 2 and the AI's hero test reads as
//! its comment says.

use serde::{Deserialize, Serialize};

use crate::game::{Event, Game};
use crate::hero::kind::Class;
use crate::hero::HeroError;
use crate::item::armory::Placed;
use crate::item::make::{make_item, Grade};
use crate::item::Item;
use crate::player::{Offers, PlayerId};
use crate::rules::{MovementType, Rules, UnitDef, UnitKind};
use crate::unit::{Experience, MAX_STACK};
use crate::wizard::magic::Retort;

/// No mercenaries are offered before this turn (`Generate_Mercenaries`).
pub const FIRST_MERCENARY_TURN: u32 = 30;
/// The computer wizards do not hire mercenaries before this turn (`AI_Accept_Mercenaries`).
pub const AI_FIRST_MERCENARY_TURN: u32 = 50;
/// The most any offer's chance can be, in percent, before the computer wizards' bonus.
const MAX_CHANCE: i32 = 10;
/// The one engineer unit no company is drawn from: Draconians fly and need no roads.
const DRACONIAN_ENGINEERS: &str = "draconian_engineers";
/// The computer wizards' extra chance of a mercenary or hero offer, in percent.
const AI_BONUS: i32 = 10;

/// What is offered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Offer {
    /// The wizard's hero of type `kind` asks to be hired.
    Hero { kind: usize, price: i32 },
    /// A company of `count` units of `unit` (an id of `units.ron`), each of `level` (0-based
    /// experience level), for `price` in all.
    Mercenaries {
        unit: String,
        count: u32,
        level: u8,
        price: i32,
    },
    /// A merchant sells a magic item.
    Item { item: Item, price: i32 },
}

impl Offer {
    pub fn price(&self) -> i32 {
        match self {
            Offer::Hero { price, .. }
            | Offer::Mercenaries { price, .. }
            | Offer::Item { price, .. } => *price,
        }
    }
}

/// The three kinds of offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferKind {
    Hero,
    Mercenaries,
    Item,
}

/// Why an offer could not be taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferError {
    /// There is no such offer (it lapsed, or was answered).
    NoOffer,
    NoGold,
    /// The summoning circle's square is too full for the mercenaries.
    NoRoom,
    Hero(HeroError),
}

impl OfferError {
    /// The key of its sentence in `content/text` (task 348).
    pub fn key(self) -> &'static str {
        match self {
            Self::NoOffer => "refuse_wizard.offer_error_no_offer",
            Self::NoGold => "refuse_wizard.offer_error_no_gold",
            Self::NoRoom => "refuse_wizard.offer_error_no_room",
            Self::Hero(e) => e.key(),
        }
    }
}

/// The chance in percent of an offer: `base + fame / per`, doubled for a Famous wizard, at most 10.
pub fn offer_chance(base: i32, per: i32, fame: i32, famous: bool) -> i32 {
    let chance = base + fame / per;
    let chance = if famous { chance * 2 } else { chance };
    chance.min(MAX_CHANCE)
}

/// The merchant's chance.
pub fn item_chance(fame: i32, famous: bool) -> i32 {
    offer_chance(2, 25, fame, famous)
}

/// The mercenaries' chance; the computer wizards get ten more.
pub fn mercenary_chance(fame: i32, famous: bool, computer: bool) -> i32 {
    offer_chance(1, 20, fame, famous) + if computer { AI_BONUS } else { 0 }
}

/// The hero's chance: the computer wizards get ten more, and the chance is divided by
/// `(heroes + 1) / 2 + 1`, so it falls as the wizard fills their slots.
pub fn hero_chance(fame: i32, famous: bool, computer: bool, heroes: usize) -> i32 {
    let chance = offer_chance(3, 25, fame, famous) + if computer { AI_BONUS } else { 0 };
    chance / ((heroes as i32 + 1) / 2 + 1)
}

/// The level of a mercenary company from a `Random(100)` roll: above 90 (with fame added) level
/// 3, above 60 level 2, else level 1. The original tests "above 90" twice, so it never offers
/// level 2.
pub fn mercenary_level(roll: i32, fame: i32) -> u8 {
    match roll + fame {
        91.. => 3,
        61..=90 => 2,
        _ => 1,
    }
}

/// The size of a company from a `Random(100)` roll: above 90 three units, above 60 two.
pub fn mercenary_count(roll: i32, fame: i32) -> u32 {
    match roll + fame {
        91.. => 3,
        61..=90 => 2,
        _ => 1,
    }
}

/// What the company costs in all: `cost * (3 + level) / 2` a unit, half for a Charismatic wizard.
pub fn mercenary_price(cost: u32, level: u8, count: u32, charismatic: bool) -> i32 {
    let price = (cost as i32 * (3 + i32::from(level)) / 2) * count as i32;
    if charismatic {
        price / 2
    } else {
        price
    }
}

/// What the merchant asks: three times the item's value, half for a Charismatic wizard.
pub fn item_price(cost: u32, charismatic: bool) -> i32 {
    let price = cost as i32 * 3;
    if charismatic {
        price / 2
    } else {
        price
    }
}

/// The computer wizard hires a hero whose price is at most nine tenths of its gold
/// (`AI_Accept_Hero`: the original's comment says so, its arithmetic allows a price a ninth over
/// the gold).
pub fn ai_accepts_hero(price: i32, gold: i32) -> bool {
    price * 100 <= gold * 90
}

/// The computer wizard buys the merchant's item when it has a hero to carry it and the price is
/// at most half its gold (the original's merchant calls on the human only; computer wizards meet
/// him too here, `divergences.md` section 11).
pub fn ai_accepts_item(price: i32, gold: i32, heroes: usize) -> bool {
    heroes > 0 && price <= gold / 2
}

/// The computer wizard hires mercenaries from turn 50 if the price is at most half its gold.
pub fn ai_accepts_mercenaries(turn: u32, price: i32, gold: i32) -> bool {
    turn >= AI_FIRST_MERCENARY_TURN && price <= gold / 2
}

/// Which units a company can be drawn from: ordinary race units, not settlers or boats and not
/// the Draconian engineers (Draconians fly and need no roads), and only of the races that live
/// on a plane the wizard has a city or unit on (`Generate_Mercenaries`).
pub fn mercenary_pool(rules: &Rules, arcanus: bool, myrror: bool) -> Vec<&UnitDef> {
    rules
        .units
        .defs()
        .iter()
        .filter(|u| {
            u.kind == UnitKind::Normal
                && u.movement != MovementType::Sailing
                && !u.abilities.iter().any(|a| a == "create_outpost")
                && u.id != DRACONIAN_ENGINEERS
                && u.race
                    .as_deref()
                    .and_then(|r| rules.races.by_name(r))
                    .is_some_and(|race| if race.myrror { myrror } else { arcanus })
        })
        .collect()
}

impl Game {
    /// What a wizard has been offered this turn and not answered.
    pub fn offers(&self, player: PlayerId) -> Vec<Offer> {
        self.player_component::<Offers>(player)
            .map_or_else(Vec::new, |o| o.0.clone())
    }

    /// The planes on which the wizard has a city or a unit: `(Arcanus, Myrror)`.
    fn planes_of(&self, player: PlayerId) -> (bool, bool) {
        use crate::map::Plane;
        let units = self.units();
        let mut planes: Vec<Plane> = units
            .player_units(player)
            .into_iter()
            .filter_map(|u| units.unit_position(u))
            .map(|p| p.plane)
            .collect();
        planes.extend(
            self.economy()
                .cities()
                .into_iter()
                .filter(|(e, ..)| self.economy().city_owner(*e) == Some(player))
                .map(|(.., pos)| pos.plane),
        );
        (
            planes.contains(&Plane::Arcanus),
            planes.contains(&Plane::Myrror),
        )
    }

    /// Roll the offers of every wizard for the turn that begins (`Determine_Offer`). Offers not
    /// answered last turn lapse first.
    pub(crate) fn determine_offers(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let ids: Vec<PlayerId> = crate::player::ids(&self.world);
        for &id in &ids {
            let old = self
                .player_component_mut::<Offers>(id)
                .map(|o| std::mem::take(&mut o.0))
                .unwrap_or_default();
            for offer in &old {
                self.release_offer(id, offer);
            }
        }
        for id in ids {
            for offer in self.offers_for(id) {
                if let Some(o) = self.player_component_mut::<Offers>(id) {
                    o.0.push(offer.clone());
                }
                events.push(Event::OfferMade { player: id, offer });
            }
        }
        events
    }

    /// An offer that was not taken is withdrawn: its item is gone from the game and a prefab one
    /// can be offered again.
    fn release_offer(&mut self, player: PlayerId, offer: &Offer) {
        if let Offer::Item { item, .. } = offer {
            if let Some(entity) = self.offered_item(player, item) {
                self.despawn_item(entity);
            }
            self.release_prefab(item);
        }
    }

    /// A prefab item that was not bought can be offered again.
    fn release_prefab(&mut self, item: &Item) {
        if let Some(id) = &item.prefab {
            self.prefab_taken.retain(|t| t != id);
        }
    }

    /// The item entity of an offer to `player` (offers carry the item's description; the entity
    /// is held as `Offered`).
    fn offered_item(&self, player: PlayerId, item: &Item) -> Option<hecs::Entity> {
        self.items()
            .held(|h| h == crate::item::entity::HeldBy::Offered(player))
            .into_iter()
            .find(|(_, i)| i == item)
            .map(|(e, _)| e)
    }

    /// One wizard's offers this turn, in the original's order: item, mercenaries, hero.
    fn offers_for(&mut self, player: PlayerId) -> Vec<Offer> {
        let mut offers = Vec::new();
        if !self.can_be_offered(player) {
            return offers;
        }
        let famous = self.wizard(player).is_some_and(|m| m.has(Retort::Famous));
        let fame = self.fame(player);
        let computer = self.human() != Some(player);

        if self.rng.roll(100) as i32 <= item_chance(fame, famous) {
            offers.extend(self.item_offer(player));
        }
        if self.rng.roll(100) as i32 <= mercenary_chance(fame, famous, computer) {
            offers.extend(self.mercenary_offer(player));
        }
        let heroes = self.units().hero_units(player).len();
        if self.rng.roll(100) as i32 <= hero_chance(fame, famous, computer, heroes) {
            offers.extend(self.hero_offer(player));
        }
        offers
    }

    /// Wizards with a standing fortress are offered things; independent cities, banished and
    /// defeated wizards are not.
    fn can_be_offered(&self, player: PlayerId) -> bool {
        !self.is_neutral_player(player)
            && !self.is_defeated(player)
            && self
                .wizard(player)
                .is_some_and(|w| !w.is_banished() && w.summon_point().is_some())
    }

    fn charismatic(&self, player: PlayerId) -> bool {
        self.wizard(player)
            .is_some_and(|m| m.has(Retort::Charismatic))
    }

    /// The merchant's item, if the wizard can pay for it (`Make_Item`). An item the wizard cannot pay for is not offered and a prefab one is given back.
    fn item_offer(&mut self, player: PlayerId) -> Option<Offer> {
        let item = make_item(
            &mut self.rng,
            &self.content.rules,
            Grade::Any,
            0,
            &mut self.prefab_taken,
        );
        let price = item_price(item.cost, self.charismatic(player));
        if price <= self.treasury(player).map_or(0, |p| p.gold) {
            self.spawn_item(item.clone(), crate::item::entity::HeldBy::Offered(player));
            Some(Offer::Item { item, price })
        } else {
            self.release_prefab(&item);
            None
        }
    }

    /// A hero for hire, if the wizard has a slot free and somebody to offer.
    fn hero_offer(&mut self, player: PlayerId) -> Option<Offer> {
        if self.units().open_hero_slots(player) == 0 {
            return None;
        }
        let kind = self.pick_hero(player, false, Class::Any)?;
        let price = self.units().hero_hire_price(player, kind)?;
        Some(Offer::Hero { kind, price })
    }

    /// `Generate_Mercenaries`: a company, if the turn is late enough, there is something to
    /// offer, it fits the summoning circle's square and the wizard can pay.
    fn mercenary_offer(&mut self, player: PlayerId) -> Option<Offer> {
        let fame = self.fame(player);
        let charismatic = self.charismatic(player);
        let gold = self.treasury(player).map_or(0, |p| p.gold);
        if self.turn < FIRST_MERCENARY_TURN {
            return None;
        }
        let (arcanus, myrror) = self.planes_of(player);
        let pool = mercenary_pool(&self.content.rules, arcanus, myrror);
        if pool.is_empty() {
            return None;
        }
        let def = pool[self.rng.roll(pool.len() as u32) as usize - 1];
        let (id, cost) = (def.id.clone(), def.cost);
        let level = mercenary_level(self.rng.roll(100) as i32, fame);
        let count = mercenary_count(self.rng.roll(100) as i32, fame);
        let price = mercenary_price(cost, level, count, charismatic);
        let at = self.summon_point(player)?;
        let room = self.units().units_at(at).len() + count as usize <= MAX_STACK;
        (price <= gold && room).then_some(Offer::Mercenaries {
            unit: id,
            count,
            level,
            price,
        })
    }

    /// Make the wizard an offer of this kind now, whatever the chance (the console's `offer`):
    /// the same offer the turn change would make, put on the list. `None` when there is nothing
    /// to offer (no free slot, too early for mercenaries, not enough gold).
    pub fn cheat_offer(&mut self, player: PlayerId, what: OfferKind) -> Option<Offer> {
        if !self.can_be_offered(player) {
            return None;
        }
        let offer = match what {
            OfferKind::Hero => self.hero_offer(player),
            OfferKind::Mercenaries => self.mercenary_offer(player),
            OfferKind::Item => self.item_offer(player),
        }?;
        self.player_component_mut::<Offers>(player)?
            .0
            .push(offer.clone());
        Some(offer)
    }

    /// Take the offer: pay, and the hero joins, the company arrives or the item goes to the
    /// armory. `name` renames a hero before it joins.
    pub fn accept_offer(
        &mut self,
        player: PlayerId,
        index: usize,
        name: Option<String>,
    ) -> Result<Vec<Event>, OfferError> {
        let offer = self
            .player_component::<Offers>(player)
            .and_then(|o| o.0.get(index).cloned())
            .ok_or(OfferError::NoOffer)?;
        if self.treasury(player).map_or(0, |t| t.gold) < offer.price() {
            return Err(OfferError::NoGold);
        }
        let taken = offer.clone();
        let events = match offer {
            Offer::Hero { kind, price } => {
                // Another wizard may have hired the hero meanwhile.
                let hero = self
                    .units()
                    .hero_entity(kind)
                    .filter(|&h| self.units().hero_state(h) == Some(crate::hero::HeroState::Free))
                    .ok_or(OfferError::Hero(crate::hero::HeroError::NotAvailable))?;
                if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
                    if let Ok(mut h) = self.world.get::<&mut crate::hero::Hero>(hero) {
                        h.name = name;
                    }
                }
                // A prisoner freed in a lair costs nothing.
                let unit = if price == 0 {
                    self.arrive(player, kind)
                } else {
                    self.hire_hero(player, kind)
                }
                .map_err(OfferError::Hero)?;
                vec![Event::HeroJoined { player, unit, kind }]
            }
            Offer::Mercenaries {
                unit,
                count,
                level,
                price,
            } => {
                let at = self.summon_point(player).ok_or(OfferError::NoRoom)?;
                if self.units().units_at(at).len() + count as usize > MAX_STACK {
                    return Err(OfferError::NoRoom);
                }
                let id = self
                    .content
                    .rules
                    .units
                    .id_of(&unit)
                    .ok_or(OfferError::NoOffer)?;
                let xp = self
                    .content
                    .rules
                    .units
                    .get(id)
                    .level_path
                    .as_deref()
                    .and_then(|p| self.content.rules.level_paths.by_name(p))
                    .and_then(|path| {
                        // A path with fewer levels gives its highest.
                        path.levels
                            .get(usize::from(level))
                            .or_else(|| path.levels.last())
                    })
                    .map_or(0, |l| l.xp);
                let units: Vec<_> = (0..count)
                    .map(|_| {
                        let e = self.spawn_unit(player, at, id);
                        let _ = self.world.insert_one(e, Experience(xp));
                        e
                    })
                    .collect();
                if let Some(t) = self.treasury_mut(player) {
                    t.gold -= price;
                }
                vec![Event::MercenariesHired { player, units }]
            }
            Offer::Item { item, price } => {
                if let Some(t) = self.treasury_mut(player) {
                    t.gold -= price;
                }
                let name = item.name.clone();
                let placed: Placed = match self.offered_item(player, &item) {
                    Some(entity) => self.place_item(player, entity),
                    None => self.give_item(player, item),
                };
                vec![Event::ItemMade {
                    player,
                    name,
                    placed,
                }]
            }
        };
        // A hero's offers (this one too) were taken off when it was hired.
        if let Some(o) = self.player_component_mut::<Offers>(player) {
            if let Some(at) = o.0.iter().position(|x| *x == taken) {
                o.0.remove(at);
            }
        }
        Ok(events)
    }

    /// Turn the offer down.
    pub fn reject_offer(&mut self, player: PlayerId, index: usize) -> Result<(), OfferError> {
        let offer = self
            .player_component_mut::<Offers>(player)
            .filter(|o| index < o.0.len())
            .map(|o| o.0.remove(index))
            .ok_or(OfferError::NoOffer)?;
        self.release_offer(player, &offer);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
