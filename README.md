# rust_fantasy_4X

A fantasy 4X strategy game written in Rust. It starts from a Master of Magic-style design (two planes, wizards and spell-casting, heroes, tactical battles) but is expected to deviate over time. It is a gameplay-inspired reimagining, not a port: no original code, data or art is committed to this repository.

- Design: [`docs/design.md`](docs/design.md), rules layer: [`docs/rules-framework.md`](docs/rules-framework.md), state storage and ECS: [`docs/ecs-structure.md`](docs/ecs-structure.md)
- Local-only original assets and data: [`docs/local-assets.md`](docs/local-assets.md)

## Run

```
cargo test --workspace
cargo run -p f4x-client            # uses content-local/ and assets-local/ if present
```

Client keys: arrows/WASD scroll, Tab switch plane, +/- zoom, B found a city at the cursor, click a city for its screen, Enter end turn, N new map, R change race, Esc quit.
`--shot FILE [--city]` saves a screenshot and exits (for automated checks).
