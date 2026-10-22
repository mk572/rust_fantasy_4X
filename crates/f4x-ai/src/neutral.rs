//! The independent cities. Each is its own player (`Player::neutral`) and this plays them:
//! farmers, taxes and what the city builds, after the original's `Player_Colony_Autobuild_NP`
//! (`AIBUILD.c`): a Barracks first, then soldiers while the garrison is short and buildings
//! once it is not, never settlers. They never move, attack or cast. The game does none of this
//! itself, so a host that ends turns must call [`play_neutrals`] first, or the cities starve.

use f4x_core::city::autobuild::{BuildPlan, Focus, Mix};
use f4x_core::city::production::ProductionItem;
use f4x_core::game::{Command, Event, Game};
use f4x_core::player::view::PlayerView;

use crate::view::is_fighter;
use crate::Ai;

/// Soldiers wanted per four citizens, as many melee as ranged (the garrison a city starts with).
const GARRISON_PER_FOUR: usize = 2;

/// Play every independent city's turn. Returns the events its commands caused.
pub fn play_neutrals(game: &mut Game) -> Vec<Event> {
    let cities: Vec<_> = game
        .players()
        .into_iter()
        .filter(|(id, _)| game.is_neutral_player(*id))
        .map(|(id, _)| id)
        .collect();
    let mut events = Vec::new();
    for id in cities {
        let mut ai = Ai::new(id);
        game.balance_farmers(id);
        // Taxes are looked at only when the city is not paying its way: finding the lowest
        // rate that does costs a forecast per rate.
        if PlayerView::new(game, id).budget().net_gold() < 1 {
            crate::economy::taxes(&mut ai, game, &mut events);
        }
        production(&ai, game, &mut events);
    }
    events
}

/// What each of the player's cities builds: the Barracks if it has none, else a plan of soldiers
/// while the garrison is short and the treasury can pay for one more, else of buildings.
fn production(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let cities = {
        let view = PlayerView::new(game, me);
        let mut mine = view.my_cities();
        mine.sort_by_key(|(e, _)| view.serial_of(*e));
        mine
    };
    for (city, pos) in cities {
        let (barracks, plan) = {
            let view = PlayerView::new(game, me);
            let Some(economy) = view.city_economy(city).filter(|e| e.figures > 0) else {
                continue;
            };
            let barracks = view
                .content()
                .rules
                .buildings
                .id_of("barracks")
                .map(ProductionItem::Building)
                .filter(|&item| view.can_build(city, item).is_ok())
                .filter(|&item| view.city_production(city).item != Some(item));
            let soldiers = view
                .garrison(pos)
                .into_iter()
                .filter(|&u| is_fighter(&view, u))
                .count();
            let wanted = GARRISON_PER_FOUR * (economy.figures as usize / 4).max(1);
            let treasury = view.budget();
            let affordable = treasury.net_gold() >= 1 && treasury.food_surplus >= 1;
            let mix = if soldiers < wanted && affordable {
                Mix::Units
            } else {
                Mix::Buildings
            };
            let plan = BuildPlan {
                mix,
                primary: Focus::Defense,
                secondary: None,
                settlers: false,
                min_settler_pop: 3,
                event_prompt: false,
                eager_settlers: false,
            };
            (barracks, plan)
        };
        if game.autobuild_of(city).map(|a| a.plan) != Some(plan) {
            ai.send(
                game,
                Command::SetAutoBuild {
                    city,
                    plan: Some(plan),
                },
                events,
            );
        }
        if let Some(item) = barracks {
            ai.send(game, Command::SetProduction { city, item }, events);
        }
    }
}
