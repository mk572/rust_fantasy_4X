//! What a hero's abilities are, so the roster, the save, the data checks and `unit_abilities` read one
//! type, and what they do: one pure function per row of the table in `heroes-items-design.md` 2.3,
//! which the hero's own stats, the battlefield (Leadership, Prayermaster), fame and research call.

use serde::{Deserialize, Serialize};

use crate::half_int::HalfInt;
use crate::rules::stats::{ModOp, Modifier, Source, Stat};
use crate::rules::Ability;

/// How much of an ability a hero has: the plain one or the doubled ("super") one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum Tier {
    #[default]
    None,
    Normal,
    Super,
}

/// The abilities one hero type has in one wizard's game: rolled when the game starts and copied to
/// the unit when the hero is hired.
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
const TIERED: &[&str] = &[
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
    /// The abilities with these numbers (`abilities.ron`); a number that is not a hero
    /// ability is ignored (the data checks reject it, see [`Self::is_hero_ability`]).
    pub fn from_ids(ids: &[Ability]) -> Self {
        let mut out = Self::default();
        for id in ids {
            let n = id.num();
            for &name in TIERED {
                let plain = tiered_number(name);
                if n == plain || n == plain + 1 {
                    if let Some(slot) = out.tier_mut(name) {
                        *slot = if n == plain {
                            Tier::Normal
                        } else {
                            Tier::Super
                        };
                    }
                }
            }
            match n {
                39 => out.lucky = true,
                95 => out.charmed = true,
                96 => out.noble = true,
                97 => out.female = true,
                _ => {}
            }
        }
        out
    }

    /// Whether the number is one of the hero abilities [`Self::from_ids`] understands.
    pub fn is_hero_ability(id: Ability) -> bool {
        Self::from_ids(&[id]).ids() == [id]
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

    /// The abilities the unit has, as numbers (`abilities.ron`): what `unit_abilities` adds to
    /// the unit definition's own.
    pub fn ids(&self) -> Vec<Ability> {
        let mut out = Vec::new();
        for &name in TIERED {
            let plain = tiered_number(name);
            match self.tier(name) {
                Some(Tier::Normal) => out.push(Ability::new(plain)),
                Some(Tier::Super) => out.push(Ability::new(plain + 1)),
                _ => {}
            }
        }
        for (on, number) in [
            (self.lucky, 39),
            (self.charmed, 95),
            (self.noble, 96),
            (self.female, 97),
        ] {
            if on {
                out.push(Ability::new(number));
            }
        }
        out
    }
}

/// The number of a tiered ability's plain form in `abilities.ron`; the doubled form is the next.
pub fn tiered_number(name: &str) -> u16 {
    match name {
        "leadership" => 75,
        "legendary" => 77,
        "blademaster" => 79,
        "armsmaster" => 81,
        "constitution" => 83,
        "might" => 85,
        "arcane_power" => 87,
        "sage" => 89,
        "prayermaster" => 91,
        _ => 93, // agility
    }
}

impl crate::rules::Rules {
    /// What the ability `name` (one of the ten with a doubled form, by its plain name) gives a hero
    /// of `level` (0 for the first) at `tier`: its `per_level` in `abilities.ron`. 0 without the
    /// ability. Whole numbers, rounded down, as the original.
    ///
    /// `leadership`: the melee bonus of every normal unit on the battlefield (half of it, rounded
    /// down, for their ranged attack), only the best hero's counts. `blademaster`: pluses to hit.
    /// `constitution`: hit points. `might`: melee. `agility`: defence. `arcane_power`: magic
    /// ranged attack (only for a hero that has one). `prayermaster`: resistance of every unit on
    /// the battlefield (the best of this and Resistance to All). `armsmaster`: experience each
    /// turn for the other units on the hero's square. `legendary`: fame. `sage`: research.
    pub fn hero_value(&self, name: &str, tier: Tier, level: usize) -> i32 {
        if !TIERED.contains(&name) {
            return 0;
        }
        let number = match tier {
            Tier::None => return 0,
            Tier::Normal => tiered_number(name),
            Tier::Super => tiered_number(name) + 1,
        };
        self.abilities
            .by_num(number)
            .and_then(|a| a.per_level)
            .map_or(0, |f| f.at(level))
    }
}

/// The problems of the hero abilities' `per_level`: each of the ten tiered abilities, plain and
/// doubled, has one with a denominator and no negative number; no other ability has one.
pub fn problems(rules: &crate::rules::Rules) -> Vec<crate::issue::ContentIssue> {
    const F: &str = "abilities.ron";
    let mut p = crate::issue::Issues::new();
    let tiered: Vec<u16> = TIERED
        .iter()
        .flat_map(|&n| [tiered_number(n), tiered_number(n) + 1])
        .collect();
    for a in rules.abilities.defs() {
        let at = format!("ability {:?}", a.id);
        match (tiered.contains(&a.num), a.per_level) {
            (true, None) => p.missing(F, &at, "per_level (a hero ability has one)"),
            (false, Some(_)) => p.conflict(F, &at, "it is no hero ability but has a per_level"),
            (true, Some(f))
                if f.denominator < 1 || f.numerator < 0 || !(0..=1).contains(&f.level_offset) =>
            {
                p.out_of_range(
                    F,
                    &at,
                    "per_level",
                    format!("{}/{} with offset {}", f.numerator, f.denominator, f.level_offset),
                    "a fraction with a denominator of at least 1, a numerator of 0 or more and an offset of 0 or 1",
                )
            }
            _ => {}
        }
    }
    p.into_vec()
}

impl HeroAbilities {
    /// The changes to the hero's own stats: Might, Agility, Blademaster, Constitution and (with
    /// a magic ranged attack) Arcane Power. Charmed is +30 resistance; Lucky acts in battle.
    pub fn modifiers(
        &self,
        f: &crate::rules::Rules,
        level: usize,
        magic_ranged: bool,
    ) -> Vec<Modifier> {
        let add = |stat, value| Modifier::new(stat, ModOp::Add, value, Source::Hero);
        let mut out = vec![
            add(
                Stat::Resistance,
                if self.charmed {
                    f.config.renown.charmed_resistance
                } else {
                    0
                },
            ),
            add(Stat::Melee, f.hero_value("might", self.might, level)),
            add(Stat::Defense, f.hero_value("agility", self.agility, level)),
            add(
                Stat::ToHit,
                f.hero_value("blademaster", self.blademaster, level),
            ),
            add(
                Stat::MaxHp,
                f.hero_value("constitution", self.constitution, level),
            ),
        ];
        if magic_ranged {
            out.push(add(
                Stat::Ranged,
                f.hero_value("arcane_power", self.arcane_power, level),
            ));
        }
        out.retain(|m| m.value != HalfInt::ZERO);
        out
    }
}
