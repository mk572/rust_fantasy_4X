# Open Tasks

Everything not yet implemented.

Next open task number: 359 (increase when a task is created)
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
| 335 | Core | Save format 2 and the game-version check |
| 336 | Core | Other crates: `PlayerView`, console, client, tools |
| 337 | Docs | Update the docs; guard tests; knowledge-base entry |
| 339 | AI | Scripted and trained AI on the new framework |
| 341 | Wizard | Banishment and defeat as in the original (`Banish_Wizard`, `Resolve_Wizard_Conquest`) |
| 342 | Core | Clean up `f4x-core/src/content.rs`: move its contents to the concepts they belong to |
| 346 | Core | Replace the loader's test data (`crates/f4x-data/testdata/`) with the real `content/` |
| 358 | Text | Names in the rules files (units, spells, buildings, races, realms, abilities, item powers) per language |
| 350 | Text | Refusals and notices the original has and ours lacks (continue-casting prompts, unit limits, Spell Blast mana, nodes, and more) |
| 351 | Text | Research the `MESSAGE.LBX` lines of task 348 that are not traced: 8, 10, 18, 19, 20, 45, 46, 61 |
| 357 | Client | Site discovery: the music and the colour slide of the node pictures |

## Descriptions

Today `Game.map` (outside the world) holds a `FeatureId` per tile, `Map.nodes`, `Map.lairs`,
`tower_owners` and an owner byte per volcano. Nodes and lairs are found by `Vec` index
(`Entry::Site(usize)`, `lair_at`, `node_at`), which breaks when one is removed.

### G. Closing

**335. Save format 2 and the game-version check.** `SAVE_VERSION = 2`. Entities are saved in `Serial`
order as now, with player, wizard, hero, spell, terrain feature and item entities. `PlayerSave`,
`MagicSave`, `UnitSave.hero`, `UnitSave.items`, the roster and the node, lair and tower tables go;
`MapSave` keeps the terrain layers (with the node base terrains of 338). The loader refuses any other game version with a plain message
("saved by 0.0.1, this is 0.0.2"); replay files are checked the same way. Round-trip tests for each
new entity. *Needs:* 317, 325, 332, 334 and 338.

**336. Other crates.** `PlayerView`, `f4x-console`, `f4x-client` (about 65 player lookups; node and
lair drawing, `surveyor`, `site_prompt`, `map_draw`) and `f4x-tools` use the new handles. The AI
crates (`f4x-ai`, `f4x-learn`) only need to compile and pass their tests here, with the smallest
change that does it; the real work on them is task 339. *Needs:* 335.

**337. Docs and guards.** Bring `ecs-structure.md`, `design.md`, `magic.md`, `map-layers.md`, `lairs.md`
and `architecture-plan.md` up to date. A test (or a script, logged in `docs/scripts.md`) fails on
plural module names. Update the method-count limits of task 183's test. A knowledge-base entry
(project docs, or `~/projects/KNOWLEDGE_BASE.md` if general) for anything non-obvious found in the
rework. *Needs:* 336.

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
- **Docs:** `ai-design.md`, `ai-objectives.md`, `learning-ai.md` and `auto-builder.md` describe the
  new entities. The tournament and training scripts (`scripts/tournament.sh`, `train_battle.sh`) still
  work, and `docs/scripts.md` says so.

*Needs:* 336, 337 and 338.

### H. Gaps found during the rework

**341. Banishment and defeat as in the original.** Read from ReMoM (`CONQUEST.c`: `Resolve_Wizard_Conquest`,
`Banish_Wizard`, `CP_Is_Dead`; `City_ovr55.c`: `Change_City_Ownership`; `Spells128.c`: `Conquest_Spells`;
`NEXTTURN.c`: `All_Players_Apply_Spell_Casting`; `CITYCALC.c`; `AIDUDES.c`; `DIPLOMAC.c`; `MagicScr.c`;
`MainScr.c`) and compared with `wizard/fortress.rs`, `game/endgame.rs` (`aftermath`) and
`city/conquest.rs`. The trigger is the same as ours (the fortress city falls, or the last city). Already
as in the original: the fortress and circle go, the winner gets 5 fame and half the loser's mana, the
other wizards' relations drop by 20 (banished) or 40 (defeated), the Spell of Return starts at once, a
banished wizard gets no income of any kind and no casting skill in battle, offers stop, the loser's
units and heroes are dismissed, the enchantments it cast on cities and its global spells end, its
nodes are freed, the winner takes half its gold, its cities turn neutral, diplomacy with it is reset.
What differs or is missing:
- **The auto-defeat test applies to everyone.** In `Banish_Wizard` the test (base casting skill plus
  hero skill under 40, or mana under 1: defeated on the spot, no banishment) runs for every loser that
  is not the player whose turn it is, which in practice is every wizard whose fortress is taken by an
  attack, the human included. Ours exempts the human. (The original asks "Do you wish to resign?" only
  when a wizard loses its fortress on its own turn, which cannot happen to us; `Command::Resign` stays
  as the way to give up.) This is the rule the project owner chose ("as ReMoM"). It is turn-based, not
  AI-only, and it also covers the wiki's "zero mana: defeated" (which leaves out the skill part).
- **Paying the Spell of Return: one rule for every wizard.** The game's rules do not decide for the AI.
  Every banished wizard, human or AI, pays with mana from its reserve, as ours does now. With no mana
  the spell cannot advance; the wizard can only resign. The original's special case (a banished AI pays
  its nominal skill every turn whether or not the reserve covers it, and the reserve floors at 0) is not
  copied; record it in `divergences.md`.
- **The AI covers the payment in its script.** In `f4x-ai` (`wizardry`), a wizard that is casting the
  Spell of Return checks whether its mana reserve covers this turn's payment; if not, it converts as much
  gold as it needs, and no more, into mana with the existing `Command::Alchemy` (2 gold for 1 mana, 1 for 1
  with the Alchemy retort). The original does this inside the rules (`AI_Update_Gold_And_Mana_Reserves`
  turns all of a returning AI's gold into mana each turn, overwriting the reserve; ReMoM marks the `=` as
  a bug); ours is a choice of the script, not a rule. Done with the AI work of 339 if that comes first.
- **Conquest spells.** `Conquest_Spells`: the winner takes up to two spells, picked at random from those
  the loser knows and the winner does not (`Get_Differential_Spell_List`), when an AI wizard is banished and
  again when a wizard is defeated. A human winner gets a box: "You find no new spells in the ruins." or
  "You found a X spell [, a Y spell] in the ruins." Ours has nothing like it. Needs a new event and the box.
- **Defeat without banishment.** When the last city falls and it is not the fortress, the original still
  gives the winner 5 fame and half the loser's mana (`Resolve_Wizard_Conquest`); ours gives them only in
  the banishment path, so that case loses them.
- **No diplomacy while banished.** The client refuses the diplomacy screen ("You may not contact other
  wizards while you are banished", `MagicScr.c`); the AI makes no overtures and no delayed orders to a
  banished human (`NPC_To_Human_Diplomacy`, `Resolve_Delayed_Diplomacy_Orders`). Do not copy the early return
  in `NPC_To_NPC_Treaty_Negotiations`: ReMoM marks it `OGBUG` (it tests the human's casting in AI-to-AI
  talks). Word of Recall and Recall Hero are refused while banished (ours: refused through the one cast at a
  time rule; add the explicit check and its message). The spell button says "You may not throw any spells
  while you are banished. There are at least N turns remaining until you may return."
- **Ownership change.** `Change_City_Ownership` ends the previous owner's enchantments on any city that
  changes hands; ours does it on the post-battle path (`city/conquest.rs`) only. Do it in `capture_city`.
- **A difference we keep.** The original cannot remove a unit enchantment by its caster (it stores none);
  ours removes the loser's enchantments on other wizards' units too (`aftermath`). With spell instances (325)
  this is "all the spell instances the wizard cast"; record it in `divergences.md`.
- **Screens and animations (client, low priority):** the conquest animation, the "begins casting the Spell
  of Return" scene for a banished AI, the defeat and victory scenes, the banished wizard's portrait.
Each rule gets a test; `magic.md` section 12 and `divergences.md` section 25 are brought up to date.
*Needs:* 315, 317 and 325.

**342. Clean up `f4x-core/src/content.rs`.** The file is a grab-bag: terrain definitions and classes,
feature definitions and spawn rules, city bonuses and the `Content` bundle sit side by side, and parts of
it belong with the concept they describe (`feature`, `terrain`, `city`, `map`). Review it once the new
structure is in place, so the review sees where each concept ended up. For each item decide where it
belongs (its concept's module, or `f4x-data` if it is loading or validation), move it with no re-export
shim, keep `Content` only if something still needs a bundle, and update the imports in every crate and the
docs that name the file (`ecs-structure.md`, `scripts.md` if any).

*Needs:* 337 (the structure is final). Best after 330 to 332, which reshape the feature types.

**346. Replace the loader's test data with the real content.** `crates/f4x-data/testdata/` holds 12 `.ron` files (abilities,
buildings, features, item_powers, items, level_paths, races, realms, rules, spells, terrain, units) with the same names as
files in `content/`, loaded by the tests in `f4x-data` (`lib.rs`, `rules.rs`, `save.rs`). A copy goes stale when a rule or a
schema changes, and the tests then pass on data the game no longer has. Make those tests load `content/` (through
`load_content_layers` with `content/` alone, so a local `content-local/` never changes a result), fix the assertions that
depended on the fixture's numbers so they read the value from the loaded data or check structure, delete `testdata/`, and
fix `docs/local-assets.md` (line 63). A test that needs a situation `content/` cannot give builds it in the test from the
loaded rules, as the other crates' tests do. Error-handling tests (a broken file is refused) keep their small inline
sources; they are not fixtures.

*Needs:* nothing.

**358. Names in the rules files per language.** The names in `units.ron`, `spells.ron`, `buildings.ron`, `races.ron`,
`realms.ron`, `abilities.ron` and `item_powers.ron` are shown as they are (`Item::display_name` and the screens read
`def.name`). Decide and build how a language gets its own: a table of names by id in `content/text/<lang>/` that the
display sites read first (falling back to the file's name), or a copy of each file per language. Then every place that
prints a `def.name` goes through it, and the plural of a race (the original keeps it apart) needs its own form.

*Needs:* 347.

**350. Refusals and notices the original has and ours lacks.** From the table of task 348: the "continue casting?" prompts (lines 12 to 16, including the second cast of a global enchantment, task 349 leaves it here); the settler refusal for a fortress's last citizens (11); too many units on the square when a spell summons or a city builds (21, 50, 66); Disenchant Area with nothing to dispel (28); Spell Blast without the mana (68); Change Terrain on a node with line 34's own words (ours refuses it as a fixed terrain; Corruption on a node, 41, is done); Volcano on a river (86; check first whether the map has rivers); Web and Sorcery immunity refused at the aim, where ours casts and the unit resists (71, 73); Lycanthropy on a mutated unit; the mass battle spells with nothing to hit (79, 80, 81, 83); wizard targets without contact (27); the notices of dispelled enchantments (18, 19, 20, 45, 46) and the monsters' rampage (10), whose triggers must be found first. Decide for each: add it, or record in `docs/divergences.md` that it is not wanted. The new texts are keys in `content/text/`.

*Needs:* 348 (done).

**351. Research the untraced `MESSAGE.LBX` lines.** Task 348, which is finished, could not say what rule stands behind these lines of the original: 8 ("All units retreat exhausted"), 10 (monsters rampage through the city), 18, 19, 20, 45, 46 (notices that other city, unit, Stasis or combat enchantments were dispelled) and 61 (Guise on a hero; no spell of that name in `spells.ron`). For each: find the original code that shows the message (ReMoM) and what triggers it, check whether our game has the rule, and write the answer in this task's "Done" paragraph as in the table of task 348: refused, no refusal, different on purpose, or no such rule. A missing rule goes to task 350 or a task of its own; the text is a key in `content/text/`.

*Needs:* 348 (done).

**357. Site discovery: the music and the node colour slide.** The original plays a short discovery tune when a stack
finds a site and slides the colours of a node's picture (`docs/lairs.md`, "Missing"). Add both to the site prompt
(`f4x-client/src/site_prompt.rs`) where the client has the music and image support for it; text stays in
`content/text/en/site.ron`.

*Needs:* nothing.

Already tracked elsewhere (not repeated): spell and book treasure that matches the wizard's books is on-hold task 305;
checking the lair data against `MONSTER.LBX` and `EVENTS.LBX` is on-hold task 241; where loot items are made (at
clearing today) belongs to task 334; a freed prisoner needs no square because the hire is free, so nothing is open there.
