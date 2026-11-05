//! The overland map's sprites, named, and recoloured for each banner.
//!
//! Almost everything the map draws besides terrain tiles sits in `MAPBACK.LBX`. This module
//! writes `map/catalog.tsv` (a name for every sprite, with its entry, frame count and size),
//! and for the pictures the game recolours at run time (cities, towers) one PNG per banner in
//! `map/locations/`:
//! the indices `214..=218` (and `224..=228` on towers) are replaced with the banner's five
//! shades, as `Draw_Map_Cities` does.

use std::fs;
use std::path::Path;

use crate::image::{decode, to_rgba, vga_to_rgb8};
use crate::lbx::Lbx;

/// Banner colours in the order the game numbers them: blue, green, purple, red, yellow, and
/// the neutral brown. Five palette indexes per banner, from `COL_City_Banner`.
pub const BANNER_NAMES: [&str; 6] = ["blue", "green", "purple", "red", "yellow", "neutral"];

/// `COL_City_Banner`: palette indexes (one-based in the original's table, as it subtracts
/// one when replacing).
const CITY_BANNER: [[u8; 5]; 6] = [
    [0x60, 0x61, 0x62, 0x63, 0x64],
    [0x48, 0x49, 0x4A, 0x4B, 0x4C],
    [0xCD, 0xCE, 0xCF, 0xD0, 0xD1],
    [0xC9, 0xA5, 0xCB, 0xA6, 0x2D],
    [0xD1, 0xD2, 0xD3, 0xD4, 0xD5],
    [0x1D, 0x1D, 0x1C, 0x1C, 0x1B],
];

/// `COL_Banners`: the five shades each banner paints units with, as palette indexes.
const UNIT_BANNER: [[u8; 5]; 6] = [
    [0x69, 0x6A, 0x6B, 0x6C, 0x6D],
    [0x49, 0x4A, 0x4B, 0x4C, 0x4D],
    [0x79, 0x7A, 0x7B, 0x7C, 0x7D],
    [0xC9, 0xA5, 0xCB, 0xA6, 0x2D],
    [0xA0, 0xA1, 0xA2, 0xB2, 0xB4],
    [0x30, 0x31, 0x32, 0x33, 0x34],
];

/// Names of the `MAPBACK.LBX` entries the overland map uses.
pub fn catalog_names() -> Vec<(String, usize)> {
    let mut v: Vec<(String, usize)> = Vec::new();
    for i in 0..14 {
        v.push((format!("unexplored_mask_{i}"), i));
    }
    for (i, b) in BANNER_NAMES.iter().enumerate() {
        v.push((format!("unit_background_{b}"), 14 + i));
    }
    v.push(("city_walled".into(), 20));
    v.push(("city_open".into(), 21));
    for (i, d) in ["none", "n", "ne", "e", "se", "s", "sw", "w", "nw"]
        .iter()
        .enumerate()
    {
        v.push((format!("road_{d}"), 45 + i));
        v.push((format!("enchanted_road_{d}"), 54 + i));
    }
    for (i, b) in BANNER_NAMES.iter().take(5).enumerate() {
        v.push((format!("node_aura_{b}"), 63 + i));
    }
    v.push(("node_aura_extra".into(), 68));
    for (name, e) in [
        ("tower_unowned", 69),
        ("tower_owned", 70),
        ("site_cave", 71),
        ("site_temple", 72),
        ("site_keep", 73),
        ("site_ruins", 74),
        ("site_fallen_temple", 75),
        ("mud", 76),
        ("corruption", 77),
        ("mineral_coal", 78),
        ("mineral_iron", 79),
        ("mineral_silver", 80),
        ("mineral_gold", 81),
        ("mineral_gems", 82),
        ("mineral_mithril", 83),
        ("mineral_adamantium", 84),
        ("mineral_quork", 85),
        ("mineral_crysx", 86),
        ("mine", 87),
        ("lumber_camp", 88),
        ("city_work_area", 89),
        ("hunters_lodge", 90),
        ("nightshade", 91),
        ("wild_game", 92),
        ("node_warped_mask", 93),
    ] {
        v.push((name.to_string(), e));
    }
    v
}

/// The folder under `art/map/` a named sprite goes to: places on the map (cities, towers, the
/// encounter sites, magic nodes) in `locations`, the rest in a folder of `overlays` by kind.
pub fn folder(name: &str) -> &'static str {
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| name.starts_with(p));
    if starts(&["city_walled", "city_open", "tower_", "site_", "node_"]) {
        "locations"
    } else if starts(&["road_", "enchanted_road_"]) {
        "overlays/roads"
    } else if starts(&[
        "mineral_",
        "mine",
        "lumber_camp",
        "hunters_lodge",
        "nightshade",
        "wild_game",
    ]) {
        "overlays/resources"
    } else if starts(&["unexplored_mask_"]) {
        "overlays/fog"
    } else if starts(&["unit_background_"]) {
        "overlays/unit_backgrounds"
    } else {
        // Corruption, mud, the city work area.
        "overlays/ground"
    }
}

/// Replace the indexes `from..from + 5` with `to`.
fn recolor(pixels: &[u8], from: u8, to: [u8; 5]) -> Vec<u8> {
    pixels
        .iter()
        .map(|&p| {
            if (from..from + 5).contains(&p) {
                // The original subtracts one from the table value when it recolours cities.
                to[usize::from(p - from)].saturating_sub(1).max(1)
            } else {
                p
            }
        })
        .collect()
}

pub fn extract_map_art(
    mapback: &[u8],
    pal: &[u8],
    out_dir: &Path,
    write_png: crate::fonts::PngWriter,
) -> Result<usize, String> {
    let lbx = Lbx::parse(mapback).map_err(|e| format!("MAPBACK.LBX: {e}"))?;
    let dir = out_dir.join("map");
    fs::create_dir_all(dir.join("locations")).map_err(|e| e.to_string())?;
    let mut catalog = String::from("# name\tlbx\tentry\tframes\twidth\theight\n");
    let mut count = 0;
    for (name, entry) in catalog_names() {
        if entry >= lbx.len() {
            continue;
        }
        let Some(pic) = decode(lbx.entry(entry)) else {
            continue;
        };
        catalog.push_str(&format!(
            "{name}\tmapback\t{entry}\t{}\t{}\t{}\n",
            pic.frames.len(),
            pic.width,
            pic.height
        ));
        count += 1;
    }
    // Cities and towers once per banner.
    for (name, entry, range) in [
        ("city_walled", 20, 214u8),
        ("city_open", 21, 214),
        ("tower_owned", 70, 224),
    ] {
        let Some(pic) = decode(lbx.entry(entry)) else {
            continue;
        };
        for (b, banner) in BANNER_NAMES.iter().enumerate() {
            for (f, frame) in pic.frames.iter().enumerate() {
                let mut px = recolor(&frame.pixels, range, CITY_BANNER[b]);
                if range == 224 {
                    // Towers carry the same five shades at 214 as well in some frames.
                    px = recolor(&px, 214, CITY_BANNER[b]);
                }
                let rgba = to_rgba(&px, pal);
                write_png(
                    &dir.join(format!("locations/{name}_{banner}_{f:02}.png")),
                    pic.width,
                    pic.height,
                    &rgba,
                )?;
            }
        }
    }
    fs::write(dir.join("catalog.tsv"), catalog).map_err(|e| e.to_string())?;
    // The banners as colours: one line each, `index  name  five shades as r,g,b ...`.
    let mut banners = String::from("# index\tname\tshade 1..5 as r,g,b\n");
    for (b, name) in BANNER_NAMES.iter().enumerate() {
        let shades: Vec<String> = UNIT_BANNER[b]
            .iter()
            .map(|&i| {
                let c = usize::from(i) * 3;
                format!(
                    "{},{},{}",
                    vga_to_rgb8(pal[c]),
                    vga_to_rgb8(pal[c + 1]),
                    vga_to_rgb8(pal[c + 2])
                )
            })
            .collect();
        banners.push_str(&format!("{b}\t{name}\t{}\n", shades.join("\t")));
    }
    fs::write(dir.join("banners.tsv"), banners).map_err(|e| e.to_string())?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recolouring_swaps_only_the_banner_indexes() {
        let px = [0u8, 5, 214, 216, 218, 219, 224];
        let out = recolor(&px, 214, [10, 20, 30, 40, 50]);
        assert_eq!(out, vec![0, 5, 9, 29, 49, 219, 224]);
    }

    #[test]
    fn places_go_to_locations_and_the_rest_to_overlays_by_kind() {
        assert_eq!(folder("city_open"), "locations");
        assert_eq!(folder("site_cave"), "locations");
        assert_eq!(folder("node_aura_blue"), "locations");
        assert_eq!(folder("tower_owned"), "locations");
        assert_eq!(folder("city_work_area"), "overlays/ground");
        assert_eq!(folder("road_n"), "overlays/roads");
        assert_eq!(folder("enchanted_road_s"), "overlays/roads");
        assert_eq!(folder("mineral_gold"), "overlays/resources");
        assert_eq!(folder("mine"), "overlays/resources");
        assert_eq!(folder("unexplored_mask_3"), "overlays/fog");
        assert_eq!(folder("unit_background_red"), "overlays/unit_backgrounds");
        assert_eq!(folder("corruption"), "overlays/ground");
    }

    #[test]
    fn every_named_sprite_has_a_unique_name_and_entry_in_range() {
        let names = catalog_names();
        let mut seen = std::collections::HashSet::new();
        for (n, e) in &names {
            assert!(seen.insert(n.clone()), "{n} twice");
            assert!(*e < 94, "{n}: entry {e}");
        }
        assert!(names.iter().any(|(n, e)| n == "wild_game" && *e == 92));
        assert!(names.iter().any(|(n, e)| n == "corruption" && *e == 77));
    }
}
