//! The environment's overrides of an AI's levers: `AIS_<NAME>` for the strategic AI (`lever.rs`),
//! `AIK_<NAME>` for the battle AI (`battle/lever.rs`). The variables of one AI never reach the
//! other.
//!
//! Tests: in `lever.rs` and `battle/lever.rs`.

use f4x_def::rules::lever::OverrideError;

/// An override: the lever's name and the value's text.
pub(crate) type Override = (String, String);

/// How one AI's variables are told apart and reported.
pub(crate) struct Family {
    pub prefix: &'static str,
    /// What its levers are called in a report.
    pub kind: &'static str,
    /// Where the other AI's overrides go.
    pub other: &'static str,
}

pub(crate) const STRATEGIC: Family = Family {
    prefix: "AIS_",
    kind: "strategic",
    other: "the battle AI's overrides are AIK_<NAME>",
};

pub(crate) const BATTLE: Family = Family {
    prefix: "AIK_",
    kind: "battle",
    other: "the strategic AI's overrides are AIS_<NAME>",
};

/// Sets the lever of a set called `name` to what the text says.
pub(crate) type Set<L> = fn(&mut L, &str, &str) -> Result<(), OverrideError>;

/// An AI's variables, by what becomes of them.
#[derive(Debug, Default)]
pub(crate) struct Sorted {
    /// Those that override a lever.
    pub taken: Vec<Override>,
    /// Those that name a lever but give it no value it can take.
    pub refused: Vec<Override>,
    /// The names that are no lever's.
    pub unknown: Vec<String>,
}

impl Sorted {
    /// A line for each variable that is not taken.
    pub fn warnings(&self, family: &Family) -> Vec<String> {
        let Family {
            prefix,
            kind,
            other,
        } = family;
        let refused = self.refused.iter().map(|(name, text)| {
            format!("{prefix}{name}={text} is no value for this lever; the content's is used")
        });
        let unknown = self
            .unknown
            .iter()
            .map(|name| format!("{prefix}{name} names no {kind} lever; it is ignored ({other})"));
        refused.chain(unknown).collect()
    }
}

/// Sort the variables that start with the family's prefix; the others are not its AI's.
pub(crate) fn sort_out<L: Clone>(
    family: &Family,
    levers: &L,
    set: Set<L>,
    vars: impl Iterator<Item = (String, String)>,
) -> Sorted {
    let mut sorted = Sorted::default();
    for (var, text) in vars {
        let Some(name) = var.strip_prefix(family.prefix) else {
            continue;
        };
        match set(&mut levers.clone(), name, &text) {
            Ok(()) => sorted.taken.push((name.to_string(), text)),
            Err(OverrideError::BadValue) => sorted.refused.push((name.to_string(), text)),
            Err(OverrideError::UnknownLever) => sorted.unknown.push(name.to_string()),
        }
    }
    sorted
}

/// The family's overrides in the environment; what cannot be taken is reported on stderr.
pub(crate) fn from_environment<L: Clone>(
    family: &Family,
    levers: &L,
    set: Set<L>,
) -> Vec<Override> {
    let vars = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.to_string_lossy().into_owned())));
    let sorted = sort_out(family, levers, set, vars);
    for warning in sorted.warnings(family) {
        eprintln!("{warning}");
    }
    sorted.taken
}

/// `levers` with each of the overrides that it takes.
pub(crate) fn overridden<L>(mut levers: L, set: Set<L>, overrides: &[Override]) -> L {
    for (name, text) in overrides {
        // `sort_out` kept only those that are taken, and one that is not changes nothing.
        if set(&mut levers, name, text).is_err() {
            continue;
        }
    }
    levers
}
