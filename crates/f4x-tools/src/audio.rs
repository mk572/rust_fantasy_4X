//! `extract-audio`: the original's sound effects as OGG and its music as the XMI files it stores.
//!
//! `SOUNDFX`, `CMBTSND`, `NEWSOUND` and `INTROSFX` hold Creative Voice (VOC) clips; each is decoded to
//! 8-bit PCM here and encoded to OGG Vorbis by `oggenc` (vorbis-tools) or, failing that, `ffmpeg`. `MUSIC` and
//! `INTROSND` hold XMI (Extended MIDI) songs; they are written unchanged as `.xmi`, because turning them
//! into audio needs a synth. The MT-32 timbre banks (`FAT*`) are copied beside them.
//! What plays when is `docs/sound.md`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

/// How OGG Vorbis is written.
enum Encoder {
    /// `oggenc` (vorbis-tools): the reference libvorbis encoder.
    Oggenc,
    /// `ffmpeg` with these codec arguments.
    Ffmpeg(Vec<&'static str>),
}

fn runs(program: &str, arg: &str) -> bool {
    Command::new(program)
        .arg(arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// The best Vorbis encoder installed: `oggenc`, else `ffmpeg` built with libvorbis, else the
/// encoder `ffmpeg` has of its own (experimental, stereo only).
fn find_encoder() -> Result<Encoder, String> {
    if runs("oggenc", "--version") {
        return Ok(Encoder::Oggenc);
    }
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .map_err(|e| format!("neither oggenc nor ffmpeg was found on the PATH ({e}); install one, e.g. `brew install vorbis-tools`"))?;
    let list = String::from_utf8_lossy(&out.stdout);
    if list.contains(" libvorbis ") {
        Ok(Encoder::Ffmpeg(vec!["-c:a", "libvorbis", "-q:a", "5"]))
    } else if list.contains(" vorbis ") {
        Ok(Encoder::Ffmpeg(vec![
            "-c:a", "vorbis", "-strict", "-2", "-ac", "2", "-b:a", "96k",
        ]))
    } else {
        Err(
            "this ffmpeg has no Vorbis encoder; install vorbis-tools (`brew install vorbis-tools`)"
                .into(),
        )
    }
}

/// Encodes a WAV to the OGG file `dest`.
fn encode_ogg(wav: &[u8], dest: &Path, encoder: &Encoder) -> Result<(), String> {
    let mut command = match encoder {
        Encoder::Oggenc => {
            let mut c = Command::new("oggenc");
            c.args(["-Q", "-q", "5", "-o"]).arg(dest).arg("-");
            c
        }
        Encoder::Ffmpeg(args) => {
            let mut c = Command::new("ffmpeg");
            c.args(["-v", "error", "-y", "-f", "wav", "-i", "pipe:0"])
                .args(args)
                .arg(dest);
            c
        }
    };
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("encoder: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("the encoder has no stdin")?
        .write_all(wav)
        .map_err(|e| format!("encoder: {e}"))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("encoder: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "encoder: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
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

pub fn extract(game_dir: &Path, out_dir: &Path) -> Result<(), String> {
    let encoder = find_encoder()?;
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let mut index = String::from("file\tarchive\tentry\tformat\trate\tseconds\n");
    let (mut effects, mut songs) = (0usize, 0usize);
    let mut skipped = Vec::new();

    for stem in EFFECT_FILES {
        let Some(path) = find_file(game_dir, &format!("{stem}.LBX")) else {
            skipped.push(format!("{stem}.LBX: not found"));
            continue;
        };
        let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let lbx = Lbx::parse(&data).map_err(|e| format!("{stem}.LBX: {e}"))?;
        let dir = out_dir.join(stem.to_lowercase());
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        for i in 0..lbx.len() {
            let entry = lbx.entry(i);
            let voc = entry.get(ENTRY_HEADER..).unwrap_or_default();
            let clip = match parse_voc(voc) {
                Ok(c) => c,
                Err(e) => {
                    skipped.push(format!("{stem}.LBX {i}: {e}"));
                    continue;
                }
            };
            let file = format!("{}.ogg", entry_name(stem, i));
            encode_ogg(&wav_bytes(&clip), &dir.join(&file), &encoder)
                .map_err(|e| format!("{stem}.LBX {i}: {e}"))?;
            let seconds = clip.samples.len() as f32 / clip.rate as f32;
            index += &format!(
                "{}/{file}\t{stem}\t{i}\togg\t{}\t{seconds:.2}\n",
                stem.to_lowercase(),
                clip.rate
            );
            effects += 1;
        }
    }

    for stem in MUSIC_FILES {
        let Some(path) = find_file(game_dir, &format!("{stem}.LBX")) else {
            skipped.push(format!("{stem}.LBX: not found"));
            continue;
        };
        let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let lbx = Lbx::parse(&data).map_err(|e| format!("{stem}.LBX: {e}"))?;
        let dir = out_dir.join(stem.to_lowercase());
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        for i in 0..lbx.len() {
            let xmi = lbx.entry(i).get(ENTRY_HEADER..).unwrap_or_default();
            if !xmi.starts_with(b"FORM") || xmi.get(8..16) != Some(b"XDIRINFO".as_slice()) {
                skipped.push(format!("{stem}.LBX {i}: not an XMI file"));
                continue;
            }
            let file = format!("{}.xmi", entry_name(stem, i));
            fs::write(dir.join(&file), xmi).map_err(|e| e.to_string())?;
            index += &format!("{}/{file}\t{stem}\t{i}\txmi\t\t\n", stem.to_lowercase());
            songs += 1;
        }
    }

    let timbres = out_dir.join("timbres");
    let mut banks = 0usize;
    for name in TIMBRE_FILES {
        if let Some(path) = find_file(game_dir, name) {
            fs::create_dir_all(&timbres).map_err(|e| e.to_string())?;
            fs::copy(&path, timbres.join(name.to_lowercase())).map_err(|e| e.to_string())?;
            banks += 1;
        }
    }
    fs::write(out_dir.join("index.tsv"), index).map_err(|e| e.to_string())?;
    println!(
        "{effects} effects (ogg), {songs} songs (xmi), {banks} timbre files -> {}",
        out_dir.display()
    );
    for s in &skipped {
        eprintln!("skipped {s}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::testdir::TestResult;

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
