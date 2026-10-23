# Architecture audit and plan: self-contained domains

Written after the question "`Game` will end up holding too much; what is the architecture and
how do we keep it from growing?". The state of `f4x-core` is in `ecs-structure.md`; this file
holds the audit (numbers measured on the code at the time) and the plan. The tasks are 177 to
183 in `TASKS.md`.

> **Status after the entity rework (tasks 307 to 337).** Section 1 is the audit as it was: its numbers
> and the names `Player::magic`, `game.players` and `Magic` as a field of a player describe the code
> before the rework. Players, wizards, heroes, spell instances, terrain features and items are now
> entities (`ecs-structure.md` section 3), and the rules in sections 2 to 4 still hold.

## 1. What the audit found

`Game` (`lib.rs`) is the one value every rule runs on: the `hecs` world, the map, the fog, the
content, the players, the RNG, the battle in progress, diplomacy, the undo stack and
more (about 20 fields). The rules are written as methods of it, in files that each add an
`impl Game` block.

| Measure | Value |
|---|---|
| Methods on `Game` | about 380, in 38 files |
| Files that never mention `Game` | 20: `rules/*`, `map`, `fog`, `player`, `content`, and the battle engine (`battle/attack`, `special`, `field`, `map`, `mod`, `ai`) |
| Largest groups | diplomacy 46, magic 36, `lib.rs` 18, units 17, production 17, battle spells 16, turn 15, city spells 14 |
| References from the other crates | `game.content` 106, `game.world` 15, `game.map` 15, `game.fog` 8, `game.players` 4, `.magic` 29, `.diplomacy` 4 |

The pure part (rules, map, battle engine) is already separate and works as the model for the
rest. The coupling is in the methods of `Game`: any of them can read or write any field, so the
module boundaries are only convention.

### Diplomacy (`diplomacy.rs`, 2,745 lines, 46 methods, 51 tests)

- Its state is already one struct, `Diplomacy` (pairs, stances, personalities, proposals, an
  event queue), held in `Game::diplomacy`. Its methods touch `self.diplomacy` 43 times and the
  rest of `Game` rarely.
- What it takes from outside is small and can be named: **the RNG** (`roll`, 23 calls), **the
  players** (`player`, `player_mut`: 5), **units and cities** for contact and trespass
  (`all_units`, `unit_owner`, `unit_position`, `unit_visible_to`, `visibility`, `cities`,
  `player_units`: 11), the wizards' **retorts and spells** (`magic`, `magic_mut`,
  `refill_research`: 10), `has_global`, `living_wizards`, `human`, and the turn number.
- Who calls it: `battle/game.rs`, `endgame`, `fortress`, `cast`, `conquest`, `magic`, `save`
  and `lib.rs`: about a dozen call sites (not looked at one by one yet).
- Its tests build a whole `Game` (a map, three players, three wizards). Whether each needs more
  than the list above has not been checked; task 179 starts with that.

### Magic (`wizard/magic.rs`, 1,443 lines, 36 methods, 18 tests)

- Its state is `Player::magic` (`Magic`: books, retorts, skill points, known and researchable
  spells, research, the spell being cast, global enchantments), one per player.
- The *wizard-side* logic (casting skill, costs and their reductions, research, mana upkeep,
  power split) reads only the content and the players. Its outward calls are few: `economy`
  (power base), `nodes` (node income), `fortress`, `globals` (Evil Omens), `experience` and
  `temp_effects` (the turn's ticks), and the cast resolution below.
- The part that does reach everywhere is **resolving a spell** (`spell/cast.rs`, `spell/dispel.rs`,
  `spell/city.rs`, `terraform`, `corruption`, `battle/spell.rs`): changing units, cities and
  the map. That is a different thing from the wizard's own state and should be a separate layer:
  the spell *executor* (`rules/exec.rs` is already pure) and the glue that applies its outcomes.
- Who calls magic: `diplomacy` (10), `battle/spell` (8), `dispel` (8), `lair` (7), `lib.rs`
  (7), `globals`, `cast`, `cheat`, and a few more: mostly `magic(player)` reads.

## 2. Target

The base rule behind all of this is in `design.md` section 3: domain logic is written over the
pieces it needs, not over `&mut Game`.

Diplomacy and magic each become a **self-contained structure**: it owns its state, holds all its
rules as methods of itself, can be created and tested without a `Game`, and sees the rest of
the game only through a small, named interface. `Game` keeps its job as the owner and the
orchestrator: it routes a `Command` to the right domain, hands the domain what it needs, and
applies what the domain returns.

```
Game  ── owns ──>  Diplomacy      impl Diplomacy { fn propose(&mut self, ctx: &mut DiplomacyCtx, ...) -> Vec<Event> }
      ── owns ──>  wizard entities (the wizards' state, in the world)
      ── owns ──>  world, map, fog, content, rng ...   (shared data the domains read through a context)
```

How a domain reaches the rest:
1. **A context value, not `&mut Game`.** The caller builds it from disjoint borrows of `Game`
   (`DiplomacyCtx { view, rng, out }`), so a method on `Diplomacy` can borrow
   `&mut self` and the context at once. This needs the RNG to be its own struct (`GameRng`,
   which keeps the draw counter the save replays), so it can be borrowed on its own.
2. **A read-only view trait for the world.** `diplomacy` needs a few questions answered:
   which units a player has and where, whether a unit is visible to a wizard, a wizard's
   cities, whether a wizard is alive, which global spells a wizard has, a wizard's retorts.
   Those became one small trait (`WorldView`) with a fake for the tests; the rework (task 336)
   dropped both: the domain reads a `&Game` (questions in `game/view.rs`) and its tests build a
   real game. `PlayerView` is what the AI and the client use to look at the game (task 181).
3. **Plain data in, plain outcomes out** where a call needs more than reads: magic's turn gets
   the power base, the node income and the list of upkeep items as numbers, and returns events
   and "drop this upkeep item" outcomes that `Game` applies to the units and cities.

Rules that keep it from growing back:
- A new rule goes on the domain it belongs to; `Game` only gets orchestration.
- A domain's fields are private to its module, and its context is the only way in.
- A test (task 183) counts the `impl Game` methods per file and fails when a file goes above
  its recorded limit, so growth shows up in review.

## 3. Order of work

1. **Groundwork (177, 178):** the RNG as its own struct, and the `WorldView` trait with
   `Game`'s implementation. 177 and 178 are done (`game/rng.rs`; `game/view.rs`).
2. **Diplomacy (179, done):** move its 46 methods onto `Diplomacy` with a context; callers in other
   files go through `Game` wrappers that build the context. Its tests first ran on a fake view; they now
   run on a real game (task 336). The result is checked by the existing 51 tests
   passing unchanged in meaning, and by saves staying compatible (`DiplomacySave`). The way it
   was done, which magic can follow: the domain's state and the generator are taken out of `Game`
   for the call so the rules run over `&Game` as the view, and what they ask of the rest comes
   back as a list of outcomes.
3. **Magic (180, done):** split the wizard's logic (research, skill, costs, upkeep, power split) from
   spell resolution. `Magic` gets the methods; the resolution layer (`cast`, `dispel`,
   `city_spells`...) stays on `Game` and calls into `Magic` for the wizard-side questions.
   Magic did not need the take-out trick: the wizard is an entity in the world (`wizard/entity.rs`,
   task 315), so the rules take the wizard's components, the content and the generator as
   arguments.
4. **Read-only view for the AI and client (181, done: `game/access.rs`, `player/view.rs`)**, replacing direct use of `game.world`,
   `game.map` and `game.content` outside the core where a query would do.
5. **Docs and guard (182, 183):** bring `ecs-structure.md` up to date, write the rules above
   into it (182, done: sections 2 and "Domains" of `ecs-structure.md`), add the method-count test.

Each step must leave the tests passing and the saves readable; no step changes behaviour.
Units, economy, production and the turn loop (the next largest groups) are the next candidates
once these two are done, in that order, because each stands on the one before: tasks 185 to 188.
Then movement and transport, the lairs, conquest, the fortress and vision (189 to 193). The same
pattern applies.

## 4. Risks and things to watch

- **Borrow splitting** is the technical heart: the context has to be built from disjoint field
  borrows inside `Game`'s own methods. If a method needs `&mut self.rng` and `&self.world`
  together, that works only when they are separate fields, which is why the RNG moves first.
- **Callbacks from a domain into the game** (diplomacy asking `magic` for a retort, magic asking
  diplomacy about a cast on another wizard's city: `begin_cast_relations`) must become either
  view questions or returned outcomes, or they will recreate the coupling inside the context.
- **Event order** is observable (tests, replays, the learning AI's scenarios): keep it as it is.
- **Save format** is version 2 (task 335): one list for each entity kind (`docs/ecs-structure.md` 3.9).
- Do not start with a big move. Each domain moves one group of methods at a time, with the
  suite green after each.
