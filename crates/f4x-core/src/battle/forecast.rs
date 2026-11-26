//! The forecast of a fight, from the game's side: what a side is worth (the combat ratings of its
//! units at their levels, the strongest `max_battle_side` added) and how it compares with the
//! other. The enum and the bands are in `f4x-def` (`rules/forecast.rs`) and re-exported here at
//! the old path.
//!
//! Tests: in battle/tests.rs.

use crate::world::World;

pub use f4x_def::rules::forecast::Forecast;

impl World {
    /// What the units of one side are worth in a battle, by their combat ratings: the strongest
    /// `max_battle_side` of them added. A unit with no rating (a settler) adds nothing.
    pub fn side_worth(&self, units: &[hecs::Entity]) -> f32 {
        let rules = &self.content().rules;
        let mut worths: Vec<f32> = units
            .iter()
            .filter_map(|&u| {
                let def = self.units().unit_def(u)?;
                rules.combat_rating(def, self.units().unit_level(u))
            })
            .collect();
        worths.sort_by(|a, b| b.total_cmp(a));
        worths.iter().take(self.max_battle_side()).sum()
    }

    /// How `own` compares with `enemy` if they fought (`None` if neither is worth anything).
    pub fn combat_forecast(
        &self,
        own: &[hecs::Entity],
        enemy: &[hecs::Entity],
    ) -> Option<Forecast> {
        Forecast::of_worths(
            self.side_worth(own),
            self.side_worth(enemy),
            self.content().rules.config.forecast.bands_pct,
        )
    }
}
