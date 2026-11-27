//! Class 9, the hooks: the Rust code the content points at (`Effect::Custom`). The shipped
//! content names only hooks the code has, with arguments they take, and uses every one of them.

use crate::support::content;
use f4x_core::hook::{problems, spell_with_hook, CUSTOM_HOOKS};
use f4x_core::rules::effects::{Effect, EffectRule};
use f4x_def::rules::Rules;
use std::collections::BTreeSet;

/// Every effect rule of the content that a hook can stand in.
fn effects(rules: &Rules) -> Vec<&EffectRule> {
    let mut all: Vec<&EffectRule> = Vec::new();
    all.extend(rules.races.defs().iter().flat_map(|d| &d.effects));
    all.extend(rules.abilities.defs().iter().flat_map(|d| &d.effects));
    all.extend(rules.buildings.defs().iter().flat_map(|d| &d.effects));
    all.extend(rules.spells.defs().iter().flat_map(|d| &d.effects));
    all.extend(rules.items.defs().iter().flat_map(|d| &d.effects));
    all.extend(
        rules
            .level_paths
            .defs()
            .iter()
            .flat_map(|d| &d.levels)
            .flat_map(|l| &l.effects),
    );
    all
}

#[test]
fn the_shipped_content_and_the_code_agree() {
    let found = problems(&content().rules);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn every_hook_the_code_has_and_every_name_it_takes_is_used_by_the_content() {
    let content = content();
    let mut used: BTreeSet<(&str, &str)> = BTreeSet::new();
    for rule in effects(&content.rules) {
        if let Effect::Custom { name, args } = &rule.effect {
            used.insert((name, ""));
            used.extend(args.iter().map(|a| (name.as_str(), a.as_str())));
        }
    }
    let mut idle = Vec::new();
    for hook in CUSTOM_HOOKS {
        // "unimplemented" marks an imported rule with no code behind it, so the content may be
        // free of it. "disintegrate" is a leftover: the spell slays by its own effect kind now,
        // and no code acts on the hook.
        if ["unimplemented", "disintegrate"].contains(&hook.name) {
            continue;
        }
        if !used.contains(&(hook.name, "")) {
            idle.push(hook.name.to_string());
        }
        for arg in hook.args {
            if !used.contains(&(hook.name, *arg)) {
                idle.push(format!("{} {arg}", hook.name));
            }
        }
    }
    assert!(idle.is_empty(), "code no content asks for: {idle:?}");
}

#[test]
fn a_spell_is_found_by_the_hook_it_runs() {
    let content = content();
    let rules = &content.rules;
    let mut found = 0;
    for hook in CUSTOM_HOOKS {
        let carriers = rules
            .spells
            .defs()
            .iter()
            .filter(|d| d.has_hook(hook.name))
            .count();
        let spell = spell_with_hook(rules, hook.name);
        assert_eq!(spell.is_some(), carriers > 0, "{}", hook.name);
        if let Some(spell) = spell {
            assert!(rules.spells.get(spell).has_hook(hook.name), "{}", hook.name);
            found += 1;
        }
    }
    assert!(found > 0);
    assert_eq!(spell_with_hook(rules, "no_such_hook"), None);
}
