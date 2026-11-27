//! Class 6: a rule setting holds over the turns. What a setting does to one operation of the world is
//! in core (`c6_setting.rs`).

use crate::support::games;

/// `max_city_size`: whatever the setting, a city that starts at its edge never grows past it over the
/// turns, and the world still holds together (the checker has the same rule).
#[test]
fn a_city_stops_growing_at_its_setting() {
    for setting in f4x_def::settings::MAX_CITY_SIZE_RANGE {
        let (_, mut w) = games().remove(0);
        w.scenario_change_settings(|s| s.rules.max_city_size = setting);
        let cities: Vec<_> = w.economy().cities().into_iter().map(|c| c.0).collect();
        for &c in &cities {
            w.scenario_set_citizens(c, u32::from(setting) - 1);
        }
        for _ in 0..15 {
            crate::support::busy_round(&mut w, 0);
            for &c in &cities {
                let size = w.economy().city_economy(c).map(|e| e.figures);
                assert!(
                    size.is_none_or(|n| n <= u32::from(setting)),
                    "setting {setting}: a city of {size:?}"
                );
            }
        }
        let bad = f4x_core::check::world_problems(&w);
        assert!(bad.is_empty(), "setting {setting}: {bad:#?}");
    }
}
