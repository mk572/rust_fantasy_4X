//! The names of spell hooks that more than one crate reads.
//!
//! Tests: none, because it only holds names.

/// The name of the hook of the city walls, and its arguments.
pub const WALL: &str = "wall";
/// The hook of the spell that does it (`spells.ron`).
pub const HEROISM: &str = "heroism";
/// The hook of Word of Recall and Recall Hero. Its argument says whom the spell takes: `anyone`
/// (any unit of the wizard) or `hero`. Overland only `anyone` is cast; in a battle both are.
pub const RECALL: &str = "recall";
pub const RECALL_ANYONE: &str = "anyone";
pub const RECALL_HERO: &str = "hero";
