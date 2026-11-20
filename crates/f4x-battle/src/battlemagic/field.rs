//! The battlefield enchantments: spells that act on the whole field, each cast by one side. What they
//! do to a battle is in `f4x-core`'s `battle/field.rs`.
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use f4x_def::rules::SpellDef;

/// Hooks the spell data names.
pub const BATTLEFIELD: &str = "battlefield";
pub const DISINTEGRATE: &str = "disintegrate";

/// The battlefield enchantments.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, strum::EnumIter,
)]
pub enum Field {
    TrueLight,
    Darkness,
    WarpReality,
    BlackPrayer,
    Wrack,
    MetalFires,
    Prayer,
    HighPrayer,
    Terror,
    CallLightning,
    MassInvisibility,
    Entangle,
    ManaLeak,
    Blur,
}

impl Field {
    /// The argument of the `battlefield` hook for each enchantment, in the enum's order.
    pub const NAMES: &[&'static str] = &[
        "true_light",
        "darkness",
        "warp_reality",
        "black_prayer",
        "wrack",
        "metal_fires",
        "prayer",
        "high_prayer",
        "terror",
        "call_lightning",
        "mass_invisibility",
        "entangle",
        "mana_leak",
        "blur",
    ];

    /// The argument of the `battlefield` hook for this enchantment.
    pub fn name(self) -> &'static str {
        match self {
            Field::TrueLight => "true_light",
            Field::Darkness => "darkness",
            Field::WarpReality => "warp_reality",
            Field::BlackPrayer => "black_prayer",
            Field::Wrack => "wrack",
            Field::MetalFires => "metal_fires",
            Field::Prayer => "prayer",
            Field::HighPrayer => "high_prayer",
            Field::Terror => "terror",
            Field::CallLightning => "call_lightning",
            Field::MassInvisibility => "mass_invisibility",
            Field::Entangle => "entangle",
            Field::ManaLeak => "mana_leak",
            Field::Blur => "blur",
        }
    }

    /// The enchantment named by a hook argument.
    pub fn named(name: &str) -> Option<Field> {
        match name {
            "true_light" => Some(Field::TrueLight),
            "darkness" => Some(Field::Darkness),
            "warp_reality" => Some(Field::WarpReality),
            "black_prayer" => Some(Field::BlackPrayer),
            "wrack" => Some(Field::Wrack),
            "metal_fires" => Some(Field::MetalFires),
            "prayer" => Some(Field::Prayer),
            "high_prayer" => Some(Field::HighPrayer),
            "terror" => Some(Field::Terror),
            "call_lightning" => Some(Field::CallLightning),
            "mass_invisibility" => Some(Field::MassInvisibility),
            "entangle" => Some(Field::Entangle),
            "mana_leak" => Some(Field::ManaLeak),
            "blur" => Some(Field::Blur),
            _ => None,
        }
    }

    /// The enchantment a spell puts up: the argument of its `battlefield` hook.
    pub fn of(def: &SpellDef) -> Option<Field> {
        Field::named(def.hook_args(BATTLEFIELD)?.first()?)
    }
}
