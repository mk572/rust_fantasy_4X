//! All loaded game content: the bundle of the registries and tables the simulation reads. The
//! tables themselves live with their concepts (`map/terrain.rs`, `feature/def.rs`, `rules/`);
//! loading and validation from files is `load`.
//!
//! Tests: none, because it only bundles the registries; `load` loads and validates them.

use crate::feature::Features;
use crate::map::Terrains;

/// All loaded game content.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Content {
    pub terrains: Terrains,
    pub features: Features,
    pub rules: crate::rules::Rules,
    /// Difficulty levels, the map and land sizes and the defaults of the new-game choices
    /// (`rules_settings.ron`).
    pub settings: crate::settings::SettingsDef,
}
