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
}

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
        }
    }
}
