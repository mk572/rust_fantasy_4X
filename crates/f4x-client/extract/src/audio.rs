//! `f4x-extract audio`: the original's sound effects as OGG and its music as the XMI files it stores.
//!
//! `SOUNDFX`, `CMBTSND`, `NEWSOUND` and `INTROSFX` hold Creative Voice (VOC) clips; each is decoded to
//! 8-bit PCM here and encoded to OGG Vorbis in this process (`ogg`, libvorbis through `vorbis_rs`; no
//! `oggenc` or `ffmpeg` is needed). `MUSIC` and `INTROSND` hold XMI (Extended MIDI) songs; they are
//! written unchanged as `.xmi`, because turning them into audio needs a synth (`tools/xmi-play`, the
//! `f4x-music` program). The MT-32 timbre banks (`FAT*`) are copied beside them.
//! What plays when is `docs/sound.md`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::lbx::Lbx;

/// Every entry of the sound archives starts with 16 bytes of header (`0xDEAF`, the type, zeros).
const ENTRY_HEADER: usize = 16;

/// The effect archives, in the order they are written. Entries without a name get their number.
const EFFECT_FILES: [&str; 4] = ["SOUNDFX", "CMBTSND", "NEWSOUND", "INTROSFX"];
/// The music archives: XMI songs.
const MUSIC_FILES: [&str; 2] = ["MUSIC", "INTROSND"];
/// The files of the DOS sound system that the MT-32 rendering needs (timbre banks and catalogues).
const TIMBRE_FILES: [&str; 8] = [
    "FAT.MT",
    "FAT.CAT",
    "FAT.AD",
    "FAT.OPL",
    "FAT-GEN.CAT",
    "FAT-OPL3.BNK",
    "FATV10.BNK",
    "SNDDRV.LBX",
];

/// `MUSIC.LBX` entry names, from ReMoM's `enum MUSIC_Tracks`.
const MUSIC_NAMES: [&str; 116] = [
    "good_arcane",
    "good_chaos",
    "good_death",
    "good_life",
    "good_nature",
    "good_sorcery",
    "bad_arcane",
    "bad_chaos",
    "bad_death",
    "bad_life",
    "bad_nature",
    "bad_sorcery",
    "som_started",
    "weak_summon",
    "avg_summon",
    "powerful_summon",
    "mighty_summon",
    "spell_failure",
    "upkeep_warning",
    "eternal_night",
    "evil_omens",
    "zombie_mastery",
    "aura_of_majesty",
    "wind_mastery",
    "suppress_magic",
    "time_stop",
    "nature_awareness",
    "force_of_nature",
    "herb_mastery",
    "chaos_surge",
    "doom_mastery",
    "great_wasting",
    "meteor_storm",
    "armageddon",
    "tranquility",
    "life_force",
    "crusade",
    "just_cause",
    "holy_arms",
    "wrath_of_god",
    "new_spell",
    "merlin_good",
    "merlin_bad",
    "raven_good",
    "raven_bad",
    "sharee_good",
    "sharee_bad",
    "lopan_good",
    "lopan_bad",
    "jafar_good",
    "jafar_bad",
    "oberic_good",
    "oberic_bad",
    "rjak_good",
    "rjak_bad",
    "sssra_good",
    "sssra_bad",
    "tauron_good",
    "tauron_bad",
    "freya_good",
    "freya_bad",
    "horus_good",
    "horus_bad",
    "ariel_good",
    "ariel_bad",
    "tlaloc_good",
    "tlaloc_bad",
    "kali_good",
    "kali_bad",
    "win_by_magic",
    "hiring_screen",
    "merlin_cmbt1",
    "merlin_cmbt2",
    "raven_cmbt1",
    "raven_cmbt2",
    "sharee_cmbt1",
    "sharee_cmbt2",
    "lopan_cmbt1",
    "lopan_cmbt2",
    "jafar_cmbt1",
    "jafar_cmbt2",
    "oberic_cmbt1",
    "oberic_cmbt2",
    "rjak_cmbt1",
    "rjak_cmbt2",
    "sssra_cmbt1",
    "sssra_cmbt2",
    "tauron_cmbt1",
    "tauron_cmbt2",
    "freya_cmbt1",
    "freya_cmbt2",
    "horus_cmbt1",
    "horus_cmbt2",
    "ariel_cmbt1",
    "ariel_cmbt2",
    "tlaloc_cmbt1",
    "tlaloc_cmbt2",
    "kali_cmbt1",
    "kali_cmbt2",
    "bkrgnd_good",
    "bkgrnd_neutral",
    "bkgrnd_negative",
    "combat_1",
    "combat_2",
    "main_menu",
    "site_discovery",
    "events",
    "new_outpost",
    "bldng_finished",
    "win_military",
    "lose_military",
    "lose_by_magic",
    "intro",
    "fanf_not_used",
    "bad_event",
    "fanfare",
];

const SOUNDFX_NAMES: [&str; 119] = [
    "std_btn_clk",
    "introt2",
    "left_btn_clk",
    "introt3",
    "introm",
    "troops",
    "cavalry",
    "flyer_s",
    "flyer_l",
    "monster_s",
    "monster_l",
    "teleport",
    "tunneling",
    "monster_m",
    "empty1",
    "atk_catapult",
    "atk_cannon",
    "atk_bows",
    "atk_slings",
    "atk_lightning",
    "atk_fire",
    "atk_illusion",
    "atk_death",
    "atk_ice",
    "atk_holy",
    "atk_drow",
    "atk_sprite",
    "atk_lightningbolt",
    "gen_nature",
    "gen_sorcery",
    "gen_chaos",
    "gen_life",
    "gen_death",
    "gen_arcane",
    "femalescream",
    "malescream",
    "malescream2",
    "empty2",
    "empty3",
    "starfires",
    "planeshift",
    "raisedead",
    "dispelevil",
    "holyword",
    "streamoflife",
    "inspirations",
    "prosperity",
    "astralgate",
    "altarofbattle",
    "consecration",
    "weakness",
    "blacksleep",
    "mindtwist",
    "possession",
    "warpnode",
    "blackwind",
    "drainpower",
    "subversion",
    "cruelunminding",
    "animatedead",
    "darkrituals",
    "evilpresence",
    "cursedlands",
    "pestilence",
    "cloudofshadow",
    "famine",
    "wallofdarkness",
    "petrify",
    "web",
    "earth2mud",
    "icestorm",
    "crackscall",
    "earthquake",
    "transmute",
    "natureseye",
    "earthgate",
    "gaiasblessing",
    "dispelmagic",
    "wordofrecall",
    "vertigo",
    "disenchanttrue",
    "spellblast",
    "whirlwind",
    "banish",
    "confusion",
    "stasis",
    "enchantroad",
    "mindstorm",
    "creaturebinding",
    "flyingfortress",
    "spellward",
    "disrupt",
    "shatter",
    "fireball",
    "warpcreature",
    "warpwood",
    "firestorm",
    "warplightning",
    "raisevolcano",
    "disintegrate",
    "doombolt",
    "callchaos",
    "callthevoid",
    "corruption",
    "walloffire",
    "chaosrift",
    "silence",
    "combatsummon",
    "changeterrain",
    "movefortress",
    "earthlore",
    "wallofstone",
    "disjunction",
    "spellbinding",
    "resurrection",
    "introt1",
    "unknown116",
    "unknown117",
    "unknown118",
];

/// What is known of a VOC clip.
#[derive(Debug, PartialEq)]
pub struct Clip {
    pub rate: u32,
    /// Unsigned 8-bit mono samples, as the file holds them.
    pub samples: Vec<u8>,
}

/// Decodes a Creative Voice File: sound blocks (1, 2) joined, silence (3) filled in, a repeat (6, 7)
/// played the stated number of times. Only the 8-bit unsigned PCM codec exists in these files.
pub fn parse_voc(data: &[u8]) -> Result<Clip, String> {
    const MAGIC: &[u8] = b"Creative Voice File\x1a";
    if !data.starts_with(MAGIC) || data.len() < 26 {
        return Err("not a Creative Voice File".into());
    }
    let mut pos = usize::from(u16::from_le_bytes([data[20], data[21]]));
    let mut rate = 0u32;
    let mut samples: Vec<u8> = Vec::new();
    let mut repeat: Option<(usize, u32)> = None; // start of the repeated blocks, plays left
    let mut extended_rate: Option<u32> = None;
    while pos < data.len() {
        let kind = data[pos];
        if kind == 0 {
            break;
        }
        if pos + 4 > data.len() {
            return Err("truncated block header".into());
        }
        let size = usize::from(data[pos + 1])
            | usize::from(data[pos + 2]) << 8
            | usize::from(data[pos + 3]) << 16;
        let body = data
            .get(pos + 4..pos + 4 + size)
            .ok_or("block runs past the end of the file")?;
        let mut next = pos + 4 + size;
        match kind {
            1 => {
                if body.len() < 2 {
                    return Err("short sound block".into());
                }
                if body[1] != 0 {
                    return Err(format!("unsupported codec {}", body[1]));
                }
                let this = extended_rate
                    .take()
                    .unwrap_or(1_000_000 / (256 - u32::from(body[0])));
                if rate != 0 && this != rate {
                    return Err("the rate changes inside a clip".into());
                }
                rate = this;
                samples.extend_from_slice(&body[2..]);
            }
            2 => samples.extend_from_slice(body),
            3 => {
                if body.len() < 3 {
                    return Err("short silence block".into());
                }
                let n = usize::from(u16::from_le_bytes([body[0], body[1]])) + 1;
                if rate == 0 {
                    rate = 1_000_000 / (256 - u32::from(body[2]));
                }
                samples.resize(samples.len() + n, 128);
            }
            4 | 5 => {}
            6 => {
                if body.len() < 2 {
                    return Err("short repeat block".into());
                }
                let count = u32::from(u16::from_le_bytes([body[0], body[1]]));
                // 0xFFFF means "forever": play once.
                repeat = Some((next, if count == 0xFFFF { 0 } else { count }));
            }
            7 => {
                if let Some((start, left)) = repeat {
                    if left > 0 {
                        repeat = Some((start, left - 1));
                        next = start;
                    } else {
                        repeat = None;
                    }
                }
            }
            8 => {
                if body.len() < 4 {
                    return Err("short extended block".into());
                }
                let div = u32::from(u16::from_le_bytes([body[0], body[1]]));
                let channels = u32::from(body[3]) + 1;
                if channels != 1 {
                    return Err("stereo extended block".into());
                }
                extended_rate = Some(256_000_000 / (65_536 - div) / channels);
            }
            other => return Err(format!("unknown block type {other}")),
        }
        pos = next;
    }
    if rate == 0 || samples.is_empty() {
        return Err("no sound data".into());
    }
    Ok(Clip { rate, samples })
}

/// A RIFF/WAVE file holding the clip as unsigned 8-bit mono PCM.
pub fn wav_bytes(clip: &Clip) -> Vec<u8> {
    let len = clip.samples.len() as u32;
    let mut out = Vec::with_capacity(44 + clip.samples.len());
    out.extend(b"RIFF");
    out.extend((36 + len).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes()); // integer PCM
    out.extend(1u16.to_le_bytes()); // mono
    out.extend(clip.rate.to_le_bytes());
    out.extend(clip.rate.to_le_bytes()); // bytes per second: one byte per frame
    out.extend(1u16.to_le_bytes());
    out.extend(8u16.to_le_bytes());
    out.extend(b"data");
    out.extend(len.to_le_bytes());
    out.extend(&clip.samples);
    out
}

/// The game's file named `stem` + `.LBX`, in any letter case.
fn find_file(game_dir: &Path, name: &str) -> Option<PathBuf> {
    fs::read_dir(game_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
}

fn entry_name(stem: &str, i: usize) -> String {
    match stem {
        "SOUNDFX" => format!("{i:03}_{}", SOUNDFX_NAMES.get(i).copied().unwrap_or("")),
        "MUSIC" => format!("{i:03}_{}", MUSIC_NAMES.get(i).copied().unwrap_or("")),
        _ => format!("{i:03}"),
    }
    .trim_end_matches('_')
    .to_string()
}

/// The effect archives the game plays (`docs/sound.md`); `INTROSFX` is not read by the game.
pub const PLAYED_EFFECTS: [&str; 3] = ["SOUNDFX", "CMBTSND", "NEWSOUND"];

/// What an extraction step did.
#[derive(Debug, Default)]
pub struct Report {
    /// Files written.
    pub written: usize,
    /// What was left out and why, one line each (an archive that is not there, an entry that is
    /// not a sound).
    pub skipped: Vec<String>,
}

/// The bytes of the archive `stem`, or `None` when the game has no such file.
fn read_archive(game_dir: &Path, stem: &str) -> Result<Option<Vec<u8>>, String> {
    let Some(path) = find_file(game_dir, &format!("{stem}.LBX")) else {
        return Ok(None);
    };
    fs::read(&path)
        .map(Some)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Decode the effects of the archives `stems` and write each as `<out_dir>/<archive>/<entry>.ogg`.
/// `index` gets a line for each (the dev listing `index.tsv`), when given.
pub fn extract_effects(
    game_dir: &Path,
    out_dir: &Path,
    stems: &[&str],
    mut index: Option<&mut String>,
) -> Result<Report, String> {
    let mut report = Report::default();
    for stem in stems {
        let Some(data) = read_archive(game_dir, stem)? else {
            report.skipped.push(format!("{stem}.LBX: not found"));
            continue;
        };
        let lbx = Lbx::parse(&data).map_err(|e| format!("{stem}.LBX: {e}"))?;
        let dir = out_dir.join(stem.to_lowercase());
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        for i in 0..lbx.len() {
            let voc = lbx.entry(i).get(ENTRY_HEADER..).unwrap_or_default();
            let clip = match parse_voc(voc) {
                Ok(c) => c,
                Err(e) => {
                    report.skipped.push(format!("{stem}.LBX {i}: {e}"));
                    continue;
                }
            };
            let file = format!("{}.ogg", entry_name(stem, i));
            crate::ogg::encode_mono_u8(clip.rate, &clip.samples, &dir.join(&file))
                .map_err(|e| format!("{stem}.LBX {i}: {e}"))?;
            if let Some(index) = index.as_deref_mut() {
                let seconds = clip.samples.len() as f32 / clip.rate as f32;
                *index += &format!(
                    "{}/{file}\t{stem}\t{i}\togg\t{}\t{seconds:.2}\n",
                    stem.to_lowercase(),
                    clip.rate
                );
            }
            report.written += 1;
        }
    }
    Ok(report)
}

/// Write the XMI songs of the archive `stem` unchanged as `<dir>/<number>_<name>.xmi`.
/// `index` gets a line for each (the dev listing), when given.
pub fn extract_songs(
    game_dir: &Path,
    stem: &str,
    dir: &Path,
    mut index: Option<&mut String>,
) -> Result<Report, String> {
    let mut report = Report::default();
    let Some(data) = read_archive(game_dir, stem)? else {
        report.skipped.push(format!("{stem}.LBX: not found"));
        return Ok(report);
    };
    let lbx = Lbx::parse(&data).map_err(|e| format!("{stem}.LBX: {e}"))?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for i in 0..lbx.len() {
        let xmi = lbx.entry(i).get(ENTRY_HEADER..).unwrap_or_default();
        if !xmi.starts_with(b"FORM") || xmi.get(8..16) != Some(b"XDIRINFO".as_slice()) {
            report
                .skipped
                .push(format!("{stem}.LBX {i}: not an XMI file"));
            continue;
        }
        let file = format!("{}.xmi", entry_name(stem, i));
        fs::write(dir.join(&file), xmi).map_err(|e| e.to_string())?;
        if let Some(index) = index.as_deref_mut() {
            *index += &format!("{}/{file}\t{stem}\t{i}\txmi\t\t\n", stem.to_lowercase());
        }
        report.written += 1;
    }
    Ok(report)
}

/// Copy the timbre files of the DOS sound system (`FAT*`) that the MT-32 rendering reads into `dir`
/// (lower-case names). Returns how many were there.
pub fn extract_timbres(game_dir: &Path, dir: &Path) -> Result<usize, String> {
    let mut banks = 0usize;
    for name in TIMBRE_FILES {
        if let Some(path) = find_file(game_dir, name) {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            fs::copy(&path, dir.join(name.to_lowercase())).map_err(|e| e.to_string())?;
            banks += 1;
        }
    }
    Ok(banks)
}

/// The development extraction: every effect archive as OGG, every song as XMI and the timbre
/// files, with the listing `index.tsv` (`f4x-extract audio`).
pub fn extract(game_dir: &Path, out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let mut index = String::from("file\tarchive\tentry\tformat\trate\tseconds\n");
    let effects = extract_effects(game_dir, out_dir, &EFFECT_FILES, Some(&mut index))?;
    let mut songs = Report::default();
    for stem in MUSIC_FILES {
        let report = extract_songs(
            game_dir,
            stem,
            &out_dir.join(stem.to_lowercase()),
            Some(&mut index),
        )?;
        songs.written += report.written;
        songs.skipped.extend(report.skipped);
    }
    let banks = extract_timbres(game_dir, &out_dir.join("timbres"))?;
    fs::write(out_dir.join("index.tsv"), index).map_err(|e| e.to_string())?;
    println!(
        "{} effects (ogg), {} songs (xmi), {banks} timbre files -> {}",
        effects.written,
        songs.written,
        out_dir.display()
    );
    for skipped in effects.skipped.iter().chain(&songs.skipped) {
        eprintln!("skipped {skipped}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestResult;

    fn voc(blocks: &[u8]) -> Vec<u8> {
        let mut v = b"Creative Voice File\x1a".to_vec();
        v.extend([0x1a, 0, 0x0a, 0x01, 0x29, 0x11]);
        v.extend(blocks);
        v.push(0);
        v
    }

    #[test]
    fn a_sound_block_gives_the_rate_and_samples() -> TestResult {
        // divisor 156 -> 1_000_000 / 100 = 10_000 Hz
        let clip = parse_voc(&voc(&[1, 5, 0, 0, 156, 0, 1, 2, 3]))?;
        assert_eq!(
            clip,
            Clip {
                rate: 10_000,
                samples: vec![1, 2, 3]
            }
        );
        Ok(())
    }

    #[test]
    fn continuation_and_silence_blocks_are_joined() -> TestResult {
        let clip = parse_voc(&voc(&[
            1, 4, 0, 0, 156, 0, 7, 8, // sound
            3, 3, 0, 0, 1, 0, 156, // silence of two samples
            2, 1, 0, 0, 9, // continuation
        ]))?;
        assert_eq!(clip.samples, vec![7, 8, 128, 128, 9]);
        Ok(())
    }

    #[test]
    fn a_repeat_plays_its_blocks_again() -> TestResult {
        let clip = parse_voc(&voc(&[
            1, 3, 0, 0, 156, 0, 5, // sound 5
            6, 2, 0, 0, 1, 0, // repeat the next blocks once more
            2, 1, 0, 0, 6, // continuation 6
            7, 0, 0, 0, // end repeat
        ]))?;
        assert_eq!(clip.samples, vec![5, 6, 6]);
        Ok(())
    }

    #[test]
    fn bad_files_are_refused() {
        assert!(parse_voc(b"nothing").is_err());
        assert!(parse_voc(&voc(&[1, 3, 0, 0, 156, 1, 5])).is_err()); // ADPCM codec
        assert!(parse_voc(&voc(&[])).is_err()); // no sound
    }

    #[test]
    fn the_wav_header_matches_the_clip() -> TestResult {
        let wav = wav_bytes(&Clip {
            rate: 8000,
            samples: vec![1, 2, 3],
        });
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into()?), 39);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into()?), 8000);
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(&wav[44..], &[1, 2, 3]);
        Ok(())
    }

    #[test]
    fn the_name_tables_cover_the_archives() {
        assert_eq!(MUSIC_NAMES.len(), 116);
        assert_eq!(SOUNDFX_NAMES.len(), 119);
        assert_eq!(entry_name("SOUNDFX", 2), "002_left_btn_clk");
        assert_eq!(entry_name("CMBTSND", 7), "007");
    }
}
