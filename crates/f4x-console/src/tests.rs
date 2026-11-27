use f4x_def::rules::Ability;

use f4x_core::player::PlayerId;
use f4x_core::rules::unit_flag::UnitFlag;
use f4x_core::world::World;
use f4x_def::content::Content;
use f4x_def::testdir::TestResult;
use f4x_master::Game;

use crate::setup::{NewGame, HUMAN};
use crate::{commands, Access, Console, ConsoleError, Kind, Mode};

/// A new game as `setup::new_game` makes its world.
fn new_game(content: &Content, spec: &NewGame) -> Result<Game, f4x_newgame::error::NewGameError> {
    let mut game = Game::new(crate::setup::new_game(content, spec)?);
    game.set_undo(!spec.all_ai);
    Ok(game)
}

/// The shipped content, read once per test run.
fn content() -> &'static Content {
    f4x_def::testdir::shipped()
}

fn game() -> Game {
    new_game(
        content(),
        &NewGame {
            seed: 7,
            ..NewGame::default()
        },
    )
    .expect("the shipped content starts a game")
}

/// The wizards of a game (the independent cities have players of their own).
fn wizards(w: &World) -> usize {
    w.player_ids()
        .into_iter()
        .filter(|id| !w.is_neutral_player(*id))
        .count()
}

/// A console that keeps its saves in the crate's permanent `tests/saves`, never in `saves`.
fn console() -> Console {
    console_for(Access::Full)
}

fn console_for(access: Access) -> Console {
    let mut c = Console::new(HUMAN, access);
    c.session.saves_dir =
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/saves"));
    c
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
    let (mut w, mut c) = (game(), console());
    let status = c.execute(&mut w, "status");
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
        let r = c.execute(&mut w, line);
        assert!(r.ok, "{line}: {}", r.text());
        assert!(!r.lines.is_empty(), "{line}");
    }
    let cities = c.execute(&mut w, "cities");
    assert!(
        cities.text().contains(&format!("city {}", first_city(&w))),
        "{}",
        cities.text()
    );
    assert!(c.execute(&mut w, "city 99").error.is_some());
}

#[test]
fn access_limits_what_a_front_end_can_ever_do() {
    let mut w = game();
    let mut spectator = console_for(Access::ReadOnly);
    assert!(spectator.execute(&mut w, "status").ok);
    assert!(!spectator.execute(&mut w, "end").ok);
    assert_eq!(w.round(), 1);
    let mut player = console_for(Access::Play);
    assert!(player.execute(&mut w, "end").ok);
    assert_eq!(w.round(), 2);
    assert!(
        !player.execute(&mut w, "new 5").ok,
        "a player may not replace the game"
    );
    assert!(!player.execute(&mut w, "load quick").ok);
    assert_eq!(player.complete("sp"), ["spells"]);
}

#[test]
fn legal_commands_check_ownership() {
    let (mut w, mut c) = (game(), console());
    w.apply(f4x_core::world::Command::CreateUnit {
        owner: PlayerId(1),
        pos: f4x_core::map::Position {
            plane: f4x_core::map::Plane::Arcanus,
            x: 3,
            y: 3,
        },
        unit: w
            .content()
            .rules
            .units
            .id_numbered(39)
            .expect("the shipped content has it"),
    });
    // The rival's unit is the newest one.
    let newest = w
        .units()
        .all_units()
        .into_iter()
        .map(|u| w.units().serial_of(u).0)
        .max()
        .expect("the game has units");
    let r = c.execute(&mut w, &format!("stop {newest}"));
    assert!(matches!(err(&r), ConsoleError::NotAllowed(_)), "{r:?}");
}

#[test]
fn a_player_can_run_a_turn_by_text() {
    let (mut w, mut c) = (game(), console());
    let n = first_city(&w);
    let city = c.execute(&mut w, &format!("city {n}"));
    assert!(city.ok, "{}", city.text());
    let options = c.execute(&mut w, &format!("options {n}"));
    let first = options.lines.first().expect("something to build");
    let id = first
        .split_whitespace()
        .next()
        .expect("there is one")
        .to_string();
    let r = c.execute(&mut w, &format!("produce {n} {id}"));
    assert!(r.ok, "{}", r.text());
    assert!(c.execute(&mut w, &format!("farmers {n} 1")).ok);
    assert!(c.execute(&mut w, "tax 1").ok);
    assert!(c.execute(&mut w, "power 34 33 33").ok);
    let end = c.execute(&mut w, "end");
    assert!(end.ok);
    assert_eq!(w.round(), 2);
    assert!(end.text().contains("Turn 2"));
    assert!(
        c.execute(&mut w, "undo").error.is_some(),
        "ending a turn cannot be undone"
    );
}

#[test]
fn unknown_and_malformed_lines_are_errors_not_panics() {
    let (mut w, mut c) = (game(), console());
    assert!(matches!(err(&c.execute(&mut w, "")), ConsoleError::Empty));
    assert!(matches!(
        err(&c.execute(&mut w, "frobnicate")),
        ConsoleError::Unknown(_)
    ));
    assert!(c.execute(&mut w, "move").error.is_some());
    assert!(c.execute(&mut w, "move x y").error.is_some());
    assert!(c.execute(&mut w, "move 999 1,1").error.is_some());
    assert!(c.execute(&mut w, "cast nonsense").error.is_some());
    assert!(c.execute(&mut w, r#"tile "1,1"#).error.is_some());
    assert!(c.execute(&mut w, "road 1").error.is_some());
}

#[test]
fn moving_into_an_enemy_starts_a_battle_you_can_fight_by_text() -> TestResult {
    let (mut w, mut c) = (game(), console());
    let first_city = c.execute(&mut w, "cities");
    let at = first_city.lines[0]
        .split(" at ")
        .nth(1)
        .expect("a city line says where it is")
        .split_whitespace()
        .next()
        .expect("the place follows")
        .to_string();
    let (x, y): (u32, u32) = {
        let mut it = at.split(',');
        (
            it.next().expect("there is one").parse()?,
            it.next().expect("there is one").parse()?,
        )
    };
    w.apply(f4x_core::world::Command::CreateUnit {
        owner: PlayerId(1),
        pos: f4x_core::map::Position {
            plane: f4x_core::map::Plane::Arcanus,
            x: x + 1,
            y,
        },
        unit: w
            .content()
            .rules
            .units
            .id_numbered(39)
            .expect("the shipped content has it"),
    });
    let r = c.execute(&mut w, &format!("attack @{x},{y} {},{y}", x + 1));
    assert!(r.ok, "{}", r.text());
    assert!(w.in_battle(), "{}", r.text());
    assert!(c.execute(&mut w, "battle").ok);
    // Fight on by text until it ends.
    for _ in 0..400 {
        if !w.in_battle() {
            break;
        }
        let b = w.battle().expect("a battle is on");
        if b.player_of(b.active) == PlayerId(1) {
            // The opponent's side: let the console play it as that player.
            let order = f4x_ai::battle::order::next_command(b);
            w.apply(f4x_core::world::Command::Battle(order));
            continue;
        }
        let order = f4x_ai::battle::order::next_command(b);
        let line = match order {
            f4x_battle::battle::BattleCommand::Move { unit, to } => {
                format!("bmove {unit} {},{}", to.0, to.1)
            }
            f4x_battle::battle::BattleCommand::Attack { unit, target } => {
                format!("battack {unit} {target}")
            }
            f4x_battle::battle::BattleCommand::Shoot { unit, target } => {
                format!("bshoot {unit} {target}")
            }
            f4x_battle::battle::BattleCommand::AttackWall { unit, cell } => {
                format!("bwall {unit} {},{}", cell.0, cell.1)
            }
            f4x_battle::battle::BattleCommand::ShootWall { unit, cell } => {
                format!("bshootwall {unit} {},{}", cell.0, cell.1)
            }
            f4x_battle::battle::BattleCommand::Cast { .. }
            | f4x_battle::battle::BattleCommand::UnitCast { .. }
            | f4x_battle::battle::BattleCommand::SummonDemon { .. } => {
                unreachable!("the plain unit AI casts nothing")
            }
            f4x_battle::battle::BattleCommand::MoveVortex { .. } => {
                unreachable!("the plain unit AI steers no vortex")
            }
            f4x_battle::battle::BattleCommand::Merge { unit } => format!("bmerge {unit}"),
            f4x_battle::battle::BattleCommand::Surface { unit, to } => {
                format!("bsurface {unit} {},{}", to.0, to.1)
            }
            f4x_battle::battle::BattleCommand::Done { unit } => format!("bdone {unit}"),
            f4x_battle::battle::BattleCommand::EndTurn => "bend".into(),
            f4x_battle::battle::BattleCommand::Flee => "bflee".into(),
        };
        let r = c.execute(&mut w, &line);
        assert!(r.ok, "{line}: {}", r.text());
    }
    assert!(!w.in_battle(), "the battle ended");
    Ok(())
}

#[test]
fn a_finished_game_makes_the_console_refuse_play() {
    let (mut w, mut c) = (game(), console());
    w.world_mut().complete_mastery(HUMAN);
    assert!(!w.check_end().is_empty());
    assert!(w.game_over().is_some());
    assert!(c.execute(&mut w, "status").text().contains("GAME OVER"));
    assert!(!c.execute(&mut w, "end").ok);
    assert!(c.execute(&mut w, "score").ok);
}

#[test]
fn read_only_mode_runs_queries_and_nothing_else() {
    let (mut w, mut c) = (game(), console());
    assert!(c.execute(&mut w, "mode readonly").ok);
    assert_eq!(c.session.mode, Mode::ReadOnly);
    assert!(c.execute(&mut w, "status").ok);
    assert!(c.execute(&mut w, "cities").ok);
    for line in ["end", "tax 1", "save", "new 3"] {
        let r = c.execute(&mut w, line);
        assert!(
            matches!(err(&r), ConsoleError::NotAllowed(_)),
            "{line}: {r:?}"
        );
    }
    assert_eq!(w.round(), 1);
    assert!(c.execute(&mut w, "mode legal").ok);
    assert!(c.execute(&mut w, "end").ok);
    assert_eq!(w.round(), 2);
    // A console limited to read-only access starts there and cannot leave.
    let mut spectator = console_for(Access::ReadOnly);
    assert_eq!(spectator.session.mode, Mode::ReadOnly);
    assert!(!spectator.execute(&mut w, "mode legal").ok);
    let mut player = console_for(Access::Play);
    assert!(player.execute(&mut w, "mode readonly").ok);
    assert!(player.execute(&mut w, "mode legal").ok);
}

#[test]
fn new_game_settings_reach_the_game() -> TestResult {
    let content = content();
    let made = |spec: NewGame| new_game(content, &spec).expect("the spec starts a game");
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
            s.basic.land_percent = f4x_newgame::setup::LAND_SMALL;
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
        f4x_newgame::setup::LAND_SMALL
    );
    assert_eq!(
        (
            small.settings().basic.difficulty,
            small.settings().basic.magic_strength
        ),
        (4, 2)
    );
    let land = |w: &World| {
        let map = w.map().expect("the game has a map");
        f4x_newgame::mapgen::land_percent(
            map.plane(f4x_core::map::Plane::Arcanus),
            &w.content().terrains,
        )
    };
    // The share asked for, and the polar caps on top of it (about 7%).
    let near = |got: u32, asked: u32| (asked..=asked + 10).contains(&got);
    assert!(near(land(&small), 15), "small land {}", land(&small));
    assert!(near(land(&base), 30), "large land {}", land(&base));
    Ok(())
}

#[test]
fn up_to_ten_wizards_start_shared_between_the_planes_evenly() -> TestResult {
    use f4x_core::map::Plane;
    for wizards in 2..=10u8 {
        for seed in [3u64, 4] {
            let mut spec = NewGame::with_wizards(wizards);
            spec.seed = seed;
            (
                spec.settings.basic.map_width,
                spec.settings.basic.map_height,
            ) = (90, 60);
            let w = new_game(content(), &spec)?;
            let planes: Vec<Plane> = (0..wizards)
                .map(|k| {
                    w.wizard(PlayerId(k))
                        .expect("the player is a wizard")
                        .fortress()
                        .expect("the wizard has a fortress")
                        .plane
                })
                .collect();
            // Every wizard has a colour of their own, and none has the neutral one.
            let mut banners: Vec<u8> = (0..wizards)
                .map(|k| {
                    w.wizard(PlayerId(k))
                        .expect("the player is a wizard")
                        .banner()
                })
                .collect();
            banners.sort_unstable();
            banners.dedup();
            assert_eq!(banners.len(), usize::from(wizards), "{wizards} wizards");
            assert!(banners
                .iter()
                .all(|&b| b < f4x_core::player::NEUTRAL_BANNER));
            let myrror = planes.iter().filter(|&&p| p == Plane::Myrror).count() as i32;
            let arcanus = i32::from(wizards) - myrror;
            assert!(
                (-1..=2).contains(&(arcanus - myrror)),
                "{wizards} wizards, seed {seed}: {arcanus} on Arcanus, {myrror} on Myrror"
            );
        }
    }
    Ok(())
}

#[test]
fn a_crowded_game_is_warned_about_and_still_starts() {
    let mut b = f4x_core::world::setting::BasicSettings::default();
    (b.map_width, b.map_height) = (90, 60);
    b.opponents = 9;
    assert_eq!(b.squares_per_player(), 1080);
    assert_eq!(b.crowding(), None);
    (b.map_width, b.map_height) = (60, 40);
    assert_eq!(b.crowding(), Some(480));
    b.opponents = 4;
    assert_eq!(b.crowding(), None);
}

#[test]
fn the_originals_map_generator_makes_a_playable_game_with_the_real_content() -> TestResult {
    let mut spec = NewGame::with_wizards(3);
    spec.seed = 11;
    let noise = new_game(content(), &spec)?;
    spec.settings.basic.original_map_generator = true;
    let mut w = new_game(content(), &spec)?;
    assert_ne!(noise.map(), w.map());
    for _ in 0..3 {
        w.play_round();
    }
    Ok(())
}

fn level_game(level: u8, wizards: u8, seed: u64) -> Game {
    let mut spec = NewGame::with_wizards(wizards);
    spec.seed = seed;
    let basic = spec.settings.basic;
    spec.settings =
        f4x_core::world::setting::GameSettings::for_difficulty(&content().settings, level);
    spec.settings.basic = f4x_core::world::setting::BasicSettings {
        difficulty: level,
        ..basic
    };
    new_game(content(), &spec).expect("the spec starts a game")
}

#[test]
fn each_level_gives_its_starting_gold_and_the_computer_wizards_their_bonuses() -> TestResult {
    for level in 0..=5u8 {
        let w = level_game(level, 2, 5);
        let human = w.human().expect("the game has a human");
        let ai = w
            .wizards()
            .into_iter()
            .find(|&id| id != human)
            .expect("it is there");
        assert_eq!(
            w.player(human).expect("the player exists").treasury().gold,
            [125, 100, 75, 50, 40, 25][usize::from(level)]
        );
        assert_eq!(
            w.player(ai).expect("the player exists").treasury().gold,
            150
        );
        let percent = |p| {
            w.player_component::<f4x_core::player::Bonus>(p)
                .expect("the entity has it")
                .production
        };
        assert_eq!(percent(human), 100);
        assert_eq!(
            percent(ai),
            [100, 125, 150, 200, 300, 400][usize::from(level)],
            "level {level}"
        );
    }
    Ok(())
}

#[test]
fn computer_wizards_are_made_to_the_levels_pick_budget() -> TestResult {
    use f4x_core::wizard::{all_retorts, retort_cost};
    for (level, picks) in [(2u8, 11), (3, 13), (4, 14), (5, 15)] {
        for seed in [1u64, 2, 3, 4] {
            let w = level_game(level, 3, seed);
            let human = w.human().expect("the game has a human");
            let mut names = Vec::new();
            for (id, p) in w
                .wizards()
                .into_iter()
                .filter(|&id| id != human)
                .filter_map(|id| Some((id, w.wizard(id)?)))
            {
                let spent = p.total_books()
                    + all_retorts(&w.content().rules)
                        .filter(|&r| p.has(r))
                        .map(|r| i32::from(retort_cost(&w.content().rules, r)))
                        .sum::<i32>();
                // Erasure takes books and refilling puts them back; a wizard may end a pick or
                // two over when the last retort did not fit, never under.
                assert!(
                    (picks..=picks + 1).contains(&spent),
                    "level {level} seed {seed}: {} spent {spent} of {picks}",
                    w.player(id).expect("the player exists").name()
                );
                names.push(w.player(id).expect("the player exists").name());
            }
            names.sort();
            names.dedup();
            assert_eq!(names.len(), 2, "two different wizards");
        }
    }
    Ok(())
}

#[test]
fn with_four_wizards_above_average_one_or_two_computer_wizards_are_myrran() {
    for level in [3u8, 4] {
        for seed in [1u64, 2, 3, 4, 5, 6] {
            let w = level_game(level, 4, seed);
            let human = w.human().expect("the game has a human");
            let n = w
                .wizards()
                .into_iter()
                .filter(|&id| id != human)
                .filter_map(|id| w.wizard(id))
                .filter(|p| p.has(f4x_core::wizard::magic::Retort::new(9) /* myrran */))
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
    let mut world = game();
    let mut c = console();
    c.execute(
        &mut world,
        "new 3 land=small difficulty=hard magic=weak players=3",
    );
    let r = c.execute(&mut world, "settings");
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
    let mut world = game();
    let mut c = console();
    let r = c.execute(
        &mut world,
        "new 3 land=small difficulty=hard magic=weak players=3",
    );
    assert!(r.error.is_none(), "{:?}", r.error);
    assert_eq!(
        (
            world.settings().basic.difficulty,
            world.settings().basic.magic_strength
        ),
        (3, 0)
    );
    assert_eq!(wizards(&world), 3);
    let r = c.execute(&mut world, "new 3 difficulty=nightmare");
    assert!(matches!(r.error, Some(ConsoleError::Parse(_))));
}

#[test]
fn the_vizier_command_turns_the_vizier_on_and_off() {
    let (mut w, mut c) = (game(), console());
    assert!(!w.player(HUMAN).is_some_and(|p| p.has_grand_vizier()));
    assert!(c.execute(&mut w, "vizier on").error.is_none());
    assert!(w.player(HUMAN).is_some_and(|p| p.has_grand_vizier()));
    assert!(c.execute(&mut w, "vizier off").error.is_none());
    assert!(!w.player(HUMAN).is_some_and(|p| p.has_grand_vizier()));
    assert!(c.execute(&mut w, "vizier maybe").error.is_some());
}

/// The number the console gives the first city (its creation number).
fn first_city(w: &World) -> u64 {
    w.units().serial_of(w.economy().cities()[0].0).0
}

#[test]
fn the_autobuild_command_sets_and_clears_a_citys_plan() {
    let (mut w, mut c) = (game(), console());
    let city = w.economy().cities()[0].0;
    let n = first_city(&w);
    let r = c.execute(
        &mut w,
        &format!("autobuild {n} both defense gold settlers=5"),
    );
    assert!(r.ok, "{}", r.text());
    let plan = w
        .autobuild_of(city)
        .expect("the city has an autobuild plan")
        .plan;
    assert_eq!(plan.mix, f4x_core::city::autobuild::Mix::Both);
    assert_eq!(plan.primary, f4x_core::city::autobuild::Focus::Defense);
    assert_eq!(plan.secondary, Some(f4x_core::city::autobuild::Focus::Gold));
    assert!(plan.settlers && plan.min_settler_pop == 5);
    assert!(c.execute(&mut w, &format!("autobuild {n} off")).ok);
    assert!(w.autobuild_of(city).is_none());
    assert!(c
        .execute(&mut w, &format!("autobuild {n} both nonsense"))
        .error
        .is_some());
    assert!(c
        .execute(&mut w, &format!("autobuild {n} stuff gold"))
        .error
        .is_some());
}

#[test]
fn a_global_enchantment_is_a_target_by_wizard_and_spell() -> TestResult {
    use f4x_core::rules::exec::Target;
    let mut w = game();
    let awareness = w
        .content()
        .rules
        .spells
        .id_numbered(209)
        .expect("the shipped content has it");
    w.attach_enchantment(Target::Player(PlayerId(1)), awareness, PlayerId(1));
    let held = w
        .global_instance(PlayerId(1), awareness)
        .expect("the spell is held");
    assert_eq!(
        crate::args::parse_target(&w, "global:1:awareness")?,
        Target::Spell(held)
    );
    assert!(crate::args::parse_target(&w, "global:1").is_err());
    assert!(crate::args::parse_target(&w, "global:1:no_such_spell").is_err());
    // A wizard that keeps nothing of the kind going has nothing to name.
    assert!(crate::args::parse_target(&w, "global:2:awareness").is_err());
    Ok(())
}

#[test]
fn nature_s_eye_reveals_visible_units_four_squares_from_the_city() -> TestResult {
    use f4x_core::rules::exec::Target;
    // Casting takes skill, and skill comes from books: a nature wizard.
    let mut w = new_game(
        content(),
        &NewGame {
            seed: 7,
            books: [
                (f4x_core::rules::RealmId::new(0 /* nature */), 6),
                (f4x_core::rules::RealmId::new(1 /* sorcery */), 5),
            ]
            .into(),
            ..NewGame::default()
        },
    )
    .expect("the shipped content starts a game");
    let (city, _, _) = w
        .economy()
        .cities()
        .into_iter()
        .find(|(_, c, _)| c.owner == HUMAN)
        .expect("it is there");
    let eye = w
        .content()
        .rules
        .spells
        .id_numbered(12)
        .expect("the shipped content has it");
    assert!(w.cheat_learn_spell(HUMAN, eye));
    assert_eq!(
        w.economy().city_sight(city),
        f4x_def::half_int::HalfInt::from_raw(5)
    );
    // A rival's plain unit four squares away and an invisible one three squares away, and a
    // plain one five squares away.
    let home = w.city_position(city).expect("the city is on the map");
    let width = w.map().expect("the game has a map").width();
    let rival = PlayerId(1);
    let beside = |dx: u32| f4x_core::map::Position {
        x: (home.x + dx) % width,
        ..home
    };
    let spawn = |w: &mut Game, id: u16, dx: u32| {
        let def = w
            .content()
            .rules
            .units
            .id_numbered(id)
            .expect("the shipped content has it");
        w.spawn_unit(rival, beside(dx), def)
    };
    let near = spawn(&mut w, 39 /* barbarian_spearmen */, 4);
    let ghost = spawn(&mut w, 63 /* dark_elf_nightblades */, 3);
    let far = spawn(&mut w, 39 /* barbarian_spearmen */, 5);
    // What the wizard makes out: the square is in sight and the unit is not hidden.
    let sees = |w: &World, unit: hecs::Entity| {
        let at = w
            .units()
            .unit_position(unit)
            .expect("the unit is on the map");
        let seen = w.economy().visibility(HUMAN, at.plane);
        seen[(at.y * width + at.x) as usize] && w.economy().unit_visible_to(unit, HUMAN)
    };
    assert!(!sees(&w, near), "four squares is out of sight at first");
    w.treasury_mut(HUMAN)
        .expect("the player has a treasury")
        .mana = f4x_def::half_int::HalfInt::from_int(5000);
    w.start_cast(HUMAN, eye, Some(Target::City(city)))
        .map_err(|e| format!("{e:?}"))?;
    // Paid over turns by the casting skill, then aimed.
    for _ in 0..60 {
        if w.spells()
            .city_enchantments(city)
            .iter()
            .any(|&(s, _)| s == eye)
        {
            break;
        }
        w.play_round();
        w.apply(f4x_core::world::Command::ResolveCast {
            player: HUMAN,
            target: Target::City(city),
        });
    }
    assert!(w
        .spells()
        .city_enchantments(city)
        .iter()
        .any(|&(s, _)| s == eye));
    assert_eq!(
        w.economy().city_sight(city),
        f4x_def::half_int::HalfInt::from_raw(9)
    );
    assert!(sees(&w, near), "the eye reveals what is four squares away");
    assert!(!sees(&w, ghost), "an invisible unit stays hidden");
    assert!(!sees(&w, far), "five squares is still out of sight");
    Ok(())
}

fn neutral_cities(w: &World) -> Vec<(hecs::Entity, f4x_core::map::Position)> {
    w.economy()
        .cities()
        .into_iter()
        .filter(|(_, c, _)| w.is_neutral_player(c.owner))
        .map(|(e, _, p)| (e, p))
        .collect()
}

#[test]
fn a_new_world_has_neutral_cities_on_both_planes() -> TestResult {
    use f4x_core::map::Plane;
    let w = game();
    let cities = neutral_cities(&w);
    for plane in [Plane::Arcanus, Plane::Myrror] {
        let here: Vec<_> = cities.iter().filter(|(_, p)| p.plane == plane).collect();
        assert!((2..=15).contains(&here.len()), "{plane:?}: {}", here.len());
        // Cities keep their distance from each other and from every fortress.
        let grid = w.map().expect("the game has a map").plane(plane);
        for (i, (_, a)) in here.iter().enumerate() {
            for (_, b) in &here[i + 1..] {
                assert!(grid.chebyshev((a.x, a.y), (b.x, b.y)) >= 4);
            }
            for f in w
                .player_ids()
                .into_iter()
                .filter_map(|id| w.wizard(id)?.fortress())
            {
                // The original's `Range`: the longer way plus half the shorter.
                let (dx, dy) = (a.x.abs_diff(f.x), a.y.abs_diff(f.y));
                let dx = dx.min(grid.width() - dx);
                assert!(
                    f.plane != plane || dx.max(dy) + dx.min(dy) / 2 >= 5,
                    "city {a:?} fortress {f:?}"
                );
            }
        }
    }
    let mut names = std::collections::BTreeSet::new();
    for (e, p) in &cities {
        let races = &w.content().rules.races;
        let race = races.get(
            w.component::<f4x_core::city::economy::CityRace>(*e)
                .expect("the entity has it")
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
            w.component::<f4x_core::city::City>(*e)
                .expect("the entity has it")
                .name
                .clone()
        ));
        let pop = w
            .component::<f4x_core::city::economy::Population>(*e)
            .expect("the entity has it")
            .figures;
        assert!((2..=14).contains(&pop), "population {pop}");
        // Garrison: a quarter of the population, once for each of the two kinds.
        let garrison = w.units().units_at(*p).len() as u32;
        assert!(garrison <= 2 * (pop / 4), "{garrison} units for {pop}");
        for u in w.units().units_at(*p) {
            assert_eq!(
                w.units().unit_owner(u),
                w.economy().city_owner(*e),
                "the garrison is the city's own"
            );
        }
    }
    assert!(
        cities
            .iter()
            .any(|(_, p)| !w.units().units_at(*p).is_empty()),
        "some city is garrisoned"
    );
    Ok(())
}

#[test]
fn neutral_worlds_follow_the_seed() -> TestResult {
    let content = content();
    let made = |seed| {
        let w = new_game(
            content,
            &NewGame {
                seed,
                ..NewGame::default()
            },
        )
        .expect("the seed starts a game");
        neutral_cities(&w)
            .into_iter()
            .map(|(_, p)| p)
            .collect::<Vec<_>>()
    };
    assert_eq!(made(3), made(3));
    assert_ne!(made(3), made(4));
    Ok(())
}

#[test]
fn neutral_cities_play_by_the_ordinary_rules() {
    let mut w = game();
    let before: Vec<_> = neutral_cities(&w)
        .into_iter()
        .map(|(e, _)| {
            let pop = w
                .component::<f4x_core::city::economy::Population>(e)
                .expect("the entity has it");
            (
                e,
                pop.figures,
                w.component::<f4x_core::city::economy::Buildings>(e)
                    .expect("the entity has it")
                    .0
                    .len(),
            )
        })
        .collect();
    let gold: Vec<_> = w
        .player_ids()
        .iter()
        .map(|id| w.player(*id).expect("the player exists").treasury().gold)
        .collect();
    for _ in 0..40 {
        f4x_ai::play_round(&mut w, &mut []);
    }
    // (The neutral players play like any AI now: they may take or found a city.)
    assert!(
        neutral_cities(&w).len() >= before.len(),
        "no neutral city is lost on its own"
    );
    let grew = before.iter().any(|&(e, pop, _)| {
        w.component::<f4x_core::city::economy::Population>(e)
            .expect("the entity has it")
            .figures
            > pop
    });
    let built = before.iter().any(|&(e, _, n)| {
        w.component::<f4x_core::city::economy::Buildings>(e)
            .expect("the entity has it")
            .0
            .len()
            > n
    });
    assert!(grew, "a neutral city grew");
    assert!(built, "a neutral city built something");
    // (The wizards here play no turns, so the neutral players may well beat one: only a neutral
    // player must not be defeated.)
    assert!(w
        .player_ids()
        .iter()
        .filter(|&&id| !w.is_wizard(id))
        .all(|id| !w.player(*id).is_some_and(|p| p.is_defeated())));
    let _ = gold;
}

#[test]
fn every_neutral_city_has_the_player_and_treasury_of_its_race() {
    let mut w = game();
    let cities = neutral_cities(&w);
    let owners: std::collections::BTreeSet<_> = cities
        .iter()
        .filter_map(|(e, _)| w.economy().city_owner(*e))
        .collect();
    let races: std::collections::BTreeSet<_> = cities
        .iter()
        .filter_map(|(e, _)| w.city_race(*e))
        .map(f4x_core::player::race_player::race_player_id)
        .collect();
    assert_eq!(owners, races, "one player for each race");
    for &o in &owners {
        assert!(
            w.is_neutral_player(o) && w.player(o).expect("the player exists").treasury().gold > 0
        );
        assert!(!w.living_wizards().contains(&o), "not a wizard");
    }
    assert_eq!(wizards(&w), 2);
    // Their taxes and upkeep move their own gold, and nobody else's.
    let wizard_gold: Vec<_> = w
        .player_ids()
        .into_iter()
        .filter(|id| !w.is_neutral_player(*id))
        .map(|id| w.player(id).expect("the player exists").treasury().gold)
        .collect();
    let before: Vec<_> = owners
        .iter()
        .map(|&o| w.player(o).expect("the player exists").treasury().gold)
        .collect();
    w.play_round();
    let after: Vec<_> = owners
        .iter()
        .map(|&o| w.player(o).expect("the player exists").treasury().gold)
        .collect();
    assert_ne!(before, after, "the treasuries moved");
    assert_eq!(
        wizard_gold.len(),
        w.player_ids()
            .into_iter()
            .filter(|id| !w.is_neutral_player(*id))
            .count()
    );
}

#[test]
fn a_neutral_city_that_cannot_pay_loses_its_garrison() {
    let mut w = game();
    let (city, pos) = neutral_cities(&w)
        .into_iter()
        .find(|(_, p)| w.units().units_at(*p).len() >= 2)
        .expect("a garrisoned city");
    let owner = w.economy().city_owner(city).expect("the city has an owner");
    let units = w.units().units_at(pos).len();
    // Buildings the city's taxes cannot carry, and an empty treasury.
    let all: Vec<_> = w
        .content()
        .rules
        .buildings
        .iter()
        .map(|(id, _)| id)
        .collect();
    for id in all {
        w.scenario_set_building(city, id, true);
    }
    w.treasury_mut(owner)
        .expect("the player has a treasury")
        .gold = f4x_def::half_int::HalfInt::from_int(0);
    w.play_round();
    assert!(
        w.units().units_at(pos).len() < units,
        "{units} units stayed"
    );
}

#[test]
fn taking_a_neutral_city_changes_its_owner_and_the_race_player_goes_with_its_last_city() {
    let mut w = game();
    let wizards_before = wizards(&w);
    let (city, _) = neutral_cities(&w)[0];
    let owner = w.economy().city_owner(city).expect("the city has an owner");
    let events = w.capture_city(city, PlayerId(0));
    assert_eq!(w.economy().city_owner(city), Some(PlayerId(0)));
    assert!(events
        .iter()
        .all(|e| matches!(e, f4x_core::world::Event::CityCaptured { .. })));
    // The race's player stays while it has a city or a unit; with neither it goes at the round
    // end, not before (a race with one city and no garrison is such a case).
    if w.economy()
        .cities()
        .iter()
        .any(|(_, c, _)| c.owner == owner)
        || !w.units().player_units(owner).is_empty()
    {
        assert!(w.has_player(owner));
    }
    // Even when the last one falls nobody is "defeated".
    for (c, _) in neutral_cities(&w) {
        assert!(w
            .capture_city(c, PlayerId(0))
            .iter()
            .all(|e| matches!(e, f4x_core::world::Event::CityCaptured { .. })));
    }
    assert!(neutral_cities(&w).is_empty());
    w.end_round();
    // (No battle was fought here, so its garrison stays with it, and so does the player.)
    assert_eq!(
        w.has_player(owner),
        !w.units().player_units(owner).is_empty(),
        "the race player goes when it has no city and no unit"
    );
    assert_eq!(wizards(&w), wizards_before);
    assert!(w
        .player_ids()
        .iter()
        .all(|id| !w.player(*id).is_some_and(|p| p.is_defeated())));
}

#[test]
fn neutral_cities_survive_a_save() -> TestResult {
    let w = game();
    let snap = w.snapshot();
    let back = snap.clone().restore(w.content().clone())?;
    assert_eq!(back.snapshot(), snap);
    assert_eq!(neutral_cities(&back).len(), neutral_cities(&w).len());
    Ok(())
}

/// The units, in the order they were made, of a player's starting army.
fn start_army(w: &World, p: PlayerId) -> Vec<String> {
    w.units()
        .player_units(p)
        .iter()
        .filter_map(|&u| w.units().unit_def(u).map(|d| d.id.clone()))
        .collect()
}

/// A wizard's capital and what is in it.
fn capital(w: &World, p: PlayerId) -> (hecs::Entity, f4x_core::map::Position) {
    w.economy()
        .cities()
        .into_iter()
        .find(|(_, c, _)| c.owner == p)
        .map(|(e, _, at)| (e, at))
        .expect("the game has a map")
}

#[test]
fn the_starting_army_and_capital_pay_their_way() -> TestResult {
    let content = content();
    for seed in [1, 2, 3, 4, 5, 6] {
        let mut w = new_game(
            content,
            &NewGame {
                seed,
                ..NewGame::default()
            },
        )?;
        let t = w.budget(HUMAN);
        assert!(t.food_surplus >= t.food_upkeep, "seed {seed}: {t:?}");
        assert!(t.net_gold() >= 0, "seed {seed}: {t:?}");
        // Growth makes new mouths, so a careful ruler keeps putting people on the farms.
        for _ in 0..8 {
            w.balance_farmers(HUMAN);
            w.play_round();
        }
        assert_eq!(start_army(&w, HUMAN).len(), 2, "seed {seed}");
        assert!(
            w.player(HUMAN).expect("the player exists").treasury().gold >= 75,
            "seed {seed}"
        );
    }
    Ok(())
}

#[test]
fn a_myrran_wizard_starts_on_myrror_with_a_myrror_race() -> TestResult {
    use f4x_core::map::Plane;
    use f4x_core::wizard::magic::Retort;
    let content = content();
    let wizard = |race: Option<u16>| NewGame {
        seed: 2,
        books: [
            (content.rules.realms.id_numbered(1).unwrap(), 5),
            (content.rules.realms.id_numbered(0).unwrap(), 3),
        ]
        .into_iter()
        .collect(),
        retorts: [Retort::new(9) /* myrran */].into_iter().collect(),
        race,
        ..NewGame::default()
    };
    let w = new_game(content, &wizard(Some(4 /* dwarven */)))?;
    let (city, at) = capital(&w, HUMAN);
    assert_eq!(at.plane, Plane::Myrror);
    // Dwarves have no spearmen: two swordsmen.
    assert_eq!(
        start_army(&w, HUMAN),
        ["dwarven_swordsmen", "dwarven_swordsmen"]
    );
    assert!(
        w.economy()
            .city_economy(city)
            .expect("the city has an economy")
            .max_size
            >= 7
    );
    // The rival is no Myrran and starts on Arcanus.
    assert_eq!(capital(&w, PlayerId(1)).1.plane, Plane::Arcanus);
    // Nobody picks an Arcanus race for a Myrran wizard.
    assert!(new_game(content, &wizard(Some(8 /* high_men */))).is_err());
    let any = new_game(content, &wizard(None))?;
    assert_eq!(capital(&any, HUMAN).1.plane, Plane::Myrror);
    Ok(())
}

#[test]
fn battle_units_carry_their_immunities_and_whether_their_weapons_are_magical() -> TestResult {
    let content = content();
    let mut w = new_game(
        content,
        &NewGame {
            seed: 1,
            ..NewGame::default()
        },
    )?;
    let (_, home) = capital(&w, HUMAN);
    let werewolves = w
        .content()
        .rules
        .units
        .id_numbered(168)
        .expect("the shipped content has it");
    w.apply(f4x_core::world::Command::CreateUnit {
        owner: HUMAN,
        pos: home,
        unit: werewolves,
    });
    let events = w.apply(f4x_core::world::Command::BeginBattle {
        attackers: w.units().player_units(HUMAN),
        defenders: w.units().player_units(PlayerId(1)),
    });
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, f4x_core::world::Event::Rejected(_))),
        "{events:?}"
    );
    let battle = w.battle().expect("a battle is on");
    let find = |num: u16| {
        battle
            .units
            .iter()
            .find(|u| u.unit_id.is_some_and(|id| id.num() == num))
            .expect("it is there")
    };
    let spearmen = battle
        .units
        .iter()
        .find(|u| u.unit_id.is_some_and(|id| id.num() != 168 /* werewolves */))
        .expect("the wizard starts with a unit");
    assert!(!spearmen.flag(UnitFlag::MagicalWeapon));
    let wolves = find(168 /* werewolves */);
    assert!(
        wolves.flag(UnitFlag::MagicalWeapon),
        "summoned creatures fight with magical weapons"
    );
    assert!(wolves.abilities.contains(&Ability::new(72)));
    Ok(())
}

#[test]
fn battle_units_carry_the_numbers_of_their_special_attacks() -> TestResult {
    use f4x_battle::battle::RangedKind;
    use f4x_battle::special::SpecialAttacks;
    let content = content();
    let mut w = new_game(
        content,
        &NewGame {
            seed: 1,
            ..NewGame::default()
        },
    )?;
    let (_, home) = capital(&w, HUMAN);
    for name in [
        181, /* cockatrices */
        183, /* giant_spiders */
        182, /* basilisk */
        170, /* wraiths */
    ] {
        let unit = w
            .content()
            .rules
            .units
            .id_numbered(name)
            .expect("the shipped content has it");
        w.apply(f4x_core::world::Command::CreateUnit {
            owner: HUMAN,
            pos: home,
            unit,
        });
    }
    w.apply(f4x_core::world::Command::BeginBattle {
        attackers: w.units().player_units(HUMAN),
        defenders: w.units().player_units(PlayerId(1)),
    });
    let battle = w.battle().expect("a battle is on");
    let special = |num: u16| {
        let u = battle
            .units
            .iter()
            .find(|u| u.unit_id.is_some_and(|id| id.num() == num))
            .expect("it is there");
        (
            u.special_level,
            SpecialAttacks::of_unit(&u.abilities, &u.granted, u.special_level),
            u.ranged_kind,
        )
    };
    let (level, s, _) = special(181 /* cockatrices */);
    assert_eq!((level, s.stoning_touch, s.penalty), (3, true, 3));
    let (_, s, _) = special(183 /* giant_spiders */);
    assert_eq!(s.poison, 4);
    let (_, s, kind) = special(182 /* basilisk */);
    assert_eq!(
        (s.stoning_gaze, s.death_gaze, s.penalty, kind),
        (true, false, 1, RangedKind::Short)
    );
    let (_, s, _) = special(170 /* wraiths */);
    assert!(s.life_steal);
    assert_eq!(s.penalty, 3);
    Ok(())
}

/// A two-wizard game, the rival's capital with `citizens` citizens and the buildings given.
fn siege_world(citizens: u32, extra: &[u16]) -> (Game, hecs::Entity) {
    let mut w = game();
    let city = capital(&w, PlayerId(1)).0;
    w.scenario_set_citizens(city, citizens);
    let ids: Vec<_> = extra
        .iter()
        .map(|b| {
            w.content()
                .rules
                .buildings
                .id_numbered(*b)
                .expect("the shipped content has it")
        })
        .collect();
    for id in ids {
        w.scenario_set_building(city, id, true);
    }
    (w, city)
}

/// A game with an independent city of `citizens` citizens and the given buildings (the
/// rival's capital is the last city of a wizard, whose fall ends the game).
fn neutral_world(citizens: u32, extra: &[u16]) -> (Game, hecs::Entity) {
    let mut w = game();
    // The first city with a garrison: a small one may have none.
    let city = neutral_cities(&w)
        .into_iter()
        .find(|(_, p)| !w.units().units_at(*p).is_empty())
        .expect("a garrisoned neutral city")
        .0;
    w.scenario_set_citizens(city, citizens);
    let ids: Vec<_> = extra
        .iter()
        .map(|b| {
            w.content()
                .rules
                .buildings
                .id_numbered(*b)
                .expect("the shipped content has it")
        })
        .collect();
    for id in ids {
        w.scenario_set_building(city, id, true);
    }
    (w, city)
}

/// A land square next to `at`, preferring the east one.
fn land_beside(w: &World, at: f4x_core::map::Position) -> f4x_core::map::Position {
    use f4x_core::map::terrain::TerrainClass;
    let grid = w.map().expect("the game has a map").plane(at.plane);
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .map(|(dx, dy)| f4x_core::map::Position {
            x: grid.wrap_x(at.x as i32 + dx),
            y: (at.y as i32 + dy).max(0) as u32,
            ..at
        })
        .find(|p| {
            grid.get(p.x as i32, p.y as i32)
                .and_then(|t| w.content().terrains.get(t))
                .is_some_and(|d| d.class != TerrainClass::Ocean)
        })
        .expect("a city has land beside it")
}

/// Put the human wizard's two units on the square next to the city and start the battle.
fn start_siege(w: &mut Game, city: hecs::Entity) {
    let at = *w
        .component::<f4x_core::map::Position>(city)
        .expect("the entity has it");
    let from = land_beside(w, at);
    for u in w.units().player_units(HUMAN) {
        w.cheat_move_unit(u, from);
    }
    let events = w.apply(f4x_core::world::Command::BeginBattle {
        attackers: w.units().player_units(HUMAN),
        defenders: w.units().units_at(at),
    });
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, f4x_core::world::Event::Rejected(_))),
        "{events:?}"
    );
}

#[test]
fn a_battle_at_a_walled_city_is_fought_round_its_walls() {
    let (mut w, city) = siege_world(6, &[35 /* city_walls */]);
    start_siege(&mut w, city);
    let b = w.battle().expect("a battle is on");
    assert!(b.map.is_walled());
    assert!(!b.map.standing_walls().is_empty(), "the walls stand");
    // The same city without walls: a city battle, but nothing to climb.
    let (mut w, city) = siege_world(6, &[]);
    start_siege(&mut w, city);
    let b = w.battle().expect("a battle is on");
    assert!(b.map.is_city() && !b.map.is_walled());
    assert!(b.map.standing_walls().is_empty(), "no walls to break");
}

#[test]
fn taking_a_city_loots_gold_costs_fame_and_hurts_the_city() {
    let (mut w, city) = siege_world(12, &[26 /* marketplace */, 16 /* library */]);
    let loser = PlayerId(1);
    for (p, gold, fame) in [(HUMAN, 10, 0), (loser, 200, 5)] {
        w.treasury_mut(p).expect("the player has a treasury").gold =
            f4x_def::half_int::HalfInt::from_int(gold);
        w.set_fame(p, fame);
    }
    // The rival's other cities share the gold out by citizens: the capital is all of theirs.
    let total: u32 = w
        .economy()
        .cities()
        .iter()
        .filter(|(_, c, _)| c.owner == loser)
        .map(|(e, ..)| {
            w.component::<f4x_core::city::economy::Population>(*e)
                .expect("the entity has it")
                .figures
        })
        .sum();
    assert_eq!(total, 12);
    assert_eq!(w.economy().conquest_loot(city), 200);
    let events = w.conquer_city(city, HUMAN, 40);
    let looted = events
        .iter()
        .find_map(|e| match e {
            f4x_core::world::Event::CityLooted {
                gold,
                citizens_lost,
                buildings_lost,
                ..
            } => Some((*gold, *citizens_lost, buildings_lost.len() as u32)),
            _ => None,
        })
        .expect("the city was looted");
    assert_eq!(looted.0, 200);
    // The event carries the fame the results scroll reports.
    assert!(events.iter().any(|e| matches!(
        e,
        f4x_core::world::Event::CityLooted {
            fame_lost: 3,
            fame_gained: 1,
            ..
        }
    )));
    assert_eq!(
        w.player(HUMAN).expect("the player exists").treasury().gold,
        210
    );
    assert_eq!(
        w.player(loser).expect("the player exists").treasury().gold,
        0
    );
    // A city of 12 is size 3: the loser loses 3 fame (5 -> 2), the taker gains 1, and taking a
    // wizard's fortress is worth the fortress prize more.
    assert_eq!(w.wizard(loser).expect("the player is a wizard").fame(), 2);
    assert_eq!(
        w.wizard(HUMAN).expect("the player is a wizard").fame(),
        1 + w.content().rules.config.renown.fortress_prize_fame
    );
    // It is the taker's, with 50 percent as the most citizens lost and the rest alive.
    assert_eq!(w.economy().city_owner(city), Some(HUMAN));
    let now = w
        .component::<f4x_core::city::economy::Population>(city)
        .expect("the entity has it")
        .figures;
    assert_eq!(now, 12 - looted.1);
    assert!(now >= 1);
    assert!(w
        .component::<f4x_core::city::conquest::Conquered>(city)
        .is_some());
}

#[test]
fn more_city_damage_means_more_lost_citizens_and_buildings() {
    let lost = |damage| {
        let mut citizens = 0;
        let mut buildings = 0;
        for seed in 0..10u64 {
            let (mut w, city) = siege_world(
                17,
                &[
                    26, /* marketplace */
                    16, /* library */
                    29, /* granary */
                    22, /* shrine */
                    15, /* sawmill */
                    23, /* temple */
                ],
            );
            // Windows of the generator that do not overlap, so the samples are independent.
            for _ in 0..seed * 50 {
                w.next_u64();
            }
            let before = w
                .component::<f4x_core::city::economy::Buildings>(city)
                .expect("the entity has it")
                .0
                .len();
            let events = w.conquer_city(city, HUMAN, damage);
            for e in events {
                if let f4x_core::world::Event::CityLooted {
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
    let (mut w, city) = neutral_world(0, &[]);
    let events = w.conquer_city(city, HUMAN, 0);
    assert!(events
        .iter()
        .any(|e| matches!(e, f4x_core::world::Event::CityDestroyed { .. })));
    assert!(w.component::<f4x_core::city::City>(city).is_none());
}

#[test]
fn a_taken_city_can_be_razed_until_the_turn_ends() -> TestResult {
    let (mut w, city) = neutral_world(12, &[26 /* marketplace */, 16 /* library */]);
    w.set_fame(HUMAN, 10);
    w.treasury_mut(HUMAN)
        .expect("the player has a treasury")
        .gold = f4x_def::half_int::HalfInt::from_int(0);
    w.conquer_city(city, HUMAN, 0);
    let fame_after_taking = w.wizard(HUMAN).expect("the player is a wizard").fame();
    let gold_after_taking = w.player(HUMAN).expect("the player exists").treasury().gold;
    // Only the taker may raze it.
    let events = w.apply(f4x_core::world::Command::RazeCity {
        player: PlayerId(2),
        city,
    });
    // (Taking the city of a neutral player also made contact and war with it: those events wait
    // in the queue.)
    let rejected: Vec<_> = events
        .iter()
        .filter(|e| !matches!(e, f4x_core::world::Event::Diplomacy(_)))
        .collect();
    assert!(
        matches!(rejected[..], [f4x_core::world::Event::Rejected(_)]),
        "{events:?}"
    );
    // The taker razes it: the size in fame is lost (the fighting leaves a size that
    // depends on the dice), and a tenth of each building's cost is looted.
    let size = f4x_core::city::conquest::city_size_class(
        &w.content().rules,
        w.component::<f4x_core::city::economy::Population>(city)
            .expect("the entity has it")
            .figures,
    ) as i32;
    let events = w.apply(f4x_core::world::Command::RazeCity {
        player: HUMAN,
        city,
    });
    assert!(
        events
            .iter()
            .any(|e| matches!(e, f4x_core::world::Event::CityRazed { .. })),
        "{events:?}"
    );
    assert!(w.component::<f4x_core::city::City>(city).is_none());
    assert_eq!(
        w.wizard(HUMAN).expect("the player is a wizard").fame(),
        (fame_after_taking - size).max(0)
    );
    assert!(w.player(HUMAN).expect("the player exists").treasury().gold > gold_after_taking);

    // Wait a turn and the window is shut.
    let (mut w, city) = neutral_world(12, &[]);
    w.conquer_city(city, HUMAN, 0);
    let snap = w.snapshot();
    let mut back = Game::new(snap.restore(w.content().clone())?);
    let again = conquered_city(&back);
    assert!(
        back.component::<f4x_core::city::conquest::Conquered>(again)
            .is_some(),
        "the chance to raze survives a save"
    );
    back.play_round();
    let events = back.apply(f4x_core::world::Command::RazeCity {
        player: HUMAN,
        city: again,
    });
    assert!(
        matches!(events[..], [f4x_core::world::Event::Rejected(_)]),
        "{events:?}"
    );
    assert!(back.component::<f4x_core::city::City>(again).is_some());
    Ok(())
}

#[test]
fn razing_a_wizards_fortress_city_leaves_the_wizard_banished_and_the_winner_unharmed() -> TestResult
{
    // Three wizards, so that the game goes on when one is out.
    let content = content();
    let mut w = new_game(
        content,
        &NewGame {
            seed: 7,
            ..NewGame::with_wizards(3)
        },
    )?;
    let loser = PlayerId(1);
    let city = w.economy().fortress_city(loser).expect("a fortress city");
    let own_fortress = w.wizard(HUMAN).expect("the player is a wizard").fortress();
    assert!(own_fortress.is_some());
    w.conquer_city(city, HUMAN, 0);
    // The fortress fell at the taking, not at the razing.
    assert!(w
        .wizard(loser)
        .expect("the player is a wizard")
        .fortress()
        .is_none());
    assert!(
        w.wizard(loser)
            .expect("the player is a wizard")
            .is_banished()
            || w.player(loser).expect("the player exists").is_defeated(),
        "banished or out"
    );
    let events = w.apply(f4x_core::world::Command::RazeCity {
        player: HUMAN,
        city,
    });
    assert!(
        events
            .iter()
            .any(|e| matches!(e, f4x_core::world::Event::CityRazed { .. })),
        "{events:?}"
    );
    assert!(w.component::<f4x_core::city::City>(city).is_none());
    // The winner keeps their fortress, circle and place in the game.
    assert_eq!(
        w.wizard(HUMAN).expect("the player is a wizard").fortress(),
        own_fortress
    );
    assert!(!w
        .wizard(HUMAN)
        .expect("the player is a wizard")
        .is_banished());
    assert!(!w.player(HUMAN).expect("the player exists").is_defeated());
    Ok(())
}

/// The one city marked as just taken (found again after a save: entities change).
fn conquered_city(w: &World) -> hecs::Entity {
    w.economy()
        .cities()
        .into_iter()
        .map(|(e, ..)| e)
        .find(|&e| w.city_conquered(e))
        .expect("it is there")
}

#[test]
fn an_auto_resolved_siege_of_a_walled_city_ends_and_may_take_it() {
    let (mut taken, mut held) = (0, 0);
    for seed in 0..4u64 {
        let (mut w, city) = neutral_world(8, &[35 /* city_walls */, 3 /* barracks */]);
        for _ in 0..seed * 50 {
            w.next_u64();
        }
        let at = *w
            .component::<f4x_core::map::Position>(city)
            .expect("the entity has it");
        let from = f4x_core::map::Position { x: at.x + 1, ..at };
        let giant = w
            .content()
            .rules
            .units
            .id_numbered(193)
            .expect("the shipped content has it");
        for _ in 0..3 {
            w.apply(f4x_core::world::Command::CreateUnit {
                owner: HUMAN,
                pos: from,
                unit: giant,
            });
        }
        for u in w.units().player_units(HUMAN) {
            w.cheat_move_unit(u, from);
        }
        let garrison = w.units().units_at(at);
        let besiegers = w.units().player_units(HUMAN);
        let events = w.auto_resolve(&besiegers, &garrison);
        assert!(!w.in_battle(), "the siege ended");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, f4x_core::world::Event::BattleEnded { .. })),
            "{events:?}"
        );
        if w.economy().city_owner(city) == Some(HUMAN) {
            taken += 1;
            assert!(events
                .iter()
                .any(|e| matches!(e, f4x_core::world::Event::CityLooted { .. })));
        } else {
            held += 1;
        }
    }
    assert!(taken + held == 4);
}

// ---- casting in battle ----------------------------------------------------------------

/// A siege of the rival's capital in which the human wizard knows `spells`, with plenty of
/// mana and skill; returns the game.
fn casting_siege(spells: &[u16]) -> Game {
    let (mut w, city) = siege_world(6, &[]);
    arm_human(&mut w, spells);
    start_siege(&mut w, city);
    w
}

/// The human wizard knows `spells` and has the mana and skill to cast them.
fn arm_human(w: &mut Game, spells: &[u16]) {
    let ids: Vec<_> = spells
        .iter()
        .map(|s| {
            w.content()
                .rules
                .spells
                .id_numbered(*s)
                .expect("every spell given is a shipped one")
        })
        .collect();
    w.treasury_mut(HUMAN)
        .expect("the player has a treasury")
        .mana = f4x_def::half_int::HalfInt::from_int(5000);
    let mut wizard = w.wizard_mut(HUMAN).expect("the player is a wizard");
    wizard.get_mut::<f4x_core::wizard::sheet::Skill>().points = 20_000;
    wizard
        .get_mut::<f4x_core::wizard::sheet::KnownSpells>()
        .known
        .extend(ids);
}

fn cast(w: &mut Game, line: &str) -> crate::Reply {
    let mut c = console();
    c.execute(w, line)
}

#[test]
fn units_list_and_cast_their_own_spells_by_text() {
    let mut w = casting_siege(&[83 /* fire_bolt */]);
    let mine = battle_units_of(&w, f4x_battle::Side::Attacker)[0];
    let r = cast(&mut w, &format!("uspells {mine}"));
    assert!(r.ok, "{}", r.text());
    assert!(r.text().contains("mana"), "{}", r.text());
    // A plain unit knows no spell of its own: the order is refused, not misread.
    let r = cast(&mut w, &format!("ucast {mine} fire bolt unit:0 mana:10"));
    assert!(!r.ok, "{}", r.text());
    assert!(cast(&mut w, "ucast 99 fire bolt").error.is_some());
    assert!(cast(&mut w, "blegal").ok);
}

/// The human side ends its turn and the rival's does too, so the human wizard casts again.
fn pass_turn(w: &mut Game) {
    let r = cast(w, "bend");
    assert!(r.ok, "{}", r.text());
    w.apply(f4x_core::world::Command::Battle(
        f4x_battle::battle::BattleCommand::EndTurn,
    ));
}

fn battle_units_of(w: &World, side: f4x_battle::Side) -> Vec<usize> {
    w.battle()
        .expect("a battle is on")
        .units
        .iter()
        .filter(|u| u.side == side)
        .map(|u| u.id)
        .collect()
}

#[test]
fn a_wizard_goes_into_battle_with_skill_mana_and_a_range_cost() {
    use f4x_battle::Side;
    let w = casting_siege(&[83 /* fire_bolt */]);
    let b = w.battle().expect("a battle is on");
    let mine = b.caster(Side::Attacker).expect("the human wizard casts");
    assert_eq!(
        mine.skill_left,
        w.wizard(HUMAN)
            .expect("the player is a wizard")
            .casting_skill(&w.content().rules)
    );
    assert_eq!(mine.mana, 5000);
    assert!(mine.range >= 5 && mine.range <= 30 && mine.range % 5 == 0);
    assert!(b.caster(Side::Defender).is_some(), "so does the rival");
    // Independent cities have no wizard.
    let (mut w, city) = neutral_world(6, &[]);
    start_siege(&mut w, city);
    assert!(w
        .battle()
        .expect("a battle is on")
        .caster(Side::Defender)
        .is_none());
}

#[test]
fn a_spell_costs_skill_and_range_priced_mana_and_only_one_a_turn() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[83 /* fire_bolt */]);
    let enemy = battle_units_of(&w, Side::Defender)[0];
    let before = *w
        .battle()
        .expect("a battle is on")
        .caster(Side::Attacker)
        .expect("the side has a caster");
    let r = cast(&mut w, &format!("bcast fire bolt unit:{enemy}"));
    assert!(r.ok, "{}", r.text());
    let after = *w
        .battle()
        .expect("a battle is on")
        .caster(Side::Attacker)
        .expect("the side has a caster");
    // Fire Bolt costs 5; the mana is that times the range multiplier.
    assert_eq!(before.skill_left - after.skill_left, 5);
    assert_eq!(before.mana - after.mana, 5 * before.range / 10);
    assert_eq!(
        w.player(HUMAN).expect("the player exists").treasury().mana,
        after.mana,
        "the reserve pays"
    );
    assert!(r.text().contains("casts fire_bolt"), "{}", r.text());
    // A second spell in the same turn is refused; the next turn allows one.
    let again = cast(&mut w, &format!("bcast fire bolt unit:{enemy}"));
    assert!(!again.ok, "{}", again.text());
    pass_turn(&mut w);
    assert_eq!(w.battle().expect("a battle is on").active, Side::Attacker);
    assert!(cast(&mut w, &format!("bcast fire bolt unit:{enemy}")).ok);
}

#[test]
fn a_battle_spell_is_refused_for_the_reason_it_cannot_be_cast() {
    use f4x_battle::Side;
    // Warp Wood needs a target with missiles, Earth Lore is overland only, Fireball is not known.
    let mut w = casting_siege(&[81 /* warp_wood */, 10 /* earth_lore */]);
    let enemy = battle_units_of(&w, Side::Defender)[0];
    for (spell, why) in [
        ("warp wood", "NeedsMissile"),
        ("earth lore", "NotForBattle"),
        ("fireball", "NotKnown"),
    ] {
        let r = cast(&mut w, &format!("bcast {spell} unit:{enemy}"));
        assert!(!r.ok, "{spell}");
        assert!(r.text().contains(why), "{spell}: {}", r.text());
    }
    let offered = cast(&mut w, "bspells").text();
    assert!(
        !offered.contains("warp wood"),
        "a spell with no legal target is not offered: {offered}"
    );
}

#[test]
fn a_spell_that_costs_more_than_the_wizard_has_is_refused() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[83 /* fire_bolt */]);
    let enemy = battle_units_of(&w, Side::Defender)[0];
    // A reserve too small for the mana the spell takes at this range.
    w.battle
        .as_mut()
        .expect("the value is there")
        .battle
        .casters[Side::Attacker.index()]
    .as_mut()
    .expect("the value is there")
    .mana = f4x_def::half_int::HalfInt::from_int(1);
    let r = cast(&mut w, &format!("bcast fire bolt unit:{enemy}"));
    assert!(!r.ok, "{}", r.text());
    assert!(
        !w.battle()
            .expect("a battle is on")
            .caster(Side::Attacker)
            .expect("the side has a caster")
            .cast_this_turn
    );
    assert!(
        cast(&mut w, "bspells").text().contains("cannot cast now"),
        "the list says so"
    );
}

#[test]
fn counter_magic_stops_a_spell_that_still_costs_its_mana() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[83 /* fire_bolt */]);
    let enemy = battle_units_of(&w, Side::Defender)[0];
    w.battle
        .as_mut()
        .expect("the value is there")
        .battle
        .cast_counter_magic(Side::Defender, 100_000);
    let hits = w
        .battle()
        .expect("a battle is on")
        .unit(enemy)
        .expect("the unit is alive")
        .hit_points();
    let mana = w.player(HUMAN).expect("the player exists").treasury().mana;
    let r = cast(&mut w, &format!("bcast fire bolt unit:{enemy}"));
    assert!(r.ok, "{}", r.text());
    assert!(r.text().contains("Counter Magic stops"), "{}", r.text());
    assert_eq!(
        w.battle()
            .expect("a battle is on")
            .unit(enemy)
            .expect("the unit is alive")
            .hit_points(),
        hits
    );
    assert!(
        w.player(HUMAN).expect("the player exists").treasury().mana < mana,
        "the mana is spent"
    );
    assert_eq!(
        w.battle().expect("a battle is on").counter_magic[Side::Defender.index()],
        100_000 - w.content().rules.battle.counter_magic_drain,
        "every attempt drains the reserve"
    );
}

#[test]
fn counter_magic_is_a_reserve_of_the_mana_put_in() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[48 /* counter_magic */]);
    let r = cast(&mut w, "bcast counter magic mana:25");
    assert!(r.ok, "{}", r.text());
    assert_eq!(
        w.battle().expect("a battle is on").counter_magic[Side::Attacker.index()],
        25
    );
    // Less than the cost cannot be put in.
    let mut w = casting_siege(&[48 /* counter_magic */]);
    assert!(!cast(&mut w, "bcast counter magic mana:3").ok);
}

#[test]
fn a_summoned_creature_joins_the_side_and_goes_with_the_battle() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[45 /* phantom_warriors */]);
    let overland = w.entity_count();
    let before = w
        .battle()
        .expect("a battle is on")
        .active_units(Side::Attacker)
        .count();
    let r = cast(&mut w, "bcast phantom warriors tile:4,8");
    assert!(r.ok, "{}", r.text());
    let b = w.battle().expect("a battle is on");
    assert_eq!(b.active_units(Side::Attacker).count(), before + 1);
    let new = b.units.last().expect("there is one");
    assert_eq!(
        (new.side, new.cell, new.source),
        (Side::Attacker, (4, 8), None)
    );
    assert_eq!(w.entity_count(), overland, "no unit is left on the map");
    // Not on the enemy's half, nor on an occupied square.
    let mut w = casting_siege(&[45 /* phantom_warriors */]);
    assert!(!cast(&mut w, "bcast phantom warriors tile:19,5").ok);
    let own = w.battle().expect("a battle is on").units[0].cell;
    assert!(
        !cast(
            &mut w,
            &format!("bcast phantom warriors tile:{},{}", own.0, own.1)
        )
        .ok
    );
}

#[test]
fn a_lasting_spell_changes_a_unit_until_a_dispel_takes_it_off() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[4 /* giant_strength */]);
    let mine = battle_units_of(&w, Side::Attacker)[0];
    let melee = w
        .battle()
        .expect("a battle is on")
        .now(mine)
        .expect("the unit is alive")
        .melee();
    let r = cast(&mut w, &format!("bcast giant strength unit:{mine}"));
    assert!(r.ok, "{}", r.text());
    assert_eq!(
        w.battle()
            .expect("a battle is on")
            .now(mine)
            .expect("the unit is alive")
            .melee(),
        melee + 1
    );
    // Not twice on one unit, and not on the enemy's.
    pass_turn(&mut w);
    assert!(!cast(&mut w, &format!("bcast giant strength unit:{mine}")).ok);
    let enemy = battle_units_of(&w, Side::Defender)[0];
    assert!(!cast(&mut w, &format!("bcast giant strength unit:{enemy}")).ok);
}

#[test]
fn a_dispel_takes_a_spell_cast_in_the_battle_off_an_enemy_unit() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[202 /* dispel_magic */]);
    let enemy = battle_units_of(&w, Side::Defender)[0];
    let strength = w
        .content()
        .rules
        .spells
        .id_numbered(4)
        .expect("the shipped content has it");
    let rival = PlayerId(1);
    {
        let b = &mut w.battle.as_mut().expect("the value is there").battle;
        let m = |stat, value| {
            f4x_def::rules::stats::Modifier::new(
                stat,
                f4x_def::rules::stats::ModOp::Add,
                value,
                f4x_def::rules::stats::Source::Other,
            )
        };
        b.enchant_unit(
            enemy,
            strength,
            rival,
            &[m(f4x_def::rules::stats::Stat::Melee, 1)],
            &[],
        );
    }
    let melee = w
        .battle()
        .expect("a battle is on")
        .now(enemy)
        .expect("the unit is alive")
        .melee();
    // So much mana that a dispel hardly ever fails; try over a few turns.
    let mut removed = false;
    for _ in 0..6 {
        let r = cast(&mut w, &format!("bcast dispel magic unit:{enemy} mana:30"));
        assert!(r.ok, "{}", r.text());
        if w.battle()
            .expect("a battle is on")
            .unit(enemy)
            .expect("the unit is alive")
            .lasting()
            .is_empty()
        {
            removed = true;
            break;
        }
        pass_turn(&mut w);
    }
    assert!(removed, "the dispel took the spell off");
    let u = w
        .battle()
        .expect("a battle is on")
        .unit(enemy)
        .expect("the unit is alive");
    assert_eq!(u.own_melee, melee - 1);
    assert!(u.enchantments.is_empty() && u.lasting().is_empty());
}

#[test]
fn a_dispel_in_battle_also_takes_the_overland_spells_off_the_unit_it_copied() {
    use f4x_battle::Side;
    let (mut w, city) = siege_world(6, &[]);
    let rival = PlayerId(1);
    let at = *w
        .component::<f4x_core::map::Position>(city)
        .expect("the entity has it");
    let defender = w.units().units_at(at)[0];
    let spell = w
        .content()
        .rules
        .spells
        .id_numbered(4)
        .expect("the shipped content has it");
    let dispel = w
        .content()
        .rules
        .spells
        .id_numbered(202)
        .expect("the shipped content has it");
    w.scenario_enchant_unit(defender, spell, rival);
    let plain = w.units().unit_stat(defender, f4x_def::rules::Stat::Melee) - 1;
    {
        w.treasury_mut(HUMAN)
            .expect("the player has a treasury")
            .mana = f4x_def::half_int::HalfInt::from_int(5000);
        let mut wizard = w.wizard_mut(HUMAN).expect("the player is a wizard");
        wizard.get_mut::<f4x_core::wizard::sheet::Skill>().points = 20_000;
        wizard
            .get_mut::<f4x_core::wizard::sheet::KnownSpells>()
            .known
            .insert(dispel);
    }
    start_siege(&mut w, city);
    let enemy = w
        .battle()
        .expect("a battle is on")
        .units
        .iter()
        .position(|u| {
            u.side == Side::Defender
                && u.own_melee == plain + 1
                && u.enchantments
                    .contains(&f4x_def::rules::SpellId::new(4) /* giant_strength */)
        })
        .expect("the copy carries the spell");
    let mut gone = false;
    for _ in 0..6 {
        let r = cast(&mut w, &format!("bcast dispel magic unit:{enemy} mana:30"));
        assert!(r.ok, "{}", r.text());
        if w.units().unit_enchantments(defender).is_empty() {
            gone = true;
            break;
        }
        pass_turn(&mut w);
    }
    assert!(gone, "the overland unit lost the spell");
    assert_eq!(
        w.battle()
            .expect("a battle is on")
            .now(enemy)
            .expect("the unit is alive")
            .melee(),
        plain
    );
}

#[test]
fn a_bolt_hurts_by_its_strength_and_star_fires_only_burns_chaos_and_death() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[
        104, /* doom_bolt */
        122, /* star_fires */
        125, /* healing */
    ]);
    let enemy = battle_units_of(&w, Side::Defender)[0];
    {
        // A tough target, so the bolt does not overkill.
        let u = &mut w.battle.as_mut().expect("the value is there").battle.units[enemy];
        u.force(f4x_def::rules::stats::Stat::MaxHp, 50);
        u.set_figures(1);
        u.set_front_damage(0);
    }
    let before = w
        .battle()
        .expect("a battle is on")
        .unit(enemy)
        .expect("the unit is alive")
        .hit_points();
    let r = cast(&mut w, &format!("bcast doom bolt unit:{enemy}"));
    assert!(r.ok, "{}", r.text());
    let after = w
        .battle()
        .expect("a battle is on")
        .unit(enemy)
        .expect("the unit is alive")
        .hit_points();
    assert_eq!(
        before - after,
        10,
        "doom damage ignores the defence: {}",
        r.text()
    );
    // Star Fires: not on ordinary units.
    pass_turn(&mut w);
    assert!(!cast(&mut w, &format!("bcast star fires unit:{enemy}")).ok);
    // Healing: one's own units only, and it brings figures back.
    let mine = battle_units_of(&w, Side::Attacker)[0];
    w.battle.as_mut().expect("the value is there").battle.units[mine].set_front_damage(1);
    assert!(!cast(&mut w, &format!("bcast healing unit:{enemy}")).ok);
    let r = cast(&mut w, &format!("bcast healing unit:{mine}"));
    assert!(r.ok, "{}", r.text());
    assert_eq!(
        w.battle()
            .expect("a battle is on")
            .unit(mine)
            .expect("the unit is alive")
            .front_damage,
        0
    );
}

#[test]
fn a_spell_that_kills_the_last_enemy_ends_the_battle() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[104 /* doom_bolt */]);
    let enemies = battle_units_of(&w, Side::Defender);
    // Leave one frail defender.
    {
        let b = &mut w.battle.as_mut().expect("the value is there").battle;
        for &e in &enemies[1..] {
            b.units[e].status = f4x_battle::Status::Dead;
        }
        b.units[enemies[0]].force(f4x_def::rules::stats::Stat::MaxHp, 3);
        b.units[enemies[0]].set_figures(1);
        b.units[enemies[0]].set_front_damage(0);
    }
    let r = cast(&mut w, &format!("bcast doom bolt unit:{}", enemies[0]));
    assert!(r.ok, "{}", r.text());
    assert!(!w.in_battle(), "the battle is over: {}", r.text());
    assert!(r.text().contains("Battle over"), "{}", r.text());
}

#[test]
fn a_unit_standing_on_hit_points_a_spell_added_is_dead_when_the_battle_is_over() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[141 /* lionheart */]);
    // Fleeing may lose the unit by chance; this test is about its hit points, not that roll.
    w.scenario_set_safe_flee(true);
    let mine = battle_units_of(&w, Side::Attacker)[0];
    let entity = {
        let a = w.battle.as_ref().expect("the value is there");
        a.sources[a
            .battle
            .unit(mine)
            .expect("the unit is alive")
            .source
            .expect("the fighter has a source")]
    };
    let r = cast(&mut w, &format!("bcast lionheart unit:{mine}"));
    assert!(r.ok, "{}", r.text());
    {
        let u = &mut w.battle.as_mut().expect("the value is there").battle.units[mine];
        assert!(u.hits > 1 && u.figures_start > 1, "{u:?}");
        // One figure left, and all its hit points are the spell's or less (divergence 45).
        u.set_figures(1);
        let own = u.max_hit_points_without_lasting();
        let spell_gave = u.max_hit_points() - own;
        assert!(u.hit_points() <= spell_gave, "{u:?}");
    }
    assert!(cast(&mut w, "bflee").ok);
    assert!(!w.in_battle());
    assert!(
        w.units().unit_position(entity).is_none(),
        "the hit points were the spell's, and they went with the battle"
    );
}

// ---- battlefield and destroying spells (tasks 44 and 159) --------------------------------

#[test]
fn a_battlefield_enchantment_goes_up_once_and_changes_the_units() {
    use f4x_battle::battlemagic::field::Field;
    use f4x_battle::Side;
    let mut w = casting_siege(&[151 /* high_prayer */]);
    let mine = battle_units_of(&w, Side::Attacker)[0];
    let defense = w
        .battle()
        .expect("a battle is on")
        .now(mine)
        .expect("the unit is alive")
        .defense();
    let enemy = battle_units_of(&w, Side::Defender)[0];
    let enemy_block = w
        .battle()
        .expect("a battle is on")
        .now(enemy)
        .expect("the unit is alive")
        .to_block();
    let r = cast(&mut w, "bcast high prayer");
    assert!(r.ok, "{}", r.text());
    assert!(r.text().contains("raises high_prayer"), "{}", r.text());
    let b = w.battle().expect("a battle is on");
    assert!(b.has_field(Side::Attacker, Field::HighPrayer));
    assert_eq!(
        b.now(mine).expect("the unit is alive").defense(),
        defense + 2
    );
    // Not again; and the enemy's units are not blessed.
    pass_turn(&mut w);
    assert!(!cast(&mut w, "bcast high prayer").ok);
    assert_eq!(
        w.battle()
            .expect("a battle is on")
            .now(enemy)
            .expect("the unit is alive")
            .to_block(),
        enemy_block
    );
}

#[test]
fn mana_leak_drains_the_rivals_reserve_in_the_game_too() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[170 /* mana_leak */]);
    w.treasury_mut(PlayerId(1))
        .expect("the player has a treasury")
        .mana = f4x_def::half_int::HalfInt::from_int(100);
    // The rival's wizard in the battle has its reserve from the game.
    w.battle
        .as_mut()
        .expect("the value is there")
        .battle
        .casters[Side::Defender.index()]
    .as_mut()
    .expect("the value is there")
    .mana = f4x_def::half_int::HalfInt::from_int(100);
    assert!(cast(&mut w, "bcast mana leak").ok);
    pass_turn(&mut w);
    // The rival's turn began a new turn: the attacker's.
    assert_eq!(
        w.player(PlayerId(1))
            .expect("the player exists")
            .treasury()
            .mana,
        100 - w.content().rules.battle.mana_leak_drain,
    );
    assert_eq!(
        w.battle()
            .expect("a battle is on")
            .caster(Side::Defender)
            .expect("the side has a caster")
            .mana,
        100 - w.content().rules.battle.mana_leak_drain
    );
}

#[test]
fn flame_strike_hits_every_enemy_and_mass_healing_every_friend() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[109 /* flame_strike */, 149 /* mass_healing */]);
    let enemies = battle_units_of(&w, Side::Defender);
    for &e in &enemies {
        let u = &mut w.battle.as_mut().expect("the value is there").battle.units[e];
        u.force(f4x_def::rules::stats::Stat::MaxHp, 100);
        u.force(f4x_def::rules::stats::Stat::Defense, 0);
        u.set_figures(1);
        u.set_front_damage(0);
    }
    let r = cast(&mut w, "bcast flame strike");
    assert!(r.ok, "{}", r.text());
    let hurt = enemies
        .iter()
        .filter(|&&e| {
            w.battle()
                .expect("a battle is on")
                .unit(e)
                .expect("the unit is alive")
                .hit_points()
                < 100
        })
        .count();
    assert!(hurt >= 1, "a strength-15 fire attack on each: {}", r.text());
    let mine = battle_units_of(&w, Side::Attacker);
    for &m in &mine {
        w.battle.as_mut().expect("the value is there").battle.units[m].set_front_damage(1);
    }
    pass_turn(&mut w);
    let r = cast(&mut w, "bcast mass healing");
    assert!(r.ok, "{}", r.text());
    for &m in &mine {
        assert_eq!(
            w.battle()
                .expect("a battle is on")
                .unit(m)
                .expect("the unit is alive")
                .front_damage,
            0,
            "{m}"
        );
    }
}

#[test]
fn mass_healing_still_heals_under_true_light() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[128 /* true_light */, 149 /* mass_healing */]);
    let r = cast(&mut w, "bcast true light");
    assert!(r.ok, "{}", r.text());
    let mine = battle_units_of(&w, Side::Attacker);
    for &m in &mine {
        w.battle.as_mut().expect("the value is there").battle.units[m].set_front_damage(1);
    }
    pass_turn(&mut w);
    let r = cast(&mut w, "bcast mass healing");
    assert!(r.ok, "{}", r.text());
    for &m in &mine {
        assert_eq!(
            w.battle()
                .expect("a battle is on")
                .unit(m)
                .expect("the unit is alive")
                .front_damage,
            0,
            "{m}"
        );
    }
}

#[test]
fn death_spell_and_word_of_death_destroy_the_figures_that_cannot_resist() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[194 /* death_spell */, 192 /* word_of_death */]);
    let enemies = battle_units_of(&w, Side::Defender);
    for &e in &enemies {
        w.battle.as_mut().expect("the value is there").battle.units[e]
            .force(f4x_def::rules::stats::Stat::Resistance, 0);
    }
    // One enemy that cannot fall keeps the battle going.
    let last = *enemies.last().expect("there is one");
    w.battle.as_mut().expect("the value is there").battle.units[last].grant(Ability::new(16));
    let r = cast(&mut w, "bcast death spell");
    assert!(r.ok, "{}", r.text());
    let b = w.battle().expect("a battle is on");
    let dead = enemies
        .iter()
        .filter(|&&e| !b.unit(e).expect("the unit is alive").is_active())
        .count();
    // A penalty of two against resistance zero: a ten-sided die over two kills a figure about
    // eight times in ten, so most of every unit goes.
    let left: u32 = enemies
        .iter()
        .map(|&e| b.unit(e).expect("the unit is alive").figures)
        .sum();
    let start: u32 = enemies
        .iter()
        .map(|&e| b.unit(e).expect("the unit is alive").figures_start)
        .sum();
    assert!(dead > 0 || left < start, "{}", r.text());
}

#[test]
fn a_curse_can_be_resisted_and_banish_only_touches_fantastic_creatures() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[
        162, /* weakness */
        64,  /* banish */
        67,  /* mind_storm */
    ]);
    let enemy = battle_units_of(&w, Side::Defender)[0];
    // Banish needs a fantastic creature.
    assert!(!cast(&mut w, &format!("bcast banish unit:{enemy}")).ok);
    // A unit with no resistance cannot save against Weakness.
    w.battle.as_mut().expect("the value is there").battle.units[enemy]
        .force(f4x_def::rules::stats::Stat::Resistance, 0);
    let melee = w
        .battle()
        .expect("a battle is on")
        .now(enemy)
        .expect("the unit is alive")
        .melee();
    let mut weakened = false;
    for _ in 0..8 {
        let r = cast(&mut w, &format!("bcast weakness unit:{enemy}"));
        assert!(r.ok, "{}", r.text());
        if w.battle()
            .expect("a battle is on")
            .now(enemy)
            .expect("the unit is alive")
            .melee()
            < melee
        {
            weakened = true;
            break;
        }
        pass_turn(&mut w);
    }
    assert!(weakened, "an unresisting unit is weakened soon enough");
    // Mind Storm: no save, and illusion immunity refuses it before it costs mana.
    let other = battle_units_of(&w, Side::Defender)[1];
    w.battle.as_mut().expect("the value is there").battle.units[other].grant(Ability::new(31));
    pass_turn(&mut w);
    let mana = w
        .player(f4x_core::player::PlayerId(0))
        .map(|p| p.treasury().mana);
    let r = cast(&mut w, &format!("bcast mind storm unit:{other}"));
    assert!(!r.ok, "{}", r.text());
    assert_eq!(
        w.player(f4x_core::player::PlayerId(0))
            .map(|p| p.treasury().mana),
        mana
    );
}

#[test]
fn shatter_warp_wood_and_warp_creature_harm_a_unit_for_the_battle() {
    use f4x_battle::battle::RangedKind;
    use f4x_battle::Side;
    let mut w = casting_siege(&[
        88, /* shatter */
        81, /* warp_wood */
        89, /* warp_creature */
    ]);
    let [a, b] = [0, 1].map(|i| battle_units_of(&w, Side::Defender)[i]);
    {
        let units = &mut w.battle.as_mut().expect("the value is there").battle.units;
        units[a].force(f4x_def::rules::stats::Stat::Resistance, 0);
        units[a].force(f4x_def::rules::stats::Stat::Melee, 5);
        units[a].force(f4x_def::rules::stats::Stat::Ranged, 3);
        units[a].put_ranged_kind(RangedKind::Missile);
        units[a].set_ammo(4);
        units[b].force(f4x_def::rules::stats::Stat::Resistance, 0);
        units[b].force(f4x_def::rules::stats::Stat::Melee, 6);
        units[b].force(f4x_def::rules::stats::Stat::Defense, 4);
    }
    // Warp Wood takes the arrows, and refuses a unit that has none left.
    let r = cast(&mut w, &format!("bcast warp wood unit:{a}"));
    assert!(r.ok, "{}", r.text());
    assert_eq!(
        w.battle()
            .expect("a battle is on")
            .unit(a)
            .expect("the unit is alive")
            .ammo,
        0
    );
    pass_turn(&mut w);
    assert!(!cast(&mut w, &format!("bcast warp wood unit:{a}")).ok);
    // Shatter: attacks drop to one, though the unit gets a resistance roll.
    let mut shattered = false;
    for _ in 0..8 {
        let r = cast(&mut w, &format!("bcast shatter unit:{a}"));
        assert!(r.ok, "{}", r.text());
        let u = w
            .battle()
            .expect("a battle is on")
            .unit(a)
            .expect("the unit is alive");
        if u.own_melee == 1 {
            assert_eq!(u.own_ranged, 1);
            shattered = true;
            break;
        }
        pass_turn(&mut w);
    }
    assert!(shattered, "an unresisting unit is shattered soon enough");
    // Warp Creature: one of three harms, once.
    let (melee, defense, resistance) = {
        let u = w
            .battle()
            .expect("a battle is on")
            .unit(b)
            .expect("the unit is alive");
        (u.own_melee, u.own_defense, u.own_resistance)
    };
    let mut warped = false;
    for _ in 0..8 {
        pass_turn(&mut w);
        let r = cast(&mut w, &format!("bcast warp creature unit:{b}"));
        assert!(r.ok, "{}", r.text());
        let u = w
            .battle()
            .expect("a battle is on")
            .unit(b)
            .expect("the unit is alive");
        if u.own_melee < melee || u.own_defense < defense {
            warped = true;
            break;
        }
        // A zero resistance cannot be told from the third harm: look at the spell list.
        if u.warp == Some(f4x_battle::WarpKind::Resistance) {
            assert_eq!(u.own_resistance, resistance.min(0));
            warped = true;
            break;
        }
    }
    assert!(warped, "an unresisting unit is warped soon enough");
    pass_turn(&mut w);
    assert!(
        !cast(&mut w, &format!("bcast warp creature unit:{b}")).ok,
        "a unit is warped once"
    );
}

#[test]
fn creature_binding_and_possession_turn_a_unit_for_the_battle() {
    use f4x_battle::Side;
    use f4x_def::rules::realm::Realm;
    // Two rival units: `a` a normal one, `b` a fantastic one, neither with resistance. The
    // spell is cast until it takes (they get a roll), the other unit keeping the fight going.
    for (spell, name, turned, other) in [
        (73 /* creature_binding */, "creature binding", 1, 0),
        (172 /* possession */, "possession", 0, 1),
    ] {
        let mut w = casting_siege(&[spell]);
        let units = battle_units_of(&w, Side::Defender);
        let (a, b) = (units[0], units[1]);
        {
            let units = &mut w.battle.as_mut().expect("the value is there").battle.units;
            units[a].force(f4x_def::rules::stats::Stat::Resistance, 0);
            units[b].force(f4x_def::rules::stats::Stat::Resistance, 0);
            (units[a].realm, units[b].realm) = (None, Some(Realm::Chaos));
        }
        let (target, spared) = ([a, b][turned], [a, b][other]);
        // The wrong kind of unit is refused.
        assert!(!cast(&mut w, &format!("bcast {name} unit:{spared}")).ok);
        let mut done = false;
        for _ in 0..12 {
            let r = cast(&mut w, &format!("bcast {name} unit:{target}"));
            assert!(r.ok, "{}", r.text());
            let u = w
                .battle()
                .expect("a battle is on")
                .unit(target)
                .expect("the unit is alive");
            if u.side == Side::Attacker {
                assert_eq!(u.bound_from, Some(Side::Defender));
                done = true;
                break;
            }
            pass_turn(&mut w);
        }
        assert!(done, "{spell}: a unit with no resistance turns soon enough");
        assert_eq!(
            w.battle()
                .expect("a battle is on")
                .unit(spared)
                .expect("the unit is alive")
                .side,
            Side::Defender
        );
    }
}

#[test]
fn call_chaos_and_invisibility_cast_and_the_spell_list_offers_them() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[115 /* call_chaos */, 62 /* invisibility */]);
    let list = cast(&mut w, "bspells").text();
    assert!(
        list.contains("call_chaos") && list.contains("invisibility"),
        "{list}"
    );
    let mine = battle_units_of(&w, Side::Attacker)[0];
    assert!(cast(&mut w, &format!("bcast invisibility unit:{mine}")).ok);
    let battle = w.battle().expect("a battle is on");
    assert!(battle.now(mine).expect("the unit is alive").is_invisible());
    pass_turn(&mut w);
    let r = cast(&mut w, "bcast call chaos");
    assert!(r.ok, "{}", r.text());
}

fn meet_all(w: &mut Game) -> Vec<PlayerId> {
    let others: Vec<PlayerId> = w.wizards().into_iter().filter(|&id| id != HUMAN).collect();
    for &id in &others {
        w.make_contact(HUMAN, id);
    }
    others
}

#[test]
fn diplomacy_lists_only_the_wizards_met() {
    let (mut w, mut c) = (game(), console());
    let none = c.execute(&mut w, "diplomacy");
    assert!(none.ok, "{}", none.text());
    assert!(
        none.text().contains("met no other wizard"),
        "{}",
        none.text()
    );
    let others = meet_all(&mut w);
    let all = c.execute(&mut w, "diplomacy");
    assert!(all.ok, "{}", all.text());
    assert_eq!(all.lines.len(), others.len(), "{}", all.text());
    assert!(all.text().contains("no treaty"), "{}", all.text());
    let one = c.execute(&mut w, &format!("diplomacy {}", others[0].0));
    assert!(
        one.text().contains("you may propose: pact, alliance"),
        "{}",
        one.text()
    );
    assert!(
        c.execute(&mut w, "diplomacy 0").error.is_some(),
        "not oneself"
    );
    assert!(c.execute(&mut w, "diplomacy 200").error.is_some());
}

#[test]
fn a_proposal_waits_and_an_answer_signs_it() {
    let (mut w, mut c) = (game(), console());
    let other = meet_all(&mut w)[0];
    let number = other.0;
    let r = c.execute(&mut w, &format!("propose {number} pact"));
    assert!(r.ok, "{}", r.text());
    assert!(r.text().contains("proposes"), "{}", r.text());
    let again = c.execute(&mut w, &format!("propose {number} alliance"));
    assert!(!again.ok);
    assert!(again.text().contains("already waiting"), "{}", again.text());
    let list = c.execute(&mut w, "diplomacy");
    assert!(list.text().contains("waiting"), "{}", list.text());
    // Nothing they have proposed to us yet.
    let none = c.execute(&mut w, &format!("answer {number} yes"));
    assert!(none.text().contains("proposed nothing"), "{}", none.text());
    // The other wizard proposes to us: we accept.
    w.apply(f4x_core::world::Command::Diplomacy(
        f4x_core::diplomacy::DiplomacyCommand::Propose {
            from: other,
            to: HUMAN,
            kind: f4x_core::diplomacy::ProposalKind::Alliance,
        },
    ));
    let ok = c.execute(&mut w, &format!("answer {number} yes"));
    assert!(ok.ok, "{}", ok.text());
    assert!(ok.text().contains("accepts"), "{}", ok.text());
    assert_eq!(
        w.treaty(HUMAN, other),
        f4x_core::diplomacy::Treaty::Alliance
    );
    assert!(c
        .execute(&mut w, &format!("answer {number} maybe"))
        .error
        .is_some());
}

#[test]
fn treaties_break_and_wars_are_declared() {
    let (mut w, mut c) = (game(), console());
    let other = meet_all(&mut w)[0];
    let number = other.0;
    let not = c.execute(&mut w, &format!("breaktreaty {number}"));
    assert!(not.text().contains("not allowed"), "{}", not.text());
    w.start_treaty(HUMAN, other, f4x_core::diplomacy::Treaty::Pact);
    let broken = c.execute(&mut w, &format!("breaktreaty {number}"));
    assert!(broken.ok, "{}", broken.text());
    assert!(broken.text().contains("breaks"), "{}", broken.text());
    let war = c.execute(&mut w, &format!("declarewar {number}"));
    assert!(war.ok, "{}", war.text());
    assert!(war.text().contains("declares war"), "{}", war.text());
    assert_eq!(w.treaty(HUMAN, other), f4x_core::diplomacy::Treaty::War);
    assert!(!c.execute(&mut w, &format!("declarewar {number}")).ok);
    let peace = c.execute(&mut w, &format!("propose {number} peace"));
    assert!(peace.ok, "{}", peace.text());
}

#[test]
fn gifts_move_gold_and_spells() {
    let (mut w, mut c) = (game(), console());
    let other = meet_all(&mut w)[0];
    let number = other.0;
    let before = w.player(HUMAN).expect("the player exists").treasury().gold;
    let r = c.execute(&mut w, &format!("givegold {number} 10"));
    assert!(r.ok, "{}", r.text());
    assert_eq!(
        w.player(HUMAN).expect("the player exists").treasury().gold,
        before - 10
    );
    let broke = c.execute(&mut w, &format!("givegold {number} 99999"));
    assert!(broke.text().contains("not enough gold"), "{}", broke.text());
    let unknown = c.execute(&mut w, &format!("givespell {number} no-such-spell"));
    assert!(!unknown.ok);
    let me = w
        .wizard(HUMAN)
        .expect("the player is a wizard")
        .known()
        .into_iter()
        .next()
        .expect("there is one");
    w.wizard_mut(other)
        .expect("the player is a wizard")
        .get_mut::<f4x_core::wizard::sheet::KnownSpells>()
        .known
        .remove(&me);
    let name = w.content().rules.spells.get(me).id.clone();
    let spell = c.execute(&mut w, &format!("givespell {number} {name}"));
    assert!(spell.ok, "{}", spell.text());
    assert!(w.wizard(other).expect("the player is a wizard").knows(me));
}

#[test]
fn the_talk_commands_are_legal_ones_and_diplomacy_a_query() {
    let kind = |name: &str| crate::find(name).expect("the command exists").kind;
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
    let (mut w, _) = (game(), console());
    let mut read_only = console_for(Access::ReadOnly);
    meet_all(&mut w);
    assert!(read_only.execute(&mut w, "diplomacy").ok);
    assert!(!read_only.execute(&mut w, "propose 1 pact").ok);
}

#[test]
fn every_wizard_of_a_new_game_has_a_picture_of_its_own() {
    let w = game();
    let mut seen = std::collections::BTreeSet::new();
    for id in w
        .player_ids()
        .into_iter()
        .filter(|id| !w.is_neutral_player(*id))
    {
        let portrait = w.player(id).expect("the player exists").portrait();
        assert!(portrait < f4x_core::player::PORTRAITS);
        assert!(seen.insert(portrait), "picture {portrait} twice");
    }
}

#[test]
fn the_legal_casts_are_the_casts_the_game_accepts() -> TestResult {
    use f4x_battle::battle::BattleCommand;
    use f4x_battle::Side;
    let spells = [
        83,  /* fire_bolt */
        96,  /* fireball */
        4,   /* giant_strength */
        125, /* healing */
        48,  /* counter_magic */
        202, /* dispel_magic */
    ];
    let known: Vec<u16> = spells
        .iter()
        .copied()
        .filter(|&s| {
            casting_siege(&[])
                .content()
                .rules
                .spells
                .id_numbered(s)
                .is_some()
        })
        .collect();
    assert!(known.len() >= 3, "{known:?}");
    let w = casting_siege(&known);
    let legal = w.legal_battle_casts();
    assert!(!legal.is_empty());
    let enemy = battle_units_of(&w, Side::Defender);
    let own = battle_units_of(&w, Side::Attacker);
    for lc in &legal {
        let def = w.content().rules.spells.get(lc.spell).id.clone();
        assert!(!lc.aims.is_empty(), "{def}");
        assert!(lc.least_mana <= lc.most_mana, "{def}");
        // The first and last aim, at the least and the most mana, are accepted.
        let picks = [lc.aims[0], *lc.aims.last().expect("there is one")];
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
    let bolt = w
        .content()
        .rules
        .spells
        .id_numbered(83)
        .expect("the shipped content has it");
    let listed = legal.iter().find(|l| l.spell == bolt).expect("fire bolt");
    assert!(listed.aims.iter().all(|a| match a {
        f4x_battle::battle::SpellAim::Unit(u) => enemy.contains(u),
        _ => false,
    }));
    let mut fresh = casting_siege(&known);
    assert!(fresh
        .battle_command(BattleCommand::Cast {
            spell: bolt,
            aim: f4x_battle::battle::SpellAim::Unit(own[0]),
            mana: 0
        })
        .is_err());
    // After a cast the wizard has none left this turn.
    let mut w = casting_siege(&known);
    w.battle_command(BattleCommand::Cast {
        spell: bolt,
        aim: f4x_battle::battle::SpellAim::Unit(enemy[0]),
        mana: 0,
    })
    .map_err(|e| format!("{e:?}"))?;
    assert!(w.legal_battle_casts().is_empty());
    Ok(())
}

#[test]
fn healing_does_not_work_on_a_death_creature() {
    use f4x_battle::Side;
    use f4x_def::rules::realm::Realm;
    let mut w = casting_siege(&[125 /* healing */]);
    let own = battle_units_of(&w, Side::Attacker)[0];
    // A wounded living unit can be healed; the same unit as a death creature cannot.
    w.battle.as_mut().expect("the value is there").battle.units[own].realm = Some(Realm::Death);
    let r = cast(&mut w, &format!("bcast healing unit:{own}"));
    assert!(!r.ok, "{}", r.text());
    w.battle.as_mut().expect("the value is there").battle.units[own].realm = None;
    let r = cast(&mut w, &format!("bcast healing unit:{own}"));
    assert!(r.ok, "{}", r.text());
}

#[test]
fn blegal_lists_what_the_game_allows_and_the_listed_orders_work() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[83 /* fire_bolt */]);
    let all = cast(&mut w, "blegal");
    assert!(all.ok, "{}", all.text());
    assert!(
        all.text().contains("cast fire_bolt: mana"),
        "{}",
        all.text()
    );
    let mine = battle_units_of(&w, Side::Attacker)[0];
    let one = cast(&mut w, &format!("blegal {mine}"));
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
    let r = cast(&mut w, &format!("bmove {mine} {first}"));
    assert!(r.ok, "{} / {}", first, r.text());
    // The side that does not play gets no list.
    assert!(cast(&mut w, "bend").ok);
    let mut play = console_for(Access::Play);
    let refused = play.execute(&mut w, "blegal");
    assert!(!refused.ok, "{}", refused.text());
}

// ---- Animate Dead and the drained dead (tasks 157 and 159) ------------------------------

/// Strike one of the human side's units down by the given kinds of damage.
fn fell(w: &mut Game, id: usize, kinds: [i32; 3]) {
    let u = &mut w.battle.as_mut().expect("the value is there").battle.units[id];
    u.set_figures(0);
    u.status = f4x_battle::Status::Dead;
    u.damage_kinds = kinds;
}

#[test]
fn animate_dead_raises_a_fallen_unit_of_the_wizard_as_undead() {
    use f4x_battle::{Side, Status};
    let mut w = casting_siege(&[195 /* animate_dead */]);
    let mine = battle_units_of(&w, Side::Attacker);
    let (fallen, standing) = (mine[0], mine[1]);
    // Nobody has fallen yet.
    assert!(!cast(&mut w, &format!("bcast animate dead unit:{fallen}")).ok);
    fell(&mut w, fallen, [4, 0, 0]);
    // Not a unit that stands, nor an enemy's dead.
    assert!(!cast(&mut w, &format!("bcast animate dead unit:{standing}")).ok);
    let enemy = battle_units_of(&w, Side::Defender)[0];
    fell(&mut w, enemy, [4, 0, 0]);
    assert!(!cast(&mut w, &format!("bcast animate dead unit:{enemy}")).ok);
    let r = cast(&mut w, &format!("bcast animate dead unit:{fallen}"));
    assert!(r.ok, "{}", r.text());
    let b = w.battle().expect("a battle is on");
    let u = b.unit(fallen).expect("the unit is alive");
    assert_eq!(u.status, Status::Active);
    assert_eq!(u.figures, u.figures_start);
    assert!(u.undead && u.realm == Some(f4x_def::rules::realm::Realm::Death));
    assert!(u.has(Ability::new(16)) && u.has(Ability::new(10)));
    assert_eq!(u.side, Side::Attacker);
    let entity = w.battle.as_ref().expect("the value is there").sources
        [u.source.expect("the fighter has a source")];
    assert!(w.units().unit_is_undead(entity), "undead for good");
}

#[test]
fn raise_dead_brings_a_fallen_normal_unit_back_with_half_its_figures() {
    use f4x_battle::{Side, Status};
    use f4x_def::rules::realm::Realm;
    let mut w = casting_siege(&[137 /* raise_dead */]);
    let mine = battle_units_of(&w, Side::Attacker);
    let (fallen, fantastic) = (mine[0], mine[1]);
    {
        let b = &mut w.battle.as_mut().expect("the value is there").battle;
        b.units[fallen]
            .sheet_mut()
            .enchantments
            .push(f4x_def::rules::SpellId::new(121) /* bless */);
        b.units[fantastic].realm = Some(Realm::Chaos);
    }
    let start = w
        .battle()
        .expect("a battle is on")
        .unit(fallen)
        .expect("the unit is alive")
        .figures_start;
    fell(&mut w, fallen, [4, 0, 0]);
    fell(&mut w, fantastic, [4, 0, 0]);
    // Fantastic creatures go home when slain; destroyed units cannot return.
    assert!(!cast(&mut w, &format!("bcast raise dead unit:{fantastic}")).ok);
    let r = cast(&mut w, &format!("bcast raise dead unit:{fallen}"));
    assert!(r.ok, "{}", r.text());
    let u = w
        .battle()
        .expect("a battle is on")
        .unit(fallen)
        .expect("the unit is alive");
    assert_eq!((u.status, u.side), (Status::Active, Side::Attacker));
    assert_eq!(u.figures, start.div_ceil(2), "half, rounded up");
    assert!(
        !u.undead
            && !u
                .enchantments
                .contains(&f4x_def::rules::SpellId::new(121) /* bless */)
    );
    assert!(
        u.hit_points() < u.max_hit_points(),
        "the missing half stays missing"
    );
}

#[test]
fn animate_dead_cannot_raise_what_was_destroyed_outright() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[195 /* animate_dead */]);
    let fallen = battle_units_of(&w, Side::Attacker)[0];
    fell(&mut w, fallen, [1, 0, 6]);
    assert!(!cast(&mut w, &format!("bcast animate dead unit:{fallen}")).ok);
    // Drained to death is another matter.
    fell(&mut w, fallen, [1, 6, 0]);
    let r = cast(&mut w, &format!("bcast animate dead unit:{fallen}"));
    assert!(r.ok, "{}", r.text());
}

#[test]
fn the_units_drained_to_death_join_the_winner_as_undead() {
    use f4x_battle::battle::BattleCommand;
    use f4x_battle::Side;
    let mut w = casting_siege(&[]);
    let mine = battle_units_of(&w, Side::Attacker);
    let (drained, slain) = (mine[0], mine[1]);
    let entity_of = |w: &World, id: usize| {
        let b = w.battle.as_ref().expect("the value is there");
        b.sources[b
            .battle
            .unit(id)
            .expect("the unit is alive")
            .source
            .expect("the fighter has a source")]
    };
    let (drained_unit, slain_unit) = (entity_of(&w, drained), entity_of(&w, slain));
    for &id in &mine {
        fell(&mut w, id, [3, 0, 0]);
    }
    w.battle.as_mut().expect("the value is there").battle.units[drained].damage_kinds = [1, 4, 0];
    let rival = PlayerId(1);
    let r = w.apply(f4x_core::world::Command::Battle(BattleCommand::EndTurn));
    assert!(
        r.iter().any(
            |e| matches!(e, f4x_core::world::Event::UndeadRaised { player, units: 1, zombies: false, .. } if *player == rival)
        ),
        "{r:?}"
    );
    assert_eq!(w.units().unit_owner(drained_unit), Some(rival));
    assert!(w.units().unit_is_undead(drained_unit));
    assert!(w.entity_exists(drained_unit) && !w.entity_exists(slain_unit));
}

#[test]
fn the_regeneration_spell_heals_a_hit_each_turn_in_battle() {
    use f4x_battle::Side;
    let mut w = casting_siege(&[31 /* regeneration */]);
    let mine = battle_units_of(&w, Side::Attacker)[0];
    w.battle.as_mut().expect("the value is there").battle.units[mine].set_front_damage(1);
    let r = cast(&mut w, &format!("bcast regeneration unit:{mine}"));
    assert!(r.ok, "{}", r.text());
    let before = w
        .battle()
        .expect("a battle is on")
        .unit(mine)
        .expect("the unit is alive")
        .hit_points();
    pass_turn(&mut w);
    let after = w
        .battle()
        .expect("a battle is on")
        .unit(mine)
        .expect("the unit is alive")
        .hit_points();
    assert_eq!(after, before + 1);
}

#[test]
fn eligibility_two_and_three_spells_are_battle_spells_and_dear_on_the_map() {
    let w = casting_siege(&[
        56, /* flight */
        3,  /* wall_of_stone */
        4,  /* giant_strength */
        13, /* ice_bolt */
    ]);
    let cost = |name: u16, in_battle: bool| {
        let id = w
            .content()
            .rules
            .spells
            .id_numbered(name)
            .expect("the shipped content has it");
        (
            w.content().rules.spells.get(id).cost as i32,
            w.casting_cost(HUMAN, id, in_battle),
        )
    };
    // Eligibility 2 (Flight) and 0 (Giant Strength): more away from a battle.
    let factor = w.content().rules.config.wizard.overland_cost_factor;
    for name in [56 /* flight */, 4 /* giant_strength */] {
        let (base, battle) = cost(name, true);
        let (_, map) = cost(name, false);
        assert!(
            battle <= base && map == battle * factor,
            "{name}: {base} {battle} {map}"
        );
    }
    // Overland-only (Wall of Stone) and battle-only (Ice Bolt) spells are not dearer.
    for name in [3 /* wall_of_stone */, 13 /* ice_bolt */] {
        let (_, battle) = cost(name, true);
        let (_, map) = cost(name, false);
        assert_eq!(battle, map, "{name}");
    }
    // Flight can now be cast in a battle.
    let flight = w
        .content()
        .rules
        .spells
        .id_numbered(56)
        .expect("the shipped content has it");
    assert!(w.battle_spells(HUMAN).iter().any(|s| s.spell == flight));
}

#[test]
fn ai_wizards_get_a_personality_and_an_objective_at_the_start() -> TestResult {
    let content = content();
    let w = new_game(
        content,
        &NewGame {
            seed: 11,
            ..NewGame::with_wizards(4)
        },
    )?;
    let human = w.human().expect("the game has a human");
    assert_eq!(
        w.wizard(human).expect("the player is a wizard").objective(),
        None,
        "the human has no objective"
    );
    let rivals: Vec<_> = w
        .player_ids()
        .into_iter()
        .filter(|id| !w.is_neutral_player(*id) && *id != human)
        .collect();
    assert_eq!(rivals.len(), 3, "three AI wizards");
    for id in rivals {
        assert!(
            w.wizard(id)
                .expect("the player is a wizard")
                .objective()
                .is_some(),
            "{} has an objective",
            w.player(id).expect("the player exists").name()
        );
    }
    let back = w.snapshot().restore(content.clone())?;
    for id in w
        .player_ids()
        .into_iter()
        .filter(|id| !w.is_neutral_player(*id))
    {
        let (b, w) = (
            back.wizard(id).expect("the player is a wizard"),
            w.wizard(id).expect("the player is a wizard"),
        );
        assert_eq!(b.objective(), w.objective());
        assert_eq!(b.personality(), w.personality());
    }
    Ok(())
}

#[test]
fn rivals_without_ready_made_wizards_draw_a_name_from_their_list() -> TestResult {
    let mut content = content().clone();
    // The fallback is for a rule set with no ready-made wizards.
    content.rules.wizards = f4x_def::rules::Registry::new(Vec::new());
    // The lists a client would hand over: four slots of ten names.
    let lists: Vec<Vec<(String, f4x_def::text::Gender)>> = (0..4)
        .map(|slot| {
            (0..10)
                .map(|i| (format!("slot{slot}_{i}"), Default::default()))
                .collect()
        })
        .collect();
    let bundle = f4x_def::names::NameBundle {
        rivals: lists.clone(),
        ..Default::default()
    };
    let names = |seed: u64| -> Vec<String> {
        let w = new_game(
            &content,
            &NewGame {
                seed,
                names: bundle.clone(),
                ..NewGame::with_wizards(5)
            },
        )
        .expect("the seed starts a game");
        w.player_ids()
            .into_iter()
            .filter(|id| {
                !w.is_neutral_player(*id) && *id != w.human().expect("the game has a human")
            })
            .map(|id| w.player(id).expect("the player exists").name())
            .collect()
    };
    for seed in 0..20 {
        let got = names(seed);
        assert_eq!(got.len(), 4);
        for (name, list) in got.iter().zip(&lists) {
            assert!(
                list.iter().any(|(n, _)| n == name),
                "{name} is not in its slot's list"
            );
        }
        assert_eq!(got, names(seed), "the same seed draws the same names");
    }
    let seen: std::collections::BTreeSet<_> = (0..20).map(|s| names(s)[0].clone()).collect();
    assert!(
        seen.len() > 3,
        "the first rival's name varies with the seed: {seen:?}"
    );
    Ok(())
}

#[test]
fn recall_hero_takes_a_hero_off_the_field_to_the_summoning_circle() -> TestResult {
    use f4x_battle::{Side, Status};
    let (mut w, city) = siege_world(6, &[]);
    arm_human(&mut w, &[205 /* recall_hero */]);
    let home = w.summon_point(HUMAN).expect("the wizard has a circle");
    let hero = w.spawn_hero(HUMAN, home, 0).map_err(|e| format!("{e:?}"))?;
    start_siege(&mut w, city);
    let mine = battle_units_of(&w, Side::Attacker);
    let source_of = |w: &World, id: usize| {
        let b = w.battle.as_ref().expect("the value is there");
        b.sources[b
            .battle
            .unit(id)
            .expect("the unit is alive")
            .source
            .expect("the fighter has a source")]
    };
    let hero_id = *mine
        .iter()
        .find(|&&id| source_of(&w, id) == hero)
        .expect("the hero fights");
    let other = *mine.iter().find(|&&id| id != hero_id).expect("it is there");
    // Only a hero of the wizard's own can be recalled.
    assert!(!cast(&mut w, &format!("bcast recall hero unit:{other}")).ok);
    let enemy = battle_units_of(&w, Side::Defender)[0];
    assert!(!cast(&mut w, &format!("bcast recall hero unit:{enemy}")).ok);
    let r = cast(&mut w, &format!("bcast recall hero unit:{hero_id}"));
    assert!(r.ok, "{}", r.text());
    assert_eq!(
        w.battle()
            .expect("a battle is on")
            .unit(hero_id)
            .expect("the unit is alive")
            .status,
        Status::Recalled
    );
    // The rest of the army gives up: the hero is not among those who flee and risk their lives.
    w.apply(f4x_core::world::Command::Battle(
        f4x_battle::battle::BattleCommand::Flee,
    ));
    assert!(w.battle().is_none(), "the battle is over");
    assert_eq!(
        w.units().unit_position(hero),
        Some(home),
        "a recalled hero is not lost"
    );
    Ok(())
}

#[test]
fn an_offer_can_be_listed_and_taken_by_number() {
    let (mut w, mut c) = (game(), console());
    assert!(c.execute(&mut w, "offers").text().contains("no offers"));
    w.add_fame(HUMAN, 100);
    w.treasury_mut(HUMAN)
        .expect("the player has a treasury")
        .gold += 5000;
    let round = w.round();
    let made = w.cheat_offer(HUMAN, f4x_core::player::offer::OfferKind::Hero, round);
    assert!(made.is_some());
    let listed = c.execute(&mut w, "offers");
    assert!(listed.text().contains("1: "), "{}", listed.text());
    assert!(
        listed.text().contains("gold to join you"),
        "{}",
        listed.text()
    );
    let taken = c.execute(&mut w, "accept 1 Rex the Bold");
    assert!(taken.ok, "{}", taken.text());
    assert!(
        taken.text().contains("Rex the Bold joins"),
        "{}",
        taken.text()
    );
    assert!(c.execute(&mut w, "offers").text().contains("no offers"));
    // An answer to an offer that is gone is refused, and so is a number below one.
    assert!(c.execute(&mut w, "accept 1").error.is_some());
    assert!(c.execute(&mut w, "reject 0").error.is_some());
    // The merchant's item and a rejection; the item's price is rolled, so there is gold enough
    // for any.
    w.treasury_mut(HUMAN)
        .expect("the player has a treasury")
        .gold += 50_000;
    let round = w.round();
    assert!(w
        .cheat_offer(HUMAN, f4x_core::player::offer::OfferKind::Item, round)
        .is_some());
    assert!(c.execute(&mut w, "reject 1").ok);
}

#[test]
fn a_recorded_game_is_played_again_by_text() {
    let (mut w, mut c) = (game(), console());
    assert!(c.execute(&mut w, "record").ok);
    assert!(c.execute(&mut w, "tax 2").ok);
    assert!(c.execute(&mut w, "end").ok);
    let r = c.execute(&mut w, "record status");
    // The tax rate and the end of the turn, and what the independent cities' AI ordered.
    let kept: u32 = r
        .text()
        .split_whitespace()
        .find_map(|w| w.parse().ok())
        .unwrap_or(0);
    assert!(kept >= 2, "{}", r.text());
    let name = "console_replay_test";
    let r = c.execute(&mut w, &format!("record stop {name}"));
    assert!(r.ok, "{}", r.text());
    let (turn, tax) = (
        w.round(),
        w.player_component::<f4x_core::player::Tax>(HUMAN)
            .expect("the entity has it")
            .0,
    );
    let mut fresh = game();
    let r = c.execute(&mut fresh, &format!("replay {name}"));
    let _ = std::fs::remove_file(c.session.saves_dir.join(format!("{name}.replay.ron")));
    assert!(r.ok, "{}", r.text());
    assert_eq!(
        (
            fresh.round(),
            fresh
                .player_component::<f4x_core::player::Tax>(HUMAN)
                .expect("the entity has it")
                .0
        ),
        (turn, tax)
    );
    assert!(fresh.is_recording());
}

#[test]
fn a_neutral_city_whose_garrison_is_gone_trains_a_new_one() {
    let mut w = game();
    // A city's plan decides whether it trains units at all (some only build), so every garrisoned
    // city loses its garrison and at least one has to train a new one.
    let cities: Vec<_> = neutral_cities(&w)
        .into_iter()
        .filter(|(_, p)| w.units().units_at(*p).len() >= 2)
        .map(|(c, p)| (w.economy().city_owner(c).expect("the city has an owner"), p))
        .collect();
    assert!(!cities.is_empty(), "a garrisoned city");
    for &(_, pos) in &cities {
        for u in w.units().units_at(pos) {
            w.apply(f4x_core::world::Command::DismissUnit { unit: u });
        }
        assert!(w.units().units_at(pos).is_empty());
    }
    for _ in 0..80 {
        f4x_ai::play_round(&mut w, &mut []);
    }
    assert!(
        cities
            .iter()
            .any(|&(owner, _)| !w.units().player_units(owner).is_empty()),
        "a city built soldiers again"
    );
}

#[test]
fn the_start_gives_the_settings_mana_and_reveals_only_the_start_radius() -> TestResult {
    use f4x_core::world::setting::Split;
    use f4x_def::half_int::HalfInt;
    let known = |radius: f32, mana: i32, diagonal_extra: bool| {
        let mut spec = NewGame::with_wizards(2);
        spec.seed = 5;
        spec.settings.basic.diagonal_extra_cost = diagonal_extra;
        spec.settings.world.start_reveal_radius = HalfInt::from_raw((radius * 2.0) as i32);
        spec.settings.wizards.starting_mana = Split { human: mana, ai: 7 };
        let w = new_game(content(), &spec).expect("the spec starts a game");
        let human = w.human().expect("the game has a human");
        let ai = w
            .wizards()
            .into_iter()
            .find(|&w| w != human)
            .expect("it is there");
        assert_eq!(
            w.player(human).expect("the player exists").treasury().mana,
            mana
        );
        assert_eq!(w.player(ai).expect("the player exists").treasury().mana, 7);
        w.explored_squares(human)
    };
    // With a diagonal step costing extra: a plus of 5, the 3 by 3 block, and the 5 by 5 block
    // without its corners (the fortress is not at the map's edge, which would cut them).
    assert_eq!(known(1.0, 0, true), 5);
    assert_eq!(known(1.5, 30, true), 9);
    assert_eq!(known(2.5, 0, true), 21);
    // Without it, a diagonal square is as near as a straight one: whole blocks.
    assert_eq!(known(1.0, 0, false), 9);
    assert_eq!(known(2.5, 0, false), 25);
    Ok(())
}

fn a_ring() -> f4x_core::item::Item {
    f4x_core::item::Item {
        name: "Test Ring".into(),
        kind: f4x_def::rules::ItemKind::Misc,
        cost: 100,
        icon: 80,
        bonus: f4x_core::item::ItemBonus {
            defense: 1,
            ..Default::default()
        },
        powers: Vec::new(),
        charges: None,
        prefab: None,
        made: None,
    }
}

#[test]
fn items_are_listed_equipped_and_scrapped_by_place() -> TestResult {
    let (mut w, mut c) = (game(), console());
    w.add_fame(HUMAN, 100);
    w.treasury_mut(HUMAN)
        .expect("the player has a treasury")
        .gold += 5000;
    let round = w.round();
    w.cheat_offer(HUMAN, f4x_core::player::offer::OfferKind::Hero, round)
        .ok_or("no hero can be offered")?;
    assert!(c.execute(&mut w, "accept 1 Rex").ok);
    let hero = w.units().hero_units(HUMAN)[0];
    let serial = w.units().serial_of(hero).0;
    w.give_item(HUMAN, a_ring());
    let listed = c.execute(&mut w, "items");
    assert!(
        listed.text().contains("vault:1 Test Ring"),
        "{}",
        listed.text()
    );
    assert!(listed.text().contains(&format!("hero:{serial}:3")));
    // Every hero's last slot takes a miscellaneous item.
    let put = c.execute(&mut w, &format!("equip vault:1 hero:{serial}:3"));
    assert!(put.ok, "{}", put.text());
    let listed = c.execute(&mut w, "items");
    assert!(
        listed
            .text()
            .contains(&format!("hero:{serial}:3 (Misc) Test Ring")),
        "{}",
        listed.text()
    );
    // A hero's item is not scrapped from there, a bad place is refused.
    assert!(c
        .execute(&mut w, &format!("scrap hero:{serial}:3"))
        .error
        .is_some());
    assert!(c.execute(&mut w, "equip vault:x hero:1:1").error.is_some());
    assert!(
        c.execute(&mut w, &format!("equip hero:{serial}:3 vault:2"))
            .ok
    );
    let mana = w.player(HUMAN).expect("the player exists").treasury().mana;
    assert!(c.execute(&mut w, "scrap vault:2").ok);
    assert_eq!(
        w.player(HUMAN).expect("the player exists").treasury().mana,
        mana + 50,
        "half its value"
    );
    Ok(())
}

#[test]
fn an_item_is_designed_and_cast_from_the_console() {
    let (mut w, mut c) = (game(), console());
    let spell = w
        .content()
        .rules
        .spells
        .id_numbered(207)
        .expect("the shipped content has it");
    w.cheat_learn_spell(HUMAN, spell);
    w.treasury_mut(HUMAN)
        .expect("the player has a treasury")
        .mana += 5000;
    // No power, a power the kind cannot have, and the wrong spell name are refused.
    assert!(!c.execute(&mut w, "craft enchant misc -").ok);
    assert!(!c.execute(&mut w, "craft enchant misc attack_6").ok);
    assert!(c
        .execute(&mut w, "craft hammer misc defense_1")
        .error
        .is_some());
    let made = c.execute(&mut w, "craft enchant misc defense_1 Ring of Test");
    assert!(made.ok, "{}", made.text());
    assert!(
        w.wizard(HUMAN)
            .expect("the player is a wizard")
            .casting()
            .is_some()
            || !w.items().armory(HUMAN).vault.is_empty(),
        "the spell is being cast or the item is made"
    );
}

#[test]
fn a_dead_hero_is_listed_and_raised_from_the_console() -> TestResult {
    let (mut w, mut c) = (game(), console());
    w.add_fame(HUMAN, 100);
    w.treasury_mut(HUMAN)
        .expect("the player has a treasury")
        .gold += 5000;
    let round = w.round();
    w.cheat_offer(HUMAN, f4x_core::player::offer::OfferKind::Hero, round)
        .ok_or("no hero can be offered")?;
    assert!(c.execute(&mut w, "accept 1 Rex").ok);
    assert!(c.execute(&mut w, "dead").text().contains("no dead heroes"));
    let hero = w.units().hero_units(HUMAN)[0];
    w.cheat_slay_unit(hero);
    let listed = c.execute(&mut w, "dead");
    assert!(listed.text().contains("Rex"), "{}", listed.text());
    let spell = w
        .content()
        .rules
        .spells
        .id_numbered(133)
        .expect("the shipped content has it");
    w.cheat_learn_spell(HUMAN, spell);
    w.treasury_mut(HUMAN)
        .expect("the player has a treasury")
        .mana += 5000;
    assert!(c.execute(&mut w, "resurrect Nobody").error.is_some());
    let raised = c.execute(&mut w, "resurrect Rex");
    assert!(raised.ok, "{}", raised.text());
    assert!(
        w.wizard(HUMAN)
            .expect("the player is a wizard")
            .casting()
            .is_some()
            || !w.units().hero_units(HUMAN).is_empty(),
        "the spell is being cast or the hero is back"
    );
    Ok(())
}

#[test]
fn every_map_size_makes_a_game_with_the_counts_in_proportion() -> TestResult {
    let content = content();
    let sizes = content.settings.map_sizes.clone();
    assert_eq!(sizes.len(), 4);
    for size in sizes {
        let mut spec = NewGame::with_wizards(5);
        spec.seed = 11;
        (
            spec.settings.basic.map_width,
            spec.settings.basic.map_height,
        ) = (size.width, size.height);
        let world = new_game(content, &spec)?;
        let map = world.map().expect("the game has a map");
        assert_eq!(
            (map.width(), map.height()),
            (u32::from(size.width), u32::from(size.height))
        );
        assert_eq!(
            (
                world.settings().basic.map_width,
                world.settings().basic.map_height
            ),
            (size.width, size.height),
            "the choice is part of the game's settings"
        );
        assert_eq!(wizards(&world), 5);
        let cities = world.economy().cities();
        let (w, h) = (u32::from(size.width), u32::from(size.height));
        // Five fortresses, and the neutral cities of both planes scale with the area.
        let neutral = cities
            .iter()
            .filter(|(c, _, _)| {
                world
                    .economy()
                    .city_owner(*c)
                    .is_some_and(|o| world.is_neutral_player(o))
            })
            .count() as u32;
        let wanted = 2 * f4x_newgame::neutral::neutral_city_count(w, h);
        // The row scan keeps its distances and stops at the target or the last row, so a small
        // map with five fortresses on it gets few: some at every size, never more than the
        // target.
        assert!(
            neutral >= 2 && neutral <= wanted,
            "{}: {neutral} neutral cities of {wanted}",
            size.id
        );
        assert_eq!(cities.len() as u32, 5 + neutral, "{}", size.id);
        // A save and a load keep the size.
        let back = world.snapshot().restore(content.clone())?;
        assert_eq!(back.map().expect("the game has a map").width(), w);
    }
    Ok(())
}

#[test]
fn the_new_command_takes_the_map_generator() {
    let mut world = game();
    let mut c = console();
    c.execute(&mut world, "new 3 generator=original");
    assert!(world.settings().basic.original_map_generator);
    let r = c.execute(&mut world, "settings");
    assert!(r.text().contains("map generator original"), "{}", r.text());
    c.execute(&mut world, "new 3 generator=custom");
    assert!(!world.settings().basic.original_map_generator);
    let r = c.execute(&mut world, "new 3 generator=other");
    assert!(r.error.is_some());
}

#[test]
fn the_new_command_takes_a_map_size() {
    let mut world = game();
    let mut c = console();
    c.execute(&mut world, "new 3 map=huge");
    let r = c.execute(&mut world, "settings");
    assert!(r.text().contains("map 90 by 60 squares"), "{}", r.text());
    assert_eq!(world.map().expect("the game has a map").width(), 90);
    c.execute(&mut world, "new 3 map=75x50");
    assert_eq!(world.map().expect("the game has a map").height(), 50);
    let r = c.execute(&mut world, "new 3 map=200x200");
    assert!(r.error.is_some());
}

mod cheat;
