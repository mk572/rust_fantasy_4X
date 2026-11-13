//! The settings a game is played with: chosen once when the game is made, saved and loaded with
//! it, and read by the rules through [`Game::settings`](crate::game::Game::settings)
//! (`docs/game-settings.md`). The records and the figures are data in `f4x-def`
//! (`f4x_def::settings`); here is the code that applies them to a game.

pub use f4x_def::settings::*;

use crate::player::PlayerId;
use f4x_def::half_int::HalfInt;

impl crate::game::Game {
    /// Copy the settings that the rules read from the content (half upkeep, the arcane spell
    /// cap) into this game's own copy of the content, so a game keeps the choices it was made or
    /// saved with. Called when a game is made and when one is loaded.
    pub(crate) fn apply_content_settings(&mut self) {
        let rules = self.settings.rules;
        self.content.settings.half_upkeep.enabled = rules.half_upkeep;
        self.content.settings.spell_library.arcane_candidate_cap = rules.arcane_candidate_cap;
        for (num, capacity) in [
            (TRIREME_UNIT, rules.trireme_capacity),
            (GALLEY_UNIT, rules.galley_capacity),
        ] {
            if let Some(unit) = self.content.rules.units.by_num_mut(num) {
                unit.stats.insert(
                    crate::rules::Stat::Transport,
                    HalfInt::from_int(i32::from(capacity)),
                );
            }
        }
    }

    /// Give each computer wizard its difficulty bonuses from the settings (a human wizard and
    /// the neutral players get none). Called when a game is made and when one is loaded, after
    /// the human is known.
    pub fn apply_wizard_settings(&mut self) {
        let ai = self.settings.ai;
        let human = self.human();
        for (_, (&id, bonus, neutral)) in self.world.query_mut::<(
            &PlayerId,
            &mut crate::player::Bonus,
            Option<&crate::player::Neutral>,
        )>() {
            let computer = neutral.is_none() && Some(id) != human;
            *bonus = if computer {
                crate::player::Bonus {
                    growth: i32::from(ai.growth_percent),
                    outpost_growth: i32::from(ai.outpost_growth_percent),
                    production: i32::from(ai.production_percent),
                    gold: i32::from(ai.gold_percent),
                    mana: i32::from(ai.mana_percent),
                    food: i32::from(ai.food_percent),
                    research: i32::from(ai.research_percent),
                    maintenance: i32::from(ai.maintenance_percent),
                    unit_upkeep: i32::from(ai.unit_upkeep_percent),
                }
            } else {
                crate::player::Bonus::default()
            };
        }
    }
}
