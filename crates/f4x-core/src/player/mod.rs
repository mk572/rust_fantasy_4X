//! Players: each one is an entity with a [`PlayerId`] and the components below.
//!
//! Tests: none, because it only declares the player components.

pub mod entity;
pub mod finance;
pub mod offer;
pub mod race_player;
pub mod shortfall;
pub mod upkeep;
pub mod view;

use f4x_def::half_int::HalfInt;

pub use f4x_def::id::PlayerId;

/// On the player: the wizard entity it plays as. Absent on the monsters' owner and on an
/// independent city's player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WizardLink(pub hecs::Entity);

/// On a unit, hero or city: the player entity it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner(pub hecs::Entity);

/// The [`PlayerId`] of a player entity.
pub fn id_of(ecs: &hecs::World, player: hecs::Entity) -> Option<PlayerId> {
    ecs.get::<&PlayerId>(player).ok().map(|id| *id)
}

/// On a player that is no wizard: the monsters' owner and each independent city's own player. It has no wizard, no magic, no victory, no fog and no AI of a wizard's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Neutral;

pub use f4x_def::id::MONSTERS;

/// On a player: its gold and mana.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Treasury {
    pub gold: HalfInt,
    pub mana: HalfInt,
}

impl Treasury {
    /// Set the whole points of mana, keeping a half point that is there (a battle counts whole
    /// mana only).
    pub fn set_whole_mana(&mut self, whole: i32) {
        let half = self.mana - HalfInt::from_int(self.mana.floor());
        self.mana = HalfInt::from_int(whole) + half;
    }

    /// Take half the gold (rounded down) out of the treasury and return it.
    pub fn take_half_gold(&mut self) -> HalfInt {
        let half = self.gold / 2;
        self.gold -= half;
        half
    }

    /// Take half the mana (rounded down) out of the treasury and return it.
    pub fn take_half_mana(&mut self) -> HalfInt {
        let half = self.mana / 2;
        self.mana -= half;
        half
    }
}

impl Default for Treasury {
    fn default() -> Self {
        Self {
            gold: HalfInt::from_int(100),
            mana: HalfInt::ZERO,
        }
    }
}

/// On a player: the index into the tax table in the rules config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tax(pub u8);

/// On a player: the wizard's power at each turn, for the History screen; the last 288
/// turns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History(pub Vec<u8>);

/// On a player whose Grand Vizier chooses what its cities build next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrandVizier;

/// On a player: what this turn's merchant, mercenaries and hero asked of the wizard and is waiting
/// for an answer; they lapse at the next turn change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Offers(pub Vec<crate::player::offer::Offer>);

/// On a player that has lost everything: who took its last city.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Defeat {
    pub by: Option<PlayerId>,
}

/// Who decides for a player: the human at the screen or the AI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Human,
    Ai,
}

/// On a player: what its cities and army are multiplied by, in percent (100 changes nothing). The
/// original gives these to computer wizards by difficulty (`difficulty_modifiers_table`). Set from
/// the game's settings (`World::apply_wizard_settings`), not saved.
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
    pub fn of_half(percent: i32, value: HalfInt) -> HalfInt {
        if percent == 100 {
            value
        } else {
            value.percent(percent)
        }
    }

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
pub const NEUTRAL_BANNER: u8 = 10;

/// How many wizard pictures there are (`wizards.lbx`, `lilwiz.lbx` and `moodwiz.lbx`).
pub const PORTRAITS: u8 = 14;

/// How many banners wizards can choose from.
pub const BANNER_CHOICES: u8 = 10;

/// A new player: the components every player starts with. A wizard's player gets its `Control`
/// and `WizardLink` when the game adds it.
///
/// `tax` is the rate everyone starts with (`start_tax_rate` of `wizard` in `rules.ron`).
pub fn bundle(id: PlayerId, tax: u8) -> (PlayerId, Treasury, Tax, Bonus, History, Offers) {
    (
        id,
        Treasury::default(),
        Tax(tax),
        Bonus::default(),
        History::default(),
        Offers::default(),
    )
}

/// A new player for a test world with no game: the start tax rate of the shipped rules.
#[cfg(test)]
pub fn test_bundle(id: PlayerId) -> (PlayerId, Treasury, Tax, Bonus, History, Offers) {
    bundle(
        id,
        f4x_def::testdir::shipped()
            .rules
            .config
            .wizard
            .start_tax_rate,
    )
}

/// The player entity with this id. The world has no index of its own (`World` keeps one), so this
/// remembers the last answer per id on the thread and checks it against the world: an entity that
/// still carries the id is the player (an id is made once), anything else is a scan. A wrong or
/// stale memory costs a scan, never a wrong answer.
pub fn find_entity(ecs: &hecs::World, id: PlayerId) -> Option<hecs::Entity> {
    thread_local! {
        static SEEN: std::cell::RefCell<std::collections::HashMap<PlayerId, hecs::Entity>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
    }
    let seen = SEEN.with(|s| s.borrow().get(&id).copied());
    if let Some(e) = seen.filter(|&e| ecs.get::<&PlayerId>(e).is_ok_and(|p| *p == id)) {
        return Some(e);
    }
    let found = ecs
        .query::<&PlayerId>()
        .iter()
        .find(|(_, &p)| p == id)
        .map(|(e, _)| e);
    if let Some(e) = found {
        SEEN.with(|s| s.borrow_mut().insert(id, e));
    }
    found
}

/// A component of the player with this id.
pub fn component<C: hecs::Component>(ecs: &hecs::World, id: PlayerId) -> Option<hecs::Ref<'_, C>> {
    ecs.get::<&C>(find_entity(ecs, id)?).ok()
}

/// Every player id but the monsters' owner, ascending (ids are handed out in the order players
/// are made).
pub fn ids(ecs: &hecs::World) -> Vec<PlayerId> {
    let mut ids: Vec<PlayerId> = ecs
        .query::<&PlayerId>()
        .iter()
        .map(|(_, &p)| p)
        .filter(|&p| p != MONSTERS)
        .collect();
    ids.sort();
    ids
}
