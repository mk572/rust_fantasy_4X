//! The Rust code the content points at: the `Effect::Custom` hooks (a tag in the `.ron` file) and
//! the spells with an `Effect::Special` of their own. The definitions know nothing of this code;
//! the code lists what it implements and [`problems`] checks the loaded content against it.
//!
//! Tests: below, on the shipped content.

use crate::rules::effects::{Effect, EffectRule};
use crate::rules::Rules;

/// Names of the `Effect::Custom` hooks implemented in Rust, shared by several spells. A
/// `Custom` effect naming anything else fails [`problems`].
///
/// `unimplemented` marks an imported rule the engine has no primitive for yet; the
/// number of uses is the to-do list for new effect primitives.
pub const CUSTOM_HOOKS: &[&str] = &[
    "unimplemented",
    crate::spell::global_enchantment::GLOBAL_EFFECT,
    crate::item::crafting::CRAFT_ITEM,
    crate::city::gate::EARTH_GATE,
    crate::city::gate::ASTRAL_GATE,
    crate::unit::HEROISM,
    crate::spell::city::CITY_FLAG,
    crate::battle::field::BATTLEFIELD,
    crate::battle::field::DISINTEGRATE,
];

/// The spells that have code of their own in Rust (`Effect::Special`), by number: the
/// number is the only link between the spell and its code. A `Special` effect on a spell
/// not in this list fails [`problems`].
pub const SPECIAL_SPELLS: [u16; 50] = [
    1,   // earth_to_mud
    3,   // wall_of_stone
    10,  // earth_lore
    11,  // cracks_call
    15,  // change_terrain
    18,  // transmute
    19,  // natures_cures
    26,  // earthquake
    28,  // move_fortress
    43,  // floating_island
    47,  // word_of_recall
    48,  // counter_magic
    55,  // enchant_road
    58,  // spell_blast
    61,  // disjunction_true
    68,  // stasis
    72,  // spell_ward
    73,  // creature_binding
    75,  // great_unsummoning
    76,  // spell_binding
    81,  // warp_wood
    82,  // disrupt
    85,  // corruption
    89,  // warp_creature
    93,  // chaos_channels
    98,  // raise_volcano
    105, // magic_vortex
    115, // call_chaos
    119, // call_the_void
    132, // plane_shift
    133, // resurrection
    137, // raise_dead
    142, // incarnation
    167, // life_drain
    171, // drain_power
    172, // possession
    173, // lycanthropy
    177, // subversion
    186, // warp_node
    187, // black_wind
    191, // cruel_unminding
    195, // animate_dead
    199, // death_wish
    203, // summoning_circle
    205, // recall_hero
    208, // summon_hero
    210, // disjunction
    212, // summon_champion
    213, // spell_of_mastery
    214, // spell_of_return
];

/// What is wrong between the content and the code: a `Custom` hook no code implements, a `Special`
/// effect on a spell with no code of its own, and the reverse, a spell in [`SPECIAL_SPELLS`] the
/// content does not give a `Special`.
pub fn problems(rules: &Rules) -> Vec<String> {
    let mut p = Vec::new();
    let mut carriers: Vec<(String, &[EffectRule])> = Vec::new();
    carriers.extend(
        rules
            .races
            .defs()
            .iter()
            .map(|d| (format!("race {:?}", d.id), &d.effects[..])),
    );
    carriers.extend(
        rules
            .abilities
            .defs()
            .iter()
            .map(|d| (format!("ability {:?}", d.id), &d.effects[..])),
    );
    carriers.extend(
        rules
            .buildings
            .defs()
            .iter()
            .map(|d| (format!("building {:?}", d.id), &d.effects[..])),
    );
    carriers.extend(
        rules
            .spells
            .defs()
            .iter()
            .map(|d| (format!("spell {:?}", d.id), &d.effects[..])),
    );
    carriers.extend(
        rules
            .items
            .defs()
            .iter()
            .map(|d| (format!("item {:?}", d.id), &d.effects[..])),
    );
    for path in rules.level_paths.defs() {
        for (i, level) in path.levels.iter().enumerate() {
            carriers.push((
                format!("level path {:?} level {i}", path.id),
                &level.effects[..],
            ));
        }
    }
    for (owner, effects) in carriers {
        for rule in effects {
            if let Effect::Custom(name) = &rule.effect {
                if !CUSTOM_HOOKS.contains(&name.as_str()) {
                    p.push(format!("{owner}: unknown custom hook {name:?}"));
                }
            }
        }
    }
    for spell in rules.spells.defs() {
        let special = spell
            .effects
            .iter()
            .any(|e| matches!(e.effect, Effect::Special));
        if special && !SPECIAL_SPELLS.contains(&spell.num) {
            p.push(format!(
                "spell {:?} has Special code but no Rust code is written for its number",
                spell.id
            ));
        }
        if !special && SPECIAL_SPELLS.contains(&spell.num) {
            p.push(format!(
                "spell {:?} has Rust code but no Special effect",
                spell.id
            ));
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::shipped;

    #[test]
    fn the_shipped_content_and_the_code_agree() {
        let c = shipped();
        let p = problems(&c.rules);
        assert!(p.is_empty(), "{p:?}");
    }

    #[test]
    fn a_hook_without_code_is_reported() {
        let mut c = shipped();
        let healing = c
            .rules
            .spells
            .defs()
            .iter()
            .position(|s| s.id == "healing")
            .expect("healing is shipped");
        let mut defs = c.rules.spells.defs().to_vec();
        defs[healing].effects[0].effect = Effect::Custom("mystery".into());
        c.rules.spells = crate::rules::Registry::new(defs);
        let p = problems(&c.rules);
        assert!(p.iter().any(|m| m.contains("unknown custom hook")), "{p:?}");
    }
}
