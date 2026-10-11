//! Research, spells and what the cities build.

use f4x_core::city::Position;
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
        let net = game.treasury(me).net_gold();
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
    let idle = game
        .magic(ai.player)
        .is_some_and(|m| m.researching.is_none());
    if !idle {
        return;
    }
    if let Some(&spell) = game.research_choices(ai.player).first() {
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
    let Some(magic) = game.magic(ai.player) else {
        return;
    };
    if magic.casting.is_some() {
        return;
    }
    let available = magic
        .skill_left
        .min(game.player(ai.player).map_or(0, |p| p.mana))
        .max(0);
    let rules = &game.content.rules;
    let mut options: Vec<_> = magic
        .known
        .iter()
        .copied()
        .filter(|&s| {
            let d = rules.spells.get(s);
            d.contexts.contains(&CastContext::Overland)
                && d.effects
                    .iter()
                    .any(|r| matches!(r.effect, Effect::Summon { .. }))
        })
        .map(|s| (game.casting_cost(ai.player, s, false), s))
        .filter(|&(cost, _)| cost > 0 && cost <= available)
        .collect();
    options.sort();
    // Keep a reserve of mana for upkeep: only cast with at least a third left over.
    if let Some(&(cost, spell)) = options.iter().rev().find(|&&(c, _)| c * 3 / 2 <= available) {
        let _ = cost;
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
    let mut mine: Vec<_> = game
        .cities()
        .into_iter()
        .filter(|(_, c, _)| c.owner == me)
        .map(|(e, _, p)| (e, p))
        .collect();
    game.sort_by_serial(&mut mine.iter().map(|(e, _)| *e).collect::<Vec<_>>());
    mine.sort_by_key(|(e, _)| game.serial_of(*e));
    let settlers_in_play = settlers_alive(game, me)
        + mine
            .iter()
            .filter(|(e, _)| {
                matches!(
                    game.city_production(*e).item,
                    Some(ProductionItem::Unit(u)) if is_settler_def(game, u)
                )
            })
            .count();
    let mut settlers_planned = settlers_in_play;
    for (city, pos) in mine {
        // Housing and trade goods never finish: look again every turn.
        let busy = matches!(
            game.city_production(city).item,
            Some(ProductionItem::Building(_) | ProductionItem::Unit(_))
        );
        if busy {
            continue;
        }
        let item = choose(game, me, city, pos, mine_count(game, me), &mut settlers_planned);
        if let Some(item) = item {
            ai.send(game, Command::SetProduction { city, item }, events);
        }
    }
    spend_gold(ai, game, events);
}

/// Spare food after feeding the citizens and the army.
fn spare_food(game: &Game, me: f4x_core::city::PlayerId) -> i32 {
    let upkeep: i32 = game
        .player_units(me)
        .iter()
        .map(|&u| game.unit_food_upkeep(u))
        .sum();
    game.treasury(me).food_surplus - upkeep
}

/// Buy what is being built when the treasury has gold to spare, settlers and the first
/// guard of each city before the rest.
fn spend_gold(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    const RESERVE: i32 = 40;
    let mut wanted: Vec<(u32, u32, hecs::Entity)> = Vec::new();
    for (city, c, pos) in game.cities() {
        if c.owner != me {
            continue;
        }
        let Some(item) = game.city_production(city).item else {
            continue;
        };
        let rank = match item {
            ProductionItem::Unit(u) if is_settler_def(game, u) => 0,
            ProductionItem::Unit(_) if game.garrison(pos).is_empty() => 1,
            ProductionItem::Building(_) => 2,
            ProductionItem::Unit(_) => 3,
            _ => continue,
        };
        if let Some(cost) = game.buy_cost(city) {
            wanted.push((rank, cost, city));
        }
    }
    wanted.sort_by_key(|&(rank, cost, city)| (rank, cost, game.serial_of(city)));
    for (_, cost, city) in wanted {
        let gold = game.player(me).map_or(0, |p| p.gold);
        if gold - cost as i32 >= RESERVE {
            ai.send(game, Command::BuyProduction { city }, events);
        }
    }
}

/// Real cities, not outposts.
fn mine_count(game: &Game, me: f4x_core::city::PlayerId) -> usize {
    game.cities()
        .iter()
        .filter(|(e, c, _)| c.owner == me && game.city_economy(*e).is_some_and(|x| x.figures > 0))
        .count()
}

fn settlers_alive(game: &Game, me: f4x_core::city::PlayerId) -> usize {
    game.player_units(me)
        .into_iter()
        .filter(|&u| game.unit_has_ability(u, "create_outpost"))
        .count()
}

fn is_settler_def(game: &Game, unit: f4x_core::rules::UnitId) -> bool {
    game.content
        .rules
        .units
        .get(unit)
        .abilities
        .iter()
        .any(|a| a == "create_outpost")
}

/// The best fighting unit a city can build, by strength for the price.
fn best_fighter(game: &Game, city: hecs::Entity) -> Option<ProductionItem> {
    let rules = &game.content.rules;
    rules
        .units
        .iter()
        .filter(|(_, d)| d.kind == UnitKind::Normal && !d.abilities.iter().any(|a| a == "create_outpost"))
        .filter(|(id, d)| {
            d.cost > 0
                && game.can_build(city, ProductionItem::Unit(*id)).is_ok()
                && (d.stats.get(&f4x_core::rules::Stat::Melee).copied().unwrap_or(0)
                    + d.stats.get(&f4x_core::rules::Stat::Ranged).copied().unwrap_or(0))
                    > 0
        })
        .max_by(|(_, a), (_, b)| {
            let value = |d: &f4x_core::rules::UnitDef| {
                let s = |k| d.stats.get(&k).copied().unwrap_or(0) as f32;
                let power = (s(f4x_core::rules::Stat::Melee).max(s(f4x_core::rules::Stat::Ranged)) * 1.5
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

fn settler_item(game: &Game, city: hecs::Entity) -> Option<ProductionItem> {
    let rules = &game.content.rules;
    rules
        .units
        .iter()
        .filter(|(_, d)| d.abilities.iter().any(|a| a == "create_outpost"))
        .map(|(id, _)| ProductionItem::Unit(id))
        .find(|&i| game.can_build(city, i).is_ok())
}

fn choose(
    game: &Game,
    me: f4x_core::city::PlayerId,
    city: hecs::Entity,
    pos: Position,
    cities: usize,
    settlers_planned: &mut usize,
) -> Option<ProductionItem> {
    let size = game.city_economy(city).map_or(0, |e| e.figures);
    // An outpost builds nothing: housing is what makes it grow into a city.
    if size == 0 {
        return Some(ProductionItem::Housing);
    }
    let garrison = game
        .garrison(pos)
        .into_iter()
        .filter(|&u| is_fighter(game, u))
        .count();
    let net = game.treasury(me).net_gold();
    let broke = net < 0 && game.player(me).is_some_and(|p| p.gold < 60);
    // Soldiers and settlers each eat one food; build none the land cannot feed.
    let fighter = best_fighter(game, city).filter(|_| spare_food(game, me) >= 1 || garrison == 0);
    // 1. Never leave a city empty.
    if garrison == 0 && !broke {
        if let Some(f) = fighter {
            return Some(f);
        }
    }
    // 2. Grow: settlers while there is room for more cities.
    let all_cities = game.cities().iter().filter(|(_, c, _)| c.owner == me).count();
    if all_cities + *settlers_planned < WANTED_CITIES
        && size >= 3
        && !broke
        && spare_food(game, me) >= 0
    {
        if let Some(s) = settler_item(game, city) {
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
        let rules = &game.content.rules;
        for id in BUILD_ORDER {
            if let Some(b) = rules.buildings.id_of(id) {
                let item = ProductionItem::Building(b);
                if game.can_build(city, item).is_ok() {
                    return Some(item);
                }
            }
        }
    }
    // 5. An army, within what the treasury pays for.
    let army = game
        .player_units(me)
        .into_iter()
        .filter(|&u| is_fighter(game, u))
        .count();
    if net >= 1 && army < cities * 3 + 2 {
        if let Some(f) = fighter {
            return Some(f);
        }
    }
    Some(ProductionItem::TradeGoods)
}
