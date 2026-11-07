//! The effects the engine reads from the spells on a unit, and the spells that change one unit.
//! A spell carries them in the `unit_effect` hook (`Custom(name: "unit_effect", args: ["bless"])`),
//! so the engine asks for an effect and never for a spell number; a spell may list several.
//!
//! Tests: below, on the shipped content.

use crate::rules::SpellDef;

/// The hook of the spells that act on one unit in a way the stat modifiers and granted abilities
/// cannot say.
pub const UNIT_EFFECT: &str = "unit_effect";

crate::hook::effect_enum! {
    /// What a spell does to or for a unit: the arguments of the `unit_effect` hook.
    pub enum UnitEffect in UNIT_EFFECT;
    // Read from the lasting spells on a unit.
    ResistElements = "resist_elements",
    ElementalArmor = "elemental_armor",
    ResistMagic = "resist_magic",
    SpellLock = "spell_lock",
    Bless = "bless",
    Righteousness = "righteousness",
    BlackChannels = "black_channels",
    Berserk = "berserk",
    Invulnerability = "invulnerability",
    BlackSleep = "black_sleep",
    // The unit cannot get away from a battle it fled (Black Sleep, Confusion, Web).
    Holds = "holds",
    Web = "web",
    // Spells that change one unit (cast once, or lasting with a rule of their own).
    NaturesCures = "natures_cures",
    Lycanthropy = "lycanthropy",
    ChaosChannels = "chaos_channels",
    BlackWind = "black_wind",
    Stasis = "stasis",
    WarpCreature = "warp_creature",
    Possession = "possession",
    CreatureBinding = "creature_binding",
}

impl UnitEffect {
    /// The first of the unit effects the spell's definition gives, if it has the hook (for the
    /// spells that change a unit: they carry one).
    pub fn of(def: &SpellDef) -> Option<UnitEffect> {
        def.hook_args(UNIT_EFFECT)?
            .iter()
            .find_map(|a| UnitEffect::named(a))
    }

    fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

/// A set of unit effects: what the lasting spells on a unit give it, kept beside the spells so the
/// battle can ask without the rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct UnitEffects(u32);

impl UnitEffects {
    pub const NONE: UnitEffects = UnitEffects(0);

    /// The effects the spell gives.
    pub fn of(def: &SpellDef) -> UnitEffects {
        let mut set = UnitEffects::NONE;
        for name in def.hook_args(UNIT_EFFECT).unwrap_or_default() {
            if let Some(e) = UnitEffect::named(name) {
                set.insert(e);
            }
        }
        set
    }

    /// Whether the set has no effect.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every effect of `other` is in this set.
    pub fn covers(self, other: UnitEffects) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, effect: UnitEffect) {
        self.0 |= effect.bit();
    }

    pub fn union(&mut self, other: UnitEffects) {
        self.0 |= other.0;
    }

    pub fn contains(self, effect: UnitEffect) -> bool {
        self.0 & effect.bit() != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::shipped;

    #[test]
    fn every_unit_effect_is_given_by_a_shipped_spell() {
        let c = shipped();
        for &name in UnitEffect::NAMES {
            let e = UnitEffect::named(name).expect("a listed name");
            assert!(
                c.rules.spells.defs().iter().any(|d| e.is_in(d)),
                "no spell gives the unit effect {name}"
            );
        }
    }

    #[test]
    fn a_set_holds_what_was_put_in() {
        let mut set = UnitEffects::NONE;
        assert!(!set.contains(UnitEffect::Bless));
        set.insert(UnitEffect::Bless);
        assert!(set.contains(UnitEffect::Bless));
        assert!(!set.contains(UnitEffect::Holds));
    }
}
