//! Making magic items (task 50): Enchant Item and Create Artifact.
//!
//! The wizard designs an item ([`ItemDesign`]): a kind, up to four powers and, for a staff or wand,
//! a spell with charges. [`check`] says whether the wizard may have it, [`cost`] what it is worth,
//! and [`build`] makes the [`Item`]. The rules are those of `ItemMake.c` (design in
//! `docs/heroes-items-design.md` section 3.3). `Game::craft_item` (below) starts the casting; the
//! item waits with the cast in progress until the spell has been paid for, then goes to the armory.

use serde::{Deserialize, Serialize};

use crate::item::{Item, ItemBonus};
use crate::rules::{ItemKind, ItemPowerDef, PowerBonus, Rules};

/// Most powers on one item.
pub const MAX_POWERS: usize = 4;
/// Most charges of an embedded spell.
pub const MAX_CHARGES: u32 = 4;
/// The id of the power that puts a spell in the item; a design names the spell and the number of
/// charges in [`ItemDesign::charges`] instead of listing the power.
pub const SPELL_CHARGES: &str = "spell_charges";
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
    pub fn spell(self) -> &'static str {
        match self {
            Craft::EnchantItem => "enchant_item",
            Craft::CreateArtifact => "create_artifact",
        }
    }

    pub fn from_spell(id: &str) -> Option<Craft> {
        [Craft::EnchantItem, Craft::CreateArtifact]
            .into_iter()
            .find(|c| c.spell() == id)
    }

    /// The most a single power may cost.
    pub fn max_power_cost(self) -> u32 {
        match self {
            Craft::EnchantItem => 200,
            Craft::CreateArtifact => u32::MAX,
        }
    }
}

/// What the wizard asks for (the item screen's choices).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDesign {
    /// The name; empty takes [`default_name`].
    pub name: String,
    pub kind: ItemKind,
    /// The picture, one of the kind's range (see [`ItemKind::icon_range`]); one outside it
    /// gives the first of the range.
    #[serde(default)]
    pub icon: u16,
    /// Ids in `item_powers.ron`, not [`SPELL_CHARGES`].
    pub powers: Vec<String>,
    /// The spell the hero can cast from the item and how many times.
    pub charges: Option<(String, u32)>,
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

/// The group of powers that are free to combine (the special powers): the others allow one
/// power each.
const FREE_GROUP: u8 = 9;

/// What the plain item of a kind costs before its powers (`m_item_type_base_cost`).
pub fn base_cost(kind: ItemKind) -> u32 {
    match kind {
        ItemKind::Sword | ItemKind::Mace | ItemKind::Axe | ItemKind::Bow => 100,
        ItemKind::Staff | ItemKind::Platemail => 300,
        ItemKind::Wand => 200,
        ItemKind::Misc => 50,
        ItemKind::Shield | ItemKind::Chainmail => 100,
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
    let mut total = base_cost(design.kind);
    for id in &design.powers {
        let Some(power) = rules.item_powers.by_name(id) else {
            continue;
        };
        total += if design.kind == ItemKind::Misc && is_plain(power) {
            power.cost * 2
        } else {
            power.cost
        };
    }
    if let Some((spell, count)) = &design.charges {
        if let Some(spell) = rules.spells.by_name(spell) {
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
    if count > MAX_POWERS {
        return Err(CraftError::TooManyPowers);
    }
    let mut groups = Vec::new();
    for (i, id) in design.powers.iter().enumerate() {
        let power = rules
            .item_powers
            .by_name(id)
            .filter(|_| id != SPELL_CHARGES)
            .ok_or(CraftError::UnknownPower)?;
        if design.powers[..i].contains(id) {
            return Err(CraftError::Repeated);
        }
        check_power(power, design.kind, craft, magic)?;
        if power.group != FREE_GROUP {
            if groups.contains(&power.group) {
                return Err(CraftError::SameGroup);
            }
            groups.push(power.group);
        }
    }
    if let Some((spell, charges)) = &design.charges {
        let power = rules
            .item_powers
            .by_name(SPELL_CHARGES)
            .ok_or(CraftError::UnknownPower)?;
        check_power(power, design.kind, craft, magic)?;
        if !(1..=MAX_CHARGES).contains(charges) {
            return Err(CraftError::BadCharges);
        }
        let id = rules.spells.id_of(spell).ok_or(CraftError::BadSpell)?;
        let def = rules.spells.get(id);
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
) -> Result<(), CraftError> {
    if !power.kinds.contains(&kind) {
        return Err(CraftError::WrongKind);
    }
    if power.cost > craft.max_power_cost() {
        return Err(CraftError::TooCostly);
    }
    if let Some(realm) = &power.realm {
        if magic.books_in(realm) < power.books as i32 {
            return Err(CraftError::NeedsBooks);
        }
    }
    Ok(())
}

/// The powers an item of `kind` may be given by this spell and wizard, in the table's order
/// (the power list of the item screen). [`SPELL_CHARGES`] is among them for a staff or wand;
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
        .filter(|p| check_power(p, kind, craft, magic).is_ok())
        .collect()
}

/// The item the design makes (`Create_Item_Record`): bonuses from the plain powers (a movement
/// power in whole moves, kept as half-moves), the special powers as they are, chain mail one
/// more defence and plate mail two. The value is the full [`cost`].
pub fn build(design: &ItemDesign, rules: &Rules) -> Item {
    let mut bonus = ItemBonus::default();
    let mut powers = Vec::new();
    for id in &design.powers {
        let Some(power) = rules.item_powers.by_name(id) else {
            continue;
        };
        match power.bonus {
            Some((PowerBonus::Attack, v)) => bonus.attack = v,
            Some((PowerBonus::ToHit, v)) => bonus.to_hit = v,
            Some((PowerBonus::Defense, v)) => bonus.defense = v,
            Some((PowerBonus::Moves, v)) => bonus.moves = v * 2,
            Some((PowerBonus::Resistance, v)) => bonus.resistance = v,
            Some((PowerBonus::SpellSkill, v)) => bonus.spell_skill = v,
            Some((PowerBonus::SpellSave, v)) => bonus.spell_save = v,
            None => powers.push(id.clone()),
        }
    }
    match design.kind {
        ItemKind::Chainmail => bonus.defense += 1,
        ItemKind::Platemail => bonus.defense += 2,
        _ => {}
    }
    let name = if design.name.trim().is_empty() {
        default_name(design, rules)
    } else {
        design.name.clone()
    };
    Item {
        name,
        kind: design.kind,
        cost: cost(design, rules),
        icon: design.picture(),
        bonus,
        powers,
        charges: design.charges.clone(),
        prefab: None,
    }
}

/// What the original calls an item with no name given (`Get_Item_Name`): "+3 Sword of Wizardry",
/// from the kind, the attack bonus of a weapon and the power highest in the table.
pub fn default_name(design: &ItemDesign, rules: &Rules) -> String {
    let kind_name = match design.kind {
        ItemKind::Sword => "Sword",
        ItemKind::Mace => "Mace",
        ItemKind::Axe => "Axe",
        ItemKind::Bow => "Bow",
        ItemKind::Staff => "Staff",
        ItemKind::Wand => "Wand",
        // A miscellaneous item is told apart by its picture (`Get_Item_Type_Name`).
        ItemKind::Misc => match design.picture() {
            ..=77 => "Amulet",
            78..=83 => "Ring",
            84..=89 => "Cloak",
            90..=93 => "Gauntlet",
            94..=100 => "Helm",
            _ => "Orb",
        },
        ItemKind::Shield => "Shield",
        ItemKind::Chainmail => "Chain Mail",
        ItemKind::Platemail => "Plate Mail",
    };
    let weapon = matches!(
        design.kind,
        ItemKind::Sword
            | ItemKind::Mace
            | ItemKind::Axe
            | ItemKind::Bow
            | ItemKind::Staff
            | ItemKind::Wand
    );
    let mut name = kind_name.to_string();
    let mut left: Vec<(usize, &ItemPowerDef)> = design
        .powers
        .iter()
        .filter_map(|id| {
            let at = rules.item_powers.id_of(id)?.index();
            Some((at, rules.item_powers.get(rules.item_powers.id_of(id)?)))
        })
        .collect();
    if weapon {
        if let Some(i) = left
            .iter()
            .position(|(_, p)| matches!(p.bonus, Some((PowerBonus::Attack, _))))
        {
            if let Some((PowerBonus::Attack, v)) = left[i].1.bonus {
                name = format!("+{v} {kind_name}");
            }
            left.remove(i);
        }
    }
    let highest = left.iter().max_by_key(|(at, _)| *at).map(|(_, p)| *p);
    if let Some(power) = highest {
        let suffix = match power.bonus {
            Some((PowerBonus::Attack, _)) => "Strength",
            Some((PowerBonus::ToHit, _)) => "Accuracy",
            Some((PowerBonus::Defense, _)) => "Defense",
            Some((PowerBonus::SpellSkill, _)) => "Wizardry",
            Some((PowerBonus::SpellSave, _)) => "Power",
            Some((PowerBonus::Moves, _)) => "Speed",
            Some((PowerBonus::Resistance, _)) => "Protection",
            None => power.name.as_str(),
        };
        name = format!("{name} of {suffix}");
    } else if let Some((spell, count)) = &design.charges {
        let spell = rules
            .spells
            .by_name(spell)
            .map_or(spell.as_str(), |s| s.name.as_str());
        name = format!("{name} of {spell} x{count}");
    }
    name.chars().take(30).collect()
}

/// Why an item could not be started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CraftingError {
    UnknownPlayer,
    /// The wizard does not know the spell, or is casting another.
    Magic(crate::wizard::magic::MagicError),
    Design(CraftError),
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
        let spell = self
            .content
            .rules
            .spells
            .id_of(craft.spell())
            .ok_or(CraftingError::Magic(
                crate::wizard::magic::MagicError::NotAvailable,
            ))?;
        let reserve = self
            .treasury(player)
            .ok_or(CraftingError::UnknownPlayer)?
            .mana;
        let (item, price, now) = {
            let magic = self.wizard(player).ok_or(CraftingError::UnknownPlayer)?;
            magic.check_can_cast(spell).map_err(CraftingError::Magic)?;
            check(design, craft, &magic, &self.content.rules).map_err(CraftingError::Design)?;
            let item = build(design, &self.content.rules);
            let value = item.cost as i32;
            let def = self.content.rules.spells.get(spell);
            let price = value - value * magic.cost_reduction(def) / 100;
            let now = price <= magic.instant_budget(reserve);
            (item, price, now)
        };
        if now {
            if let Some(mut w) = self.wizard_mut(player) {
                w.spend_skill(price);
            }
        } else {
            self.begin_casting(player, spell, price);
            if let Some(mut w) = self.wizard_mut(player) {
                w.set_crafting(item.clone());
            }
        }
        if now {
            if let Some(t) = self.treasury_mut(player) {
                t.mana -= price;
            }
        }
        self.begin_cast_relations(player, spell);
        if now {
            Ok(self.finish_crafted(player, spell, item))
        } else {
            Ok(Vec::new())
        }
    }

    /// The spell has been paid for and the item made: it goes to the armory.
    fn finish_crafted(
        &mut self,
        player: crate::player::PlayerId,
        spell: crate::rules::SpellId,
        item: Item,
    ) -> Vec<crate::game::Event> {
        let name = item.name.clone();
        let placed = self.give_item(player, item);
        vec![
            crate::game::Event::SpellCast { player, spell },
            crate::game::Event::ItemMade {
                player,
                name,
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
        let Some(item) = self.wizard_mut(player).and_then(|mut w| w.take_crafting()) else {
            return Vec::new();
        };
        let name = item.name.clone();
        let placed = self.give_item(player, item);
        vec![crate::game::Event::ItemMade {
            player,
            name,
            placed,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::content_with_magic;
    use crate::game::{Command, Event, Game};
    use crate::item::armory::Placed;
    use crate::map::{Grid, Map};
    use crate::player::PlayerId;
    use crate::rules::{ItemPowerDef, PowerBonus, Registry};

    const P0: PlayerId = PlayerId(0);

    fn power(
        id: &str,
        group: u8,
        cost: u32,
        kinds: &[ItemKind],
        bonus: Option<(PowerBonus, i32)>,
        realm: Option<(&str, u32)>,
    ) -> ItemPowerDef {
        ItemPowerDef {
            id: id.into(),
            name: id.into(),
            group,
            cost,
            kinds: kinds.to_vec(),
            realm: realm.map(|(r, _)| r.into()),
            books: realm.map_or(0, |(_, b)| b),
            bonus,
            spell: None,
            attack: None,
        }
    }

    fn rules_with_items() -> crate::content::Content {
        use ItemKind::*;
        let mut c = content_with_magic();
        c.rules.item_powers = Registry::new(vec![
            power(
                "attack_1",
                0,
                50,
                &[Sword, Axe, Misc],
                Some((PowerBonus::Attack, 1)),
                None,
            ),
            power(
                "attack_2",
                0,
                100,
                &[Sword, Axe, Misc],
                Some((PowerBonus::Attack, 2)),
                None,
            ),
            power(
                "defense_2",
                2,
                100,
                &[Sword, Misc, Shield, Chainmail],
                Some((PowerBonus::Defense, 2)),
                None,
            ),
            power(
                "movement_2",
                5,
                200,
                &[Misc, Chainmail],
                Some((PowerBonus::Moves, 2)),
                None,
            ),
            power("flaming", 9, 300, &[Sword, Staff], None, Some(("chaos", 2))),
            power("cloak", 9, 150, &[Sword, Misc], None, None),
            power("spell_charges", 9, 1000, &[Staff, Wand], None, None),
        ]);
        c
    }

    fn world(skill: i32) -> Game {
        let c = rules_with_items();
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(7);
        g.content = c;
        g.map = Some(Map::new(Grid::new(8, 8, grass), Grid::new(8, 8, grass)));
        g.add_player(P0, "Merlin");
        g.treasury_mut(P0).unwrap().mana = 2000;
        g.init_wizard(P0, &[("chaos", 2)], &[], None).unwrap();
        let known: Vec<_> = ["enchant_item", "create_artifact", "giant_strength"]
            .iter()
            .map(|s| g.content.rules.spells.id_of(s).unwrap())
            .collect();
        let mut m = g.wizard_mut(P0).unwrap();
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .researchable
            .retain(|s| !known.contains(s));
        m.get_mut::<crate::wizard::sheet::Candidates>()
            .0
            .retain(|s| !known.contains(s));
        if m.get_mut::<crate::wizard::sheet::Research>()
            .spell
            .is_some_and(|s| known.contains(&s))
        {
            m.get_mut::<crate::wizard::sheet::Research>().spell = None;
        }
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .extend(known);
        m.get_mut::<crate::wizard::sheet::Skill>().left = skill;
        g
    }

    fn design(kind: ItemKind, powers: &[&str]) -> ItemDesign {
        ItemDesign {
            name: String::new(),
            kind,
            icon: 0,
            powers: powers.iter().map(|p| p.to_string()).collect(),
            charges: None,
        }
    }

    fn check_in(g: &Game, craft: Craft, d: &ItemDesign) -> Result<(), CraftError> {
        check(d, craft, &g.wizard(P0).unwrap(), &g.content.rules)
    }

    #[test]
    fn the_cost_is_the_base_plus_each_power_and_a_plain_power_on_a_misc_item_costs_double() {
        let g = world(0);
        let rules = &g.content.rules;
        assert_eq!(cost(&design(ItemKind::Sword, &["attack_2"]), rules), 200);
        assert_eq!(cost(&design(ItemKind::Misc, &["attack_2"]), rules), 250);
        // A special power is never doubled.
        assert_eq!(cost(&design(ItemKind::Misc, &["cloak"]), rules), 200);
        let mut staff = design(ItemKind::Staff, &[]);
        staff.charges = Some(("giant_strength".into(), 3));
        // 300 base, and the spell's 8 mana times 3 charges times 20.
        assert_eq!(cost(&staff, rules), 300 + 8 * 3 * 20);
    }

    #[test]
    fn a_design_is_checked_power_by_power() {
        let g = world(0);
        let ok = |kind, powers: &[&str]| check_in(&g, Craft::CreateArtifact, &design(kind, powers));
        assert_eq!(ok(ItemKind::Sword, &["attack_1", "defense_2"]), Ok(()));
        assert_eq!(ok(ItemKind::Sword, &[]), Err(CraftError::NoPowers));
        assert_eq!(
            ok(ItemKind::Sword, &["nope"]),
            Err(CraftError::UnknownPower)
        );
        assert_eq!(
            ok(ItemKind::Sword, &["spell_charges"]),
            Err(CraftError::UnknownPower),
            "charges are asked for with a spell, not as a power"
        );
        assert_eq!(
            ok(ItemKind::Sword, &["attack_1", "attack_1"]),
            Err(CraftError::Repeated)
        );
        assert_eq!(
            ok(ItemKind::Sword, &["attack_1", "attack_2"]),
            Err(CraftError::SameGroup)
        );
        assert_eq!(
            ok(ItemKind::Shield, &["attack_1"]),
            Err(CraftError::WrongKind)
        );
        assert_eq!(
            ok(
                ItemKind::Misc,
                &["attack_1", "defense_2", "movement_2", "cloak", "x"]
            ),
            Err(CraftError::TooManyPowers)
        );
        // Two books in chaos are enough for Flaming, and one is not.
        assert_eq!(ok(ItemKind::Sword, &["flaming"]), Ok(()));
        let mut poor = world(0);
        poor.wizard_mut(P0)
            .unwrap()
            .get_mut::<crate::wizard::sheet::Books>()
            .0
            .insert("chaos".into(), 1);
        assert_eq!(
            check_in(
                &poor,
                Craft::CreateArtifact,
                &design(ItemKind::Sword, &["flaming"])
            ),
            Err(CraftError::NeedsBooks)
        );
    }

    #[test]
    fn enchant_item_takes_only_powers_costing_200_or_less() {
        let g = world(0);
        let flaming = design(ItemKind::Sword, &["flaming"]);
        assert_eq!(
            check_in(&g, Craft::EnchantItem, &flaming),
            Err(CraftError::TooCostly)
        );
        assert_eq!(check_in(&g, Craft::CreateArtifact, &flaming), Ok(()));
        let cheap = design(ItemKind::Sword, &["attack_2", "cloak"]);
        assert_eq!(check_in(&g, Craft::EnchantItem, &cheap), Ok(()));
        let mut staff = design(ItemKind::Staff, &[]);
        staff.charges = Some(("giant_strength".into(), 1));
        assert_eq!(
            check_in(&g, Craft::EnchantItem, &staff),
            Err(CraftError::TooCostly),
            "the spell power costs 1000"
        );
        assert_eq!(check_in(&g, Craft::CreateArtifact, &staff), Ok(()));
    }

    #[test]
    fn charges_need_a_known_battle_spell_and_one_to_four_of_them() {
        let g = world(0);
        let mut staff = design(ItemKind::Staff, &[]);
        let mut with = |spell: &str, n: u32| {
            staff.charges = Some((spell.into(), n));
            check_in(&g, Craft::CreateArtifact, &staff)
        };
        assert_eq!(with("giant_strength", 4), Ok(()));
        assert_eq!(with("giant_strength", 0), Err(CraftError::BadCharges));
        assert_eq!(with("giant_strength", 5), Err(CraftError::BadCharges));
        assert_eq!(with("nowhere", 1), Err(CraftError::BadSpell));
        assert_eq!(
            with("magic_spirit", 1),
            Err(CraftError::BadSpell),
            "not known, and not for battle"
        );
        let sword = ItemDesign {
            charges: Some(("giant_strength".into(), 1)),
            ..design(ItemKind::Sword, &["attack_1"])
        };
        assert_eq!(
            check_in(&g, Craft::CreateArtifact, &sword),
            Err(CraftError::WrongKind)
        );
    }

    #[test]
    fn the_powers_on_offer_follow_the_kind_the_cost_and_the_books() {
        let g = world(0);
        let ids = |kind, craft| -> Vec<String> {
            available_powers(kind, craft, &g.wizard(P0).unwrap(), &g.content.rules)
                .iter()
                .map(|p| p.id.clone())
                .collect()
        };
        assert_eq!(
            ids(ItemKind::Sword, Craft::EnchantItem),
            ["attack_1", "attack_2", "defense_2", "cloak"]
        );
        assert_eq!(
            ids(ItemKind::Sword, Craft::CreateArtifact),
            ["attack_1", "attack_2", "defense_2", "flaming", "cloak"]
        );
        assert_eq!(
            ids(ItemKind::Staff, Craft::EnchantItem),
            ["spell_charges"][..0]
        );
    }

    #[test]
    fn building_an_item_gives_bonuses_armour_extras_and_a_default_name() {
        let g = world(0);
        let rules = &g.content.rules;
        let sword = build(&design(ItemKind::Sword, &["attack_2", "cloak"]), rules);
        assert_eq!(sword.bonus.attack, 2);
        assert_eq!(sword.powers, ["cloak"]);
        assert_eq!(sword.cost, 100 + 100 + 150);
        assert_eq!(sword.name, "+2 Sword of cloak");
        // Chain mail gains a defence of its own, and a movement power is whole moves in half-moves.
        let mail = build(
            &design(ItemKind::Chainmail, &["defense_2", "movement_2"]),
            rules,
        );
        assert_eq!(mail.bonus.defense, 3);
        assert_eq!(mail.bonus.moves, 4);
        assert_eq!(mail.name, "Chain Mail of Speed");
        // The picture is one of the kind's range, or the first of it.
        let pick = |icon| {
            build(
                &ItemDesign {
                    icon,
                    ..design(ItemKind::Mace, &["attack_1"])
                },
                rules,
            )
            .icon
        };
        assert_eq!(pick(12), 12);
        assert_eq!(pick(0), 9);
        assert_eq!(pick(90), 9);
        let misc = |icon| ItemDesign {
            icon,
            ..design(ItemKind::Misc, &[])
        };
        assert_eq!(default_name(&misc(0), rules), "Amulet");
        assert_eq!(default_name(&misc(80), rules), "Ring");
        assert_eq!(default_name(&misc(86), rules), "Cloak");
        assert_eq!(default_name(&misc(92), rules), "Gauntlet");
        assert_eq!(default_name(&misc(100), rules), "Helm");
        assert_eq!(default_name(&misc(101), rules), "Orb");
        let named = ItemDesign {
            name: "Sting".into(),
            ..design(ItemKind::Sword, &["attack_1"])
        };
        assert_eq!(build(&named, rules).name, "Sting");
        let mut staff = design(ItemKind::Staff, &[]);
        staff.charges = Some(("giant_strength".into(), 2));
        let staff = build(&staff, rules);
        assert_eq!(staff.name, "Staff of Giant Strength x2");
        assert_eq!(staff.charges, Some(("giant_strength".into(), 2)));
    }

    #[test]
    fn an_item_that_can_be_paid_at_once_is_made_at_once_and_costs_the_reduced_price() {
        let mut g = world(1000);
        let d = design(ItemKind::Sword, &["attack_2"]);
        let events = g.apply(Command::CraftItem {
            player: P0,
            craft: Craft::EnchantItem,
            design: d,
        });
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::ItemMade {
                    placed: Placed::Vault(0),
                    ..
                }
            )),
            "{events:?}"
        );
        assert_eq!(
            g.pl::<crate::item::armory::Armory>(0).vault[0]
                .as_ref()
                .unwrap()
                .bonus
                .attack,
            2
        );
        assert_eq!(g.tr(0).mana, 2000 - 200);
        assert_eq!(g.wizard(P0).unwrap().skill_left(), 800);
        assert!(g.wizard(P0).unwrap().crafting().is_none());
    }

    #[test]
    fn artificer_halves_the_mana_not_the_value() {
        let mut g = world(1000);
        g.wizard_mut(P0)
            .unwrap()
            .get_mut::<crate::wizard::sheet::Retorts>()
            .0
            .insert(crate::wizard::magic::Retort::Artificer);
        g.apply(Command::CraftItem {
            player: P0,
            craft: Craft::EnchantItem,
            design: design(ItemKind::Sword, &["attack_2"]),
        });
        assert_eq!(g.tr(0).mana, 2000 - 100);
        assert_eq!(
            g.pl::<crate::item::armory::Armory>(0).vault[0]
                .as_ref()
                .unwrap()
                .cost,
            200
        );
    }

    #[test]
    fn an_item_too_dear_for_this_turn_waits_while_it_is_paid_for() {
        let mut g = world(4);
        let d = design(ItemKind::Sword, &["attack_2"]);
        let events = g.apply(Command::CraftItem {
            player: P0,
            craft: Craft::EnchantItem,
            design: d,
        });
        assert!(events.is_empty(), "{events:?}");
        let m = g.wizard(P0).unwrap();
        assert_eq!(m.casting().unwrap().remaining, 200);
        assert!(m.crafting().is_some());
        // A second item cannot be started meanwhile.
        let again = g.apply(Command::CraftItem {
            player: P0,
            craft: Craft::EnchantItem,
            design: design(ItemKind::Sword, &["attack_1"]),
        });
        assert!(matches!(
            again[0],
            Event::Rejected(crate::game::Rejection::Craft(CraftingError::Magic(_)))
        ));
        let mut made = false;
        for _ in 0..100 {
            let events = g.apply(Command::EndTurn);
            if events.iter().any(|e| matches!(e, Event::ItemMade { .. })) {
                made = true;
                break;
            }
        }
        assert!(made, "the item should come when the cost is paid");
        assert_eq!(
            g.pl::<crate::item::armory::Armory>(0).vault[0]
                .as_ref()
                .unwrap()
                .bonus
                .attack,
            2
        );
        assert!(g.wizard(P0).unwrap().crafting().is_none());
        assert!(g.wizard(P0).unwrap().casting().is_none());
    }

    #[test]
    fn cancelling_the_cast_drops_the_item() {
        let mut g = world(4);
        g.apply(Command::CraftItem {
            player: P0,
            craft: Craft::EnchantItem,
            design: design(ItemKind::Sword, &["attack_2"]),
        });
        g.wizard_mut(P0).unwrap().cancel_casting();
        assert!(g.wizard(P0).unwrap().crafting().is_none());
    }

    #[test]
    fn a_bad_design_or_an_unknown_spell_is_refused_and_costs_nothing() {
        let mut g = world(1000);
        let events = g.apply(Command::CraftItem {
            player: P0,
            craft: Craft::EnchantItem,
            design: design(ItemKind::Sword, &["flaming"]),
        });
        assert!(matches!(
            events[0],
            Event::Rejected(crate::game::Rejection::Craft(CraftingError::Design(
                CraftError::TooCostly
            )))
        ));
        assert_eq!(g.tr(0).mana, 2000);
        g.wizard_mut(P0)
            .unwrap()
            .get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .clear();
        let events = g.apply(Command::CraftItem {
            player: P0,
            craft: Craft::CreateArtifact,
            design: design(ItemKind::Sword, &["attack_1"]),
        });
        assert!(matches!(
            events[0],
            Event::Rejected(crate::game::Rejection::Craft(CraftingError::Magic(_)))
        ));
    }

    #[test]
    fn the_crafting_spells_are_not_cast_without_a_design() {
        let mut g = world(1000);
        let spell = g.content.rules.spells.id_of("enchant_item").unwrap();
        assert!(g.start_cast(P0, spell, None).is_err());
    }

    #[test]
    fn the_made_item_is_saved_while_it_waits() {
        let mut g = world(4);
        g.apply(Command::CraftItem {
            player: P0,
            craft: Craft::EnchantItem,
            design: design(ItemKind::Sword, &["attack_2"]),
        });
        let text = g.snapshot().to_text().unwrap();
        let back = crate::game::save::Snapshot::from_text(&text)
            .unwrap()
            .restore(g.content.clone())
            .unwrap();
        let (was, now) = (g.wizard(P0).unwrap(), back.wizard(P0).unwrap());
        assert!(was.crafting().is_some());
        assert_eq!(now.crafting(), was.crafting());
        assert_eq!(now.casting(), was.casting());
    }
}
