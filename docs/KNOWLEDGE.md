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
