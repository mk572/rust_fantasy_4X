//! What the rules need to know about a subject (a unit, a spell) to evaluate a condition, and the
//! evaluation: the same on the map and in a battle.
//!
//! Tests: in `f4x-core`'s rules/exec/tests.rs.

use super::effects::{Condition, SpellTag};
use super::{Ability, RealmId, UnitKind};

/// What the rules need to know about a target to evaluate a [`Condition`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// The target's abilities, which the executor reads for the immunities of a damage effect.
    pub abilities: Vec<Ability>,
    pub realm: Option<RealmId>,
    pub kind: Option<UnitKind>,
    /// The kinds the spell in question is (for a wizard's retorts).
    pub spell_tags: Vec<SpellTag>,
}

/// Evaluate a condition against the facts about the subject.
pub fn eval(cond: &Condition, facts: &Facts) -> bool {
    match cond {
        Condition::Always => true,
        Condition::RealmIs(r) => facts.realm == Some(*r),
        Condition::KindIs(k) => facts.kind == Some(*k),
        Condition::SpellIs(t) => facts.spell_tags.contains(t),
        Condition::Not(c) => !eval(c, facts),
    }
}
