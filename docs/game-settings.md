# Game settings: design

Status: **draft, decisions of 16 October folded in**, nothing built. It turns tasks 89 and 94 (and
parts of 148, 155, 68, 69, 82 and `PLANNED.md` items 2, 3 and 8) into one plan: a per-game
settings record that is saved with the game, the base settings screen of task 94 on top of it, and
an advanced screen, opened from a button on the base screen, for the numbers the original never
lets the player touch. Larger stacks have their own design, [`large-stacks.md`](large-stacks.md),
for later.

**Built so far (16 October 2026):** stage 1 (task 246) and stage 2 (task 247, with the
settings screen of task 94). The record holds the basic block, the wizard, AI, world, rule, event
and score groups of the levels; `GameSettings::flat()` is the default of `NewGame` and `Game::new`,
so tests, the soak and the learning AI play the symmetric game, and the client and the console's
`new difficulty=...` start from a level. The number of opponents is `settings.basic.opponents`,
the one place it lives. Not yet read by the rules: `picks.human` (stage 3), the event group
(task 68). Old saves are not migrated.

Read first: `divergences.md` section 4 (starting setup), `game_end.md`, `screen_list.md`
section 3 (the new-game steps). `design.md` section 8 and `divergences.md` section 1 still say
"no AI advantage"; this design reverses that (section 2), so those passages need updating when
it is accepted.

## 1. Goals and what is out of scope

Goals:

1. One typed record, `GameSettings`, that holds every number and switch a game is played with,
   chosen once when the game is made, saved and loaded with it.
2. The **base settings screen** (task 94) is what a player sees first: the original's difficulty,
   opponents, land size and magic power, with difficulty as one setting. Anyone who never opens
   the advanced screen gets the original experience. The screen has an **Advanced settings...**
   button.
3. The **advanced screen** exposes what is hardcoded today: everything the original's difficulty
   touches, starting units, gold and mana, the stack limit, how often random events fire, the
   Spell of Mastery win, the Spell of Return rule, switches for heroes and lairs, and room for more.
   It has a difficulty dropdown (the five levels plus Custom).
4. The record is the only place the rules read these values from. No more `const MAX_STACK`.

**The base screen is task 94's screen.** It has two parts: the **game** part (this design, saved
with the game) and the **presentation** part (volume, animation and message speeds, "show enemy
moves", end-of-turn summary and wait, strategic-combat-only, node-owner display, spell book
ordering, what the hall of fame records: the original's `Settings_Screen`), which is client
configuration and **not** saved with a game. Only the game part is designed here. Two entries of the
original's list change rules and belong to the game part: **random events** (4.5) and the flee rule
(4.7). "Ask before razing a city" stays a preference. From the start menu both parts can be edited;
in a running game the game part is shown read-only and the presentation part stays editable.

## 2. Difficulty is one setting, as in the original

The base screen has **one** difficulty control, exactly as the original's options step does: Intro,
Easy, Average, Hard, Impossible. It is the whole story for a player who never opens the advanced
screen, and it brings the original's full effect, **including the effects on computer wizards**:
`difficulty_modifiers_table` multiplies the AI's city food, production, gold, mana, research and
growth (Intro 100% up to Impossible 400%, research up to 150%) and cuts its maintenance (to 30%),
Hard and Impossible cut AI unit upkeep, AI wizards get extra picks (13 and 15 against 11), and the
"erasure dice" weaken them on low levels. This reverses the earlier rule of `design.md` section 8
(difficulty only from how well the AI plays); that passage and `divergences.md` sections 1 and 4
are to be updated when this design is accepted.

The advanced screen then **fine-tunes**: every value a difficulty fills in is a stored setting,
and any of them can be changed afterwards. Where it makes sense the setting is **split into a
human value and an AI value**, so a player can, for example, start with 200 gold while the AI
wizards start with 100, or turn the AI's production bonus off on Hard.

Difficulty therefore means two things, kept apart in the data:

| Part | What it is |
|---|---|
| **Preset** | The values the original gives each level (section 3.2). Changing the difficulty fills them in. |
| **Play strength of the AI** | How well the new AI plans and fights (task 148). Read from `basic.difficulty`, not from the numbers. |

## 3. Data model

### 3.1 `GameSettings`

A new module `crates/f4x-core/src/settings.rs`. One struct, grouped in sub-structs so the screen
and the save can walk it, every field `#[serde(default)]` so a new field never breaks an old save.

```rust
pub struct GameSettings {
    pub basic: BasicSettings,        // the original's four choices
    pub wizards: WizardSettings,     // picks, starting gold and mana: human and AI
    pub ai: AiSettings,              // the AI multipliers of the original's difficulty, and AI wizard making
    pub world: WorldSettings,        // map and the neutral world
    pub rules: RuleSettings,         // stack limit, city limits and the like
    pub victory: VictorySettings,    // Mastery, Return, hardcore
    pub events: EventSettings,       // random events
    pub scoring: ScoreSettings,      // final score scale
}

/// A value that can differ between the human wizard and the computer wizards.
pub struct Split<T> { pub human: T, pub ai: T }
```

`Split<T>` is the one mechanism for "human and AI separately". The screen shows it as two
steppers on one row (or one stepper with a "same for both" link that is on while the two are
equal).

Draft of the fields (names are proposals; ranges are enforced by `GameSettings::validate`):

| Group | Field | Type / range | Default (Average) | Today |
|---|---|---|---|---|
| basic | `difficulty` | 0..=4 (Intro to Impossible) | 2 | `Game::difficulty` |
| basic | `opponents` | 1..=4 (more: `PLANNED.md` 3) | 1 in the client | `NewGame` wizard count |
| basic | `land_percent` | preset 15 / 20 / 30, free 10..=60 | 30 | `GenParams::land_percent` |
| basic | `magic_strength` | 0..=2 (Weak, Normal, Powerful) | 1 | `Game::magic_strength` |
| wizards | `picks` | `Split`, 5..=20 | 11 / 11 | `wizard::PICKS` |
| wizards | `max_books_per_realm` | 3..=11 | 11 | `wizard::MAX_BOOKS` |
| wizards | `starting_gold` | `Split`, 0..=2000 | 75 / 150 | `Player::new` (`gold: 100`) |
| wizards | `starting_mana` | `Split`, 0..=2000 | 0 / 0 | `Player::new` (`mana: 0`) |
| wizards | `starting_units` | `Split<Vec<StartUnit>>`, at most 12 entries, 20 units | Spearmen, Swordsmen | `setup.rs::start_player` |
| ai | `apply_to_human` | bool | off | not built (a custom setting: the human gets the same multipliers) |
| ai | `randomise_wizards` and the AI wizard rules | see 4.8 | original | `setup.rs` |
| ai | `growth_percent` | 100..=800 | 150 | not built (original table) |
| ai | `outpost_growth_percent` | 100..=800 | 150 | not built |
| ai | `production_percent` | 100..=800 | 150 | not built |
| ai | `gold_percent` | 100..=800 | 150 | not built |
| ai | `mana_percent` | 100..=800 | 150 | not built |
| ai | `food_percent` | 100..=800 | 150 | not built |
| ai | `research_percent` | 100..=800 | 100 | not built |
| ai | `maintenance_percent` | 10..=100 | 75 | not built |
| ai | `unit_upkeep_percent` | 10..=100 | 100 | not built (original: 75 Hard, 50 Impossible) |
| world | `map_width`, `map_height` | 8..=1024 (the generator's own limits) | 60, 40 | `GenParams` |
| world | `tower_count` | 0..=20 | 6 | `GenParams` |
| world | `strong_sites`, `weak_sites` | 0..=100 | 25, 32 | `GenParams` |
| world | `nodes_arcanus`, `nodes_myrror` | 0..=40 | 16, 14 | `PlaneParams` |
| world | `mineral_percent_arcanus`, `_myrror` | 0..=30 | 6, 10 | `PlaneParams` |
| world | `lairs_enabled`, `neutral_cities_enabled`, `raiders_enabled` | bool | on | always on |
| world | `myrror_enabled` | bool | on | always two planes (stage 4, 4.9) |
| world | `heroes_enabled` | bool | on | heroes are not built yet (tasks 218, 219); read when they are |
| world | `lair_guard_percent` | 0..=300 | from preset | `lairs.rs`: `budget * (d + 1) / 4` |
| world | `lair_treasure_percent` | 0..=500 | from preset | `lairs.rs`: `loot * 4 / (d + 1)` |
| world | `neutral_city_size_bonus` | -2..=6 | from preset | `neutral_cities.rs`: `(d + 1) / 3` terms |
| world | `raider_percent` | 0..=300 | from preset | task 154's raiders, `troops * d / 6` in the original |
| rules | `max_stack` | one value, 1..=9 now (4.3) | 9 | `units::MAX_STACK` |
| rules | `max_city_size` | 5..=40 | 25 | `rules.ron` |
| rules | `min_city_distance` | 1..=8 | 4 | `rules.ron` |
| rules | `safe_flee` | `Split<bool>` | Intro, Easy: on | `battle/game.rs::overland_aftermath` (4.7) |
| victory | `spell_of_mastery` | on / off | on | always on |
| victory | `spell_of_return` | `Off` / `HumanOnly` / `All` | `All` | always all |
| victory | `hardcore` | bool | off | `PLANNED.md` 8 |
| events | `random_events` | on / off | on | task 68 |
| events | `event_frequency` | `Half` / `Normal` / `Double` | `Normal` | task 68 |
| events | `event_start_turn` | 1..=50 | 6 | the original's five-turn delay |
| scoring | `score_percent` | 10..=500 | 100 | `endgame::scale_for_difficulty` |

What is split and what is not: anything a wizard **owns or starts with** (picks, gold, mana,
erasure) is split; anything about **the world's rules** (stack limit, city limits, events, Mastery,
Return) is one value, because a rule that differs by wizard stops being a rule. The AI multipliers
in the `ai` group are AI-only by nature, as in the original; a custom switch, `ai.apply_to_human`
(off by default), gives the human the same multipliers.

`rules.ron` keeps the base values as the defaults of the ruleset; the settings **override** them
(`max_city_size` and `min_city_distance` are read through `game.settings()` instead of
`content.rules`). Combat constants (`to_hit_percent`, `block_percent`) and tax tables are left as
ruleset data on purpose: changing them changes what every unit and spell means, which is modding
(`PLANNED.md` 5), not a game setting.

### 3.2 Presets

`Difficulty` is a preset that fills in the record. `GameSettings::for_difficulty(d)` returns the
full record; the screens apply it when the player moves the difficulty control. The record also
stores `difficulty` itself, because the AI (task 148) and the hall of fame read it.

**Custom.** The advanced screen has a **difficulty dropdown** with the five levels and **Custom**.
Choosing a level fills in all its values (after a confirmation when something was changed by hand).
**Custom covers only the settings the original's difficulty touches**: starting gold, picks, the AI
city multipliers and upkeep, the AI wizard block, lair guards and treasure, neutral city size,
raiders, whether random events are on, safe flee and the score scale. Changing any of these switches
the dropdown to Custom at once, and putting them all back shows the level again. "Custom" is
therefore derived, not stored: it is the state `settings != for_difficulty(settings.basic.difficulty)`
over those settings. `basic.difficulty` keeps the level the game was based on (shown as "Custom
(based on Hard)"); the AI's play strength (task 148) and the score label read it.

Everything else is an **extra** and never changes the dropdown: starting mana, books per realm,
starting units, `ai.apply_to_human`, event frequency, the Spell of Mastery and Spell of Return
switches, heroes, lairs and neutral cities on or off, the stack limit, city limits, map size,
no Myrror and hardcore. A Hard game with settlers added is still honestly "Hard".

All values are the original's where it has one (`INITGAME.c`, `EVENTS.c`, `CITYCALC.c`,
`MOM_DAT.c`, `MAPGEN.c`, `SCORE.c`):

| Setting | Intro | Easy | Average | Hard | Impossible |
|---|---|---|---|---|---|
| `starting_gold` human / AI | 125 / 150 | 100 / 150 | 75 / 150 | 50 / 150 | 25 / 150 |
| `picks` human / AI | 11 / 11 | 11 / 11 | 11 / 11 | 11 / 13 | 11 / 15 |
| `ai.wizards.erasure_dice` | 0 | 0 | 1d3 | 2d3 | 2d5 |
| `ai.growth_percent` | 100 | 100 | 150 | 200 | 400 |
| `ai.outpost_growth_percent` | 100 | 100 | 150 | 200 | 400 |
| `ai.production_percent` | 100 | 125 | 150 | 200 | 400 |
| `ai.gold_percent` | 100 | 125 | 150 | 200 | 400 |
| `ai.mana_percent` | 100 | 125 | 150 | 200 | 400 |
| `ai.food_percent` | 100 | 125 | 150 | 200 | 400 |
| `ai.research_percent` | 100 | 100 | 100 | 110 | 150 |
| `ai.maintenance_percent` | 100 | 90 | 75 | 60 | 30 |
| `ai.unit_upkeep_percent` | 100 | 100 | 100 | 75 | 50 |
| `lair_guard_percent` | 25 | 50 | 75 | 100 | 125 |
| `lair_treasure_percent` | 400 | 200 | 133 | 100 | 100 |
| `neutral_city_size_bonus` | 0 | 0 | 1 | 1 | 1 |
| `raider_percent` | 0 | 17 | 33 | 50 | 67 |
| event pressure (derived from the level, not a setting) | 50% | 66% | 75% | 80% | 100% |
| `random_events` | **off** | on | on | on | on |
| `score_percent` | 50 | 75 | 100 | 200 | 300 |
| `max_stack`, `starting_mana`, `max_books_per_realm` | no original effect: 9, 0, 11 at every level | | | | |

Notes:

- **This changes today's game.** Our start is 100 gold for everyone and no AI bonus at all, so at
  Average the human starts with 75 instead of 100, the AI with 150, and every AI city gets the
  150% bonuses. The soak baseline changes once, when the AI group lands (stage 2), and the commit
  says so. A **`flat` preset (today's symmetric game) exists internally, for the soak and the
  learning AI only**: it is not on any screen and not in the dropdown. It is what the console's
  `new` makes when no `difficulty=` is given, and what tests build.
- Rows that are today's numbers (`lair_*`, `neutral_city_size_bonus`) must reproduce the present
  output exactly at every level; a test per level checks it.
- The original applies the AI multipliers to computer wizards' cities and mana only; the human is
  skipped (`CITYCALC.c`: "Skip human player 0"). Neutral cities and monsters are not computer
  wizards and get none. Keep both.
- The Hard-level "knowable starting spell" for AI wizards (`INITGAME.c`, which has bugs the ReMoM
  source marks `OGBUG`) is **not** a setting; it is left out and noted in `divergences.md`.
- The original's AI aggression against the human (+5 target value above Average, again above
  Hard) is AI behaviour, part of task 148, not a number here.
- Two lines in `Combat.c` (3863 and 4075) treat Intro and Easy differently when the current
  player flees. **I have not read them closely enough to say what the effect is**; read before the
  table is final and add a setting if it is a real rule.

### 3.3 Where it lives and how it is saved

- `Game` gets `settings: GameSettings` (read through `Game::settings()`; the old public
  `difficulty` and `magic_strength` fields are gone, callers read `settings.basic`).
- `Snapshot` (`save.rs`) holds `#[serde(default)] settings: GameSettings`. **Old saves are not
  migrated**: the two old fields `difficulty` and `magic_strength` are no longer written or read,
  and a save without the record reads as the defaults. Out-of-range values are clamped on load.
  - Settings that took effect at creation (`picks`, `starting_gold`, map size) are saved too, for
    the screens and the hall of fame; only the ones read during play (stack limit, events, Mastery,
    Return, city limits, scoring) have to be right after a load.
- **Settings are fixed once the game starts.** There is no in-game editing, no console `set`
  command and no cheat for them, so a replay of the same seed and commands stays the same game. The
  exception is a deliberately listed set of **mid-game-safe** fields (none in version 1). A game
  that wants a change starts again or goes through the override log (`override_log`, the cheat
  path), which marks the game as modified.
- Snapshot equality tests (`save/tests.rs`) gain a settings round trip; a settings record with
  every field away from its default must round-trip.

### 3.4 Plumbing

Today the values travel in separate places: `setup::NewGame` (difficulty, magic, land percent, a
`WizardBuild`), `GenParams` (map and difficulty) and constants. The plan:

1. `setup::NewGame` carries one `GameSettings` instead of loose fields; `GenParams` is built from
   it by `GameSettings::gen_params()`.
2. `Game::with_generated_map` takes the settings and stores them before any rule runs (setup
   already uses `difficulty` at creation).
3. The wizard builder (`wizard.rs`) takes `picks` and `max_books_per_realm` from settings; the
   custom-wizard screen shows "picks left" from the same source.
4. The console's `new` command takes `key=value` pairs for every field (it already takes `land=`,
   `difficulty=` and `magic=`), so tests and the soak can set them without a screen.
5. `f4x-ai` reads the stack limit through `game.max_stack()` instead of `MAX_STACK`
   (`military.rs` chunks of nine, `naval.rs` loads) and checks the Mastery and Return settings
   before planning for them.

## 4. The settings, one by one

### 4.1 Starting gold and mana, picks and books

- `starting_gold` and `starting_mana` are each **two values, human and AI** (`Split`). The preset
  gives the original's: the human `(5 - difficulty) * 25`, every AI 150. This settles
  `divergences.md` section 4 and task 155 item 3 the other way: those entries are rewritten when
  the design is accepted.
- `starting_mana` goes into `Player::mana` at setup. It does not change the mana income or the
  skill split; a wizard with 500 mana can cast at once.
- `picks` is the custom-wizard budget (`PICKS`, 11 today), also split: the preset gives AI wizards
  13 on Hard and 15 on Impossible, as the original does. `max_books_per_realm` caps one realm
  (`MAX_BOOKS`, 11) for everyone. The starting-spell rule ("a realm with `n` books chooses `n - 1`
  common spells") needs no new numbers. Ready-made wizards (the 14 presets) stay on offer when
  the human `picks` is not 11. A preset that has more picks to spend gets them as **extra books in
  the realms it already has**, shared out evenly; when the extra does not divide (an odd book
  between two equal realms) a **50/50 coin flip** from the game's generator decides which realm gets
  it. A realm stops at the book limit (`max_books_per_realm`, 11); when every realm the wizard has
  is full, the remaining picks become **random retorts** the wizard could take (the usual book
  requirements apply, and a retort that costs more picks than are left is skipped). The same
  top-up serves the AI's ready-made wizards when `ai_wizards.randomise` is off and the AI `picks`
  is above the preset's cost. **Fewer picks than a preset costs is not decided** (open question 1).
- The AI builds its wizard from the AI `picks`, then the original's erasure dice (`ai.wizards.erasure_dice`, 4.8)
  take books or retorts away on Average and above. Rolls come from the game's seeded generator.

### 4.2 Starting units and other start values

Today `setup.rs::start_player` gives each wizard a capital of four citizens (three farming), the
race's spearmen and swordsmen (two swordsmen for a race without spearmen), and three buildings.
The unit list becomes a setting, **split human and AI** like gold:

```rust
pub struct StartUnit { pub kind: StartUnitKind, pub count: u8 }

pub enum StartUnitKind {
    RaceUnit(String),   // by the race's own unit name: "Spearmen", "Swordsmen", "Settlers"
    Settlers,           // the race's settler unit
    FirstSummon,        // the creature of the first summoning spell the wizard starts with
    Scout,              // a free Magic Spirit (`magic_spirit`), given to the wizard
    Ship,               // the race's basic boat, put on water next to the capital
    Unit(String),       // any unit by id, for tests and mods
}

// WizardSettings
pub starting_units: Split<Vec<StartUnit>>,   // default: Spearmen x1, Swordsmen x1 for both
```

The default list is today's start, so nothing changes unless the player edits it. Resolution rules:

- **Race units** fall back as today: no spearmen means the swordsmen again. A name the race lacks
  is skipped, and the start reports it.
- **FirstSummon** is the creature of the **first summoning spell among the wizard's starting
  spells**, the ones the books bought (the common spells chosen at creation, section 4.1 of
  `wizard.rs`'s rules). A summoning spell is one whose cast effect is `Summon(unit, count)` in
  `spells.ron` (War Bears, Sprites and the like), so no new data is needed. "First" is the order the
  starting spells are listed in when the wizard is made; **I have not checked that order** (it
  may be by realm, or the order of choice), and it must be fixed and tested. A wizard with no
  summoning spell among the starters gets nothing and the start reports it.
- **Scout** is a **free starting Magic Spirit** (`magic_spirit` in `units.ron`), the same for every
  race and wizard. It is not a Summon spell cast: the unit is simply given at the start. Its data
  gives it an upkeep of 1 mana a turn (`upkeep: (mana: 1)`), so the wizard must really get at
  least one mana a turn. **What the code does today** (`magic_game.rs::process_magic`,
  `magic.rs::upkeep_plan`, `magic_income.rs`):
  1. Each turn the mana share of the power is added to the reserve first, then the upkeep is
     paid from the reserve. If the reserve cannot pay, **the newest upkeep items are given up**,
     so a spirit with no mana is lost at the first turn change.
  2. The default split is 33 mana, 34 research, 33 skill, and the mana share is
     `(power * 33 + 50) / 100`: **one mana only when the power base is at least 2**, two at 5.
     The power base at the start is the books (a point each, five more on Myrror) plus what the
     capital makes. Nothing requires two books, and the lowered `picks` of 4.1 makes few books
     possible, so a wizard can start with a mana share of **0**.
  3. A human who sets the mana slider to 0 loses the spirit too, as with any upkeep item. That
     stays the rule. The AI already sizes its split to cover its upkeep (`wizardry.rs::split`:
     mana share minus upkeep at least 2, or 0 with a reserve over 400).
  4. A Channeler pays half of the total, rounded down (`sum / 2`), so one spirit alone costs a
     Channeler nothing.
  **Proposed rule:** when the start list has mana upkeep, `start_player` sets the wizard's slider
  to the smallest mana share, in whole percents from the default upward (research gives way, since it takes
  the remainder in `split_power`), that makes mana income cover the upkeep. If even all of the power cannot
  (power base 0), the spirit is not given and the start reports it. `starting_mana` is not used for
  this: it only delays the loss. Tests: one book, two books, a Channeler, Myrror, and a human who
  then sets mana to 0 (the spirit goes at the first turn change).
- **Ship** is placed on a water square next to the capital if there is one; a wizard whose capital
  is inland gets none and the start reports it. A ship in a port is not a stack member of the
  capital's square, so it does not use the stack limit there.
- **The stack limit** (4.3) applies: units beyond `max_stack` on the capital square are put on the
  nearest free adjacent square, and the screen warns when the list is longer than the limit.
- Units cost no gold and no upkeep exemption: the normal upkeep applies from turn 1, so a long
  list is a real cost. The advanced screen shows the upkeep of the list.
- The list is limited to 12 entries and 20 units in all by `validate`.

Other start values that stay hardcoded for now and have a place in `WizardSettings` when asked:
the capital's size (four citizens), starting fame (10 for Famous), the starting buildings
(smithy, barracks, builders hall).

### 4.3 Maximum units per stack

`MAX_STACK = 9` is read in movement, gates, special spells, transport, the neutral-city garrison,
the lair garrison, the AI's gathering, and the battle code (`MAX_SIDE_UNITS` in `battle/casting.rs`
is a second copy of the same 9, and `battle/game.rs` caps the raised dead by `MAX_STACK`).

- `max_stack` replaces both constants, and **lowering it (1 to 8) is a stage 3 setting**: it only
  makes the existing code stricter.
- **Raising it above nine is a stage 4 setting with its own design, [`large-stacks.md`](large-stacks.md),
  for later.** Until then the range is 1..=9 and a larger value in a hand-edited save is clamped.
- `MAX_SIDE_UNITS` follows `max_stack`. A raised-dead or summoned unit is refused when the side is
  full, as now. The generator caps neutral garrisons and lairs at `max_stack`.
- Settings are fixed once the game starts, so a stack never exceeds a limit that was lowered
  mid-game.
- A galley's five places and the way passengers count are task 236's business; this setting only
  changes the cap they are checked against.

### 4.4 Spell of Mastery victory on / off

Today the Spell of Mastery is always in the spell pool (the research list offers it last, see
`magic.rs`; `endgame.rs` ends the game when it is cast). With `victory.spell_of_mastery = false`:

- the spell is **not offered in research** and cannot be cast (the cast refuses with a clear
  reason, for saves of games that had it started);
- the AI never plans for it (`wizardry.rs` skips the Mastery branch);
- the game then ends only when all other wizards are defeated, or the human is. (A game with no
  other ending can run on indefinitely. That is the point of the setting, and the score screen
  works the same way when the human conquers the world.)
- the final score drops its "Spell of Mastery: 250" line only if nobody cast it, as now.

Interplay: `hardcore` and `spell_of_mastery` are independent.

### 4.5 Random events: switch and frequency

Task 68 is built, and `random_events` and `event_frequency` are read by the draw (task 274: the preferences page's Random Events entry is the default for a new game, and a level that plays without events, Intro, keeps them off; the console's `new events=on|off event-frequency=half|normal|double` and `settings` show the rest; the three-way choice with the chances on a screen waits for the advanced screen of task 248; `event_start_turn` is not built, the first five turns are fixed). The original's rule (`EVENTS.c`, `Determine_Event`): every turn, a "pressure"
equal to the turns since the last event (nothing for the first five turns after the game starts, a
flaw the help conflicts list as B18) is scaled by a difficulty fraction (50% on Intro, 66%, 75%, 80%,
100%), and an event fires when `Random(512)` is at most the pressure. The chance therefore grows
the longer it has been quiet, then resets. Intro also switches events off (`LoadScr.c`).

The settings keep that shape so the original's pacing is the default:

- `random_events` (on / off; the Intro preset is off);
- `event_frequency`: **Half, Normal or Double**, a multiplier on the chance. The difficulty's own
  fraction (50% to 100%) is not a setting; it comes from `basic.difficulty` and is multiplied by
  this. At Normal on Average the chance of an event on the turn after *n* quiet turns is
  `0.75 n / 512` (about 1.5% at ten turns); Half and Double halve and double that;
- `event_start_turn`, the first turn events may fire (the original: five turns after launch, which
  is a bug on loading; ours counts from the game's turn, so a load does not reset it).

The screen shows a three-way choice and, beside it, the chance it makes at turns 10, 20 and 40.

The same record holds, later, a per-event on/off list (the original has 18 event types), so that
a player who dislikes Meteor can drop just that one. Not in the first stages.

### 4.6 Spell of Return: off / human only / all

`PLANNED.md` item 2. Today `fortress.rs::banish` always gives the banished wizard the Spell of
Return casting. The check goes there:

| Setting | Human banished | AI wizard banished |
|---|---|---|
| `All` (as the original) | Return casting starts | Return casting starts, unless skill is under 40 or mana is gone: then it resigns (`fortress.rs`) |
| `HumanOnly` | Return casting starts | defeated at once (`defeat`) |
| `Off` | defeated at once | defeated at once |

`hardcore` forces `Off`, whatever is stored. "Defeated at once" uses the existing `defeat(loser,
winner)` path, so the winner's conquest bookkeeping, the human-defeat game end and the score all
work as for a last-city loss. The Spell of Return stays in the spell data (it is not researchable
anyway); the setting only decides whether the cast starts. The new-game screen says which of the
three is chosen, and the game-over text for a defeat by banishment names the rule.

### 4.7 Where each of the original's difficulty effects lands

| Original effect | In this design |
|---|---|
| Starting gold (human `(5 - d) * 25`, AI 150) | `starting_gold`, split (4.1) |
| Wizard picks (AI 13 / 15 on Hard / Impossible), erasure dice | `picks` split (4.1); `ai.wizards.erasure_dice` (4.8) |
| AI city food, production, gold, mana, research, growth, outpost growth | the `ai` group, applied to computer wizards' cities and mana only |
| AI maintenance and Hard / Impossible unit upkeep cut | `ai.maintenance_percent`, `ai.unit_upkeep_percent` |
| Lair guards, lair treasure | `lair_guard_percent`, `lair_treasure_percent` |
| Neutral city size | `neutral_city_size_bonus` |
| Raiders | `raider_percent` (task 154's code reads it) |
| Random events on or off, frequency | section 4.5 |
| Final score scale | `score_percent`, applied where the score is made (task 148) |
| Fleeing (`Combat.c` 3863 and 4075) | `rules.safe_flee`, below |
| AI race limits above Normal (nine races, then five); Myrran count rule above Normal with more than three wizards | `wizards.ai_race_limit` switch (on / off), default by preset; task 155 item 2 |
| Hard-level extra starting spells for AI wizards | left out (the original's code is buggy there); noted in `divergences.md` |
| AI aggression against the human | AI behaviour, task 148 |

**The flee rule (read in `Combat.c`).** When a side flees, each of its fleeing units is lost with a
chance: **50%, or 25% for a hero** (the attacker's branch writes it as a survival roll of 50% and
75%; the defender's as a death roll of 50% and 25%). Three details:

- **On Intro and Easy, the player whose turn it is loses nothing when they flee** (`_difficulty <=
  god_Easy && fleeing_player == _current_player`). The test is on whose turn it is, not on who is
  human, so it applies to an AI that flees on its own turn as well. It is one rule for both sides
  of the table: `safe_flee` is a `Split<bool>`, on for both at Intro and Easy.
- **Units that are asleep (Black Sleep), confused or held by a Web (with Web hit points left) are
  always lost** when their side flees, whatever the roll.
- The rolls happen after the battle, per unit, in the order of the original's battle list.

Ours (`battle/game.rs::overland_aftermath`) has all three since task 250: the 50% and 25% rolls,
the sure loss, and the safe flee on Intro and Easy. The safe flee reads `Game::difficulty <= 1`
and the attacker side (a battle starts on its mover's turn) directly; stage 2 replaces that test
with the `rules.safe_flee` setting. Web hit points are not modelled yet (task 159), so any Web
counts as holding the unit.

Only one place in the code reads `difficulty` outside the generator today (neutral city size,
`neutral_cities.rs`). The `ai` group is new code: city calculation (`city.rs`, `economy.rs`,
`finance.rs`) multiplies the city's figures by the owner's percent when the owner is not the human.
All of these are applied **at the end of the calculation**, as the original does, so the figures the
city screen shows for an AI city are the multiplied ones and nothing else needs to know.

### 4.8 How the original makes the AI wizards (`Init_Computer_Players_Wizard_Profile`)

Read in `INITGAME.c`. The original does not build AI wizards from fixed presets in turn; it
randomises them in phases, and several phases depend on difficulty:

1. **Who.** Each AI draws one of the 14 ready-made wizards at random, never one already taken
   (`Random(14)`, retried while taken). Names come from the preset. (Ours picks the next free
   preset from a seed-chosen index; same set, different order and no repeat rule beyond that.)
2. **Books.** The preset's five realm ranks are copied. **Above Normal, one time in five**
   (`Random(5) == 1`), `Consolidate_Spell_Book_Realms` merges opposed realms: where a wizard has
   books in both Death and Chaos, Nature or Sorcery, or Life and Chaos (and so on across the
   opposed pairs), a coin flip moves all books of one into the other. The result is a more
   focused wizard.
3. **Erasure** ("erasure dice"). By difficulty, 0, 0, 1d3, 2d3 or 2d5 rolls; each roll picks one of
   six things and removes a book from Life, Death, Chaos, Nature or Sorcery (never below one), or
   clears the wizard's retorts (the original's loop has an index bug: the ReMoM source marks it
   `OGBUG`; we clear them all, the evident intent). Higher levels erase more, but picks are then
   refilled, so the effect is a reshuffle, not a cut.
4. **Refill to the pick budget.** The picks used (books count one each; Myrran 3; Warlord, Infernal
   Power, Divine Power, Famous and Channeler 2; other retorts 1) are topped up to **11, or 13 on
   Hard, or 15 on Impossible**. Each step rolls a trait: five in eight add 1 to 4 books to a realm
   (Life and Death exclude each other; a realm stops at 11 books, the total at 12), three in eight
   add a random retort (realm masteries and Divine or Infernal Power only for a realm the wizard
   has books in, weighted by books; Warlord, Channeler and Famous only with a pick to spare,
   Myrran only with two to spare).
5. **Myrran wizards.** With **more than three wizards above Normal**, the original redraws the
   whole set of wizards until one or two AI wizards are Myrran. **We do not redraw.** The draw
   in phase 1 takes at most two Myrran presets (once two are in, Myrran presets are skipped);
   then, if no AI wizard is Myrran, the game **picks one or two AI wizards** (a coin flip for the
   count, the wizards chosen at random), **removes three books** from each (random books, weighted
   by the realm's books, the wizard keeping at least one book) and **gives them the Myrran retort**,
   which costs those three picks, so the pick total is unchanged. Done after phase 3 and before the
   refill of phase 4. The condition (above Normal, four or more wizards) is the original's;
   `myrran_rule` turns it off.
6. **Banner colour.** An AI takes the first banner free whose colour matches a realm it has more
   than three books in (green nature, red chaos, blue sorcery, purple death, and white for life),
   else a random free one. The human's banner is excluded.
7. Starting spells come from `Init_Computer_Players_Spell_Library` (the usual starting-spell rules).
   On Hard and above the original also adds one extra knowable spell per realm (buggy: it tests
   the wrong spell id for most realms, `OGBUG`); we leave it out.

The record keeps this as an `AiWizardSettings` block, so each phase can be turned off or tuned:

```rust
pub struct AiWizardSettings {
    pub randomise: bool,              // phase 1 and 4: random preset, refilled to the AI picks
    pub consolidate_percent: u8,      // phase 2: 0, 0, 0, 20, 20 by preset (the original's "above Normal")
    pub erasure_dice: (u8, u8),       // phase 3: dice count and sides: (0,0), (0,0), (1,3), (2,3), (2,5)
    pub myrran_rule: bool,            // phase 5: one or two AI wizards made Myrran (three books for the retort) above Normal with 4+ wizards
}
```

`picks` (4.1) is the budget of phase 4; with `randomise` off the AI uses the presets as they are,
today's behaviour. Everything uses the game's seeded generator, so the soak stays reproducible.
Races are chosen later, in the start (task 155 item 2): the original limits AI races above Normal
to nine, then five; that is `wizards.ai_race_limit`. Whether a preset wizard keeps its fixed race
or portrait is already handled in `setup.rs`; this list does not change it.

### 4.9 No Myrror (stage 4)

A new-game switch, `world.myrror_enabled` (default on): with it off the game is played on
**Arcanus alone**. The map generator makes one plane, and everything that exists only because
there are two is removed or refused. From the code and data (a first list, **not yet worked
through; each line is a check**):

- **Map and features.** `GenParams::planes` has an entry per plane; the second is skipped. Towers
  are placed at the same square on both planes to connect them, so there are none (or they are
  plain features). The Myrror node count and mineral share are unused; the lairs and sites that go
  on Myrror are dropped from the totals (`strong_sites`, `weak_sites` count both planes today; they
  need halving or a per-plane count).
- **Wizards.** The **Myrran retort** is not offered (it only means starting on Myrror, and Myrror
  spells and units), the Myrran ready-made wizards are hidden from the list and the AI's random
  draw skips them, and the Myrran rule of 4.8 is off. The custom wizard screen's pick budget is
  unchanged.
- **Content that needs Myrror.** Myrror-only races and units (a wizard cannot be one), Planar
  Travel, Plane Shift and Planar Seal (they would have no effect: they are removed from the
  spell pool, as the Spell of Mastery is when off, so they cannot be researched), and the
  overland plane switch in the client (the Plane button, the minimap's second plane).
- **The AI.** Any code that picks a plane or plans plane travel must treat a one-plane map as
  normal. To check how `f4x-ai` reads the plane.
- **Saves.** A saved map with one plane must load; `MapSave` stores planes by list, so to check
  that a missing plane is accepted and that the fog has one plane's layers only.
- **Score.** The final score has no plane term, to check.

Because it removes content, the screen shows what goes ("Myrran wizards, Planar Travel, Plane
Shift and Planar Seal are not in this game"). It is stage 4 because of the number of places that
assume two planes.

### 4.10 Hardcore mode (stage 4)

Designed in `PLANNED.md` item 8 and built on this record as `victory.hardcore`, a switch under
Extras. It bundles three rules:

- **no undo**: the undo of orders (task 90) is off for the game;
- **permanent death**: a banished wizard is defeated at once, which is `spell_of_return = Off`
  forced (4.6), and the Return spell is not available;
- **one save file, constantly updated**: the game saves after every command that matters and at
  every turn end into a single file, cannot be saved elsewhere and cannot load an older state;
  the client refuses the quick-load key and the file picker.

The first two rules are plain reads of the record. The third needs client work (auto-save on
every important command, the file lock, the menu changes) and a note on the hall of fame entry
(task 88), which is why it is stage 4. It is saved in the settings, so a load keeps the rule.
Hardcore does not change the dropdown (it is an extra) but the screen shows it in the base block's
note ("Average + extras").

## 5. The screens

### 5.1 Base settings (task 94's screen)

The screen of task 94, in two blocks. The first is the original's options step, as
`options_screen.rs` builds it; the second is the presentation part (not saved with a game).

```
 Settings
  GAME
   Difficulty      < Intro | Easy | [Average] | Hard | Impossible >
   Opponents       < 1 | 2 | 3 | 4 >
   Land size       < Small | Medium | [Large] >
   Magic power     < Weak | [Normal] | Powerful >
   Seed            [ 4711          ]  (task 148)
   [ Advanced settings... ]                 Average (original values)
  PRESENTATION
   Music / sound volume, animation and message speeds, show enemy moves ...
   [ Cancel ]                               [ OK ]
```

- **Advanced settings...** opens the advanced screen over this one and comes back to it. The note
  beside the button reads "Average (original values)" while nothing differs from the level,
  "Custom (based on Hard)" once the dropdown shows Custom, and "Average + extras" when only the
  extras (units, switches, map) were changed.
- Changing the difficulty here re-applies the level to the advanced values, after a confirmation
  when they were changed by hand.
- From the start menu (86) and in game: in game the GAME block and the advanced screen are
  read-only.
- Stage 2 of section 9 builds this screen with the **original's** four choices and no button;
  the button arrives with stage 3.

### 5.2 Advanced settings

A new `advanced_settings_screen.rs`, a scrolling list in sections, each row a label, a control and
a short help line, drawn with the existing `ui` widgets. Controls are four kinds: **stepper**
(number with left/right and a typed value), **choice** (the enum rows), **toggle** and **percent**.
Rows that hold a `Split` show two steppers, "You" and "AI". The top row is the **difficulty
dropdown**.

```
 Advanced settings
 Difficulty  [ Average v ]          (Intro, Easy, Average, Hard, Impossible, Custom)
 ───────────────────────────────────────────────────────────────────────────────
 START                                  (rows below change the dropdown to Custom)
   Starting gold                  You < 75 >   AI < 150 >
   Picks to spend                 You < 11 >   AI < 11 >
 COMPUTER WIZARDS
   City growth                    < 150% >
   Production, gold, mana, food   < 150% >  < 150% >  < 150% >  < 150% >
   Research                       < 100% >
   Upkeep and maintenance         < 75% >   < 100% >
   Random wizards, refill picks   [x]      Books merged  < 0% >   Erasure  < 1d3 >
 WORLD
   Lair guards                    < 75% >    Lair treasure   < 133% >
   Neutral city size              < +1 >     Raiders         < 33% >
 EVENTS AND END
   Random events                  [x]
   Fleeing is safe                You [ ]    AI [ ]
   Final score                    < 100% >
 EXTRAS                                 (rows below do not change the dropdown)
   Starting mana                  You < 0 >    AI < 0 >
   Books per realm, most          < 11 >
   Human gets the computer wizards' bonuses   [ ]
   Event frequency                < Half | [Normal] | Double >
   Starting units                 You [Spearmen x1] [Swordsmen x1] [+]   AI [same as you]
                                  kinds: race unit, Settlers, first summon, Scout, Ship
   Spell of Mastery wins          [x]
   Spell of Return                < All wizards | Human only | Off >
   Heroes                         [x]        Lairs  [x]    Neutral cities  [x]
   Units in a stack, most         < 9 >      (1 to 9; more: see large-stacks.md)
   City size, largest             < 25 >     Distance between cities < 4 >
   Hardcore                       [ ]       (stage 4)
   No Myrror                      [ ]       (stage 4)
   Map width / height             < 60 > < 40 >    Towers, sites, nodes ...

   [ Reset to level ]   [ Back ]
```

Rules for the screen:

- Rows of the START, COMPUTER WIZARDS, WORLD and EVENTS AND END blocks are the level-filled ones; each has a default marker and, when it differs from the level, shows the level's value as a
  dim hint, and **Reset to level** puts them all back.
- A value outside its range cannot be typed; the stepper clamps, and the cross-checks (section 7)
  show an inline message rather than blocking silently.
- The start menu's **Load** list shows the difficulty (or "Custom") for each save, read from the
  saved settings.
- Console: `settings` prints the record, `new ... key=value` sets fields (3.4).

## 6. Room for more

1. **A new field is a three-line change**: add it to the struct with `#[serde(default = ...)]`, give
   it a range in `validate`, add a row in the advanced screen's table. No save migration.
2. **Groups are fixed, fields are not**; the screen is built from a `&[Row]` table, so a new field
   needs no new layout code. Each row says whether it counts for Custom (a level fills it in) or is
   an extra.
3. **Candidates already named** in other docs: more than four opponents (`PLANNED.md` 3); starting
   fame and the capital's size (4.2); per-event on/off (4.5); ruleset switches such as the
   community patch's rule changes (`PLANNED.md` 4: each patch rule is a setting, default original);
   the undo setting (task 90); stack limits above nine ([`large-stacks.md`](large-stacks.md)); a
   raised city size cap (stage 4).
4. **The human's multipliers** are the `ai.apply_to_human` switch, off by default.

## 7. Validation

`GameSettings::validate` returns a list of problems (not just the first), used by both screens, the
console and the loader:

- every field within its range;
- `picks` at least the cheapest legal wizard; `max_books_per_realm * 5 >= picks - 1` is not
  required (a wizard may leave picks unused? No: the original forces all picks to be spent, and
  `wizard.rs` reports `UnspentPicks`; a `picks` the books cannot absorb is refused here with the
  reason);
- `opponents` fit on the map (a minimum area per wizard; the existing check
  `Fortress sites that feed eight` fails for tiny maps, so a size below it is refused here instead of
  at generation);
- `map_width * map_height * land_percent` leaves room for `tower_count`, sites and nodes (the
  generator's `GenError::BadParams` is the backstop, and the screen explains it);
- `hardcore` forces `spell_of_return = Off` and is shown as such.

A loaded save with an out-of-range value is **clamped, not refused**, and the load reports it, in
the same spirit as `game.magic_strength = self.magic_strength.min(2)` today.

## 8. Tests

- Round trip: a non-default `GameSettings` through snapshot save and load, byte for byte.
- Out-of-range values in a save are brought into range on load.
- Preset check: for each difficulty, `lair_*`, `neutral_city_size_bonus` and `score_percent` give the
  numbers the code produced before (golden values from the current functions).
- Baseline: the internal `flat` preset (100 gold for all, AI percents 100, picks 11, no
  randomisation) reproduces today's soak byte for byte, which proves the plumbing is neutral. The
  Average preset then changes the soak once (3.2); regenerate it and say so in the commit, as for
  task 219. `flat` is used by the soak and learning tools only.
- Custom: changing any level-filled value shows Custom; putting it back shows the level again;
  changing an extra does not.
- AI wizards: with a fixed seed, no repeated preset, the Myrran rule holds for four or more wizards
  above Normal, picks used equal the budget, consolidation and erasure only where the level says.
- Flee: Intro and Easy lose nothing when the player on turn flees; Average does the roll; a
  sleeping, confused or webbed unit is always lost.
- AI group: an AI city's figures are multiplied by its percents and a human city's are not; a
  neutral city's are not either; the multiplier is applied at the end of the calculation.
- Split values: human and AI wizards get their own gold, mana and picks at setup.
- One test per switch: Mastery off (the cast is refused and research does not offer it); Return in
  its three modes with a human and an AI loser; stack limit 5 (move, gate, spell, battle summons,
  AI gathering all stop at five); starting gold and mana at setup, split human and AI; events off fire
  nothing and `event_frequency` Half fires about half as many nothing over a long run.
- Screen: the pure parts (preset application, "customised" detection, row table ranges) in unit
  tests; drawing is checked by hand with a screenshot, as other screens are.

## 9. Stages

1. **Wireframe, save, load, default difficulty.** `settings.rs` with `GameSettings` holding the
   basic block and the groups as empty or default structs; `Game::settings()`; the snapshot field
   (old saves are not migrated: a save without the record reads as the defaults);
   `validate`; presets as functions that so far return the same numbers as the code uses today;
   the new-game path takes a `GameSettings`; the console `new` and `settings` commands. **No
   behaviour change**, soak identical, and the default difficulty is Average as now. Tests:
   round trip, `validate`.
2. **The base screen with the original's settings.** Task 94's base block (difficulty, opponents,
   land size, magic power, seed) over the record, and the original's difficulty effects behind it:
   starting gold and picks by level (human and AI), the `ai` multipliers, the AI wizard rules
   (4.8), lair guards and treasure, neutral city size, score scale, safe flee, event frequency and
   the Intro switch-off (with task 68). The soak changes once; `flat` stays for the soak.
3. **Advanced settings that are easy to build.** The advanced screen with the dropdown and Custom,
   and the **Advanced settings...** button. Rows: starting gold, mana and picks (human and AI),
   the AI multipliers and `apply_to_human`, the AI wizard block, starting units (including
   settlers, scouts, ships and the first summon), the switches (Spell of Mastery, Spell of
   Return, heroes, lairs, neutral cities, raiders, random events and their frequency, fleeing),
   the lowered stack limit and city size cap, city distance, map size and counts, score scale.
4. **Hard settings.** **No Myrror** (4.9) and **hardcore mode** (4.10), larger stacks ([`large-stacks.md`](large-stacks.md)), a raised city size cap
   and the other settings that need new art, layout or balance work: more than four opponents, per
   event switches. Each gets its own task and its own tests, and its range on the screen is raised
   only when it is done.

Tasks 89 and 94 become stages 1 to 3 (tasks 246 to 248; stage 4 is task 249); the presentation half of task 94 is separate (not saved per
game).

## 10. Open questions

Settled on 16 October: the screen is task 94's base screen plus an Advanced button; the advanced
screen has a difficulty dropdown with Custom; `flat` is internal; event frequency is Half, Normal
or Double; the human getting the AI multipliers is a custom switch; settlers are a custom option,
not in the default; larger stacks are a later design.

Settled later the same day: ready-made wizards stay on offer with other picks and get the extra
books or retorts as in 4.1; the Myrran AI wizards are made by taking three books, not by
redrawing (4.8); the dropdown covers only what the original's difficulty touches (3.2); the basic
summon is the first summoning spell from the wizard's starting spells (4.2); no Myrror and
hardcore are stage 4.

Still open:

1. **Fewer picks than a ready-made wizard costs.** The extra-picks rule is decided; the opposite
   is not. Remove books by the same rule run backwards (the realm with most books loses one, a coin
   flip on ties, then retorts), or refuse such a count for ready-made wizards and offer the custom
   screen only?
3. **The free spirit's mana.** (Decided 16 October: the zero-power edge case, a wizard with no
   mana share who loses the spirit, is accepted for now.) Adopt the slider rule proposed in 4.2 (raise the mana share at the
   start so the upkeep is covered), or give the starting spirit no upkeep (a flag on the starting
   unit)?
4. **Order of the starting spells** (4.2): "first" summoning spell needs the order the starting
   spells are given in, which I have not checked.
5. **Which three books** go to a new Myrran AI wizard (4.8): random, weighted by books, keeping at
   least one book, is a proposal.
