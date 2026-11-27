//! Class 1 for the game: a refused command leaves nothing in the replay and nothing to undo, and
//! changes nothing in a game under way either. The sweep over a world alone is in core
//! (`c1_refusal.rs`).

use f4x_core::world::testutil::sweep::commands;
use f4x_core::world::Event;

use crate::support::{busy_round, games, state};

/// Try every command of the sweep on each world, warmed up with `warm` rounds of play first, and check
/// each refusal. Returns how many commands were tried and how many were refused.
fn sweep(every: usize, warm: usize) -> (usize, usize) {
    let mut tried = 0;
    let mut refused = 0;
    let mut recorded = 0;
    for (name, mut w) in games().into_iter().step_by(every) {
        for _ in 0..warm {
            busy_round(&mut w, 6);
        }
        w.start_recording();
        w.set_undo(true);
        // The game goes on changing as commands are accepted (a unit is dismissed, a city founded), so
        // later commands name things that are gone: a stale handle is one more kind of bad input.
        for command in commands(&w) {
            let label = format!("{command:?}");
            let before = state(&w);
            let history = (
                w.replay_so_far().ok().map(|r| r.entries.len()),
                w.can_undo(),
                w.undo_label().map(str::to_owned),
            );
            let events =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.command(command)))
                    .unwrap_or_else(|_| panic!("{name}: {label} panicked"));
            tried += 1;
            if events.iter().any(|e| matches!(e, Event::Rejected(_))) {
                refused += 1;
                assert_eq!(
                    state(&w),
                    before,
                    "{name}: {label} was refused, and changed the game"
                );
                // A command that cannot be written down (it names a unit that is gone) ends the
                // recording; as long as there is one, a refusal adds nothing to it.
                if let (Some(was), Ok(now)) = (history.0, w.replay_so_far()) {
                    recorded += 1;
                    assert_eq!(
                        now.entries.len(),
                        was,
                        "{name}: {label} was refused and recorded"
                    );
                }
                assert_eq!(
                    (w.can_undo(), w.undo_label().map(str::to_owned)),
                    (history.1, history.2),
                    "{name}: {label} was refused and changed the undo history"
                );
            }
        }
    }
    assert!(
        recorded > 300,
        "the recording was checked only {recorded} times in {tried}"
    );
    (tried, refused)
}

#[test]
fn a_refused_command_leaves_no_trace_in_the_replay_or_the_undo_history() {
    // One of the four worlds in the normal run, all four in the deep one.
    let every = if f4x_def::testdir::deep() { 1 } else { 4 };
    sweep(every, 0);
}

/// The same on every world, each after rounds of play, so that refusals are tried against games that
/// have more going on (orders given, units hurt, cities grown). Slow: run it with `--ignored`.
#[test]
#[ignore]
fn a_refused_command_changes_nothing_in_a_game_under_way() {
    for warm in [4, 12, 25] {
        let (tried, refused) = sweep(1, warm);
        assert!(
            refused > tried / 4,
            "after {warm} rounds: only {refused} of {tried} were refused"
        );
    }
}
