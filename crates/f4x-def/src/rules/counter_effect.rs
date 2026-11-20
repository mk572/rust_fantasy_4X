//! What a spell with the `counter_effect` hook does against other spells.
//!
//! Tests: none, because it only names the hook's arguments (the spell tests in `f4x-core` read them).

use super::SpellDef;

/// The hook of the spells that act against other spells. The first argument is what it does; a
/// second, a number, is its strength.
pub const COUNTER_EFFECT: &str = "counter_effect";

crate::effect_enum! {
    /// What a spell with the `counter_effect` hook does against other spells: the first argument
    /// of the hook (`Custom(name: "counter_effect", args: ["dispel", "10"])`).
    pub enum CounterEffect in COUNTER_EFFECT;
    // Take off other wizards' enchantments from the unit or square aimed at: the number is the
    // strength (the mana put in adds to it in proportion to the cost).
    Dispel = "dispel",
    // Attack a rival's global enchantment: the number multiplies the mana (Disjunction True: 3).
    Disjunction = "disjunction",
    // As Disjunction, but a success takes the enchantment over.
    SpellBinding = "spell_binding",
    // Raise a reserve in a battle that stops the enemy's spells.
    CounterMagic = "counter_magic",
}

impl CounterEffect {
    /// What the spell does and its strength, if it has the hook.
    pub fn of(def: &SpellDef) -> Option<(CounterEffect, u32)> {
        let args = def.hook_args(COUNTER_EFFECT)?;
        let effect = CounterEffect::named(args.first()?)?;
        let number = args.iter().find_map(|a| a.parse().ok()).unwrap_or(0);
        Some((effect, number))
    }

    /// Whether extra mana put into the spell overland makes it stronger.
    pub fn takes_extra_mana(self) -> bool {
        !matches!(self, CounterEffect::CounterMagic)
    }
}
