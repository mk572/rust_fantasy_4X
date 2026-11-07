# Installers: design

Status: proposal, 2 November 2026. Nothing here is built. Related tasks: 81 (packaging for Mac),
176 (Mac installer), 363 (music from the MT-32 emulation), 460 (what is shipped in the executable).
Facts about this repository below were read from the code and measured; facts about other projects
and tools are from memory and marked **(verify)**. The spikes in section 9 check them.

## 1. The question and the short answer

The game must be set up on four targets: macOS on Apple silicon, macOS on Intel, Windows and Linux.
Setting up means: take the player's own copy of the original game, extract art, sound and text from
it, fetch the MT-32 ROMs for the music (after the player agrees), render the music, and end with a
game the player can start.

There is no established Rust way for this, and no Rust tool that does the import step. What exists
(section 4) packages and distributes a finished program. Games that need the player's original data
(OpenMW, ScummVM, OpenRCT2, devilutionX and the like **(verify)**) all do the same thing: the
program is distributed as an ordinary download with no original data in it, and **the program itself
does the data import, on first start or through a small setup tool**. Nobody makes an OS installer
that does it.

**Recommendation:**

1. Ship a **prebuilt game** per platform (built by GitHub Actions, no original material inside, CC0).
   Players do not compile.
2. Ship one **setup program, `f4x-setup`**, in the same download. It does the import: finds the
   original files, extracts art, audio and text into the player's data folder, asks before downloading
   the ROMs, renders the music, and verifies the result. It has no dependency on Python, `oggenc` or
   the Rust toolchain.
3. The game starts `f4x-setup` itself when its data folder is empty. A platform package (dmg, zip,
   AppImage) is only a wrapper around 1 and 2.
4. Keep "compile locally" as the developer path (`scripts/make_app.sh`), not the player path.

**Rule: `scripts/import_mom.py`, Python and ReMoM are for development only.** The player's machine
needs none of them, and neither does the game: the client and `f4x-setup` read the original's files
directly and nothing else. The script stays as the developer's tool for the local files (city names,
event texts, diplomacy lines, building descriptions) and is never shipped, run or
required by an installer.

This differs from the wording of the request in one place: the request says the installer builds an
executable. Section 5 says why a prebuilt executable is better for players, and what it costs. If the
author decides players must build, section 5, option A is the design for it.

## 2. What setting up consists of today

Measured or read in this repository:

| # | Step | Tool today | Needs on the player's machine | Writes |
|---|---|---|---|---|
| 1 | Find the original game folder, check it is the right one | none (paths are defaults in scripts) | | |
| 2 | Extract art (about 25,000 PNG) | `f4x-extract art` (Rust) | nothing | `assets-local/art`, `assets-local/screens` (86 MB and 53 MB) |
| 3 | Extract sound effects to OGG | `f4x-extract audio` (Rust) | `oggenc` or `ffmpeg` for the encoding | `assets-local/audio` |
| 4 | Extract the 118 songs as XMI | same | nothing | `music/*.xmi` |
| 5 | Local texts: city names, the wizards' diplomacy lines, event texts | `scripts/import_mom.py` (Python, developer only) | system Python and a clone of ReMoM: **not acceptable for players**, replaced by `f4x-import` (3.4) | `content-local/` (320 KB) |
| 6 | Get the MT-32 ROMs | by hand | | `reference/Roland MT-32/rom-new/` |
| 7 | Render the songs to OGG | `tools/xmi-play -R` (Rust, own workspace, LGPL `mt32-rs`) | the ROMs; `oggenc` or `ffmpeg`; about 2 minutes | `music/*.ogg` (129 MB) |
| 8 | Build the game | `cargo build --release -p f4x-client` via `make_app.sh` | Rust toolchain, a C toolchain, system libraries | `target/release/f4x-client` (12.6 MB) |
| 9 | Place content, assets and saves where the game finds them | `make_app.sh` launcher (macOS only) | | `~/Library/Application Support/Fantasy4X/` |

After extraction the game needs about 270 MB on disk (art 86 MB, screens 53 MB, audio 132 MB, of
which the music is 129 MB).

## 3. Constraints

### 3.1 Nothing original is ever distributed

The repository is CC0. The original's art, sound, music and prose are copyrighted and are never
committed or shipped (`design.md` section 9, `local-assets.md`). So every installer artifact must be
free of original material, and steps 2 to 7 must run on the player's machine from the player's own
copy. The installer must say that in plain words and never upload anything.

One existing exception to check: the six battlefield river pictures are committed and **embedded in
the client** (`river_art.rs`, kept at the owner's choice on 17 October 2026). A prebuilt, publicly
distributed binary would therefore contain a few original pictures. Decision needed (section 8,
number 6).

### 3.2 The MT-32 ROMs

The ROMs are Roland's copyrighted firmware. The project cannot include them. The request is that
the installer downloads them from archive.org after the user confirms. I can describe the design;
whether that download is acceptable is the author's decision, and I cannot judge the law. What the
design does to keep it as clean as it can be:

- The project ships **no ROM**. The URL lives in the setup program's configuration (one constant),
  so it can be changed or removed without touching the rest.
- The download happens only after a dialog that names the files, the source site and the size, says
  the ROMs are Roland's copyrighted firmware and that the player is responsible for having the right
  to use them, and has **Cancel on the left and OK on the right** (the project rule).
- The download is checked against pinned SHA-256 values of the two files (`mt32_ctrl_2_07.rom`,
  `mt32_pcm.rom`, the new revision the author picked in task 363). The author writes the hashes down
  once from files they trust; I do not have them and will not guess them.
- There is an equal second path: "I already have the ROMs", a folder picker. The download is a
  convenience, never the only way.
- **The music is optional.** If the player declines, the game installs without music, as it runs now
  without files (silent). Effects still play. The setup can be run again later (`f4x-setup --music`).
- Which Internet Archive item to use, and whether it is still there, the author must check. I did not
  look.

### 3.3 Licences of the parts

- The game and `f4x-setup` are CC0 and may depend only on compatible crates.
- `mt32-rs` is LGPL-2.1-or-later, which is why `tools/xmi-play` sits outside the workspace
  (task 363). The setup program must not link it. The music renderer therefore stays a **separate
  executable, `f4x-music`** (built from `tools/xmi-play`), which `f4x-setup` starts as a child process.
  Shipping it as a separate LGPL program with its source and licence text beside it is the usual way
  **(verify against the licence text before a release)**.

### 3.4 Dependencies that a player cannot be asked to have

- **Python and ReMoM: not needed by the player.** `import_mom.py` stays a developer script. What the
  game reads from `content-local/` is only three small files (`text/en/name/city.ron`, `diplomsg.ron`,
  `event_texts.ron`; the loaders are in `f4x-def/src/load`), and all three come from
  the original's LBX files alone:
  - `CITYNAME.LBX` gives the names per race in race-table order. The script takes the race list from
    ReMoM's `RACETYPE.c`; the Rust import takes it from the committed `crates/f4x-def/content/races.ron`, whose `num`
    is the same race number.
  - `DIPLOMSG.LBX` and `EVENTMSG.LBX` are read by the script without ReMoM.
  - The help screens (`crates/f4x-client/content/help_screens.ron`) are committed and edited by hand;
    the script no longer reads `HLPENTRY.LBX` or `HLPENTRY_LBX.md`.
  The rest of the script (the building texts check) is a development check and is not
  ported. The Rust import is checked against the script: both write the three files from the author's
  copy and the results must be identical.
- **`oggenc` / `ffmpeg`**: steps 3 and 7. Options: (a) write WAV and skip encoding: the client
  already plays `.wav`, but 129 MB of OGG music becomes well over a gigabyte of WAV, too large.
  (b) Use a Rust binding that compiles `libvorbis` into the tool (for example `vorbis_rs` **(verify the
  crate, its licence and that it builds on all four targets)**). (c) Ship `oggenc` beside the setup
  program (a binary per platform). Suggested: (b), checked in a spike; (c) is the fallback.
- **Rust toolchain and a C compiler**: only needed for option A.

### 3.5 Where the game finds its files

`crates/f4x-client/src/main.rs` looks for `content-local`/`content`, `assets-local/art`,
`assets-local/audio` and `saves/` **relative to the working directory**. On macOS the launcher script
links the data folder and starts the game from there. That does not carry over to Windows and Linux.
Needed: a **data folder per user**, found by the program itself:

| OS | Data folder (saves, imported art and text, rendered music) | Read-only game files (`crates/f4x-def/content/`, the binary) |
|---|---|---|
| macOS | `~/Library/Application Support/Fantasy4X/` | inside the `.app` |
| Windows | `%APPDATA%\Fantasy4X\` (or `%LOCALAPPDATA%`) | next to the `.exe` |
| Linux | `$XDG_DATA_HOME/fantasy4x/`, default `~/.local/share/fantasy4x/` | next to the binary, or `/usr/share/fantasy4x` |

With `--data DIR` as an override (the tests and the soak keep the current relative paths). Finding the
folders needs either the `dirs` crate or a few lines per OS; I have not checked which fits the
dependency policy.

### 3.6 The platforms are not equally tested

CI builds and tests on `ubuntu-latest` and `macos-latest` only. **Windows has never been built.**
macroquad supports Windows **(verify)**, but audio, file paths (`\`), the `saves/` code and the
scripts have not been tried there. Linux needs the ALSA libraries for sound (`sound.md`) and X11/GL
libraries for the window **(verify the exact packages)**. Apple's CI runners are Apple silicon; the
Intel target needs a cross target on that runner or an Intel runner.

## 4. How other projects do it (from memory, to verify)

Tools that package a finished Rust program:

| Tool | What it does | Fit |
|---|---|---|
| `cargo-dist` | builds release archives per target on GitHub Actions, makes shell, PowerShell and MSI installers, a Homebrew formula **(verify)** | good for the release pipeline; its installers install a program, they do not run an import |
| `cargo-packager` / `cargo-bundle` | `.app` and `.dmg`, `.deb`, AppImage, NSIS and MSI **(verify)** | good for the wrappers of 6.5 |
| `cargo-deb`, `cargo-wix`, `cargo-appimage` | one format each | alternatives to the above |
| Generic installers (Inno Setup, NSIS, WiX; `pkgbuild`/`productbuild`; Flatpak) | native installer UIs, can run a post-install program | usable, but the work (progress, errors, ROM dialog) would be done once per platform |

Games that need the player's original data:

- **OpenMW, ScummVM, OpenTTD, OpenRCT2, devilutionX** **(verify each)** ship the program as a plain
  download; the player points the program at the original files, or copies one file next to the
  program. Some open a small first-run dialog for it. None has an OS installer that extracts the data.
- Rust game projects mostly publish zip files from GitHub Actions and push them to itch.io with
  `butler` **(verify)**; importing assets from a commercial original is not a solved problem there.

So the pattern to follow is the data-import-in-the-program one. Rust matters only for the shared
code: one crate does the import on all four platforms.

## 5. Options

| | Description | Player needs | For | Against |
|---|---|---|---|---|
| **A. Source and compile locally** | Installer = script that checks for rustup, a C toolchain and libraries, runs the extraction, runs `cargo build --release`, creates the app | Rust, a compiler (Xcode command line tools; MSVC Build Tools on Windows, several GB), libraries, minutes of compile (the client is about 70 crates; an incremental release build here took 1 minute 9 seconds; a clean build is longer and I have not timed it) | no binary to sign; no Windows or Intel build machine for us | the worst experience on Windows; every failure is the player's compiler; an installer that installs a compiler is a lot of code; four platforms of toolchain trouble |
| **B. Prebuilt game + `f4x-setup` (recommended)** | CI builds the game and `f4x-setup` for the targets; the player downloads one archive; `f4x-setup` imports; nothing compiles | the original game; internet only for the optional ROM download | one code path for the import on all platforms; small testable steps; works offline except the ROMs; matches comparable projects | CI builds for the targets and, for a smooth macOS and Windows start, signing; the embedded river pictures (3.1) |
| **C. Native installers (MSI, pkg, deb) with a post-install step** | B wrapped in each OS's installer, import run at the end | as B | familiar to players | an import run by the installer (often elevated) is the wrong user and the wrong folder; the original files are in the player's home; progress and the ROM dialog are awkward; three build pipelines |
| **D. Import inside the game (first-run screen)** | the client itself shows an import screen | as B | one executable | needs a UI for file selection, progress and the ROM dialog in macroquad; the import code then sits in the client |

B first, then D as an improvement: `f4x-setup` starts as a console program with a clear text
interface (a short step list, progress lines, a Cancel/OK confirmation), and an in-game screen can
call the same library function later. C is only the wrapper layer of B (6.5), never the logic.

If the author insists on A for some platform (for example Linux, where players are used to
compiling), A needs nothing beyond B's setup program plus a documented `cargo install`.

## 6. Design of B

### 6.1 Parts

| Part | What | Where |
|---|---|---|
| `f4x-client` | the game; finds its data folder (3.5); starts `f4x-setup` when the folder has no art and no `import.json` | `crates/f4x-client` |
| `f4x-import` (library) | the whole import: read LBX, art, audio, the three local text files; no process calls, no UI; returns progress events and a report | new; move `lbx.rs`, `image.rs`, `art.rs`, `audio.rs`, `fonts.rs`, `map_art.rs` and the like out of `f4x-tools`, which keeps its development commands and calls the library |
| `f4x-setup` | the wizard around the library: locate, confirm, extract, ROM step, render music, report | new binary crate |
| `f4x-music` | MT-32 renderer started by `f4x-setup` with a folder of XMI and a folder of ROMs; LGPL, separate; today `tools/xmi-play` | `tools/`, outside the workspace |
| release pipeline | GitHub Actions workflow for the targets and the packaging of 6.5 | `.github/workflows/release.yml` |

`f4x-client` does not depend on `f4x-import`; only `f4x-setup` does. That is also part of the answer
to task 460 (what the executable carries): the import code is not in the game.

### 6.2 What the wizard does

1. **Find the original.** Look in the usual places (Steam library folders for "Master of Magic
   Classic", GOG default folders, the current folder), else ask for a path. Check the files: the
   expected `.lbx` names and sizes or a hash of a few of them. Say exactly what is missing. Support
   only the versions the author has verified (so far: Steam app 1146370).
2. **Show the plan:** what will be written, where (the data folder, about 270 MB), and the sentence
   that nothing is uploaded or shared.
3. **Extract** art, screens, fonts, effects, XMI and text, with a progress line per archive. Write
   `import.json` in the data folder at the end: tool version, date, the original's fingerprint, steps
   done. A rerun skips finished steps (`--force` redoes them).
4. **Music (optional).** Ask whether to render the music. If yes: look for ROMs in the data folder, then
   in a folder the player names; if none, the download dialog of 3.2. Verify the hashes, then run
   `f4x-music` (about 2 minutes) with progress. Skip it if declined.
5. **Report.** What was done, what failed (never silent), where the data is, how to run the setup again.
6. Offer to start the game.

Errors are plain sentences with the file name and the next step. A failed step leaves the earlier ones
in place.

### 6.3 Layout on disk

```
<data folder>/
  import.json
  assets-local/art/ screens/ audio/       (as today, so the client code changes little)
  content-local/                          (help text, city names, event texts)
  roms/                                   (the two ROMs, if the player allowed it)
  saves/
```

### 6.4 Command line (so the same steps run in tests and by hand)

```
f4x-setup                       interactive
f4x-setup --game DIR            skip the search
f4x-setup --music               only the music step
f4x-setup --roms DIR            use these ROMs, no download
f4x-setup --yes --no-music      unattended, for tests
f4x-setup --data DIR            another data folder
```

`--yes` never accepts the ROM download. The download needs an interactive confirmation, always.

### 6.5 Packaging per platform

| Platform | Build target | Artifact | Notes |
|---|---|---|---|
| macOS Apple silicon and Intel | `aarch64-apple-darwin`, `x86_64-apple-darwin` | one universal `Fantasy4X.app` (both built, joined with `lipo`) in a dmg or zip | one download serves both Macs. Unsigned apps are blocked by Gatekeeper; signing and notarisation need an Apple developer account (a yearly fee) and are an author decision (task 81); without it, document right-click Open. The app contains the game and `f4x-setup`; the launcher starts the setup first when the data folder is empty |
| Windows | `x86_64-pc-windows-msvc` | zip (portable) first; an installer later (Inno Setup, NSIS or MSI) | a portable zip needs no admin rights, which suits an import into the user's own folder. Unsigned executables raise a SmartScreen warning; signing needs a certificate (a cost) |
| Linux | `x86_64-unknown-linux-gnu` | `tar.gz`; AppImage later | build on an older Ubuntu runner so the glibc requirement is low **(verify)**; document the ALSA and X11/GL packages. Flatpak sandboxes file access, which fights with "find the original game folder", so it needs a permission design first and is not a first step |

ARM Linux and ARM Windows are out of scope.

## 7. Work list

Ordered so that each step is useful on its own. These are proposals, not tasks yet.

1. **Windows build and CI.** Add `windows-latest` to `ci.yml`; fix what breaks (paths, saves, audio).
   First, because everything for Windows depends on it.
2. **Data folder** (3.5): the client and `f4x-tools` find the per-user folder, with `--data` and the
   current relative paths as the fallback. No change to tests.
3. **The three local text files in Rust** (`text/en/name/city.ron`, `diplomsg`, `event_texts`; 3.4), reading the
   original's LBX and the committed `crates/f4x-def/content/races.ron`, with no Python and no ReMoM. The script stays
   for development; a test on the author's copy (not in CI) compares the two outputs byte for byte.
4. **Audio encoding without `oggenc`** (3.4), by spike.
5. **`f4x-import` library**, then **`f4x-setup`** (6.2), with tests that run the unattended path on a
   fixture and on the author's real folder (not in CI; CI has no original files).
6. **`f4x-music` as a packaged helper**, the ROM dialog and download with hash check, and the licence
   files for the LGPL part.
7. **Release workflow**: build the targets, make the packages of 6.5, upload to a GitHub release.
8. **Signing and notarisation** for macOS (and a Windows certificate) if the author wants them (task 81).
9. **First-run hand-off**: the client starts `f4x-setup` when the data folder is empty (later the
   in-game screen, option D).
10. **Documentation**: `local-assets.md` rewritten for players; the developer path stays.

When the author accepts the design, tasks 81 and 176 are replaced by this list. Task 176's wording
("asks for the original files, extracts, builds a local app") is option A of section 5.

## 8. Open decisions

| # | Decision | Suggested answer |
|---|---|---|
| 1 | Players compile (A) or download a prebuilt game (B)? | B. A stays as the developer path and for platforms without a prebuilt |
| 2 | Is the ROM download from archive.org acceptable, and which item? | The author decides and supplies the URL and the two SHA-256 values; the dialog and the "I have them" path are in the design either way |
| 3 | Which versions of the original are supported? | Steam first (verified); GOG when the author has checked the file names and hashes |
| 4 | The ReMoM inputs of the importer (3.4) | Decided by the user, 2 November 2026: `import_mom.py` is for development only. The player path needs neither Python nor ReMoM; the Rust import uses the committed `crates/f4x-def/content/races.ron` |
| 5 | Sound encoder (3.4)? | A Rust binding with libvorbis compiled in, after the spike; `oggenc` shipped beside it as fallback |
| 6 | Keep the embedded river pictures for a public release (3.1)? | Move them to the local extraction, so the binary is clean |
| 7 | Sign and notarise the macOS app? Windows certificate? | Not at first; document the warnings. Decide when the first public release is near |
| 8 | One universal macOS app or two? | One universal app |
| 9 | Windows installer or portable zip? | Zip first |
| 10 | Linux: tar.gz, AppImage, Flatpak? | tar.gz, then AppImage |

## 9. Spikes before committing to the design

Each is a day or less and answers one "verify" above:

1. A Windows build of `f4x-client` on CI: does it start and open a window?
2. `vorbis_rs` (or another encoder) building on all targets, with its licence.
3. `cargo-dist` and `cargo-packager`: what they produce and whether their dmg, zip and AppImage fit
   the wrapper layer, so we do not hand-write it.
4. Reading `CITYNAME.LBX`, `DIPLOMSG.LBX` and `EVENTMSG.LBX` in Rust and matching the script's output byte for byte.
5. A universal macOS build of the client with `lipo` on the Apple silicon runner.
