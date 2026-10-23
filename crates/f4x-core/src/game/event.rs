//! What the game reports back from a command, and why it can refuse one.

use crate::hero::HeroError;
use crate::{
    battle, city, diplomacy, feature, game, item, map, player, rules, unit, wizard, world_event,
};

/// Why a unit left its owner's army at the turn change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossCause {
    /// Deserted for lack of food.
    Food,
    /// Deserted for lack of gold.
    Gold,
    /// A summoned creature dispelled for lack of mana.
    Mana,
}

/// What a wizard could not pay for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortage {
    Mana,
    Gold,
    Food,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A wizard's reserve could not pay the upkeep, so something had to go; announced to the
    /// wizards in contact with them.
    Shortage {
        player: player::PlayerId,
        what: Shortage,
    },
    /// A ship let go for lack of gold reached the shore and put its passengers on land.
    ShipLanded {
        player: player::PlayerId,
        units: u32,
        at: map::Position,
    },
    /// A hero was let go for lack of gold or food; the hero is free to hire again.
    HeroLeft {
        player: player::PlayerId,
        hero: rules::UnitId,
    },
    TurnStarted {
        turn: u32,
    },
    CityFounded {
        city: hecs::Entity,
        pos: map::Position,
    },
    ProductionCompleted {
        city: hecs::Entity,
        item: city::production::ProductionItem,
    },
    /// A Chaos Rift struck the city: units it destroyed, and a building that fell.
    ChaosRiftStrikes {
        city: hecs::Entity,
        units_lost: u32,
        building: Option<rules::BuildingId>,
    },
    CityGrew {
        city: hecs::Entity,
        figures: u32,
    },
    CityShrank {
        city: hecs::Entity,
        figures: u32,
    },
    /// One unit let go at the turn change, for the Chancellor's report (task 272); the
    /// `UnitsDisbanded` of the same turn counts them.
    UnitLost {
        player: player::PlayerId,
        unit: rules::UnitId,
        cause: LossCause,
    },
    /// Units dismissed because their owner could not feed or pay them.
    UnitsDisbanded {
        player: player::PlayerId,
        units: u32,
    },
    FortressPlaced {
        player: player::PlayerId,
        pos: map::Position,
    },
    FortressMoved {
        player: player::PlayerId,
        pos: map::Position,
    },
    SummoningCircleMoved {
        player: player::PlayerId,
        pos: map::Position,
    },
    /// The fortress city fell; the wizard now casts the Spell of Return.
    WizardBanished {
        player: player::PlayerId,
        by: player::PlayerId,
    },
    WizardReturned {
        player: player::PlayerId,
        pos: map::Position,
    },
    /// `winner` takes up to two spells from the wizard it banished or defeated (`Conquest_Spells`);
    /// `spells` is empty when the loser knew none that the winner lacked.
    ConquestSpells {
        winner: player::PlayerId,
        loser: player::PlayerId,
        spells: Vec<rules::SpellId>,
    },
    /// The wizard lost their last city.
    WizardDefeated {
        player: player::PlayerId,
    },
    /// The game is over: `winner` won, by conquest or by the Spell of Mastery.
    GameOver {
        winner: player::PlayerId,
        kind: game::endgame::EndKind,
        turn: u32,
    },
    CityCaptured {
        city: hecs::Entity,
        from: player::PlayerId,
        to: player::PlayerId,
    },
    CityDestroyed {
        pos: map::Position,
    },
    /// A city fell: what its taker looted, and the citizens and buildings the fighting cost it.
    CityLooted {
        city: hecs::Entity,
        gold: i32,
        citizens_lost: u32,
        /// The buildings the fighting destroyed.
        buildings_lost: Vec<rules::BuildingId>,
        /// The fame the old owner lost and the taker gained.
        fame_lost: i32,
        fame_gained: i32,
    },
    /// The taker razed the city they had just won.
    CityRazed {
        player: player::PlayerId,
        gold: i32,
        /// The fame it cost the one who razed it.
        fame_lost: i32,
    },
    VolcanoRaised {
        player: player::PlayerId,
        pos: map::Position,
    },
    /// Buildings lost when a volcano rose under a city.
    BuildingsCollapsed {
        city: hecs::Entity,
        count: u32,
    },
    TerrainChanged {
        pos: map::Position,
    },
    SpellLearned {
        player: player::PlayerId,
        spell: rules::SpellId,
    },
    SpellCast {
        player: player::PlayerId,
        spell: rules::SpellId,
    },
    /// A dispel took `removed` enchantments off its target.
    Dispelled {
        player: player::PlayerId,
        removed: u32,
    },
    /// A dispel removed `victim`'s enchantment `spell` from a unit or a city.
    EnchantmentDispelled {
        victim: player::PlayerId,
        spell: rules::SpellId,
        on_unit: bool,
    },
    /// A dispel against the global enchantment `spell` failed.
    DispelFailed {
        player: player::PlayerId,
        spell: rules::SpellId,
    },
    /// Disjunction ended `victim`'s global enchantment.
    GlobalSpellRemoved {
        player: player::PlayerId,
        victim: player::PlayerId,
        spell: rules::SpellId,
    },
    /// Spell Binding took `victim`'s global enchantment.
    GlobalSpellTaken {
        player: player::PlayerId,
        victim: player::PlayerId,
        spell: rules::SpellId,
    },
    /// Spell Blast ended the spell `victim` was casting, for `mana` of the caster's own.
    SpellBlasted {
        player: player::PlayerId,
        victim: player::PlayerId,
        spell: rules::SpellId,
        mana: i32,
    },
    /// Drain Power took `amount` mana from `victim`.
    PowerDrained {
        player: player::PlayerId,
        victim: player::PlayerId,
        amount: i32,
    },
    /// Cruel Unminding took `amount` points of casting skill from `victim` for good.
    SkillDestroyed {
        player: player::PlayerId,
        victim: player::PlayerId,
        amount: i32,
    },
    /// A spell that strikes units across the map (Great Unsummoning, Death Wish) slew `count`.
    UnitsSlain {
        player: player::PlayerId,
        spell: rules::SpellId,
        count: u32,
    },
    /// A unit became another kind of unit (Lycanthropy).
    UnitTransformed {
        unit: hecs::Entity,
        from: rules::UnitId,
        to: rules::UnitId,
    },
    /// An Earthquake shook `city`.
    EarthquakeStruck {
        city: hecs::Entity,
        units_lost: u32,
        buildings_lost: u32,
    },
    /// A wizard has started casting the Spell of Mastery (`Game::mastery_casters`): every
    /// wizard is told (the original shows "<name> has started casting the Spell of Mastery").
    MasteryStarted {
        caster: player::PlayerId,
    },
    /// A unit broke free of Stasis.
    StasisEnded {
        unit: hecs::Entity,
    },
    /// A unit was mutated by chaos (Chaos Channels or Doom Mastery).
    UnitMutated {
        unit: hecs::Entity,
        mutation: unit::mutation::Mutation,
    },
    /// A world-scale spell struck: Meteor Storm (no victim), Call the Void on a city, or
    /// Nature's Wrath on the wizard (`victim`) who cast a death or chaos spell.
    WorldSpellStruck {
        spell: rules::SpellId,
        victim: Option<player::PlayerId>,
        units_lost: u32,
        buildings_lost: u32,
        population_lost: u32,
    },
    /// Another wizard's Suppress Magic stopped the spell.
    SpellSuppressed {
        player: player::PlayerId,
        spell: rules::SpellId,
        by: player::PlayerId,
    },
    /// A spell aimed at a city was turned away by the city's nightshade.
    SpellFizzled {
        player: player::PlayerId,
        spell: rules::SpellId,
        city: hecs::Entity,
    },
    /// The spell being cast has nothing to aim at (no rival global enchantment for Disjunction,
    /// say). It is not paid for at the turn change; its wizard may stop casting it.
    NoTargetForCast {
        player: player::PlayerId,
        spell: rules::SpellId,
    },
    /// A spell has been paid for and waits for a target.
    SpellReady {
        player: player::PlayerId,
        spell: rules::SpellId,
    },
    /// The spell was cast but part of what it does is not implemented.
    /// An item a wizard made is ready: where it went.
    ItemMade {
        player: player::PlayerId,
        name: String,
        placed: item::armory::Placed,
    },
    /// Items fell to a wizard from heroes that were slain; `lost` did not fit the armory.
    ItemsAwarded {
        player: player::PlayerId,
        items: Vec<String>,
        lost: u32,
    },
    SpellHasNoEffectYet {
        spell: rules::SpellId,
    },
    /// A hero joined the army at the summoning circle (`kind` is its roster index).
    HeroJoined {
        player: player::PlayerId,
        unit: hecs::Entity,
        kind: usize,
    },
    /// The turn change made the wizard an offer (`Game::offers` lists the ones waiting).
    OfferMade {
        player: player::PlayerId,
        offer: player::offer::Offer,
    },
    /// A company of mercenaries joined the army at the summoning circle.
    MercenariesHired {
        player: player::PlayerId,
        units: Vec<hecs::Entity>,
    },
    /// A summoning spell was cast and found no hero to bring.
    HeroSpellFailed {
        player: player::PlayerId,
        spell: rules::SpellId,
    },
    /// A lasting spell ended because its upkeep could not be paid.
    EnchantmentLapsed {
        player: player::PlayerId,
        spell: rules::SpellId,
    },
    BattleStarted {
        attacker: player::PlayerId,
        defender: player::PlayerId,
        site: map::Position,
    },
    /// Something happened in the battle being fought.
    Battle(battle::BattleEvent),
    /// Units of a side that fled a battle were lost on the way.
    UnitsLostFleeing {
        player: player::PlayerId,
        units: u32,
    },
    /// Units drained to death in a battle rose again as undead for the winner.
    UndeadRaised {
        player: player::PlayerId,
        units: u32,
    },
    /// What a won battle gave the winner: experience for each of its surviving units, and fame.
    BattleSpoils {
        player: player::PlayerId,
        experience: u32,
        fame: i32,
        /// The fame the loser lost (nothing or less than nothing): a hero that fell.
        loser_fame: i32,
    },
    BattleEnded {
        winner: player::PlayerId,
        loser: player::PlayerId,
    },
    LandCorrupted {
        pos: map::Position,
    },
    LandPurified {
        pos: map::Position,
    },
    NodeWarped {
        pos: map::Position,
    },
    RoadBuilt {
        pos: map::Position,
    },
    RoadsEnchanted {
        pos: map::Position,
    },
    /// Land units lost at sea without a ship.
    UnitsDrowned {
        player: player::PlayerId,
        units: u32,
    },
    NodeMelded {
        player: player::PlayerId,
        pos: map::Position,
    },
    /// A spirit died against a guardian: the node stays as it was.
    MeldFailed {
        player: player::PlayerId,
        pos: map::Position,
    },
    /// A random event happened or ended, and the player is told (task 68).
    RandomEvent(world_event::EventNotice),
    UnitCreated {
        unit: hecs::Entity,
        pos: map::Position,
    },
    UnitsMoved {
        units: Vec<hecs::Entity>,
        from: map::Position,
        to: map::Position,
    },
    /// Moving into hostile units or a hostile city: the attack is for the combat layer.
    AttackRequested {
        attackers: Vec<hecs::Entity>,
        from: map::Position,
        to: map::Position,
        defenders: Vec<hecs::Entity>,
    },
    /// Moving onto a guarded site: its guards must be fought first (`Command::EnterSite`).
    SiteEncounter {
        units: Vec<hecs::Entity>,
        from: map::Position,
        to: map::Position,
        lair: hecs::Entity,
    },
    /// A site was cleared and its treasure taken.
    SiteCleared {
        player: player::PlayerId,
        pos: map::Position,
        /// The site's feature id (`ruins`, `chaos_node`, `tower`).
        site: String,
        /// The site had guards, which were fought; an unguarded site is taken on arrival.
        guarded: bool,
        loot: feature::lair::Loot,
    },
    /// Something happened between wizards: a meeting, a change of relations, a treaty, war.
    Diplomacy(diplomacy::DiplomacyEvent),
    Rejected(Rejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    Diplomacy(diplomacy::DiplomacyError),
    Site(feature::lair::SiteError),
    FoundCity(city::FoundCityError),
    Move(unit::movement::MoveError),
    Production(city::production::ProductionError),
    Magic(wizard::magic::MagicError),
    Fortress(wizard::fortress::FortressError),
    Meld(feature::node::MeldError),
    Road(map::road::RoadError),
    Purify(map::corruption::PurifyError),
    BattleStart(battle::game::BattleStartError),
    Raze(city::conquest::RazeError),
    Battle(battle::BattleError),
    Item(item::armory::ItemError),
    Craft(item::crafting::CraftingError),
    Hero(HeroError),
    Offer(player::offer::OfferError),
    /// The game has ended; nothing more can be done.
    GameOver,
}

impl Rejection {
    /// The key of the sentence that tells why the command was refused (task 348): the client, the
    /// log and the console read this one source.
    pub fn key(self) -> &'static str {
        match self {
            Self::Diplomacy(e) => e.key(),
            Self::Site(e) => e.key(),
            Self::FoundCity(e) => e.key(),
            Self::Move(e) => e.key(),
            Self::Production(e) => e.key(),
            Self::Magic(e) => e.key(),
            Self::Fortress(e) => e.key(),
            Self::Meld(e) => e.key(),
            Self::Road(e) => e.key(),
            Self::Purify(e) => e.key(),
            Self::BattleStart(e) => e.key(),
            Self::Raze(e) => e.key(),
            Self::Battle(e) => e.key(),
            Self::Item(e) => e.key(),
            Self::Craft(e) => e.key(),
            Self::Hero(e) => e.key(),
            Self::Offer(e) => e.key(),
            Self::GameOver => "refuse_map.game_over",
        }
    }
}
