//! Players: each one is an entity with a [`PlayerId`] and, until task 317 spreads its data over
//! components, a [`Player`] holding what is not moved yet.

pub mod finance;
pub mod offer;
pub mod shortfall;
pub mod upkeep;
pub mod view;

use serde::{Deserialize, Serialize};

/// Who a player is outside the world (fog, diplomacy, map owners, saves, commands). A component of
/// the player entity; never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerId(pub u8);

/// On the player: the wizard entity it plays as. Absent on the monsters' owner and on an
/// independent city's player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WizardLink(pub hecs::Entity);

/// On a unit, hero or city: the player entity it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner(pub hecs::Entity);

/// The [`PlayerId`] of a player entity.
pub fn id_of(world: &hecs::World, player: hecs::Entity) -> Option<PlayerId> {
    world.get::<&PlayerId>(player).ok().map(|id| *id)
}

/// On a player that is no wizard: the monsters' owner and each independent city's own player (task
/// 31). It has no wizard, no magic, no victory, no fog and no AI of a wizard's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Neutral;

/// The id of the one neutral player that owns the monsters of lairs and nodes. Its units last
/// until someone kills them (the lair guards are made for a fight and removed after it; undead
/// raised for the monsters stay).
pub const MONSTERS: PlayerId = PlayerId(u8::MAX);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    pub gold: i32,
    pub mana: i32,
    /// Index into the tax table in the rules config.
    pub tax_rate: u8,
    pub fame: i32,
    pub magic: crate::wizard::magic::Magic,
    /// The wizard's items that are not on a hero: the vault and the award list.
    pub armory: crate::item::armory::Armory,
    /// The wizard's record of each hero type: abilities rolled at the start, whether it can be
    /// hired, is serving, dead or gone, and the experience it keeps.
    pub roster: crate::hero::roster::Roster,
    /// What this turn's merchant, mercenaries and hero asked of the wizard and is waiting for
    /// an answer (task 220); they lapse at the next turn change.
    pub offers: Vec<crate::player::offer::Offer>,
    /// Where the wizard's fortress stands (a city); `None` before it is placed or while
    /// banished.
    pub fortress: Option<crate::map::Position>,
    /// Where summoned creatures appear; defaults to the fortress.
    pub summon_at: Option<crate::map::Position>,
    /// The fortress fell: the wizard is casting the Spell of Return.
    pub banished: bool,
    /// Who took the fortress while the wizard is banished (the one a resignation is to).
    pub banished_by: Option<PlayerId>,
    /// The wizard has lost everything.
    pub defeated: bool,
    /// Who took the wizard's last city.
    pub defeated_by: Option<PlayerId>,
    /// The Grand Vizier chooses what this player's cities build next.
    pub grand_vizier: bool,
    /// The computer wizards' difficulty bonuses (percents of what a human would get). Set from
    /// the game's settings (`Game::apply_wizard_settings`), not saved.
    pub bonus: Bonus,
    /// The wizard's power at each turn, for the History screen (task 71); the last 288 turns.
    pub history: Vec<u8>,
}

/// What a wizard's cities and army are multiplied by, in percent (100 changes nothing). The
/// original gives these to computer wizards by difficulty (`difficulty_modifiers_table`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bonus {
    pub growth: i32,
    pub outpost_growth: i32,
    pub production: i32,
    pub gold: i32,
    pub mana: i32,
    pub food: i32,
    pub research: i32,
    /// Building maintenance.
    pub maintenance: i32,
    /// Gold upkeep of units.
    pub unit_upkeep: i32,
}

impl Default for Bonus {
    fn default() -> Self {
        Self {
            growth: 100,
            outpost_growth: 100,
            production: 100,
            gold: 100,
            mana: 100,
            food: 100,
            research: 100,
            maintenance: 100,
            unit_upkeep: 100,
        }
    }
}

impl Bonus {
    /// `value` as a percent of itself.
    pub fn of(percent: i32, value: i32) -> i32 {
        if percent == 100 {
            value
        } else {
            value * percent / 100
        }
    }
}

/// The banner of the neutral player and of monsters.
pub const NEUTRAL_BANNER: u8 = 5;

/// The tax rate everyone starts with: a gold for each citizen.
pub const START_TAX_RATE: u8 = 2;

/// How many wizard pictures there are (`wizards.lbx`, `lilwiz.lbx` and `moodwiz.lbx`).
pub const PORTRAITS: u8 = 14;

/// How many banners wizards can choose from.
pub const BANNER_CHOICES: u8 = 5;

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Player {
    pub fn new() -> Self {
        Self {
            gold: 100,
            mana: 0,
            tax_rate: START_TAX_RATE,
            fame: 0,
            magic: crate::wizard::magic::Magic::default(),
            armory: crate::item::armory::Armory::default(),
            roster: crate::hero::roster::Roster::default(),
            offers: Vec::new(),
            fortress: None,
            summon_at: None,
            banished: false,
            banished_by: None,
            defeated: false,
            defeated_by: None,
            grand_vizier: false,
            bonus: Bonus::default(),
            history: Vec::new(),
        }
    }
}

/// A read handle on one player's data.
pub type PlayerRef<'a> = hecs::Ref<'a, Player>;

impl<'a> PlayerHandle<'a> {
    /// The wizard's magic.
    pub fn into_magic(self) -> MagicRef<'a> {
        match self {
            Self::Plain(p) => MagicRef::Plain(&p.magic),
            Self::Held(p) => MagicRef::Held(hecs::Ref::map(p, |p| &p.magic)),
        }
    }
}

/// A wizard's magic from a game or from a test double of one (`WorldView`, `IncomeView`).
pub enum MagicRef<'a> {
    Plain(&'a crate::wizard::magic::Magic),
    Held(hecs::Ref<'a, crate::wizard::magic::Magic>),
}

impl std::ops::Deref for MagicRef<'_> {
    type Target = crate::wizard::magic::Magic;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Plain(m) => m,
            Self::Held(m) => m,
        }
    }
}

/// A player's data from a game or from a test double of one (`WorldView`).
pub enum PlayerHandle<'a> {
    Plain(&'a Player),
    Held(PlayerRef<'a>),
}

impl std::ops::Deref for PlayerHandle<'_> {
    type Target = Player;
    fn deref(&self) -> &Player {
        match self {
            Self::Plain(p) => p,
            Self::Held(p) => p,
        }
    }
}

/// The player entity with this id (a scan; `Game` keeps an index for the same question).
pub fn find_entity(world: &hecs::World, id: PlayerId) -> Option<hecs::Entity> {
    world
        .query::<&PlayerId>()
        .iter()
        .find(|(_, &p)| p == id)
        .map(|(e, _)| e)
}

/// A player's magic.
pub fn magic(
    world: &hecs::World,
    id: PlayerId,
) -> Option<hecs::Ref<'_, crate::wizard::magic::Magic>> {
    find(world, id).map(|p| hecs::Ref::map(p, |p| &p.magic))
}

/// The data of the player with this id.
pub fn find(world: &hecs::World, id: PlayerId) -> Option<PlayerRef<'_>> {
    world.get::<&Player>(find_entity(world, id)?).ok()
}

/// Every player with its id, ascending by id.
pub fn all(world: &hecs::World) -> Vec<(PlayerId, PlayerRef<'_>)> {
    ids(world)
        .into_iter()
        .filter_map(|id| Some((id, find(world, id)?)))
        .collect()
}

/// Every player id, ascending (ids are handed out in the order players are made).
pub fn ids(world: &hecs::World) -> Vec<PlayerId> {
    let mut ids: Vec<PlayerId> = world
        .query::<(&PlayerId, &Player)>()
        .iter()
        .map(|(_, (&p, _))| p)
        .collect();
    ids.sort();
    ids
}

/// The data of the player with this id, to change.
pub fn find_mut(world: &mut hecs::World, id: PlayerId) -> Option<&mut Player> {
    let e = find_entity(world, id)?;
    world.query_one_mut::<&mut Player>(e).ok()
}
