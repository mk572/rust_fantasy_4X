//! Making magic items: Enchant Item and Create Artifact.
//!
//! The wizard designs an item ([`ItemDesign`]): a kind, up to four powers and, for a staff or wand,
//! a spell with charges. [`check`] says whether the wizard may have it, [`cost`] what it is worth,
//! and [`build`] makes the [`Item`]. The rules are those of `ItemMake.c` (design in
//! `docs/heroes-items-design.md` section 3.3). `Game::craft_item` (below) starts the casting; the
//! item waits with the cast in progress until the spell has been paid for, then goes to the armory.

use crate::rules::ItemPowerId;
use crate::rules::SpellId;
use f4x_def::half_int::HalfInt;
use serde::{Deserialize, Serialize};

use crate::item::{Item, ItemBonus};
use crate::rules::{ItemKind, ItemPowerDef, PowerBonus, Rules};

/// The power that puts a spell in the item (the row with `holds_spell`); a design names the spell
/// and the number of charges in [`ItemDesign::charges`] instead of listing the power.
pub fn spell_charges(rules: &Rules) -> Option<ItemPowerId> {
    rules
        .item_powers
        .defs()
        .iter()
        .find(|p| p.holds_spell)
        .map(|p| ItemPowerId::new(p.num))
}
/// The effect hook of the two crafting spells: the finished item goes to the armory.
pub const CRAFT_ITEM: &str = "craft_item";

/// The two spells that make items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Craft {
    /// Powers that cost 200 or less (help 207).
    EnchantItem,
    /// Any power.
    CreateArtifact,
}

impl Craft {
    pub fn spell(self) -> SpellId {
        match self {
            Craft::EnchantItem => SpellId::new(207), /* enchant_item */
            Craft::CreateArtifact => SpellId::new(211), /* create_artifact */
        }
    }

    pub fn from_spell(id: SpellId) -> Option<Craft> {
        [Craft::EnchantItem, Craft::CreateArtifact]
            .into_iter()
            .find(|c| c.spell() == id)
    }

    /// The most a single power may cost.
    pub fn max_power_cost(self, rules: &Rules) -> u32 {
        match self {
            Craft::EnchantItem => rules.config.item.enchant_max_power_cost,
            Craft::CreateArtifact => u32::MAX,
        }
    }
}

/// What the wizard asks for (the item screen's choices).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDesign {
    /// The name; empty takes the default one ([`made_name`]).
    pub name: String,
    pub kind: ItemKind,
    /// The picture, one of the kind's range (see [`ItemKind::icon_range`]); one outside it
    /// gives the first of the range.
    #[serde(default)]
    pub icon: u16,
    /// Numbers in `item_powers.ron`, not [`spell_charges`].
    pub powers: Vec<ItemPowerId>,
    /// The spell the hero can cast from the item and how many times.
    pub charges: Option<(SpellId, u32)>,
}

impl ItemDesign {
    /// The picture the item gets: the asked one if it belongs to the kind, else the kind's first.
    pub fn picture(&self) -> u16 {
        let (first, last) = self.kind.icon_range();
        if (first..=last).contains(&self.icon) {
            self.icon
        } else {
            first
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CraftError {
    /// An item needs at least one power.
    NoPowers,
    /// At most four powers, the charges counting as one.
    TooManyPowers,
    UnknownPower,
    /// The same power twice.
    Repeated,
    /// A kind of item that cannot carry the power.
    WrongKind,
    /// Two powers of one group (two attack bonuses, say).
    SameGroup,
    /// Enchant Item takes only powers costing 200 or less.
    TooCostly,
    /// The wizard has too few books in the power's realm.
    NeedsBooks,
    /// The spell for the charges is not known, or cannot be cast in battle.
    BadSpell,
    /// Charges are one to four, and only a staff or a wand holds them.
    BadCharges,
}

impl CraftError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::NoPowers => "refuse_wizard.craft_error_no_powers",
            Self::TooManyPowers => "refuse_wizard.craft_error_too_many_powers",
            Self::UnknownPower => "refuse_wizard.craft_error_unknown_power",
            Self::Repeated => "refuse_wizard.craft_error_repeated",
            Self::WrongKind => "refuse_wizard.craft_error_wrong_kind",
            Self::SameGroup => "refuse_wizard.craft_error_same_group",
            Self::TooCostly => "refuse_wizard.craft_error_too_costly",
            Self::NeedsBooks => "refuse_wizard.craft_error_needs_books",
            Self::BadSpell => "refuse_wizard.craft_error_bad_spell",
            Self::BadCharges => "refuse_wizard.craft_error_bad_charges",
        }
    }
}

fn is_plain(power: &ItemPowerDef) -> bool {
    power.bonus.is_some()
}

/// The value of the item: its base cost, the cost of each power (a plain bonus on a
/// miscellaneous item costs twice) and `spell cost * charges * 20` for an embedded spell. It is
/// what the anvil and the merchants go by; casting it costs this less the wizard's reduction
/// (`Wizard::cost_reduction`, where Artificer takes half off).
pub fn cost(design: &ItemDesign, rules: &Rules) -> u32 {
    let mut total = rules.config.item.base_cost(design.kind);
    for &id in &design.powers {
        let Some(power) = rules.item_powers.try_get(id) else {
            continue;
        };
        total += if design.kind == ItemKind::Misc && is_plain(power) {
            power.cost * 2
        } else {
            power.cost
        };
    }
    if let Some((spell, count)) = &design.charges {
        if let Some(spell) = rules.spells.try_get(*spell) {
            total += spell.cost * count * 20;
        }
    }
    total
}

/// May the wizard have this item made? The rules of the item screen: at most four powers, each
/// one that fits the kind, costs no more than the spell allows and whose realm the wizard has the
/// books for; one power of each group but the special powers.
pub fn check(
    design: &ItemDesign,
    craft: Craft,
    magic: &crate::wizard::entity::Wizard<'_>,
    rules: &Rules,
) -> Result<(), CraftError> {
    let count = design.powers.len() + usize::from(design.charges.is_some());
    if count == 0 {
        return Err(CraftError::NoPowers);
    }
    if count > rules.config.item.max_powers {
        return Err(CraftError::TooManyPowers);
    }
    let mut groups = Vec::new();
    for (i, &id) in design.powers.iter().enumerate() {
        let power = rules
            .item_powers
            .try_get(id)
            .filter(|p| !p.holds_spell)
            .ok_or(CraftError::UnknownPower)?;
        if design.powers[..i].contains(&id) {
            return Err(CraftError::Repeated);
        }
        check_power(power, design.kind, craft, magic, rules)?;
        if power.group != rules.config.item.free_group {
            if groups.contains(&power.group) {
                return Err(CraftError::SameGroup);
            }
            groups.push(power.group);
        }
    }
    if let Some((spell, charges)) = &design.charges {
        let power = rules
            .item_powers
            .defs()
            .iter()
            .find(|p| p.holds_spell)
            .ok_or(CraftError::UnknownPower)?;
        check_power(power, design.kind, craft, magic, rules)?;
        if !(1..=rules.config.item.max_charges).contains(charges) {
            return Err(CraftError::BadCharges);
        }
        let id = *spell;
        let def = rules.spells.try_get(id).ok_or(CraftError::BadSpell)?;
        if !magic.knows(id) || !def.contexts.contains(&crate::rules::CastContext::Battle) {
            return Err(CraftError::BadSpell);
        }
    }
    Ok(())
}

fn check_power(
    power: &ItemPowerDef,
    kind: ItemKind,
    craft: Craft,
    magic: &crate::wizard::entity::Wizard<'_>,
    rules: &Rules,
) -> Result<(), CraftError> {
    if !power.kinds.contains(&kind) {
        return Err(CraftError::WrongKind);
    }
    if power.cost > craft.max_power_cost(rules) {
        return Err(CraftError::TooCostly);
    }
    if let Some(realm) = power.realm {
        if magic.books_in(realm) < power.books as i32 {
            return Err(CraftError::NeedsBooks);
        }
    }
    Ok(())
}

/// The powers an item of `kind` may be given by this spell and wizard, in the table's order
/// (the power list of the item screen). [`spell_charges`] is among them for a staff or wand;
/// choosing it needs a spell and a count as well.
pub fn available_powers<'a>(
    kind: ItemKind,
    craft: Craft,
    magic: &crate::wizard::entity::Wizard<'_>,
    rules: &'a Rules,
) -> Vec<&'a ItemPowerDef> {
    rules
        .item_powers
        .defs()
        .iter()
        .filter(|p| check_power(p, kind, craft, magic, rules).is_ok())
        .collect()
}

/// The item the design makes (`Create_Item_Record`): bonuses from the plain powers (a movement
/// power in whole moves), the special powers as they are, chain mail one
/// more defence and plate mail two. The value is the full [`cost`].
pub fn build(design: &ItemDesign, rules: &Rules) -> Item {
    let mut bonus = ItemBonus::default();
    let mut powers = Vec::new();
    for &id in &design.powers {
        let Some(power) = rules.item_powers.try_get(id) else {
            continue;
        };
        match power.bonus {
            Some((PowerBonus::Attack, v)) => bonus.attack = v,
            Some((PowerBonus::ToHit, v)) => bonus.to_hit = v,
            Some((PowerBonus::Defense, v)) => bonus.defense = v,
            Some((PowerBonus::Moves, v)) => bonus.moves = HalfInt::from_int(v),
            Some((PowerBonus::Resistance, v)) => bonus.resistance = v,
            Some((PowerBonus::SpellSkill, v)) => bonus.spell_skill = v,
            Some((PowerBonus::SpellSave, v)) => bonus.spell_save = v,
            None => powers.push(id),
        }
    }
    bonus.defense += rules.config.item.kind_defense(design.kind);
    let (name, made) = if design.name.trim().is_empty() {
        (String::new(), Some(made_name(design, rules)))
    } else {
        (design.name.clone(), None)
    };
    Item {
        name,
        kind: design.kind,
        cost: cost(design, rules),
        icon: design.picture(),
        bonus,
        powers,
        charges: design.charges,
        prefab: None,
        made,
    }
}

/// The kind of thing an item is called (`Get_Item_Type_Name`): the base kinds, and for a
/// miscellaneous item the one its picture shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeName {
    Sword,
    Mace,
    Axe,
    Bow,
    Staff,
    Wand,
    Shield,
    ChainMail,
    PlateMail,
    Amulet,
    Ring,
    Cloak,
    Gauntlet,
    Helm,
    Orb,
}

impl TypeName {
    /// Its word in `content/text/*/item.ron`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Sword => "item.type_sword",
            Self::Mace => "item.type_mace",
            Self::Axe => "item.type_axe",
            Self::Bow => "item.type_bow",
            Self::Staff => "item.type_staff",
            Self::Wand => "item.type_wand",
            Self::Shield => "item.type_shield",
            Self::ChainMail => "item.type_chain_mail",
            Self::PlateMail => "item.type_plate_mail",
            Self::Amulet => "item.type_amulet",
            Self::Ring => "item.type_ring",
            Self::Cloak => "item.type_cloak",
            Self::Gauntlet => "item.type_gauntlet",
            Self::Helm => "item.type_helm",
            Self::Orb => "item.type_orb",
        }
    }

    /// The name of a design's kind and picture.
    pub fn of(kind: ItemKind, icon: u16) -> TypeName {
        match kind {
            ItemKind::Sword => Self::Sword,
            ItemKind::Mace => Self::Mace,
            ItemKind::Axe => Self::Axe,
            ItemKind::Bow => Self::Bow,
            ItemKind::Staff => Self::Staff,
            ItemKind::Wand => Self::Wand,
            // A miscellaneous item is told apart by its picture.
            ItemKind::Misc => match icon {
                ..=77 => Self::Amulet,
                78..=83 => Self::Ring,
                84..=89 => Self::Cloak,
                90..=93 => Self::Gauntlet,
                94..=100 => Self::Helm,
                _ => Self::Orb,
            },
            ItemKind::Shield => Self::Shield,
            ItemKind::Chainmail => Self::ChainMail,
            ItemKind::Platemail => Self::PlateMail,
        }
    }
}

/// What follows "of" in a default name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Suffix {
    Strength,
    Accuracy,
    Defense,
    Wizardry,
    Power,
    Speed,
    Protection,
    /// A special power, by its number in `item_powers.ron` (the name is the content's).
    Special(ItemPowerId),
    /// The embedded spell, by its number, and the charges.
    Spell(SpellId, u32),
}

impl Suffix {
    pub fn key(&self) -> Option<&'static str> {
        Some(match self {
            Self::Strength => "item.suffix_strength",
            Self::Accuracy => "item.suffix_accuracy",
            Self::Defense => "item.suffix_defense",
            Self::Wizardry => "item.suffix_wizardry",
            Self::Power => "item.suffix_power",
            Self::Speed => "item.suffix_speed",
            Self::Protection => "item.suffix_protection",
            Self::Special(_) | Self::Spell(..) => return None,
        })
    }
}

/// The parts of what the original calls an item with no name given (`Get_Item_Name`): "+3
/// Sword of Wizardry", from the kind, the attack bonus of a weapon and the power highest in
/// the table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MadeName {
    pub kind: TypeName,
    /// The attack bonus of a weapon, "+3".
    pub bonus: Option<i32>,
    pub suffix: Option<Suffix>,
}

/// The parts of the default name of a design.
pub fn made_name(design: &ItemDesign, rules: &Rules) -> MadeName {
    let kind = TypeName::of(design.kind, design.picture());
    let weapon = matches!(
        design.kind,
        ItemKind::Sword
            | ItemKind::Mace
            | ItemKind::Axe
            | ItemKind::Bow
            | ItemKind::Staff
            | ItemKind::Wand
    );
    let mut left: Vec<(usize, &ItemPowerDef)> = design
        .powers
        .iter()
        .filter_map(|&at| Some((at.num() as usize, rules.item_powers.try_get(at)?)))
        .collect();
    let mut bonus = None;
    if weapon {
        if let Some(i) = left
            .iter()
            .position(|(_, p)| matches!(p.bonus, Some((PowerBonus::Attack, _))))
        {
            if let Some((PowerBonus::Attack, v)) = left[i].1.bonus {
                bonus = Some(v);
            }
            left.remove(i);
        }
    }
    let highest = left.iter().max_by_key(|(at, _)| *at).map(|(_, p)| *p);
    let suffix = if let Some(power) = highest {
        Some(match power.bonus {
            Some((PowerBonus::Attack, _)) => Suffix::Strength,
            Some((PowerBonus::ToHit, _)) => Suffix::Accuracy,
            Some((PowerBonus::Defense, _)) => Suffix::Defense,
            Some((PowerBonus::SpellSkill, _)) => Suffix::Wizardry,
            Some((PowerBonus::SpellSave, _)) => Suffix::Power,
            Some((PowerBonus::Moves, _)) => Suffix::Speed,
            Some((PowerBonus::Resistance, _)) => Suffix::Protection,
            None => Suffix::Special(ItemPowerId::new(power.num)),
        })
    } else {
        design
            .charges
            .as_ref()
            .map(|(spell, count)| Suffix::Spell(*spell, *count))
    };
    MadeName {
        kind,
        bonus,
        suffix,
    }
}

/// Why an item could not be started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CraftingError {
    UnknownPlayer,
    /// The wizard does not know the spell, or is casting another.
    Magic(crate::wizard::magic::MagicError),
    Design(CraftError),
}

impl CraftingError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::UnknownPlayer => "refuse_wizard.crafting_error_unknown_player",
            Self::Magic(e) => e.key(),
            Self::Design(e) => e.key(),
        }
    }
}

impl crate::game::Game {
    /// Start casting Enchant Item or Create Artifact for the item `design` describes. The cost
    /// is the item's value less the wizard's reduction for the spell; if skill and mana pay for
    /// it now the item is made now (the vault, else the award list), otherwise it waits in
    /// the cast in progress while the cost is paid over turns. Returns the events.
    pub fn craft_item(
        &mut self,
        player: crate::player::PlayerId,
        craft: Craft,
        design: &ItemDesign,
    ) -> Result<Vec<crate::game::Event>, CraftingError> {
        let spell = craft.spell();
        if self.content.rules.spells.try_get(spell).is_none() {
            return Err(CraftingError::Magic(
                crate::wizard::magic::MagicError::NotAvailable,
            ));
        }
        let reserve = self
            .player(player)
            .ok_or(CraftingError::UnknownPlayer)?
            .treasury()
            .mana;
        let (item, price, now) = {
            let magic = self.wizard(player).ok_or(CraftingError::UnknownPlayer)?;
            magic.check_can_cast(spell).map_err(CraftingError::Magic)?;
            check(design, craft, &magic, &self.content.rules).map_err(CraftingError::Design)?;
            let item = build(design, &self.content.rules);
            let price = self.crafting_price(player, spell, item.cost);
            let now = price <= magic.instant_budget(reserve.floor());
            (item, price, now)
        };
        if now {
            if let Some(mut w) = self.wizard_mut(player) {
                w.spend_skill(price);
            }
        } else {
            self.begin_casting(player, spell, price);
            self.spawn_item(item.clone(), crate::item::entity::HeldBy::Crafting(player));
        }
        if now {
            if let Some(t) = self.treasury_mut(player) {
                t.mana -= price;
            }
        }
        self.begin_cast_relations(player, spell);
        if now {
            // Another wizard's Suppress Magic resists the spell at the mana it cost, not at the
            // spell's own small cost (patch 1.50, note 270); the mana is spent.
            if let Some((by, counter)) =
                self.suppressed_at_cost(player, spell, Some(price.max(0) as u32))
            {
                return Ok(vec![crate::game::Event::SpellSuppressed {
                    player,
                    spell,
                    by,
                    counter,
                }]);
            }
            Ok(self.finish_crafted(player, spell, item))
        } else {
            Ok(Vec::new())
        }
    }

    /// The mana an item of `value` gold costs `player` to make with `spell`: the value less the
    /// wizard's reduction for the spell.
    pub(crate) fn crafting_price(
        &self,
        player: crate::player::PlayerId,
        spell: crate::rules::SpellId,
        value: u32,
    ) -> i32 {
        let value = value as i32;
        let def = self.content.rules.spells.get(spell);
        match self.wizard(player) {
            Some(magic) => value - value * magic.cost_reduction(&self.content.rules, def) / 100,
            None => value,
        }
    }

    /// The spell has been paid for and the item made: it goes to the armory.
    fn finish_crafted(
        &mut self,
        player: crate::player::PlayerId,
        spell: crate::rules::SpellId,
        item: Item,
    ) -> Vec<crate::game::Event> {
        let shown = item.clone();
        let placed = self.give_item(player, item);
        vec![
            crate::game::Event::SpellCast { player, spell },
            crate::game::Event::ItemMade {
                player,
                item: shown,
                placed,
            },
        ]
    }

    /// The effect of the crafting spells when they have been paid for over turns: the waiting
    /// item goes to the armory. Does nothing if none is waiting.
    pub(crate) fn complete_crafting(
        &mut self,
        player: crate::player::PlayerId,
    ) -> Vec<crate::game::Event> {
        let Some((entity, item)) = crate::item::entity::crafting_of(&self.world, player) else {
            return Vec::new();
        };
        let shown = item.clone();
        let placed = self.place_item(player, entity);
        vec![crate::game::Event::ItemMade {
            player,
            item: shown,
            placed,
        }]
    }
}
