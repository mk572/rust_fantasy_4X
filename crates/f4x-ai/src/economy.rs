//! Research, spells and what the cities build.

use f4x_core::city::autobuild::{BuildPlan, Focus, Mix};
use f4x_core::city::production::ProductionItem;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_def::half_int::HalfInt;
use f4x_def::rules::Ability;

use crate::builder;
use crate::view::is_fighter;
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
    ai.order(game, Command::SetTaxRate { player: me, rate }, events);
}

/// How many ships the wizard wants. A landing on a big army needs a fleet: one ship per twelve
/// fighters beyond the first, at most [`MAX_SHIPS`].
fn ships_wanted(ai: &Ai, view: &PlayerView) -> usize {
    let base = crate::knob("SHIPS", 1.0) as usize;
    if ai.naval.is_none() {
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

/// How a wizard should grow: how many places for a city no settler has taken yet
/// (`city_site::free_sites`), and how wide it leans (`phase::width`).
#[derive(Clone, Copy)]
struct Growth {
    free_sites: usize,
    wide: f32,
}

/// What the unit registry says about the units the wizard has (`roles.rs`): the autobuilder reads
/// it to build what is missing and no more of what stands idle.
#[derive(Clone, Copy, Default)]
struct Roster {
    /// Fighters holding `AwaitingOrders`: nothing to do.
    idle_fighters: usize,
    /// Units marching on a campaign target.
    marching: usize,
    /// Ships with no plan (`TransportFree`).
    free_ships: usize,
    /// Units awaiting transport on a shore (not the settlers waiting in a port).
    waiting: usize,
    /// Units out scouting.
    scouts: usize,
}

/// Idle fighters from which the wizard builds no more soldiers: the army it has is waiting for
/// something to do.
const IDLE_SURPLUS: usize = 4;

impl Roster {
    fn of(ai: &Ai, view: &PlayerView) -> Roster {
        use crate::roles::Role;
        let units = view.my_units();
        let idle_fighters = units
            .iter()
            .filter(|&&u| {
                is_fighter(view, u) && ai.roles.is(view.serial_of(u), Role::AwaitingOrders)
            })
            .count();
        let waiting = ai.roles.waiting_for_ship().len();
        Roster {
            idle_fighters,
            marching: ai.roles.count(Role::March),
            free_ships: ai.roles.count(Role::TransportFree),
            waiting,
            scouts: ai.roles.scouting().len(),
        }
    }

    /// Whether the army has more soldiers than it has work for.
    fn idle_army(&self) -> bool {
        self.idle_fighters >= IDLE_SURPLUS
    }
}

/// The fastest unit the city can build that fights or can scout, the cheapest of those: the
/// scout of a wizard that has none, if the empire has the food to keep it (`spare_food`).
fn scout_item(
    view: &PlayerView,
    city: hecs::Entity,
    spare_food: HalfInt,
) -> Option<ProductionItem> {
    let rules = &view.content().rules;
    rules
        .units
        .iter()
        .filter(|(_, d)| {
            d.kind == f4x_def::rules::UnitKind::Normal
                && !d.abilities.contains(&Ability::new(12))
                && d.stats
                    .get(&f4x_def::rules::Stat::Melee)
                    .is_some_and(|m| m.floor() > 0)
                && d.deck.is_none()
                && d.upkeep.food <= spare_food
        })
        .map(|(id, d)| {
            let moves = d
                .stats
                .get(&f4x_def::rules::Stat::Moves)
                .map_or(0, |m| m.floor());
            (std::cmp::Reverse(moves), d.cost, id)
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|(_, _, id)| ProductionItem::Unit(id))
        .find(|&i| view.can_build(city, i).is_ok())
}

/// What presses on a city's plan: an empty treasury, a rival close by, and how many fighters the
/// campaign wants at the front.
#[derive(Clone, Copy)]
struct Pressure {
    threatened: bool,
    war_force: Option<usize>,
    roster: Roster,
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
    // The list of sites: its length is the room for more cities (`phase::width` leans wide with it).
    let (free_sites, room) = {
        let view = PlayerView::new(game, me);
        let taken = ai.roles.targets(crate::roles::Role::Settle);
        let list = ai.sites.refreshed(&view);
        (
            crate::city_site::free_sites(list, &taken, &view),
            !list.is_empty(),
        )
    };
    let wide = crate::phase::width(&PlayerView::new(game, me), room);
    let war_force = ai.campaign.map(|target| {
        let view = PlayerView::new(game, me);
        let strength = crate::war_plan::strength_wanted(&view, &ai.city_memory, game.turn, target);
        crate::war_plan::units_for(&view, strength)
    });
    let empire = Empire::of(&PlayerView::new(game, me));
    let roster = Roster::of(ai, &PlayerView::new(game, me));
    // Fewer scouts than a wizard that builds up wants (`scout::wanted`) and no soldier spare: one
    // city builds the unit that scouts (the first that is not full, so that the others go on with
    // their plans).
    let scout_city = (roster.scouts < crate::scout::wanted(mine.len(), true)
        && roster.idle_fighters == 0
        && !ai.frontier_closed
        && crate::phase::phase(&PlayerView::new(game, me)) == crate::phase::Phase::Build)
        .then(|| {
            let view = PlayerView::new(game, me);
            mine.iter()
                .map(|&(c, p)| (c, p))
                .find(|&(c, p)| {
                    view.city_economy(c).is_some_and(|e| e.figures > 0)
                        && view.garrison(p).len() < view.max_tile()
                        && scout_item(&view, c, builder::spare_food(&view)).is_some()
                })
                .map(|(c, _)| c)
        })
        .flatten();
    let (elite, forges) = {
        let view = PlayerView::new(game, me);
        (crate::forge::active(&view), crate::forge::forges(&view))
    };
    // Each city's auto-builder reads the net gold: kept and patched for the cities whose item
    // changes, not worked out for the whole empire in every city.
    game.begin_kept_budget();
    // The food of the units the cities have in production: they are not in `empire.spare_food`
    // yet, and each unit chosen below joins them.
    let mut queued_food = {
        let view = PlayerView::new(game, me);
        mine.iter()
            .filter_map(|&(c, _)| match view.city_production(c).item {
                Some(ProductionItem::Unit(u)) => {
                    Some(view.content().rules.units.get(u).upkeep.food)
                }
                _ => None,
            })
            .sum::<HalfInt>()
    };
    for (city, pos) in mine {
        let port_item = {
            let view = PlayerView::new(game, me);
            let current = view.city_production(city).item;
            if matches!(current, Some(ProductionItem::Unit(u)) if is_settler_def(&view, u)) {
                settlers_planned = settlers_planned.saturating_sub(1);
            }
            let size = view.city_economy(city).map_or(0, |e| e.figures);
            // An outpost builds nothing: housing is what makes it grow into a city.
            if size == 0 {
                if current != Some(ProductionItem::Housing) {
                    ai.order(
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
            // Units awaiting transport and no ship free to take them: the nearest port builds one.
            let rescue_port = (roster.waiting > 0 && roster.free_ships == 0)
                .then(|| rescue_port(ai, &view))
                .flatten();
            let port = ai.naval.map(|p| p.port).or(ai.sea_port).or(rescue_port);
            let broke = empire.broke();
            // The port of a landing builds the ship (and what the ship needs) when there is none.
            let port_item = (port == Some(pos)
                && !broke
                && !matches!(current, Some(ProductionItem::Building(_))))
            .then(|| crate::naval::port_item(&view, city, ai.naval.is_some()))
            .flatten()
            // Ships that stand idle (`TransportFree`) are the fleet: no more are built.
            .filter(|_| roster.free_ships == 0)
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
                        ai.naval.is_none()
                            || view.unit_stat(u, f4x_def::rules::Stat::Transport) >= needed
                    })
                    .count()
                    < ships_wanted(ai, &view)
            });
            let threatened = ai.threatened.contains(&pos);
            // Only a city with nothing in hand (or Housing, or Trade Goods) gets a new goal; a
            // standing order is left alone, and counts no plan.
            let mut plan = builder::is_open(current).then(|| {
                plan_for(
                    &view,
                    &empire,
                    city,
                    pos,
                    Pressure {
                        threatened,
                        war_force,
                        roster,
                    },
                    &mut settlers_planned,
                    Growth { free_sites, wide },
                )
            });
            let mut port_item = port_item;
            if port_item.is_none() && scout_city == Some(city) && !broke {
                port_item = scout_item(&view, city, empire.spare_food - queued_food);
            }
            // Past the build-up the army is a few strong units: every city prefers the dear
            // ones, and the forge cities build out toward the race's top unit and make it.
            if let Some(plan) = plan.as_mut() {
                plan.elite_units |= elite;
            }
            if forges.contains(&city)
                && !threatened
                && !broke
                && port_item.is_none()
                && view.garrison(pos).len() < view.max_tile()
            {
                match crate::forge::step(&view, city) {
                    crate::forge::Step::Ready(_) => {
                        if let Some(plan) = plan.as_mut() {
                            plan.mix = Mix::Units;
                            plan.primary = Focus::Offense;
                        }
                    }
                    crate::forge::Step::Build(b) => {
                        // Only between items: a building started is not thrown away.
                        if !matches!(current, Some(ProductionItem::Building(_))) {
                            port_item = Some(ProductionItem::Building(b));
                        }
                    }
                    crate::forge::Step::Wait => {}
                }
            }
            // A city that has run out of what its focus values and sits on Trade Goods looks at
            // every building it can build (task 540): a focus of Offense gives a library or a
            // temple no weight at all, so the city idled for hundreds of turns.
            if current == Some(ProductionItem::TradeGoods) {
                if let Some(plan) = plan.as_mut().filter(|p| p.mix != Mix::Units) {
                    plan.primary = Focus::Balanced;
                    plan.secondary = None;
                }
            }
            // Emergencies and the ports' ships go first; otherwise the chooser picks.
            let item = port_item.or_else(|| {
                plan.map(|plan| {
                    let rival_near = empire.known.iter().any(|&(_, o, p)| {
                        view.is_rival_wizard(o)
                            && view.distance(pos, p).is_some_and(|d| d <= RIVAL_RANGE)
                    });
                    let defenders = view
                        .garrison(pos)
                        .into_iter()
                        .filter(|&u| is_fighter(&view, u))
                        .count() as u32;
                    builder::next_item(
                        &view,
                        city,
                        &plan,
                        builder::Need {
                            net_gold: empire.net,
                            rival_near,
                            garrison_cap: quiet_cap(&view, pos),
                            settler_first: empire.cities <= 1,
                            spare_food: empire.spare_food - queued_food,
                        },
                        defenders,
                    )
                })
            });
            item
        };
        if let Some(ProductionItem::Unit(u)) = port_item {
            if game.economy().city_production(city).item != port_item {
                queued_food += game.content().rules.units.get(u).upkeep.food;
            }
        }
        if let Some(item) = port_item {
            if game.economy().city_production(city).item != Some(item) {
                ai.order(game, Command::SetProduction { city, item }, events);
            }
        }
    }
    game.end_kept_budget();
    spend_gold(ai, game, events);
}

/// The most soldiers the garrison floor asks of a city while the wizard builds up and no enemy is near
/// it: the soldiers the phase gives it (`garrison::wanted_share`), rounded up; no limit otherwise.
fn quiet_cap(view: &PlayerView, pos: Position) -> u32 {
    let share = crate::garrison::wanted_share(view, pos);
    if share < crate::view::garrison_wanted(view, pos) as f32 {
        share.ceil() as u32
    } else {
        u32::MAX
    }
}

/// The own coastal city nearest to the units awaiting transport: it builds the ship that fetches
/// them.
fn rescue_port(ai: &Ai, view: &PlayerView) -> Option<Position> {
    let waiting = ai
        .roles
        .waiting_for_ship()
        .into_iter()
        .filter_map(|(s, _)| {
            view.my_units()
                .into_iter()
                .find(|&u| view.serial_of(u) == s)
                .and_then(|u| view.unit_position(u))
        })
        .min_by_key(|t| (t.plane as u8, t.y, t.x))?;
    view.my_cities()
        .into_iter()
        .filter(|&(c, p)| {
            p.plane == waiting.plane
                && crate::view::is_coastal(view, p)
                && view.city_economy(c).is_some_and(|e| e.figures > 0)
        })
        .min_by_key(|&(_, p)| (view.distance(p, waiting).unwrap_or(u32::MAX), p.y, p.x))
        .map(|(_, p)| p)
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
        let my_cities = view.my_cities();
        Empire {
            gold,
            net: budget.net_gold() + (gold / 100).max(0),
            spare_food: builder::spare_food(view),
            fighters: view
                .my_units()
                .into_iter()
                .filter(|&u| is_fighter(view, u))
                .count(),
            cities: my_cities.len(),
            home_guard: my_cities
                .iter()
                .map(|&(_, p)| crate::garrison::wanted_share(view, p))
                .sum::<f32>()
                .ceil() as usize,
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
        roster,
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
    let Growth { free_sites, wide } = growth;
    let room = free_sites > 0;
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
    // The registry says what the army is doing: soldiers waiting for orders are enough, and a war
    // with nobody marching wants attackers.
    let army_short = if threatened {
        true
    } else if roster.idle_army() || crate::phase::phase(view) == crate::phase::Phase::Build {
        // While the wizard builds up, with nothing threatening, the army waits: the food and the
        // production go to settlers and buildings (a soldier eats a settler's food).
        false
    } else {
        (fighters < home_guard + strike || (war_force.is_some() && roster.marching == 0))
            && treasury_ok
    };
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
    let full = view.garrison(pos).len() >= view.max_tile();
    let mix = if full {
        Mix::Buildings
    } else if spare_food < HalfInt::from_int(1) && !threatened {
        // A unit the land cannot feed starves at the turn change: nothing is made but buildings
        // (a farm, a granary) until there is food to spare.
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
    // Settlers: one for each place for a city that no settler has taken (`city_site`), no more, none
    // while no place is known (the scouts find them: `scout.rs`, which looks harder while the wizard
    // builds up), except the first settler of a wizard with one city; a wizard under the number of cities its objective wants takes them all, any other
    // keeps one on the way (two or three when it leans wide), from a city big enough to spare them
    // and fed well enough to keep them.
    let wanted = crate::knob("CITIES", traits.cities_wanted as f32) as usize;
    let cap = if free_sites == 0 {
        // Nothing known yet: the wizard with one city starts its first settler anyway. A settler takes
        // longer to build than the scouts take to find a place, and the city would spend those turns
        // on units that eat the settler's food.
        usize::from(all <= 1 && all + *settlers_planned < wanted)
    } else if all + *settlers_planned < wanted {
        free_sites
    } else {
        free_sites.min(more_settlers(wide).max(1))
    };
    let settlers = *settlers_planned < cap
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

/// The gold the wizard keeps in the treasury when it buys production: a base and some turns of
/// its net income, so that a richer wizard saves more and has something to pay a trade with
/// (`AIK_GOLD_RESERVE_BASE`, `AIK_GOLD_RESERVE_INCOME`).
pub(crate) fn gold_reserve(game: &Game, me: PlayerId) -> i32 {
    let net = PlayerView::new(game, me).budget().net_gold().floor().max(0);
    let base = crate::knob("GOLD_RESERVE_BASE", 40.0) as i32;
    let turns = crate::knob("GOLD_RESERVE_INCOME", 3.0) as i32;
    base + turns * net
}

/// Buy what is being built when the treasury has gold to spare, settlers and the first
/// guard of each city before the rest.
fn spend_gold(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let reserve = gold_reserve(game, me);
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
        if gold - cost >= HalfInt::from_int(reserve) {
            ai.order(game, Command::BuyProduction { city }, events);
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
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The capital of player 0.
    fn capital(g: &Game) -> Result<hecs::Entity, &'static str> {
        g.economy()
            .city_at(crate::testworld::at(COAST_CITY.0, COAST_CITY.1))
            .ok_or("player 0 has a city on the coast")
    }

    /// The plan the AI works out for a city of player 0 (the plan decides what the chooser may
    /// build; it is not stored in the game).
    fn plan_of(g: &Game, ai: &Ai, city: hecs::Entity) -> Result<BuildPlan, &'static str> {
        let view = PlayerView::new(g, P0);
        let pos = g.city_position(city).ok_or("the city has a position")?;
        let empire = Empire::of(&view);
        let mut planned = settlers_alive(&view);
        Ok(plan_for(
            &view,
            &empire,
            city,
            pos,
            Pressure {
                threatened: false,
                war_force: None,
                roster: Roster::of(ai, &view),
            },
            &mut planned,
            Growth {
                free_sites: crate::city_site::free_sites(
                    &crate::city_site::scan(&view),
                    &ai.roles.targets(crate::roles::Role::Settle),
                    &view,
                ),
                wide: crate::phase::width(&view, crate::city_site::has_sites(&view)),
            },
        ))
    }

    #[test]
    fn a_young_wizard_plans_for_growth_and_the_city_builds_by_the_plan() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        // The wizard has looked at its island: there are places for cities.
        g.explore(P0, crate::testworld::at(7, 6), HalfInt::from_int(4));
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        let city = capital(&g)?;
        let plan = plan_of(&g, &ai, city)?;
        assert_eq!(plan.primary, Focus::Production, "room for more cities");
        assert_eq!(plan.secondary, Some(Focus::Population));
        assert!(plan.settlers && plan.eager_settlers && plan.min_settler_pop == 3);
        // The city is building something now (the chooser took over from housing), and a second
        // look leaves a standing order alone.
        cities(&mut ai, &mut g, &mut events);
        let item = g.economy().city_production(city).item;
        assert!(!builder::is_open(item), "{item:?}");
        cities(&mut ai, &mut g, &mut events);
        assert_eq!(g.economy().city_production(city).item, item);
        assert!(
            g.autobuild_of(city).is_none(),
            "the AI does not use the helper"
        );
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
    fn a_wizard_with_nobody_scouting_builds_a_unit_that_can() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let mut ai = Ai::new(P0);
        cities(&mut ai, &mut g, &mut Vec::new());
        let item = g.economy().city_production(capital(&g)?).item;
        assert!(
            matches!(item, Some(ProductionItem::Unit(_))),
            "a unit for the scouting: {item:?}"
        );
        // Once somebody scouts, the city goes back to its plan.
        ai.frontier_closed = true;
        let city = capital(&g)?;
        let _ = g.set_production(city, ProductionItem::Housing);
        cities(&mut ai, &mut g, &mut Vec::new());
        assert!(!matches!(
            g.economy().city_production(city).item,
            Some(ProductionItem::Unit(u)) if g.content().rules.units.get(u).id == "high_men_cavalry"
        ));
        Ok(())
    }

    #[test]
    fn units_awaiting_transport_and_no_ship_make_the_nearest_port_build_one() -> TestResult {
        use crate::testworld::{at, build, spawn};
        let port = at(COAST_CITY.0, COAST_CITY.1);
        let mut g = game(COAST_CITY, 8 /* high_men */);
        build(
            &mut g,
            port,
            &[
                12, /* ship_wrights_guild */
                15, /* sawmill */
                13, /* ship_yard */
            ],
        );
        let stranded = spawn(&mut g, P0, at(9, 9), 104 /* high_men_spearmen */, 2);
        let mut ai = Ai::new(P0);
        ai.frontier_closed = true;
        let serials: Vec<_> = {
            let view = PlayerView::new(&g, P0);
            stranded.iter().map(|&u| view.serial_of(u)).collect()
        };
        for s in &serials {
            let _ = ai
                .roles
                .claim(*s, crate::roles::Role::Reinforce, 1, Some(port));
            ai.roles.set_crossing(*s, Some(crate::roles::Crossing::Sea));
        }
        cities(&mut ai, &mut g, &mut Vec::new());
        let item = g.economy().city_production(capital(&g)?).item;
        let rules = &g.content().rules;
        assert!(
            matches!(item, Some(ProductionItem::Unit(u)) if rules.units.get(u).id == "galley"),
            "{item:?}"
        );
        Ok(())
    }

    #[test]
    fn soldiers_awaiting_orders_are_enough_and_no_more_are_built() -> TestResult {
        use crate::testworld::{at, spawn};
        let mut g = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut g, P0, at(9, 9), 104 /* high_men_spearmen */, 6);
        let mut ai = Ai::new(P0);
        ai.frontier_closed = true;
        crate::roles::register(&mut ai, &g);
        let roster = Roster::of(&ai, &PlayerView::new(&g, P0));
        assert!(roster.idle_army(), "{}", roster.idle_fighters);
        let plan = plan_of(&g, &ai, capital(&g)?)?;
        assert_ne!(plan.mix, Mix::Units);
        Ok(())
    }

    #[test]
    fn a_city_that_cannot_feed_settlers_or_has_no_room_plans_without_them() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let ai = Ai::new(P0);
        // Six settlers on their way count as cities to come: no more are wanted.
        crate::testworld::spawn(
            &mut g,
            P0,
            crate::testworld::at(10, 6),
            111, /* high_men_settlers */
            6,
        );
        let plan = plan_of(&g, &ai, capital(&g)?)?;
        assert!(!plan.settlers);
        assert_eq!(plan.primary, Focus::Balanced);
        Ok(())
    }

    #[test]
    fn a_wizard_that_builds_up_still_has_one_settler_on_the_way_while_there_is_room() -> TestResult
    {
        let g = game(COAST_CITY, 8 /* high_men */);
        let ai = Ai::new(P0);
        let city = capital(&g)?;
        let view = PlayerView::new(&g, P0);
        let pos = g.city_position(city).ok_or("the city has a position")?;
        // All the cities it wanted, and a leaning to build up: it does not press for more.
        let mut empire = Empire::of(&view);
        empire.traits.cities_wanted = 0;
        let plan_with = |planned: usize| {
            let mut planned = planned;
            plan_for(
                &view,
                &empire,
                city,
                pos,
                Pressure {
                    threatened: false,
                    war_force: None,
                    roster: Roster::of(&ai, &view),
                },
                &mut planned,
                Growth {
                    free_sites: 1,
                    wide: 0.0,
                },
            )
        };
        assert!(plan_with(0).settlers, "room and no settler on the way");
        assert!(!plan_with(1).settlers, "one is on the way");
        Ok(())
    }

    /// The plan of the capital with `free` places for a city and `planned` settlers on the way, for
    /// a wizard that has all the cities its objective wants.
    fn plan_with_sites(
        free: usize,
        planned: usize,
    ) -> Result<BuildPlan, Box<dyn std::error::Error>> {
        let g = game(COAST_CITY, 8 /* high_men */);
        let ai = Ai::new(P0);
        let city = capital(&g)?;
        let view = PlayerView::new(&g, P0);
        let pos = g.city_position(city).ok_or("the city has a position")?;
        let mut empire = Empire::of(&view);
        empire.traits.cities_wanted = 0;
        let mut planned = planned;
        Ok(plan_for(
            &view,
            &empire,
            city,
            pos,
            Pressure {
                threatened: false,
                war_force: None,
                roster: Roster::of(&ai, &view),
            },
            &mut planned,
            Growth {
                free_sites: free,
                wide: 1.0,
            },
        ))
    }

    #[test]
    fn no_settler_is_planned_for_a_wizard_that_knows_no_place_for_a_city() -> TestResult {
        assert!(!plan_with_sites(0, 0)?.settlers);
        Ok(())
    }

    #[test]
    fn a_settler_is_planned_only_while_places_are_left_that_no_settler_has_taken() -> TestResult {
        assert!(
            plan_with_sites(3, 2)?.settlers,
            "three places, two settlers"
        );
        assert!(!plan_with_sites(3, 3)?.settlers, "a settler for each place");
        assert!(!plan_with_sites(1, 1)?.settlers, "the one place is taken");
        Ok(())
    }

    #[test]
    fn a_wizard_with_many_free_places_plans_more_than_one_settler() -> TestResult {
        // The wizard builds wide (`phase::width`): several at a time, not just the one on the way.
        assert!(
            plan_with_sites(8, 1)?.settlers,
            "one on the way, eight places"
        );
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
        let plan = plan_of(&g, &Ai::new(P0), capital(&g)?)?;
        assert_eq!(plan.primary, Focus::Offense);
        assert_eq!(plan.secondary, Some(Focus::Production));
        Ok(())
    }

    #[test]
    fn a_city_on_trade_goods_looks_at_every_building_it_can_build() -> TestResult {
        use f4x_core::wizard::objective::Objective;
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.wizard_mut(P0)
            .ok_or("player 0 is a wizard")?
            .put(Some(Objective::Militarist));
        crate::testworld::spawn(
            &mut g,
            P0,
            crate::testworld::at(10, 6),
            111, /* high_men_settlers */
            6,
        );
        let city = capital(&g)?;
        // A garrison stands, so that the city is free for its buildings.
        crate::testworld::spawn(
            &mut g,
            P0,
            crate::testworld::at(COAST_CITY.0, COAST_CITY.1),
            104, /* high_men_spearmen */
            2,
        );
        // An army that waits for orders is enough: the city builds no more soldiers.
        crate::testworld::spawn(&mut g, P0, crate::testworld::at(9, 9), 104, 4);
        g.apply(Command::SetProduction {
            city,
            item: ProductionItem::TradeGoods,
        });
        let mut ai = Ai::new(P0);
        ai.frontier_closed = true;
        crate::roles::register(&mut ai, &g);
        cities(&mut ai, &mut g, &mut Vec::new());
        // It looks at every building it can build: a library or a temple is chosen over trade goods.
        let item = g.economy().city_production(city).item;
        assert!(
            matches!(item, Some(ProductionItem::Building(_))),
            "{item:?}"
        );
        Ok(())
    }
}
