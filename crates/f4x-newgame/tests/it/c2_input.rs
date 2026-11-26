//! Class 2 (`docs/f4x-newgame-test-design.md`): a bad call is an error, never a panic, a hang or a
//! silent change of what was asked. Every input the settings allow gives a game that keeps the
//! rules of classes 3 and 4.
//!
//! The attacks: seeds at the ends of their range, every setting at both ends of its range and just
//! outside it, `NewGame` fields that disagree, name lists that are empty or odd, the map generator
//! at the ends of its own range, and content with a row taken away.

use crate::c3_map::map_problems;
use crate::c4_start::start_problems;
use crate::support::*;
use f4x_newgame::error::NewGameError;
use f4x_newgame::mapgen::{generate, GenParams, LAND_RANGE, MINERAL_MOST, SIDE_RANGE};
use f4x_newgame::setup::{gen_params, new_game, NewGame};
use std::sync::mpsc;
use std::time::Duration;

/// Run `f` on a thread and give up on it after `secs`: a hang is a failure, not a stuck test run.
fn bounded<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)));
    });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(Ok(v)) => v,
        Ok(Err(panic)) => {
            let what = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            panic!("a panic: {what}")
        }
        Err(_) => panic!("no answer in {secs} s"),
    }
}

// ---- seeds ----

#[test]
fn every_seed_makes_a_game_for_every_kind_of_game() {
    for &seed in SEEDS {
        for all_ai in [true, false] {
            let case = WorldCase {
                seed,
                all_ai,
                ..standard()
            };
            let problems = bounded(60, move || {
                let w = new_game(content(), &case.spec())
                    .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
                start_problems(&w, case)
            });
            assert!(problems.is_empty(), "{case:?}: {problems:?}");
        }
    }
}

// ---- settings at their ends ----

#[test]
fn every_setting_at_either_end_makes_a_game_that_keeps_the_rules() {
    let std = standard();
    let mut failures = Vec::new();
    for knob in knobs() {
        let (low, high) = limits(&knob);
        let name = knob.name;
        for (end, value) in [("low", low), ("high", high)] {
            for variant in [Variant::Plain, Variant::OriginalGenerator] {
                let case = WorldCase { variant, ..std };
                let mut settings = case.settings();
                (knob.set)(&mut settings, value);
                // The original generator is asked for no more land than it can make.
                if settings.basic.original_map_generator {
                    settings.basic.land_percent = settings
                        .basic
                        .land_percent
                        .min(f4x_def::settings::ORIGINAL_GENERATOR_MAX_LAND);
                }
                let wizards = settings.basic.opponents + 1;
                let seed = 17;
                let result = bounded(120, move || {
                    let spec = NewGame {
                        seed,
                        all_ai: true,
                        settings,
                        ..Default::default()
                    };
                    let w = new_game(content(), &spec)?;
                    let case = WorldCase {
                        wizards,
                        seed,
                        ..case
                    };
                    let mut problems = start_problems(&w, case);
                    let map = generate(seed, &gen_params(&settings), content())
                        .map_err(|e| e.to_string())?;
                    problems.extend(map_problems(&map, content(), &gen_params(&settings), true));
                    Ok::<_, String>(problems)
                });
                match result {
                    Err(e) => failures.push(format!("{name} {end} {variant:?}: refused: {e}")),
                    Ok(p) if !p.is_empty() => {
                        failures.push(format!("{name} {end} {variant:?}: {p:?}"))
                    }
                    Ok(_) => {}
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Each setting just outside the range the validator takes, and at the most its field holds:
/// refused, with the name of the setting in the issue.
#[test]
fn every_setting_just_outside_its_range_is_refused_by_name() {
    let mut cases = Vec::new();
    for knob in knobs() {
        let (low, high) = limits(&knob);
        if low > 0 {
            cases.push((knob.name, knob.set, low - 1));
            cases.push((knob.name, knob.set, 0));
        }
        if high < knob.cap {
            cases.push((knob.name, knob.set, high + 1));
            cases.push((knob.name, knob.set, knob.cap));
        }
    }
    assert!(cases.len() > 10, "the validator bounds hardly any setting");
    for (name, set, value) in cases {
        let mut settings = standard().settings();
        set(&mut settings, value);
        let spec = NewGame {
            settings,
            all_ai: true,
            ..Default::default()
        };
        let result = bounded(30, move || new_game(content(), &spec).map(|_| ()));
        let error = result.expect_err(name);
        let NewGameError::Settings(issues) = &error else {
            panic!("{name} {value}: {error:?}");
        };
        assert!(
            issues
                .iter()
                .any(|i| i.detail.first().is_some_and(|d| d.ends_with(name))),
            "{name} {value}: {issues:?}"
        );
    }
}

// ---- the call's own fields ----

fn human_spec() -> NewGame {
    NewGame {
        seed: 3,
        all_ai: false,
        settings: standard().settings(),
        ..Default::default()
    }
}

#[test]
fn a_preset_that_is_not_there_is_refused_by_number() {
    let spec = NewGame {
        preset: Some(u16::MAX),
        ..human_spec()
    };
    let error =
        bounded(30, move || new_game(content(), &spec).map(|_| ())).expect_err("no such wizard");
    assert_eq!(error, NewGameError::NoPreset { num: u16::MAX });
}

#[test]
fn every_ready_made_wizard_starts_a_game_for_a_human() {
    for def in content().rules.wizards.defs() {
        let spec = NewGame {
            preset: Some(def.num),
            ..human_spec()
        };
        let num = def.num;
        let problems = bounded(60, move || {
            let w = new_game(content(), &spec).unwrap_or_else(|e| panic!("wizard {num}: {e}"));
            start_problems(
                &w,
                WorldCase {
                    all_ai: false,
                    ..standard()
                },
            )
        });
        assert!(problems.is_empty(), "wizard {num}: {problems:?}");
    }
}

#[test]
fn a_race_that_is_not_there_or_of_the_other_plane_is_refused() {
    let races = content().rules.races.defs();
    let myrror = races.iter().find(|r| r.myrror).expect("a Myrror race").num;
    for race in [u16::MAX, myrror] {
        let spec = NewGame {
            race: Some(race),
            ..human_spec()
        };
        let error =
            bounded(30, move || new_game(content(), &spec).map(|_| ())).expect_err("a bad race");
        assert!(
            matches!(error, NewGameError::RaceNotOnPlane { race: r, myrror: false } if r == race),
            "{race}: {error:?}"
        );
    }
}

#[test]
fn every_race_of_the_planes_can_be_chosen_by_a_human() {
    for def in content().rules.races.defs().iter().filter(|r| !r.myrror) {
        let spec = NewGame {
            race: Some(def.num),
            ..human_spec()
        };
        let num = def.num;
        let problems = bounded(60, move || {
            let w = new_game(content(), &spec).unwrap_or_else(|e| panic!("race {num}: {e}"));
            start_problems(
                &w,
                WorldCase {
                    all_ai: false,
                    ..standard()
                },
            )
        });
        assert!(problems.is_empty(), "race {num}: {problems:?}");
    }
}

#[test]
fn choices_for_a_player_who_is_not_there_are_refused_and_not_half_taken() {
    // An all-AI game has no player 0: each of these used to be taken or dropped without a word.
    let first_race = content().rules.races.defs()[0].num;
    let choices: Vec<NewGame> = vec![
        NewGame {
            race: Some(first_race),
            ..NewGame::default()
        },
        NewGame {
            preset: Some(content().rules.wizards.defs()[0].num),
            ..NewGame::default()
        },
        NewGame {
            name: Some("Nobody".into()),
            ..NewGame::default()
        },
        NewGame {
            books: [(f4x_core::rules::RealmId::new(0), 3)]
                .into_iter()
                .collect(),
            ..NewGame::default()
        },
    ];
    for choice in choices {
        let spec = NewGame {
            all_ai: true,
            settings: standard().settings(),
            ..choice
        };
        let error =
            bounded(30, move || new_game(content(), &spec).map(|_| ())).expect_err("refused");
        assert_eq!(error, NewGameError::ChoicesWithoutHuman);
    }
}

#[test]
fn a_build_over_the_limits_is_refused() {
    let mut build = f4x_core::wizard::WizardBuild::default();
    let most = standard().settings().wizards.max_books_per_realm;
    build
        .books
        .insert(f4x_core::rules::RealmId::new(0), most + 1);
    let spec = NewGame {
        build: Some(build),
        ..human_spec()
    };
    let result = bounded(30, move || new_game(content(), &spec).map(|_| ()));
    assert!(result.is_err(), "a wizard over the book limit was accepted");
}

// ---- the lists of names ----

#[test]
fn name_lists_that_are_empty_or_odd_give_a_game() {
    use f4x_def::names::NameBundle;
    use f4x_def::text::Gender;
    let long = "x".repeat(100_000);
    let bundles: Vec<NameBundle> = vec![
        NameBundle::default(),
        NameBundle {
            rivals: vec![vec![]; usize::from(most_wizards())],
            ..Default::default()
        },
        NameBundle {
            rivals: vec![vec![(String::new(), Gender::default())]],
            ..Default::default()
        },
        NameBundle {
            rivals: vec![vec![("Ünï \u{1F9D9} \u{202E}x".into(), Gender::default())]],
            cities: [(0, vec!["../..".into(), "a\0b".into()])]
                .into_iter()
                .collect(),
            ..Default::default()
        },
        NameBundle {
            rivals: vec![vec![(long.clone(), Gender::default())]],
            cities: [(0, vec![long])].into_iter().collect(),
            ..Default::default()
        },
    ];
    for names in bundles {
        let spec = NewGame {
            names,
            all_ai: true,
            settings: standard().settings(),
            ..Default::default()
        };
        let problems = bounded(60, move || {
            let w = new_game(content(), &spec).expect("a game with these names");
            start_problems(&w, standard())
        });
        assert!(problems.is_empty(), "{problems:?}");
    }
}

// ---- the map generator at the ends of its own range ----

#[test]
fn the_map_generator_takes_the_ends_of_its_sizes_and_shares() {
    // Each end of a side, the ends of the land share and of the minerals, and a size in between.
    let (lo, hi) = (*SIDE_RANGE.start(), *SIDE_RANGE.end());
    let sizes = [
        (lo, lo),
        (lo + 1, lo + 5),
        (lo, hi),
        (hi, lo),
        (lo * 25, lo * 19),
    ];
    for (width, h) in sizes {
        let land_middle = (LAND_RANGE.start() + LAND_RANGE.end()) / 2;
        for land in [*LAND_RANGE.start(), land_middle, *LAND_RANGE.end()] {
            for minerals in [0u8, MINERAL_MOST] {
                let outcome = bounded(60, move || {
                    let mut p = GenParams {
                        land_percent: land,
                        ..GenParams::sized(width, h)
                    };
                    for plane in &mut p.planes {
                        plane.mineral_percent = minerals;
                    }
                    let g = generate(5, &p, content()).map_err(|e| e.to_string())?;
                    Ok::<_, String>(map_problems(&g, content(), &p, true))
                });
                let problems = outcome.unwrap_or_else(|e| panic!("{width}x{h} land {land}: {e}"));
                assert!(
                    problems.is_empty(),
                    "{width}x{h} land {land} minerals {minerals}: {problems:?}"
                );
            }
        }
    }
}

#[test]
fn a_size_or_a_share_outside_the_generators_range_is_an_error() {
    let content = content();
    let (lo, hi) = (*SIDE_RANGE.start(), *SIDE_RANGE.end());
    for (w, h) in [
        (0, 0),
        (lo - 1, 50),
        (50, lo - 1),
        (hi + 1, 50),
        (50, hi + 1),
        (u32::MAX, u32::MAX),
    ] {
        let r = bounded(10, move || {
            generate(1, &GenParams::sized(w, h), content).map(|_| ())
        });
        assert!(r.is_err(), "{w}x{h}");
    }
    for land in [LAND_RANGE.start() - 1, LAND_RANGE.end() + 1, u8::MAX] {
        let p = GenParams {
            land_percent: land,
            ..GenParams::default()
        };
        assert!(generate(1, &p, content).is_err(), "land {land}");
    }
    let mut p = GenParams::default();
    p.planes[0].mineral_percent = MINERAL_MOST + 1;
    assert!(generate(1, &p, content).is_err(), "minerals over the most");
}

#[test]
fn a_big_map_with_many_sites_is_made_in_reasonable_time() {
    // The sites of a big map are tens of thousands: placing each must not look at all the others.
    let seconds = bounded(30, || {
        let t = std::time::Instant::now();
        let p = GenParams {
            land_percent: 1,
            ..GenParams::sized(1000, 1000)
        };
        generate(7, &p, content()).expect("a big map");
        t.elapsed().as_secs_f32()
    });
    assert!(seconds < 10.0, "{seconds} s");
}

// ---- content with a row taken away ----

/// A folder with a copy of the shipped content, for a test to damage.
fn copy_of_shipped(label: &str) -> f4x_def::testdir::TestDir {
    let dir = f4x_def::testdir::TestDir::new(label);
    for entry in std::fs::read_dir(f4x_def::testdir::shipped_dir()).expect("the content folder") {
        let path = entry.expect("an entry").path();
        if path.is_file() {
            std::fs::copy(&path, dir.join(path.file_name().expect("a name"))).expect("a copy");
        }
    }
    dir
}

/// Change the rows of a file of the copy through their type and write them back.
fn edit<T: serde::de::DeserializeOwned + serde::Serialize>(
    dir: &std::path::Path,
    file: &str,
    change: impl FnOnce(&mut T),
) {
    let text = std::fs::read_to_string(dir.join(file)).expect("the file");
    let mut rows: T = ron::from_str(&text).expect("the shipped file parses as its type");
    change(&mut rows);
    let text = ron::ser::to_string_pretty(&rows, ron::ser::PrettyConfig::default())
        .expect("the rows can be written");
    std::fs::write(dir.join(file), text).expect("written");
}

/// What a new game does with the damaged content in `dir`: refused by the loader, refused by the
/// call, or a game. Never a panic or a hang; the answer is described.
fn with_damage(label: &str, damage: impl FnOnce(&std::path::Path)) -> String {
    let dir = copy_of_shipped(label);
    damage(&dir);
    let loaded = match f4x_def::load::load_content(&dir) {
        Err(e) => return format!("the content is refused: {} issues", e.issues.len()),
        Ok(c) => c,
    };
    let spec = NewGame {
        seed: 5,
        all_ai: true,
        settings: standard().settings(),
        ..Default::default()
    };
    bounded(60, move || match new_game(&loaded, &spec) {
        Ok(_) => "a game".to_string(),
        Err(e) => format!("the call is refused: {e}"),
    })
}

/// The damaged content did not give a game: the loader or the call refused it.
fn is_refusal(answer: &str) -> bool {
    answer.starts_with("the content is refused") || answer.starts_with("the call is refused")
}

#[test]
fn content_without_a_kind_of_feature_is_refused_or_played_never_a_panic() {
    use f4x_def::feature::{FeatureDef, FeatureKind};
    for kind in [
        FeatureKind::Tower,
        FeatureKind::Node,
        FeatureKind::Encounter,
        FeatureKind::Mineral,
    ] {
        let answer = with_damage("no-feature-kind", |dir| {
            edit::<Vec<FeatureDef>>(dir, "features.ron", |rows| rows.retain(|f| f.kind != kind));
        });
        assert!(is_refusal(&answer), "without {kind:?}: {answer}");
    }
}

#[test]
fn content_without_a_class_of_terrain_is_refused_or_played_never_a_panic() {
    use f4x_def::map::TerrainDef;
    let classes: Vec<_> = content().terrains.defs().iter().map(|d| d.class).collect();
    let mut seen = std::collections::BTreeSet::new();
    for class in classes {
        if !seen.insert(format!("{class:?}")) {
            continue;
        }
        let answer = with_damage("no-terrain-class", |dir| {
            edit::<Vec<TerrainDef>>(dir, "terrain.ron", |rows| rows.retain(|t| t.class != class));
        });
        assert!(is_refusal(&answer), "without {class:?}: {answer}");
    }
}

#[test]
fn content_without_the_races_of_a_plane_is_refused_or_played_never_a_panic() {
    use f4x_def::rules::RaceDef;
    for myrror in [true, false] {
        let answer = with_damage("no-races", |dir| {
            edit::<Vec<RaceDef>>(dir, "races.ron", |rows| rows.retain(|r| r.myrror != myrror));
        });
        assert!(
            is_refusal(&answer),
            "without the races of Myrror={myrror}: {answer}"
        );
    }
    let answer = with_damage("no-army", |dir| {
        edit::<Vec<RaceDef>>(dir, "races.ron", |rows| {
            for r in rows {
                r.start_army.clear();
            }
        });
    });
    assert!(is_refusal(&answer), "without start armies: {answer}");
}

#[test]
fn content_without_ready_made_wizards_heroes_or_a_retort_is_refused_or_played_never_a_panic() {
    use f4x_def::rules::{HeroTemplate, RetortDef, WizardDef};
    let answer = with_damage("no-wizards", |dir| {
        edit::<Vec<WizardDef>>(dir, "wizards.ron", |rows| rows.clear());
    });
    // Without ready-made wizards the computer wizards are made plain; without heroes there are none.
    assert_eq!(answer, "a game", "without ready-made wizards");
    let answer = with_damage("no-heroes", |dir| {
        edit::<Vec<HeroTemplate>>(dir, "heroes.ron", |rows| rows.clear());
    });
    assert_eq!(answer, "a game", "without heroes");
    let myrran = f4x_newgame::setup::parse_retort(&content().rules, "myrran").expect("Myrran");
    let answer = with_damage("no-myrran", |dir| {
        edit::<Vec<RetortDef>>(dir, "retorts.ron", |rows| {
            rows.retain(|r| r.num != myrran.num())
        });
    });
    assert!(
        answer.contains("retort 9 as myrran"),
        "without the Myrran retort: {answer}"
    );
}

#[test]
fn the_original_generator_is_refused_more_land_than_it_can_make() {
    let most = f4x_def::settings::ORIGINAL_GENERATOR_MAX_LAND;
    let make = |land: u8, original: bool| {
        let mut settings = standard().settings();
        settings.basic.original_map_generator = original;
        settings.basic.land_percent = land;
        let spec = NewGame {
            settings,
            all_ai: true,
            ..Default::default()
        };
        bounded(30, move || new_game(content(), &spec).map(|_| ()))
    };
    // The limit is the original generator's own: the noise generator takes the same land.
    assert!(make(most, true).is_ok());
    assert!(make(most + 1, false).is_ok());
    assert!(make(99, false).is_ok());
    for land in [most + 1, 75, 99] {
        let error = make(land, true).expect_err("more land than the walks can make");
        let NewGameError::Settings(issues) = &error else {
            panic!("{land}: {error:?}");
        };
        assert!(
            issues.iter().any(|i| i
                .detail
                .first()
                .is_some_and(|d| d.ends_with("land_percent"))),
            "{land}: {issues:?}"
        );
    }
}

#[test]
fn the_original_generator_makes_the_land_it_may_be_asked_for_on_every_size() {
    // The limit is only as good as the reach: at the limit, on every corner size and many seeds,
    // each plane has the share asked (less a point).
    let most = f4x_def::settings::ORIGINAL_GENERATOR_MAX_LAND;
    for (width, height) in sizes() {
        for seed in 0..40u64 {
            let case = WorldCase {
                seed,
                width,
                height,
                land: most,
                variant: Variant::OriginalGenerator,
                ..standard()
            };
            let g = generated(case);
            for plane in f4x_core::map::Plane::all() {
                let got =
                    f4x_newgame::mapgen::land_percent(g.map.plane(plane), &content().terrains);
                assert!(got + 1 >= u32::from(most), "{case:?} {plane:?}: {got}%");
            }
        }
    }
}
