//! Running short of mana, gold or food: the AI chooses what to give up as a
//! player does, through `ReleaseUpkeep` and `ReleaseTroops`, instead of leaving it to the turn
//! change. The rules are deliberately plain and fixed, so the scripted AI is deterministic:
//! mana: the newest spells and summons go (what the turn change would do); gold: first sell
//! buildings that do the AI no good, then let go of the cheapest units, ships with passengers and
//! heroes last; food: the cheapest units that eat.

use f4x_core::game::{Command, Event, Game};
use f4x_core::player::shortfall::TreasuryShortfall;

use crate::Ai;

/// Buildings the AI sells first when the gold runs out, most dispensable first.
const SELL_ORDER: &[&str] = &[
    "university",
    "sages_guild",
    "library",
    "armory",
    "smithy",
    "sawmill",
    "foresters_guild",
    "miners_guild",
    "bank",
];

pub fn run(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    if let Some(short) = game.upkeep_shortfall(me) {
        let items = short.default_release();
        ai.send(game, Command::ReleaseUpkeep { player: me, items }, events);
    }
    sell_for_gold(ai, game, events);
    let Some(short) = game.treasury_shortfall(me) else {
        return;
    };
    let units = choose_troops(&short);
    let scrap = game
        .item_overflow(me, &units)
        .map(|o| o.default_scrap)
        .unwrap_or_default();
    ai.send(
        game,
        Command::ReleaseTroops {
            player: me,
            units,
            scrap,
        },
        events,
    );
}

/// Sell dispensable buildings while the gold is short, one at a time.
fn sell_for_gold(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    loop {
        let missing = game.treasury_shortfall(me).map_or(0, |s| s.missing(&[]).1);
        if missing == 0 {
            return;
        }
        let Some((city, building)) = next_sale(game, me) else {
            return;
        };
        if !ai.send(game, Command::SellBuilding { city, building }, events) {
            return;
        }
    }
}

fn next_sale(
    game: &Game,
    me: f4x_core::player::PlayerId,
) -> Option<(hecs::Entity, f4x_core::rules::BuildingId)> {
    let cities = f4x_core::player::view::PlayerView::new(game, me).my_cities();
    let rules = &game.content().rules;
    for name in SELL_ORDER {
        for &(city, _) in &cities {
            if game.economy().sold_this_turn(city) {
                continue;
            }
            // Not one that another building of the city needs: the sale would be refused.
            if let Some(b) = game.city_buildings(city).into_iter().find(|&b| {
                rules.buildings.get(b).id == *name
                    && rules.buildings.get(b).upkeep_gold > 0
                    && game.economy().sale_price(city, b).is_ok()
            }) {
                return Some((city, b));
            }
        }
    }
    None
}

/// The units to let go: cheap ones first, ships carrying passengers and heroes last. Food is
/// settled first, and a unit let go for food no longer counts for gold.
fn choose_troops(short: &TreasuryShortfall) -> Vec<hecs::Entity> {
    let mut order: Vec<usize> = (0..short.troops.len()).collect();
    order.sort_by_key(|&i| {
        let t = &short.troops[i];
        (t.hero, t.ship && t.aboard > 0, t.cost, std::cmp::Reverse(i))
    });
    let mut released: Vec<hecs::Entity> = Vec::new();
    for want_food in [true, false] {
        for &i in &order {
            let (food, gold) = short.missing(&released);
            let need = if want_food { food } else { gold };
            if need == 0 {
                break;
            }
            let t = &short.troops[i];
            let helps = if want_food { t.food > 0 } else { t.gold > 0 };
            if helps && !released.contains(&t.unit) {
                released.push(t.unit);
            }
        }
    }
    released
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn the_ai_lets_go_of_the_units_it_cannot_feed_or_pay_for() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        let army = spawn(&mut g, P0, at(9, 6), "high_men_spearmen", 8);
        g.treasury_mut(P0).ok_or("player 0 has a treasury")?.gold = 3;
        g.apply(Command::SetTaxRate {
            player: P0,
            rate: 0,
        });
        let t = g.budget(P0);
        assert!(t.food_upkeep > t.food_surplus, "the town cannot feed eight");
        assert!(g.treasury_shortfall(P0).is_some());
        let mut events = Vec::new();
        run(&Ai::new(P0), &mut g, &mut events);
        assert!(g.treasury_shortfall(P0).is_none(), "the books balance");
        let left = army
            .iter()
            .filter(|&&u| g.units().unit_owner(u).is_some())
            .count();
        assert!(left < 8, "some went");
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Shortage {
                what: f4x_core::game::Shortage::Food,
                ..
            }
        )));
        // Those let go are the last made: the oldest stay as long as any do.
        let kept: Vec<_> = army
            .iter()
            .map(|&u| g.units().unit_owner(u).is_some())
            .collect();
        assert!(kept.windows(2).all(|w| w[0] || !w[1]), "{kept:?}");
        Ok(())
    }

    #[test]
    fn a_building_another_one_needs_is_not_put_up_for_sale() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        let pos = at(COAST_CITY.0, COAST_CITY.1);
        crate::testworld::build(&mut g, pos, &["barracks", "smithy", "library"]);
        let city = g.economy().city_at(pos).ok_or("player 0 has a city")?;
        let buildings = &g.content().rules.buildings;
        let smithy = buildings.id_of("smithy").ok_or("smithy is shipped")?;
        let library = buildings.id_of("library").ok_or("library is shipped")?;
        let name = |g: &Game, b| g.content().rules.buildings.get(b).id.clone();
        let sale = |g: &Game| next_sale(g, P0).map(|(_, b)| name(g, b));
        assert_eq!(sale(&g).as_deref(), Some("library"));
        // The stables need the smithy: the smithy is no longer on offer, the library still is.
        crate::testworld::build(&mut g, pos, &["stables"]);
        assert!(g.economy().sale_price(city, smithy).is_err());
        assert_eq!(sale(&g).as_deref(), Some("library"));
        // Without the library nothing else is both dispensable and free to sell.
        g.scenario_set_building(city, library, false);
        assert_eq!(sale(&g), None);
        Ok(())
    }

    #[test]
    fn heroes_and_loaded_ships_are_the_last_to_go() {
        use f4x_core::player::shortfall::{TreasuryShortfall, TroopLine};
        let mut world = hecs::World::new();
        let e: Vec<_> = (0..4).map(|_| world.spawn(())).collect();
        let line = |unit, cost, hero, ship, aboard| TroopLine {
            unit,
            label: String::new(),
            gold: 1,
            food: 0,
            hero,
            cost,
            ship,
            aboard,
        };
        let short = TreasuryShortfall {
            food_upkeep: 0,
            food_surplus: 0,
            gold: 1,
            fame: 0,
            troops: vec![
                line(e[0], 500, true, false, 0),
                line(e[1], 100, false, true, 2),
                line(e[2], 100, false, false, 0),
                line(e[3], 50, false, false, 0),
            ],
            default_release: Vec::new(),
        };
        // Four gold of upkeep against one: three must go, the hero stays.
        assert_eq!(choose_troops(&short), vec![e[3], e[2], e[1]]);
    }
}
