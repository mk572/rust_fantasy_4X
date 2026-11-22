//! Class 4: the ways a game is rebuilt from text (a save, a replay, an undo) lose nothing.

use crate::support::{busy_round, content, shape, text, worlds};
use f4x_core::game::save::{Integrity, LoadError, Snapshot};
use f4x_core::game::Game;

fn restored(g: &Game) -> Game {
    Snapshot::from_text(&text(g))
        .expect("the text reads")
        .restore(content())
        .expect("the game restores")
}

#[test]
fn a_game_written_and_read_is_the_same_game() {
    for (name, mut g) in worlds() {
        for round in 0..12 {
            let back = restored(&g);
            assert_eq!(text(&back), text(&g), "{name}, round {round}");
            busy_round(&mut g, 6);
        }
    }
}

#[test]
fn a_loaded_game_goes_on_as_the_saved_one_would_have() {
    for (name, mut live) in worlds() {
        for _ in 0..8 {
            busy_round(&mut live, 6);
        }
        let mut loaded = restored(&live);
        for round in 0..15 {
            let a = shape(&busy_round(&mut live, 6));
            let b = shape(&busy_round(&mut loaded, 6));
            assert_eq!(a, b, "{name}, round {round}: the events differ");
            assert_eq!(text(&live), text(&loaded), "{name}, round {round}");
        }
    }
}

fn saved() -> String {
    let (_, g) = worlds().remove(0);
    text(&g)
}

/// Read and rebuild `text`; a panic is a failure of its own and says where it came from.
fn load(text: &str, what: &str) -> Result<Game, LoadError> {
    let result = std::panic::catch_unwind(|| Snapshot::from_text(text)?.restore(content()));
    match result {
        Ok(r) => r,
        Err(_) => panic!("loading panicked: {what}"),
    }
}

#[test]
fn text_that_is_no_save_is_refused() {
    for junk in [
        "",
        "   \n",
        "not a save",
        "(",
        "()",
        "// f4x save\n",
        "\u{0}\u{0}\u{0}",
    ] {
        assert!(
            matches!(load(junk, junk), Err(LoadError::Parse(_))),
            "{junk:?} was not refused as unreadable"
        );
    }
}

#[test]
fn a_save_cut_short_is_refused_wherever_it_is_cut() {
    let full = saved();
    let step = full.len() / 150;
    for cut in (0..full.len()).step_by(step.max(1)) {
        let Some(part) = full.get(..cut) else {
            continue;
        };
        assert!(
            load(part, &format!("cut at {cut}")).is_err(),
            "a save cut at byte {cut} of {} loaded",
            full.len()
        );
    }
}

#[test]
fn a_save_of_another_layout_or_game_version_is_refused_by_name() {
    use f4x_core::game::save::{GAME_VERSION, SAVE_VERSION};
    let full = saved();
    let edit = |from: &str, to: &str| {
        assert!(full.contains(from), "{from} is not in the save");
        full.replacen(from, to, 1)
    };
    let layout = edit(&format!("version: {SAVE_VERSION},"), "version: 9999,");
    assert!(matches!(
        load(&layout, "layout"),
        Err(LoadError::Version { found: 9999, .. })
    ));
    // Older than any game could be, newer than any, and not a version at all.
    for game in ["0.0.0", "9999.0.0", "", "banana", "0.0"] {
        let edited = edit(
            &format!("game_version: \"{GAME_VERSION}\""),
            &format!("game_version: \"{game}\""),
        );
        assert!(
            matches!(load(&edited, game), Err(LoadError::GameVersion { .. })),
            "game version {game:?} was not refused by name"
        );
    }
}

#[test]
fn a_save_of_other_content_is_refused_unless_the_player_chose_to_load_it() {
    use f4x_core::game::save::ContentCheck;
    let snapshot = Snapshot::from_text(&saved()).expect("reads");
    let mut other = content();
    other.fingerprint = "f".repeat(64);
    assert!(matches!(
        snapshot.clone().restore(other.clone()),
        Err(LoadError::Content(_))
    ));
    snapshot
        .restore_with(other, ContentCheck::Ignore)
        .expect("the player chose to load it");
}

#[test]
fn settings_out_of_range_are_refused_and_say_which() {
    let (_, g) = worlds().remove(0);
    let full = text(&g);
    let (tile, width) = (g.settings().rules.max_tile, g.settings().basic.map_width);
    for (name, now) in [("max_tile", tile), ("map_width", width as usize)] {
        let from = format!("{name}: {now}");
        assert!(full.contains(&from), "{from} is not in the save");
        let err = load(&full.replacen(&from, &format!("{name}: 0"), 1), name)
            .err()
            .expect("refused");
        let LoadError::Content(e) = err else {
            panic!("{name} 0: {err:?}")
        };
        assert!(
            format!("{e:?}").contains(name),
            "{name} is not named: {e:?}"
        );
    }
}

#[test]
fn an_edited_save_is_noticed() {
    let full = saved();
    assert_eq!(Snapshot::integrity(&full), Integrity::Intact);
    let edited = full.replacen("turn: ", "turn: 1", 1);
    assert_eq!(Snapshot::integrity(&edited), Integrity::Edited);
    assert!(
        load(&edited, "edited").is_ok(),
        "an edited save still loads"
    );
    let unsigned = full.split_once('\n').expect("a first line").1;
    assert_eq!(Snapshot::integrity(unsigned), Integrity::Unsigned);
}

/// A small fixed generator, so a failure names its own damage and repeats.
fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

#[test]
fn a_damaged_save_never_panics_and_never_loads_into_a_broken_world() {
    use f4x_core::check::world_problems;
    let full = saved();
    let body = full.find('(').expect("a body");
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut loaded = 0;
    for n in 0..400 {
        let at = body + (next(&mut state) as usize) % (full.len() - body);
        if !full.is_char_boundary(at) {
            continue;
        }
        let damage = match next(&mut state) % 3 {
            0 => {
                // One digit becomes another.
                let Some((i, c)) = full[at..].char_indices().find(|(_, c)| c.is_ascii_digit())
                else {
                    continue;
                };
                let new = char::from_digit(
                    (c as u32 - '0' as u32 + 1 + (next(&mut state) % 8) as u32) % 10,
                    10,
                )
                .expect("a digit");
                format!("{}{}{}", &full[..at + i], new, &full[at + i + 1..])
            }
            1 => {
                // A stretch of text is dropped.
                let end = (at + 1 + (next(&mut state) as usize) % 40).min(full.len());
                if !full.is_char_boundary(end) {
                    continue;
                }
                format!("{}{}", &full[..at], &full[end..])
            }
            _ => {
                // A stretch of text is said twice.
                let end = (at + 1 + (next(&mut state) as usize) % 40).min(full.len());
                if !full.is_char_boundary(end) {
                    continue;
                }
                format!("{}{}{}", &full[..end], &full[at..end], &full[end..])
            }
        };
        if let Ok(game) = load(&damage, &format!("damage {n} at byte {at}")) {
            loaded += 1;
            let bad = world_problems(&game);
            assert!(
                bad.is_empty(),
                "damage {n} at byte {at} loads into a broken world: {bad:#?}\n...{}...",
                &damage[at.saturating_sub(40)..(at + 40).min(damage.len())]
            );
        }
    }
    assert!(
        loaded > 0,
        "no damaged save loaded at all: the test is not reaching the loader"
    );
}

#[test]
fn a_recorded_game_plays_again_to_the_same_game() {
    use f4x_core::game::replay::Replay;
    for (name, mut g) in worlds() {
        g.start_recording().expect("recording starts");
        for _ in 0..10 {
            busy_round(&mut g, 6);
        }
        let replay = g.replay_so_far().expect("a recording");
        // Written and read again, as a file would carry it.
        let replay = Replay::from_text(&replay.to_text().expect("writes")).expect("reads");
        let (played, _) = replay.play(content(), usize::MAX).expect("plays");
        assert_eq!(text(&played), text(&g), "{name}");
    }
}

#[test]
fn a_replay_stopped_part_way_is_the_game_as_it_was_then() {
    for (name, mut g) in worlds() {
        g.start_recording().expect("recording starts");
        let mut states = vec![(0, text(&g))];
        for _ in 0..6 {
            busy_round(&mut g, 4);
            let done = g.replay_so_far().expect("a recording").commands.len();
            states.push((done, text(&g)));
        }
        let replay = g.stop_recording().expect("a recording");
        for (count, state) in states {
            let (part, _) = replay.play(content(), count).expect("plays");
            assert_eq!(text(&part), state, "{name}, after {count} commands");
        }
        assert!(!g.is_recording());
    }
}

#[test]
fn a_replay_that_is_no_replay_or_names_nothing_is_refused() {
    use f4x_core::game::replay::{Replay, ReplayError};
    assert!(matches!(
        Replay::from_text("nonsense"),
        Err(ReplayError::Load(_))
    ));
    let (_, mut g) = worlds().remove(0);
    g.start_recording().expect("starts");
    busy_round(&mut g, 3);
    let mut replay = g.stop_recording().expect("recorded");
    replay.commands.insert(
        0,
        "MoveStack(units: [999999], to: (x: 0, y: 0, plane: Arcanus))".into(),
    );
    assert!(matches!(
        replay.play(content(), usize::MAX),
        Err(ReplayError::Unresolved(0))
    ));
    let (_, g2) = worlds().remove(0);
    assert!(matches!(g2.replay_so_far(), Err(ReplayError::NotRecording)));
}

/// A command that changes the game, draws no random number and explores nothing: the tax rate of a wizard.
fn a_change_that_can_be_taken_back(g: &mut Game) -> Option<String> {
    use f4x_core::game::{Command, Event};
    let player = g
        .player_ids()
        .into_iter()
        .find(|&p| !g.is_neutral_player(p) && g.wizard(p).is_some())?;
    let before = text(g);
    for rate in 0..=10u8 {
        let out = g.apply(Command::SetTaxRate { player, rate });
        if !out.iter().any(|e| matches!(e, Event::Rejected(_))) && text(g) != before {
            return Some(before);
        }
    }
    None
}

#[test]
fn undo_gives_back_the_game_as_it_was() {
    for (name, mut g) in worlds() {
        g.set_undo(true);
        let before = a_change_that_can_be_taken_back(&mut g).expect("a change");
        assert!(g.can_undo(), "{name}: nothing to take back");
        g.undo().expect("takes it back");
        assert_eq!(text(&g), before, "{name}");
        assert!(!g.can_undo() || g.undo_label().is_some());
    }
}

#[test]
fn what_cannot_be_taken_back_clears_the_history() {
    use f4x_core::game::undo::UndoError;
    for (name, mut g) in worlds() {
        g.set_undo(true);
        a_change_that_can_be_taken_back(&mut g).expect("a change");
        assert!(g.can_undo(), "{name}");
        busy_round(&mut g, 3); // ends the turn
        assert!(
            !g.can_undo(),
            "{name}: the turn ended and the history stayed"
        );
        assert_eq!(g.undo(), Err(UndoError::NothingToUndo), "{name}");
    }
}

#[test]
fn a_game_with_undo_off_keeps_no_history() {
    use f4x_core::game::undo::UndoError;
    let (_, mut g) = worlds().remove(0);
    assert!(!g.undo_enabled());
    a_change_that_can_be_taken_back(&mut g).expect("a change");
    assert!(!g.can_undo());
    assert_eq!(g.undo(), Err(UndoError::NothingToUndo));
}
