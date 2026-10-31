use super::*;
use crate::city::economy::{Buildings, Population};
use crate::city::production::{Production, ProductionError, ProductionItem};
use crate::game::testutil::{num, shipped};
use crate::map::{Grid, Map, Plane, Position};
use crate::player::PlayerId;
use crate::testdir::TestResult;

use crate::game::Command;
use crate::unit::Hurt;

const P0: PlayerId = PlayerId(0);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

/// For the tests that count the gold of trade: taxes up to rate 2 cause no unrest, so no citizen
/// turns rebel and the sums stay simple. (The game's own table has 20% unrest at rate 2.)
fn calm_taxes(g: &mut Game) {
    g.content.rules.config.tax_unrest_percent = vec![0, 0, 0, 30, 45, 60, 75];
}

/// 24x14 grassland with the sea at x=0. One player with 500 gold, tax rate 2.
fn world() -> Game {
    let c = shipped();
    let grass = c
        .terrains
        .id_numbered(num::terrain::GRASSLAND)
        .expect("id_of succeeds");
    let ocean = c
        .terrains
        .id_numbered(num::terrain::OCEAN)
        .expect("id_of succeeds");
    let build = || {
        let mut g = Grid::new(24, 14, grass);
        for y in 0..14 {
            g.set(0, y, ocean);
        }
        g
    };
    let map = Map::new(build(), build());
    let mut game = Game::new(7);
    game.content = c;
    game.map = Some(map);
    game.add_player(P0, "Merlin");
    game.add_player(PlayerId(1), "Rival");
    game.treasury_mut(P0).expect("treasury_mut succeeds").gold = 500;
    game.player_component_mut::<crate::player::Tax>(P0)
        .expect("value succeeds")
        .0 = 2;
    game
}

fn race(g: &Game, id: &str) -> Option<crate::rules::RaceId> {
    g.content.rules.races.id_of(id)
}

fn found(g: &mut Game, pos: Position, race_id: &str, pop: u32) -> hecs::Entity {
    let race = race(g, race_id);
    g.apply(Command::FoundCity {
        owner: P0,
        pos,
        name: "Town".into(),
        race,
        population: pop,
    });
    g.economy().city_at(pos).expect("city_at succeeds")
}

fn building(g: &Game, id: &str) -> crate::rules::BuildingId {
    g.content.rules.buildings.id_of(id).expect("id_of succeeds")
}

/// Stop the city building housing (which speeds growth) so growth tests see the base rate.
fn idle(g: &mut Game, city: hecs::Entity) {
    g.world
        .insert_one(city, Production::default())
        .expect("insert_one succeeds");
}

/// Put every citizen of a city on the farms, so the food upkeep of real units is covered.
fn farm_all(g: &mut Game, city: hecs::Entity) {
    g.apply(Command::SetFarmers { city, farmers: 99 });
}

fn add_building(g: &mut Game, city: hecs::Entity, id: &str) {
    let b = building(g, id);
    g.world
        .get::<&mut Buildings>(city)
        .expect("value succeeds")
        .0
        .push(b);
}

#[test]
fn the_worked_area_is_five_by_five_without_corners() {
    let g = world();
    assert_eq!(g.economy().city_area(at(10, 6)).len(), 21);
    // Near the top edge the rows above the map are dropped, and x wraps.
    assert_eq!(g.economy().city_area(at(1, 0)).len(), 13);
    assert!(
        g.economy()
            .city_area(at(1, 6))
            .iter()
            .any(|(p, _)| p.x == 23),
        "wraps across the seam"
    );
}

#[test]
fn a_grassland_city_can_grow_to_the_size_cap() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 3);
    let eco = g
        .economy()
        .city_economy(city)
        .ok_or("city_economy is missing")?;
    // 21 tiles x 3 half-food = 31 people fed, capped at 25.
    assert_eq!(eco.max_size, 25);
    assert!(eco.food >= 3);
    Ok(())
}

#[test]
fn farmers_and_workers_share_the_citizens() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    // High elves farm 2 food each: two farmers feed four citizens exactly.
    assert_eq!(g.economy().city_min_farmers(city), 2);
    let eco = g
        .economy()
        .city_economy_with(city, 2)
        .ok_or("city_economy_with is missing")?;
    assert_eq!((eco.farmers, eco.workers, eco.rebels), (2, 2, 0));
    assert_eq!(eco.food, 4);
    // 2 workers x 2 production + 2 farmers x half, rounded: (2*4 + 2 + 1)/2 = 5
    assert_eq!(eco.production, 5);
    // Dwarves work harder (3 each) but farm 2 food; under a High Elf capital one of the four is a
    // rebel (racial unrest), so one works.
    g.player_component_mut::<crate::player::Tax>(P0)
        .ok_or("value is missing")?
        .0 = 0;
    let dwarf = found(&mut g, at(18, 6), "dwarven", 4);
    assert_eq!(g.economy().city_min_farmers(dwarf), 2);
    assert_eq!(
        g.economy()
            .city_economy_with(dwarf, 2)
            .ok_or("city_economy_with is missing")?
            .production,
        (6 + 2 + 1) / 2
    );
    Ok(())
}

#[test]
fn gold_comes_from_taxes_and_the_sea_and_markets() -> TestResult {
    let mut g = world();
    calm_taxes(&mut g);
    let inland = found(&mut g, at(10, 6), "high_elf", 4);
    // 4 citizens x tax rate 2 / 2 = 4 gold.
    assert_eq!(
        g.economy()
            .city_economy(inland)
            .ok_or("city_economy is missing")?
            .gold,
        4
    );
    add_building(&mut g, inland, "marketplace");
    assert_eq!(
        g.economy()
            .city_economy(inland)
            .ok_or("city_economy is missing")?
            .gold,
        6,
        "+50%"
    );
    // A coastal city (sea at x=0) gets +10% trade; 10 gold base needs more citizens.
    let coastal = found(&mut g, at(1, 6), "high_elf", 10);
    assert_eq!(
        g.economy()
            .city_economy(coastal)
            .ok_or("city_economy is missing")?
            .gold,
        11,
        "10 * 1.10"
    );
    // Dwarves double the tax, but under a High Elf capital the racial unrest (3, so 30%) makes
    // one of the four a rebel.
    let dwarf = found(&mut g, at(18, 6), "dwarven", 4);
    assert_eq!(
        g.economy()
            .city_economy(dwarf)
            .ok_or("city_economy is missing")?
            .gold,
        6
    );
    Ok(())
}

#[test]
fn a_lake_counts_as_a_shore_for_trade() -> TestResult {
    let mut g = world();
    calm_taxes(&mut g);
    let city = found(&mut g, at(10, 6), "high_elf", 10);
    assert_eq!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .gold,
        10
    );
    let ocean = g
        .content
        .terrains
        .first_of_class(crate::map::terrain::TerrainClass::Ocean, Plane::Arcanus)
        .ok_or("first_of_class is missing")?;
    let grid = g
        .map
        .as_mut()
        .ok_or("as_mut is missing")?
        .plane_mut(Plane::Arcanus);
    grid.set(11, 6, ocean);
    grid.set_lake(11, 6, true);
    assert_eq!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .gold,
        11,
        "+10% beside the water"
    );
    Ok(())
}

#[test]
fn rivers_pay_twenty_percent_and_river_mouths_thirty() -> TestResult {
    let mut g = world();
    calm_taxes(&mut g);
    let city = found(&mut g, at(10, 6), "high_elf", 10);
    assert_eq!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .gold,
        10
    );
    g.map
        .as_mut()
        .ok_or("as_mut is missing")?
        .plane_mut(Plane::Arcanus)
        .set_river(10, 6, true);
    assert_eq!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .gold,
        12,
        "river"
    );
    let mouth = found(&mut g, at(1, 6), "high_elf", 10);
    g.map
        .as_mut()
        .ok_or("as_mut is missing")?
        .plane_mut(Plane::Arcanus)
        .set_river(1, 6, true);
    assert_eq!(
        g.economy()
            .city_economy(mouth)
            .ok_or("city_economy is missing")?
            .gold,
        13,
        "river mouth"
    );
    Ok(())
}

/// Put one of the game's real buildings in a city.
fn add_real_building(g: &mut Game, city: hecs::Entity, id: &str) {
    let new = g.content.rules.buildings.id_of(id).expect("id_of succeeds");
    g.world
        .get::<&mut Buildings>(city)
        .expect("value succeeds")
        .0
        .push(new);
}

#[test]
fn the_animists_guild_adds_food_for_every_farmer() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 6);
    let food = |g: &Game, farmers| {
        g.economy()
            .city_economy_with(city, farmers)
            .expect("city_economy_with succeeds")
            .food
    };
    let (two, four) = (food(&g, 2), food(&g, 4));
    add_real_building(&mut g, city, "animists_guild");
    assert_eq!(food(&g, 2), two + 2, "one more for each of two farmers");
    assert_eq!(
        food(&g, 4),
        four + 4,
        "and for each of four, not a flat bonus"
    );
    Ok(())
}

#[test]
fn the_foresters_guilds_food_raises_the_size_limit_too() -> TestResult {
    let mut g = world();
    // Barren land: nothing to feed anyone but the guild.
    let desert = g
        .content
        .terrains
        .first_of_class(crate::map::terrain::TerrainClass::Desert, Plane::Arcanus)
        .ok_or("first_of_class is missing")?;
    for x in 0..24 {
        for y in 0..14 {
            g.map
                .as_mut()
                .ok_or("as_mut is missing")?
                .plane_mut(Plane::Arcanus)
                .set(x, y, desert);
        }
    }
    let city = found(&mut g, at(10, 6), "high_elf", 1);
    let size = |g: &Game| {
        g.economy()
            .city_economy(city)
            .expect("city_economy succeeds")
            .max_size
    };
    assert_eq!(size(&g), 0);
    add_real_building(&mut g, city, "foresters_guild");
    assert_eq!(size(&g), 2, "+2 food feeds two more people (B14)");
    Ok(())
}

#[test]
fn mines_add_gold_mana_and_cheaper_units() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    let gold_ore = g
        .content
        .features
        .id_numbered(num::feature::GOLD_ORE)
        .ok_or("gold is not in the content")?;
    let adamant = g
        .content
        .features
        .id_numbered(num::feature::ADAMANTIUM_ORE)
        .ok_or("adamantium_ore is not in the content")?;
    let iron = g
        .content
        .features
        .id_numbered(num::feature::IRON_ORE)
        .ok_or("iron is not in the content")?;
    let plane = g
        .map
        .as_mut()
        .ok_or("as_mut is missing")?
        .plane_mut(Plane::Arcanus);
    plane.set_mineral(10, 8, Some(gold_ore));
    plane.set_mineral(11, 6, Some(adamant));
    plane.set_mineral(9, 6, Some(iron));
    let eco = g
        .economy()
        .city_economy(city)
        .ok_or("city_economy is missing")?;
    assert_eq!(eco.gold, 4 + 3, "taxes plus the gold ore");
    assert_eq!(
        eco.mana,
        2 + 2,
        "half a mana per elf citizen plus adamantium"
    );
    assert_eq!(eco.unit_cost_percent, 95, "iron saves 5%");
    // A miners' guild improves the yields; a second city sharing a tile halves it.
    add_building(&mut g, city, "miners_guild");
    assert_eq!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .gold,
        4 + 4,
        "3 x 1.5 rounds down to 4"
    );
    assert_eq!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .unit_cost_percent,
        90
    );
    // A second city sharing the gold ore's tile halves what each gets from it.
    let before = g
        .economy()
        .city_economy(city)
        .ok_or("city_economy is missing")?
        .gold;
    found(&mut g, at(10, 10), "high_elf", 1);
    assert!(g
        .economy()
        .city_area(at(10, 6))
        .iter()
        .any(|(p, shared)| *p == at(10, 8) && *shared));
    assert!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .gold
            < before,
        "the shared ore pays less"
    );
    Ok(())
}

#[test]
fn a_shared_tile_feeds_each_city_half() -> TestResult {
    let mut g = world();
    let a = found(&mut g, at(8, 6), "high_elf", 3);
    let before = g
        .economy()
        .city_economy_with(a, 0)
        .ok_or("city_economy_with is missing")?
        .max_size;
    found(&mut g, at(12, 6), "high_elf", 3);
    let shared = g
        .economy()
        .city_area(at(8, 6))
        .iter()
        .filter(|(_, s)| *s)
        .count();
    assert_eq!(
        shared, 3,
        "the middle column, without the corners of the left city's area"
    );
    let after = g
        .economy()
        .city_economy_with(a, 0)
        .ok_or("city_economy_with is missing")?
        .max_size;
    assert!(after <= before, "sharing never helps: {before} -> {after}");
    Ok(())
}

#[test]
fn taxes_and_race_cause_unrest_and_temples_calm_it() -> TestResult {
    let mut g = world();
    g.player_component_mut::<crate::player::Tax>(P0)
        .ok_or("value is missing")?
        .0 = 4; // 45% unrest
    let city = found(&mut g, at(10, 6), "high_elf", 10);
    // Capital race is elf, so no racial unrest: 10 * 45% = 4 rebels.
    assert_eq!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .rebels,
        4
    );
    add_building(&mut g, city, "temple");
    assert_eq!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .rebels,
        3
    );
    // A dwarven city under a High Elf capital: the racial table says 3 -> +30%.
    let dwarf = found(&mut g, at(18, 6), "dwarven", 10);
    assert_eq!(
        g.economy()
            .city_economy(dwarf)
            .ok_or("city_economy is missing")?
            .rebels,
        7,
        "10 * (45+30)% = 7"
    );
    Ok(())
}

#[test]
fn stream_of_life_ends_unrest_and_heals_the_units_in_the_city() -> TestResult {
    let mut g = world();
    g.player_component_mut::<crate::player::Tax>(P0)
        .ok_or("value is missing")?
        .0 = 4;
    // The spell costs eight mana a turn to keep.
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.mana = 1000;
    let city = found(&mut g, at(10, 6), "high_elf", 10);
    farm_all(&mut g, city);
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    let home = g.spawn_unit(P0, at(10, 6), spear);
    let field = g.spawn_unit(P0, at(14, 3), spear);
    for u in [home, field] {
        g.world.insert_one(u, Hurt(5))?;
    }
    assert!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .rebels
            > 0
    );
    let stream = g
        .content
        .rules
        .spells
        .id_of("stream_of_life")
        .ok_or("stream_of_life is not in the content")?;
    g.add_city_enchantment(city, stream, P0);
    assert_eq!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .rebels,
        0,
        "no unrest"
    );
    g.apply(Command::EndTurn);
    let hurt = |u| g.world.get::<&Hurt>(u).expect("value succeeds").0;
    assert_eq!(hurt(home), 0, "healed completely in one turn");
    assert!(hurt(field) > 0, "units outside the city are not");
    Ok(())
}

#[test]
fn undead_units_are_not_healed_eat_nothing_and_are_death_creatures() -> TestResult {
    let mut g = world();
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    let living = g.spawn_unit(P0, at(10, 6), spear);
    let raised = g.spawn_unit(P0, at(10, 7), spear);
    for u in [living, raised] {
        g.world.insert_one(u, Hurt(5))?;
    }
    assert!(g.make_undead(raised));
    g.heal_units();
    let hurt = |u| g.world.get::<&Hurt>(u).expect("value succeeds").0;
    assert!(hurt(living) < 5, "the living heal");
    assert_eq!(hurt(raised), 5, "the undead do not");
    assert_eq!(g.units().unit_food_upkeep(living), 1);
    assert_eq!(g.units().unit_food_upkeep(raised), 0);
    assert_eq!(g.units().unit_gold_upkeep(raised), 0);
    assert_eq!(g.units().unit_realm(raised).as_deref(), Some("death"));
    assert_eq!(g.units().unit_realm(living), None);
    assert!(g.units().unit_is_death(raised) && !g.units().unit_is_death(living));
    for a in [
        "death_immunity",
        "cold_immunity",
        "poison_immunity",
        "illusion_immunity",
    ] {
        assert!(g.units().unit_has_ability(raised, a), "{a}");
        assert!(!g.units().unit_has_ability(living, a), "{a}");
    }
    Ok(())
}

#[test]
fn troops_in_a_city_keep_order() -> TestResult {
    let mut g = world();
    g.player_component_mut::<crate::player::Tax>(P0)
        .ok_or("value is missing")?
        .0 = 4;
    let city = found(&mut g, at(10, 6), "high_elf", 10);
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    for _ in 0..4 {
        g.spawn_unit(P0, at(10, 6), spear);
    }
    // 4 rebels, minus 4 troops / 2 = 2 calmed.
    assert_eq!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .rebels,
        2
    );
    Ok(())
}

#[test]
fn growth_follows_the_room_to_grow_and_starvation_shrinks() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 3);
    idle(&mut g, city);
    let eco = g
        .economy()
        .city_economy(city)
        .ok_or("city_economy is missing")?;
    // (25 - 3 + 1) / 2 = 11 tens, less the high elves' 20 percent: 9; the granary adds 20
    // people = 2 tens.
    assert_eq!(eco.growth_tens, 9);
    add_building(&mut g, city, "granary");
    assert_eq!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .growth_tens,
        11
    );
    // No farmers and too little food: shrinks five tens per missing food.
    let starving = g
        .economy()
        .city_economy_with(city, 0)
        .ok_or("city_economy_with is missing")?;
    assert!(starving.growth_tens < 0);
    Ok(())
}

#[test]
fn turns_grow_the_population_and_carry_the_surplus() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 3);
    idle(&mut g, city);
    g.world.insert_one(
        city,
        Population {
            figures: 3,
            tens: 95,
        },
    )?;
    g.apply(Command::EndTurn);
    let pop = *g.world.get::<&Population>(city)?;
    assert_eq!(
        (pop.figures, pop.tens),
        (4, 4),
        "95 + 9 = 104: one citizen and 4 left over"
    );
    Ok(())
}

#[test]
fn production_builds_units_and_buildings() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    let barracks = building(&g, "barracks");
    assert_eq!(
        g.apply(Command::SetProduction {
            city,
            item: ProductionItem::Building(barracks)
        }),
        vec![]
    );
    // 5 production per turn, 30 to build: done on turn 6.
    for _ in 0..5 {
        assert!(g
            .apply(Command::EndTurn)
            .iter()
            .all(|e| !matches!(e, Event::ProductionCompleted { .. })));
    }
    let events = g.apply(Command::EndTurn);
    assert!(events.contains(&Event::ProductionCompleted {
        city,
        item: ProductionItem::Building(barracks)
    }));
    assert!(g.economy().has_building(city, barracks));
    assert_eq!(
        g.economy().city_production(city).item,
        Some(ProductionItem::Housing),
        "falls back to housing"
    );
    // Now swordsmen can be built (they need the barracks and a smithy); the fighters guild
    // trains them as Regulars.
    add_building(&mut g, city, "smithy");
    add_building(&mut g, city, "fighters_guild");
    // Three farmers feed the new soldier too, and one worker makes 5 production a turn.
    g.apply(Command::SetFarmers { city, farmers: 3 });
    let swords = g
        .content
        .rules
        .units
        .id_of("high_elf_swordsmen")
        .ok_or("high_elf_swordsmen is not in the content")?;
    g.apply(Command::SetProduction {
        city,
        item: ProductionItem::Unit(swords),
    });
    for _ in 0..10 {
        g.apply(Command::EndTurn);
    }
    let units = g.units().units_at(at(10, 6));
    assert_eq!(
        units.len(),
        1,
        "30 production in a few turns: one swordsman"
    );
    assert!(
        g.world.get::<&crate::unit::Experience>(units[0])?.0 >= 20,
        "fighters guild: regular at least"
    );
    Ok(())
}

#[test]
fn requirements_and_races_limit_what_can_be_built() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_swordsmen")
        .ok_or("high_elf_swordsmen is not in the content")?;
    let guild = building(&g, "fighters_guild");
    let parthenon = building(&g, "parthenon");
    let barracks = building(&g, "barracks");
    assert_eq!(
        g.economy().can_build(city, ProductionItem::Unit(spear)),
        Err(ProductionError::MissingBuilding)
    );
    assert_eq!(
        g.economy().can_build(city, ProductionItem::Building(guild)),
        Err(ProductionError::MissingBuilding)
    );
    assert_eq!(
        g.economy()
            .can_build(city, ProductionItem::Building(parthenon)),
        Err(ProductionError::WrongRace)
    );
    add_building(&mut g, city, "barracks");
    add_building(&mut g, city, "smithy");
    assert_eq!(
        g.economy().can_build(city, ProductionItem::Unit(spear)),
        Ok(())
    );
    assert_eq!(
        g.economy()
            .can_build(city, ProductionItem::Building(barracks)),
        Err(ProductionError::AlreadyBuilt)
    );
    let galley = g
        .content
        .rules
        .units
        .id_of("galley")
        .ok_or("galley is not in the content")?;
    add_building(&mut g, city, "ship_yard");
    assert_eq!(
        g.economy().can_build(city, ProductionItem::Unit(galley)),
        Err(ProductionError::NotCoastal)
    );
    let port = found(&mut g, at(1, 6), "high_elf", 3);
    add_building(&mut g, port, "ship_yard");
    assert_eq!(
        g.economy().can_build(port, ProductionItem::Unit(galley)),
        Ok(())
    );
    Ok(())
}

/// A city that has just finished the barracks, with the Vizier's list set.
fn vizier_city(on: bool) -> (Game, hecs::Entity) {
    let mut g = world();
    g.content.rules.config.autobuild_order = [
        "barracks",
        "smithy",
        "armory",
        "fighters_guild",
        "granary",
        "nope",
    ]
    .map(String::from)
    .to_vec();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    g.apply(Command::SetGrandVizier { player: P0, on });
    let barracks = building(&g, "barracks");
    g.set_production(city, ProductionItem::Building(barracks))
        .expect("value succeeds");
    (g, city)
}

#[test]
fn without_the_vizier_a_finished_building_leaves_the_city_on_housing() {
    let (mut g, city) = vizier_city(false);
    g.apply_production(city, 1000);
    assert_eq!(
        g.economy().city_production(city).item,
        Some(ProductionItem::Housing)
    );
}

#[test]
fn the_vizier_keeps_choosing_new_buildings_until_none_is_worth_building() {
    let (mut g, city) = vizier_city(true);
    let mut seen = Vec::new();
    for _ in 0..40 {
        g.apply_production(city, 1000);
        match g.economy().city_production(city).item {
            Some(ProductionItem::Building(b)) => {
                assert!(!seen.contains(&b), "a building is chosen twice");
                seen.push(b);
            }
            Some(ProductionItem::TradeGoods) | Some(ProductionItem::Housing) => break,
            other => panic!("the Vizier builds buildings only, not {other:?}"),
        }
    }
    assert!(seen.len() >= 2, "it built {} buildings", seen.len());
    let smithy = building(&g, "smithy");
    assert!(
        seen.contains(&smithy),
        "the Smithy, which the barracks' list names next, is worth building"
    );
}

#[test]
fn a_poor_treasury_builds_gold_and_skips_buildings_it_cannot_keep() -> TestResult {
    let (mut g, city) = vizier_city(true);
    g.player_component_mut::<crate::player::Tax>(P0)
        .ok_or("value is missing")?
        .0 = 0;
    let net = g.budget(P0).net_gold();
    add_building(&mut g, city, "barracks");
    assert!(
        net < 3,
        "the test needs a poor treasury, net is {net} gold a turn"
    );
    let market = building(&g, "marketplace");
    let choice = g.autobuild_choice(city, P0);
    assert!(
        choice == Some(ProductionItem::Building(market))
            || choice == Some(ProductionItem::TradeGoods),
        "low on gold the city builds gold or trades, not {choice:?}"
    );
    Ok(())
}

#[test]
fn turning_the_vizier_on_gives_idle_cities_a_building() -> TestResult {
    let (mut g, city) = vizier_city(false);
    g.set_production(city, ProductionItem::Housing)
        .map_err(|e| format!("{e:?}"))?;
    g.apply(Command::SetGrandVizier {
        player: P0,
        on: true,
    });
    let barracks = building(&g, "barracks");
    assert_eq!(
        g.economy().city_production(city).item,
        Some(ProductionItem::Building(barracks))
    );
    // A city already building something is left alone.
    let other = found(&mut g, at(16, 6), "high_elf", 4);
    add_building(&mut g, other, "builders_hall");
    let granary = building(&g, "granary");
    g.set_production(other, ProductionItem::Building(granary))
        .map_err(|e| format!("{e:?}"))?;
    g.apply(Command::SetGrandVizier {
        player: P0,
        on: true,
    });
    assert_eq!(
        g.economy().city_production(other).item,
        Some(ProductionItem::Building(granary))
    );
    Ok(())
}

#[test]
fn the_vizier_leaves_outposts_alone() {
    let (mut g, _) = vizier_city(true);
    let outpost = found(&mut g, at(4, 6), "high_elf", 0);
    assert_eq!(g.autobuild_choice(outpost, P0), None);
}

#[test]
fn buying_costs_more_the_less_is_done() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    let hall = building(&g, "builders_hall"); // 60
    g.set_production(city, ProductionItem::Building(hall))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        g.economy().buy_cost(city),
        Some(240),
        "nothing done: 4 x 60"
    );
    g.world.insert_one(
        city,
        Production {
            item: Some(ProductionItem::Building(hall)),
            progress: 10,
        },
    )?;
    assert_eq!(
        g.economy().buy_cost(city),
        Some(150),
        "remaining 50, accumulated 10 < 25: 3 x 50"
    );
    g.world.insert_one(
        city,
        Production {
            item: Some(ProductionItem::Building(hall)),
            progress: 40,
        },
    )?;
    assert_eq!(
        g.economy().buy_cost(city),
        Some(40),
        "remaining 20, accumulated 40: 2 x 20"
    );
    let gold = g.player(P0).ok_or("player is missing")?.treasury().gold;
    g.apply(Command::BuyProduction { city });
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().gold,
        gold - 40
    );
    let events = g.apply(Command::EndTurn);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ProductionCompleted { .. })),
        "done next turn"
    );
    Ok(())
}

#[test]
fn buying_needs_the_gold() -> TestResult {
    let mut g = world();
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold = 10;
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    let hall = building(&g, "builders_hall");
    g.set_production(city, ProductionItem::Building(hall))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        g.apply(Command::BuyProduction { city }),
        vec![Event::Rejected(crate::game::Rejection::Production(
            ProductionError::NotEnoughGold
        ))]
    );
    Ok(())
}

#[test]
fn selling_a_building_refunds_it_unless_others_need_it() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    add_building(&mut g, city, "barracks");
    add_building(&mut g, city, "fighters_guild");
    let barracks = building(&g, "barracks");
    assert_eq!(
        g.sell_building(city, barracks),
        Err(ProductionError::MissingBuilding)
    );
    let guild = building(&g, "fighters_guild");
    let gold = g.player(P0).ok_or("player is missing")?.treasury().gold;
    assert_eq!(g.sell_building(city, guild), Ok(66));
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().gold,
        gold + 66
    );
    g.apply(Command::EndTurn);
    assert_eq!(g.sell_building(city, guild), Err(ProductionError::NotBuilt));
    Ok(())
}

#[test]
fn selling_what_the_production_needs_puts_the_city_back_to_housing() {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    add_building(&mut g, city, "barracks");
    add_building(&mut g, city, "smithy");
    let guild = building(&g, "armory");
    let barracks = building(&g, "barracks");
    assert_eq!(g.economy().first_dependant(city, barracks), None);
    g.apply(Command::SetProduction {
        city,
        item: ProductionItem::Building(guild),
    });
    assert!(g.economy().production_needs(city, barracks));
    assert_eq!(g.sell_building(city, barracks), Ok(10));
    assert_eq!(
        g.economy().city_production(city).item,
        Some(ProductionItem::Housing)
    );
}

#[test]
fn selling_is_blocked_through_a_chain_with_a_missing_link() {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    // The war college needs the fighters guild, which needs the barracks; the guild is
    // not built, but the barracks still cannot go while the college stands.
    add_building(&mut g, city, "barracks");
    add_building(&mut g, city, "war_college");
    let barracks = building(&g, "barracks");
    assert_eq!(
        g.sell_building(city, barracks),
        Err(ProductionError::MissingBuilding)
    );
    let college = building(&g, "war_college");
    assert_eq!(g.sell_building(city, college), Ok(166));
    // Only one sale a turn in a city: the barracks goes the turn after.
    assert_eq!(
        g.sell_building(city, barracks),
        Err(ProductionError::AlreadySold)
    );
    assert!(g.economy().sold_this_turn(city));
    g.apply(Command::EndTurn);
    assert!(!g.economy().sold_this_turn(city));
    assert_eq!(g.sell_building(city, barracks), Ok(10));
}

#[test]
fn a_sale_is_remembered_in_a_save() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    add_building(&mut g, city, "barracks");
    let barracks = building(&g, "barracks");
    g.sell_building(city, barracks)
        .map_err(|e| format!("{e:?}"))?;
    let text = g.snapshot().to_text()?;
    let back = crate::game::save::Snapshot::from_text(&text)?.restore(g.content.clone())?;
    let city = back.economy().cities()[0].0;
    assert!(back.economy().sold_this_turn(city));
    // The city picture shows rubble where it stood, so the save keeps which one it was.
    assert_eq!(back.economy().sold_building(city), Some(barracks));
    Ok(())
}

#[test]
fn done_lasts_the_turn_and_patrol_until_the_unit_moves() -> TestResult {
    use crate::unit::Standby;
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    farm_all(&mut g, city);
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold = 100;
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    let done = g.spawn_unit(P0, at(10, 6), spear);
    let patrol = g.spawn_unit(P0, at(11, 6), spear);
    g.apply(Command::SetStandby {
        units: vec![done],
        standby: Some(Standby::Done),
    });
    g.apply(Command::SetStandby {
        units: vec![patrol],
        standby: Some(Standby::Patrol),
    });
    let text = g.snapshot().to_text()?;
    let back = crate::game::save::Snapshot::from_text(&text)?.restore(g.content.clone())?;
    let both = back.units().all_units();
    assert_eq!(
        both.iter()
            .map(|&u| back.units().unit_standby(u))
            .collect::<Vec<_>>(),
        vec![Some(Standby::Done), Some(Standby::Patrol)]
    );
    g.apply(Command::EndTurn);
    assert_eq!(g.units().unit_standby(done), None);
    assert_eq!(g.units().unit_standby(patrol), Some(Standby::Patrol));
    g.apply(Command::MoveStack {
        units: vec![patrol],
        to: at(11, 7),
    });
    assert_eq!(g.units().unit_standby(patrol), None);
    Ok(())
}

#[test]
fn a_wizard_turns_off_their_own_spell_on_a_city_and_no_one_elses() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    let spell = g
        .content
        .rules
        .spells
        .id_of("earth_to_mud")
        .ok_or("earth_to_mud is not in the content")?;
    g.add_city_enchantment(city, spell, P0);
    g.add_city_enchantment(city, spell, PlayerId(1));
    let off = |g: &mut Game, player| {
        g.apply(Command::TurnOffCitySpell {
            player,
            city,
            spell,
        })
    };
    assert!(matches!(
        off(&mut g, PlayerId(2))[..],
        [crate::game::Event::Rejected(_)]
    ));
    assert_eq!(
        off(&mut g, P0),
        vec![crate::game::Event::EnchantmentLapsed { player: P0, spell }]
    );
    assert_eq!(g.city_enchantments(city), vec![(spell, PlayerId(1))]);
    assert!(matches!(
        off(&mut g, P0)[..],
        [crate::game::Event::Rejected(_)]
    ));
    Ok(())
}

fn put_ore(g: &mut Game, ore: u16, x: u32, y: u32) {
    let id = g
        .content
        .features
        .id_numbered(ore)
        .expect("id_numbered succeeds");
    g.map
        .as_mut()
        .expect("value succeeds")
        .plane_mut(Plane::Arcanus)
        .set_mineral(x, y, Some(id));
}

#[test]
fn weapon_quality_follows_the_alchemists_guild_and_the_ore() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    put_ore(&mut g, num::feature::MITHRIL_ORE, 11, 6);
    assert_eq!(
        g.economy().city_weapon_quality(city),
        0,
        "ore alone does nothing"
    );
    add_building(&mut g, city, "alchemists_guild");
    assert_eq!(g.economy().city_weapon_quality(city), 2, "mithril");
    put_ore(&mut g, num::feature::ADAMANTIUM_ORE, 9, 6);
    assert_eq!(
        g.economy().city_weapon_quality(city),
        3,
        "adamantium beats mithril"
    );
    for x in [9, 11] {
        g.map
            .as_mut()
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus)
            .set_corrupted(x, 6, true);
    }
    assert_eq!(
        g.economy().city_weapon_quality(city),
        1,
        "corrupted ore is useless"
    );
    Ok(())
}

#[test]
fn units_are_trained_with_the_cities_weapons_and_alchemy_gives_magic() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    add_building(&mut g, city, "barracks");
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    let train = |g: &mut Game| {
        g.set_production(city, ProductionItem::Unit(spear))
            .expect("set_production succeeds");
        g.apply_production(city, 100);
        g.units()
            .units_at(at(10, 6))
            .into_iter()
            .last()
            .expect("last succeeds")
    };
    let plain = train(&mut g);
    assert_eq!(g.units().unit_weapon_quality(plain), 0);
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Retorts>()
        .0
        .insert(crate::wizard::magic::Retort::Alchemy);
    let magic = train(&mut g);
    assert_eq!(
        g.units().unit_weapon_quality(magic),
        1,
        "Alchemy: magic weapons"
    );
    put_ore(&mut g, num::feature::MITHRIL_ORE, 11, 6);
    add_building(&mut g, city, "alchemists_guild");
    let mithril = train(&mut g);
    assert_eq!(g.units().unit_weapon_quality(mithril), 2);
    Ok(())
}

#[test]
fn replaced_buildings_cost_nothing_to_keep() {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    add_building(&mut g, city, "barracks"); // upkeep 0 anyway
    add_building(&mut g, city, "smithy"); // 1
    add_building(&mut g, city, "armory"); // 2, replaces barracks
    assert_eq!(g.economy().city_building_upkeep(city), 3);
    add_building(&mut g, city, "library"); // 1
    assert_eq!(g.economy().city_building_upkeep(city), 4);
}

#[test]
fn the_treasury_collects_taxes_and_pays_for_troops() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    g.apply(Command::SetFarmers { city, farmers: 3 });
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    g.spawn_unit(P0, at(10, 6), spear);
    g.spawn_unit(P0, at(10, 6), spear);
    let eco = g
        .economy()
        .city_economy(city)
        .ok_or("city_economy is missing")?;
    let before = g.player(P0).ok_or("player is missing")?.treasury().gold;
    g.apply(Command::EndTurn);
    // Income 4 gold; food: three farmers make 6 - 4 eaten = 2 spare, the 2 soldiers eat it all -> no food gold.
    // Upkeep: two spearmen at 1 gold each.
    assert_eq!(eco.gold, 4);
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().gold,
        before + 4 - 2
    );
    Ok(())
}

#[test]
fn spare_food_is_sold_for_gold() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    g.apply(Command::SetFarmers { city, farmers: 3 });
    let eco = g
        .economy()
        .city_economy(city)
        .ok_or("city_economy is missing")?;
    assert_eq!(eco.food, 6);
    let before = g.player(P0).ok_or("player is missing")?.treasury().gold;
    g.apply(Command::EndTurn);
    // 6 food - 4 citizens = 2 spare, sold at two for one: +1, plus 4 tax.
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().gold,
        before + 4 + 1
    );
    Ok(())
}

#[test]
fn dismissing_a_unit_removes_it_and_says_so() -> TestResult {
    let mut g = world();
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    let keep = g.spawn_unit(P0, at(11, 6), spear);
    let go = g.spawn_unit(P0, at(11, 6), spear);
    let events = g.apply(Command::DismissUnit { unit: go });
    assert_eq!(
        events,
        vec![Event::UnitsDisbanded {
            player: P0,
            units: 1
        }]
    );
    assert_eq!(g.units().player_units(P0), vec![keep]);
    // Dismissing it again does nothing.
    assert!(g.apply(Command::DismissUnit { unit: go }).is_empty());
    Ok(())
}

#[test]
fn an_army_that_cannot_be_fed_or_paid_is_disbanded() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    g.apply(Command::SetFarmers { city, farmers: 3 });
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    // Spare food is 2: three soldiers eat 3, so one goes.
    for _ in 0..3 {
        g.spawn_unit(P0, at(11, 6), spear);
    }
    let events = g.apply(Command::EndTurn);
    assert!(events.contains(&Event::UnitsDisbanded {
        player: P0,
        units: 1
    }));
    assert!(events.iter().any(|e| matches!(
        e,
        Event::UnitLost {
            cause: crate::game::LossCause::Food,
            ..
        }
    )));
    assert_eq!(g.units().player_units(P0).len(), 2);
    // Broke: upkeep above the treasury sends the rest home.
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold = 0;
    g.player_component_mut::<crate::player::Tax>(P0)
        .ok_or("value is missing")?
        .0 = 0;
    let events = g.apply(Command::EndTurn);
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::UnitsDisbanded { .. })));
    Ok(())
}

#[test]
fn wounded_units_heal_faster_in_cities() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    farm_all(&mut g, city);
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    let field = g.spawn_unit(P0, at(14, 3), spear);
    let home = g.spawn_unit(P0, at(10, 6), spear);
    // Spearman: 3 figures x 2 hp = 6 hit points. Give both 4 damage.
    g.world.insert_one(field, Hurt(4))?;
    g.world.insert_one(home, Hurt(4))?;
    g.apply(Command::EndTurn);
    let hurt = |u| g.world.get::<&Hurt>(u).expect("value succeeds").0;
    assert!(
        hurt(home) <= hurt(field),
        "city {} vs field {}",
        hurt(home),
        hurt(field)
    );
    assert!(hurt(field) < 4, "even in the field something mends");
    Ok(())
}

#[test]
fn regenerating_units_mend_completely_each_turn() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    farm_all(&mut g, city);
    let mut units = g.content.rules.units.defs().to_vec();
    for u in units.iter_mut() {
        if u.id == "high_elf_spearmen" {
            u.abilities.push("regeneration".into());
        }
    }
    g.content.rules.units = crate::rules::Registry::new_sorted(units);
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    let troll = g.spawn_unit(P0, at(14, 3), spear);
    g.world.insert_one(troll, Hurt(5))?;
    g.apply(Command::EndTurn);
    assert_eq!(g.world.get::<&Hurt>(troll)?.0, 0);
    Ok(())
}

#[test]
fn wild_game_raises_an_outposts_growth_chance_by_its_food_and_its_bonus() -> TestResult {
    let mut g = world();
    // The test land feeds more than the usual largest city, which would hide the food.
    g.content.rules.config.max_city_size = 100;
    let outpost = found(&mut g, at(10, 6), "high_elf", 0);
    let plain = g.economy().outpost_growth_chance(outpost);
    let game = g
        .content
        .features
        .id_numbered(num::feature::WILD_GAME)
        .ok_or("game is not in the content")?;
    g.map
        .as_mut()
        .ok_or("as_mut is missing")?
        .plane_mut(Plane::Arcanus)
        .set_mineral(10, 8, Some(game));
    // The special's own +10 and the 2 food it adds to what the land can feed.
    assert_eq!(g.economy().outpost_growth_chance(outpost), plain + 10 + 2);
    Ok(())
}

#[test]
fn difficulty_does_not_change_outpost_growth_for_anyone() {
    let mut g = world();
    let outpost = found(&mut g, at(10, 6), "high_elf", 0);
    let normal = g.economy().outpost_growth_chance(outpost);
    for difficulty in [0, 4] {
        g.settings.basic.difficulty = difficulty;
        assert_eq!(g.economy().outpost_growth_chance(outpost), normal);
    }
}

#[test]
fn outposts_grow_into_cities_in_time() -> TestResult {
    let mut g = world();
    let outpost = found(&mut g, at(10, 6), "high_elf", 0);
    for _ in 0..300 {
        g.apply(Command::EndTurn);
        if g.world.get::<&Population>(outpost)?.figures > 0 {
            break;
        }
    }
    assert_eq!(
        g.world.get::<&Population>(outpost)?.figures,
        1,
        "grew to a hamlet"
    );
    assert!(g.world.get::<&Farmers>(outpost)?.0 <= 1);
    Ok(())
}

#[test]
fn farmers_can_be_set_but_not_below_what_feeds_the_city() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    g.apply(Command::SetFarmers { city, farmers: 0 });
    assert_eq!(g.world.get::<&Farmers>(city)?.0, 2, "raised to the minimum");
    g.apply(Command::SetFarmers { city, farmers: 4 });
    assert_eq!(g.world.get::<&Farmers>(city)?.0, 4);
    g.apply(Command::SetFarmers { city, farmers: 99 });
    assert_eq!(
        g.world.get::<&Farmers>(city)?.0,
        4,
        "never more than the citizens"
    );
    Ok(())
}

#[test]
fn balancing_farmers_feeds_the_army() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 6);
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    for _ in 0..3 {
        g.spawn_unit(P0, at(10, 6), spear);
    }
    g.balance_farmers(P0);
    let t = g.budget(P0);
    assert!(t.food_surplus >= t.food_upkeep, "{t:?}");
    assert_eq!(t.food_upkeep, 3);
    // High elves farm 2 food each: 6 citizens and 3 soldiers need 9 food, so five farmers.
    let farmers = g.world.get::<&Farmers>(city)?.0;
    assert_eq!(
        farmers, 5,
        "five farmers: 10 food for 6 citizens and 3 soldiers"
    );
    // Not enough land or people: everyone farms and the rest cannot be helped.
    for _ in 0..20 {
        g.spawn_unit(P0, at(11, 6), spear);
    }
    g.balance_farmers(P0);
    assert_eq!(g.world.get::<&Farmers>(city)?.0, 6);
    Ok(())
}

#[test]
fn settlers_found_outposts_and_are_used_up() -> TestResult {
    let mut g = world();
    let settler = g
        .content
        .rules
        .units
        .id_of("high_elf_settlers")
        .ok_or("high_elf_settlers is not in the content")?;
    let unit = g.spawn_unit(P0, at(10, 6), settler);
    let events = g.apply(Command::SettleOutpost {
        unit,
        name: "Newtown".into(),
    });
    assert!(
        matches!(events[..], [Event::CityFounded { .. }]),
        "{events:?}"
    );
    let city = g.economy().city_at(at(10, 6)).ok_or("city_at is missing")?;
    assert_eq!(g.world.get::<&Population>(city)?.figures, 0, "an outpost");
    assert!(
        g.world.get::<&crate::city::economy::CityRace>(city).is_ok(),
        "of the settlers' race"
    );
    assert!(
        g.units().unit_position(unit).is_none(),
        "the settlers are gone"
    );
    // Too close to the new outpost.
    let again = g.spawn_unit(P0, at(12, 6), settler);
    assert_eq!(
        g.apply(Command::SettleOutpost {
            unit: again,
            name: "Nope".into()
        }),
        vec![Event::Rejected(crate::game::Rejection::FoundCity(
            crate::city::FoundCityError::TooCloseToCity
        ))]
    );
    assert!(
        g.units().unit_position(again).is_some(),
        "refused settlers stay alive"
    );
    // Soldiers cannot found cities.
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    let soldier = g.spawn_unit(P0, at(18, 6), spear);
    assert!(matches!(
        g.apply(Command::SettleOutpost {
            unit: soldier,
            name: "x".into()
        })[..],
        [Event::Rejected(_)]
    ));
    Ok(())
}

#[test]
fn tax_rate_is_clamped_to_the_table() -> TestResult {
    let mut g = world();
    g.apply(Command::SetTaxRate {
        player: P0,
        rate: 99,
    });
    assert_eq!(
        g.player_component::<crate::player::Tax>(P0)
            .ok_or("value is missing")?
            .0,
        6
    );
    Ok(())
}

#[test]
fn famine_and_gaias_blessing_change_the_land() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    let base = g
        .economy()
        .city_economy_with(city, 2)
        .ok_or("city_economy_with is missing")?;
    let famine = g
        .content
        .rules
        .spells
        .id_of("famine")
        .ok_or("famine is not in the content")?;
    g.add_city_enchantment(city, famine, PlayerId(1));
    let hungry = g
        .economy()
        .city_economy_with(city, 2)
        .ok_or("city_economy_with is missing")?;
    assert!(hungry.food < base.food);
    let gaia = g
        .content
        .rules
        .spells
        .id_of("gaias_blessing")
        .ok_or("gaias_blessing is not in the content")?;
    g.clear_city_enchantments(city);
    g.add_city_enchantment(city, gaia, P0);
    // Gaia's blessing raises the food the land can carry by half.
    let blessed = g
        .economy()
        .city_economy_with(city, 2)
        .ok_or("city_economy_with is missing")?;
    assert!(blessed.max_size >= base.max_size);
    Ok(())
}

#[test]
fn gaias_blessing_pacifies_two_rebels() -> TestResult {
    let mut g = world();
    g.player_component_mut::<crate::player::Tax>(P0)
        .ok_or("value is missing")?
        .0 = 4; // 45% unrest: 4 rebels of 10
    let city = found(&mut g, at(10, 6), "high_elf", 10);
    assert_eq!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .rebels,
        4
    );
    let gaia = g
        .content
        .rules
        .spells
        .id_of("gaias_blessing")
        .ok_or("gaias_blessing is not in the content")?;
    g.add_city_enchantment(city, gaia, P0);
    assert_eq!(
        g.economy()
            .city_economy(city)
            .ok_or("city_economy is missing")?
            .rebels,
        2
    );
    Ok(())
}

#[test]
fn pestilence_adds_unrest_and_each_citizen_may_die_but_the_last_is_spared() -> TestResult {
    let mut g = world();
    g.treasury_mut(PlayerId(1))
        .ok_or("treasury_mut is missing")?
        .mana = 100_000;
    let city = found(&mut g, at(10, 6), "high_elf", 10);
    let unrest = |g: &Game| {
        g.economy()
            .city_economy(city)
            .expect("city_economy succeeds")
            .rebels
    };
    let calm = unrest(&g);
    let pestilence = g
        .content
        .rules
        .spells
        .id_of("pestilence")
        .ok_or("pestilence is not in the content")?;
    g.add_city_enchantment(city, pestilence, PlayerId(1));
    assert_eq!(unrest(&g), calm + 2, "two more rebels");
    // Each citizen but the last has a tenth of a chance to die: twenty turns show the plague.
    let mut deaths = 0;
    for _ in 0..20 {
        deaths += g
            .apply(Command::EndTurn)
            .iter()
            .filter(|e| matches!(e, Event::CityShrank { .. }))
            .count();
    }
    assert!(deaths >= 3, "the plague killed {deaths} times in 20 turns");
    Ok(())
}

#[test]
fn a_city_of_one_is_safe_from_pestilence() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 1);
    let pestilence = g
        .content
        .rules
        .spells
        .id_of("pestilence")
        .ok_or("pestilence is not in the content")?;
    g.add_city_enchantment(city, pestilence, PlayerId(1));
    for _ in 0..20 {
        let events = g.apply(Command::EndTurn);
        assert!(!events.iter().any(|e| matches!(e, Event::CityShrank { .. })));
    }
    Ok(())
}

#[test]
fn the_moons_change_the_mana_of_the_temples_by_the_wizards_books() -> TestResult {
    use crate::world_event::EventKind;
    let mut g = world();
    g.init_wizard(P0, &[("life", 2)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    add_building(&mut g, city, "temple");
    let mana = |g: &Game| {
        g.economy()
            .city_economy(city)
            .expect("city_economy succeeds")
            .mana
    };
    let plain = mana(&g);
    g.scenario_start_event(EventKind::BadMoon);
    assert_eq!(
        mana(&g),
        plain - 1,
        "the evil moon halves a life wizard's temple (2)"
    );
    let mut g = world();
    g.init_wizard(P0, &[("life", 2)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    add_building(&mut g, city, "temple");
    g.scenario_start_event(EventKind::GoodMoon);
    assert_eq!(mana(&g), plain + 2, "the good moon doubles it");
    Ok(())
}

#[test]
fn dark_rituals_double_the_mana_of_the_religious_buildings_and_divine_power_makes_it_triple(
) -> TestResult {
    use crate::wizard::magic::Retort;
    let mut g = world();
    g.init_wizard(P0, &[("death", 2)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    add_building(&mut g, city, "temple");
    let mana = |g: &Game| {
        g.economy()
            .city_economy(city)
            .expect("city_economy succeeds")
            .mana
    };
    let plain = mana(&g);
    let rituals = g
        .content
        .rules
        .spells
        .id_of("dark_rituals")
        .ok_or("dark_rituals is not in the content")?;
    g.add_city_enchantment(city, rituals, P0);
    assert_eq!(mana(&g), plain + 2, "a temple's 2 mana doubled");
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Retorts>()
        .0
        .insert(Retort::DivinePower);
    assert_eq!(
        mana(&g),
        plain + 4,
        "150 percent twice over: 6 for the temple"
    );
    Ok(())
}

#[test]
fn just_cause_pacifies_one_rebel_in_every_city() -> TestResult {
    let mut g = world();
    g.player_component_mut::<crate::player::Tax>(P0)
        .ok_or("value is missing")?
        .0 = 4; // 4 rebels of 10
    let city = found(&mut g, at(10, 6), "high_elf", 10);
    let rebels = |g: &Game| {
        g.economy()
            .city_economy(city)
            .expect("city_economy succeeds")
            .rebels
    };
    assert_eq!(rebels(&g), 4);
    let just_cause = g
        .content
        .rules
        .spells
        .id_of("just_cause")
        .ok_or("just_cause is not in the content")?;
    g.add_global(P0, just_cause);
    assert_eq!(rebels(&g), 3);
    Ok(())
}

#[test]
fn a_finished_unit_waits_while_nine_units_stand_in_the_city() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    add_building(&mut g, city, "barracks");
    add_building(&mut g, city, "fighters_guild");
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    for _ in 0..crate::unit::MAX_TILE_LIMIT {
        g.spawn_unit(P0, at(10, 6), spear);
    }
    g.set_production(city, ProductionItem::Unit(spear))
        .map_err(|e| format!("{e:?}"))?;
    let cost = g.economy().item_cost(city, ProductionItem::Unit(spear));
    assert!(g.production_blocked(city, cost as i32).is_some());
    assert_eq!(
        g.units().units_at(at(10, 6)).len(),
        crate::unit::MAX_TILE_LIMIT
    );
    assert_eq!(
        g.economy().city_production(city).item,
        Some(ProductionItem::Unit(spear))
    );
    assert!(g.economy().city_production(city).progress >= cost);
    Ok(())
}

#[test]
fn a_city_that_lost_the_building_its_item_needs_turns_to_trade_goods() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 4);
    add_building(&mut g, city, "barracks");
    add_building(&mut g, city, "smithy");
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_swordsmen")
        .ok_or("high_elf_swordsmen is not in the content")?;
    g.set_production(city, ProductionItem::Unit(spear))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.production_impossible(city), None);
    let barracks = g
        .content
        .rules
        .buildings
        .id_of("barracks")
        .ok_or("barracks is not in the content")?;
    g.world
        .get::<&mut Buildings>(city)?
        .0
        .retain(|b| *b != barracks);
    assert_eq!(
        g.production_impossible(city),
        Some(ProductionItem::Unit(spear))
    );
    assert_eq!(
        g.economy().city_production(city).item,
        Some(ProductionItem::TradeGoods)
    );
    Ok(())
}

#[test]
fn settlers_cost_a_citizen_unless_the_setting_makes_them_free() -> TestResult {
    let mut g = world();
    let city = found(&mut g, at(10, 6), "high_elf", 3);
    let settlers = g
        .content
        .rules
        .units
        .id_of("high_elf_settlers")
        .ok_or("high_elf_settlers is not in the content")?;
    let item = ProductionItem::Unit(settlers);
    assert!(g.economy().costs_a_citizen(settlers));
    g.world.insert_one(
        city,
        crate::city::economy::Population {
            figures: 1,
            tens: 0,
        },
    )?;
    assert_eq!(
        g.economy().can_build(city, item),
        Err(ProductionError::TooSmall)
    );
    g.world.insert_one(
        city,
        crate::city::economy::Population {
            figures: 3,
            tens: 0,
        },
    )?;
    assert_eq!(g.economy().can_build(city, item), Ok(()));
    g.set_production(city, item).map_err(|e| format!("{e:?}"))?;
    let cost = g.economy().item_cost(city, item);
    g.apply_production(city, cost as i32);
    assert_eq!(g.city_population(city).figures, 2);
    g.settings.rules.settlers_cost_citizen = false;
    assert!(!g.economy().costs_a_citizen(settlers));
    Ok(())
}
