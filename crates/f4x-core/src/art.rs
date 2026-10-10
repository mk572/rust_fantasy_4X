//! Presentation data: which sprite, icon and fallback colour represent each
//! definition. Kept apart from the rules definitions so the simulation never
//! depends on graphics, and so a pack can restyle the game without touching rules.
//!
//! Entries are keyed `"<kind>:<id>"`, for example `"unit:spearman"`. Sprite files
//! are paths relative to the owning pack's `art/` directory.

use serde::{Deserialize, Serialize};

use crate::rules::{Def, Registry};

/// Definition kinds that can have art; the part of a key before the colon.
pub const ART_KINDS: [&str; 9] = [
    "terrain", "feature", "race", "unit", "ability", "building", "realm", "spell", "item",
];

fn one() -> u8 {
    1
}

/// A (possibly animated) image. Animation frames are laid out left to right in one
/// strip, each `frame_size` pixels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sprite {
    pub file: String,
    #[serde(default = "one")]
    pub frames: u8,
    /// Pixel size of one frame; `None` means the whole image is one frame.
    #[serde(default)]
    pub frame_size: Option<(u16, u16)>,
    /// Animation speed; 0 means static.
    #[serde(default)]
    pub fps: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ArtEntry {
    pub key: String,
    /// Main image: map sprite, building picture, unit figure.
    #[serde(default)]
    pub sprite: Option<Sprite>,
    /// Small icon for lists and tooltips.
    #[serde(default)]
    pub icon: Option<Sprite>,
    /// Flat colour used when the image is missing, so a pack can ship without art.
    #[serde(default)]
    pub color: Option<(u8, u8, u8)>,
    /// Id of the pack that supplied this entry (set by the loader).
    #[serde(skip)]
    pub pack: String,
}

impl Def for ArtEntry {
    fn id(&self) -> &str {
        &self.key
    }
}

pub type Art = Registry<ArtEntry>;

pub fn key(kind: &str, id: &str) -> String {
    format!("{kind}:{id}")
}

/// A loaded content pack.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Directory the pack was loaded from.
    pub dir: String,
}
