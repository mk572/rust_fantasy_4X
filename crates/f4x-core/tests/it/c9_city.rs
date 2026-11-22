//! Class 9, the city: what a player sees of one. A city grows and starves, builds and buys, sells
//! a building and renames itself. The numbers come from the shipped content and the game's own
//! tallies (`city_economy`); the rules the outcomes are checked against are written here from
//! the content, not read back from the code that keeps them.

use crate::support::worlds;
use f4x_core::city::production::{ProductionError, ProductionItem};
use f4x_core::game::{Command, Event, Game, Rejection};
use f4x_core::player::PlayerId;
use f4x_core::rules::{BuildingId, UnitId};
use f4x_def::half_int::HalfInt;
use f4x_def::rules::UnitKind;

/// The wizards of a game.
fn wizards(g: &Game) -> Vec<PlayerId> {
    g.player_ids()
        .into_iter()
        .filter(|&p| !g.is_neutral_player(p) && g.wizard(p).is_some())
        .collect()
}

/// The cities of a wizard.
fn cities_of(g: &Game, p: PlayerId) -> Vec<hecs::Entity> {
    g.economy().cities_of(p).into_iter().map(|c| c.0).collect()
}

fn gold(g: &Game, p: PlayerId) -> HalfInt {
    g.player(p).expect("a player").treasury().gold
}

fn refusal(events: &[Event]) -> Option<Rejection> {
    events.iter().find_map(|e| match e {
        Event::Rejected(r) => Some(*r),
        _ => None,
    })
}

/// Where in a round something is asked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    BeforeStart,
    BeforeEnd,
    AfterEnd,
}

/// One round as the master runs it, player by player, with `look` called at each step of each player's
/// turn. The events of the round come back.
fn round(g: &mut Game, look: &mut dyn FnMut(&mut Game, PlayerId, Phase)) -> Vec<Event> {
    let mut events = g.start_round();
    for p in g.turn_players() {
        look(g, p, Phase::BeforeStart);
        events.extend(g.start_player_turn(p));
        look(g, p, Phase::BeforeEnd);
        events.extend(g.end_player_turn(p));
        look(g, p, Phase::AfterEnd);
    }
    events.extend(g.end_round());
    events
}

// ---- what a city may build ----

/// Whether `item` may be built in `city`, from the content: the race, the buildings it needs, the shore,
/// the citizens a settler takes with it.
fn may_build(g: &Game, city: hecs::Entity, item: ProductionItem) -> Result<(), ProductionError> {
    let rules = &g.content().rules;
    let eco = g.economy();
    let race = eco.city_race(city).expect("a race");
    let have = |b: &BuildingId| eco.has_building(city, *b);
    match item {
        ProductionItem::TradeGoods | ProductionItem::Housing => Ok(()),
        ProductionItem::Building(b) => {
            let def = rules.buildings.get(b);
            if have(&b) {
                Err(ProductionError::AlreadyBuilt)
            } else if !def.races.contains(&race) {
                Err(ProductionError::WrongRace)
            } else if def.coastal && !eco.city_is_coastal(city) {
                Err(ProductionError::NotCoastal)
            } else if !def.requires.iter().all(have) {
                Err(ProductionError::MissingBuilding)
            } else {
                Ok(())
            }
        }
        ProductionItem::Unit(u) => {
            let def = rules.units.get(u);
            let may = match def.kind {
                UnitKind::Normal => rules.races.get(race).units.contains(&u),
                UnitKind::Fantastic => true,
                UnitKind::Hero | UnitKind::Summoned => false,
            };
            if !may {
                Err(ProductionError::WrongRace)
            } else if def.movement == f4x_def::rules::MovementType::Sailing
                && !eco.city_is_coastal(city)
            {
                Err(ProductionError::NotCoastal)
            } else if eco.costs_a_citizen(u) && g.city_population(city).figures < 2 {
                Err(ProductionError::TooSmall)
            } else if !def.requires.iter().all(have) {
                Err(ProductionError::MissingBuilding)
            } else {
                Ok(())
            }
        }
    }
}

fn every_item(g: &Game) -> Vec<ProductionItem> {
    let rules = &g.content().rules;
    let mut items = vec![ProductionItem::TradeGoods, ProductionItem::Housing];
    items.extend(
        rules
            .buildings
            .iter()
            .map(|(_, d)| ProductionItem::Building(BuildingId::new(d.num))),
    );
    items.extend(
        rules
            .units
            .defs()
            .iter()
            .map(|d| ProductionItem::Unit(UnitId::new(d.num))),
    );
    items
}

#[test]
fn a_city_takes_what_the_content_lets_it_build_and_refuses_the_rest_with_the_reason() {
    let mut accepted = 0;
    let mut reasons = std::collections::BTreeSet::new();
    for (name, mut g) in worlds() {
        let items = every_item(&g);
        for p in wizards(&g) {
            for city in cities_of(&g, p) {
                for &item in &items {
                    let before = g.economy().city_production(city);
                    let events = g.apply(Command::SetProduction { city, item });
                    match (may_build(&g, city, item), refusal(&events)) {
                        (Ok(()), None) => {
                            accepted += 1;
                            assert_eq!(
                                g.economy().city_production(city).item,
                                Some(item),
                                "{name}: {item:?}"
                            );
                        }
                        (Err(why), Some(Rejection::Production(got))) => {
                            reasons.insert(format!("{why:?}"));
                            assert_eq!(got, why, "{name}: {item:?}");
                            assert_eq!(g.economy().city_production(city), before, "{name}");
                        }
                        (want, got) => panic!("{name}: {item:?}: wanted {want:?}, got {got:?}"),
                    }
                }
            }
        }
    }
    assert!(accepted > 100, "only {accepted} items were accepted");
    // The kinds of refusal the shipped content gives a fresh city.
    for kind in ["AlreadyBuilt", "WrongRace", "MissingBuilding"] {
        assert!(
            reasons.contains(kind),
            "{kind} was never given: {reasons:?}"
        );
    }
}

// ---- building ----

/// The cheapest building the city may build (its cost, from the content).
fn cheapest_building(g: &Game, city: hecs::Entity) -> (BuildingId, u32) {
    g.content()
        .rules
        .buildings
        .iter()
        .map(|(_, d)| (BuildingId::new(d.num), d.cost))
        .filter(|&(b, _)| may_build(g, city, ProductionItem::Building(b)).is_ok())
        .min_by_key(|&(_, cost)| cost)
        .expect("a building the city may build")
}

#[test]
fn an_item_is_finished_the_turn_the_production_covers_its_cost() {
    let mut completed = 0;
    for (name, mut g) in worlds().into_iter().step_by(2) {
        let p = wizards(&g)[0];
        let city = cities_of(&g, p)[0];
        let (building, cost) = cheapest_building(&g, city);
        g.apply(Command::SetProduction {
            city,
            item: ProductionItem::Building(building),
        });
        let cost = HalfInt::from_int(cost as i32);
        let mut turns = 0;
        loop {
            turns += 1;
            assert!(turns < 400, "{name}: the building was never finished");
            let mut seen = None;
            let events = round(&mut g, &mut |g, who, phase| {
                if who == p && phase == Phase::BeforeEnd {
                    let eco = g.economy();
                    let gathered = eco.city_production(city).progress;
                    let made = eco.city_economy(city).expect("an economy").production;
                    seen = Some((gathered, made));
                }
            });
            let (gathered, made) = seen.expect("the turn was seen");
            let done = events.iter().any(|e| {
                matches!(e, Event::ProductionCompleted { city: c, item }
                    if *c == city && *item == ProductionItem::Building(building))
            });
            // Done exactly when what was gathered and what the city made cover the cost.
            assert_eq!(
                done,
                gathered + made >= cost,
                "{name}, turn {turns}: {gathered:?} and {made:?} against {cost:?}"
            );
            if done {
                assert!(g.economy().has_building(city, building), "{name}");
                assert_eq!(g.economy().city_production(city).progress, HalfInt::ZERO);
                completed += 1;
                break;
            }
            assert_eq!(
                g.economy().city_production(city).progress,
                gathered + made,
                "{name}, turn {turns}: the production was not added up"
            );
        }
    }
    assert!(completed >= 2);
}

#[test]
fn a_unit_appears_in_the_city_when_it_is_finished_and_a_settler_takes_a_citizen() {
    let mut finished = 0;
    for (name, mut g) in worlds().into_iter().step_by(2) {
        let p = wizards(&g)[0];
        let city = cities_of(&g, p)[0];
        let at = g.economy().cities_of(p)[0].1;
        let units: Vec<UnitId> = g
            .content()
            .rules
            .units
            .defs()
            .iter()
            .map(|d| UnitId::new(d.num))
            .filter(|&u| may_build(&g, city, ProductionItem::Unit(u)).is_ok())
            .collect();
        assert!(!units.is_empty(), "{name}: the city builds no unit");
        // The cheapest unit that costs no citizen, and the cheapest that does.
        for want_citizen in [false, true] {
            let pick = units
                .iter()
                .copied()
                .filter(|&u| g.economy().costs_a_citizen(u) == want_citizen)
                .min_by_key(|&u| g.content().rules.units.get(u).cost);
            let Some(unit) = pick else { continue };
            let size = g.city_population(city).figures.max(2);
            g.scenario_set_citizens(city, size);
            g.apply(Command::SetProduction {
                city,
                item: ProductionItem::Unit(unit),
            });
            let here = g.units().units_at(at).len();
            let mut turns = 0;
            let mut grown = size;
            loop {
                turns += 1;
                assert!(turns < 400, "{name}: {unit:?} was never finished");
                let events = round(&mut g, &mut |_, _, _| {});
                if events
                    .iter()
                    .any(|e| matches!(e, Event::ProductionCompleted { city: c, .. } if *c == city))
                {
                    grown = g.city_population(city).figures;
                    break;
                }
            }
            assert_eq!(g.units().units_at(at).len(), here + 1, "{name}: {unit:?}");
            let made = *g.units().units_at(at).last().expect("a unit");
            assert_eq!(g.units().unit_owner(made), Some(p), "{name}");
            assert_eq!(g.units().unit_def(made).map(|d| d.num), Some(unit.num()));
            if want_citizen {
                assert!(grown <= size, "{name}: a settler did not take a citizen");
            }
            finished += 1;
        }
    }
    assert!(finished >= 2);
}

// ---- buying ----

/// What finishing the item costs, from the rule of the original: four times what is left from scratch,
/// three times when less than half of it is done, else two.
fn price(remaining: HalfInt, gathered: HalfInt) -> HalfInt {
    if gathered == HalfInt::ZERO {
        remaining * 4
    } else if gathered * 2 < remaining {
        remaining * 3
    } else {
        remaining * 2
    }
}

#[test]
fn buying_costs_what_the_rule_says_at_each_stage_and_finishes_the_item_at_the_turn_end() {
    let mut bought = 0;
    for (name, mut g) in worlds().into_iter().step_by(2) {
        let p = wizards(&g)[0];
        let city = cities_of(&g, p)[0];
        // The dearest building, so that it takes several turns and passes each stage.
        let (building, cost) = g
            .content()
            .rules
            .buildings
            .iter()
            .map(|(_, d)| (BuildingId::new(d.num), d.cost))
            .filter(|&(b, _)| may_build(&g, city, ProductionItem::Building(b)).is_ok())
            .max_by_key(|&(_, c)| c)
            .expect("a building");
        g.apply(Command::SetProduction {
            city,
            item: ProductionItem::Building(building),
        });
        let cost = HalfInt::from_int(cost as i32);
        for turn in 0..6 {
            let gathered = g.economy().city_production(city).progress;
            let remaining = (cost - gathered).max(HalfInt::ZERO);
            let wanted = price(remaining, gathered);
            // With too little gold nothing is bought and nothing changes.
            g.treasury_mut(p).expect("a treasury").gold = wanted - HalfInt::HALF;
            let before = (gold(&g, p), g.economy().city_production(city));
            let refused = refusal(&g.apply(Command::BuyProduction { city }));
            assert_eq!(
                refused,
                Some(Rejection::Production(ProductionError::NotEnoughGold)),
                "{name}, turn {turn}"
            );
            assert_eq!(
                (gold(&g, p), g.economy().city_production(city)),
                before,
                "{name}"
            );
            // With enough, the price is taken and the rest of the cost is made up.
            g.treasury_mut(p).expect("a treasury").gold = wanted + HalfInt::from_int(5);
            let events = g.apply(Command::BuyProduction { city });
            assert_eq!(refusal(&events), None, "{name}, turn {turn}");
            assert_eq!(
                gold(&g, p),
                HalfInt::from_int(5),
                "{name}, turn {turn}: {wanted:?} was not what it cost"
            );
            assert!(events.iter().any(|e| matches!(
                e,
                Event::ProductionBought { city: c, cost: paid, .. }
                    if *c == city && *paid == wanted.floor() as u32
            )));
            // Another purchase has nothing left to finish.
            assert_eq!(
                refusal(&g.apply(Command::BuyProduction { city })),
                Some(Rejection::Production(ProductionError::NothingToBuy)),
                "{name}"
            );
            let done = round(&mut g, &mut |_, _, _| {})
                .iter()
                .any(|e| matches!(e, Event::ProductionCompleted { city: c, .. } if *c == city));
            assert!(done, "{name}: the building bought was not finished");
            bought += 1;
            // The next stage: start the same kind of item again, a turn further on.
            g.scenario_set_building(city, building, false);
            g.apply(Command::SetProduction {
                city,
                item: ProductionItem::Building(building),
            });
            for _ in 0..turn {
                round(&mut g, &mut |_, _, _| {});
            }
        }
    }
    assert!(bought >= 12);
    // Nothing can be bought of what costs nothing.
    let (_, mut g) = worlds().remove(0);
    let p = wizards(&g)[0];
    let city = cities_of(&g, p)[0];
    for item in [ProductionItem::TradeGoods, ProductionItem::Housing] {
        g.apply(Command::SetProduction { city, item });
        assert_eq!(
            refusal(&g.apply(Command::BuyProduction { city })),
            Some(Rejection::Production(ProductionError::NothingToBuy)),
            "{item:?}"
        );
    }
}

// ---- selling ----

/// What a sale brings, from the content: a third of the cost, walls half of that, never under one.
fn sale(cost: u32, walls: bool) -> i32 {
    let third = cost / 3;
    (if walls { third / 2 } else { third }).max(1) as i32
}

#[test]
fn a_building_sells_for_a_third_of_its_cost_once_a_turn_and_not_while_another_needs_it() {
    let mut sold = 0;
    for (name, mut g) in worlds().into_iter().step_by(2) {
        let p = wizards(&g)[0];
        let city = cities_of(&g, p)[0];
        let rules = g.content().rules.clone();
        // A pair of buildings of the content, one needing the other, both of this city's race.
        let race = g.economy().city_race(city).expect("a race");
        let pair = rules.buildings.iter().find_map(|(_, d)| {
            let needs = *d.requires.first()?;
            (d.races.contains(&race) && rules.buildings.get(needs).races.contains(&race))
                .then_some((needs, BuildingId::new(d.num)))
        });
        let (base, upper) = pair.expect("a building that needs another");
        for b in [base, upper] {
            g.scenario_set_building(city, b, true);
        }
        // The one that is needed is not for sale.
        let before = gold(&g, p);
        assert_eq!(
            refusal(&g.apply(Command::SellBuilding {
                city,
                building: base
            })),
            Some(Rejection::Production(ProductionError::MissingBuilding)),
            "{name}"
        );
        assert_eq!(gold(&g, p), before);
        assert!(g.economy().has_building(city, base));
        // The one that needs it is: its price is a third of its cost.
        let def = rules.buildings.get(upper);
        let price = sale(def.cost, def.num == 35 /* city_walls */);
        assert_eq!(
            refusal(&g.apply(Command::SellBuilding {
                city,
                building: upper
            })),
            None
        );
        assert_eq!(gold(&g, p), before + HalfInt::from_int(price), "{name}");
        assert!(!g.economy().has_building(city, upper));
        sold += 1;
        // Only one a turn: the base is free of what needed it, and still not for sale today.
        assert_eq!(
            refusal(&g.apply(Command::SellBuilding {
                city,
                building: base
            })),
            Some(Rejection::Production(ProductionError::AlreadySold)),
            "{name}"
        );
        // The next turn of the wizard opens the sale again.
        g.start_round();
        g.start_player_turn(p);
        let def = rules.buildings.get(base);
        let price = sale(def.cost, def.num == 35);
        let before = gold(&g, p);
        assert_eq!(
            refusal(&g.apply(Command::SellBuilding {
                city,
                building: base
            })),
            None
        );
        assert_eq!(gold(&g, p), before + HalfInt::from_int(price), "{name}");
        sold += 1;
    }
    assert!(sold >= 4);
}

// ---- growth ----

/// The people of a city after a turn, from the definition: tens of a hundred make a citizen up to the cap,
/// and a city never goes below one citizen.
fn after_growth(figures: u32, tens: u32, delta: i32, cap: u32) -> (u32, u32) {
    let total = (i64::from(figures) * 100 + i64::from(tens) + i64::from(delta))
        .clamp(100, i64::from(cap) * 100 + 99);
    ((total / 100) as u32, (total % 100) as u32)
}

/// The worlds, each with the size to put its cities at: one citizen, half the cap and the cap.
fn sizes() -> Vec<(String, Game, u32)> {
    let mut out = Vec::new();
    for fraction in [0, 1, 2] {
        for (name, g) in worlds().into_iter().step_by(2) {
            let cap = g.economy().max_city_size();
            out.push((
                format!("{name}, size {fraction}"),
                g,
                [1, cap / 2, cap][fraction],
            ));
        }
    }
    out
}

#[test]
fn a_city_grows_or_shrinks_by_what_its_economy_says_and_stays_within_one_and_the_cap() {
    let mut grew = 0;
    let mut shrank = 0;
    for (name, mut g, size) in sizes() {
        let cap = g.economy().max_city_size();
        let p = wizards(&g)[0];
        for city in cities_of(&g, p) {
            g.scenario_set_citizens(city, size.min(cap));
        }
        for turn in 1..=12 {
            let mut before = Vec::new();
            let events = round(&mut g, &mut |g, who, phase| {
                if who == p && phase == Phase::BeforeStart {
                    before = cities_of(g, p)
                        .into_iter()
                        .map(|c| {
                            let pop = g.city_population(c);
                            let delta = g.economy().city_economy(c).map_or(0, |e| e.growth_tens);
                            (c, pop.figures, pop.tens, delta)
                        })
                        .collect();
                }
            });
            for (city, figures, tens, delta) in before {
                let now = g.city_population(city);
                let want = after_growth(figures, tens, delta, cap);
                assert_eq!(
                    (now.figures, now.tens),
                    want,
                    "{name}, turn {turn}: {figures} citizens and {tens} tens, {delta} tens a turn"
                );
                let said_grew = events
                    .iter()
                    .any(|e| matches!(e, Event::CityGrew { city: c, .. } if *c == city));
                let said_shrank = events
                    .iter()
                    .any(|e| matches!(e, Event::CityShrank { city: c, .. } if *c == city));
                assert_eq!(said_grew, now.figures > figures, "{name}: CityGrew");
                assert_eq!(said_shrank, now.figures < figures, "{name}: CityShrank");
                grew += usize::from(now.figures > figures);
                shrank += usize::from(now.figures < figures);
            }
        }
    }
    assert!(
        grew > 0 && shrank > 0,
        "{grew} cities grew, {shrank} shrank"
    );
}

// ---- farmers and names ----

#[test]
fn farmers_are_kept_between_what_the_city_needs_and_its_people() {
    for (name, mut g) in worlds().into_iter().step_by(2) {
        let p = wizards(&g)[0];
        for city in cities_of(&g, p) {
            let people = g.city_population(city).figures;
            let min = g.economy().city_min_farmers(city);
            assert!(min <= people, "{name}: more farmers needed than people");
            let farmers = |g: &Game| g.city_farmers(city);
            for want in [0, min, people, people + 5, u32::MAX] {
                g.apply(Command::SetFarmers {
                    city,
                    farmers: want,
                });
                assert_eq!(
                    farmers(&g),
                    want.clamp(min, people),
                    "{name}: asked for {want}"
                );
            }
            // More farmers feed more: the food never falls as one more citizen goes to the fields.
            let mut last = None;
            for f in min..=people {
                let food = g
                    .economy()
                    .city_economy_with(city, f)
                    .expect("an economy")
                    .food;
                if let Some(l) = last {
                    assert!(
                        food >= l,
                        "{name}: {f} farmers make less food than {}",
                        f - 1
                    );
                }
                last = Some(food);
            }
        }
    }
}

#[test]
fn a_city_is_renamed_trimmed_and_cut_and_an_empty_name_changes_nothing() {
    use f4x_core::city::NAME_LEN;
    let (_, mut g) = worlds().remove(0);
    let p = wizards(&g)[0];
    let city = cities_of(&g, p)[0];
    let name_of = |g: &Game| g.city_name(city).expect("a name");
    let was = name_of(&g);
    for (given, wanted) in [
        ("  Newtown  ".to_string(), "Newtown".to_string()),
        ("x".repeat(NAME_LEN + 8), "x".repeat(NAME_LEN)),
        (
            "äöü".repeat(NAME_LEN),
            "äöü".repeat(NAME_LEN).chars().take(NAME_LEN).collect(),
        ),
    ] {
        g.apply(Command::RenameCity { city, name: given });
        assert_eq!(name_of(&g), wanted);
    }
    let now = name_of(&g);
    for empty in ["", "   "] {
        g.apply(Command::RenameCity {
            city,
            name: empty.into(),
        });
        assert_eq!(name_of(&g), now, "{empty:?}");
    }
    assert_ne!(was, now);
}
