# Map sprites

`extract-art` writes the overland pictures to `assets-local/art/` (local only, never committed).

| Source | Used for |
|---|---|
| `terrain/tile_NNNN.png`, `types.tsv` | ground by terrain type (762 per plane, Myrror offset 762); animated types use four tiles |
| `terrain/autotile.tsv` | tables 0 (shores, desert, tundra edges) and 1 (mountains, hills) by 8-neighbour mask |
| `mapback` entries 45-62 | roads and enchanted roads (clockwise from north; Myrror roads are all enchanted) |
| 63-67 | node auras per banner, 69/70 towers, 71-75 encounter sites, 77 corruption, 78-86 minerals, 91/92 nightshade/wild game |
| `map/city_{walled,open}_<banner>_<frame>.png` | cities by banner and size (`(pop-1)/4`, at most 4) |
| `map/tower_owned_<banner>_00.png` | owned towers |
| `map/banners.tsv` | banner shades (blue, green, purple, red, yellow, neutral) |

The client picks ground types in `crates/f4x-client/src/terrain_pick.rs` and draws a square in `map_draw.rs`.
Rivers are drawn as the river piece's water pixels only, over any ground. Gaps are tasks 121-125.
