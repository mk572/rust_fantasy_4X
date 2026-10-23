# What the entity rework taught (tasks 307 to 337)

Non-obvious findings about this code base. General lessons (shell, tools) are in
`~/projects/KNOWLEDGE_BASE.md`; the design itself is in `ecs-structure.md`.

- **Run a domain's tests on a real `Game`, never on a fake of the world.** The diplomacy tests ran on
  a `FakeView` until task 336. On the real game the Spell of Mastery test failed (-80 instead of
  -25): the first half of the test had put two players at war at random. The fake had hidden a state the
  game reaches in play. A test that needs a fixed relation sets it (`set_rel` in
  `diplomacy/tests.rs`). Code kept only so a fake compiles (a trait method, a wrapper) is a smell:
  build test entities instead.
- **A player is not always a wizard.** Independent cities' players and the monsters' player
  (`MONSTERS`, 255) have `Neutral` and no `WizardLink`. The monsters' player is in no player list
  (`Game::has_player` is false for it). Handles answer for a missing wizard (`Player::banner` is
  `NEUTRAL_BANNER`, `portrait` is 0); `Player::treasury` is 0/0 when missing. Keep a fallback where
  an id might be unknown (console input, a defeated neutral player).
- **A handle borrows the `Game`.** In the client a `Player<'_>` or `Wizard<'_>` held across a
  `&mut self` call (a draw, a command) does not compile. Read what the screen needs into locals first.
- **Lending state to a domain.** `Game::with_diplomacy` takes the state and the RNG out of the game,
  runs the rules over `&Game`, puts both back and applies the outcomes. The domain never sees
  `&mut Game`, and the RNG draw order stays what the save replays.
- **Mass call-site rewrites.** A regex over `.treasury(` or `.history(` also hits methods of other
  types with the same name, and receivers such as `dyn Trait` or test rigs. Rewrite, then let the
  compiler list what broke (`cargo check --workspace --tests`), fix by hand, and diff the result.
  Check the scratch script's location: the scratchpad directory, not `/tmp`.
- **Guard.** `tests/game_methods.rs` reports `impl Game` growth per file against
  `game_method_limits.txt` (regenerate with `F4X_WRITE_LIMITS=1`). It never fails. The singular-name
  rule has no test on purpose: it is a guideline (`CLAUDE.md`), and a hard test would fail in
  surprising ways later.
- **A battle can be won without its command saying so.** `Battle::winner` is set by `check_winner`, and
  `Ended` is only in the events of the command that set it. A cast, a field refresh or an end-of-turn
  effect can settle it earlier, and then `battle_command` found no `Ended` and left the battle open: every
  later `AutoResolve`, `EnterSite` and `BeginBattle` was refused (`InProgress`, `Busy`) for the rest of the
  game (found by the soak, 4 players seed 2, turn 219). `Game::finish_if_over` now closes any battle with a
  winner after every battle command and after `auto_resolve`. `f4x-tools soak` reports "rounds ended with
  a battle open".
- **A search over the world costs queries per step.** `unit_abilities` reads the unit's spell instances and
  items (a hecs query over all archetypes each), and `hostile_at` and `lair_at` scan units and features.
  Called for each of the eight neighbours of each square, a reachability search of the AI took 100 ms.
  `reachable_squares` and `find_path` now take a unit's `StepProfile` once and remember which squares stop
  the walk. Do the same for any new search: take what does not depend on the square out of the loop.

- **An outpost becomes a city at 10 tens, not 100.** ReMoM's `Apply_City_Changes` promotes at
  `Pop_10s >= 10` and keeps the tens, a new outpost starts with 3 (`Outpost.c`), and one at 0 or less is
  destroyed. We used 100, start 0 and no death, which made outposts about ten times too slow (about 230
  turns instead of 16) and looked like an AI problem in the soak ("most outposts are still outposts after
  200 turns"). When a number of the engine looks off in a soak, check the original's constant first.
