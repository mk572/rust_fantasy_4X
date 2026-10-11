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
    /// Which of the original's fourteen wizard pictures (0 to 13) shows the wizard: the
    /// portrait of the creation screen, the gem on the magic screen and the diplomacy screen.
    pub portrait: u8,
    /// The Grand Vizier chooses what this player's cities build next.
    pub grand_vizier: bool,
    /// An independent city's own player (task 31): it has a treasury and pays its way like a
    /// wizard, but is no wizard (no magic, no victory, no AI).
    pub neutral: bool,
}

/// The banner of the neutral player and of monsters.
pub const NEUTRAL_BANNER: u8 = 5;

/// The tax rate everyone starts with: a gold for each citizen.
pub const START_TAX_RATE: u8 = 2;

/// How many wizard pictures there are (`wizards.lbx`, `lilwiz.lbx` and `moodwiz.lbx`).
pub const PORTRAITS: u8 = 14;

/// How many banners wizards can choose from.
pub const BANNER_CHOICES: u8 = 5;

impl Player {
    pub fn new(id: PlayerId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            gold: 100,
            mana: 0,
            tax_rate: START_TAX_RATE,
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
            portrait: if id.0 == u8::MAX { 0 } else { id.0 % PORTRAITS },
            grand_vizier: false,
            neutral: false,
        }
    }

    /// The player of one neutral city: builds by the Grand Vizier's order, brown banner.
    pub fn new_neutral(id: PlayerId, name: impl Into<String>) -> Self {
        Self {
            banner: NEUTRAL_BANNER,
            grand_vizier: true,
            neutral: true,
            ..Self::new(id, name)
        }
    }
}
