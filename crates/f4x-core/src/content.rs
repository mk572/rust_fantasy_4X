//! All loaded game content: the bundle of the registries and tables the simulation reads. The
//! tables themselves live with their concepts (`map/terrain.rs`, `feature/def.rs`, `rules/`);
//! loading and validation from files lives in `f4x-data`.
//!
//! Tests: none, because it only bundles the registries; `f4x-data` loads and validates them.

use crate::feature::def::Features;
use crate::map::terrain::Terrains;

/// All loaded game content.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Content {
    pub terrains: Terrains,
    pub features: Features,
    pub rules: crate::rules::Rules,
    /// Difficulty levels, the map and land sizes and the defaults of the new-game choices
    /// (`rules_settings.ron`).
    pub settings: crate::game::setting::SettingsDef,
    /// Every player-facing message, in the language the content was loaded with.
    pub texts: crate::text::Texts,
}
