//! What the shipped feature rows say about the sites and minerals the code names by number.

use f4x_core::world_event::ORES;
use f4x_def::feature::FeatureKind;

use crate::shipped::content;

/// The minerals an event can place are minerals, and Transmute swaps pairs of them: what it makes
/// of one makes the first again, and adamantium and the crystals have no change.
#[test]
fn the_ores_and_the_transmute_pairs_are_minerals() {
    let c = content();
    for (num, _) in ORES {
        let id = c.features.id_numbered(num).expect("an ore is a feature");
        assert_eq!(
            c.features.get(id).map(|d| d.kind),
            Some(FeatureKind::Mineral)
        );
    }
    let mut swapping = 0;
    for def in c.features.defs() {
        let Some(to) = def.transmute else { continue };
        swapping += 1;
        let other = c
            .features
            .get(c.features.id_numbered(to).expect("a mineral"));
        assert_eq!(
            other.map(|d| d.kind),
            Some(FeatureKind::Mineral),
            "{}",
            def.id
        );
        assert_eq!(
            other.and_then(|d| d.transmute),
            Some(def.num),
            "{} swaps back",
            def.id
        );
    }
    assert_eq!(swapping, 6);
    for num in [
        2,  /* adamantium_ore */
        17, /* quork_crystal */
        6,  /* crysx_crystal */
    ] {
        let id = c.features.id_numbered(num).expect("a mineral");
        assert_eq!(c.features.get(id).and_then(|d| d.transmute), None);
    }
}

/// A node carries its realm and the book it gives; the temples draw on life, the places of the
/// dead on death, and the other sites give a random book.
#[test]
fn the_sites_say_which_book_they_give() {
    let c = content();
    let reward = |num: u16| {
        let id = c.features.id_numbered(num).expect("a shipped feature");
        c.features.get(id).and_then(|d| d.reward).map(|r| r.num())
    };
    assert_eq!(reward(15 /* nature_node */), Some(0 /* nature */));
    assert_eq!(reward(20 /* sorcery_node */), Some(1 /* sorcery */));
    assert_eq!(reward(4 /* chaos_node */), Some(2 /* chaos */));
    assert_eq!(reward(3 /* ancient_temple */), Some(3 /* life */));
    assert_eq!(reward(8 /* fallen_temple */), Some(3 /* life */));
    for num in [
        7,  /* dungeon */
        1,  /* abandoned_keep */
        18, /* ruins */
    ] {
        assert_eq!(reward(num), Some(4 /* death */), "{num}");
    }
    for num in [
        13, /* monster_lair */
        14, /* mysterious_cave */
        21, /* tower */
    ] {
        assert_eq!(reward(num), None, "{num}");
    }
}
