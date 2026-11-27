//! A saved game holds the game's own state, the world and the computer players' plans.

use std::path::PathBuf;

use f4x_core::player::PlayerId;
use f4x_core::world::save::{ContentCheck, Integrity, LoadError, GAME_VERSION, SAVE_VERSION};
use f4x_master::{load_game, save_game, summarize_save, Game, Phase, Plans, SaveFile};

use crate::support::{busy_round, content, games, text};

fn plans() -> Plans {
    vec![
        (PlayerId(1), "march north".to_string()),
        (
            PlayerId(0),
            "(\"quoted\", [1, 2])\nand a second line".to_string(),
        ),
    ]
}

fn written(game: &Game, plans: Plans) -> String {
    game.save_file(plans).to_text().expect("a save")
}

fn read(text: &str) -> Result<(Game, Plans), LoadError> {
    SaveFile::from_text(text)?
        .restore(content(), ContentCheck::Strict)
        .map(|r| (r.game, r.plans))
}

/// A game some rounds on, and over: every field of the game's own state says something.
fn finished() -> Game {
    let (_, mut game) = games().remove(0);
    for _ in 0..3 {
        busy_round(&mut game, 4);
    }
    let winner = game
        .player_ids()
        .into_iter()
        .find(|&p| game.is_wizard(p))
        .expect("a wizard");
    game.world_mut().complete_mastery(winner);
    game.check_end();
    game
}

/// A folder of its own for a test's files.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("f4x-save-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder");
    dir
}

#[test]
fn a_game_written_and_read_has_the_same_round_end_world_and_plans() {
    let over = finished();
    let running = games().into_iter().map(|(_, g)| g);
    for game in running.chain([over]) {
        let saved = written(&game, plans());
        let (back, back_plans) = read(&saved).expect("loads");
        assert_eq!(back.round(), game.round());
        assert_eq!(back.game_over(), game.game_over());
        assert_eq!(back.phase(), game.phase());
        assert_eq!(text(back.world()), text(game.world()));
        assert_eq!(back_plans, plans());
        assert_eq!(written(&back, back_plans), saved);
    }
}

#[test]
fn the_world_of_a_save_loads_on_its_own() {
    let game = finished();
    let file = SaveFile::from_text(&written(&game, Vec::new())).expect("reads");
    let world = file.world.restore(content()).expect("the world restores");
    assert_eq!(text(&world), text(game.world()));
}

#[test]
fn a_saved_file_loads_and_says_whether_it_was_edited() {
    let dir = folder("file");
    let path = dir.join("slot.ron");
    let game = finished();
    save_game(&path, &game, plans()).expect("saves");
    let loaded = load_game(&path, content(), ContentCheck::Strict).expect("loads");
    assert_eq!(loaded.integrity, Integrity::Intact);
    assert_eq!(loaded.plans, plans());
    assert_eq!(loaded.game.game_over(), game.game_over());

    let summary = summarize_save(&path).expect("summarizes");
    let winner = game.game_over().expect("over").winner;
    assert_eq!(summary.turn, game.round());
    assert!(summary.finished);
    let human = game.human().unwrap_or(winner);
    assert_eq!(
        summary.wizard,
        game.wizard(human).map(|w| w.name()).unwrap_or_default()
    );

    let text = std::fs::read_to_string(&path).expect("reads");
    std::fs::write(&path, text.replacen("march north", "march south", 1)).expect("writes");
    let edited = load_game(&path, content(), ContentCheck::Strict).expect("still loads");
    assert_eq!(edited.integrity, Integrity::Edited);
    std::fs::remove_dir_all(&dir).expect("removes");
}

#[test]
fn a_save_of_another_layout_or_game_version_is_refused_by_name() {
    let (_, game) = games().remove(0);
    let full = written(&game, Vec::new());
    let edit = |from: String, to: String| {
        assert!(full.contains(&from), "{from} is not in the save");
        full.replacen(&from, &to, 1)
    };
    let layout = edit(format!("version: {SAVE_VERSION},"), "version: 9999,".into());
    assert!(matches!(
        read(&layout),
        Err(LoadError::Version { found: 9999, .. })
    ));
    for version in ["0.0.0", "9999.0.0", "banana"] {
        let edited = edit(
            format!("game_version: \"{GAME_VERSION}\""),
            format!("game_version: \"{version}\""),
        );
        assert!(
            matches!(read(&edited), Err(LoadError::GameVersion { .. })),
            "game version {version:?} was not refused by name"
        );
    }
}

#[test]
fn a_save_cut_short_is_refused() {
    let (_, game) = games().remove(0);
    let full = written(&game, plans());
    let step = (full.len() / 100).max(1);
    for cut in (0..full.len() - 1).step_by(step) {
        let Some(part) = full.get(..cut) else {
            continue;
        };
        assert!(read(part).is_err(), "a save cut at byte {cut} loaded");
    }
}

#[test]
fn a_game_keeps_the_place_in_the_round_it_was_saved_at() {
    use f4x_master::{Controller, Master, Seat, Status, Until};
    struct Waits;
    impl Controller for Waits {
        fn play(
            &mut self,
            _: &mut Game,
            _: &[PlayerId],
            _: &mut Vec<f4x_core::world::Event>,
        ) -> Status {
            Status::Waiting
        }
    }
    let (_, mut game) = games().remove(0);
    let seats: Vec<Seat> = game
        .player_ids()
        .into_iter()
        .filter(|&p| game.is_wizard(p))
        .map(|p| Seat::new(vec![p], Box::new(Waits)))
        .collect();
    Master::new(seats).run(&mut game, Until::Waiting);
    assert_eq!(game.phase(), Some(Phase::Acting(0)));
    let (back, _) = read(&written(&game, Vec::new())).expect("loads");
    assert_eq!(back.phase(), Some(Phase::Acting(0)));
}
