//! Magic items (task 49): what an item is, where it fits and what it does for the hero that
//! carries it.
//!
//! An [`Item`] is an instance, not an id of the content: a crafted item (task 50) is not in any
//! file, so the vault, a hero's slots and the loot all hold the same type. A prefab item is a copy
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
pub mod make;

use serde::{Deserialize, Serialize};

use self::armory::Armory;
use crate::battle::game::ranged_kind_of;
use crate::battle::RangedKind;
use crate::content::Content;
use crate::player::PlayerId;

use crate::game::Game;
use crate::rules::effects::Effect;
use crate::rules::stats::{Modifier, Source};
use crate::rules::{ItemDef, ItemKind, ItemPowerDef, ModOp, Rules, SlotKind, SpellId, Stat};
use crate::unit::UnitView;
use hecs::World;

/// The plain bonuses of an item (the original's record: attack, to hit, defence, movement in
/// half-moves, resistance, spell skill and spell save).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemBonus {
    pub attack: i32,
    pub to_hit: i32,
    pub defense: i32,
    /// In half-moves.
    pub moves: i32,
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
    /// The picture, an entry of the original's `ITEMS.LBX`.
    #[serde(default)]
    pub icon: u16,
    pub bonus: ItemBonus,
    /// The special powers (ids in `item_powers.ron`).
    pub powers: Vec<String>,
    /// The spell the hero can cast from the item, and how many times.
    pub charges: Option<(String, u32)>,
    /// The id of the prefab item this is a copy of; `None` for one made by a wizard or at random.
    pub prefab: Option<String>,
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
            } = rule.effect
            {
                match stat {
                    Stat::Melee => bonus.attack += value,
                    Stat::ToHit => bonus.to_hit += value,
                    Stat::Defense => bonus.defense += value,
                    Stat::HalfMoves => bonus.moves += value,
                    Stat::Resistance => bonus.resistance += value,
                    _ => {}
                }
            }
        }
        Item {
            name: def.name.clone(),
            kind: def.kind,
            cost: def.cost,
            icon: def.icon,
            bonus,
            powers: def.powers.clone(),
            charges: def.charges.clone(),
            prefab: Some(def.id.clone()),
        }
    }

    /// The slot type of the item, fixed by its kind (`Create_Item_Record`).
    pub fn slot_kind(&self) -> SlotKind {
        slot_of(self.kind)
    }

    pub fn has_power(&self, power: &str) -> bool {
        self.powers.iter().any(|p| p == power)
    }

    /// What the anvil gives for it: half its value in mana (help 246).
    pub fn anvil_mana(&self) -> i32 {
        (self.cost / 2) as i32
    }

    /// The lasting spells its powers put on the hero that carries it (the original turns them
    /// into unit enchantments, `Item_Powers_To_Unit_Enchantments`; they are not the wizard's, so
    /// dispelling does not touch them). The ids are those of `spells.ron`.
    pub fn granted_spells<'a>(&self, rules: &'a Rules) -> Vec<&'a str> {
        self.powers
            .iter()
            .filter_map(|p| rules.item_powers.by_name(p)?.spell.as_deref())
            .collect()
    }

    /// The abilities the item gives its carrier: a shield gives the large shield, and the attack
    /// powers give the matching special attack (`Item_Powers_To_Attack_Attributes`). The original
    /// puts an attack power on the melee or the ranged attack by the kind of the item; here the
    /// carrier has it for both.
    pub fn granted_abilities<'a>(&self, rules: &'a Rules) -> Vec<&'a str> {
        let mut out: Vec<&'a str> = Vec::new();
        if self.kind == ItemKind::Shield {
            out.push("large_shield");
        }
        for p in &self.powers {
            if let Some(ability) = rules
                .item_powers
                .by_name(p)
                .and_then(|d| d.attack.as_deref())
            {
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

/// The items a hero carries, one per slot (the slot kinds are the hero's `HeroKind::slots`).
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
    pub fn has_power(&self, power: &str) -> bool {
        self.iter().any(|i| i.has_power(power))
    }
}

/// The special powers that change the carrier's attacks in [`modifiers`] rather than becoming a
/// lasting spell or an ability, or that do nothing to a unit.
pub const OTHER_POWERS: &[&str] = &["flaming", "giant_strength", "merging", "spell_charges"];

/// Whether the rules do something with a special power: it puts a lasting spell on the carrier,
/// gives an attack ability, or is one of [`OTHER_POWERS`].
pub fn power_is_handled(power: &ItemPowerDef) -> bool {
    power.spell.is_some() || power.attack.is_some() || OTHER_POWERS.contains(&power.id.as_str())
}

/// The stat modifiers of the item in slot `slot` of a hero whose abilities are `abilities` and
/// who has a ranged attack if `has_ranged`. Defence, resistance and movement count for all
/// items; attack and to hit go to the melee attack or the ranged one by the kind of the item
/// (`Battle_Unit_Item_Stats`). Flaming adds 3 to both attacks and Giant Strength 1 to the melee
/// attack (and to a thrown axe's). The item's to hit is kept apart from the unit's
/// ([`Stat::MeleeToHit`] and [`Stat::RangedToHit`]) because it counts for one attack only.
pub fn modifiers(item: &Item, slot: u8, abilities: &[String], has_ranged: bool) -> Vec<Modifier> {
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
    add(Stat::HalfMoves, b.moves);
    let flaming = if item.has_power("flaming") { 3 } else { 0 };
    let giant = if item.has_power("giant_strength") {
        1
    } else {
        0
    };
    if item.adds_to_melee() {
        add(Stat::Melee, b.attack + flaming + giant);
        add(Stat::MeleeToHit, b.to_hit);
    }
    let kind = ranged_kind_of(abilities);
    let thrown = abilities.iter().any(|a| a == "thrown");
    if has_ranged && item.adds_to_ranged(kind, thrown) {
        let giant = if item.kind == ItemKind::Axe { giant } else { 0 };
        add(Stat::Ranged, b.attack + flaming + giant);
        add(Stat::RangedToHit, b.to_hit);
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

    /// The items a unit carries with the slot each is in (only heroes carry any).
    pub fn carried(&self, unit: hecs::Entity) -> Vec<(u8, Item)> {
        let Ok(items) = self.world.get::<&HeroItems>(unit) else {
            return Vec::new();
        };
        items
            .0
            .iter()
            .enumerate()
            .filter_map(|(i, item)| Some((i as u8, item.clone()?)))
            .collect()
    }

    /// The lasting spells the unit's items put on it (see [`Item::granted_spells`]).
    pub fn spells(&self, unit: hecs::Entity) -> Vec<SpellId> {
        let mut out = Vec::new();
        for (_, item) in self.carried(unit) {
            for name in item.granted_spells(&self.content.rules) {
                if let Some(id) = self.content.rules.spells.id_of(name) {
                    if !out.contains(&id) {
                        out.push(id);
                    }
                }
            }
        }
        out
    }

    /// The special attack and shield abilities the unit's items give (see
    /// [`Item::granted_abilities`]), each once.
    pub fn abilities(&self, unit: hecs::Entity) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (_, item) in self.carried(unit) {
            for a in item.granted_abilities(&self.content.rules) {
                if !out.iter().any(|x| x == a) {
                    out.push(a.to_string());
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
            .is_some_and(|d| d.stats.get(&Stat::Ranged).copied().unwrap_or(0) > 0);
        items
            .iter()
            .flat_map(|(slot, item)| modifiers(item, *slot, &abilities, has_ranged))
            .collect()
    }

    /// A wizard's vault and award list.
    pub fn armory(&self, player: PlayerId) -> Option<hecs::Ref<'a, Armory>> {
        crate::player::find(self.world, player).map(|p| hecs::Ref::map(p, |p| &p.armory))
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

    fn item(kind: ItemKind) -> Item {
        Item {
            name: "Test".into(),
            kind,
            cost: 100,
            icon: 0,
            bonus: ItemBonus::default(),
            powers: Vec::new(),
            charges: None,
            prefab: None,
        }
    }

    fn abilities(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn total(mods: &[Modifier], stat: Stat) -> i32 {
        mods.iter()
            .filter(|m| m.stat == stat)
            .map(|m| m.value)
            .sum()
    }

    #[test]
    fn the_slot_follows_the_kind() {
        assert_eq!(item(ItemKind::Mace).slot_kind(), SlotKind::Sword);
        assert_eq!(item(ItemKind::Wand).slot_kind(), SlotKind::Staff);
        assert_eq!(item(ItemKind::Bow).slot_kind(), SlotKind::Bow);
        assert_eq!(item(ItemKind::Platemail).slot_kind(), SlotKind::Armor);
        assert_eq!(item(ItemKind::Misc).slot_kind(), SlotKind::Misc);
    }

    #[test]
    fn a_sword_adds_attack_to_melee_only() {
        let mut sword = item(ItemKind::Sword);
        sword.bonus.attack = 2;
        sword.bonus.to_hit = 1;
        let mods = modifiers(&sword, 0, &abilities(&["bow"]), true);
        assert_eq!(total(&mods, Stat::Melee), 2);
        assert_eq!(total(&mods, Stat::MeleeToHit), 1);
        assert_eq!(total(&mods, Stat::Ranged), 0);
        assert_eq!(total(&mods, Stat::RangedToHit), 0);
    }

    #[test]
    fn a_bow_needs_arrows_and_a_staff_needs_magic() {
        let mut bow = item(ItemKind::Bow);
        bow.bonus.attack = 3;
        let mut staff = item(ItemKind::Staff);
        staff.bonus.attack = 2;
        let archer = abilities(&["bow"]);
        let mage = abilities(&["fireball_attack"]);
        assert_eq!(total(&modifiers(&bow, 0, &archer, true), Stat::Ranged), 3);
        assert_eq!(total(&modifiers(&bow, 0, &mage, true), Stat::Ranged), 0);
        assert_eq!(total(&modifiers(&staff, 0, &mage, true), Stat::Ranged), 2);
        assert_eq!(total(&modifiers(&staff, 0, &archer, true), Stat::Ranged), 0);
        // Neither of them touches the melee attack.
        assert_eq!(total(&modifiers(&bow, 0, &archer, true), Stat::Melee), 0);
    }

    #[test]
    fn a_misc_item_counts_for_both_attacks() {
        let mut ring = item(ItemKind::Misc);
        ring.bonus.attack = 1;
        let mods = modifiers(&ring, 2, &abilities(&["bow"]), true);
        assert_eq!(total(&mods, Stat::Melee), 1);
        assert_eq!(total(&mods, Stat::Ranged), 1);
        // A hero with no ranged attack gets no phantom one.
        let mods = modifiers(&ring, 2, &[], false);
        assert_eq!(total(&mods, Stat::Ranged), 0);
    }

    #[test]
    fn a_thrown_axe_counts_for_the_ranged_attack() {
        let mut axe = item(ItemKind::Axe);
        axe.bonus.attack = 1;
        axe.powers = vec!["giant_strength".into()];
        let thrower = abilities(&["thrown"]);
        let mods = modifiers(&axe, 0, &thrower, true);
        assert_eq!(total(&mods, Stat::Melee), 2);
        assert_eq!(total(&mods, Stat::Ranged), 2);
        let mods = modifiers(&axe, 0, &[], false);
        assert_eq!(total(&mods, Stat::Ranged), 0);
    }

    #[test]
    fn flaming_and_giant_strength_add_to_the_blow() {
        let mut blade = item(ItemKind::Sword);
        blade.powers = vec!["flaming".into(), "giant_strength".into()];
        let mods = modifiers(&blade, 0, &[], false);
        assert_eq!(total(&mods, Stat::Melee), 4);
        // A bow's flaming arrows get the 3 but no Giant Strength.
        let mut bow = item(ItemKind::Bow);
        bow.powers = vec!["flaming".into(), "giant_strength".into()];
        let mods = modifiers(&bow, 0, &abilities(&["bow"]), true);
        assert_eq!(total(&mods, Stat::Ranged), 3);
        assert_eq!(total(&mods, Stat::Melee), 0);
    }

    #[test]
    fn defence_resistance_and_moves_count_for_every_item() {
        let mut mail = item(ItemKind::Chainmail);
        mail.bonus.defense = 3;
        mail.bonus.resistance = 2;
        mail.bonus.moves = 2;
        let mods = modifiers(&mail, 1, &[], false);
        assert_eq!(total(&mods, Stat::Defense), 3);
        assert_eq!(total(&mods, Stat::Resistance), 2);
        assert_eq!(total(&mods, Stat::HalfMoves), 2);
        assert!(mods.iter().all(|m| m.source == Source::Item(1)));
    }

    #[test]
    fn powers_become_lasting_spells_and_attacks() {
        let mut thing = item(ItemKind::Shield);
        thing.powers = vec![
            "holy_avenger".into(),
            "righteousness".into(),
            "vampiric".into(),
            "merging".into(),
        ];
        let mut rules = Rules::default();
        let power = crate::game::testutil::item_power;
        rules.item_powers = crate::rules::Registry::new(vec![
            power("holy_avenger", Some("bless"), Some("dispel_evil")),
            power("righteousness", Some("righteousness"), None),
            power("vampiric", None, Some("life_steal")),
            power("merging", None, None),
        ]);
        assert_eq!(thing.granted_spells(&rules), ["bless", "righteousness"]);
        assert_eq!(
            thing.granted_abilities(&rules),
            ["large_shield", "dispel_evil", "life_steal"]
        );
    }

    #[test]
    fn the_anvil_gives_half_the_value() {
        let mut thing = item(ItemKind::Staff);
        thing.cost = 1001;
        assert_eq!(thing.anvil_mana(), 500);
    }
}
