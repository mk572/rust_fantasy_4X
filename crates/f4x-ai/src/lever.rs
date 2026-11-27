//! The strategic AI's levers as it plays them: those of the content (`ai_strategy.ron`), with the
//! environment's overrides (`AIS_<NAME>`, read once; one that cannot be taken is reported on
//! stderr).
//!
//! Tests: at the end of this file.

use std::sync::{Mutex, OnceLock, PoisonError};

use f4x_core::player::view::PlayerView;
use f4x_core::rules::Rules;
use f4x_core::world::World;
use f4x_def::rules::ai_strategy::StrategyAiRules;

use crate::overrides::{self, Override, STRATEGIC};

/// What gives the strategic AI its levers.
pub(crate) trait Levered {
    fn levers(&self) -> &StrategyAiRules;
}

impl Levered for Rules {
    fn levers(&self) -> &StrategyAiRules {
        static FROM_ENVIRONMENT: OnceLock<Vec<Override>> = OnceLock::new();
        let found = FROM_ENVIRONMENT.get_or_init(|| {
            overrides::from_environment(&STRATEGIC, &self.ai_strategy, StrategyAiRules::set)
        });
        if found.is_empty() {
            &self.ai_strategy
        } else {
            overridden(&self.ai_strategy, found)
        }
    }
}

impl Levered for World {
    fn levers(&self) -> &StrategyAiRules {
        self.content().rules.levers()
    }
}

impl Levered for PlayerView<'_> {
    fn levers(&self) -> &StrategyAiRules {
        self.content().rules.levers()
    }
}

/// The content's levers with the overrides, made once for each content and kept until the
/// program ends (a run has one content).
fn overridden(content: &StrategyAiRules, found: &[Override]) -> &'static StrategyAiRules {
    type Made = Vec<(StrategyAiRules, &'static StrategyAiRules)>;
    static MADE: Mutex<Made> = Mutex::new(Vec::new());
    let mut made = MADE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(&(_, levers)) = made.iter().find(|(of, _)| of == content) {
        return levers;
    }
    let levers = overrides::overridden(content.clone(), StrategyAiRules::set, found);
    let levers: &'static StrategyAiRules = Box::leak(Box::new(levers));
    made.push((content.clone(), levers));
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
    fn a_variable_overrides_the_strategic_lever_it_names() {
        let content = shipped().rules.ai_strategy.clone();
        let more = content.attack.attack_force + 1.0;
        let sorted = overrides::sort_out(
            &STRATEGIC,
            &content,
            StrategyAiRules::set,
            vars(&[("AIS_ATTACK_FORCE", &more.to_string())]),
        );
        assert!(sorted.warnings(&STRATEGIC).is_empty(), "{sorted:?}");
        let played = overrides::overridden(content.clone(), StrategyAiRules::set, &sorted.taken);
        assert_eq!(played.attack.attack_force, more);
        let mut rest = played;
        rest.attack.attack_force = content.attack.attack_force;
        assert_eq!(rest, content, "the other levers are the content's");
    }

    #[test]
    fn a_bad_value_and_an_unknown_name_are_each_reported_and_the_battle_ones_passed_over() {
        let content = shipped().rules.ai_strategy.clone();
        let sorted = overrides::sort_out(
            &STRATEGIC,
            &content,
            StrategyAiRules::set,
            vars(&[
                ("AIS_ATTACK_FORCE", "never"),
                ("AIS_NO_SUCH_LEVER", "1"),
                ("AIK_ATTACK", "1"),
                ("ATTACK_FORCE", "1"),
            ]),
        );
        assert!(sorted.taken.is_empty(), "{sorted:?}");
        assert_eq!(
            sorted.refused,
            [("ATTACK_FORCE".to_string(), "never".to_string())]
        );
        assert_eq!(sorted.unknown, ["NO_SUCH_LEVER"]);
        let warnings = sorted.warnings(&STRATEGIC);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].starts_with("AIS_ATTACK_FORCE=never "));
        assert!(warnings[1].starts_with("AIS_NO_SUCH_LEVER "));
        assert!(warnings[1].contains("AIK_<NAME>"));
    }
}
