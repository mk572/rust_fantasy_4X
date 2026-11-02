# Stone Gen

A small standalone Macroquad tool for generating seeded, VGA-style procedural stone textures.

## Run

```sh
cd tools/stone-gen
cargo run --release
```

Use the left-hand panel to choose a material, seed, dimensions, colours, granularity, vein angle, and palette size. Press **Generate** to update the preview and **Save PNG** to write `stone_texture.png` in the current working directory.

The preview zoom slider changes display scale only; it does not change exported dimensions. Enable **Tile preview** to repeat the generated image in the preview. The generator uses periodic noise and integer lattice directions, so the exported image itself also tiles seamlessly.

## Materials

- Marble: warped directional bands and cloudy veining.
- Granite: multi-scale mineral variation and discrete flecks.
- Sandstone: layered sedimentary bands with granular breakup.
- Slate: fine, directional cleavage and mottling.

The palette is interpolated between the darkest, main, and lightest colours and capped at 16 entries. Every output pixel is mapped to one of those exact palette entries.

## Notes

This is intentionally an independent tool crate, not a member of the game workspace. It adds no dependencies to the main game.
