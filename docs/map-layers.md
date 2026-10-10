# The map: layers, coordinates, generation

How the world map is stored and generated. For where the map sits among the rest of the
game state, see [`ecs-structure.md`](ecs-structure.md); for what a city does with the land
it works, see the city economy in [`design.md`](design.md) §6.

Code: `crates/f4x-core/src/{map,content,mapgen,featuregen,fog,vision,city}.rs`.
Data: `content/terrain.ron`, `content/features.ron`.

## 1. Space

- The world is **two planes**, Arcanus and Myrror, of identical size. The default is 60 by
  40 tiles (`GenParams`); any size from 8 to 1024 per side is accepted.
- Coordinates are `Position { plane, x, y }`. `x` runs `0..width` and **wraps**: the world is
  a cylinder, so there is no east or west edge. `y` runs `0..height` and does **not** wrap:
  the top and bottom rows are the poles.
- Distances between tiles take the wrap into account (`Grid::chebyshev` for king-move
  distance, `Grid::distance_sq` for squared distance). Neighbours are the eight surrounding
  tiles. Anything that scans an area must wrap `x` and clip `y`.
- A plane is a `Grid`; the pair is a `Map`. Both planes share dimensions, so a tile
  coordinate means the same place on each (towers rely on this).

## 2. The layers

| Layer | Stored as | Holds | Who changes it |
|---|---|---|---|
| **Base terrain** | `Grid` tile array of `TerrainId` | one terrain per tile | map generation; terrain-changing spells (planned) |
| **Feature** | `Grid` array of `Option<FeatureId>` | at most one feature per tile | map generation; spells and encounters (planned) |
| **Road** | `Grid` array of `bool` | whether a road runs through the tile | nothing yet (engineers are planned) |
| **Cities** | entities with a `Position` | founded cities and outposts | `FoundCity`, `SettleOutpost` |
| **Units** | entities with a `Position` | everything that moves | movement commands |
| **Exploration** | `Fog`, one flag per tile per plane per player | what a player has seen | moving, founding, creating units |
| **Visibility** | computed on demand (`Game::visibility`) | what a player sees right now | derived from units and cities |

Terrain, features and roads are the **static map** (changes rarely); cities and units are
entities and appear on top of it; exploration is remembered; visibility is never stored.

## 3. Base terrain

Eight terrain **classes** are fixed in code: ocean, grassland, forest, hills, mountains,
swamp, desert, tundra. Map generation and rules work on classes. A **terrain definition**
(`TerrainDef`) is a data entry in a class, and content must define at least one per class and
per plane (checked by `f4x-tools validate` and again by the generator).

| Field | Meaning |
|---|---|
| `id`, `name` | stable id and display name |
| `class` | one of the eight classes |
| `walkable` | ground units can enter (false for ocean) |
| `can_found_city` | a city or outpost may be founded here |
| `move_cost` | movement cost in whole moves (0 for ocean); see the movement rules in `design.md` |
| `food` | food a city gets from working the tile, in **half-food units** (3 = one and a half) |
| `production_percent` | percent added to a city's production per worked tile |
| `planes` | which planes the definition exists on |

The shipped sample set:

| Terrain | Move | Food (half-units) | Production % | City allowed |
|---|---|---|---|---|
| Ocean | - | 0 | 0 | no |
| Grassland | 1 | 3 | 0 | yes |
| Forest | 2 | 1 | 3 | yes |
| Hills | 2 | 1 | 3 | yes |
| Mountains | 3 | 0 | 5 | no |
| Swamp | 2 | 1 | 0 | yes |
| Desert | 1 | 0 | 0 | yes |
| Tundra | 2 | 0 | 0 | yes |

Arcanus and Myrror use the same classes and rules. In the original the planes differ in
artwork and in what spawns, not in terrain behaviour, so a definition simply lists the
planes it exists on and the client chooses plane-specific art. Swamp food is a half food, as
the original's manual says (its code gives none; see the conflicts document).

**Not modelled as terrain types:** the original stores shore, river, lake and volcano variants
as terrain types (762 of them). Here shores follow from neighbouring ocean tiles and are a
rendering matter (the client picks the tile from the original's neighbour-mask table). Rivers
and volcanoes are per-tile flags on `Grid` (a river flag, and a volcano owner); lakes are not
modelled yet.

- **Rivers:** generated from high ground to the sea; a city on a river gets +20% trade, on a
  river mouth +30%, on a shore +10%.
- **Volcanoes:** raised by a spell on a land tile that becomes mountains; each gives its
  owner +1 power (see `terraform.rs`).

## 4. Features

A **feature** is a sparse overlay: at most one per tile, on top of any base terrain. Its
`kind` fixes how the engine treats it; its data entry (`FeatureDef`) holds the rest.

| Kind | Examples | Effect |
|---|---|---|
| `Mineral` | iron, coal, silver, gold, mithril, adamantium, gems, quork, crysx, wild game, nightshade | bonuses to cities that work the tile; a city may be founded on it |
| `Encounter` | lair, cave, temples, ruins, keep, dungeon | a guarded site (guards and loot are not built yet); **blocks** founding a city |
| `Tower` | tower | links the planes (shift plane on it); **blocks** founding |
| `Node` | nature, sorcery, chaos | magic node (power and ownership not built yet); **blocks** founding |

Each `FeatureDef` can carry a `city` bonus, applied to every city whose worked area includes
the tile:

| Feature | Gold | Mana | Food | Unit cost cut | Outpost growth |
|---|---|---|---|---|---|
| Iron ore | | | | 5% | 5 |
| Coal | | | | 10% | 10 |
| Silver ore | 2 | | | | 5 |
| Gold ore | 3 | | | | 10 |
| Gems | 5 | | | | 10 |
| Mithril ore | | 1 | | | 10 |
| Adamantium ore (Myrror) | | 2 | | | 10 |
| Quork crystal | | 3 | | | 10 |
| Crysx crystal (Myrror) | | 5 | | | 10 |
| Wild game | | | 2 | | 10 |
| Nightshade | | | | | 10 |

Mine yields (gold, mana, unit-cost cut) are doubled for races that mine well and increased
by half (gold, mana) or doubled (cost cut) by a miners' guild; every bonus is halved when
two cities share the tile. Food from wild game is unaffected by mining. Total unit-cost
reduction is capped at 50%.

A feature's `spawn` rules say where generation may place it: a list of
`(terrain classes, planes, weight)`. Weights are relative among features of the same kind
on a given class and plane. Myrror differs by data: adamantium and crysx exist only there,
and its gold, mithril and quork weights are higher.

## 5. Roads

A road is a flag on a tile. Effects today:
- **Movement:** a step between two road tiles costs half a move for any walking unit.
- **Trade:** a city on a road network gets a gold bonus from every other city reached by
  contiguous road tiles on the same plane (8-neighbour connectivity): that city's citizens
  count in full if its race differs, half if it is the same race. The bonus, together with
  the coast bonus and any race trade bonus, is capped at three times the city's citizens.

Roads are drawn by the client as a dot (gold when enchanted). Engineers build them (`roads.rs`)
and Enchant Road makes every road within two squares free to travel; all roads on Myrror are
enchanted already.

## 6. Cities and outposts on the map

Cities are entities with a `Position`, not tiles, so a tile can hold a city, units and a
feature at once.

**Founding** (`Game::can_found_city`) fails with, in order: no map; off the map; terrain that
is ocean or has `can_found_city: false`; a blocking feature on the tile (tower, node,
encounter; minerals do not block); a city already there; or another city on the same plane
within **three** tiles (`min_city_distance` is 4, i.e. cities are at least four apart).

**Worked area:** a city works the 5x5 square around it without the four corner tiles (21
tiles), clipped at the poles and wrapped sideways. A tile in two cities' areas is *shared*;
each city gets half its food, production bonus and mine yield. The city's own tile counts.

**Coastal:** a city is coastal if any of its eight neighbours is ocean. Coastal cities get a
10% trade bonus to gold and may build ships.

## 7. Exploration and visibility

- **Exploration** is permanent. Whenever a unit is created or moves, or a city is founded,
  the owner explores a square of radius *r* around it: the unit's `Sight` stat (at least 1)
  or 2 for a city. Exploring a plane is independent of the other.
- **Visibility** is what a player can see *now*: the same squares, computed from the
  player's units and cities as they stand (`Game::visibility`). Enemy units are shown only on
  visible tiles; explored but not visible tiles show terrain, dimmed.
- Fog is per player and per plane, a flag per tile, sized when the first exploration
  happens.

## 8. Generation

`mapgen::generate(seed, &GenParams, &Content)` returns a `Map`; the same seed and parameters
give the same map on every platform (integer value noise, no floats, a seeded PCG
generator).

**Parameters (`GenParams`):**

| Field | Default | Meaning |
|---|---|---|
| `width`, `height` | 60, 40 | tiles per plane |
| `land_percent` | 30 | share of tiles that are land (1..=99) |
| `tower_count` | 6 | towers, at the same coordinates on both planes |
| `planes[].mineral_percent` | 6 (Arcanus), 10 (Myrror) | share of mineral-capable land tiles that get a mineral |
| `planes[].node_count` | 16 (Arcanus), 14 (Myrror) | magic nodes per plane |
| `planes[].encounter_ratio` | 10 | one encounter site per this many land tiles (0 disables) |

**Terrain, per plane:**
1. Two noise fields, *elevation* and *moisture*, are built from several octaves of value
   noise that wraps sideways, so there is no seam. Each plane has its own seed.
2. The land is the highest `land_percent` of elevation.
3. Each land tile gets a class, first match wins:
   - within `height/12` rows of a pole: tundra;
   - elevation in the top 6% of land: mountains; top 20%: hills;
   - moisture in the lowest 10% of land: desert; highest 8%: swamp; highest 33%: forest;
   - otherwise grassland.
4. Everything else is ocean.

**Features, in this order** (each plane gets its own generator state derived from the seed):
1. **Towers.** Random spots at least 10 tiles apart (squared-distance test), at least 3 rows
   from a pole. A tower on water turns that tile into grassland on *both* planes, and the
   same coordinates hold a tower on both planes.
2. **Nodes.** At least 4 tiles from every tower and from each other node, at least 3 rows
   from a pole. The type is chosen by the terrain under it (nature favours forest and swamp,
   sorcery grassland, tundra and desert, chaos mountains and hills).
3. **Encounter sites.** One per `encounter_ratio` land tiles, anywhere on free land.
4. **Minerals.** `mineral_percent` of the free land tiles whose class has any mineral that
   can spawn there, drawn by weight.

Placement scans candidates in a fixed order and shuffles them with the seeded generator, so
results are reproducible. Anything impossible (no free tile for another node, say) is
quietly skipped, so counts are upper bounds.

**Errors:** sizes outside 8..=1024, `land_percent` outside 1..=99, a mineral percentage above
100, a class with no terrain for a plane, or a missing tower definition.

This generator is **our own design, not a port** of the original's (which builds
landmasses with a directional growth process and then fixes up shores). Shapes will differ
from the original's. Reproducing the original's generator is a possible later parity task.

## 9. Differences from the original at a glance

| Original | Here |
|---|---|
| One number per tile encodes terrain, shore variant and river variant (762 types) | A terrain per tile; transitions are derived for drawing |
| Specials (minerals), flags and exploration are separate bitfields | Features, roads and fog are separate, typed layers |
| Lairs, nodes and towers are separate fixed arrays (102, 30, 6) | One feature per tile with a `kind`; counts are generation parameters |
| Corrupted tiles, node auras, rivers, volcanoes | Not yet modelled |
| Myrror nodes produce double power | Not yet modelled (nodes have no power yet) |

## 10. Extending the map

- **A terrain variant:** add a `TerrainDef` in `terrain.ron` with the same class; the first
  definition per class and plane is the one generation uses.
- **A new feature:** add a `FeatureDef` with a kind, a `city` bonus if it has one, and
  spawn rules; `f4x-tools validate` checks it. No code is needed unless it needs a new kind.
- **A new layer** (say corruption): add the array to `Grid` with `#[serde(default)]` so
  existing saves still load, a getter and setter, and decide whether it affects the worked
  area; keep derived information (like a shore) out of the grid.
- **Previewing a map:** `f4x-tools mapgen <seed> [width height] [land%] [content-dir]` prints
  both planes as ASCII (`T` tower, `N` node, `!` encounter, `*` mineral).
