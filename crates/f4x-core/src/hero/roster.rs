//! A wizard's hero roster (task 219): one record for each of the 35 hero types, kept in every
//! wizard's game, with the abilities rolled when the game started (`Init_Heroes`), whether the
//! hero can be hired, is serving, is dead, or is gone for good, and the experience it keeps
//! from one service to the next.
//!
//! These are plain functions over the records and the rules; the game's side (spawning the
//! unit, removing it, the gold) is in `hero/game.rs`.

use serde::{Deserialize, Serialize};

use crate::game::rng::GameRng;
use crate::hero::{HeroAbilities, Tier};
use crate::rules::{HeroTemplate, PickType, Rules, UnitId};

/// How many heroes a wizard may have at once (`NUM_HERO_SLOTS`).
pub const MAX_HEROES: usize = 6;

/// The hero types before the champions, and the champions after them (`Pick_Random_Hero`):
/// Torin the Chosen is the last type and only Incarnation summons him.
pub const HEROES: usize = 24;
pub const CHAMPIONS: usize = 10;

/// Whether a hero can be hired now (the original keeps this in the hero's level: 0 or more
/// free, `-20` gone, other negatives dead).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Availability {
    /// Not in anybody's service here: offered by `pick`, summoned, or hired.
    Free,
    /// Has a unit in the wizard's army.
    Serving,
    /// Fell in battle: Resurrection brings it back.
    Dead,
    /// Gone for good (Disintegrate, Banish and the like).
    Gone,
}

/// Why a hero left the army.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    /// Killed in a fight or by a spell: Dead, and Resurrection can bring it back.
    Slain,
    /// Dismissed or let go (no gold to pay it): Free again, with its experience.
    Released,
    /// Destroyed so that nothing can bring it back.
    Gone,
}

/// Which heroes `pick` draws from (`hero_type_class`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Any of the 34 (Torin excluded).
    Any,
    /// One of the first 24.
    Hero,
    /// One of the 10 champions; if none is available, any.
    Champion,
}

/// One hero type in one wizard's game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroRecord {
    pub state: Availability,
    pub abilities: HeroAbilities,
    pub casting_skill: u8,
    /// The innate spells (ids in `spells.ron`).
    pub spells: Vec<String>,
    /// The hero's name in this wizard's game; the player renames a hired hero (task 143).
    pub name: String,
    /// Experience points kept while the hero is free or dead.
    pub xp: u32,
}

/// On a player: the records of its wizard's heroes, in the order of `Rules::hero_templates`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Roster(pub Vec<HeroRecord>);

impl Roster {
    pub fn get(&self, kind: usize) -> Option<&HeroRecord> {
        self.0.get(kind)
    }

    pub fn get_mut(&mut self, kind: usize) -> Option<&mut HeroRecord> {
        self.0.get_mut(kind)
    }

    /// The hero types with this state, in order.
    pub fn with_state(&self, state: Availability) -> Vec<usize> {
        (0..self.0.len())
            .filter(|&k| self.0[k].state == state)
            .collect()
    }

    /// Heroes that fell and can be resurrected (`Player_Dead_Hero_Count` counts them).
    pub fn dead(&self) -> Vec<usize> {
        self.with_state(Availability::Dead)
    }

    /// A hero joined the army.
    pub fn on_hired(&mut self, kind: usize) {
        if let Some(r) = self.get_mut(kind) {
            r.state = Availability::Serving;
        }
    }

    /// A hero left the army with `xp` experience points, which it keeps unless it is gone.
    pub fn on_dead(&mut self, kind: usize, xp: u32, fate: Fate) {
        if let Some(r) = self.get_mut(kind) {
            r.xp = xp;
            r.state = match fate {
                Fate::Slain => Availability::Dead,
                Fate::Released => Availability::Free,
                Fate::Gone => Availability::Gone,
            };
        }
    }
}

/// The hero type of a unit: its index among `Rules::hero_templates`.
pub fn kind_of(rules: &Rules, unit: UnitId) -> Option<usize> {
    let id = &rules.units.get(unit).id;
    rules.hero_templates.iter().position(|t| &t.unit == id)
}

/// The unit definition of a hero type.
pub fn unit_of(rules: &Rules, kind: usize) -> Option<UnitId> {
    rules.units.id_of(&rules.hero_templates.get(kind)?.unit)
}

/// The magic ranged attacks, the only ones Arcane Power adds to. (The original's `Init_Heroes`
/// tests the range `rat_Fireball` to `rat_Nat_Bolt`, which leaves the lightning attack out, and
/// lets a hero without any of them draw Arcane Power; we count lightning and skip the rest.)
pub const MAGIC_RANGED: [&str; 7] = [
    "lightning_bolt",
    "fireball_attack",
    "sorcery_bolt",
    "death_bolt",
    "ice_bolt",
    "shaman_bolt",
    "nature_bolt",
];

/// The 0-based experience level `xp` gives a unit on its level path.
pub fn level_of(rules: &Rules, unit: UnitId, xp: u32) -> usize {
    rules
        .units
        .get(unit)
        .level_path
        .as_deref()
        .and_then(|p| rules.level_paths.by_name(p))
        .map_or(0, |path| {
            path.levels.iter().rposition(|l| l.xp <= xp).unwrap_or(0)
        })
}

/// The hire price: `cost + level * cost / 4`, half of it for a Charismatic wizard
/// (`Hire_Hero_Popup`). `level` is the 0-based level the kept experience gives.
pub fn hire_cost(cost: i32, level: usize, charismatic: bool) -> i32 {
    let price = cost + level as i32 * cost / 4;
    if charismatic {
        price / 2
    } else {
        price
    }
}

/// The record of every hero type for the wizard `player` (0-based among the wizards), as
/// `Init_Heroes` makes it: the template's fixed abilities, then the template's picks, one
/// `Random(14)` draw at a time. The draws are made in the original's order (hero type by hero
/// type), so a seed gives the same result whatever else the game rolls.
pub fn roll(rng: &mut GameRng, rules: &Rules, player: usize) -> Roster {
    Roster(
        rules
            .hero_templates
            .iter()
            .map(|t| roll_one(rng, rules, t, player))
            .collect(),
    )
}

fn roll_one(rng: &mut GameRng, rules: &Rules, t: &HeroTemplate, player: usize) -> HeroRecord {
    let def = rules.units.id_of(&t.unit).map(|id| rules.units.get(id));
    let has = |ability: &str| def.is_some_and(|d| d.abilities.iter().any(|a| a == ability));
    let mut abilities = HeroAbilities::from_names(&t.abilities);
    let mut skill = t.casting_skill as i32;
    let picks = t.picks as i32;
    let (mut warrior, mut mage) = match t.pick_type {
        PickType::Warrior => (picks, 0),
        PickType::Mage => (0, picks),
        PickType::Any => (picks, picks),
    };
    // One more or the doubled version; false when it is doubled already.
    fn raise(tier: &mut Tier) -> bool {
        match *tier {
            Tier::Super => false,
            Tier::Normal => {
                *tier = Tier::Super;
                true
            }
            Tier::None => {
                *tier = Tier::Normal;
                true
            }
        }
    }
    while warrior > 0 || mage > 0 {
        // Each branch is one case of the original's switch; a pick that does not apply is
        // simply drawn again.
        let drawn = rng.roll(14) - 1;
        let mut spent = false;
        match drawn {
            0..=5 | 12 | 13 | 7 => {
                // The martial abilities need a warrior pick, prayermaster and sage a mage pick.
                let (tier, martial) = match drawn {
                    0 => (&mut abilities.leadership, true),
                    1 => (&mut abilities.legendary, true),
                    2 => (&mut abilities.armsmaster, true),
                    3 => (&mut abilities.blademaster, true),
                    4 => (&mut abilities.might, true),
                    5 => (&mut abilities.constitution, true),
                    7 => (&mut abilities.prayermaster, false),
                    12 => (&mut abilities.agility, true),
                    _ => (&mut abilities.sage, false),
                };
                let picks_left = if martial { warrior } else { mage };
                if *tier != Tier::Super && picks_left > 0 {
                    raise(tier);
                    spent = true;
                }
            }
            6 => {
                // Arcane Power adds to a magic ranged attack, so a hero without one skips the
                // draw (the original gives it to anyone but the Knight).
                if abilities.arcane_power != Tier::Super
                    && mage > 0
                    && MAGIC_RANGED.iter().any(|a| has(a))
                {
                    raise(&mut abilities.arcane_power);
                    spent = true;
                }
            }
            8 => {
                if mage > 0 {
                    skill += 1;
                    spent = true;
                }
            }
            9 => {
                if !abilities.noble && t.unit != "chosen" {
                    abilities.noble = true;
                    spent = true;
                }
            }
            10 => {
                if !abilities.charmed && !has("magic_immunity") {
                    abilities.charmed = true;
                    spent = true;
                }
            }
            // Lucky, like Noble and Charmed, belongs to neither budget. (The original has no
            // `break` here, so a Lucky draw also tries Agility; we stop at Lucky.)
            11 if !abilities.lucky => {
                abilities.lucky = true;
                spent = true;
            }
            _ => {}
        }
        if spent {
            warrior -= 1;
            mage -= 1;
        }
    }
    let name = t
        .names
        .get(player % t.names.len().max(1))
        .cloned()
        .or_else(|| def.map(|d| d.name.clone()))
        .unwrap_or_else(|| t.unit.clone());
    HeroRecord {
        state: Availability::Free,
        abilities,
        casting_skill: skill.clamp(0, i32::from(u8::MAX)) as u8,
        spells: t.spells.clone(),
        name,
        xp: 0,
    }
}

/// What `pick` needs to know of the wizard.
#[derive(Debug, Clone, Copy)]
pub struct Standing {
    pub fame: i32,
    pub life_books: i32,
    pub death_books: i32,
}

/// Whether the hero type `kind` can be offered to the wizard now: free, within its fame, and
/// the books it needs (the priestess and paladin a life book, the black knight and the
/// necromancer a death book).
pub fn can_offer(rules: &Rules, roster: &Roster, kind: usize, standing: &Standing) -> bool {
    let (Some(record), Some(unit)) = (roster.get(kind), unit_of(rules, kind)) else {
        return false;
    };
    if record.state != Availability::Free {
        return false;
    }
    let def = rules.units.get(unit);
    let id = def.id.as_str();
    if matches!(id, "priestess" | "paladin") && standing.life_books <= 0 {
        return false;
    }
    if matches!(id, "black_knight" | "necromancer") && standing.death_books <= 0 {
        return false;
    }
    (def.cost as i32 - 100) / 10 < standing.fame
}

/// `Pick_Random_Hero`: a random hero type the wizard can be offered, or `None`. `zero_cost`
/// (Summon Hero, Summon Champion, a freed prisoner) ignores the fame and tries up to 100
/// times, otherwise 10. A champion pick that finds nobody goes on to pick from all.
pub fn pick(
    rng: &mut GameRng,
    rules: &Rules,
    roster: &Roster,
    standing: &Standing,
    zero_cost: bool,
    class: Class,
) -> Option<usize> {
    let max_attempts = if zero_cost { 100 } else { 10 };
    // The highest cost is 600, so a fame of 200 lets every hero through.
    let standing = &Standing {
        fame: if zero_cost { 200 } else { standing.fame },
        ..*standing
    };
    let mut class = class;
    let mut attempts = 0;
    loop {
        // The original's loop body always runs once, even when the attempts are used up.
        loop {
            attempts += 1;
            let kind = match class {
                Class::Any => rng.roll(34) - 1,
                Class::Hero => rng.roll(HEROES as u32) - 1,
                Class::Champion => HEROES as u32 - 1 + rng.roll(CHAMPIONS as u32),
            } as usize;
            if can_offer(rules, roster, kind, standing) {
                return Some(kind);
            }
            if attempts >= max_attempts {
                break;
            }
        }
        if class == Class::Champion {
            class = Class::Any;
        } else {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_price_grows_with_the_level_and_halves_for_a_charismatic_wizard() {
        assert_eq!(hire_cost(100, 0, false), 100);
        assert_eq!(hire_cost(100, 4, false), 200);
        assert_eq!(hire_cost(300, 2, false), 450);
        assert_eq!(hire_cost(300, 2, true), 225);
    }

    #[test]
    fn leaving_the_army_sets_the_state_and_keeps_the_experience() {
        let rec = |state| HeroRecord {
            state,
            abilities: HeroAbilities::default(),
            casting_skill: 0,
            spells: Vec::new(),
            name: "H".into(),
            xp: 0,
        };
        let mut r = Roster(vec![rec(Availability::Free); 4]);
        r.on_hired(0);
        r.on_hired(1);
        r.on_hired(2);
        assert_eq!(r.get(0).unwrap().state, Availability::Serving);
        r.on_dead(0, 120, Fate::Slain);
        r.on_dead(1, 80, Fate::Released);
        r.on_dead(2, 60, Fate::Gone);
        assert_eq!(r.dead(), vec![0]);
        assert_eq!(r.get(0).unwrap().xp, 120);
        assert_eq!(r.get(1).unwrap().state, Availability::Free);
        assert_eq!(r.get(1).unwrap().xp, 80);
        assert_eq!(r.get(2).unwrap().state, Availability::Gone);
        assert_eq!(r.with_state(Availability::Free), vec![1, 3]);
    }
}
