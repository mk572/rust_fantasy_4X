//! Items as entities (task 333): one entity for each item that exists, with its data ([`Item`]),
//! a creation number and who holds it ([`HeldBy`]). Moving an item changes its `HeldBy`; an item
//! that nobody holds is despawned. The holders are a hero's three slots, the fortress vault, the
//! award list, an item being crafted and an item on offer.

use hecs::Entity;

use super::armory::Armory;
use super::{HeroItems, Item, ItemView};
use crate::game::serial::Serial;
use crate::game::Game;
use crate::player::PlayerId;

/// Who holds an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldBy {
    /// A slot (0 to 2) of a hero unit.
    Hero(Entity, u8),
    /// A slot of the player's fortress vault.
    Vault(PlayerId, u16),
    /// The player's list of items waiting for a place; their order is the creation order.
    Award(PlayerId),
    /// The item the player's Enchant Item or Create Artifact is making.
    Crafting(PlayerId),
    /// An item offered to the player (a merchant, a wandering trader).
    Offered(PlayerId),
}

impl HeldBy {
    /// The player whose items these are, for the holders that belong to a player.
    pub fn player(self) -> Option<PlayerId> {
        match self {
            Self::Vault(p, _) | Self::Award(p) | Self::Crafting(p) | Self::Offered(p) => Some(p),
            Self::Hero(..) => None,
        }
    }
}

/// The item a player's crafting spell is making, with its entity.
pub(crate) fn crafting_of(world: &hecs::World, player: PlayerId) -> Option<(Entity, Item)> {
    world
        .query::<(&Item, &HeldBy)>()
        .iter()
        .find(|(_, (_, h))| **h == HeldBy::Crafting(player))
        .map(|(e, (i, _))| (e, i.clone()))
}

impl<'a> ItemView<'a> {
    /// The item the player's Enchant Item or Create Artifact is making.
    pub fn crafting(&self, player: PlayerId) -> Option<Item> {
        crafting_of(self.world, player).map(|(_, i)| i)
    }

    /// The item an entity is.
    pub fn item(&self, entity: Entity) -> Option<Item> {
        self.world.get::<&Item>(entity).ok().map(|i| (*i).clone())
    }

    /// Who holds the item.
    pub fn held_by(&self, entity: Entity) -> Option<HeldBy> {
        self.world.get::<&HeldBy>(entity).ok().map(|h| *h)
    }

    /// The items `by` selects, oldest first.
    pub fn held(&self, by: impl Fn(HeldBy) -> bool) -> Vec<(Entity, Item)> {
        let mut found: Vec<(Serial, Entity, Item)> = self
            .world
            .query::<(&Item, &HeldBy, &Serial)>()
            .iter()
            .filter(|(_, (_, h, _))| by(**h))
            .map(|(e, (i, _, s))| (*s, e, i.clone()))
            .collect();
        found.sort_by_key(|(s, e, _)| (*s, e.id()));
        found.into_iter().map(|(_, e, i)| (e, i)).collect()
    }

    /// The item in a slot of a hero.
    pub fn hero_item_entity(&self, unit: Entity, slot: usize) -> Option<Entity> {
        self.held(|h| h == HeldBy::Hero(unit, slot as u8))
            .first()
            .map(|(e, _)| *e)
    }

    /// The item in a slot of the player's vault.
    pub fn vault_item_entity(&self, player: PlayerId, slot: usize) -> Option<Entity> {
        self.held(|h| h == HeldBy::Vault(player, slot as u16))
            .first()
            .map(|(e, _)| *e)
    }

    /// The items waiting on the player's award list, oldest first.
    pub fn award_entities(&self, player: PlayerId) -> Vec<Entity> {
        self.held(|h| h == HeldBy::Award(player))
            .into_iter()
            .map(|(e, _)| e)
            .collect()
    }

    /// The items of a hero by slot.
    pub fn hero_items(&self, unit: Entity) -> HeroItems {
        let mut slots = HeroItems::default();
        for (_, item, by) in self.all_held_by_hero(unit) {
            if let HeldBy::Hero(_, slot) = by {
                slots.0[slot as usize] = Some(item);
            }
        }
        slots
    }

    fn all_held_by_hero(&self, unit: Entity) -> Vec<(Entity, Item, HeldBy)> {
        self.world
            .query::<(&Item, &HeldBy)>()
            .iter()
            .filter(|(_, (_, h))| matches!(h, HeldBy::Hero(u, _) if *u == unit))
            .map(|(e, (i, h))| (e, i.clone(), *h))
            .collect()
    }

    /// The vault and the award list of a player, as one value.
    pub fn armory(&self, player: PlayerId) -> Armory {
        let mut armory = Armory::default();
        for (_, (item, by)) in self.world.query::<(&Item, &HeldBy)>().iter() {
            if let HeldBy::Vault(p, slot) = by {
                if *p == player {
                    let slot = *slot as usize;
                    if armory.vault.len() <= slot {
                        armory.vault.resize(slot + 1, None);
                    }
                    armory.vault[slot] = Some(item.clone());
                }
            }
        }
        armory.awards = self
            .held(|h| h == HeldBy::Award(player))
            .into_iter()
            .map(|(_, i)| i)
            .collect();
        armory
    }

    /// How many items `by` selects.
    pub fn count_held(&self, by: impl Fn(HeldBy) -> bool) -> usize {
        self.world
            .query::<&HeldBy>()
            .iter()
            .filter(|(_, h)| by(**h))
            .count()
    }
}

impl Game {
    /// Make an item held by `by`.
    pub(crate) fn spawn_item(&mut self, item: Item, by: HeldBy) -> Entity {
        let serial = self.take_serial();
        self.spawn_item_with(serial, item, by)
    }

    /// [`Game::spawn_item`] with a creation number already taken (a load).
    pub(crate) fn spawn_item_with(&mut self, serial: Serial, item: Item, by: HeldBy) -> Entity {
        self.world.spawn((item, by, serial))
    }

    /// Put items in a hero's slots (for tests that build a hero already armed).
    #[cfg(test)]
    pub(crate) fn arm_hero(&mut self, hero: Entity, items: [Option<Item>; 3]) {
        for (slot, item) in items.into_iter().enumerate() {
            if let Some(item) = item {
                self.spawn_item(item, HeldBy::Hero(hero, slot as u8));
            }
        }
    }

    /// Hand an item to another holder.
    ///
    /// An item that joins the award list gets a new creation number, so that the list keeps the
    /// order in which items were put on it.
    pub(crate) fn hold_item(&mut self, item: Entity, by: HeldBy) {
        if matches!(by, HeldBy::Award(_))
            && self.world.get::<&HeldBy>(item).map_or(true, |h| *h != by)
        {
            let serial = self.take_serial();
            let _ = self.world.insert_one(item, serial);
        }
        let _ = self.world.insert_one(item, by);
    }

    /// The item is gone from the game.
    pub(crate) fn despawn_item(&mut self, item: Entity) {
        let _ = self.world.despawn(item);
    }

    /// Despawn every item a hero still holds (the hero is gone, or the items were lost).
    pub(crate) fn despawn_items_of(&mut self, unit: Entity) {
        let held: Vec<Entity> = self
            .world
            .query::<&HeldBy>()
            .iter()
            .filter(|(_, h)| matches!(h, HeldBy::Hero(u, _) if *u == unit))
            .map(|(e, _)| e)
            .collect();
        for e in held {
            self.despawn_item(e);
        }
    }

    /// Put an item in the player's first free vault slot. The vault has no limit, so an item is
    /// lost only when the player does not exist.
    pub(crate) fn place_item(&mut self, player: PlayerId, item: Entity) -> super::armory::Placed {
        use super::armory::Placed;
        if self.player_entity(player).is_none() {
            self.despawn_item(item);
            return Placed::Lost;
        }
        let items = self.items();
        let slot = (0..)
            .find(|&s| items.vault_item_entity(player, s).is_none())
            .unwrap_or(0);
        self.hold_item(item, HeldBy::Vault(player, slot as u16));
        Placed::Vault(slot)
    }
}
