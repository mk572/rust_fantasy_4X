//! The small pictures of game elements (`crates/f4x-client/content/icon_art.ron`): the picture of
//! each prefab item and of each line of the battle information window. The rules do not know
//! pictures; `asset_check` checks the table against them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

/// The picture of one prefab item.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ItemArt {
    pub id: String,
    /// An entry of the original's `ITEMS.LBX`.
    pub icon: u16,
}

/// The table.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct IconArt {
    /// In the order of the rules' items, so an item's number finds its row.
    pub items: Vec<ItemArt>,
    /// An entry of `COMPIX.LBX` by the key of the line's name.
    pub battle_info: BTreeMap<String, usize>,
}

impl IconArt {
    /// The picture of the prefab item with this number, if the table has it.
    pub fn item(&self, num: u16) -> Option<u16> {
        self.items.get(usize::from(num)).map(|i| i.icon)
    }

    /// The `COMPIX.LBX` entry of a line of the battle information window.
    pub fn battle_info(&self, key: &str) -> Option<usize> {
        self.battle_info.get(key).copied()
    }
}

pub fn parse_icon_art(src: &str) -> Result<IconArt, String> {
    let art: IconArt = ron::from_str(src).map_err(|e| format!("icon_art.ron: {e}"))?;
    let mut seen = BTreeSet::new();
    let twice: Vec<_> = art
        .items
        .iter()
        .filter(|i| !seen.insert(&i.id))
        .map(|i| format!("item \"{}\" twice", i.id))
        .collect();
    if twice.is_empty() {
        Ok(art)
    } else {
        Err(format!("icon_art.ron: {}", twice.join(", ")))
    }
}

/// Read `icon_art.ron` from the first of `dirs` that has it.
pub fn load_icon_art(dirs: &[&Path]) -> Result<IconArt, String> {
    let path = dirs
        .iter()
        .map(|d| d.join("icon_art.ron"))
        .find(|p| p.exists())
        .ok_or_else(|| "icon_art.ron: not found".to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_icon_art(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_reads_and_an_item_twice_is_reported() {
        let ok = "(items: [(id: \"a\", icon: 3)], battle_info: {\"k\": 44})";
        let art = parse_icon_art(ok).expect("parses");
        assert_eq!(art.item(0), Some(3));
        assert_eq!(art.item(1), None);
        assert_eq!(art.battle_info("k"), Some(44));
        let twice = "(items: [(id: \"a\", icon: 3), (id: \"a\", icon: 4)], battle_info: {})";
        assert!(parse_icon_art(twice).is_err());
    }
}
