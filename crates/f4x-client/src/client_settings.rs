//! The client's own settings (`crates/f4x-client/content/client_settings.ron`): the language of
//! the messages and the starting state of the Settings screen's switches. None of it changes the
//! rules of a game; the choices that do (starting gold, map size, difficulty) are in the rules'
//! `rules_settings.ron`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ClientSettings {
    /// The language of the messages: a folder of `content/text/` (two letters, `en`).
    pub language: String,
    /// The Settings screen's switches by name (the client knows the names), on or off.
    pub preferences: BTreeMap<String, bool>,
}

impl Default for ClientSettings {
    /// Without a file: the fallback language, every switch at the client's own default.
    fn default() -> Self {
        Self {
            language: f4x_def::text::FALLBACK_LANGUAGE.into(),
            preferences: BTreeMap::new(),
        }
    }
}

impl ClientSettings {
    /// Read `client_settings.ron` from the first of `dirs` that has it. The file is optional; one
    /// that is there must parse.
    pub fn load(dirs: &[&Path]) -> Result<Self, String> {
        let Some(path) = dirs
            .iter()
            .map(|d| d.join("client_settings.ron"))
            .find(|p| p.exists())
        else {
            return Ok(Self::default());
        };
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        ron::from_str(&text).map_err(|e| format!("client_settings.ron: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_def::testdir::{TestDir, TestResult};

    #[test]
    fn the_shipped_settings_name_the_language_and_every_switch() -> TestResult {
        let c = ClientSettings::load(&[Path::new(crate::test_app::CLIENT_CONTENT)])?;
        assert_eq!(c.language, "en");
        assert_eq!(c.preferences.len(), 19);
        assert!(!c.preferences["strategic_combat_only"] && c.preferences["sound_effects"]);
        Ok(())
    }

    #[test]
    fn a_settings_file_can_change_the_language_and_must_parse() -> TestResult {
        let over = TestDir::new("client-settings");
        assert_eq!(ClientSettings::load(&[&over])?, ClientSettings::default());
        std::fs::write(over.join("client_settings.ron"), r#"(language: "de")"#)?;
        assert_eq!(ClientSettings::load(&[&over])?.language, "de");
        std::fs::write(over.join("client_settings.ron"), "(language: ")?;
        assert!(ClientSettings::load(&[&over]).is_err());
        Ok(())
    }
}
