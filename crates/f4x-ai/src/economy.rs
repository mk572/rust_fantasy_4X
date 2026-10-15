//! Research, spells and what the cities build.

use f4x_core::city::Position;
use f4x_core::player_view::PlayerView;
use f4x_core::production::ProductionItem;
use f4x_core::rules::UnitKind;
use f4x_core::{Command, Event, Game};

use crate::view::{garrison_wanted, is_fighter};
use crate::Ai;

/// Buildings in the order the AI wants them, when it can have them.
const BUILD_ORDER: &[&str] = &[
    "builders_hall",
    "granary",
    "barracks",
    "smithy",
    "marketplace",
    "library",
    "temple",
    "sawmill",
    "shrine",
    "farmers_market",
    "sages_guild",
    "city_walls",
    "foresters_guild",
    "miners_guild",
    "armory",
    "bank",
    "university",
];

/// Fighters beyond the home guard the AI wants, before a half per city more.
const STRIKE_BASE: usize = 4;
/// The most cities the AI settles before it only builds up.
const WANTED_CITIES: usize = 7;

/// The lowest tax rate (up to a limit) that leaves the treasury a small surplus, else the
/// rate that earns the most. Rebels from high taxes are counted by the treasury forecast.
pub fn taxes(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    const MAX_RATE: u8 = 3;
    let wanted_net = crate::knob("NET", 3.0) as i32;
    let me = ai.player;
    let mut best: Option<(i32, u8)> = None;
    let mut chosen = None;
    for rate in 0..=MAX_RATE {
        game.apply(Command::SetTaxRate { player: me, rate });
        let net = PlayerView::new(game, me).treasury().net_gold();
        if best.is_none_or(|(n, _)| net > n) {
            best = Some((net, rate));
        }
        if net >= wanted_net {
            chosen = Some(rate);
            break;
        }
    }
    let rate = chosen.or(best.map(|(_, r)| r)).unwrap_or(1);
    ai.send(game, Command::SetTaxRate { player: me, rate }, events);
}

/// Set production in every city that is not building anything.
pub fn cities(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let (mine, mut settlers_planned) = {
        let view = PlayerView::new(game, me);
        let mut mine = view.my_cities();
        mine.sort_by_key(|(e, _)| view.serial_of(*e));
        let in_production = mine
            .iter()
            .filter(|(e, _)| {
                matches!(
                    view.city_production(*e).item,
                    Some(ProductionItem::Unit(u)) if is_settler_def(&view, u)
                )
            })
            .count();
        let planned = settlers_alive(&view) + in_production;
        (mine, planned)
    };
    for (city, pos) in mine {
        let (item, current) = {
            let view = PlayerView::new(game, me);
            let current = view.city_production(city).item;
            // A building is built once and then the city asks again, but a unit is built over and
            // over until the city is told otherwise: look at units again every turn.
            if matches!(current, Some(ProductionItem::Building(_))) {
                continue;
            }
            if matches!(current, Some(ProductionItem::Unit(u)) if is_settler_def(&view, u)) {
                settlers_planned = settlers_planned.saturating_sub(1);
            }
            (
                choose(
                    &view,
                    city,
                    pos,
                    ai.naval.map(|p| p.port).or(ai.sea_port),
                    &mut settlers_planned,
                ),
                current,
            )
        };
        if let Some(item) = item.filter(|&i| Some(i) != current) {
            ai.send(game, Command::SetProduction { city, item }, events);
        }
    }
    spend_gold(ai, game, events);
}

/// Spare food after feeding the citizens and the army.
fn spare_food(view: &PlayerView) -> i32 {
    let upkeep: i32 = view
        .my_units()
        .iter()
        .map(|&u| view.unit_food_upkeep(u))
        .sum();
    view.treasury().food_surplus - upkeep
}

/// Buy what is being built when the treasury has gold to spare, settlers and the first
/// guard of each city before the rest.
fn spend_gold(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    const RESERVE: i32 = 40;
    let wanted = {
        let view = PlayerView::new(game, me);
        let mut wanted: Vec<(u32, u32, hecs::Entity)> = Vec::new();
        for (city, pos) in view.my_cities() {
            let Some(item) = view.city_production(city).item else {
                continue;
            };
            let rank = match item {
                ProductionItem::Unit(u) if is_settler_def(&view, u) => 0,
                ProductionItem::Unit(_) if view.garrison(pos).is_empty() => 1,
                ProductionItem::Building(_) => 2,
                ProductionItem::Unit(_) => 3,
                _ => continue,
            };
            if let Some(cost) = view.buy_cost(city) {
                wanted.push((rank, cost, city));
            }
        }
        wanted.sort_by_key(|&(rank, cost, city)| (rank, cost, view.serial_of(city)));
        wanted
    };
    for (_, cost, city) in wanted {
        let gold = PlayerView::new(game, me).player().map_or(0, |p| p.gold);
        if gold - cost as i32 >= RESERVE {
            ai.send(game, Command::BuyProduction { city }, events);
        }
    }
}

fn settlers_alive(view: &PlayerView) -> usize {
    view.my_units()
        .into_iter()
        .filter(|&u| view.unit_has_ability(u, "create_outpost"))
        .count()
}

fn is_settler_def(view: &PlayerView, unit: f4x_core::rules::UnitId) -> bool {
    view.content()
        .rules
        .units
        .get(unit)
        .abilities
        .iter()
        .any(|a| a == "create_outpost")
}

/// The best fighting unit a city can build, by strength for the price.
fn best_fighter(view: &PlayerView, city: hecs::Entity) -> Option<ProductionItem> {
    let rules = &view.content().rules;
    rules
        .units
        .iter()
        .filter(|(_, d)| {
            d.kind == UnitKind::Normal && !d.abilities.iter().any(|a| a == "create_outpost")
        })
        .filter(|(id, d)| {
            d.cost > 0
                && view.can_build(city, ProductionItem::Unit(*id)).is_ok()
                && (d
                    .stats
                    .get(&f4x_core::rules::Stat::Melee)
                    .copied()
                    .unwrap_or(0)
                    + d.stats
                        .get(&f4x_core::rules::Stat::Ranged)
                        .copied()
                        .unwrap_or(0))
                    > 0
        })
        .max_by(|(_, a), (_, b)| {
            let value = |d: &f4x_core::rules::UnitDef| {
                let s = |k| d.stats.get(&k).copied().unwrap_or(0) as f32;
                let power = (s(f4x_core::rules::Stat::Melee).max(s(f4x_core::rules::Stat::Ranged))
                    * 1.5
                    + s(f4x_core::rules::Stat::Defense)
                    + 1.0)
                    * s(f4x_core::rules::Stat::MaxHp).max(1.0)
                    * s(f4x_core::rules::Stat::Figures).max(1.0);
                power / (d.cost as f32).powf(crate::knob("COSTEXP", 1.0))
            };
            value(a).total_cmp(&value(b))
        })
        .map(|(id, _)| ProductionItem::Unit(id))
}

fn settler_item(view: &PlayerView, city: hecs::Entity) -> Option<ProductionItem> {
    let rules = &view.content().rules;
    rules
        .units
        .iter()
        .filter(|(_, d)| d.abilities.iter().any(|a| a == "create_outpost"))
        .map(|(id, _)| ProductionItem::Unit(id))
        .find(|&i| view.can_build(city, i).is_ok())
}

fn choose(
    view: &PlayerView,
    city: hecs::Entity,
    pos: Position,
    port: Option<Position>,
    settlers_planned: &mut usize,
) -> Option<ProductionItem> {
    let size = view.city_economy(city).map_or(0, |e| e.figures);
    // An outpost builds nothing: housing is what makes it grow into a city.
    if size == 0 {
        return Some(ProductionItem::Housing);
    }
    let garrison = view
        .garrison(pos)
        .into_iter()
        .filter(|&u| is_fighter(view, u))
        .count();
    // A full treasury can bear a deficit: the gold on hand, spread over a hundred turns, counts
    // as income.
    let gold = view.player().map_or(0, |p| p.gold);
    let net = view.treasury().net_gold() + (gold / 100).max(0);
    let broke = net < 0 && gold < 60;
    // Soldiers and settlers each eat one food; build none the land cannot feed.
    let fighter = best_fighter(view, city).filter(|_| spare_food(view) >= 1 || garrison == 0);
    // 0. The port of a landing builds the ship (and what the ship needs) when there is none.
    if port == Some(pos)
        && !broke
        && !view
            .my_units()
            .iter()
            .any(|&u| crate::view::is_ship(view, u))
    {
        if let Some(item) = crate::naval::port_item(view, city) {
            return Some(item);
        }
    }
    // 1. Never leave a city empty.
    if garrison == 0 && !broke {
        if let Some(f) = fighter {
            return Some(f);
        }
    }
    // 2. Grow: settlers while there is room for more cities.
    let all_cities = view.my_cities().len();
    if all_cities + *settlers_planned < WANTED_CITIES
        && size >= 3
        && !broke
        && spare_food(view) >= 0
    {
        if let Some(s) = settler_item(view, city) {
            *settlers_planned += 1;
            return Some(s);
        }
    }
    // 3. The garrison the city should have.
    if garrison < garrison_wanted(view, pos) && !broke {
        if let Some(f) = fighter {
            return Some(f);
        }
    }
    // 4. A strike force before most of the buildings: the wars are won by armies.
    let fighters = view
        .my_units()
        .into_iter()
        .filter(|&u| is_fighter(view, u))
        .count();
    let all = view.my_cities().len();
    let home_guard: usize = view
        .my_cities()
        .iter()
        .map(|&(_, p)| garrison_wanted(view, p))
        .sum();
    let strike = crate::knob("STRIKE", STRIKE_BASE as f32) as usize + all / 2;
    let treasury_ok = net >= 1 && spare_food(view) >= 1;
    if fighters < home_guard + strike && treasury_ok {
        if let Some(f) = fighter {
            return Some(f);
        }
    }
    // 5. Buildings, while the treasury can bear their upkeep.
    if net >= 2 {
        let rules = &view.content().rules;
        for id in BUILD_ORDER {
            if let Some(b) = rules.buildings.id_of(id) {
                let item = ProductionItem::Building(b);
                if view.can_build(city, item).is_ok() {
                    return Some(item);
                }
            }
        }
    }
    // 6. More army, within what the treasury pays for.
    if treasury_ok && fighters < home_guard + strike * 3 {
        if let Some(f) = fighter {
            return Some(f);
        }
    }
    Some(ProductionItem::TradeGoods)
}
