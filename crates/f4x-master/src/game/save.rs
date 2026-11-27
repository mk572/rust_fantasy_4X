//! The saved game: the game's own state, the world, and what each computer player has in mind
//! (`docs/turn-structure.md` 10.8). Saves from before game version 0.0.8 are converted
//! (`convert.rs`).
//!
//! Tests: in tests/it/save.rs.

use std::path::Path;
use std::sync::Arc;

use f4x_core::player::PlayerId;
use f4x_core::world::rng::GameRng;
use f4x_core::world::save::file::{read_text, with_content_fallback, write_text};
use f4x_core::world::save::{
    check_versions, signed, with_setting_fallback, ContentCheck, Integrity, LoadError, SaveError,
    Snapshot, GAME_VERSION, SAVE_VERSION,
};
use f4x_def::content::Content;
use f4x_def::load::ContentIssue;
use serde::{Deserialize, Serialize};

use super::attack::HeldAttack;
use super::end::GameOver;
use super::reaction::Reaction;
use super::Game;
use crate::{convert, turn_players, Phase};

/// What each computer player has in mind, as text its controller wrote, with the player. The game
/// never reads them: they are saved beside it and handed back.
pub type Plans = Vec<(PlayerId, String)>;

/// The part of a game that is not in its world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameState {
    /// The generator the random numbers come from, saved as its seed and the number drawn.
    pub rng: GameRng,
    /// The round the game is in, the first being 1.
    pub round: u32,
    /// Where the master was in the round, as it last wrote it: `None` for a game that no master
    /// has driven since its last step.
    #[serde(default)]
    pub phase: Option<Phase>,
    /// How the game ended, once it has.
    #[serde(default)]
    pub game_over: Option<GameOver>,
    /// The players that take turns, in the order they take them. Fixed when the game begins.
    /// Empty only in a save converted from an older layout, until it is restored.
    #[serde(default)]
    pub players: Vec<PlayerId>,
    /// What the players may still answer of what happened to them in this turn.
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    /// The attacks that wait for their defender's answer, the oldest first.
    #[serde(default)]
    pub attacks: Vec<HeldAttack>,
}

impl GameState {
    /// The state of a game that begins.
    pub fn first_round(rng: GameRng, players: Vec<PlayerId>) -> Self {
        Self {
            rng,
            round: f4x_core::world::FIRST_ROUND,
            phase: None,
            game_over: None,
            players,
            reactions: Vec::new(),
            attacks: Vec::new(),
        }
    }
}

/// A saved game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveFile {
    pub version: u32,
    /// The game version that wrote the save.
    pub game_version: String,
    pub game: GameState,
    /// The world, which also loads on its own.
    pub world: Snapshot,
    #[serde(default)]
    pub plans: Plans,
}

/// A game rebuilt from a save, and what the player should be told about the load: rows the game
/// plays without, and rows it was played without that are back.
pub struct Restored {
    pub game: Game,
    pub plans: Plans,
    pub notes: Vec<ContentIssue>,
}

impl std::fmt::Debug for Restored {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Restored")
            .field("state", &self.game.state)
            .field("notes", &self.notes)
            .finish_non_exhaustive()
    }
}

/// Just the version fields of a save, read before the rest so that a file of another layout is
/// refused for its version and not for whatever it fails to parse as.
#[derive(Deserialize)]
pub(crate) struct Header {
    pub version: u32,
    #[serde(default)]
    pub game_version: String,
}

fn parse_error(e: impl std::fmt::Display) -> LoadError {
    LoadError::Parse(e.to_string())
}

impl SaveFile {
    /// The save as readable text. The first line names the versions and carries the checksum of
    /// the rest.
    pub fn to_text(&self) -> Result<String, LoadError> {
        let body = ron::ser::to_string_pretty(
            self,
            ron::ser::PrettyConfig::default()
                .depth_limit(3)
                .compact_arrays(true),
        )
        .map_err(parse_error)?;
        Ok(signed(&body))
    }

    /// Read a save, of this version of the game or an older one that can be converted.
    pub fn from_text(text: &str) -> Result<Self, LoadError> {
        let header: Header = ron::from_str(text).map_err(|e| parse_error(e.code))?;
        with_setting_fallback(|| {
            if convert::is_old(header.version) {
                return convert::from_old(text, &header.game_version);
            }
            check_versions(header.version, &header.game_version)?;
            ron::from_str(text).map_err(parse_error)
        })
    }

    /// Rebuild the game against `content`; see [`Snapshot::restore_with`].
    pub fn restore(
        self,
        content: impl Into<Arc<Content>>,
        check: ContentCheck,
    ) -> Result<Restored, LoadError> {
        let (mut world, notes) = self.world.restore_with(content, check)?;
        let mut state = self.game;
        if state.players.is_empty() {
            // A converted save: older games made a race's player only while it had something,
            // and kept no list of the players.
            world.add_race_players();
            state.players = turn_players(&world);
        }
        let mut game = Game::new(world, state.rng.clone());
        game.state = state;
        Ok(Restored {
            game,
            plans: self.plans,
            notes,
        })
    }
}

impl Game {
    /// Everything needed to continue this game later, with the plans of its computer players.
    pub fn save_file(&self, plans: Plans) -> SaveFile {
        SaveFile {
            version: SAVE_VERSION,
            game_version: GAME_VERSION.to_string(),
            game: self.state.clone(),
            world: self.world.snapshot(),
            plans,
        }
    }
}

/// Write the game to `path`; a crash while saving never leaves a half-written save over a good one.
pub fn save_game(path: &Path, game: &Game, plans: Plans) -> Result<(), SaveError> {
    let text = game.save_file(plans).to_text().map_err(SaveError::Load)?;
    write_text(path, &text)
}

/// A save that was read from a file.
#[derive(Debug)]
pub struct Loaded {
    pub game: Game,
    pub plans: Plans,
    /// Whether the file is as the game wrote it: an edited save still loads, and the caller warns
    /// about it.
    pub integrity: Integrity,
    pub notes: Vec<ContentIssue>,
}

impl std::fmt::Debug for Game {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Game")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

/// Read a save and rebuild the game against `content`. A save whose settings are out of range is
/// refused, and so is one made with other content files unless `check` says to ignore that.
pub fn load_game(path: &Path, content: Content, check: ContentCheck) -> Result<Loaded, SaveError> {
    let text = read_text(path)?;
    let content = Arc::new(content);
    let restored = with_content_fallback(&content, || {
        SaveFile::from_text(&text)?.restore(content.clone(), check)
    })?;
    Ok(Loaded {
        game: restored.game,
        plans: restored.plans,
        integrity: Snapshot::integrity(&text),
        notes: restored.notes,
    })
}

/// What a save slot list shows about a save without rebuilding the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveSummary {
    /// The human wizard's name.
    pub wizard: String,
    pub turn: u32,
    /// The game had ended when it was saved.
    pub finished: bool,
}

/// Read just enough of a save to describe it.
pub fn summarize_save(path: &Path) -> Result<SaveSummary, SaveError> {
    let save = SaveFile::from_text(&read_text(path)?).map_err(SaveError::Load)?;
    let human = save.world.human.unwrap_or(0);
    let wizard = save
        .world
        .wizards
        .iter()
        .find(|w| w.player == human)
        .or_else(|| save.world.wizards.first())
        .map_or_else(String::new, |w| w.name.clone());
    Ok(SaveSummary {
        wizard,
        turn: save.game.round,
        finished: save.game.game_over.is_some(),
    })
}
