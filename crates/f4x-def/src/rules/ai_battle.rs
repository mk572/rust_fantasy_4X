//! The levers of the battle AI (`crates/f4x-def/content/ai_battle.ron`, whose comments say what
//! each steers). AI tuning, not rules of the game.
//!
//! Tests: at the end of this file; the shipped file's refusals in `tests/it/c1_content.rs`.

use serde::{Deserialize, Serialize};

use crate::issue::{ContentIssue, Issues};

const FILE: &str = "ai_battle.ron";

/// One lever, as a check or an override reaches it.
pub enum Lever<'a> {
    /// A weight, a share or odds.
    Number(&'a mut f32),
    /// A distance or a count.
    Count(&'a mut u32),
}

impl<'a> From<&'a mut f32> for Lever<'a> {
    fn from(v: &'a mut f32) -> Self {
        Lever::Number(v)
    }
}

impl<'a> From<&'a mut u32> for Lever<'a> {
    fn from(v: &'a mut u32) -> Self {
        Lever::Count(v)
    }
}

/// A group of levers: the struct, and `each`, which goes over its levers by name, so that no list
/// of them can leave one out. `Default` (zeros) is for the empty `Rules` only; the file names
/// every lever.
macro_rules! levers {
    ($(#[$meta:meta])* $group:ident { $($name:ident: $kind:ty,)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
        #[serde(deny_unknown_fields)]
        pub struct $group {
            $(pub $name: $kind,)*
        }

        impl $group {
            fn each(&mut self, f: &mut dyn FnMut(&'static str, Lever<'_>)) {
                $(f(stringify!($name), Lever::from(&mut self.$name));)*
            }
        }
    };
}

levers!(
    /// How the units move and strike.
    OrderLevers {
        flee_strength_ratio: f32,
        shooter_threat: f32,
        least_hit_share: f32,
        merge_reach: u32,
        merge_hurt_health: f32,
        merge_dying_health: f32,
        surface_far_health: f32,
    }
);

levers!(
    /// How the casts are ranked, and which are made.
    ChoiceLevers {
        worth_floor: f32,
        blow_share: f32,
        wound_enough: f32,
        mana_softening: f32,
        kill_odds: f32,
        kill_bonus: f32,
    }
);

levers!(
    /// The spells on the whole field.
    FieldSpellLevers {
        prayer_share: f32,
        high_prayer_share: f32,
        black_prayer_share: f32,
        metal_fires_share: f32,
        mass_invisibility_share: f32,
        blur_share: f32,
        terror_share: f32,
        entangle_share: f32,
        wrack_share: f32,
        lightning_share: f32,
        light_share: f32,
        warp_reality_share: f32,
        mana_leak_share: f32,
    }
);

levers!(
    /// The spells on a unit, the summons, and the defences against magic.
    UnitSpellLevers {
        lasting_share: f32,
        hold_share: f32,
        steal_share: f32,
        slay_share: f32,
        arm_share: f32,
        warp_wood_share: f32,
        drain_share: f32,
        heal_health: f32,
        recall_health: f32,
        recall_share: f32,
        raise_share: f32,
        animate_share: f32,
        summon_share: f32,
        chaos_damage: f32,
        dispel_share: f32,
        dispel_most_spells: u32,
        wall_share: f32,
        counter_share: f32,
    }
);

levers!(
    /// The spells on a square of the ground, and the vortex.
    GroundSpellLevers {
        mud_share: f32,
        mud_reach: u32,
        wall_break_share: f32,
        breach_reach: u32,
        breach_far_part: f32,
        vortex_reach: u32,
        vortex_share: f32,
        vortex_landing_share: f32,
    }
);

/// The levers that are a share of one thing or odds, so at most 1.
const AT_MOST_ONE: &[&str] = &[
    "least_hit_share",
    "merge_hurt_health",
    "merge_dying_health",
    "surface_far_health",
    "blow_share",
    "kill_odds",
    "heal_health",
    "recall_health",
    "breach_far_part",
];

/// The levers that must be more than 0, as the AI divides by them.
const ABOVE_ZERO: &[&str] = &["mana_softening"];

/// Why an override was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideError {
    /// No lever has the name.
    UnknownLever,
    /// The text is not a value the lever can take.
    BadValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BattleAiRules {
    pub orders: OrderLevers,
    pub choice: ChoiceLevers,
    pub field_spells: FieldSpellLevers,
    pub unit_spells: UnitSpellLevers,
    pub ground_spells: GroundSpellLevers,
}

impl BattleAiRules {
    /// Every lever of every group, with the group's name.
    pub fn each(&mut self, f: &mut dyn FnMut(&'static str, &'static str, Lever<'_>)) {
        self.orders.each(&mut |n, l| f("orders", n, l));
        self.choice.each(&mut |n, l| f("choice", n, l));
        self.field_spells.each(&mut |n, l| f("field_spells", n, l));
        self.unit_spells.each(&mut |n, l| f("unit_spells", n, l));
        self.ground_spells
            .each(&mut |n, l| f("ground_spells", n, l));
    }

    /// Set the lever called `name` (in any case) to what `text` says. Nothing changes on an
    /// error.
    pub fn set(&mut self, name: &str, text: &str) -> Result<(), OverrideError> {
        let mut next = *self;
        let mut result = Err(OverrideError::UnknownLever);
        next.each(&mut |_, lever_name, lever| {
            if !lever_name.eq_ignore_ascii_case(name) {
                return;
            }
            result = match lever {
                Lever::Number(v) => text
                    .parse()
                    .ok()
                    .filter(|&n| allows(lever_name, n))
                    .map(|n| *v = n),
                Lever::Count(v) => text.parse().ok().map(|n| *v = n),
            }
            .ok_or(OverrideError::BadValue);
        });
        result?;
        *self = next;
        Ok(())
    }

    /// The problems of the numbers: one that is not finite or below zero, a share above 1, a
    /// divisor of zero.
    pub fn problems(&self) -> Vec<ContentIssue> {
        let mut p = Issues::new();
        let mut copy = *self;
        copy.each(&mut |group, name, lever| {
            if let Lever::Number(&mut v) = lever {
                if !allows(name, v) {
                    p.out_of_range(FILE, group, name, v, wanted(name));
                }
            }
        });
        p.into_vec()
    }
}

/// Whether the number lever `name` may be `v`.
fn allows(name: &str, v: f32) -> bool {
    v.is_finite()
        && v >= 0.0
        && (v <= 1.0 || !AT_MOST_ONE.contains(&name))
        && (v > 0.0 || !ABOVE_ZERO.contains(&name))
}

/// What [`allows`] asks of the lever, for the report.
fn wanted(name: &str) -> &'static str {
    if AT_MOST_ONE.contains(&name) {
        "a share, 0 to 1"
    } else if ABOVE_ZERO.contains(&name) {
        "a number above 0"
    } else {
        "a number of 0 or more"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> BattleAiRules {
        crate::testdir::shipped().rules.ai_battle
    }

    /// The group and the name of every lever.
    fn names() -> Vec<(&'static str, &'static str)> {
        let mut found = Vec::new();
        shipped().each(&mut |group, name, _| found.push((group, name)));
        found
    }

    #[test]
    fn a_lever_has_a_name_of_its_own_across_the_groups() {
        let mut all: Vec<&str> = names().iter().map(|&(_, n)| n).collect();
        let count = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), count, "two groups share a lever's name");
    }

    #[test]
    fn the_checked_names_are_names_of_levers() {
        for checked in AT_MOST_ONE.iter().chain(ABOVE_ZERO) {
            assert!(
                names().iter().any(|(_, n)| n == checked),
                "{checked} is no lever"
            );
        }
    }

    #[test]
    fn an_override_sets_the_lever_it_names_and_no_other() {
        for (_, wanted_name) in names() {
            let before = shipped();
            let mut after = before;
            // 1 is a value every lever may take.
            assert_eq!(
                after.set(&wanted_name.to_uppercase(), "1"),
                Ok(()),
                "{wanted_name}"
            );
            let values = |mut r: BattleAiRules| {
                let mut found = Vec::new();
                r.each(&mut |_, name, lever| {
                    found.push((
                        name,
                        match lever {
                            Lever::Number(v) => *v,
                            Lever::Count(v) => *v as f32,
                        },
                    ))
                });
                found
            };
            for ((name, was), (_, is)) in values(before).into_iter().zip(values(after)) {
                if name == wanted_name {
                    assert_eq!(is, 1.0, "{name}");
                } else {
                    assert_eq!(is, was, "{name} moved with {wanted_name}");
                }
            }
        }
    }

    #[test]
    fn an_override_that_names_no_lever_or_gives_no_value_for_it_changes_nothing() {
        let mut levers = shipped();
        assert_eq!(
            levers.set("BUILD_GARRISON", "1"),
            Err(OverrideError::UnknownLever)
        );
        for (name, text) in [
            ("FLEE_STRENGTH_RATIO", "soon"),
            ("FLEE_STRENGTH_RATIO", "-1"),
            ("FLEE_STRENGTH_RATIO", "inf"),
            ("KILL_ODDS", "1.5"),
            ("MANA_SOFTENING", "0"),
            ("MUD_REACH", "2.5"),
            ("MUD_REACH", "-2"),
        ] {
            assert_eq!(
                levers.set(name, text),
                Err(OverrideError::BadValue),
                "{name}={text}"
            );
        }
        assert_eq!(levers, shipped());
    }
}
