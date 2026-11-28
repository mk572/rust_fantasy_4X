//! `f4x-tools soak`: let AI wizards play whole games against each other, with no window, and
//! report how each game went. Used to find AI and rules bugs (content balance, `docs/soak-baseline.md`).
//! `game` plays one game and is also what `soak-versus` uses.
//!
//! Tests: below for the option reading and the parallel runner; whole games are run by hand
//! (`f4x-tools soak`), not by `cargo test`, because they take minutes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use f4x_core::player::PlayerId;
use f4x_core::world::World;
use f4x_core::world_event::EventKind;
use f4x_def::content::Content;
use f4x_newgame::setup::{new_game, NewGame};
use serde::{Deserialize, Serialize};

use crate::contestant::{self, Contestant, Version};
use crate::metrics::{
    casts_line, failure_lines, spell_table, unmet_line, Metrics, Standing, Tally,
};
use crate::report::{self, Played};
use f4x_master::{Game, GameOver};

pub const USAGE: &str = "usage: f4x-tools soak [--seeds FIRST..LAST] [--turns N] [--players 2..5] [--content DIR] [--every N] [--trace TURNS] [--detail] [--events] [--grant-mastery TURN] [--refused-log FILE | --no-refused-log] [--snapshots-every N] [--out DIR] [--jobs N] [--from SAVE] [--map WxH] [--land PERCENT] [--magic 0..2] [--difficulty 0..4] [--rules 1.31|1.6|mk] [--mastery all|human|off] [--return all|human|off] [--start-units settler,spirit,trireme]";

/// The options `soak` and `soak-versus` share.
pub struct Options {
    pub first: u64,
    pub last: u64,
    pub turns: u32,
    pub players: u8,
    pub every: u32,
    pub trace: u32,
    pub detail: bool,
    /// Print how the random events went in each game.
    pub events: bool,
    /// Print how often each spell was cast, and the map spells nobody cast.
    pub grant_mastery: Option<u32>,
    pub refused_log: Option<PathBuf>,
    /// `--no-refused-log`: write no refused-order log (the default is a file in `out`).
    pub no_refused_log: bool,
    pub dir: PathBuf,
    /// Save the game every this many turns into `out` (none: not at all).
    pub snapshots_every: Option<u32>,
    pub out: Option<PathBuf>,
    /// Games played at once (none: one for each core; one when the game prints as it goes).
    pub jobs: Option<usize>,
    /// Play the reference-only games again instead of reusing the kept ones.
    pub no_cache: bool,
    /// Go on from this saved game (a snapshot of an earlier soak, or any save) instead of
    /// starting a new one: one game, with the save's seed and its AIs' plans.
    pub from: Option<PathBuf>,
    /// Map size, land share, node strength and difficulty (none: the game's defaults).
    pub map: Option<(u16, u16)>,
    pub land: Option<u8>,
    pub magic: Option<u8>,
    pub difficulty: Option<u8>,
    /// Who may research and cast the Spell of Mastery, and who may cast the Spell of Return
    /// (none: the rule set's). The games have no human wizard, so "human" is nobody.
    pub mastery: Option<f4x_def::settings::SpellAccess>,
    pub spell_return: Option<f4x_def::settings::SpellAccess>,
    /// The rules the games are played with; 1.31 unless asked otherwise.
    pub rules: f4x_def::settings::RuleSet,
    /// The units every wizard starts with on top of the race's army; none unless asked.
    pub starting_units: f4x_def::settings::StartingUnits,
}

/// A comma-separated list of `settler`, `spirit` and `trireme`.
fn start_units(text: &str) -> Result<f4x_def::settings::StartingUnits, String> {
    let mut units = f4x_def::settings::StartingUnits::default();
    for word in text.split(',') {
        match word {
            "settler" => units.settler = true,
            "spirit" => units.magic_spirit = true,
            "trireme" => units.trireme = true,
            other => return Err(format!("{other:?} is not settler, spirit or trireme")),
        }
    }
    Ok(units)
}

/// `all`, `human` or `off`: who a special spell is open to.
fn spell_access(text: &str) -> Result<f4x_def::settings::SpellAccess, String> {
    use f4x_def::settings::SpellAccess;
    match text {
        "all" => Ok(SpellAccess::All),
        "human" => Ok(SpellAccess::HumanOnly),
        "off" => Ok(SpellAccess::Off),
        other => Err(format!("{other:?} is not all, human or off")),
    }
}

impl Options {
    /// Read the shared options; `usage` is shown after an unknown one.
    pub fn parse(args: &[String], usage: &str) -> Result<Self, String> {
        let mut o = Options {
            first: 1,
            last: 3,
            turns: 200,
            players: 2,
            every: 0,
            trace: 0,
            detail: false,
            events: false,
            grant_mastery: None,
            refused_log: None,
            no_refused_log: false,
            dir: f4x_def::SHIPPED_CONTENT.into(),
            snapshots_every: None,
            out: None,
            jobs: None,
            no_cache: false,
            from: None,
            map: None,
            land: None,
            magic: None,
            difficulty: None,
            mastery: None,
            spell_return: None,
            rules: Default::default(),
            starting_units: Default::default(),
        };
        let mut it = args.iter();
        while let Some(a) = it.next() {
            let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
            match a.as_str() {
                "--seeds" => {
                    let v = value("--seeds")?;
                    let (a, b) = v.split_once("..").unwrap_or((&v, &v));
                    o.first = a.parse().map_err(|_| "bad seed range")?;
                    o.last = b.parse().map_err(|_| "bad seed range")?;
                }
                "--turns" => o.turns = value("--turns")?.parse().map_err(|_| "bad turns")?,
                "--players" => {
                    o.players = value("--players")?.parse().map_err(|_| "bad players")?
                }
                "--detail" => o.detail = true,
                "--events" => o.events = true,
                "--grant-mastery" => {
                    o.grant_mastery =
                        Some(value("--grant-mastery")?.parse().map_err(|_| "bad turn")?)
                }
                "--trace" => o.trace = value("--trace")?.parse().map_err(|_| "bad turns")?,
                "--every" => o.every = value("--every")?.parse().map_err(|_| "bad interval")?,
                "--content" => o.dir = value("--content")?.into(),
                "--refused-log" => o.refused_log = Some(value("--refused-log")?.into()),
                "--no-refused-log" => o.no_refused_log = true,
                "--snapshots-every" => {
                    o.snapshots_every = Some(
                        value("--snapshots-every")?
                            .parse()
                            .map_err(|_| "bad interval")?,
                    )
                }
                "--out" => o.out = Some(value("--out")?.into()),
                "--jobs" => o.jobs = Some(value("--jobs")?.parse().map_err(|_| "bad jobs")?),
                "--no-cache" => o.no_cache = true,
                "--from" => o.from = Some(value("--from")?.into()),
                "--map" => {
                    let v = value("--map")?;
                    let (w, h) = v.split_once('x').ok_or("--map takes WIDTHxHEIGHT")?;
                    o.map = Some((
                        w.parse().map_err(|_| "bad map width")?,
                        h.parse().map_err(|_| "bad map height")?,
                    ));
                }
                "--land" => o.land = Some(value("--land")?.parse().map_err(|_| "bad land")?),
                "--magic" => o.magic = Some(value("--magic")?.parse().map_err(|_| "bad magic")?),
                "--rules" => {
                    o.rules = match value("--rules")?.as_str() {
                        "1.31" => f4x_def::settings::RuleSet::V131,
                        "1.6" => f4x_def::settings::RuleSet::V160,
                        "mk" => f4x_def::settings::RuleSet::MkMod,
                        other => {
                            return Err(format!("{other:?} is not a rule set: use 1.31, 1.6 or mk"))
                        }
                    }
                }
                "--difficulty" => {
                    o.difficulty = Some(
                        value("--difficulty")?
                            .parse()
                            .map_err(|_| "bad difficulty")?,
                    )
                }
                "--mastery" => o.mastery = Some(spell_access(&value("--mastery")?)?),
                "--return" => o.spell_return = Some(spell_access(&value("--return")?)?),
                "--start-units" => o.starting_units = start_units(&value("--start-units")?)?,
                other => return Err(format!("unknown option {other:?}\n{usage}")),
            }
        }
        if let Some(path) = &o.from {
            // The game is the save's: its seed is the one the reports, files and cache name.
            if args.iter().any(|a| a == "--seeds") {
                return Err("--from takes the seed from the save; leave out --seeds".into());
            }
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let saved = f4x_master::SaveFile::from_text(&text)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            (o.first, o.last) = (saved.game.rng.seed(), saved.game.rng.seed());
        }
        Ok(o)
    }

    /// How many games to play at once. Games that print as they go (`--every`, `--trace`,
    /// `--detail`) are played one at a time so their lines stay together.
    pub fn jobs(&self) -> usize {
        if self.every > 0 || self.trace > 0 || self.detail {
            return 1;
        }
        self.jobs
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
            .max(1)
    }

    pub fn load_content(&self) -> Result<Content, String> {
        f4x_def::load::load_content_layers(&[&self.dir, Path::new(f4x_def::SHIPPED_CONTENT)])
            .map_err(|e| e.to_string())
    }
}

/// Who plays which wizard, and by which version the neutral cities and monsters play.
pub struct Roster {
    pub versions: Vec<Version>,
    pub neutrals: Version,
}

/// The refused-order log, one line per refused command, tab separated.
pub type RefusedLog = Mutex<Option<std::io::BufWriter<std::fs::File>>>;

/// The refused-order log of a run: the file of `--refused-log`, none for `--no-refused-log`,
/// else `refused-<seconds since 1970>.tsv` in the output folder (`soak-out`), so a soak always
/// leaves the data of its refusals and an earlier log is never overwritten. The path is told on stderr.
pub fn refused_log_path(opts: &Options) -> Option<PathBuf> {
    if opts.no_refused_log {
        return None;
    }
    let path = opts.refused_log.clone().unwrap_or_else(|| {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        opts.out
            .clone()
            .unwrap_or_else(|| "soak-out".into())
            .join(format!("refused-{secs}.tsv"))
    });
    eprintln!("refused-order log: {}", path.display());
    Some(path)
}

pub fn open_refused_log(path: &Option<PathBuf>) -> Result<RefusedLog, String> {
    let Some(path) = path else {
        return Ok(Mutex::new(None));
    };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut f = std::io::BufWriter::new(
        std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?,
    );
    writeln!(
        f,
        "seed\tturn\tplayer\tsource\treason\tcommand\tstood\tdetail"
    )
    .map_err(|e| e.to_string())?;
    Ok(Mutex::new(Some(f)))
}

/// One line of the refused-order log. The AI gives the facts of a failure after its reason,
/// split by `f4x_ai::DETAIL_SEPARATOR`; they go to the last column.
fn refused_line(seed: u64, r: &contestant::Refused, stood: u32) -> String {
    let (reason, detail) = r
        .reason
        .split_once(f4x_ai::DETAIL_SEPARATOR)
        .unwrap_or((&r.reason, ""));
    format!(
        "{seed}\t{}\t{}\t{}\t{reason}\t{}\t{stood}\t{detail}",
        r.turn, r.player.0, r.source, r.command
    )
}

/// For how many turns in a row the same order has been refused: the same wizard sending the same
/// command from the same line, in the turns one after the other. The last column of the refused
/// log; 1 is the first refusal, and a command refused every turn for a hundred turns reads 100.
#[derive(Default)]
struct Stood {
    /// By wizard, source and command: the first turn of the run and the last turn it was refused.
    runs: std::collections::HashMap<(u8, String, String), (u32, u32)>,
}

impl Stood {
    fn count(&mut self, r: &contestant::Refused) -> u32 {
        let run = self
            .runs
            .entry((r.player.0, r.source.clone(), r.command.clone()))
            .or_insert((r.turn, r.turn));
        if r.turn > run.1 + 1 {
            *run = (r.turn, r.turn);
        }
        run.1 = run.1.max(r.turn);
        r.turn - run.0 + 1
    }
}

/// How one game went.
#[derive(Serialize, Deserialize)]
pub struct GameResult {
    pub over: Option<GameOver>,
    pub tally: Tally,
    /// Each wizard's standing at the end, by player number (neutrals included).
    pub standing: Vec<(PlayerId, Standing)>,
    /// A fingerprint of the saved game at each snapshot turn.
    pub fingerprints: Vec<(u32, u64)>,
    /// The lines the game printed when it ended (the same for every kind of soak), kept so that a
    /// game taken from the cache prints them as a played one does.
    #[serde(default)]
    pub log: Vec<String>,
}

/// The lines of one game's report, tagged with the seed (and the variant). A game that prints as
/// it goes (`--every`, `--trace`, `--detail`) prints each line at once; any other game prints
/// them together when it ends, so that games played side by side do not interleave.
struct Log {
    tag: String,
    live: bool,
    lines: Vec<String>,
}

impl Log {
    fn line(&mut self, text: String) {
        let line = format!("{}: {text}", self.tag);
        if self.live {
            println!("{line}");
        }
        self.lines.push(line);
    }

    fn finish(self) -> Vec<String> {
        if !self.live {
            let mut out = std::io::stdout().lock();
            for line in &self.lines {
                let _ = writeln!(out, "{line}");
            }
        }
        self.lines
    }
}

/// `log.line(format!(..))` without the `format!`.
macro_rules! logln {
    ($log:expr, $($arg:tt)*) => {
        $log.line(format!($($arg)*))
    };
}

/// A panic names the seed, turn and player whose turn it happened in, and the last order the AI
/// sent (which holds the unit), before the usual report (task 668).
fn name_panics() {
    let usual = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = contestant::playing();
        if !at.is_empty() {
            let order = f4x_ai::last_order().unwrap_or_default();
            eprintln!("panic in {at}; last order sent: {order}");
        }
        usual(info);
    }));
}

pub fn run(args: &[String]) -> Result<(), String> {
    let opts = Options::parse(args, USAGE)?;
    name_panics();
    let content = opts.load_content()?;
    let refused = open_refused_log(&refused_log_path(&opts))?;
    let roster = Roster {
        versions: vec![Version::New; usize::from(opts.players)],
        neutrals: Version::New,
    };
    let seeds: Vec<u64> = (opts.first..=opts.last).collect();
    let results = in_parallel(opts.jobs(), &seeds, |&seed| {
        game(&content, seed, &opts, &roster, "", &refused)
    })?;
    let played: Vec<Played> = seeds
        .iter()
        .zip(&results)
        .map(|(&seed, result)| Played {
            seed,
            label: "",
            versions: &roster.versions,
            result,
        })
        .collect();
    report::print(&opts, &content, "Soak", &played);
    Ok(())
}

/// Run `work` on every item, `jobs` at a time, and return the results in the order of the items
/// (the first error, if any). Each game builds its own `World` on its thread, so nothing of the
/// game itself crosses threads.
pub fn in_parallel<T: Sync, R: Send>(
    jobs: usize,
    items: &[T],
    work: impl Fn(&T) -> Result<R, String> + Sync,
) -> Result<Vec<R>, String> {
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<Option<Result<R, String>>>> =
        Mutex::new(items.iter().map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..jobs.min(items.len()) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(i) else {
                    break;
                };
                let result = work(item);
                done.lock().unwrap_or_else(PoisonError::into_inner)[i] = Some(result);
            });
        }
    });
    done.into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .into_iter()
        .map(|r| r.expect("every item was worked on"))
        .collect()
}

/// Play one game to its end or the turn limit, printing as the options ask, and report it.
/// `label` names the variant in the output ("" for a plain soak game).
pub fn game(
    content: &Content,
    seed: u64,
    opts: &Options,
    roster: &Roster,
    label: &str,
    refused_out: &RefusedLog,
) -> Result<GameResult, String> {
    let players = opts.players;
    let (mut game, saved_plans) = match &opts.from {
        Some(path) => {
            let check = f4x_core::world::save::ContentCheck::Strict;
            let loaded = f4x_master::load_game(path, content.clone(), check)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if loaded.integrity != f4x_core::world::save::Integrity::Intact {
                eprintln!("{}: was edited after it was saved", path.display());
            }
            let seated = loaded
                .game
                .player_ids()
                .into_iter()
                .filter(|&p| loaded.game.is_wizard(p))
                .count();
            if seated != roster.versions.len() {
                return Err(format!(
                    "{} has {seated} wizards; use --players {seated}",
                    path.display()
                ));
            }
            (loaded.game, loaded.plans)
        }
        None => {
            let mut spec = NewGame {
                seed,
                all_ai: true,
                ..NewGame::with_wizards(players)
            };
            if let Some(level) = opts.difficulty {
                let opponents = spec.settings.basic.opponents;
                spec.settings =
                    f4x_def::settings::GameSettings::for_difficulty(&content.settings, level);
                spec.settings.basic.opponents = opponents;
            }
            spec.settings = opts.rules.applied(spec.settings, &content.settings);
            spec.settings.starting_units = opts.starting_units;
            if let Some(access) = opts.mastery {
                spec.settings.rules.spell_of_mastery = access;
            }
            if let Some(access) = opts.spell_return {
                spec.settings.rules.spell_of_return = access;
            }
            let basic = &mut spec.settings.basic;
            if let Some((w, h)) = opts.map {
                (basic.map_width, basic.map_height) = (w, h);
            }
            if let Some(land) = opts.land {
                basic.land_percent = land;
            }
            if let Some(magic) = opts.magic {
                basic.magic_strength = magic;
            }
            {
                let (world, rng) = new_game(content, &spec)?;
                (Game::new(world, rng), Vec::new())
            }
        }
    };
    // A game that was loaded has its own seed: the one asked for here is only the default range.
    let seed = game.seed();
    let tag = if label.is_empty() {
        format!("seed {seed}")
    } else {
        format!("seed {seed} [{label}]")
    };
    let mut ais: Vec<Box<dyn Contestant>> = roster
        .versions
        .iter()
        .enumerate()
        .map(|(k, &v)| contestant::make(v, PlayerId(k as u8)))
        .collect();
    let mut neutrals = contestant::Neutrals::new(roster.neutrals);
    // The wizards' memories of their dealings, shared for answering a proposal (task 760).
    let diplomats = f4x_ai::Diplomats::new();
    for ai in &mut ais {
        ai.join(&diplomats);
    }
    neutrals.share(&diplomats);
    neutrals.recall(&game, &saved_plans);
    neutrals.record_refusals(
        refused_out
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some(),
    );
    for ai in &mut ais {
        // A game that was saved carries what its AIs had in mind.
        ai.recall(&saved_plans);
        ai.record_refusals(
            refused_out
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_some(),
        );
    }
    let snapshot_dir = match (&opts.out, opts.snapshots_every) {
        (Some(out), Some(n)) if n > 0 => {
            let dir = out.join(if label.is_empty() {
                format!("seed{seed}")
            } else {
                format!("seed{seed}-{label}")
            });
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            Some(dir)
        }
        _ => None,
    };
    let started = Instant::now();
    let mut last_line = started;
    let mut tally = Tally::new(&game);
    let mut stood = Stood::default();
    let mut log = Log {
        tag: tag.clone(),
        live: opts.every > 0 || opts.trace > 0 || opts.detail,
        lines: Vec::new(),
    };
    for line in profile_lines(&game) {
        logln!(log, "{line}");
    }
    // Rounds that ended with a battle still open (nothing may be: every battle is fought out
    // inside the round), and the first such turn.
    let (mut open_rounds, mut first_open) = (0usize, None::<u32>);
    let mut fingerprints = Vec::new();
    let mut watch = EventWatch::default();
    // The time of each phase since the last `--every` line, in order: one per wizard, the
    // neutrals, the rest (turn changes, other players' turn ends).
    let mut phases = vec![std::time::Duration::ZERO; usize::from(players) + 2];
    while game.round() <= opts.turns && game.game_over().is_none() {
        if opts.grant_mastery == Some(game.round()) {
            grant_mastery_to_all(game.world_mut());
        }
        let (turns, rest, times) = contestant::play_round_timed(&mut game, &mut ais, &mut neutrals);
        for (player, d) in &times.players {
            if let Some(slot) = phases.get_mut(usize::from(player.0)) {
                *slot += *d;
            }
        }
        phases[usize::from(players)] += times.neutrals;
        phases[usize::from(players) + 1] += times.rest();
        for (player, events) in &turns {
            tally.record(&game, game.round(), Some(*player), events);
        }
        tally.record(&game, game.round(), None, &rest);
        tally.sample_mana(&game, game.round());
        watch.observe(&game);
        if game.in_battle() {
            open_rounds += 1;
            first_open.get_or_insert(game.round());
        }
        if game.round() <= opts.trace && opts.trace > 0 {
            trace_turn(&game, game.round(), &ais, &turns, &rest);
        }
        if let Some(out) = refused_out
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            let all = ais
                .iter()
                .flat_map(|ai| ai.take_refusals())
                .chain(neutrals.take_refusals());
            {
                for r in all {
                    tally.note_refused_cast(&game, &r.command, &r.reason);
                    let stood = stood.count(&r);
                    writeln!(out, "{}", refused_line(seed, &r, stood))
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        if opts.every > 0 && game.round().is_multiple_of(opts.every) {
            // The time since the last line shows where a game slows down.
            let now = Instant::now();
            let per_turn = now.duration_since(last_line).as_secs_f32() * 1000.0 / opts.every as f32;
            last_line = now;
            println!(
                "{tag} turn {}: {} ({} entities) [{:.1}s, {per_turn:.0} ms a turn]",
                game.round(),
                status(&game),
                game.entity_count(),
                started.elapsed().as_secs_f32(),
            );
            let ms = |d: &std::time::Duration| d.as_secs_f32() * 1000.0 / opts.every as f32;
            let names = (0..players)
                .map(|k| format!("p{k}"))
                .chain(["neutrals".to_string(), "rest".to_string()]);
            println!(
                "    ms a turn by phase: {}",
                names
                    .zip(&phases)
                    .map(|(n, d)| format!("{n} {:.0}", ms(d)))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            phases
                .iter_mut()
                .for_each(|d| *d = std::time::Duration::ZERO);
            if opts.detail {
                for line in detail_lines(&game, game.round(), &ais) {
                    println!("    {line}");
                }
            }
        }
        if let Some(n) = opts.snapshots_every.filter(|&n| n > 0) {
            if game.round().is_multiple_of(n) {
                // The plans of the AIs go into the save, so it is a full game save.
                let plans: Vec<_> = ais
                    .iter()
                    .filter_map(|ai| Some((ai.player(), ai.plans()?)))
                    .chain(neutrals.plans())
                    .collect();
                fingerprints.push((
                    game.round(),
                    save_snapshot(&game, plans, snapshot_dir.as_deref())?,
                ));
            }
        }
    }
    if opts.detail {
        city_lines(&game);
    }
    let outcome = match game.game_over() {
        Some(o) => format!(
            "won by player {} ({:?}) on turn {}",
            o.winner.0, o.kind, o.turn
        ),
        None => format!("no winner after {} turns", game.round() - 1),
    };
    let open = match first_open {
        Some(turn) => {
            format!(", {open_rounds} rounds ended with a battle open (first on turn {turn})")
        }
        None => String::new(),
    };
    let total = tally.total();
    let seconds = started.elapsed().as_secs_f32();
    logln!(log,
        "{outcome}; {}; {} battles, {} cities taken, {} wars declared, {} wizards met, {} diplomatic actions started, {} refused orders ({} by a treaty){open}; {seconds:.1}s",
        status(&game),
        total.battles_won,
        total.cities_captured,
        total.wars_declared,
        total.contacts_made,
        total.diplomacy_started,
        total.refused_orders,
        total.treaty_refusals,
    );
    logln!(log, "diplomacy: {}", diplomacy_line(&total));
    logln!(
        log,
        "planar: {}",
        total
            .rows()
            .into_iter()
            .filter(|(label, _)| {
                [
                    "plane crossings",
                    "plane shifts",
                    "sites cleared",
                    "towers cleared",
                ]
                .contains(label)
            })
            .map(|(label, n)| format!("{label} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    for line in magic_lines(&game) {
        logln!(log, "{line}");
    }
    for line in empire_lines(&game) {
        logln!(log, "{line}");
    }
    if game.game_over().is_none() {
        for line in scout_lines(&game) {
            logln!(log, "{line}");
        }
        for line in reach_lines(&game) {
            logln!(log, "{line}");
        }
    }
    logln!(log, "defeats: {}", defeat_line(&game, &tally));
    logln!(log, "far captures: {}", far_capture_line(&tally));
    for line in undefended_lines(&game, &tally) {
        logln!(log, "{line}");
    }
    logln!(log, "wars: {}", war_line(&tally));
    for (p, place, count) in tally.worst_places(3) {
        logln!(
            log,
            "attacks lost on a neutral city or lair: p{p} {count} at {place}"
        );
    }
    for line in unit_lines(&game) {
        logln!(log, "{line}");
    }
    // Every counter of every wizard, one line each, for the report script (`scripts/soak_report.py`).
    for p in game.player_ids() {
        if game.wizard(p).is_none() {
            continue;
        }
        logln!(
            log,
            "stats p{}: {}",
            p.0,
            tally
                .of(p)
                .rows()
                .into_iter()
                .map(|(label, n)| format!("{label} = {n}"))
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    // The same for each race's neutral player, without what only a wizard has. The prefix keeps
    // them apart from the wizards for the report script.
    // (a race whose cities are all gone has no player any more, but its counts stay)
    let races = game.content().rules.races.iter().count();
    for race in 0..races {
        let race = f4x_core::rules::RaceId::new(race as u16);
        let p = f4x_core::player::race_player::race_player_id(race);
        if tally.of(p) == Default::default() {
            continue;
        }
        logln!(
            log,
            "neutral stats p{} ({}): {}",
            p.0,
            game.content().rules.races.get(race).id,
            tally
                .of(p)
                .neutral_rows()
                .into_iter()
                .map(|(label, n)| format!("{label} = {n}"))
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    logln!(
        log,
        "production rushed with gold (items, gold): {}",
        game.player_ids()
            .into_iter()
            .filter(|&p| game.wizard(p).is_some())
            .map(|p| {
                let m = tally.of(p);
                format!("p{} {} ({}g)", p.0, m.production_rushed, m.gold_rushed)
            })
            .collect::<Vec<_>>()
            .join(", ")
    );
    logln!(
        log,
        "research stalled turns: {}",
        game.player_ids()
            .into_iter()
            .filter(|&p| game.wizard(p).is_some())
            .map(|p| format!("p{} {}", p.0, tally.of(p).research_stalled))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let dropped: Vec<String> = ais
        .iter()
        .map(|ai| {
            let kinds = ai.plans_dropped();
            let total: u32 = kinds.iter().map(|(_, n)| n).sum();
            let by_kind = kinds
                .iter()
                .filter(|(_, n)| *n > 0)
                .map(|(k, n)| format!("{k} {n}"))
                .collect::<Vec<_>>()
                .join(", ");
            if by_kind.is_empty() {
                format!("p{} {total}", ai.player().0)
            } else {
                format!("p{} {total} ({by_kind})", ai.player().0)
            }
        })
        .collect();
    logln!(
        log,
        "plans dropped by the time limit: {}",
        dropped.join(", ")
    );
    // Why settlers failed: the AI's own reasons for ending a plan, and the settlers the game took
    // from it (killed, let go for lack of food or gold).
    let failed: Vec<String> = ais
        .iter()
        .map(|ai| {
            let mut reasons: Vec<(String, u32)> = ai
                .settler_failures()
                .into_iter()
                .map(|(k, n)| (k.to_string(), n))
                .collect();
            let lost = tally.of(ai.player());
            reasons.push(("lost in battle".into(), lost.settlers_killed));
            reasons.push(("let go for lack of food".into(), lost.settlers_starved));
            reasons.push(("let go for lack of gold".into(), lost.settlers_unpaid));
            let by_reason = reasons
                .iter()
                .filter(|(_, n)| *n > 0)
                .map(|(k, n)| format!("{k} {n}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "p{} {}",
                ai.player().0,
                if by_reason.is_empty() {
                    "none".into()
                } else {
                    by_reason
                }
            )
        })
        .collect();
    logln!(log, "settlers that failed: {}", failed.join("; "));
    for ai in &ais {
        tally.note_unmet(&ai.unmet_spell_wants());
    }
    logln!(
        log,
        "wanted a spell it did not know (kind of spell, turns): {}",
        unmet_line(tally.unmet_wants())
    );
    tally.note_known(&game);
    logln!(log, "mana {}", mana_line(&game, &tally));
    for line in neutral_lines(&game, &tally) {
        logln!(log, "{line}");
    }
    logln!(log, "casts: {}", casts_line(tally.casts()));
    logln!(log, "battle casts: {}", casts_line(tally.battle_casts()));
    logln!(
        log,
        "battle mana by spell: {}",
        casts_line(
            &tally
                .battle_mana()
                .iter()
                .map(|(k, v)| (k.clone(), (*v).max(0) as u32))
                .collect()
        )
    );
    logln!(log, "spells known (wizards): {}", casts_line(tally.known()));
    logln!(
        log,
        "spell table (wizards that could research, learned, knew, cast; casts; per knower):\n{}",
        spell_table(&game.content().rules, tally.spell_rows())
    );
    logln!(
        log,
        "failed casts (spell, count: reason):\n{}",
        failure_lines(tally.cast_failures())
    );
    if opts.events {
        logln!(log, "{}", watch.summary(game.round().saturating_sub(1)));
    }
    let standing = game
        .player_ids()
        .into_iter()
        .filter(|&p| game.is_wizard(p))
        .map(|p| (p, Standing::of(&game, p, game.round())))
        .collect();
    Ok(GameResult {
        over: game.game_over(),
        tally,
        standing,
        fingerprints,
        log: log.finish(),
    })
}

/// What the random events did over a game (`--events`). Only the table is seen: the lasting events
/// by kind and how long each ran, and the draws by their number, because a one-off event leaves
/// nothing in the table and an all-AI game is told of none.
#[derive(Default)]
struct EventWatch {
    last_draw: u32,
    draws: u32,
    running: Vec<(EventKind, u32)>,
    ended: Vec<(EventKind, u32)>,
}

impl EventWatch {
    /// Look at the table after a round.
    fn observe(&mut self, world: &World) {
        let events = world.events();
        if events.last_event() != self.last_draw {
            self.last_draw = events.last_event();
            self.draws += 1;
        }
        let now: Vec<(EventKind, u32)> = events.ongoing().map(|l| (l.kind, l.turns)).collect();
        for (kind, turns) in std::mem::take(&mut self.running) {
            if !now.iter().any(|(k, _)| *k == kind) {
                self.ended.push((kind, turns + 1));
            }
        }
        self.running = now;
    }

    /// One line: the draws, and for each lasting kind the number of runs and their lengths.
    fn summary(&self, turns: u32) -> String {
        let mut kinds: Vec<EventKind> = self.ended.iter().map(|(k, _)| *k).collect();
        kinds.extend(self.running.iter().map(|(k, _)| *k));
        kinds.sort();
        kinds.dedup();
        let lasting: Vec<String> = kinds
            .into_iter()
            .map(|kind| {
                let runs: Vec<u32> = self
                    .ended
                    .iter()
                    .filter(|(k, _)| *k == kind)
                    .map(|(_, t)| *t)
                    .collect();
                let open = self.running.iter().filter(|(k, _)| *k == kind).count();
                let longest = runs.iter().max().copied().unwrap_or(0);
                let mean = runs.iter().sum::<u32>() as f32 / runs.len().max(1) as f32;
                format!(
                    "{} {}x (mean {mean:.1}, longest {longest} turns, {open} still on)",
                    kind.id(),
                    runs.len()
                )
            })
            .collect();
        let per = if self.draws == 0 {
            "none".to_string()
        } else {
            format!("one in {:.1} turns", turns as f32 / self.draws as f32)
        };
        format!(
            "events: {} drawn in {turns} turns ({per}); lasting: {}",
            self.draws,
            if lasting.is_empty() {
                "none".to_string()
            } else {
                lasting.join(", ")
            }
        )
    }
}

/// Fingerprint the saved game (without its checksum line) and, when `dir` is given, write it as
/// `tNNNN.f4x`.
fn save_snapshot(game: &Game, plans: f4x_master::Plans, dir: Option<&Path>) -> Result<u64, String> {
    let text = game.save_file(plans).to_text().map_err(|e| e.to_string())?;
    let mut hasher = DefaultHasher::new();
    text.split_once('\n')
        .map_or(&*text, |(_, body)| body)
        .hash(&mut hasher);
    if let Some(dir) = dir {
        let path = dir.join(format!("t{:04}.f4x", game.round()));
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(hasher.finish())
}

fn trace_turn(
    world: &World,
    round: u32,
    ais: &[Box<dyn Contestant>],
    turns: &[(PlayerId, Vec<f4x_core::world::Event>)],
    rest: &[f4x_core::world::Event],
) {
    for ai in ais {
        println!("  [t{}] ai {} {}", round, ai.player().0, ai.describe(world));
    }
    for (c, city, _) in world.economy().cities() {
        let prod = world.economy().city_production(c);
        let item = prod.item.map(|i| f4x_console::describe::item(world, i));
        let eco = world.economy().city_economy(c);
        println!(
            "  [t{}] {} (p{}): {:?} {}/{} eco {:?}",
            round,
            city.name,
            city.owner.0,
            item,
            prod.progress,
            prod.item.map_or(0, |i| world.economy().item_cost(c, i)),
            eco.map(|e| (e.figures, e.farmers, e.production, e.food))
        );
    }
    for e in turns.iter().flat_map(|(_, events)| events).chain(rest) {
        println!("  [t{}] {}", round, f4x_console::describe::event(world, e));
    }
}

/// Each city of a wizard with who has explored it (the soak's `--detail` lines).
fn city_lines(world: &World) {
    for (c, city, pos) in world.economy().cities() {
        if world.is_neutral_player(city.owner) {
            continue;
        }
        let seen_by: Vec<String> = world
            .player_ids()
            .into_iter()
            .filter(|&p| p != city.owner && world.is_explored(p, pos))
            .map(|p| format!("p{}", p.0))
            .collect();
        println!(
            "    city {} of p{} at {:?} {},{} size {}: explored by [{}]",
            city.name,
            city.owner.0,
            pos.plane,
            pos.x,
            pos.y,
            world.economy().city_economy(c).map_or(0, |e| e.figures),
            seen_by.join(" ")
        );
    }
}

/// For a game with no winner: what each wizard with cities could do about each rival with cities, so a
/// stall reads at a glance. `p2>p0 War: sees 4 of 13 cities, rival on Myrror 13, own Arcanus 33`.
fn reach_lines(world: &World) -> Vec<String> {
    let cities: Vec<(PlayerId, f4x_core::map::Position)> = world
        .economy()
        .cities()
        .into_iter()
        .filter(|(_, city, _)| !world.is_neutral_player(city.owner))
        .map(|(_, city, pos)| (city.owner, pos))
        .collect();
    let planes = |who: PlayerId| {
        let mut names: Vec<String> = Vec::new();
        for plane in [f4x_core::map::Plane::Arcanus, f4x_core::map::Plane::Myrror] {
            let n = cities
                .iter()
                .filter(|(o, p)| *o == who && p.plane == plane)
                .count();
            if n > 0 {
                names.push(format!("{plane:?} {n}"));
            }
        }
        names.join(" ")
    };
    let owners: Vec<PlayerId> = world
        .player_ids()
        .into_iter()
        .filter(|&p| cities.iter().any(|(o, _)| *o == p))
        .collect();
    let mut lines = Vec::new();
    for &a in &owners {
        for &b in &owners {
            if a == b {
                continue;
            }
            let theirs: Vec<_> = cities.iter().filter(|(o, _)| *o == b).collect();
            let seen = theirs
                .iter()
                .filter(|(_, p)| world.is_explored(a, *p))
                .count();
            lines.push(format!(
                "reach: p{}>p{} {:?}: sees {} of {} cities (rival {}; own {})",
                a.0,
                b.0,
                world.treaty(a, b),
                seen,
                theirs.len(),
                planes(b),
                planes(a)
            ));
        }
    }
    lines
}

/// Per wizard with cities: how much of each plane it has explored, how many of its units stand on
/// each, and its towers (seen, guarded, open and whether any of its units could walk to the open
/// ones). Why a wizard does or does not find a rival on the other plane (tasks 435, 502).
fn scout_lines(world: &World) -> Vec<String> {
    use f4x_core::map::Plane;
    use f4x_core::player::view::PlayerView;
    let mut lines = Vec::new();
    for p in world.player_ids() {
        if world.wizard(p).is_none() || world.is_neutral_player(p) {
            continue;
        }
        let view = PlayerView::new(world, p);
        if view.my_cities().is_empty() {
            continue;
        }
        let units = view.my_units();
        let on = |plane: Plane| {
            units
                .iter()
                .filter(|&&u| view.unit_position(u).is_some_and(|at| at.plane == plane))
                .count()
        };
        // One unit from each square it stands on, at most a dozen, to ask whether a tower can
        // be walked to.
        let mut walkers = Vec::new();
        let mut squares = Vec::new();
        for &u in &units {
            if let Some(at) = view.unit_position(u) {
                if !squares.contains(&at) && walkers.len() < 12 {
                    squares.push(at);
                    walkers.push(u);
                }
            }
        }
        let towers = view.towers();
        let open: Vec<_> = towers.iter().filter(|(_, intact)| !intact).collect();
        let reachable = open
            .iter()
            .filter(|(at, _)| walkers.iter().any(|&u| view.can_reach(&[u], *at)))
            .count();
        let named: Vec<String> = open
            .iter()
            .map(|(at, _)| format!("{:?} {},{}", at.plane, at.x, at.y))
            .collect();
        lines.push(format!(
            "scouting p{}: explored Arcanus {:.0}% Myrror {:.0}%; units on Arcanus {} Myrror {}; towers seen {} ({} guarded, {} open, {} of those reachable) [{}]",
            p.0,
            100.0 * view.explored_share(Plane::Arcanus),
            100.0 * view.explored_share(Plane::Myrror),
            on(Plane::Arcanus),
            on(Plane::Myrror),
            towers.len(),
            towers.len() - open.len(),
            open.len(),
            reachable,
            named.join(", ")
        ));
    }
    lines
}

/// Per wizard with cities: the power before the split, the mana the split gives, the upkeep of its
/// lasting spells and creatures, the pool, and how many of its cities hold each of the four
/// buildings that give mana (shrine, temple, parthenon, cathedral). Where the mana goes.
fn magic_lines(world: &World) -> Vec<String> {
    let rules = &world.content().rules;
    let ids: Vec<_> = ["shrine", "temple", "parthenon", "cathedral"]
        .iter()
        .filter_map(|name| {
            rules
                .buildings
                .iter()
                .find(|(_, d)| d.id == *name)
                .map(|(id, _)| id)
        })
        .collect();
    let mut lines = Vec::new();
    for p in world.player_ids() {
        if world.wizard(p).is_none() || world.is_neutral_player(p) {
            continue;
        }
        let cities: Vec<_> = world
            .economy()
            .cities_of(p)
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        if cities.is_empty() {
            continue;
        }
        let income = world.income(p);
        let built: Vec<String> = ids
            .iter()
            .map(|&b| {
                cities
                    .iter()
                    .filter(|&&c| world.economy().city_has_building(c, b))
                    .count()
                    .to_string()
            })
            .collect();
        lines.push(format!(
            "magic: p{} power {} mana income {} upkeep {} pool {}; of {} cities shrines/temples/parthenons/cathedrals {}",
            p.0,
            income.power_base.as_f32(),
            income.split.mana.as_f32(),
            income.mana_upkeep.as_f32(),
            f4x_core::player::view::PlayerView::new(world, p)
                .treasury()
                .mana
                .as_f32(),
            cities.len(),
            built.join("/")
        ));
    }
    lines
}

/// Per wizard with cities: how many cities, the average population of a city (figures) and the
/// average and total number of buildings (every building a city has, replaced ones included).
fn empire_lines(world: &World) -> Vec<String> {
    let rules = &world.content().rules;
    let all: Vec<_> = rules.buildings.iter().map(|(id, _)| id).collect();
    let mut lines = Vec::new();
    for p in world.player_ids() {
        if world.wizard(p).is_none() || world.is_neutral_player(p) {
            continue;
        }
        let cities: Vec<_> = world
            .economy()
            .cities_of(p)
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        if cities.is_empty() {
            continue;
        }
        let figures: u32 = cities
            .iter()
            .map(|&c| world.economy().city_economy(c).map_or(0, |e| e.figures))
            .sum();
        let buildings: usize = cities
            .iter()
            .map(|&c| {
                all.iter()
                    .filter(|&&b| world.economy().city_has_building(c, b))
                    .count()
            })
            .sum();
        let n = cities.len() as f32;
        lines.push(format!(
            "empire: p{} {} cities, average population {:.1}, buildings {} ({:.1} a city)",
            p.0,
            cities.len(),
            figures as f32 / n,
            buildings,
            buildings as f32 / n
        ));
    }
    lines
}

/// Mana spent over the game and left at the end, per wizard, e.g. `p0 spent 5200 left 80`.
/// One line per wizard about what was rolled and picked for it: the race of its capital, the plane
/// it started on, its personality and objective, its books and its retorts.
pub(crate) fn profile_lines(world: &World) -> Vec<String> {
    let rules = &world.content().rules;
    world.player_ids()
        .into_iter()
        .filter(|&p| world.is_wizard(p))
        .filter_map(|p| {
            let w = world.wizard(p)?;
            let race = w
                .capital_race()
                .map_or("none".to_string(), |r| rules.races.get(r).id.clone());
            let books = w
                .books()
                .iter()
                .filter(|(_, &n)| n > 0)
                .map(|(&r, n)| format!("{} {n}", rules.realms.get(r).id))
                .collect::<Vec<_>>()
                .join(", ");
            let retorts = w
                .retorts()
                .iter()
                .map(|&r| rules.retorts.get(r).id.clone())
                .collect::<Vec<_>>()
                .join(", ");
            let plane = w
                .fortress()
                .map_or("none".to_string(), |at| format!("{:?}", at.plane));
            Some(format!(
                "profile p{}: race {race}; started on {plane}; personality {:?}; objective {}; books [{books}]; retorts [{retorts}]",
                p.0,
                w.personality(),
                w.objective().map_or("none".to_string(), |o| format!("{o:?}")),
            ))
        })
        .collect()
}

/// One line for the neutral players (the races' cities and the monsters): what they hold at the end
/// and what happened to them over the game. The monsters' slot also takes whatever has no number.
fn neutral_lines(world: &World, tally: &Tally) -> Vec<String> {
    let rules = &world.content().rules;
    let mut ids: Vec<PlayerId> = world
        .player_ids()
        .into_iter()
        .filter(|&p| world.is_neutral_player(p))
        .collect();
    ids.push(f4x_core::player::MONSTERS);
    let parts: Vec<String> = ids
        .into_iter()
        .map(|p| {
            let name = if p == f4x_core::player::MONSTERS {
                "monsters".to_string()
            } else {
                world.race_of_player(p)
                    .map_or(format!("p{}", p.0), |r| rules.races.get(r).id.clone())
            };
            let m = tally.of(p);
            let cities = world
                .economy()
                .cities()
                .iter()
                .filter(|(_, c, _)| c.owner == p)
                .count();
            let units = world.units().player_units(p).len();
            format!(
                "{name} {cities}c/{units}u (founded {}, lost {}, taken {}, battles won {} lost {}, units lost in battle {} outside {}, city-turns undefended {})",
                m.cities_founded,
                m.cities_lost,
                m.cities_captured,
                m.battles_won,
                m.battles_lost,
                m.units_lost_in_battle,
                m.units_lost,
                m.undefended_cities
            )
        })
        .collect();
    vec![format!("neutrals: {}", parts.join("; "))]
}

/// What each wizard fields at the end: the units by kind, most numerous first, with the share of
/// spearmen and the units a city holds on average, e.g. `units p0: 120 in 14 cities (8.6 a city),
/// spearmen 54% [high_men_spearmen 60, high_men_swordsmen 20, ...]`.
fn unit_lines(world: &World) -> Vec<String> {
    let mut lines = Vec::new();
    for p in world.player_ids() {
        if world.wizard(p).is_none() || world.is_neutral_player(p) {
            continue;
        }
        let units = world.units().player_units(p);
        if units.is_empty() {
            continue;
        }
        let mut kinds: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for &u in &units {
            if let Some(d) = world.units().unit_def(u) {
                *kinds.entry(d.id.clone()).or_default() += 1;
            }
        }
        let spearmen: usize = kinds
            .iter()
            .filter(|(k, _)| k.ends_with("spearmen"))
            .map(|(_, n)| n)
            .sum();
        let cities = world.economy().cities_of(p).len().max(1);
        let mut list: Vec<(String, usize)> = kinds.into_iter().collect();
        list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        lines.push(format!(
            "units p{}: {} in {} cities ({:.1} a city), spearmen {}% [{}]",
            p.0,
            units.len(),
            cities,
            units.len() as f32 / cities as f32,
            100 * spearmen / units.len(),
            list.iter()
                .map(|(k, n)| format!("{k} {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    lines
}

/// The cities taken on the other plane than the taker started on, per taker and former owner:
/// `p0 took 3 of p2 on Myrror (first turn 540)`; `none` when no city was. Task 435's evidence: a
/// wizard that crosses by a tower to a rival's cities on the other plane.
fn far_capture_line(tally: &Tally) -> String {
    // (taker, former owner, plane) -> (cities, turn of the first)
    let mut groups: std::collections::BTreeMap<(u8, u8, usize), (u32, u32)> = Default::default();
    for &(turn, taker, from, plane) in tally.far_captures() {
        groups.entry((taker, from, plane)).or_insert((0, turn)).0 += 1;
    }
    if groups.is_empty() {
        return "none".to_string();
    }
    groups
        .iter()
        .map(|(&(taker, from, plane), &(n, first))| {
            let name = if plane == 0 { "Arcanus" } else { "Myrror" };
            format!("p{taker} took {n} of p{from} on {name} (first turn {first})")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The cities (outposts too) that held no unit but engineers, settlers and mana spirits: one line
/// for each run of turns in a row, `undefended: p1 Arcanus (12, 7) turns 30-45 (16)`; `undefended:
/// none` when there was none.
fn undefended_lines(world: &World, tally: &Tally) -> Vec<String> {
    use std::collections::BTreeMap;
    let mut by_city: BTreeMap<(u8, usize, u32, u32), Vec<u32>> = BTreeMap::new();
    for &(turn, owner, at) in tally.undefended() {
        by_city
            .entry((owner, at.plane.index(), at.y, at.x))
            .or_default()
            .push(turn);
    }
    if by_city.is_empty() {
        return vec!["undefended: none".to_string()];
    }
    let mut lines = Vec::new();
    for ((owner, plane, y, x), turns) in by_city {
        let name = if plane == 0 { "Arcanus" } else { "Myrror" };
        let mut runs: Vec<(u32, u32)> = Vec::new();
        for t in turns {
            match runs.last_mut() {
                Some((_, end)) if t <= *end + 1 => *end = t,
                _ => runs.push((t, t)),
            }
        }
        for (first, last) in runs {
            // A neutral player is a race: its name follows the number.
            let who = world
                .race_of_player(PlayerId(owner))
                .map_or(format!("p{owner}"), |r| {
                    format!("p{owner} ({})", world.content().rules.races.get(r).id)
                });
            lines.push(format!(
                "undefended: {who} {name} ({x}, {y}) turns {first}-{last} ({})",
                last - first + 1
            ));
        }
    }
    lines
}

/// The wars declared, `p1>p3@88` for the turn: who declared on whom. `none` without a war.
fn war_line(tally: &Tally) -> String {
    let parts: Vec<String> = tally
        .wars()
        .iter()
        .map(|&(turn, from, to)| format!("p{from}>p{to}@{turn}"))
        .collect();
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(" ")
    }
}

/// Who defeated each wizard that fell: `p3 by p1 (turn 340)` for a rival wizard, `p0 by neutral
/// high_men (turn 120)` for an independent race's cities, `by monsters`, or `by nobody` when it lost
/// its last city without anyone taking it. `none` when no wizard fell.
fn defeat_line(world: &World, tally: &Tally) -> String {
    let rules = &world.content().rules;
    let parts: Vec<String> = tally
        .defeats()
        .iter()
        .map(|&(turn, loser, killer)| {
            let who = match killer.map(PlayerId) {
                None => "nobody".to_string(),
                Some(k) if k == f4x_core::player::MONSTERS => "monsters".to_string(),
                Some(k) if world.is_neutral_player(k) => format!(
                    "neutral {}",
                    world.race_of_player(k).map_or("?".to_string(), |r| rules
                        .races
                        .get(r)
                        .id
                        .clone())
                ),
                Some(k) => format!("p{}", k.0),
            };
            format!("p{loser} by {who} (turn {turn})")
        })
        .collect();
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(", ")
    }
}

fn mana_line(world: &World, tally: &Tally) -> String {
    let (spent, left) = tally.mana();
    world
        .player_ids()
        .into_iter()
        .filter(|&p| world.wizard(p).is_some())
        .map(|p| {
            let k = usize::from(p.0);
            format!(
                "p{} spent {} left {}",
                p.0,
                spent.get(k).copied().unwrap_or(0),
                left.get(k).copied().unwrap_or(0)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Objective and temper, cities, units and gold of each wizard, e.g. `p0[Mil/Agg] 5c(1o)/14u/80g` (`o`: outposts, counted in the cities).
pub(crate) fn status(world: &World) -> String {
    world
        .player_ids()
        .into_iter()
        .filter(|id| world.is_wizard(*id))
        .map(|id| {
            let cities = world
                .economy()
                .cities()
                .iter()
                .filter(|(_, c, _)| c.owner == id)
                .count();
            // Outposts: cities with no citizen yet (they are counted in the cities).
            let outposts = world
                .economy()
                .cities()
                .iter()
                .filter(|(e, c, _)| c.owner == id && world.city_population(*e).figures == 0)
                .count();
            let units = world.units().player_units(id).len();
            let tag = if world.player(id).is_some_and(|p| p.is_defeated()) {
                " (defeated)"
            } else {
                ""
            };
            let temper = world.wizard(id).map_or(String::new(), |w| {
                let short = |s: String| s.chars().take(3).collect::<String>();
                format!(
                    "[{}/{}]",
                    w.objective()
                        .map_or("-".into(), |o| short(format!("{o:?}"))),
                    short(format!("{:?}", w.personality()))
                )
            });
            format!(
                "p{}{temper} {cities}c({outposts}o)/{units}u/{}g{tag}",
                id.0,
                world.player(id).map_or(0, |p| p.treasury().gold.floor())
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// One line per wizard about magic and exploring: mana, spells known, research, the spell being
/// cast and how much of the map is explored.
fn detail_lines(world: &World, round: u32, ais: &[Box<dyn Contestant>]) -> Vec<String> {
    let mut out = Vec::new();
    for id in world
        .player_ids()
        .into_iter()
        .filter(|id| world.is_wizard(*id))
    {
        let Some(m) = world.wizard(id) else {
            continue;
        };
        let name = |s: f4x_def::rules::SpellId| world.content().rules.spells.get(s).id.clone();
        let explored = world.explored_squares(id);
        let income = world.income(id);
        let mut known: Vec<String> = m.known().iter().map(|&s| name(s)).collect();
        known.sort();
        out.push(format!("  known: {}", known.join(" ")));
        out.push(format!(
            "p{}: skill {}/{}, gold {} (cities {}, buildings -{}, units -{}, food {}), food {}, mana {} (+{} upkeep {}), spells {}, researching {} ({} left), casting {}, split {}/{}/{}, explored {}{}",
            id.0,
            m.skill_left(),
            m.casting_skill(&world.content().rules),
            world.budget(id).net_gold(),
            world.budget(id).city_gold,
            world.budget(id).building_upkeep,
            world.budget(id).unit_upkeep,
            world.budget(id).food_gold,
            world.budget(id).food_surplus,
            world.player(id).map_or(0, |p| p.treasury().mana.floor()),
            income.split.mana,
            income.mana_upkeep,
            m.known().len(),
            m.researching().map_or("-".into(), name),
            m.research_remaining(),
            m.casting()
                .as_ref()
                .map_or("-".into(), |c| format!("{} ({} left)", name(c.spell), c.remaining)),
            m.power_split().mana,
            m.power_split().research,
            m.power_split().skill,
            explored,
            ais.iter()
                .find(|a| a.player() == id)
                .map_or(String::new(), |a| a.describe(world))
        ));
        if let Some(a) = ais.iter().find(|a| a.player() == id) {
            let note = a.diplomacy_note(world, round);
            if !note.is_empty() {
                out.push(format!("  p{}: {note}", id.0));
            }
        }
    }
    out
}

/// Give every wizard the Spell of Mastery with the mana and skill to cast it, so a game shows
/// whether the AI casts it and wins without waiting for the 60,000 research points.
fn grant_mastery_to_all(world: &mut World) {
    let Some(spell) = world.content().rules.spells.id_numbered(213)
    /* spell_of_mastery */
    else {
        return;
    };
    let ids: Vec<PlayerId> = world
        .player_ids()
        .into_iter()
        .filter(|id| world.is_wizard(*id))
        .collect();
    for id in ids {
        if let Some(t) = world.treasury_mut(id) {
            t.mana += 6000;
        }
        if let Some(mut w) = world.wizard_mut(id) {
            let mut spells = w.get_mut::<f4x_core::wizard::sheet::KnownSpells>();
            spells.known.insert(spell);
            spells.researchable.remove(&spell);
            drop(spells);
            w.get_mut::<f4x_core::wizard::sheet::Skill>().points += 2000;
        }
    }
}

/// The kinds of diplomatic action of a game, each with its count (all wizards together).
fn diplomacy_line(total: &Metrics) -> String {
    total
        .rows()
        .into_iter()
        .skip_while(|(label, _)| *label != "diplomacy started")
        .take(DIPLOMACY_ROWS)
        .map(|(label, n)| format!("{label} {n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// "diplomacy started" and the counts of each kind that follow it in `Metrics::rows`.
const DIPLOMACY_ROWS: usize = 24;

#[cfg(test)]
mod tests {
    #[test]
    fn a_refusal_repeated_every_turn_counts_the_turns_it_has_stood_and_a_gap_starts_it_again() {
        let refused = |turn, command: &str| contestant::Refused {
            turn,
            player: PlayerId(1),
            source: "x.rs:1".into(),
            command: command.into(),
            reason: "StackFull".into(),
        };
        let mut stood = Stood::default();
        let counts: Vec<u32> = [4, 4, 5, 6, 7, 20, 21]
            .iter()
            .map(|&t| stood.count(&refused(t, "go")))
            .collect();
        assert_eq!(counts, [1, 1, 2, 3, 4, 1, 2]);
        assert_eq!(stood.count(&refused(21, "another")), 1, "another command");
    }

    use super::*;
    use f4x_def::testdir::TestResult;

    #[test]
    fn the_facts_of_a_failure_go_to_their_own_column() {
        let refused = |reason: &str| contestant::Refused {
            turn: 9,
            player: PlayerId(2),
            source: "x.rs:1".into(),
            command: "GoTo".into(),
            reason: reason.into(),
        };
        let with = format!(
            "NoPath(Target(Water)){}to (3, 4): target is water",
            f4x_ai::DETAIL_SEPARATOR
        );
        assert_eq!(
            refused_line(131, &refused(&with), 3),
            "131\t9\t2\tx.rs:1\tNoPath(Target(Water))\tGoTo\t3\tto (3, 4): target is water"
        );
        assert_eq!(
            refused_line(131, &refused("StackFull"), 1),
            "131\t9\t2\tx.rs:1\tStackFull\tGoTo\t1\t"
        );
    }

    #[test]
    fn the_profile_and_neutral_lines_name_each_wizard_and_the_neutral_races() -> TestResult {
        let content = crate::testutil::shipped();
        let (world, _) = new_game(
            content,
            &NewGame {
                seed: 3,
                all_ai: true,
                ..NewGame::with_wizards(3)
            },
        )?;
        let profiles = profile_lines(&world);
        assert_eq!(profiles.len(), 3, "{profiles:?}");
        for (k, line) in profiles.iter().enumerate() {
            assert!(line.starts_with(&format!("profile p{k}:")), "{line}");
            for part in [
                "race ",
                "started on ",
                "personality ",
                "objective ",
                "books [",
                "retorts [",
            ] {
                assert!(line.contains(part), "{part}: {line}");
            }
            assert!(
                line.contains("started on Arcanus") || line.contains("started on Myrror"),
                "{line}"
            );
        }
        let tally = Tally::new(&world);
        let neutrals = neutral_lines(&world, &tally);
        assert_eq!(neutrals.len(), 1);
        assert!(neutrals[0].contains("monsters "), "{neutrals:?}");
        // Which races the independent cities have is the map's: some race is named.
        assert!(
            world
                .content()
                .rules
                .races
                .defs()
                .iter()
                .any(|r| neutrals[0].contains(&format!("{} ", r.id))),
            "{neutrals:?}"
        );
        Ok(())
    }

    fn parse(line: &[&str]) -> Result<Options, String> {
        let args: Vec<String> = line.iter().map(|s| s.to_string()).collect();
        Options::parse(&args, "usage")
    }

    #[test]
    fn the_rule_set_is_read_by_its_short_name() -> TestResult {
        use f4x_def::settings::RuleSet;
        assert_eq!(parse(&["--rules", "1.31"])?.rules, RuleSet::V131);
        assert_eq!(parse(&["--rules", "1.6"])?.rules, RuleSet::V160);
        assert_eq!(parse(&["--rules", "mk"])?.rules, RuleSet::MkMod);
        assert!(parse(&["--rules", "2.0"]).is_err());
        Ok(())
    }

    #[test]
    fn the_starting_units_are_read_from_a_list() -> TestResult {
        let none = parse(&[])?.starting_units;
        assert_eq!(none, Default::default());
        let two = parse(&["--start-units", "trireme,settler"])?.starting_units;
        assert!(two.trireme && two.settler && !two.magic_spirit);
        assert!(
            parse(&["--start-units", "spirit"])?
                .starting_units
                .magic_spirit
        );
        assert!(parse(&["--start-units", "galley"]).is_err());
        Ok(())
    }

    #[test]
    fn a_seed_range_and_a_single_seed_are_read() -> TestResult {
        let o = parse(&["--seeds", "4..9", "--turns", "50", "--players", "3"])?;
        assert_eq!((o.first, o.last, o.turns, o.players), (4, 9, 50, 3));
        let one = parse(&["--seeds", "7"])?;
        assert_eq!((one.first, one.last), (7, 7));
        Ok(())
    }

    #[test]
    fn bad_options_are_refused_with_the_reason() {
        assert_eq!(
            parse(&["--seeds", "a..b"]).err().as_deref(),
            Some("bad seed range")
        );
        assert_eq!(
            parse(&["--turns"]).err().as_deref(),
            Some("--turns needs a value")
        );
        let unknown = parse(&["--nope"]).err().unwrap_or_default();
        assert!(
            unknown.starts_with("unknown option \"--nope\""),
            "{unknown}"
        );
    }

    #[test]
    fn games_that_print_as_they_go_are_played_one_at_a_time() -> TestResult {
        for flag in ["--every", "--trace"] {
            assert_eq!(parse(&[flag, "5", "--jobs", "4"])?.jobs(), 1, "{flag}");
        }
        assert_eq!(parse(&["--detail", "--jobs", "4"])?.jobs(), 1);
        assert_eq!(parse(&["--jobs", "4"])?.jobs(), 4);
        assert_eq!(parse(&["--jobs", "0"])?.jobs(), 1, "never fewer than one");
        Ok(())
    }

    #[test]
    fn parallel_results_come_back_in_the_order_of_the_items() -> TestResult {
        let items: Vec<u32> = (0..20).collect();
        let doubled = in_parallel(4, &items, |&n| Ok(n * 2))?;
        assert_eq!(doubled, (0..20).map(|n| n * 2).collect::<Vec<_>>());
        Ok(())
    }

    #[test]
    fn a_failing_item_fails_the_run_with_its_error() {
        let items: Vec<u32> = (0..10).collect();
        let r = in_parallel(3, &items, |&n| {
            if n == 6 {
                Err(format!("item {n} failed"))
            } else {
                Ok(n)
            }
        });
        assert_eq!(r.err().as_deref(), Some("item 6 failed"));
    }

    #[test]
    fn a_soak_that_goes_on_from_a_snapshot_plays_the_same_game() -> TestResult {
        let content = f4x_def::testdir::shipped();
        let dir = std::env::temp_dir().join(format!("f4x-soak-from-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let roster = Roster {
            versions: vec![Version::New; 2],
            neutrals: Version::New,
        };
        let none: RefusedLog = Mutex::new(None);
        let mut opts = parse(&["--turns", "30", "--snapshots-every", "10"])?;
        opts.out = Some(dir.join("straight"));
        game(content, 7, &opts, &roster, "", &none)?;
        let mut again = parse(&["--turns", "30", "--snapshots-every", "10"])?;
        again.out = Some(dir.join("resumed"));
        again.from = Some(dir.join("straight/seed7/t0010.f4x"));
        game(content, 7, &again, &roster, "", &none)?;
        let line = |extra: &[&str]| {
            let from = dir.join("straight/seed7/t0010.f4x");
            let mut args = vec!["--from", from.to_str().unwrap_or_default()];
            args.extend_from_slice(extra);
            parse(&args)
        };
        let o = line(&[])?;
        assert_eq!((o.first, o.last), (7, 7), "the seed is the save's, not 1");
        assert!(line(&["--seeds", "3"]).is_err());
        let last = |d: &str| std::fs::read_to_string(dir.join(d).join("seed7/t0030.f4x"));
        assert_eq!(last("straight")?, last("resumed")?);
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
