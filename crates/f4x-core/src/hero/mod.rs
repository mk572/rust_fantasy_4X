//! Heroes: a hero is an entity of its own that outlives its body. Each wizard
//! has one hero entity for each of the 35 hero types from the start of the game, holding the
//! abilities rolled for it, its name, its experience and its state; while it serves it also
//! carries the unit components (`Unit`, `Position`...), and it loses them when the body dies.
//!
//! What the abilities are, so the roster, the save and `unit_abilities` read one type, and what
//! they do: one pure function per row of the table in `heroes-items-design.md` 2.3,
//! which `unit/mod.rs` (the hero's own stats), `battle/field.rs` (Leadership, Prayermaster),
//! `unit/mod.rs` (fame) and `wizard/magic_income.rs` (research) call. A hero's three items are the
//! separate [`HeroItems`](crate::item::HeroItems) component.

pub(crate) mod game;
pub mod kind;

use crate::rules::SpellId;
use serde::{Deserialize, Serialize};

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
    /// The key of its sentence in `content/text`.
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

pub use f4x_def::hero::*;

/// The component of a hero entity: what was rolled for it when the game began. (Its experience is
/// the `Experience` component; its state is [`HeroState`]; while it serves, its unit definition
/// is the `Unit` component.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hero {
    /// The hero type: the number of the hero's unit, which is also its row among
    /// `Rules::hero_templates` (a wizard has one hero of each type).
    pub kind: usize,
    pub name: String,
    pub abilities: HeroAbilities,
    pub casting_skill: u8,
    /// The four innate spells (ids in `spells.ron`), fewer when the hero knows fewer.
    pub spells: Vec<SpellId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Game;
    use crate::hero::game::tests::{at, world, P0};
    use crate::rules::stats::{Modifier, Stat};
    use crate::rules::Ability;
    use crate::unit::Experience;
    use f4x_def::testdir::TestResult;

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
        let a = HeroAbilities::from_ids(&[
            Ability::new(87), /* arcane_power */
            Ability::new(86), /* super_might */
        ]);
        let find = |mods: &[Modifier], stat| mods.iter().find(|m| m.stat == stat).map(|m| m.value);
        assert_eq!(find(&a.modifiers(1, false), Stat::Ranged), None);
        assert_eq!(find(&a.modifiers(1, true), Stat::Ranged), Some(2));
        assert_eq!(find(&a.modifiers(1, true), Stat::Melee), Some(3));
        assert_eq!(find(&a.modifiers(1, true), Stat::Defense), None);
        let charmed = HeroAbilities::from_ids(&[Ability::new(95) /* charmed */]);
        assert_eq!(
            find(&charmed.modifiers(1, false), Stat::Resistance),
            Some(30)
        );
    }

    #[test]
    fn a_hero_gets_its_abilities_as_stats_fame_and_research() -> TestResult {
        let mut g = world();
        // The Dervish has no stat abilities of its own.
        let kind = crate::hero::game::tests::kind_of(&g, 3 /* dervish */);
        let h = g
            .spawn_hero(P0, at(1, 1), kind)
            .map_err(|e| format!("{e:?}"))?;
        let stats = [Stat::Melee, Stat::Defense, Stat::ToHit, Stat::MaxHp];
        let read = |g: &Game| stats.map(|s| g.units().unit_stat(h, s));
        // Experience 20 is the second level (x = 2); the test path adds its own pluses there, so
        // the ability's part is the difference to the same hero without abilities.
        g.world.insert_one(h, Experience(20))?;
        let (fame, research) = (g.fame(P0), g.income(P0).research);
        let bare = read(&g);
        g.world.get::<&mut Hero>(h)?.abilities = HeroAbilities::from_ids(&[
            Ability::new(85), // might
            Ability::new(94), // super_agility
            Ability::new(79), // blademaster
            Ability::new(83), // constitution
        ]);
        let with: Vec<i32> = read(&g).iter().zip(bare).map(|(n, b)| n - b).collect();
        assert_eq!(with, vec![2, 3, 1, 2]);
        // Super Legendary is 9 fame at x = 2, Sage 3 research at level 1.
        g.world.get::<&mut Hero>(h)?.abilities = HeroAbilities::from_ids(&[
            Ability::new(78), /* super_legendary */
            Ability::new(89), /* sage */
        ]);
        assert_eq!(g.fame(P0), fame + 9);
        assert_eq!(g.income(P0).research, research + 3);
        let other = crate::player::PlayerId(1);
        assert_eq!(
            g.fame(other),
            g.wizard(other).ok_or("wizard is missing")?.fame()
        );
        Ok(())
    }

    #[test]
    fn numbers_make_tiers_and_flags() {
        let a = HeroAbilities::from_ids(&[
            Ability::new(75), // leadership
            Ability::new(86), // super_might
            Ability::new(96), // noble
            Ability::new(97), // female
        ]);
        assert_eq!(a.leadership, Tier::Normal);
        assert_eq!(a.might, Tier::Super);
        assert_eq!(a.sage, Tier::None);
        assert!(a.noble && a.female && !a.lucky);
    }

    #[test]
    fn ids_round_trip() {
        // super armsmaster, legendary, lucky, charmed
        let numbers = [82, 77, 39, 95];
        let given: Vec<Ability> = numbers.iter().map(|&n| Ability::new(n)).collect();
        let a = HeroAbilities::from_ids(&given);
        let ids = a.ids();
        assert_eq!(ids.len(), 4);
        for n in numbers {
            assert!(ids.contains(&Ability::new(n)), "{n} missing from {ids:?}");
        }
        assert_eq!(HeroAbilities::from_ids(&ids), a);
    }

    #[test]
    fn other_abilities_are_ignored() {
        // Flying (a unit ability) and a number no ability has.
        let other = [Ability::new(1), Ability::new(9999)];
        assert_eq!(HeroAbilities::from_ids(&other), HeroAbilities::default());
        assert!(!HeroAbilities::is_hero_ability(other[0]));
        assert!(HeroAbilities::is_hero_ability(Ability::new(97)));
    }
}
