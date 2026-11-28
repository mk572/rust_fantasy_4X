//! Class 9, the city: what a player sees of one. A city grows and starves, builds and buys, sells
//! a building and renames itself. The numbers come from the shipped content and the game's own
//! tallies (`city_economy`); the rules the outcomes are checked against are written here from
//! the content, not read back from the code that keeps them.

use f4x_core::city::production::{ProductionError, ProductionItem};
use f4x_core::player::PlayerId;
use f4x_core::rules::{BuildingId, UnitId};
use f4x_core::world::save::Snapshot;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event, Rejection, World};

use crate::support::{content, worlds};
use f4x_def::half_int::HalfInt;
use f4x_def::rules::UnitKind;

/// The wizards of a game.
fn wizards(w: &World) -> Vec<PlayerId> {
    w.player_ids()
        .into_iter()
        .filter(|&p| !w.is_neutral_player(p) && w.wizard(p).is_some())
        .collect()
}

/// The cities of a wizard.
fn cities_of(w: &World, p: PlayerId) -> Vec<hecs::Entity> {
    w.economy().cities_of(p).into_iter().map(|c| c.0).collect()
}

fn gold(w: &World, p: PlayerId) -> HalfInt {
    w.player(p).expect("a player").treasury().gold
}

fn refusal(events: &[Event]) -> Option<Rejection> {
    events.iter().find_map(|e| match e {
        Event::Rejected(r) => Some(*r),
        _ => None,
    })
}

// ---- what a city may build ----

/// Whether `item` may be built in `city`, from the content: the race, the buildings it needs, the shore,
/// the citizens a settler takes with it.
fn may_build(w: &World, city: hecs::Entity, item: ProductionItem) -> Result<(), ProductionError> {
    let rules = &w.content().rules;
    let eco = w.economy();
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
            } else if eco.costs_a_citizen(u) && w.city_population(city).figures < 2 {
                Err(ProductionError::TooSmall)
            } else if !def.requires.iter().all(have) {
                Err(ProductionError::MissingBuilding)
            } else {
                Ok(())
            }
        }
    }
}

fn every_item(w: &World) -> Vec<ProductionItem> {
    let rules = &w.content().rules;
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
    for (name, mut w) in worlds() {
        let items = every_item(&w);
        for p in wizards(&w) {
            for city in cities_of(&w, p) {
                for &item in &items {
                    let before = w.economy().city_production(city);
                    let events = w.apply(Command::SetProduction { city, item });
                    match (may_build(&w, city, item), refusal(&events)) {
                        (Ok(()), None) => {
                            accepted += 1;
                            assert_eq!(
                                w.economy().city_production(city).item,
                                Some(item),
                                "{name}: {item:?}"
                            );
                        }
                        (Err(why), Some(Rejection::Production(got))) => {
                            reasons.insert(format!("{why:?}"));
                            assert_eq!(got, why, "{name}: {item:?}");
                            assert_eq!(w.economy().city_production(city), before, "{name}");
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
fn cheapest_building(w: &World, city: hecs::Entity) -> (BuildingId, u32) {
    w.content()
        .rules
        .buildings
        .iter()
        .map(|(_, d)| (BuildingId::new(d.num), d.cost))
        .filter(|&(b, _)| may_build(w, city, ProductionItem::Building(b)).is_ok())
        .min_by_key(|&(_, cost)| cost)
        .expect("a building the city may build")
}

#[test]
fn an_item_is_finished_the_turn_the_production_covers_its_cost() {
    let mut completed = 0;
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let p = wizards(&w)[0];
        let city = cities_of(&w, p)[0];
        let (building, cost) = cheapest_building(&w, city);
        w.apply(Command::SetProduction {
            city,
            item: ProductionItem::Building(building),
        });
        let cost = HalfInt::from_int(cost as i32);
        let mut turns = 0;
        loop {
            turns += 1;
            assert!(turns < 400, "{name}: the building was never finished");
            let eco = w.economy();
            let gathered = eco.city_production(city).progress;
            let made = eco.city_economy(city).expect("an economy").production;
            let events = w.end_player_turn(p);
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
                assert!(w.economy().has_building(city, building), "{name}");
                assert_eq!(w.economy().city_production(city).progress, HalfInt::ZERO);
                completed += 1;
                break;
            }
            assert_eq!(
                w.economy().city_production(city).progress,
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
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let p = wizards(&w)[0];
        let city = cities_of(&w, p)[0];
        let at = w.economy().cities_of(p)[0].1;
        let units: Vec<UnitId> = w
            .content()
            .rules
            .units
            .defs()
            .iter()
            .map(|d| UnitId::new(d.num))
            .filter(|&u| may_build(&w, city, ProductionItem::Unit(u)).is_ok())
            .collect();
        assert!(!units.is_empty(), "{name}: the city builds no unit");
        // The cheapest unit that costs no citizen, and the cheapest that does.
        for want_citizen in [false, true] {
            let pick = units
                .iter()
                .copied()
                .filter(|&u| w.economy().costs_a_citizen(u) == want_citizen)
                .min_by_key(|&u| w.content().rules.units.get(u).cost);
            let Some(unit) = pick else { continue };
            let size = w.city_population(city).figures.max(2);
            w.scenario_set_citizens(city, size);
            w.apply(Command::SetProduction {
                city,
                item: ProductionItem::Unit(unit),
            });
            let here = w.units().units_at(at).len();
            let mut turns = 0;
            let grown;
            loop {
                turns += 1;
                assert!(turns < 400, "{name}: {unit:?} was never finished");
                let events = w.end_player_turn(p);
                if events
                    .iter()
                    .any(|e| matches!(e, Event::ProductionCompleted { city: c, .. } if *c == city))
                {
                    grown = w.city_population(city).figures;
                    break;
                }
            }
            assert_eq!(w.units().units_at(at).len(), here + 1, "{name}: {unit:?}");
            let made = *w.units().units_at(at).last().expect("a unit");
            assert_eq!(w.units().unit_owner(made), Some(p), "{name}");
            assert_eq!(w.units().unit_def(made).map(|d| d.num), Some(unit.num()));
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
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let p = wizards(&w)[0];
        let city = cities_of(&w, p)[0];
        // The dearest building, so that it takes several turns and passes each stage.
        let (building, cost) = w
            .content()
            .rules
            .buildings
            .iter()
            .map(|(_, d)| (BuildingId::new(d.num), d.cost))
            .filter(|&(b, _)| may_build(&w, city, ProductionItem::Building(b)).is_ok())
            .max_by_key(|&(_, c)| c)
            .expect("a building");
        w.apply(Command::SetProduction {
            city,
            item: ProductionItem::Building(building),
        });
        let cost = HalfInt::from_int(cost as i32);
        for turn in 0..6 {
            let gathered = w.economy().city_production(city).progress;
            let remaining = (cost - gathered).max(HalfInt::ZERO);
            let wanted = price(remaining, gathered);
            // With too little gold nothing is bought and nothing changes.
            w.treasury_mut(p).expect("a treasury").gold = wanted - HalfInt::HALF;
            let before = (gold(&w, p), w.economy().city_production(city));
            let refused = refusal(&w.apply(Command::BuyProduction { city }));
            assert_eq!(
                refused,
                Some(Rejection::Production(ProductionError::NotEnoughGold)),
                "{name}, turn {turn}"
            );
            assert_eq!(
                (gold(&w, p), w.economy().city_production(city)),
                before,
                "{name}"
            );
            // With enough, the price is taken and the rest of the cost is made up.
            w.treasury_mut(p).expect("a treasury").gold = wanted + HalfInt::from_int(5);
            let events = w.apply(Command::BuyProduction { city });
            assert_eq!(refusal(&events), None, "{name}, turn {turn}");
            assert_eq!(
                gold(&w, p),
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
                refusal(&w.apply(Command::BuyProduction { city })),
                Some(Rejection::Production(ProductionError::NothingToBuy)),
                "{name}"
            );
            let done = w
                .end_player_turn(p)
                .iter()
                .any(|e| matches!(e, Event::ProductionCompleted { city: c, .. } if *c == city));
            assert!(done, "{name}: the building bought was not finished");
            bought += 1;
            // The next stage: start the same kind of item again, a turn further on.
            w.scenario_set_building(city, building, false);
            w.apply(Command::SetProduction {
                city,
                item: ProductionItem::Building(building),
            });
            for _ in 0..turn {
                w.end_player_turn(p);
            }
        }
    }
    assert!(bought >= 12);
    // Nothing can be bought of what costs nothing.
    let (_, mut w) = worlds().remove(0);
    let p = wizards(&w)[0];
    let city = cities_of(&w, p)[0];
    for item in [ProductionItem::TradeGoods, ProductionItem::Housing] {
        w.apply(Command::SetProduction { city, item });
        assert_eq!(
            refusal(&w.apply(Command::BuyProduction { city })),
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
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let p = wizards(&w)[0];
        let city = cities_of(&w, p)[0];
        let rules = w.content().rules.clone();
        // A pair of buildings of the content, one needing the other, both of this city's race.
        let race = w.economy().city_race(city).expect("a race");
        let pair = rules.buildings.iter().find_map(|(_, d)| {
            let needs = *d.requires.first()?;
            (d.races.contains(&race) && rules.buildings.get(needs).races.contains(&race))
                .then_some((needs, BuildingId::new(d.num)))
        });
        let (base, upper) = pair.expect("a building that needs another");
        for b in [base, upper] {
            w.scenario_set_building(city, b, true);
        }
        // The one that is needed is not for sale.
        let before = gold(&w, p);
        assert_eq!(
            refusal(&w.apply(Command::SellBuilding {
                city,
                building: base
            })),
            Some(Rejection::Production(ProductionError::MissingBuilding)),
            "{name}"
        );
        assert_eq!(gold(&w, p), before);
        assert!(w.economy().has_building(city, base));
        // The one that needs it is: its price is a third of its cost.
        let def = rules.buildings.get(upper);
        let price = sale(def.cost, def.num == 35 /* city_walls */);
        assert_eq!(
            refusal(&w.apply(Command::SellBuilding {
                city,
                building: upper
            })),
            None
        );
        assert_eq!(gold(&w, p), before + HalfInt::from_int(price), "{name}");
        assert!(!w.economy().has_building(city, upper));
        sold += 1;
        // Only one a turn: the base is free of what needed it, and still not for sale today.
        assert_eq!(
            refusal(&w.apply(Command::SellBuilding {
                city,
                building: base
            })),
            Some(Rejection::Production(ProductionError::AlreadySold)),
            "{name}"
        );
        // The next turn of the wizard opens the sale again.
        w.start_round(2);
        w.start_player_turn(p, 2);
        let def = rules.buildings.get(base);
        let price = sale(def.cost, def.num == 35);
        let before = gold(&w, p);
        assert_eq!(
            refusal(&w.apply(Command::SellBuilding {
                city,
                building: base
            })),
            None
        );
        assert_eq!(gold(&w, p), before + HalfInt::from_int(price), "{name}");
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
fn sizes() -> Vec<(String, Table, u32)> {
    let mut out = Vec::new();
    for fraction in [0, 1, 2] {
        for (name, w) in worlds().into_iter().step_by(2) {
            let cap = w.economy().max_city_size();
            out.push((
                format!("{name}, size {fraction}"),
                w,
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
    for (name, mut w, size) in sizes() {
        let cap = w.economy().max_city_size();
        let p = wizards(&w)[0];
        for city in cities_of(&w, p) {
            w.scenario_set_citizens(city, size.min(cap));
        }
        for turn in 1..=12 {
            let before: Vec<_> = cities_of(&w, p)
                .into_iter()
                .map(|c| {
                    let pop = w.city_population(c);
                    let delta = w.economy().city_economy(c).map_or(0, |e| e.growth_tens);
                    (c, pop.figures, pop.tens, delta)
                })
                .collect();
            let events = w.start_player_turn(p, turn + 1);
            for (city, figures, tens, delta) in before {
                let now = w.city_population(city);
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
            w.end_player_turn(p);
        }
    }
    assert!(
        grew > 0 && shrank > 0,
        "{grew} cities grew, {shrank} shrank"
    );
}

// ---- farmers and names ----

/// The game saves and loads: the loader refuses a city with more farmers than citizens.
fn saves_and_loads(w: &Table, name: &str) {
    let text = w.snapshot().to_text().expect("a save");
    let back = Snapshot::from_text(&text).and_then(|s| s.restore(content()));
    assert!(back.is_ok(), "{name}: {:?}", back.err());
}

#[test]
fn a_change_of_citizens_up_or_down_readjusts_the_farmers() {
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let p = wizards(&w)[0];
        for city in cities_of(&w, p) {
            let largest = u32::from(w.settings().rules.max_city_size);
            // From the fewest and from all farming, to fewer citizens and to more.
            for farm_all in [false, true] {
                for (from, to) in [(largest, 1), (largest, largest / 2), (1, largest)] {
                    assert!(w.cheat_set_citizens(city, from));
                    w.apply(Command::SetFarmers {
                        city,
                        farmers: if farm_all { u32::MAX } else { 0 },
                    });
                    let before = w.city_farmers(city);
                    assert!(w.cheat_set_citizens(city, to));
                    let min = w.economy().city_min_farmers(city);
                    assert_eq!(
                        w.city_farmers(city),
                        before.clamp(min, to),
                        "{name}: {before} farmers, {from} citizens to {to}"
                    );
                    saves_and_loads(&w, &name);
                }
            }
        }
    }
}

/// A settler takes a citizen with it (task 841: the farmers stayed, and the save did not load).
#[test]
fn a_city_where_all_farm_loses_a_farmer_with_its_settler() {
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let p = wizards(&w)[0];
        let city = cities_of(&w, p)[0];
        w.scenario_set_citizens(city, 3);
        let settler = w
            .content()
            .rules
            .units
            .defs()
            .iter()
            .map(|d| UnitId::new(d.num))
            .filter(|&u| w.economy().costs_a_citizen(u))
            .find(|&u| may_build(&w, city, ProductionItem::Unit(u)).is_ok())
            .expect("a settler the city builds");
        w.apply(Command::SetProduction {
            city,
            item: ProductionItem::Unit(settler),
        });
        let mut turns = 0;
        loop {
            turns += 1;
            assert!(turns < 400, "{name}: {settler:?} was never finished");
            w.apply(Command::SetFarmers {
                city,
                farmers: u32::MAX,
            });
            let people = w.city_population(city).figures;
            assert_eq!(w.city_farmers(city), people, "{name}: all farm");
            let events = w.end_player_turn(p);
            if events
                .iter()
                .any(|e| matches!(e, Event::ProductionCompleted { city: c, .. } if *c == city))
            {
                assert_eq!(w.city_population(city).figures, people - 1, "{name}");
                assert_eq!(w.city_farmers(city), people - 1, "{name}");
                break;
            }
        }
        saves_and_loads(&w, &name);
    }
}

#[test]
fn farmers_are_kept_between_what_the_city_needs_and_its_people() {
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let p = wizards(&w)[0];
        for city in cities_of(&w, p) {
            let people = w.city_population(city).figures;
            let min = w.economy().city_min_farmers(city);
            assert!(min <= people, "{name}: more farmers needed than people");
            let farmers = |w: &World| w.city_farmers(city);
            for want in [0, min, people, people + 5, u32::MAX] {
                w.apply(Command::SetFarmers {
                    city,
                    farmers: want,
                });
                assert_eq!(
                    farmers(&w),
                    want.clamp(min, people),
                    "{name}: asked for {want}"
                );
            }
            // More farmers feed more: the food never falls as one more citizen goes to the fields.
            let mut last = None;
            for f in min..=people {
                let food = w
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
    let (_, mut w) = worlds().remove(0);
    let p = wizards(&w)[0];
    let city = cities_of(&w, p)[0];
    let name_of = |w: &World| w.city_name(city).expect("a name");
    let was = name_of(&w);
    for (given, wanted) in [
        ("  Newtown  ".to_string(), "Newtown".to_string()),
        ("x".repeat(NAME_LEN + 8), "x".repeat(NAME_LEN)),
        (
            "äöü".repeat(NAME_LEN),
            "äöü".repeat(NAME_LEN).chars().take(NAME_LEN).collect(),
        ),
    ] {
        w.apply(Command::RenameCity { city, name: given });
        assert_eq!(name_of(&w), wanted);
    }
    let now = name_of(&w);
    for empty in ["", "   "] {
        w.apply(Command::RenameCity {
            city,
            name: empty.into(),
        });
        assert_eq!(name_of(&w), now, "{empty:?}");
    }
    assert_ne!(was, now);
}

/// The one shape of a city's area, which the game and the AI both ask for.
#[test]
fn a_city_works_the_squares_of_its_area_round_the_edge_of_the_map_and_no_corner() {
    use f4x_core::city::economy::{area_offsets, works, AREA_RADIUS};
    use f4x_core::map::{Plane, Position};
    let (width, y) = (60, 10);
    let at = |x: i32, y: i32| Position {
        plane: Plane::Arcanus,
        x: x.rem_euclid(width) as u32,
        y: y as u32,
    };
    let city = at(0, y);
    let reach = AREA_RADIUS + 1;
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let listed = area_offsets().any(|o| o == (dx, dy));
            assert_eq!(
                works(width as u32, city, at(dx, y + dy)),
                listed,
                "{dx},{dy}"
            );
        }
    }
    let corner = at(AREA_RADIUS, y + AREA_RADIUS);
    assert!(!works(width as u32, city, corner), "a corner");
}
