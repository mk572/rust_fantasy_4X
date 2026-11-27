//! The battle AI's levers as it plays them: those of the content, with the environment's
//! overrides (`AIK_<NAME>`, read once; one that cannot be taken is reported on stderr).
//!
//! Tests: at the end of this file.

use std::sync::OnceLock;

use f4x_core::rules::Rules;
use f4x_def::rules::ai_battle::BattleAiRules;

use crate::overrides::{self, Override, BATTLE};

pub(super) fn levers(rules: &Rules) -> BattleAiRules {
    static FROM_ENVIRONMENT: OnceLock<Vec<Override>> = OnceLock::new();
    let found = FROM_ENVIRONMENT
        .get_or_init(|| overrides::from_environment(&BATTLE, &rules.ai_battle, BattleAiRules::set));
    overrides::overridden(rules.ai_battle, BattleAiRules::set, found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overrides::{overridden, sort_out, Sorted};
    use crate::testworld::shipped;

    fn vars(of: &[(&str, &str)]) -> impl Iterator<Item = (String, String)> {
        of.iter()
            .map(|&(k, v)| (k.to_string(), v.to_string()))
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn sorted_out(content: BattleAiRules, of: &[(&str, &str)]) -> Sorted {
        sort_out(&BATTLE, &content, BattleAiRules::set, vars(of))
    }

    fn played(content: BattleAiRules, sorted: &Sorted) -> BattleAiRules {
        overridden(content, BattleAiRules::set, &sorted.taken)
    }

    #[test]
    fn a_variable_overrides_the_lever_it_names() {
        let content = shipped().rules.ai_battle;
        let more = content.orders.flee_strength_ratio + 1.0;
        let sorted = sorted_out(content, &[("AIK_FLEE_STRENGTH_RATIO", &more.to_string())]);
        assert!(sorted.warnings(&BATTLE).is_empty(), "{sorted:?}");
        let taken = played(content, &sorted);
        assert_eq!(taken.orders.flee_strength_ratio, more);
        let mut rest = taken;
        rest.orders.flee_strength_ratio = content.orders.flee_strength_ratio;
        assert_eq!(rest, content, "the other levers are the content's");
    }

    #[test]
    fn a_bad_value_and_an_unknown_name_are_each_reported() {
        let content = shipped().rules.ai_battle;
        let sorted = sorted_out(
            content,
            &[
                ("AIK_FLEE_STRENGTH_RATIO", "never"),
                ("AIK_NO_SUCH_LEVER", "1"),
            ],
        );
        assert!(sorted.taken.is_empty(), "{sorted:?}");
        assert_eq!(
            sorted.refused,
            [("FLEE_STRENGTH_RATIO".to_string(), "never".to_string())]
        );
        assert_eq!(sorted.unknown, ["NO_SUCH_LEVER"]);
        let warnings = sorted.warnings(&BATTLE);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].starts_with("AIK_FLEE_STRENGTH_RATIO=never "));
        assert!(warnings[1].starts_with("AIK_NO_SUCH_LEVER "));
        assert_eq!(played(content, &sorted), content);
    }

    #[test]
    fn the_variables_of_others_are_passed_over_without_a_word() {
        let content = shipped().rules.ai_battle;
        let sorted = sorted_out(
            content,
            &[
                ("AIS_FLEE_STRENGTH_RATIO", "1"),
                ("FLEE_STRENGTH_RATIO", "1"),
                ("AIKIDO", "1"),
            ],
        );
        assert!(sorted.taken.is_empty(), "{sorted:?}");
        assert!(sorted.warnings(&BATTLE).is_empty(), "{sorted:?}");
    }
}
