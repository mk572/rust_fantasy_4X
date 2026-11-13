# Lessons of this code base

Non-obvious findings, one rule up front in bold. General lessons (shell, tools) are in
`~/projects/KNOWLEDGE_BASE.md`; the design is in `ecs-structure.md`; the history of each task is in
`TASKS-DONE.md` and git.

## Tests

- **Run a domain's tests on a real `Game`, never on a fake world.** The diplomacy tests ran on a
  fake view until task 336; on the real game a test failed because two players were at war at random,
  a state the fake had hidden. A test that needs a fixed relation sets it (`set_rel`,
  `diplomacy/tests.rs`).
- **A crate above core cannot be used by core's unit tests** (task 458). `f4x-newgame` builds a `Game`,
  so it sits above `f4x-core`; a unit test in core that depended on it would see two copies of every
  type. Core tests that need a whole new game load a saved one (`game::testutil::fresh`,
  `crates/f4x-core/tests/saves/fresh_w*_s*.save`); the test `fixtures` in `f4x-newgame` fails when a file
  no longer equals what `new_game` makes. After an intended change to the setup or the generator,
  refresh the files (the command is in the failure message) and look at the core tests that depend on a
  seed's map. To check that a move of setup code changed nothing, rebuild the regression saves
  (`F4X_REFRESH_REGRESSION=saves`) and see that `git status tests/regression` stays empty.
- **A bare integer beside a `HalfInt` compiles and means whole units** (`PartialEq<i32>`). After
  converting an amount counted in halves, halve every literal compared with it and run the tests; the
  compiler only catches the other direction.

## The model

- **A player is not always a wizard.** The neutral players of the races (100 plus the race number) and the monsters' player
  (`MONSTERS`, 255) have `Neutral` and no `WizardLink`; the monsters' player is in no player list
  (`Game::has_player` is false). Handles answer for a missing wizard (`banner` is `NEUTRAL_BANNER`,
  `portrait` 0, `treasury` 0/0). Keep a fallback wherever an id might be unknown (console input, a
  defeated neutral player).
- **A handle borrows the `Game`.** A `Player<'_>` or `Wizard<'_>` held across a `&mut self` call does
  not compile; read what the screen needs into locals first.
- **Lending state to a domain.** `Game::with_diplomacy` takes the state and the RNG out of the game,
  runs the rules over `&Game`, puts both back and applies the outcomes: the domain never sees
  `&mut Game`, and the RNG draw order stays what saves replay.
- **Saved numbers keep their meaning unless a conversion says otherwise.** `HalfInt` is written as the
  amount (`1.5`). A field that older saves held as a raw half count uses
  `#[serde(with = "f4x_def::half_int::raw")]`, so no `save::UPGRADES` entry is needed; use `UPGRADES`
  only when the saved number itself must change.
- **Check first, change second** (tasks 547 to 549). Any step that can answer `None` or `Err` goes
  before the first change (`scrap_item` took the item out, then failed on a missing treasury). A spell
  chosen on one turn and resolved later is checked at both times. A caller of a function with two
  failure causes must not answer both with the same fallback (`destroy_city` callers despawned the city
  on any error). A made-up fallback value is worse than a refusal. Where a rule decides a city is
  gone, call `Game::remove_city`, not `destroy_city`.
- **A battle can be won without its command saying so.** `Battle::winner` is set by `check_winner`;
  `Ended` is only in the events of the command that set it, and a cast or end-of-turn effect can settle
  it earlier. `Game::finish_if_over` closes any battle with a winner after every battle command and
  after `auto_resolve`.
- **A rebellion makes a city independent without any event in an all-AI game** (`EventKind::Rebellion`
  calls `make_city_neutral`; the notice is pushed only when a human hears of it), so the soak's tally
  cannot count the loss and `founded_cities_account_for_the_cities_a_wizard_ends_with` fails on a seed
  where one happens. The test pins seed 4; an AI change that rolls the dice differently may need another.
- **Guard.** `f4x-tools game-methods` reports files with more than 12 methods on `Game`. It is a tool,
  not a test; the singular-name rule has no test on purpose.

## Orders that stop (tasks 515, 555)

- **A step that fails must be reported, not dropped.** `follow_orders` used to end a `GoTo` on any refused
  step with `path.clear()` and no event, so units waited for hundreds of turns while the AI believed
  they were on their way. It now raises `Event::OrderFailed` (units, squares, reason), which the soak
  counts as a refused order; running out of moves stays silent because the order goes on.
- **A command can be accepted and still not go.** `Game::apply(GoTo)` gives no `Rejected` when the
  order is taken, and the first step can fail at once. `Ai::send` reports that as `SendError::Stopped`;
  a caller that sends several units to one square stops at the first refusal for want of room. When a
  soak counts refused orders, look at the sources first: one loop was two thirds of 13,888.

## Speed and caches (tasks 428, 512, 514)

- **Take what does not depend on the square out of a search's loop.** `unit_abilities` and `hostile_at`
  are queries over the world; for each of eight neighbours of each square a reachability search cost
  100 ms. `reachable_squares` and `find_path` take a unit's `StepProfile` once. Hash maps keyed by
  `Position` or `Entity` on a hot path use `crate::fast` (`FastMap`, `FastSet`).
- **A computer wizard's vision is counted once per turn and patched** (`map/vision.rs`).
  `Ai::play_turn` calls `Game::keep_for_turn`; `PlayerView::sees` reads a `VisionGrid` of counts.
  `Game::execute` calls `vision_after` with the command's `VisionEffect`: `None` for commands that
  cannot touch a unit, city or spell, `Walk` for `MoveStack` and `GoTo` (moved units are lifted and
  put down; the rest stays), `Recount` for everything else and for any command whose events are not all
  harmless. **A new command is a `Recount` until it is named.** Anything that changes the game outside
  `Game::apply` during the turn breaks this.
- **A unit's modifiers and abilities are kept for the same turn too** (`unit/kept.rs`, a thread-local
  cache armed by `keep_for_turn`). `Game::execute` empties it after every `Recount`; a command that
  changes what a unit is made of must therefore be one.
- **Prove a speedup by the games it plays.** Games are deterministic: build the commit before into a
  second worktree (`git worktree add DIR HEAD`, `CARGO_TARGET_DIR=OTHER cargo build --release -p
  f4x-tools`), play the bench snapshots with both binaries (`soak --from soak-out/bench/p5/seed25/t0400.f4x
  --players 5 --turns 460 --snapshots-every 20 --out DIR`) and `diff -r` the saves.
- **A cache gets an equality check on the read path, not where the value is made.** The env checks
  (`F4X_CHECK_VISION`, `F4X_CHECK_UNITS`, `F4X_CHECK_BUDGET`) work the answer out afresh and compare.
  The budget check once ran only when the value was worked out, so a stale memo (kept across a building
  that completed mid-turn) went unseen; equal saves on one bench game did not see it either. Check
  every hit, on late-game snapshots of 2 and 5 wizards and on new games, against a build that never
  caches. Refresh at the moments the old code recomputed, not where the new code thinks something changed.
- **Profile** with `sample PID 15 -file OUT` on `f4x-tools soak --from ...` in the background (the release
  binary keeps symbols): self time at the top of the stack, shares in the call graph tree.
- **Timing a soak: not through plain `cool_run.sh`.** It runs under `taskpolicy -b` (efficiency cores):
  820 ms a turn against 130 ms plain. Timing runs (`scripts/perf_guard.py`, `soak --every`) use
  `cool_run.sh --timing` or plain. `soak --every N` prints milliseconds per phase.

## The original (ReMoM) did it this way

- **An outpost becomes a city at 10 tens, not 100** (`Apply_City_Changes`: `Pop_10s >= 10`, keeps the
  tens, a new outpost starts with 3, one at 0 or less is destroyed). We had 100, start 0 and no death,
  which looked like an AI problem in the soak. When a number looks off in a soak, check the original's
  constant first.
- **Forester, Mountaineer and Pathfinding do nothing in battle** (`Battle_Unit_Movement_Mode` maps them
  to ground movement). A `Min` on `Moves` (Flight) is applied before the bonuses (`rules/stats.rs`).
- **`FLIC_Draw` advances the animation one frame per call.** Code that draws one picture several times
  in a row layers successive frames: the city's river and ocean overlay is drawn `water counter / 2 + 1`
  times, the victory and conquest scenes draw frames 0 to the current one over each other. Read the
  call count before the frame number.
- **The books hide the realm titles on the new-game picks screen** ("Life" shows as "ife"). The original
  does the same: do not move them.

## Tools and builds

- **A client screenshot is the same every time only with `--seed N`.** Two runs of `--city` without it differ in
  about a million pixels; with `--seed 5` they are equal. So a change in how pictures are read (task 471, the sprite
  packs) is checked by shooting a dozen screens (`--city`, `--armies`, `--unit-list`, `--magic`...) from the loose
  files and from the packed ones and counting differing pixels: 0.
- **A script that changes pictures works on loose files.** `f4x-extract unpack`, run it, `f4x-extract pack`
  (`docs/local-assets.md`). A loose file next to a sheet is merged into it by the next `pack`; until then the
  sheet wins, so an edited loose file looks unchanged in the client.
- **A font atlas must stay under 16384 pixels in each direction.** The BMFont atlases stack one copy
  of the glyphs per colour set; past the limit the texture fails to load and the text is a black box.
  `BM_COLUMNS` in `f4x-client/extract/src/fonts.rs` is 48 for this reason; check the size when adding ramps.
- **`scripts/fork_ai.py` must give the copy new mtimes** (`copy_function=shutil.copy`), or cargo keeps
  the old build of `f4x-ai-ref` and `soak-versus` plays a stale reference. Check a new reference with a
  null test (identical copy, identical columns).
- **`target/debug` grows without bound.** Cargo never deletes old test binaries and incremental
  caches (592 GB in 2.8 million files once). `cargo clean --profile dev` is always safe, but slow on a
  huge directory; do not `du` it meanwhile. The dev and test profiles keep `debug = "line-tables-only"`
  for workspace crates and none for dependencies; for a debugger use `cargo test --profile debug-full`.
  `scripts/prune_target.sh` cleans both past 10 GB; the pre-push hook (`--install-hook`) runs it.
- **A tally of per-player counts must have a slot for every player that can appear.** The soak's `Tally`
  (`f4x-tools/src/metrics.rs`) sized its slots by the players that existed at the start; the neutral race
  players are made later (when a city of the race turns neutral), so their counts fell into the monsters'
  slot. It now reserves `FIRST_RACE_PLAYER` plus the number of races.
