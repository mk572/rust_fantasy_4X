//! What a city builds next, chosen by the AI (`docs/design.md`, the rule for a finished item).
//!
//! A finished item leaves the city with nothing in hand. At the start of its turn the AI looks at
//! the cities with nothing, Housing or Trade Goods (never at a standing order) and picks the next
//! item here: the garrison first, then the basics (gold, production, food), then settlers, then
//! what the plan of the city asks for. The plan is the AI's own (`economy::plan_for`); the
//! auto-builder of the core is a helper for the human player and the AI does not use it.
//!
//! Engineers and ships are not chosen here: their own modules order them (`road`, `naval`), so
//! that nothing builds them as a side effect of ranking units.

use f4x_core::city::autobuild::{garrison_minimum, BuildPlan, Focus, Mix};
use f4x_core::city::production::ProductionItem;
use f4x_core::player::view::PlayerView;
use f4x_def::half_int::HalfInt;
use f4x_def::rules::effects::Effect;
use f4x_def::rules::{Ability, BuildingId, ModOp, MovementType, Stat, UnitDef, UnitId, UnitKind};

/// Net gold per turn below which a city builds toward gold first.
const GOLD_FLOOR: i32 = 3;
/// Gold buildings are the first of the basics only below this net; above it production and food
/// come first.
const RICH_ENOUGH: i32 = 8;
/// Weight of the unit's price in its score: the square root of the cost over its life.
const UPKEEP_HORIZON: f32 = 100.0;

/// What the city needs to know besides its plan.
#[derive(Clone, Copy)]
pub struct Need {
    /// The treasury's net gold per turn.
    pub net_gold: HalfInt,
    /// A rival wizard's city is near.
    pub rival_near: bool,
}

/// Whether production in this city is for the AI to decide at the start of its turn: nothing in
/// hand, Housing or Trade Goods. A standing order is left alone.
pub fn is_open(item: Option<ProductionItem>) -> bool {
    matches!(
        item,
        None | Some(ProductionItem::Housing | ProductionItem::TradeGoods)
    )
}

fn stat(d: &UnitDef, s: Stat) -> f32 {
    d.stats.get(&s).map_or(0.0, |v| v.as_f32())
}

/// What the building adds to the stats: a flat value as it is, a percentage as a tenth of it (half
/// as much again on a city's gold is worth about five points).
fn effect_total(view: &PlayerView, b: BuildingId, wanted: &[Stat]) -> f32 {
    view.content()
        .rules
        .buildings
        .get(b)
        .effects
        .iter()
        .filter_map(|r| match r.effect {
            Effect::Modify {
                stat,
                op: ModOp::Add,
                value,
            } if wanted.contains(&stat) => Some(value.as_f32()),
            Effect::Modify {
                stat,
                op: ModOp::Percent,
                value,
            } if wanted.contains(&stat) => Some(value.as_f32() / 10.0),
            _ => None,
        })
        .sum()
}

/// The best building of those the city may start that adds one of the stats, by what it adds for
/// the price; buildings the treasury cannot carry are left out. When none can be started yet, the
/// building it waits for (a marketplace asks for a builders hall) is the one to build.
fn best_adding(
    view: &PlayerView,
    city: hecs::Entity,
    need: Need,
    stats: &[Stat],
) -> Option<BuildingId> {
    let rules = &view.content().rules;
    let affordable =
        |d: &f4x_def::rules::BuildingDef| need.net_gold >= HalfInt::from_int(d.upkeep_gold as i32);
    let startable = |b: BuildingId| view.can_build(city, ProductionItem::Building(b)).is_ok();
    let gain_per_cost = |b: BuildingId, cost: u32| {
        let gain = effect_total(view, b, stats);
        (gain > 0.0).then(|| gain / (cost.max(1) as f32).sqrt())
    };
    let best = |v: Vec<(BuildingId, f32)>| {
        v.into_iter()
            .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
            .map(|(b, _)| b)
    };
    let direct: Vec<(BuildingId, f32)> = rules
        .buildings
        .iter()
        .filter(|&(b, d)| affordable(d) && startable(b))
        .filter_map(|(b, d)| gain_per_cost(b, d.cost).map(|g| (b, g)))
        .collect();
    if let Some(b) = best(direct) {
        return Some(b);
    }
    // Nothing to start that adds the stat: what is missing for one that does.
    let opening: Vec<(BuildingId, f32)> = rules
        .buildings
        .iter()
        .filter(|&(b, d)| affordable(d) && !startable(b))
        .filter_map(|(b, d)| {
            let own = gain_per_cost(b, d.cost)?;
            Some(
                d.requires
                    .iter()
                    .copied()
                    .filter(|&r| startable(r) && affordable(rules.buildings.get(r)))
                    .map(move |r| (r, own)),
            )
        })
        .flatten()
        .collect();
    best(opening)
}

fn lifetime_cost(d: &UnitDef) -> f32 {
    let upkeep = d.upkeep.gold.as_f32() + 0.5 * d.upkeep.food.as_f32() + d.upkeep.mana as f32;
    (d.cost.max(1) as f32 + UPKEEP_HORIZON * upkeep).max(1.0)
}

/// A unit the city may garrison with or raise an army from: it fights, walks and is neither a
/// settler nor an engineer nor a ship.
fn is_soldier(d: &UnitDef) -> bool {
    d.kind == UnitKind::Normal
        && d.cost > 0
        && d.movement != MovementType::Sailing
        && !d.abilities.contains(&Ability::new(12))
        && !d.abilities.contains(&Ability::new(42 /* meld */))
        && stat(d, Stat::Construction) <= 0.0
        && stat(d, Stat::Melee).max(stat(d, Stat::Ranged)) > 0.0
}

fn soldiers(view: &PlayerView, city: hecs::Entity) -> Vec<UnitId> {
    view.content()
        .rules
        .units
        .iter()
        .filter(|&(_, d)| is_soldier(d))
        .map(|(u, _)| u)
        .filter(|&u| view.can_build(city, ProductionItem::Unit(u)).is_ok())
        .collect()
}

/// The soldier that is best for the price: `strength / sqrt(cost over its life)`. An elite army
/// takes the dear ones (a tenth power of the cost only).
fn best_soldier(view: &PlayerView, pool: &[UnitId], elite: bool, walks: bool) -> Option<UnitId> {
    let rules = &view.content().rules;
    pool.iter()
        .copied()
        .map(|u| {
            let d = rules.units.get(u);
            let power = f4x_core::battle::power::def_power(d);
            // An army walks to the fight, a garrison does not.
            let reach = if walks {
                1.0 + 0.15 * (f4x_core::unit::mobility::def_mobility(d) - 1.0).max(0.0)
            } else {
                1.0
            };
            let cost = lifetime_cost(d);
            let price = if elite { cost.powf(0.1) } else { cost.sqrt() };
            (u, power * reach / price)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(u, _)| u)
}

/// The cheapest settlers the city may build.
fn cheapest_settler(view: &PlayerView, city: hecs::Entity) -> Option<UnitId> {
    let rules = &view.content().rules;
    rules
        .units
        .iter()
        .filter(|(_, d)| d.abilities.contains(&Ability::new(12)))
        .filter(|&(u, _)| view.can_build(city, ProductionItem::Unit(u)).is_ok())
        .min_by_key(|&(u, d)| (d.cost, u))
        .map(|(u, _)| u)
}

/// The buildings the city may start and the treasury carries, in the order of the rules'
/// `autobuild_order` (those not listed after, in the order of the content).
fn listed_buildings(view: &PlayerView, city: hecs::Entity, need: Need) -> Vec<BuildingId> {
    let rules = &view.content().rules;
    let order = &rules.config.autobuild_order;
    let place = |b: BuildingId| order.iter().position(|&o| o == b).unwrap_or(usize::MAX);
    let mut v: Vec<BuildingId> = rules
        .buildings
        .iter()
        .filter(|&(_, d)| need.net_gold >= HalfInt::from_int(d.upkeep_gold as i32))
        .map(|(b, _)| b)
        .filter(|&b| view.can_build(city, ProductionItem::Building(b)).is_ok())
        .collect();
    v.sort_by_key(|&b| (place(b), b));
    v
}

/// The miners guild: it adds half to the gold, mana and ore savings of the tiles in the city's area.
const MINERS_GUILD: BuildingId = BuildingId::new(34); // miners_guild

/// Land that gives production (forests, hills, mountains, desert) is a specialty only when there is
/// a good deal of it: a point of worth for each tile's 3%, and nothing below [`LAND_PRODUCTION_MIN`].
const LAND_PRODUCTION_MIN: f32 = 12.0;

/// A river square gives two food: a specialty in food from this many of them, half a point each.
const RIVERS_MIN: u32 = 4;

fn river_worth(rivers: u32) -> f32 {
    if rivers >= RIVERS_MIN {
        rivers as f32 * 0.5
    } else {
        0.0
    }
}

fn terrain_production_worth(percent: f32) -> f32 {
    if percent >= LAND_PRODUCTION_MIN {
        percent / 3.0
    } else {
        0.0
    }
}

/// What the special tiles of a city make worth building, the strongest first: gold and mana from
/// ore and gems (gold and mana buildings, and the miners guild), wild game (food buildings), coal
/// and iron (production buildings and the miners guild), nightshade (the buildings that let a
/// city use it). `None` when the city has no special tile or has built what there is to build.
fn specialty(view: &PlayerView, city: hecs::Entity, need: Need) -> Option<BuildingId> {
    let sp = view.city_specials(city)?;
    let startable = |b: BuildingId| {
        view.can_build(city, ProductionItem::Building(b)).is_ok()
            && need.net_gold
                >= HalfInt::from_int(view.content().rules.buildings.get(b).upkeep_gold as i32)
    };
    let miners = startable(MINERS_GUILD).then_some(MINERS_GUILD);
    let nightshade = || {
        f4x_core::city::nightshade::NIGHTSHADE_BUILDINGS
            .iter()
            .copied()
            .filter(|&b| startable(b))
            .min_by_key(|&b| (view.content().rules.buildings.get(b).cost, b))
    };
    // The worth of each special, in points: a point of gold or mana, a food, a tenth of a percent
    // off the unit cost, two points a nightshade plant.
    let mut wants: Vec<(f32, Option<BuildingId>)> = vec![
        (
            sp.gold,
            best_adding(view, city, need, &[Stat::CityGold]).or(miners),
        ),
        (
            sp.mana,
            best_adding(view, city, need, &[Stat::CityMana]).or(miners),
        ),
        (
            sp.food + river_worth(sp.rivers),
            best_adding(
                view,
                city,
                need,
                &[Stat::CityFood, Stat::CityFoodFlat, Stat::FarmerFood],
            ),
        ),
        (
            sp.unit_cost_percent / 5.0 + terrain_production_worth(sp.terrain_production),
            best_adding(view, city, need, &[Stat::CityProduction]).or(miners),
        ),
        (sp.nightshade as f32 * 2.0, nightshade()),
    ];
    wants.retain(|&(worth, b)| worth > 0.0 && b.is_some());
    wants.sort_by(|a, b| b.0.total_cmp(&a.0));
    wants.first().and_then(|&(_, b)| b)
}

/// What the city builds next. `defenders` is the number of soldiers standing in it.
pub fn next_item(
    view: &PlayerView,
    city: hecs::Entity,
    plan: &BuildPlan,
    need: Need,
    defenders: u32,
) -> ProductionItem {
    let eco = view.city_economy(city).unwrap_or_default();
    let units_ok = plan.mix != Mix::Buildings;
    let buildings_ok = plan.mix != Mix::Units;
    let pool = soldiers(view, city);

    // 1. The garrison.
    let mut minimum = garrison_minimum(eco.figures);
    if minimum > 0 {
        minimum += u32::from(need.rival_near);
        match (plan.primary, plan.secondary) {
            (Focus::Defense, _) => minimum += 2,
            (_, Some(Focus::Defense)) => minimum += 1,
            _ => {}
        }
    }
    if units_ok && defenders < minimum {
        if let Some(u) = best_soldier(view, &pool, false, false) {
            return ProductionItem::Unit(u);
        }
    }

    // 2. Unrest.
    if eco.rebels > 0 {
        if let Some(b) = best_adding_unrest(view, city, need) {
            return ProductionItem::Building(b);
        }
    }

    // 3. The basics, before anything the plan wants: gold when the treasury runs short, then
    // production, food where the city is stuck, and gold again.
    if buildings_ok {
        if need.net_gold < HalfInt::from_int(GOLD_FLOOR) {
            if let Some(b) = best_adding(view, city, need, &[Stat::CityGold]) {
                return ProductionItem::Building(b);
            }
        }
        let stuck = eco.food <= HalfInt::ZERO || eco.figures + 1 >= eco.max_size;
        let basics: [(&[Stat], bool); 3] = [
            (
                &[Stat::CityFood, Stat::CityFoodFlat, Stat::FarmerFood],
                stuck,
            ),
            (&[Stat::CityProduction], true),
            (
                &[Stat::CityGold],
                need.net_gold < HalfInt::from_int(RICH_ENOUGH),
            ),
        ];
        for (stats, wanted) in basics {
            if wanted {
                if let Some(b) = best_adding(view, city, need, stats) {
                    return ProductionItem::Building(b);
                }
            }
        }
    }

    // 4. Settlers, where the plan lets this city build them.
    if units_ok && plan.settlers && eco.figures >= plan.min_settler_pop {
        if let Some(u) = cheapest_settler(view, city) {
            return ProductionItem::Unit(u);
        }
    }

    // 4b. The specialty: a city with special tiles builds what makes more of them.
    if buildings_ok {
        if let Some(b) = specialty(view, city, need) {
            return ProductionItem::Building(b);
        }
    }

    // 5. What the plan asks for: soldiers when the army is short, buildings otherwise.
    let building = buildings_ok
        .then(|| listed_buildings(view, city, need).first().copied())
        .flatten()
        .map(ProductionItem::Building);
    let unit = units_ok
        .then(|| best_soldier(view, &pool, plan.elite_units, true))
        .flatten()
        .map(ProductionItem::Unit);
    let pick = match plan.mix {
        Mix::Units => unit.or(building),
        Mix::Buildings | Mix::Both => building.or(unit),
    };
    if let Some(item) = pick {
        return item;
    }

    // 6. Nothing left: housing where the city can still grow, else trade goods.
    if eco.figures < eco.max_size {
        ProductionItem::Housing
    } else {
        ProductionItem::TradeGoods
    }
}

/// A building that lowers unrest, the cheapest of those the city may start.
fn best_adding_unrest(view: &PlayerView, city: hecs::Entity, need: Need) -> Option<BuildingId> {
    let rules = &view.content().rules;
    rules
        .buildings
        .iter()
        .filter(|&(_, d)| need.net_gold >= HalfInt::from_int(d.upkeep_gold as i32))
        .filter(|&(b, _)| view.can_build(city, ProductionItem::Building(b)).is_ok())
        .filter(|&(b, _)| {
            rules.buildings.get(b).effects.iter().any(|r| {
                matches!(r.effect, Effect::Modify { stat: Stat::CityUnrest, value, .. } if value < HalfInt::ZERO)
            })
        })
        .min_by_key(|&(b, d)| (d.cost, b))
        .map(|(b, _)| b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn only_nothing_housing_and_trade_goods_are_open() {
        let rules = game(COAST_CITY, 8).content().rules.clone();
        let building = rules.buildings.iter().next().map(|(b, _)| b);
        let unit = rules.units.iter().next().map(|(u, _)| u);
        assert!(is_open(None));
        assert!(is_open(Some(ProductionItem::Housing)));
        assert!(is_open(Some(ProductionItem::TradeGoods)));
        assert!(!is_open(building.map(ProductionItem::Building)));
        assert!(!is_open(unit.map(ProductionItem::Unit)));
    }

    #[test]
    fn a_garrisoned_city_builds_a_basic_building_before_the_rest_of_the_list() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let city = g
            .economy()
            .city_at(at(COAST_CITY.0, COAST_CITY.1))
            .ok_or("a city on the coast")?;
        spawn(&mut g, P0, at(COAST_CITY.0, COAST_CITY.1), 104, 2);
        let view = PlayerView::new(&g, P0);
        let plan = BuildPlan::default();
        let item = next_item(
            &view,
            city,
            &plan,
            Need {
                net_gold: HalfInt::from_int(5),
                rival_near: false,
            },
            4,
        );
        let ProductionItem::Building(b) = item else {
            return Err(format!("a building was expected: {item:?}").into());
        };
        // A basic: it adds gold, production or food (the list of the rules comes after).
        let basics = [
            Stat::CityGold,
            Stat::CityProduction,
            Stat::CityFood,
            Stat::CityFoodFlat,
            Stat::FarmerFood,
        ];
        let rules = &view.content().rules;
        assert!(
            effect_total(&view, b, &basics) > 0.0,
            "{}",
            rules.buildings.get(b).id
        );
        Ok(())
    }

    #[test]
    fn a_city_with_a_gold_tile_specializes_in_what_adds_gold_and_one_without_does_not() -> TestResult
    {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let city = g
            .economy()
            .city_at(at(COAST_CITY.0, COAST_CITY.1))
            .ok_or("a city on the coast")?;
        let need = Need {
            net_gold: HalfInt::from_int(10),
            rival_near: false,
        };
        assert_eq!(specialty(&PlayerView::new(&g, P0), city, need), None);
        let gold_ore = g.content().features.id_numbered(10 /* gold_ore */);
        let mut map = g.map().ok_or("the game has a map")?.clone();
        map.plane_mut(f4x_core::map::Plane::Arcanus).set_mineral(
            COAST_CITY.0,
            COAST_CITY.1 + 2,
            gold_ore,
        );
        g.set_map(map);
        let view = PlayerView::new(&g, P0);
        let b = specialty(&view, city, need).ok_or("a gold tile makes a specialty")?;
        let rules = &view.content().rules;
        let adds_gold = |x: BuildingId| effect_total(&view, x, &[Stat::CityGold]) > 0.0;
        let opens_gold = rules
            .buildings
            .iter()
            .any(|(o, d)| d.requires.contains(&b) && adds_gold(o));
        assert!(
            adds_gold(b) || opens_gold || b == MINERS_GUILD,
            "{}",
            rules.buildings.get(b).id
        );
        Ok(())
    }

    /// A city whose area gets the special `change` puts on the map, and what it specializes in.
    fn specialty_after(
        change: impl Fn(&mut f4x_core::map::Grid, u32, u32),
    ) -> Result<(Option<BuildingId>, f4x_core::game::Game), Box<dyn std::error::Error>> {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let city = g
            .economy()
            .city_at(at(COAST_CITY.0, COAST_CITY.1))
            .ok_or("a city on the coast")?;
        let mut map = g.map().ok_or("the game has a map")?.clone();
        change(
            map.plane_mut(f4x_core::map::Plane::Arcanus),
            COAST_CITY.0,
            COAST_CITY.1,
        );
        g.set_map(map);
        let need = Need {
            net_gold: HalfInt::from_int(10),
            rival_near: false,
        };
        let b = specialty(&PlayerView::new(&g, P0), city, need);
        Ok((b, g))
    }

    #[test]
    fn mana_crystals_make_a_city_build_what_adds_mana() -> TestResult {
        let crystal = game(COAST_CITY, 8)
            .content()
            .features
            .id_numbered(17 /* quork_crystal */);
        let (b, g) = specialty_after(|grid, x, y| grid.set_mineral(x, y + 2, crystal))?;
        let b = b.ok_or("a mana tile makes a specialty")?;
        let view = PlayerView::new(&g, P0);
        let rules = &view.content().rules;
        let adds = |x: BuildingId| effect_total(&view, x, &[Stat::CityMana]) > 0.0;
        assert!(
            adds(b)
                || b == MINERS_GUILD
                || rules
                    .buildings
                    .iter()
                    .any(|(o, d)| d.requires.contains(&b) && adds(o)),
            "{}",
            rules.buildings.get(b).id
        );
        Ok(())
    }

    #[test]
    fn rivers_make_a_city_build_what_adds_food() -> TestResult {
        let (b, g) = specialty_after(|grid, x, y| {
            for dx in 0..RIVERS_MIN {
                grid.set_river(x + dx - 1, y + 1, true);
            }
        })?;
        let b = b.ok_or("rivers make a specialty")?;
        let view = PlayerView::new(&g, P0);
        let rules = &view.content().rules;
        let food = [Stat::CityFood, Stat::CityFoodFlat, Stat::FarmerFood];
        let adds = |x: BuildingId| effect_total(&view, x, &food) > 0.0;
        assert!(
            adds(b)
                || rules
                    .buildings
                    .iter()
                    .any(|(o, d)| d.requires.contains(&b) && adds(o)),
            "{}",
            rules.buildings.get(b).id
        );
        Ok(())
    }
}
