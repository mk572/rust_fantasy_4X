# Content and modding guide

Everything that defines the game's content (units, buildings, races, spells, terrain, graphics) lives in plain-text files you can open in any editor. No code changes or recompiling are needed to change values, add content or restyle the game.

Check your changes at any time with:

```
cargo run -p f4x-tools -- validate content/base path/to/your/mod
cargo run -p f4x-tools -- list units content/base path/to/your/mod
```

The validator reports every problem it finds, in plain language (unknown ids, missing requirements, impossible values), before the game ever loads your content.

## Packs

Content comes in **packs**. A pack is a folder with a `pack.ron` manifest and any of the data files below; all files are optional except the manifest. The game's own content is the `base` pack (`content/base/`). A mod is another pack that loads after it.

```
content/base/            the built-in game
  pack.ron  terrain.ron  features.ron  rules.ron  realms.ron  races.ron
  units.ron  abilities.ron  buildings.ron  spells.ron  items.ron
  level_paths.ron  art.ron  art/...(image files)
mymod/
  pack.ron  units.ron  art.ron  art/units/pikeman.png
```

**Load order and overriding.** Packs load in the order given; later packs win:
- An entry with the **same id** as an earlier one **replaces it** (it keeps its position in the list).
- An entry with a **new id** is **added**.
- Entries can be **removed** with `remove` in `pack.ron`.
- `rules.ron` is replaced as a whole block (anything you leave out goes back to its default).

Because a unit names its own race, and a building names which races may build it, adding a unit or building is a **single entry in a single file**: nothing else needs to list it.

### `pack.ron`
```
(
    id: "mymod",                     // lowercase letters, digits, underscore; unique
    name: "My Mod",
    version: "1.0.0",
    author: "you",
    description: "Adds pikemen and removes cave trolls.",
    depends: ["base"],               // packs that must load before this one
    remove: ["unit:cave_troll", "art:unit:cave_troll"],   // "<kind>:<id>"
)
```
Removable kinds: `terrain`, `feature`, `race`, `unit`, `ability`, `building`, `realm`, `spell`, `item`, `level_path`, `art`. Removing something that is still referenced elsewhere is reported as an error, so you cannot leave dangling references.

## Data files

Files use [RON](https://github.com/ron-rs/ron): like JSON but with comments (`//`), trailing commas, and named fields. A file is a list `[ ... ]` of entries. Every entry has a unique `id` (lowercase `a-z 0-9 _`) and a display `name`.

### `units.ron`
```
(id: "spearman", name: "Spearman", kind: Normal, race: Some("sunfolk"),
 cost: 30, upkeep: (gold: 1),
 stats: {Melee: 3, Defense: 3, Resistance: 4, MaxHp: 1, Figures: 6, Moves: 1, Sight: 1},
 movement: Walking,
 abilities: ["first_strike"],
 requires: ["barracks"],            // buildings the city needs
 level_path: Some("standard_xp"))
```
- `kind`: `Normal` (built by a city of its `race`), `Hero` (needs `hero: Some((slots: [Weapon, Armor, Misc], hire_cost: 150))`, no race), `Summoned` (needs a `realm`, `cost` is mana), `Fantastic` (neutral creature, no race).
- `stats`: required are `Melee`, `Defense`, `Resistance`, `MaxHp`, `Figures`, `Moves`. Optional: `Ranged` (then `Ammo` is required), `Sight`, `ToHit`. Values are small numbers (0 to 99). `Figures x MaxHp` is the unit's total hit points.
- `movement`: `Walking`, `Flying`, `Swimming`, `Sailing`.
- The list of available stats is fixed by the game (`Stat` in `crates/f4x-core/src/rules/stats.rs`).

### `buildings.ron`
```
(id: "armoury", name: "Armoury", cost: 80, upkeep_gold: 1,
 requires: ["barracks"],
 races: [],                         // empty = every race; otherwise only these
 effects: [(target: City, effect: Modify(stat: UnitCost, op: Percent, value: -10))])
```

### `races.ron`
```
(id: "sunfolk", name: "Sunfolk", home_terrain: [Grassland, Forest],
 effects: [(target: City, effect: Modify(stat: CityGrowthPercent, op: Add, value: 10))])
```

### `abilities.ron`, `items.ron`, `spells.ron`, `level_paths.ron`, `realms.ron`
See the files in `content/base/` for working examples of each. Abilities without `effects` are flags the combat and movement rules look for by id (for example `flying`).

### Effects
Abilities, buildings, races, spells, items and experience levels all describe what they do with the same small vocabulary:

```
(when: Passive,        // Passive | OnTurnStart | OnCast | OnAttack | OnDefend | OnDeath | OnBuilt
 condition: Always,    // Always | HasAbility("x") | TerrainIs(Hills) | OnPlane(Myrror) | InBattle | All([..]) | Not(..) ...
 target: This,         // This | Owner | City | Chosen | AllOwnUnits | AllEnemyUnits | UnitsInTile | UnitsInArea(2) ...
 effect: Modify(stat: Melee, op: Add, value: 1))
```
`when`, `condition` and `target` can be left out (defaults: `Passive`, `Always`, `This`). Effects: `Modify`, `GrantAbility`, `RemoveAbility`, `Damage`, `Heal`, `AddResource`, `Summon`, `ChangeTerrain`, `SetFeature`, `ApplyEnchantment`, `Dispel`. `op` is `Add`, `Percent` (adds to a percentage), `Min`, `Max` or `Set`. See `docs/rules-framework.md` for the exact rules of how stats are calculated.

Today the always-on `Modify` effects take part in calculating unit and city stats. Triggered effects (`OnCast`, `OnTurnStart`, `OnAttack`, ...) are accepted and validated now and run once spells and combat exist.

### `terrain.ron`, `features.ron`
Terrain definitions (movement cost, food, whether cities can be founded) and the map features (minerals, encounter sites, towers, magic nodes) with their spawn rules per terrain class and plane. Myrror-only content is simply a spawn rule listing `planes: [Myrror]`.

### `rules.ron`
Tunable game numbers: to-hit and block chances, minimum distance between cities, base city production, stack limit. Comments next to each value say what it does.

## Graphics (`art.ron`)

Graphics are kept apart from the rules so you can restyle the game without touching any gameplay file. Each entry connects a definition to its images, keyed `"<kind>:<id>"`:

```
(key: "unit:spearman",
 sprite: Some((file: "units/spearman.png", frames: 4, frame_size: Some((32, 32)), fps: 6)),
 icon:   Some((file: "icons/spearman.png")),
 color:  Some((200, 190, 120)))
```
- Kinds: `terrain`, `feature`, `race`, `unit`, `ability`, `building`, `realm`, `spell`, `item`.
- `file` is relative to the pack's `art/` folder. It cannot point outside it (no `..`, no absolute paths).
- Animation frames are laid out left to right in one strip; `frame_size` is the pixel size of one frame, `fps` is the speed (0 = static).
- `color` is drawn when the image is missing, so a pack works without any images. A definition with no art entry at all gets a default placeholder.
- A missing image file is a warning, not an error: `validate` lists them so you can see what is left to draw.

## Tips
- Start by copying an existing entry and changing it; the validator will point out anything you break.
- Keep your own changes in your own pack rather than editing `content/base/`, so game updates do not overwrite them.
- Numbers are intentionally small (a basic soldier has Melee 3). A change of 1 is significant.
