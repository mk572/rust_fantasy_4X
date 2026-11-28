//! The hero types: the rules over `Rules::hero_templates` that do not need the game's
//! world: which unit a type is, its level and hire price, how a wizard's heroes are rolled when the
//! game starts (`Init_Heroes`) and which one is offered or summoned (`Pick_Random_Hero`). The
//! heroes themselves are entities (`hero/world.rs`).
//!
//! Tests: `tests/it/c9_hero.rs`.

use crate::hero::{Hero, HeroAbilities, HeroState, Tier};
use crate::rules::Ability;
use crate::rules::{HeroTemplate, PickType, Rules, UnitId};
use crate::world::rng::GameRng;
use f4x_def::names::NameBundle;

/// The hero types before the champions, and the champions after them (`Pick_Random_Hero`):
/// Torin the Chosen is the last type and only Incarnation summons him.
pub const HEROES: usize = 24;
pub const CHAMPIONS: usize = 10;

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

/// The hero type of a unit: the unit's own number, which is also its row among
/// `Rules::hero_templates` (the data check keeps them equal).
pub fn kind_of(rules: &Rules, unit: UnitId) -> Option<usize> {
    let kind = usize::from(unit.num());
    (rules.hero_templates.get(kind)?.unit == unit).then_some(kind)
}

/// The unit definition of a hero type: the unit with that number.
pub fn unit_of(rules: &Rules, kind: usize) -> Option<UnitId> {
    let id = rules.units.id_numbered(u16::try_from(kind).ok()?)?;
    (rules.hero_templates.get(kind)?.unit == id).then_some(id)
}

/// The 0-based experience level `xp` gives a unit on its level path.
pub fn level_of(rules: &Rules, unit: UnitId, xp: u32) -> usize {
    rules.xp_level(rules.units.get(unit), xp)
}

/// The hire price: `cost + level * cost / 4`, `percent` of it (50 for a Charismatic wizard;
/// `Hire_Hero_Popup`). `level` is the 0-based level the kept experience gives.
pub fn hire_cost(cost: i32, level: usize, percent: i32) -> i32 {
    (cost + level as i32 * cost / 4) * percent / 100
}

/// The 35 heroes of the game, as `Init_Heroes` makes them (once for the game, not for each
/// wizard: a hero is one individual that any wizard may hire): the template's fixed abilities, then the template's picks, one
/// `Random(14)` draw at a time. The draws are made in the original's order (hero type by hero
/// type), so a seed gives the same result whatever else the game rolls.
pub fn roll(rng: &mut GameRng, rules: &Rules, names: &NameBundle) -> Vec<Hero> {
    rules
        .hero_templates
        .iter()
        .enumerate()
        .map(|(kind, t)| roll_one(rng, rules, names, kind, t))
        .collect()
}

pub fn roll_one(
    rng: &mut GameRng,
    rules: &Rules,
    names: &NameBundle,
    kind: usize,
    t: &HeroTemplate,
) -> Hero {
    let def = Some(rules.units.get(t.unit));
    let has = |ability: Ability| def.is_some_and(|d| d.abilities.contains(&ability));
    let mut abilities = HeroAbilities::from_ids(&t.abilities);
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
        // Each branch is one case of the switch; a pick that does not apply is simply drawn again.
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
                // Arcane Power adds to a magic ranged attack, so a hero without one skips the draw.
                if abilities.arcane_power != Tier::Super
                    && mage > 0
                    && rules
                        .config
                        .abilities
                        .magic_ranged
                        .iter()
                        .any(|&n| has(Ability::new(n)))
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
                if !abilities.noble {
                    abilities.noble = true;
                    spent = true;
                }
            }
            10 => {
                if !abilities.charmed && !has(Ability::new(40 /* magic_immunity */)) {
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
    let unit = rules.units.get(t.unit);
    let name = names.hero(unit.num, &unit.id);
    Hero {
        kind,
        name,
        abilities,
        casting_skill: skill.clamp(0, i32::from(u8::MAX)) as u8,
        spells: t.spells.clone(),
    }
}

/// What `pick` needs to know of the wizard.
#[derive(Debug, Clone, Copy)]
pub struct Standing {
    pub fame: i32,
    pub life_books: i32,
    pub death_books: i32,
}

/// Whether the hero type `kind` can be offered to the wizard now (`states` is the state of each
/// type, in order): free, within its fame, and
/// the books it needs (the priestess and paladin a life book, the black knight and the
/// necromancer a death book).
pub fn can_offer(rules: &Rules, states: &[HeroState], kind: usize, standing: &Standing) -> bool {
    let (Some(state), Some(unit)) = (states.get(kind), unit_of(rules, kind)) else {
        return false;
    };
    if *state != HeroState::Free {
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
/// (Summon Hero, Summon Champion, a freed prisoner) ignores the fame and picks among all who
/// qualify; an offer tries 10 random types and may find nobody. A champion pick that finds
/// nobody goes on to pick from all.
pub fn pick(
    rng: &mut GameRng,
    rules: &Rules,
    states: &[HeroState],
    standing: &Standing,
    zero_cost: bool,
    class: Class,
) -> Option<usize> {
    // The highest cost is 600, so a fame of 200 lets every hero through.
    let standing = &Standing {
        fame: if zero_cost { 200 } else { standing.fame },
        ..*standing
    };
    if zero_cost {
        let eligible = |class| -> Vec<usize> {
            let range = match class {
                Class::Any => 0..HEROES + CHAMPIONS,
                Class::Hero => 0..HEROES,
                Class::Champion => HEROES..HEROES + CHAMPIONS,
            };
            range
                .filter(|&k| can_offer(rules, states, k, standing))
                .collect()
        };
        let mut found = eligible(class);
        if found.is_empty() && class == Class::Champion {
            found = eligible(Class::Any);
        }
        return (!found.is_empty()).then(|| found[rng.roll(found.len() as u32) as usize - 1]);
    }
    let mut class = class;
    let mut attempts = 0;
    loop {
        // The loop body always runs once, even when the attempts are used up.
        loop {
            attempts += 1;
            let kind = match class {
                Class::Any => rng.roll(34) - 1,
                Class::Hero => rng.roll(HEROES as u32) - 1,
                Class::Champion => HEROES as u32 - 1 + rng.roll(CHAMPIONS as u32),
            } as usize;
            if can_offer(rules, states, kind, standing) {
                return Some(kind);
            }
            if attempts >= 10 {
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
