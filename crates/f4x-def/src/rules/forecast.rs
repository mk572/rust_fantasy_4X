//! The forecast of a fight: how two sides compare by the unit combat ratings (`unit_ratings.ron`,
//! `docs/combat-rating.md`), for the question before an attack when the setting `combat_forecast`
//! is on. Walls, terrain, spells and the players' skill are not in it, so it is a hint, not a
//! promise. This file is the verdict on the worths: the bands (`config.forecast.bands_pct`) that
//! split the ratio of two worths, and the key of the message for each. What a side is worth is
//! worked out by the game (`Game::side_worth` in `f4x-core`).
//!
//! Tests: below.

/// How the sides compare, from one side's point of view (the human's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Forecast {
    /// The side is far weaker than the other.
    CompletelyOutmatched,
    MuchWeaker,
    SlightlyWeaker,
    Even,
    SlightlyStronger,
    MuchStronger,
    /// The other side is far weaker than this one.
    OtherOutmatched,
}

impl Forecast {
    /// The key of the message that says it, in `attack.ron`.
    pub fn text_key(self) -> &'static str {
        match self {
            Forecast::CompletelyOutmatched => "attack.forecast_outmatched",
            Forecast::MuchWeaker => "attack.forecast_much_weaker",
            Forecast::SlightlyWeaker => "attack.forecast_slightly_weaker",
            Forecast::Even => "attack.forecast_even",
            Forecast::SlightlyStronger => "attack.forecast_slightly_stronger",
            Forecast::MuchStronger => "attack.forecast_much_stronger",
            Forecast::OtherOutmatched => "attack.forecast_other_outmatched",
        }
    }

    /// The forecast for a side of worth `own` against `enemy`. `bands_pct` are the three shares
    /// (percent, rising) below which own/enemy is completely outmatched, much weaker and slightly
    /// weaker; the same shares turned over (100 / share) mark the other way. `None` when neither
    /// side is worth anything.
    pub fn of_worths(own: f32, enemy: f32, bands_pct: [u32; 3]) -> Option<Self> {
        if own <= 0.0 && enemy <= 0.0 {
            return None;
        }
        let share = |share: u32| share as f32 / 100.0;
        // own / enemy against a share, without dividing by zero.
        let below = |share: f32| own < enemy * share;
        // own / enemy above 1 / share.
        let above = |share: f32| own * share > enemy;
        let [outmatched, much, slightly] = bands_pct.map(share);
        Some(if below(outmatched) {
            Forecast::CompletelyOutmatched
        } else if below(much) {
            Forecast::MuchWeaker
        } else if below(slightly) {
            Forecast::SlightlyWeaker
        } else if above(outmatched) {
            Forecast::OtherOutmatched
        } else if above(much) {
            Forecast::MuchStronger
        } else if above(slightly) {
            Forecast::SlightlyStronger
        } else {
            Forecast::Even
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bands_split_the_ratio_the_same_way_both_ways() {
        // Shares of 25, 50 and 80 percent, and so marks at 4, 2 and 1.25 times the other side.
        let bands = [25, 50, 80];
        let seen = |own: f32| Forecast::of_worths(own, 100.0, bands);
        assert_eq!(seen(20.0), Some(Forecast::CompletelyOutmatched));
        assert_eq!(seen(40.0), Some(Forecast::MuchWeaker));
        assert_eq!(seen(70.0), Some(Forecast::SlightlyWeaker));
        assert_eq!(seen(100.0), Some(Forecast::Even));
        assert_eq!(seen(120.0), Some(Forecast::Even));
        assert_eq!(seen(130.0), Some(Forecast::SlightlyStronger));
        assert_eq!(seen(250.0), Some(Forecast::MuchStronger));
        assert_eq!(seen(500.0), Some(Forecast::OtherOutmatched));
    }
}
