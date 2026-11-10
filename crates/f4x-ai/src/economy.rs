//! Research, spells and what the cities build.

use f4x_core::city::autobuild::{BuildPlan, Focus, Mix};
use f4x_core::city::production::ProductionItem;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_def::half_int::HalfInt;
use f4x_def::rules::Ability;

use crate::expand;
use crate::view::{garrison_wanted, is_fighter};
use crate::Ai;

/// The lowest tax rate (up to a limit) that leaves the treasury a small surplus, else the
/// rate that earns the most. Rebels from high taxes are counted by the treasury forecast.
pub fn taxes(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    const MAX_RATE: u8 = 3;
    let wanted_net = crate::knob("NET", 3.0) as i32;
    let me = ai.player;
    let mut best: Option<(HalfInt, u8)> = None;
    let mut chosen = None;
    for rate in 0..=MAX_RATE {
        game.apply(Command::SetTaxRate { player: me, rate });
        let net = PlayerView::new(game, me).budget().net_gold();
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

/// How many ships the wizard wants. A landing on a big army needs a fleet: one ship per twelve
/// fighters beyond the first, at most [`MAX_SHIPS`].
fn ships_wanted(ai: &Ai, view: &PlayerView) -> usize {
    let base = crate::knob("SHIPS", 1.0) as usize;
    if ai.naval.is_none() || ai.colony.is_some() {
        return base;
    }
    let fighters = view
        .my_units()
        .into_iter()
        .filter(|&u| is_fighter(view, u))
        .count();
    (base + fighters / 12).min(MAX_SHIPS)
}

/// The most ships a landing builds.
const MAX_SHIPS: usize = 4;

/// How a wizard should grow: whether the land has room for another city, and how wide it leans
/// (`phase::width`).
#[derive(Clone, Copy)]
struct Growth {
    room: bool,
    wide: f32,
}

/// What presses on a city's plan: an empty treasury, a rival close by, and how many fighters the
/// campaign wants at the front.
#[derive(Clone, Copy)]
struct Pressure {
    threatened: bool,
    war_force: Option<usize>,
}

/// Gold from which the cities build units to spend it on, tall or not.
const RICH_GOLD: i32 = 1500;

/// Below this the wizard builds up (tall); above [`WIDE_ABOVE`] it spreads out (wide).
const TALL_BELOW: f32 = 0.35;
const WIDE_ABOVE: f32 = 0.65;

/// Settlers a wizard that has all the cities it wanted keeps going while the land has room: more
/// the wider it leans, none when it builds up.
fn more_settlers(wide: f32) -> usize {
    if wide <= TALL_BELOW {
        0
    } else if wide >= WIDE_ABOVE {
        3
    } else {
        2
    }
}

/// How far a rival's city counts as near, for the focus of a city's plan.
const RIVAL_RANGE: u32 = 12;

/// Give every city its auto-builder plan (`docs/auto-builder.md`); the cities then pick what to
/// build by themselves each time they finish something. The AI's own part is the plan: what
/// matters (war, expansion or both), how much of the production goes to the army, and which
/// cities may build settlers. It keeps the port that builds the ship, and buys what it can.
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
    let room = expand::room_for_city(&PlayerView::new(game, me));
    let wide = crate::phase::width(&PlayerView::new(game, me));
    let war_force = ai.campaign.map(|target| {
        let view = PlayerView::new(game, me);
        crate::military::force_wanted(&view, &ai.city_memory, game.turn, target)
    });
    let empire = Empire::of(&PlayerView::new(game, me));
    for (city, pos) in mine {
        let (plan, port_item) = {
            let view = PlayerView::new(game, me);
            let current = view.city_production(city).item;
            if matches!(current, Some(ProductionItem::Unit(u)) if is_settler_def(&view, u)) {
                settlers_planned = settlers_planned.saturating_sub(1);
            }
            let size = view.city_economy(city).map_or(0, |e| e.figures);
            // An outpost builds nothing: housing is what makes it grow into a city.
            if size == 0 {
                if current != Some(ProductionItem::Housing) {
                    ai.send(
                        game,
                        Command::SetProduction {
                            city,
                            item: ProductionItem::Housing,
                        },
                        events,
                    );
                }
                continue;
            }
            let port = ai
                .naval
                .map(|p| p.port)
                .or(ai.sea_port)
                .or(ai.colony.map(|c| c.port));
            let broke = empire.broke();
            // The port of a landing builds the ship (and what the ship needs) when there is none.
            let port_item = (port == Some(pos)
                && !broke
                && !matches!(current, Some(ProductionItem::Building(_))))
            .then(|| {
                crate::naval::port_item(&view, city, ai.naval.is_some() && ai.colony.is_none())
            })
            .flatten()
            // Without a ship yard the best the port can build is a trireme, which takes two:
            // enough for a settler and a guard, so it counts as the ship; otherwise a landing
            // wants a ship that carries a landing party.
            .filter(|&item| {
                let needed = if crate::naval::is_trireme(&view, item) {
                    1
                } else {
                    crate::naval::MIN_LOAD as i32
                };
                view.my_units()
                    .iter()
                    .filter(|&&u| crate::view::is_ship(&view, u))
                    .filter(|&&u| {
                        (ai.naval.is_none() || ai.colony.is_some())
                            || view.unit_stat(u, f4x_def::rules::Stat::Transport) >= needed
                    })
                    .count()
                    < ships_wanted(ai, &view)
            })
            .or_else(|| colonist_item(ai, &view, &empire, city, pos));
            (
                plan_for(
                    &view,
                    &empire,
                    city,
                    pos,
                    Pressure {
                        threatened: ai.threatened.contains(&pos),
                        war_force,
                    },
                    &mut settlers_planned,
                    Growth { room, wide },
                ),
                port_item,
            )
        };
        let wanted = game.autobuild_of(city).map(|a| a.plan);
        if wanted != Some(plan) {
            ai.send(
                game,
                Command::SetAutoBuild {
                    city,
                    plan: Some(plan),
                },
                events,
            );
        }
        if let Some(item) = port_item {
            if game.economy().city_production(city).item != Some(item) {
                ai.send(game, Command::SetProduction { city, item }, events);
            }
        }
    }
    spend_gold(ai, game, events);
}

/// The settler a port builds for a voyage by sea once its ship is there: none while one waits in
/// the port.
fn colonist_item(
    ai: &Ai,
    view: &PlayerView,
    empire: &Empire,
    city: hecs::Entity,
    pos: Position,
) -> Option<ProductionItem> {
    let waiting = view
        .garrison(pos)
        .into_iter()
        .any(|u| view.unit_has_ability(u, Ability::new(12)));
    let ship = view
        .my_units()
        .iter()
        .any(|&u| crate::view::is_ship(view, u));
    let size = view.city_economy(city).map_or(0, |e| e.figures);
    (ai.colony.is_some_and(|c| c.port == pos)
        && ai.colonists.is_none()
        && ship
        && !waiting
        && !empire.broke()
        && size >= 3
        && empire.spare_food >= HalfInt::ZERO)
        .then(|| settler_item(view, city))
        .flatten()
}

/// What every city's plan reads of the whole empire, worked out once for the round of cities:
/// the budget, the army and the rivals do not change from one city to the next (a plan only
/// sets what a city builds), and each of them costs a pass over all the units or cities.
struct Empire {
    /// The gold on hand.
    gold: i32,
    /// Net gold with the gold on hand, spread over a hundred turns, counted as income.
    net: HalfInt,
    spare_food: HalfInt,
    fighters: usize,
    cities: usize,
    home_guard: usize,
    traits: crate::objective::Traits,
    temper: i32,
    known: Vec<(hecs::Entity, f4x_core::player::PlayerId, Position)>,
}

impl Empire {
    fn of(view: &PlayerView) -> Empire {
        let gold = view.treasury().gold.floor();
        let budget = view.budget();
        let upkeep = view.food_of(&view.my_units());
        let my_cities = view.my_cities();
        Empire {
            gold,
            net: budget.net_gold() + (gold / 100).max(0),
            spare_food: budget.food_surplus - upkeep,
            fighters: view
                .my_units()
                .into_iter()
                .filter(|&u| is_fighter(view, u))
                .count(),
            cities: my_cities.len(),
            home_guard: my_cities
                .iter()
                .map(|&(_, p)| garrison_wanted(view, p))
                .sum(),
            traits: crate::objective::traits(view),
            temper: view.wizard().map_or(0, |w| {
                crate::objective::army_bias(w.personality(), view.difficulty())
            }),
            known: view.known_cities(),
        }
    }

    /// The treasury cannot bear a deficit: net gold is negative and the gold on hand is low.
    fn broke(&self) -> bool {
        self.net < 0 && self.gold < 60
    }
}

/// The plan of one city: the army comes first while it is short of its target and the
/// treasury can pay for it, a rival close by makes the city a fighter, and a wizard with room
/// for more cities lets the city build settlers.
fn plan_for(
    view: &PlayerView,
    empire: &Empire,
    city: hecs::Entity,
    pos: Position,
    pressure: Pressure,
    settlers_planned: &mut usize,
    growth: Growth,
) -> BuildPlan {
    let Pressure {
        threatened,
        war_force,
    } = pressure;
    let broke = empire.broke();
    let Empire {
        gold,
        net: _,
        spare_food,
        fighters,
        cities: all,
        home_guard,
        ref traits,
        temper,
        ref known,
    } = *empire;
    let Growth { room, wide } = growth;
    let size = view.city_economy(city).map_or(0, |e| e.figures);
    let garrison = view
        .garrison(pos)
        .into_iter()
        .filter(|&u| is_fighter(view, u))
        .count();
    let treasury_ok = empire.net >= 1 && spare_food >= HalfInt::from_int(1);
    // The strike force is what the campaign needs, once there is one; before that a few per city.
    let strike = war_force.unwrap_or_else(|| {
        (crate::knob("STRIKE", traits.strike_base as f32) as i32).max(1) as usize + all / 2
    });
    let strike = (strike as i32 + temper).max(1) as usize;
    let army_short = (fighters < home_guard + strike && treasury_ok) || threatened;
    // A wizard settles up to the number its objective wants at once, and after that for as long as
    // there is ground for a city (two settlers at a time): nothing in this game punishes size.
    let expanding = all + *settlers_planned
        < crate::knob("CITIES", traits.cities_wanted as f32) as usize
        || (room && *settlers_planned < more_settlers(wide));
    let rival_near = known.iter().any(|&(_, o, p)| {
        view.is_rival_wizard(o) && view.distance(pos, p).is_some_and(|d| d <= RIVAL_RANGE)
    });
    let (primary, secondary) = if rival_near {
        (Focus::Offense, Some(Focus::Defense))
    } else if (expanding && wide > TALL_BELOW) || wide >= WIDE_ABOVE {
        traits.expanding
    } else {
        match traits.variety {
            Some((other, every)) if view.serial_of(city).0.is_multiple_of(every) => other,
            _ => traits.settled,
        }
    };
    // A city whose square is full cannot finish a unit: the progress just piles up and the gold
    // has nothing to buy. It builds buildings until the army has made room.
    let full = view.garrison(pos).len() >= f4x_core::unit::MAX_STACK;
    let mix = if full {
        Mix::Buildings
    } else if army_short {
        Mix::Units
    } else if gold >= RICH_GOLD {
        // Gold with nothing to buy: the cities make units, which it can buy.
        Mix::Units
    } else if wide <= TALL_BELOW && garrison >= 1 {
        // A tall wizard builds up its cities and keeps the army it has.
        Mix::Buildings
    } else if broke && garrison >= 1 {
        Mix::Buildings
    } else {
        Mix::Both
    };
    // Settlers: while there is room for more cities, from a city big enough to spare them and
    // fed well enough to keep them.
    let settlers = expanding
        && size >= 3
        && !broke
        && spare_food >= HalfInt::ZERO
        && settler_item(view, city).is_some();
    if settlers {
        *settlers_planned += 1;
    }
    BuildPlan {
        mix,
        primary,
        secondary,
        settlers,
        min_settler_pop: 3,
        event_prompt: false,
        eager_settlers: true,
        elite_units: crate::phase::phase(view) == crate::phase::Phase::Win,
    }
}

/// Buy what is being built when the treasury has gold to spare, settlers and the first
/// guard of each city before the rest.
fn spend_gold(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    const RESERVE: i32 = 40;
    let wanted = {
        let view = PlayerView::new(game, me);
        let mut wanted: Vec<(u32, HalfInt, hecs::Entity)> = Vec::new();
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
        let gold = PlayerView::new(game, me).treasury().gold;
        if gold - cost >= RESERVE {
            ai.send(game, Command::BuyProduction { city }, events);
        }
    }
}

fn settlers_alive(view: &PlayerView) -> usize {
    view.my_units()
        .into_iter()
        .filter(|&u| view.unit_has_ability(u, Ability::new(12)))
        .count()
}

fn is_settler_def(view: &PlayerView, unit: f4x_def::rules::UnitId) -> bool {
    view.content()
        .rules
        .units
        .get(unit)
        .abilities
        .contains(&f4x_def::rules::Ability::new(12)) // create_outpost
}

fn settler_item(view: &PlayerView, city: hecs::Entity) -> Option<ProductionItem> {
    let rules = &view.content().rules;
    rules
        .units
        .iter()
        .filter(
            |(_, d)| d.abilities.contains(&f4x_def::rules::Ability::new(12)), /* create_outpost */
        )
        .map(|(id, _)| ProductionItem::Unit(id))
        .find(|&i| view.can_build(city, i).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{game, COAST_CITY, P0};
    use f4x_core::city::autobuild::Focus;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The capital of player 0.
    fn capital(g: &Game) -> Result<hecs::Entity, &'static str> {
        g.economy()
            .city_at(crate::testworld::at(COAST_CITY.0, COAST_CITY.1))
            .ok_or("player 0 has a city on the coast")
    }

    #[test]
    fn a_young_wizard_plans_for_growth_and_the_city_builds_by_the_plan() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        cities(&mut ai, &mut g, &mut events);
        let city = capital(&g)?;
        let plan = g.autobuild_of(city).ok_or("the city has a plan")?.plan;
        assert_eq!(plan.primary, Focus::Production, "room for more cities");
        assert_eq!(plan.secondary, Some(Focus::Population));
        assert!(plan.settlers && plan.eager_settlers && plan.min_settler_pop == 3);
        // A second look changes nothing and keeps what the city has spent.
        let again = g.autobuild_of(city).ok_or("the city has a plan")?;
        cities(&mut ai, &mut g, &mut events);
        assert_eq!(g.autobuild_of(city), Some(again));
        // The city is building something now (the plan took over from housing).
        assert!(g.economy().city_production(city).item.is_some());
        Ok(())
    }

    #[test]
    fn a_port_that_can_only_build_triremes_builds_one_and_counts_it_as_its_ship() -> TestResult {
        use crate::testworld::{at, build, spawn, ENEMY_CITY};
        let enemy = at(ENEMY_CITY.0, ENEMY_CITY.1);
        let builds_trireme = |g: &Game| -> Result<bool, &'static str> {
            let item = g.economy().city_production(capital(g)?).item;
            let rules = &g.content().rules;
            Ok(matches!(item, Some(ProductionItem::Unit(u))
                if rules.units.get(u).id == "trireme"))
        };
        // Beastmen have no Ship Yard: the Ship Wrights Guild is the end of their ships.
        let mut g = game(COAST_CITY, 1 /* beastmen */);
        build(
            &mut g,
            at(COAST_CITY.0, COAST_CITY.1),
            &[12 /* ship_wrights_guild */],
        );
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        ai.unreachable.push(enemy);
        crate::naval::run(&mut ai, &mut g, &mut Vec::new());
        assert!(ai.naval.is_some(), "a landing is planned");
        cities(&mut ai, &mut g, &mut Vec::new());
        assert!(
            builds_trireme(&g)?,
            "no ship yet: the port builds the trireme"
        );
        // With the trireme there, the port goes back to its usual builds.
        spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            35, /* trireme */
            1,
        );
        let city = capital(&g)?;
        let _ = g.set_production(city, ProductionItem::Housing);
        cities(&mut ai, &mut g, &mut Vec::new());
        assert!(!builds_trireme(&g)?, "one trireme is the ship");
        Ok(())
    }

    #[test]
    fn a_city_that_cannot_feed_settlers_or_has_no_room_plans_without_them() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        // Six settlers on their way count as cities to come: no more are wanted.
        crate::testworld::spawn(
            &mut g,
            P0,
            crate::testworld::at(10, 6),
            111, /* high_men_settlers */
            6,
        );
        cities(&mut ai, &mut g, &mut events);
        let plan = g
            .autobuild_of(capital(&g)?)
            .ok_or("the city has a plan")?
            .plan;
        assert!(!plan.settlers);
        assert_eq!(plan.primary, Focus::Balanced);
        Ok(())
    }

    #[test]
    fn a_militarist_that_has_settled_builds_for_offence() -> TestResult {
        use f4x_core::wizard::objective::Objective;
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.wizard_mut(P0)
            .ok_or("player 0 is a wizard")?
            .put(Some(Objective::Militarist));
        // Six cities to come (Militarist wants six): the wizard is done expanding.
        crate::testworld::spawn(
            &mut g,
            P0,
            crate::testworld::at(10, 6),
            111, /* high_men_settlers */
            6,
        );
        cities(&mut Ai::new(P0), &mut g, &mut Vec::new());
        let plan = g
            .autobuild_of(capital(&g)?)
            .ok_or("the city has a plan")?
            .plan;
        assert_eq!(plan.primary, Focus::Offense);
        assert_eq!(plan.secondary, Some(Focus::Production));
        Ok(())
    }
}
