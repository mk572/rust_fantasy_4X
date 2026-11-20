//! The macro that declares the enum of the effects a spell hook's arguments name.
//!
//! Tests: with the enums it declares (`rules/unit_effect.rs`).

/// Declares the enum of the effects a hook's arguments name (`GlobalEffect`, `CityEffect`): the
/// variants with their argument strings, `NAMES` for the hook table, `name`, `named`, and
/// `is_in(def)`, whether a spell's definition carries the effect in the given hook.
#[macro_export]
macro_rules! effect_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident in $hook:expr; $($variant:ident = $text:literal),+ $(,)?) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $name {
            $($variant),+
        }

        impl $name {
            /// The hook arguments, in the enum's order.
            pub const NAMES: &[&'static str] = &[$($text),+];

            /// The hook argument for this effect.
            pub fn name(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }

            /// The effect a hook argument names.
            pub fn named(name: &str) -> Option<$name> {
                match name {
                    $($text => Some($name::$variant),)+
                    _ => None,
                }
            }

            /// The first spell of the rules that carries this effect: for the code that has to
            /// name the spell a change came from, or find the spell to cast.
            pub fn spell_in(self, rules: &$crate::rules::Rules) -> Option<$crate::rules::SpellId> {
                rules
                    .spells
                    .defs()
                    .iter()
                    .find(|d| self.is_in(d))
                    .map(|d| $crate::rules::SpellId::new(d.num))
            }

            /// Whether the spell carries this effect.
            pub fn is_in(self, def: &$crate::rules::SpellDef) -> bool {
                def.hook_args($hook)
                    .is_some_and(|a| a.iter().any(|x| x == self.name()))
            }
        }
    };
}
