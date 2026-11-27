//! Forge cities (task 502): from the second half of the game a few of the wizard's best cities
//! build out toward the race's top unit and then make little else, so that the field army is a
//! few strong units and not a crowd of spearmen. Cheap units stay as garrisons; the strong ones
//! are what clears a tower or storms a rival's city.

use f4x_core::city::production::{ProductionError, ProductionItem};
use f4x_core::player::view::PlayerView;
use f4x_def::rules::{BuildingId, UnitId, UnitKind};

use crate::phase::{phase, Phase};

/// The turn from which the forge cities are chosen (earlier, a wizard still grows).
const FORGE_TURN: u32 = 150;
/// One forge city for this many cities, at least two and at most [`MOST_FORGES`].
const CITIES_PER_FORGE: usize = 5;
const MOST_FORGES: usize = 4;
/// Gold above which a conqueror's forges grow to half its cities, at most [`MOST_FORGES_RICH`].
const RICH_GOLD: i32 = 3000;
const MOST_FORGES_RICH: usize = 12;

/// Is the wizard past the build-up, so that its cities work toward the strong units?
pub(crate) fn active(view: &PlayerView, round: u32) -> bool {
    round >= FORGE_TURN && phase(view, round) != Phase::Build
}

/// How many forge cities a wizard of `cities` cities has: one for [`CITIES_PER_FORGE`], two at
/// least; a rich conqueror forges in half its cities.
fn forge_count(cities: usize, rich: bool) -> usize {
    if rich {
        (cities / 2).clamp(2, MOST_FORGES_RICH)
    } else {
        (cities / CITIES_PER_FORGE).clamp(2, MOST_FORGES)
    }
}

/// The forge cities: the biggest cities (the most production), the largest first; none before the
/// wizard is past its build-up.
pub(crate) fn forges(view: &PlayerView, round: u32) -> Vec<hecs::Entity> {
    if !active(view, round) {
        return Vec::new();
    }
    let mut cities: Vec<(u32, hecs::Entity)> = view
        .my_cities()
        .into_iter()
        .filter_map(|(c, _)| {
            let size = view.city_economy(c)?.figures;
            (size > 0).then_some((size, c))
        })
        .collect();
    cities.sort_by_key(|&(size, c)| (std::cmp::Reverse(size), view.serial_of(c)));
    // A conqueror with gold to spare makes the strong units in half its cities.
    let rich = view.treasury().gold.floor() >= RICH_GOLD
        && crate::phase::goal(view) == crate::phase::Goal::Conquest;
    let n = forge_count(cities.len(), rich);
    cities.into_iter().take(n).map(|(_, c)| c).collect()
}

/// The buildings a unit needs, and those they need in turn.
pub(crate) fn chain(view: &PlayerView, unit: UnitId) -> Vec<BuildingId> {
    let rules = &view.content().rules;
    let mut found: Vec<BuildingId> = Vec::new();
    let mut todo: Vec<BuildingId> = rules.units.get(unit).requires.clone();
    while let Some(b) = todo.pop() {
        if found.contains(&b) {
            continue;
        }
        found.push(b);
        todo.extend(rules.buildings.get(b).requires.iter().copied());
    }
    found
}

/// The dearest land fighter of the city's race whose buildings the city can still get (a coastal
/// building is no use to an inland city).
pub(crate) fn top_unit(view: &PlayerView, city: hecs::Entity) -> Option<UnitId> {
    let rules = &view.content().rules;
    let race = rules.races.get(view.city_race(city)?);
    race.units
        .iter()
        .copied()
        .filter(|&u| view.content().rules.units.get(u).is_fighter())
        .filter(|&u| {
            chain(view, u).into_iter().all(|b| {
                !matches!(
                    view.can_build(city, ProductionItem::Building(b)),
                    Err(ProductionError::WrongRace | ProductionError::NotCoastal)
                )
            })
        })
        .max_by_key(|&u| (rules.units.get(u).cost, u))
}

/// A unit is low-tier when it costs this percent of its race's top land unit or less (spearmen
/// and swordsmen against the berserkers or the drakes).
const LOW_TIER_PCT: u32 = 35;

/// Is the unit one of the cheap ones of its race: no use in the field late in the game?
pub(crate) fn low_tier(view: &PlayerView, unit: hecs::Entity) -> bool {
    let Some(def) = view.unit_def(unit) else {
        return false;
    };
    let rules = &view.content().rules;
    let Some(race) = def.race else {
        return false;
    };
    def.kind == UnitKind::Normal
        && rules
            .races
            .get(race)
            .units
            .iter()
            .copied()
            .filter(|&u| view.content().rules.units.get(u).is_fighter())
            .map(|u| rules.units.get(u).cost)
            .max()
            .is_some_and(|top| def.cost * 100 <= LOW_TIER_PCT * top)
}

/// What a forge city does next toward its top unit.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// The unit can be built now.
    Ready(UnitId),
    /// A building on the way to it can be started now.
    Build(BuildingId),
    /// Nothing to do (no top unit, or a building is still waiting for another).
    Wait,
}

/// The next step of a city toward its top unit: the cheapest missing building that can be started,
/// or the unit itself.
pub(crate) fn step(view: &PlayerView, city: hecs::Entity) -> Step {
    let Some(unit) = top_unit(view, city) else {
        return Step::Wait;
    };
    if view.can_build(city, ProductionItem::Unit(unit)).is_ok() {
        return Step::Ready(unit);
    }
    let rules = &view.content().rules;
    chain(view, unit)
        .into_iter()
        .filter(|&b| view.can_build(city, ProductionItem::Building(b)).is_ok())
        .min_by_key(|&b| (rules.buildings.get(b).cost, b))
        .map_or(Step::Wait, Step::Build)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{game, COAST_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn a_young_game_has_no_forge_cities() -> TestResult {
        let w = game(COAST_CITY, 8 /* high_men */);
        assert!(forges(&PlayerView::new(&w, P0), w.round()).is_empty());
        Ok(())
    }

    #[test]
    fn a_city_without_barracks_builds_toward_the_top_unit() -> TestResult {
        let w = game(COAST_CITY, 8 /* high_men */);
        let view = PlayerView::new(&w, P0);
        let (city, _) = view.my_cities()[0];
        let rules = &view.content().rules;
        let top = top_unit(&view, city).ok_or("no top unit")?;
        let cheapest = rules
            .races
            .get(view.city_race(city).ok_or("no race")?)
            .units
            .iter()
            .copied()
            .filter(|&u| view.content().rules.units.get(u).is_fighter())
            .map(|u| rules.units.get(u).cost)
            .min()
            .ok_or("no unit")?;
        assert!(rules.units.get(top).cost > cheapest);
        // A new city has not built what the top unit needs, so it is told to build toward it.
        assert!(matches!(step(&view, city), Step::Build(_) | Step::Ready(_)));
        Ok(())
    }

    #[test]
    fn a_conqueror_with_gold_to_spare_forges_in_more_cities() {
        assert_eq!(forge_count(10, false), 2);
        assert_eq!(forge_count(30, false), MOST_FORGES);
        assert_eq!(forge_count(10, true), 5);
        assert_eq!(forge_count(60, true), MOST_FORGES_RICH);
        assert_eq!(forge_count(3, true), 2, "never fewer than two");
    }
}
