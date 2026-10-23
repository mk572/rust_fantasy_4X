//! Where a wizard's items are (task 49): the four slots of the fortress vault, the items waiting
//! after a reward, and the three slots of each hero (the [`HeroItems`] component), with the rules
//! of the item screen for moving one between them (`ItemScrn.c`, design in
//! `docs/heroes-items-design.md` section 3.4).
//!
//! The items are entities held by [`HeldBy`] (task 333, `item/entity.rs`); [`Armory`] is the
//! vault and the award list read as one value (`ItemView::armory`), which the move rules look at,
//! and `Game::move_item` and `Game::give_item` are what the commands call.

use serde::{Deserialize, Serialize};

use crate::game::Game;
use crate::item::entity::HeldBy;
use crate::item::{Item, ItemView};
use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::SlotKind;

/// The vault slots the item screen shows at a time. The vault itself has no limit (the original's
/// four are a screen size here): the screen scrolls through the rest.
pub const VAULT_SLOTS: usize = 4;
/// The most items that wait on the award list. Nothing is put there since the vault has room for
/// all; the list remains for the item screen's "held" item of a reward.
pub const MAX_AWARDS: usize = 6;
/// What moving an item to a hero somewhere else costs, in mana (the original asks first).
pub const TRANSFER_MANA: i32 = 20;

/// The wizard's items that are not on a hero, as one value (`ItemView::armory`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Armory {
    /// The slots up to the last one with an item in it.
    pub vault: Vec<Option<Item>>,
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
    /// The hero is somewhere else: moving the item costs [`TRANSFER_MANA`]; ask again with
    /// `pay` set to agree to it.
    Transfer,
    /// Not enough mana for the transfer.
    NoMana,
    /// Taking the item away would leave the hero to drown.
    WouldDrown,
}

impl ItemError {
    /// The key of its sentence in `content/text` (task 348).
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
/// fit the slot it comes from. Moving to a hero somewhere else costs [`TRANSFER_MANA`], which
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
        if mana < TRANSFER_MANA {
            return Err(ItemError::NoMana);
        }
        TRANSFER_MANA
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
            let mana = self.player(player).map_or(0, |p| p.treasury().mana);
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

    /// The anvil: destroy the item in a vault slot or on the award list for half its value in
    /// mana. Returns the mana gained.
    pub fn scrap_item(&mut self, player: PlayerId, from: Spot) -> Option<i32> {
        let spot = match from {
            Spot::Vault(i) => ItemSpot::Vault(i),
            Spot::Award(i) => ItemSpot::Award(i),
        };
        let item = self.remove_item_at(player, spot)?;
        let mana = item.anvil_mana();
        self.treasury_mut(player)?.mana += mana;
        Some(mana)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::{content_with_magic, unit};
    use crate::item::ItemBonus;
    use crate::map::{Grid, Map, Plane};
    use crate::rules::effects::{Effect, EffectRule};
    use crate::rules::{HeroDef, HeroKind, ItemKind, MovementType, Registry, Stat, UnitKind};

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
            icon: 0,
            bonus: ItemBonus::default(),
            powers: Vec::new(),
            charges: None,
            prefab: None,
            made: None,
        }
    }

    /// Row 4 of a 14x8 map is grass up to x4, then sea to x9. A warrior hero and a mage hero.
    fn world() -> Game {
        let mut c = content_with_magic();
        // Flight as this test reads it: a lasting spell that gives the ability to fly.
        let mut spells = c.rules.spells.defs().to_vec();
        for flight in spells.iter_mut().filter(|s| s.id == "flight") {
            flight.effects = vec![EffectRule {
                when: crate::rules::effects::Trigger::Passive,
                condition: crate::rules::effects::Condition::Always,
                target: crate::rules::effects::TargetSel::This,
                effect: Effect::GrantAbility("flying".into()),
            }];
        }
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
        c.rules.item_powers = Registry::new(vec![
            crate::game::testutil::item_power("flight", Some("flight"), None),
            crate::game::testutil::item_power("flaming", None, None),
        ]);
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
        g.add_player(P0, "A");
        g.add_player(P1, "B");
        g.wiz_mut(0)
            .put(Some(crate::wizard::fortress::Fortress(at(1, 1))));
        g.tr_mut(0).mana = 100;
        g
    }

    fn hero(g: &mut Game, id: &str, pos: Position) -> hecs::Entity {
        let u = g.content.rules.units.id_of(id).unwrap();
        g.spawn_unit(P0, pos, u)
    }

    #[test]
    fn the_vault_has_no_limit_and_items_fill_the_first_free_slot() {
        let mut g = world();
        for i in 0..10 {
            assert_eq!(
                g.give_item(P0, item("v", ItemKind::Misc)),
                Placed::Vault(i),
                "item {i}"
            );
        }
        let armory = g.items().armory(P0);
        assert_eq!(armory.items().count(), 10);
        assert!(armory.awards.is_empty());
        // A slot that is emptied is filled first.
        assert!(g.scrap_item(P0, Spot::Vault(3)).is_some());
        assert_eq!(
            g.give_item(P0, item("again", ItemKind::Misc)),
            Placed::Vault(3)
        );
        // A player that does not exist loses the item.
        assert_eq!(
            g.give_item(PlayerId(77), item("x", ItemKind::Misc)),
            Placed::Lost
        );
    }

    #[test]
    fn the_anvil_gives_half_the_value_in_mana() {
        let mut g = world();
        let mut staff = item("Staff", ItemKind::Staff);
        staff.cost = 900;
        g.give_item(P0, staff);
        assert_eq!(g.scrap_item(P0, Spot::Vault(0)), Some(450));
        assert_eq!(g.tr(0).mana, 550);
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
        assert_eq!(g.items().armory(P0).vault.get(1).cloned().flatten(), None);
        assert_eq!(g.items().carried(w).len(), 2);
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
        assert_eq!(
            g.items()
                .armory(P0)
                .vault
                .get(1)
                .cloned()
                .flatten()
                .as_ref()
                .unwrap()
                .name,
            "Sword"
        );
        assert_eq!(g.items().carried(w)[0].1.name, "Axe");
        // Armour does not go in the sword slot, and nothing changes.
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(2), ItemSpot::Hero(w, 0), false),
            Err(ItemError::WrongSlot)
        );
        assert_eq!(g.items().carried(w)[0].1.name, "Axe");
        assert_eq!(
            g.items()
                .armory(P0)
                .vault
                .get(2)
                .cloned()
                .flatten()
                .as_ref()
                .unwrap()
                .name,
            "Plate"
        );
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
        g.tr_mut(0).mana = 19;
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(0), ItemSpot::Hero(far, 0), true),
            Err(ItemError::NoMana)
        );
        g.tr_mut(0).mana = 100;
        assert_eq!(
            g.move_item(P0, ItemSpot::Vault(0), ItemSpot::Hero(far, 0), true),
            Ok(())
        );
        assert_eq!(g.tr(0).mana, 80);
        // Back into the vault is free, wherever the hero is.
        assert_eq!(
            g.move_item(P0, ItemSpot::Hero(far, 0), ItemSpot::Vault(0), false),
            Ok(())
        );
        assert_eq!(g.tr(0).mana, 80);
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
        g.tr_mut(0).mana = 0;
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
        g.wiz_mut(0)
            .put(Some(crate::wizard::fortress::Fortress(at(6, 4))));
        g.move_item(P0, ItemSpot::Vault(0), ItemSpot::Hero(m, 2), false)
            .unwrap();
        assert_eq!(
            g.move_item(P0, ItemSpot::Hero(m, 2), ItemSpot::Vault(0), false),
            Err(ItemError::WouldDrown)
        );
        assert_eq!(g.items().carried(m).len(), 1);
        assert_eq!(g.items().armory(P0).vault.first().cloned().flatten(), None);
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
        g.spawn_item(
            item("Offered", ItemKind::Misc),
            crate::item::entity::HeldBy::Offered(P1),
        );
        g.wiz_mut(0).put::<crate::wizard::fortress::Fortress>(None);
        let text = g.snapshot().to_text().unwrap();
        let back = crate::game::save::Snapshot::from_text(&text)
            .unwrap()
            .restore(g.content.clone())
            .unwrap();
        assert_eq!(back.items().armory(P0), g.items().armory(P0));
        assert_eq!(
            back.items()
                .held(|h| h == crate::item::entity::HeldBy::Offered(P1))
                .len(),
            1
        );
        assert!(back.items().armory(P0).vault.len() > VAULT_SLOTS);
        let carried = back.units().all_units();
        let hero = carried
            .iter()
            .find(|&&u| !back.items().carried(u).is_empty())
            .unwrap();
        assert_eq!(back.items().carried(*hero), g.items().carried(w));
    }

    struct FakeHero {
        unit: hecs::Entity,
        slots: [SlotKind; 3],
        at: Position,
        items: [Option<Item>; 3],
    }

    /// A world of two heroes, answered from plain data: no `Game`.
    struct Fake {
        hero: [FakeHero; 2],
        fortress: Position,
    }

    impl ItemWorld for Fake {
        fn hero_slots(&self, player: PlayerId, unit: hecs::Entity) -> Option<[SlotKind; 3]> {
            self.hero
                .iter()
                .find(|h| h.unit == unit && player == P0)
                .map(|h| h.slots)
        }

        fn hero_item(&self, unit: hecs::Entity, slot: usize) -> Option<Item> {
            self.hero.iter().find(|h| h.unit == unit)?.items[slot].clone()
        }

        fn hero_position(&self, unit: hecs::Entity) -> Option<Position> {
            self.hero.iter().find(|h| h.unit == unit).map(|h| h.at)
        }

        fn fortress(&self, _: PlayerId) -> Option<Position> {
            Some(self.fortress)
        }
    }

    fn fake() -> (Fake, hecs::Entity, hecs::Entity) {
        let mut w = hecs::World::new();
        let (a, b) = (w.spawn(()), w.spawn(()));
        let kinds = HeroKind::Warrior.slots();
        let hero = |unit, items| FakeHero {
            unit,
            slots: kinds,
            at: at(1, 1),
            items,
        };
        let fake = Fake {
            hero: [
                hero(a, [None, None, None]),
                hero(b, [Some(item("Old", ItemKind::Sword)), None, None]),
            ],
            fortress: at(1, 1),
        };
        (fake, a, b)
    }

    fn vault_with(items: &[(usize, ItemKind)]) -> Armory {
        let mut a = Armory::default();
        for &(i, kind) in items {
            if a.vault.len() <= i {
                a.vault.resize(i + 1, None);
            }
            a.vault[i] = Some(item("V", kind));
        }
        a
    }

    #[test]
    fn the_rules_need_no_game_to_allow_or_refuse_a_move() {
        let (w, a, _) = fake();
        let armory = vault_with(&[(0, ItemKind::Bow), (1, ItemKind::Sword)]);
        let plan = |from, to, mana, pay| plan_move(&w, &armory, mana, P0, from, to, pay);
        // A bow does not fit a warrior's sword slot; a sword does, free in the fortress.
        assert_eq!(
            plan(ItemSpot::Vault(0), ItemSpot::Hero(a, 0), 0, false),
            Err(ItemError::WrongSlot)
        );
        let ok = plan(ItemSpot::Vault(1), ItemSpot::Hero(a, 0), 0, false).unwrap();
        assert_eq!((ok.cost, ok.other), (0, None));
        // Nothing there, no such slot, someone else's hero.
        assert_eq!(
            plan(ItemSpot::Vault(2), ItemSpot::Hero(a, 0), 0, false),
            Err(ItemError::Empty)
        );
        assert_eq!(
            plan(ItemSpot::Vault(1), ItemSpot::Hero(a, 3), 0, false),
            Err(ItemError::NoSuchPlace)
        );
        assert_eq!(
            plan_move(
                &w,
                &armory,
                0,
                P1,
                ItemSpot::Vault(1),
                ItemSpot::Hero(a, 0),
                false
            ),
            Err(ItemError::NoSuchPlace)
        );
        // The award list takes nothing.
        assert_eq!(
            plan(ItemSpot::Vault(1), ItemSpot::Award(0), 0, false),
            Err(ItemError::WrongSlot)
        );
    }

    #[test]
    fn a_swap_must_fit_the_old_place_and_the_cost_needs_consent_and_mana() {
        let (mut w, a, b) = fake();
        let armory = vault_with(&[(0, ItemKind::Sword), (1, ItemKind::Bow)]);
        // The hero b already holds a sword: swapping gives it back to the vault.
        let swap = plan_move(
            &w,
            &armory,
            0,
            P0,
            ItemSpot::Vault(0),
            ItemSpot::Hero(b, 0),
            false,
        )
        .unwrap();
        assert_eq!(swap.other.as_ref().map(|i| i.name.as_str()), Some("Old"));
        // Heroes on one square trade for free; the sword cannot go into a misc slot.
        let trade = plan_move(
            &w,
            &armory,
            0,
            P0,
            ItemSpot::Hero(b, 0),
            ItemSpot::Hero(a, 0),
            false,
        );
        assert_eq!(trade.map(|p| p.cost), Ok(0));
        assert_eq!(
            plan_move(
                &w,
                &armory,
                0,
                P0,
                ItemSpot::Hero(b, 0),
                ItemSpot::Hero(a, 2),
                false
            ),
            Err(ItemError::WrongSlot)
        );
        // Move a hero away: the vault is no longer in reach, and a transfer must be agreed to
        // and paid for.
        w.hero[0].at = at(9, 9);
        let go = |mana, pay| {
            plan_move(
                &w,
                &armory,
                mana,
                P0,
                ItemSpot::Vault(0),
                ItemSpot::Hero(a, 0),
                pay,
            )
        };
        assert_eq!(go(100, false), Err(ItemError::Transfer));
        assert_eq!(go(19, true), Err(ItemError::NoMana));
        assert_eq!(go(20, true).map(|p| p.cost), Ok(TRANSFER_MANA));
        // Back into the vault is always free.
        let back = plan_move(
            &w,
            &armory,
            0,
            P0,
            ItemSpot::Hero(b, 0),
            ItemSpot::Vault(3),
            false,
        );
        assert_eq!(back.map(|p| p.cost), Ok(0));
    }
}
