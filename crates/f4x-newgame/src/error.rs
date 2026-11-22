//! Why `new_game` refused to make a game: one variant for each reason, so a caller can tell a bad
//! input from a content that is not fit, and a client can show the reason in the player's language.
//! `text_key` and `args` name the message in `crates/f4x-lang/content/text/en/new_game_error.ron`
//! (`docs/text.md`); `Display` is the English wording for logs, tests and the console.

use crate::mapgen::GenError;
use f4x_core::wizard::BuildError;
use f4x_def::issue::ContentIssue;

#[derive(Debug, Clone, PartialEq)]
pub enum NewGameError {
    /// A setting is out of its range (the issues say which, and what is allowed).
    Settings(Vec<ContentIssue>),
    /// The content is not fit to start from (a hook or special spell the code lacks, a number the
    /// code names that leads to another row).
    Content(Vec<String>),
    /// More opponents than the content allows.
    TooManyOpponents { asked: u8, most: u8 },
    /// A wizard or race was chosen for player 0 in a game that has no human.
    ChoicesWithoutHuman,
    /// The chosen ready-made wizard is not in the content.
    NoPreset { num: u16 },
    /// The human's wizard breaks the rules of the picks, books or retorts.
    Wizard(BuildError),
    /// The chosen race is not one of the races of the plane the wizard starts on.
    RaceNotOnPlane { race: u16, myrror: bool },
    /// The content has no race for a wizard to be.
    NoRaces,
    /// A race number the start needs is not in the content.
    NoRace { race: u16 },
    /// The map generator refused.
    Map(GenError),
    /// The game has no map where the start needs one.
    NoMap,
    /// No square for a wizard's fortress was found.
    NoFortressPlace,
}

const KEY: &str = "new_game_error.";

impl NewGameError {
    /// The key of the message (`new_game_error.<reason>`).
    pub fn text_key(&self) -> &'static str {
        match self {
            NewGameError::Settings(_) => "new_game_error.settings",
            NewGameError::Content(_) => "new_game_error.content",
            NewGameError::TooManyOpponents { .. } => "new_game_error.too_many_opponents",
            NewGameError::ChoicesWithoutHuman => "new_game_error.choices_without_human",
            NewGameError::NoPreset { .. } => "new_game_error.no_preset",
            NewGameError::Wizard(e) => match e {
                BuildError::TooManyPicks { .. } => "new_game_error.too_many_picks",
                BuildError::UnspentPicks { .. } => "new_game_error.unspent_picks",
                BuildError::TooManyRetorts => "new_game_error.too_many_retorts",
                BuildError::RetortNotAvailable(_) => "new_game_error.retort_not_available",
                BuildError::UnknownRealm(_) => "new_game_error.unknown_realm",
                BuildError::TooManyBooks(_) => "new_game_error.too_many_books",
                BuildError::TooManyBooksInAll => "new_game_error.too_many_books_in_all",
                BuildError::BadSpell(_) => "new_game_error.bad_spell",
                BuildError::Magic(_) => "new_game_error.wizard_magic",
            },
            NewGameError::RaceNotOnPlane { myrror: false, .. } => {
                "new_game_error.race_not_on_arcanus"
            }
            NewGameError::RaceNotOnPlane { myrror: true, .. } => {
                "new_game_error.race_not_on_myrror"
            }
            NewGameError::NoRaces => "new_game_error.no_races",
            NewGameError::NoRace { .. } => "new_game_error.no_race",
            NewGameError::Map(_) => "new_game_error.map",
            NewGameError::NoMap => "new_game_error.no_map",
            NewGameError::NoFortressPlace => "new_game_error.no_fortress_place",
        }
    }

    /// The values the message names, as `(name, value)`; the names are the `{...}` of the message.
    pub fn args(&self) -> Vec<(&'static str, String)> {
        match self {
            NewGameError::Settings(issues) => vec![(
                "why",
                issues
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join("; "),
            )],
            NewGameError::Content(problems) => vec![("why", problems.join("; "))],
            NewGameError::TooManyOpponents { asked, most } => {
                vec![("asked", asked.to_string()), ("most", most.to_string())]
            }
            NewGameError::NoPreset { num } => vec![("num", num.to_string())],
            NewGameError::Wizard(e) => match e {
                BuildError::TooManyPicks { spent } => vec![("spent", spent.to_string())],
                BuildError::UnspentPicks { left } => vec![("left", left.to_string())],
                _ => Vec::new(),
            },
            NewGameError::RaceNotOnPlane { race, .. } | NewGameError::NoRace { race } => {
                vec![("race", race.to_string())]
            }
            NewGameError::Map(e) => vec![("why", e.to_string())],
            _ => Vec::new(),
        }
    }

    /// Every key a message can have, for the test that the texts have them all.
    pub const KEYS: &'static [&'static str] = &[
        "new_game_error.settings",
        "new_game_error.content",
        "new_game_error.too_many_opponents",
        "new_game_error.choices_without_human",
        "new_game_error.no_preset",
        "new_game_error.too_many_picks",
        "new_game_error.unspent_picks",
        "new_game_error.too_many_retorts",
        "new_game_error.retort_not_available",
        "new_game_error.unknown_realm",
        "new_game_error.too_many_books",
        "new_game_error.too_many_books_in_all",
        "new_game_error.bad_spell",
        "new_game_error.wizard_magic",
        "new_game_error.race_not_on_arcanus",
        "new_game_error.race_not_on_myrror",
        "new_game_error.no_races",
        "new_game_error.no_race",
        "new_game_error.map",
        "new_game_error.no_map",
        "new_game_error.no_fortress_place",
    ];
}

impl std::fmt::Display for NewGameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reason = self.text_key().strip_prefix(KEY).unwrap_or("");
        match self {
            NewGameError::Settings(_) | NewGameError::Content(_) | NewGameError::Map(_) => {
                let why = self.args().pop().map(|a| a.1).unwrap_or_default();
                write!(f, "{reason}: {why}")
            }
            NewGameError::Wizard(e) => write!(f, "{reason}: {e:?}"),
            _ => {
                let args: Vec<String> = self
                    .args()
                    .iter()
                    .map(|(k, v)| format!("{k} {v}"))
                    .collect();
                if args.is_empty() {
                    write!(f, "{reason}")
                } else {
                    write!(f, "{reason} ({})", args.join(", "))
                }
            }
        }
    }
}

impl std::error::Error for NewGameError {}

impl From<NewGameError> for String {
    fn from(e: NewGameError) -> String {
        e.to_string()
    }
}

impl From<GenError> for NewGameError {
    fn from(e: GenError) -> Self {
        NewGameError::Map(e)
    }
}

impl From<BuildError> for NewGameError {
    fn from(e: BuildError) -> Self {
        NewGameError::Wizard(e)
    }
}
