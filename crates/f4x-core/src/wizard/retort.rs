//! What a wizard's retorts do to one spell, read from the rules: the `effects` of each retort's
//! row in `retorts.ron`, under a condition on the spell (its realm, whether it summons or crafts).
//!
//! Tests: below.

use std::collections::BTreeSet;

use crate::rules::effects::{Effect, SpellTag, Trigger};
use crate::rules::exec::{eval, Facts};
use crate::rules::stats::compute;
use crate::rules::{ModOp, Modifier, Rules, Source, SpellDef, Stat};
use crate::wizard::magic::{is_crafting, is_summoning, Retort};
use f4x_def::half_int::HalfInt;

/// What the retort conditions can ask about `def`.
fn facts(def: &SpellDef) -> Facts {
    let mut spell_tags = Vec::new();
    if is_summoning(def) {
        spell_tags.push(SpellTag::Summoning);
    }
    if is_crafting(def) {
        spell_tags.push(SpellTag::Crafting);
    }
    Facts {
        realm: Some(def.realm),
        spell_tags,
        ..Facts::default()
    }
}

/// The changes the retorts make that apply to `def`.
fn modifiers(rules: &Rules, retorts: &BTreeSet<Retort>, def: &SpellDef) -> Vec<Modifier> {
    let facts = facts(def);
    let mut out = Vec::new();
    for &retort in retorts {
        let Some(row) = rules.retorts.try_get(retort) else {
            continue;
        };
        for rule in row.effects.iter().filter(|r| r.when == Trigger::Passive) {
            if let Effect::Modify {
                stat, op, value, ..
            } = rule.effect
            {
                if eval(&rule.condition, &facts) {
                    out.push(Modifier::amount(stat, op, value, Source::Retort(retort)));
                }
            }
        }
    }
    out
}

/// `base` changed by the retorts' rules for `stat` (a wizard-wide stat such as `ManaIncome` or
/// `CastingSkill`, not one about a spell), the rules whose condition holds for `facts`.
pub fn stat_with(
    rules: &Rules,
    retorts: &BTreeSet<Retort>,
    stat: Stat,
    base: i32,
    facts: &Facts,
) -> i32 {
    f4x_def::rules::retort_stat::stat_with(rules, retorts.iter().copied(), stat, base, facts)
}

/// [`stat`] over an amount in halves (the mana income).
pub fn stat_half(rules: &Rules, retorts: &BTreeSet<Retort>, stat: Stat, base: HalfInt) -> HalfInt {
    f4x_def::rules::retort_stat::stat_half(rules, retorts.iter().copied(), stat, base)
}

/// [`stat_with`] for a rule that asks nothing (its condition is `Always`).
pub fn stat(rules: &Rules, retorts: &BTreeSet<Retort>, stat: Stat, base: i32) -> i32 {
    stat_with(rules, retorts, stat, base, &Facts::default())
}

/// Percent off the mana cost of `def` that the retorts give.
pub fn cost_cut(rules: &Rules, retorts: &BTreeSet<Retort>, def: &SpellDef) -> i32 {
    -compute(Stat::SpellCostPercent, 0, &modifiers(rules, retorts, def))
}

/// Percent the retorts add to the research of `def`.
pub fn research_bonus(rules: &Rules, retorts: &BTreeSet<Retort>, def: &SpellDef) -> i32 {
    let mods = modifiers(rules, retorts, def);
    debug_assert!(mods
        .iter()
        .all(|m| m.stat != Stat::ResearchIncome || m.op != ModOp::Set));
    compute(Stat::ResearchIncome, 100, &mods) - 100
}
