# References

The other projects and sources this one is checked against. None of their code is used in the game;
they are read to find out what the original *Master of Magic* does. The local copies of the original's
files and of ReMoM are described in [`local-assets.md`](local-assets.md); art and music we use from
elsewhere is in `reference/CREDITS.md`.

## Clones and reverse-engineerings of Master of Magic

| Project | Link | Use |
|---|---|---|
| **ReMoM** (jbalcomb) | <https://github.com/jbalcomb/ReMoM> | A reverse-engineering of the original's code in C. Our main reference: unit, race and building tables, the rules in `MoM/src/*.c`, the notes in `doc/`. Cloned at `~/projects/ReMoM`; the export scripts read it (`--remom DIR`). A private full-history mirror is `ReMoM-reference` on GitHub. |
| **openmom** (Jakz) | <https://github.com/Jakz/openmom> | A reimplementation in Java. A second opinion when ReMoM is unclear or has a gap. Not cloned yet. |
| **master-of-magic** (kazzmir) | <https://github.com/kazzmir/master-of-magic> | A remake in Go. Another cross-check for rules and screens. Not cloned yet. |

## Tools and libraries

| Project | Link | Use |
|---|---|---|
| **mt32-rs** (CopperlineHQ) | <https://github.com/CopperlineHQ/mt32-rs> | A Rust port of the Roland MT-32 emulator, a git submodule in `third_party/mt32-rs`; plays the original's music (see the README). |
| **cnc-formats** (iron-curtain-engine) | <https://github.com/iron-curtain-engine/cnc-formats> | Reads the XMI music files (`XmiTimbre`, `to_mid()`), see the done task about music. |

## Art

| Source | Link | Use |
|---|---|---|
| **[LPC] Ship** (bluecarrot16) | <https://opengameart.org/content/lpc-ship> | Ship pictures, tried and not used (task 373). Licence and notice: `reference/CREDITS.md`. |

## Wikis and guides (saved copies in `reference/`)

The Master of Magic Wiki pages on the Community Patch, the Unofficial Patch 1.50, the Insecticide patch, the
Consolidation Patch (v1.31c) and the races, and the Caster of Magic page, are saved in `reference/` as HTML
files. They tell what the patches changed and what the original's bugs are (`docs/help-vs-implementation-conflicts.md`).

## How to use them

- Rules come from the original game first (its files, then ReMoM), then from the patches' notes.
- Write the source in the doc or test that uses it (`File.c:function` for ReMoM), as `docs/heroes-items-design.md` does.
- Do not copy code or text from a clone into the project; the original's own prose stays in `content-local/`
  (never committed).
