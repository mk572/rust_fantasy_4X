# rust_fantasy_4X: project notes

Context for picking this project up in a fresh session. User-facing docs are in
`README.md`, `docs/design.md` (architecture), `docs/rules-framework.md` (rules
design and status) and `docs/modding.md` (content authoring). This file is the
"why", the decisions, and the loose ends.

## What this is

A fantasy 4X strategy game in Rust, a gameplay-inspired reimagining of Microprose's
Master of Magic (two planes, wizards and spell books, heroes, tactical battles).
Not a port: no original code, data or art is used or copied. It will deviate from
MoM, but **only after parity**: first implement everything in `docs/design.md` as MoM
does it; when in doubt, follow MoM. Target platforms: Mac and Linux; low-res pixel
graphics scaled by integer factors.

## Decisions already made (do not re-litigate)

- Rust workspace: `f4x-core` (deterministic simulation, no I/O or rendering),
  `f4x-data` (content loading/validation), `f4x-ai`, `f4x-client`, `f4x-tools`.
- ECS inside the core via `hecs`; Cities, units etc. are entities, map tiles are a dense grid.
- Rules: **MoM-scale small numbers**; **five realms / spell-book structure as MoM**;
  **`Stat` is a fixed enum in code** (content cannot invent stats); **heroes and items
  are in from day one**.
- Everything is data-driven RON in content packs (`content/base/` plus mods that
  override/add/remove by id). Presentation (`art.ron`) is separate from rules.
- Determinism: integer math in rules, seeded `Pcg64Mcg`, no `HashMap` iteration in
  logic, all state changes through `Game::apply(Command) -> Vec<Event>`.
- Art: original game art is copyrighted and **must never be committed**. Placeholder
  art may be extracted locally from the author's own Steam/GOG copy into
  `assets-local/` (gitignored). Client falls back to flat colours. Final art must be
  original or freely licensed before any public release.
- Authorization/secrecy-style rules are enforced in core code, not just by convention.

## Where things are

- `crates/f4x-core/src/`: `map.rs` (two-plane grid, x wraps, feature overlay layer),
  `mapgen.rs` + `featuregen.rs` (deterministic generator, towers/nodes/encounters/minerals),
  `city.rs` (founding rules), `units.rs`, `production.rs`, `art.rs`,
  `rules/` (defs, registry, typed ids, `stats.rs` modifier engine, `effects.rs` vocabulary),
  `testutil.rs` (hand-built test content and worlds).
- `crates/f4x-data/src/`: `pack.rs` (pack loading, merge, art validation), `rules.rs`
  (cross-reference validation), `lib.rs` (terrain/feature validation), `testutil.rs`.
- `content/base/`: the built-in pack. `f4x-tools validate | list | mapgen`.

## Working conventions

- Before pushing: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `cargo run -p f4x-tools -- validate`. CI runs the same on
  Linux and macOS.
- Git: work on a `claude/...` branch; the user asks explicitly for merges to `main`
  (fast-forward). Do **not** open PRs unless asked. Commit messages end with the
  attribution lines the session provides.
- Tests for content rules use edited copies of the real base pack
  (`f4x-data/src/testutil.rs`); core tests use hand-built RON in `f4x-core/src/testutil.rs`.
- New validation rules should come with a test that breaks the shipped content in
  exactly that way.

## Environment notes (cloud sandbox)

- Outbound access is restricted: GitHub works; wikis, forums and most other sites are
  blocked (403). For MoM reference facts, the Go clone `kazzmir/master-of-magic` on
  GitHub was read (read-only, via git clone) for terrain/mineral/encounter tables;
  those numbers are reference starting points, not authoritative.
- The GitHub integration cannot create repositories (403); the user creates them.
- `cargo` and a Rust toolchain are available; crates.io works.

## State and loose ends

Done: map + generator (Myrror-specific minerals, towers, nodes, encounters), content
packs + art data, rules definitions and validation, stats engine, units (stats,
movement, equipment), cities (founding, buildings, production queue, upkeep).

Next, roughly in order (see `docs/rules-framework.md` section 7 and `docs/design.md` section 11):
1. Effect executor: conditions, targets, triggered effects (`OnCast`, `OnTurnStart`, ...).
2. City economy beyond production: population growth, food, gold, unrest.
3. Wizards/players as entities: mana, research, spell books, casting, enchantments, treasuries and an item inventory.
4. Combat resolution (auto-resolve first, then the tactical battle).
5. Hero hiring, AI, diplomacy, then the minimal client (framebuffer, scaling, map scroll, assets-local placeholders).

Known guesses to revisit (tunable, not verified against MoM): mountains cannot host
cities; minimum city distance 3; tower spacing 10, node spacing 4; node type weights per
terrain; "a unit with any movement left may enter an adjacent tile"; lowest `Set`
modifier wins. Not modelled yet: node aura zones, rivers, lakes, shores, roads, volcanoes,
plane shifting, stronger Myrror nodes/encounters.

The earlier repo `mk572/creative_agent_rpg` is unrelated (an LLM council RPG). A copy of
the first design doc remains on its branch `claude/mom-style-4x-design`; it can be deleted.
