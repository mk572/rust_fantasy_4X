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
            if let Effect::Modify { stat, op, value } = rule.effect {
                if eval(&rule.condition, &facts) {
                    out.push(Modifier::new(stat, op, value, Source::Retort(retort)));
                }
            }
        }
    }
    out
}

/// `base` changed by the retorts' unconditional rules for `stat` (a wizard-wide stat such as
/// `ManaIncome` or `SpellUpkeepPercent`, not one about a spell).
pub fn stat(rules: &Rules, retorts: &BTreeSet<Retort>, stat: Stat, base: i32) -> i32 {
    let mods = retorts
        .iter()
        .filter_map(|&r| rules.retorts.try_get(r).map(|row| (r, row)))
        .flat_map(|(r, row)| {
            row.effects
                .iter()
                .filter(|rule| rule.when == Trigger::Passive)
                .filter(|rule| eval(&rule.condition, &Facts::default()))
                .filter_map(move |rule| match rule.effect {
                    Effect::Modify { stat, op, value } => {
                        Some(Modifier::new(stat, op, value, Source::Retort(r)))
                    }
                    _ => None,
                })
        })
        .collect::<Vec<_>>();
    compute(stat, base, &mods)
}

/// Percent off the mana cost of `def` that the retorts give.
pub fn cost_cut(rules: &Rules, retorts: &BTreeSet<Retort>, def: &SpellDef) -> i32 {
    -compute(Stat::SpellCostPercent, 0, &modifiers(rules, retorts, def))
}

/// Percent the retorts add to the research of `def`.
pub fn research_bonus(rules: &Rules, retorts: &BTreeSet<Retort>, def: &SpellDef) -> i32 {
    let mods = modifiers(rules, retorts, def);
    debug_assert!(mods.iter().all(|m| m.op != ModOp::Set));
    compute(Stat::ResearchIncome, 100, &mods) - 100
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::{realm_ref, shipped};

    fn retorts(nums: &[u16]) -> BTreeSet<Retort> {
        nums.iter().map(|&n| Retort::new(n)).collect()
    }

    fn spell<'a>(rules: &'a Rules, alias: &str) -> &'a SpellDef {
        rules
            .spells
            .iter()
            .map(|(_, d)| d)
            .find(|d| d.id == alias)
            .unwrap_or_else(|| panic!("no spell {alias}"))
    }

    #[test]
    fn a_mastery_cuts_cost_and_speeds_research_in_its_own_realm_only() {
        let rules = &shipped().rules;
        let mastery = retorts(&[3 /* nature_mastery */]);
        let nature = rules
            .spells
            .iter()
            .map(|(_, d)| d)
            .find(|d| d.realm == realm_ref(0 /* nature */))
            .expect("a nature spell");
        let other = rules
            .spells
            .iter()
            .map(|(_, d)| d)
            .find(|d| d.realm == realm_ref(2 /* chaos */))
            .expect("a chaos spell");
        assert_eq!(cost_cut(rules, &mastery, nature), 15);
        assert_eq!(research_bonus(rules, &mastery, nature), 15);
        assert_eq!(cost_cut(rules, &mastery, other), 0);
        assert_eq!(research_bonus(rules, &mastery, other), 0);
    }

    #[test]
    fn the_crafting_and_summoning_retorts_single_out_their_spells() {
        let rules = &shipped().rules;
        let craft = spell(rules, "enchant_item");
        assert_eq!(cost_cut(rules, &retorts(&[17 /* artificer */]), craft), 50);
        assert_eq!(cost_cut(rules, &retorts(&[15 /* conjurer */]), craft), 0);
        let summon = rules
            .spells
            .iter()
            .map(|(_, d)| d)
            .find(|d| is_summoning(d))
            .expect("a summoning spell");
        let conjurer = retorts(&[15 /* conjurer */]);
        assert_eq!(cost_cut(rules, &conjurer, summon), 25);
        assert_eq!(research_bonus(rules, &conjurer, summon), 25);
    }

    #[test]
    fn mana_focusing_adds_a_quarter_to_the_mana_and_channeler_halves_the_upkeep() {
        let rules = &shipped().rules;
        let focusing = retorts(&[11 /* mana_focusing */]);
        for mana in [0, 3, 7, 40] {
            assert_eq!(
                stat(rules, &focusing, Stat::ManaIncome, mana),
                mana + mana / 4
            );
        }
        let channeler = retorts(&[8 /* channeler */]);
        assert_eq!(stat(rules, &channeler, Stat::SpellUpkeepPercent, 100), 50);
        assert_eq!(
            stat(rules, &retorts(&[]), Stat::SpellUpkeepPercent, 100),
            100
        );
    }

    #[test]
    fn sage_master_and_a_mastery_add_up() {
        let rules = &shipped().rules;
        let both = retorts(&[7 /* sage_master */, 3 /* nature_mastery */]);
        let nature = rules
            .spells
            .iter()
            .map(|(_, d)| d)
            .find(|d| d.realm == realm_ref(0 /* nature */))
            .expect("a nature spell");
        assert_eq!(research_bonus(rules, &both, nature), 40);
    }
}
