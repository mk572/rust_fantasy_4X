//! What a wizard's retorts do to one spell, read from the rules: the `effects` of each retort's
//! row in `retorts.ron`, under a condition on the spell (its realm, whether it summons or crafts).
//!
//! Tests: below.

use std::collections::BTreeSet;

use crate::rules::effects::{Effect, SpellTag, Trigger};
use crate::rules::exec::{eval, Facts};
use crate::rules::stats::{compute, compute_half};
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
            if let Effect::Modify { stat, op, value } = rule.effect {
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
    compute(stat, base, &stat_modifiers(rules, retorts, facts))
}

/// [`stat`] over an amount in halves (the mana income).
pub fn stat_half(rules: &Rules, retorts: &BTreeSet<Retort>, stat: Stat, base: HalfInt) -> HalfInt {
    compute_half(
        stat,
        base,
        &stat_modifiers(rules, retorts, &Facts::default()),
    )
}

fn stat_modifiers(rules: &Rules, retorts: &BTreeSet<Retort>, facts: &Facts) -> Vec<Modifier> {
    retorts
        .iter()
        .filter_map(|&r| rules.retorts.try_get(r).map(|row| (r, row)))
        .flat_map(|(r, row)| {
            row.effects
                .iter()
                .filter(|rule| rule.when == Trigger::Passive)
                .filter(|rule| eval(&rule.condition, facts))
                .filter_map(move |rule| match rule.effect {
                    Effect::Modify { stat, op, value } => {
                        Some(Modifier::amount(stat, op, value, Source::Retort(r)))
                    }
                    _ => None,
                })
        })
        .collect()
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

    #[test]
    fn the_wizard_wide_retorts_change_their_stats() {
        let rules = &shipped().rules;
        let none = retorts(&[]);
        let check = |nums: &[u16], stat_: Stat, base: i32, want: i32| {
            assert_eq!(
                stat(rules, &retorts(nums), stat_, base),
                want,
                "{nums:?} {stat_:?}"
            );
        };
        // Alchemy: all the mana back, magic weapons at least.
        check(&[0], Stat::AlchemyPercent, 50, 100);
        check(&[], Stat::AlchemyPercent, 50, 50);
        check(&[0], Stat::StartingWeapons, 0, 1);
        check(&[0], Stat::StartingWeapons, 3, 3);
        // Warlord: a level more.
        check(&[1], Stat::UnitLevels, 0, 1);
        // Channeler: two squares from the fortress wherever the battle is.
        check(&[8], Stat::CastingDistance, 25, 2);
        check(&[], Stat::CastingDistance, 25, 25);
        // Node Mastery.
        check(&[12], Stat::NodeImmunity, 0, 1);
        assert_eq!(stat(rules, &none, Stat::NodeImmunity, 0), 0);
        // Famous, Charismatic.
        check(&[13], Stat::StartingFame, 0, 10);
        check(&[16], Stat::DiplomacyGainPercent, 100, 200);
        check(&[16], Stat::DiplomacyLossPercent, 100, 50);
        check(&[16], Stat::HireCostPercent, 100, 50);
        check(&[13], Stat::OfferChancePercent, 100, 200);
        check(&[], Stat::OfferChancePercent, 100, 100);
        // Divine and Infernal Power, alone or together (they do not add up).
        check(&[5], Stat::ReligiousPercent, 100, 150);
        check(&[6], Stat::ReligiousPercent, 100, 150);
        // Archmage: ten more casting skill, the same to start with, half as much more gained.
        check(&[10], Stat::CastingSkill, 0, 10);
        check(&[10], Stat::StartingSkill, 6, 16);
        check(&[10], Stat::SkillGainPercent, 100, 150);
    }

    #[test]
    fn a_dispel_resists_once_more_for_the_casters_mastery_of_the_realm_and_for_archmage() {
        let rules = &shipped().rules;
        let times = |nums: &[u16], realm: u16| {
            let facts = Facts {
                realm: Some(realm_ref(realm)),
                ..Facts::default()
            };
            stat_with(rules, &retorts(nums), Stat::DispelMultiplier, 1, &facts)
        };
        assert_eq!(times(&[], 0), 1);
        assert_eq!(times(&[3 /* nature_mastery */], 0 /* nature */), 2);
        assert_eq!(times(&[3], 2 /* chaos */), 1);
        assert_eq!(times(&[10 /* archmage */, 3], 0), 3);
        assert_eq!(
            times(&[10, 2 /* chaos_mastery */, 4 /* sorcery_mastery */], 1),
            3
        );
    }

    #[test]
    fn a_node_doubles_for_its_realms_mastery_and_again_for_node_mastery() {
        let rules = &shipped().rules;
        let doublings = |nums: &[u16], realm: u16| {
            let facts = Facts {
                realm: Some(realm_ref(realm)),
                ..Facts::default()
            };
            stat_with(rules, &retorts(nums), Stat::NodePowerDoublings, 0, &facts)
        };
        assert_eq!(doublings(&[], 0), 0);
        assert_eq!(doublings(&[3 /* nature_mastery */], 0 /* nature */), 1);
        assert_eq!(doublings(&[3], 1 /* sorcery */), 0);
        assert_eq!(doublings(&[12 /* node_mastery */], 2 /* chaos */), 1);
        assert_eq!(doublings(&[12, 2 /* chaos_mastery */], 2), 2);
        assert_eq!(doublings(&[12, 2, 4, 3], 1), 2);
    }
}
