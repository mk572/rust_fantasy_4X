# Lairs (guarded sites)

Source: ReMoM `Create_Lair`, `Generate_Lairs`, `Lair_Make_Guardians`, `Lair.c`. Code: `crates/f4x-core/src/lairs.rs`.

- Every tower, node and encounter site gets a `Lair` (guards + hoard) at map generation; towers share one on both planes.
- Guards are neutral units made on entry (`Command::EnterSite`), fought with the battle model, then removed. Winning takes the hoard and clears the site (encounter sites vanish; towers and nodes stay).
- Moving onto a guarded site gives `Event::SiteEncounter` and nothing moves; an unguarded site pays out on arrival.

Differences from the original (gap tasks in TASKS.md):
- Strong/weak split of encounter sites is random (25:32), not fixed counts; site types follow the data, not a random 7-way roll.
- The lair-race switch fall-through (ReMoM-marked bug) is fixed: death 2, chaos 2, nature 1 in five.
- Spell rewards and book/retort rewards follow the intent, not the original's muddled index arithmetic.
- Items and prisoner heroes are recorded in `Loot` only (need heroes/items tasks 48-49).
- No client prompt yet (task 128); the battle screen exists.
