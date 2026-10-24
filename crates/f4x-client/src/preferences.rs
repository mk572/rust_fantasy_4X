//! The player's preferences (task 89): the original's Settings screen, `MAGIC.SET`, and two of
//! ours (Left Click Moves Units; off: a right click moves; Play Intro, task 85). Twenty-one
//! switches that change how the game is shown or asked about, not its rules. They live outside
//! the saves, in `saves/preferences.txt`, one `name=0|1` line each, so every game shares them.

use std::collections::BTreeMap;
use std::path::Path;

/// The switches in the order of the original's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pref {
    SoundEffects,
    BackgroundMusic,
    EventMusic,
    CitySpellEvents,
    OverlandSpellEvents,
    SummoningEvents,
    EndOfTurnSummary,
    RazeCity,
    RandomEvents,
    EndOfTurnWait,
    StrategicCombatOnly,
    AutoUnitInformation,
    EnemyMoves,
    EnemySpells,
    SpellBookOrdering,
    SpellAnimations,
    ShowNodeOwners,
    ExpandingHelp,
    MovementAnimations,
    LeftClickMoves,
    PlayIntro,
}

pub(crate) const PREFS: [Pref; 21] = [
    Pref::SoundEffects,
    Pref::BackgroundMusic,
    Pref::EventMusic,
    Pref::CitySpellEvents,
    Pref::OverlandSpellEvents,
    Pref::SummoningEvents,
    Pref::EndOfTurnSummary,
    Pref::RazeCity,
    Pref::RandomEvents,
    Pref::EndOfTurnWait,
    Pref::StrategicCombatOnly,
    Pref::AutoUnitInformation,
    Pref::EnemyMoves,
    Pref::EnemySpells,
    Pref::SpellBookOrdering,
    Pref::SpellAnimations,
    Pref::ShowNodeOwners,
    Pref::ExpandingHelp,
    Pref::MovementAnimations,
    Pref::LeftClickMoves,
    Pref::PlayIntro,
];

impl Pref {
    /// The name in the file.
    pub fn key(self) -> &'static str {
        match self {
            Pref::SoundEffects => "sound_effects",
            Pref::BackgroundMusic => "background_music",
            Pref::EventMusic => "event_music",
            Pref::CitySpellEvents => "city_spell_events",
            Pref::OverlandSpellEvents => "overland_spell_events",
            Pref::SummoningEvents => "summoning_events",
            Pref::EndOfTurnSummary => "end_of_turn_summary",
            Pref::RazeCity => "raze_city",
            Pref::RandomEvents => "random_events",
            Pref::EndOfTurnWait => "end_of_turn_wait",
            Pref::StrategicCombatOnly => "strategic_combat_only",
            Pref::AutoUnitInformation => "auto_unit_information",
            Pref::EnemyMoves => "enemy_moves",
            Pref::EnemySpells => "enemy_spells",
            Pref::SpellBookOrdering => "spell_book_ordering",
            Pref::SpellAnimations => "spell_animations",
            Pref::ShowNodeOwners => "show_node_owners",
            Pref::ExpandingHelp => "expanding_help",
            Pref::MovementAnimations => "movement_animations",
            Pref::LeftClickMoves => "left_click_moves",
            Pref::PlayIntro => "play_intro",
        }
    }

    /// The key of the label on the Settings screen.
    pub fn label(self) -> &'static str {
        match self {
            Pref::SoundEffects => "preferences.sound_effects",
            Pref::BackgroundMusic => "preferences.background_music",
            Pref::EventMusic => "preferences.event_music",
            Pref::CitySpellEvents => "preferences.city_spell_events",
            Pref::OverlandSpellEvents => "preferences.overland_spell_events",
            Pref::SummoningEvents => "preferences.summoning_events",
            Pref::EndOfTurnSummary => "preferences.end_of_turn_summary",
            Pref::RazeCity => "preferences.raze_city",
            Pref::RandomEvents => "preferences.random_events",
            Pref::EndOfTurnWait => "preferences.end_of_turn_wait",
            Pref::StrategicCombatOnly => "preferences.strategic_combat_only",
            Pref::AutoUnitInformation => "preferences.auto_unit_information",
            Pref::EnemyMoves => "preferences.enemy_moves",
            Pref::EnemySpells => "preferences.enemy_spells",
            Pref::SpellBookOrdering => "preferences.spell_book_ordering",
            Pref::SpellAnimations => "preferences.spell_animations",
            Pref::ShowNodeOwners => "preferences.show_node_owners",
            Pref::ExpandingHelp => "preferences.expanding_help",
            Pref::MovementAnimations => "preferences.movement_animations",
            Pref::LeftClickMoves => "preferences.left_click_moves",
            Pref::PlayIntro => "preferences.play_intro",
        }
    }

    /// The state without a `settings.ron`: everything is on except strategic combat.
    fn default_on(self) -> bool {
        self != Pref::StrategicCombatOnly
    }
}

/// Where the preferences are kept.
pub(crate) const FILE: &str = "saves/preferences.txt";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Preferences {
    on: [bool; 21],
    /// The music and effect volumes, 0 to 100 (the Settings screen's sliders; not in the
    /// original).
    pub music_volume: u8,
    pub effects_volume: u8,
}

/// The volumes a game starts with.
const MUSIC_VOLUME: u8 = 70;
const EFFECTS_VOLUME: u8 = 100;

impl Default for Preferences {
    fn default() -> Self {
        Self {
            on: PREFS.map(Pref::default_on),
            music_volume: MUSIC_VOLUME,
            effects_volume: EFFECTS_VOLUME,
        }
    }
}

impl Preferences {
    pub fn get(&self, p: Pref) -> bool {
        self.on[p as usize]
    }

    pub fn set(&mut self, p: Pref, on: bool) {
        self.on[p as usize] = on;
    }

    pub fn toggle(&mut self, p: Pref) {
        self.on[p as usize] = !self.on[p as usize];
    }

    /// The switches as `content/settings.ron` sets them; one it does not name stays at the
    /// built-in default. A name that is no switch is an error (a typo would go unnoticed).
    pub fn from_defaults(named: &BTreeMap<String, bool>) -> Result<Self, String> {
        let mut prefs = Self::default();
        for (key, &on) in named {
            let Some(&p) = PREFS.iter().find(|p| p.key() == key) else {
                return Err(format!("settings.ron: {key:?} is not a preference"));
            };
            prefs.set(p, on);
        }
        Ok(prefs)
    }

    /// Read the file over `base`; a missing or damaged one gives `base`, and a name it does not
    /// know is skipped.
    pub fn load(path: &Path, base: Self) -> Self {
        let mut prefs = base;
        let Ok(text) = std::fs::read_to_string(path) else {
            return prefs;
        };
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=') {
                let volume = value.trim().parse::<u8>().ok().map(|v| v.min(100));
                match (key.trim(), volume) {
                    ("music_volume", Some(v)) => prefs.music_volume = v,
                    ("effects_volume", Some(v)) => prefs.effects_volume = v,
                    _ => {}
                }
                if let Some(&p) = PREFS.iter().find(|p| p.key() == key.trim()) {
                    prefs.set(p, value.trim() == "1");
                }
            }
        }
        prefs
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut text: String = PREFS
            .iter()
            .map(|&p| format!("{}={}\n", p.key(), u8::from(self.get(p))))
            .collect();
        for (key, value) in [
            ("music_volume", self.music_volume),
            ("effects_volume", self.effects_volume),
        ] {
            text += &format!("{}={}\n", key, value);
        }
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_is_on_but_strategic_combat() {
        let p = Preferences::default();
        assert!(p.get(Pref::SoundEffects) && p.get(Pref::MovementAnimations));
        assert!(!p.get(Pref::StrategicCombatOnly));
    }

    #[test]
    fn preferences_survive_a_save_and_unknown_lines_are_skipped() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-saves/prefs");
        let path = dir.join("p.txt");
        let mut p = Preferences::default();
        p.toggle(Pref::RazeCity);
        p.toggle(Pref::StrategicCombatOnly);
        p.music_volume = 30;
        p.effects_volume = 0;
        p.save(&path).unwrap();
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("nonsense=1\njunk\n");
        std::fs::write(&path, text).unwrap();
        assert_eq!(Preferences::load(&path, Preferences::default()), p);
        assert_eq!(
            Preferences::load(&dir.join("gone.txt"), Preferences::default()),
            Preferences::default()
        );
    }

    #[test]
    fn the_defaults_come_from_the_settings_file_and_a_saved_file_wins() {
        let named: BTreeMap<_, _> = [("sound_effects".to_string(), false)].into();
        let base = Preferences::from_defaults(&named).unwrap();
        assert!(!base.get(Pref::SoundEffects) && base.get(Pref::RazeCity));
        let gone = Path::new("no/such/preferences.txt");
        assert_eq!(Preferences::load(gone, base.clone()), base);
        let typo: BTreeMap<_, _> = [("sound_effect".to_string(), false)].into();
        assert!(Preferences::from_defaults(&typo).is_err());
    }

    #[test]
    fn the_shipped_settings_name_every_preference() {
        let def = f4x_data::load_settings(&[Path::new("../../content")]).unwrap();
        for p in PREFS {
            assert!(def.preferences.contains_key(p.key()), "{}", p.key());
        }
        let base = Preferences::from_defaults(&def.preferences).unwrap();
        assert!(!base.get(Pref::StrategicCombatOnly) && base.get(Pref::SoundEffects));
    }
}
