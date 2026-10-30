# Larger unit stacks: design (for later)

Status: **partly done by `PLANNED.md` item 34 (30 October 2026: up to 20 units on a square, an army of nine, the `max_tile` setting); this doc still describes the larger battle side.** Was: design only, not scheduled. This is stage 4 of [`game-settings.md`](game-settings.md):
the `max_stack` setting goes up to nine in stage 3 (lowering it is easy) and above nine only when
this work is done. Raising the number is cheap; the cost is everything around it that was built
for nine. This doc lists those places so the work can be planned, tested and done in one go.

What is checked and what is not is marked. Where a line says "to check", I have not looked.

## 1. What the limit is today

`unit::MAX_STACK = 9` (`crates/f4x-core/src/unit.rs`). It is read in:

| Place | What it limits |
|---|---|
| `unit/movement.rs` (two checks), `city/gate.rs` (two), `spell/special.rs` (three) | moving, gating and teleporting a stack onto a square |
| `unit/transport.rs` | to check: boarding and landing |
| `city/neutral.rs` | the garrison of a neutral city (`population / 4`, at most nine) |
| `feature/lair.rs` | the defenders of a lair |
| `battle/game.rs` | how many raised dead a winner can keep (`room = MAX_STACK - standing units`) |
| `battle/casting.rs` | `MAX_SIDE_UNITS = 9`, a **second copy** of the same number: summons, Possession, Creature Binding, Raise Dead, Animate Dead |
| `f4x-ai` `military.rs`, `naval.rs` | gathering in chunks of nine, boat loads of two to five |

## 2. What is built for nine

Checked:

- **Battle deployment** (`battle/map.rs::deployment`): three columns of nine squares each per side,
  so up to **27** units fit, with the index arithmetic `i / 9 % 3` and `i % 9`. A side above 27
  would wrap onto squares already used. A **city defence** layout (`city_deployment` in the same
  file) has twelve slots (three columns of four) and is cut with `truncate(count)`, so a defender
  stack larger than twelve in a city loses units from the battle's start layout; the real number
  that is placed today in a city fight is to check.
- **Client city screen** (`city_screen.rs`): the unit list takes nine (`.take(9)`); task 119 already
  lists scrolling past nine as missing.

To check (not looked at):

- the main screen's stack panel (units of the selected square) and how many it can draw;
- the battle screen's unit rows, the start-of-battle layout on screen and the unit info panel;
- how a transport's capacity and a tile's stack interact (task 236);
- the AI's evaluation: does a stack of 20 change its chunking, its battle odds tables and the
  learning AI's inputs (`f4x-learn`'s observation has a fixed size per side? to check);
- the save: stacks are not saved as lists, so probably no change.

## 3. Design

### 3.1 One number

`max_stack` lives in `GameSettings::rules` (see the settings doc) and is read through
`Game::max_stack()`. `MAX_STACK` and `MAX_SIDE_UNITS` both go away; the battle's per-side limit is
the same number. A test fails the build if either constant is added back (a `grep` in the test).

### 3.2 A hard ceiling

The setting is limited to a value the code is **tested** at. Proposal: 9 (today), then 12, 15, 20
and 27 are tested steps; the screen offers the stepper only up to the highest tested one. 27 is the
battle deployment's own bound; more would need a fourth column.

### 3.3 Battle

- **Deployment** uses `ceil(max_stack / 9)` columns, or rows of a size chosen for the field, so a
  stack of `max_stack` always has squares; the deployment test checks no overlaps and no walls
  at each tested limit.
- **City defence** squares are generated, not listed, for larger counts; the walled-city corner
  rule (no unit on a wall corner) stays.
- Both sides' totals count: a summon, a Possession or a raise is refused when the side is full.
  The summon and Raise Dead squares ("a free summon square") need room that exists at 27 units a
  side on the field.
- Battle time: a side of 27 acts three times as often per turn; to check the turn limit (100
  turns) and the AI's per-turn cost.

### 3.4 The client

- A scrolling or two-row stack panel on the main screen; unit selection by click and by key must
  still cover every unit.
- The battle screen's unit list and the order the next-unit key walks.
- The city screen's unit list (task 119's scroll).
- Screenshots at each tested limit, taken by the client's own screenshot path, checked by eye.

### 3.5 The AI

- `military.rs` gathers in chunks of `max_stack`; `naval.rs` loads boats up to their capacity
  (unchanged, the galley's five).
- Odds and evaluation tables that assume nine are to be found (grep for `9`, `[_; 9]`, `0..9`)
  and replaced by the setting.
- The learning AI (`f4x-learn`) has fixed input sizes; a higher limit needs either a retrain or
  the limit kept at nine for the learned agent, and the checkpoint must record the limit it was
  trained at. To check how its inputs are laid out.
- No soak is run unless asked; a soak at the highest tested limit is part of the sign-off, when
  the user asks for it.

### 3.6 Rules that interact

- **Transport:** a galley carries five, so a stack on a ship is not the stack's limit; task 236
  says how passengers count against a tile. The setting does not change a ship's capacity.
- **Heroes:** no limit on how many a wizard serves (`divergences.md`); unchanged.
- **Neutral and lair garrisons:** capped at `max_stack`; a larger limit does not make them
  larger by itself. Whether a neutral city's `population / 4` should scale is a balance question.
- **Balance:** a bigger stack makes area spells and the Fireball-like attacks stronger relative to
  a single hit, and a fortified city's defenders more numerous. A note for whoever sets a preset;
  there is no change to the unit or spell data.

## 4. Tests

- Movement, gate, teleport and transport at limits 1, 5, 9 and each raised step: the move that
  would exceed the limit is refused with the same error and the one that fits is not.
- Battle deployment: no two units on one square and none on a wall corner at every tested limit,
  for both sides, in the open, in a city and with walls.
- A full side refuses a summon, a Possession, a Creature Binding and a Raise Dead.
- The AI gathers the right number of units per stack at 5 and at a raised limit.
- A game saved at one limit and loaded: the saved limit is used and a stack above a hand-edited
  lower limit is left alone (nothing is dropped; the limit applies to new moves).
- The screenshots of 3.4.

## 5. Order of work

1. Replace both constants with `Game::max_stack()` at limits up to nine (stage 3 of the settings).
2. Make battle deployment and the city layout general, with the overlap tests.
3. The client panels, with screenshots.
4. The AI's tables, then the learning AI's inputs, then the tested limits are raised one at a time.
5. Raise the settings screen's range to the highest limit that passes.

## 6. Open questions

1. The highest limit worth building: 12, 15, 20 or 27?
2. Whether a neutral city's garrison and a lair's defenders should grow with the limit.
3. Whether the learning AI must be retrained for each limit or is simply kept at nine.
