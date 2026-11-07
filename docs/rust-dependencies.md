# Rust dependencies, and where the megabytes come from

Measured on 2 November 2026 (Cargo.lock at version 0.0.3, macOS arm64 release build). Numbers marked
"not measured" are guesses worth checking, not facts.

**Short answer:** the Rust crates are not the bloat. The shipped client binary is 12.7 MB, of which about
3 MB is fonts and river pictures compiled in. The several hundred MB are the extracted **art and music**,
about 195 MB of real bytes (about 270 MB on disk), against 23.7 MB of `.LBX` files in the original. One thing
alone, the music, is 135 MB of the 195.

## 1. What the game compiles in

`cargo tree -p f4x-client` shows 57 third-party crates (6 of them proc-macros that only run at build time).
`Cargo.lock` lists 712 packages, but most of those are only for tools and experiments that are not in the game
(section 3).

### Direct dependencies of the client (`crates/f4x-client/Cargo.toml`)

| Crate | Used for | Notes |
|---|---|---|
| `macroquad` 0.4 | window, input, drawing, sound | `default-features = false`, only `audio`. Pulls in most of the list below. |
| `macroquad_bitmap_font_engine` | draws the BMFont bitmap fonts | git dependency pinned to a commit |
| `hecs` 0.10 | the ECS (`docs/ecs-structure.md`) | with `serde` in core |
| `ron` 0.8 | the rules, text and save files | |
| `serde` 1 (via core and data) | derives for the above | |
| `rand` 0.8, `rand_pcg` 0.3 | seeded random numbers | `rand` without default features |

The workspace crates themselves (`f4x-def`, `f4x-core`, `f4x-ai`, `f4x-console`) are ours.

### Pulled in by `macroquad`

| Crate | What it does | Needed by us? |
|---|---|---|
| `miniquad` | the window and OpenGL/Metal layer, with `objc-rs`, `malloc_buf` on macOS | yes |
| `quad-snd` | sound output; decodes with `audrey` (`hound` for WAV, `lewton` + `ogg` for OGG, `dasp_*`, `smallvec` 0.6) | yes: it plays our `.ogg` music and sounds |
| `quad-rand` | macroquad's own random numbers | no, but macroquad needs it |
| `image` 0.24 | loads textures (`png`, `tga` features), with `png`, `flate2`, `miniz_oxide`, `fdeflate`, `crc32fast`, `adler2`, `simd-adler32`, `byteorder`, `color_quant`, `bytemuck`, `num-traits` | yes for PNG; the TGA decoder is unused |
| `fontdue` | macroquad's built-in vector font drawing, with `ttf-parser`, `core_maths`, `libm`, `hashbrown` 0.15 | **no**: the game draws bitmap fonts. macroquad has no feature switch for it (`fontdue` is an unconditional dependency) |
| `glam` | vector and matrix maths | yes |
| `macroquad_macro` | the `#[macroquad::main]` macro | yes |

### Pulled in by `hecs`, `ron` and `serde`

`hashbrown` 0.14 (with `ahash`, `allocator-api2`, `zerocopy`, `once_cell`, `cfg-if`, `foldhash`), `spin`,
`bitflags` 2 (ron) and 1.3 (miniquad), `base64` (ron), `serde_core`, `serde_derive`, `syn`, `quote`,
`proc-macro2`, `unicode-ident`, `libc`, `equivalent`, `rand_core`, `maybe-uninit`.

Duplicated versions (the compiler builds both): `bitflags` 1 and 2, `hashbrown` 0.14 and 0.15, `miniz_oxide`
0.8 and 0.9. They are small.

All licences in the list are MIT, Apache-2.0, Zlib, BSD, 0BSD or Unlicense (or a choice including one of
them), so none stops us from shipping. (`mt32-rs`, which is LGPL, is not in this list: it is a separate
program, see `docs/installer-design.md`.)

## 2. Where the megabytes are

| What | Size | Note |
|---|---|---|
| The original game, `.LBX` files | 23.7 MB (101 files) | the whole game in the original format |
| of which `MUSIC.LBX` (116 songs as XMI, MIDI-like notes) | 0.6 MB | |
| of which `SOUNDFX.LBX` | 2.8 MB | |
| Our client binary | 12.7 MB | code, plus about 2.7 MB of font atlases and 0.24 MB of river pictures compiled in. Code per crate: not measured yet, task 470 (`cargo-bloat` is installed) |
| `content/` | 0.8 MB | rules and text, committed |
| `assets-local/audio/music` | 135.7 MB | **116 songs rendered to OGG** (the 0.6 MB of XMI beside them are the sources) |
| `assets-local/audio/soundfx` and others | 2 MB | |
| `assets-local/screens` | 37.8 MB | 6,056 PNG files (shown as 53 MB by `du`) |
| `assets-local/art` | 18.9 MB | 19,414 PNG files (shown as 86 MB by `du`): 13,103 are unit frames, 4.0 MB |
| Total real bytes | about 195 MB | `du` says about 270 MB because of the next point |

Where the growth comes from:

1. **Music is rendered, not played.** The original plays 0.6 MB of note data through the sound card's synthesiser.
   We cannot assume one, so the songs are rendered to audio once (`tools/xmi-play`, MT-32 emulation, then OGG
   Vorbis). 116 songs at about 1.2 MB each is 135 MB: about 220 times the size of the notes. This is by far
   the largest item (70% of the total), and the only one that would change the picture if shrunk.
   Levers, if wanted: a lower OGG quality or mono, another codec (only what `quad-snd` can decode: not checked), or play the notes live from a bundled synthesiser (a software MT-32 or a General MIDI
   soundfont, which needs a licence check, see `docs/installer-design.md`).
2. **Pictures are one file per frame.** The `.LBX` stores paletted frames (not measured how compact); we write each
   frame as its own 8-bit RGBA PNG (checked: `figure_2_03.png`, 28 x 30, RGBA). Roughly 25,000 files that
   average 1 to 6 KB. The real bytes are 57 MB, but a file takes whole 4 KB blocks on disk, so `du` reports
   139 MB for the same files. An archive or a few sprite sheets would remove that overhead; the gain is about
   80 MB on disk, no change in download size if the installer zips them. The rate of PNG against the original's
   coding was not measured.
3. **Screens.** 37.8 MB of full-screen and panel pictures as RGBA PNG. Whether the original's coding is smaller
   per picture was not measured.
4. **Code and libraries.** 12.7 MB for the client is ordinary for a Rust game with a graphics layer: static
   linking, no shared system libraries, `macroquad` and its crates, our own code, `lto = "thin"` and one codegen unit already set in the release profile. About 3 MB of it
   is the embedded fonts: each atlas stacks every glyph 16 times, once per colour set (`docs/extract-art.md`
   section 5, `docs/KNOWLEDGE.md`).

## 3. What is in `Cargo.lock` but not in the game

Not compiled into `f4x-client`:

- **`burn`** (the machine-learning framework, `f4x-learn`, behind a feature): most of the 712 packages come from
  here. It is off in the game and the normal tests.
- **`fontdue`, `png`** in `f4x-extract` (the extractor and font tools, under `crates/f4x-client/extract`): 44 crates for that program in all.
- `f4x-ai-ref` (a local copy, ignored by git, outside the workspace), `f4x-learn`, `f4x-tools`: our own dev-side crates. `scripts/make_app.sh` builds only
  `f4x-client`.

## 4. What could be trimmed

- **`fontdue` and its three small crates** (`ttf-parser`, `core_maths`, `libm`): not used, but macroquad does not
  let us switch it off. Removing it means a patched or forked macroquad, or replacing macroquad. A saving of
  perhaps a few hundred KB of the binary (not measured). Not worth it alone.
- **`image`'s TGA support** is cheap to drop the same way, same objection.
- **`serde` + `ron` + `base64`** are core to the content format.
- The sound crates (`audrey`, `lewton`, `hound`, `dasp_*`, `smallvec` 0.6) are old but small and needed.
- Binary size options not yet used: `strip = true`, `opt-level = "s"`, `panic = "abort"`. Not measured; the
  gain would be a few MB at the cost of backtraces and some speed. Ask before changing the profile.

None of that matters next to the music. If the install size is to come down, start there (section 2, point 1),
then the per-frame PNG files (point 2).

## 5. Why not GIF or animated PNG for the sprite sheets

Asked on 2 November 2026 (task 471). Not chosen, for these reasons:

- **The client does not play animations as clips.** It picks a frame by the unit's facing, action and the game's own
  timing (walk, attack, hit, death). It needs random access to frame N of a set, uploaded as a texture. A GIF or
  APNG is a stream of frames played in order with its own timing; the client would decode it fully into frames
  and then do the same as with a sheet, with more work and no gain.
- **Not enabled in the libraries we have.** macroquad's `image` crate is built with the `png` and `tga` features
  only (`macroquad-0.4.16/Cargo.toml`), so GIF decoding is not compiled in. Adding it means a feature that
  macroquad does not expose (a fork), or a new crate (`gif`) and our own decode path. APNG: macroquad does not
  mention it anywhere (checked 0.4.16 README, docs and source); its loader (`Image::from_file_with_format`) calls
  `image::load_from_memory`, which returns one still image, the first frame. The libraries underneath can decode
  APNG (`image` 0.24.9 has `PngDecoder::apng()`, `png` 0.17.16 reads and writes it), so it would work with our own
  code calling them, with no new dependency, but it gives nothing a sheet does not.
- **GIF has 256 colours and one-bit transparency,** which matches the original's paletted frames but not the soft
  edges and shadows some of our pictures have, and it compresses no better than PNG's deflate for small sprites.
- **A sprite sheet is what GPUs and engines expect:** one texture, a table of rectangles, no decoder beyond the PNG
  one already compiled in.

**Decision (2 November 2026):** sprite sheets are the default. A possible later step, not a task: offer macroquad a
pull request that loads APNG frames (a `load_animation` or frames on `Image`), after which one animated PNG per
animation could replace a sheet if it proves simpler. Revisit then.
