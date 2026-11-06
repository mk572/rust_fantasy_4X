//! The grammatical gender the definitions carry (heroes and wizards). The messages that use it are
//! the client's (task 479).

use serde::{Deserialize, Serialize};

/// The grammatical gender of a hero or a wizard, for the messages that name one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Gender {
    Masculine,
    Feminine,
    #[default]
    Neuter,
}
