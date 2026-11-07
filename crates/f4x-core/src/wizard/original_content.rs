//! The retorts' costs and the ready-made wizards against the original's creation screen.
//!
//! Tests: in this file; the tables are `f4x_def::original`.

use f4x_def::original::plain;

/// The original's retorts ("special abilities") in the order of its wizard record
/// (`wsa_*` of `NewGame.c`), which is the order of `retorts.ron`.
const ORIGINAL_RETORTS: &[&str] = &[
    "Alchemy",
    "Warlord",
    "ChaosMastery",
    "NatureMastery",
    "SorceryMastery",
    "InfernalPower",
    "DivinePower",
    "SageMaster",
    "Channeler",
    "Myrran",
    "Archmage",
    "ManaFocusing",
    "NodeMastery",
    "Famous",
    "Runemaster",
    "Conjurer",
    "Charismatic",
    "Artificer",
];

/// Picks a retort costs in the original's creation screen: one, two for the five of
/// `TBL_2pickRetort_Lbls` (Warlord, Channeler, Divine Power, Famous, Infernal Power), three for Myrran.
fn original_retort_cost(name: &str) -> u8 {
    match name {
        "Warlord" | "Channeler" | "DivinePower" | "Famous" | "InfernalPower" => 2,
        "Myrran" => 3,
        _ => 1,
    }
}

/// Whether the original's creation screen lets the wizard pick the retort with these books
/// (`wsa_picklist_start` and the unlocks after it), not counting the pick budget. `n` is
/// life, death, chaos, nature, sorcery.
fn original_retort_available(name: &str, n: [u8; 5]) -> bool {
    let [life, death, chaos, nature, sorcery] = n;
    let realms = n.iter().filter(|&&b| b > 0).count();
    let strong_realms = n.iter().filter(|&&b| b > 1).count();
    let mastery_book = n.iter().any(|&b| b > 3);
    match name {
        "Alchemy" | "Warlord" | "Channeler" | "Artificer" | "Conjurer" | "Myrran" | "Famous"
        | "Charismatic" => true,
        "SageMaster" => realms > 1,
        "Runemaster" => strong_realms > 2,
        "Archmage" | "ManaFocusing" => mastery_book,
        "ChaosMastery" => chaos > 3,
        "NatureMastery" => nature > 3,
        "SorceryMastery" => sorcery > 3,
        "InfernalPower" => death > 3,
        "DivinePower" => life > 3,
        "NodeMastery" => chaos > 0 && sorcery > 0 && nature > 0,
        other => panic!("{other} is not a retort of the original"),
    }
}

/// A retort's alias as the original's name for it ("sage_master" is "SageMaster").
fn camel(alias: &str) -> String {
    alias
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        })
        .collect()
}

#[test]
fn every_retort_costs_and_unlocks_as_in_the_originals_creation_screen() {
    use f4x_core::wizard::{all_retorts, retort_available, retort_cost};
    let c = f4x_def::testdir::shipped();
    let rules = &c.rules;
    let retorts: Vec<_> = all_retorts(rules).collect();
    assert_eq!(retorts.len(), ORIGINAL_RETORTS.len());
    for (&retort, name) in retorts.iter().zip(ORIGINAL_RETORTS.iter().copied()) {
        assert_eq!(
            camel(&rules.retorts.get(retort).id),
            name,
            "the retort list follows the original's order"
        );
        assert_eq!(
            usize::from(retort.num()),
            retorts
                .iter()
                .position(|&r| r == retort)
                .unwrap_or(usize::MAX),
            "{name}: the number is the place in the original's list"
        );
        assert_eq!(
            retort_cost(rules, retort),
            original_retort_cost(name),
            "{name} cost"
        );
    }
    let realms = [
        3, /* life */
        4, /* death */
        2, /* chaos */
        0, /* nature */
        1, /* sorcery */
    ];
    let mut checked = 0;
    // Every way to spend up to eleven picks on books.
    for life in 0..=11u8 {
        for death in 0..=11 - life {
            for chaos in 0..=11 - life - death {
                for nature in 0..=11 - life - death - chaos {
                    for sorcery in 0..=11 - life - death - chaos - nature {
                        let n = [life, death, chaos, nature, sorcery];
                        let books = realms
                            .iter()
                            .zip(n)
                            .map(|(r, b)| (rules.realms.id_numbered(*r).unwrap(), b))
                            .collect();
                        for (&retort, name) in retorts.iter().zip(ORIGINAL_RETORTS) {
                            assert_eq!(
                                retort_available(rules, retort, &books),
                                original_retort_available(name, n),
                                "{name} with life {life}, death {death}, chaos {chaos}, nature {nature}, sorcery {sorcery}"
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 50_000, "only {checked} cases checked");
}

/// The original's ready-made wizards (`_wizard_presets_table`): name, books in the order life,
/// sorcery, nature, death, chaos, and the retort's place in `ORIGINAL_RETORTS` (none for Raven).
const ORIGINAL_WIZARDS: &[(&str, [u8; 5], Option<usize>)] = &[
    ("Merlin", [5, 0, 5, 0, 0], Some(7)),
    ("Raven", [0, 6, 5, 0, 0], None),
    ("Sharee", [0, 0, 0, 5, 5], Some(15)),
    ("Lo Pan", [0, 5, 0, 0, 5], Some(8)),
    ("Jafar", [0, 10, 0, 0, 0], Some(0)),
    ("Oberic", [0, 0, 5, 0, 5], Some(11)),
    ("Rjak", [0, 0, 0, 9, 0], Some(5)),
    ("Sss'ra", [4, 0, 0, 0, 4], Some(9)),
    ("Tauron", [0, 0, 0, 0, 10], Some(2)),
    ("Freya", [0, 0, 10, 0, 0], Some(3)),
    ("Horus", [5, 5, 0, 0, 0], Some(10)),
    ("Ariel", [10, 0, 0, 0, 0], Some(16)),
    ("Tlaloc", [0, 0, 4, 5, 0], Some(1)),
    ("Kali", [0, 5, 0, 5, 0], Some(17)),
];

#[test]
fn every_ready_made_wizard_has_the_originals_books_and_retort() {
    let c = f4x_def::testdir::shipped();
    let rules = &c.rules;
    let defs = c.rules.wizards.defs();
    assert_eq!(defs.len(), ORIGINAL_WIZARDS.len(), "ready-made wizards");
    let realms = ["life", "sorcery", "nature", "death", "chaos"];
    for (def, (name, books, retort)) in defs.iter().zip(ORIGINAL_WIZARDS) {
        assert_eq!(
            plain(&def.id),
            plain(name),
            "wizards come in the original's order"
        );
        let mut ours: Vec<(&str, u8)> = def
            .books
            .iter()
            .map(|(r, b)| (rules.realms.get(*r).id.as_str(), *b))
            .collect();
        ours.sort_unstable();
        let mut theirs: Vec<(&str, u8)> = realms
            .iter()
            .zip(books.iter().copied())
            .filter(|(_, b)| *b > 0)
            .map(|(r, b)| (*r, b))
            .collect();
        // Lo Pan has one sorcery book fewer than the original (docs/divergences.md).
        if def.id == "lo_pan" {
            theirs.iter_mut().for_each(|(r, b)| {
                if *r == "sorcery" {
                    *b -= 1;
                }
            });
        }
        theirs.sort_unstable();
        assert_eq!(ours, theirs, "{} books", def.id);
        let retorts: Vec<_> = retort
            .map(|i| f4x_core::wizard::magic::Retort::new(rules.retorts.defs()[i].num))
            .into_iter()
            .collect();
        assert_eq!(def.retorts, retorts, "{} retorts", def.id);
    }
}
