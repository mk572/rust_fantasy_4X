//! The text console (`docs/console.md`): every action of the game as a line of text.
//!
//! One [`Console`] drives one [`Game`] for one player. A line is looked up in a table of
//! [`CommandDef`]s, each of a [`Kind`]:
//!
//! * **query** commands only read the game;
//! * **legal** commands are what a player may do in normal play, and go through the same
//!   `Command` path as the client, with ownership checked;
//! * **override** commands are cheats (spawn units, remove players, set gold). They run only
//!   in [`Mode::Override`], and every use is written to the game's saved override log.
//!
//! The table ([`commands`]) and [`Console::execute`] are the only hooks a front end needs: the
//! client's console, the headless runner, an AI player, an MCP server or a remote client all
//! send lines and read a [`Reply`]. What a front end may do at most is its [`Access`].

pub mod args;
mod cheats;
pub mod describe;
mod legal;
mod meta;
mod query;
pub mod setup;
pub mod talk;

use f4x_core::city::PlayerId;
use f4x_core::{Command, Event, Game};

use args::Args;

/// What a command does to the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// Reads the game (and the console's own settings).
    Query,
    /// Something a player may do in normal play.
    Legal,
    /// Cheating: changes the game in ways the rules never allow.
    Override,
}

/// What the console accepts right now; the user switches it with `mode`, within the limit set
/// by the console's [`Access`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    /// Queries only: nothing typed can change the game.
    ReadOnly,
    /// Queries and legal commands.
    Legal,
    /// Everything, including override commands (cheats).
    Override,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::ReadOnly => "readonly",
            Mode::Legal => "legal",
            Mode::Override => "override",
        }
    }

    /// The mode words `mode` accepts.
    pub fn parse(word: &str) -> Option<Mode> {
        match word {
            "readonly" | "read-only" => Some(Mode::ReadOnly),
            "legal" => Some(Mode::Legal),
            "override" => Some(Mode::Override),
            _ => None,
        }
    }
}

/// The most a front end may ever do through its console.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Access {
    /// Queries only (a spectator, a read-only remote).
    ReadOnly,
    /// Queries and legal commands (an AI player, a networked player, hardcore mode).
    Play,
    /// Everything, if the mode is switched to override.
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleError {
    Empty,
    Unknown(String),
    Usage(String),
    Parse(String),
    NoSuch(String),
    /// Not yours, or not allowed in this mode or access level.
    NotAllowed(String),
    /// The game turned the order down.
    Refused(String),
    Failed(String),
}

impl std::fmt::Display for ConsoleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsoleError::Empty => write!(f, "empty line"),
            ConsoleError::Unknown(c) => write!(f, "unknown command {c:?} (try `help`)"),
            ConsoleError::Usage(m)
            | ConsoleError::Parse(m)
            | ConsoleError::NotAllowed(m)
            | ConsoleError::Failed(m) => write!(f, "{m}"),
            ConsoleError::NoSuch(m) => write!(f, "no such {m}"),
            ConsoleError::Refused(m) => write!(f, "refused: {m}"),
        }
    }
}

impl std::error::Error for ConsoleError {}

/// The answer to one line.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub ok: bool,
    pub lines: Vec<String>,
    /// The game events the line caused, for front ends that want more than the text.
    pub events: Vec<Event>,
    pub error: Option<ConsoleError>,
}

impl Reply {
    /// The reply as the console prints it.
    pub fn text(&self) -> String {
        let mut out = self.lines.join("\n");
        if let Some(e) = &self.error {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("error: {e}"));
        }
        out
    }
}

/// What a command handler may use.
pub struct Ctx<'a> {
    pub game: &'a mut Game,
    pub session: &'a mut Session,
    pub out: Vec<String>,
    pub events: Vec<Event>,
    /// A legal command was run on something that is not the player's (override mode only).
    pub bypassed: bool,
}

type Run = fn(&mut Ctx, &Args) -> Result<(), ConsoleError>;

/// One entry of the command table.
pub struct CommandDef {
    pub name: &'static str,
    pub kind: Kind,
    pub usage: &'static str,
    pub help: &'static str,
    pub run: Run,
}

/// The console's settings that commands read and change.
#[derive(Debug, Clone)]
pub struct Session {
    /// The player the console plays for.
    pub player: PlayerId,
    pub mode: Mode,
    pub access: Access,
    /// Set by `quit`: the front end should close.
    pub quit: bool,
}

/// A console for one game.
pub struct Console {
    pub session: Session,
    /// Every line given, with whether it worked.
    pub history: Vec<(String, bool)>,
}

/// All commands, in the order `help` lists them.
pub fn commands() -> Vec<&'static CommandDef> {
    let mut all: Vec<&'static CommandDef> = Vec::new();
    for table in [
        meta::TABLE,
        query::TABLE,
        legal::TABLE,
        talk::TABLE,
        cheats::TABLE,
    ] {
        all.extend(table.iter());
    }
    all
}

pub fn find(name: &str) -> Option<&'static CommandDef> {
    commands().into_iter().find(|c| c.name == name)
}

impl Console {
    pub fn new(player: PlayerId, access: Access) -> Self {
        Self {
            session: Session {
                player,
                mode: if access == Access::ReadOnly {
                    Mode::ReadOnly
                } else {
                    Mode::Legal
                },
                access,
                quit: false,
            },
            history: Vec::new(),
        }
    }

    /// Command names starting with `prefix`, for completion.
    pub fn complete(&self, prefix: &str) -> Vec<&'static str> {
        commands()
            .into_iter()
            .filter(|c| self.permitted(c.kind).is_ok() && c.name.starts_with(prefix))
            .map(|c| c.name)
            .collect()
    }

    fn permitted(&self, kind: Kind) -> Result<(), ConsoleError> {
        let s = &self.session;
        match kind {
            Kind::Query => Ok(()),
            Kind::Legal if s.access < Access::Play => {
                Err(ConsoleError::NotAllowed("this console is read-only".into()))
            }
            Kind::Legal if s.mode == Mode::ReadOnly => Err(ConsoleError::NotAllowed(
                "read-only mode: switch with `mode legal` to change the game".into(),
            )),
            Kind::Legal => Ok(()),
            Kind::Override if s.access < Access::Full => Err(ConsoleError::NotAllowed(
                "override commands are not available to this console".into(),
            )),
            Kind::Override if s.mode != Mode::Override => Err(ConsoleError::NotAllowed(
                "override command: switch on override mode first (`mode override`)".into(),
            )),
            Kind::Override => Ok(()),
        }
    }

    /// Run one line against `game`.
    pub fn execute(&mut self, game: &mut Game, line: &str) -> Reply {
        let result = self.run(game, line);
        let (ok, lines, events, error) = match result {
            Ok((lines, events)) => (true, lines, events, None),
            Err((lines, events, e)) => (false, lines, events, Some(e)),
        };
        if !line.trim().is_empty() {
            self.history.push((line.trim().to_string(), ok));
        }
        Reply {
            ok,
            lines,
            events,
            error,
        }
    }

    #[allow(clippy::type_complexity)]
    fn run(
        &mut self,
        game: &mut Game,
        line: &str,
    ) -> Result<(Vec<String>, Vec<Event>), (Vec<String>, Vec<Event>, ConsoleError)> {
        let fail = |e| (Vec::new(), Vec::new(), e);
        let words = args::tokenize(line).map_err(fail)?;
        let Some((name, rest)) = words.split_first() else {
            return Err(fail(ConsoleError::Empty));
        };
        let def =
            find(&name.to_lowercase()).ok_or_else(|| fail(ConsoleError::Unknown(name.clone())))?;
        self.permitted(def.kind).map_err(fail)?;
        let mut ctx = Ctx {
            game,
            session: &mut self.session,
            out: Vec::new(),
            events: Vec::new(),
            bypassed: false,
        };
        let result = (def.run)(&mut ctx, &Args { words: rest });
        let (mut out, events, bypassed) = (ctx.out, ctx.events, ctx.bypassed);
        if result.is_ok() && out.is_empty() && def.kind != Kind::Query {
            out.push("ok".into());
        }
        match result {
            Ok(()) => {
                if def.kind == Kind::Override || bypassed {
                    game.record_override(line.trim());
                }
                Ok((out, events))
            }
            Err(e) => Err((out, events, e)),
        }
    }
}

impl Ctx<'_> {
    pub fn say(&mut self, line: impl Into<String>) {
        self.out.push(line.into());
    }

    /// Whether the console may act on things of any owner.
    pub fn overriding(&self) -> bool {
        self.session.mode == Mode::Override
    }

    /// Carry out a game command as the console's player, print what happened and follow up
    /// the way the client does: an attack on a hostile stack starts a battle and a move onto
    /// a guarded site assaults it.
    pub fn apply(&mut self, command: Command) -> Result<(), ConsoleError> {
        let mut queue = vec![command];
        let mut refused = None;
        while let Some(cmd) = queue.pop() {
            let events = self.game.apply(cmd);
            for e in &events {
                match e {
                    Event::Rejected(why) => {
                        refused.get_or_insert_with(|| format!("{why:?}"));
                    }
                    Event::AttackRequested {
                        attackers,
                        defenders,
                        ..
                    } if attackers
                        .first()
                        .and_then(|&u| self.game.units().unit_owner(u))
                        == Some(self.session.player) =>
                    {
                        queue.push(Command::BeginBattle {
                            attackers: attackers.clone(),
                            defenders: defenders.clone(),
                        });
                    }
                    Event::SiteEncounter { units, to, .. }
                        if units.first().and_then(|&u| self.game.units().unit_owner(u))
                            == Some(self.session.player) =>
                    {
                        queue.push(Command::EnterSite {
                            units: units.clone(),
                            to: *to,
                        });
                    }
                    _ => {}
                }
                let text = describe::event(self.game, e);
                self.out.push(text);
            }
            self.events.extend(events);
        }
        match refused {
            Some(why) => Err(ConsoleError::Refused(why)),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
