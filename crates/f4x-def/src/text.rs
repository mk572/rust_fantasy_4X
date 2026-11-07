//! What the crates below the client share about text: the fallback language and the grammatical
//! gender of a name. The messages themselves (`Texts`) are the client's (`f4x-client/src/text.rs`).

use serde::{Deserialize, Serialize};

/// The language that fills in the keys another language lacks.
pub const FALLBACK_LANGUAGE: &str = "en";

/// The grammatical gender of a hero or a wizard, for the messages that name one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Gender {
    Masculine,
    Feminine,
    #[default]
    Neuter,
}
