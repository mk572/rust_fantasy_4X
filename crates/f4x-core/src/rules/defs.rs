//! Definition types: static content describing races, units, buildings, spells and
//! so on. Runtime state (damage taken, remaining duration) lives in ECS components.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::effects::EffectRule;
use super::registry::{Def, Registry};
use super::stats::Stat;
use crate::content::TerrainClass;

macro_rules! impl_def {
    ($($t:ty),*) => { $(impl Def for $t { fn id(&self) -> &str { &self.id } })* };
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceDef {
    pub id: String,
    pub name: String,
    /// Units cities of this race can build.
    pub units: Vec<String>,
    /// Buildings cities of this race can build (besides race-neutral ones).
    pub buildings: Vec<String>,
    /// Terrain the race's cities prefer; used by AI and map placement.
    #[serde(default)]
    pub home_terrain: Vec<TerrainClass>,
    /// Applied to every city of the race.
    #[serde(default)]
    pub effects: Vec<EffectRule>,
    /// Names handed to new cities, in order.
    #[serde(default)]
    pub city_names: Vec<String>,
    /// Food each farmer grows.
    #[serde(default = "two")]
    pub farmer_food: u8,
    /// Production per worker, in half-units (4 = two).
    #[serde(default = "four")]
    pub worker_production_x2: u8,
    /// Mana per citizen, in half-units (2 = one per citizen).
    #[serde(default)]
    pub mana_per_citizen_x2: u8,
    /// Doubles taxes and the yield of mines.
    #[serde(default)]
    pub double_mining: bool,
    /// Extra percent of gold from trade.
    #[serde(default)]
    pub trade_percent: i32,
    /// Base chance (percent) per turn that an outpost of the race grows.
    #[serde(default = "ten")]
    pub outpost_growth: u8,
    /// Houses in the city picture: 0 standard, 1 tree houses, 2 mounds.
    #[serde(default)]
    pub house_type: u8,
    /// Neutral cities of this race stand on Myrror; the others on Arcanus.
    #[serde(default)]
    pub myrror: bool,
}

fn two() -> u8 {
    2
}
fn four() -> u8 {
    4
}
fn ten() -> u8 {
    10
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitKind {
    /// Trained by a city of its race.
    Normal,
    /// Hired or summoned individual with items and levels.
    Hero,
    /// Conjured by a spell; costs mana upkeep.
    Summoned,
    /// Neutral or realm creature that is not built by a race.
    Fantastic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MovementType {
    Walking,
    Flying,
    Swimming,
    Sailing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Upkeep {
    #[serde(default)]
    pub gold: u32,
    #[serde(default)]
    pub food: u32,
    #[serde(default)]
    pub mana: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemSlot {
    Weapon,
    Armor,
    Misc,
}

/// The kind of a hero, which decides what its three item slots take (`Hero_Slot_Types`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HeroKind {
    /// Sword, armour, miscellaneous.
    Warrior,
    /// Bow, armour, miscellaneous.
    Bow,
    /// Sword or staff, armour, miscellaneous.
    SwordOrStaff,
    /// Staff, miscellaneous, miscellaneous.
    Mage,
}

/// What a hero's item slot takes (the original's six item slot types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SlotKind {
    /// Sword, mace or axe.
    Sword,
    /// A bow, or what the sword slot takes.
    Bow,
    /// What the sword slot takes, or a staff or wand.
    SwordOrStaff,
    /// Staff or wand.
    Staff,
    /// Shields and mail.
    Armor,
    Misc,
}

impl HeroKind {
    /// The kinds of the hero's three slots.
    pub fn slots(self) -> [SlotKind; 3] {
        match self {
            Self::Warrior => [SlotKind::Sword, SlotKind::Armor, SlotKind::Misc],
            Self::Bow => [SlotKind::Bow, SlotKind::Armor, SlotKind::Misc],
            Self::SwordOrStaff => [SlotKind::SwordOrStaff, SlotKind::Armor, SlotKind::Misc],
            Self::Mage => [SlotKind::Staff, SlotKind::Misc, SlotKind::Misc],
        }
    }
}

impl SlotKind {
    /// Whether an item of this kind may go in the slot (`ItemScrn.c`, `Move_Item`): an item
    /// fits the slot of its own type; the bow slot also takes the sword slot's items and the
    /// sword-or-staff slot those and the staff slot's.
    pub fn fits(self, item: ItemKind) -> bool {
        use ItemKind as K;
        let sword = matches!(item, K::Sword | K::Mace | K::Axe);
        let staff = matches!(item, K::Staff | K::Wand);
        match self {
            Self::Sword => sword,
            Self::Bow => sword || item == K::Bow,
            Self::SwordOrStaff => sword || staff,
            Self::Staff => staff,
            Self::Armor => matches!(item, K::Shield | K::Chainmail | K::Platemail),
            Self::Misc => item == K::Misc,
        }
    }
}

/// Extra data for [`UnitKind::Hero`] units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroDef {
    /// Decides the hero's three item slots.
    pub kind: HeroKind,
    /// The hero's portrait: an entry of the original's `PORTRAIT.LBX`.
    pub portrait: u32,
    /// Gold to hire.
    pub hire_cost: u32,
}

impl HeroDef {
    /// The kinds of the hero's three item slots.
    pub fn slots(&self) -> [SlotKind; 3] {
        self.kind.slots()
    }
}

/// Which abilities a hero's random picks may draw (`Init_Heroes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PickType {
    Warrior,
    Mage,
    /// Both the warrior and the mage abilities.
    Any,
}

/// How a hero type starts in each wizard's game (`NewGame.c:_hero_premade_table`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroTemplate {
    /// The hero's unit id.
    pub unit: String,
    /// Abilities drawn at random for each wizard at the start of the game.
    pub picks: u32,
    pub pick_type: PickType,
    /// Abilities every wizard's copy has (names in [`HERO_ABILITIES`]).
    pub abilities: Vec<String>,
    pub casting_skill: u32,
    /// The innate spells (ids in `spells.ron`), at most four.
    pub spells: Vec<String>,
    /// The hero's name in each wizard's game, by the wizard's number (the original's
    /// `NAMES.LBX`, local only, from `hero_names.ron`); empty uses the unit's own name.
    #[serde(default)]
    pub names: Vec<String>,
}

/// The hero abilities, by the name `heroes.ron` uses (`super_` is the doubled version of the
/// one before it; they are the original's `HSA_*` flags).
pub const HERO_ABILITIES: [&str; 24] = [
    "leadership",
    "super_leadership",
    "legendary",
    "super_legendary",
    "blademaster",
    "super_blademaster",
    "armsmaster",
    "super_armsmaster",
    "constitution",
    "super_constitution",
    "might",
    "super_might",
    "arcane_power",
    "super_arcane_power",
    "sage",
    "super_sage",
    "prayermaster",
    "super_prayermaster",
    "agility",
    "super_agility",
    "lucky",
    "charmed",
    "noble",
    "female",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitDef {
    pub id: String,
    pub name: String,
    pub kind: UnitKind,
    /// Required for `Normal` units; heroes and creatures have none.
    #[serde(default)]
    pub race: Option<String>,
    /// Required for `Summoned` units.
    #[serde(default)]
    pub realm: Option<String>,
    /// Production cost (gold-equivalent) or summoning mana cost.
    pub cost: u32,
    #[serde(default)]
    pub upkeep: Upkeep,
    pub stats: BTreeMap<Stat, i32>,
    pub movement: MovementType,
    #[serde(default)]
    pub abilities: Vec<String>,
    /// Buildings the producing city must have.
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub level_path: Option<String>,
    #[serde(default)]
    pub hero: Option<HeroDef>,
    /// Picture for the client: `(file, entry)` in the local art set.
    #[serde(default)]
    pub art: Option<(String, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub effects: Vec<EffectRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingDef {
    pub id: String,
    pub name: String,
    pub cost: u32,
    #[serde(default)]
    pub upkeep_gold: u32,
    #[serde(default)]
    pub requires: Vec<String>,
    /// Building made redundant by this one. It still counts as built (for requirements
    /// and effects) but costs no upkeep.
    #[serde(default)]
    pub replaces: Option<String>,
    /// Races that can build it; empty means every race.
    #[serde(default)]
    pub races: Vec<String>,
    #[serde(default)]
    pub effects: Vec<EffectRule>,
    /// Footprint in the city picture: 0 is 2x2, 1 is 2x3, 2 is 3x2, 3 is 3x3.
    #[serde(default)]
    pub shape: u8,
    /// Only a coastal city (one next to the sea) can build it.
    #[serde(default)]
    pub coastal: bool,
}

/// A school of magic. Data, so the set of realms is not hard-coded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealmDef {
    pub id: String,
    pub name: String,
    pub color: (u8, u8, u8),
    /// Spells (ids) a wizard with books in this realm starts with when they choose none, in
    /// the order they are taken: common ones first.
    #[serde(default)]
    pub default_spells: Vec<String>,
}

/// A ready-made wizard for the creation screen: a book and retort loadout, and the name and
/// description that the local texts file fills in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardDef {
    pub id: String,
    /// The original's name is local-only; without it the wizard is named after its id.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Spell books as `(realm id, count)`.
    pub books: Vec<(String, u8)>,
    pub retorts: Vec<crate::magic::Retort>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    VeryRare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastContext {
    Overland,
    Battle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpellTarget {
    None,
    OwnUnit,
    EnemyUnit,
    AnyUnit,
    OwnCity,
    EnemyCity,
    Tile,
    Battlefield,
    Wizard,
    /// Another wizard, by clicking one of their cities or units (Spell Blast).
    EnemyWizard,
    /// One of another wizard's global enchantments (Disjunction, Spell Binding).
    GlobalEnchantment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Duration {
    Instant,
    Permanent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellDef {
    pub id: String,
    pub name: String,
    pub realm: String,
    pub rarity: Rarity,
    /// Mana to cast.
    pub cost: u32,
    pub research_cost: u32,
    pub contexts: Vec<CastContext>,
    /// A spell that can also be cast in battle costs five times as much on the map (the
    /// original's eligibility 0, 2 and 3; the spells it sells only for the map, or for
    /// battle, are not dearer).
    #[serde(default)]
    pub overland_x5: bool,
    pub target: SpellTarget,
    pub duration: Duration,
    /// Mana per turn to keep a lasting spell going.
    #[serde(default)]
    pub upkeep: u32,
    /// Strength a dispel must beat.
    #[serde(default)]
    pub dispel_difficulty: u32,
    /// A curse the target can resist when it is cast in a battle.
    #[serde(default)]
    pub save: Option<SpellSave>,
    pub effects: Vec<EffectRule>,
}

/// The resistance check against a lasting spell that harms its target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellSave {
    /// Added to the target's resistance (negative: harder to resist).
    pub modifier: i32,
    /// A target with one of these abilities cannot be affected at all.
    #[serde(default)]
    pub immune: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDef {
    pub id: String,
    pub name: String,
    pub slot: ItemSlot,
    /// The original's item type; it fixes the slot (see [`ItemKind::slot`]).
    pub kind: ItemKind,
    pub cost: u32,
    /// The picture, an entry of the original's `ITEMS.LBX` (task 261).
    #[serde(default)]
    pub icon: u16,
    /// The plain bonuses: attack, to hit, defence, movement (in half-moves), resistance.
    pub effects: Vec<EffectRule>,
    /// Added to the hero's mana in battle.
    #[serde(default)]
    pub spell_skill: i32,
    /// The size of the resistance penalty put on what the hero's spells hit, as the original stores it.
    #[serde(default)]
    pub spell_save: i32,
    /// Special powers (ids in `item_powers.ron`). What each one does is task 49.
    #[serde(default)]
    pub powers: Vec<String>,
    /// The spell the hero can cast from the item, and how many times (Spell Charges).
    #[serde(default)]
    pub charges: Option<(String, u32)>,
    /// The original's flag byte: set items are left out of the Enchant Item grade, unset ones out
    /// of the Create Artifact grade (`Make_Item`).
    #[serde(default)]
    pub flag: bool,
}

/// The ten item types of the original, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemKind {
    Sword,
    Mace,
    Axe,
    Bow,
    Staff,
    Wand,
    Misc,
    Shield,
    Chainmail,
    Platemail,
}

impl ItemKind {
    /// The hero slot the kind goes in.
    pub fn slot(self) -> ItemSlot {
        match self {
            Self::Sword | Self::Mace | Self::Axe | Self::Bow | Self::Staff | Self::Wand => {
                ItemSlot::Weapon
            }
            Self::Misc => ItemSlot::Misc,
            Self::Shield | Self::Chainmail | Self::Platemail => ItemSlot::Armor,
        }
    }

    /// The pictures of `ITEMS.LBX` that belong to the kind, first and last (the crafting
    /// screen's arrows step through them; `docs/heroes-items-design.md` section 6).
    pub fn icon_range(self) -> (u16, u16) {
        match self {
            Self::Sword => (0, 8),
            Self::Mace => (9, 19),
            Self::Axe => (20, 28),
            Self::Bow => (29, 37),
            Self::Staff => (38, 46),
            Self::Chainmail => (47, 54),
            Self::Platemail => (55, 61),
            Self::Shield => (62, 71),
            Self::Misc => (72, 106),
            Self::Wand => (107, 115),
        }
    }
}

/// One entry of the item power table (`ITEMPOW.LBX`): a plain bonus or a special power an item
/// can carry, with what it costs to put on one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemPowerDef {
    pub id: String,
    pub name: String,
    /// At most one power of a group on an item.
    pub group: u8,
    /// Mana cost of adding the power to an item.
    pub cost: u32,
    /// The item types that may carry it.
    pub kinds: Vec<ItemKind>,
    /// Special powers: the realm and the number of books a wizard needs to add it.
    #[serde(default)]
    pub realm: Option<String>,
    #[serde(default)]
    pub books: u32,
    /// Plain bonuses: what it adds (movement in half-moves; spell save is the size of the penalty).
    #[serde(default)]
    pub bonus: Option<(PowerBonus, i32)>,
    /// The lasting spell (an id of `spells.ron`) the power puts on the hero that carries the item.
    #[serde(default)]
    pub spell: Option<String>,
    /// The special attack ability the power gives the carrier.
    #[serde(default)]
    pub attack: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PowerBonus {
    Attack,
    ToHit,
    Defense,
    Moves,
    Resistance,
    SpellSkill,
    SpellSave,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelDef {
    pub name: String,
    /// Experience needed to reach this level.
    pub xp: u32,
    pub effects: Vec<EffectRule>,
}

/// Experience thresholds and what each level grants (units and heroes share the mechanism).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelPathDef {
    pub id: String,
    pub levels: Vec<LevelDef>,
    /// Levels above the last one experience reaches: only Warlord, Crusade and the like lift a
    /// unit into them (Ultra-Elite and Champion for normal units).
    #[serde(default)]
    pub bonus: Vec<BonusLevelDef>,
}

/// A level that experience alone never gives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BonusLevelDef {
    pub name: String,
    pub effects: Vec<EffectRule>,
}

impl LevelPathDef {
    /// Levels in all, those experience gives and the bonus levels.
    pub fn level_count(&self) -> usize {
        self.levels.len() + self.bonus.len()
    }

    /// Name and effects of level `index` (0-based), bonus levels last.
    pub fn level(&self, index: usize) -> Option<(&str, &[EffectRule])> {
        match self.levels.get(index) {
            Some(l) => Some((&l.name, &l.effects)),
            None => self
                .bonus
                .get(index - self.levels.len())
                .map(|b| (&b.name[..], &b.effects[..])),
        }
    }
}

/// Tunable numbers that are not content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesConfig {
    /// Base chance (percent) for each figure's attack to hit.
    pub to_hit_percent: u8,
    /// Chance (percent) for each point of defense to block a hit.
    pub block_percent: u8,
    /// Minimum Chebyshev distance between cities on one plane.
    pub min_city_distance: u32,
    /// Largest city, in citizens (thousands of people).
    pub max_city_size: u32,
    /// Unrest percent caused by each tax rate; the rate is also half the gold per citizen.
    pub tax_unrest_percent: Vec<u8>,
    /// Unrest (in tens of percent) in a city of one race under a capital of another:
    /// `racial_unrest[capital race][city race]`, indexed in race file order. Empty means none.
    pub racial_unrest: Vec<Vec<i8>>,
    /// Building ids in the order the Grand Vizier picks them (our own list).
    pub autobuild_order: Vec<String>,
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self {
            to_hit_percent: 30,
            block_percent: 30,
            min_city_distance: 4,
            max_city_size: 25,
            tax_unrest_percent: vec![0, 10, 20, 30, 45, 60, 75],
            racial_unrest: Vec::new(),
            autobuild_order: Vec::new(),
        }
    }
}

impl_def!(
    RaceDef,
    UnitDef,
    AbilityDef,
    BuildingDef,
    RealmDef,
    SpellDef,
    ItemDef,
    ItemPowerDef,
    LevelPathDef,
    WizardDef
);

/// All rules definitions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rules {
    pub config: RulesConfig,
    pub races: Registry<RaceDef>,
    pub units: Registry<UnitDef>,
    pub abilities: Registry<AbilityDef>,
    pub buildings: Registry<BuildingDef>,
    pub realms: Registry<RealmDef>,
    pub spells: Registry<SpellDef>,
    pub items: Registry<ItemDef>,
    pub item_powers: Registry<ItemPowerDef>,
    pub level_paths: Registry<LevelPathDef>,
    pub wizards: Registry<WizardDef>,
    /// The starting record of each hero type, in the order of the hero units (optional).
    pub hero_templates: Vec<HeroTemplate>,
    /// Names for rival wizards when no ready-made wizards are loaded: a list for each rival slot.
    pub wizard_names: Vec<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_slots_take_what_the_original_lets_them() {
        use ItemKind::*;
        // A sword slot takes swords, maces and axes; the bow slot those and bows.
        assert!(SlotKind::Sword.fits(Mace) && !SlotKind::Sword.fits(Bow));
        assert!(SlotKind::Bow.fits(Bow) && SlotKind::Bow.fits(Axe) && !SlotKind::Bow.fits(Staff));
        // Sword or staff takes the sword slot's items and staves and wands, but no bow.
        assert!(SlotKind::SwordOrStaff.fits(Sword) && SlotKind::SwordOrStaff.fits(Wand));
        assert!(!SlotKind::SwordOrStaff.fits(Bow));
        assert!(SlotKind::Staff.fits(Wand) && !SlotKind::Staff.fits(Sword));
        assert!(SlotKind::Armor.fits(Shield) && SlotKind::Armor.fits(Platemail));
        assert!(SlotKind::Misc.fits(Misc) && !SlotKind::Misc.fits(Shield));
        // A mage carries a staff and two miscellaneous items.
        assert_eq!(
            HeroKind::Mage.slots(),
            [SlotKind::Staff, SlotKind::Misc, SlotKind::Misc]
        );
    }
}
