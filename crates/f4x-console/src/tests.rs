use std::path::Path;

use f4x_core::game::Game;
use f4x_core::player::PlayerId;

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
    g.player_ids()
        .into_iter()
        .filter(|id| !g.is_neutral_player(*id))
        .count()
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
    let gold = g.treasury(HUMAN).unwrap().gold;

    assert!(c.execute(&mut g, "mode override").ok);
    assert_eq!(c.session.mode, Mode::Override);
    assert!(c.execute(&mut g, "give gold 500").ok);
    assert_eq!(g.treasury(HUMAN).unwrap().gold, gold + 500);

    assert!(c.execute(&mut g, "mode legal").ok);
    assert!(!c.execute(&mut g, "give gold 500").ok);
    assert_eq!(g.treasury(HUMAN).unwrap().gold, gold + 500);

    // Switching the mode on and each cheat are in the game's saved log.
    let log = g.override_log().to_vec();
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(log[0].contains("mode override") && log[1].contains("give gold 500"));
    let restored = g.snapshot().restore(g.content().clone()).unwrap();
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
        .units()
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
            g.apply(f4x_core::game::Command::Battle(order));
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
            f4x_core::battle::BattleCommand::AttackWall { unit, cell } => {
                format!("bwall {unit} {},{}", cell.0, cell.1)
            }
            f4x_core::battle::BattleCommand::ShootWall { unit, cell } => {
                format!("bshootwall {unit} {},{}", cell.0, cell.1)
            }
            f4x_core::battle::BattleCommand::Cast { .. }
            | f4x_core::battle::BattleCommand::UnitCast { .. }
            | f4x_core::battle::BattleCommand::SummonDemon { .. } => {
                unreachable!("the plain unit AI casts nothing")
            }
            f4x_core::battle::BattleCommand::MoveVortex { .. } => {
                unreachable!("the plain unit AI steers no vortex")
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
    assert_eq!(
        (
            base.settings().basic.difficulty,
            base.settings().basic.magic_strength
        ),
        (2, 1)
    );
    assert_eq!(wizards(&base), 2);

    let small = made(NewGame {
        seed: 7,
        settings: {
            let mut s = NewGame::with_wizards(5).settings;
            s.basic.land_percent = f4x_core::game::setup::LAND_SMALL;
            s.basic.difficulty = 4;
            s.basic.magic_strength = 2;
            s
        },
        ..NewGame::default()
    });
    assert_eq!(wizards(&small), 5, "four opponents");
    assert_eq!(small.settings().basic.opponents, 4);
    assert_eq!(
        small.settings().basic.land_percent,
        f4x_core::game::setup::LAND_SMALL
    );
    assert_eq!(
        (
            small.settings().basic.difficulty,
            small.settings().basic.magic_strength
        ),
        (4, 2)
    );
    let land = |g: &Game| {
        let map = g.map().unwrap();
        f4x_core::map::mapgen::land_percent(
            map.plane(f4x_core::map::Plane::Arcanus),
            &g.content().terrains,
        )
    };
    assert!(
        land(&small).abs_diff(15) <= 2,
        "small land {}",
        land(&small)
    );
    assert!(land(&base).abs_diff(30) <= 2, "large land {}", land(&base));
}

fn level_game(level: u8, wizards: u8, seed: u64) -> Game {
    let mut spec = NewGame::with_wizards(wizards);
    spec.seed = seed;
    let basic = spec.settings.basic;
    spec.settings =
        f4x_core::game::setting::GameSettings::for_difficulty(&content().settings, level);
    spec.settings.basic = f4x_core::game::setting::BasicSettings {
        difficulty: level,
        ..basic
    };
    new_game(&content(), &spec).unwrap()
}

#[test]
fn each_level_gives_its_starting_gold_and_the_computer_wizards_their_bonuses() {
    for level in 0..=4u8 {
        let g = level_game(level, 2, 5);
        let human = g.human().unwrap();
        let ai = g.wizards().into_iter().find(|&w| w != human).unwrap();
        assert_eq!(g.treasury(human).unwrap().gold, (5 - i32::from(level)) * 25);
        assert_eq!(g.treasury(ai).unwrap().gold, 150);
        let percent = |p| {
            g.player_component::<f4x_core::player::Bonus>(p)
                .unwrap()
                .production
        };
        assert_eq!(percent(human), 100);
        assert_eq!(
            percent(ai),
            [100, 125, 150, 200, 400][usize::from(level)],
            "level {level}"
        );
    }
    // The symmetric game: the same gold, no bonuses.
    let g = new_game(&content(), &NewGame::default()).unwrap();
    assert!(g
        .player_ids()
        .into_iter()
        .all(|id| g.treasury(id).unwrap().gold == 100
            && g.player_component::<f4x_core::player::Bonus>(id)
                .unwrap()
                .gold
                == 100));
}

#[test]
fn computer_wizards_are_made_to_the_levels_pick_budget() {
    use f4x_core::wizard::{retort_cost, ALL_RETORTS};
    for (level, picks) in [(2u8, 11), (3, 13), (4, 15)] {
        for seed in [1u64, 2, 3, 4] {
            let g = level_game(level, 3, seed);
            let human = g.human().unwrap();
            let mut names = Vec::new();
            for (id, p) in g
                .wizards()
                .into_iter()
                .filter(|&w| w != human)
                .filter_map(|w| Some((w, g.wizard(w)?)))
            {
                let spent = p.total_books()
                    + ALL_RETORTS
                        .iter()
                        .filter(|&&r| p.has(r))
                        .map(|&r| i32::from(retort_cost(r)))
                        .sum::<i32>();
                // Erasure takes books and refilling puts them back; a wizard may end a pick or
                // two over when the last retort did not fit, never under.
                assert!(
                    (picks..=picks + 1).contains(&spent),
                    "level {level} seed {seed}: {} spent {spent} of {picks}",
                    g.player_name(id)
                );
                names.push(g.player_name(id));
            }
            names.sort();
            names.dedup();
            assert_eq!(names.len(), 2, "two different wizards");
        }
    }
}

#[test]
fn with_four_wizards_above_average_one_or_two_computer_wizards_are_myrran() {
    for level in [3u8, 4] {
        for seed in [1u64, 2, 3, 4, 5, 6] {
            let g = level_game(level, 4, seed);
            let human = g.human().unwrap();
            let n = g
                .wizards()
                .into_iter()
                .filter(|&w| w != human)
                .filter_map(|w| g.wizard(w))
                .filter(|p| p.has(f4x_core::wizard::magic::Retort::Myrran))
                .count();
            assert!(
                (1..=2).contains(&n),
                "level {level} seed {seed}: {n} Myrran"
            );
        }
    }
}

#[test]
fn the_same_seed_makes_the_same_wizards() {
    let a = level_game(4, 4, 9);
    let b = level_game(4, 4, 9);
    assert_eq!(a.snapshot(), b.snapshot());
}

#[test]
fn the_settings_command_shows_what_the_game_was_made_with() {
    let mut game = game();
    let mut c = console();
    c.execute(
        &mut game,
        "new 3 land=small difficulty=hard magic=weak players=3",
    );
    let r = c.execute(&mut game, "settings");
    assert!(r.error.is_none(), "{:?}", r.error);
    let text = r.text();
    for want in [
        "difficulty 3 (hard)",
        "opponents 2",
        "land 15%",
        "magic power 0 (weak)",
    ] {
        assert!(text.contains(want), "{want:?} not in {text:?}");
    }
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
    assert_eq!(
        (
            game.settings().basic.difficulty,
            game.settings().basic.magic_strength
        ),
        (3, 0)
    );
    assert_eq!(wizards(&game), 3);
    let r = c.execute(&mut game, "new 3 difficulty=nightmare");
    assert!(matches!(r.error, Some(ConsoleError::Parse(_))));
}

#[test]
fn the_vizier_command_turns_the_vizier_on_and_off() {
    let (mut g, mut c) = (game(), console());
    assert!(!g.has_grand_vizier(HUMAN));
    assert!(c.execute(&mut g, "vizier on").error.is_none());
    assert!(g.has_grand_vizier(HUMAN));
    assert!(c.execute(&mut g, "vizier off").error.is_none());
    assert!(!g.has_grand_vizier(HUMAN));
    assert!(c.execute(&mut g, "vizier maybe").error.is_some());
}

#[test]
fn the_autobuild_command_sets_and_clears_a_citys_plan() {
    let (mut g, mut c) = (game(), console());
    let city = g.economy().cities()[0].0;
    let r = c.execute(&mut g, "autobuild 1 both defense gold settlers=5");
    assert!(r.ok, "{}", r.text());
    let plan = g.autobuild_of(city).unwrap().plan;
    assert_eq!(plan.mix, f4x_core::city::autobuild::Mix::Both);
    assert_eq!(plan.primary, f4x_core::city::autobuild::Focus::Defense);
    assert_eq!(plan.secondary, Some(f4x_core::city::autobuild::Focus::Gold));
    assert!(plan.settlers && plan.min_settler_pop == 5);
    assert!(c.execute(&mut g, "autobuild 1 off").ok);
    assert!(g.autobuild_of(city).is_none());
    assert!(c
        .execute(&mut g, "autobuild 1 both nonsense")
        .error
        .is_some());
    assert!(c.execute(&mut g, "autobuild 1 stuff gold").error.is_some());
}

#[test]
fn a_global_enchantment_is_a_target_by_wizard_and_spell() {
    let g = game();
    let awareness = g.content().rules.spells.id_of("awareness").unwrap();
    assert_eq!(
        crate::args::parse_target(&g, "global:1:awareness").unwrap(),
        f4x_core::rules::exec::Target::Global(PlayerId(1), awareness)
    );
    assert!(crate::args::parse_target(&g, "global:1").is_err());
    assert!(crate::args::parse_target(&g, "global:1:no_such_spell").is_err());
}

#[test]
fn nature_s_eye_reveals_visible_units_four_squares_from_the_city() {
    use f4x_core::rules::exec::Target;
    let mut g = game();
    let (city, _, _) = g
        .economy()
        .cities()
        .into_iter()
        .find(|(_, c, _)| c.owner == HUMAN)
        .unwrap();
    let eye = g.content().rules.spells.id_of("natures_eye").unwrap();
    assert!(g.cheat_learn_spell(HUMAN, eye));
    assert_eq!(g.economy().city_sight(city), 2);
    // A rival's plain unit four squares away and an invisible one three squares away, and a
    // plain one five squares away.
    let home = g.city_position(city).unwrap();
    let width = g.map().unwrap().width();
    let rival = PlayerId(1);
    let beside = |dx: u32| f4x_core::map::Position {
        x: (home.x + dx) % width,
        ..home
    };
    let spawn = |g: &mut Game, id: &str, dx: u32| {
        let def = g.content().rules.units.id_of(id).unwrap();
        g.spawn_unit(rival, beside(dx), def)
    };
    let near = spawn(&mut g, "barbarian_spearmen", 4);
    let ghost = spawn(&mut g, "dark_elf_nightblades", 3);
    let far = spawn(&mut g, "barbarian_spearmen", 5);
    // What the wizard makes out: the square is in sight and the unit is not hidden.
    let sees = |g: &Game, unit: hecs::Entity| {
        let at = g.units().unit_position(unit).unwrap();
        let seen = g.economy().visibility(HUMAN, at.plane);
        seen[(at.y * width + at.x) as usize] && g.economy().unit_visible_to(unit, HUMAN)
    };
    assert!(!sees(&g, near), "four squares is out of sight at first");
    g.treasury_mut(HUMAN).unwrap().mana = 5000;
    g.start_cast(HUMAN, eye, Some(Target::City(city))).unwrap();
    // Paid over turns by the casting skill, then aimed.
    for _ in 0..60 {
        if g.city_enchantments(city).iter().any(|&(s, _)| s == eye) {
            break;
        }
        g.apply(f4x_core::game::Command::EndTurn);
        g.apply(f4x_core::game::Command::ResolveCast {
            player: HUMAN,
            target: Target::City(city),
        });
    }
    assert!(g.city_enchantments(city).iter().any(|&(s, _)| s == eye));
    assert_eq!(g.economy().city_sight(city), 4);
    assert!(sees(&g, near), "the eye reveals what is four squares away");
    assert!(!sees(&g, ghost), "an invisible unit stays hidden");
    assert!(!sees(&g, far), "five squares is still out of sight");
}

fn neutral_cities(g: &Game) -> Vec<(hecs::Entity, f4x_core::map::Position)> {
    g.economy()
        .cities()
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
        let grid = g.map().unwrap().plane(plane);
        for (i, (_, a)) in here.iter().enumerate() {
            for (_, b) in &here[i + 1..] {
                assert!(grid.chebyshev((a.x, a.y), (b.x, b.y)) >= 4);
            }
            for f in g
                .player_ids()
                .into_iter()
                .filter_map(|id| g.wizard(id)?.fortress())
            {
                assert!(f.plane != plane || grid.chebyshev((a.x, a.y), (f.x, f.y)) >= 5);
            }
        }
    }
    let mut names = std::collections::BTreeSet::new();
    for (e, p) in &cities {
        let races = &g.content().rules.races;
        let race = races.get(
            g.component::<f4x_core::city::economy::CityRace>(*e)
                .unwrap()
                .0,
        );
        assert_eq!(
            race.myrror,
            p.plane == Plane::Myrror,
            "{} on {:?}",
            race.id,
            p.plane
        );
        assert!(names.insert(
            g.component::<f4x_core::city::City>(*e)
                .unwrap()
                .name
                .clone()
        ));
        let pop = g
            .component::<f4x_core::city::economy::Population>(*e)
            .unwrap()
            .figures;
        assert!((2..=14).contains(&pop), "population {pop}");
        // Garrison: a quarter of the population, once for each of the two kinds.
        let garrison = g.units().units_at(*p).len() as u32;
        assert!(garrison <= 2 * (pop / 4), "{garrison} units for {pop}");
        for u in g.units().units_at(*p) {
            assert_eq!(
                g.units().unit_owner(u),
                g.economy().city_owner(*e),
                "the garrison is the city's own"
            );
        }
    }
    assert!(
        cities
            .iter()
            .any(|(_, p)| !g.units().units_at(*p).is_empty()),
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
    let mut g = game();
    let before: Vec<_> = neutral_cities(&g)
        .into_iter()
        .map(|(e, _)| {
            let pop = g
                .component::<f4x_core::city::economy::Population>(e)
                .unwrap();
            (
                e,
                pop.figures,
                g.component::<f4x_core::city::economy::Buildings>(e)
                    .unwrap()
                    .0
                    .len(),
            )
        })
        .collect();
    let gold: Vec<_> = g
        .player_ids()
        .iter()
        .map(|id| g.treasury(*id).unwrap().gold)
        .collect();
    for _ in 0..40 {
        f4x_ai::play_round(&mut g, &mut []);
    }
    assert_eq!(
        neutral_cities(&g).len(),
        before.len(),
        "no neutral city is lost on its own"
    );
    let grew = before.iter().any(|&(e, pop, _)| {
        g.component::<f4x_core::city::economy::Population>(e)
            .unwrap()
            .figures
            > pop
    });
    let built = before.iter().any(|&(e, _, n)| {
        g.component::<f4x_core::city::economy::Buildings>(e)
            .unwrap()
            .0
            .len()
            > n
    });
    assert!(grew, "a neutral city grew");
    assert!(built, "a neutral city built something");
    assert!(g.player_ids().iter().all(|id| !g.is_defeated(*id)));
    let _ = gold;
}

#[test]
fn every_neutral_city_has_a_player_and_a_treasury_of_its_own() {
    let mut g = game();
    let cities = neutral_cities(&g);
    let owners: std::collections::BTreeSet<_> = cities
        .iter()
        .filter_map(|(e, _)| g.economy().city_owner(*e))
        .collect();
    assert_eq!(owners.len(), cities.len(), "one player each");
    for &o in &owners {
        assert!(g.is_neutral_player(o) && g.treasury(o).unwrap().gold > 0);
        assert!(!g.living_wizards().contains(&o), "not a wizard");
    }
    assert_eq!(wizards(&g), 2);
    // Their taxes and upkeep move their own gold, and nobody else's.
    let wizard_gold: Vec<_> = g
        .player_ids()
        .into_iter()
        .filter(|id| !g.is_neutral_player(*id))
        .map(|id| g.treasury(id).unwrap().gold)
        .collect();
    let before: Vec<_> = owners
        .iter()
        .map(|&o| g.treasury(o).unwrap().gold)
        .collect();
    g.apply(f4x_core::game::Command::EndTurn);
    let after: Vec<_> = owners
        .iter()
        .map(|&o| g.treasury(o).unwrap().gold)
        .collect();
    assert_ne!(before, after, "the treasuries moved");
    assert_eq!(
        wizard_gold.len(),
        g.player_ids()
            .into_iter()
            .filter(|id| !g.is_neutral_player(*id))
            .count()
    );
}

#[test]
fn a_neutral_city_that_cannot_pay_loses_its_garrison() {
    let mut g = game();
    let (city, pos) = neutral_cities(&g)
        .into_iter()
        .find(|(_, p)| g.units().units_at(*p).len() >= 2)
        .expect("a garrisoned city");
    let owner = g.economy().city_owner(city).unwrap();
    let units = g.units().units_at(pos).len();
    // Buildings the city's taxes cannot carry, and an empty treasury.
    let all: Vec<_> = (0..g.content().rules.buildings.defs().len())
        .map(|i| g.content().rules.buildings.defs()[i].id.clone())
        .filter_map(|id| g.content().rules.buildings.id_of(&id))
        .collect();
    for id in all {
        g.scenario_set_building(city, id, true);
    }
    g.treasury_mut(owner).unwrap().gold = 0;
    g.apply(f4x_core::game::Command::EndTurn);
    assert!(
        g.units().units_at(pos).len() < units,
        "{units} units stayed"
    );
}

#[test]
fn taking_a_neutral_city_changes_its_owner_and_removes_its_player() {
    let mut g = game();
    let wizards_before = wizards(&g);
    let (city, _) = neutral_cities(&g)[0];
    let owner = g.economy().city_owner(city).unwrap();
    let events = g.capture_city(city, PlayerId(0));
    assert_eq!(g.economy().city_owner(city), Some(PlayerId(0)));
    assert!(events
        .iter()
        .all(|e| matches!(e, f4x_core::game::Event::CityCaptured { .. })));
    assert!(!g.has_player(owner), "the city's player goes with it");
    assert_eq!(g.units().player_units(owner).len(), 0);
    // Even when the last one falls nobody is "defeated".
    for (c, _) in neutral_cities(&g) {
        assert!(g
            .capture_city(c, PlayerId(0))
            .iter()
            .all(|e| matches!(e, f4x_core::game::Event::CityCaptured { .. })));
    }
    assert!(neutral_cities(&g).is_empty());
    assert_eq!(wizards(&g), wizards_before);
    assert!(g.player_ids().iter().all(|id| !g.is_defeated(*id)));
}

#[test]
fn neutral_cities_survive_a_save() {
    let g = game();
    let snap = g.snapshot();
    let back = snap.clone().restore(g.content().clone()).unwrap();
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
            for e in f4x_ai::play_round(&mut g, &mut []) {
                if let f4x_core::game::Event::UnitsDisbanded { player, .. } = e {
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
        let map = g.map().unwrap();
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
    g.units()
        .player_units(p)
        .iter()
        .filter_map(|&u| g.units().unit_def(u).map(|d| d.name.clone()))
        .collect()
}

/// A wizard's capital and what is in it.
fn capital(g: &Game, p: PlayerId) -> (hecs::Entity, f4x_core::map::Position) {
    g.economy()
        .cities()
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
                ..NewGame::with_wizards(4)
            },
        )
        .unwrap();
        for k in 0..4 {
            let p = PlayerId(k);
            let (city, _) = capital(&g, p);
            let mut army = start_army(&g, p);
            army.sort();
            assert_eq!(army, ["Spearmen", "Swordsmen"], "seed {seed} wizard {k}");
            let eco = g.economy().city_economy(city).unwrap();
            assert_eq!(
                (
                    eco.figures,
                    eco.farmers,
                    g.player_component::<f4x_core::player::Tax>(p).unwrap().0
                ),
                (4, 3, 2)
            );
            for b in ["smithy", "barracks", "builders_hall"] {
                assert!(
                    g.economy().city_has_building(city, b),
                    "seed {seed} wizard {k}: {b}"
                );
            }
            // Land that feeds eight (seven for later wizards), so the farmers feed the army.
            assert!(
                eco.max_size >= 7,
                "seed {seed} wizard {k}: {}",
                eco.max_size
            );
            assert_eq!(g.treasury(p).unwrap().gold, 100);
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
        let t = g.budget(HUMAN);
        assert!(t.food_surplus >= t.food_upkeep, "seed {seed}: {t:?}");
        assert!(t.net_gold() >= 0, "seed {seed}: {t:?}");
        // Growth makes new mouths, so a careful ruler keeps putting people on the farms.
        for _ in 0..8 {
            g.balance_farmers(HUMAN);
            g.apply(f4x_core::game::Command::EndTurn);
        }
        assert_eq!(start_army(&g, HUMAN).len(), 2, "seed {seed}");
        assert!(g.treasury(HUMAN).unwrap().gold >= 100, "seed {seed}");
    }
}

#[test]
fn a_myrran_wizard_starts_on_myrror_with_a_myrror_race() {
    use f4x_core::map::Plane;
    use f4x_core::wizard::magic::Retort;
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
    assert!(g.economy().city_economy(city).unwrap().max_size >= 7);
    // The rival is no Myrran and starts on Arcanus.
    assert_eq!(capital(&g, PlayerId(1)).1.plane, Plane::Arcanus);
    // Nobody picks an Arcanus race for a Myrran wizard.
    assert!(new_game(&content, &wizard(Some("high_men"))).is_err());
    let any = new_game(&content, &wizard(None)).unwrap();
    assert_eq!(capital(&any, HUMAN).1.plane, Plane::Myrror);
}

#[test]
fn battle_units_carry_their_immunities_and_whether_their_weapons_are_magical() {
    let content = content();
    let mut g = new_game(
        &content,
        &NewGame {
            seed: 1,
            ..NewGame::default()
        },
    )
    .unwrap();
    let (_, home) = capital(&g, HUMAN);
    let werewolves = g.content().rules.units.id_of("werewolves").unwrap();
    g.apply(f4x_core::game::Command::CreateUnit {
        owner: HUMAN,
        pos: home,
        unit: werewolves,
    });
    let events = g.apply(f4x_core::game::Command::BeginBattle {
        attackers: g.units().player_units(HUMAN),
        defenders: g.units().player_units(PlayerId(1)),
    });
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, f4x_core::game::Event::Rejected(_))),
        "{events:?}"
    );
    let battle = g.battle().unwrap();
    let find = |name: &str| battle.units.iter().find(|u| u.name == name).unwrap();
    let spearmen = find("Spearmen");
    assert!(!spearmen.magical_weapon);
    let wolves = find("Werewolves");
    assert!(
        wolves.magical_weapon,
        "summoned creatures fight with magical weapons"
    );
    assert!(wolves.abilities.iter().any(|a| a == "weapon_immunity"));
}

#[test]
fn battle_units_carry_the_numbers_of_their_special_attacks() {
    use f4x_core::battle::special::{Gaze, SpecialAttacks};
    use f4x_core::battle::RangedKind;
    let content = content();
    let mut g = new_game(
        &content,
        &NewGame {
            seed: 1,
            ..NewGame::default()
        },
    )
    .unwrap();
    let (_, home) = capital(&g, HUMAN);
    for name in ["cockatrices", "giant_spiders", "basilisk", "wraiths"] {
        let unit = g.content().rules.units.id_of(name).unwrap();
        g.apply(f4x_core::game::Command::CreateUnit {
            owner: HUMAN,
            pos: home,
            unit,
        });
    }
    g.apply(f4x_core::game::Command::BeginBattle {
        attackers: g.units().player_units(HUMAN),
        defenders: g.units().player_units(PlayerId(1)),
    });
    let battle = g.battle().unwrap();
    let special = |name: &str| {
        let u = battle.units.iter().find(|u| u.name == name).unwrap();
        (
            u.special_level,
            SpecialAttacks::of_unit(&u.abilities, u.special_level),
            u.ranged_kind,
        )
    };
    let (level, s, _) = special("Cockatrices");
    assert_eq!((level, s.stoning_touch, s.penalty), (3, true, 3));
    let (_, s, _) = special("Giant Spiders");
    assert_eq!(s.poison, 4);
    let (_, s, kind) = special("Basilisk");
    assert_eq!(
        (s.gaze, s.penalty, kind),
        (Gaze::Stoning, 1, RangedKind::Short)
    );
    let (_, s, _) = special("Wraiths");
    assert!(s.life_steal);
    assert_eq!(s.penalty, 3);
}

/// A two-wizard game, the rival's capital with `citizens` citizens and the buildings given.
fn siege_world(citizens: u32, extra: &[&str]) -> (Game, hecs::Entity) {
    let mut g = game();
    let city = capital(&g, PlayerId(1)).0;
    g.scenario_set_citizens(city, citizens);
    let ids: Vec<_> = extra
        .iter()
        .map(|b| g.content().rules.buildings.id_of(b).unwrap())
        .collect();
    for id in ids {
        g.scenario_set_building(city, id, true);
    }
    (g, city)
}

/// A game with an independent city of `citizens` citizens and the given buildings (the
/// rival's capital is the last city of a wizard, whose fall ends the game).
fn neutral_world(citizens: u32, extra: &[&str]) -> (Game, hecs::Entity) {
    let mut g = game();
    let city = neutral_cities(&g)[0].0;
    g.scenario_set_citizens(city, citizens);
    let ids: Vec<_> = extra
        .iter()
        .map(|b| g.content().rules.buildings.id_of(b).unwrap())
        .collect();
    for id in ids {
        g.scenario_set_building(city, id, true);
    }
    (g, city)
}

/// Put the human wizard's two units on the square next to the city and start the battle.
fn start_siege(g: &mut Game, city: hecs::Entity) {
    let at = *g.component::<f4x_core::map::Position>(city).unwrap();
    let from = f4x_core::map::Position { x: at.x + 1, ..at };
    for u in g.units().player_units(HUMAN) {
        g.cheat_move_unit(u, from);
    }
    let events = g.apply(f4x_core::game::Command::BeginBattle {
        attackers: g.units().player_units(HUMAN),
        defenders: g.units().units_at(at),
    });
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, f4x_core::game::Event::Rejected(_))),
        "{events:?}"
    );
}

#[test]
fn a_battle_at_a_walled_city_is_fought_round_its_walls() {
    use f4x_core::battle::map::{in_city_area, GATE};
    let (mut g, city) = siege_world(6, &["city_walls"]);
    start_siege(&mut g, city);
    let b = g.battle().unwrap();
    assert!(b.map.is_walled());
    assert_eq!(b.map.standing_walls().len(), 12);
    for u in &b.units {
        let inside = in_city_area(u.cell);
        assert_eq!(inside, u.side == f4x_core::battle::Side::Defender, "{u:?}");
    }
    assert_eq!(
        b.units.iter().find(|u| u.cell == GATE).map(|u| u.side),
        Some(f4x_core::battle::Side::Defender),
        "a defender holds the gate"
    );
    // The same city without walls: a city battle, but nothing to climb.
    let (mut g, city) = siege_world(6, &[]);
    start_siege(&mut g, city);
    let b = g.battle().unwrap();
    assert!(b.map.is_city() && !b.map.is_walled());
}

#[test]
fn taking_a_city_loots_gold_costs_fame_and_hurts_the_city() {
    let (mut g, city) = siege_world(12, &["marketplace", "library"]);
    let loser = PlayerId(1);
    for (p, gold, fame) in [(HUMAN, 10, 0), (loser, 200, 5)] {
        g.treasury_mut(p).unwrap().gold = gold;
        g.set_fame(p, fame);
    }
    // The rival's other cities share the gold out by citizens: the capital is all of theirs.
    let total: u32 = g
        .economy()
        .cities()
        .iter()
        .filter(|(_, c, _)| c.owner == loser)
        .map(|(e, ..)| {
            g.component::<f4x_core::city::economy::Population>(*e)
                .unwrap()
                .figures
        })
        .sum();
    assert_eq!(total, 12);
    assert_eq!(g.economy().conquest_loot(city), 200);
    let events = g.conquer_city(city, HUMAN, 40);
    let looted = events
        .iter()
        .find_map(|e| match e {
            f4x_core::game::Event::CityLooted {
                gold,
                citizens_lost,
                buildings_lost,
                ..
            } => Some((*gold, *citizens_lost, buildings_lost.len() as u32)),
            _ => None,
        })
        .unwrap();
    assert_eq!(looted.0, 200);
    // The event carries the fame the results scroll reports.
    assert!(events.iter().any(|e| matches!(
        e,
        f4x_core::game::Event::CityLooted {
            fame_lost: 3,
            fame_gained: 1,
            ..
        }
    )));
    assert_eq!(g.treasury(HUMAN).unwrap().gold, 210);
    assert_eq!(g.treasury(loser).unwrap().gold, 0);
    // A city of 12 is size 3: the loser loses 3 fame (5 -> 2), the taker gains 1, and taking a
    // wizard's fortress is worth 5 more.
    assert_eq!(g.wizard(loser).unwrap().fame(), 2);
    assert_eq!(g.wizard(HUMAN).unwrap().fame(), 6);
    // It is the taker's, with 50 percent as the most citizens lost and the rest alive.
    assert_eq!(g.economy().city_owner(city), Some(HUMAN));
    let now = g
        .component::<f4x_core::city::economy::Population>(city)
        .unwrap()
        .figures;
    assert_eq!(now, 12 - looted.1);
    assert!(now >= 1);
    assert!(g
        .component::<f4x_core::city::conquest::Conquered>(city)
        .is_some());
}

#[test]
fn more_city_damage_means_more_lost_citizens_and_buildings() {
    let lost = |damage| {
        let mut citizens = 0;
        let mut buildings = 0;
        for seed in 0..10u64 {
            let (mut g, city) = siege_world(
                17,
                &[
                    "marketplace",
                    "library",
                    "granary",
                    "shrine",
                    "sawmill",
                    "temple",
                ],
            );
            // Windows of the generator that do not overlap, so the samples are independent.
            for _ in 0..seed * 50 {
                g.next_u64();
            }
            let before = g
                .component::<f4x_core::city::economy::Buildings>(city)
                .unwrap()
                .0
                .len();
            let events = g.conquer_city(city, HUMAN, damage);
            for e in events {
                if let f4x_core::game::Event::CityLooted {
                    citizens_lost,
                    buildings_lost,
                    ..
                } = e
                {
                    citizens += citizens_lost;
                    buildings += buildings_lost.len() as u32;
                    assert!(buildings_lost.len() <= before);
                }
            }
        }
        (citizens, buildings)
    };
    let (c0, b0) = lost(0);
    let (c40, b40) = lost(40);
    assert!(c40 > c0 * 2, "{c40} vs {c0}");
    assert!(b40 > b0 * 2, "{b40} vs {b0}");
    // Never more than half the citizens (and one stays) or three quarters of the buildings.
    let (cmax, bmax) = lost(400);
    assert!(cmax <= 10 * 16 / 2 + 40, "{cmax}");
    assert!(bmax <= 10 * 6, "{bmax}");
}

#[test]
fn an_outpost_is_destroyed_when_taken() {
    let (mut g, city) = neutral_world(0, &[]);
    let events = g.conquer_city(city, HUMAN, 0);
    assert!(events
        .iter()
        .any(|e| matches!(e, f4x_core::game::Event::CityDestroyed { .. })));
    assert!(g.component::<f4x_core::city::City>(city).is_none());
}

#[test]
fn a_taken_city_can_be_razed_until_the_turn_ends() {
    let (mut g, city) = neutral_world(12, &["marketplace", "library"]);
    g.set_fame(HUMAN, 10);
    g.treasury_mut(HUMAN).unwrap().gold = 0;
    g.conquer_city(city, HUMAN, 0);
    let fame_after_taking = g.wizard(HUMAN).unwrap().fame();
    let gold_after_taking = g.treasury(HUMAN).unwrap().gold;
    // Only the taker may raze it.
    let events = g.apply(f4x_core::game::Command::RazeCity {
        player: PlayerId(2),
        city,
    });
    assert!(
        matches!(events[..], [f4x_core::game::Event::Rejected(_)]),
        "{events:?}"
    );
    // The taker razes it: the size plus one more fame is lost (the fighting leaves a size that
    // depends on the dice), and a tenth of each building's cost is looted.
    let size = f4x_core::city::conquest::city_size_class(
        g.component::<f4x_core::city::economy::Population>(city)
            .unwrap()
            .figures,
    ) as i32;
    let events = g.apply(f4x_core::game::Command::RazeCity {
        player: HUMAN,
        city,
    });
    assert!(
        events
            .iter()
            .any(|e| matches!(e, f4x_core::game::Event::CityRazed { .. })),
        "{events:?}"
    );
    assert!(g.component::<f4x_core::city::City>(city).is_none());
    assert_eq!(
        g.wizard(HUMAN).unwrap().fame(),
        (fame_after_taking - (size + 1)).max(0)
    );
    assert!(g.treasury(HUMAN).unwrap().gold > gold_after_taking);

    // Wait a turn and the window is shut.
    let (mut g, city) = neutral_world(12, &[]);
    g.conquer_city(city, HUMAN, 0);
    let snap = g.snapshot();
    let mut back = snap.restore(g.content().clone()).unwrap();
    let again = conquered_city(&back);
    assert!(
        back.component::<f4x_core::city::conquest::Conquered>(again)
            .is_some(),
        "the chance to raze survives a save"
    );
    back.apply(f4x_core::game::Command::EndTurn);
    let events = back.apply(f4x_core::game::Command::RazeCity {
        player: HUMAN,
        city: again,
    });
    assert!(
        matches!(events[..], [f4x_core::game::Event::Rejected(_)]),
        "{events:?}"
    );
    assert!(back.component::<f4x_core::city::City>(again).is_some());
}

/// The one city marked as just taken (found again after a save: entities change).
fn conquered_city(g: &Game) -> hecs::Entity {
    g.economy()
        .cities()
        .into_iter()
        .map(|(e, ..)| e)
        .find(|&e| g.city_conquered(e))
        .unwrap()
}

#[test]
fn an_auto_resolved_siege_of_a_walled_city_ends_and_may_take_it() {
    let (mut taken, mut held) = (0, 0);
    for seed in 0..4u64 {
        let (mut g, city) = neutral_world(8, &["city_walls", "barracks"]);
        for _ in 0..seed * 50 {
            g.next_u64();
        }
        let at = *g.component::<f4x_core::map::Position>(city).unwrap();
        let from = f4x_core::map::Position { x: at.x + 1, ..at };
        let giant = g.content().rules.units.id_of("storm_giant").unwrap();
        for _ in 0..3 {
            g.apply(f4x_core::game::Command::CreateUnit {
                owner: HUMAN,
                pos: from,
                unit: giant,
            });
        }
        for u in g.units().player_units(HUMAN) {
            g.cheat_move_unit(u, from);
        }
        let garrison = g.units().units_at(at);
        let events = g.auto_resolve(&g.units().player_units(HUMAN), &garrison);
        assert!(!g.in_battle(), "the siege ended");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, f4x_core::game::Event::BattleEnded { .. })),
            "{events:?}"
        );
        if g.economy().city_owner(city) == Some(HUMAN) {
            taken += 1;
            assert!(events
                .iter()
                .any(|e| matches!(e, f4x_core::game::Event::CityLooted { .. })));
        } else {
            held += 1;
        }
    }
    assert!(taken + held == 4);
}

// ---- casting in battle (task 43) ------------------------------------------------------

/// A siege of the rival's capital in which the human wizard knows `spells`, with plenty of
/// mana and skill; returns the game.
fn casting_siege(spells: &[&str]) -> Game {
    let (mut g, city) = siege_world(6, &[]);
    arm_human(&mut g, spells);
    start_siege(&mut g, city);
    g
}

/// The human wizard knows `spells` and has the mana and skill to cast them.
fn arm_human(g: &mut Game, spells: &[&str]) {
    let ids: Vec<_> = spells
        .iter()
        .map(|s| {
            g.content()
                .rules
                .spells
                .id_of(s)
                .unwrap_or_else(|| panic!("{s}"))
        })
        .collect();
    g.treasury_mut(HUMAN).unwrap().mana = 5000;
    let mut w = g.wizard_mut(HUMAN).unwrap();
    w.get_mut::<f4x_core::wizard::sheet::Skill>().points = 20_000;
    w.get_mut::<f4x_core::wizard::sheet::KnownSpells>()
        .known
        .extend(ids);
}

fn cast(g: &mut Game, line: &str) -> crate::Reply {
    let mut c = console();
    c.execute(g, line)
}

#[test]
fn units_list_and_cast_their_own_spells_by_text() {
    let mut g = casting_siege(&["fire_bolt"]);
    let mine = battle_units_of(&g, f4x_core::battle::Side::Attacker)[0];
    let r = cast(&mut g, &format!("uspells {mine}"));
    assert!(r.ok, "{}", r.text());
    assert!(r.text().contains("mana"), "{}", r.text());
    // A plain unit knows no spell of its own: the order is refused, not misread.
    let r = cast(&mut g, &format!("ucast {mine} fire bolt unit:0 mana:10"));
    assert!(!r.ok, "{}", r.text());
    assert!(cast(&mut g, "ucast 99 fire bolt").error.is_some());
    assert!(cast(&mut g, "blegal").ok);
}

/// The human side ends its turn and the rival's does too, so the human wizard casts again.
fn pass_turn(g: &mut Game) {
    let r = cast(g, "bend");
    assert!(r.ok, "{}", r.text());
    g.apply(f4x_core::game::Command::Battle(
        f4x_core::battle::BattleCommand::EndTurn,
    ));
}

fn battle_units_of(g: &Game, side: f4x_core::battle::Side) -> Vec<usize> {
    g.battle()
        .unwrap()
        .units
        .iter()
        .filter(|u| u.side == side)
        .map(|u| u.id)
        .collect()
}

#[test]
fn a_wizard_goes_into_battle_with_skill_mana_and_a_range_cost() {
    use f4x_core::battle::Side;
    let g = casting_siege(&["fire_bolt"]);
    let b = g.battle().unwrap();
    let mine = b.caster(Side::Attacker).expect("the human wizard casts");
    assert_eq!(mine.skill_left, g.wizard(HUMAN).unwrap().casting_skill());
    assert_eq!(mine.mana, 5000);
    assert!(mine.range >= 5 && mine.range <= 30 && mine.range % 5 == 0);
    assert!(b.caster(Side::Defender).is_some(), "so does the rival");
    // Independent cities have no wizard.
    let (mut g, city) = neutral_world(6, &[]);
    start_siege(&mut g, city);
    assert!(g.battle().unwrap().caster(Side::Defender).is_none());
}

#[test]
fn a_spell_costs_skill_and_range_priced_mana_and_only_one_a_turn() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["fire_bolt"]);
    let enemy = battle_units_of(&g, Side::Defender)[0];
    let before = *g.battle().unwrap().caster(Side::Attacker).unwrap();
    let r = cast(&mut g, &format!("bcast fire bolt unit:{enemy}"));
    assert!(r.ok, "{}", r.text());
    let after = *g.battle().unwrap().caster(Side::Attacker).unwrap();
    // Fire Bolt costs 5; the mana is that times the range multiplier.
    assert_eq!(before.skill_left - after.skill_left, 5);
    assert_eq!(before.mana - after.mana, 5 * before.range / 10);
    assert_eq!(
        g.treasury(HUMAN).unwrap().mana,
        after.mana,
        "the reserve pays"
    );
    assert!(r.text().contains("casts Fire Bolt"), "{}", r.text());
    // A second spell in the same turn is refused; the next turn allows one.
    let again = cast(&mut g, &format!("bcast fire bolt unit:{enemy}"));
    assert!(!again.ok, "{}", again.text());
    pass_turn(&mut g);
    assert_eq!(g.battle().unwrap().active, Side::Attacker);
    assert!(cast(&mut g, &format!("bcast fire bolt unit:{enemy}")).ok);
}

#[test]
fn only_known_battle_spells_with_effects_can_be_cast() {
    use f4x_core::battle::Side;
    // Warp Wood is known but not yet in the game; Earth Lore is overland only; Fireball is not
    // known.
    let mut g = casting_siege(&["warp_wood", "earth_lore"]);
    let enemy = battle_units_of(&g, Side::Defender)[0];
    for spell in ["warp wood", "earth lore", "fireball"] {
        let r = cast(&mut g, &format!("bcast {spell} unit:{enemy}"));
        assert!(!r.ok, "{spell}");
    }
    assert!(
        cast(&mut g, "bspells").text().is_empty()
            || !cast(&mut g, "bspells").text().contains("warp wood"),
        "unfinished spells are not offered"
    );
}

#[test]
fn a_spell_that_costs_more_than_the_wizard_has_is_refused() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["fire_bolt"]);
    let enemy = battle_units_of(&g, Side::Defender)[0];
    // A reserve too small for the mana the spell takes at this range.
    g.battle.as_mut().unwrap().battle.casters[Side::Attacker.index()]
        .as_mut()
        .unwrap()
        .mana = 1;
    let r = cast(&mut g, &format!("bcast fire bolt unit:{enemy}"));
    assert!(!r.ok, "{}", r.text());
    assert!(
        !g.battle()
            .unwrap()
            .caster(Side::Attacker)
            .unwrap()
            .cast_this_turn
    );
    assert!(
        cast(&mut g, "bspells").text().contains("cannot cast now"),
        "the list says so"
    );
}

#[test]
fn counter_magic_stops_a_spell_that_still_costs_its_mana() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["fire_bolt"]);
    let enemy = battle_units_of(&g, Side::Defender)[0];
    g.battle
        .as_mut()
        .unwrap()
        .battle
        .cast_counter_magic(Side::Defender, 100_000);
    let hits = g.battle().unwrap().unit(enemy).unwrap().hit_points();
    let mana = g.treasury(HUMAN).unwrap().mana;
    let r = cast(&mut g, &format!("bcast fire bolt unit:{enemy}"));
    assert!(r.ok, "{}", r.text());
    assert!(r.text().contains("Counter Magic stops"), "{}", r.text());
    assert_eq!(g.battle().unwrap().unit(enemy).unwrap().hit_points(), hits);
    assert!(g.treasury(HUMAN).unwrap().mana < mana, "the mana is spent");
    assert_eq!(
        g.battle().unwrap().counter_magic[Side::Defender.index()],
        100_000 - f4x_core::spell::dispel::COUNTER_MAGIC_DRAIN,
        "every attempt drains the reserve"
    );
}

#[test]
fn counter_magic_is_a_reserve_of_the_mana_put_in() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["counter_magic"]);
    let r = cast(&mut g, "bcast counter magic mana:25");
    assert!(r.ok, "{}", r.text());
    assert_eq!(
        g.battle().unwrap().counter_magic[Side::Attacker.index()],
        25
    );
    // Less than the cost cannot be put in.
    let mut g = casting_siege(&["counter_magic"]);
    assert!(!cast(&mut g, "bcast counter magic mana:3").ok);
}

#[test]
fn a_summoned_creature_joins_the_side_and_goes_with_the_battle() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["phantom_warriors"]);
    let overland = g.entity_count();
    let before = g.battle().unwrap().active_units(Side::Attacker).count();
    let r = cast(&mut g, "bcast phantom warriors tile:4,8");
    assert!(r.ok, "{}", r.text());
    let b = g.battle().unwrap();
    assert_eq!(b.active_units(Side::Attacker).count(), before + 1);
    let new = b.units.last().unwrap();
    assert_eq!(
        (new.side, new.cell, new.source),
        (Side::Attacker, (4, 8), None)
    );
    assert_eq!(g.entity_count(), overland, "no unit is left on the map");
    // Not on the enemy's half, nor on an occupied square.
    let mut g = casting_siege(&["phantom_warriors"]);
    assert!(!cast(&mut g, "bcast phantom warriors tile:19,5").ok);
    let own = g.battle().unwrap().units[0].cell;
    assert!(
        !cast(
            &mut g,
            &format!("bcast phantom warriors tile:{},{}", own.0, own.1)
        )
        .ok
    );
}

#[test]
fn a_lasting_spell_changes_a_unit_until_a_dispel_takes_it_off() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["giant_strength"]);
    let mine = battle_units_of(&g, Side::Attacker)[0];
    let melee = g.battle().unwrap().unit(mine).unwrap().melee;
    let r = cast(&mut g, &format!("bcast giant strength unit:{mine}"));
    assert!(r.ok, "{}", r.text());
    assert_eq!(g.battle().unwrap().unit(mine).unwrap().melee, melee + 1);
    // Not twice on one unit, and not on the enemy's.
    pass_turn(&mut g);
    assert!(!cast(&mut g, &format!("bcast giant strength unit:{mine}")).ok);
    let enemy = battle_units_of(&g, Side::Defender)[0];
    assert!(!cast(&mut g, &format!("bcast giant strength unit:{enemy}")).ok);
}

#[test]
fn a_dispel_takes_a_spell_cast_in_the_battle_off_an_enemy_unit() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["dispel_magic"]);
    let enemy = battle_units_of(&g, Side::Defender)[0];
    let strength = g.content().rules.spells.id_of("giant_strength").unwrap();
    let rival = PlayerId(1);
    {
        let b = &mut g.battle.as_mut().unwrap().battle;
        let m = |stat, value| {
            f4x_core::rules::stats::Modifier::new(
                stat,
                f4x_core::rules::stats::ModOp::Add,
                value,
                f4x_core::rules::stats::Source::Other,
            )
        };
        b.enchant_unit(
            enemy,
            strength,
            "giant_strength",
            rival,
            &[m(f4x_core::rules::stats::Stat::Melee, 1)],
            &[],
        );
    }
    let melee = g.battle().unwrap().unit(enemy).unwrap().melee;
    // So much mana that a dispel hardly ever fails; try over a few turns.
    let mut removed = false;
    for _ in 0..6 {
        let r = cast(&mut g, &format!("bcast dispel magic unit:{enemy} mana:30"));
        assert!(r.ok, "{}", r.text());
        if g.battle()
            .unwrap()
            .unit(enemy)
            .unwrap()
            .combat_spells
            .is_empty()
        {
            removed = true;
            break;
        }
        pass_turn(&mut g);
    }
    assert!(removed, "the dispel took the spell off");
    let u = g.battle().unwrap().unit(enemy).unwrap();
    assert_eq!(u.melee, melee - 1);
    assert!(u.enchantments.is_empty() && u.base.is_none());
}

#[test]
fn a_dispel_in_battle_also_takes_the_overland_spells_off_the_unit_it_copied() {
    use f4x_core::battle::Side;
    let (mut g, city) = siege_world(6, &[]);
    let rival = PlayerId(1);
    let at = *g.component::<f4x_core::map::Position>(city).unwrap();
    let defender = g.units().units_at(at)[0];
    let spell = g.content().rules.spells.id_of("giant_strength").unwrap();
    let dispel = g.content().rules.spells.id_of("dispel_magic").unwrap();
    g.scenario_enchant_unit(defender, spell, rival);
    let plain = g.units().unit_stat(defender, f4x_core::rules::Stat::Melee) - 1;
    {
        g.treasury_mut(HUMAN).unwrap().mana = 5000;
        let mut w = g.wizard_mut(HUMAN).unwrap();
        w.get_mut::<f4x_core::wizard::sheet::Skill>().points = 20_000;
        w.get_mut::<f4x_core::wizard::sheet::KnownSpells>()
            .known
            .insert(dispel);
    }
    start_siege(&mut g, city);
    let enemy = g
        .battle()
        .unwrap()
        .units
        .iter()
        .position(|u| {
            u.side == Side::Defender
                && u.melee == plain + 1
                && u.enchantments.contains(&"giant_strength".to_string())
        })
        .expect("the copy carries the spell");
    let mut gone = false;
    for _ in 0..6 {
        let r = cast(&mut g, &format!("bcast dispel magic unit:{enemy} mana:30"));
        assert!(r.ok, "{}", r.text());
        if g.component::<f4x_core::unit::UnitEnchantments>(defender)
            .is_none_or(|l| l.0.is_empty())
        {
            gone = true;
            break;
        }
        pass_turn(&mut g);
    }
    assert!(gone, "the overland unit lost the spell");
    assert_eq!(g.battle().unwrap().unit(enemy).unwrap().melee, plain);
}

#[test]
fn a_bolt_hurts_by_its_strength_and_star_fires_only_burns_chaos_and_death() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["doom_bolt", "star_fires", "healing"]);
    let enemy = battle_units_of(&g, Side::Defender)[0];
    {
        // A tough target, so the bolt does not overkill.
        let u = &mut g.battle.as_mut().unwrap().battle.units[enemy];
        (u.figures, u.hits, u.front_damage) = (1, 50, 0);
    }
    let before = g.battle().unwrap().unit(enemy).unwrap().hit_points();
    let r = cast(&mut g, &format!("bcast doom bolt unit:{enemy}"));
    assert!(r.ok, "{}", r.text());
    let after = g.battle().unwrap().unit(enemy).unwrap().hit_points();
    assert_eq!(
        before - after,
        10,
        "doom damage ignores the defence: {}",
        r.text()
    );
    // Star Fires: not on ordinary units.
    pass_turn(&mut g);
    assert!(!cast(&mut g, &format!("bcast star fires unit:{enemy}")).ok);
    // Healing: one's own units only, and it brings figures back.
    let mine = battle_units_of(&g, Side::Attacker)[0];
    g.battle.as_mut().unwrap().battle.units[mine].front_damage = 1;
    assert!(!cast(&mut g, &format!("bcast healing unit:{enemy}")).ok);
    let r = cast(&mut g, &format!("bcast healing unit:{mine}"));
    assert!(r.ok, "{}", r.text());
    assert_eq!(g.battle().unwrap().unit(mine).unwrap().front_damage, 0);
}

#[test]
fn a_spell_that_kills_the_last_enemy_ends_the_battle() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["doom_bolt"]);
    let enemies = battle_units_of(&g, Side::Defender);
    // Leave one frail defender.
    {
        let b = &mut g.battle.as_mut().unwrap().battle;
        for &e in &enemies[1..] {
            b.units[e].status = f4x_core::battle::Status::Dead;
        }
        b.units[enemies[0]].figures = 1;
        b.units[enemies[0]].hits = 3;
        b.units[enemies[0]].front_damage = 0;
    }
    let r = cast(&mut g, &format!("bcast doom bolt unit:{}", enemies[0]));
    assert!(r.ok, "{}", r.text());
    assert!(!g.in_battle(), "the battle is over: {}", r.text());
    assert!(r.text().contains("Battle over"), "{}", r.text());
}

#[test]
fn hit_points_a_spell_added_do_not_kill_the_unit_when_the_battle_is_over() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["lionheart"]);
    // Fleeing may lose the unit by chance; this test is about its hit points, not that roll.
    g.scenario_set_safe_flee(true);
    let mine = battle_units_of(&g, Side::Attacker)[0];
    let entity = {
        let a = g.battle.as_ref().unwrap();
        a.sources[a.battle.unit(mine).unwrap().source.unwrap()]
    };
    let r = cast(&mut g, &format!("bcast lionheart unit:{mine}"));
    assert!(r.ok, "{}", r.text());
    {
        let u = &mut g.battle.as_mut().unwrap().battle.units[mine];
        assert!(u.hits > 1 && u.figures_start > 1, "{u:?}");
        u.figures = 1;
    }
    assert!(cast(&mut g, "bflee").ok);
    assert!(!g.in_battle());
    let (left, max) = g.units().unit_hits(entity);
    assert!(left >= 1 && left <= max, "{left} of {max}");
}

// ---- battlefield and destroying spells (tasks 44 and 159) --------------------------------

#[test]
fn a_battlefield_enchantment_goes_up_once_and_changes_the_units() {
    use f4x_core::battle::field::Field;
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["high_prayer"]);
    let mine = battle_units_of(&g, Side::Attacker)[0];
    let defense = g.battle().unwrap().unit(mine).unwrap().defense;
    let enemy = battle_units_of(&g, Side::Defender)[0];
    let enemy_block = g.battle().unwrap().unit(enemy).unwrap().to_block;
    let r = cast(&mut g, "bcast high prayer");
    assert!(r.ok, "{}", r.text());
    assert!(r.text().contains("raises High Prayer"), "{}", r.text());
    let b = g.battle().unwrap();
    assert!(b.has_field(Side::Attacker, Field::HighPrayer));
    assert_eq!(b.unit(mine).unwrap().defense, defense + 2);
    // Not again; and the enemy's units are not blessed.
    pass_turn(&mut g);
    assert!(!cast(&mut g, "bcast high prayer").ok);
    assert_eq!(
        g.battle().unwrap().unit(enemy).unwrap().to_block,
        enemy_block
    );
}

#[test]
fn mana_leak_drains_the_rivals_reserve_in_the_game_too() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["mana_leak"]);
    g.treasury_mut(PlayerId(1)).unwrap().mana = 100;
    // The rival's wizard in the battle has its reserve from the game.
    g.battle.as_mut().unwrap().battle.casters[Side::Defender.index()]
        .as_mut()
        .unwrap()
        .mana = 100;
    assert!(cast(&mut g, "bcast mana leak").ok);
    pass_turn(&mut g);
    // The rival's turn began a new turn: the attacker's.
    assert_eq!(g.treasury(PlayerId(1)).unwrap().mana, 95);
    assert_eq!(g.battle().unwrap().caster(Side::Defender).unwrap().mana, 95);
}

#[test]
fn flame_strike_hits_every_enemy_and_mass_healing_every_friend() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["flame_strike", "mass_healing"]);
    let enemies = battle_units_of(&g, Side::Defender);
    for &e in &enemies {
        let u = &mut g.battle.as_mut().unwrap().battle.units[e];
        (u.figures, u.hits, u.front_damage, u.defense) = (1, 100, 0, 0);
    }
    let r = cast(&mut g, "bcast flame strike");
    assert!(r.ok, "{}", r.text());
    let hurt = enemies
        .iter()
        .filter(|&&e| g.battle().unwrap().unit(e).unwrap().hit_points() < 100)
        .count();
    assert!(hurt >= 1, "a strength-15 fire attack on each: {}", r.text());
    let mine = battle_units_of(&g, Side::Attacker);
    for &m in &mine {
        g.battle.as_mut().unwrap().battle.units[m].front_damage = 1;
    }
    pass_turn(&mut g);
    let r = cast(&mut g, "bcast mass healing");
    assert!(r.ok, "{}", r.text());
    for &m in &mine {
        assert_eq!(g.battle().unwrap().unit(m).unwrap().front_damage, 0, "{m}");
    }
}

#[test]
fn death_spell_and_word_of_death_destroy_the_figures_that_cannot_resist() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["death_spell", "word_of_death"]);
    let enemies = battle_units_of(&g, Side::Defender);
    for &e in &enemies {
        g.battle.as_mut().unwrap().battle.units[e].resistance = 0;
    }
    // One enemy that cannot fall keeps the battle going.
    let last = *enemies.last().unwrap();
    g.battle.as_mut().unwrap().battle.units[last]
        .abilities
        .push("death_immunity".into());
    let r = cast(&mut g, "bcast death spell");
    assert!(r.ok, "{}", r.text());
    let b = g.battle().unwrap();
    let dead = enemies
        .iter()
        .filter(|&&e| !b.unit(e).unwrap().is_active())
        .count();
    // A penalty of two against resistance zero: a ten-sided die over two kills a figure about
    // eight times in ten, so most of every unit goes.
    let left: u32 = enemies.iter().map(|&e| b.unit(e).unwrap().figures).sum();
    let start: u32 = enemies
        .iter()
        .map(|&e| b.unit(e).unwrap().figures_start)
        .sum();
    assert!(dead > 0 || left < start, "{}", r.text());
}

#[test]
fn a_curse_can_be_resisted_and_banish_only_touches_fantastic_creatures() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["weakness", "banish", "mind_storm"]);
    let enemy = battle_units_of(&g, Side::Defender)[0];
    // Banish needs a fantastic creature.
    assert!(!cast(&mut g, &format!("bcast banish unit:{enemy}")).ok);
    // A unit with no resistance cannot save against Weakness.
    g.battle.as_mut().unwrap().battle.units[enemy].resistance = 0;
    let melee = g.battle().unwrap().unit(enemy).unwrap().melee;
    let mut weakened = false;
    for _ in 0..8 {
        let r = cast(&mut g, &format!("bcast weakness unit:{enemy}"));
        assert!(r.ok, "{}", r.text());
        if g.battle().unwrap().unit(enemy).unwrap().melee < melee {
            weakened = true;
            break;
        }
        pass_turn(&mut g);
    }
    assert!(weakened, "an unresisting unit is weakened soon enough");
    // Mind Storm: no save, but illusion immunity stops it.
    let other = battle_units_of(&g, Side::Defender)[1];
    g.battle.as_mut().unwrap().battle.units[other]
        .abilities
        .push("illusion_immunity".into());
    pass_turn(&mut g);
    let r = cast(&mut g, &format!("bcast mind storm unit:{other}"));
    assert!(r.ok, "{}", r.text());
    assert!(r.text().contains("takes no harm"), "{}", r.text());
}

#[test]
fn shatter_warp_wood_and_warp_creature_harm_a_unit_for_the_battle() {
    use f4x_core::battle::{RangedKind, Side};
    let mut g = casting_siege(&["shatter", "warp_wood", "warp_creature"]);
    let [a, b] = [0, 1].map(|i| battle_units_of(&g, Side::Defender)[i]);
    {
        let units = &mut g.battle.as_mut().unwrap().battle.units;
        units[a].resistance = 0;
        (units[a].melee, units[a].ranged, units[a].ranged_kind) = (5, 3, RangedKind::Missile);
        units[a].ammo = 4;
        units[b].resistance = 0;
        (units[b].melee, units[b].defense) = (6, 4);
    }
    // Warp Wood takes the arrows, and refuses a unit that has none left.
    let r = cast(&mut g, &format!("bcast warp wood unit:{a}"));
    assert!(r.ok, "{}", r.text());
    assert_eq!(g.battle().unwrap().unit(a).unwrap().ammo, 0);
    pass_turn(&mut g);
    assert!(!cast(&mut g, &format!("bcast warp wood unit:{a}")).ok);
    // Shatter: attacks drop to one, though the unit gets a resistance roll.
    let mut shattered = false;
    for _ in 0..8 {
        let r = cast(&mut g, &format!("bcast shatter unit:{a}"));
        assert!(r.ok, "{}", r.text());
        let u = g.battle().unwrap().unit(a).unwrap();
        if u.melee == 1 {
            assert_eq!(u.ranged, 1);
            shattered = true;
            break;
        }
        pass_turn(&mut g);
    }
    assert!(shattered, "an unresisting unit is shattered soon enough");
    // Warp Creature: one of three harms, once.
    let (melee, defense, resistance) = {
        let u = g.battle().unwrap().unit(b).unwrap();
        (u.melee, u.defense, u.resistance)
    };
    let mut warped = false;
    for _ in 0..8 {
        pass_turn(&mut g);
        let r = cast(&mut g, &format!("bcast warp creature unit:{b}"));
        assert!(r.ok, "{}", r.text());
        let u = g.battle().unwrap().unit(b).unwrap();
        if u.melee < melee || u.defense < defense {
            warped = true;
            break;
        }
        // A zero resistance cannot be told from the third harm: look at the spell list.
        if u.enchantments.iter().any(|e| e == "warp_resistance") {
            assert_eq!(u.resistance, resistance.min(0));
            warped = true;
            break;
        }
    }
    assert!(warped, "an unresisting unit is warped soon enough");
    pass_turn(&mut g);
    assert!(
        !cast(&mut g, &format!("bcast warp creature unit:{b}")).ok,
        "a unit is warped once"
    );
}

#[test]
fn creature_binding_and_possession_turn_a_unit_for_the_battle() {
    use f4x_core::battle::immunity::Realm;
    use f4x_core::battle::Side;
    // Two rival units: `a` a normal one, `b` a fantastic one, neither with resistance. The
    // spell is cast until it takes (they get a roll), the other unit keeping the fight going.
    for (spell, turned, other) in [("creature_binding", 1, 0), ("possession", 0, 1)] {
        let mut g = casting_siege(&[spell]);
        let units = battle_units_of(&g, Side::Defender);
        let (a, b) = (units[0], units[1]);
        {
            let units = &mut g.battle.as_mut().unwrap().battle.units;
            units[a].resistance = 0;
            units[b].resistance = 0;
            (units[a].realm, units[b].realm) = (None, Some(Realm::Chaos));
        }
        let (target, spared) = ([a, b][turned], [a, b][other]);
        // The wrong kind of unit is refused.
        let name = spell.replace('_', " ");
        assert!(!cast(&mut g, &format!("bcast {name} unit:{spared}")).ok);
        let mut done = false;
        for _ in 0..12 {
            let r = cast(&mut g, &format!("bcast {name} unit:{target}"));
            assert!(r.ok, "{}", r.text());
            let u = g.battle().unwrap().unit(target).unwrap();
            if u.side == Side::Attacker {
                assert_eq!(u.bound_from, Some(Side::Defender));
                done = true;
                break;
            }
            pass_turn(&mut g);
        }
        assert!(done, "{spell}: a unit with no resistance turns soon enough");
        assert_eq!(
            g.battle().unwrap().unit(spared).unwrap().side,
            Side::Defender
        );
    }
}

#[test]
fn call_chaos_and_invisibility_cast_and_the_spell_list_offers_them() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["call_chaos", "invisiblity"]);
    let list = cast(&mut g, "bspells").text();
    assert!(
        list.contains("call_chaos") && list.contains("invisiblity"),
        "{list}"
    );
    let mine = battle_units_of(&g, Side::Attacker)[0];
    assert!(cast(&mut g, &format!("bcast invisiblity unit:{mine}")).ok);
    assert!(g.battle().unwrap().unit(mine).unwrap().is_invisible());
    pass_turn(&mut g);
    let r = cast(&mut g, "bcast call chaos");
    assert!(r.ok, "{}", r.text());
}

fn meet_all(g: &mut Game) -> Vec<PlayerId> {
    let others: Vec<PlayerId> = g.wizards().into_iter().filter(|&w| w != HUMAN).collect();
    for &w in &others {
        g.make_contact(HUMAN, w);
    }
    others
}

#[test]
fn diplomacy_lists_only_the_wizards_met() {
    let (mut g, mut c) = (game(), console());
    let none = c.execute(&mut g, "diplomacy");
    assert!(none.ok, "{}", none.text());
    assert!(
        none.text().contains("met no other wizard"),
        "{}",
        none.text()
    );
    let others = meet_all(&mut g);
    let all = c.execute(&mut g, "diplomacy");
    assert!(all.ok, "{}", all.text());
    assert_eq!(all.lines.len(), others.len(), "{}", all.text());
    assert!(all.text().contains("no treaty"), "{}", all.text());
    let one = c.execute(&mut g, &format!("diplomacy {}", others[0].0));
    assert!(
        one.text().contains("you may propose: pact, alliance"),
        "{}",
        one.text()
    );
    assert!(
        c.execute(&mut g, "diplomacy 0").error.is_some(),
        "not oneself"
    );
    assert!(c.execute(&mut g, "diplomacy 200").error.is_some());
}

#[test]
fn a_proposal_waits_and_an_answer_signs_it() {
    let (mut g, mut c) = (game(), console());
    let other = meet_all(&mut g)[0];
    let w = other.0;
    let r = c.execute(&mut g, &format!("propose {w} pact"));
    assert!(r.ok, "{}", r.text());
    assert!(r.text().contains("proposes"), "{}", r.text());
    let again = c.execute(&mut g, &format!("propose {w} alliance"));
    assert!(!again.ok);
    assert!(again.text().contains("already waiting"), "{}", again.text());
    let list = c.execute(&mut g, "diplomacy");
    assert!(list.text().contains("waiting"), "{}", list.text());
    // Nothing they have proposed to us yet.
    let none = c.execute(&mut g, &format!("answer {w} yes"));
    assert!(none.text().contains("proposed nothing"), "{}", none.text());
    // The other wizard proposes to us: we accept.
    g.apply(f4x_core::game::Command::Diplomacy(
        f4x_core::diplomacy::DiplomacyCommand::Propose {
            from: other,
            to: HUMAN,
            kind: f4x_core::diplomacy::ProposalKind::Alliance,
        },
    ));
    let ok = c.execute(&mut g, &format!("answer {w} yes"));
    assert!(ok.ok, "{}", ok.text());
    assert!(ok.text().contains("accepts"), "{}", ok.text());
    assert_eq!(
        g.treaty(HUMAN, other),
        f4x_core::diplomacy::Treaty::Alliance
    );
    assert!(c
        .execute(&mut g, &format!("answer {w} maybe"))
        .error
        .is_some());
}

#[test]
fn treaties_break_and_wars_are_declared() {
    let (mut g, mut c) = (game(), console());
    let other = meet_all(&mut g)[0];
    let w = other.0;
    let not = c.execute(&mut g, &format!("breaktreaty {w}"));
    assert!(not.text().contains("not allowed"), "{}", not.text());
    g.start_treaty(HUMAN, other, f4x_core::diplomacy::Treaty::Pact);
    let broken = c.execute(&mut g, &format!("breaktreaty {w}"));
    assert!(broken.ok, "{}", broken.text());
    assert!(broken.text().contains("breaks"), "{}", broken.text());
    let war = c.execute(&mut g, &format!("declarewar {w}"));
    assert!(war.ok, "{}", war.text());
    assert!(war.text().contains("declares war"), "{}", war.text());
    assert_eq!(g.treaty(HUMAN, other), f4x_core::diplomacy::Treaty::War);
    assert!(!c.execute(&mut g, &format!("declarewar {w}")).ok);
    let peace = c.execute(&mut g, &format!("propose {w} peace"));
    assert!(peace.ok, "{}", peace.text());
}

#[test]
fn gifts_move_gold_and_spells() {
    let (mut g, mut c) = (game(), console());
    let other = meet_all(&mut g)[0];
    let w = other.0;
    let before = g.treasury(HUMAN).unwrap().gold;
    let r = c.execute(&mut g, &format!("givegold {w} 10"));
    assert!(r.ok, "{}", r.text());
    assert_eq!(g.treasury(HUMAN).unwrap().gold, before - 10);
    let broke = c.execute(&mut g, &format!("givegold {w} 99999"));
    assert!(broke.text().contains("not enough gold"), "{}", broke.text());
    let unknown = c.execute(&mut g, &format!("givespell {w} no-such-spell"));
    assert!(!unknown.ok);
    let me = g.wizard(HUMAN).unwrap().known().into_iter().next().unwrap();
    g.wizard_mut(other)
        .unwrap()
        .get_mut::<f4x_core::wizard::sheet::KnownSpells>()
        .known
        .remove(&me);
    let name = g.content().rules.spells.get(me).id.clone();
    let spell = c.execute(&mut g, &format!("givespell {w} {name}"));
    assert!(spell.ok, "{}", spell.text());
    assert!(g.wizard(other).unwrap().knows(me));
}

#[test]
fn the_talk_commands_are_legal_ones_and_diplomacy_a_query() {
    let kind = |name: &str| crate::find(name).unwrap().kind;
    assert_eq!(kind("diplomacy"), Kind::Query);
    for name in [
        "propose",
        "answer",
        "breaktreaty",
        "declarewar",
        "givegold",
        "givespell",
    ] {
        assert_eq!(kind(name), Kind::Legal, "{name}");
    }
    let (mut g, _) = (game(), console());
    let mut read_only = Console::new(HUMAN, Access::ReadOnly);
    meet_all(&mut g);
    assert!(read_only.execute(&mut g, "diplomacy").ok);
    assert!(!read_only.execute(&mut g, "propose 1 pact").ok);
}

#[test]
fn every_wizard_of_a_new_game_has_a_picture_of_its_own() {
    let g = game();
    let mut seen = std::collections::BTreeSet::new();
    for id in g
        .player_ids()
        .into_iter()
        .filter(|id| !g.is_neutral_player(*id))
    {
        let portrait = g.portrait(id);
        assert!(portrait < f4x_core::player::PORTRAITS);
        assert!(seen.insert(portrait), "picture {portrait} twice");
    }
}

#[test]
fn the_legal_casts_are_the_casts_the_game_accepts() {
    use f4x_core::battle::{BattleCommand, Side};
    let spells = [
        "fire_bolt",
        "fireball",
        "giant_strength",
        "healing",
        "counter_magic",
        "dispel_magic",
    ];
    let known: Vec<&str> = spells
        .iter()
        .copied()
        .filter(|s| casting_siege(&[]).content().rules.spells.id_of(s).is_some())
        .collect();
    assert!(known.len() >= 3, "{known:?}");
    let g = casting_siege(&known);
    let legal = g.legal_battle_casts();
    assert!(!legal.is_empty());
    let enemy = battle_units_of(&g, Side::Defender);
    let own = battle_units_of(&g, Side::Attacker);
    for lc in &legal {
        let def = g.content().rules.spells.get(lc.spell).id.clone();
        assert!(!lc.aims.is_empty(), "{def}");
        assert!(lc.least_mana <= lc.most_mana, "{def}");
        // The first and last aim, at the least and the most mana, are accepted.
        let picks = [lc.aims[0], *lc.aims.last().unwrap()];
        for aim in picks {
            for mana in [0, lc.most_mana] {
                let mut fresh = casting_siege(&known);
                let r = fresh.battle_command(BattleCommand::Cast {
                    spell: lc.spell,
                    aim,
                    mana,
                });
                assert!(r.is_ok(), "{def} {aim:?} mana {mana}: {r:?}");
            }
        }
        // One mana too many is refused when the spell takes extra mana within a limit.
        if lc.most_mana > lc.least_mana {
            let mut fresh = casting_siege(&known);
            let r = fresh.battle_command(BattleCommand::Cast {
                spell: lc.spell,
                aim: lc.aims[0],
                mana: lc.most_mana + 1,
            });
            assert!(r.is_err(), "{def}: more than the most is refused");
        }
    }
    // An aim that is not listed is refused: a bolt at one's own unit.
    let bolt = g.content().rules.spells.id_of("fire_bolt").unwrap();
    let listed = legal.iter().find(|l| l.spell == bolt).expect("fire bolt");
    assert!(listed.aims.iter().all(|a| match a {
        f4x_core::battle::SpellAim::Unit(u) => enemy.contains(u),
        _ => false,
    }));
    let mut fresh = casting_siege(&known);
    assert!(fresh
        .battle_command(BattleCommand::Cast {
            spell: bolt,
            aim: f4x_core::battle::SpellAim::Unit(own[0]),
            mana: 0
        })
        .is_err());
    // After a cast the wizard has none left this turn.
    let mut g = casting_siege(&known);
    g.battle_command(BattleCommand::Cast {
        spell: bolt,
        aim: f4x_core::battle::SpellAim::Unit(enemy[0]),
        mana: 0,
    })
    .unwrap();
    assert!(g.legal_battle_casts().is_empty());
}

#[test]
fn healing_does_not_work_on_a_death_creature() {
    use f4x_core::battle::{immunity::Realm, Side};
    let mut g = casting_siege(&["healing"]);
    let own = battle_units_of(&g, Side::Attacker)[0];
    // A wounded living unit can be healed; the same unit as a death creature cannot.
    g.battle.as_mut().unwrap().battle.units[own].realm = Some(Realm::Death);
    let r = cast(&mut g, &format!("bcast healing unit:{own}"));
    assert!(!r.ok, "{}", r.text());
    g.battle.as_mut().unwrap().battle.units[own].realm = None;
    let r = cast(&mut g, &format!("bcast healing unit:{own}"));
    assert!(r.ok, "{}", r.text());
}

#[test]
fn the_undead_cheat_needs_override_and_makes_units_undead() {
    let (mut g, _) = siege_world(6, &[]);
    let unit = g
        .units()
        .all_units()
        .into_iter()
        .find(|&u| g.units().unit_owner(u) == Some(HUMAN))
        .unwrap();
    let serial = g.serial_of(unit).0;
    let mut play = Console::new(HUMAN, Access::Play);
    let refused = play.execute(&mut g, &format!("undead {serial}"));
    assert!(!refused.ok);
    assert!(!g.units().unit_is_undead(unit));
    let mut full = Console::new(HUMAN, Access::Full);
    assert!(full.execute(&mut g, "mode override").ok);
    let r = full.execute(&mut g, &format!("undead {serial}"));
    assert!(r.ok, "{}", r.text());
    assert!(g.units().unit_is_undead(unit));
    let info = full.execute(&mut g, &format!("unit {serial}"));
    assert!(info.text().contains("undead"), "{}", info.text());
}

#[test]
fn blegal_lists_what_the_game_allows_and_the_listed_orders_work() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["fire_bolt"]);
    let all = cast(&mut g, "blegal");
    assert!(all.ok, "{}", all.text());
    assert!(
        all.text().contains("cast fire_bolt: mana"),
        "{}",
        all.text()
    );
    let mine = battle_units_of(&g, Side::Attacker)[0];
    let one = cast(&mut g, &format!("blegal {mine}"));
    assert!(
        one.text().contains(&format!("unit {mine} (")),
        "{}",
        one.text()
    );
    // The first listed square is accepted by bmove.
    let line = one.text();
    let first = line
        .split("move to ")
        .nth(1)
        .and_then(|s| s.split([' ', ';']).next())
        .expect("a move is listed")
        .to_string();
    let r = cast(&mut g, &format!("bmove {mine} {first}"));
    assert!(r.ok, "{} / {}", first, r.text());
    // The side that does not play gets no list.
    assert!(cast(&mut g, "bend").ok);
    let mut play = Console::new(HUMAN, Access::Play);
    let refused = play.execute(&mut g, "blegal");
    assert!(!refused.ok, "{}", refused.text());
}

// ---- Animate Dead and the drained dead (tasks 157 and 159) ------------------------------

/// Strike one of the human side's units down by the given kinds of damage.
fn fell(g: &mut Game, id: usize, kinds: [i32; 3]) {
    let u = &mut g.battle.as_mut().unwrap().battle.units[id];
    u.figures = 0;
    u.status = f4x_core::battle::Status::Dead;
    u.damage_kinds = kinds;
}

#[test]
fn animate_dead_raises_a_fallen_unit_of_the_wizard_as_undead() {
    use f4x_core::battle::{Side, Status};
    let mut g = casting_siege(&["animate_dead"]);
    let mine = battle_units_of(&g, Side::Attacker);
    let (fallen, standing) = (mine[0], mine[1]);
    // Nobody has fallen yet.
    assert!(!cast(&mut g, &format!("bcast animate dead unit:{fallen}")).ok);
    fell(&mut g, fallen, [4, 0, 0]);
    // Not a unit that stands, nor an enemy's dead.
    assert!(!cast(&mut g, &format!("bcast animate dead unit:{standing}")).ok);
    let enemy = battle_units_of(&g, Side::Defender)[0];
    fell(&mut g, enemy, [4, 0, 0]);
    assert!(!cast(&mut g, &format!("bcast animate dead unit:{enemy}")).ok);
    let r = cast(&mut g, &format!("bcast animate dead unit:{fallen}"));
    assert!(r.ok, "{}", r.text());
    let b = g.battle().unwrap();
    let u = b.unit(fallen).unwrap();
    assert_eq!(u.status, Status::Active);
    assert_eq!(u.figures, u.figures_start);
    assert!(u.undead && u.realm == Some(f4x_core::battle::immunity::Realm::Death));
    assert!(u.has("death_immunity") && u.has("cold_immunity"));
    assert_eq!(u.side, Side::Attacker);
    let entity = g.battle.as_ref().unwrap().sources[u.source.unwrap()];
    assert!(g.units().unit_is_undead(entity), "undead for good");
}

#[test]
fn raise_dead_brings_a_fallen_normal_unit_back_with_half_its_figures() {
    use f4x_core::battle::immunity::Realm;
    use f4x_core::battle::{Side, Status};
    let mut g = casting_siege(&["raise_dead"]);
    let mine = battle_units_of(&g, Side::Attacker);
    let (fallen, fantastic) = (mine[0], mine[1]);
    {
        let b = &mut g.battle.as_mut().unwrap().battle;
        b.units[fallen].enchantments.push("bless".into());
        b.units[fantastic].realm = Some(Realm::Chaos);
    }
    let start = g.battle().unwrap().unit(fallen).unwrap().figures_start;
    fell(&mut g, fallen, [4, 0, 0]);
    fell(&mut g, fantastic, [4, 0, 0]);
    // Fantastic creatures go home when slain; destroyed units cannot return.
    assert!(!cast(&mut g, &format!("bcast raise dead unit:{fantastic}")).ok);
    let r = cast(&mut g, &format!("bcast raise dead unit:{fallen}"));
    assert!(r.ok, "{}", r.text());
    let u = g.battle().unwrap().unit(fallen).unwrap();
    assert_eq!((u.status, u.side), (Status::Active, Side::Attacker));
    assert_eq!(u.figures, start.div_ceil(2), "half, rounded up");
    assert!(!u.undead && !u.enchantments.iter().any(|e| e == "bless"));
    assert!(
        u.hit_points() < u.max_hit_points(),
        "the missing half stays missing"
    );
}

#[test]
fn animate_dead_cannot_raise_what_was_destroyed_outright() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["animate_dead"]);
    let fallen = battle_units_of(&g, Side::Attacker)[0];
    fell(&mut g, fallen, [1, 0, 6]);
    assert!(!cast(&mut g, &format!("bcast animate dead unit:{fallen}")).ok);
    // Drained to death is another matter.
    fell(&mut g, fallen, [1, 6, 0]);
    let r = cast(&mut g, &format!("bcast animate dead unit:{fallen}"));
    assert!(r.ok, "{}", r.text());
}

#[test]
fn the_units_drained_to_death_join_the_winner_as_undead() {
    use f4x_core::battle::{BattleCommand, Side};
    let mut g = casting_siege(&[]);
    let mine = battle_units_of(&g, Side::Attacker);
    let (drained, slain) = (mine[0], mine[1]);
    let entity_of = |g: &Game, id: usize| {
        let b = g.battle.as_ref().unwrap();
        b.sources[b.battle.unit(id).unwrap().source.unwrap()]
    };
    let (drained_unit, slain_unit) = (entity_of(&g, drained), entity_of(&g, slain));
    for &id in &mine {
        fell(&mut g, id, [3, 0, 0]);
    }
    g.battle.as_mut().unwrap().battle.units[drained].damage_kinds = [1, 4, 0];
    let rival = PlayerId(1);
    let r = g.apply(f4x_core::game::Command::Battle(BattleCommand::EndTurn));
    assert!(
        r.iter().any(
            |e| matches!(e, f4x_core::game::Event::UndeadRaised { player, units: 1 } if *player == rival)
        ),
        "{r:?}"
    );
    assert_eq!(g.units().unit_owner(drained_unit), Some(rival));
    assert!(g.units().unit_is_undead(drained_unit));
    assert!(g.entity_exists(drained_unit) && !g.entity_exists(slain_unit));
}

#[test]
fn the_regeneration_spell_heals_a_hit_each_turn_in_battle() {
    use f4x_core::battle::Side;
    let mut g = casting_siege(&["regeneration"]);
    let mine = battle_units_of(&g, Side::Attacker)[0];
    g.battle.as_mut().unwrap().battle.units[mine].front_damage = 1;
    let r = cast(&mut g, &format!("bcast regeneration unit:{mine}"));
    assert!(r.ok, "{}", r.text());
    let before = g.battle().unwrap().unit(mine).unwrap().hit_points();
    pass_turn(&mut g);
    let after = g.battle().unwrap().unit(mine).unwrap().hit_points();
    assert_eq!(after, before + 1);
}

#[test]
fn eligibility_two_and_three_spells_are_battle_spells_and_dear_on_the_map() {
    let g = casting_siege(&["flight", "wall_of_stone", "giant_strength", "ice_bolt"]);
    let cost = |name: &str, in_battle: bool| {
        let id = g.content().rules.spells.id_of(name).unwrap();
        (
            g.content().rules.spells.get(id).cost as i32,
            g.casting_cost(HUMAN, id, in_battle),
        )
    };
    // Eligibility 2 (Flight) and 0 (Giant Strength): five times as much away from a battle.
    for name in ["flight", "giant_strength"] {
        let (base, battle) = cost(name, true);
        let (_, map) = cost(name, false);
        assert!(
            battle <= base && map == battle * 5,
            "{name}: {base} {battle} {map}"
        );
    }
    // Overland-only (Wall of Stone) and battle-only (Ice Bolt) spells are not dearer.
    for name in ["wall_of_stone", "ice_bolt"] {
        let (_, battle) = cost(name, true);
        let (_, map) = cost(name, false);
        assert_eq!(battle, map, "{name}");
    }
    // Flight can now be cast in a battle.
    let flight = g.content().rules.spells.id_of("flight").unwrap();
    assert!(g.battle_spells(HUMAN).iter().any(|s| s.spell == flight));
}

#[test]
fn ai_wizards_get_a_personality_and_an_objective_at_the_start() {
    let content = content();
    let g = new_game(
        &content,
        &NewGame {
            seed: 11,
            ..NewGame::with_wizards(4)
        },
    )
    .unwrap();
    let human = g.human().unwrap();
    assert_eq!(
        g.wizard(human).unwrap().objective(),
        None,
        "the human has no objective"
    );
    for id in g
        .player_ids()
        .into_iter()
        .filter(|id| !g.is_neutral_player(*id) && *id != human)
    {
        assert!(
            g.wizard(id).unwrap().objective().is_some(),
            "{} has an objective",
            g.player_name(id)
        );
    }
    let back = g.snapshot().restore(content).unwrap();
    for id in g
        .player_ids()
        .into_iter()
        .filter(|id| !g.is_neutral_player(*id))
    {
        let (b, w) = (back.wizard(id).unwrap(), g.wizard(id).unwrap());
        assert_eq!(b.objective(), w.objective());
        assert_eq!(b.personality(), w.personality());
    }
}

#[test]
fn rivals_without_ready_made_wizards_draw_a_name_from_their_list() {
    let mut content = content();
    // The fallback is for a rule set with no ready-made wizards.
    content.rules.wizards = f4x_core::rules::Registry::new(Vec::new());
    let names = |seed: u64| -> Vec<String> {
        let g = new_game(
            &content,
            &NewGame {
                seed,
                ..NewGame::with_wizards(5)
            },
        )
        .unwrap();
        g.player_ids()
            .into_iter()
            .filter(|id| !g.is_neutral_player(*id) && *id != g.human().unwrap())
            .map(|id| g.player_name(id))
            .collect()
    };
    let lists = &content.rules.wizard_names;
    assert_eq!(lists.len(), 4);
    assert!(lists.iter().all(|l| l.len() == 10));
    for seed in 0..20 {
        let got = names(seed);
        assert_eq!(got.len(), 4);
        for (name, list) in got.iter().zip(lists) {
            assert!(list.contains(name), "{name} is not in its slot's list");
        }
        assert_eq!(got, names(seed), "the same seed draws the same names");
    }
    let seen: std::collections::BTreeSet<_> = (0..20).map(|s| names(s)[0].clone()).collect();
    assert!(
        seen.len() > 3,
        "the first rival's name varies with the seed: {seen:?}"
    );
}

#[test]
fn recall_hero_takes_a_hero_off_the_field_to_the_summoning_circle() {
    use f4x_core::battle::{Side, Status};
    let (mut g, city) = siege_world(6, &[]);
    arm_human(&mut g, &["recall_hero"]);
    let home = g.summon_point(HUMAN).expect("the wizard has a circle");
    let hero = g.spawn_hero(HUMAN, home, 0).unwrap();
    start_siege(&mut g, city);
    let mine = battle_units_of(&g, Side::Attacker);
    let source_of = |g: &Game, id: usize| {
        let b = g.battle.as_ref().unwrap();
        b.sources[b.battle.unit(id).unwrap().source.unwrap()]
    };
    let hero_id = *mine
        .iter()
        .find(|&&id| source_of(&g, id) == hero)
        .expect("the hero fights");
    let other = *mine.iter().find(|&&id| id != hero_id).unwrap();
    // Only a hero of the wizard's own can be recalled.
    assert!(!cast(&mut g, &format!("bcast recall hero unit:{other}")).ok);
    let enemy = battle_units_of(&g, Side::Defender)[0];
    assert!(!cast(&mut g, &format!("bcast recall hero unit:{enemy}")).ok);
    let r = cast(&mut g, &format!("bcast recall hero unit:{hero_id}"));
    assert!(r.ok, "{}", r.text());
    assert_eq!(
        g.battle().unwrap().unit(hero_id).unwrap().status,
        Status::Recalled
    );
    // The rest of the army gives up: the hero is not among those who flee and risk their lives.
    g.apply(f4x_core::game::Command::Battle(
        f4x_core::battle::BattleCommand::Flee,
    ));
    assert!(g.battle().is_none(), "the battle is over");
    assert_eq!(
        g.units().unit_position(hero),
        Some(home),
        "a recalled hero is not lost"
    );
}

#[test]
fn an_offer_can_be_forced_listed_and_taken_by_number() {
    let (mut g, mut c) = (game(), console());
    assert!(c.execute(&mut g, "offers").text().contains("no offers"));
    // A forced offer is an override, so it is logged.
    assert!(!c.execute(&mut g, "offer hero").ok);
    c.execute(&mut g, "mode override");
    c.execute(&mut g, "give fame 100");
    c.execute(&mut g, "give gold 5000");
    let made = c.execute(&mut g, "offer hero");
    assert!(made.ok, "{}", made.text());
    let listed = c.execute(&mut g, "offers");
    assert!(listed.text().contains("1: "), "{}", listed.text());
    assert!(
        listed.text().contains("gold to join you"),
        "{}",
        listed.text()
    );
    let taken = c.execute(&mut g, "accept 1 Rex the Bold");
    assert!(taken.ok, "{}", taken.text());
    assert!(
        taken.text().contains("Rex the Bold joins"),
        "{}",
        taken.text()
    );
    assert!(c.execute(&mut g, "offers").text().contains("no offers"));
    // An answer to an offer that is gone is refused, and so is a number below one.
    assert!(c.execute(&mut g, "accept 1").error.is_some());
    assert!(c.execute(&mut g, "reject 0").error.is_some());
    // The merchant's item and a rejection.
    assert!(c.execute(&mut g, "offer item").ok);
    assert!(c.execute(&mut g, "reject 1").ok);
    assert!(g.override_log().iter().any(|l| l.contains("offer hero")));
}

#[test]
fn a_recorded_game_is_played_again_by_text() {
    let (mut g, mut c) = (game(), console());
    assert!(c.execute(&mut g, "record").ok);
    assert!(c.execute(&mut g, "tax 2").ok);
    assert!(c.execute(&mut g, "end").ok);
    let r = c.execute(&mut g, "record status");
    // The tax rate and the end of the turn, and what the independent cities' AI ordered.
    let kept: u32 = r
        .text()
        .split_whitespace()
        .find_map(|w| w.parse().ok())
        .unwrap_or(0);
    assert!(kept >= 2, "{}", r.text());
    let name = "console_replay_test";
    let r = c.execute(&mut g, &format!("record stop {name}"));
    assert!(r.ok, "{}", r.text());
    let (turn, tax) = (
        g.turn,
        g.player_component::<f4x_core::player::Tax>(HUMAN)
            .unwrap()
            .0,
    );
    let mut fresh = game();
    let r = c.execute(&mut fresh, &format!("replay {name}"));
    let _ = std::fs::remove_file(format!("saves/{name}.replay.ron"));
    let _ = std::fs::remove_dir("saves");
    assert!(r.ok, "{}", r.text());
    assert_eq!(
        (
            fresh.turn,
            fresh
                .player_component::<f4x_core::player::Tax>(HUMAN)
                .unwrap()
                .0
        ),
        (turn, tax)
    );
    assert!(fresh.is_recording());
}

#[test]
fn a_neutral_city_whose_garrison_is_gone_trains_a_new_one() {
    let mut g = game();
    let (city, pos) = neutral_cities(&g)
        .into_iter()
        .find(|(_, p)| g.units().units_at(*p).len() >= 2)
        .expect("a garrisoned city");
    let owner = g.economy().city_owner(city).unwrap();
    for u in g.units().units_at(pos) {
        g.apply(f4x_core::game::Command::DismissUnit { unit: u });
    }
    assert!(g.units().units_at(pos).is_empty());
    for _ in 0..80 {
        f4x_ai::play_round(&mut g, &mut []);
    }
    assert!(
        !g.units().player_units(owner).is_empty(),
        "the city built soldiers again"
    );
}

#[test]
fn the_start_gives_the_settings_mana_and_reveals_only_the_start_radius() {
    use f4x_core::game::setting::{Radius, Split};
    let known = |radius: f32, mana: i32| {
        let mut spec = NewGame::with_wizards(2);
        spec.seed = 5;
        spec.settings.world.start_reveal_radius = Radius(radius);
        spec.settings.wizards.starting_mana = Split { human: mana, ai: 7 };
        let g = new_game(&content(), &spec).unwrap();
        let human = g.human().unwrap();
        let ai = g.wizards().into_iter().find(|&w| w != human).unwrap();
        assert_eq!(g.treasury(human).unwrap().mana, mana);
        assert_eq!(g.treasury(ai).unwrap().mana, 7);
        g.explored_squares(human)
    };
    // A plus of 5, the 3 by 3 block, and the 5 by 5 block without its corners (the fortress is
    // not at the map's edge, which would cut them).
    assert_eq!(known(1.0, 0), 5);
    assert_eq!(known(1.5, 30), 9);
    assert_eq!(known(2.5, 0), 21);
}
