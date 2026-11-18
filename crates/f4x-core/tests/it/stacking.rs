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
