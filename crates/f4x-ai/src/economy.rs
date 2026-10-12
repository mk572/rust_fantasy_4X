//! Research, spells and what the cities build.

use f4x_core::city::Position;
use f4x_core::player_view::PlayerView;
use f4x_core::production::ProductionItem;
use f4x_core::rules::effects::Effect;
use f4x_core::rules::{CastContext, UnitKind};
use f4x_core::{Command, Event, Game};

use crate::view::is_fighter;
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

/// The most cities the AI settles before it only builds up.
const WANTED_CITIES: usize = 7;
/// Fighters each city should have at home.
const GARRISON: usize = 2;

/// The lowest tax rate (up to a limit) that leaves the treasury a small surplus, else the
/// rate that earns the most. Rebels from high taxes are counted by the treasury forecast.
pub fn taxes(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    const MAX_RATE: u8 = 3;
    const WANTED_NET: i32 = 3;
    let me = ai.player;
    let mut best: Option<(i32, u8)> = None;
    let mut chosen = None;
    for rate in 0..=MAX_RATE {
        game.apply(Command::SetTaxRate { player: me, rate });
        let net = PlayerView::new(game, me).treasury().net_gold();
        if best.is_none_or(|(n, _)| net > n) {
            best = Some((net, rate));
        }
        if net >= WANTED_NET {
            chosen = Some(rate);
            break;
        }
    }
    let rate = chosen.or(best.map(|(_, r)| r)).unwrap_or(1);
    ai.send(game, Command::SetTaxRate { player: me, rate }, events);
}

/// Pick the cheapest spell to research when nothing is being researched.
pub fn research(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let spell = {
        let view = PlayerView::new(game, ai.player);
        if !view.magic().is_some_and(|m| m.researching.is_none()) {
            return;
        }
        view.research_choices().first().copied()
    };
    if let Some(spell) = spell {
        ai.send(
            game,
            Command::SetResearch {
                player: ai.player,
                spell,
            },
            events,
        );
    }
}

/// Cast the dearest summoning spell that is paid for this turn.
pub fn cast(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let spell = {
        let view = PlayerView::new(game, ai.player);
        let Some(magic) = view.magic() else {
            return;
        };
        if magic.casting.is_some() {
            return;
        }
        let available = magic
            .skill_left
            .min(view.player().map_or(0, |p| p.mana))
            .max(0);
        let mut options: Vec<_> = magic
            .known
            .iter()
            .copied()
            .filter(|&s| {
                let d = view.spell(s);
                d.contexts.contains(&CastContext::Overland)
                    && d.effects
                        .iter()
                        .any(|r| matches!(r.effect, Effect::Summon { .. }))
            })
            .map(|s| (view.casting_cost(s), s))
            .filter(|&(cost, _)| cost > 0 && cost <= available)
            .collect();
        options.sort();
        // Keep a reserve of mana for upkeep: only cast with at least a third left over.
        options
            .iter()
            .rev()
            .find(|&&(c, _)| c * 3 / 2 <= available)
            .map(|&(_, spell)| spell)
    };
    if let Some(spell) = spell {
        ai.send(
            game,
            Command::CastSpell {
                player: ai.player,
                spell,
                target: None,
            },
            events,
        );
    }
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
        let item = {
            let view = PlayerView::new(game, me);
            // Housing and trade goods never finish: look again every turn.
            let busy = matches!(
                view.city_production(city).item,
                Some(ProductionItem::Building(_) | ProductionItem::Unit(_))
            );
            if busy {
                continue;
            }
            choose(&view, city, pos, mine_count(&view), &mut settlers_planned)
        };
        if let Some(item) = item {
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

/// Real cities, not outposts.
fn mine_count(view: &PlayerView) -> usize {
    view.my_cities()
        .iter()
        .filter(|(e, _)| view.city_economy(*e).is_some_and(|x| x.figures > 0))
        .count()
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
                power / d.cost as f32
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
    cities: usize,
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
    let net = view.treasury().net_gold();
    let broke = net < 0 && view.player().is_some_and(|p| p.gold < 60);
    // Soldiers and settlers each eat one food; build none the land cannot feed.
    let fighter = best_fighter(view, city).filter(|_| spare_food(view) >= 1 || garrison == 0);
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
    // 3. Two soldiers in every city once there is more than one.
    if garrison < GARRISON && cities >= 2 && !broke {
        if let Some(f) = fighter {
            return Some(f);
        }
    }
    // 4. Buildings, while the treasury can bear their upkeep.
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
    // 5. An army, within what the treasury pays for.
    let army = view
        .my_units()
        .into_iter()
        .filter(|&u| is_fighter(view, u))
        .count();
    if net >= 1 && army < cities * 3 + 2 {
        if let Some(f) = fighter {
            return Some(f);
        }
    }
    Some(ProductionItem::TradeGoods)
}
