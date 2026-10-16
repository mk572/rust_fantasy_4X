# Spells still to implement

The one list of spells that are imported but still carry the `unimplemented` placeholder
(`content/spells.ron`; casting one says "no effect yet"). Other tasks point here instead of keeping
their own lists.

**None left** (checked 17 October 2026: `content/spells.ron` has no `unimplemented` spell). If a new
one is imported, list it here with the original's spell number (its index in the game's spell file,
which is also the help topic and the `spl_*` constant in ReMoM, `Spellbook.h`) and what it needs
first, and delete the row when it is wired. Where help and code disagree for a spell, add a row to
`help-vs-implementation-conflicts.md`.

## Where the last ten went

| Spell | Where | Notes |
|---|---|---|
| Life Force, Tranquility | `dispel.rs` (`suppressed_by`) | Other wizards' overland death (chaos) spells resist a strength 500 dispel, like Suppress Magic. |
| Eternal Night | `battle/game.rs` | Every battle starts under Darkness unless the city has Heavenly Light. |
| Zombie Mastery | `battle/game.rs` (`is_raised_by`) | The winner's normal dead, either side, rise as undead. |
| Wind Mastery | `units.rs` (`unit_max_moves`) | Our own rule, `divergences.md` 16 and conflicts B37. |
| Meteor Storm, Great Wasting, Armageddon, Call the Void, Nature's Wrath | `world_spells.rs` | Divergences 17, conflicts B38 to B41. |
| Chaos Channels, Doom Mastery | `mutation.rs` | Conflicts B42. |
| Stasis | `stasis.rs` | Conflicts B43. |
| Time Stop | `globals.rs`, `turn.rs`, `magic_game.rs`, `lib.rs` | Divergences 18. |
| Magic Vortex | `battle/vortex.rs` | Divergences 19. |

Leftovers of these spells are in task 228 (the AI casting them), task 285 (the interface) and
task 286 (Time Stop's remaining parts).
