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
    assert_eq!(base.players.len(), 2);

    let small = made(NewGame {
        seed: 7,
        wizards: 5,
        land_percent: f4x_core::setup::LAND_SMALL,
        difficulty: 4,
        magic_strength: 2,
        ..NewGame::default()
    });
    assert_eq!(small.players.len(), 5, "four opponents");
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
    assert_eq!(game.players.len(), 3);
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
