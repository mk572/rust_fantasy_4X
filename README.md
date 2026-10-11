# rust_fantasy_4X

A fantasy 4X strategy game written in Rust. It starts from a Master of Magic-style design (two planes, wizards and spell-casting, heroes, tactical battles) but is expected to deviate over time. It is a gameplay-inspired reimagining, not a port: nothing protected by intellectual property is committed to this repository: no original art, music, sound or help text, and no original code. The game's rules (the names and numbers of races, units, buildings, spells and items) are data like any other and are committed.

- Design: [`docs/design.md`](docs/design.md), rules layer: [`docs/rules-framework.md`](docs/rules-framework.md), map: [`docs/map-layers.md`](docs/map-layers.md), game flow: [`docs/game-flow.md`](docs/game-flow.md), state storage and ECS: [`docs/ecs-structure.md`](docs/ecs-structure.md)
- Local-only original assets and data: [`docs/local-assets.md`](docs/local-assets.md)

## Run

```
cargo test --workspace
cargo run -p f4x-client            # uses content-local/ and assets-local/ if present
```

Client keys: arrows/WASD scroll, Tab switch plane, +/- zoom, B found a city at the cursor, click a city for its screen, Enter end turn, M magic screen, N new map, R change race, F5 quick-save, F9 quick-load (also autosaved each turn to `saves/`), Esc quit.
`--shot FILE [--city]` saves a screenshot and exits (for automated checks).
