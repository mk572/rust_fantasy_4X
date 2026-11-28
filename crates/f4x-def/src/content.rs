//! All loaded game content: the bundle of the registries and tables the simulation reads. The
//! tables themselves live with their concepts (`map/terrain.rs`, `feature/def.rs`, `rules/`);
//! loading and validation from files is `load`.
//!
//! Tests: none, because it only bundles the registries; `load` loads and validates them.

use crate::feature::Features;
use crate::map::Terrains;

/// All loaded game content.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Content {
    pub terrains: Terrains,
    pub features: Features,
    pub rules: crate::rules::Rules,
    /// Difficulty levels, the map and land sizes and the defaults of the new-game choices
    /// (`rules_settings.ron`).
    pub settings: crate::settings::SettingsDef,
    /// The fingerprint of the files this was loaded from (`content_hash::fingerprint_of`); empty for
    /// content that was not loaded from files. A save carries it.
    pub fingerprint: String,
    /// The content as it was loaded, when this is a game's own copy with the game's settings
    /// written into it; none for loaded content. Read it through [`Content::as_loaded`].
    pub loaded: Option<std::sync::Arc<Content>>,
}

impl Content {
    /// The content without any game's settings in it: what a new game starts from and what the
    /// choices for it are made with.
    pub fn as_loaded(&self) -> &Content {
        self.loaded.as_deref().unwrap_or(self)
    }
}
