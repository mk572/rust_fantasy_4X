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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::shipped;
    use crate::rules::effects::Condition;
    use crate::rules::{RealmId, RetortDef};

    fn retorts(nums: &[u16]) -> BTreeSet<Retort> {
        nums.iter().map(|&n| Retort::new(n)).collect()
    }

    /// What a retort's rule asks about: the realm and the tags of a spell, or only a realm (a
    /// dispel, a node).
    struct Subject {
        realm: RealmId,
        summoning: bool,
        crafting: bool,
    }

    impl Subject {
        fn of_spell(def: &SpellDef) -> Self {
            Self {
                realm: def.realm,
                summoning: is_summoning(def),
                crafting: is_crafting(def),
            }
        }

        fn of_realm(realm: RealmId) -> Self {
            Self {
                realm,
                summoning: false,
                crafting: false,
            }
        }

        fn meets(&self, condition: &Condition) -> bool {
            match condition {
                Condition::Always => true,
                Condition::RealmIs(realm) => *realm == self.realm,
                Condition::SpellIs(SpellTag::Summoning) => self.summoning,
                Condition::SpellIs(SpellTag::Crafting) => self.crafting,
                Condition::KindIs(_) => false,
                Condition::Not(inner) => !self.meets(inner),
            }
        }
    }

    /// `base` after the passive rules of the retort `row` (the rows of `retorts.ron`) that name
    /// `stat` and whose condition `subject` meets, each applied by its operation. A retort names
    /// a stat once, which is checked.
    fn after_rules(row: &RetortDef, stat: Stat, base: i32, subject: &Subject) -> i32 {
        let mut value = base;
        let mut seen = 0;
        for rule in row.effects.iter().filter(|r| r.when == Trigger::Passive) {
            let Effect::Modify {
                stat: s,
                op,
                value: v,
                ..
            } = rule.effect
            else {
                continue;
            };
            if s != stat {
                continue;
            }
            seen += 1;
            if !subject.meets(&rule.condition) {
                continue;
            }
            let v = v.floor();
            value = match op {
                ModOp::Add => value + v,
                ModOp::Percent => (value * (100 + v)).div_euclid(100),
                ModOp::Min => value.max(v),
                ModOp::Max => value.min(v),
                ModOp::Set => v,
            };
        }
        assert!(seen <= 1, "{} names {stat:?} {seen} times", row.id);
        value
    }

    /// Every stat the passive rules of `row` change.
    fn stats_of(row: &RetortDef) -> BTreeSet<Stat> {
        row.effects
            .iter()
            .filter(|r| r.when == Trigger::Passive)
            .filter_map(|r| match r.effect {
                Effect::Modify { stat, .. } => Some(stat),
                _ => None,
            })
            .collect()
    }

    fn one(row: &RetortDef) -> BTreeSet<Retort> {
        retorts(&[row.num])
    }

    #[test]
    fn a_retort_changes_the_cost_and_the_research_of_the_spells_its_rules_name_and_no_others() {
        // The rules of each retort's row: `SpellCostPercent` and `ResearchIncome`, under their
        // condition (a realm, summoning, crafting).
        let rules = &shipped().rules;
        let mut changed = 0;
        for (_, row) in rules.retorts.iter() {
            let set = one(row);
            for (_, def) in rules.spells.iter() {
                let subject = Subject::of_spell(def);
                let cost = after_rules(row, Stat::SpellCostPercent, 0, &subject);
                let research = after_rules(row, Stat::ResearchIncome, 100, &subject) - 100;
                assert_eq!(
                    cost_cut(rules, &set, def),
                    -cost,
                    "{} on {}",
                    row.id,
                    def.id
                );
                assert_eq!(
                    research_bonus(rules, &set, def),
                    research,
                    "{} on {}",
                    row.id,
                    def.id
                );
                changed += usize::from(cost != 0 || research != 0);
            }
        }
        assert!(changed > 0, "no retort changes any spell");
    }

    #[test]
    fn retorts_that_change_the_research_of_a_spell_add_up() {
        // Two retorts whose rules both raise the research of one spell: the bonus is the sum.
        let rules = &shipped().rules;
        let rows: Vec<&RetortDef> = rules.retorts.iter().map(|(_, row)| row).collect();
        let mut pairs = 0;
        for (_, def) in rules.spells.iter() {
            let subject = Subject::of_spell(def);
            let bonus =
                |row: &RetortDef| after_rules(row, Stat::ResearchIncome, 100, &subject) - 100;
            for (i, a) in rows.iter().enumerate() {
                for b in &rows[i + 1..] {
                    if bonus(a) == 0 || bonus(b) == 0 {
                        continue;
                    }
                    let both: BTreeSet<Retort> = one(a).union(&one(b)).copied().collect();
                    assert_eq!(
                        research_bonus(rules, &both, def),
                        bonus(a) + bonus(b),
                        "{} and {} on {}",
                        a.id,
                        b.id,
                        def.id
                    );
                    pairs += 1;
                }
            }
        }
        assert!(pairs > 0, "no two retorts raise the research of one spell");
    }

    #[test]
    fn the_wizard_wide_retorts_change_their_stats_by_their_rules() {
        // Each stat of each retort, over a few bases; a rule with a condition on a realm is read
        // for a wizard-wide stat as met (no spell is asked about).
        let rules = &shipped().rules;
        let nowhere = Subject {
            realm: RealmId::new(u16::MAX),
            summoning: false,
            crafting: false,
        };
        let mut checked = 0;
        for (_, row) in rules.retorts.iter() {
            for stat_ in stats_of(row) {
                if matches!(
                    stat_,
                    Stat::SpellCostPercent
                        | Stat::ResearchIncome
                        | Stat::DispelMultiplier
                        | Stat::NodePowerDoublings
                ) {
                    continue; // asked about a spell or a realm: the tests above and below
                }
                for base in [0, 3, 7, 40, 100] {
                    assert_eq!(
                        stat(rules, &one(row), stat_, base),
                        after_rules(row, stat_, base, &nowhere),
                        "{} {stat_:?} from {base}",
                        row.id
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 0);
        let none = retorts(&[]);
        for base in [0, 3, 40] {
            assert_eq!(stat(rules, &none, Stat::CastingSkill, base), base);
        }
    }

    #[test]
    fn a_dispel_and_a_node_follow_the_realm_their_retorts_name() {
        // `DispelMultiplier` and `NodePowerDoublings` are asked per realm.
        let rules = &shipped().rules;
        let realms: Vec<RealmId> = rules.realms.iter().map(|(id, _)| id).collect();
        let mut named = 0;
        for (_, row) in rules.retorts.iter() {
            for (stat_, base) in [(Stat::DispelMultiplier, 1), (Stat::NodePowerDoublings, 0)] {
                for &realm in &realms {
                    let facts = Facts {
                        realm: Some(realm),
                        ..Facts::default()
                    };
                    let want = after_rules(row, stat_, base, &Subject::of_realm(realm));
                    assert_eq!(
                        stat_with(rules, &one(row), stat_, base, &facts),
                        want,
                        "{} {stat_:?} in realm {realm:?}",
                        row.id
                    );
                    named += usize::from(want != base);
                }
            }
        }
        assert!(named > 0);
    }
}
