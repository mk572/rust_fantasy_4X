# Test review log

Review of every `.rs` file's tests, in alphabetical order by folder and file (`f4x-ai-ref` is skipped:
it is a generated copy). Checks: fixtures that should be the shipped game data, tests of outdated code,
slow or wasteful tests (cloning), `unwrap`/`expect`/`panic!` instead of error handling. Rules:
`rust_coding_rules.md`, "Testing".

| File | Status | Notes |
|---|---|---|
| `crates/f4x-ai/src/diplomacy.rs` | done | Test `game()` now loads the shipped content (before: empty `Content::default()`, so the spell-swap check in `a_pact_is_refused...` was silently skipped; it now asserts). One seeded helper replaces three copies of the two-player setup. 9 `unwrap`/`expect` sites replaced, tests return `Result`. `testworld::content()` parses the shipped content once per process and hands out copies: the two seed-loop tests went from 0.37 s to under 0.06 s. |
