# Test report, 15 October 2026

State of the working tree before the next commit (last commit: task 56, first group). Nothing
here was fixed except the debug line noted under "Code check".

## Automated checks

| Check | Result |
|---|---|
| `cargo test --workspace` | all pass (f4x-ai behaviour 5, f4x-ai unit 5, core 531, others pass) |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --tests` | no warnings |
| `cargo test -p f4x-learn --features train` | pass (mirrored battle test included) |

## Soak: `f4x-tools soak --seeds 1..8 --turns 400 --every 100`

Full output: `log/soak-2026-10-15.txt` (git-ignored). I ran it twice; seeds 2 to 8 gave identical
lines both times (the first run's seed 1 line scrolled out of view).

| Seed | Result | Battles | Cities taken | Refused orders | Time |
|---|---|---|---|---|---|
| 1 | p0 wins by conquest, turn 400 (last turn of the run) | 86 | 22 | 65 | 5.7 s |
| 2 | no winner (p0 2 cities, p1 14) | 10 | 7 | 11 | 30 s |
| 3 | no winner (p0 9 cities, p1 3) | 66 | 29 | 40 | 4.4 s |
| 4 | no winner (p0 12, p1 1) | 6 | 6 | 0 | 11 s |
| 5 | no winner (13 against 13) | 8 | 13 | 13 | 208 s |
| 6 | p1 wins by conquest, turn 204 | 4 | 7 | 13 | 2.1 s |
| 7 | no winner (7 against 7) | 13 | 3 | 0 | 31 s |
| 8 | no winner (p0 13, p1 7) | 55 | 7 | 71 | 14 s |

**2 of 8 games ended.** `ai-design.md` section 8.1 records 3 of 8 (seeds 3, 6 and 8, at turns 251,
252 and 269). Seed 6 still ends, much earlier (204 against 252). Seeds 3 and 8 no longer end
and seed 1 now ends, only on the last turn. The doc's table is out of date or something
regressed; not yet found out which.

## Open findings

1. **Refused orders.** Six of eight seeds show them (up to 71). The AI should not send orders the
   game refuses. Source not yet found.
2. **Seed 5 takes 208 s** for 400 turns (the others take 2 to 31 s). The doc lists speed as not
   profiled.
3. **Soak table in `ai-design.md` 8.1 is stale** (see above).
4. **Task 230 (ships) is marked "not started"**, but `naval.rs` (472 lines) is built, wired into
   `Ai::play_turn` and the island test passes. Doc and tasks need to say what is really done.

## Code check of the new files

Read: `naval.rs`, `scout.rs`, `wizardry.rs`, `battle/brain.rs`, `tests/behaviour.rs`,
`f4x-learn/tests/mirror.rs`. Not read in full: the changed (not new) files.

- **Fixed:** `naval.rs` `sea_scout` held a leftover debug print (`AIDBG` environment variable,
  `eprintln!`). Removed; clippy and the f4x-ai tests pass after.
- `naval.rs` has no unit tests, only the island exploration test in `behaviour.rs`. No test
  checks that a landing happens. `scout.rs` likewise has none of its own.
- `naval.rs`: `view_turn` is a one-line wrapper over `view.turn()` while another call uses
  `game.turn`; harmless, worth tidying.
- `mirror.rs`: `bolt_scenario` takes a `content` argument it discards (`let _ = content;`).
- `wizardry.rs` `split` applies trial power splits straight with `game.apply`, then sends the
  final one through `ai.send`; works, but the trials skip the AI's own order accounting.
- No `todo!`, `unimplemented!`, `dbg!`, `TODO` or `FIXME` in the new files.
