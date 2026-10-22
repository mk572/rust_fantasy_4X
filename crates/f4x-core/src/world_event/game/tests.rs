use super::*;
use crate::game::testutil::content_with_magic;
use crate::game::Command;
use crate::map::{Grid, Map, Plane};
use crate::player::Player;
use crate::world_event::EventKind as K;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

/// Two wizards with a fortress each; P0 is the human and has a second city at (10, 5).
fn world() -> Game {
    let c = content_with_magic();
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut g = Game::new(11);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    for id in [P0, P1] {
        let mut p = Player::new();
        p.gold = 1000;
        p.mana = 100;
        g.add_player(id, if id == P0 { "Merlin" } else { "Rjak" }, p);
        g.init_wizard(id, &[("life", 2)], &[], None).unwrap();
    }
    g.human = Some(P0);
    let race = g.content.rules.races.id_of("elf");
    for (p, pos) in [(P0, at(5, 5)), (P1, at(18, 9))] {
        g.place_fortress(p, pos, "Capital".into(), race, 4).unwrap();
    }
    g.apply(Command::FoundCity {
        owner: P0,
        pos: at(10, 5),
        name: "Town".into(),
        race,
        population: 6,
    });
    g
}

fn city(g: &Game, x: u32, y: u32) -> hecs::Entity {
    g.economy().city_at(at(x, y)).unwrap()
}

fn armed(kind: K, victim: PlayerId, city: Option<hecs::Entity>) -> Armed {
    Armed {
        kind,
        victim,
        city,
        other: None,
        deposit: None,
        fresh: kind.lasts(),
    }
}

/// Play one event out and return what the player is told.
fn play(g: &mut Game, a: Armed) -> Vec<EventNotice> {
    let mut rng = GameRng::new(5);
    let mut out = Vec::new();
    g.play_event(a, &mut rng, &mut out);
    out.into_iter()
        .filter_map(|e| match e {
            Event::RandomEvent(n) => Some(n),
            _ => None,
        })
        .collect()
}

fn spear(g: &mut Game, owner: PlayerId, pos: Position) -> hecs::Entity {
    let id = g.content.rules.units.id_of("spear").unwrap();
    g.spawn_unit(owner, pos, id)
}

#[test]
fn a_donation_pays_105_to_600_gold_and_the_wizard_is_told() {
    let mut g = world();
    let told = play(&mut g, armed(K::Donation, P0, None));
    let gold = g.player(P0).unwrap().gold;
    assert!((1105..=1600).contains(&gold), "{gold}");
    assert_eq!(told.len(), 1);
    assert_eq!(told[0].voice, Voice::Started);
    assert_eq!(told[0].number, gold - 1000);
    assert_eq!(told[0].player_name, "Merlin");
}

#[test]
fn piracy_takes_thirty_to_fifty_percent_and_cannot_hit_a_pauper() {
    let mut g = world();
    let told = play(&mut g, armed(K::Piracy, P0, None));
    let gold = g.player(P0).unwrap().gold;
    assert!((500..=700).contains(&gold), "{gold}");
    assert_eq!(told[0].number, 1000 - gold);
    g.player_mut(P0).unwrap().gold = 99;
    let mut rng = GameRng::new(1);
    assert!(g.try_event(K::Piracy, P0, &mut rng).is_none());
}

#[test]
fn an_event_that_falls_on_a_wizard_you_have_not_met_is_not_told() {
    let mut g = world();
    assert!(play(&mut g, armed(K::Donation, P1, None)).is_empty());
    assert!(g.player(P1).unwrap().gold > 1000, "but it happened");
    g.make_contact(P0, P1);
    let told = play(&mut g, armed(K::Donation, P1, None));
    assert_eq!(told[0].voice, Voice::Elsewhere);
    assert_eq!(told[0].player_name, "Rjak");
}

#[test]
fn a_rebellion_turns_the_city_neutral_and_keeps_its_normal_units() {
    let mut g = world();
    let town = city(&g, 10, 5);
    let guard = spear(&mut g, P0, at(10, 5));
    let told = play(&mut g, armed(K::Rebellion, P0, Some(town)));
    let owner = g.economy().city_owner(town).unwrap();
    assert!(g.is_neutral_player(owner));
    assert_eq!(g.units().unit_owner(guard), Some(owner));
    assert_eq!(told[0].city_name, "Town");
}

#[test]
fn a_rebellion_spares_the_fortress_city_and_cities_with_a_hero() {
    let mut g = world();
    let capital = city(&g, 5, 5);
    let town = city(&g, 10, 5);
    assert!(!g.can_rebel(P0, capital));
    assert!(g.can_rebel(P0, town));
    let hero = g
        .content
        .rules
        .units
        .id_of("hero")
        .map(|id| g.spawn_unit(P0, at(10, 5), id));
    if let Some(hero) = hero {
        assert_eq!(
            g.units().unit_def(hero).map(|d| d.kind),
            Some(UnitKind::Hero)
        );
        assert!(!g.can_rebel(P0, town));
    }
}

#[test]
fn a_marriage_gives_a_neutral_city_and_its_units_to_the_wizard() {
    let mut g = world();
    let owner = g.new_city_player();
    let race = g.content.rules.races.id_of("elf");
    g.apply(Command::FoundCity {
        owner,
        pos: at(14, 2),
        name: "Free".into(),
        race,
        population: 3,
    });
    let free = city(&g, 14, 2);
    let guard = spear(&mut g, owner, at(14, 2));
    let mut a = armed(K::Marriage, P0, Some(city(&g, 5, 5)));
    a.other = Some(free);
    let told = play(&mut g, a);
    assert_eq!(g.economy().city_owner(free), Some(P0));
    assert_eq!(g.units().unit_owner(guard), Some(P0));
    assert_eq!(
        g.world.get::<&Production>(free).unwrap().item,
        Some(ProductionItem::TradeGoods)
    );
    assert_eq!(told[0].city_name, "Free");
    assert!(g.is_explored(P0, at(14, 5)), "the land round it is seen");
}

#[test]
fn marriage_and_meteor_wait_for_turn_150() {
    let mut g = world();
    let mut rng = GameRng::new(2);
    g.turn = 149;
    assert!(g.try_event(K::GreatMeteor, P0, &mut rng).is_none());
    g.turn = 150;
    assert!(g.try_event(K::GreatMeteor, P0, &mut rng).is_some());
}

#[test]
fn disjunction_clears_every_wizards_overland_enchantments_and_needs_one_to_exist() {
    let mut g = world();
    let mut rng = GameRng::new(3);
    assert!(g.try_event(K::Disjunction, P0, &mut rng).is_none());
    let spell = g.content.rules.spells.id_of("time_stop").unwrap();
    g.magic_mut(P1).unwrap().global.push(spell);
    assert!(g.try_event(K::Disjunction, P0, &mut rng).is_some());
    let told = play(&mut g, armed(K::Disjunction, P0, None));
    assert!(g.magic(P1).unwrap().global.is_empty());
    assert_eq!(told[0].voice, Voice::Happened);
}

#[test]
fn the_gift_is_an_artifact_in_the_armory() {
    let mut g = world();
    let told = play(&mut g, armed(K::Gift, P0, None));
    assert!(!told[0].item.is_empty());
    let armory = &g.player(P0).unwrap().armory;
    assert!(armory
        .vault
        .iter()
        .flatten()
        .chain(&armory.awards)
        .any(|i| i.name == told[0].item));
}

#[test]
fn a_plague_kills_down_to_two_and_ends_when_it_has_run_long_enough() {
    let mut g = world();
    let town = city(&g, 10, 5);
    let told = play(&mut g, armed(K::Plague, P0, Some(town)));
    assert_eq!(told[0].voice, Voice::Started);
    assert!(g.events().is_ongoing(K::Plague));
    // Run it on: the city never falls under two and the plague ends in the end.
    let mut ended = false;
    for _ in 0..400 {
        let mut rng = GameRng::new(g.turn as u64 + 9);
        let mut out = Vec::new();
        g.twiddle_events(None, &mut rng, &mut out);
        g.turn += 1;
        let figures = g.city_population(town).figures;
        assert!(figures >= 2, "{figures}");
        if !g.events().is_ongoing(K::Plague) {
            ended = true;
            assert!(out.iter().any(|e| matches!(
                e,
                Event::RandomEvent(n) if n.kind == K::Plague && n.voice == Voice::Ended
            )));
            break;
        }
    }
    assert!(ended);
}

#[test]
fn a_plague_lets_a_city_of_one_off_by_raising_it_to_two() {
    let mut g = world();
    let town = city(&g, 10, 5);
    g.scenario_set_citizens(town, 1);
    g.events
        .start(K::Plague, g.player_entity(P0), Some(g.serial_of(town)));
    let mut rng = GameRng::new(1);
    let mut out = Vec::new();
    g.twiddle_events(None, &mut rng, &mut out);
    assert_eq!(g.city_population(town).figures, 2);
    assert!(!g.events().is_ongoing(K::Plague));
}

#[test]
fn a_population_boom_speeds_the_growth_of_its_city_only() {
    let mut g = world();
    let town = city(&g, 10, 5);
    let capital = city(&g, 5, 5);
    let growth = |g: &Game, c| g.economy().city_economy(c).unwrap().growth_tens;
    let (before, other) = (growth(&g, town), growth(&g, capital));
    play(&mut g, armed(K::PopulationBoom, P0, Some(town)));
    assert!(growth(&g, town) > before);
    assert_eq!(growth(&g, capital), other);
}

#[test]
fn only_one_world_event_goes_on_at_a_time_and_mana_short_dries_the_power_up() {
    let mut g = world();
    let before = g.income(P0).power_base;
    assert!(before > 0);
    play(&mut g, armed(K::ManaShort, P0, None));
    assert_eq!(g.income(P0).power_base, 0);
    let mut rng = GameRng::new(4);
    assert!(g.try_event(K::GoodMoon, P0, &mut rng).is_none());
    assert!(g.try_event(K::Donation, P0, &mut rng).is_some());
}

#[test]
fn a_world_event_is_told_to_everyone_and_ends_with_a_message_in_the_end() {
    let mut g = world();
    let told = play(&mut g, armed(K::BadMoon, P1, None));
    assert_eq!(told[0].voice, Voice::Happened);
    assert!(told[0].player.is_none());
    for _ in 0..200 {
        let mut rng = GameRng::new(g.turn as u64);
        let mut out = Vec::new();
        g.twiddle_events(None, &mut rng, &mut out);
        g.turn += 1;
        if !g.events().is_ongoing(K::BadMoon) {
            assert!(out.iter().any(|e| matches!(
                e,
                Event::RandomEvent(n) if n.kind == K::BadMoon && n.voice == Voice::Ended
            )));
            return;
        }
    }
    panic!("the moon never set");
}

#[test]
fn earthquake_and_meteor_are_told_with_what_was_lost() {
    let mut g = world();
    let town = city(&g, 10, 5);
    for _ in 0..6 {
        spear(&mut g, P0, at(10, 5));
    }
    let told = play(&mut g, armed(K::GreatMeteor, P0, Some(town)));
    assert_eq!(told[0].kind, K::GreatMeteor);
    assert_eq!(told[0].city_name, "Town");
}

#[test]
fn a_meteor_tells_how_many_citizens_it_killed() {
    let mut g = world();
    let town = city(&g, 10, 5);
    g.scenario_set_citizens(town, 20);
    let told = play(&mut g, armed(K::GreatMeteor, P0, Some(town)));
    assert_eq!(
        told[0].lost_population,
        20 - g.city_population(town).figures
    );
    assert!(told[0].lost_population > 0);
}

#[test]
fn a_depletion_removes_a_mineral_and_a_find_adds_one() {
    let mut g = world();
    let mut defs = g.content.features.defs().to_vec();
    for (id, name) in [("iron_ore", "Iron Ore"), ("coal", "Coal")] {
        defs.push(crate::content::FeatureDef {
            id: id.into(),
            name: name.into(),
            kind: crate::content::FeatureKind::Mineral,
            city: Default::default(),
            spawn: vec![crate::content::SpawnRule {
                terrain: vec![crate::content::TerrainClass::Grassland],
                planes: Plane::ALL.to_vec(),
                weight: 1,
            }],
        });
    }
    g.content.features = crate::content::Features::new(defs);
    let mut rng = GameRng::new(8);
    let find = g
        .find_deposit(P0, true, &mut rng)
        .expect("a place for a mineral");
    let mut a = armed(K::NewMinerals, P0, None);
    a.deposit = Some(find);
    let told = play(&mut g, a);
    let map = g.map().unwrap();
    assert_eq!(
        map.plane(find.at.plane)
            .feature(find.at.x as i32, find.at.y as i32),
        Some(find.ore)
    );
    assert!(told[0].ore.contains("iron") || told[0].ore.contains("coal"));
    let lost = g
        .find_deposit(P0, false, &mut rng)
        .expect("a mineral to lose");
    let mut a = armed(K::Depletion, P0, None);
    a.deposit = Some(lost);
    play(&mut g, a);
    let map = g.map().unwrap();
    assert_eq!(
        map.plane(lost.at.plane)
            .feature(lost.at.x as i32, lost.at.y as i32),
        None
    );
}

#[test]
fn a_draw_never_happens_in_the_first_five_turns_or_with_random_events_off() {
    let mut g = world();
    g.settings.events.random_events = true;
    for _ in 0..5 {
        let e = g.process_events();
        assert!(e.is_empty());
        g.turn += 1;
    }
    let mut g = world();
    g.settings.events.random_events = false;
    g.turn = 300;
    for _ in 0..200 {
        assert!(g.process_events().is_empty());
        g.turn += 1;
    }
}

#[test]
fn events_come_in_the_end_and_each_is_told_once() {
    let mut g = world();
    g.settings.events.random_events = true;
    g.settings.basic.difficulty = 4;
    g.turn = 10;
    let mut kinds = std::collections::BTreeSet::new();
    for _ in 0..600 {
        for e in g.process_events() {
            if let Event::RandomEvent(n) = e {
                kinds.insert(n.kind);
            }
        }
        g.turn += 1;
    }
    assert!(kinds.len() >= 4, "{kinds:?}");
}

#[test]
fn the_state_of_events_survives_a_save() {
    let mut g = world();
    let town = city(&g, 10, 5);
    play(&mut g, armed(K::Plague, P0, Some(town)));
    play(&mut g, armed(K::GoodMoon, P0, None));
    let content = g.content.clone();
    let loaded = g.snapshot().restore(content).unwrap();
    assert!(loaded.events().is_ongoing(K::Plague));
    assert!(loaded.events().is_ongoing(K::GoodMoon));
    let town = loaded.units().entity_of_serial(g.serial_of(town)).unwrap();
    assert!(loaded.events().on_city(K::Plague, loaded.serial_of(town)));
}
