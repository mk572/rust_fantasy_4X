# rust_fantasy_4X

**Version: 0.0.2** (set in `Cargo.toml`; saves and replays of another version will be refused, task 335).

A fantasy 4X strategy game written in Rust. It starts from a Master of Magic-style design (two planes, wizards and spell-casting, heroes, tactical battles) but is expected to deviate over time. It is a gameplay-inspired reimagining, not a port: nothing protected by intellectual property is committed to this repository: no original art, music, sound or help text, and no original code. The game's rules (the names and numbers of races, units, buildings, spells and items) are data like any other and are committed.

- Design: [`docs/design.md`](docs/design.md), rules layer: [`docs/rules-framework.md`](docs/rules-framework.md), map: [`docs/map-layers.md`](docs/map-layers.md), game flow: [`docs/game-flow.md`](docs/game-flow.md), state storage and ECS: [`docs/ecs-structure.md`](docs/ecs-structure.md), all screens of the original and their status: [`docs/screen_list.md`](docs/screen_list.md), the original's hotkeys: [`docs/hotkeys.md`](docs/hotkeys.md)
- Local-only original assets and data: [`docs/local-assets.md`](docs/local-assets.md), art extraction, palettes and fonts: [`docs/extract-art.md`](docs/extract-art.md)

## Run

```
cargo test --workspace
cargo run -p f4x-client            # uses content-local/ and assets-local/ if present
```

Client keys follow the original's (the full table is in `docs/hotkeys.md`): N or Enter end turn, M magic screen, S spells, B found a city, arrows or numpad move the selected stack (arrows scroll when nothing is selected), P or Tab switch plane, +/- zoom, F10 quick-save, Ctrl-L quick-load (also autosaved each turn to `saves/`), Ctrl-N new map, Ctrl-R change race, Esc game menu.
`--shot FILE [--city]` saves a screenshot and exits (for automated checks).
