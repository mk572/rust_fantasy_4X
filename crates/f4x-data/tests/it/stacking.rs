//! The spells that cannot be combined, as the shipped spell data names them (`spell/stacking.rs`).

use crate::shipped::content;

#[test]
fn the_names_in_not_with_and_already_able_exist() {
    let c = content();
    for (_, def) in c.rules.spells.iter() {
        for other in &def.not_with {
            assert!(
                c.rules.spells.try_get(*other).is_some(),
                "{} is not cumulative with {}, which is no spell",
                def.id,
                other.num()
            );
        }
        for ability in &def.already_able {
            assert!(
                c.rules.abilities.try_get(*ability).is_some(),
                "{} refuses units with {}, which is no ability",
                def.id,
                ability.num()
            );
        }
    }
}

/// The pairs and the nine spells the original refuses (`OverSpel.c`, `SBookScr.c`); Resist Magic
/// with Magic Immunity is the third pair its message names.
#[test]
fn the_original_refusals_are_in_the_data() {
    let c = content();
    let spell = |id: u16| {
        c.rules
            .spells
            .get(c.rules.spells.id_numbered(id).expect("the spell exists"))
    };
    for (id, rival) in [
        (7 /* stone_skin */, 24 /* iron_skin */),
        (2 /* resist_elements */, 21 /* elemental_armor */),
        (41 /* resist_magic */, 69 /* magic_immunity */),
        (124 /* holy_weapon */, 154 /* holy_arms */),
    ] {
        let refused: Vec<u16> = spell(id).not_with.iter().map(|s| s.num()).collect();
        assert_eq!(refused, [rival], "{id}");
    }
    for (id, ability) in [
        (99 /* immolation */, 32 /* immolation */),
        (164 /* cloak_of_fear */, 8 /* cause_fear */),
        (131 /* true_sight */, 31 /* illusion_immunity */),
        (69 /* magic_immunity */, 40 /* magic_immunity */),
        (138 /* planar_travel */, 51 /* planar_travel */),
        (62 /* invisibility */, 33 /* invisibility */),
        (181 /* wraith_form */, 49 /* non_corporeal */),
        (63 /* wind_walking */, 74 /* wind_walking */),
        (31 /* regeneration */, 55 /* regeneration */),
    ] {
        let refused: Vec<u16> = spell(id).already_able.iter().map(|a| a.num()).collect();
        assert_eq!(refused, [ability], "{id}");
    }
}
