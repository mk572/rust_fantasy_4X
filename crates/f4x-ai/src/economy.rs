//! Research, spells and what the cities build.

use f4x_core::city::autobuild::{BuildPlan, Focus, Mix};
use f4x_core::city::production::ProductionItem;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

use crate::view::{garrison_wanted, is_fighter};
use crate::Ai;

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
    let war_force = ai.campaign.map(|target| {
        let view = PlayerView::new(game, me);
        crate::military::force_wanted(&view, &ai.city_memory, game.turn, target)
    });
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
            let port = ai.naval.map(|p| p.port).or(ai.sea_port);
            let broke = broke(&view);
            // The port of a landing builds the ship (and what the ship needs) when there is none.
            let port_item = (port == Some(pos)
                && !broke
                && !matches!(current, Some(ProductionItem::Building(_)))
                && !view
                    .my_units()
                    .iter()
                    .any(|&u| crate::view::is_ship(&view, u)))
            .then(|| crate::naval::port_item(&view, city))
            .flatten();
            (
                plan_for(
                    &view,
                    city,
                    pos,
                    broke,
                    ai.threatened.contains(&pos),
                    war_force,
                    &mut settlers_planned,
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

/// The treasury cannot bear a deficit: net gold (with the gold on hand, spread over a hundred
/// turns, counted as income) is negative and the gold on hand is low.
fn broke(view: &PlayerView) -> bool {
    let gold = view.treasury().gold;
    let net = view.budget().net_gold() + (gold / 100).max(0);
    net < 0 && gold < 60
}

/// The plan of one city: the army comes first while it is short of its target and the
/// treasury can pay for it, a rival close by makes the city a fighter, and a wizard with room
/// for more cities lets the city build settlers.
fn plan_for(
    view: &PlayerView,
    city: hecs::Entity,
    pos: Position,
    broke: bool,
    threatened: bool,
    war_force: Option<usize>,
    settlers_planned: &mut usize,
) -> BuildPlan {
    let size = view.city_economy(city).map_or(0, |e| e.figures);
    let garrison = view
        .garrison(pos)
        .into_iter()
        .filter(|&u| is_fighter(view, u))
        .count();
    let gold = view.treasury().gold;
    let net = view.budget().net_gold() + (gold / 100).max(0);
    let treasury_ok = net >= 1 && spare_food(view) >= 1;
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
    let traits = crate::objective::traits(view);
    let temper = view.wizard().map_or(0, |w| {
        crate::objective::army_bias(w.personality(), view.difficulty())
    });
    // The strike force is what the campaign needs, once there is one; before that a few per city.
    let strike = war_force.unwrap_or_else(|| {
        (crate::knob("STRIKE", traits.strike_base as f32) as i32).max(1) as usize + all / 2
    });
    let strike = (strike as i32 + temper).max(1) as usize;
    let army_short = (fighters < home_guard + strike && treasury_ok) || threatened;
    let expanding =
        all + *settlers_planned < crate::knob("CITIES", traits.cities_wanted as f32) as usize;
    let rival_near = view.known_cities().into_iter().any(|(_, o, p)| {
        view.is_rival_wizard(o) && view.distance(pos, p).is_some_and(|d| d <= RIVAL_RANGE)
    });
    let (primary, secondary) = if rival_near {
        (Focus::Offense, Some(Focus::Defense))
    } else if expanding {
        traits.expanding
    } else {
        match traits.variety {
            Some((other, every)) if view.serial_of(city).0.is_multiple_of(every) => other,
            _ => traits.settled,
        }
    };
    let mix = if army_short {
        Mix::Units
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
        && spare_food(view) >= 0
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
    }
}

/// Spare food after feeding the citizens and the army.
fn spare_food(view: &PlayerView) -> i32 {
    let upkeep: i32 = view
        .my_units()
        .iter()
        .map(|&u| view.unit_food_upkeep(u))
        .sum();
    view.budget().food_surplus - upkeep
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
        let gold = PlayerView::new(game, me).treasury().gold;
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

fn settler_item(view: &PlayerView, city: hecs::Entity) -> Option<ProductionItem> {
    let rules = &view.content().rules;
    rules
        .units
        .iter()
        .filter(|(_, d)| d.abilities.iter().any(|a| a == "create_outpost"))
        .map(|(id, _)| ProductionItem::Unit(id))
        .find(|&i| view.can_build(city, i).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{game, COAST_CITY, P0};
    use f4x_core::city::autobuild::Focus;

    #[test]
    fn a_young_wizard_plans_for_growth_and_the_city_builds_by_the_plan() {
        let mut g = game(COAST_CITY, "high_men");
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        cities(&mut ai, &mut g, &mut events);
        let city = g.economy().city_at(crate::testworld::at(10, 6)).unwrap();
        let plan = g.autobuild_of(city).expect("the city has a plan").plan;
        assert_eq!(plan.primary, Focus::Production, "room for more cities");
        assert_eq!(plan.secondary, Some(Focus::Population));
        assert!(plan.settlers && plan.eager_settlers && plan.min_settler_pop == 3);
        // A second look changes nothing and keeps what the city has spent.
        let again = g.autobuild_of(city).unwrap();
        cities(&mut ai, &mut g, &mut events);
        assert_eq!(g.autobuild_of(city).unwrap(), again);
        // The city is building something now (the plan took over from housing).
        assert!(g.economy().city_production(city).item.is_some());
    }

    #[test]
    fn a_city_that_cannot_feed_settlers_or_has_no_room_plans_without_them() {
        let mut g = game(COAST_CITY, "high_men");
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        // Six settlers on their way count as cities to come: no more are wanted.
        crate::testworld::spawn(
            &mut g,
            P0,
            crate::testworld::at(10, 6),
            "high_men_settlers",
            6,
        );
        cities(&mut ai, &mut g, &mut events);
        let city = g.economy().city_at(crate::testworld::at(10, 6)).unwrap();
        let plan = g.autobuild_of(city).unwrap().plan;
        assert!(!plan.settlers);
        assert_eq!(plan.primary, Focus::Balanced);
    }

    #[test]
    fn a_militarist_that_has_settled_builds_for_offence() {
        use f4x_core::wizard::objective::Objective;
        let mut g = game(COAST_CITY, "high_men");
        g.wizard_mut(P0).unwrap().put(Some(Objective::Militarist));
        // Six cities to come (Militarist wants six): the wizard is done expanding.
        crate::testworld::spawn(
            &mut g,
            P0,
            crate::testworld::at(10, 6),
            "high_men_settlers",
            6,
        );
        cities(&mut Ai::new(P0), &mut g, &mut Vec::new());
        let city = g.economy().city_at(crate::testworld::at(10, 6)).unwrap();
        let plan = g.autobuild_of(city).unwrap().plan;
        assert_eq!(plan.primary, Focus::Offense);
        assert_eq!(plan.secondary, Some(Focus::Production));
    }
}
