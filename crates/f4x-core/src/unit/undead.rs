//! Undead units (task 29, `UM_UNDEAD` in the original).
//!
//! A unit can be made undead: raised by the Animate Dead spell in a battle (`battle/spells.rs`),
//! taken by the winner of a battle after a Life Steal or Create Undead attack drained it to
//! death (`battle/game.rs`), or turned by Zombie Mastery (task 87; the `undead` console cheat and
//! [`Game::make_undead`] do it for now). An undead unit:
//! - is a creature of the Death realm and counts as fantastic in battle;
//! - has the immunities of the Death creatures: death, cold, poison and illusion. The original
//!   forgot the last three for raised units (`COMBINIT.c`, "OGBUG"); the help gives them, and
//!   so does the game (`help-vs-implementation-conflicts.md` B10);
//! - is not healed: not in the field or a city, not by a healer, Stream of Life or Herb
//!   Mastery, and not by the Healing spell. (Death creatures that are born so, such as
//!   skeletons and ghouls, are not healed either.)
//! - is harmed more by Dispel Evil (an extra -5, so -9) and by Holy Word (an extra -5; the
//!   original's code applies it the wrong way round, the help is followed);
//! - costs no gold and eats no food; a fantastic unit made undead pays half as much mana again;
//! - gets nothing from Holy Arms or Crusade, which are for the living normal units.
//!
//! A unit under Black Channels is undead in the help's words ("whether Animated or Black
//! Channeled"): it fights as a Death creature with the same immunities, cannot be healed and
//! suffers Holy Word and Dispel Evil as the undead do. It is not a unit made undead for the
//! upkeep (it keeps its gold and food cost, as in the original) or Holy Arms and Crusade.

use crate::game::Game;
use crate::unit::{Unit, UnitView};
use hecs::World;

/// Marks a unit that was made undead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Undead;

/// The realm of the death creatures.
pub const DEATH: &str = "death";

/// The spell that makes a unit a vile undead being (a lasting enchantment on a normal unit).
pub const BLACK_CHANNELS: &str = "black_channels";

/// What an undead unit is immune to, besides what its kind already is.
pub const UNDEAD_IMMUNITIES: [&str; 4] = [
    "death_immunity",
    "cold_immunity",
    "poison_immunity",
    "illusion_immunity",
];

impl UnitView<'_> {
    /// Whether the unit was made undead (not merely black channeled).
    pub fn unit_is_undead(&self, unit: hecs::Entity) -> bool {
        self.world.get::<&Undead>(unit).is_ok()
    }

    /// Whether the Black Channels spell is on the unit.
    pub fn unit_is_black_channeled(&self, unit: hecs::Entity) -> bool {
        let Some(spell) = self.content.rules.spells.id_of(BLACK_CHANNELS) else {
            return false;
        };
        crate::spell::instance::Spells::new(self.world).unit_has(unit, spell)
    }

    /// Undead in the sense of the help: made undead or black channeled. Such a unit fights as
    /// a Death creature and cannot be healed.
    pub fn unit_counts_as_undead(&self, unit: hecs::Entity) -> bool {
        self.unit_is_undead(unit) || self.unit_is_black_channeled(unit)
    }

    /// The realm of the unit's creature: Death for an undead or black channeled unit, else its
    /// own.
    pub fn unit_realm(&self, unit: hecs::Entity) -> Option<String> {
        if self.unit_counts_as_undead(unit) {
            return Some(DEATH.to_string());
        }
        if self.unit_is_mutated(unit) {
            return Some("chaos".to_string());
        }
        self.unit_def(unit).and_then(|d| d.realm.clone())
    }

    /// A creature of the Death realm: born so, made undead or black channeled.
    pub fn unit_is_death(&self, unit: hecs::Entity) -> bool {
        self.unit_realm(unit).as_deref() == Some(DEATH)
    }
}

/// Make a unit undead. Its lasting spells fall away, as they do for the dead raised in a
/// battle. Returns false for a unit that is gone.
pub(crate) fn make_undead(world: &mut World, unit: hecs::Entity) -> bool {
    if world.get::<&Unit>(unit).is_err() {
        return false;
    }
    let _ = world.insert_one(unit, Undead);
    crate::spell::instance::remove_on(
        world,
        crate::spell::instance::SpellTarget::Unit(unit),
        |_, _| true,
    );
    true
}

impl Game {
    /// Make a unit undead (see [`make_undead`]).
    pub fn make_undead(&mut self, unit: hecs::Entity) -> bool {
        make_undead(&mut self.world, unit)
    }
}
