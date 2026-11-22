//! What the tests share: the matrix of worlds, a cache so that a world is made once per run, the
//! settings of a variant, and the distances the checkers measure with (written here from their
//! definition, not taken from the crate under test).

use f4x_core::game::setting::GameSettings;
use f4x_core::game::Game;
use f4x_def::content::Content;
use f4x_def::testdir::shipped;
use f4x_newgame::mapgen::{generate, GenParams, Generated};
use f4x_newgame::setup::{gen_params, new_game, NewGame};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

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
pub struct World {
    pub seed: u64,
    pub wizards: u8,
    pub all_ai: bool,
    pub width: u16,
    pub height: u16,
    pub land: u8,
    pub variant: Variant,
}

impl World {
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
pub fn standard() -> World {
    let s = shipped()
        .settings
        .default_game()
        .expect("the shipped content has a default game");
    World {
        seed: 1,
        wizards: s.basic.opponents + 1,
        all_ai: true,
        width: s.basic.map_width,
        height: s.basic.map_height,
        land: s.basic.land_percent,
        variant: Variant::Plain,
    }
}

/// The seeds the matrix tries: small ones, a spread, and both ends of the range, where an addition
/// or a shift of the seed goes wrong.
pub const SEEDS: &[u64] = &[
    0,
    1,
    2,
    3,
    7,
    11,
    42,
    1000,
    0xDEAD_BEEF,
    u32::MAX as u64,
    1 << 63,
    u64::MAX / 3,
    u64::MAX - 1,
    u64::MAX,
];

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

/// The land shares the settings allow at their ends, and the standard one.
pub fn lands() -> Vec<u8> {
    vec![1, standard().land, 99]
}

/// The most wizards the content allows.
pub fn most_wizards() -> u8 {
    shipped().settings.max_opponents + 1
}

/// The worlds of the normal run: a cut of the matrix of `docs/f4x-newgame-test-design.md`. The
/// standard size over every seed; every corner size and land share over three seeds; every variant
/// over two seeds. `F4X_WORLDS=N` adds N more worlds from a generator (the seed is in the failure).
pub fn worlds() -> Vec<World> {
    let std = standard();
    let mut out = Vec::new();
    for &seed in SEEDS {
        out.push(World { seed, ..std });
    }
    for (width, height) in sizes() {
        for &land in &lands() {
            for &seed in &[1, 2, 0xDEAD_BEEF] {
                out.push(World {
                    seed,
                    width,
                    height,
                    land,
                    ..std
                });
            }
        }
    }
    for &variant in VARIANTS {
        for &seed in &[5, 9] {
            out.push(World {
                seed,
                variant,
                ..std
            });
        }
    }
    // Both generators at the ends of the land share.
    for &land in &lands() {
        for &variant in &[Variant::OriginalGenerator, Variant::ExtendTinyIslands] {
            out.push(World {
                seed: 12,
                land,
                variant,
                ..std
            });
        }
    }
    for wizards in [2, 3, 5, most_wizards()] {
        for all_ai in [true, false] {
            out.push(World {
                seed: 4,
                wizards,
                all_ai,
                ..std
            });
        }
    }
    out.extend(extra_worlds());
    out.sort_by_key(|w| format!("{w:?}"));
    out.dedup();
    out
}

/// `F4X_WORLDS=N` more worlds, from a generator seeded by `F4X_WORLDS_SEED` (default 1).
fn extra_worlds() -> Vec<World> {
    let n: usize = std::env::var("F4X_WORLDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut z: u64 = std::env::var("F4X_WORLDS_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let mut next = move || {
        z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut x = z;
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^ (x >> 31)
    };
    let sizes = sizes();
    (0..n)
        .map(|_| World {
            seed: next(),
            wizards: 2 + (next() % u64::from(most_wizards() - 1)) as u8,
            all_ai: next() % 2 == 0,
            width: sizes[(next() % sizes.len() as u64) as usize].0,
            height: sizes[(next() % sizes.len() as u64) as usize].1,
            land: 1 + (next() % 99) as u8,
            variant: VARIANTS[(next() % VARIANTS.len() as u64) as usize],
        })
        .collect()
}

type Cache<T> = OnceLock<Mutex<HashMap<World, Arc<T>>>>;

fn cached<T>(cache: &'static Cache<T>, world: World, make: impl FnOnce() -> T) -> Arc<T> {
    let map = cache.get_or_init(Default::default);
    if let Some(found) = map.lock().expect("the cache").get(&world) {
        return found.clone();
    }
    // Made outside the lock: two tests may make the same world at once, and both are the same.
    let made = Arc::new(make());
    map.lock()
        .expect("the cache")
        .entry(world)
        .or_insert(made)
        .clone()
}

/// The map of a world, as the generator makes it (the parameters the settings give).
pub fn generated(world: World) -> Arc<Generated> {
    static CACHE: Cache<Generated> = OnceLock::new();
    cached(&CACHE, world, || {
        generate(world.seed, &params(world), shipped())
            .unwrap_or_else(|e| panic!("{world:?}: the map is refused: {e}"))
    })
}

/// What the generator is asked for by the settings of the world.
pub fn params(world: World) -> GenParams {
    gen_params(&world.settings())
}

/// A game of the world, new. A game is not `Sync`, so the cache is the thread's own; the games
/// are read only (a test that changes one makes its own with [`fresh_game`]).
pub fn game(world: World) -> std::rc::Rc<Game> {
    thread_local! {
        static CACHE: std::cell::RefCell<HashMap<World, std::rc::Rc<Game>>> = Default::default();
    }
    if let Some(found) = CACHE.with(|c| c.borrow().get(&world).cloned()) {
        return found;
    }
    let made = std::rc::Rc::new(fresh_game(world));
    CACHE.with(|c| c.borrow_mut().insert(world, made.clone()));
    made
}

/// A game of the world, made again, for a test that changes it.
pub fn fresh_game(world: World) -> Game {
    new_game(shipped(), &world.spec())
        .unwrap_or_else(|e| panic!("{world:?}: the game is refused: {e}"))
}

pub fn content() -> &'static Content {
    shipped()
}

/// The short way round a map that wraps east and west: the difference of two columns.
pub fn dx(width: u32, a: u32, b: u32) -> u32 {
    let d = a.abs_diff(b);
    d.min(width - d)
}

/// Squared distance on a map that wraps east and west.
pub fn dist_sq(width: u32, a: (u32, u32), b: (u32, u32)) -> u32 {
    let (x, y) = (dx(width, a.0, b.0), a.1.abs_diff(b.1));
    x * x + y * y
}

/// The larger of the two differences (a square ring round a point), on a map that wraps.
pub fn chebyshev(width: u32, a: (u32, u32), b: (u32, u32)) -> u32 {
    dx(width, a.0, b.0).max(a.1.abs_diff(b.1))
}

/// The original's range: the longer side plus half the shorter.
pub fn range(width: u32, a: (u32, u32), b: (u32, u32)) -> u32 {
    let (x, y) = (dx(width, a.0, b.0), a.1.abs_diff(b.1));
    x.max(y) + x.min(y) / 2
}
