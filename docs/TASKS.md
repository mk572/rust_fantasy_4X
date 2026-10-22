# Open Tasks

Everything not yet implemented.

Next open task number: 342 (increase when a task is created)
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
| 318 | Spell | Spell instance model: components, lifecycle, queries |
| 319 | Spell | Global enchantments as spell entities |
| 320 | Spell | City enchantments as spell entities |
| 321 | Spell | Unit enchantments and temporary effects as spell entities |
| 322 | Spell | Casts in progress, crafting and raising as spell entities |
| 323 | Spell | Map-square spells and lasting world events |
| 324 | Spell | Battlefield enchantments in the battle's own world |
| 325 | Spell | Dispel, upkeep and removal in one place |
| 326 | Hero | Hero entity that outlives its unit body |
| 327 | Hero | Hero slots, hiring, recall, resurrection and offers over hero entities; remove `Roster` |
| 328 | Hero | Hero spells as spell instances; the armory moves to the player |
| 329 | Feature | Terrain feature entity: position index, mapgen as a placement list, minerals and encounter markers |
| 330 | Feature | Node entity |
| 331 | Feature | Lair and site entity, tower on both planes, loot |
| 332 | Feature | Volcano and tower ownership as feature entities |
| 333 | Item | Item entity: components, holder, lifecycle |
| 334 | Item | Hero slots, armory, crafting, offers, awards, spoils and lair loot over item entities |
| 335 | Core | Save format 2 and the game-version check |
| 336 | Core | Other crates: `PlayerView`, console, client, tools |
| 337 | Docs | Update the docs; guard tests; knowledge-base entry |
| 338 | Feature | Node base terrains (sorcery, nature, chaos) under the node features, with the original's food and production |
| 339 | AI | Scripted and trained AI on the new framework |
| 340 | Spell | One art folder per spell in `assets-local/art/spells` |
| 341 | Wizard | Banishment and defeat as in the original (`Banish_Wizard`, `Resolve_Wizard_Conquest`) |
| 342 | Core | Clean up `f4x-core/src/content.rs`: move its contents to the concepts they belong to |

## Descriptions

### C. Spell

**318. Spell instance model.** Components `SpellCast`, `Caster`, `SpellTarget`, `Duration`; helpers to
spawn, despawn and query instances by target, caster or definition, in `Serial` order wherever the
order is observable. Lifecycle: an instance is despawned when it expires, is dispelled, its caster is
banished or defeated, or its target is gone (unit, city or square). New module `spell/`. *Needs:* 315.

**319. Global enchantments.** `Magic.global`, `globals.rs` (Evil Omens and the rest) and
`world_spells.rs` read and write spell instances with target `Global`. *Needs:* 318.

**320. City enchantments.** `city::economy::Enchantments`, `spell::city`, wards and the Nightshade dispel
strength use spell instances with target `City`. The caster is the wizard entity. *Needs:* 318.

**321. Unit enchantments and temporary effects.** `UnitEnchantments` and `TempEffects` become spell
instances with target `Unit`; the stat modifier engine reads the instances of a unit. Temporary
effects that no spell causes (the three kinds `Modifier`, `Grant`, `Remove` with a duration) are
checked first and kept as a component if some source is not a spell. *Needs:* 318.

**322. Casts in progress.** `Magic.casting`, `crafting` and `raising` become spell instances in a
casting state, owned by the wizard; `raising` stops indexing the roster (finished in 327). The
transitional component from 315 is removed. *Needs:* 318.

**323. Map-square spells and lasting world events.** Enchanted roads and `events::Lasting` use spell
instances with target `Square` or `Global`. Corruption is map data, not a spell, and stays in `Map`.
Warped nodes use target `Feature` and are finished in 330, when the node entity exists (until then
the `warped` flag stays on `Node`). *Needs:* 318.

**324. Battlefield enchantments.** `Battle.fields`, `counter_magic`, `wall_of_fire`, `warded_realms`
and `vortex` become spell instances in a battle-local `hecs::World`. Battles are not saved today;
saving a battle (on hold, task 99) will need to include this world. *Needs:* 318.

**325. Dispel, upkeep and removal in one place.** `dispel.rs`, the mana upkeep and "caster banished or
defeated" become queries over spell instances. Remove the old containers and the transitional
component. *Needs:* 319 to 324.

**340. One art folder per spell.** The extracted art is one folder per source `.lbx` in
`assets-local/art/` (`specfx` 503 files, `cmbtfx` 312, `spellscr` 255, `special` 107, `special2` 44,
`magic` 54, `spells` 12), files named `<entry>_<frame>.png`, listed in `manifest.tsv`. Which spell
uses which picture is only in code and docs: the tables in `spell_fx.rs` (battle), `global_spell.rs`,
`event_popup.rs`, `learn_spell.rs` and `overland_spell_animations.md`. Change it to
`assets-local/art/spells/<spell id>/`, one folder for each spell of `content/spells.ron`, holding
that spell's pictures and animation frames, moved out of the `.lbx` folders (not copied), file names
kept as `<lbx>_<entry>_<frame>.png` so the source stays traceable.
- **The spell-to-art table first.** Write it as one data file (`content/spell_art.ron`: spell id to
  a list of `(lbx, entry, role)`, roles such as cast, projectile, impact, summon, global, notice),
  built from the existing tables and the doc, and make `spell_fx.rs`, `global_spell.rs`,
  `event_popup.rs` and `learn_spell.rs` read it instead of their own copies. A test checks that every
  id in it is a spell and every spell with art has a folder.
- **Shared pictures.** Some pictures belong to many spells (the five realm pictures `specfx` 0 to
  5, the summon backgrounds `spellscr` 46 and up, the city-spell scroll `spellscr` 73, the sparkle
  `specfx` 49). They go to `spells/_shared/` and the table points at them; they are not copied into
  every spell folder. Pictures that belong to no spell stay in their `.lbx` folder (units,
  screens). The 12 spell book pictures now directly in `spells/` move to `spells/_book/`.
- **Extractor and migration.** `f4x-tools` (`art.rs`, `extract-art`) writes the new layout and
  `manifest.tsv` gets the new paths, so a fresh extraction is right from the start. For art
  already extracted, a move mode (or a script logged in `docs/scripts.md`, with `--help`) moves the
  existing files by the table, so the original files need not be read again.
- **Client.** `Art` (`art.rs`) loads a spell's pictures by spell id and role; `draw_pic` by
  `(lbx, entry, frame)` stays for everything else. The game still runs with no files (no art found,
  no change).
- **Docs.** `extract-art.md`, `local-assets.md`, `overland_spell_animations.md` and the on-hold tasks
  that name `specfx` and `cmbtfx` entries (285, 287, 306) say where the files are now.

*Needs:* nothing in code; it is placed last in phase C so that it does not collide with the spell
entity work in 318 to 325 (the folder name is the spell's definition id, which does not change).

### D. Hero

**326. Hero entity.** A `Hero` entity with `HeroState`, abilities, experience, casting skill and
spells, linked to its unit body while it serves (`Unit` and `Position` only then). Replaces the
`Hero` component copied from the roster record at hiring. *Needs:* 317.

**327. Hero slots, hiring, recall and resurrection over hero entities.** The six slots, hire, recall,
resurrect and `Offer::Hero` work on hero entities. `Roster`, `HeroRecord`, `Availability` and the
roster index in `Magic.raising` go away. `hero::game::remove_unit` stays the single exit path for a
hero's body. *Needs:* 326.

**328. Hero spells and the armory.** The hero's own spells (a hero's casting skill and its spell
list) cast as spell instances; the armory (vault slots and awards) moves from `Player` to the player
entity's components. The items themselves stay plain structs until 333. *Needs:* 327 and 318.

### E. Terrain feature

Today `Game.map` (outside the world) holds a `FeatureId` per tile, `Map.nodes`, `Map.lairs`,
`tower_owners` and an owner byte per volcano. Nodes and lairs are found by `Vec` index
(`Entry::Site(usize)`, `lair_at`, `node_at`), which breaks when one is removed.

**329. Terrain feature entity.** The `TerrainFeature(FeatureId)` entity with `Position` and `Serial`,
and a position index (square to entity) in place of `Grid.features`. `feature::place`, `feature::node::generate_nodes`
and `feature::lair::generate_lairs` become one placement step: the map generator returns the terrain layers
and a list of placements, and the game spawns the entities. Minerals and encounter markers are features with no further component. A tower is one entity
on both planes. The move of `Node` and `Lair` out of `map.rs` (309) is already done. *Needs:* 313.

**330. Node entity.** The `Node` component (realm, power, aura, guardian, `Owner(Entity)`) on the
feature entity; `Map.nodes`, `node_at`, `node_covering`, melding, node income, corruption of nodes and
the dispel and Armageddon paths use it. A warped node is a spell instance targeting the feature
(323). *Needs:* 329, 323.

**331. Lair and site entity.** The `Site` component (intact, guards, gold, mana, loot values, prisoner,
special) on the feature entity; `Map.lairs` and `Entry::Site(usize)` go, entry and clearing address
the entity. Check first whether the twin lairs of a tower differ between the planes; if they do,
`Site` keeps a guard list per plane. Guards are still spawned as neutral units at entry and
despawned after the fight (no change in the rules). Decide where the loot items are made: at map
creation (item entities held as `Loot` by the feature) or at clearing, as now; the draw order no
longer matters. *Needs:* 329.

**332. Volcano and tower ownership.** `Grid.volcanoes` (owner byte) and `tower_owners` become
`Volcano` and `Owner(Entity)` on feature entities. `terraform` spawns a volcano feature when it makes
one and despawns it when it cools. Mana per turn, the tower claim (`claim_tower`) and `map_draw` in
the client read the entities. *Needs:* 329 and 313.

**338. Node base terrains.** In the original a node is two things: the base terrain of its square
(`tt_SorceryNode`, `tt_NatureNode`, `tt_ChaosNode`, terrain types 168 to 170) and the node itself
(`_NODES`). Here the node is a feature on an ordinary terrain, so a node square counts as grassland,
hills or whatever it was generated on. Add three terrain definitions to `content/terrain.ron`
(`sorcery_node`, `nature_node`, `chaos_node`, on both planes) and put the matching base terrain under
every node feature when the placement step (329) spawns it, as `MAPGEN.c` does. Only the node's own
square changes; its aura squares keep their terrain. Values from ReMoM (`MoM/src/Terrain.c`,
`Square_Food_x2` and `Square_Production_Bonus`; half-food units as in `terrain.ron`):

| Base terrain | Food | Production | Like |
|---|---|---|---|
| Sorcery node | 4 | 0 | none |
| Nature node | 5 (the MGC build has 4: a recorded original bug, `OGBUG` in ReMoM; pick one and write it in `divergences.md`) | 3 | forest |
| Chaos node | 0 | 5 | mountains |

Still to read from the original: the movement cost of the three (the `TERRSTAT.LBX` records 168 to
170; `f4x-tools` has the LBX reader), whether a city may be founded on them (the original has
`Square_Is_Node`; check its use), the wild-game and mineral rules, and the battlefield each node type
gives (`Combat.c`, around line 15627). The terrain classes in `terrain.ron` gain three node classes,
so `f4x-tools validate` and the map generator's class check (at least one definition per class and
plane) are updated; the client needs a tile for each (the original's TERRAIN tiles, not the node
overlay). **This task changes behaviour on purpose:** cities and units near a node see new food,
production and movement values, so the tests that pin them change with it, and the learned networks
in `models-local/` and the mirror tests (`cargo test -p f4x-learn --test mirror -- --ignored`) are
checked afterwards and retrained if they drift. Update `map-layers.md` and `divergences.md`.
*Needs:* 329 and 330.

### F. Item

**333. Item entity.** One entity per item instance with `ItemKind`, `Name`, `Bonus`, `Powers`,
`Charges`, `Prefab` and `HeldBy` (see `ecs-structure.md` section 3.7). Replaces the `Item` struct that is copied around today
(`from_def`). Lifecycle: spawned when made (`make_item`, `random_item`, crafting, offer, loot); an
item with no holder is despawned. `ItemSpot` and the `MoveItem` and `ScrapItem` commands address
holders. `Game.prefab_taken` stays as a registry. `Game.item_awards` (names only) becomes items
held as `Award`. *Needs:* 313, 315 and 326.

**334. Items over the holders.** Convert every holder: `HeroItems` (hero slots), the armory vault and
awards, `Magic.crafting`, `Offer::Item`, the spoils of a battle (an item of a slain hero passes to the
heir or winner by changing `HeldBy`) and lair loot (`Loot.items`, `claim_lair`). Item charges
written back after a battle are a component change. *Needs:* 333, 322, 327, 328 and 331.

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
