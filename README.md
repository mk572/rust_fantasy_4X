# rust_fantasy_4X

**Version: 0.0.3**

A fantasy 4X strategy game written in Rust. It starts from a Master of Magic-style design (two planes, wizards and spell-casting, heroes, tactical battles) but is expected to deviate over time. It is a gameplay-inspired reimagining, not a port: no original game IP is committed to this repository: no original art, music, sound or help text, and no original code.

- Design: [`docs/design.md`](docs/design.md), rules layer: [`docs/rules-framework.md`](docs/rules-framework.md), map: [`docs/map-layers.md`](docs/map-layers.md), game flow: [`docs/game-flow.md`](docs/game-flow.md), state storage and ECS: [`docs/ecs-structure.md`](docs/ecs-structure.md), all screens of the original and their status: [`docs/screen_list.md`](docs/screen_list.md), the original's hotkeys: [`docs/hotkeys.md`](docs/hotkeys.md)
- Local-only original assets and data: [`docs/local-assets.md`](docs/local-assets.md), art extraction, palettes and fonts: [`docs/extract-art.md`](docs/extract-art.md), sound and music catalogue: [`docs/sound.md`](docs/sound.md)

## Run

```
cargo test --workspace
cargo run -p f4x-client            # uses content-local/ and assets-local/ if present
```

Client keys follow the original's (the original's against ours: `docs/hotkeys.md`): N or Enter end turn, M magic screen, S spells, B found a city, arrows or numpad move the selected stack (arrows scroll when nothing is selected), P or Tab switch plane, +/- zoom, F10 quick-save, Ctrl-L quick-load (also autosaved each turn to `saves/`), Esc game menu. The keys can be changed (`content/hotkeys.ron`, `docs/hotkey-settings.md`).

Use ` (backtick) to open the in-game console.

## Music and the Roland MT-32 ROMs

The original's music was written for the Roland MT-32. The songs are
rendered once, from your own copy of the original, by `tools/xmi-play` (`docs/sound.md`), which uses the MT-32
emulator [`mt32-rs`](https://github.com/CopperlineHQ/mt32-rs) (a git submodule in `third_party/mt32-rs`, with its own
licence, LGPL-2.1-or-later). The emulator needs the ROM images of a real unit.
They are Roland's copyright and are **not included**: You can get the MAME-versioned set from
<https://archive.org/details/mame-versioned-roland-mt-32-and-cm-32l-rom-files> (the author uses the new
revision: `mt32_ctrl_2_07.rom` and `mt32_pcm.rom`) and put them in `reference/Roland MT-32/rom-new/`.
Clone with `git clone --recurse-submodules`, or run
`git submodule update --init` afterwards.
