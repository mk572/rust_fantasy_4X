//! Commands about the console and the game as a whole: help, mode, save, load, new.

use std::path::PathBuf;

use f4x_core::game::setting::EventFrequency;

use crate::args::Args;
use crate::setup::{new_game, parse_retort, NewGame};
use crate::{commands, find, CommandDef, ConsoleError, Ctx, Kind, Mode};

pub const TABLE: &[CommandDef] = &[
    CommandDef {
        name: "help",
        kind: Kind::Query,
        usage: "help [command]",
        help: "List the commands, or describe one.",
        run: help,
    },
    CommandDef {
        name: "commands",
        kind: Kind::Query,
        usage: "commands",
        help: "One line per command: name, kind (query, legal) and usage, tab separated.",
        run: list,
    },
    CommandDef {
        name: "mode",
        kind: Kind::Query,
        usage: "mode [readonly|legal]",
        help: "Show or set the mode. In readonly mode only queries run; legal allows normal play.",
        run: mode,
    },
    CommandDef {
        name: "settings",
        kind: Kind::Query,
        usage: "settings",
        help: "The settings this game is played with (difficulty, opponents, land, magic power).",
        run: settings,
    },
    CommandDef {
        name: "save",
        kind: Kind::Legal,
        usage: "save [name]",
        help: "Save the game to saves/<name>.ron (default quick).",
        run: save,
    },
    CommandDef {
        name: "load",
        kind: Kind::Legal,
        usage: "load [name]",
        help: "Replace the game with saves/<name>.ron (default quick).",
        run: load,
    },
    CommandDef {
        name: "record",
        kind: Kind::Legal,
        usage: "record [start|stop [name]|status]",
        help: "Record the commands played from now on. `stop` writes the record, with the game as it was at the start, to saves/<name>.replay.ron (default quick); `status` says how many commands are in it.",
        run: record,
    },
    CommandDef {
        name: "replay",
        kind: Kind::Legal,
        usage: "replay [name] [count]",
        help: "Replace the game with a record from saves/<name>.replay.ron played to its end, or through its first <count> commands. The game goes on recording.",
        run: replay,
    },
    CommandDef {
        name: "new",
        kind: Kind::Legal,
        usage: "new [seed] [wizard=<preset>] [name=<text>] [books=life:3,nature:2] [retorts=alchemy,...] [race=<id>] [players=2..5] [land=small|medium|large|<percent>] [difficulty=intro|easy|average|hard|impossible] [magic=weak|normal|powerful]",
        help: "Start a new game on a generated map, replacing this one. Without wizard or books, a fixed wizard is used.",
        run: new,
    },
    CommandDef {
        name: "quit",
        kind: Kind::Query,
        usage: "quit",
        help: "Close the console (the front end decides what that means).",
        run: quit,
    },
];

fn help(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    if let Some(name) = a.get(0) {
        let def = find(name).ok_or_else(|| ConsoleError::NoSuch(format!("command {name:?}")))?;
        ctx.say(format!("{} [{}]", def.usage, kind_name(def.kind)));
        ctx.say(def.help.to_string());
        return Ok(());
    }
    ctx.say(format!(
        "Mode: {}. Kinds: query reads, legal is normal play.",
        ctx.session.mode.name()
    ));
    for def in commands() {
        ctx.say(format!(
            "{:<9} {:<9} {}",
            def.name,
            kind_name(def.kind),
            def.usage
        ));
    }
    Ok(())
}

pub fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Query => "query",
        Kind::Legal => "legal",
    }
}

fn list(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    for def in commands() {
        ctx.say(format!(
            "{}\t{}\t{}",
            def.name,
            kind_name(def.kind),
            def.usage
        ));
    }
    Ok(())
}

fn mode(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let Some(word) = a.get(0) else {
        ctx.say(format!("mode {}", ctx.session.mode.name()));
        return Ok(());
    };
    let wanted = Mode::parse(word)
        .ok_or_else(|| ConsoleError::Usage(format!("mode is readonly or legal, not {word:?}")))?;
    // Going down is always allowed; going up is limited by the console's access.
    let needed = match wanted {
        Mode::ReadOnly => crate::Access::ReadOnly,
        Mode::Legal => crate::Access::Play,
    };
    if ctx.session.access < needed {
        return Err(ConsoleError::NotAllowed(format!(
            "{} mode is not available to this console",
            wanted.name()
        )));
    }
    ctx.session.mode = wanted;
    ctx.say(match wanted {
        Mode::ReadOnly => "mode readonly: only queries run".to_string(),
        Mode::Legal => "mode legal".to_string(),
    });
    Ok(())
}

fn settings(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    let b = ctx.game.settings().basic;
    let level = |names: &[(&'static str, u8)], i: u8| {
        names
            .iter()
            .find(|(_, v)| *v == i)
            .map_or("?", |(name, _)| *name)
    };
    ctx.say(format!(
        "difficulty {} ({})",
        b.difficulty,
        level(&DIFFICULTY, b.difficulty)
    ));
    ctx.say(format!("opponents {}", b.opponents));
    ctx.say(format!(
        "diagonal steps cost {}",
        if b.diagonal_extra_cost {
            "50% more"
        } else {
            "the same"
        }
    ));
    ctx.say(format!("land {}%", b.land_percent));
    ctx.say(format!(
        "magic power {} ({})",
        b.magic_strength,
        level(&MAGIC, b.magic_strength)
    ));
    let e = ctx.game.settings().events;
    if e.random_events {
        let chance = |turns| {
            f4x_core::world_event::scaled_pressure(turns, e.pressure_percent, e.frequency) as f32
                * 100.0
                / 512.0
        };
        ctx.say(format!(
            "random events on, {:?} frequency: a chance of {:.1}% after 10 quiet turns, {:.1}% after 20, {:.1}% after 40",
            e.frequency,
            chance(10),
            chance(20),
            chance(40)
        ));
    } else {
        ctx.say("random events off");
    }
    Ok(())
}

fn save_path(a: &Args) -> Result<PathBuf, ConsoleError> {
    named_path(a.get(0))
}

fn named_path(name: Option<&str>) -> Result<PathBuf, ConsoleError> {
    let name = name.unwrap_or("quick");
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ConsoleError::Parse(
            "a save name is letters, digits, - and _".into(),
        ));
    }
    Ok(PathBuf::from("saves").join(format!("{name}.ron")))
}

fn save(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let path = save_path(a)?;
    f4x_data::save::save_game(&path, ctx.game).map_err(|e| ConsoleError::Failed(e.to_string()))?;
    ctx.say(format!("saved to {}", path.display()));
    Ok(())
}

/// `new` and `load` replace the whole game: only a console with full access may.
fn replaces_game(ctx: &Ctx) -> Result<(), ConsoleError> {
    if ctx.session.access < crate::Access::Full {
        return Err(ConsoleError::NotAllowed(
            "this console may not replace the game".into(),
        ));
    }
    Ok(())
}

fn load(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    replaces_game(ctx)?;
    let path = save_path(a)?;
    let content = ctx.game.content().clone();
    let mut game = f4x_data::save::load_game(&path, content)
        .map_err(|e| ConsoleError::Failed(e.to_string()))?;
    game.ensure_human(ctx.session.player);
    game.set_undo(true);
    *ctx.game = game;
    ctx.say(format!(
        "loaded {} (turn {})",
        path.display(),
        ctx.game.turn
    ));
    Ok(())
}

fn replay_path(name: Option<&str>) -> Result<PathBuf, ConsoleError> {
    let path = named_path(name)?;
    Ok(path.with_extension("replay.ron"))
}

fn record(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let failed = |e: f4x_core::game::replay::ReplayError| ConsoleError::Refused(format!("{e:?}"));
    match a.get(0).unwrap_or("start") {
        "start" => {
            ctx.game.start_recording().map_err(failed)?;
            ctx.say("recording".to_string());
        }
        "status" => match ctx.game.replay_so_far() {
            Ok(r) => ctx.say(format!("recording, {} commands so far", r.commands.len())),
            Err(_) => ctx.say("not recording".to_string()),
        },
        "stop" => {
            let path = replay_path(a.get(1))?;
            let replay = ctx
                .game
                .stop_recording()
                .ok_or_else(|| ConsoleError::Refused("not recording".into()))?;
            let text = replay
                .to_text()
                .map_err(|e| ConsoleError::Failed(format!("{e:?}")))?;
            std::fs::create_dir_all("saves").map_err(|e| ConsoleError::Failed(e.to_string()))?;
            std::fs::write(&path, text).map_err(|e| ConsoleError::Failed(e.to_string()))?;
            ctx.say(format!(
                "wrote {} ({} commands)",
                path.display(),
                replay.commands.len()
            ));
        }
        other => {
            return Err(ConsoleError::Usage(format!(
                "record start, stop or status, not {other:?}"
            )))
        }
    }
    Ok(())
}

fn replay(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    replaces_game(ctx)?;
    let path = replay_path(a.get(0))?;
    let count = match a.get(1) {
        Some(n) => n
            .parse()
            .map_err(|_| ConsoleError::Parse(format!("{n:?} is not a number")))?,
        None => usize::MAX,
    };
    let text = std::fs::read_to_string(&path).map_err(|e| ConsoleError::Failed(e.to_string()))?;
    let replay = f4x_core::game::replay::Replay::from_text(&text)
        .map_err(|e| ConsoleError::Failed(format!("{e:?}")))?;
    let (mut game, events) = replay
        .play(ctx.game.content().clone(), count)
        .map_err(|e| ConsoleError::Failed(format!("{e:?}")))?;
    game.ensure_human(ctx.session.player);
    game.set_undo(true);
    *ctx.game = game;
    ctx.say(format!(
        "played {} commands from {} (turn {})",
        events.len(),
        path.display(),
        ctx.game.turn
    ));
    Ok(())
}

fn new(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    replaces_game(ctx)?;
    let (plain, opts) = a.split_options();
    let mut spec = NewGame {
        seed: match plain.first() {
            Some(s) => s
                .parse()
                .map_err(|_| ConsoleError::Parse(format!("{s:?} is not a seed")))?,
            None => ctx.game.seed().wrapping_add(1),
        },
        ..NewGame::default()
    };
    // The events' options apply after a difficulty has set the rest, whatever the order.
    let (mut events_on, mut frequency) = (None, None);
    for (k, v) in opts {
        match k {
            "events" => events_on = Some(parse_choice(v, "events", &ON_OFF, 0, 1)? == 1),
            "event-frequency" => {
                frequency = Some(match v {
                    "half" => EventFrequency::Half,
                    "normal" => EventFrequency::Normal,
                    "double" => EventFrequency::Double,
                    _ => {
                        return Err(ConsoleError::Parse(
                            "event-frequency is half, normal or double".into(),
                        ))
                    }
                })
            }
            "wizard" => spec.preset = Some(v.to_string()),
            "name" => spec.name = Some(v.to_string()),
            "race" => spec.race = Some(v.to_string()),
            "players" => {
                let n: u8 = v.parse().map_err(|_| {
                    ConsoleError::Parse(format!("{v:?} is not a number of wizards"))
                })?;
                let most = ctx.game.content().settings.max_opponents + 1;
                if !(2..=most).contains(&n) {
                    return Err(ConsoleError::Parse(format!(
                        "a game has 2 to {most} wizards"
                    )));
                }
                spec.settings.basic.opponents = n - 1;
            }
            "land" => {
                // The sizes of `settings.ron`, by lower-case name.
                let sizes = &ctx.game.content().settings.land_sizes;
                let names: Vec<String> = sizes.iter().map(|s| s.name.to_lowercase()).collect();
                let table: Vec<(&str, u8)> = names
                    .iter()
                    .map(String::as_str)
                    .zip(sizes.iter().map(|s| s.land_percent))
                    .collect();
                spec.settings.basic.land_percent = parse_choice(v, "land", &table, 1, 99)?;
            }
            "difficulty" => {
                // A level brings the original's figures for it; the other basic choices stay.
                let level = parse_choice(v, "difficulty", &DIFFICULTY, 0, 4)?;
                let basic = spec.settings.basic;
                spec.settings = f4x_core::game::setting::GameSettings::for_difficulty(
                    &ctx.game.content().settings,
                    level,
                );
                spec.settings.basic = f4x_core::game::setting::BasicSettings {
                    difficulty: level,
                    ..basic
                };
            }
            "magic" => spec.settings.basic.magic_strength = parse_choice(v, "magic", &MAGIC, 0, 2)?,
            "books" => {
                for part in v.split(',') {
                    let (realm, n) = part.split_once(':').ok_or_else(|| {
                        ConsoleError::Parse("books are realm:count, e.g. life:3,nature:2".into())
                    })?;
                    let n = n
                        .parse()
                        .map_err(|_| ConsoleError::Parse(format!("{n:?} is not a count")))?;
                    spec.books.insert(realm.to_string(), n);
                }
            }
            "retorts" => {
                for r in v.split(',').filter(|r| !r.is_empty()) {
                    let retort = parse_retort(r)
                        .ok_or_else(|| ConsoleError::NoSuch(format!("retort {r:?}")))?;
                    spec.retorts.insert(retort);
                }
            }
            other => return Err(ConsoleError::Usage(format!("unknown option {other:?}"))),
        }
    }
    if let Some(on) = events_on {
        spec.settings.events.random_events = on;
    }
    if let Some(f) = frequency {
        spec.settings.events.frequency = f;
    }
    let content = ctx.game.content().clone();
    let game = new_game(&content, &spec).map_err(ConsoleError::Failed)?;
    *ctx.game = game;
    ctx.session.player = crate::setup::HUMAN;
    ctx.say(format!("new game, seed {}", spec.seed));
    Ok(())
}

const ON_OFF: [(&str, u8); 2] = [("off", 0), ("on", 1)];
const DIFFICULTY: [(&str, u8); 5] = [
    ("intro", 0),
    ("easy", 1),
    ("average", 2),
    ("hard", 3),
    ("impossible", 4),
];
const MAGIC: [(&str, u8); 3] = [("weak", 0), ("normal", 1), ("powerful", 2)];

/// A setting given by name or as a number in `min..=max`.
fn parse_choice(
    v: &str,
    what: &str,
    names: &[(&str, u8)],
    min: u8,
    max: u8,
) -> Result<u8, ConsoleError> {
    let lower = v.to_lowercase();
    if let Some(&(_, n)) = names.iter().find(|(name, _)| *name == lower) {
        return Ok(n);
    }
    match lower.parse::<u8>() {
        Ok(n) if (min..=max).contains(&n) => Ok(n),
        _ => Err(ConsoleError::Parse(format!(
            "{v:?} is not a {what}: use a name or a number from {min} to {max}"
        ))),
    }
}

fn quit(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    ctx.session.quit = true;
    Ok(())
}
