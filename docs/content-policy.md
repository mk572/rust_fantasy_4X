# Content policy: what is a setting, what happens when it is wrong

Status: decided by the owner on 22 November 2026 (R6 and the granularity); not built yet. Tasks 764 to 769 in `docs/TASKS.md`.

## Goal

- The game ships with a complete default rule set, **in the settings files**, not in Rust.
- A user may mod the game by layering their own files over the shipped ones.
- A problem with a setting (missing, corrupt, out of bounds, wrong type, a reference to nothing) produces a
  **verbose issue** that reaches the player **in the client**.
- Every kind of issue has exactly one defined outcome: **Refuse** (the game does not start / the save does not
  load) or **Degrade** (the game plays without that element, and says so).
- Rust holds a default value **only** where a setting is declared optional, and then only the value that means
  "absent" (see below).

## What is today (the survey)

- `#[serde(default)]` and `impl Default` carry tuning numbers in `defs.rs` (113 places), `settings.rs` (66),
  `effects.rs`, `feature.rs`, `lair.rs`, `damage_type.rs`, `help.rs`, `stats.rs`, `ai_site.rs`, `map.rs`.
- Eight files are "optional" and silently fall back to a Rust copy of their numbers: `heroes`, `wizards`,
  `unit_ratings`, `unit_efficiency`, `battle`, `lairs`, `ai_sites`, `rules_settings`. The Rust copy and the shipped
  file can drift apart; nothing notices.
- `ContentError` is three strings (`Io`, `Parse`, `Invalid(Vec<String>)`) with no severity and no structure.
- The client prints a load error to stderr and `exit(1)`s (`f4x-client/src/main.rs`); a window never shows it.
  Help loading uses `unwrap_or_default` and hides the error.
- The client loads the shipped content only; the mod layers exist for the tools (`load_content_layers`), not for the
  player.
- A save is not checked against the content it is loaded into.

## Rules

### R1. Where values live

1. Every game value (a number, a list, a name, a switch) lives in a settings file. Rust has no tuning number.
2. Layers: the mod folder first, the shipped folder second. A key the mod omits comes from the shipped layer, never
   from Rust. (A layer may override a whole file or, later, single keys.)
3. A struct field has no `#[serde(default)]` unless the field is declared optional (R2).

### R2. What may be optional, and what "absent" means

A setting may be optional only if its absence has a natural meaning that is not a number the designer would tune:

| Absent means | Hardcoded in Rust |
|---|---|
| a list is empty | `Vec::new()` |
| a feature is off | `None` / `false` |
| a modifier is neutral | `0` for an add, `100` for a percent |

Each optional setting is listed in the schema docs with its absence meaning. Everything else is **required**.
A required setting that is missing is an issue (R4), never a silent default.

**The optional settings today** (all other fields are required; `crates/f4x-def/tests/it/c5_coupling.rs` keeps the code
free of other defaults):

| Kind of field | Absent means |
|---|---|
| a list (`Vec`, a map), a name, a description | empty |
| an `Option` (a help topic, a race, a level path, a node count) | none: the game decides by its own rule (`towers`, `nodes_arcanus`, `farmer_food` of a race follows the setting) |
| a flag (`holy`, `coastal`, `double_mining`, the immunity exceptions of a spell, `flag` of an item) | off |
| an amount that adds (`upkeep`, `help`-less `bonus`, `production_percent`, `trade_percent`, `melee_bonus`, `gold_per_rating`) | 0 |
| the parts of an effect rule (`when`, `condition`, `target`) | passive, always, on this |
| `DamageTraits`, `AbilityAttack`, `CityBonus` | nothing added |
| a setting a save lacks (`fb_*` in `settings.rs`) | the player's default, else the loaded content's level (`load_game`) |

### R3. The checks, and when they run

1. **On start**: load every layer, parse, then validate (ranges, references, uniqueness, couplings with code).
2. **On game load**: validate the settings the save carries (ranges, `normalised` equals `validate`), and compare the
   save's content hash with the loaded content; a different one is an issue, not a silent mix.
3. The validators are the ones `f4x-def` has now; they return structured issues instead of strings.

### R4. The issue and its outcome

```
ContentIssue {
    file:    "units.ron",            // which file
    at:      "units[12].cost",       // row / field path, with the row's alias when it has one
    kind:    Missing | Unreadable | Corrupt | WrongType | OutOfRange | Dangling | Duplicate | UnknownKey | Conflict,
    detail:  params for the text,    // the value found, the limit, the name that does not exist
    outcome: Refuse | Degrade(Element),
}
```

- The text of an issue is a key in `crates/f4x-client/content/text/en/*.ron` with parameters (the text rule in
  `CLAUDE.md`), so it can be localised. `f4x-def` returns the kind and the details, never a sentence.
- A load returns a **report**: `content: Option<Content>` and `issues: Vec<ContentIssue>`. Any `Refuse` issue means no
  content. Only `Degrade` issues means content with the named element removed.
- Every issue of a load is collected and reported together, not just the first.

### R5. The two outcomes

- **Refuse**: the default for every issue. The client shows the issue list on a screen (file, path, what was found,
  what is allowed) with a single OK (Quit) button. A save load shows the same list and returns to the menu.
- **Degrade**: only for an element declared degradable (R6). The game starts, and the client shows the issue list once
  with OK (Continue); the element is absent for the whole session (the wizard presets are not offered; units have no
  rating).

### R6. Degradable elements (decided)

Granularity: a **single row**. A bad row of a degradable element is dropped, the rest plays, and the client says
which row was dropped and why. Every other row of every other file is Refuse.

| Element | Dropped row means |
|---|---|
| `wizards.ron` | that ready-made wizard is not offered |
| `unit_ratings.ron`, `unit_efficiency.ron` | that unit has no rating / no efficiency figure |
| Help text, help picture | that topic is missing / has no picture |
| Premade named items (`items.ron` rows) | that item does not exist in this game |

Rules for a drop:

1. A row may be dropped only if **nothing else refers to it** after the drop (a unit's item, a lair reward, a hero
   template, a loot table). If something does, the issue becomes **Refuse** and names the referrers.
2. A drop never renumbers: the other rows keep their numbers (saves and references use them).
3. A file that is unreadable or whole-file corrupt is Refuse, also for these elements (there is no row to drop).
4. A degraded game is marked as such in the save, so a later load can say it was played without the element
   (built: `Snapshot.degraded`, `load_game_with` notes).

Built (task 765): `assemble` and `Help::repaired`. A row of a degradable file that is out of order or twice keeps the
longest rising run of numbers; the other rows are dropped. The rules are checked again without the dropped rows, and an
issue that then appears refuses the game. The issues of the dropped rows are in `Rules::degraded` (and `load_help_checked`
for the help), each with `Outcome::Degrade`. Not built: the mark in the save (task 767) and the client screen (task 768).
No shipped file refers to a droppable row, so the refusal for a referred row is not tested (task 770).

Open: whether `heroes.ron` is degradable (not chosen; it stays Refuse).

### R8. Fail loud (decided 22 November 2026)

A value that is zero or empty is a fine start. What must never happen is a content problem that is swallowed, with
the game then running on values the code holds: the behaviour would differ from the files and be hard to trace.

1. No load of content (rules, settings, help, event texts) goes through `unwrap_or_default`, `.ok()` or a quiet
   fallback. A load either gives the content or the issues; the client stops with them (until the issue screen of
   task 768 shows them), a tool returns them as its error.
2. `Default` on a content type is the empty value for a game that has no content yet (`World::new`); the loader never
   calls it, and a test game sets its content before it plays.
3. Closed (22 November 2026): `World::set_content` sets the game's settings from the level the content's new-game
   choices name, so a modded content plays its own figures; a save from before a setting existed migrates from the
   player's default, else from the content it is loaded into (`load_game`), never from a compiled-in copy.
   `rules_settings.ron` must have all six levels. Still open: a game made by `World::new` and never given content
   plays on the empty rules (tests only; no production path does it).

### R7. Mods (decided: in this work; built: `f4x-client --mod DIR`, `issue_screen.rs`)

The client builds the layer list `[mod folder, shipped folder]` (today only the shipped one) and reports issues
with the layer the file came from, so the player knows whether the mod or the shipped game is at fault.

## Consequences for the code (in the order to build them)

1. Delete `impl Default` and `#[serde(default)]` that carry tuning numbers; the shipped files must then state every
   field (a one-time content change, covered by the hash guard and a test that every field of every struct is present).
2. Replace `ContentError` with the report and `ContentIssue` above; keep the attack tests, which then assert on
   `kind`, `file` and `at` instead of the message.
3. A client screen for the issue list; the client's `exit(1)` paths and the `unwrap_or_default` for help go.
4. Let the client load a mod folder (today only the shipped content).
5. The tests of `f4x-def` change: the "built-in equals shipped" test disappears with the built-ins; a test per issue
   kind checks its outcome and that the details name the file and path.

## Order of work

1. `ContentIssue` and the report in `f4x-def`; the loader collects instead of stopping; the outcome of each kind.
2. Row-level drop for the degradable elements, with the reference check of R6.
3. Remove the Rust defaults file by file (the shipped file gets every field; a test checks it), `battle`, `lairs`,
   `ai_sites`, `rules_settings` first, then `defs.rs`, `settings.rs`.
4. Save-load checks (R3.2) and the degraded mark.
5. Client: the issue screen, the mod folder layers, text keys; the `exit(1)` paths go.
6. The `f4x-def` tests move from messages to `kind`, `file`, `at`.
