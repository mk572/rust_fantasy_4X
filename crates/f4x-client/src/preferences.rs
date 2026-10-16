//! The player's preferences (task 89): the original's Settings screen, `MAGIC.SET`. Eighteen
//! switches that change how the game is shown or asked about, not its rules. They live outside
//! the saves, in `saves/preferences.txt`, one `name=0|1` line each, so every game shares them.

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
}

pub(crate) const PREFS: [Pref; 19] = [
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
        }
    }

    /// The label on the Settings screen.
    pub fn label(self) -> &'static str {
        match self {
            Pref::SoundEffects => "Sound Effects",
            Pref::BackgroundMusic => "Background Music",
            Pref::EventMusic => "Event Music",
            Pref::CitySpellEvents => "City Spell Events",
            Pref::OverlandSpellEvents => "Overland Spell Events",
            Pref::SummoningEvents => "Summoning Events",
            Pref::EndOfTurnSummary => "End of Turn Summary",
            Pref::RazeCity => "Raze City",
            Pref::RandomEvents => "Random Events",
            Pref::EndOfTurnWait => "End of Turn Wait",
            Pref::StrategicCombatOnly => "Strategic Combat Only",
            Pref::AutoUnitInformation => "Auto Unit Information",
            Pref::EnemyMoves => "Enemy Moves",
            Pref::EnemySpells => "Enemy Spells",
            Pref::SpellBookOrdering => "Spell Book Ordering",
            Pref::SpellAnimations => "Spell Animations",
            Pref::ShowNodeOwners => "Show Node Owners",
            Pref::ExpandingHelp => "Expanding Help",
            Pref::MovementAnimations => "Movement Animations",
        }
    }

    /// Everything is on except strategic combat.
    fn default_on(self) -> bool {
        self != Pref::StrategicCombatOnly
    }
}

/// Where the preferences are kept.
pub(crate) const FILE: &str = "saves/preferences.txt";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Preferences {
    on: [bool; 19],
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            on: PREFS.map(Pref::default_on),
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

    /// Read the file; a missing or damaged one gives the defaults, and a name it does not
    /// know is skipped.
    pub fn load(path: &Path) -> Self {
        let mut prefs = Self::default();
        let Ok(text) = std::fs::read_to_string(path) else {
            return prefs;
        };
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=') {
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
        let text: String = PREFS
            .iter()
            .map(|&p| format!("{}={}\n", p.key(), u8::from(self.get(p))))
            .collect();
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
        p.save(&path).unwrap();
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("nonsense=1\njunk\n");
        std::fs::write(&path, text).unwrap();
        assert_eq!(Preferences::load(&path), p);
        assert_eq!(
            Preferences::load(&dir.join("gone.txt")),
            Preferences::default()
        );
    }
}
