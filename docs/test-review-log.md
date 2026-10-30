# Test review log

Review of every `.rs` file's tests, in alphabetical order by folder and file (`f4x-ai-ref` is skipped:
it is a generated copy). Checks: fixtures that should be the shipped game data, tests of outdated code,
slow or wasteful tests (cloning), `unwrap`/`expect`/`panic!` instead of error handling, files without tests
or a `//! Tests:` line, stale or wordy comments (task numbers, history, general project information). Rules:
`rust_coding_rules.md`, "Testing".

| File | Status | Notes |
|---|---|---|
| `crates/f4x-ai/src/diplomacy.rs` | done | Test `game()` now loads the shipped content (before: empty `Content::default()`, so the spell-swap check in `a_pact_is_refused...` was silently skipped; it now asserts). One seeded helper replaces three copies of the two-player setup. 9 `unwrap`/`expect` sites replaced, tests return `Result`. `testworld::content()` parses the shipped content once per process and hands out copies: the two seed-loop tests went from 0.37 s to under 0.06 s. |
| `crates/f4x-ai/src/economy.rs` | done | Already on the shipped content. Three tests return `Result`; one `capital(&g)` helper replaces the repeated city lookup with `unwrap`. |
| `crates/f4x-ai/src/expand.rs` | done | Had no tests. Added five on the shipped content: a settler walks to a site and founds a city; two settlers keep `SPACING` apart; a settler that did not move all turn gives its site up (`bad_sites`) and chooses another; a settler with no explored site scouts; the city name comes from the race's list. Not covered: targets of settlers that died are forgotten (no way to remove a unit in a test without an invented situation). |
| `crates/f4x-ai/src/lib.rs` | done | Only module declarations and `Ai`; the `#[cfg(test)]` is the `testworld` declaration. Covered by the module tests. Has the `//! Tests:` line the rule "Every file has tests" asks for. |
| `crates/f4x-ai/src/military.rs` | done | Clean: shipped content, no unwraps. Reuses `P0`/`P1` from `testworld`. |
| `crates/f4x-ai/src/monster.rs` | done | 2 unwraps replaced; the silent `if let` on the city owner is an assertion; the dead `units` variable is gone; `monsters_that_see_a_defended_city...` was satisfied by any event and now asserts the battle (monsters attack, win), the rampage and the dead defender. |
| comments, `crates/f4x-ai/src` (files above) | done | Removed task numbers (341, 376) and the stale `lib.rs` paragraph ("first version is deliberately simple", "tasks 64 to 67 add ..."; all four are done). Only task and history wording was searched in the other five files; `diplomacy.rs` was read in full (below). |
| comments, `crates/f4x-ai/src/diplomacy.rs` | done | Read all 60 comments. Section references in `docs/diplomacy.md` (4, 8) are right. Kept the doc comments that give the original's rule or formula. Shortened the module doc (it described the core/AI split), `approach_human` (listed what its body does, with the odds that are in the constants) and `negotiate`. Test comments are short and say why. |
