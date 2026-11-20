//! What a wizard's retorts do to a stat of the wizard or of what it owns (the levels of its units,
//! its mana income), read from the `effects` of each retort's row in `retorts.ron`.
//!
//! Tests: in `f4x-core`'s wizard/retort.rs.

use super::effects::{Effect, Trigger};
use super::facts::{eval, Facts};
use super::stats::{compute, compute_half, Modifier, Source, Stat};
use super::{RetortDef, Rules};
use crate::half_int::HalfInt;
use crate::rules::Id;

/// The changes the retorts make to wizard-wide stats, the rules whose condition holds for `facts`.
pub fn stat_modifiers(
    rules: &Rules,
    retorts: impl IntoIterator<Item = Id<RetortDef>>,
    facts: &Facts,
) -> Vec<Modifier> {
    retorts
        .into_iter()
        .filter_map(|r| rules.retorts.try_get(r).map(|row| (r, row)))
        .flat_map(|(r, row)| {
            row.effects
                .iter()
                .filter(|rule| rule.when == Trigger::Passive)
                .filter(|rule| eval(&rule.condition, facts))
                .filter_map(move |rule| match rule.effect {
                    Effect::Modify {
                        stat, op, value, ..
                    } => Some(Modifier::amount(stat, op, value, Source::Retort(r))),
                    _ => None,
                })
        })
        .collect()
}

/// `base` changed by the retorts' rules for `stat` (a wizard-wide stat such as `ManaIncome` or
/// `CastingSkill`, not one about a spell), the rules whose condition holds for `facts`.
pub fn stat_with(
    rules: &Rules,
    retorts: impl IntoIterator<Item = Id<RetortDef>>,
    stat: Stat,
    base: i32,
    facts: &Facts,
) -> i32 {
    compute(stat, base, &stat_modifiers(rules, retorts, facts))
}

/// [`stat_with`] over an amount in halves (the mana income).
pub fn stat_half(
    rules: &Rules,
    retorts: impl IntoIterator<Item = Id<RetortDef>>,
    stat: Stat,
    base: HalfInt,
) -> HalfInt {
    compute_half(
        stat,
        base,
        &stat_modifiers(rules, retorts, &Facts::default()),
    )
}
