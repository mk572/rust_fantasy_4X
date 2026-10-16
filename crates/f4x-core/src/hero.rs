//! Heroes (task 219): what a hero unit carries and the abilities it was rolled with.
//!
//! What the abilities do in play is task 47; this file only says what they are, so the roster,
//! the save and `unit_abilities` read one type. A hero's three items are the separate
//! [`HeroItems`](crate::items::HeroItems) component.

use serde::{Deserialize, Serialize};

/// How much of an ability a hero has: the plain one or the doubled ("super") one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum Tier {
    #[default]
    None,
    Normal,
    Super,
}

/// The abilities one hero type has in one wizard's game (the original's `HSA_*` flags): rolled
/// when the game starts and copied to the unit when the hero is hired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HeroAbilities {
    pub leadership: Tier,
    pub legendary: Tier,
    pub blademaster: Tier,
    pub armsmaster: Tier,
    pub constitution: Tier,
    pub might: Tier,
    pub arcane_power: Tier,
    pub prayermaster: Tier,
    pub agility: Tier,
    pub sage: Tier,
    pub lucky: bool,
    pub charmed: bool,
    pub noble: bool,
    pub female: bool,
}

/// The ability names with a plain and a doubled form, as `heroes.ron` spells them.
const TIERED: [&str; 10] = [
    "leadership",
    "legendary",
    "blademaster",
    "armsmaster",
    "constitution",
    "might",
    "arcane_power",
    "prayermaster",
    "agility",
    "sage",
];

impl HeroAbilities {
    /// The abilities named in `names` (the spellings of [`HERO_ABILITIES`](crate::rules::HERO_ABILITIES));
    /// a name that is not one is ignored (the data checks reject them).
    pub fn from_names<S: AsRef<str>>(names: &[S]) -> Self {
        let mut out = Self::default();
        for name in names {
            let name = name.as_ref();
            let (base, tier) = match name.strip_prefix("super_") {
                Some(base) => (base, Tier::Super),
                None => (name, Tier::Normal),
            };
            if let Some(slot) = out.tier_mut(base) {
                *slot = tier;
                continue;
            }
            match name {
                "lucky" => out.lucky = true,
                "charmed" => out.charmed = true,
                "noble" => out.noble = true,
                "female" => out.female = true,
                _ => {}
            }
        }
        out
    }

    /// The tier of one of the ten abilities that have one, by its plain name (`None` for a
    /// name that is not one of them).
    pub fn tier(&self, name: &str) -> Option<Tier> {
        let mut copy = *self;
        copy.tier_mut(name).map(|t| *t)
    }

    /// The same, to change.
    pub fn tier_mut(&mut self, name: &str) -> Option<&mut Tier> {
        Some(match name {
            "leadership" => &mut self.leadership,
            "legendary" => &mut self.legendary,
            "blademaster" => &mut self.blademaster,
            "armsmaster" => &mut self.armsmaster,
            "constitution" => &mut self.constitution,
            "might" => &mut self.might,
            "arcane_power" => &mut self.arcane_power,
            "prayermaster" => &mut self.prayermaster,
            "agility" => &mut self.agility,
            "sage" => &mut self.sage,
            _ => return None,
        })
    }

    /// The ability ids the unit has, in the spellings of [`HERO_ABILITIES`](crate::rules::HERO_ABILITIES):
    /// what `unit_abilities` adds to the unit definition's own.
    pub fn ids(&self) -> Vec<String> {
        let mut out = Vec::new();
        for name in TIERED {
            match self.tier(name) {
                Some(Tier::Normal) => out.push(name.to_string()),
                Some(Tier::Super) => out.push(format!("super_{name}")),
                _ => {}
            }
        }
        for (on, name) in [
            (self.lucky, "lucky"),
            (self.charmed, "charmed"),
            (self.noble, "noble"),
            (self.female, "female"),
        ] {
            if on {
                out.push(name.to_string());
            }
        }
        out
    }
}

/// The component of a hero unit: what the roster knew of the hero when it was hired. (Its type
/// is the unit's own `Unit` component; its level comes from its experience.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hero {
    pub name: String,
    pub abilities: HeroAbilities,
    pub casting_skill: u8,
    /// The four innate spells (ids in `spells.ron`), fewer when the hero knows fewer.
    pub spells: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_make_tiers_and_flags() {
        let a = HeroAbilities::from_names(&["leadership", "super_might", "noble", "female"]);
        assert_eq!(a.leadership, Tier::Normal);
        assert_eq!(a.might, Tier::Super);
        assert_eq!(a.sage, Tier::None);
        assert!(a.noble && a.female && !a.lucky);
    }

    #[test]
    fn ids_round_trip_through_the_names() {
        let names = ["super_armsmaster", "legendary", "lucky", "charmed"];
        let a = HeroAbilities::from_names(&names);
        let ids = a.ids();
        assert_eq!(ids.len(), 4);
        for n in names {
            assert!(ids.iter().any(|i| i == n), "{n} missing from {ids:?}");
        }
        assert_eq!(HeroAbilities::from_names(&ids), a);
    }

    #[test]
    fn unknown_names_are_ignored() {
        assert_eq!(
            HeroAbilities::from_names(&["flying", "super_nothing"]),
            HeroAbilities::default()
        );
    }
}
