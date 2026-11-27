//! The battle AI's levers as it plays them: those of the content, with the environment's
//! overrides (`AIK_<NAME>`, read once; one that cannot be taken is reported on stderr).
//!
//! Tests: at the end of this file.

use std::sync::OnceLock;

use f4x_core::rules::Rules;
use f4x_def::rules::ai_battle::{BattleAiRules, OverrideError};

/// An override: the lever's name and the value's text.
type Override = (String, String);

pub(super) fn levers(rules: &Rules) -> BattleAiRules {
    static FROM_ENVIRONMENT: OnceLock<Vec<Override>> = OnceLock::new();
    let overrides = FROM_ENVIRONMENT.get_or_init(|| {
        let vars = std::env::vars_os()
            .filter_map(|(k, v)| Some((k.into_string().ok()?, v.to_string_lossy().into_owned())));
        let sorted = sort_out(rules.ai_battle, vars);
        for warning in sorted.warnings() {
            eprintln!("{warning}");
        }
        sorted.taken
    });
    overridden(rules.ai_battle, overrides)
}

/// The `AIK_` variables, by what becomes of them.
#[derive(Debug, Default)]
struct Sorted {
    /// Those that override a lever.
    taken: Vec<Override>,
    /// Those that name a lever but give it no value it can take.
    refused: Vec<Override>,
    /// The names that are no lever's.
    unknown: Vec<String>,
}

impl Sorted {
    /// A line for each variable that is not taken.
    fn warnings(&self) -> Vec<String> {
        let refused = self.refused.iter().map(|(name, text)| {
            format!("AIK_{name}={text} is no value for this lever; the content's is used")
        });
        let unknown = self.unknown.iter().map(|name| {
            format!(
                "AIK_{name} names no battle lever; it is ignored (the strategic AI's overrides \
                 are AIS_<NAME>)"
            )
        });
        refused.chain(unknown).collect()
    }
}

/// Sort the variables that start with `AIK_`; the others are not the battle AI's.
fn sort_out(levers: BattleAiRules, vars: impl Iterator<Item = (String, String)>) -> Sorted {
    let mut sorted = Sorted::default();
    for (var, text) in vars {
        let Some(name) = var.strip_prefix("AIK_") else {
            continue;
        };
        let mut tried = levers;
        match tried.set(name, &text) {
            Ok(()) => sorted.taken.push((name.to_string(), text)),
            Err(OverrideError::BadValue) => sorted.refused.push((name.to_string(), text)),
            Err(OverrideError::UnknownLever) => sorted.unknown.push(name.to_string()),
        }
    }
    sorted
}

/// `levers` with each of the overrides that it takes.
fn overridden(mut levers: BattleAiRules, overrides: &[Override]) -> BattleAiRules {
    for (name, text) in overrides {
        // `sort_out` kept only those that are taken, and one that is not changes nothing.
        if levers.set(name, text).is_err() {
            continue;
        }
    }
    levers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::shipped;

    fn vars(of: &[(&str, &str)]) -> impl Iterator<Item = (String, String)> {
        of.iter()
            .map(|&(k, v)| (k.to_string(), v.to_string()))
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn a_variable_overrides_the_lever_it_names() {
        let content = shipped().rules.ai_battle;
        let more = content.orders.flee_strength_ratio + 1.0;
        let sorted = sort_out(
            content,
            vars(&[("AIK_FLEE_STRENGTH_RATIO", &more.to_string())]),
        );
        assert!(sorted.warnings().is_empty(), "{sorted:?}");
        let played = overridden(content, &sorted.taken);
        assert_eq!(played.orders.flee_strength_ratio, more);
        let mut rest = played;
        rest.orders.flee_strength_ratio = content.orders.flee_strength_ratio;
        assert_eq!(rest, content, "the other levers are the content's");
    }

    #[test]
    fn a_bad_value_and_an_unknown_name_are_each_reported() {
        let content = shipped().rules.ai_battle;
        let sorted = sort_out(
            content,
            vars(&[
                ("AIK_FLEE_STRENGTH_RATIO", "never"),
                ("AIK_NO_SUCH_LEVER", "1"),
            ]),
        );
        assert!(sorted.taken.is_empty(), "{sorted:?}");
        assert_eq!(
            sorted.refused,
            [("FLEE_STRENGTH_RATIO".to_string(), "never".to_string())]
        );
        assert_eq!(sorted.unknown, ["NO_SUCH_LEVER"]);
        let warnings = sorted.warnings();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].starts_with("AIK_FLEE_STRENGTH_RATIO=never "));
        assert!(warnings[1].starts_with("AIK_NO_SUCH_LEVER "));
        assert_eq!(overridden(content, &sorted.taken), content);
    }

    #[test]
    fn the_variables_of_others_are_passed_over_without_a_word() {
        let content = shipped().rules.ai_battle;
        let sorted = sort_out(
            content,
            vars(&[
                ("AIS_FLEE_STRENGTH_RATIO", "1"),
                ("FLEE_STRENGTH_RATIO", "1"),
                ("AIKIDO", "1"),
            ]),
        );
        assert!(sorted.taken.is_empty(), "{sorted:?}");
        assert!(sorted.warnings().is_empty(), "{sorted:?}");
    }
}
