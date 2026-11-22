//! Class 6 (`docs/f4x-newgame-test-design.md`): the numbers the crate names lead to the rows of
//! their aliases in the shipped content, and every number in the source is on the list the check
//! of `new_game` uses.

use crate::support::*;
use f4x_core::map::terrain::TerrainClass;
use f4x_def::feature::FeatureKind;
use f4x_newgame::coupling::{problems, REALMS, RETORTS};

#[test]
fn the_shipped_content_agrees_with_every_number_the_crate_names() {
    assert_eq!(problems(&content().rules), Vec::<String>::new());
}

#[test]
fn the_lists_name_each_number_and_each_alias_once() {
    for (name, list) in [("retorts", RETORTS), ("realms", REALMS)] {
        let nums: std::collections::BTreeSet<_> = list.iter().map(|e| e.0).collect();
        let aliases: std::collections::BTreeSet<_> = list.iter().map(|e| e.1).collect();
        assert_eq!(nums.len(), list.len(), "{name}: a number twice");
        assert_eq!(aliases.len(), list.len(), "{name}: an alias twice");
    }
}

#[test]
fn a_renumbered_or_renamed_row_is_found_by_the_check() {
    // The check can fail: the shipped rules with the Myrran retort renamed, a realm renumbered.
    let mut rules = content().rules.clone();
    let myrran = rules
        .retorts
        .defs()
        .iter()
        .position(|d| d.id == "myrran")
        .expect("the Myrran retort");
    let mut defs = rules.retorts.defs().to_vec();
    defs[myrran].id = "not_myrran".into();
    rules.retorts = f4x_def::rules::Registry::try_new(defs).expect("a registry");
    let found = problems(&rules);
    assert!(found.iter().any(|p| p.contains("myrran")), "{found:?}");
}

#[test]
fn every_realm_with_books_in_the_content_is_one_the_crate_names() {
    // A sixth book realm in the content would be left out of the wizards the crate makes.
    let named: Vec<u16> = REALMS.iter().map(|e| e.0).collect();
    for realm in f4x_def::rules::BOOK_REALMS {
        assert!(named.contains(&realm.num()), "realm {}", realm.num());
    }
    assert_eq!(f4x_def::rules::BOOK_REALMS.len(), REALMS.len());
}

#[test]
fn every_node_of_the_content_has_a_realm_with_a_ground_of_its_own_on_both_planes() {
    let c = content();
    let mut nodes = 0;
    for def in c
        .features
        .defs()
        .iter()
        .filter(|d| d.kind == FeatureKind::Node)
    {
        nodes += 1;
        let realm = def
            .realm
            .unwrap_or_else(|| panic!("{} has no realm", def.id));
        let class = TerrainClass::of_node_realm(realm)
            .unwrap_or_else(|| panic!("{}: realm {} has no node ground", def.id, realm.num()));
        for plane in f4x_core::map::Plane::all() {
            assert!(
                c.terrains.first_of_class(class, plane).is_some(),
                "{}: no {class:?} on {plane:?}",
                def.id
            );
        }
    }
    assert!(nodes > 0);
}

/// Every `Retort::new(N)` and `RealmId::new(N)` of the crate's source (outside its inline tests):
/// the number, and the alias written beside it.
fn numbers_in_source() -> Vec<(String, &'static str, u16, Option<String>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    let mut files = vec![dir];
    while let Some(path) = files.pop() {
        if path.is_dir() {
            for e in std::fs::read_dir(&path).expect("the source folder") {
                files.push(e.expect("an entry").path());
            }
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a source file");
        // The inline tests of a module are not the code.
        let code = text.split("#[cfg(test)]").next().unwrap_or("");
        // Comment lines talk about the numbers; only the code uses them.
        let code: String = code
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let code = code.as_str();
        for (kind, needle) in [("retort", "Retort::new("), ("realm", "RealmId::new(")] {
            let mut rest = code;
            while let Some(at) = rest.find(needle) {
                rest = &rest[at + needle.len()..];
                let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                if digits.is_empty() {
                    continue;
                }
                let after = &rest[digits.len()..];
                // The alias is the first comment within the next line or so.
                let near: String = after.chars().take(60).collect();
                let alias = near.find("/*").and_then(|a| {
                    let inner = &near[a + 2..];
                    inner.find("*/").map(|b| inner[..b].trim().to_string())
                });
                found.push((
                    path.file_name()
                        .expect("a name")
                        .to_string_lossy()
                        .into_owned(),
                    kind,
                    digits.parse().expect("a number"),
                    alias,
                ));
            }
        }
    }
    found
}

#[test]
fn every_number_in_the_source_is_on_the_list_with_the_alias_written_beside_it() {
    let uses = numbers_in_source();
    assert!(uses.len() > 20, "the scan found {} numbers", uses.len());
    for (file, kind, num, alias) in uses {
        let list = if kind == "retort" { RETORTS } else { REALMS };
        let Some(&(_, listed)) = list.iter().find(|e| e.0 == num) else {
            panic!("{file}: {kind} {num} is not on the list of numbers the check covers");
        };
        assert_eq!(
            alias.as_deref(),
            Some(listed),
            "{file}: {kind} {num} is {listed}; the comment beside it says {alias:?}"
        );
    }
}
