//! Magic items: what an item is, where it fits and what it does for the hero that
//! carries it.
//!
//! An [`Item`] is an instance, not an id of the content: a crafted item is not in any
//! file. Each one is an entity (`entity.rs`: the `Item` component and `HeldBy`). A prefab item is a copy
//! of its [`ItemDef`] ([`Item::from_def`]). Its effects reach the rules through the stat engine
//! ([`modifiers`]) and the unit's abilities and lasting spells ([`Item::granted_spells`],
//! [`Item::granted_abilities`]), so no rule needs a special case for items.
//!
//! The module has three parts, as units have: this file (the item, the hero's slots, the stat
//! rules and the [`ItemView`] view the other rules ask), [`armory`] (where a wizard's items are and the
//! item screen's rules) and [`crafting`] (Enchant Item and Create Artifact).
//!
//! The rules are those of the original (`COMBINIT.c:Battle_Unit_Item_Stats`, `ItemScrn.c`):
//! design in `docs/heroes-items-design.md` sections 2.1, 3.1 and 3.4.

pub mod armory;
pub mod crafting;
pub mod entity;
pub mod make;

use crate::rules::Ability;
use crate::rules::ItemId;
use crate::rules::ItemPowerId;
use f4x_def::half_int::HalfInt;
use serde::{Deserialize, Serialize};

use crate::battle::game::ranged_kind_of;
use crate::battle::RangedKind;
use f4x_def::content::Content;

use crate::game::Game;
use crate::rules::damage_type::{DamageType, DamageTypes};
use crate::rules::effects::Effect;
use crate::rules::stats::{Modifier, Source};
use crate::rules::{ItemDef, ItemKind, ItemPowerDef, ModOp, Rules, SlotKind, SpellId, Stat};
use crate::unit::UnitView;
use hecs::World;

/// The plain bonuses of an item (the original's record: attack, to hit, defence, movement,
/// resistance, spell skill and spell save).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemBonus {
    pub attack: i32,
    pub to_hit: i32,
    pub defense: i32,
    /// Whole moves. Saved as half-moves, the form older saves have.
    #[serde(with = "f4x_def::half_int::raw")]
    pub moves: HalfInt,
    pub resistance: i32,
    pub spell_skill: i32,
    /// The size of the resistance penalty put on what the hero's spells hit.
    pub spell_save: i32,
}

/// A magic item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub name: String,
    pub kind: ItemKind,
    /// The item's value in gold: what the anvil and the merchants go by.
    pub cost: u32,
    /// The picture, an entry of the original's `ITEMS.LBX`, of an item a wizard made or one found
    /// at random. A copy of a prefab item has none (0): the client's table `item_art.ron` says
    /// which picture the prefab has.
    #[serde(default)]
    pub icon: u16,
    pub bonus: ItemBonus,
    /// The special powers (numbers in `item_powers.ron`).
    pub powers: Vec<ItemPowerId>,
    /// The spell the hero can cast from the item, and how many times.
    pub charges: Option<(SpellId, u32)>,
    /// The prefab item this is a copy of; `None` for one made by a wizard or at random.
    pub prefab: Option<ItemId>,
    /// The parts of the default name of an item made without one: a language puts
    /// them in its own order. `name` is empty then.
    #[serde(default)]
    pub made: Option<crafting::MadeName>,
}

impl Item {
    /// A copy of a prefab item of the content.
    pub fn from_def(def: &ItemDef) -> Item {
        let mut bonus = ItemBonus {
            spell_skill: def.spell_skill,
            spell_save: def.spell_save,
            ..ItemBonus::default()
        };
        for rule in &def.effects {
            if let Effect::Modify {
                stat,
                op: ModOp::Add,
                value,
                ..
            } = rule.effect
            {
                match stat {
                    Stat::Melee => bonus.attack += value.floor(),
                    Stat::ToHit => bonus.to_hit += value.floor(),
                    Stat::Defense => bonus.defense += value.floor(),
                    Stat::Moves => bonus.moves += value,
                    Stat::Resistance => bonus.resistance += value.floor(),
                    _ => {}
                }
            }
        }
        Item {
            name: String::new(),
            kind: def.kind,
            cost: def.cost,
            icon: 0,
            bonus,
            powers: def.powers.clone(),
            charges: def.charges.as_ref().map(|&(spell, n)| (spell, n)),
            prefab: Some(ItemId::new(def.num)),
            made: None,
        }
    }

    /// The slot type of the item, fixed by its kind (`Create_Item_Record`).
    pub fn slot_kind(&self) -> SlotKind {
        slot_of(self.kind)
    }

    pub fn has_power(&self, power: ItemPowerId) -> bool {
        self.powers.contains(&power)
    }

    /// What the anvil gives for it: half its value in mana (help 246).
    pub fn anvil_mana(&self) -> i32 {
        (self.cost / 2) as i32
    }

    /// The lasting spells its powers put on the hero that carries it (the original turns them
    /// into unit enchantments, `Item_Powers_To_Unit_Enchantments`; they are not the wizard's, so
    /// dispelling does not touch them).
    pub fn granted_spells(&self, rules: &Rules) -> Vec<SpellId> {
        self.powers
            .iter()
            .filter_map(|&p| rules.item_powers.get(p).spell)
            .collect()
    }

    /// The abilities the item gives its carrier: a shield gives the large shield, and the attack
    /// powers give the matching special attack (`Item_Powers_To_Attack_Attributes`). The original
    /// puts an attack power on the melee or the ranged attack by the kind of the item; here the
    /// carrier has it for both.
    pub fn granted_abilities(&self, rules: &Rules) -> Vec<Ability> {
        let mut out: Vec<Ability> = Vec::new();
        if self.kind == ItemKind::Shield {
            out.push(Ability::new(34) /* large_shield */);
        }
        for &p in &self.powers {
            if let Some(ability) = rules.item_powers.get(p).attack {
                if !out.contains(&ability) {
                    out.push(ability);
                }
            }
        }
        out
    }

    /// Does the item's attack bonus count for the hero's melee attack?
    fn adds_to_melee(&self) -> bool {
        matches!(
            self.kind,
            ItemKind::Sword | ItemKind::Mace | ItemKind::Axe | ItemKind::Misc
        )
    }

    /// Does the item's attack bonus count for a hero with this ranged attack? A bow adds to
    /// arrows, a staff or wand to magic, an axe to a thrown attack and a miscellaneous item to
    /// any.
    fn adds_to_ranged(&self, kind: RangedKind, thrown: bool) -> bool {
        match self.kind {
            ItemKind::Bow => kind == RangedKind::Missile,
            ItemKind::Staff | ItemKind::Wand => kind == RangedKind::Magic,
            ItemKind::Axe => thrown,
            ItemKind::Misc => true,
            _ => false,
        }
    }
}

/// The slot type an item kind goes in.
pub fn slot_of(kind: ItemKind) -> SlotKind {
    match kind {
        ItemKind::Sword | ItemKind::Mace | ItemKind::Axe => SlotKind::Sword,
        ItemKind::Bow => SlotKind::Bow,
        ItemKind::Staff | ItemKind::Wand => SlotKind::Staff,
        ItemKind::Misc => SlotKind::Misc,
        ItemKind::Shield | ItemKind::Chainmail | ItemKind::Platemail => SlotKind::Armor,
    }
}

/// The items a hero carries, one per slot (the slot kinds are the hero's `HeroKind::slots`), as
/// one value: `ItemView::hero_items` reads it, the save writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroItems(pub [Option<Item>; 3]);

impl HeroItems {
    pub fn iter(&self) -> impl Iterator<Item = &Item> {
        self.0.iter().flatten()
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(Option::is_none)
    }

    /// Whether any item has the power.
    pub fn has_power(&self, power: ItemPowerId) -> bool {
        self.iter().any(|i| i.has_power(power))
    }
}

/// Whether the rules do something with a special power: it puts a lasting spell on the carrier,
/// gives an attack ability, adds to the blows (`melee_bonus`, `ranged_bonus`), or holds a spell
/// with charges (`holds_spell`), which the item's own charges carry out.
pub fn power_is_handled(power: &ItemPowerDef) -> bool {
    power.spell.is_some()
        || power.attack.is_some()
        || power.melee_bonus != 0
        || power.ranged_bonus != 0
        || power.holds_spell
}

/// The stat modifiers of the item in slot `slot` of a hero whose abilities are `abilities` and
/// who has a ranged attack if `has_ranged`. Defence, resistance and movement count for all
/// items; attack and to hit go to the melee attack or the ranged one by the kind of the item
/// (`Battle_Unit_Item_Stats`). A power adds its `melee_bonus` and `ranged_bonus` (`item_powers.ron`:
/// Flaming to both attacks, Giant Strength to the melee attack and a thrown axe's). The item's to hit is kept apart from the unit's
/// ([`Stat::MeleeToHit`] and [`Stat::RangedToHit`]) because it counts for one attack only.
pub fn modifiers(
    rules: &Rules,
    item: &Item,
    slot: u8,
    abilities: &[Ability],
    has_ranged: bool,
) -> Vec<Modifier> {
    let source = Source::Item(slot);
    let mut mods = Vec::new();
    let mut add = |stat: Stat, value: i32| {
        if value != 0 {
            mods.push(Modifier::new(stat, ModOp::Add, value, source));
        }
    };
    let b = &item.bonus;
    add(Stat::Defense, b.defense);
    add(Stat::Resistance, b.resistance);
    add(Stat::Moves, b.moves.floor());
    let powers: Vec<&ItemPowerDef> = item
        .powers
        .iter()
        .filter_map(|&p| rules.item_powers.try_get(p))
        .collect();
    // The bonus dice of a power with `bonus_types` are of those types (Flaming: fire); the rest
    // of the blow is the item's own.
    let mut add_typed = |stat: Stat, value: i32, types: &[DamageType]| {
        if value == 0 {
            return;
        }
        let m = Modifier::new(stat, ModOp::Add, value, source);
        mods.push(m.of_types(DamageTypes::of(types)));
    };
    let plain = |p: &&&ItemPowerDef| p.bonus_types.is_empty();
    if item.adds_to_melee() {
        let melee: i32 = powers.iter().filter(plain).map(|p| p.melee_bonus).sum();
        add_typed(Stat::Melee, b.attack + melee, &[]);
        for p in powers.iter().filter(|p| !p.bonus_types.is_empty()) {
            add_typed(Stat::Melee, p.melee_bonus, &p.bonus_types);
        }
        add_typed(Stat::MeleeToHit, b.to_hit, &[]);
    }
    let kind = ranged_kind_of(abilities, rules);
    let thrown = abilities.iter().any(|a| *a == Ability::new(69));
    if has_ranged && item.adds_to_ranged(kind, thrown) {
        let counts =
            |p: &&&ItemPowerDef| p.ranged_kinds.is_empty() || p.ranged_kinds.contains(&item.kind);
        let ranged: i32 = powers
            .iter()
            .filter(plain)
            .filter(counts)
            .map(|p| p.ranged_bonus)
            .sum();
        add_typed(Stat::Ranged, b.attack + ranged, &[]);
        for p in powers
            .iter()
            .filter(|p| !p.bonus_types.is_empty())
            .filter(counts)
        {
            add_typed(Stat::Ranged, p.ranged_bonus, &p.bonus_types);
        }
        add_typed(Stat::RangedToHit, b.to_hit, &[]);
    }
    mods
}

/// The item rules' view of the game, as [`UnitView`] is the unit rules': the world, the content and
/// the players, read-only. Get one with `Game::items()`; the unit rules build one from their own
/// view to ask what a unit carries.
#[derive(Clone, Copy)]
pub struct ItemView<'a> {
    world: &'a World,
    content: &'a Content,
}

impl<'a> ItemView<'a> {
    pub fn new(world: &'a World, content: &'a Content) -> Self {
        ItemView { world, content }
    }

    fn units(&self) -> UnitView<'a> {
        UnitView::new(self.world, self.content)
    }

    /// Is the unit of a kind that carries items?
    fn is_hero(&self, unit: hecs::Entity) -> bool {
        self.world
            .get::<&crate::unit::Unit>(unit)
            .ok()
            .is_some_and(|u| self.content.rules.units.get(u.0).hero.is_some())
    }

    /// The items a unit carries with the slot each is in (only heroes carry any).
    pub fn carried(&self, unit: hecs::Entity) -> Vec<(u8, Item)> {
        // Only heroes carry items: skip the scan of every item for the other units.
        if !self.is_hero(unit) {
            return Vec::new();
        }
        let mut out: Vec<(u8, Item)> = self
            .world
            .query::<(&Item, &entity::HeldBy)>()
            .iter()
            .filter_map(|(_, (item, by))| match by {
                entity::HeldBy::Hero(u, slot) if *u == unit => Some((*slot, item.clone())),
                _ => None,
            })
            .collect();
        out.sort_by_key(|(slot, _)| *slot);
        out
    }

    /// The items a unit carries as entities, in slot order (only heroes carry any).
    pub fn carried_entities(&self, unit: hecs::Entity) -> Vec<(hecs::Entity, Item)> {
        // Only heroes carry items: skip the scan of every item for the other units.
        if !self.is_hero(unit) {
            return Vec::new();
        }
        let mut out: Vec<(u8, hecs::Entity, Item)> = self
            .world
            .query::<(&Item, &entity::HeldBy)>()
            .iter()
            .filter_map(|(e, (item, by))| match by {
                entity::HeldBy::Hero(u, slot) if *u == unit => Some((*slot, e, item.clone())),
                _ => None,
            })
            .collect();
        out.sort_by_key(|(slot, ..)| *slot);
        out.into_iter().map(|(_, e, item)| (e, item)).collect()
    }

    /// The lasting spells the unit's items put on it (see [`Item::granted_spells`]).
    pub fn spells(&self, unit: hecs::Entity) -> Vec<SpellId> {
        let mut out = Vec::new();
        for (_, item) in self.carried(unit) {
            for id in item.granted_spells(&self.content.rules) {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        out
    }

    /// The special attack and shield abilities the unit's items give (see
    /// [`Item::granted_abilities`]), each once.
    pub fn abilities(&self, unit: hecs::Entity) -> Vec<Ability> {
        let mut out: Vec<Ability> = Vec::new();
        for (_, item) in self.carried(unit) {
            for a in item.granted_abilities(&self.content.rules) {
                if !out.contains(&a) {
                    out.push(a);
                }
            }
        }
        out
    }

    /// The stat modifiers of the unit's items (see [`modifiers`]).
    pub(crate) fn modifiers(&self, unit: hecs::Entity) -> Vec<Modifier> {
        let items = self.carried(unit);
        if items.is_empty() {
            return Vec::new();
        }
        let units = self.units();
        let abilities = units.unit_abilities(unit);
        let has_ranged = units
            .unit_def(unit)
            .is_some_and(|d| d.stats.get(&Stat::Ranged).map_or(0, |v| v.floor()) > 0);
        items
            .iter()
            .flat_map(|(slot, item)| {
                modifiers(&self.content.rules, item, *slot, &abilities, has_ranged)
            })
            .collect()
    }
}

impl Game {
    /// The item rules' view of the game.
    pub fn items(&self) -> ItemView<'_> {
        ItemView::new(&self.world, &self.content)
    }
}

impl UnitView<'_> {
    /// The item rules' view, built from this one.
    pub(crate) fn items(&self) -> ItemView<'_> {
        ItemView::new(self.world, self.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::shipped;

    /// The first shipped item (in the original's order) that `wanted` accepts, as the game
    /// hands it out.
    fn real(wanted: impl Fn(&ItemDef) -> bool) -> Item {
        let content = shipped();
        let (_, def) = content
            .rules
            .items
            .iter()
            .filter(|(_, d)| wanted(d))
            .min_by_key(|(_, d)| d.num)
            .expect("the content has such an item");
        Item::from_def(def)
    }

    fn of_kind(kind: ItemKind) -> Item {
        real(|d| d.kind == kind)
    }

    fn total(mods: &[Modifier], stat: Stat) -> i32 {
        mods.iter()
            .filter(|m| m.stat == stat)
            .map(|m| m.value.floor())
            .sum()
    }

    #[test]
    fn the_slot_follows_the_kind() {
        assert_eq!(of_kind(ItemKind::Mace).slot_kind(), SlotKind::Sword);
        assert_eq!(of_kind(ItemKind::Wand).slot_kind(), SlotKind::Staff);
        assert_eq!(of_kind(ItemKind::Bow).slot_kind(), SlotKind::Bow);
        assert_eq!(of_kind(ItemKind::Platemail).slot_kind(), SlotKind::Armor);
        assert_eq!(of_kind(ItemKind::Misc).slot_kind(), SlotKind::Misc);
        // Every shipped item's slot kind agrees with the slot its definition names.
        for (_, def) in shipped().rules.items.iter() {
            let item = Item::from_def(def);
            assert_eq!(item.kind.slot(), def.slot, "{}", def.id);
        }
    }

    #[test]
    fn a_sword_adds_attack_to_melee_only() {
        let sword = real(|d| {
            let b = Item::from_def(d).bonus;
            d.kind == ItemKind::Sword && d.powers.is_empty() && b.attack > 0 && b.to_hit > 0
        });
        let mods = modifiers(
            &shipped().rules,
            &sword,
            0,
            &[Ability::new(3 /* bow */)],
            true,
        );
        assert_eq!(total(&mods, Stat::Melee), sword.bonus.attack);
        assert_eq!(total(&mods, Stat::MeleeToHit), sword.bonus.to_hit);
        assert_eq!(total(&mods, Stat::Ranged), 0);
        assert_eq!(total(&mods, Stat::RangedToHit), 0);
    }

    #[test]
    fn a_bow_needs_arrows_and_a_staff_needs_magic() {
        let bow = real(|d| {
            d.kind == ItemKind::Bow && d.powers.is_empty() && Item::from_def(d).bonus.attack > 0
        });
        let staff = real(|d| {
            d.kind == ItemKind::Staff && d.powers.is_empty() && Item::from_def(d).bonus.attack > 0
        });
        let archer = vec![Ability::new(3 /* bow */)];
        let mage = vec![Ability::new(22 /* fireball_attack */)];
        let ranged = |item: &Item, who: &[Ability]| {
            total(
                &modifiers(&shipped().rules, item, 0, who, true),
                Stat::Ranged,
            )
        };
        assert_eq!(ranged(&bow, &archer), bow.bonus.attack);
        assert_eq!(ranged(&bow, &mage), 0);
        assert_eq!(ranged(&staff, &mage), staff.bonus.attack);
        assert_eq!(ranged(&staff, &archer), 0);
        // Neither of them touches the melee attack.
        assert_eq!(
            total(
                &modifiers(&shipped().rules, &bow, 0, &archer, true),
                Stat::Melee
            ),
            0
        );
    }

    #[test]
    fn a_misc_item_counts_for_both_attacks() {
        let ring = real(|d| {
            d.kind == ItemKind::Misc && d.powers.is_empty() && Item::from_def(d).bonus.attack > 0
        });
        let mods = modifiers(
            &shipped().rules,
            &ring,
            2,
            &[Ability::new(3 /* bow */)],
            true,
        );
        assert_eq!(total(&mods, Stat::Melee), ring.bonus.attack);
        assert_eq!(total(&mods, Stat::Ranged), ring.bonus.attack);
        // A hero with no ranged attack gets no phantom one.
        let mods = modifiers(&shipped().rules, &ring, 2, &[], false);
        assert_eq!(total(&mods, Stat::Ranged), 0);
    }

    #[test]
    fn a_thrown_axe_counts_for_the_ranged_attack() {
        // An axe with a power that adds to the blow (Giant Strength): its `melee_bonus` and, thrown,
        // its `ranged_bonus` on top of the axe's own bonus.
        let rules = &shipped().rules;
        let (id, power) = rules
            .item_powers
            .iter()
            .find(|(_, p)| p.ranged_kinds.contains(&ItemKind::Axe))
            .expect("a power that adds to a thrown axe");
        let axe = real(|d| d.kind == ItemKind::Axe && d.powers == [id]);
        let thrower = vec![Ability::new(69 /* thrown */)];
        let mods = modifiers(rules, &axe, 0, &thrower, true);
        assert_eq!(
            total(&mods, Stat::Melee),
            axe.bonus.attack + power.melee_bonus
        );
        assert_eq!(
            total(&mods, Stat::Ranged),
            axe.bonus.attack + power.ranged_bonus
        );
        let mods = modifiers(&shipped().rules, &axe, 0, &[], false);
        assert_eq!(total(&mods, Stat::Ranged), 0);
    }

    #[test]
    fn a_power_with_bonus_types_adds_typed_dice() {
        let rules = &shipped().rules;
        let (id, power) = rules
            .item_powers
            .iter()
            .find(|(_, p)| !p.bonus_types.is_empty())
            .expect("a power whose bonus has a type");
        let sword = real(|d| d.kind == ItemKind::Sword && d.powers == [id]);
        let mods = modifiers(rules, &sword, 0, &[Ability::new(3 /* bow */)], true);
        let typed: i32 = mods
            .iter()
            .filter(|m| m.stat == Stat::Melee && !m.types.is_empty())
            .map(|m| m.value.floor())
            .sum();
        assert_eq!(typed, power.melee_bonus);
        assert!(mods
            .iter()
            .filter(|m| !m.types.is_empty())
            .all(|m| m.types == DamageTypes::of(&power.bonus_types)));
        // They are part of the melee total, not on top of it.
        assert!(total(&mods, Stat::Melee) >= typed);
        // An item without the power has no typed dice.
        let plain = real(|d| d.kind == ItemKind::Sword && d.powers.is_empty());
        let mods = modifiers(rules, &plain, 0, &[], false);
        assert!(mods.iter().all(|m| m.types.is_empty()));
    }

    #[test]
    fn a_power_adds_its_bonus_to_the_blow() {
        // `melee_bonus`, `ranged_bonus` and `ranged_kinds` of the item power rows: every power that
        // adds to the blows, on a premade item that has it.
        let rules = &shipped().rules;
        let mut checked = 0;
        for (id, power) in rules.item_powers.iter() {
            if power.melee_bonus == 0 && power.ranged_bonus == 0 {
                continue;
            }
            for (_, def) in rules.items.iter().filter(|(_, d)| d.powers == [id]) {
                let item = Item::from_def(def);
                let mods = modifiers(rules, &item, 0, &[Ability::new(3 /* bow */)], true);
                if item.adds_to_melee() {
                    assert_eq!(
                        total(&mods, Stat::Melee),
                        item.bonus.attack + power.melee_bonus,
                        "{}",
                        def.id
                    );
                }
                let ranged_counts =
                    power.ranged_kinds.is_empty() || power.ranged_kinds.contains(&item.kind);
                if item.kind == ItemKind::Bow && ranged_counts {
                    assert_eq!(
                        total(&mods, Stat::Ranged),
                        item.bonus.attack + power.ranged_bonus,
                        "{}",
                        def.id
                    );
                }
                checked += 1;
            }
        }
        assert!(
            checked > 0,
            "no premade item carries a power that adds to the blows"
        );
    }

    #[test]
    fn defence_resistance_and_moves_count_for_every_item() {
        let mail = real(|d| {
            let b = Item::from_def(d).bonus;
            d.kind == ItemKind::Chainmail
                && d.powers.is_empty()
                && b.defense > 0
                && b.resistance > 0
                && b.moves > HalfInt::from_int(0)
        });
        let mods = modifiers(&shipped().rules, &mail, 1, &[], false);
        assert_eq!(total(&mods, Stat::Defense), mail.bonus.defense);
        assert_eq!(total(&mods, Stat::Resistance), mail.bonus.resistance);
        assert_eq!(total(&mods, Stat::Moves), mail.bonus.moves.floor());
        assert!(mods.iter().all(|m| m.source == Source::Item(1)));
    }

    #[test]
    fn powers_become_lasting_spells_and_attacks() {
        // What each power of the shipped items gives, looked for over all of them.
        let rules = shipped().rules;
        let has = |power: u16| {
            rules
                .items
                .iter()
                .map(|(_, d)| Item::from_def(d))
                .filter(|i| i.powers.contains(&ItemPowerId::new(power)))
                .collect::<Vec<_>>()
        };
        for (power, spell, ability) in [
            (40, Some(121 /* bless */), Some(17 /* dispel_evil */)),
            (54, Some(144 /* righteousness */), None),
            (37, None, Some(35 /* life_steal */)),
            (62, None, Some(43 /* merging */)),
        ] {
            let items = has(power);
            assert!(!items.is_empty(), "no item has power {power}");
            for item in items {
                if let Some(s) = spell {
                    assert!(item.granted_spells(&rules).contains(&SpellId::new(s)));
                }
                if let Some(a) = ability {
                    assert!(item.granted_abilities(&rules).contains(&Ability::new(a)));
                }
            }
        }
    }

    #[test]
    fn the_anvil_gives_half_the_value() {
        for (_, def) in shipped().rules.items.iter() {
            assert_eq!(
                Item::from_def(def).anvil_mana(),
                (def.cost / 2) as i32,
                "{}",
                def.id
            );
        }
    }
}
