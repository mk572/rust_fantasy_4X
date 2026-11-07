//! Loads the locally extracted original art (`assets-local/art`) on demand.
//! Everything is optional: a missing file just means the caller draws a flat
//! placeholder, so the game runs and CI passes without the original assets.

use crate::icon_art::IconArt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use f4x_extract::spell_art::{Role, SpellArt};
use f4x_extract::unit_art::UnitArt;
use f4x_extract::wizard_art::{WizardArt, WizardPictures};
use macroquad::prelude::*;
use macroquad_bitmap_font_engine::{self as bm, BmFont, FontData};

pub const TILE_W: f32 = 20.0;
pub const TILE_H: f32 = 18.0;

/// Terrain type numbers in the original's tile table.
pub mod terrain_type {
    /// Myrror's types follow Arcanus's.
    pub const MYRROR_OFFSET: usize = 762;
}

/// What the original's minimap is painted with (`f4x-extract art` writes `minimap.tsv`): a colour
/// for each terrain type, and for the cities of each banner and of the neutral ones, the box
/// round the map window and the mark on the selected stack.
#[derive(Default)]
pub struct MinimapColours {
    terrain: Vec<Color>,
    banners: Vec<Color>,
    neutral: Option<Color>,
    box_: Option<Color>,
    mark: Option<Color>,
    /// Other colours by their row's name (`reserve_text`, `combat_text`...).
    named: HashMap<String, Color>,
}

impl MinimapColours {
    fn parse(text: &str) -> Self {
        let mut c = Self::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            let (Some(&kind), Some(rgb)) = (f.first(), f.get(2..5)) else {
                continue;
            };
            let Ok([r, g, b]) = rgb
                .iter()
                .map(|v| v.parse::<f32>())
                .collect::<Result<Vec<_>, _>>()
                .map(|v| [v[0], v[1], v[2]])
            else {
                continue;
            };
            // An optional sixth field is the colour's opacity out of 255.
            let alpha = f
                .get(5)
                .and_then(|a| a.parse::<f32>().ok())
                .unwrap_or(255.0);
            let colour = Color::new(r / 255.0, g / 255.0, b / 255.0, alpha / 255.0);
            match kind {
                "terrain" => c.terrain.push(colour),
                "banner" => c.banners.push(colour),
                "neutral" => c.neutral = Some(colour),
                "box" => c.box_ = Some(colour),
                "mark" => c.mark = Some(colour),
                other => {
                    c.named.insert(other.to_string(), colour);
                }
            }
        }
        c
    }

    /// The colour of a terrain type (Myrror's follow Arcanus's), black without the table.
    pub fn terrain(&self, terrain_type: usize) -> Color {
        self.terrain.get(terrain_type).copied().unwrap_or(BLACK)
    }

    /// The colour of a city of this banner (10 is the neutral banner).
    pub fn city(&self, banner: u8) -> Color {
        self.banners
            .get(usize::from(banner))
            .copied()
            .or(self.neutral)
            .unwrap_or(WHITE)
    }

    pub fn window_box(&self) -> Color {
        self.box_.unwrap_or(MAGENTA)
    }

    pub fn mark(&self) -> Color {
        self.mark.unwrap_or(WHITE)
    }

    /// A colour of the table by name, `fallback` without it.
    pub fn named(&self, kind: &str, fallback: Color) -> Color {
        self.named.get(kind).copied().unwrap_or(fallback)
    }
}

/// The eight bitmap fonts of the original as BMFonts (`.fnt` text and atlas PNG, sixteen colour
/// sets in each atlas), embedded in the client; the index is the font style.
const FONTS: &[(&str, &[u8])] = &[
    (
        include_str!("../assets/fonts/style0.fnt"),
        include_bytes!("../assets/fonts/style0.png"),
    ),
    (
        include_str!("../assets/fonts/style1.fnt"),
        include_bytes!("../assets/fonts/style1.png"),
    ),
    (
        include_str!("../assets/fonts/style2.fnt"),
        include_bytes!("../assets/fonts/style2.png"),
    ),
    (
        include_str!("../assets/fonts/style3.fnt"),
        include_bytes!("../assets/fonts/style3.png"),
    ),
    (
        include_str!("../assets/fonts/style4.fnt"),
        include_bytes!("../assets/fonts/style4.png"),
    ),
    (
        include_str!("../assets/fonts/style5.fnt"),
        include_bytes!("../assets/fonts/style5.png"),
    ),
    (
        include_str!("../assets/fonts/style6.fnt"),
        include_bytes!("../assets/fonts/style6.png"),
    ),
    (
        include_str!("../assets/fonts/style7.fnt"),
        include_bytes!("../assets/fonts/style7.png"),
    ),
];

/// Make the BMFont of a style from the embedded files (needs the window).
pub(crate) fn load_font(style: usize) -> BmFont {
    let (fnt, png) = FONTS[style];
    BmFont::from_bytes(fnt, png).expect("an embedded font loads")
}

/// A combat figure: unit type, direction, animation frame and banner.
pub type FigureKey = (usize, u8, u8, u8);

/// A spell's change to the look of a unit's figures (`Combat_Figure_Effect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FigureEffect {
    /// Black Sleep: in shades of grey.
    Asleep,
    /// Warp Creature: the shades of grey turned inside out, in red.
    Warped,
    /// An invisible unit that can be seen: a flat, translucent silhouette (glass block 1).
    Revealed,
}

pub struct Art {
    dir: PathBuf,
    /// The parsed metrics of the fonts (no window needed): widths and heights.
    font_data: Vec<FontData>,
    /// Terrain type -> first tile, from `map/tiles/types.tsv`.
    type_tiles: Vec<usize>,
    /// Terrain type -> the folder under `map/tiles/` its tiles are in (`arcanus/shore`).
    type_dirs: Vec<String>,
    textures: HashMap<PathBuf, Option<Texture2D>>,
    /// Pictures filed away from `art/<lbx>/`: lbx -> entry -> path from `assets-local/` without
    /// the frame (`screens/<screen>/<name>`, `art/units/<id>/<name>`; `index.tsv`, written by
    /// `f4x-extract art`).
    placed: HashMap<String, HashMap<usize, String>>,
    /// Terrain type by neighbour mask, five tables of 256 (`map/tiles/autotile.tsv`): 0 for
    /// ocean shores, deserts and tundra, 1 for mountains and hills.
    autotile: Vec<Vec<usize>>,
    /// Whether a terrain type animates (its next three tiles are the other frames).
    type_anim: Vec<bool>,
    /// The minimap's colours (`map/tiles/minimap.tsv`).
    minimap: MinimapColours,
    /// The banners' five shades each (`map/banners.tsv`).
    banners: Vec<[Color; 5]>,
    /// Tiles of rivers with everything but the water cut out, to lay over any ground.
    river_overlays: HashMap<usize, Option<Texture2D>>,
    /// Combat figures painted in a banner, by (unit type, direction, frame, banner).
    figures: HashMap<FigureKey, Option<Texture2D>>,
    unit_pictures: HashMap<(String, usize, u8), Option<Texture2D>>,
    masked: HashMap<(String, usize, usize, usize, usize), Option<Texture2D>>,
    /// The pixels of pictures that code reads or edits (kept so they are read back from the
    /// graphics card once): lower-case name, entry, frame.
    pixels: HashMap<(String, usize, usize), Option<Image>>,
    /// Figures with the glow of an enchantment: the figure's key, the realm and the step.
    glows: HashMap<(FigureKey, usize, usize), Option<Texture2D>>,
    /// Map pictures of units with the glow of an enchantment: (file, entry, banner, realm, step).
    unit_glows: HashMap<(String, usize, u8, usize, usize), Option<Texture2D>>,
    /// Which pictures belong to which spell (`crates/f4x-client/content/spell_art.ron`).
    spells: SpellArt,
    /// Which picture each unit and hero uses, by the unit's id (`crates/f4x-client/content/unit_art.ron`).
    units: HashMap<String, UnitArt>,
    /// Which pictures each ready-made wizard uses, by slot (`crates/f4x-client/content/wizard_art.ron`).
    wizards: Vec<WizardArt>,
    /// The small pictures of items and of the battle information window (`content/icon_art.ron`).
    icons: IconArt,
}

/// Banner names in the order the game numbers them.
pub const BANNER_NAMES: &[&str] = &[
    "blue",
    "green",
    "purple",
    "red",
    "yellow",
    "pink",
    "brown",
    "turquoise",
    "dark_blue",
    "orange",
    "neutral",
];

/// The `mapback` entry of a banner's unit background: the original's 14 to 18 and 19 for the
/// neutral one, 100 to 104 for the new colours (`scripts/make_banner_colours.py`).
pub fn background_entry(banner: u8) -> usize {
    match banner {
        0..=4 => 14 + usize::from(banner),
        5..=9 => 95 + usize::from(banner),
        _ => 19,
    }
}

/// The `lilwiz` entry of a wizard's gem, from the wizard's first gem (`WizardPictures::gem`): that
/// plus the colour for the original's five, 100 + that + the new colour's number from 0 for the new
/// ones. None for the neutral banner, which has no gem.
pub fn gem_entry(gem: usize, banner: u8) -> Option<usize> {
    match banner {
        0..=4 => Some(gem + usize::from(banner)),
        5..=9 => Some(100 + gem + usize::from(banner) - 5),
        _ => None,
    }
}

impl Art {
    pub fn load(
        dir: PathBuf,
        spells: SpellArt,
        units: Vec<UnitArt>,
        wizards: Vec<WizardArt>,
        icons: IconArt,
    ) -> Self {
        let types = std::fs::read_to_string(dir.join("map/tiles/types.tsv")).unwrap_or_default();
        let type_tiles = types
            .lines()
            .filter_map(|l| l.split('\t').nth(1)?.parse().ok())
            .collect();
        // The folder under `map/tiles/` of each type's tiles (`arcanus/shore`).
        let type_dirs = types
            .lines()
            .map(|l| l.split('\t').nth(3).unwrap_or("other").trim().to_string())
            .collect();
        let type_anim = types
            .lines()
            .map(|l| l.split('\t').nth(2).is_some_and(|v| v.trim() == "1"))
            .collect();
        let mut autotile = vec![vec![0usize; 256]; 5];
        if let Ok(text) = std::fs::read_to_string(dir.join("map/tiles/autotile.tsv")) {
            for line in text.lines() {
                let mut f = line.split('\t').map(|v| v.parse::<usize>().ok());
                if let (Some(Some(table)), Some(Some(mask)), Some(Some(t))) =
                    (f.next(), f.next(), f.next())
                {
                    if table < 5 && mask < 256 {
                        autotile[table][mask] = t;
                    }
                }
            }
        }
        let mut banners = Vec::new();
        if let Ok(text) = std::fs::read_to_string(dir.join("map/banners.tsv")) {
            for line in text.lines().filter(|l| !l.starts_with('#')) {
                let shades: Vec<Color> = line
                    .split('\t')
                    .skip(2)
                    .filter_map(|c| {
                        let v: Vec<f32> = c.split(',').filter_map(|n| n.parse().ok()).collect();
                        (v.len() == 3)
                            .then(|| Color::new(v[0] / 255.0, v[1] / 255.0, v[2] / 255.0, 1.0))
                    })
                    .collect();
                if let Ok(five) = <[Color; 5]>::try_from(shades) {
                    banners.push(five);
                }
            }
        }
        let minimap = MinimapColours::parse(
            &std::fs::read_to_string(dir.join("map/tiles/minimap.tsv")).unwrap_or_default(),
        );
        let placed = parse_placed(
            &std::fs::read_to_string(dir.with_file_name("index.tsv")).unwrap_or_default(),
        );
        Self {
            dir,
            placed,
            font_data: FONTS
                .iter()
                .map(|(fnt, _)| FontData::parse(fnt).expect("an embedded font parses"))
                .collect(),
            type_tiles,
            type_dirs,
            textures: HashMap::new(),
            autotile,
            type_anim,
            minimap,
            banners,
            river_overlays: HashMap::new(),
            figures: HashMap::new(),
            glows: HashMap::new(),
            unit_glows: HashMap::new(),
            unit_pictures: HashMap::new(),
            masked: HashMap::new(),
            pixels: HashMap::new(),
            spells,
            units: units.into_iter().map(|u| (u.id.clone(), u)).collect(),
            wizards,
            icons,
        }
    }

    /// Whether the table has a wizard for `slot`.
    pub fn has_wizard(&self, slot: u8) -> bool {
        usize::from(slot) < self.wizards.len()
    }

    /// The pictures of the wizard in `slot` (the portrait number the game keeps); a slot the table
    /// does not have gets entry 0 everywhere.
    pub fn wizard(&self, slot: u8) -> WizardPictures {
        self.wizards
            .get(usize::from(slot))
            .map_or_else(WizardPictures::default, |w| w.pictures)
    }

    /// The picture of an item, an entry of `ITEMS.LBX`: a prefab item has its picture in the
    /// table, one a wizard made or found at random carries its own.
    pub fn item_icon(&self, item: &f4x_core::item::Item) -> usize {
        let own = item.prefab.and_then(|id| self.icons.item(id.num()));
        usize::from(own.unwrap_or(item.icon))
    }

    /// The `COMPIX.LBX` entry of a line of the battle information window, if it has a picture.
    pub fn battle_info_icon(&self, key: &str) -> Option<usize> {
        self.icons.battle_info(key)
    }

    /// The `(file, entry)` of a unit's picture in the original's unit files, by the unit's id.
    pub fn unit_entry(&self, unit: &str) -> Option<(String, usize)> {
        self.units.get(unit)?.art.clone()
    }

    /// A hero's entry in the original's `PORTRAIT.LBX`, by the unit's id.
    pub fn hero_portrait(&self, unit: &str) -> Option<usize> {
        self.units.get(unit)?.portrait
    }

    /// Which pictures belong to which spell.
    pub fn spells(&self) -> &SpellArt {
        &self.spells
    }

    /// The `(file, entry)` of a spell's picture for `role`.
    pub fn spell_picture(&self, spell: &str, role: &Role) -> Option<(String, usize)> {
        let (lbx, entry) = self.spells.picture(spell, role)?;
        Some((lbx.to_string(), entry))
    }

    /// The `(file, entry)` of the picture for `role` that no one spell owns.
    pub fn shared_picture(&self, role: &Role) -> Option<(String, usize)> {
        let (lbx, entry) = self.spells.shared(role)?;
        Some((lbx.to_string(), entry))
    }

    /// A font style as a BMFont of the embedded files.
    pub fn bm_font(&self, style: usize) -> std::rc::Rc<BmFont> {
        crate::font::shared_font(style)
    }

    /// Draw `text` in the colour set `set` of a font style: `(x, y)` is the top-left in window
    /// pixels and `scale` the window pixels per original pixel. Returns the width drawn.
    pub fn draw_bitmap_text(
        &mut self,
        style: usize,
        set: usize,
        text: &str,
        (x, y): (f32, f32),
        scale: f32,
    ) -> f32 {
        self.draw_engine_text(style, bm::Ink::Set(set), text, (x, y), scale)
    }

    /// Text in one flat colour: the font's lightest colour set, tinted.
    pub fn draw_flat_text(
        &mut self,
        style: usize,
        text: &str,
        at: (f32, f32),
        scale: f32,
        colour: Color,
    ) -> f32 {
        self.draw_engine_text(style, bm::Ink::Tint(colour), text, at, scale)
    }

    fn draw_engine_text(
        &mut self,
        style: usize,
        ink: bm::Ink,
        text: &str,
        (x, y): (f32, f32),
        scale: f32,
    ) -> f32 {
        let font = self.bm_font(style);
        let params = bm::TextParams {
            scale,
            ink,
            ..Default::default()
        };
        font.draw_text_ex(text, x, y + font.base * scale, &params);
        font.measure_text(text, scale).width
    }

    /// Width in the original's pixels of `text` in a font style.
    pub fn text_width(&self, style: usize, text: &str) -> f32 {
        self.font_data[style].layout(text).width()
    }

    /// Height of a font style in the original's pixels.
    pub fn text_height(&self, style: usize) -> f32 {
        self.font_data[style].base
    }

    pub fn available(&self) -> bool {
        !self.type_tiles.is_empty()
    }

    fn texture(&mut self, relative: PathBuf) -> Option<&Texture2D> {
        let (dir, relative_path) = (self.dir.clone(), relative.clone());
        self.textures
            .entry(relative)
            .or_insert_with(|| load_texture(&dir, &relative_path))
            .as_ref()
    }

    /// The ocean tile type for a mask of land neighbours (NW 1, N 2, NE 4, E 8, SE 16, S 32,
    /// SW 64, W 128): plain ocean when there is no land or no table.
    /// The terrain type the original's table `table` gives for a neighbour mask.
    pub fn autotile_type(&self, table: usize, mask: usize) -> usize {
        self.autotile
            .get(table)
            .and_then(|t| t.get(mask))
            .copied()
            .unwrap_or(0)
    }

    /// The minimap's colours: of a terrain type, and the other palette colours it uses.
    pub fn minimap(&self) -> &MinimapColours {
        &self.minimap
    }

    /// A banner's colour (its middle shade); brown for numbers beyond the table.
    pub fn banner_color(&self, banner: u8) -> Color {
        self.banners
            .get(usize::from(banner))
            .map_or_else(|| default_banner_color(banner), |s| s[2])
    }

    /// A picture recoloured for a banner by `f4x-extract art` (`map/locations/<name>_<banner>_<frame>.png`).
    pub fn banner_picture(&mut self, name: &str, banner: u8, frame: usize) -> Option<&Texture2D> {
        let colour = BANNER_NAMES
            .get(usize::from(banner))
            .copied()
            .unwrap_or("neutral");
        self.texture(PathBuf::from(format!(
            "map/locations/{name}_{colour}_{frame:02}.png"
        )))
    }

    /// The ground tile for a terrain type at an animation frame (0 to 3; fixed tiles ignore it).
    pub fn terrain_frame(&mut self, terrain_type: usize, frame: usize) -> Option<&Texture2D> {
        let tile = *self.type_tiles.get(terrain_type)?;
        let animated = self.type_anim.get(terrain_type).copied().unwrap_or(false);
        let tile = if animated { tile + frame % 4 } else { tile };
        let path = self.tile_path(terrain_type, tile);
        self.texture(path)
    }

    /// A river tile with only its water kept, so it can be laid over forest, desert and the rest.
    pub fn river_overlay(&mut self, terrain_type: usize) -> Option<&Texture2D> {
        if !self.river_overlays.contains_key(&terrain_type) {
            let tex = self.type_tiles.get(terrain_type).and_then(|&tile| {
                let bytes =
                    std::fs::read(self.dir.join(self.tile_path(terrain_type, tile))).ok()?;
                let mut img = Image::from_file_with_format(&bytes, Some(ImageFormat::Png)).ok()?;
                for px in img.bytes.as_chunks_mut::<4>().0.iter_mut() {
                    let water = i32::from(px[2]) > i32::from(px[0]) + 40
                        && i32::from(px[2]) > i32::from(px[1]) + 25;
                    if !water {
                        px[3] = 0;
                    }
                }
                let tex = Texture2D::from_image(&img);
                tex.set_filter(FilterMode::Nearest);
                Some(tex)
            });
            self.river_overlays.insert(terrain_type, tex);
        }
        self.river_overlays
            .get(&terrain_type)
            .and_then(Option::as_ref)
    }

    /// A combat figure of a unit type facing one of eight directions (0 north, clockwise) at an
    /// animation frame, painted in a banner's shades. `None` without the extracted pictures.
    pub fn figure(
        &mut self,
        unit_type: usize,
        direction: u8,
        frame: u8,
        banner: u8,
    ) -> Option<&Texture2D> {
        let key = (unit_type, direction, frame, banner);
        if !self.figures.contains_key(&key) {
            let tex = self.load_figure(unit_type, direction, frame, banner);
            self.figures.insert(key, tex);
        }
        self.figures.get(&key).and_then(Option::as_ref)
    }

    /// A combat figure with the glow the original draws round an enchanted unit's figures
    /// (`Combat_Unit_Enchantment_Outline_Draw`): the transparent pixels next to the figure take
    /// the colours of the realm's ramp, which move with `step`. `realm` is 0 chaos, 1 death,
    /// 2 life, 3 nature, 4 sorcery.
    pub fn glowing_figure(
        &mut self,
        figure: FigureKey,
        realm: usize,
        step: usize,
    ) -> Option<&Texture2D> {
        const REALMS: &[&str] = &["chaos", "death", "life", "nature", "sorcery"];
        let key = (figure, realm, step % 8);
        if !self.glows.contains_key(&key) {
            let ramp: Vec<Color> = (0..8)
                .map(|i| {
                    self.minimap.named(
                        &format!("outline_{}_{}", REALMS[realm % 5], (i + step) % 8),
                        WHITE,
                    )
                })
                .collect();
            let glow = self
                .figure(figure.0, figure.1, figure.2, figure.3)
                .cloned()
                .map(|tex| {
                    let mut img = tex.get_texture_data();
                    outline_pixels(&mut img, &ramp);
                    let glowing = Texture2D::from_image(&img);
                    glowing.set_filter(FilterMode::Nearest);
                    glowing
                });
            self.glows.insert(key, glow);
        }
        self.glows.get(&key).and_then(Option::as_ref)
    }

    /// The colours of a realm's glow at an animation step, as the eight-colour ramp
    /// `outline_pixels` walks.
    fn glow_ramp(&self, realm: usize, step: usize) -> Vec<Color> {
        const REALMS: &[&str] = &["chaos", "death", "life", "nature", "sorcery"];
        (0..8)
            .map(|i| {
                self.minimap.named(
                    &format!("outline_{}_{}", REALMS[realm % 5], (i + step) % 8),
                    WHITE,
                )
            })
            .collect()
    }

    /// A unit's picture on the map or in the unit window with the glow the original draws round an
    /// enchanted unit (`Draw_Unit_Enchantment_Outline`); `realm` and `step` as for
    /// [`Self::glowing_figure`].
    pub fn unit_glow(
        &mut self,
        lbx: &str,
        entry: usize,
        banner: u8,
        realm: usize,
        step: usize,
    ) -> Option<&Texture2D> {
        let key = (lbx.to_string(), entry, banner, realm, step % 8);
        if !self.unit_glows.contains_key(&key) {
            let ramp = self.glow_ramp(realm, step);
            let glow = self.unit_picture(lbx, entry, banner).cloned().map(|tex| {
                let mut img = tex.get_texture_data();
                outline_pixels(&mut img, &ramp);
                let glowing = Texture2D::from_image(&img);
                glowing.set_filter(FilterMode::Nearest);
                glowing
            });
            self.unit_glows.insert(key.clone(), glow);
        }
        self.unit_glows.get(&key).and_then(Option::as_ref)
    }

    /// A figure under a spell's effect, recoloured pixel by pixel as the original does.
    pub fn affected_figure(
        &mut self,
        figure: FigureKey,
        effect: FigureEffect,
    ) -> Option<&Texture2D> {
        let key = (figure, 100 + effect as usize, 0);
        let glass = self
            .minimap
            .named("glass_1", Color::new(0.55, 0.65, 0.9, 0.4));
        if !self.glows.contains_key(&key) {
            let changed = self
                .figure(figure.0, figure.1, figure.2, figure.3)
                .cloned()
                .map(|tex| {
                    let mut img = tex.get_texture_data();
                    for px in img
                        .bytes
                        .as_chunks_mut::<4>()
                        .0
                        .iter_mut()
                        .filter(|p| p[3] > 0)
                    {
                        let grey =
                            (u32::from(px[0]) * 30 + u32::from(px[1]) * 59 + u32::from(px[2]) * 11)
                                / 100;
                        let grey = grey as u8;
                        match effect {
                            FigureEffect::Asleep => px[..3].fill(grey),
                            FigureEffect::Revealed => {
                                px[..3].copy_from_slice(&[
                                    (glass.r * 255.0) as u8,
                                    (glass.g * 255.0) as u8,
                                    (glass.b * 255.0) as u8,
                                ]);
                                px[3] = (f32::from(px[3]) * glass.a) as u8;
                            }
                            FigureEffect::Warped => {
                                let v = 255 - grey;
                                px[..3].copy_from_slice(&[v, v / 4, v / 4]);
                            }
                        }
                    }
                    let tex = Texture2D::from_image(&img);
                    tex.set_filter(FilterMode::Nearest);
                    tex
                });
            self.glows.insert(key, changed);
        }
        self.glows.get(&key).and_then(Option::as_ref)
    }

    fn load_figure(
        &self,
        unit_type: usize,
        direction: u8,
        frame: u8,
        banner: u8,
    ) -> Option<Texture2D> {
        let (lbx, entry) = figure_entry(unit_type, direction);
        self.load_painted(&lbx, entry, usize::from(frame), banner)
    }

    /// A picture painted in a banner's shades, where `f4x-extract art` wrote a banner mask for it.
    fn load_painted(&self, lbx: &str, entry: usize, frame: usize, banner: u8) -> Option<Texture2D> {
        let base = picture_base(&self.placed, lbx, entry, frame);
        let mut img = read_image(&self.dir, Path::new(&format!("{base}.png")))?;
        if let Some(shades) = self.banners.get(usize::from(banner)) {
            if let Some(mask) = read_image(&self.dir, Path::new(&format!("{base}_banner.png"))) {
                let rgb = shades.map(|c| [c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8));
                paint_banner(&mut img.bytes, &mask.bytes, &rgb);
            }
        }
        let tex = Texture2D::from_image(&img);
        tex.set_filter(FilterMode::Nearest);
        Some(tex)
    }

    /// A unit's overland picture (`units1`, `units2`) painted in a banner's shades.
    pub fn unit_picture(&mut self, lbx: &str, entry: usize, banner: u8) -> Option<&Texture2D> {
        let key = (lbx.to_string(), entry, banner);
        if !self.unit_pictures.contains_key(&key) {
            let tex = self.load_painted(lbx, entry, 0, banner);
            self.unit_pictures.insert(key.clone(), tex);
        }
        self.unit_pictures.get(&key).and_then(Option::as_ref)
    }

    /// A ground tile's file under the art directory (`map/tiles/<plane>/<kind>/tile_NNNN.png`,
    /// where `f4x-extract art` put it).
    fn tile_path(&self, terrain_type: usize, tile: usize) -> PathBuf {
        let folder = self
            .type_dirs
            .get(terrain_type)
            .map_or("other", String::as_str);
        PathBuf::from(format!("map/tiles/{folder}/tile_{tile:04}.png"))
    }

    /// A river picture of a battlefield: one of the six shapes of Arcanus or of Myrror (see
    /// [`crate::river_art`]), from the embedded files.
    pub fn river(&mut self, shape: usize, frame: usize, myrror: bool) -> Option<&Texture2D> {
        let frame = frame % crate::river_art::FRAMES;
        let key = PathBuf::from(format!("river/{}_{shape}_{frame}", u8::from(myrror)));
        if !self.textures.contains_key(&key) {
            let tex = crate::river_art::png(shape, frame, myrror).and_then(|bytes| {
                let img = Image::from_file_with_format(bytes, Some(ImageFormat::Png)).ok()?;
                let tex = Texture2D::from_image(&enlarged(
                    img,
                    usize::from(crate::resolution::art_scale()),
                ));
                tex.set_filter(FilterMode::Nearest);
                Some(tex)
            });
            self.textures.insert(key.clone(), tex);
        }
        self.textures.get(&key)?.as_ref()
    }

    /// The pixels of a picture that use the cycling palette index (`cycle.rs`), if it has any.
    pub fn cycle_mask(&mut self, lbx: &str, entry: usize, frame: usize) -> Option<&Texture2D> {
        let base = picture_base(&self.placed, lbx, entry, frame);
        self.texture(PathBuf::from(format!("{base}_cycle.png")))
    }

    /// The node box's colour slide (`VGA_SlideColors(247, 8, ..)`): the pixels of the picture in
    /// the sliding range, in the colours of step `step` (0 to 7), if it has any.
    pub fn slide_frame(
        &mut self,
        lbx: &str,
        entry: usize,
        frame: usize,
        step: usize,
    ) -> Option<&Texture2D> {
        let base = picture_base(&self.placed, lbx, entry, frame);
        self.texture(PathBuf::from(format!("{base}_slide_{}.png", step % 8)))
    }

    /// How many frames a picture of an `.lbx` file has (those that were extracted).
    pub fn frames(&mut self, lbx: &str, entry: usize) -> usize {
        (0..64)
            .take_while(|&f| self.picture(lbx, entry, f).is_some())
            .count()
    }

    /// One of the original's mouse pointers, numbered from 1 (`crsr_Finger`).
    pub fn cursor(&mut self, number: usize) -> Option<&Texture2D> {
        self.texture(PathBuf::from(format!("cursors/{number:02}.png")))
    }

    /// A picture of `lbx` with its pixels cleared wherever frame `mask_frame` of the `mask`
    /// picture is transparent (`Clipped_Copy_Mask` as `OVL_DrawGlobalAnim` uses it): only
    /// what lies under the mask's solid part shows.
    pub fn masked_picture(
        &mut self,
        lbx: &str,
        entry: usize,
        frame: usize,
        mask: (&str, usize),
        mask_frame: usize,
    ) -> Option<&Texture2D> {
        let key = (lbx.to_string(), entry, frame, mask.1, mask_frame);
        if !self.masked.contains_key(&key) {
            let tex = self.load_masked(lbx, entry, frame, mask, mask_frame);
            self.masked.insert(key.clone(), tex);
        }
        self.masked.get(&key).and_then(Option::as_ref)
    }

    fn load_masked(
        &self,
        lbx: &str,
        entry: usize,
        frame: usize,
        mask: (&str, usize),
        mask_frame: usize,
    ) -> Option<Texture2D> {
        let read = |lbx: &str, entry: usize, frame: usize| {
            read_image(&self.dir, &picture_path(&self.placed, lbx, entry, frame))
        };
        let mut img = read(lbx, entry, frame)?;
        let mask = read(mask.0, mask.1, mask_frame)?;
        let (w, h) = (usize::from(img.width), usize::from(img.height));
        let (mw, mh) = (usize::from(mask.width), usize::from(mask.height));
        for y in 0..h {
            for x in 0..w {
                let solid = x < mw && y < mh && mask.bytes[(y * mw + x) * 4 + 3] > 0;
                if !solid {
                    img.bytes[(y * w + x) * 4 + 3] = 0;
                }
            }
        }
        let tex = Texture2D::from_image(&img);
        tex.set_filter(FilterMode::Nearest);
        Some(tex)
    }

    /// The pixels of a picture (at the art scale), read back from its texture the first time.
    pub fn pixels(&mut self, lbx: &str, entry: usize, frame: usize) -> Option<&Image> {
        let key = (lbx.to_string(), entry, frame);
        if !self.pixels.contains_key(&key) {
            let image = self
                .picture(lbx, entry, frame)
                .map(Texture2D::get_texture_data);
            self.pixels.insert(key.clone(), image);
        }
        self.pixels.get(&key).and_then(Option::as_ref)
    }

    /// A picture from an original `.lbx` file: lower-case name, entry, frame.
    pub fn picture(&mut self, lbx: &str, entry: usize, frame: usize) -> Option<&Texture2D> {
        let path = picture_path(&self.placed, lbx, entry, frame);
        self.texture(path)
    }
}

/// Paint the outline of a picture (`Outline_Bitmap_Pixels_No_Glass`, `Bitmap_Aura_Pixels`): every
/// transparent pixel with a figure pixel above, below or beside it takes the next colour of the
/// ramp, which advances as the picture is read a column at a time.
fn outline_pixels(img: &mut Image, ramp: &[Color]) {
    let (w, h) = (usize::from(img.width), usize::from(img.height));
    let inside: Vec<bool> = (0..w * h).map(|i| img.bytes[i * 4 + 3] > 0).collect();
    let (mut at, mut step) = (0usize, 0usize);
    for x in 0..w {
        for y in 0..h {
            let near = |dx: isize, dy: isize| {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                (0..w as isize).contains(&nx)
                    && (0..h as isize).contains(&ny)
                    && inside[ny as usize * w + nx as usize]
            };
            if !inside[y * w + x] && (near(-1, 0) || near(1, 0) || near(0, -1) || near(0, 1)) {
                let c = ramp[at % ramp.len()];
                let i = (y * w + x) * 4;
                img.bytes[i..i + 4].copy_from_slice(&[
                    (c.r * 255.0) as u8,
                    (c.g * 255.0) as u8,
                    (c.b * 255.0) as u8,
                    255,
                ]);
            }
            at = (at + step) & 7;
            step = (step + 1) & 7;
        }
    }
}

/// Read `screens/index.tsv`: `lbx  entry  <screen>/<name>` a row.
fn parse_placed(text: &str) -> HashMap<String, HashMap<usize, String>> {
    let mut placed: HashMap<String, HashMap<usize, String>> = HashMap::new();
    for line in text.lines() {
        let mut f = line.split('\t');
        if let (Some(lbx), Some(Ok(entry)), Some(place)) =
            (f.next(), f.next().map(str::parse::<usize>), f.next())
        {
            placed
                .entry(lbx.to_string())
                .or_default()
                .insert(entry, place.to_string());
        }
    }
    placed
}

/// Where a picture's file is, relative to the art directory and without `.png`: in
/// `art/<lbx>/`, or in the folder `f4x-extract art` filed it under (a screen's, a unit's).
fn picture_base(
    placed: &HashMap<String, HashMap<usize, String>>,
    lbx: &str,
    entry: usize,
    frame: usize,
) -> String {
    match placed.get(lbx).and_then(|e| e.get(&entry)) {
        Some(place) => format!("../{place}_{frame:02}"),
        None => format!("{lbx}/{entry:03}_{frame:02}"),
    }
}

/// A picture file as an image at the art scale in use (`crate::resolution`): the finer art in
/// `x2/` or `x4/` beside the originals when it has the file, otherwise the original enlarged by
/// whole pixels, so an image is always the art scale times the size of its picture on the design
/// grid.
fn read_image(dir: &Path, relative: &Path) -> Option<Image> {
    trace_read(relative);
    let scale = crate::resolution::art_scale();
    if scale > 1 {
        let fine = dir.join(format!("x{scale}")).join(relative);
        if let Some(image) = std::fs::read(fine)
            .ok()
            .and_then(|bytes| Image::from_file_with_format(&bytes, Some(ImageFormat::Png)).ok())
        {
            return Some(image);
        }
    }
    let bytes = std::fs::read(dir.join(relative)).ok()?;
    let image = Image::from_file_with_format(&bytes, Some(ImageFormat::Png)).ok()?;
    Some(enlarged(image, usize::from(scale)))
}

/// With `F4X_TRACE_ART=FILE` in the environment, every picture file the game reads is appended to
/// FILE (`scripts/unused_art.py` lists what no run reads).
fn trace_read(relative: &Path) {
    use std::io::Write;
    if let Some(Ok(mut file)) = std::env::var_os("F4X_TRACE_ART").map(|path| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    }) {
        let _ = writeln!(file, "{}", relative.display());
    }
}

/// `image` with every pixel repeated `by` times across and down.
fn enlarged(image: Image, by: usize) -> Image {
    if by <= 1 {
        return image;
    }
    let (w, h) = (usize::from(image.width), usize::from(image.height));
    let mut bytes = Vec::with_capacity(w * h * by * by * 4);
    for y in 0..h * by {
        for x in 0..w * by {
            let at = ((y / by) * w + x / by) * 4;
            bytes.extend_from_slice(&image.bytes[at..at + 4]);
        }
    }
    Image {
        bytes,
        width: (w * by) as u16,
        height: (h * by) as u16,
    }
}

fn load_texture(dir: &Path, relative: &Path) -> Option<Texture2D> {
    let tex = Texture2D::from_image(&read_image(dir, relative)?);
    tex.set_filter(FilterMode::Nearest);
    Some(tex)
}

fn picture_path(
    placed: &HashMap<String, HashMap<usize, String>>,
    lbx: &str,
    entry: usize,
    frame: usize,
) -> PathBuf {
    PathBuf::from(format!("{}.png", picture_base(placed, lbx, entry, frame)))
}

/// The picture file and entry of a unit type's figure facing `direction`: fifteen unit
/// types to a file, eight directions each (`Combat_Figure_Load`).
pub fn figure_entry(unit_type: usize, direction: u8) -> (String, usize) {
    let file = unit_type / 15 + 1;
    let name = if file < 10 {
        format!("figures{file}")
    } else {
        format!("figure{file}")
    };
    (name, (unit_type % 15) * 8 + usize::from(direction))
}

/// The unit type number the original's tables use for a unit whose overland picture is entry
/// `entry` of `units1` (types 0 to 119) or `units2` (120 on).
pub fn unit_type_number(lbx: &str, entry: usize) -> Option<usize> {
    match lbx {
        "units1" => Some(entry),
        "units2" => Some(120 + entry),
        _ => None,
    }
}

/// Paint the pixels a banner mask marks (red channel 1 to 5, alpha 255) in the banner's
/// shades. Both slices are RGBA of the same picture.
pub fn paint_banner(rgba: &mut [u8], mask: &[u8], shades: &[[u8; 3]; 5]) {
    for (px, m) in rgba
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(mask.as_chunks::<4>().0)
    {
        if m[3] == 255 && (1..=5).contains(&m[0]) {
            let c = shades[usize::from(m[0]) - 1];
            px[..3].copy_from_slice(&c);
        }
    }
}

/// Colours to use before the original's banners are extracted.
fn default_banner_color(banner: u8) -> Color {
    match banner {
        0 => Color::new(0.33, 0.33, 0.65, 1.0),
        1 => Color::new(0.33, 0.65, 0.33, 1.0),
        2 => Color::new(0.65, 0.33, 0.65, 1.0),
        3 => Color::new(0.68, 0.0, 0.0, 1.0),
        4 => Color::new(1.0, 0.8, 0.14, 1.0),
        5 => Color::new(0.65, 0.33, 0.5, 1.0),
        6 => Color::new(0.5, 0.38, 0.26, 1.0),
        7 => Color::new(0.0, 0.68, 0.68, 1.0),
        8 => Color::new(0.18, 0.25, 0.45, 1.0),
        9 => Color::new(1.0, 0.54, 0.14, 1.0),
        _ => Color::new(0.75, 0.6, 0.46, 1.0),
    }
}

#[cfg(test)]
mod banner_tests {
    use super::*;

    #[test]
    fn every_banner_has_a_name_a_background_and_a_gem() {
        assert_eq!(
            BANNER_NAMES.len(),
            usize::from(f4x_core::player::NEUTRAL_BANNER) + 1
        );
        assert_eq!(
            BANNER_NAMES[usize::from(f4x_core::player::NEUTRAL_BANNER)],
            "neutral"
        );
        let backgrounds: Vec<usize> = (0..=10).map(background_entry).collect();
        assert_eq!(
            backgrounds,
            [14, 15, 16, 17, 18, 100, 101, 102, 103, 104, 19]
        );
        assert_eq!(gem_entry(10, 3), Some(13));
        assert_eq!(gem_entry(0, 5), Some(100));
        assert_eq!(gem_entry(65, 9), Some(169));
        assert_eq!(gem_entry(0, f4x_core::player::NEUTRAL_BANNER), None);
    }
}

#[cfg(test)]
mod figure_tests {
    use super::*;

    #[test]
    fn a_picture_of_one_screen_is_found_in_that_screens_folder() {
        let placed = parse_placed("main\t8\tscreens/main_screen/button_done\nbad line\n");
        assert_eq!(
            picture_path(&placed, "main", 8, 1),
            PathBuf::from("../screens/main_screen/button_done_01.png")
        );
        assert_eq!(
            picture_path(&placed, "main", 9, 0),
            PathBuf::from("main/009_00.png")
        );
        assert_eq!(
            picture_path(&placed, "backgrnd", 24, 0),
            PathBuf::from("backgrnd/024_00.png")
        );
    }

    #[test]
    fn figures_are_fifteen_types_to_a_file_with_eight_directions() {
        assert_eq!(figure_entry(0, 0), ("figures1".to_string(), 0));
        assert_eq!(figure_entry(14, 7), ("figures1".to_string(), 119));
        assert_eq!(figure_entry(15, 2), ("figures2".to_string(), 2));
        assert_eq!(figure_entry(135, 0), ("figure10".to_string(), 0));
        assert_eq!(figure_entry(197, 7), ("figure14".to_string(), 2 * 8 + 7));
    }

    #[test]
    fn unit_types_follow_the_two_unit_picture_files() {
        assert_eq!(unit_type_number("units1", 5), Some(5));
        assert_eq!(unit_type_number("units2", 5), Some(125));
        assert_eq!(unit_type_number("mainscrn", 5), None);
    }

    #[test]
    fn banner_pixels_take_the_banners_shades() {
        let mut px = [9u8, 9, 9, 255, 7, 7, 7, 255, 1, 2, 3, 255];
        let mask = [0u8, 0, 0, 0, 2, 0, 0, 255, 5, 0, 0, 255];
        let shades = [[10; 3], [20; 3], [30; 3], [40; 3], [50; 3]];
        paint_banner(&mut px, &mask, &shades);
        assert_eq!(px, [9, 9, 9, 255, 20, 20, 20, 255, 50, 50, 50, 255]);
    }

    /// The embedded BMFonts parse, each page names its own file, and every style has the ASCII
    /// glyphs.
    #[test]
    fn the_embedded_fonts_parse_and_cover_ascii() {
        let art = Art::load(
            PathBuf::new(),
            SpellArt::default(),
            Vec::new(),
            Vec::new(),
            IconArt::default(),
        );
        for (n, font) in art.font_data.iter().enumerate() {
            assert_eq!(font.page, format!("style{n}.png"));
            for ascii in 32u8..=126 {
                assert!(
                    font.glyph(char::from(ascii)).is_some(),
                    "style {n} lacks glyph {ascii}"
                );
            }
            assert!(art.text_height(n) > 0.0, "style {n}");
        }
    }

    /// The atlases of styles 0 to 5 stack the original's 16 colour sets and then the ramps of
    /// `font_ramp.rs`; regenerate them (`scripts/font_ramps.py`, `docs/extract-art.md`) when this fails.
    #[test]
    fn the_atlases_of_the_letter_styles_hold_the_ramps() {
        for (n, (fnt, png)) in FONTS.iter().enumerate() {
            let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]) as usize;
            let line = fnt
                .lines()
                .find(|l| l.contains("scaleH="))
                .expect("common line");
            let grid: usize = line
                .split("scaleH=")
                .nth(1)
                .and_then(|r| r.split_whitespace().next())
                .and_then(|v| v.parse().ok())
                .expect("scaleH");
            let sets = if n < 6 {
                16 + crate::font_ramp::COUNT
            } else {
                16
            };
            assert_eq!(height, grid * sets, "style {n}");
        }
    }
}
