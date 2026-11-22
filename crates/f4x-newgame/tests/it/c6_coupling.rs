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

// ---- the messages of `NewGameError` ----

/// One of each reason `new_game` can give, with the values it names.
fn every_error() -> Vec<f4x_newgame::error::NewGameError> {
    use f4x_core::wizard::BuildError as B;
    use f4x_newgame::error::NewGameError as E;
    let realm = f4x_core::rules::RealmId::new(0);
    let issue = f4x_def::issue::ContentIssue::new(
        "rules_settings.ron",
        "basic",
        f4x_def::issue::IssueKind::OutOfRange,
    );
    vec![
        E::Settings(vec![issue]),
        E::Content(vec!["a problem".into()]),
        E::TooManyOpponents { asked: 9, most: 5 },
        E::ChoicesWithoutHuman,
        E::NoPreset { num: 7 },
        E::Wizard(B::TooManyPicks { spent: 12 }),
        E::Wizard(B::UnspentPicks { left: 2 }),
        E::Wizard(B::TooManyRetorts),
        E::Wizard(B::RetortNotAvailable(f4x_core::wizard::magic::Retort::new(
            9,
        ))),
        E::Wizard(B::UnknownRealm(realm)),
        E::Wizard(B::TooManyBooks(realm)),
        E::Wizard(B::TooManyBooksInAll),
        E::Wizard(B::Magic(f4x_core::wizard::magic::MagicError::UnknownPlayer)),
        E::Wizard(B::BadSpell(f4x_core::rules::SpellId::new(1))),
        E::RaceNotOnPlane {
            race: 3,
            myrror: false,
        },
        E::RaceNotOnPlane {
            race: 3,
            myrror: true,
        },
        E::NoRaces,
        E::NoRace { race: 4 },
        E::Map(f4x_newgame::mapgen::GenError::BadParams("x".into())),
        E::NoMap,
        E::NoFortressPlace,
    ]
}

fn error_texts() -> std::collections::BTreeMap<String, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../f4x-lang/content/text/en/new_game_error.ron");
    ron::from_str(&std::fs::read_to_string(path).expect("the file")).expect("a map of texts")
}

#[test]
fn every_new_game_error_has_its_message_and_the_message_names_only_what_the_error_gives() {
    let texts = error_texts();
    for e in every_error() {
        let key = e.text_key();
        assert!(
            f4x_newgame::error::NewGameError::KEYS.contains(&key),
            "{key} is not listed"
        );
        let text = texts
            .get(key)
            .unwrap_or_else(|| panic!("{key} has no message"));
        let given: Vec<&str> = e.args().iter().map(|a| a.0).collect();
        let mut rest = text.as_str();
        while let Some(open) = rest.find('{') {
            let close = rest[open..].find('}').expect("a closed brace") + open;
            let name = &rest[open + 1..close];
            assert!(
                given.contains(&name),
                "{key} names {{{name}}}, the error gives {given:?}"
            );
            rest = &rest[close + 1..];
        }
        for name in given {
            assert!(
                text.contains(&format!("{{{name}}}")),
                "{key} does not show {name}"
            );
        }
    }
}

#[test]
fn the_file_of_messages_holds_exactly_the_keys_the_error_can_give() {
    let mut in_file: Vec<String> = error_texts().into_keys().collect();
    let mut listed: Vec<String> = f4x_newgame::error::NewGameError::KEYS
        .iter()
        .map(|k| k.to_string())
        .collect();
    in_file.sort();
    listed.sort();
    assert_eq!(in_file, listed);
    let mut used: Vec<String> = every_error()
        .iter()
        .map(|e| e.text_key().to_string())
        .collect();
    used.sort();
    used.dedup();
    assert_eq!(
        used, listed,
        "a key no error gives, or an error the test does not build"
    );
}
