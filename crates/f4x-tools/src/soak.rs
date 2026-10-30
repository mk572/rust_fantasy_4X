//! `f4x-tools soak`: let AI wizards play whole games against each other, with no window, and
//! report how each game went. Used to find AI and rules bugs (planned 24, `docs/PLANNED.md`).
//! `game` plays one game and is also what `soak-versus` uses.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use f4x_core::content::Content;
use f4x_core::game::endgame::GameOver;
use f4x_core::game::setup::{new_game, NewGame};
use f4x_core::game::Game;
use f4x_core::player::PlayerId;

use crate::contestant::{self, Contestant, Version};
use crate::metrics::{Standing, Tally};

pub const USAGE: &str = "usage: f4x-tools soak [--seeds FIRST..LAST] [--turns N] [--players 2..5] [--content DIR] [--every N] [--trace TURNS] [--detail] [--grant-mastery TURN] [--battle-ai CHECKPOINT] [--refused-log FILE] [--snapshots-every N] [--out DIR]";

/// The options `soak` and `soak-versus` share.
pub struct Options {
    pub first: u64,
    pub last: u64,
    pub turns: u32,
    pub players: u8,
    pub every: u32,
    pub trace: u32,
    pub detail: bool,
    pub grant_mastery: Option<u32>,
    pub battle_ai: Option<PathBuf>,
    pub refused_log: Option<PathBuf>,
    pub dir: PathBuf,
    /// Save the game every this many turns into `out` (none: not at all).
    pub snapshots_every: Option<u32>,
    pub out: Option<PathBuf>,
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
            grant_mastery: None,
            battle_ai: None,
            refused_log: None,
            dir: if Path::new("content-local").exists() {
                "content-local".into()
            } else {
                "content".into()
            },
            snapshots_every: None,
            out: None,
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
                "--grant-mastery" => {
                    o.grant_mastery =
                        Some(value("--grant-mastery")?.parse().map_err(|_| "bad turn")?)
                }
                "--trace" => o.trace = value("--trace")?.parse().map_err(|_| "bad turns")?,
                "--every" => o.every = value("--every")?.parse().map_err(|_| "bad interval")?,
                "--content" => o.dir = value("--content")?.into(),
                "--battle-ai" => o.battle_ai = Some(value("--battle-ai")?.into()),
                "--refused-log" => o.refused_log = Some(value("--refused-log")?.into()),
                "--snapshots-every" => {
                    o.snapshots_every = Some(
                        value("--snapshots-every")?
                            .parse()
                            .map_err(|_| "bad interval")?,
                    )
                }
                "--out" => o.out = Some(value("--out")?.into()),
                other => return Err(format!("unknown option {other:?}\n{usage}")),
            }
        }
        Ok(o)
    }

    pub fn load_content(&self) -> Result<Content, String> {
        f4x_data::load_content_layers(&[&self.dir, Path::new("content")]).map_err(|e| e.to_string())
    }
}

/// Who plays which wizard, and by which version the neutral cities and monsters play.
pub struct Roster {
    pub versions: Vec<Version>,
    pub neutrals: Version,
}

/// The refused-order log, one line per refused command, tab separated.
pub type RefusedLog = Option<std::io::BufWriter<std::fs::File>>;

pub fn open_refused_log(path: &Option<PathBuf>) -> Result<RefusedLog, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut f = std::io::BufWriter::new(
        std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?,
    );
    writeln!(f, "seed\tturn\tplayer\tsource\treason\tcommand").map_err(|e| e.to_string())?;
    Ok(Some(f))
}

/// How one game went.
pub struct GameResult {
    pub over: Option<GameOver>,
    pub tally: Tally,
    /// Each wizard's standing at the end, by player number (neutrals included).
    pub standing: Vec<(PlayerId, Standing)>,
    /// A fingerprint of the saved game at each snapshot turn.
    pub fingerprints: Vec<(u32, u64)>,
}

pub fn run(args: &[String]) -> Result<(), String> {
    let opts = Options::parse(args, USAGE)?;
    let content = opts.load_content()?;
    let mut refused = open_refused_log(&opts.refused_log)?;
    let roster = Roster {
        versions: vec![Version::New; usize::from(opts.players)],
        neutrals: Version::New,
    };
    let mut summary = Vec::new();
    for seed in opts.first..=opts.last {
        let result = game(&content, seed, &opts, &roster, "", &mut refused)?;
        summary.push(result.over.is_some());
    }
    println!(
        "{} of {} games ended with a winner",
        summary.iter().filter(|&&w| w).count(),
        summary.len()
    );
    Ok(())
}

/// Play one game to its end or the turn limit, printing as the options ask, and report it.
/// `label` names the variant in the output ("" for a plain soak game).
pub fn game(
    content: &Content,
    seed: u64,
    opts: &Options,
    roster: &Roster,
    label: &str,
    refused_out: &mut RefusedLog,
) -> Result<GameResult, String> {
    let players = opts.players;
    let tag = if label.is_empty() {
        format!("seed {seed}")
    } else {
        format!("seed {seed} [{label}]")
    };
    let mut game = new_game(
        content,
        &NewGame {
            seed,
            all_ai: true,
            ..NewGame::with_wizards(players)
        },
    )?;
    game.set_undo(false);
    if let Some(path) = &opts.battle_ai {
        #[cfg(feature = "learned")]
        for k in 0..players {
            let brain = f4x_learn::brain::LearnedBrain::load(content, path)?;
            game.set_battle_brain(PlayerId(k), Some(Box::new(brain)));
        }
        #[cfg(not(feature = "learned"))]
        return Err(format!(
            "--battle-ai {} needs f4x-tools built with `--features learned`",
            path.display()
        ));
    }
    let mut ais: Vec<Box<dyn Contestant>> = roster
        .versions
        .iter()
        .enumerate()
        .map(|(k, &v)| contestant::make(v, PlayerId(k as u8)))
        .collect();
    for ai in &mut ais {
        ai.record_refusals(refused_out.is_some());
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
    let mut tally = Tally::new(&game);
    // Rounds that ended with a battle still open (nothing may be: every battle is fought out
    // inside the round), and the first such turn.
    let (mut open_rounds, mut first_open) = (0usize, None::<u32>);
    let mut fingerprints = Vec::new();
    while game.turn <= opts.turns && game.game_over.is_none() {
        if opts.grant_mastery == Some(game.turn) {
            grant_mastery_to_all(&mut game);
        }
        let (turns, rest) = contestant::play_round(&mut game, &mut ais, roster.neutrals);
        for (player, events) in &turns {
            tally.record(&game, Some(*player), events);
        }
        tally.record(&game, None, &rest);
        if game.in_battle() {
            open_rounds += 1;
            first_open.get_or_insert(game.turn);
        }
        if game.turn <= opts.trace && opts.trace > 0 {
            trace_turn(&game, &ais, &turns, &rest);
        }
        if let Some(out) = refused_out.as_mut() {
            for ai in &ais {
                for r in ai.take_refusals() {
                    writeln!(
                        out,
                        "{seed}\t{}\t{}\t{}\t{}\t{}",
                        r.turn, r.player.0, r.source, r.reason, r.command
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
        }
        if opts.every > 0 && game.turn % opts.every == 0 {
            println!(
                "{tag} turn {}: {} ({} entities)",
                game.turn,
                status(&game),
                game.entity_count()
            );
            if opts.detail {
                for line in detail_lines(&game, &ais) {
                    println!("    {line}");
                }
            }
        }
        if let Some(n) = opts.snapshots_every.filter(|&n| n > 0) {
            if game.turn % n == 0 {
                fingerprints.push((game.turn, save_snapshot(&game, snapshot_dir.as_deref())?));
            }
        }
    }
    if opts.detail {
        city_lines(&game);
    }
    let outcome = match game.game_over {
        Some(o) => format!(
            "won by player {} ({:?}) on turn {}",
            o.winner.0, o.kind, o.turn
        ),
        None => format!("no winner after {} turns", game.turn - 1),
    };
    let open = match first_open {
        Some(turn) => {
            format!(", {open_rounds} rounds ended with a battle open (first on turn {turn})")
        }
        None => String::new(),
    };
    let total = tally.total();
    let seconds = started.elapsed().as_secs_f32();
    println!(
        "{tag}: {outcome}; {}; {} battles, {} cities taken, {} wars declared, {} refused orders{open}; {seconds:.1}s",
        status(&game),
        total.battles_won,
        total.cities_captured,
        total.wars_declared,
        total.refused_orders,
    );
    let standing = game
        .player_ids()
        .into_iter()
        .filter(|&p| !game.is_neutral_player(p))
        .map(|p| (p, Standing::of(&game, p)))
        .collect();
    Ok(GameResult {
        over: game.game_over,
        tally,
        standing,
        fingerprints,
    })
}

/// Fingerprint the saved game (without its checksum line) and, when `dir` is given, write it as
/// `tNNNN.f4x`.
fn save_snapshot(game: &Game, dir: Option<&Path>) -> Result<u64, String> {
    let text = game.snapshot().to_text().map_err(|e| e.to_string())?;
    let mut hasher = DefaultHasher::new();
    text.split_once('\n')
        .map_or(&*text, |(_, body)| body)
        .hash(&mut hasher);
    if let Some(dir) = dir {
        let path = dir.join(format!("t{:04}.f4x", game.turn));
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(hasher.finish())
}

fn trace_turn(
    game: &Game,
    ais: &[Box<dyn Contestant>],
    turns: &[(PlayerId, Vec<f4x_core::game::Event>)],
    rest: &[f4x_core::game::Event],
) {
    for ai in ais {
        println!("  [t{}] ai {} {}", game.turn, ai.player().0, ai.describe());
    }
    for (c, city, _) in game.economy().cities() {
        let prod = game.economy().city_production(c);
        let item = prod.item.map(|i| f4x_console::describe::item(game, i));
        let eco = game.economy().city_economy(c);
        println!(
            "  [t{}] {} (p{}): {:?} {}/{} eco {:?}",
            game.turn,
            city.name,
            city.owner.0,
            item,
            prod.progress,
            prod.item.map_or(0, |i| game.economy().item_cost(c, i)),
            eco.map(|e| (e.figures, e.farmers, e.production, e.food))
        );
    }
    for e in turns.iter().flat_map(|(_, events)| events).chain(rest) {
        println!(
            "  [t{}] {}",
            game.turn,
            f4x_console::describe::event(game, e)
        );
    }
}

/// Each city of a wizard with who has explored it (the soak's `--detail` lines).
fn city_lines(game: &Game) {
    for (c, city, pos) in game.economy().cities() {
        if game.is_neutral_player(city.owner) {
            continue;
        }
        let seen_by: Vec<String> = game
            .player_ids()
            .into_iter()
            .filter(|&p| p != city.owner && game.is_explored(p, pos))
            .map(|p| format!("p{}", p.0))
            .collect();
        println!(
            "    city {} of p{} at {:?} {},{} size {}: explored by [{}]",
            city.name,
            city.owner.0,
            pos.plane,
            pos.x,
            pos.y,
            game.economy().city_economy(c).map_or(0, |e| e.figures),
            seen_by.join(" ")
        );
    }
}

/// Objective and temper, cities, units and gold of each wizard, e.g. `p0[Mil/Agg] 5c(1o)/14u/80g` (`o`: outposts, counted in the cities).
pub(crate) fn status(game: &Game) -> String {
    game.player_ids()
        .into_iter()
        .filter(|id| !game.is_neutral_player(*id))
        .map(|id| {
            let cities = game
                .economy()
                .cities()
                .iter()
                .filter(|(_, c, _)| c.owner == id)
                .count();
            // Outposts: cities with no citizen yet (they are counted in the cities).
            let outposts = game
                .economy()
                .cities()
                .iter()
                .filter(|(e, c, _)| c.owner == id && game.city_population(*e).figures == 0)
                .count();
            let units = game.units().player_units(id).len();
            let tag = if game.player(id).is_some_and(|p| p.is_defeated()) {
                " (defeated)"
            } else {
                ""
            };
            let temper = game.wizard(id).map_or(String::new(), |w| {
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
                game.player(id).map_or(0, |p| p.treasury().gold)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// One line per wizard about magic and exploring: mana, spells known, research, the spell being
/// cast and how much of the map is explored.
fn detail_lines(game: &Game, ais: &[Box<dyn Contestant>]) -> Vec<String> {
    let mut out = Vec::new();
    for id in game
        .player_ids()
        .into_iter()
        .filter(|id| !game.is_neutral_player(*id))
    {
        let Some(m) = game.wizard(id) else {
            continue;
        };
        let name = |s: f4x_core::rules::SpellId| game.content().rules.spells.get(s).id.clone();
        let explored = game.explored_squares(id);
        let income = game.income(id);
        let mut known: Vec<String> = m.known().iter().map(|&s| name(s)).collect();
        known.sort();
        out.push(format!("  known: {}", known.join(" ")));
        out.push(format!(
            "p{}: skill {}/{}, gold {} (cities {}, buildings -{}, units -{}, food {}), food {}, mana {} (+{} upkeep {}), spells {}, researching {} ({} left), casting {}, split {}/{}/{}, explored {}{}",
            id.0,
            m.skill_left(),
            m.casting_skill(),
            game.budget(id).net_gold(),
            game.budget(id).city_gold,
            game.budget(id).building_upkeep,
            game.budget(id).unit_upkeep,
            game.budget(id).food_gold,
            game.budget(id).food_surplus,
            game.player(id).map_or(0, |p| p.treasury().mana),
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
                .map_or(String::new(), |a| a.describe())
        ));
    }
    out
}

/// Give every wizard the Spell of Mastery with the mana and skill to cast it, so a game shows
/// whether the AI casts it and wins without waiting for the 60,000 research points.
fn grant_mastery_to_all(game: &mut Game) {
    let Some(spell) = game.content().rules.spells.id_of("spell_of_mastery") else {
        return;
    };
    let ids: Vec<PlayerId> = game
        .player_ids()
        .into_iter()
        .filter(|id| !game.is_neutral_player(*id))
        .collect();
    for id in ids {
        if let Some(t) = game.treasury_mut(id) {
            t.mana += 6000;
        }
        if let Some(mut w) = game.wizard_mut(id) {
            let mut spells = w.get_mut::<f4x_core::wizard::sheet::KnownSpells>();
            spells.known.insert(spell);
            spells.researchable.remove(&spell);
            drop(spells);
            w.get_mut::<f4x_core::wizard::sheet::Skill>().points += 2000;
        }
    }
}
