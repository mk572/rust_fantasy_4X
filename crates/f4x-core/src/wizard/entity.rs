//! The wizard entity: what the creation screen makes (name, picture, banner) and the race of the
//! first city. Each wizard and its player are made together (`Game::add_player`) and point at each
//! other with [`PlayerLink`] and [`crate::player::WizardLink`]. A player without a wizard (the
//! monsters' owner, an independent city's player) has no [`WizardLink`](crate::player::WizardLink).

use crate::player::{PlayerId, BANNER_CHOICES, PORTRAITS};
use crate::rules::RaceId;
use crate::wizard::sheet::{
    self, Books, Candidates, Fame, KnownSpells, PowerSplit, Research, Retorts, Skill,
};

/// The wizard's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name(pub String);

/// Which of the original's fourteen wizard pictures (0 to 13) shows the wizard: the portrait of
/// the creation screen, the gem on the magic screen and the diplomacy screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Portrait(pub u8);

/// The banner colour: 0 blue, 1 green, 2 purple, 3 red, 4 yellow. Flags, cities and units are
/// drawn in it. The neutral banner ([`crate::player::NEUTRAL_BANNER`]) belongs to no wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Banner(pub u8);

/// Race of the first city; some races dislike or like each other's cities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapitalRace(pub Option<RaceId>);

/// On the wizard: the player entity it plays as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerLink(pub hecs::Entity);

/// The components of a new wizard.
pub(crate) type WizardBundle = (
    Name,
    Portrait,
    Banner,
    CapitalRace,
    PlayerLink,
    Books,
    Retorts,
    Skill,
    PowerSplit,
    Research,
    KnownSpells,
    Candidates,
    Fame,
);

/// The components a new wizard starts with; banner and picture default to the player's place.
/// The sheet (books, retorts, skill and the rest) is empty until [`Game::init_wizard`] fills it.
///
/// [`Game::init_wizard`]: crate::game::Game::init_wizard
pub(crate) fn bundle(id: PlayerId, name: String, player: hecs::Entity) -> WizardBundle {
    let (books, retorts, skill, split, research, spells, candidates, fame) = sheet::bundle();
    (
        Name(name),
        Portrait(id.0 % PORTRAITS),
        Banner(id.0 % BANNER_CHOICES),
        CapitalRace(None),
        PlayerLink(player),
        books,
        retorts,
        skill,
        split,
        research,
        spells,
        candidates,
        fame,
    )
}

/// A read handle on one wizard.
#[derive(Clone, Copy)]
pub struct Wizard<'a> {
    world: &'a hecs::World,
    entity: hecs::Entity,
}

impl<'a> Wizard<'a> {
    pub(crate) fn new(world: &'a hecs::World, entity: hecs::Entity) -> Self {
        Self { world, entity }
    }

    pub fn entity(&self) -> hecs::Entity {
        self.entity
    }

    pub(crate) fn world(&self) -> &'a hecs::World {
        self.world
    }

    /// A component of the wizard.
    pub(crate) fn component<T: hecs::Component>(&self) -> Option<hecs::Ref<'a, T>> {
        self.world.get::<&T>(self.entity).ok()
    }

    pub fn name(&self) -> String {
        self.world
            .get::<&Name>(self.entity)
            .map(|n| n.0.clone())
            .unwrap_or_default()
    }

    pub fn gender(&self) -> crate::text::Gender {
        self.world
            .get::<&crate::text::Gender>(self.entity)
            .map_or_else(|_| crate::text::Gender::default(), |g| *g)
    }

    pub fn portrait(&self) -> u8 {
        self.world.get::<&Portrait>(self.entity).map_or(0, |p| p.0)
    }

    pub fn banner(&self) -> u8 {
        self.world
            .get::<&Banner>(self.entity)
            .map_or(crate::player::NEUTRAL_BANNER, |b| b.0)
    }

    pub fn capital_race(&self) -> Option<RaceId> {
        self.world.get::<&CapitalRace>(self.entity).ok()?.0
    }

    /// The cast the wizard is paying for over turns.
    pub fn cast_instance(&self) -> Option<hecs::Entity> {
        crate::spell::instance::Spells::new(self.world).cast_of(self.entity)
    }

    /// The lasting spells the wizard keeps going on the whole world, oldest first, with the
    /// creation number of each.
    pub fn global_instances(&self) -> Vec<(crate::rules::SpellId, crate::game::serial::Serial)> {
        let spells = crate::spell::instance::Spells::new(self.world);
        spells
            .globals_of(self.entity)
            .into_iter()
            .filter_map(|i| {
                let serial = *self.world.get::<&crate::game::serial::Serial>(i).ok()?;
                Some((spells.def(i)?, serial))
            })
            .collect()
    }

    /// The player this wizard plays as.
    pub fn player(&self) -> Option<hecs::Entity> {
        Some(self.world.get::<&PlayerLink>(self.entity).ok()?.0)
    }
}

/// The wizard entity of a player.
pub fn of_player(world: &hecs::World, player: hecs::Entity) -> Option<hecs::Entity> {
    Some(world.get::<&crate::player::WizardLink>(player).ok()?.0)
}

/// The wizard of the player with this id, if it has one.
pub fn find(world: &hecs::World, id: PlayerId) -> Option<Wizard<'_>> {
    let player = crate::player::find_entity(world, id)?;
    Some(Wizard::new(world, of_player(world, player)?))
}

/// How a player is called: the wizard's name, or for an independent city's player the name of
/// its (oldest) city.
pub fn player_name(world: &hecs::World, id: PlayerId) -> String {
    if let Some(w) = find(world, id) {
        return w.name();
    }
    world
        .query::<(
            &crate::city::City,
            &crate::player::Owner,
            &crate::game::serial::Serial,
        )>()
        .iter()
        .filter(|(_, (_, o, _))| crate::player::id_of(world, o.0) == Some(id))
        .min_by_key(|(_, (_, _, s))| s.0)
        .map(|(_, (c, _, _))| c.name.clone())
        .unwrap_or_default()
}

/// Every wizard, in the order of the players.
pub fn all(world: &hecs::World) -> Vec<Wizard<'_>> {
    crate::player::ids(world)
        .into_iter()
        .filter_map(|id| find(world, id))
        .collect()
}
