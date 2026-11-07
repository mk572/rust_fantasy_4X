//! The names a game is dealt when it starts: the cities of each race, the heroes and the rival
//! wizards. The client reads its name lists and hands them over as one plain bundle; the
//! rules carry none, and nothing below the client knows a language. A list that is missing
//! falls back to a name made of the definition's alias.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::text::Gender;

/// The name lists a new game draws from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameBundle {
    /// City names by race number, handed out in order.
    pub cities: BTreeMap<u16, Vec<String>>,
    /// Hero names by the hero unit's number; the first is the name the hero takes.
    pub heroes: BTreeMap<u16, Vec<String>>,
    /// Rival wizards: one list for each rival slot, first rival first.
    pub rivals: Vec<Vec<(String, Gender)>>,
}

impl NameBundle {
    /// The `index`th city name of a race (counting from 0, wrapping round its list); with no
    /// list, `<race id>_city_00`, `_01` and so on.
    pub fn city(&self, race_num: u16, race_id: &str, index: usize) -> String {
        match self.cities.get(&race_num).filter(|l| !l.is_empty()) {
            Some(list) => list[index % list.len()].clone(),
            None => format!("{race_id}_city_{index:02}"),
        }
    }

    /// A hero's name: the first of its list, else the unit's alias.
    pub fn hero(&self, unit_num: u16, unit_id: &str) -> String {
        self.heroes
            .get(&unit_num)
            .and_then(|l| l.first())
            .cloned()
            .unwrap_or_else(|| unit_id.to_string())
    }
}
