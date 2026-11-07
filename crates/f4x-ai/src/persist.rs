//! Writing the AI's plans to a save: maps go out in key order, so the same plans always give the
//! same text (a save has a checksum, and two saves are compared).

use std::collections::HashMap;
use std::hash::Hash;

use serde::{Serialize, Serializer};

/// A `HashMap` as a map sorted by the text of its keys.
pub(crate) fn sorted<K, V, S>(map: &HashMap<K, V>, out: S) -> Result<S::Ok, S::Error>
where
    K: Serialize + Eq + Hash,
    V: Serialize,
    S: Serializer,
{
    let mut entries: Vec<(String, &K, &V)> = map
        .iter()
        .map(|(k, v)| (ron::ser::to_string(k).unwrap_or_default(), k, v))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    out.collect_map(entries.into_iter().map(|(_, k, v)| (k, v)))
}
