//! Players (wizards): what each one owns that is not an entity on the map.

use crate::city::PlayerId;
use crate::rules::RaceId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub gold: i32,
    pub mana: i32,
    /// Index into the tax table in the rules config.
    pub tax_rate: u8,
    /// Race of the first city; some races dislike or like each other's cities.
    pub capital_race: Option<RaceId>,
    pub fame: i32,
    pub magic: crate::magic::Magic,
    /// Where the wizard's fortress stands (a city); `None` before it is placed or while
    /// banished.
    pub fortress: Option<crate::city::Position>,
    /// Where summoned creatures appear; defaults to the fortress.
    pub summon_at: Option<crate::city::Position>,
    /// The fortress fell: the wizard is casting the Spell of Return.
    pub banished: bool,
    /// The wizard has lost everything.
    pub defeated: bool,
    /// Who took the wizard's last city.
    pub defeated_by: Option<PlayerId>,
    /// The banner colour: 0 blue, 1 green, 2 purple, 3 red, 4 yellow (5, brown, is the
    /// neutral player's). Flags, cities and units are drawn in it.
    pub banner: u8,
}

/// The banner of the neutral player and of monsters.
pub const NEUTRAL_BANNER: u8 = 5;

/// How many banners wizards can choose from.
pub const BANNER_CHOICES: u8 = 5;

impl Player {
    pub fn new(id: PlayerId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            gold: 100,
            mana: 0,
            tax_rate: 1,
            capital_race: None,
            fame: 0,
            magic: crate::magic::Magic::default(),
            fortress: None,
            summon_at: None,
            banished: false,
            defeated: false,
            defeated_by: None,
            banner: if id.0 == u8::MAX {
                NEUTRAL_BANNER
            } else {
                id.0 % BANNER_CHOICES
            },
        }
    }
}
