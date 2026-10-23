# Open Tasks

Everything not yet implemented.

Next open task number: 362 (increase when a task is created)
Keep the table in ascending order by number.
Add a summary to the table, then details under Descriptions.

Bracketed numbers like [430] point at topics in the original help
(`content-local/help.ron`)
B/C numbers point at `help-vs-implementation-conflicts.md`.

Gaps found while working on the base game are added here as new tasks, not left in notes.

Finished tasks are moved to [`TASKS-DONE.md`](TASKS-DONE.md) with
`python3 scripts/archive_done_tasks.py` (a task is done when its row or title says "(done)").

The older open tasks are parked in [`TASKS-ONHOLD.md`](TASKS-ONHOLD.md).

## The rework: concept entities (tasks 307 to 342)

Goal: the code is organised by the game's concepts, and the concepts are entities. Today only unit
and city are `hecs` entities. After the rework **player**, **wizard**, **hero**, **spell instance**,
**terrain feature** (nodes, lairs, towers, minerals, volcanoes) and **item** are too, names are
singular, and `PlayerId` lives with the player it identifies.

Rules for every task of the rework:
- **No change in the game's rules** (the one exception is task 338, which says so). Seeds do not
  need to stay consistent: the RNG draw order, the event order, the learned-AI observations and old
  saves may change. The tests (`cargo test --workspace`), `cargo fmt --all` and
  `cargo clippy --workspace --tests` pass after each task.
- **Do not keep a bad structure to avoid churn.** When a type, a name or a field is in the wrong place,
  move it and fix every use. No re-export shims or wrapper methods are left behind.
- **Docs first**: the design that the later tasks implement is in `docs/ecs-structure.md` section 3
  (written by task 310). Read it before starting task 311. Where a task and the design disagree, the
  design wins; fix the task text.
- **Saves in between.** Until task 335 the save format keeps its layout: each task adapts the mapping
  in `game/save.rs` so that the save round-trip tests stay green, and 335 redesigns the format. Old
  saves need not load after 335 (the game version is checked).
- **The rules treat the AI like a human.** A game rule never decides for the AI. If an AI wizard needs
  to do something (convert gold to mana, answer an offer), its script in `f4x-ai` does it with the same
  commands a human has.
- **One task at a time, then mark it done.** Run `cargo fmt --all`, `cargo clippy --workspace --tests`
  and `cargo test --workspace`; mark the task "(done)" in `TASKS.md` and run
  `python3 scripts/archive_done_tasks.py`. Update the docs the task names. Commits are made when the
  project owner asks.

Phases and dependencies (a task needs the tasks before it in its chain):

```
A prepare  307 > 308 > 309 > 310
B player   311 > 312 > 313 > 314 > 315 > 316 > 317     (needs A)
C spell    318 (needs 315) > 319, 320, 321, 322, 323, 324 (each needs 318) > 325
           340 (the art folders; no code dependency, placed last in C)
D hero     326 (needs 317) > 327 > 328                  (328 also needs 318)
E feature  329 (needs 313) > 330, 331, 332 (each needs 329)   (330 also needs 323 for warped nodes; 332 needs 313)
           338 (needs 329 and 330; it belongs to phase E but was added last)
F item     333 (needs 313, 315, 326) > 334 (needs 322, 327, 328, 331)
G close    335 (needs B to F and 338) > 336 > 337 > 339 (the AI, last)
H gaps     341 (needs 315, 317 and 325; its AI part is done with 339's AI work, after 339 if 339 comes first)
           342 (needs 337; clean-up of `content.rs` once the structure is final)
```

Phases B and C meet only at 315 (the wizard's spell lists) and 322 (the cast in progress). Phase E
needs only the player entity, so it can run in parallel with C and D; phase F needs both its
neighbours.

## Table

| # | Area | Summary |
|---|---|---|
| 339 | AI | Scripted and trained AI on the new framework |
| 342 | Core | Clean up `f4x-core/src/content.rs`: move its contents to the concepts they belong to |
| 358 | Text | Names in the rules files (units, spells, buildings, races, realms, abilities, item powers) per language |
| 350 | Text | Refusals and notices the original has and ours lacks (continue-casting prompts, unit limits, Spell Blast mana, nodes, and more) |
| 351 | Text | Research the `MESSAGE.LBX` lines of task 348 that are not traced: 8, 10, 18, 19, 20, 45, 46, 61 |
| 357 | Client | Site discovery: the music and the colour slide of the node pictures |
| 359 | Client | Replace pictures with text baked in by text-free pictures plus font text, for localization |
| 360 | Client | Conquest, banishment and defeat scenes |
| 361 | AI | The AI makes diplomacy proposals, to the human and to each other |

## Descriptions

Today `Game.map` (outside the world) holds a `FeatureId` per tile, `Map.nodes`, `Map.lairs`,
`tower_owners` and an owner byte per volcano. Nodes and lairs are found by `Vec` index
(`Entry::Site(usize)`, `lair_at`, `node_at`), which breaks when one is removed.

### G. Closing

**339. Scripted and trained AI on the new framework.** The last task, because both AIs sit on top of
everything else.
- **Scripted AI (`f4x-ai`, about 4,400 lines):** `economy`, `expand`, `military`, `wizardry`,
  `naval`, `scouts`, `race`, `offers`, `shortfall`, `diplomacy` read the game through
  `PlayerView` (336). Rewrite them on the player and wizard handles (a wizard's books, retorts,
  skill and fame; a player's gold, mana and orders), on spell instances for enchantments and
  casts in progress, on hero entities instead of the roster, and on feature and item entities
  (node and lair targets by entity, not by `Vec` index; items by holder). Neutral players have no
  wizard: the neutral-city AI (`city/neutral.rs`) takes that explicitly. The objectives and
  personalities now live on the wizard (316). Check the tests in `crates/f4x-ai/tests` and the
  scripted battle AI (`battle/ai.rs`, `brain.rs`).
- **Trained AI (`f4x-learn`, about 6,200 lines):** `obs.rs`, `act.rs`, `env.rs`, `reward.rs`,
  `scenario.rs`, `duel.rs`, `net.rs`, `train.rs` and the agents. The observation and action
  encodings may change where the new entities make them simpler. Stored networks in `models-local/`
  (`battle-v1.bpk`, the banks and the run folders) that no longer load or no longer match are
  retrained or dropped; say which in the task's notes. Scenario files that use the old save or
  setup format are converted or rebuilt. After 338 (node terrains change food, production and
  movement) run the mirror tests and the battle behaviour tests. Slow runs only when asked:
  `cargo test -p f4x-ai --test behaviour -- --ignored`,
  `cargo test -p f4x-learn --test mirror -- --ignored`, `f4x-tools soak`.
- **Banished wizard (from 341):** a computer wizard casting the Spell of Return converts the gold it needs
  into mana with `Command::Alchemy` when its reserve does not cover the turn's payment (see task 341's text
  in `TASKS-DONE.md`); it starts no talk while banished.
- **Docs:** `ai-design.md`, `ai-objectives.md`, `learning-ai.md` and `auto-builder.md` describe the
  new entities. The tournament and training scripts (`scripts/tournament.sh`, `train_battle.sh`) still
  work, and `docs/scripts.md` says so.

*Needs:* 336, 337 and 338.

### H. Gaps found during the rework

**342. Clean up `f4x-core/src/content.rs`.** The file is a grab-bag: terrain definitions and classes,
feature definitions and spawn rules, city bonuses and the `Content` bundle sit side by side, and parts of
it belong with the concept they describe (`feature`, `terrain`, `city`, `map`). Review it once the new
structure is in place, so the review sees where each concept ended up. For each item decide where it
belongs (its concept's module, or `f4x-data` if it is loading or validation), move it with no re-export
shim, keep `Content` only if something still needs a bundle, and update the imports in every crate and the
docs that name the file (`ecs-structure.md`, `scripts.md` if any).

*Needs:* 337 (the structure is final). Best after 330 to 332, which reshape the feature types.

**358. Names in the rules files per language.** The names in `units.ron`, `spells.ron`, `buildings.ron`, `races.ron`,
`realms.ron`, `abilities.ron` and `item_powers.ron` are shown as they are (`Item::display_name` and the screens read
`def.name`). Decide and build how a language gets its own: a table of names by id in `content/text/<lang>/` that the
display sites read first (falling back to the file's name), or a copy of each file per language. Then every place that
prints a `def.name` goes through it, and the plural of a race (the original keeps it apart) needs its own form.

*Needs:* 347.

**360. Conquest, banishment and defeat scenes (client).** From task 341: the original's conquest
animation, the "begins casting the Spell of Return" scene for a banished computer wizard, the defeat
and victory scenes and the banished wizard's portrait. Read `Conquest_Animation`, `Return_Animation`,
`Lose_Animation` and `Win_Animation` (ReMoM `CONQUEST.c`) first. *Needs:* 341 (done).

**361. The AI makes diplomacy proposals.** Today the AI only answers (`f4x-ai/src/diplomacy.rs`: `answer` rates
each proposal with `Game::rate_proposal`); it never proposes, threatens, declares war or makes peace, so the
human is never approached and AI wizards never treat with each other. The rules and numbers are in
`docs/diplomacy.md` sections 6 to 8 (`NPC_To_Human_Diplomacy`, `NPC_To_NPC_Treaty_Negotiations`,
`DIPL_HumanWarOrPeace`, `G_DIPL_NeedForWar`, `DIPL_GetOffMyLawn`, `NPC_Required_Alliances`); read those and ReMoM
(`DIPLOMAC.c`) first. As always the rules do not decide for the AI: the script sends the same
`Command::Diplomacy(..)` a human has (`Propose`, `DeclareWar`, `BreakTreaty`, `GiveGold`, `GiveSpell`), and the
core only checks that they are legal.
- **Towards the human**, once a turn for each AI wizard with a fortress: greet a wizard newly met; otherwise the
  score test, the war-or-peace decision (warnings, breaking a treaty, declaring war, offering peace), demands,
  the required-alliance request and the need-for-war rolls (chaotic, militarist, superiority war, joining an
  ally's war), and "get off my lawn". The human sees each as a message or a talk screen, and the client needs
  to show a proposal the AI makes (`Event::Diplomacy`; check what `diplomacy_screen.rs` does with a
  proposal that is waiting for the human, and add the answer path if it is missing).
- **Towards each other**, for each ordered pair of living AI wizards that have met, with chance
  `1 / (15 - 2 * difficulty)`: the score, pact or alliance, spell exchange, peace between wizards at war,
  allies joining wars. Do not copy the early return in `NPC_To_NPC_Treaty_Negotiations` that `ReMoM` marks
  `OGBUG`.
- **Not while banished** (task 341): a banished wizard starts nothing (`DiplomacyError::Banished`), and no AI
  makes overtures to a banished human or sends it delayed orders.
- **Tests** for each decision with fixed rolls on a real game; an AI-only game test (soak, run only when
  asked) shows treaties and wars forming; `diplomacy.md`, `ai-design.md` and `divergences.md` are updated.
*Needs:* nothing in the core; do it with or after task 339 (the AI on the new framework), since both touch the
scripts' structure.

**350. Refusals and notices the original has and ours lacks.** From the table of task 348: the "continue casting?" prompts (lines 12 to 16, including the second cast of a global enchantment, task 349 leaves it here); the settler refusal for a fortress's last citizens (11); too many units on the square when a spell summons or a city builds (21, 50, 66); Disenchant Area with nothing to dispel (28); Spell Blast without the mana (68); Change Terrain on a node with line 34's own words (ours refuses it as a fixed terrain; Corruption on a node, 41, is done); Volcano on a river (86; check first whether the map has rivers); Web and Sorcery immunity refused at the aim, where ours casts and the unit resists (71, 73); Lycanthropy on a mutated unit; the mass battle spells with nothing to hit (79, 80, 81, 83); wizard targets without contact (27); the notices of dispelled enchantments (18, 19, 20, 45, 46) and the monsters' rampage (10), whose triggers must be found first. Decide for each: add it, or record in `docs/divergences.md` that it is not wanted. The new texts are keys in `content/text/`.

*Needs:* 348 (done).

**351. Research the untraced `MESSAGE.LBX` lines.** Task 348, which is finished, could not say what rule stands behind these lines of the original: 8 ("All units retreat exhausted"), 10 (monsters rampage through the city), 18, 19, 20, 45, 46 (notices that other city, unit, Stasis or combat enchantments were dispelled) and 61 (Guise on a hero; no spell of that name in `spells.ron`). For each: find the original code that shows the message (ReMoM) and what triggers it, check whether our game has the rule, and write the answer in this task's "Done" paragraph as in the table of task 348: refused, no refusal, different on purpose, or no such rule. A missing rule goes to task 350 or a task of its own; the text is a key in `content/text/`.

*Needs:* 348 (done).

**357. Site discovery: the music and the node colour slide.** The original plays a short discovery tune when a stack
finds a site and slides the colours of a node's picture (`docs/lairs.md`, "Missing"). Add both to the site prompt
(`f4x-client/src/site_prompt.rs`) where the client has the music and image support for it; text stays in
`content/text/en/site.ron`.

*Needs:* nothing.

**359. Pictures with text baked in become plain pictures plus font text.** Many of the original's pictures carry
words in the bitmap itself: button faces ("Done", "Patrol", "Buy", "OK"), screen titles and headings, the labels on the
magic, city, army, diplomacy and load screens, and the unit and spell names on cards. The client draws these as they
are, so a language other than English would still show English words on them (and the original's font cannot draw
every script). Make every such picture text-free and draw its words from `content/text/<lang>/` in a font, as the rest
of the player text already is (`docs/text.md`):
1. **Find them.** List the pictures that contain text, per screen: start from `crates/f4x-tools/screens.tsv` and the
   labelled files in `assets-local/art` (`main_screen/button_done_00.png` and the like), look at each, and write the
   list (file, screen, the words, where the client draws it) into a table in `docs/extract-art.md`. Say which of them
   the original itself draws as text over a plain picture (those need nothing) and which are baked.
2. **Make them plain.** For each baked picture, a text-free version: a tool in `f4x-tools` that blanks the text area
   of the extracted picture (the plain button face, banner or plaque), or a redraw where a blank cannot be cut out.
   The extractor writes the plain version; the baked one is not used by the client any more. Log the tool in
   `docs/scripts.md` with `--help` as the project rules say.
3. **Draw the words.** The client draws the text on the plain picture, centred in the area the old words filled, with
   a key in `content/text/en/*.ron` for each (count, plural and gender forms where the language needs them). Button
   labels shrink or wrap to fit; a test fails when a key is missing, as for other text.
4. **Fonts.** The original's bitmap fonts (`FONTS.LBX`, `docs/extract-art.md` section 5) hold only ASCII 32 to 127.
   Decide and build the font the localized text uses: a vector font loaded by the client (a file in `assets/` with
   its licence noted) for languages with accents or other scripts, and the bitmap font kept for English where it
   keeps the look. A language names its font in `content/text/<lang>/`'s settings.
5. **Check.** A screenshot of each changed screen in English matches the old look closely (position and size of the
   words); a second language, even a test one with long words, shows no overflow.

*Needs:* nothing for 1 to 3; 358 is the same job for the names in the rules files and can share the fonts of step 4.

Already tracked elsewhere (not repeated): spell and book treasure that matches the wizard's books is on-hold task 305;
checking the lair data against `MONSTER.LBX` and `EVENTS.LBX` is on-hold task 241; where loot items are made (at
clearing today) belongs to task 334; a freed prisoner needs no square because the hire is free, so nothing is open there.
