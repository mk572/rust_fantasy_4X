//! The Rust code the content points at: the `Effect::Custom` hooks (a tag in the `.ron` file) and
//! the spells with an `Effect::Special` of their own. The definitions know nothing of this code;
//! the code lists what it implements and [`problems`] checks the loaded content against it.
//!
//! Tests: below, on the shipped content.

use crate::rules::effects::{Effect, EffectRule};
use crate::rules::Rules;

/// Declares the enum of the effects a hook's arguments name (`GlobalEffect`, `CityEffect`): the
/// variants with their argument strings, `NAMES` for the hook table, `name`, `named`, and
/// `is_in(def)`, whether a spell's definition carries the effect in the given hook.
macro_rules! effect_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident in $hook:expr; $($variant:ident = $text:literal),+ $(,)?) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $name {
            $($variant),+
        }

        impl $name {
            /// The hook arguments, in the enum's order.
            pub const NAMES: [&'static str; [$($text),+].len()] = [$($text),+];

            const ALL: [$name; [$($text),+].len()] = [$($name::$variant),+];

            /// The hook argument for this effect.
            pub fn name(self) -> &'static str {
                Self::NAMES[self as usize]
            }

            /// The effect a hook argument names.
            pub fn named(name: &str) -> Option<$name> {
                Self::NAMES.iter().position(|n| *n == name).map(|i| Self::ALL[i])
            }

            /// Whether the spell carries this effect.
            pub fn is_in(self, def: &crate::rules::SpellDef) -> bool {
                def.hook_args($hook)
                    .is_some_and(|a| a.iter().any(|x| x == self.name()))
            }
        }
    };
}
pub(crate) use effect_enum;

/// A hook the Rust code implements: its name and the arguments it takes. `args` empty means the
/// hook takes none; otherwise it takes one of these, or, with `several`, one or more.
pub struct Hook {
    pub name: &'static str,
    pub args: &'static [&'static str],
    pub several: bool,
}

/// The name of the hook of the city walls, and its arguments.
pub const WALL: &str = "wall";

const fn plain(name: &'static str) -> Hook {
    Hook {
        name,
        args: &[],
        several: false,
    }
}

/// The `Effect::Custom` hooks implemented in Rust, shared by several spells. A `Custom` effect
/// naming anything else, or giving arguments the hook does not take, fails [`problems`].
///
/// `unimplemented` marks an imported rule the engine has no primitive for yet; the
/// number of uses is the to-do list for new effect primitives.
pub const CUSTOM_HOOKS: &[Hook] = &[
    plain("unimplemented"),
    plain(crate::item::crafting::CRAFT_ITEM),
    Hook {
        name: crate::city::gate::GATE,
        args: &[
            crate::city::gate::EARTH_GATE,
            crate::city::gate::ASTRAL_GATE,
        ],
        several: false,
    },
    plain(crate::unit::HEROISM),
    Hook {
        name: crate::spell::city::CITY_EFFECT,
        args: &crate::spell::city::CityEffect::NAMES,
        several: true,
    },
    Hook {
        name: crate::battle::field::BATTLEFIELD,
        args: &crate::battle::field::Field::NAMES,
        several: false,
    },
    Hook {
        name: crate::spell::terraform::TERRAIN_EFFECT,
        args: &crate::spell::terraform::TerrainEffect::NAMES,
        several: false,
    },
    Hook {
        name: crate::spell::global_enchantment::GLOBAL_EFFECT,
        args: &crate::spell::global_enchantment::GlobalEffect::NAMES,
        several: true,
    },
    Hook {
        name: crate::battle::spell::BATTLE_EFFECT,
        args: &crate::battle::spell::BattleEffect::NAMES,
        several: false,
    },
    plain(crate::battle::field::DISINTEGRATE),
    Hook {
        name: WALL,
        args: &["stone", "fire", "darkness"],
        several: false,
    },
];

/// The spells that have code of their own in Rust (`Effect::Special`), by number: the
/// number is the only link between the spell and its code. A `Special` effect on a spell
/// not in this list fails [`problems`].
pub const SPECIAL_SPELLS: [u16; 31] = [
    19,  // natures_cures
    26,  // earthquake
    28,  // move_fortress
    47,  // word_of_recall
    58,  // spell_blast
    61,  // disjunction_true
    68,  // stasis
    72,  // spell_ward
    73,  // creature_binding
    75,  // great_unsummoning
    76,  // spell_binding
    89,  // warp_creature
    93,  // chaos_channels
    119, // call_the_void
    132, // plane_shift
    133, // resurrection
    142, // incarnation
    171, // drain_power
    172, // possession
    173, // lycanthropy
    177, // subversion
    187, // black_wind
    191, // cruel_unminding
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
            if let Effect::Custom { name, args } = &rule.effect {
                let Some(hook) = CUSTOM_HOOKS.iter().find(|h| h.name == name) else {
                    p.push(format!("{owner}: unknown custom hook {name:?}"));
                    continue;
                };
                let ok = if hook.args.is_empty() {
                    args.is_empty()
                } else {
                    (hook.several || args.len() == 1)
                        && !args.is_empty()
                        && args.iter().all(|a| hook.args.contains(&a.as_str()))
                };
                if !ok {
                    p.push(format!(
                        "{owner}: hook {name:?} takes {}, not {args:?}",
                        if hook.args.is_empty() {
                            "no arguments".to_string()
                        } else {
                            format!("one of {:?}", hook.args)
                        }
                    ));
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
    fn every_global_city_and_field_effect_is_given_by_a_shipped_spell() {
        use crate::battle::field::Field;
        use crate::spell::global_enchantment::GlobalEffect;
        let c = shipped();
        for name in GlobalEffect::NAMES {
            let e = GlobalEffect::named(name).expect("a listed name");
            assert!(
                c.rules.spells.defs().iter().any(|d| e.is_in(d)),
                "no spell gives the global effect {name}"
            );
        }
        for name in crate::spell::city::CityEffect::NAMES {
            let e = crate::spell::city::CityEffect::named(name).expect("a listed name");
            assert!(
                c.rules.spells.defs().iter().any(|d| e.is_in(d)),
                "no spell gives the city effect {name}"
            );
        }
        for name in crate::spell::terraform::TerrainEffect::NAMES {
            let e = crate::spell::terraform::TerrainEffect::named(name).expect("a listed name");
            assert!(
                c.rules.spells.defs().iter().any(|d| e.is_in(d)),
                "no spell gives the terrain effect {name}"
            );
        }
        for name in crate::battle::spell::BattleEffect::NAMES {
            let e = crate::battle::spell::BattleEffect::named(name).expect("a listed name");
            assert!(
                c.rules
                    .spells
                    .defs()
                    .iter()
                    .any(|d| crate::battle::spell::BattleEffect::of(d) == Some(e)),
                "no spell gives the battle effect {name}"
            );
        }
        for name in Field::NAMES {
            let f = Field::named(name).expect("a listed name");
            assert!(
                c.rules
                    .spells
                    .defs()
                    .iter()
                    .any(|d| Field::of(d) == Some(f)),
                "no spell gives the battle field {name}"
            );
        }
    }

    #[test]
    fn a_hook_with_arguments_it_does_not_take_is_reported() {
        let mut c = shipped();
        let wall = c
            .rules
            .spells
            .defs()
            .iter()
            .position(|s| s.id == "wall_of_fire")
            .expect("wall of fire is shipped");
        for (args, bad) in [(vec!["lava"], true), (vec![], true), (vec!["fire"], false)] {
            let mut defs = c.rules.spells.defs().to_vec();
            defs[wall].effects[0].effect = Effect::Custom {
                name: WALL.into(),
                args: args.iter().map(|a| a.to_string()).collect(),
            };
            c.rules.spells = crate::rules::Registry::new(defs);
            let p = problems(&c.rules);
            assert_eq!(
                p.iter().any(|m| m.contains("takes")),
                bad,
                "{args:?}: {p:?}"
            );
        }
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
        defs[healing].effects[0].effect = Effect::hook("mystery");
        c.rules.spells = crate::rules::Registry::new(defs);
        let p = problems(&c.rules);
        assert!(p.iter().any(|m| m.contains("unknown custom hook")), "{p:?}");
    }
}
