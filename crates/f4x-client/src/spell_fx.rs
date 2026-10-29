//! How a spell cast in a battle is played (`Combat_Spell_Animation`, `Spell_Animation_Load_Graphics`
//! in ReMoM): how its animation moves. Which file and entry hold its pictures is in
//! `content/spell_art.ron` (`f4x_data::spell_art`), under the files `cmbtfx` and `specfx`.

use f4x_core::battle::field::Field;
use f4x_data::spell_art::{Role, SpellArt};

/// The four spells whose picture flies in from the top right (Ice Bolt's from the top left) (`Combat_Spell_Animation_Bolt`).
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
            // The original's own picture (cmbtfx 11): it comes in from the top left.
            Bolt::Ice => ((-12.0, -20.0), (-110.0, -66.0), (10.0, 6.0)),
            Bolt::Doom => ((-14.0, -25.0), (0.0, -72.0), (0.0, 6.0)),
        }
    }
}

/// How a spell's animation plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpellFx {
    /// The picture plays once on the target square; `big` pictures (60 by 54) are centred on it,
    /// the others (28 by 34) stand on it.
    Cell {
        lbx: String,
        entry: usize,
        big: bool,
    },
    /// A picture that flies in and bursts on the target.
    Bolt {
        lbx: String,
        entry: usize,
        bolt: Bolt,
    },
    /// Cracks Call: the picture plays slowly on the ground of the square.
    Ground { lbx: String, entry: usize },
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

/// Whether the picture of each of Call Chaos's eight outcomes (`Cast_Call_Chaos`: nothing, healing,
/// chaos channels, warp creature, fire bolt, warp lightning, doom bolt, disintegrate) is a big one.
const CHAOS_BIG: [bool; 8] = [false, true, true, true, false, false, false, false];

/// The picture of one of Call Chaos's outcomes (0, which does nothing, has none): file, entry and
/// whether it is a big picture.
pub fn chaos_effect(art: &SpellArt, outcome: u8) -> Option<(String, usize, bool)> {
    let outcome = outcome % 8;
    let (lbx, entry) = art.picture("call_chaos", &Role::Chaos(outcome))?;
    Some((lbx.to_string(), entry, CHAOS_BIG[usize::from(outcome)]))
}

/// What a spell looks like. `summons` is whether it brings a creature in; `lasting` whether it is
/// a spell that works on the whole field.
pub fn fx_for(art: &SpellArt, id: &str, realm: &str, summons: bool) -> SpellFx {
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
    let own = art
        .picture(id, &Role::Cast)
        .map(|(lbx, entry)| (lbx.to_string(), entry));
    let cell = |(lbx, entry): (String, usize), big| SpellFx::Cell { lbx, entry, big };
    let bolt = |bolt| {
        own.clone()
            .map(|(lbx, entry)| SpellFx::Bolt { lbx, entry, bolt })
    };
    match id {
        "fire_bolt" => bolt(Bolt::Fire),
        "fireball" => bolt(Bolt::Fireball),
        "ice_bolt" => bolt(Bolt::Ice),
        "doom_bolt" => bolt(Bolt::Doom),
        "lightning_bolt" => Some(SpellFx::Lightning),
        "cracks_call" => own
            .clone()
            .map(|(lbx, entry)| SpellFx::Ground { lbx, entry }),
        "call_chaos" => Some(SpellFx::Chaos),
        // Three spells play a big picture of their own.
        "earth_to_mud" | "black_wind" | "petrify" => own.clone().map(|pic| cell(pic, true)),
        _ => own.clone().map(|pic| cell(pic, false)),
    }
    .or_else(|| {
        // A summon plays the summon flame, any other spell the picture of its realm.
        if summons && id != "floating_island" {
            art.shared(&Role::Summon)
                .map(|(lbx, entry)| cell((lbx.to_string(), entry), false))
        } else {
            None
        }
    })
    .unwrap_or_else(|| match art.realm(realm) {
        Some((lbx, entry)) => cell((lbx.to_string(), entry), true),
        None => SpellFx::Tint {
            realm: realm_number(realm),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn ice_bolt_comes_in_from_the_left_where_fire_bolt_comes_from_the_right() {
        let (_, fire_start, fire_step) = Bolt::Fire.path();
        let (_, ice_start, ice_step) = Bolt::Ice.path();
        assert!(fire_start.0 > 0.0 && fire_step.0 < 0.0);
        assert!(ice_start.0 < 0.0 && ice_step.0 > 0.0);
        assert_eq!((ice_start.1, ice_step.1), (fire_start.1, fire_step.1));
    }

    fn art() -> SpellArt {
        let dir = Path::new("../../content");
        let content = f4x_data::load_content(dir).unwrap();
        f4x_data::spell_art::load_spell_art(&[dir], &content.rules).unwrap()
    }

    fn cell(lbx: &str, entry: usize, big: bool) -> SpellFx {
        SpellFx::Cell {
            lbx: lbx.into(),
            entry,
            big,
        }
    }

    #[test]
    fn spells_find_their_pictures() {
        let art = art();
        assert!(matches!(
            fx_for(&art, "fireball", "chaos", false),
            SpellFx::Bolt { entry: 23, .. }
        ));
        assert!(matches!(
            fx_for(&art, "prayer", "life", false),
            SpellFx::Tint { realm: 3 }
        ));
        // A spell without a picture of its own plays its realm's.
        assert_eq!(
            fx_for(&art, "heroism", "life", false),
            cell("specfx", 3, true)
        );
        assert_eq!(
            fx_for(&art, "hydra", "chaos", true),
            cell("cmbtfx", 22, false)
        );
        // Flame Strike plays over each unit, not as a tint.
        assert!(matches!(
            fx_for(&art, "flame_strike", "chaos", false),
            SpellFx::Cell { entry: 33, .. }
        ));
        assert_eq!(
            fx_for(&art, "earth_to_mud", "nature", false),
            cell("cmbtfx", 14, true)
        );
    }

    #[test]
    fn call_chaos_has_a_picture_for_each_outcome_but_the_first() {
        let art = art();
        assert_eq!(chaos_effect(&art, 0), None);
        assert_eq!(
            chaos_effect(&art, 4),
            Some(("cmbtfx".to_string(), 33, false))
        );
        assert_eq!(chaos_effect(&art, 1), Some(("specfx".to_string(), 3, true)));
        assert!((1..8).all(|n| chaos_effect(&art, n).is_some()));
    }
}
