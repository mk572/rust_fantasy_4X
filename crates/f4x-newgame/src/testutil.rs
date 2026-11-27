//! The worlds tests make: a case (seed, wizards, size, land share) and the changes of the settings
//! that move the placement or the start. For the tests of this crate and of `f4x-master`, behind the
//! `testutil` feature.
//!
//! Tests: through the tests that use them.

use f4x_core::world::setting::GameSettings;
use f4x_def::testdir::shipped;

use crate::setup::NewGame;

/// A change of the settings that moves the placement or the start. Each is tried alone against the
/// settings the shipped content starts a game from, never in a product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variant {
    /// The settings of the content's new game, nothing changed.
    Plain,
    OriginalGenerator,
    ExtendTinyIslands,
    NoExtendTinyIslands,
    RoadsAtStart,
    MixNeutralRaces,
    LimitAiRaces,
    BanLifeAndDeath,
    NoBanLifeAndDeath,
    RandomisedAiWizards,
    PlainAiWizards,
}

pub const VARIANTS: &[Variant] = &[
    Variant::Plain,
    Variant::OriginalGenerator,
    Variant::ExtendTinyIslands,
    Variant::NoExtendTinyIslands,
    Variant::RoadsAtStart,
    Variant::MixNeutralRaces,
    Variant::LimitAiRaces,
    Variant::BanLifeAndDeath,
    Variant::NoBanLifeAndDeath,
    Variant::RandomisedAiWizards,
    Variant::PlainAiWizards,
];

impl Variant {
    pub fn apply(self, s: &mut GameSettings) {
        match self {
            Variant::Plain => {}
            Variant::OriginalGenerator => s.basic.original_map_generator = true,
            Variant::ExtendTinyIslands => s.rules.extend_tiny_islands = true,
            Variant::NoExtendTinyIslands => s.rules.extend_tiny_islands = false,
            Variant::RoadsAtStart => s.rules.roads_at_start = true,
            Variant::MixNeutralRaces => s.rules.mix_neutral_races = true,
            Variant::LimitAiRaces => s.rules.limit_ai_races = true,
            Variant::BanLifeAndDeath => s.rules.ban_life_and_death = true,
            Variant::NoBanLifeAndDeath => s.rules.ban_life_and_death = false,
            Variant::RandomisedAiWizards => s.ai.wizards.randomise = true,
            Variant::PlainAiWizards => s.ai.wizards.randomise = false,
        }
    }
}

/// One world to make: everything that decides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldCase {
    pub seed: u64,
    pub wizards: u8,
    pub all_ai: bool,
    pub width: u16,
    pub height: u16,
    pub land: u8,
    pub variant: Variant,
}

impl WorldCase {
    /// The settings of this world: the content's own new game, the size, the land share and the
    /// variant put over it.
    pub fn settings(&self) -> GameSettings {
        let mut s = shipped()
            .settings
            .default_game()
            .expect("the shipped content has a default game");
        s.basic.opponents = self.wizards - 1;
        s.basic.map_width = self.width;
        s.basic.map_height = self.height;
        s.basic.land_percent = self.land;
        self.variant.apply(&mut s);
        // The original generator takes no more land than its limit (the settings refuse more).
        if s.basic.original_map_generator {
            s.basic.land_percent = s
                .basic
                .land_percent
                .min(f4x_def::settings::ORIGINAL_GENERATOR_MAX_LAND);
        }
        s
    }

    pub fn spec(&self) -> NewGame {
        NewGame {
            seed: self.seed,
            all_ai: self.all_ai,
            settings: self.settings(),
            ..Default::default()
        }
    }
}

/// The size, land share and wizards of the content's own new game.
pub fn standard() -> WorldCase {
    let s = shipped()
        .settings
        .default_game()
        .expect("the shipped content has a default game");
    WorldCase {
        seed: 1,
        wizards: s.basic.opponents + 1,
        all_ai: true,
        width: s.basic.map_width,
        height: s.basic.map_height,
        land: s.basic.land_percent,
        variant: Variant::Plain,
    }
}

/// The corners of the sizes the settings allow, and the standard one.
pub fn sizes() -> Vec<(u16, u16)> {
    let (w, h) = (
        f4x_def::settings::MAP_WIDTHS,
        f4x_def::settings::MAP_HEIGHTS,
    );
    let std = standard();
    let mut v = vec![
        (std.width, std.height),
        (*w.start(), *h.start()),
        (*w.end(), *h.start()),
        (*w.start(), *h.end()),
        (*w.end(), *h.end()),
    ];
    v.dedup();
    v
}

/// The most wizards the content allows.
pub fn most_wizards() -> u8 {
    shipped().settings.max_opponents + 1
}
