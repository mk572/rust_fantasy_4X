# Spells still to implement

The one list of spells that are imported but still carry the `unimplemented` placeholder
(`content/spells.ron`; casting one says "no effect yet"). Other tasks point here instead of keeping
their own lists.

- **No.** is the original's spell number: its index in the game's spell file, which is also the
  number of the spell's help topic and the `spl_*` constant in ReMoM (`Spellbook.h`).
- **Needs first** names what has to exist before the spell can work. A spell with a heroes or items
  prerequisite waits for those tasks (48, 50, 218 onward).
- 35 spells are listed. The count should drop as they are done: when a spell is wired, delete its
  row, and keep `docs/TASKS.md` and the importer (`scripts/import_mom.py`) in step.
- Where help and code disagree for a spell, add a row to
  `help-vs-implementation-conflicts.md` when it is wired.

The spells wired so far are not listed: see tasks 25, 56 and 58 in `TASKS.md` and `wizard_spells.rs`,
`special_spells.rs`, `city_spells.rs`.

## A. Heroes and items (not implemented)

| No. | Spell | Realm | Needs first |
|---|---|---|---|
| 133 | Resurrection | life | Heroes that can die and be kept as dead (task 219), a summoning circle with room |
| 142 | Incarnation | life | The hero roster and Torin as a hero (tasks 218, 219) |
| 205 | Recall Hero | arcane | Heroes (task 219), taking a unit out of a fight |
| 208 | Summon Hero | arcane | Hero data and roster (tasks 218, 219), six-hero limit |
| 212 | Summon Champion | arcane | The same as Summon Hero, with the greater heroes |
| 207 | Enchant Item | arcane | Items and the vault (tasks 49, 50, 218) |
| 211 | Create Artifact | arcane | The same as Enchant Item, with all powers |

## B. Unit enchantments the combat code must read

None left: Immolation, Cloak of Fear, True Sight, Eldritch Weapon, Spell Lock, Elemental Armor, Bless, Righteousness and Invulnerability are all wired (tasks 156 and 157).

## C. Battle spells that need a per-unit state or a changed unit

| No. | Spell | Realm | Needs first |
|---|---|---|---|
| 5 | Web | nature | A web with its own hit points on a unit: no moving, no flying; rules to break it (task 159) |
| 46 | Confusion | sorcery | Random behaviour each turn for a unit (do nothing, wander, hit friends, hit foes) |
| 165 | Black Sleep | death | A unit that cannot act and takes maximum damage |
| 179 | Berserk | death | Doubled melee and defence 0 for a unit, for the battle |
| 73 | Creature Binding | sorcery | A unit that changes sides for one battle and goes home after; the same for Possession |
| 172 | Possession | death | The same side change for a normal unit; heroes and fantastic creatures are refused |
| 81 | Warp Wood | chaos | Ammunition for a unit's ranged attack to be removable |
| 88 | Shatter | chaos | A resistance roll, then a lasting cut of the unit's attacks to one |
| 89 | Warp Creature | chaos | A permanent per-figure change (half attack, half defence or no ranged attack) |
| 137 | Raise Dead | life | The dead of a battle kept per unit so one can be brought back at half strength; not fantastic creatures |

## D. Battlefield spells

| No. | Spell | Realm | Needs first |
|---|---|---|---|
| 1 | Earth to Mud | nature | Mud on a five by five area of the battlefield that costs all movement; flyers and bodiless units pass |
| 11 | Cracks Call | nature | A fissure on the field: 25 percent death to units over it that walk, and breaking stone walls (tasks 158, 163); items of dead heroes (task 221) |
| 82 | Disrupt | chaos | Wall sections that can be broken one at a time (tasks 158, 163) |
| 105 | Magic Vortex | chaos | A moving battlefield object with controlled moves, damage to units in its way and to a city it crosses (task 158 item 5) |

## E. Mutations

| No. | Spell | Realm | Needs first |
|---|---|---|---|
| 93 | Chaos Channels | chaos | A per-unit mutation record (armour, wings or fire breath) the unit stats read, never given twice; Lycanthropy already refuses mutated units once it exists |
| 117 | Doom Mastery | chaos | The same mutation, applied to every new normal unit of the wizard (global enchantment) |

## F. Global enchantments

| No. | Spell | Realm | Needs first |
|---|---|---|---|
| 57 | Wind Mastery | sorcery | Movement rules that depend on the owner: double for own ships and sky units, half for everyone else's |
| 80 | Time Stop | sorcery | Turn flow where only the caster moves and pays upkeep (and a combat part) |
| 156 | Life Force | life | A spell-resistance check for other wizards' non-combat death spells, like Suppress Magic's (`suppressed_by`) |
| 157 | Tranquility | life | The same for chaos spells |
| 113 | Meteor Storm | chaos | A strike each turn on units outside cities (strength 4 fire) and a 1 percent chance for each building elsewhere; the caster's cities are safe |
| 114 | Great Wasting | chaos | Corrupting several squares each turn, away from the caster's cities (task 123) |
| 120 | Armageddon | chaos | A volcano each turn with the caster gaining power from each (task 123) |
| 119 | Call the Void | chaos | An instant spell: 50 percent of a city's buildings, 10 damage to each unit inside, corruption around (tasks 123, 158) |
| 40 | Nature's Wrath | nature | Triggering on every non-combat death or chaos spell the caster casts, then shaking the caster's cities: units that do not fly or lack a body, and buildings. Check help against code when wired |
| 188 | Zombie Mastery | death | Normal creatures that die in a won battle become the caster's zombies afterwards (the dead of a battle, as for Raise Dead) |
| 197 | Eternal Night | death | Every battle treated as under Darkness, except in cities with Heavenly Light |

## G. Overland instants needing new state

| No. | Spell | Realm | Needs first |
|---|---|---|---|
| 68 | Stasis | sorcery | Overland paralysis of every enemy unit on a square: a unit that cannot move or fight for a turn, freeing itself by a resistance roll at -5 |

## Order that gives the most spells for the least work

1. (Done: Aura of Majesty and Just Cause, small hooks onto parts that already existed.)
2. (Done: the realm-aware defence bonus of task 156, with Elemental Armor, Bless, Righteousness and Invulnerability.)
3. A per-unit battle state (section C): about ten spells share it.
4. Corruption and volcano turns (task 123): Great Wasting, Armageddon, Call the Void.
5. Heroes and items (section A) come with their own tasks.
