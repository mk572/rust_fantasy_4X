//! Where a wizard's items are: the four slots of the fortress vault, the items waiting
//! after a reward, and the three slots of each hero (the [`HeroItems`] component), with the rules
//! of the item screen for moving one between them (`ItemScrn.c`, design in
//! `docs/heroes-items-design.md` section 3.4).
//!
//! The items are entities held by [`HeldBy`] (`item/entity.rs`); [`Armory`] is the
//! vault and the award list read as one value (`ItemView::armory`), which the move rules look at,
//! and `Game::move_item` and `Game::give_item` are what the commands call.

use serde::{Deserialize, Serialize};

use crate::game::Game;
use crate::item::entity::HeldBy;
use crate::item::{Item, ItemView};
use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::SlotKind;

/// The wizard's items that are not on a hero, as one value (`ItemView::armory`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Armory {
    /// The slots up to the last one with an item in it.
    pub vault: Vec<Option<Item>>,
    /// Items won or made that wait for a place.
    pub awards: Vec<Item>,
}

/// Where an item went when it was handed to the wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placed {
    Vault(usize),
    /// Waiting on the award list.
    Awards,
    /// No room anywhere: the item is lost.
    Lost,
}

impl Armory {
    /// Every item the armory holds.
    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.vault.iter().flatten().chain(self.awards.iter())
    }
}

/// A place in the armory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Spot {
    Vault(usize),
    Award(usize),
}

/// A place an item can be taken from or put in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ItemSpot {
    Vault(usize),
    Award(usize),
    /// A slot (0 to 2) of a hero unit.
    Hero(
        #[serde(with = "crate::game::replay::stable_ref::one")] hecs::Entity,
        usize,
    ),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemError {
    /// No such slot, or not the wizard's hero.
    NoSuchPlace,
    /// Nothing is there to move.
    Empty,
    /// The item does not fit the slot, or the one it would swap out does not fit the old one.
    WrongSlot,
    /// The hero is somewhere else: moving the item costs `ItemRules::transfer_mana`; ask again with
    /// `pay` set to agree to it.
    Transfer,
    /// Not enough mana for the transfer.
    NoMana,
    /// Taking the item away would leave the hero to drown.
    WouldDrown,
}

impl ItemError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::NoSuchPlace => "refuse_wizard.item_error_no_such_place",
            Self::Empty => "refuse_wizard.item_error_empty",
            Self::WrongSlot => "refuse_wizard.item_error_wrong_slot",
            Self::Transfer => "refuse_wizard.item_error_transfer",
            Self::NoMana => "refuse_wizard.item_error_no_mana",
            Self::WouldDrown => "refuse_wizard.item_error_would_drown",
        }
    }
}

/// What the item screen's rules ask of the world: a hero's slots, items and place, and where
/// a wizard's fortress is. [`ItemView`] answers for the game; a test can answer with a fake.
pub trait ItemWorld {
    /// The three slot kinds of `unit`, if it is a hero of `player`.
    fn hero_slots(&self, player: PlayerId, unit: hecs::Entity) -> Option<[SlotKind; 3]>;
    /// The item in a slot (0 to 2) of a hero.
    fn hero_item(&self, unit: hecs::Entity, slot: usize) -> Option<Item>;
    /// The square a hero stands on.
    fn hero_position(&self, unit: hecs::Entity) -> Option<Position>;
    /// Where the wizard's fortress is.
    fn fortress(&self, player: PlayerId) -> Option<Position>;
    /// What moving an item to a hero somewhere else costs, in mana.
    fn transfer_mana(&self) -> i32;
}

/// A move the rules have allowed, ready to be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovePlan {
    /// The item being moved.
    pub moving: Item,
    /// What it swaps with, which goes back to where `moving` came from.
    pub other: Option<Item>,
    /// The mana it costs (0 when the move is free).
    pub cost: i32,
}

/// The item at a spot (`Ok(None)` when the slot is empty).
fn item_at(
    world: &dyn ItemWorld,
    armory: &Armory,
    player: PlayerId,
    spot: ItemSpot,
) -> Result<Option<Item>, ItemError> {
    match spot {
        ItemSpot::Vault(i) => Ok(armory.vault.get(i).cloned().flatten()),
        ItemSpot::Award(i) => Ok(armory.awards.get(i).cloned()),
        ItemSpot::Hero(unit, i) => {
            world
                .hero_slots(player, unit)
                .ok_or(ItemError::NoSuchPlace)?;
            if i >= 3 {
                return Err(ItemError::NoSuchPlace);
            }
            Ok(world.hero_item(unit, i))
        }
    }
}

/// Whether `item` may be put in the spot: the vault takes anything, the award list nothing, a
/// hero's slot what [`SlotKind::fits`] says.
fn fits(world: &dyn ItemWorld, player: PlayerId, spot: ItemSpot, item: &Item) -> bool {
    match spot {
        ItemSpot::Vault(_) => true,
        ItemSpot::Award(_) => false,
        ItemSpot::Hero(unit, i) => world
            .hero_slots(player, unit)
            .is_some_and(|kinds| kinds[i].fits(item.kind)),
    }
}

/// Whether moving an item between the two spots is free: into the vault always; from the
/// vault to a hero standing in the fortress; between two heroes on one square. The award
/// list is where the wizard is, which the game does not track, so it is free everywhere.
fn move_is_free(world: &dyn ItemWorld, player: PlayerId, from: ItemSpot, to: ItemSpot) -> bool {
    let place = |spot: ItemSpot| match spot {
        ItemSpot::Hero(unit, _) => world.hero_position(unit),
        _ => None,
    };
    match (from, to) {
        (_, ItemSpot::Vault(_)) => true,
        (ItemSpot::Award(_), _) => true,
        (ItemSpot::Vault(_), ItemSpot::Hero(..)) => {
            place(to).is_some() && place(to) == world.fortress(player)
        }
        (ItemSpot::Hero(..), ItemSpot::Hero(..)) => {
            place(from).is_some() && place(from) == place(to)
        }
        _ => true,
    }
}

/// The item screen's rules for moving the item at `from` to `to`, swapping with what is there if
/// it can go back (`Item_Screen`'s pick up and put down, in one step). A hero's slot takes what
/// [`SlotKind::fits`] says; the award list takes nothing; an item that would be swapped back must
/// fit the slot it comes from. Moving to a hero somewhere else costs `ItemRules::transfer_mana`, which
/// `pay` agrees to and `mana` (the wizard's) must cover. Nothing is changed: the caller applies
/// the plan, and checks what the plan cannot see (a hero left to drown at sea).
pub fn plan_move(
    world: &dyn ItemWorld,
    armory: &Armory,
    mana: i32,
    player: PlayerId,
    from: ItemSpot,
    to: ItemSpot,
    pay: bool,
) -> Result<MovePlan, ItemError> {
    let moving = item_at(world, armory, player, from)?.ok_or(ItemError::Empty)?;
    let other = item_at(world, armory, player, to)?;
    if !fits(world, player, to, &moving) {
        return Err(ItemError::WrongSlot);
    }
    if let Some(other) = &other {
        // What comes out goes back where the first item was (an award slot takes it back).
        let back = matches!(from, ItemSpot::Award(_)) || fits(world, player, from, other);
        if !back {
            return Err(ItemError::WrongSlot);
        }
    }
    let cost = if move_is_free(world, player, from, to) {
        0
    } else {
        if !pay {
            return Err(ItemError::Transfer);
        }
        let price = world.transfer_mana();
        if mana < price {
            return Err(ItemError::NoMana);
        }
        price
    };
    Ok(MovePlan {
        moving,
        other,
        cost,
    })
}

impl ItemWorld for ItemView<'_> {
    fn hero_slots(&self, player: PlayerId, unit: hecs::Entity) -> Option<[SlotKind; 3]> {
        let units = self.units();
        if units.unit_owner(unit) != Some(player) {
            return None;
        }
        Some(units.unit_def(unit)?.hero.as_ref()?.kind.slots())
    }

    fn hero_item(&self, unit: hecs::Entity, slot: usize) -> Option<Item> {
        self.hero_item_entity(unit, slot).and_then(|e| self.item(e))
    }

    fn hero_position(&self, unit: hecs::Entity) -> Option<Position> {
        self.units().unit_position(unit)
    }

    fn fortress(&self, player: PlayerId) -> Option<Position> {
        crate::wizard::entity::find(self.world, player)?.fortress()
    }

    fn transfer_mana(&self) -> i32 {
        self.content.rules.config.item.transfer_mana
    }
}

impl Game {
    /// Who holds a spot of the player's.
    fn holder_of(player: PlayerId, spot: ItemSpot) -> HeldBy {
        match spot {
            ItemSpot::Vault(i) => HeldBy::Vault(player, i as u16),
            ItemSpot::Award(_) => HeldBy::Award(player),
            ItemSpot::Hero(unit, i) => HeldBy::Hero(unit, i as u8),
        }
    }

    /// The item entity at a spot.
    pub(crate) fn item_entity_at(&self, player: PlayerId, spot: ItemSpot) -> Option<hecs::Entity> {
        let items = self.items();
        match spot {
            ItemSpot::Vault(i) => items.vault_item_entity(player, i),
            ItemSpot::Award(i) => items.award_entities(player).get(i).copied(),
            ItemSpot::Hero(unit, i) => items.hero_item_entity(unit, i),
        }
    }

    /// Take the item at a spot out of the game (scrapped, or released with its hero).
    pub(crate) fn remove_item_at(&mut self, player: PlayerId, spot: ItemSpot) -> Option<Item> {
        let entity = self.item_entity_at(player, spot)?;
        let item = self.items().item(entity);
        self.despawn_item(entity);
        item
    }

    /// The item screen: move the item at `from` to `to` by the rules of [`plan_move`], then
    /// apply it. The one thing checked here is the drowning of a hero that loses its flight or
    /// water walking over the sea, which is refused and put back.
    pub fn move_item(
        &mut self,
        player: PlayerId,
        from: ItemSpot,
        to: ItemSpot,
        pay: bool,
    ) -> Result<(), ItemError> {
        if from == to {
            return Ok(());
        }
        let plan = {
            let armory = self.items().armory(player);
            let mana = self.player(player).map_or(0, |p| p.treasury().mana.floor());
            plan_move(&self.items(), &armory, mana, player, from, to, pay)?
        };
        let heroes: Vec<hecs::Entity> = [from, to]
            .iter()
            .filter_map(|s| match s {
                ItemSpot::Hero(unit, _) => Some(*unit),
                _ => None,
            })
            .collect();
        let stranded = |game: &Game| -> Vec<hecs::Entity> {
            game.economy()
                .stranded()
                .into_iter()
                .map(|(_, unit)| unit)
                .filter(|unit| heroes.contains(unit))
                .collect()
        };
        let before = stranded(self);
        let moving = self.item_entity_at(player, from).ok_or(ItemError::Empty)?;
        let other = self.item_entity_at(player, to);
        let (from_by, to_by) = (Self::holder_of(player, from), Self::holder_of(player, to));
        self.swap_holders(moving, other, from_by, to_by);
        if stranded(self).iter().any(|unit| !before.contains(unit)) {
            self.swap_holders(moving, other, to_by, from_by);
            return Err(ItemError::WouldDrown);
        }
        if let Some(t) = self.treasury_mut(player) {
            t.mana -= plan.cost;
        }
        Ok(())
    }

    /// `moving` goes from the holder `from_by` to `to_by`, and `other`, if any, comes back.
    fn swap_holders(
        &mut self,
        moving: hecs::Entity,
        other: Option<hecs::Entity>,
        from_by: HeldBy,
        to_by: HeldBy,
    ) {
        if let Some(other) = other {
            self.hold_item(other, from_by);
        }
        self.hold_item(moving, to_by);
    }

    /// Hand a wizard an item (a reward, a made item): the vault, else the award list.
    pub fn give_item(&mut self, player: PlayerId, item: Item) -> Placed {
        if self.player_entity(player).is_none() {
            return Placed::Lost;
        }
        let entity = self.spawn_item(item, HeldBy::Award(player));
        self.place_item(player, entity)
    }

    /// Take an item out of a vault or award slot to be scrapped or sold. Everything that can
    /// refuse is checked before anything is changed, so a refusal leaves the item where it is:
    /// [`ItemError::NoSuchPlace`] for a wizard that does not exist (it has no treasury to pay
    /// into), [`ItemError::Empty`] for a slot with no item.
    pub(crate) fn take_item_to_sell(
        &mut self,
        player: PlayerId,
        from: Spot,
    ) -> Result<Item, ItemError> {
        let spot = match from {
            Spot::Vault(i) => ItemSpot::Vault(i),
            Spot::Award(i) => ItemSpot::Award(i),
        };
        if self.treasury_mut(player).is_none() {
            return Err(ItemError::NoSuchPlace);
        }
        self.remove_item_at(player, spot).ok_or(ItemError::Empty)
    }

    /// The anvil: destroy the item in a vault slot or on the award list for half its value in
    /// mana. Returns the mana gained, or why not (the item stays where it was).
    pub fn scrap_item(&mut self, player: PlayerId, from: Spot) -> Result<(Item, i32), ItemError> {
        let item = self.take_item_to_sell(player, from)?;
        let mana = item.anvil_mana();
        if let Some(treasury) = self.treasury_mut(player) {
            treasury.mana += mana;
        }
        Ok((item, mana))
    }
}
