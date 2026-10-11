use std::path::Path;

use f4x_core::city::PlayerId;
use f4x_core::Game;

use crate::setup::{new_game, NewGame, HUMAN};
use crate::{commands, Access, Console, ConsoleError, Kind, Mode};

fn game() -> Game {
    let content =
        f4x_data::load_content(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"))
            .unwrap();
    new_game(
        &content,
        &NewGame {
            seed: 7,
            ..NewGame::default()
        },
    )
    .unwrap()
}

/// The wizards of a game (the independent cities have players of their own).
fn wizards(g: &Game) -> usize {
    g.players.iter().filter(|p| !p.neutral).count()
}

fn console() -> Console {
    Console::new(HUMAN, Access::Full)
}

fn err(r: &crate::Reply) -> &ConsoleError {
    r.error.as_ref().expect("an error")
}

#[test]
fn the_table_is_well_formed() {
    let all = commands();
    let mut names: Vec<_> = all.iter().map(|c| c.name).collect();
    names.sort();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len(), "command names are unique");
    for c in &all {
        assert!(
            c.usage.starts_with(c.name),
            "{} usage {:?}",
            c.name,
            c.usage
        );
        assert!(!c.help.is_empty());
    }
    for kind in [Kind::Query, Kind::Legal, Kind::Override] {
        assert!(all.iter().any(|c| c.kind == kind));
    }
}

#[test]
fn queries_describe_the_new_game() {
    let (mut g, mut c) = (game(), console());
    let status = c.execute(&mut g, "status");
    assert!(status.ok, "{}", status.text());
    assert!(status.text().contains("turn 1"));
    for line in [
        "players",
        "units",
        "cities",
        "spells",
        "help",
        "help move",
        "commands",
        "map",
    ] {
        let r = c.execute(&mut g, line);
        assert!(r.ok, "{line}: {}", r.text());
        assert!(!r.lines.is_empty(), "{line}");
    }
    let cities = c.execute(&mut g, "cities");
    assert!(cities.text().contains("city 1"), "{}", cities.text());
    assert!(c.execute(&mut g, "city 99").error.is_some());
}

#[test]
fn override_commands_need_the_override_mode() {
    let (mut g, mut c) = (game(), console());
    let refused = c.execute(&mut g, "give gold 500");
    assert!(!refused.ok);
    assert!(
        matches!(err(&refused), ConsoleError::NotAllowed(_)),
        "{refused:?}"
    );
    let gold = g.player(HUMAN).unwrap().gold;

    assert!(c.execute(&mut g, "mode override").ok);
    assert_eq!(c.session.mode, Mode::Override);
    assert!(c.execute(&mut g, "give gold 500").ok);
    assert_eq!(g.player(HUMAN).unwrap().gold, gold + 500);

    assert!(c.execute(&mut g, "mode legal").ok);
    assert!(!c.execute(&mut g, "give gold 500").ok);
    assert_eq!(g.player(HUMAN).unwrap().gold, gold + 500);

    // Switching the mode on and each cheat are in the game's saved log.
    let log = g.override_log().to_vec();
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(log[0].contains("mode override") && log[1].contains("give gold 500"));
    let restored = g.snapshot().restore(g.content.clone()).unwrap();
    assert_eq!(restored.override_log(), log.as_slice());
    assert!(c.execute(&mut g, "history").text().contains("give gold"));
}

#[test]
fn access_limits_what_a_front_end_can_ever_do() {
    let mut g = game();
    let mut spectator = Console::new(HUMAN, Access::ReadOnly);
    assert!(spectator.execute(&mut g, "status").ok);
    assert!(!spectator.execute(&mut g, "end").ok);
    assert_eq!(g.turn, 1);
    let mut player = Console::new(HUMAN, Access::Play);
    assert!(!player.execute(&mut g, "mode override").ok);
    assert!(player.execute(&mut g, "end").ok);
    assert_eq!(g.turn, 2);
    assert!(!player.execute(&mut g, "give gold 1").ok);
    assert!(
        !player.execute(&mut g, "new 5").ok,
        "a player may not replace the game"
    );
    assert!(!player.execute(&mut g, "load quick").ok);
    // Placing a fortress is not normal play: the legal commands do not have it.
    assert!(!player.execute(&mut g, "fortress 5,5").ok);
    assert_eq!(player.complete("sp"), ["spells"]);
    assert!(!console().complete("sp").contains(&"spawn"));
    let mut full = console();
    full.execute(&mut g, "mode override");
    assert!(full.complete("sp").contains(&"spawn"));
}

#[test]
fn legal_commands_check_ownership() {
    let (mut g, mut c) = (game(), console());
    c.execute(&mut g, "mode override");
    let spawn = c.execute(&mut g, "spawn spearmen 3,3 1");
    assert!(spawn.ok, "{}", spawn.text());
    c.execute(&mut g, "mode legal");
    // The rival's unit is the newest one.
    let newest = g
        .all_units()
        .into_iter()
        .map(|u| g.serial_of(u).0)
        .max()
        .unwrap();
    let r = c.execute(&mut g, &format!("stop {newest}"));
    assert!(matches!(err(&r), ConsoleError::NotAllowed(_)), "{r:?}");
    // In override mode the same line works and is logged.
    c.execute(&mut g, "mode override");
    let before = g.override_log().len();
    let r = c.execute(&mut g, &format!("stop {newest}"));
    assert!(r.ok, "{}", r.text());
    assert_eq!(g.override_log().len(), before + 1);
}

#[test]
fn a_player_can_run_a_turn_by_text() {
    let (mut g, mut c) = (game(), console());
    let city = c.execute(&mut g, "city 1");
    assert!(city.ok, "{}", city.text());
    let options = c.execute(&mut g, "options 1");
    let first = options.lines.first().expect("something to build");
    let id = first.split_whitespace().next().unwrap().to_string();
    let r = c.execute(&mut g, &format!("produce 1 {id}"));
    assert!(r.ok, "{}", r.text());
    assert!(c.execute(&mut g, "farmers 1 1").ok);
    assert!(c.execute(&mut g, "tax 1").ok);
    assert!(c.execute(&mut g, "power 34 33 33").ok);
    let end = c.execute(&mut g, "end");
    assert!(end.ok);
    assert_eq!(g.turn, 2);
    assert!(end.text().contains("Turn 2"));
    assert!(
        c.execute(&mut g, "undo").error.is_some(),
        "ending a turn cannot be undone"
    );
}

#[test]
fn unknown_and_malformed_lines_are_errors_not_panics() {
    let (mut g, mut c) = (game(), console());
    assert!(matches!(err(&c.execute(&mut g, "")), ConsoleError::Empty));
    assert!(matches!(
        err(&c.execute(&mut g, "frobnicate")),
        ConsoleError::Unknown(_)
    ));
    assert!(c.execute(&mut g, "move").error.is_some());
    assert!(c.execute(&mut g, "move x y").error.is_some());
    assert!(c.execute(&mut g, "move 999 1,1").error.is_some());
    assert!(c.execute(&mut g, "cast nonsense").error.is_some());
    assert!(c.execute(&mut g, r#"tile "1,1"#).error.is_some());
    assert!(c.execute(&mut g, "road 1").error.is_some());
}

#[test]
fn moving_into_an_enemy_starts_a_battle_you_can_fight_by_text() {
    let (mut g, mut c) = (game(), console());
    let first_city = c.execute(&mut g, "cities");
    let at = first_city.lines[0]
        .split(" at ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    let (x, y): (u32, u32) = {
        let mut it = at.split(',');
        (
            it.next().unwrap().parse().unwrap(),
            it.next().unwrap().parse().unwrap(),
        )
    };
    c.execute(&mut g, "mode override");
    let r = c.execute(&mut g, &format!("spawn spearmen {},{},a 1", x + 1, y));
    assert!(r.ok, "{}", r.text());
    c.execute(&mut g, "mode legal");
    let r = c.execute(&mut g, &format!("attack @{x},{y} {},{y}", x + 1));
    assert!(r.ok, "{}", r.text());
    assert!(g.in_battle(), "{}", r.text());
    assert!(c.execute(&mut g, "battle").ok);
    // Fight on by text until it ends.
    for _ in 0..400 {
        if !g.in_battle() {
            break;
        }
        let b = g.battle().unwrap();
        if b.player_of(b.active) == PlayerId(1) {
            // The opponent's side: let the console play it as that player.
            let order = f4x_core::battle::ai::next_command(b);
            g.apply(f4x_core::Command::Battle(order));
            continue;
        }
        let order = f4x_core::battle::ai::next_command(b);
        let line = match order {
            f4x_core::battle::BattleCommand::Move { unit, to } => {
                format!("bmove {unit} {},{}", to.0, to.1)
            }
            f4x_core::battle::BattleCommand::Attack { unit, target } => {
                format!("battack {unit} {target}")
            }
            f4x_core::battle::BattleCommand::Shoot { unit, target } => {
                format!("bshoot {unit} {target}")
            }
            f4x_core::battle::BattleCommand::Done { unit } => format!("bdone {unit}"),
            f4x_core::battle::BattleCommand::EndTurn => "bend".into(),
            f4x_core::battle::BattleCommand::Flee => "bflee".into(),
        };
        let r = c.execute(&mut g, &line);
        assert!(r.ok, "{line}: {}", r.text());
    }
    assert!(!g.in_battle(), "the battle ended");
}

#[test]
fn a_cheat_can_end_the_game_and_the_console_then_refuses_play() {
    let (mut g, mut c) = (game(), console());
    c.execute(&mut g, "mode override");
    let r = c.execute(&mut g, "win 0");
    assert!(r.ok, "{}", r.text());
    assert!(g.game_over.is_some());
    assert!(c.execute(&mut g, "status").text().contains("GAME OVER"));
    assert!(!c.execute(&mut g, "end").ok);
    assert!(c.execute(&mut g, "score").ok);
}

#[test]
fn eliminating_the_rival_wins_by_conquest() {
    let (mut g, mut c) = (game(), console());
    c.execute(&mut g, "mode override");
    let r = c.execute(&mut g, "eliminate 1");
    assert!(r.ok, "{}", r.text());
    assert!(r.text().contains("GAME OVER"), "{}", r.text());
    assert_eq!(g.game_over.unwrap().winner, HUMAN);
}

#[test]
fn read_only_mode_runs_queries_and_nothing_else() {
    let (mut g, mut c) = (game(), console());
    assert!(c.execute(&mut g, "mode readonly").ok);
    assert_eq!(c.session.mode, Mode::ReadOnly);
    assert!(c.execute(&mut g, "status").ok);
    assert!(c.execute(&mut g, "cities").ok);
    for line in [
        "end",
        "tax 1",
        "save",
        "new 3",
        "give gold 5",
        "mode override",
    ] {
        let r = c.execute(&mut g, line);
        // Switching up needs a deliberate `mode legal` first for legal commands; the
        // mode word itself is how you leave read-only, so that one is allowed.
        if line == "mode override" {
            assert!(r.ok);
            assert!(c.execute(&mut g, "mode readonly").ok);
            continue;
        }
        assert!(
            matches!(err(&r), ConsoleError::NotAllowed(_)),
            "{line}: {r:?}"
        );
    }
    assert_eq!(g.turn, 1);
    assert!(c.execute(&mut g, "mode legal").ok);
    assert!(c.execute(&mut g, "end").ok);
    assert_eq!(g.turn, 2);
    // A console limited to read-only access starts there and cannot leave.
    let mut spectator = Console::new(HUMAN, Access::ReadOnly);
    assert_eq!(spectator.session.mode, Mode::ReadOnly);
    assert!(!spectator.execute(&mut g, "mode legal").ok);
    assert!(!spectator.execute(&mut g, "mode override").ok);
    let mut player = Console::new(HUMAN, Access::Play);
    assert!(player.execute(&mut g, "mode readonly").ok);
    assert!(player.execute(&mut g, "mode legal").ok);
    assert!(!player.execute(&mut g, "mode override").ok);
}

fn content() -> f4x_core::content::Content {
    f4x_data::load_content(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")).unwrap()
}

#[test]
fn new_game_settings_reach_the_game() {
    let content = content();
    let made = |spec: NewGame| new_game(&content, &spec).unwrap();
    let base = made(NewGame {
        seed: 7,
        ..NewGame::default()
    });
    assert_eq!((base.difficulty, base.magic_strength), (2, 1));
    assert_eq!(wizards(&base), 2);

    let small = made(NewGame {
        seed: 7,
        wizards: 5,
        land_percent: f4x_core::setup::LAND_SMALL,
        difficulty: 4,
        magic_strength: 2,
        ..NewGame::default()
    });
    assert_eq!(wizards(&small), 5, "four opponents");
    assert_eq!((small.difficulty, small.magic_strength), (4, 2));
    let land = |g: &Game| {
        let map = g.map.as_ref().unwrap();
        f4x_core::mapgen::land_percent(
            map.plane(f4x_core::map::Plane::Arcanus),
            &g.content.terrains,
        )
    };
    assert!(
        land(&small).abs_diff(15) <= 2,
        "small land {}",
        land(&small)
    );
    assert!(land(&base).abs_diff(30) <= 2, "large land {}", land(&base));
}

#[test]
fn the_new_command_takes_the_settings_by_name() {
    let mut game = game();
    let mut c = console();
    let r = c.execute(
        &mut game,
        "new 3 land=small difficulty=hard magic=weak players=3",
    );
    assert!(r.error.is_none(), "{:?}", r.error);
    assert_eq!((game.difficulty, game.magic_strength), (3, 0));
    assert_eq!(wizards(&game), 3);
    let r = c.execute(&mut game, "new 3 difficulty=nightmare");
    assert!(matches!(r.error, Some(ConsoleError::Parse(_))));
}

#[test]
fn the_vizier_command_turns_the_vizier_on_and_off() {
    let (mut g, mut c) = (game(), console());
    assert!(!g.player(HUMAN).unwrap().grand_vizier);
    assert!(c.execute(&mut g, "vizier on").error.is_none());
    assert!(g.player(HUMAN).unwrap().grand_vizier);
    assert!(c.execute(&mut g, "vizier off").error.is_none());
    assert!(!g.player(HUMAN).unwrap().grand_vizier);
    assert!(c.execute(&mut g, "vizier maybe").error.is_some());
}

#[test]
fn a_global_enchantment_is_a_target_by_wizard_and_spell() {
    let g = game();
    let awareness = g.content.rules.spells.id_of("awareness").unwrap();
    assert_eq!(
        crate::args::parse_target(&g, "global:1:awareness").unwrap(),
        f4x_core::rules::exec::Target::Global(PlayerId(1), awareness)
    );
    assert!(crate::args::parse_target(&g, "global:1").is_err());
    assert!(crate::args::parse_target(&g, "global:1:no_such_spell").is_err());
}

fn neutral_cities(g: &Game) -> Vec<(hecs::Entity, f4x_core::city::Position)> {
    g.cities()
        .into_iter()
        .filter(|(_, c, _)| g.is_neutral_player(c.owner))
        .map(|(e, _, p)| (e, p))
        .collect()
}

#[test]
fn a_new_world_has_neutral_cities_on_both_planes() {
    use f4x_core::map::Plane;
    let g = game();
    let cities = neutral_cities(&g);
    for plane in [Plane::Arcanus, Plane::Myrror] {
        let here: Vec<_> = cities.iter().filter(|(_, p)| p.plane == plane).collect();
        assert!((2..=15).contains(&here.len()), "{plane:?}: {}", here.len());
        // Cities keep their distance from each other and from every fortress.
        let grid = g.map.as_ref().unwrap().plane(plane);
        for (i, (_, a)) in here.iter().enumerate() {
            for (_, b) in &here[i + 1..] {
                assert!(grid.chebyshev((a.x, a.y), (b.x, b.y)) >= 4);
            }
            for f in g.players.iter().filter_map(|p| p.fortress) {
                assert!(f.plane != plane || grid.chebyshev((a.x, a.y), (f.x, f.y)) >= 5);
            }
        }
    }
    let mut names = std::collections::BTreeSet::new();
    for (e, p) in &cities {
        let races = &g.content.rules.races;
        let race = races.get(g.world.get::<&f4x_core::economy::CityRace>(*e).unwrap().0);
        assert_eq!(
            race.myrror,
            p.plane == Plane::Myrror,
            "{} on {:?}",
            race.id,
            p.plane
        );
        assert!(names.insert(
            g.world
                .get::<&f4x_core::city::City>(*e)
                .unwrap()
                .name
                .clone()
        ));
        let pop = g
            .world
            .get::<&f4x_core::economy::Population>(*e)
            .unwrap()
            .figures;
        assert!((2..=14).contains(&pop), "population {pop}");
        // Garrison: a quarter of the population, once for each of the two kinds.
        let garrison = g.units_at(*p).len() as u32;
        assert!(garrison <= 2 * (pop / 4), "{garrison} units for {pop}");
        for u in g.units_at(*p) {
            assert_eq!(
                g.unit_owner(u),
                g.city_owner(*e),
                "the garrison is the city's own"
            );
        }
    }
    assert!(
        cities.iter().any(|(_, p)| !g.units_at(*p).is_empty()),
        "some city is garrisoned"
    );
}

#[test]
fn neutral_worlds_follow_the_seed() {
    let content = content();
    let made = |seed| {
        let g = new_game(
            &content,
            &NewGame {
                seed,
                ..NewGame::default()
            },
        )
        .unwrap();
        neutral_cities(&g)
            .into_iter()
            .map(|(_, p)| p)
            .collect::<Vec<_>>()
    };
    assert_eq!(made(3), made(3));
    assert_ne!(made(3), made(4));
}

#[test]
fn neutral_cities_play_by_the_ordinary_rules() {
    use f4x_core::Command;
    let mut g = game();
    let before: Vec<_> = neutral_cities(&g)
        .into_iter()
        .map(|(e, _)| {
            let pop = g.world.get::<&f4x_core::economy::Population>(e).unwrap();
            (
                e,
                pop.figures,
                g.world
                    .get::<&f4x_core::economy::Buildings>(e)
                    .unwrap()
                    .0
                    .len(),
            )
        })
        .collect();
    let gold: Vec<_> = g.players.iter().map(|p| p.gold).collect();
    for _ in 0..40 {
        g.apply(Command::EndTurn);
    }
    assert_eq!(
        neutral_cities(&g).len(),
        before.len(),
        "no neutral city is lost on its own"
    );
    let grew = before.iter().any(|&(e, pop, _)| {
        g.world
            .get::<&f4x_core::economy::Population>(e)
            .unwrap()
            .figures
            > pop
    });
    let built = before.iter().any(|&(e, _, n)| {
        g.world
            .get::<&f4x_core::economy::Buildings>(e)
            .unwrap()
            .0
            .len()
            > n
    });
    assert!(grew, "a neutral city grew");
    assert!(built, "a neutral city built something");
    assert!(g.players.iter().all(|p| !p.defeated));
    let _ = gold;
}

#[test]
fn every_neutral_city_has_a_player_and_a_treasury_of_its_own() {
    let mut g = game();
    let cities = neutral_cities(&g);
    let owners: std::collections::BTreeSet<_> = cities
        .iter()
        .filter_map(|(e, _)| g.city_owner(*e))
        .collect();
    assert_eq!(owners.len(), cities.len(), "one player each");
    for &o in &owners {
        let p = g.player(o).unwrap();
        assert!(p.neutral && p.grand_vizier && p.gold > 0);
        assert!(!g.living_wizards().contains(&o), "not a wizard");
    }
    assert_eq!(wizards(&g), 2);
    // Their taxes and upkeep move their own gold, and nobody else's.
    let wizard_gold: Vec<_> = g
        .players
        .iter()
        .filter(|p| !p.neutral)
        .map(|p| p.gold)
        .collect();
    let before: Vec<_> = owners.iter().map(|&o| g.player(o).unwrap().gold).collect();
    g.apply(f4x_core::Command::EndTurn);
    let after: Vec<_> = owners.iter().map(|&o| g.player(o).unwrap().gold).collect();
    assert_ne!(before, after, "the treasuries moved");
    assert_eq!(
        wizard_gold.len(),
        g.players.iter().filter(|p| !p.neutral).count()
    );
}

#[test]
fn a_neutral_city_that_cannot_pay_loses_its_garrison() {
    let mut g = game();
    let (city, pos) = neutral_cities(&g)
        .into_iter()
        .find(|(_, p)| g.units_at(*p).len() >= 2)
        .expect("a garrisoned city");
    let owner = g.city_owner(city).unwrap();
    let units = g.units_at(pos).len();
    // Buildings the city's taxes cannot carry, and an empty treasury.
    let all: Vec<_> = (0..g.content.rules.buildings.defs().len())
        .map(|i| g.content.rules.buildings.defs()[i].id.clone())
        .filter_map(|id| g.content.rules.buildings.id_of(&id))
        .collect();
    let _ = g.world.insert_one(city, f4x_core::economy::Buildings(all));
    g.player_mut(owner).unwrap().gold = 0;
    g.apply(f4x_core::Command::EndTurn);
    assert!(g.units_at(pos).len() < units, "{units} units stayed");
}

#[test]
fn taking_a_neutral_city_changes_its_owner_and_removes_its_player() {
    let mut g = game();
    let wizards_before = wizards(&g);
    let (city, _) = neutral_cities(&g)[0];
    let owner = g.city_owner(city).unwrap();
    let events = g.capture_city(city, PlayerId(0));
    assert_eq!(g.city_owner(city), Some(PlayerId(0)));
    assert!(events
        .iter()
        .all(|e| matches!(e, f4x_core::Event::CityCaptured { .. })));
    assert!(g.player(owner).is_none(), "the city's player goes with it");
    assert_eq!(g.player_units(owner).len(), 0);
    // Even when the last one falls nobody is "defeated".
    for (c, _) in neutral_cities(&g) {
        assert!(g
            .capture_city(c, PlayerId(0))
            .iter()
            .all(|e| matches!(e, f4x_core::Event::CityCaptured { .. })));
    }
    assert!(neutral_cities(&g).is_empty());
    assert_eq!(wizards(&g), wizards_before);
    assert!(g.players.iter().all(|p| !p.defeated));
}

#[test]
fn neutral_cities_survive_a_save() {
    let g = game();
    let snap = g.snapshot();
    let back = snap.clone().restore(g.content.clone()).unwrap();
    assert_eq!(back.snapshot(), snap);
    assert_eq!(neutral_cities(&back).len(), neutral_cities(&g).len());
}

#[test]
fn neutral_cities_keep_their_garrisons_for_sixty_turns() {
    let content = content();
    for seed in [1, 2] {
        let mut g = new_game(
            &content,
            &NewGame {
                seed,
                ..NewGame::default()
            },
        )
        .unwrap();
        for turn in 0..60 {
            for e in g.apply(f4x_core::Command::EndTurn) {
                if let f4x_core::Event::UnitsDisbanded { player, .. } = e {
                    assert!(
                        !g.is_neutral_player(player),
                        "seed {seed} turn {turn}: {player:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_world_has_the_originals_number_of_lairs() {
    use f4x_core::map::Plane;
    let content = content();
    for seed in [1, 2, 3, 4] {
        let g = new_game(
            &content,
            &NewGame {
                seed,
                ..NewGame::default()
            },
        )
        .unwrap();
        let map = g.map.as_ref().unwrap();
        // 6 towers on each plane, a lair at each of the 30 nodes, and 25 + 32 random sites.
        assert_eq!(map.nodes().len(), 30, "seed {seed}");
        assert_eq!(map.lairs().len(), 12 + 30 + 57, "seed {seed}");
        for plane in [Plane::Arcanus, Plane::Myrror] {
            // Up to 15 a plane: the original gives up after 1000 tries, too.
            let n = neutral_cities(&g)
                .iter()
                .filter(|(_, p)| p.plane == plane)
                .count();
            assert!((2..=15).contains(&n), "seed {seed} {plane:?}: {n}");
        }
    }
}

/// The units, in the order they were made, of a player's starting army.
fn start_army(g: &Game, p: PlayerId) -> Vec<String> {
    g.player_units(p)
        .iter()
        .filter_map(|&u| g.unit_def(u).map(|d| d.name.clone()))
        .collect()
}

/// A wizard's capital and what is in it.
fn capital(g: &Game, p: PlayerId) -> (hecs::Entity, f4x_core::city::Position) {
    g.cities()
        .into_iter()
        .find(|(_, c, _)| c.owner == p)
        .map(|(e, _, at)| (e, at))
        .unwrap()
}

#[test]
fn a_wizard_starts_as_in_the_original() {
    let content = content();
    for seed in [1, 2, 3, 4, 5, 6] {
        let g = new_game(
            &content,
            &NewGame {
                seed,
                wizards: 4,
                ..NewGame::default()
            },
        )
        .unwrap();
        for k in 0..4 {
            let p = PlayerId(k);
            let (city, _) = capital(&g, p);
            let mut army = start_army(&g, p);
            army.sort();
            assert_eq!(army, ["Spearmen", "Swordsmen"], "seed {seed} wizard {k}");
            let eco = g.city_economy(city).unwrap();
            assert_eq!(
                (eco.figures, eco.farmers, g.player(p).unwrap().tax_rate),
                (4, 3, 2)
            );
            for b in ["smithy", "barracks", "builders_hall"] {
                assert!(g.city_has_building(city, b), "seed {seed} wizard {k}: {b}");
            }
            // Land that feeds eight (seven for later wizards), so the farmers feed the army.
            assert!(
                eco.max_size >= 7,
                "seed {seed} wizard {k}: {}",
                eco.max_size
            );
            assert_eq!(g.player(p).unwrap().gold, 100);
        }
    }
}

#[test]
fn the_starting_army_and_capital_pay_their_way() {
    let content = content();
    for seed in [1, 2, 3, 4, 5, 6] {
        let mut g = new_game(
            &content,
            &NewGame {
                seed,
                ..NewGame::default()
            },
        )
        .unwrap();
        let t = g.treasury(HUMAN);
        assert!(t.food_surplus >= t.food_upkeep, "seed {seed}: {t:?}");
        assert!(t.net_gold() >= 0, "seed {seed}: {t:?}");
        // Growth makes new mouths, so a careful ruler keeps putting people on the farms.
        for _ in 0..8 {
            g.balance_farmers(HUMAN);
            g.apply(f4x_core::Command::EndTurn);
        }
        assert_eq!(start_army(&g, HUMAN).len(), 2, "seed {seed}");
        assert!(g.player(HUMAN).unwrap().gold >= 100, "seed {seed}");
    }
}

#[test]
fn a_myrran_wizard_starts_on_myrror_with_a_myrror_race() {
    use f4x_core::magic::Retort;
    use f4x_core::map::Plane;
    let content = content();
    let wizard = |race: Option<&str>| NewGame {
        seed: 2,
        books: [("sorcery".to_string(), 5), ("nature".to_string(), 3)]
            .into_iter()
            .collect(),
        retorts: [Retort::Myrran].into_iter().collect(),
        race: race.map(str::to_string),
        ..NewGame::default()
    };
    let g = new_game(&content, &wizard(Some("dwarven"))).unwrap();
    let (city, at) = capital(&g, HUMAN);
    assert_eq!(at.plane, Plane::Myrror);
    // Dwarves have no spearmen: two swordsmen.
    assert_eq!(start_army(&g, HUMAN), ["Swordsmen", "Swordsmen"]);
    assert!(g.city_economy(city).unwrap().max_size >= 7);
    // The rival is no Myrran and starts on Arcanus.
    assert_eq!(capital(&g, PlayerId(1)).1.plane, Plane::Arcanus);
    // Nobody picks an Arcanus race for a Myrran wizard.
    assert!(new_game(&content, &wizard(Some("high_men"))).is_err());
    let any = new_game(&content, &wizard(None)).unwrap();
    assert_eq!(capital(&any, HUMAN).1.plane, Plane::Myrror);
}
