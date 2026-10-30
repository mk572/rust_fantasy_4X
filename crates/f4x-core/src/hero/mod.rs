//! Heroes (tasks 219 and 326): a hero is an entity of its own that outlives its body. Each wizard
//! has one hero entity for each of the 35 hero types from the start of the game, holding the
//! abilities rolled for it, its name, its experience and its state; while it serves it also
//! carries the unit components (`Unit`, `Position`...), and it loses them when the body dies.
//!
//! What the abilities are, so the roster, the save and `unit_abilities` read one type, and what
//! they do (task 47): one pure function per row of the table in `heroes-items-design.md` 2.3,
//! which `unit/mod.rs` (the hero's own stats), `battle/field.rs` (Leadership, Prayermaster),
//! `unit/mod.rs` (fame) and `wizard/magic_income.rs` (research) call. A hero's three items are the
//! separate [`HeroItems`](crate::item::HeroItems) component.

pub(crate) mod game;
pub mod kind;

use serde::{Deserialize, Serialize};

use crate::rules::stats::{ModOp, Modifier, Source, Stat};

/// Why a hero could not be brought into service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeroError {
    /// The wizard has no record of this hero type (no roster, or no such type).
    Unknown,
    /// The hero is serving already, or gone for good.
    NotAvailable,
    /// The wizard has no summoning circle for the hero to appear in.
    NoCircle,
    /// The wizard cannot pay the hero's price.
    NoGold,
}

impl HeroError {
    /// The key of its sentence in `content/text` (task 348).
    pub fn key(self) -> &'static str {
        match self {
            Self::Unknown => "refuse_wizard.hero_error_unknown",
            Self::NotAvailable => "refuse_wizard.hero_error_not_available",
            Self::NoCircle => "refuse_wizard.hero_error_no_circle",
            Self::NoGold => "refuse_wizard.hero_error_no_gold",
        }
    }
}

/// Where a hero is in its wizard's service (the original keeps this in the hero's level: 0 or
/// more free, `-20` gone, other negatives dead). The entity carries the unit components only
/// while `Serving`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeroState {
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

impl Fate {
    /// The state the hero is in after leaving the army this way.
    pub fn state(self) -> HeroState {
        match self {
            Fate::Slain => HeroState::Dead,
            Fate::Released => HeroState::Free,
            Fate::Gone => HeroState::Gone,
        }
    }
}

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

/// What Charmed adds to a hero's resistance: enough to pass every check. The original adds it
/// only inside battle checks (`Combat_Effective_Resistance`), so the unit sheet never shows it.
pub const CHARMED_RESISTANCE: i32 = 30;

/// `x * n / d` for the plain tier, `x * sn / sd` for the doubled one, 0 without the ability
/// (whole numbers, rounded down, as the original).
fn by_tier(tier: Tier, x: i32, plain: (i32, i32), doubled: (i32, i32)) -> i32 {
    match tier {
        Tier::None => 0,
        Tier::Normal => x * plain.0 / plain.1,
        Tier::Super => x * doubled.0 / doubled.1,
    }
}

/// `level` is the hero's 0-based experience level; the formulas of the table use `x = level + 1`.
/// Leadership: the melee bonus of every normal unit on the battlefield (half of it, rounded down,
/// for their ranged attack); only the best hero's counts.
pub fn leadership(tier: Tier, level: usize) -> i32 {
    by_tier(tier, level as i32 + 1, (1, 3), (1, 2))
}

/// Blademaster: pluses to hit.
pub fn blademaster(tier: Tier, level: usize) -> i32 {
    by_tier(tier, level as i32 + 1, (1, 2), (3, 4))
}

/// Constitution: hit points.
pub fn constitution(tier: Tier, level: usize) -> i32 {
    by_tier(tier, level as i32 + 1, (1, 1), (3, 2))
}

/// Might: melee attack.
pub fn might(tier: Tier, level: usize) -> i32 {
    by_tier(tier, level as i32 + 1, (1, 1), (3, 2))
}

/// Agility: defence.
pub fn agility(tier: Tier, level: usize) -> i32 {
    by_tier(tier, level as i32 + 1, (1, 1), (3, 2))
}

/// Arcane Power: magic ranged attack (only for a hero that has one).
pub fn arcane_power(tier: Tier, level: usize) -> i32 {
    by_tier(tier, level as i32 + 1, (1, 1), (3, 2))
}

/// Prayermaster: resistance of every unit on the battlefield; only the best of this and
/// Resistance to All counts.
pub fn prayermaster(tier: Tier, level: usize) -> i32 {
    by_tier(tier, level as i32 + 1, (1, 1), (3, 2))
}

/// Legendary: fame for the wizard.
pub fn legendary(tier: Tier, level: usize) -> i32 {
    by_tier(tier, level as i32 + 1, (3, 1), (9, 2))
}

/// Sage: research points for the wizard (here `level` itself, not one more).
pub fn sage(tier: Tier, level: usize) -> i32 {
    by_tier(tier, level as i32, (3, 1), (9, 2))
}

impl HeroAbilities {
    /// The changes to the hero's own stats: Might, Agility, Blademaster, Constitution and (with
    /// a magic ranged attack) Arcane Power. Charmed is +30 resistance; Lucky acts in battle.
    pub fn modifiers(&self, level: usize, magic_ranged: bool) -> Vec<Modifier> {
        let add = |stat, value| Modifier::new(stat, ModOp::Add, value, Source::Hero);
        let mut out = vec![
            add(
                Stat::Resistance,
                if self.charmed { CHARMED_RESISTANCE } else { 0 },
            ),
            add(Stat::Melee, might(self.might, level)),
            add(Stat::Defense, agility(self.agility, level)),
            add(Stat::ToHit, blademaster(self.blademaster, level)),
            add(Stat::MaxHp, constitution(self.constitution, level)),
        ];
        if magic_ranged {
            out.push(add(Stat::Ranged, arcane_power(self.arcane_power, level)));
        }
        out.retain(|m| m.value != 0);
        out
    }
}

/// The component of a hero entity: what was rolled for it when the game began. (Its experience is
/// the `Experience` component; its state is [`HeroState`]; while it serves, its unit definition
/// is the `Unit` component.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hero {
    /// The hero type: its index among `Rules::hero_templates`, the one number that names the hero
    /// to its wizard (a wizard has one hero of each type).
    pub kind: usize,
    pub name: String,
    pub abilities: HeroAbilities,
    pub casting_skill: u8,
    /// The four innate spells (ids in `spells.ron`), fewer when the hero knows fewer.
    pub spells: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Game;
    use crate::hero::game::tests::{at, world, P0};
    use crate::unit::Experience;

    #[test]
    fn the_formulas_follow_the_table() {
        // Level 5 (the sixth): x = 6.
        let (n, s) = (Tier::Normal, Tier::Super);
        assert_eq!((leadership(n, 5), leadership(s, 5)), (2, 3));
        assert_eq!((blademaster(n, 5), blademaster(s, 5)), (3, 4));
        assert_eq!((constitution(n, 5), constitution(s, 5)), (6, 9));
        assert_eq!((might(n, 5), might(s, 5)), (6, 9));
        assert_eq!((agility(n, 5), agility(s, 5)), (6, 9));
        assert_eq!((arcane_power(n, 5), arcane_power(s, 5)), (6, 9));
        assert_eq!((prayermaster(n, 5), prayermaster(s, 5)), (6, 9));
        assert_eq!((legendary(n, 5), legendary(s, 5)), (18, 27));
        // Sage goes by the level itself.
        assert_eq!((sage(n, 5), sage(s, 5)), (15, 22));
        assert_eq!(might(Tier::None, 5), 0);
        // Rounded down: x = 2.
        assert_eq!((blademaster(s, 1), might(s, 1), legendary(s, 1)), (1, 3, 9));
    }

    #[test]
    fn arcane_power_needs_a_magic_attack() {
        let a = HeroAbilities::from_names(&["arcane_power", "super_might"]);
        let find = |mods: &[Modifier], stat| mods.iter().find(|m| m.stat == stat).map(|m| m.value);
        assert_eq!(find(&a.modifiers(1, false), Stat::Ranged), None);
        assert_eq!(find(&a.modifiers(1, true), Stat::Ranged), Some(2));
        assert_eq!(find(&a.modifiers(1, true), Stat::Melee), Some(3));
        assert_eq!(find(&a.modifiers(1, true), Stat::Defense), None);
        let charmed = HeroAbilities::from_names(&["charmed"]);
        assert_eq!(
            find(&charmed.modifiers(1, false), Stat::Resistance),
            Some(30)
        );
    }

    #[test]
    fn a_hero_gets_its_abilities_as_stats_fame_and_research() {
        let mut g = world();
        // The Dervish has no stat abilities of its own.
        let kind = crate::hero::game::tests::kind_of(&g, "dervish");
        let h = g.spawn_hero(P0, at(1, 1), kind).unwrap();
        let stats = [Stat::Melee, Stat::Defense, Stat::ToHit, Stat::MaxHp];
        let read = |g: &Game| stats.map(|s| g.units().unit_stat(h, s));
        // Experience 20 is the second level (x = 2); the test path adds its own pluses there, so
        // the ability's part is the difference to the same hero without abilities.
        g.world.insert_one(h, Experience(20)).unwrap();
        let (fame, research) = (g.fame(P0), g.income(P0).research);
        let bare = read(&g);
        g.world.get::<&mut Hero>(h).unwrap().abilities =
            HeroAbilities::from_names(&["might", "super_agility", "blademaster", "constitution"]);
        let with: Vec<i32> = read(&g).iter().zip(bare).map(|(n, b)| n - b).collect();
        assert_eq!(with, vec![2, 3, 1, 2]);
        // Super Legendary is 9 fame at x = 2, Sage 3 research at level 1.
        g.world.get::<&mut Hero>(h).unwrap().abilities =
            HeroAbilities::from_names(&["super_legendary", "sage"]);
        assert_eq!(g.fame(P0), fame + 9);
        assert_eq!(g.income(P0).research, research + 3);
        let other = crate::player::PlayerId(1);
        assert_eq!(g.fame(other), g.wizard(other).unwrap().fame());
    }

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
