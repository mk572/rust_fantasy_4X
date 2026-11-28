//! The Rust code the content points at: the `Effect::Custom` hooks (a tag with arguments in the
//! `.ron` file). The definitions know nothing of this code;
//! the code lists what it implements and [`problems`] checks the loaded content against it.
//!
//! Tests: `tests/it/c9_hook.rs`.

use crate::rules::effects::{Effect, EffectRule};
use crate::rules::Rules;

pub(crate) use f4x_def::effect_enum;

/// A hook the Rust code implements: its name and the arguments it takes. `args` empty means the
/// hook takes none; otherwise it takes one of these, or, with `several`, one or more.
pub struct Hook {
    pub name: &'static str,
    pub args: &'static [&'static str],
    pub several: bool,
    /// After the names, the hook takes one number (a strength): `args: ["dispel", "10"]`.
    pub number: bool,
}

pub use f4x_def::rules::hook_name::WALL;

const fn plain(name: &'static str) -> Hook {
    Hook {
        name,
        args: &[],
        several: false,
        number: false,
    }
}

/// The first spell of the rules that runs the named hook, for the code that has to find "the Spell
/// of Mastery" and the like.
pub fn spell_with_hook(rules: &Rules, hook: &str) -> Option<crate::rules::SpellId> {
    rules
        .spells
        .defs()
        .iter()
        .find(|d| d.has_hook(hook))
        .map(|d| crate::rules::SpellId::new(d.num))
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
        number: false,
    },
    plain(crate::unit::HEROISM),
    Hook {
        name: crate::spell::city::CITY_EFFECT,
        args: crate::spell::city::CityEffect::NAMES,
        several: true,
        number: false,
    },
    Hook {
        name: crate::battle::field::BATTLEFIELD,
        args: crate::battle::field::Field::NAMES,
        several: false,
        number: false,
    },
    Hook {
        name: crate::spell::terraform::TERRAIN_EFFECT,
        args: crate::spell::terraform::TerrainEffect::NAMES,
        several: false,
        number: false,
    },
    Hook {
        name: crate::spell::global_enchantment::GLOBAL_EFFECT,
        args: crate::spell::global_enchantment::GlobalEffect::NAMES,
        several: true,
        number: false,
    },
    Hook {
        name: crate::battle::spell::BATTLE_EFFECT,
        args: crate::battle::spell::BattleEffect::NAMES,
        several: false,
        number: false,
    },
    Hook {
        name: crate::spell::special::RECALL,
        args: &[
            crate::spell::special::RECALL_ANYONE,
            crate::spell::special::RECALL_HERO,
        ],
        several: false,
        number: false,
    },
    plain(crate::spell::special::PLANE_SHIFT),
    Hook {
        name: crate::spell::special::MOVE_BUILDING,
        args: &[
            crate::spell::special::MOVE_FORTRESS,
            crate::spell::special::MOVE_CIRCLE,
        ],
        several: false,
        number: false,
    },
    Hook {
        name: crate::spell::wizard::WIZARD_EFFECT,
        args: crate::spell::wizard::WizardEffect::NAMES,
        several: false,
        number: false,
    },
    Hook {
        name: crate::spell::unit_effect::UNIT_EFFECT,
        args: crate::spell::unit_effect::UnitEffect::NAMES,
        several: true,
        number: false,
    },
    Hook {
        name: crate::spell::dispel::COUNTER_EFFECT,
        args: crate::spell::dispel::CounterEffect::NAMES,
        several: false,
        number: true,
    },
    Hook {
        name: crate::spell::hero::SUMMON_HERO,
        args: crate::spell::hero::HeroSummon::NAMES,
        several: false,
        number: false,
    },
    Hook {
        name: crate::spell::wizard::CITY_STRIKE,
        args: crate::spell::wizard::CityStrike::NAMES,
        several: false,
        number: false,
    },
    Hook {
        name: crate::spell::wizard::WORLD_KILL,
        args: crate::spell::wizard::WorldKill::NAMES,
        several: false,
        number: false,
    },
    plain(crate::spell::special::SPELL_OF_MASTERY),
    plain(crate::spell::special::SPELL_OF_RETURN),
    Hook {
        name: WALL,
        args: &["stone", "fire", "darkness"],
        several: false,
        number: false,
    },
];

/// What is wrong between the content and the code: a `Custom` hook no code implements, or one
/// given arguments it does not take.
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
                let is_number = |a: &String| a.parse::<u32>().is_ok_and(|n| n > 0);
                let numbers = args.iter().filter(|a| is_number(a)).count();
                let names: Vec<&String> = args.iter().filter(|a| !is_number(a)).collect();
                let ok = if hook.args.is_empty() {
                    args.is_empty()
                } else {
                    (hook.several || names.len() == 1)
                        && !names.is_empty()
                        && names.iter().all(|a| hook.args.contains(&a.as_str()))
                        && numbers <= usize::from(hook.number)
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
    p
}
