//! The spells that cannot be combined, as the shipped spell data names them (`spell/stacking.rs`).

fn content() -> f4x_core::content::Content {
    f4x_data::load_content(std::path::Path::new("../../content")).expect("the rules data loads")
}

#[test]
fn the_names_in_not_with_and_already_able_exist() {
    let c = content();
    for (_, def) in c.rules.spells.iter() {
        for other in &def.not_with {
            assert!(
                c.rules.spells.id_of(other).is_some(),
                "{} is not cumulative with {other}, which is no spell",
                def.id
            );
        }
        for ability in &def.already_able {
            assert!(
                c.rules.abilities.id_of(ability).is_some(),
                "{} refuses units with {ability}, which is no ability",
                def.id
            );
        }
    }
}

/// The pairs and the nine spells the original refuses (`OverSpel.c`, `SBookScr.c`); Resist Magic
/// with Magic Immunity is the third pair its message names.
#[test]
fn the_original_refusals_are_in_the_data() {
    let c = content();
    let spell = |id: &str| c.rules.spells.get(c.rules.spells.id_of(id).unwrap());
    for (id, rival) in [
        ("stone_skin", "iron_skin"),
        ("resist_elements", "elemental_armor"),
        ("resist_magic", "magic_immunity"),
        ("holy_weapon", "holy_arms"),
    ] {
        assert_eq!(spell(id).not_with, [rival], "{id}");
    }
    for (id, ability) in [
        ("immolation", "immolation"),
        ("cloak_of_fear", "cause_fear"),
        ("true_sight", "illusion_immunity"),
        ("magic_immunity", "magic_immunity"),
        ("planar_travel", "planar_travel"),
        ("invisiblity", "invisibility"),
        ("wraith_form", "non_corporeal"),
        ("wind_walking", "wind_walking"),
        ("regeneration", "regeneration"),
    ] {
        assert_eq!(spell(id).already_able, [ability], "{id}");
    }
}
