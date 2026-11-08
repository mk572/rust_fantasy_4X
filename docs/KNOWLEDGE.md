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
- **Guard.** `f4x-tools game-methods` reports the files with more than 12 methods on `Game`; it is
  a tool, not a test, so the test run does not pay for it. The singular-name
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
- **Forester, Mountaineer and Pathfinding do nothing in battle.** The original's `Battle_Unit_Movement_Mode`
  maps them all to ground movement; the battle's trees and rocks cost them the same as anyone. A `Min` on
  `Moves` (Flight) is applied before the bonuses, not after (`rules/stats.rs`).
- **ReMoM's `FLIC_Draw` moves the animation on a frame each time it is called.** Code that draws one picture
  several times in a row is layering successive frames, not drawing one frame again: the city's river and
  ocean overlay (`Cityscape_Draw_Background`) is drawn `water counter / 2 + 1` times so the water builds up, the
  victory scene's hands draw frames 0 to the current one over each other, and the conquest scene's dissolving wizard
  does the same. `Set_Animation_Frame` first sets where it starts; a frame number past the end loops back (to the
  loop frame, 0 for these pictures). Read the call count before you read the frame number.


- **A `GoTo` that fails midway is dropped without a word.** `Game::follow_orders` clears the whole path on any
  error but `NoMovesLeft`, and `ai.send` still reports success because the order was accepted. A ship that was
  sent home to a city holding nine units (`StackFull`) sat outside for 80 turns while the AI believed it was on
  its way. When a unit "has an order" and does not move, print the `MoveError` in `follow_orders` first.
- **`scripts/fork_ai.py` must give the copy new mtimes** (it does since 30 October 2026, `copy_function=shutil.copy`):
  with `copytree`'s preserved mtimes cargo keeps the old build of `f4x-ai-ref` and `soak-versus` plays a stale
  reference. Check a new reference with a null test (reference = identical copy gives identical columns).
- **A rebellion makes a city independent without any event in an all-AI game.** `EventKind::Rebellion` calls
  `make_city_neutral`, and the notice is only pushed when a human exists or hears of it (`play_event`), so the
  soak's tally cannot count the loss and `founded_cities_account_for_the_cities_a_wizard_ends_with` fails on a
  seed where one happens. The test pins seed 4, where none does; a different random stream (any AI change
  that rolls the dice more or less often) may need another seed.

## A font atlas must stay under 16384 pixels in each direction

The BMFont atlases stack one copy of the glyphs per colour set (16 of the original + the ramps of
`scripts/font_ramps.py`). With 16 glyphs per row, styles 4 and 5 passed 16384 pixels in height: the
texture failed to load ("unloadable" in the log) and every text of the style was a solid black box.
`BM_COLUMNS` in `f4x-tools/src/fonts.rs` is 48 for this reason; check the atlas size when adding ramps.

## The books hide the realm titles on the new-game picks screen (as in the original)

On the spell picks screen the books of a realm are drawn over the realm's title ("Life" shows as
"ife", "Nature" as "ture"). The original does the same: it is intended, not a bug. Do not move the
books or the titles to avoid it.


## A bare integer beside a HalfInt compiles and means whole units

`HalfInt` compares with and adds an `i32` as whole units (`PartialEq<i32>`), so a test that held a count
of half-moves (`assert_eq!(moves_left, 4)`) still compiles after the conversion and fails only when it
runs (4 is now four moves, not two). After converting an amount that was counted in halves, halve every
literal that is compared with it, and run the tests rather than trusting the compiler. A compile error
("expected `HalfInt`, found integer") is the easy case; the silent one is the comparison.

## Saved numbers keep their meaning unless a conversion says otherwise

`HalfInt` is written as the amount (`1.5`). A field that older saves held as a raw half count (a unit's
moves, an item's movement bonus, a battle unit's moves) uses `#[serde(with = "f4x_def::half_int::raw")]`,
which keeps the old number, so no `save::UPGRADES` entry or version bump is needed. Use `UPGRADES` only
when the saved number itself must change.

- **A computer wizard's vision is counted once per turn and patched** (task 428, `map/vision.rs`). `Ai::play_turn`
  calls `Game::keep_vision`; `PlayerView::sees` then reads a `VisionGrid` of counts (how many units and cities see
  each square) instead of counting it for each new view. `Game::execute` calls `vision_after` with the command's
  `VisionEffect`: `None` for the commands that cannot touch a unit, a city or a spell (orders, production, farmers,
  taxes, research, Alchemy, diplomacy, refusing an offer, cancelling a cast), `Walk` for `MoveStack` and `GoTo`
  (the units whose square changed are lifted off the old square and put on the new one; the counts of the rest stay),
  `Recount` for every other command, and for any command whose events are not all harmless (`with_events`: a walk
  may raise only `UnitsMoved`, `AttackRequested`, `SiteEncounter` or a refusal). A new command is a `Recount` until it
  is named. Anything that changes the game outside `Game::apply` during the turn breaks this; the AI's only such
  calls (`balance_farmers`, `set_plans`) cannot change what is seen. `F4X_CHECK_VISION=1 f4x-tools soak ...` counts
  afresh after every command and panics when the kept grid differs (the test of the cache; slow).

