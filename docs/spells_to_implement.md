# Spells still to implement

The one list of spells that are imported but still carry the `unimplemented` placeholder
(`content/spells.ron`; casting one says "no effect yet"). Other tasks point here instead of keeping
their own lists.

- **No.** is the original's spell number: its index in the game's spell file, which is also the
  number of the spell's help topic and the `spl_*` constant in ReMoM (`Spellbook.h`).
- **Needs first** names what has to exist before the spell can work. A spell with a heroes or items
  prerequisite waits for those tasks (48, 50, 218 onward).
- 10 spells are listed, the ones `content/spells.ron` still marks `unimplemented` (checked 16 October
  2026). The count should drop as they are done: when a spell is wired, delete its
  row, and keep `docs/TASKS.md` in step. Spells are edited directly in `content/spells.ron` (the
  importer no longer writes it).
- Where help and code disagree for a spell, add a row to
  `help-vs-implementation-conflicts.md` when it is wired.

The spells wired so far are not listed: see tasks 25, 56 and 58 in `TASKS.md` and `wizard_spells.rs`,
`special_spells.rs`, `city_spells.rs`.

## A. Heroes and items

None left: Resurrection, Incarnation, Recall Hero, Summon Hero and Summon Champion are wired
(`hero_spells.rs`, `battle/spells.rs`, tasks 218 and 219), and Enchant Item and Create Artifact cast
through `Command::CraftItem` (task 50). What is still missing for those two is the client's design
screen, which is task 260, not a spell.

## B. Unit enchantments the combat code must read

None left: Immolation, Cloak of Fear, True Sight, Eldritch Weapon, Spell Lock, Elemental Armor, Bless, Righteousness and Invulnerability are all wired (tasks 156 and 157).

## C. Battle spells that need a per-unit state or a changed unit

| No. | Spell | Realm | Needs first |
|---|---|---|---|

## D. Battlefield spells

| No. | Spell | Realm | Needs first |
|---|---|---|---|
| 105 | Magic Vortex | chaos | A moving battlefield object with controlled moves, damage to units in its way and to a city it crosses (task 158 item 5) |

## E. Mutations

| No. | Spell | Realm | Needs first |
|---|---|---|---|
| 93 | Chaos Channels | chaos | A per-unit mutation record (armour, wings or fire breath) the unit stats read, never given twice; Lycanthropy already refuses mutated units once it exists |
| 117 | Doom Mastery | chaos | The same mutation, applied to every new normal unit of the wizard (global enchantment) |

## F. Global enchantments

| No. | Spell | Realm | Needs first |
|---|---|---|---|
| 80 | Time Stop | sorcery | Turn flow where only the caster moves and pays upkeep (and a combat part) |
| 113 | Meteor Storm | chaos | A strike each turn on units outside cities (strength 4 fire) and a 1 percent chance for each building elsewhere; the caster's cities are safe |
| 114 | Great Wasting | chaos | Corrupting several squares each turn, away from the caster's cities (task 123) |
| 120 | Armageddon | chaos | A volcano each turn with the caster gaining power from each (task 123) |
| 119 | Call the Void | chaos | An instant spell: 50 percent of a city's buildings, 10 damage to each unit inside, corruption around (tasks 123, 158) |
| 40 | Nature's Wrath | nature | Triggering on every non-combat death or chaos spell the caster casts, then shaking the caster's cities: units that do not fly or lack a body, and buildings. Check help against code when wired |

## G. Overland instants needing new state

| No. | Spell | Realm | Needs first |
|---|---|---|---|
| 68 | Stasis | sorcery | Overland paralysis of every enemy unit on a square: a unit that cannot move or fight for a turn, freeing itself by a resistance roll at -5 |

## Order that gives the most spells for the least work

1. (Done: Aura of Majesty and Just Cause, small hooks onto parts that already existed.)
2. (Done: the realm-aware defence bonus of task 156, with Elemental Armor, Bless, Righteousness and Invulnerability.)
3. A per-unit battle state (section C): about ten spells share it.
4. Corruption and volcano turns (task 123): Great Wasting, Armageddon, Call the Void.
5. (Done: the heroes and items spells of section A.)
