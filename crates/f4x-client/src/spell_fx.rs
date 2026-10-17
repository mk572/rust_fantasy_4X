//! The pictures the original plays for a spell cast in a battle (`Combat_Spell_Animation`,
//! `Spell_Animation_Load_Graphics` in ReMoM): which file and entry a spell uses, and how its
//! animation moves. The pictures are in `cmbtfx.lbx` and `specfx.lbx`.

use f4x_core::battle::field::Field;

/// The four spells whose picture flies in from the top right (`Combat_Spell_Animation_Bolt`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bolt {
    Fire,
    Fireball,
    Ice,
    Doom,
}

impl Bolt {
    /// Frames of the animation (`frame_count`).
    pub fn frames(self) -> usize {
        match self {
            Bolt::Fire | Bolt::Ice => 11,
            Bolt::Fireball => 16,
            Bolt::Doom => 13,
        }
    }

    /// Where the picture rests, from the target square's point (the picture's top left), where
    /// it starts and the step it flies in each frame.
    pub fn path(self) -> ((f32, f32), (f32, f32), (f32, f32)) {
        match self {
            Bolt::Fire => ((-16.0, -20.0), (110.0, -66.0), (-10.0, 6.0)),
            Bolt::Fireball => ((-14.0, -21.0), (110.0, -66.0), (-10.0, 6.0)),
            Bolt::Ice => ((-17.0, -20.0), (110.0, -110.0), (-10.0, 10.0)),
            Bolt::Doom => ((-14.0, -25.0), (0.0, -72.0), (0.0, 6.0)),
        }
    }
}

/// How a spell's animation plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellFx {
    /// The picture plays once on the target square; `big` pictures (60 by 54) are centred on it,
    /// the others (28 by 34) stand on it.
    Cell {
        lbx: &'static str,
        entry: usize,
        big: bool,
    },
    /// A picture that flies in and bursts on the target.
    Bolt {
        lbx: &'static str,
        entry: usize,
        bolt: Bolt,
    },
    /// Cracks Call: the picture plays slowly on the ground of the square.
    Ground { lbx: &'static str, entry: usize },
    /// Call Chaos: each of the opponent's units gets one of eight effects, by luck.
    Chaos,
    /// Lightning Bolt: a stroke of Call Lightning's picture down onto the square.
    Lightning,
    /// A battlefield spell: the screen is tinted in the realm's colour and back, with the message.
    Tint { realm: usize },
}

/// The realm numbers of `specfx.lbx`'s first six pictures: nature, sorcery, chaos, life, death,
/// arcane.
pub fn realm_number(realm: &str) -> usize {
    ["nature", "sorcery", "chaos", "life", "death", "arcane"]
        .iter()
        .position(|r| *r == realm)
        .unwrap_or(5)
}

/// The tint of a realm (red, green, blue, as the original gives them).
pub fn realm_tint(realm: usize) -> (f32, f32, f32) {
    match realm {
        0 => (0.0, 1.0, 0.0),
        1 => (0.0, 0.0, 1.0),
        2 => (1.0, 0.0, 0.0),
        4 => (0.0, 0.0, 0.0),
        _ => (1.0, 1.0, 1.0),
    }
}

/// Spells that strike every unit of a side at once, each with the picture over it
/// (`Combat_Battlefield_Instant`), and whether it is the opponent's units they strike.
pub fn strikes_a_side(spell: &str) -> Option<bool> {
    match spell {
        "flame_strike" | "death_spell" | "holy_word" | "call_chaos" => Some(true),
        "mass_healing" => Some(false),
        _ => None,
    }
}

/// The pictures of Call Chaos's eight outcomes (`Cast_Call_Chaos`): nothing, healing, chaos channels,
/// warp creature, fire bolt, warp lightning, doom bolt, disintegrate; and whether it is a big
/// picture.
pub const CHAOS_EFFECTS: [Option<(&str, usize, bool)>; 8] = [
    None,
    Some(("specfx", 3, true)),
    Some(("specfx", 2, true)),
    Some(("specfx", 2, true)),
    Some(("cmbtfx", 33, false)),
    Some(("cmbtfx", 3, false)),
    Some(("cmbtfx", 3, false)),
    Some(("cmbtfx", 4, false)),
];

/// What a spell looks like. `summons` is whether it brings a creature in; `lasting` whether it is
/// a spell that works on the whole field.
pub fn fx_for(id: &str, realm: &str, summons: bool) -> SpellFx {
    let cell = |lbx, entry| SpellFx::Cell {
        lbx,
        entry,
        big: false,
    };
    let battlefield = Field::of(id).is_some()
        || matches!(
            id,
            "counter_magic"
                | "dispel_magic"
                | "dispel_magic_true"
                | "disenchant_area"
                | "disenchant_true"
                | "disjunction"
                | "disjunction_true"
                | "raise_dead"
                | "animate_dead"
        );
    if strikes_a_side(id).is_none() && battlefield {
        return SpellFx::Tint {
            realm: realm_number(realm),
        };
    }
    match id {
        "fire_bolt" => SpellFx::Bolt {
            lbx: "cmbtfx",
            entry: 0,
            bolt: Bolt::Fire,
        },
        "fireball" => SpellFx::Bolt {
            lbx: "cmbtfx",
            entry: 23,
            bolt: Bolt::Fireball,
        },
        "ice_bolt" => SpellFx::Bolt {
            lbx: "specfx",
            entry: 11,
            bolt: Bolt::Ice,
        },
        "doom_bolt" => SpellFx::Bolt {
            lbx: "cmbtfx",
            entry: 5,
            bolt: Bolt::Doom,
        },
        "lightning_bolt" => SpellFx::Lightning,
        "cracks_call" => SpellFx::Ground {
            lbx: "cmbtfx",
            entry: 15,
        },
        "call_chaos" => SpellFx::Chaos,
        "earth_to_mud" => SpellFx::Cell {
            lbx: "cmbtfx",
            entry: 14,
            big: true,
        },
        "black_wind" => SpellFx::Cell {
            lbx: "specfx",
            entry: 14,
            big: true,
        },
        "web" => cell("cmbtfx", 13),
        "change_terrain" => cell("specfx", 8),
        "petrify" => cell("specfx", 12),
        "ice_storm" => cell("specfx", 10),
        "call_lightning" => cell("cmbtfx", 24),
        "confusion" => cell("cmbtfx", 20),
        "psionic_blast" => cell("cmbtfx", 16),
        "vertigo" => cell("cmbtfx", 17),
        "spell_blast" => cell("specfx", 40),
        "banish" => cell("cmbtfx", 19),
        "mind_storm" => cell("cmbtfx", 21),
        "stasis" => cell("specfx", 53),
        "spell_binding" => cell("specfx", 44),
        "warp_wood" => cell("cmbtfx", 2),
        "disrupt" => cell("cmbtfx", 1),
        "corruption" => cell("specfx", 7),
        "raise_volcano" => cell("specfx", 11),
        "warp_lightning" => cell("cmbtfx", 3),
        "magic_vortex" => cell("cmbtfx", 25),
        "fire_storm" => cell("specfx", 6),
        "flame_strike" => cell("cmbtfx", 33),
        "disintegrate" => cell("cmbtfx", 4),
        "call_the_void" => cell("specfx", 12),
        "star_fires" => cell("cmbtfx", 9),
        "dispel_evil" => cell("cmbtfx", 10),
        "life_drain" => cell("cmbtfx", 6),
        "drain_power" => cell("specfx", 42),
        "possession" => cell("cmbtfx", 8),
        "subversion" | "warp_node" | "word_of_death" | "death_spell" => cell("specfx", 13),
        "cruel_unminding" => cell("specfx", 41),
        _ if summons && id != "floating_island" => cell("cmbtfx", 22),
        _ => SpellFx::Cell {
            lbx: "specfx",
            entry: realm_number(realm),
            big: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_find_their_pictures() {
        assert!(matches!(
            fx_for("fireball", "chaos", false),
            SpellFx::Bolt { entry: 23, .. }
        ));
        assert!(matches!(
            fx_for("prayer", "life", false),
            SpellFx::Tint { realm: 3 }
        ));
        // A spell without a picture of its own plays its realm's.
        assert_eq!(
            fx_for("heroism", "life", false),
            SpellFx::Cell {
                lbx: "specfx",
                entry: 3,
                big: true
            }
        );
        assert_eq!(
            fx_for("hydra", "chaos", true),
            SpellFx::Cell {
                lbx: "cmbtfx",
                entry: 22,
                big: false
            }
        );
        // Flame Strike plays over each unit, not as a tint.
        assert!(matches!(
            fx_for("flame_strike", "chaos", false),
            SpellFx::Cell { entry: 33, .. }
        ));
    }
}
