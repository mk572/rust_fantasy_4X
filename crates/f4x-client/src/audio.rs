//! Sound effects and music (tasks 76 and 97). What plays when is `docs/sound.md`.
//!
//! The sounds are the original's, extracted by `f4x-tools extract-audio` to `assets-local/audio`
//! (`<archive>/NNN_name.ogg`); the repository ships none, and a game without them is silent. A
//! sound is named by its archive and entry, as the original names it; `content/sound.ron` says
//! which entry a spell or unit plays. Effects are played where things happen (a click, a
//! battle animation starting); the music is declared: every frame `App::want_music` says what
//! should be playing for the state the game is in, and `Audio::music` changes the track only
//! when that differs, as the original does by stopping all sound, playing a piece and starting
//! the background track again when the box closes.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::task::{Context, Poll, Waker};

use crate::input::Input;
use f4x_core::player::PlayerId;
use f4x_data::sound::SoundTable;
use macroquad::audio::{
    load_sound_from_bytes, play_sound, set_sound_volume, stop_sound, PlaySoundParams, Sound,
};
use macroquad::prelude::*;

use crate::preferences::Pref;
use crate::view::{App, Screen, HUMAN};

/// The sound archives the client plays from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Archive {
    SoundFx,
    Combat,
    NewSound,
    Music,
}

impl Archive {
    const ALL: [Archive; 4] = [
        Archive::SoundFx,
        Archive::Combat,
        Archive::NewSound,
        Archive::Music,
    ];

    /// The archive by the name of its folder, for `--hear`.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.folder() == name)
    }

    fn folder(self) -> &'static str {
        match self {
            Archive::SoundFx => "soundfx",
            Archive::Combat => "cmbtsnd",
            Archive::NewSound => "newsound",
            Archive::Music => "music",
        }
    }
}

/// One sound: an entry of an archive.
pub(crate) type Key = (Archive, u16);

pub(crate) fn fx(entry: u16) -> Key {
    (Archive::SoundFx, entry)
}

/// The cast sound of a spell row: positive is a `SOUNDFX` entry, negative a `NEWSOUND` entry.
pub(crate) fn spell_sound(n: i16) -> Option<Key> {
    match n {
        0 => None,
        n if n > 0 => Some(fx(n as u16)),
        n => Some((Archive::NewSound, n.unsigned_abs())),
    }
}

/// The sound of a unit's step: 0 is silent, 13 is `NEWSOUND` entry 0 (`Reload_Battle_Unit_Move_Sound`).
pub(crate) fn step_sound(n: u8) -> Option<Key> {
    match n {
        0 => None,
        13 => Some((Archive::NewSound, 0)),
        n => Some(fx(u16::from(n))),
    }
}

/// The sound of a melee blow: `CMBTSND` entry 0 to 7, 8 is silent (`Reload_Melee_Sound`).
pub(crate) fn melee_sound(n: u8) -> Option<Key> {
    (n != 8).then_some((Archive::Combat, u16::from(n)))
}

/// The sound of a shot by the missile picture set it throws (`Ranged_Animation`).
pub(crate) fn shot_sound(missile: usize) -> Option<Key> {
    Some(fx(match missile {
        0 => 19,  // lightning
        1 => 17,  // arrow
        2 => 20,  // fireball
        3 => 21,  // illusion
        4 => 15,  // rocks: the catapult
        5 => 18,  // sling
        6 => 22,  // death bolt
        7 => 23,  // ice bolt
        9 => 24,  // priest
        10 => 25, // dark elf
        11 => 26, // sprite and nature bolt
        _ => return None,
    }))
}

/// The scream of a wizard struck down in the conquest scene: a woman's for a feminine wizard,
/// else a man's (`SOUNDFX` 34 and 35; the original keys on the wizard's number).
pub(crate) fn scream_sound(feminine: bool) -> Key {
    fx(if feminine { 34 } else { 35 })
}

/// The button click (`Play_Left_Click`; `Play_Standard_Click`, entry 0, is only for warnings).
pub(crate) const CLICK: Key = (Archive::SoundFx, 2);

/// A track of `MUSIC.LBX` and whether it is background music or one of the two event pieces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Track {
    pub entry: u16,
    pub looped: bool,
    /// Event pieces play with Event Music on; everything else with Background Music.
    pub event: bool,
}

impl Track {
    /// Plays until the screen changes.
    pub fn looping(entry: u16) -> Self {
        Self {
            entry,
            looped: true,
            event: false,
        }
    }

    /// A short piece that plays once.
    pub fn piece(entry: u16) -> Self {
        Self {
            entry,
            looped: false,
            event: false,
        }
    }
}

/// `MUSIC.LBX` entries by name (`MUSIC_Tracks` of ReMoM's `SOUND.h`).
pub(crate) mod music {
    pub const SOM_STARTED: u16 = 12;
    pub const INTRO: u16 = 112;
    pub const FANFARE: u16 = 115;
    pub const SPELL_FAILURE: u16 = 17;
    pub const NEW_SPELL: u16 = 40;
    pub const WIZARD_GOOD: u16 = 41; // + 2 * wizard
    pub const WIZARD_BAD: u16 = 42; // + 2 * wizard
    pub const WIZARD_COMBAT: u16 = 71; // + 2 * wizard, + 0 or 1
    pub const BACKGROUND_GOOD: u16 = 99;
    pub const BACKGROUND_NEUTRAL: u16 = 100;
    pub const BACKGROUND_NEGATIVE: u16 = 101;
    pub const COMBAT: u16 = 102; // + 0 or 1
    pub const MAIN_MENU: u16 = 104;
    pub const SITE_DISCOVERY: u16 = 105;
    pub const EVENT_GOOD: u16 = 106;
    pub const NEW_OUTPOST: u16 = 107;
    pub const BUILDING_FINISHED: u16 = 108;
    pub const WIN_MILITARY: u16 = 109;
    pub const LOSE_MILITARY: u16 = 110;
    pub const LOSE_BY_MAGIC: u16 = 111;
    pub const EVENT_BAD: u16 = 114;
}

/// Seconds-long summing of `Poll::Pending` is never needed: the loaders do not wait on anything.
fn ready<F: Future>(future: F) -> Option<F::Output> {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => Some(v),
        Poll::Pending => None,
    }
}

pub(crate) struct Audio {
    /// The files found, by archive and entry.
    files: HashMap<Key, PathBuf>,
    /// The effects heard so far; `None` for one that would not load.
    loaded: HashMap<Key, Option<Sound>>,
    /// The music playing: what, and its sound (none if there is no file for it).
    playing: Option<(Track, Option<Sound>)>,
    /// The combat track picked when the battle began, so that it does not change mid-fight.
    pub battle_track: Option<(PlayerId, Track)>,
    /// What `content/sound.ron` says.
    pub table: SoundTable,
    pub effects_on: bool,
    pub music_on: bool,
    pub event_music_on: bool,
    /// 0 to 1 (the Settings screen's sliders).
    music_volume: f32,
    effects_volume: f32,
}

impl Audio {
    /// No sound at all (tests, screenshots).
    pub fn silent() -> Self {
        Self {
            files: HashMap::new(),
            loaded: HashMap::new(),
            playing: None,
            battle_track: None,
            table: SoundTable::default(),
            effects_on: false,
            music_on: false,
            event_music_on: false,
            music_volume: 0.7,
            effects_volume: 1.0,
        }
    }

    /// Find the files under `dir`: `<archive>/NNN_name.ogg` (or `.wav`). A missing folder gives
    /// a silent game.
    pub fn scan(dir: &Path, table: SoundTable) -> Self {
        let mut audio = Self::silent();
        audio.table = table;
        for archive in Archive::ALL {
            let Ok(entries) = std::fs::read_dir(dir.join(archive.folder())) else {
                continue;
            };
            for e in entries.flatten() {
                let path = e.path();
                let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("");
                if !ext.eq_ignore_ascii_case("ogg") && !ext.eq_ignore_ascii_case("wav") {
                    continue;
                }
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
                if let Ok(entry) = digits.parse::<u16>() {
                    audio.files.insert((archive, entry), path);
                }
            }
        }
        audio
    }

    /// Whether any sound file was found.
    pub fn has_files(&self) -> bool {
        !self.files.is_empty()
    }

    fn load(&mut self, key: Key) -> Option<&Sound> {
        if !self.loaded.contains_key(&key) {
            let sound = self
                .files
                .get(&key)
                .and_then(|p| std::fs::read(p).ok())
                .and_then(|bytes| ready(load_sound_from_bytes(&bytes)))
                .and_then(Result::ok);
            self.loaded.insert(key, sound);
        }
        self.loaded.get(&key)?.as_ref()
    }

    /// The volumes in percent; the music already playing follows at once.
    pub fn set_volumes(&mut self, music: u8, effects: u8) {
        let music = f32::from(music.min(100)) / 100.0;
        self.effects_volume = f32::from(effects.min(100)) / 100.0;
        if (music - self.music_volume).abs() > f32::EPSILON {
            self.music_volume = music;
            if let Some((_, Some(sound))) = &self.playing {
                set_sound_volume(sound, music);
            }
        }
    }

    /// Whether the sound is there and decodes.
    pub fn can_play(&mut self, key: Key) -> bool {
        self.load(key).is_some()
    }

    /// Play an effect once, if effects are on and the file is there.
    pub fn effect(&mut self, key: Option<Key>) {
        let Some(key) = key else { return };
        if !self.effects_on {
            return;
        }
        let volume = self.effects_volume;
        if let Some(sound) = self.load(key) {
            play_sound(
                sound,
                PlaySoundParams {
                    looped: false,
                    volume,
                },
            );
        }
    }

    /// Stop an effect that may still be playing (its sound is only stopped if it was loaded).
    pub fn stop_effect(&mut self, key: Option<Key>) {
        if let Some(Some(sound)) = key.and_then(|k| self.loaded.get(&k)) {
            stop_sound(sound);
        }
    }

    /// Make `want` the music that plays: nothing changes while it already does. A track that
    /// its switch forbids is not played; turning the switch on starts it.
    pub fn music(&mut self, want: Option<Track>) {
        let allowed = want.filter(|t| {
            if t.event {
                self.event_music_on
            } else {
                self.music_on
            }
        });
        if self.playing.as_ref().map(|(t, _)| *t) == allowed {
            return;
        }
        if let Some((_, Some(sound))) = self.playing.take() {
            stop_sound(&sound);
        }
        let Some(track) = allowed else { return };
        // Music is large once decoded: it is loaded for its turn and dropped after.
        let sound = self
            .files
            .get(&(Archive::Music, track.entry))
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| ready(load_sound_from_bytes(&bytes)))
            .and_then(Result::ok);
        if let Some(s) = &sound {
            play_sound(
                s,
                PlaySoundParams {
                    looped: track.looped,
                    volume: self.music_volume,
                },
            );
        }
        self.playing = Some((track, sound));
    }

    /// The music that is playing, for tests.
    #[cfg(test)]
    fn current(&self) -> Option<Track> {
        self.playing.as_ref().map(|(t, _)| *t)
    }
}

/// The overland track by the human's power against the rivals' (`Get_Background_Music`): bad when
/// it is at or below 110% of the weakest rival's (a weakest above 0), good when it is within 10%
/// of the strongest's or above; bad wins.
fn background_entry(me: i32, rivals: &[i32]) -> u16 {
    let (Some(&best), Some(&worst)) = (rivals.iter().max(), rivals.iter().min()) else {
        return music::BACKGROUND_NEUTRAL;
    };
    if worst > 0 && me - worst / 10 <= worst {
        music::BACKGROUND_NEGATIVE
    } else if me + best / 10 >= best {
        music::BACKGROUND_GOOD
    } else {
        music::BACKGROUND_NEUTRAL
    }
}

/// A battle's music: the pair of the wizard fought (`pick` 0 or 1 of its two), or the two of
/// the monsters and independent cities.
fn combat_entry(wizard: Option<u16>, pick: usize) -> u16 {
    let pick = (pick % 2) as u16;
    match wizard {
        Some(w) => music::WIZARD_COMBAT + 2 * w + pick,
        None => music::COMBAT + pick,
    }
}

/// A wizard's diplomacy tune: its friendly one, or the angry one.
fn diplomacy_entry(wizard: u16, bad: bool) -> u16 {
    (if bad {
        music::WIZARD_BAD
    } else {
        music::WIZARD_GOOD
    }) + 2 * wizard
}

/// The wizards in the order of the original's music: a wizard's portrait picks its tracks.
const WIZARDS: u16 = 14;

impl App {
    /// Once a frame: switch the music to what the game's state asks for, and play the button
    /// click for a press that lands on a button.
    pub(crate) fn update_audio(&mut self, input: &Input) {
        self.audio
            .set_volumes(self.prefs.music_volume, self.prefs.effects_volume);
        self.audio.effects_on = self.prefs.get(Pref::SoundEffects);
        self.audio.music_on = self.prefs.get(Pref::BackgroundMusic);
        self.audio.event_music_on = self.prefs.get(Pref::EventMusic);
        if input.mouse_pressed(MouseButton::Left) && self.click_lands_on_button(input) {
            self.audio.effect(Some(CLICK));
        }
        let want = self.want_music();
        self.audio.music(want);
    }

    /// The overland map and the battle field take clicks that are no buttons.
    fn click_lands_on_button(&self, input: &Input) -> bool {
        let at: Vec2 = input.mouse_position().into();
        let lay = crate::frame::Layout::new();
        match self.screen {
            Screen::Console => false,
            Screen::Map => !lay.rect(crate::frame::spot::MAP).contains(at),
            Screen::Battle => !lay.rect((0.0, 0.0, 320.0, 164.0)).contains(at),
            _ => true,
        }
    }

    /// What should be playing now: a short piece while its box is up, else the screen's music.
    fn want_music(&mut self) -> Option<Track> {
        if self.game.in_battle() || matches!(self.screen, Screen::Battle) {
            return self.combat_track();
        }
        self.audio.battle_track = None;
        if let Some(track) = self.end_scene_track() {
            return Some(track);
        }
        match self.screen {
            Screen::MainMenu => return Some(Track::looping(music::MAIN_MENU)),
            Screen::Intro => return Some(Track::piece(music::INTRO)),
            Screen::Create | Screen::Options => return None,
            Screen::Diplomacy => return self.diplomacy_track(),
            Screen::GameOver => return self.game_over_track(),
            _ => {}
        }
        self.piece_track()
            .or_else(|| Some(Track::looping(self.background_track())))
    }

    /// The short piece that belongs to the box on top, if one is up.
    fn piece_track(&self) -> Option<Track> {
        if !matches!(
            self.screen,
            Screen::Map | Screen::Magic | Screen::Outpost(_)
        ) {
            return None;
        }
        if let Screen::Outpost(_) = self.screen {
            return self
                .outpost_ui
                .new
                .then(|| Track::piece(music::NEW_OUTPOST));
        }
        if self.game_over_pending() {
            return None;
        }
        if !self.level_ups.is_empty() {
            return Some(Track {
                entry: music::FANFARE,
                looped: false,
                event: true,
            });
        }
        if let Some(c) = self.global_casts.front() {
            let entry = self
                .game
                .content()
                .rules
                .spells
                .get(c.spell())
                .id
                .as_str()
                .to_owned();
            let entry = self.audio.table.global_music.get(&entry).copied()?;
            return Some(Track::piece(u16::from(entry)));
        }
        if !self.mastery_notice.is_empty() {
            return Some(Track::piece(music::SOM_STARTED));
        }
        if let Some(n) = self.event_notices.front() {
            return Some(Track {
                entry: if n.kind.is_good() {
                    music::EVENT_GOOD
                } else {
                    music::EVENT_BAD
                },
                looped: false,
                event: true,
            });
        }
        if !self.built_notices.is_empty() {
            return Some(Track::piece(music::BUILDING_FINISHED));
        }
        if !self.site_notices.is_empty() {
            return Some(Track::piece(music::SITE_DISCOVERY));
        }
        if !self.big_ui.learned.is_empty() {
            return Some(Track::piece(music::NEW_SPELL));
        }
        if self.spell_notices.front().is_some_and(|n| n.fizzled()) {
            return Some(Track::piece(music::SPELL_FAILURE));
        }
        None
    }

    fn game_over_pending(&self) -> bool {
        self.game.game_over.is_some() && !self.game_over_seen
    }

    /// The overland track by how the human's power compares with the rivals' (`Get_Background_Music`).
    fn background_track(&self) -> u16 {
        let at = (self.game.turn as usize).saturating_sub(1).min(287);
        let power = |p: PlayerId| {
            self.game
                .player(p)
                .and_then(|h| h.history().get(at).copied())
                .map(i32::from)
        };
        let rivals: Vec<i32> = self
            .game
            .player_ids()
            .into_iter()
            .filter(|&p| p != HUMAN && self.game.is_wizard(p))
            .filter_map(power)
            .collect();
        power(HUMAN).map_or(music::BACKGROUND_NEUTRAL, |me| {
            background_entry(me, &rivals)
        })
    }

    /// The wizard's number in the original's music order: its portrait, when it is one of the 14.
    fn wizard_tracks_of(&self, player: PlayerId) -> Option<u16> {
        let p = u16::from(self.game.wizard(player)?.portrait());
        (p < WIZARDS).then_some(p)
    }

    /// The pair of the opposing wizard, or the general pair against monsters, picked once.
    fn combat_track(&mut self) -> Option<Track> {
        let battle = self.game.battle()?;
        let foe = if battle.attacker == HUMAN {
            battle.defender
        } else {
            battle.attacker
        };
        if let Some((who, track)) = self.audio.battle_track {
            if who == foe {
                return Some(track);
            }
        }
        let pick = (self.game.turn as usize + usize::from(foe.0)) % 2;
        let wizard = self
            .wizard_tracks_of(foe)
            .filter(|_| self.game.is_wizard(foe));
        let track = Track::looping(combat_entry(wizard, pick));
        self.audio.battle_track = Some((foe, track));
        Some(track)
    }

    /// The talking wizard's own tune: the angry one when its mood is bad.
    fn diplomacy_track(&self) -> Option<Track> {
        let w = self.wizard_tracks_of(self.diplo_ui.with)?;
        Some(Track::looping(diplomacy_entry(
            w,
            self.diplo_ui.mood_is_bad(),
        )))
    }

    fn game_over_track(&self) -> Option<Track> {
        let over = self.game.game_over?;
        let won = over.winner == HUMAN;
        Some(Track::piece(match (won, over.kind) {
            (true, _) => music::WIN_MILITARY,
            (false, f4x_core::game::endgame::EndKind::SpellOfMastery) => music::LOSE_BY_MAGIC,
            (false, _) => music::LOSE_MILITARY,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sound_numbers_pick_the_files_the_original_does() {
        assert_eq!(spell_sound(0), None);
        assert_eq!(spell_sound(93), Some((Archive::SoundFx, 93)));
        assert_eq!(spell_sound(-20), Some((Archive::NewSound, 20)));
        assert_eq!(step_sound(0), None);
        assert_eq!(step_sound(6), Some((Archive::SoundFx, 6)));
        assert_eq!(step_sound(13), Some((Archive::NewSound, 0)));
        assert_eq!(melee_sound(8), None);
        assert_eq!(melee_sound(3), Some((Archive::Combat, 3)));
        assert_eq!(shot_sound(1), Some((Archive::SoundFx, 17)));
        assert_eq!(shot_sound(8), None);
        assert_eq!(scream_sound(true), (Archive::SoundFx, 34));
        assert_eq!(scream_sound(false), (Archive::SoundFx, 35));
    }

    #[test]
    fn every_missile_the_client_draws_has_a_sound_but_the_unknown() {
        for missile in [0, 1, 2, 3, 4, 5, 6, 7, 9, 10, 11] {
            assert!(shot_sound(missile).is_some(), "{missile}");
        }
    }

    #[test]
    fn the_background_track_follows_the_standing() {
        // Alone: neutral. Level with the best: good. Far below the worst: bad. Bad wins.
        assert_eq!(background_entry(50, &[]), music::BACKGROUND_NEUTRAL);
        assert_eq!(background_entry(100, &[100, 40]), music::BACKGROUND_GOOD);
        assert_eq!(background_entry(90, &[120, 80]), music::BACKGROUND_NEUTRAL);
        assert_eq!(background_entry(60, &[100, 80]), music::BACKGROUND_NEGATIVE);
        assert_eq!(background_entry(10, &[100, 80]), music::BACKGROUND_NEGATIVE);
        assert_eq!(background_entry(80, &[80, 80]), music::BACKGROUND_NEGATIVE);
        // A weakest rival with no power does not make the player's standing bad.
        assert_eq!(background_entry(100, &[0, 100]), music::BACKGROUND_GOOD);
    }

    #[test]
    fn battles_and_talks_use_the_wizards_own_pair() {
        assert_eq!(combat_entry(None, 0), 102);
        assert_eq!(combat_entry(None, 3), 103);
        assert_eq!(combat_entry(Some(0), 0), 71);
        assert_eq!(combat_entry(Some(13), 1), 98);
        assert_eq!(diplomacy_entry(0, false), 41);
        assert_eq!(diplomacy_entry(13, true), 68);
    }

    #[test]
    fn music_follows_its_switch_and_event_pieces_their_own() {
        let mut a = Audio::silent();
        let bg = Track::looping(100);
        let event = Track {
            entry: 106,
            looped: false,
            event: true,
        };
        a.music(Some(bg));
        assert_eq!(a.current(), None, "music off");
        a.music_on = true;
        a.music(Some(bg));
        assert_eq!(a.current(), Some(bg));
        a.music(Some(event));
        assert_eq!(a.current(), None, "Event Music is off");
        a.event_music_on = true;
        a.music(Some(event));
        assert_eq!(a.current(), Some(event));
        a.music(None);
        assert_eq!(a.current(), None);
    }

    #[test]
    fn files_are_found_by_archive_and_entry() {
        let dir = std::env::temp_dir().join("f4x-audio-scan-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("soundfx")).unwrap();
        std::fs::create_dir_all(dir.join("music")).unwrap();
        std::fs::write(dir.join("soundfx/002_left_btn_clk.ogg"), b"x").unwrap();
        std::fs::write(dir.join("soundfx/readme.txt"), b"x").unwrap();
        std::fs::write(dir.join("music/104_main_menu.ogg"), b"x").unwrap();
        std::fs::write(dir.join("music/104_main_menu.xmi"), b"x").unwrap();
        let a = Audio::scan(&dir, SoundTable::default());
        assert!(a.files.contains_key(&(Archive::SoundFx, 2)));
        assert!(a.files.contains_key(&(Archive::Music, 104)));
        assert_eq!(a.files.len(), 2);
        assert!(!Audio::scan(&dir.join("none"), SoundTable::default()).has_files());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
