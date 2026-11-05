//! The alias of a definition (the `id` of its entry in the rules files) back to its number, for
//! the client's own tables and developer flags that are written by hand with the alias. The game
//! itself refers to every definition by its number.
//!
//! Tests: none of their own; `asset_check` and the developer flags use it.

use f4x_core::rules::ids::Id;
use f4x_core::rules::{Def, Registry};

/// The definition whose alias is exactly `alias`.
pub fn id_of<T: Def>(registry: &Registry<T>, alias: &str) -> Option<Id<T>> {
    registry
        .iter()
        .find(|(_, d)| d.id() == alias)
        .map(|(id, _)| id)
}
