//! Class 9, the random events (`docs/events.md`): the draw each turn, whom it falls on, what each
//! event does, how the lasting ones end and who is told. The events are made to happen with
//! `cheat_event`, which draws them as the game does; the turns pass with `start_round`.

use crate::support::{another_city, around, content, text, worlds};
use f4x_core::check::world_problems;
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_core::rules::{RealmId, SpellId, UnitId};
use f4x_core::world::save::Snapshot;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event, World};
use f4x_core::world_event::{
    donation, fires, flat_end, pick_victim, pirates_take, scaled_pressure, victim_weights,
    world_end, EventKind, EventNotice, Voice,
};
use f4x_def::half_int::HalfInt;
use f4x_def::testdir::{deep, sample};

const SPEARMEN: u16 = 104; // high_men_spearmen
const MAGIC_SPIRIT: u16 = 154; // magic_spirit
const CRUSADE: u16 = 158; // crusade

struct Game {
    name: String,
    w: Table,
    /// The human wizard.
    p: PlayerId,
    /// A computer wizard the human has not met.
    other: PlayerId,
}

/// The worlds with random events off (the tests make their own) and the first wizard human.
fn games() -> Vec<Game> {
    worlds()
        .into_iter()
        .map(|(name, mut w)| {
            let ids = w.wizards();
            w.set_human(Some(ids[0]));
            w.scenario_change_settings(|g| g.events.random_events = false);
            Game {
                name,
                w,
                p: ids[0],
                other: ids[1],
            }
        })
        .collect()
}

fn notices(events: &[Event]) -> Vec<EventNotice> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::RandomEvent(n) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

fn happen(w: &mut Table, kind: EventKind, victim: PlayerId) -> EventNotice {
    let told = w
        .cheat_event(kind, victim)
        .unwrap_or_else(|e| panic!("{e}"));
    let told = notices(&told);
    assert_eq!(told.len(), 1, "{kind:?}: {told:?}");
    told[0].clone()
}

fn refused(w: &mut Table, kind: EventKind, victim: PlayerId, name: &str) {
    let before = text(w);
    let result = w.cheat_event(kind, victim);
    assert!(result.is_err(), "{name}: {kind:?} happened: {result:?}");
    assert_eq!(text(w), before, "{name}: {kind:?} changed the game");
}

fn gold(w: &World, p: PlayerId) -> i32 {
    w.player(p).expect("a player").treasury().gold.floor()
}

fn set_gold(w: &mut Table, p: PlayerId, gold: i32) {
    w.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(gold);
}

fn create(w: &mut Table, owner: PlayerId, pos: Position, unit: u16) -> hecs::Entity {
    let before = w.units().units_at(pos);
    let events = w.apply(Command::CreateUnit {
        owner,
        pos,
        unit: UnitId::new(unit),
    });
    let made = w
        .units()
        .units_at(pos)
        .into_iter()
        .find(|u| !before.contains(u));
    made.unwrap_or_else(|| panic!("no unit was made: {events:?}"))
}

/// A second city of the player with `figures` citizens: not its fortress city.
fn town(g: &mut Game, figures: u32) -> (hecs::Entity, Position) {
    let city = another_city(&mut g.w, g.p, figures)
        .unwrap_or_else(|| panic!("{}: no room for a city", g.name));
    let at =
        g.w.economy()
            .cities_of(g.p)
            .into_iter()
            .find(|c| c.0 == city)
            .expect("the new city")
            .1;
    (city, at)
}

fn buildings(w: &World, city: hecs::Entity) -> usize {
    w.content()
        .rules
        .buildings
        .iter()
        .filter(|&(b, _)| w.economy().has_building(city, b))
        .count()
}

/// A round starts as the game starts it: the world is tidied up after, as after a command.
fn start_round(w: &mut Table, round: u32) -> Vec<Event> {
    let (world, rng) = w.world_and_rng();
    world.advance(|world| world.start_round(round, rng))
}

/// Rounds pass until `done` holds, at most `most`; the notices told on the way.
fn pass_until(w: &mut Table, most: u32, mut done: impl FnMut(&Table) -> bool) -> Vec<EventNotice> {
    let mut told = Vec::new();
    for round in 2..2 + most {
        told.extend(notices(&start_round(w, round)));
        if done(w) {
            return told;
        }
    }
    panic!("not done after {most} rounds");
}

// ---- the plain rules ----

#[test]
fn the_pressure_grows_with_the_quiet_turns_and_the_settings() {
    use f4x_core::world::setting::EventFrequency as F;
    for percent in (10..=200).step_by(if deep() { 1 } else { 19 }) {
        for waited in 0..sample(1000, 200) as u32 {
            let [half, normal, double] =
                [F::Half, F::Normal, F::Double].map(|f| scaled_pressure(waited, percent, f));
            let what = format!("{waited} turns at {percent} %");
            assert!(half <= normal && normal <= double, "{what}");
            assert!(
                normal <= scaled_pressure(waited + 1, percent, F::Normal),
                "{what}"
            );
            assert!(
                normal <= scaled_pressure(waited, percent + 1, F::Normal),
                "{what}"
            );
        }
        assert_eq!(scaled_pressure(0, percent, F::Double), 0);
    }
    // No pressure never fires; a roll of one fires with any; the higher the likelier.
    for roll in 1..=512 {
        assert!(!fires(0, roll));
        assert!(fires(512, roll));
        assert!(!fires(roll - 1, roll));
    }
}

#[test]
fn good_events_favour_the_weak_and_bad_ones_the_strong() {
    let powers = [20, 80, 300, 900, 0];
    for good in [false, true] {
        let weights = victim_weights(&powers, good);
        assert!(weights.iter().all(|&w| w >= 0), "{weights:?}");
        assert!(weights.iter().sum::<i32>() <= 500, "{weights:?}");
        for (a, &pa) in powers.iter().enumerate() {
            for (b, &pb) in powers.iter().enumerate() {
                if pa < pb {
                    let weak_first = if good {
                        weights[a] >= weights[b]
                    } else {
                        weights[a] <= weights[b]
                    };
                    assert!(weak_first, "good {good}: {weights:?}");
                }
            }
        }
        // Each roll lands on one wizard, each wizard on as many rolls as its weight.
        let total: i32 = weights.iter().sum();
        let mut hits = vec![0; powers.len()];
        for roll in 1..=total {
            hits[pick_victim(&weights, roll).expect("a victim")] += 1;
        }
        assert_eq!(hits, weights, "good {good}");
        assert_eq!(pick_victim(&weights, total + 1), None);
    }
    assert_eq!(pick_victim(&[], 1), None);
}

#[test]
fn a_lasting_event_ends_only_after_its_first_turns_a_world_event_ever_likelier() {
    let ends = |turns, end: fn(u32, u32) -> bool| (1..=20).filter(|&r| end(r, turns)).count();
    let first = (0..40).find(|&t| ends(t, flat_end) > 0).expect("it ends");
    assert!(first > 0, "a plague can end the turn it starts");
    assert_eq!((0..40).find(|&t| ends(t, world_end) > 0), Some(first));
    for turns in first..40 {
        assert_eq!(ends(turns, flat_end), ends(first, flat_end), "{turns}");
        assert!(
            ends(turns, world_end) <= ends(turns + 1, world_end),
            "{turns}"
        );
    }
    assert_eq!(ends(40, world_end), 20, "a world event ends in the end");
    assert!(ends(40, flat_end) < 20);
}

#[test]
fn pirates_take_a_share_of_the_gold_and_a_donation_brings_some() {
    for gold in [100, 105, 999, 12_345] {
        for roll in 1..=21 {
            let taken = pirates_take(gold, roll);
            assert!(taken >= 0 && taken <= gold, "{gold} {roll}: {taken}");
            assert_eq!(taken % 10, 0, "{gold} {roll}");
            assert!(taken <= pirates_take(gold, roll + 1), "{gold} {roll}");
        }
    }
    for roll in 1..100 {
        assert!(donation(roll) > 0);
        assert!(donation(roll) < donation(roll + 1));
    }
}

#[test]
fn every_event_is_known_by_its_roll_and_its_name() {
    let all: Vec<EventKind> = EventKind::all().collect();
    for (i, &kind) in all.iter().enumerate() {
        let roll = i as u32 + 1;
        assert_eq!(EventKind::from_roll(roll), Some(kind));
        assert_eq!(kind.number(), roll);
        assert_eq!(EventKind::from_id(kind.id()), Some(kind));
        assert!(
            !kind.is_world() || (kind.lasts() && !kind.needs_city()),
            "{kind:?}"
        );
    }
    assert_eq!(EventKind::from_roll(0), None);
    assert_eq!(EventKind::from_roll(EventKind::count() + 1), None);
    assert_eq!(EventKind::from_id("nothing"), None);
}

// ---- gold and things ----

#[test]
fn a_donation_pays_the_wizard_what_the_notice_says() {
    for mut g in games() {
        let before = gold(&g.w, g.p);
        let told = happen(&mut g.w, EventKind::Donation, g.p);
        assert_eq!(told.voice, Voice::Started, "{}", g.name);
        assert_eq!(told.player, Some(g.p), "{}", g.name);
        assert_eq!(gold(&g.w, g.p) - before, told.number, "{}", g.name);
        assert!(
            (donation(1)..=donation(100)).contains(&told.number),
            "{}: {}",
            g.name,
            told.number
        );
    }
}

#[test]
fn piracy_takes_part_of_the_gold_and_leaves_a_pauper_alone() {
    for mut g in games() {
        set_gold(&mut g.w, g.p, 1000);
        let told = happen(&mut g.w, EventKind::Piracy, g.p);
        assert_eq!(1000 - gold(&g.w, g.p), told.number, "{}", g.name);
        assert!(
            (pirates_take(1000, 1)..=pirates_take(1000, 21)).contains(&told.number),
            "{}: {}",
            g.name,
            told.number
        );
        set_gold(&mut g.w, g.p, 0);
        refused(&mut g.w, EventKind::Piracy, g.p, &g.name);
    }
}

#[test]
fn the_gift_puts_an_item_in_the_wizards_armory() {
    for mut g in games() {
        let before = g.w.items().armory(g.p).items().count();
        let told = happen(&mut g.w, EventKind::Gift, g.p);
        let item = told.item.unwrap_or_else(|| panic!("{}: no item", g.name));
        let armory = g.w.items().armory(g.p);
        assert_eq!(armory.items().count(), before + 1, "{}", g.name);
        assert!(armory.items().any(|i| *i == item), "{}", g.name);
    }
}

#[test]
fn a_disjunction_needs_a_global_spell_and_ends_every_one() {
    for mut g in games() {
        refused(&mut g.w, EventKind::Disjunction, g.p, &g.name);
        let spell = SpellId::new(CRUSADE);
        g.w.scenario_add_global(g.other, spell);
        g.w.scenario_add_global(g.p, spell);
        let told = happen(&mut g.w, EventKind::Disjunction, g.p);
        assert_eq!(told.voice, Voice::Happened, "{}", g.name);
        assert_eq!(told.player, None, "{}", g.name);
        for p in [g.p, g.other] {
            assert!(!g.w.has_global(p, spell), "{}: {p:?}", g.name);
        }
    }
}

#[test]
fn minerals_are_found_and_used_up_in_the_wizards_land() {
    for mut g in games() {
        let minerals = |w: &World| -> Vec<(Position, u16)> {
            let map = w.map().expect("a map");
            w.economy()
                .cities_of(g.p)
                .into_iter()
                .flat_map(|(_, at)| w.economy().city_area(at))
                .filter_map(|(q, _)| {
                    let found = map.plane(q.plane).mineral(q.x as i32, q.y as i32)?;
                    Some((q, found.0))
                })
                .collect()
        };
        // Hills round the home, where minerals can lie.
        let (_, home) = g.w.economy().cities_of(g.p)[0];
        let hills =
            g.w.content()
                .terrains
                .first_of_class(f4x_core::map::terrain::TerrainClass::Hills, home.plane)
                .expect("hills");
        for q in around(&g.w, home) {
            g.w.cheat_set_feature(q, None);
            g.w.scenario_set_terrain(q, hills);
        }
        let before = minerals(&g.w);
        let told = happen(&mut g.w, EventKind::NewMinerals, g.p);
        let found = minerals(&g.w);
        assert_eq!(found.len(), before.len() + 1, "{}", g.name);
        assert!(told.city.is_some(), "{}", g.name);
        let new = found
            .iter()
            .find(|m| !before.contains(m))
            .expect("the new one");
        assert_eq!(told.ore, Some(new.1), "{}", g.name);
        assert!(
            g.w.content().rules.config.event_ores.contains(&new.1),
            "{}",
            g.name
        );
        let told = happen(&mut g.w, EventKind::Depletion, g.p);
        let left = minerals(&g.w);
        assert_eq!(left.len(), found.len() - 1, "{}", g.name);
        let gone = found
            .iter()
            .find(|m| !left.contains(m))
            .expect("the one gone");
        assert_eq!(told.ore, Some(gone.1), "{}", g.name);
    }
}

// ---- cities ----

#[test]
fn a_marriage_gives_a_neutral_city_and_its_units_to_the_wizard() {
    for mut g in games() {
        let cities = g.w.economy().cities_of(g.p).len();
        let neutrals: Vec<hecs::Entity> =
            g.w.economy()
                .cities()
                .into_iter()
                .filter(|c| g.w.is_neutral_player(c.1.owner))
                .map(|c| c.0)
                .collect();
        // A guard in every neutral city, so the one that marries has one.
        for &city in &neutrals {
            let info = g.w.economy().cities().into_iter().find(|c| c.0 == city);
            let (_, info, at) = info.expect("a city");
            create(&mut g.w, info.owner, at, SPEARMEN);
        }
        let told = happen(&mut g.w, EventKind::Marriage, g.p);
        let city = told.city.expect("the city");
        assert!(neutrals.contains(&city), "{}", g.name);
        assert_eq!(g.w.economy().city_owner(city), Some(g.p), "{}", g.name);
        assert_eq!(g.w.economy().cities_of(g.p).len(), cities + 1, "{}", g.name);
        let at =
            g.w.economy()
                .cities_of(g.p)
                .into_iter()
                .find(|c| c.0 == city)
                .expect("the city")
                .1;
        let guards = g.w.units().units_at(at);
        assert!(!guards.is_empty(), "{}", g.name);
        assert!(
            guards
                .iter()
                .all(|&u| g.w.units().unit_owner(u) == Some(g.p)),
            "{}",
            g.name
        );
        assert!(g.w.is_explored(g.p, at), "{}", g.name);
        assert_eq!(
            g.w.economy().city_production(city).item,
            Some(f4x_core::city::production::ProductionItem::TradeGoods),
            "{}",
            g.name
        );
        assert!(world_problems(&g.w).is_empty(), "{}", g.name);
    }
}

#[test]
fn a_rebel_city_goes_neutral_with_its_soldiers_and_dismisses_the_conjured() {
    for mut g in games() {
        // Only the fortress city: nothing can rise.
        refused(&mut g.w, EventKind::Rebellion, g.p, &g.name);
        let (city, at) = town(&mut g, 4);
        let soldiers = [(); 2].map(|_| create(&mut g.w, g.p, at, SPEARMEN));
        let spirit = create(&mut g.w, g.p, at, MAGIC_SPIRIT);
        // The draw picks one of the cities: the fortress city refuses, so try until it rises.
        let told = (0..200)
            .find_map(|_| g.w.cheat_event(EventKind::Rebellion, g.p).ok())
            .unwrap_or_else(|| panic!("{}: never rose", g.name));
        assert_eq!(notices(&told)[0].city, Some(city), "{}", g.name);
        let owner = g.w.economy().city_owner(city).expect("an owner");
        assert!(g.w.is_neutral_player(owner), "{}", g.name);
        for u in soldiers {
            assert_eq!(g.w.units().unit_owner(u), Some(owner), "{}", g.name);
        }
        assert!(!g.w.units().all_units().contains(&spirit), "{}", g.name);
        assert!(world_problems(&g.w).is_empty(), "{}", g.name);
    }
}

#[test]
fn a_hero_or_a_conjured_majority_holds_a_city_back_from_rebelling() {
    for with_hero in [false, true] {
        for mut g in games() {
            let (city, at) = town(&mut g, 4);
            if with_hero {
                g.w.cheat_hire_hero(g.p, 0, Some(at))
                    .unwrap_or_else(|e| panic!("{}: {e}", g.name));
            } else {
                create(&mut g.w, g.p, at, MAGIC_SPIRIT);
            }
            for _ in 0..60 {
                refused(&mut g.w, EventKind::Rebellion, g.p, &g.name);
            }
            assert_eq!(g.w.economy().city_owner(city), Some(g.p), "{}", g.name);
        }
    }
}

#[test]
fn a_meteor_and_an_earthquake_tell_what_the_city_lost() {
    for kind in [EventKind::GreatMeteor, EventKind::Earthquake] {
        // How many losses were seen at all, over the worlds.
        let mut lost = 0;
        for mut g in games() {
            for _ in 0..sample(6, 2) {
                let cities = g.w.economy().cities_of(g.p);
                for &(city, at) in &cities {
                    g.w.scenario_set_citizens(city, 12);
                    for _ in g.w.units().units_at(at).len()..6 {
                        create(&mut g.w, g.p, at, SPEARMEN);
                    }
                }
                let measure = |w: &Table, city, at| {
                    (
                        w.units().units_at(at).len() as u32,
                        buildings(w, city) as u32,
                        w.city_population(city).figures,
                    )
                };
                let before: Vec<_> = cities.iter().map(|&(c, at)| measure(&g.w, c, at)).collect();
                let Ok(events) = g.w.cheat_event(kind, g.p) else {
                    continue;
                };
                let Some(told) = notices(&events).pop() else {
                    // An earthquake that took nothing is not told of.
                    assert_eq!(kind, EventKind::Earthquake, "{}", g.name);
                    continue;
                };
                let city = told.city.expect("a city");
                let i = cities.iter().position(|c| c.0 == city).expect("its own");
                let after = measure(&g.w, city, cities[i].1);
                assert_eq!(
                    (told.lost_units, told.lost_buildings, told.lost_population),
                    (
                        before[i].0 - after.0,
                        before[i].1 - after.1,
                        before[i].2 - after.2
                    ),
                    "{}: {kind:?}",
                    g.name
                );
                lost += told.lost_units + told.lost_buildings + told.lost_population;
                assert!(world_problems(&g.w).is_empty(), "{}", g.name);
            }
        }
        assert!(lost > 0, "{kind:?} never took anything");
    }
}

#[test]
fn a_plague_thins_its_city_but_not_below_two_and_ends() {
    for mut g in games() {
        let (city, home) = g.w.economy().cities_of(g.p)[0];
        g.w.scenario_set_citizens(city, 12);
        let told = happen(&mut g.w, EventKind::Plague, g.p);
        assert_eq!(told.voice, Voice::Started, "{}", g.name);
        assert_eq!(told.city, Some(city), "{} {home:?}", g.name);
        let serial = g.w.units().serial_of(city);
        assert!(
            g.w.events().on_city(EventKind::Plague, serial),
            "{}",
            g.name
        );
        refused(&mut g.w, EventKind::Plague, g.p, &g.name);
        // The one city is struck: no boom can share it.
        refused(&mut g.w, EventKind::PopulationBoom, g.p, &g.name);
        let mut fewest = 12;
        let told = pass_until(&mut g.w, 600, |w| {
            fewest = fewest.min(w.city_population(city).figures);
            !w.events().is_ongoing(EventKind::Plague)
        });
        assert!((2..12).contains(&fewest), "{}: {fewest}", g.name);
        assert_eq!(told.len(), 1, "{}: {told:?}", g.name);
        assert_eq!(told[0].voice, Voice::Ended, "{}", g.name);
    }
}

#[test]
fn a_plague_on_a_city_of_one_ends_and_leaves_it_two() {
    for mut g in games() {
        let (city, _) = g.w.economy().cities_of(g.p)[0];
        happen(&mut g.w, EventKind::Plague, g.p);
        g.w.scenario_set_citizens(city, 1);
        start_round(&mut g.w, 2);
        assert_eq!(g.w.city_population(city).figures, 2, "{}", g.name);
        assert!(!g.w.events().is_ongoing(EventKind::Plague), "{}", g.name);
    }
}

#[test]
fn a_population_boom_speeds_the_growth_of_its_city_until_it_ends() {
    for mut g in games() {
        let (city, _) = g.w.economy().cities_of(g.p)[0];
        let growth = |w: &World| w.economy().city_economy(city).expect("a city").growth_tens;
        let before = growth(&g.w);
        assert!(before > 0, "{}: the city does not grow", g.name);
        happen(&mut g.w, EventKind::PopulationBoom, g.p);
        assert!(growth(&g.w) > before, "{}", g.name);
        refused(&mut g.w, EventKind::Plague, g.p, &g.name);
        let told = pass_until(&mut g.w, 600, |w| {
            !w.events().is_ongoing(EventKind::PopulationBoom)
        });
        assert_eq!(told.len(), 1, "{}: {told:?}", g.name);
        assert_eq!(told[0].voice, Voice::Ended, "{}", g.name);
        assert_eq!(growth(&g.w), before, "{}", g.name);
    }
}

// ---- the world events ----

#[test]
fn only_one_world_event_goes_on_at_a_time_and_each_ends_with_a_message() {
    for kind in EventKind::all().filter(|k| k.is_world()) {
        for mut g in games() {
            let told = happen(&mut g.w, kind, g.other);
            assert_eq!(told.voice, Voice::Happened, "{}: {kind:?}", g.name);
            assert_eq!(told.player, None, "{}: {kind:?}", g.name);
            assert_eq!(g.w.events().world_event(), Some(kind), "{}", g.name);
            for second in EventKind::all().filter(|k| k.is_world()) {
                refused(&mut g.w, second, g.p, &g.name);
            }
            // Another sort of event can happen beside it.
            happen(&mut g.w, EventKind::Donation, g.p);
            let told = pass_until(&mut g.w, 200, |w| w.events().world_event().is_none());
            assert_eq!(told.len(), 1, "{}: {kind:?} {told:?}", g.name);
            assert_eq!((told[0].kind, told[0].voice), (kind, Voice::Ended));
        }
    }
}

#[test]
fn a_conjunction_doubles_its_own_nodes_and_halves_the_others() {
    let conjunctions = [
        (EventKind::ConjunctionNature, 0),
        (EventKind::ConjunctionSorcery, 1),
        (EventKind::ConjunctionChaos, 2),
    ];
    for (kind, realm) in conjunctions {
        for mut g in games() {
            let nodes: Vec<_> = g.w.economy().nodes();
            // Every free node melded.
            for node in &nodes {
                if g.w.units().units_at(node.pos).is_empty() {
                    let spirit = create(&mut g.w, g.p, node.pos, MAGIC_SPIRIT);
                    g.w.apply(Command::Meld {
                        units: vec![spirit],
                    });
                }
            }
            let power = |w: &World| -> Vec<(RealmId, i32)> {
                w.economy()
                    .nodes()
                    .iter()
                    .filter(|n| n.owner == Some(g.p))
                    .map(|n| (n.realm, w.node_power(n)))
                    .collect()
            };
            let before = power(&g.w);
            assert!(!before.is_empty(), "{}: no node melded", g.name);
            happen(&mut g.w, kind, g.p);
            for (&(r, was), (_, now)) in before.iter().zip(power(&g.w)) {
                let wanted = if r == RealmId::new(realm) {
                    was * 2
                } else {
                    was / 2
                };
                assert_eq!(now, wanted, "{}: {kind:?}, realm {r:?}", g.name);
            }
        }
    }
}

#[test]
fn mana_short_dries_up_the_power_of_every_wizard_while_it_lasts() {
    for mut g in games() {
        let power = |w: &World| -> Vec<HalfInt> {
            w.wizards()
                .into_iter()
                .map(|p| w.income(p).power_base)
                .collect()
        };
        let before = power(&g.w);
        assert!(before.iter().all(|&p| p > 0), "{}: {before:?}", g.name);
        happen(&mut g.w, EventKind::ManaShort, g.p);
        assert!(g.w.events().mana_is_short(), "{}", g.name);
        assert!(
            power(&g.w).iter().all(|&p| p == HalfInt::ZERO),
            "{}: {:?}",
            g.name,
            power(&g.w)
        );
    }
}

#[test]
fn a_moon_raises_the_religious_mana_of_its_realm_and_lowers_the_other() {
    for (kind, favoured_life) in [(EventKind::GoodMoon, true), (EventKind::BadMoon, false)] {
        let (_, mut w) = worlds().remove(0);
        let plain = w.events().clone();
        w.scenario_start_event(kind);
        let events = w.events();
        for religious in 0..30 {
            let change = |life, death| events.moon_mana(religious, life, death);
            let (mine, theirs) = if favoured_life {
                (change(true, false), change(false, true))
            } else {
                (change(false, true), change(true, false))
            };
            assert_eq!(mine, religious, "{kind:?}: doubled");
            assert_eq!(theirs, -(religious / 2), "{kind:?}: halved");
            assert_eq!(change(true, true), mine + theirs, "{kind:?}: both");
            assert_eq!(change(false, false), 0, "{kind:?}: neither");
            assert_eq!(plain.moon_mana(religious, true, true), 0);
        }
    }
}

// ---- who is told ----

#[test]
fn news_of_another_wizard_reaches_only_those_who_have_met_it() {
    for mut g in games() {
        let before = gold(&g.w, g.other);
        let events =
            g.w.cheat_event(EventKind::Donation, g.other)
                .expect("a donation");
        assert_eq!(notices(&events), Vec::new(), "{}", g.name);
        assert!(gold(&g.w, g.other) > before, "{}: but it happened", g.name);
        g.w.make_contact(g.p, g.other);
        let told = happen(&mut g.w, EventKind::Donation, g.other);
        assert_eq!(told.voice, Voice::Elsewhere, "{}", g.name);
        assert_eq!(told.player, Some(g.other), "{}", g.name);
        // A game with no human tells nobody.
        g.w.set_human(None);
        for kind in [EventKind::Donation, EventKind::ManaShort] {
            let events = g.w.cheat_event(kind, g.p).expect("an event");
            assert_eq!(notices(&events), Vec::new(), "{}: {kind:?}", g.name);
        }
        assert!(g.w.events().mana_is_short(), "{}", g.name);
    }
}

// ---- the draw ----

/// The rounds of a game with events often, with what was told and the state kept sound.
fn drawn(g: &mut Game, rounds: u32, mut each: impl FnMut(&Table, u32, &[EventNotice])) {
    g.w.scenario_change_settings(|s| {
        s.events.random_events = true;
        s.events.frequency = f4x_core::world::setting::EventFrequency::Double;
        s.events.pressure_percent = 200;
    });
    for p in g.w.wizards() {
        if p != g.p {
            g.w.make_contact(g.p, p);
        }
    }
    for round in 1..=rounds {
        let told = notices(&start_round(&mut g.w, round));
        each(&g.w, round, &told);
    }
}

#[test]
fn no_event_is_drawn_with_random_events_off_or_before_their_first_turn() {
    for mut g in games() {
        for round in 1..=sample(600, 200) as u32 {
            assert_eq!(
                notices(&start_round(&mut g.w, round)),
                Vec::new(),
                "{}",
                g.name
            );
        }
        assert_eq!(g.w.events().ongoing().count(), 0, "{}", g.name);
        assert_eq!(g.w.events().last_event(), 0, "{}", g.name);
    }
    for start in [2, 10, 40] {
        for mut g in games() {
            g.w.scenario_change_settings(|s| s.events.start_turn = start);
            drawn(&mut g, u32::from(start) - 1, |w, round, told| {
                assert_eq!(told, &[], "round {round} of a start at {start}");
                assert_eq!(w.events().last_event(), 0);
            });
        }
    }
}

#[test]
fn drawn_events_come_often_enough_and_leave_a_sound_world() {
    let mut seen = std::collections::BTreeSet::new();
    for mut g in games() {
        let big = g.w.content().rules.config.threat.big_events_from_turn;
        let name = g.name.clone();
        let mut last = 0;
        drawn(&mut g, big + sample(1500, 300) as u32, |w, round, told| {
            let new = w.events().last_event() != last;
            last = w.events().last_event();
            if new {
                assert_eq!(last, round, "{name}");
            }
            for n in told {
                if matches!(n.kind, EventKind::Marriage | EventKind::GreatMeteor) {
                    assert!(round >= big, "{name}: {:?} in round {round}", n.kind);
                }
                seen.insert(n.kind.id());
            }
            assert!(
                w.events().ongoing().filter(|e| e.kind.is_world()).count() <= 1,
                "{name}: round {round}"
            );
            if round % 50 == 0 {
                let bad = world_problems(w);
                assert!(bad.is_empty(), "{name}: round {round}: {bad:#?}");
            }
        });
        assert!(g.w.events().last_event() > 0, "{}", g.name);
    }
    assert!(seen.len() > 8, "only {seen:?} ever came");
}

#[test]
fn the_events_going_on_survive_a_save() -> Result<(), Box<dyn std::error::Error>> {
    for mut g in games() {
        happen(&mut g.w, EventKind::Plague, g.p);
        happen(&mut g.w, EventKind::BadMoon, g.p);
        for round in 2..4 {
            start_round(&mut g.w, round);
        }
        let saved = g.w.snapshot().to_text()?;
        let back = Snapshot::from_text(&saved)?.restore(content())?;
        assert_eq!(back.events(), g.w.events(), "{}", g.name);
        assert_eq!(text(&back), saved, "{}", g.name);
    }
    Ok(())
}
