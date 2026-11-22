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

/// FNV-1a of the bytes, worked out here from its definition.
fn fnv1a(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[test]
fn the_checksum_of_a_save_is_the_fnv_hash_of_the_rest() {
    for (name, g) in worlds() {
        let full = text(&g);
        let (first, body) = full.split_once('\n').expect("a first line");
        let sum = first.rsplit_once("checksum ").expect("a checksum").1;
        assert_eq!(sum, format!("{:016x}", fnv1a(body)), "{name}");
    }
}

#[test]
fn a_save_that_contradicts_itself_is_refused() {
    let full = saved();
    // A change made inside the one list (`units: [(` or `cities: [(`) of the save.
    let in_list = |list: &str, from: &str, to: &str| {
        let at = full.find(list).expect("the list");
        let rest = &full[at..];
        assert!(rest.contains(from), "{from} is not in {list}");
        format!("{}{}", &full[..at], rest.replacen(from, to, 1))
    };
    // The values of the first two cities and the first unit, read from the save.
    let value = |list: &str, nth: usize, key: &str| -> String {
        let at = full.find(list).expect("the list");
        let line = full[at..]
            .lines()
            .filter(|l| {
                l.trim().starts_with(key)
                    || l.trim()
                        .starts_with(&format!("pos: (plane: Arcanus, {key}"))
            })
            .nth(nth)
            .unwrap_or_else(|| panic!("no {key} in {list}"));
        line.trim().to_string()
    };
    let serial_line = |nth| value("cities: [(", nth, "serial: ");
    let (first, second) = (serial_line(0), serial_line(1));
    let next = full
        .lines()
        .find_map(|l| l.trim().strip_prefix("next_serial: "))
        .expect("the next serial")
        .to_string();
    let city_pos = value("cities: [(", 0, "pos: (plane");
    let unit_pos = value("units: [(", 0, "pos: (plane");
    let farmers = value("cities: [(", 0, "farmers: ");
    let off = |pos: &str| {
        pos.replacen(
            pos.split("x: ").nth(1).unwrap().split(',').next().unwrap(),
            "9999",
            1,
        )
    };
    let cases = [
        (
            "a creation number of zero",
            in_list("cities: [(", &first, "serial: 0,"),
        ),
        (
            "a creation number not yet given out",
            in_list("cities: [(", &first, &format!("serial: {next}")),
        ),
        (
            "one creation number twice",
            in_list("cities: [(", &second, &first),
        ),
        (
            "a unit off the map",
            in_list("units: [(", &unit_pos, &off(&unit_pos)),
        ),
        (
            "a city off the map",
            in_list("cities: [(", &city_pos, &off(&city_pos)),
        ),
        (
            "more farmers than citizens",
            in_list("cities: [(", &farmers, "farmers: 99,"),
        ),
        (
            "a hundred tens of people",
            in_list("cities: [(", "tens: 0,", "tens: 100,"),
        ),
    ];
    for (what, edited) in cases {
        match load(&edited, what) {
            Err(LoadError::Inconsistent(_)) => {}
            other => panic!("{what}: {:?}", other.map(|_| "it loaded")),
        }
    }
}

#[test]
fn a_refusal_says_what_is_wrong_in_words() {
    let full = saved();
    let err = load(&full.replacen("farmers: ", "farmers: 99", 1), "farmers")
        .err()
        .expect("refused");
    let said = err.to_string();
    assert!(said.contains("impossible population"), "{said}");
    let said = load("nonsense", "nonsense")
        .err()
        .expect("refused")
        .to_string();
    assert!(said.contains("not a readable save"), "{said}");
    // A save of other content names the fingerprints, cut to a length a person can read.
    let snapshot = Snapshot::from_text(&full).expect("reads");
    let mut other = content();
    other.fingerprint = "abcdef0123456789".repeat(4);
    let Err(LoadError::Content(e)) = snapshot.restore(other) else {
        panic!("other content loaded")
    };
    let said = format!("{e:?}");
    assert!(said.contains("abcdef012345"), "{said}");
    assert!(!said.contains("abcdef0123456"), "{said}");
}

#[test]
fn a_recording_runs_from_its_start_to_its_stop_and_takes_back_what_undo_takes_back() {
    let (_, mut g) = worlds().remove(0);
    assert!(!g.is_recording());
    g.start_recording().expect("starts");
    assert!(g.is_recording());
    g.set_undo(true);
    assert!(g.undo_enabled());
    assert!(!g.can_undo());
    let before = g.replay_so_far().expect("recording").commands.len();
    a_change_that_can_be_taken_back(&mut g).expect("a change");
    assert_eq!(g.undo_label(), Some("SetTaxRate"));
    let during = g.replay_so_far().expect("recording").commands.len();
    assert!(during > before, "the change was not recorded");
    assert_eq!(g.undo().as_deref(), Ok("SetTaxRate"));
    assert_eq!(
        g.replay_so_far().expect("recording").commands.len(),
        before,
        "what was taken back is still in the recording"
    );
    g.set_undo(false);
    assert!(!g.undo_enabled());
    assert!(!g.can_undo());
    let replay = g.stop_recording().expect("a recording");
    assert_eq!(replay.commands.len(), before);
    assert!(!g.is_recording());
    assert!(g.stop_recording().is_none());
}

#[test]
fn the_units_a_recorded_command_names_are_told_apart() {
    let (_, mut g) = worlds().remove(0);
    g.start_recording().expect("starts");
    busy_round(&mut g, 6);
    let replay = g.stop_recording().expect("a recording");
    let moves: std::collections::BTreeSet<&str> = replay
        .commands
        .iter()
        .filter(|c| c.starts_with("MoveStack"))
        .map(|c| c.as_str())
        .collect();
    assert!(moves.len() >= 3, "{moves:?}");
    let units: std::collections::BTreeSet<&str> = moves
        .iter()
        .filter_map(|c| c.split("units:[").nth(1)?.split(']').next())
        .collect();
    assert!(units.len() >= 3, "the moves name too few units: {units:?}");
}

#[test]
fn a_save_of_a_version_with_one_player_for_each_city_is_brought_to_one_for_each_race() {
    use f4x_def::half_int::HalfInt;
    let base = Snapshot::from_text(&saved()).expect("reads");
    let race_player = base
        .players
        .iter()
        .find(|p| {
            p.neutral
                && (100..255).contains(&p.id)
                && base.cities.iter().filter(|c| c.owner == p.id).count() >= 2
        })
        .expect("a player of two independent cities, one of which gets a player of its own")
        .clone();
    let city_at = base
        .cities
        .iter()
        .find(|c| c.owner == race_player.id)
        .expect("a city of it")
        .pos;
    // The old game: that city has a player of its own, with a treasury of its own.
    let own = (100..255u8)
        .find(|id| base.players.iter().all(|p| p.id != *id))
        .expect("a free player number");
    let old_form = |version: &str| {
        let mut s = base.clone();
        s.game_version = version.to_string();
        let mut extra = race_player.clone();
        extra.id = own;
        extra.gold = HalfInt::from_int(7);
        s.players.push(extra);
        s.cities
            .iter_mut()
            .find(|c| c.pos == city_at)
            .expect("the city")
            .owner = own;
        // The upgrade is part of reading a save from text.
        Snapshot::from_text(&s.to_text().expect("writes")).expect("reads")
    };
    // Written before the change: the city goes to the player of its race, the gold is added up.
    let g = old_form("0.0.5")
        .restore(content())
        .expect("an old save loads");
    assert!(
        g.player(f4x_core::player::PlayerId(own)).is_none(),
        "the old player is still there"
    );
    let city = g.economy().city_at(city_at).expect("the city");
    assert_eq!(
        g.economy().city_owner(city),
        Some(f4x_core::player::PlayerId(race_player.id))
    );
    let gold = g
        .player(f4x_core::player::PlayerId(race_player.id))
        .expect("the player")
        .treasury()
        .gold;
    assert_eq!(gold, race_player.gold + HalfInt::from_int(7));
    // Written in the version of the change: it is not touched.
    let g = old_form("0.0.6").restore(content()).expect("loads");
    let city = g.economy().city_at(city_at).expect("the city");
    assert_eq!(
        g.economy().city_owner(city),
        Some(f4x_core::player::PlayerId(own))
    );
}

#[test]
fn a_replay_of_a_game_version_this_game_cannot_read_is_refused() {
    use f4x_core::game::replay::{Replay, ReplayError};
    use f4x_core::game::save::GAME_VERSION;
    let (_, mut g) = worlds().remove(0);
    g.start_recording().expect("starts");
    busy_round(&mut g, 2);
    let text = g
        .stop_recording()
        .expect("recorded")
        .to_text()
        .expect("writes");
    assert!(Replay::from_text(&text).is_ok());
    let from = format!("game_version: \"{GAME_VERSION}\"");
    assert!(text.contains(&from), "the replay does not say its version");
    for version in ["9999.0.0", "0.0.0", "banana"] {
        let edited = text.replacen(&from, &format!("game_version: \"{version}\""), 1);
        assert!(
            matches!(Replay::from_text(&edited), Err(ReplayError::Load(_))),
            "{version}"
        );
    }
}

#[test]
fn a_banner_and_a_portrait_out_of_range_are_brought_into_it_and_a_feature_off_the_map_is_refused() {
    let base = Snapshot::from_text(&saved()).expect("reads");
    let mut s = base.clone();
    s.wizards[0].banner = 250;
    s.wizards[0].portrait = 250;
    let back = s.restore(content()).expect("loads").snapshot();
    assert_eq!(back.wizards[0].portrait, f4x_core::player::PORTRAITS - 1);
    assert_eq!(back.wizards[0].banner, f4x_core::player::NEUTRAL_BANNER);
    for (what, edit) in [
        ("east of the map", (9999u32, None)),
        ("south of the map", (0, Some(9999u32))),
    ] {
        let mut s = base.clone();
        let f = s.features.first_mut().expect("a feature");
        match edit.1 {
            None => f.at.x = edit.0,
            Some(y) => f.at.y = y,
        }
        assert!(
            matches!(s.restore(content()), Err(LoadError::Inconsistent(_))),
            "a feature {what} was accepted"
        );
    }
}
