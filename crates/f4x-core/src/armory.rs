//! Where a wizard's items are (task 49): the four slots of the fortress vault, the items waiting
//! after a reward, and the three slots of each hero (the [`HeroItems`] component), with the rules
//! of the item screen for moving one between them (`ItemScrn.c`, design in
//! `docs/heroes-items-design.md` section 3.4).
//!
//! Nothing here is a method of the rules; [`Armory`] is plain data on the player and
//! `Game::move_item` and `Game::give_item` are what the commands call.

use serde::{Deserialize, Serialize};

use crate::city::{PlayerId, Position};
use crate::items::{HeroItems, Item};
use crate::rules::SlotKind;
use crate::Game;

/// The slots of the fortress vault.
pub const VAULT_SLOTS: usize = 4;
/// The most items that wait after a reward (`Process_Item_Pool`).
pub const MAX_AWARDS: usize = 6;
/// What moving an item to a hero somewhere else costs, in mana (the original asks first).
pub const TRANSFER_MANA: i32 = 20;

/// A wizard's items that are not on a hero.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Armory {
    pub vault: [Option<Item>; VAULT_SLOTS],
    /// Items won or made that wait for a place (up to [`MAX_AWARDS`]).
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
    /// Put an item in the first free vault slot, else on the award list, else lose it.
    pub fn place(&mut self, item: Item) -> Placed {
        if let Some(i) = self.vault.iter().position(Option::is_none) {
            self.vault[i] = Some(item);
            Placed::Vault(i)
        } else if self.awards.len() < MAX_AWARDS {
            self.awards.push(item);
            Placed::Awards
        } else {
            Placed::Lost
        }
    }

    /// The anvil: destroy the item in a vault slot or on the award list and return the mana it
    /// gives (half its value). `None` if there is no item there.
    pub fn scrap(&mut self, from: Spot) -> Option<i32> {
        let item = match from {
            Spot::Vault(i) => self.vault.get_mut(i)?.take()?,
            Spot::Award(i) => {
                if i >= self.awards.len() {
                    return None;
                }
                self.awards.remove(i)
            }
        };
        Some(item.anvil_mana())
    }

    /// Every item the armory holds.
    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.vault.iter().flatten().chain(self.awards.iter())
    }
}

/// A place in the armory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    Vault(usize),
    Award(usize),
}

/// A place an item can be taken from or put in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemSpot {
    Vault(usize),
    Award(usize),
    /// A slot (0 to 2) of a hero unit.
    Hero(hecs::Entity, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemError {
    /// No such slot, or not the wizard's hero.
    NoSuchPlace,
    /// Nothing is there to move.
    Empty,
    /// The item does not fit the slot, or the one it would swap out does not fit the old one.
    WrongSlot,
    /// The hero is somewhere else: moving the item costs [`TRANSFER_MANA`]; ask again with
    /// `pay` set to agree to it.
    Transfer,
    /// Not enough mana for the transfer.
    NoMana,
    /// Taking the item away would leave the hero to drown.
    WouldDrown,
}

impl Game {
    /// The three slot kinds of a hero unit of `player` (`None` for anything else).
    fn hero_slot_kinds(&self, player: PlayerId, unit: hecs::Entity) -> Option<[SlotKind; 3]> {
        let units = self.units();
        if units.unit_owner(unit) != Some(player) {
            return None;
        }
        Some(units.unit_def(unit)?.hero.as_ref()?.kind.slots())
    }

    fn player_index(&self, player: PlayerId) -> Option<usize> {
        self.players.iter().position(|p| p.id == player)
    }

    /// The item at a spot (`Ok(None)` when the slot is empty).
    fn item_at(&self, player: PlayerId, spot: ItemSpot) -> Result<Option<Item>, ItemError> {
        let armory = &self.players[self.player_index(player).ok_or(ItemError::NoSuchPlace)?].armory;
        match spot {
            ItemSpot::Vault(i) => armory.vault.get(i).cloned().ok_or(ItemError::NoSuchPlace),
            ItemSpot::Award(i) => Ok(armory.awards.get(i).cloned()),
            ItemSpot::Hero(unit, i) => {
                self.hero_slot_kinds(player, unit)
                    .ok_or(ItemError::NoSuchPlace)?;
                if i >= 3 {
                    return Err(ItemError::NoSuchPlace);
                }
                Ok(self
                    .world
                    .get::<&HeroItems>(unit)
                    .ok()
                    .and_then(|items| items.0[i].clone()))
            }
        }
    }

    /// Put an item (or nothing) in a spot, which has been checked to exist.
    fn set_item_at(&mut self, player: PlayerId, spot: ItemSpot, item: Option<Item>) {
        let Some(pi) = self.player_index(player) else {
            return;
        };
        match spot {
            ItemSpot::Vault(i) => self.players[pi].armory.vault[i] = item,
            ItemSpot::Award(i) => {
                let awards = &mut self.players[pi].armory.awards;
                match item {
                    Some(item) if i < awards.len() => awards[i] = item,
                    Some(item) => awards.push(item),
                    None if i < awards.len() => {
                        awards.remove(i);
                    }
                    None => {}
                }
            }
            ItemSpot::Hero(unit, i) => {
                if self.world.get::<&HeroItems>(unit).is_err() {
                    let _ = self.world.insert_one(unit, HeroItems::default());
                }
                if let Ok(mut items) = self.world.get::<&mut HeroItems>(unit) {
                    items.0[i] = item;
                }
            }
        }
    }

    fn spot_position(&self, spot: ItemSpot) -> Option<Position> {
        match spot {
            ItemSpot::Hero(unit, _) => self.units().unit_position(unit),
            _ => None,
        }
    }

    /// Whether moving an item between the two spots is free: into the vault always; from the
    /// vault to a hero standing in the fortress; between two heroes on one square. The award
    /// list is where the wizard is, which the game does not track, so it is free everywhere.
    fn move_is_free(&self, player: PlayerId, from: ItemSpot, to: ItemSpot) -> bool {
        let fortress = self
            .player_index(player)
            .and_then(|i| self.players[i].fortress);
        match (from, to) {
            (_, ItemSpot::Vault(_)) => true,
            (ItemSpot::Award(_), _) => true,
            (ItemSpot::Vault(_), ItemSpot::Hero(..)) => {
                self.spot_position(to).is_some() && self.spot_position(to) == fortress
            }
            (ItemSpot::Hero(..), ItemSpot::Hero(..)) => {
                self.spot_position(from).is_some()
                    && self.spot_position(from) == self.spot_position(to)
            }
            _ => true,
        }
    }

    /// Move the item at `from` to `to`, swapping with what is there if it is back in the old
    /// place's reach (`Item_Screen`'s pick up and put down, in one step). A hero's slot takes
    /// what [`SlotKind::fits`] says; the award list takes nothing; an item that would be
    /// swapped back must fit the slot it comes from. Moving to a hero somewhere else costs
    /// [`TRANSFER_MANA`], which `pay` agrees to; taking away an item that keeps a hero afloat
    /// at sea is refused.
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
        let moving = self.item_at(player, from)?.ok_or(ItemError::Empty)?;
        let other = self.item_at(player, to)?;
        let fits = |game: &Game, spot: ItemSpot, item: &Item| match spot {
            ItemSpot::Vault(_) => true,
            ItemSpot::Award(_) => false,
            ItemSpot::Hero(unit, i) => game
                .hero_slot_kinds(player, unit)
                .is_some_and(|kinds| kinds[i].fits(item.kind)),
        };
        if !fits(self, to, &moving) {
            return Err(ItemError::WrongSlot);
        }
        if let Some(other) = &other {
            // What comes out goes back where the first item was (an award slot takes it back).
            let back = match from {
                ItemSpot::Award(_) => true,
                _ => fits(self, from, other),
            };
            if !back {
                return Err(ItemError::WrongSlot);
            }
        }
        let free = self.move_is_free(player, from, to);
        if !free {
            if !pay {
                return Err(ItemError::Transfer);
            }
            let mana = self.players[self.player_index(player).ok_or(ItemError::NoSuchPlace)?].mana;
            if mana < TRANSFER_MANA {
                return Err(ItemError::NoMana);
            }
        }
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
        self.set_item_at(player, from, other.clone());
        self.set_item_at(player, to, Some(moving.clone()));
        // A hero that loses its flight or water walking over the sea would drown.
        if stranded(self).iter().any(|unit| !before.contains(unit)) {
            self.set_item_at(player, to, other);
            self.set_item_at(player, from, Some(moving));
            return Err(ItemError::WouldDrown);
        }
        if !free {
            if let Some(pi) = self.player_index(player) {
                self.players[pi].mana -= TRANSFER_MANA;
            }
        }
        Ok(())
    }

    /// Hand a wizard an item (a reward, a made item): the vault, else the award list.
    pub fn give_item(&mut self, player: PlayerId, item: Item) -> Placed {
        match self.player_index(player) {
            Some(pi) => self.players[pi].armory.place(item),
            None => Placed::Lost,
        }
    }

    /// The anvil: destroy the item in a vault slot or on the award list for half its value in
    /// mana. Returns the mana gained.
    pub fn scrap_item(&mut self, player: PlayerId, from: Spot) -> Option<i32> {
        let pi = self.player_index(player)?;
        let mana = self.players[pi].armory.scrap(from)?;
        self.players[pi].mana += mana;
        Some(mana)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ItemBonus;
    use crate::map::{Grid, Map, Plane};
    use crate::rules::effects::{Effect, EffectRule};
    use crate::rules::{HeroDef, HeroKind, ItemKind, MovementType, Registry, Stat, UnitKind};
    use crate::testutil::{content_with_magic, unit};

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    fn item(name: &str, kind: ItemKind) -> Item {
        Item {
            name: name.into(),
            kind,
            cost: 100,
            bonus: ItemBonus::default(),
            powers: Vec::new(),
            charges: None,
            prefab: None,
        }
    }

    /// Row 4 of a 14x8 map is grass up to x4, then sea to x9. A warrior hero and a mage hero.
    fn world() -> Game {
        let mut c = content_with_magic();
        let mut flight = c.rules.spells.defs()[0].clone();
        flight.id = "flight".into();
        flight.effects = vec![EffectRule {
            when: crate::rules::effects::Trigger::Passive,
            condition: crate::rules::effects::Condition::Always,
            target: crate::rules::effects::TargetSel::This,
            effect: Effect::GrantAbility("flying".into()),
        }];
        let mut spells = c.rules.spells.defs().to_vec();
        spells.push(flight);
        c.rules.spells = Registry::new(spells);
        let mut units = c.rules.units.defs().to_vec();
        for (id, kind) in [
            ("test_warrior", HeroKind::Warrior),
            ("test_mage", HeroKind::Mage),
        ] {
            let mut u = unit(id, MovementType::Walking, 2, &[]);
            u.kind = UnitKind::Hero;
            u.stats.insert(Stat::Melee, 3);
            u.hero = Some(HeroDef {
                kind,
                portrait: 0,
                hire_cost: 100,
            });
            units.push(u);
        }
        c.rules.units = Registry::new(units);
        let (grass, ocean) = (
            c.terrains.id_of("grassland").unwrap(),
            c.terrains.id_of("ocean").unwrap(),
        );
        let build = || {
            let mut g = Grid::new(14, 8, grass);
            for x in 5..10 {
                g.set(x, 4, ocean);
            }
            g
        };
        let mut g = Game::new(2);
        g.content = c;
        g.map = Some(Map::new(build(), build()));
        g.players = vec![
            crate::player::Player::new(P0, "A"),
            crate::player::Player::new(P1, "B"),
        ];
        g.players[0].fortress = Some(at(1, 1));
        g.players[0].mana = 100;
        g
    }

    fn hero(g: &mut Game, id: &str, pos: Position) -> hecs::Entity {
        let u = g.content.rules.units.id_of(id).unwrap();
        g.spawn_unit(P0, pos, u)
    }

    #[test]
    fn items_go_to_the_vault_then_the_award_list_then_are_lost() {
        let mut a = Armory::default();
        for i in 0..VAULT_SLOTS {
            assert_eq!(a.place(item("v", ItemKind::Misc)), Placed::Vault(i));
        }
        for _ in 0..MAX_AWARDS {
            assert_eq!(a.place(item("a", ItemKind::Misc)), Placed::Awards);
        }
        assert_eq!(a.place(item("x", ItemKind::Misc)), Placed::Lost);
        assert_eq!(a.items().count(), VAULT_SLOTS + MAX_AWARDS);
    }

    #[test]
    fn the_anvil_gives_half_the_value_in_mana() {
        let mut g = world();
        let mut staff = item("Staff", ItemKind::Staff);
        staff.cost = 900;
        g.give_item(P0, staff);
        assert_eq!(g.scrap_item(P0, Spot::Vault(0)), Some(450));
        assert_eq!(g.players[0].mana, 550);
        assert_eq!(g.scrap_item(P0, Spot::Vault(0)), None);
    }

    #[test]
    fn a_hero_takes_what_fits_its_slots() {
        let mut g = world();
        let w = hero(&mut g, "test_warrior", at(1, 1));
        g.give_item(P0, item("Bow", ItemKind::Bow));
        g.give_item(P0, item("Sword", ItemKind::Sword));
        g.give_item(P0, item("Plate", ItemKind::Platemail));
        // A warrior's slots: sword, armour, misc.
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(0), ItemSpot::Hero(w, 0), false),
            Err(ItemError::WrongSlot)
        );
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(1), ItemSpot::Hero(w, 0), false),
            Ok(())
        );
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(2), ItemSpot::Hero(w, 1), false),
            Ok(())
        );
        assert_eq!(g.players[0].armory.vault[1], None);
        assert_eq!(g.units().unit_items(w).len(), 2);
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(2), ItemSpot::Hero(w, 1), false),
            Err(ItemError::Empty)
        );
    }

    #[test]
    fn a_mage_has_a_staff_slot_and_two_misc_slots() {
        let mut g = world();
        let m = hero(&mut g, "test_mage", at(1, 1));
        g.give_item(P0, item("Wand", ItemKind::Wand));
        g.give_item(P0, item("Ring", ItemKind::Misc));
        g.give_item(P0, item("Sword", ItemKind::Sword));
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(0), ItemSpot::Hero(m, 0), false),
            Ok(())
        );
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(1), ItemSpot::Hero(m, 2), false),
            Ok(())
        );
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(2), ItemSpot::Hero(m, 1), false),
            Err(ItemError::WrongSlot)
        );
    }

    #[test]
    fn a_full_slot_swaps_with_the_item_put_in_it() {
        let mut g = world();
        let w = hero(&mut g, "test_warrior", at(1, 1));
        g.give_item(P0, item("Sword", ItemKind::Sword));
        g.give_item(P0, item("Axe", ItemKind::Axe));
        g.give_item(P0, item("Plate", ItemKind::Platemail));
        g.move_item(P0, ItemSpot::Vault(0), ItemSpot::Hero(w, 0), false)
            .unwrap();
        g.move_item(P0, ItemSpot::Vault(1), ItemSpot::Hero(w, 0), false)
            .unwrap();
        assert_eq!(g.players[0].armory.vault[1].as_ref().unwrap().name, "Sword");
        assert_eq!(g.units().unit_items(w)[0].1.name, "Axe");
        // Armour does not go in the sword slot, and nothing changes.
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(2), ItemSpot::Hero(w, 0), false),
            Err(ItemError::WrongSlot)
        );
        assert_eq!(g.units().unit_items(w)[0].1.name, "Axe");
        assert_eq!(g.players[0].armory.vault[2].as_ref().unwrap().name, "Plate");
    }

    #[test]
    fn a_hero_far_from_the_fortress_costs_mana() {
        let mut g = world();
        let far = hero(&mut g, "test_warrior", at(3, 3));
        g.give_item(P0, item("Sword", ItemKind::Sword));
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(0), ItemSpot::Hero(far, 0), false),
            Err(ItemError::Transfer)
        );
        g.players[0].mana = 19;
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(0), ItemSpot::Hero(far, 0), true),
            Err(ItemError::NoMana)
        );
        g.players[0].mana = 100;
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(0), ItemSpot::Hero(far, 0), true),
            Ok(())
        );
        assert_eq!(g.players[0].mana, 80);
        // Back into the vault is free, wherever the hero is.
        assert_eq!(
            g.move_item(P0, ItemSpot::Hero(far, 0), ItemSpot::Vault(0), false),
            Ok(())
        );
        assert_eq!(g.players[0].mana, 80);
    }

    #[test]
    fn two_heroes_on_one_square_trade_for_free() {
        let mut g = world();
        let a = hero(&mut g, "test_warrior", at(3, 3));
        let b = hero(&mut g, "test_warrior", at(3, 3));
        let c = hero(&mut g, "test_warrior", at(4, 3));
        g.give_item(P0, item("Sword", ItemKind::Sword));
        g.move_item(P0, ItemSpot::Vault(0), ItemSpot::Hero(a, 0), true)
            .unwrap();
        g.players[0].mana = 0;
        assert_eq!(
            g.move_item(P0, ItemSpot::Hero(a, 0), ItemSpot::Hero(b, 0), false),
            Ok(())
        );
        assert_eq!(
            g.move_item(P0, ItemSpot::Hero(b, 0), ItemSpot::Hero(c, 0), false),
            Err(ItemError::Transfer)
        );
    }

    #[test]
    fn the_heroes_of_another_wizard_are_out_of_reach() {
        let mut g = world();
        let u = g.content.rules.units.id_of("test_warrior").unwrap();
        let theirs = g.spawn_unit(P1, at(1, 1), u);
        g.give_item(P0, item("Sword", ItemKind::Sword));
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(0), ItemSpot::Hero(theirs, 0), false),
            Err(ItemError::NoSuchPlace)
        );
    }

    #[test]
    fn an_item_that_keeps_a_hero_afloat_cannot_be_taken_away() {
        let mut g = world();
        // The hero stands on the sea with Flight in a misc slot; it is not a flier itself.
        let m = hero(&mut g, "test_warrior", at(6, 4));
        let mut boots = item("Boots", ItemKind::Misc);
        boots.powers = vec!["flight".into()];
        g.give_item(P0, boots);
        g.players[0].fortress = Some(at(6, 4));
        g.move_item(P0, ItemSpot::Vault(0), ItemSpot::Hero(m, 2), false)
            .unwrap();
        assert_eq!(
            g.move_item(P0, ItemSpot::Hero(m, 2), ItemSpot::Vault(0), false),
            Err(ItemError::WouldDrown)
        );
        assert_eq!(g.units().unit_items(m).len(), 1);
        assert_eq!(g.players[0].armory.vault[0], None);
    }

    #[test]
    fn the_vault_the_award_list_and_a_heros_items_are_saved() {
        let mut g = world();
        let w = hero(&mut g, "test_warrior", at(1, 1));
        let mut sword = item("Sword", ItemKind::Sword);
        sword.bonus.attack = 2;
        sword.powers = vec!["flaming".into()];
        g.give_item(P0, sword);
        g.give_item(P0, item("Ring", ItemKind::Misc));
        g.move_item(P0, ItemSpot::Vault(0), ItemSpot::Hero(w, 0), false)
            .unwrap();
        for _ in 0..VAULT_SLOTS + 1 {
            g.give_item(P0, item("Filler", ItemKind::Misc));
        }
        g.players[0].fortress = None;
        let text = g.snapshot().to_text().unwrap();
        let back = crate::save::Snapshot::from_text(&text)
            .unwrap()
            .restore(g.content.clone())
            .unwrap();
        assert_eq!(back.players[0].armory, g.players[0].armory);
        assert!(!back.players[0].armory.awards.is_empty());
        let carried = back.units().all_units();
        let hero = carried
            .iter()
            .find(|&&u| !back.units().unit_items(u).is_empty())
            .unwrap();
        assert_eq!(back.units().unit_items(*hero), g.units().unit_items(w));
    }
}
