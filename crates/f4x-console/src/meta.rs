//! Commands about the console and the game as a whole: help, mode, save, load, new.

use std::path::PathBuf;

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
        help: "One line per command: name, kind (query, legal, override) and usage, tab separated.",
        run: list,
    },
    CommandDef {
        name: "mode",
        kind: Kind::Query,
        usage: "mode [legal|override]",
        help: "Show or set the mode. Override commands (cheats) only run in override mode, and switching it on is written to the game's override log.",
        run: mode,
    },
    CommandDef {
        name: "history",
        kind: Kind::Query,
        usage: "history",
        help: "The override commands used in this game.",
        run: history,
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
        name: "new",
        kind: Kind::Legal,
        usage: "new [seed] [wizard=<preset>] [name=<text>] [books=life:3,nature:2] [retorts=alchemy,...] [race=<id>]",
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
        "Mode: {}. Kinds: query reads, legal is normal play, override is cheating (needs `mode override`).",
        if ctx.overriding() { "override" } else { "legal" }
    ));
    for def in commands() {
        ctx.say(format!("{:<9} {:<9} {}", def.name, kind_name(def.kind), def.usage));
    }
    Ok(())
}

pub fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Query => "query",
        Kind::Legal => "legal",
        Kind::Override => "override",
    }
}

fn list(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    for def in commands() {
        ctx.say(format!("{}\t{}\t{}", def.name, kind_name(def.kind), def.usage));
    }
    Ok(())
}

fn mode(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    match a.get(0) {
        None => {
            let m = if ctx.overriding() { "override" } else { "legal" };
            ctx.say(format!("mode {m}"));
        }
        Some("legal") => {
            ctx.session.mode = Mode::Legal;
            ctx.say("mode legal");
        }
        Some("override") => {
            if ctx.session.access < crate::Access::Full {
                return Err(ConsoleError::NotAllowed(
                    "override mode is not available to this console".into(),
                ));
            }
            if ctx.session.mode != Mode::Override {
                ctx.session.mode = Mode::Override;
                ctx.game.record_override("mode override");
            }
            ctx.say("mode override: cheats are on, and logged in the game");
        }
        Some(other) => {
            return Err(ConsoleError::Usage(format!(
                "mode is legal or override, not {other:?}"
            )))
        }
    }
    Ok(())
}

fn history(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    let log: Vec<String> = ctx.game.override_log().to_vec();
    if log.is_empty() {
        ctx.say("no override commands used");
    }
    for l in log {
        ctx.say(l);
    }
    Ok(())
}

fn save_path(a: &Args) -> Result<PathBuf, ConsoleError> {
    let name = a.get(0).unwrap_or("quick");
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err(ConsoleError::Parse(
            "a save name is letters, digits, - and _".into(),
        ));
    }
    Ok(PathBuf::from("saves").join(format!("{name}.ron")))
}

fn save(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    if ctx.game.in_battle() {
        return Err(ConsoleError::Refused(
            "finish or flee the battle first: battles are not saved".into(),
        ));
    }
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
    let content = ctx.game.content.clone();
    let mut game = f4x_data::save::load_game(&path, content)
        .map_err(|e| ConsoleError::Failed(e.to_string()))?;
    game.human.get_or_insert(ctx.session.player);
    game.set_undo(true);
    *ctx.game = game;
    ctx.say(format!("loaded {} (turn {})", path.display(), ctx.game.turn));
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
    for (k, v) in opts {
        match k {
            "wizard" => spec.preset = Some(v.to_string()),
            "name" => spec.name = Some(v.to_string()),
            "race" => spec.race = Some(v.to_string()),
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
    let content = ctx.game.content.clone();
    let game = new_game(&content, &spec).map_err(ConsoleError::Failed)?;
    *ctx.game = game;
    ctx.session.player = crate::setup::HUMAN;
    ctx.say(format!("new game, seed {}", spec.seed));
    Ok(())
}

fn quit(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    ctx.session.quit = true;
    Ok(())
}
