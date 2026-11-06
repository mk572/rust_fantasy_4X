//! The settings a game is played with: chosen once when the game is made, saved and loaded with
//! it, and read by the rules through [`Game::settings`](crate::game::Game::settings)
//! (`docs/game-settings.md`). The records and the figures are data in `f4x-def`
//! (`f4x_def::settings`); here is the code that applies them to a game.

pub use f4x_def::settings::*;

use crate::player::PlayerId;

impl crate::game::Game {
    /// Give each computer wizard its difficulty bonuses from the settings (a human wizard and
    /// the neutral players get none). Called when a game is made and when one is loaded, after
    /// the human is known.
    pub(crate) fn apply_wizard_settings(&mut self) {
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
