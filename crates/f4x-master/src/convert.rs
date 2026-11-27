//! Saves written before game version 0.0.8 (save version 4), where the round, the end of the game,
//! the master's place in the round and the computer players' plans were fields of the world, and a
//! war was saved with the round it began (`docs/turn-structure.md` 10.8).
//!
//! This module is the only one that knows that form, so that it can be retired (task 823):
//! `SaveFile::from_text` is its one caller.
//!
//! Tests: in tests/it/convert.rs.

mod race_player;

use f4x_core::diplomacy::{DiplomacySave, Pair, ProposalSave, Stance, Treaty};
use f4x_core::wizard::objective::{Objective, Personality};
use f4x_core::world::endgame::EndKind;
use f4x_core::world::save::{
    CitySave, FeatureSave, FogSave, HeroSave, LoadError, MagicItemSave, MapSave, PlayerSave,
    Snapshot, SpellSave, UnitSave, WizardSave, GAME_VERSION, SAVE_VERSION,
};
use serde::Deserialize;

use crate::game::end::GameOver;
use crate::game::save::{GameState, Plans, SaveFile};
use crate::State;

/// The save layout this module reads.
const OLD_SAVE_VERSION: u32 = 4;
/// The oldest game version it reads.
const OLDEST: &str = "0.0.2";
/// The first game version that writes the current layout.
const CURRENT_SINCE: &str = "0.0.8";

/// Whether a save of this layout version is one to convert.
pub(crate) fn is_old(version: u32) -> bool {
    version == OLD_SAVE_VERSION
}

/// `"0.0.4"` as numbers, to compare versions (`"0.0.10"` is newer than `"0.0.9"`).
fn version_parts(version: &str) -> Option<Vec<u32>> {
    version.split('.').map(|p| p.parse().ok()).collect()
}

/// A world as it was saved then.
#[derive(Deserialize)]
#[serde(rename = "Snapshot")]
struct OldSnapshot {
    seed: u64,
    draws: u64,
    turn: u32,
    next_serial: u64,
    settings: f4x_core::world::setting::GameSettings,
    #[serde(default)]
    content: String,
    #[serde(default)]
    degraded: Vec<(String, u16)>,
    game_over: Option<GameOver>,
    human: Option<u8>,
    diplomacy: OldDiplomacy,
    events: f4x_core::world_event::EventsSave,
    #[serde(default)]
    threat: f4x_core::threat::Threat,
    #[serde(default)]
    flow: Option<String>,
    #[serde(default)]
    plans: Plans,
    #[serde(default)]
    locations: Vec<(f4x_core::player::PlayerId, u8, f4x_core::map::Position)>,
    override_log: Vec<String>,
    prefab_taken: Vec<f4x_core::rules::ItemId>,
    map: Option<MapSave>,
    fog: FogSave,
    players: Vec<PlayerSave>,
    wizards: Vec<WizardSave>,
    heroes: Vec<HeroSave>,
    units: Vec<UnitSave>,
    cities: Vec<CitySave>,
    features: Vec<FeatureSave>,
    items: Vec<MagicItemSave>,
    spells: Vec<SpellSave>,
    #[serde(default)]
    battle: Option<f4x_core::battle::save::ActiveBattleSave>,
}

#[derive(Deserialize)]
#[serde(rename = "DiplomacySave")]
struct OldDiplomacy {
    pairs: Vec<(u8, u8, OldPair)>,
    stances: Vec<(u8, u8, Stance)>,
    personalities: Vec<(u8, Personality)>,
    #[serde(default)]
    objectives: Vec<(u8, Objective)>,
    proposals: Vec<ProposalSave>,
}

#[derive(Deserialize)]
#[serde(rename = "Pair")]
struct OldPair {
    contacted: bool,
    status: Treaty,
    /// The round the war began, while the two are at war.
    #[serde(default)]
    war_since: Option<u32>,
    #[serde(default)]
    war_cities: [u16; 2],
}

/// The save of the current layout that holds what the old save `text`, written by `game_version`,
/// held.
pub(crate) fn from_old(text: &str, game_version: &str) -> Result<SaveFile, LoadError> {
    let found = version_parts(game_version);
    if found < version_parts(OLDEST) || found >= version_parts(CURRENT_SINCE) {
        return Err(LoadError::GameVersion {
            found: game_version.to_string(),
            supported: GAME_VERSION,
            oldest: OLDEST,
        });
    }
    let old: OldSnapshot = ron::from_str(text).map_err(|e| LoadError::Parse(e.to_string()))?;
    let round = old.turn;
    let game = GameState {
        round,
        // A place in the round that cannot be read is no place: the first seat plays.
        phase: old
            .flow
            .as_deref()
            .and_then(|flow| ron::from_str::<State>(flow).ok())
            .map(|state| state.phase),
        game_over: old.game_over,
    };
    let diplomacy = DiplomacySave {
        pairs: old
            .diplomacy
            .pairs
            .into_iter()
            .map(|(a, b, p)| {
                let pair = Pair {
                    contacted: p.contacted,
                    status: p.status,
                    war_age: p.war_since.map(|since| round.saturating_sub(since)),
                    war_cities: p.war_cities,
                };
                (a, b, pair)
            })
            .collect(),
        stances: old.diplomacy.stances,
        personalities: old.diplomacy.personalities,
        objectives: old.diplomacy.objectives,
        proposals: old.diplomacy.proposals,
    };
    let mut world = Snapshot {
        version: SAVE_VERSION,
        // What comes out is a save of this version, which can be written and read again.
        game_version: GAME_VERSION.to_string(),
        seed: old.seed,
        draws: old.draws,
        next_serial: old.next_serial,
        settings: old.settings,
        content: old.content,
        degraded: old.degraded,
        mastery: old
            .game_over
            .filter(|over| over.kind == EndKind::SpellOfMastery)
            .map(|over| over.winner.0),
        human: old.human,
        diplomacy,
        events: old.events,
        threat: old.threat,
        locations: old.locations,
        override_log: old.override_log,
        prefab_taken: old.prefab_taken,
        map: old.map,
        fog: old.fog,
        players: old.players,
        wizards: old.wizards,
        heroes: old.heroes,
        units: old.units,
        cities: old.cities,
        features: old.features,
        items: old.items,
        spells: old.spells,
        battle: old.battle,
    };
    let mut plans = old.plans;
    if found < version_parts(race_player::SINCE) {
        race_player::upgrade(&mut world, &mut plans);
    }
    Ok(SaveFile {
        version: SAVE_VERSION,
        game_version: GAME_VERSION.to_string(),
        game,
        world,
        plans,
    })
}
