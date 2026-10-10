# Finished tasks

Moved here from [`TASKS.md`](TASKS.md) by `scripts/archive_done_tasks.py`. Numbers are the
original task numbers. Newest work is merged in ascending order by number.

## Table

| # | Area | Summary |
|---|---|---|
| 1 | Map | ~~Rivers~~ (done) |
| 3 | Map | ~~Volcanoes~~ (done) |
| 4 | Map | ~~Shore and coastline variants for drawing~~ (done) |
| 15 | City | ~~Fortress and summoning circle, banishment~~ (done) |
| 52 | Magic | ~~Timed spell durations~~ (done) |
| 54 | Magic | ~~Wizard creation screen and named wizards~~ (done) |
| 57 | Magic | ~~Global enchantments and wizard-level state~~ (done) |

## Descriptions

**1. Rivers (done).** A river flag per tile in `Grid` (saved as `PlaneSave::rivers`), generated in
`mapgen::place_rivers` (from high ground downhill to the sea or another river, never on hills,
mountains or ocean). Effect as in the original (`Square_Gold_Bonus`): a city on a river gets +20
percent trade, on a river next to the sea (river mouth) +30, shore alone +10; capped with roads
and race at three times the citizens. The client draws blue strokes. The original also lets
rivers change movement art only, so no movement rule. Lakes are task 2.

**3. Volcanoes (done).** `terraform.rs`: a volcano is a mountain tile with an owner (`Grid`
volcano layer, saved). Raise Volcano (hook `raise_volcano`, map square target) turns a non-hill,
non-mountain, non-site land tile into a volcano, destroys its minerals, gives the caster +1 power
per volcano (`power_base`), and collapses each building of a city on it with 15 percent. Change
Terrain (hook `change_terrain`) follows the help table: desert, forest, hills, swamp to grassland,
grassland to forest, mountains to hills, volcano to plain mountains. Not done: Armageddon's
volcano rain (needs timed global spells, 57) and Gaia's Blessing turning volcanoes into hills (24).

**4. Shore and coastline variants (done).** `f4x-tools extract-art` now also writes
`assets-local/art/terrain/autotile.tsv` from TERRTYPE.LBX (five 256-entry tables keyed by the
8-neighbour mask); the client picks the ocean tile for each water square from the land
neighbours. Only table 0 (ocean shores) is used; the tables for mountains, hills and tundra
edges are extracted but not yet drawn.

**15. Fortress and summoning circle (done).** Implemented in `fortress.rs`; see `magic.md`
section 12. What is left is split off as task 82.

**52. Timed spell durations (done).** `timers.rs`: a spell with `Duration::Turns(n)` gets a timer
(`Game::timers`, saved by creation number) when attached to a unit, a city or the wizard; each
turn's magic step counts it down and removes the enchantment with `Event::EnchantmentExpired`.
Timers whose enchantment was dispelled or whose bearer died are dropped silently. No imported
spell uses `Turns` (the original has none), so this is for mods and the community patch.

**54. Wizard creation screen and named wizards (done).** `wizard.rs` holds the pick rules of the
original's custom-wizard screen (eleven picks; books one each; retorts one, two for Warlord,
Channeler, Divine Power, Famous and Infernal Power, three for Myrran; at most six retorts;
prerequisites such as four books for a mastery, Archmage or Mana Focusing, two realms for Sage
Master, two books in three realms for Runemaster) and `Game::create_wizard`, which also takes
the starting spells: a realm with `n` books lets the wizard choose `n - 1` common spells, filled
from the realm's default list (`RealmDef::default_spells`). The 14 named wizards are data
(`wizards.ron`, written by the importer; Lo Pan is over budget in the original and is exempt).
The client's creation screen (`create_screen.rs`, shown at start unless `--quick`, `--load` or
`--shot`; `--create` forces it) offers the presets, a custom build, a name, starting spells and
a race. The rival gets another preset by seed. Not done: portraits and banner colours (83), the
Myrran start on Myrror and the race it controls (53), and eleven-book realms.

**57. Global enchantments and wizard-level state (done).** `globals.rs` reads the wizard's
global spells where they matter: Evil Omens (life and nature spells cost half as much again for
every wizard but the holder; one cost function, B11 decided as "follow the help"), Herb Mastery
(heals every unit each turn), Planar Seal (blocks every plane shift, towers and planar travel
included), Awareness (explores all city squares), Nature's Awareness (explores and shows the
whole map), Detect Magic (`detect_magic_view`, shown on the magic screen), and the stat
bonuses Holy Arms (+1 melee and ranged for normal units), Chaos Surge (+2 for chaos creatures)
and Charm of Life (a quarter of the hit points, at least one). Unit rules can now carry
conditions on a unit's kind (`Condition::KindIs`). The data marks these spells with the
`global_effect` hook so that casting does not report "no effect yet".
