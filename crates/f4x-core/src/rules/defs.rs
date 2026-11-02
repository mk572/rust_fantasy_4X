//! Definition types: static content describing races, units, buildings, spells and
//! so on. Runtime state (damage taken, remaining duration) lives in ECS components.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::effects::{resolve_rules, EffectRule};
use super::refs::{Names, Ref};
use super::registry::{Def, Registry};
use super::stats::Stat;
use super::{Ability, BuildingId, RealmId};
use crate::map::terrain::TerrainClass;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceDef {
    /// The original's race number, the id the game passes around (and the row of the
    /// racial unrest matrix).
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// From `text/<language>/name/` by `num`, not from the rules file.
    #[serde(default)]
    pub name: String,
    /// The race's townsfolk topic in `help.ron` (an index).
    #[serde(default)]
    pub help: Option<u16>,
    /// The name for more than one ("Barbarians"); empty uses the name.
    #[serde(default)]
    pub plural: String,
    /// Units cities of this race can build.
    pub units: Vec<Ref<UnitDef>>,
    /// Buildings cities of this race can build (besides race-neutral ones).
    pub buildings: Vec<Ref<BuildingDef>>,
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
    /// In half food, like moves: 2 is one food a turn, 1 is half a food, 0 eats nothing.
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
    /// The same unit as a number, filled once when the rules load ([`resolve_hero_units`]).
    #[serde(skip)]
    pub unit_id: Option<super::UnitId>,
    /// Abilities drawn at random for each wizard at the start of the game.
    pub picks: u32,
    pub pick_type: PickType,
    /// Abilities every wizard's copy has: hero abilities of `abilities.ron` (the ones
    /// [`HeroAbilities::ids`](crate::hero::HeroAbilities::ids) can give).
    pub abilities: Vec<Ref<AbilityDef>>,
    pub casting_skill: u32,
    /// The innate spells (ids in `spells.ron`), at most four.
    pub spells: Vec<Ref<SpellDef>>,
    /// The hero's name in each wizard's game, by the wizard's number (the original's
    /// `NAMES.LBX`, from `text/<lang>/name/hero.ron`).
    #[serde(default)]
    pub names: Vec<String>,
    /// For the messages that name the hero (`docs/text.md`).
    #[serde(default)]
    pub gender: crate::text::Gender,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitDef {
    /// The original's unit type number, the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// From `text/<language>/name/` by `num`, not from the rules file.
    #[serde(default)]
    pub name: String,
    pub kind: UnitKind,
    /// Required for `Normal` units; heroes and creatures have none.
    #[serde(default)]
    pub race: Option<Ref<RaceDef>>,
    /// Required for `Summoned` units.
    #[serde(default)]
    pub realm: Option<Ref<RealmDef>>,
    /// Production cost (gold-equivalent) or summoning mana cost.
    pub cost: u32,
    #[serde(default)]
    pub upkeep: Upkeep,
    pub stats: BTreeMap<Stat, i32>,
    pub movement: MovementType,
    #[serde(default)]
    pub abilities: Vec<String>,
    /// The same abilities as numbers, filled once when the rules load ([`resolve_unit_abilities`]).
    #[serde(skip)]
    pub ability_ids: Vec<Ability>,
    /// Buildings the producing city must have (author names).
    #[serde(default)]
    pub requires: Vec<String>,
    /// The same buildings as numbers, filled once when the rules load
    /// ([`resolve_building_links`]).
    #[serde(skip)]
    pub require_ids: Vec<BuildingId>,
    #[serde(default)]
    pub level_path: Option<Ref<LevelPathDef>>,
    #[serde(default)]
    pub hero: Option<HeroDef>,
    /// Picture for the client: `(file, entry)` in the local art set.
    #[serde(default)]
    pub art: Option<(String, u16)>,
    /// A ship's deck in a sea battle: `(width, length)` in squares, pointed at both ends. The
    /// battle field is a diamond of about 200 squares, so two facing decks need
    /// `width + width + 1 + length <= 20`.
    #[serde(default)]
    pub deck: Option<(u8, u8)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityDef {
    /// The ability's number, the id the game passes around: the original has no table of
    /// abilities, so they are our own, counted from 1 and frozen.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// From `text/<language>/name/` by `num`, not from the rules file.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The ability's topic in `help.ron` (an index).
    #[serde(default)]
    pub help: Option<u16>,
    pub effects: Vec<EffectRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingDef {
    /// The original's building number, the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// From `text/<language>/name/` by `num`, not from the rules file.
    #[serde(default)]
    pub name: String,
    /// The building's topic in `help.ron` (an index): its description in the city builder and its
    /// help pop-up.
    #[serde(default)]
    pub help: Option<u16>,
    pub cost: u32,
    #[serde(default)]
    pub upkeep_gold: u32,
    #[serde(default)]
    pub requires: Vec<String>,
    /// The same buildings as numbers, filled once when the rules load
    /// ([`resolve_building_links`]).
    #[serde(skip)]
    pub require_ids: Vec<BuildingId>,
    /// Building made redundant by this one. It still counts as built (for requirements
    /// and effects) but costs no upkeep.
    #[serde(default)]
    pub replaces: Option<String>,
    /// The replaced building as a number ([`resolve_building_links`]).
    #[serde(skip)]
    pub replace_id: Option<BuildingId>,
    /// Races that can build it; empty means every race.
    #[serde(default)]
    pub races: Vec<Ref<RaceDef>>,
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
    /// The original's realm number, the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// From `text/<language>/name/` by `num`, not from the rules file.
    #[serde(default)]
    pub name: String,
    /// The realm's spells topic in `help.ron` (an index).
    #[serde(default)]
    pub help: Option<u16>,
    pub color: (u8, u8, u8),
    /// Spells (ids) a wizard with books in this realm starts with when they choose none, in
    /// the order they are taken: common ones first.
    #[serde(default)]
    pub default_spells: Vec<Ref<SpellDef>>,
}

/// The five realms that have spell books, in the original's order (`sbr_Nature` to `sbr_Death`);
/// arcane has none.
pub const BOOK_REALMS: [RealmId; 5] = [
    RealmId::new(0), // nature
    RealmId::new(1), // sorcery
    RealmId::new(2), // chaos
    RealmId::new(3), // life
    RealmId::new(4), // death
];

/// What the books must be before a retort may be picked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetortNeed {
    /// At least this many books in the realm (by alias).
    Books(Ref<RealmDef>, u8),
    /// At least `count` realms with this many books or more each.
    Realms(u8, u8),
}

/// A retort: a special ability of a wizard, picked at the creation screen or won as loot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetortDef {
    /// The retort's place in the original's wizard record, the id the game passes around and the
    /// saved game writes.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// From `text/<language>/name/retort.ron` by `num`, not from the rules file.
    #[serde(default)]
    pub name: String,
    /// The retort's topic in `help.ron` (an index).
    #[serde(default)]
    pub help: Option<u16>,
    /// Picks it takes at the creation screen.
    pub cost: u8,
    /// What the books must be before it may be picked.
    #[serde(default)]
    pub requires: Vec<RetortNeed>,
}

impl RetortDef {
    /// Do these books (per realm) meet every need?
    pub fn available(&self, books: &std::collections::BTreeMap<RealmId, u8>) -> bool {
        self.requires.iter().all(|need| match need {
            RetortNeed::Books(realm, min) => {
                realm.id().and_then(|r| books.get(&r)).copied().unwrap_or(0) >= *min
            }
            RetortNeed::Realms(count, min) => {
                books.values().filter(|&&n| n >= *min).count() >= usize::from(*count)
            }
        })
    }
}

/// A ready-made wizard for the creation screen: a book and retort loadout, and the name and
/// description that the local texts file fills in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardDef {
    /// The wizard's place in the original's list, the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// From `text/<language>/name/wizard.ron` by `num`, not from the rules file.
    #[serde(default)]
    pub name: String,
    /// The wizard's topic in `help.ron` (an index).
    pub help: u16,
    #[serde(default)]
    pub gender: crate::text::Gender,
    /// Spell books as `(realm id, count)`.
    pub books: Vec<(Ref<RealmDef>, u8)>,
    /// The retorts the wizard starts with.
    pub retorts: Vec<Ref<RetortDef>>,
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
    /// The original's spell number (`SPELLDAT.LBX` record), the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// From `text/<language>/name/` by `num`, not from the rules file.
    #[serde(default)]
    pub name: String,
    /// The spell's topic in `help.ron` (an index).
    #[serde(default)]
    pub help: Option<u16>,
    pub realm: Ref<RealmDef>,
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
    /// Spells this one is not cumulative with. It cannot be cast on a unit that has one of them
    /// (or, for a global enchantment, on a wizard who keeps it), and while the unit has one its
    /// own bonuses do not count: Stone Skin gives way to Iron Skin.
    #[serde(default)]
    pub not_with: Vec<Ref<SpellDef>>,
    /// Abilities a unit has of its own that make this spell pointless: it cannot be cast on a
    /// unit that has one (a unit that flies by nature cannot be given Wind Walking).
    #[serde(default)]
    pub already_able: Vec<Ref<AbilityDef>>,
    pub effects: Vec<EffectRule>,
}

/// The resistance check against a lasting spell that harms its target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellSave {
    /// Added to the target's resistance (negative: harder to resist).
    pub modifier: i32,
    /// A target with one of these abilities cannot be affected at all.
    #[serde(default)]
    pub immune: Vec<Ref<AbilityDef>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDef {
    /// The original's item number, the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// From `text/<language>/name/` by `num`, not from the rules file.
    #[serde(default)]
    pub name: String,
    pub slot: ItemSlot,
    /// The original's item type; it fixes the slot (see [`ItemKind::slot`]).
    pub kind: ItemKind,
    pub cost: u32,
    /// The picture, an entry of the original's `ITEMS.LBX`.
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
    /// Special powers (ids in `item_powers.ron`).
    #[serde(default)]
    pub powers: Vec<Ref<ItemPowerDef>>,
    /// The spell the hero can cast from the item, and how many times (Spell Charges).
    #[serde(default)]
    pub charges: Option<(Ref<SpellDef>, u32)>,
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
    /// The original's item power number, the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// From `text/<language>/name/` by `num`, not from the rules file.
    #[serde(default)]
    pub name: String,
    /// At most one power of a group on an item.
    pub group: u8,
    /// Mana cost of adding the power to an item.
    pub cost: u32,
    /// The item types that may carry it.
    pub kinds: Vec<ItemKind>,
    /// Special powers: the realm and the number of books a wizard needs to add it.
    #[serde(default)]
    pub realm: Option<Ref<RealmDef>>,
    #[serde(default)]
    pub books: u32,
    /// Plain bonuses: what it adds (movement in half-moves; spell save is the size of the penalty).
    #[serde(default)]
    pub bonus: Option<(PowerBonus, i32)>,
    /// The lasting spell (an id of `spells.ron`) the power puts on the hero that carries the item.
    #[serde(default)]
    pub spell: Option<Ref<SpellDef>>,
    /// The special attack ability the power gives the carrier.
    #[serde(default)]
    pub attack: Option<Ref<AbilityDef>>,
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
    /// From `text/<language>/name/level_path.ron`, not from the rules file.
    #[serde(default)]
    pub name: String,
    /// Experience needed to reach this level.
    pub xp: u32,
    pub effects: Vec<EffectRule>,
}

/// Experience thresholds and what each level grants (units and heroes share the mechanism).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelPathDef {
    /// The level path's number (ours: counted from 1 in the alphabetical order of the ids).
    pub num: u16,
    /// The alias for human editing.
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
    /// From `text/<language>/name/level_path.ron`, not from the rules file.
    #[serde(default)]
    pub name: String,
    pub effects: Vec<EffectRule>,
}

impl LevelPathDef {
    /// Put the names of a language on the levels, the bonus levels last; a list of the wrong
    /// length names only as many levels as it has.
    pub fn set_level_names(&mut self, names: Vec<String>) {
        let mut names = names.into_iter();
        for l in &mut self.levels {
            l.name = names.next().unwrap_or_default();
        }
        for b in &mut self.bonus {
            b.name = names.next().unwrap_or_default();
        }
    }

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
    /// `racial_unrest[capital race][city race]`, indexed by race number. Empty means none.
    pub racial_unrest: Vec<Vec<i8>>,
    /// Building ids in the order the Grand Vizier picks them (our own list).
    pub autobuild_order: Vec<Ref<BuildingDef>>,
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

macro_rules! impl_numbered_def {
    ($($t:ty),* $(,)?) => {
        $(impl Def for $t {
            fn id(&self) -> &str {
                &self.id
            }

            fn num(&self) -> u16 {
                self.num
            }
        })*
    };
}

impl_numbered_def!(
    RealmDef,
    ItemDef,
    ItemPowerDef,
    LevelPathDef,
    WizardDef,
    RetortDef
);

/// Turn each unit's ability names into numbers. The names must exist (content validation
/// reports an unknown one); an unknown name is left out here.
pub fn resolve_unit_abilities(units: &mut [UnitDef], abilities: &Registry<AbilityDef>) {
    for u in units {
        u.ability_ids = u
            .abilities
            .iter()
            .filter_map(|a| abilities.id_of(a))
            .collect();
    }
}

/// Every table that holds names of other definitions, for [`resolve_references`].
pub struct Tables<'a> {
    pub races: &'a mut [RaceDef],
    pub units: &'a mut [UnitDef],
    pub abilities: &'a mut [AbilityDef],
    pub buildings: &'a mut [BuildingDef],
    pub realms: &'a mut [RealmDef],
    pub retorts: &'a mut [RetortDef],
    pub spells: &'a mut [SpellDef],
    pub item_powers: &'a mut [ItemPowerDef],
    pub level_paths: &'a mut [LevelPathDef],
    pub wizards: &'a mut [WizardDef],
    pub items: &'a mut [ItemDef],
    pub hero_templates: &'a mut [HeroTemplate],
    pub config: &'a mut RulesConfig,
}

/// Turn the names the tables hold of other definitions into numbers ([`Ref`]), once, when the
/// rules load. A name that is no definition is left unresolved (content validation reports it).
pub fn resolve_references(t: &mut Tables) {
    let mut names = Names::new();
    names.add(t.realms);
    names.add(t.races);
    names.add(t.spells);
    names.add(t.abilities);
    names.add(t.units);
    names.add(t.buildings);
    names.add(t.retorts);
    names.add(t.item_powers);
    names.add(t.level_paths);
    for r in t.realms.iter_mut() {
        names.resolve_all(&mut r.default_spells);
    }
    for r in t.races.iter_mut() {
        names.resolve_all(&mut r.units);
        names.resolve_all(&mut r.buildings);
        resolve_rules(&mut r.effects, &names);
    }
    for u in t.units.iter_mut() {
        names.resolve_opt(&mut u.realm);
        names.resolve_opt(&mut u.race);
        names.resolve_opt(&mut u.level_path);
    }
    for a in t.abilities.iter_mut() {
        resolve_rules(&mut a.effects, &names);
    }
    for b in t.buildings.iter_mut() {
        names.resolve_all(&mut b.races);
        resolve_rules(&mut b.effects, &names);
    }
    for s in t.spells.iter_mut() {
        names.resolve(&mut s.realm);
        names.resolve_all(&mut s.not_with);
        names.resolve_all(&mut s.already_able);
        if let Some(save) = &mut s.save {
            names.resolve_all(&mut save.immune);
        }
        resolve_rules(&mut s.effects, &names);
    }
    for p in t.item_powers.iter_mut() {
        names.resolve_opt(&mut p.realm);
        names.resolve_opt(&mut p.spell);
        names.resolve_opt(&mut p.attack);
    }
    for p in t.level_paths.iter_mut() {
        for l in p.levels.iter_mut() {
            resolve_rules(&mut l.effects, &names);
        }
        for l in p.bonus.iter_mut() {
            resolve_rules(&mut l.effects, &names);
        }
    }
    for r in t.retorts.iter_mut() {
        for need in r.requires.iter_mut() {
            if let RetortNeed::Books(realm, _) = need {
                names.resolve(realm);
            }
        }
    }
    for w in t.wizards.iter_mut() {
        for (realm, _) in w.books.iter_mut() {
            names.resolve(realm);
        }
        names.resolve_all(&mut w.retorts);
    }
    for i in t.items.iter_mut() {
        names.resolve_all(&mut i.powers);
        if let Some((spell, _)) = &mut i.charges {
            names.resolve(spell);
        }
        resolve_rules(&mut i.effects, &names);
    }
    for h in t.hero_templates.iter_mut() {
        names.resolve_all(&mut h.spells);
        names.resolve_all(&mut h.abilities);
    }
    names.resolve_all(&mut t.config.autobuild_order);
}

/// Turn the unit each hero template names into a number. A name that is not a unit is left
/// `None` (content validation reports it).
pub fn resolve_hero_units(templates: &mut [HeroTemplate], units: &[UnitDef]) {
    for t in templates {
        t.unit_id = units
            .iter()
            .find(|u| u.id == t.unit)
            .map(|u| super::UnitId::new(u.num));
    }
}

/// Turn the unit a summoning spell names into a number. A name that is not a unit is left
/// `None` (content validation reports it).
pub fn resolve_summons(spells: &mut [SpellDef], units: &[UnitDef]) {
    for rule in spells.iter_mut().flat_map(|s| s.effects.iter_mut()) {
        if let super::effects::Effect::Summon { unit, unit_id, .. } = &mut rule.effect {
            *unit_id = units
                .iter()
                .find(|u| u.id == *unit)
                .map(|u| super::UnitId::new(u.num));
        }
    }
}

/// Turn the building names in the building and unit tables into numbers. A name that is not
/// a building is left out (content validation reports it).
pub fn resolve_building_links(buildings: &mut [BuildingDef], units: &mut [UnitDef]) {
    let nums: std::collections::HashMap<String, u16> =
        buildings.iter().map(|b| (b.id.clone(), b.num)).collect();
    let id = |name: &String| nums.get(name).map(|&n| BuildingId::new(n));
    for b in buildings.iter_mut() {
        b.require_ids = b.requires.iter().filter_map(id).collect();
        b.replace_id = b.replaces.as_ref().and_then(id);
    }
    for u in units {
        u.require_ids = u.requires.iter().filter_map(id).collect();
    }
}

impl Def for UnitDef {
    fn id(&self) -> &str {
        &self.id
    }

    fn num(&self) -> u16 {
        self.num
    }
}

impl Def for RaceDef {
    fn id(&self) -> &str {
        &self.id
    }

    fn num(&self) -> u16 {
        self.num
    }
}

impl Def for AbilityDef {
    fn id(&self) -> &str {
        &self.id
    }

    fn num(&self) -> u16 {
        self.num
    }
}

impl BuildingDef {
    /// Whether a city of `race` may build it (a building with no race list is for every race).
    pub fn buildable_by(&self, race: super::RaceId) -> bool {
        self.races.is_empty() || self.races.iter().any(|r| r.id() == Some(race))
    }
}

impl Def for BuildingDef {
    fn id(&self) -> &str {
        &self.id
    }

    fn num(&self) -> u16 {
        self.num
    }
}

impl Def for SpellDef {
    fn id(&self) -> &str {
        &self.id
    }

    fn num(&self) -> u16 {
        self.num
    }
}

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
    pub retorts: Registry<RetortDef>,
    /// The starting record of each hero type, in the order of the hero units (optional).
    pub hero_templates: Vec<HeroTemplate>,
    /// Names for rival wizards when no ready-made wizards are loaded: a list for each rival slot.
    pub wizard_names: Vec<Vec<(String, crate::text::Gender)>>,
    /// The guard and treasure rules of the sites (`lairs.ron`).
    pub lairs: super::lair::LairRules,
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
