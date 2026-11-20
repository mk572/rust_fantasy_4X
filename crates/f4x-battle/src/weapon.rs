//! What a unit's abilities make of its weapon and its casting, from the rules.
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use f4x_def::rules::damage_type::{AbilityAttack, DamageType, RangedKind};
use f4x_def::rules::{Ability, Rules};

/// The kind of the weapon shot the abilities give (a bow, sling, boulder or thrown weapon: a ranged
/// attack of the mundane weapon type), `None` for no such shot. A breath, gaze or bolt is not one.
pub fn weapon_shot_of(abilities: &[Ability], rules: &Rules) -> Option<RangedKind> {
    weapon_attack_of(abilities, rules).map(|attack| attack.kind)
}

/// The attack of that weapon shot.
pub fn weapon_attack_of<'a>(abilities: &[Ability], rules: &'a Rules) -> Option<&'a AbilityAttack> {
    abilities
        .iter()
        .filter_map(|&a| rules.abilities.get(a).attack.as_ref())
        .find(|attack| attack.types.contains(&DamageType::MundaneWeapon))
}

/// The mana the abilities make a spell caster of: the most any of them gives, 0 for none.
pub fn caster_mana_of(abilities: &[Ability], rules: &Rules) -> i32 {
    abilities
        .iter()
        .map(|&a| rules.abilities.get(a).caster_mana)
        .max()
        .unwrap_or(0)
}
