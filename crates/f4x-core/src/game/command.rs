//! The commands: the one way a player (or the AI, or a test) changes the game.

use crate::{battle, city, diplomacy, game, item, map, player, rules, spell, unit};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    EndTurn,
    /// Create a unit (summoning, production and scenario setup go through this).
    CreateUnit {
        owner: player::PlayerId,
        pos: map::Position,
        unit: rules::UnitId,
    },
    /// Move a stack one tile.
    MoveStack {
        #[serde(with = "game::replay::stable_ref::many")]
        units: Vec<hecs::Entity>,
        to: map::Position,
    },
    /// Give a stack a standing order to walk to a tile.
    GoTo {
        #[serde(with = "game::replay::stable_ref::many")]
        units: Vec<hecs::Entity>,
        to: map::Position,
    },
    /// Cancel standing orders.
    ClearOrders {
        #[serde(with = "game::replay::stable_ref::many")]
        units: Vec<hecs::Entity>,
    },
    /// Put units on standby (Done or Patrol), or take them off it with `None`.
    SetStandby {
        #[serde(with = "game::replay::stable_ref::many")]
        units: Vec<hecs::Entity>,
        standby: Option<unit::Standby>,
    },
    /// Begin a battle between two stacks of units.
    BeginBattle {
        #[serde(with = "game::replay::stable_ref::many")]
        attackers: Vec<hecs::Entity>,
        #[serde(with = "game::replay::stable_ref::many")]
        defenders: Vec<hecs::Entity>,
    },
    /// An order in the battle being fought.
    Battle(battle::BattleCommand),
    /// Fight a battle between two stacks to the end without the tactical map.
    AutoResolve {
        #[serde(with = "game::replay::stable_ref::many")]
        attackers: Vec<hecs::Entity>,
        #[serde(with = "game::replay::stable_ref::many")]
        defenders: Vec<hecs::Entity>,
    },
    /// A stack next to a guarded site (tower, node or encounter site) assaults its guards.
    EnterSite {
        #[serde(with = "game::replay::stable_ref::many")]
        units: Vec<hecs::Entity>,
        to: map::Position,
    },
    /// A stack with a purifying unit cleanses the corrupted square it stands on.
    Purify {
        #[serde(with = "game::replay::stable_ref::many")]
        units: Vec<hecs::Entity>,
    },
    /// Engineers build a road along `path` (starting where they stand).
    BuildRoad {
        #[serde(with = "game::replay::stable_ref::many")]
        units: Vec<hecs::Entity>,
        path: Vec<map::Position>,
    },
    /// Meld a spirit of the stack into the magic node it stands on.
    Meld {
        #[serde(with = "game::replay::stable_ref::many")]
        units: Vec<hecs::Entity>,
    },
    /// Move a stack to the same tile on the other plane.
    ShiftPlane {
        #[serde(with = "game::replay::stable_ref::many")]
        units: Vec<hecs::Entity>,
    },
    /// Teleport a stack from a city with an Earth Gate to another such city of the same wizard.
    EarthGate {
        #[serde(with = "game::replay::stable_ref::many")]
        units: Vec<hecs::Entity>,
        to: map::Position,
    },
    /// Found a city. No population makes an outpost; race decides what it can build.
    FoundCity {
        owner: player::PlayerId,
        pos: map::Position,
        name: String,
        race: Option<rules::RaceId>,
        population: u32,
    },
    /// The unit sheet's Dismiss: a unit leaves its owner's service for good (not heroes yet).
    DismissUnit {
        #[serde(with = "game::replay::stable_ref::one")]
        unit: hecs::Entity,
    },
    /// The unit sheet: turn off one of the wizard's own lasting spells on a unit. A unit that
    /// then stands at sea without flight or water walking drowns.
    TurnOffUnitSpell {
        player: player::PlayerId,
        #[serde(with = "game::replay::stable_ref::one")]
        unit: hecs::Entity,
        spell: rules::SpellId,
    },
    /// The item screen: move an item between the vault, the award list and a hero's slots.
    /// `pay` agrees to the mana a transfer to a far-off hero costs.
    MoveItem {
        player: player::PlayerId,
        from: item::armory::ItemSpot,
        to: item::armory::ItemSpot,
        pay: bool,
    },
    /// Enchant Item or Create Artifact: design an item and start casting it.
    CraftItem {
        player: player::PlayerId,
        craft: item::crafting::Craft,
        design: item::crafting::ItemDesign,
    },
    /// The anvil: destroy a vault or award item for half its value in mana.
    ScrapItem {
        player: player::PlayerId,
        from: item::armory::Spot,
    },
    /// Take one of the offers made at the turn change (the merchant's item, mercenaries or a
    /// hero for hire); `name` renames a hero before it joins.
    AcceptOffer {
        player: player::PlayerId,
        index: usize,
        name: Option<String>,
    },
    /// Turn one of the offers down.
    RejectOffer {
        player: player::PlayerId,
        index: usize,
    },
    /// A settler founds an outpost where it stands and is used up.
    SettleOutpost {
        #[serde(with = "game::replay::stable_ref::one")]
        unit: hecs::Entity,
        name: String,
    },
    SetProduction {
        #[serde(with = "game::replay::stable_ref::one")]
        city: hecs::Entity,
        item: city::production::ProductionItem,
    },
    /// Pay gold to finish the current item at the next turn change.
    BuyProduction {
        #[serde(with = "game::replay::stable_ref::one")]
        city: hecs::Entity,
    },
    SetFarmers {
        #[serde(with = "game::replay::stable_ref::one")]
        city: hecs::Entity,
        farmers: u32,
    },
    /// Found the wizard's first city with the fortress and summoning circle in it.
    PlaceFortress {
        player: player::PlayerId,
        pos: map::Position,
        name: String,
        race: Option<rules::RaceId>,
        population: u32,
    },
    /// Give a city an auto-builder plan, or switch it off with `None` (docs/auto-builder.md).
    SetAutoBuild {
        #[serde(with = "game::replay::stable_ref::one")]
        city: hecs::Entity,
        plan: Option<city::autobuild::BuildPlan>,
    },
    /// A banished wizard gives up (the "Do you wish to resign?" of the original).
    Resign {
        player: player::PlayerId,
    },
    SellBuilding {
        #[serde(with = "game::replay::stable_ref::one")]
        city: hecs::Entity,
        building: rules::BuildingId,
    },
    /// Give a city a new name (the box that names a new outpost). The name is trimmed and cut to
    /// twelve characters; an empty one changes nothing.
    RenameCity {
        #[serde(with = "game::replay::stable_ref::one")]
        city: hecs::Entity,
        name: String,
    },
    /// Raze a city just taken, instead of keeping it (until the end of the turn).
    RazeCity {
        player: player::PlayerId,
        #[serde(with = "game::replay::stable_ref::one")]
        city: hecs::Entity,
    },
    SetTaxRate {
        player: player::PlayerId,
        rate: u8,
    },
    /// Let the Grand Vizier choose what the player's cities build (`city::production::autobuild`).
    SetGrandVizier {
        player: player::PlayerId,
        on: bool,
    },
    /// Split the power base between mana, research and casting skill (percent, sum 100).
    SetPowerSplit {
        player: player::PlayerId,
        mana: u8,
        research: u8,
        skill: u8,
    },
    /// Give up lasting spells and summoned creatures now, chosen from `Game::upkeep_shortfall`
    /// before the turn change would drop the newest ones.
    ReleaseUpkeep {
        player: player::PlayerId,
        items: Vec<spell::cast::UpkeepKind>,
    },
    /// Let these units go for lack of gold or food (chosen from `Game::treasury_shortfall`),
    /// and destroy the listed items for mana first to make room for a released hero's items.
    ReleaseTroops {
        player: player::PlayerId,
        #[serde(with = "game::replay::stable_ref::many")]
        units: Vec<hecs::Entity>,
        scrap: Vec<item::armory::ItemSpot>,
    },
    SetResearch {
        player: player::PlayerId,
        spell: rules::SpellId,
    },
    /// Start casting a spell overland; the target is needed only if it is paid in full now.
    CastSpell {
        player: player::PlayerId,
        spell: rules::SpellId,
        target: Option<rules::exec::Target>,
    },
    /// Aim a spell that has been paid for over several turns.
    ResolveCast {
        player: player::PlayerId,
        target: rules::exec::Target,
    },
    CancelCast {
        player: player::PlayerId,
    },
    /// Turn off one of the wizard's own lasting spells that is on the wizard (a global
    /// enchantment): it ends and stops costing upkeep.
    TurnOffSpell {
        player: player::PlayerId,
        spell: rules::SpellId,
    },
    /// Turn off one of the wizard's own spells on a city.
    TurnOffCitySpell {
        player: player::PlayerId,
        #[serde(with = "game::replay::stable_ref::one")]
        city: hecs::Entity,
        spell: rules::SpellId,
    },
    /// Trade gold and mana (two for one, one for one with the alchemy retort).
    Alchemy {
        player: player::PlayerId,
        amount: i32,
        gold_to_mana: bool,
    },
    /// A talk with another wizard: proposals, answers, gifts, treaties and war.
    Diplomacy(diplomacy::DiplomacyCommand),
}
