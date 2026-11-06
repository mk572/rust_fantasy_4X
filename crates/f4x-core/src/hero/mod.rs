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
    use crate::rules::stats::Stat;
    use crate::rules::Ability;
    use crate::unit::Experience;
    use f4x_def::testdir::TestResult;

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
}
