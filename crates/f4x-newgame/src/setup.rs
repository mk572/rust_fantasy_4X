//! Starting a game without the client: a map, the wizards, a fortress for each spread round the map
//! and a few starting units. The client's new-game screen, the console, the AI soak runs and tests
//! all start games here.
//!
//! Tests: below, and in setup/original.rs for the rolled fortress sites.

/// The least maximum population of a fortress's land, kept through every pass but the last.
pub const MIN_START_SIZE: u32 = 5;

use f4x_def::half_int::HalfInt;
use std::collections::{BTreeMap, BTreeSet};

mod original;

use crate::error::NewGameError;
use crate::mapgen::{generate, GenParams, PlaneParams};
use f4x_core::map::{Plane, Position};
use f4x_core::player::PlayerId;
use f4x_core::rules::RealmId;
use f4x_core::world::rng::GameRng;
use f4x_core::world::setting::GameSettings;
use f4x_core::world::FIRST_ROUND;
use f4x_def::content::Content;

use f4x_core::wizard::magic::Retort;
use f4x_core::wizard::WizardBuild;
use f4x_core::world::{Command, World};

pub const HUMAN: PlayerId = PlayerId(0);
pub const RIVAL: PlayerId = PlayerId(1);

/// A retort by name (case and punctuation ignored): `alchemy`, `sage_master`, `sagemaster`.
pub fn parse_retort(rules: &f4x_core::rules::Rules, text: &str) -> Option<Retort> {
    let squash = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let key = squash(text);
    rules
        .retorts
        .iter()
        .find(|(_, def)| squash(&def.id) == key)
        .map(|(id, _)| id)
}

/// The original's land sizes, as a share of each plane's tiles (`NEWG_CreateLands`: 360, 480 and
/// 720 of 2400).
pub use f4x_core::world::setting::{LAND_LARGE, LAND_MEDIUM, LAND_SMALL};

/// What a new game is made from. Everything but the seed is optional.
#[derive(Debug, Clone, Default)]
pub struct NewGame {
    pub seed: u64,
    /// Every wizard is a ready-made one for an AI to play (no human): `human` stays unset.
    pub all_ai: bool,
    /// A ready-made wizard's number for player 0; the default is a fixed sorcery and nature wizard.
    pub preset: Option<u16>,
    /// A custom wizard for player 0 (used when no preset is given).
    pub name: Option<String>,
    pub gender: f4x_def::text::Gender,
    pub books: BTreeMap<RealmId, u8>,
    pub retorts: BTreeSet<Retort>,
    /// Player 0's race number; the default is the first race.
    pub race: Option<u16>,
    /// A finished wizard for player 0 (name, books, retorts, spells, banner); it wins over
    /// `preset`, `name`, `books` and `retorts`.
    pub build: Option<WizardBuild>,
    /// The settings the game is played with: difficulty, the number of opponents (2 to 5
    /// wizards play in all, the first is player 0), land size, node strength and the rest.
    pub settings: GameSettings,
    /// The name lists for the cities, heroes and rival wizards (the client's; empty gives
    /// `<race id>_city_00`, the unit's alias and `rival_1`).
    pub names: f4x_def::names::NameBundle,
}

impl NewGame {
    /// A new game with `wizards` playing (2 to 10) and every other choice at its default.
    pub fn with_wizards(wizards: u8) -> Self {
        let mut spec = Self::default();
        spec.settings.basic.opponents = wizards.saturating_sub(1);
        spec
    }
}

/// What the map generator is asked for by a game's settings: the land, the lairs, and the counts of
/// towers, nodes and encounter sites (which follow the map's area unless the settings name a
/// number) and the share of minerals.
pub fn gen_params(settings: &GameSettings) -> GenParams {
    let params = GenParams {
        land_percent: settings.basic.land_percent,
        difficulty: settings.basic.difficulty,
        magic_strength: settings.basic.magic_strength,
        original_generator: settings.basic.original_map_generator,
        extend_tiny_islands: settings.rules.extend_tiny_islands,
        lair_guard_percent: settings.world.lair_guard_percent,
        lair_treasure_percent: settings.world.lair_treasure_percent,
        lair_style: f4x_core::rules::lair::LairStyle {
            stronger: settings.rules.stronger_lairs,
            treasure: settings.rules.treasure_patch_150,
        },
        ..GenParams::sized(
            u32::from(settings.basic.map_width),
            u32::from(settings.basic.map_height),
        )
    };
    let plane = |k: usize, percent: u8| PlaneParams {
        mineral_percent: percent,
        node_count: settings.node_count(k),
    };
    GenParams {
        tower_count: settings.basic.tower_count(),
        strong_sites: settings.strong_site_count(),
        weak_sites: settings.weak_site_count(),
        planes: [
            plane(0, settings.world.mineral_percent_arcanus),
            plane(1, settings.world.mineral_percent_myrror),
        ],
        ..params
    }
}

/// A new game's world and the generator that made it, which the game goes on drawing from
/// (`f4x_master::Game::new`).
pub fn new_game(content: &Content, spec: &NewGame) -> Result<(World, GameRng), NewGameError> {
    let problems = spec.settings.validate();
    if !problems.is_empty() {
        return Err(NewGameError::Settings(problems));
    }
    // The hooks and special spells the code implements are the only ones the content may name.
    let hooks = f4x_core::hook::problems(&content.rules);
    if !hooks.is_empty() {
        return Err(NewGameError::Content(hooks));
    }
    // The numbers this crate names (the Myrran retort, the five realms, ...) must lead to the rows
    // of their aliases, or the wizards would be made with other retorts than the code means.
    let coupling = crate::coupling::problems(&content.rules);
    if !coupling.is_empty() {
        return Err(NewGameError::Content(coupling));
    }
    if spec.settings.basic.opponents > content.settings.max_opponents {
        return Err(NewGameError::TooManyOpponents {
            asked: spec.settings.basic.opponents,
            most: content.settings.max_opponents,
        });
    }
    // Every wizard of an all-AI game is a ready-made one: the choices for player 0 have nobody to
    // go to, and one of them (the race) used to be taken while the others were dropped.
    if spec.all_ai
        && (spec.preset.is_some()
            || spec.build.is_some()
            || spec.race.is_some()
            || spec.name.is_some()
            || !spec.books.is_empty()
            || !spec.retorts.is_empty())
    {
        return Err(NewGameError::ChoicesWithoutHuman);
    }
    let settings = spec.settings;
    let wizards = settings.basic.opponents + 1;
    let params = gen_params(&settings);
    let map = generate(spec.seed, &params, content)?;
    let mut rng = GameRng::new(spec.seed);
    let mut world = World::begin_world(
        settings,
        content,
        spec.names.clone(),
        map.map,
        map.placements,
    );
    for k in 0..wizards {
        // A stand-in until the wizard's own name replaces it.
        let name = match (k, spec.all_ai) {
            (0, false) => "player".to_string(),
            _ => format!("wizard_{}", k + 1),
        };
        world.add_player(PlayerId(k), name);
    }
    let fixed = || WizardBuild {
        name: String::new(),
        gender: Default::default(),
        books: Default::default(),
        retorts: Default::default(),
        spells: Vec::new(),
        banner: None,
        portrait: None,
        preset: true,
    };
    let preset_ids: Vec<u16> = world
        .content()
        .rules
        .wizards
        .defs()
        .iter()
        .map(|w| w.num)
        .collect();
    let mut taken: Vec<u16> = Vec::new();
    let mut builds = Vec::new();
    // The computer wizards, made together when the settings ask for it (see `ai_wizards`).
    let mut made: Option<std::collections::VecDeque<WizardBuild>> = None;
    let myrran_build = |b: &WizardBuild| b.retorts.contains(&Retort::new(9) /* myrran */);
    for k in 0..wizards as usize {
        let build = if k == 0 && !spec.all_ai {
            if let Some(build) = &spec.build {
                if let Some(id) = build.preset.then_some(spec.preset).flatten() {
                    taken.push(id);
                }
                build.clone()
            } else if let Some(&num) = spec.preset.as_ref() {
                taken.push(num);
                let mut build = world
                    .preset_build(num)
                    .ok_or(NewGameError::NoPreset { num })?;
                // Over the most books in a realm, it loses the surplus and has picks to spend:
                // the pick rules then ask for them.
                build.trim_books(settings.wizards.max_books_per_realm);
                build
            } else if !spec.books.is_empty() {
                WizardBuild {
                    name: spec.name.clone().unwrap_or_else(|| "Wizard".into()),
                    gender: spec.gender,
                    books: spec.books.clone(),
                    retorts: spec.retorts.clone(),
                    spells: Vec::new(),
                    banner: None,
                    portrait: None,
                    preset: false,
                }
            } else {
                // Nobody said which wizard: one with no books and no retort. A test or a caller
                // that needs more says so (`books`, `retorts`, `preset`, `build`).
                fixed()
            }
        } else if let Some(build) = settings
            .ai
            .wizards
            .randomise
            .then(|| {
                made.get_or_insert_with(|| {
                    let computers = usize::from(wizards) - usize::from(!spec.all_ai);
                    let first = usize::from(!spec.all_ai);
                    let plain = |i: usize| plain_computer_build(&spec.names, spec.seed, first + i);
                    let plain: Vec<WizardBuild> = (0..computers).map(plain).collect();
                    let human_myrran = !spec.all_ai && builds.first().is_some_and(myrran_build);
                    let presets = world.content().rules.wizards.defs().to_vec();
                    crate::computer_wizard::make(
                        &mut world,
                        &presets,
                        &settings,
                        &mut taken,
                        computers,
                        human_myrran,
                        &|i| plain[i].clone(),
                        &mut rng,
                    )
                    .into()
                })
                .pop_front()
            })
            .flatten()
        {
            build
        } else {
            // The next ready-made wizard nobody has yet, starting from a seed-chosen one.
            let free: Vec<&u16> = preset_ids.iter().filter(|id| !taken.contains(id)).collect();
            let pick = free
                .get(seed_offset(spec.seed, k) % free.len().max(1))
                .and_then(|&&id| world.preset_build(id).map(|b| (id, b)));
            match pick {
                Some((id, mut b)) => {
                    taken.push(id);
                    crate::computer_wizard::fit_books_and_refill(&mut world, &mut b, &mut rng);
                    b
                }
                None => plain_computer_build(&spec.names, spec.seed, k),
            }
        };
        builds.push(build);
    }
    // The wizards are shared between the planes, as evenly as the Myrran retort allows
    // (`ai::myrran_range`). Computer wizards that were made together are balanced already.
    if !settings.ai.wizards.randomise {
        let first = usize::from(!spec.all_ai);
        let human_myrran = first == 1 && builds.first().is_some_and(myrran_build);
        crate::computer_wizard::balance_planes(
            &mut world,
            &mut builds[first..],
            human_myrran,
            usize::from(wizards),
            settings.wizards.picks.ai,
            settings.rules.ban_life_and_death,
            &mut rng,
        );
    }
    for (k, build) in builds.iter().enumerate() {
        world.create_wizard(PlayerId(k as u8), build, &mut rng)?;
    }
    // Every wizard has a banner of their own.
    let mut used = BTreeSet::new();
    for k in 0..wizards {
        let id = PlayerId(k);
        let mut banner = world.player(id).map_or(0, |p| p.banner());
        // Each wizard has a colour of their own while there are colours left.
        while used.len() < usize::from(f4x_core::player::BANNER_CHOICES) && used.contains(&banner) {
            banner = (banner + 1) % f4x_core::player::BANNER_CHOICES;
        }
        used.insert(banner);
        if let Some(mut w) = world.wizard_mut(id) {
            w.put(Some(f4x_core::wizard::entity::Banner(banner)));
        }
    }
    // And a picture of their own.
    let mut shown = BTreeSet::new();
    for k in 0..wizards {
        let id = PlayerId(k);
        let mut portrait = world.player(id).map_or(0, |p| p.portrait());
        while shown.contains(&portrait) {
            portrait = (portrait + 1) % f4x_core::player::PORTRAITS;
        }
        shown.insert(portrait);
        if let Some(mut w) = world.wizard_mut(id) {
            w.put(Some(f4x_core::wizard::entity::Portrait(portrait)));
        }
    }
    world.set_human((!spec.all_ai).then_some(HUMAN));
    world.apply_wizard_settings();
    for k in 0..wizards {
        crate::main_realm_spell::give_main_realm_spell(&mut world, PlayerId(k), &mut rng);
    }
    for k in 0..wizards {
        let id = PlayerId(k);
        let human = world.human() == Some(id);
        let (gold, mana) = (
            settings.wizards.starting_gold.of(human),
            settings.wizards.starting_mana.of(human),
        );
        if let Some(t) = world.treasury_mut(id) {
            t.gold = HalfInt::from_int(gold);
            t.mana = HalfInt::from_int(mana);
        }
    }
    // Races: player 0's choice, then the others in turn, skipping races already taken. A
    // Myrran wizard has a Myrror race and starts there; everyone else an Arcanus one.
    let race_defs = world.content().rules.races.defs().to_vec();
    let mut races: Vec<u16> = Vec::new();
    for k in 0..wizards as usize {
        let myrran = is_myrran(&world, PlayerId(k as u8));
        let on_plane = |r: &&f4x_core::rules::RaceDef| r.myrror == myrran;
        let id = match spec.race.as_ref().filter(|_| k == 0) {
            Some(want) => {
                let ok = race_defs.iter().filter(on_plane).any(|r| r.num == *want);
                if !ok {
                    return Err(NewGameError::RaceNotOnPlane {
                        race: *want,
                        myrror: myrran,
                    });
                }
                *want
            }
            None => {
                let limited =
                    settings.rules.limit_ai_races && !myrran && settings.basic.difficulty > 2;
                let all: Vec<u16> = race_defs
                    .iter()
                    .filter(on_plane)
                    .map(|r| r.num)
                    .filter(|n| {
                        !limited
                            || world.human() == Some(PlayerId(k as u8))
                            || world.content().rules.config.limited_ai_races.contains(n)
                    })
                    .collect();
                let free: Vec<u16> = all.iter().copied().filter(|r| !races.contains(r)).collect();
                let pool = if free.is_empty() { all } else { free };
                pool.get(seed_offset(spec.seed, k * 3) % pool.len().max(1))
                    .copied()
                    .ok_or(NewGameError::NoRaces)?
            }
        };
        races.push(id);
    }
    // The original's rolled fortress sites, when the setting asks for them; if no pass finds
    // them all the wizards start where `start_player` looks for them.
    let rolled = settings.basic.original_map_generator.then(|| {
        let planes: Vec<Plane> = (0..wizards)
            .map(|k| {
                if is_myrran(&world, PlayerId(k)) {
                    Plane::Myrror
                } else {
                    Plane::Arcanus
                }
            })
            .collect();
        original::fortress_sites(&mut world, &planes, &mut rng)
    });
    for k in 0..wizards {
        let site = rolled.clone().flatten().map(|v| v[k as usize]);
        start_player(
            &mut world,
            PlayerId(k),
            wizards,
            races[k as usize],
            site,
            &mut rng,
        )?;
        if settings.rules.extend_tiny_islands {
            build_out_start_island(&mut world, PlayerId(k), &mut rng);
        }
        reveal_start(&mut world, PlayerId(k), settings.world.start_reveal_radius);
    }
    // Independent cities on both planes, once the wizards' fortresses are down.
    for plane in [Plane::Arcanus, Plane::Myrror] {
        crate::neutral::generate_neutral_cities(&mut world, plane, &mut rng);
    }
    // The roads between them come after every city of the start is down.
    if settings.rules.roads_at_start {
        for plane in [Plane::Arcanus, Plane::Myrror] {
            world.lay_start_roads(plane);
        }
    }
    crate::temperament::assign_temperaments(&mut world, &mut rng);
    // The heroes' abilities are rolled last, so the map and the wizards' start stay as they were.
    init_heroes(&mut world, &mut rng);
    Ok((world, rng))
}

/// The seed moved on by `k`, as an index to be taken modulo a count. A seed is any `u64`: the sum
/// wraps at the end of the range instead of overflowing.
fn seed_offset(seed: u64, k: usize) -> usize {
    seed.wrapping_add(k as u64) as usize
}

/// The plain build of computer wizard `k` when no ready-made wizard is free: a chaos and death
/// wizard or a life and nature one, with a name from the lists.
/// Make the game's heroes (`Init_Heroes`): one of each hero type, in the template order, which is
/// the order the original draws their abilities in.
fn init_heroes(world: &mut World, rng: &mut GameRng) {
    for kind in 0..world.content().rules.hero_templates.len() {
        world.add_hero(kind, rng);
    }
}

fn plain_computer_build(names: &f4x_def::names::NameBundle, seed: u64, k: usize) -> WizardBuild {
    let (books, retort): (&[(RealmId, u8)], Retort) = if k % 2 == 1 {
        (
            &[
                (RealmId::new(2) /* chaos */, 3),
                (RealmId::new(4) /* death */, 2),
            ],
            Retort::new(1), /* warlord */
        )
    } else {
        (
            &[
                (RealmId::new(3) /* life */, 3),
                (RealmId::new(0) /* nature */, 2),
            ],
            Retort::new(16), /* charismatic */
        )
    };
    let (name, gender) = fallback_name(names, seed, k);
    WizardBuild {
        name,
        gender,
        books: books.iter().map(|&(r, n)| (r, n)).collect(),
        retorts: [retort].into_iter().collect(),
        spells: Vec::new(),
        banner: None,
        portrait: None,
        preset: true,
    }
}

/// A name for rival `k` when no ready-made wizards are loaded: one of those in that slot's list
/// of the bundle, picked from the seed without touching the game's random stream. With no list,
/// `rival_k`.
fn fallback_name(
    names: &f4x_def::names::NameBundle,
    seed: u64,
    k: usize,
) -> (String, f4x_def::text::Gender) {
    let lists = &names.rivals;
    let Some(list) = lists
        .get(k.saturating_sub(1).min(lists.len().saturating_sub(1)))
        .filter(|l| !l.is_empty())
    else {
        return (format!("rival_{k}"), Default::default());
    };
    // splitmix64 of the seed and the slot.
    let mut z = seed
        .wrapping_add((k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    list[(z % list.len() as u64) as usize].clone()
}

/// Whether the wizard has the Myrran retort (and so starts on Myrror).
fn is_myrran(world: &World, player: PlayerId) -> bool {
    world
        .wizard(player)
        .is_some_and(|w| w.has(Retort::new(9) /* myrran */))
}

/// A sine or cosine that is exactly 0 or +-1 where the angle is a quarter turn: `sin(PI)` is a tiny
/// number, not 0, and on an odd map size it decided whether a start row of x.5 went up or down.
fn exact(t: f64) -> f64 {
    [-1.0, 0.0, 1.0]
        .into_iter()
        .find(|snap| (t - snap).abs() < 1e-9)
        .unwrap_or(t)
}

/// A value rounded to a whole number, a half going up. Away from a half the usual rounding; the
/// value is a sum of products of a trigonometric function and a map size, never within a thousandth
/// of a half unless it is exactly one (`tests::the_start_spot_is_stable_over_every_size`).
fn round_half_up(v: f64) -> f64 {
    if (v - v.floor() - 0.5).abs() < 1e-9 {
        v.floor() + 1.0
    } else {
        v.round()
    }
}

/// Where wizard `k` of `n` starts: spread round the middle of the map.
fn anchor(width: u32, height: u32, k: u8, n: u8) -> (u32, u32) {
    let (w, h) = (f64::from(width), f64::from(height));
    let angle = std::f64::consts::PI + std::f64::consts::TAU * f64::from(k) / f64::from(n);
    (
        round_half_up(w / 2.0 + exact(angle.cos()) * w / 4.0) as u32 % width,
        round_half_up(h / 2.0 + exact(angle.sin()) * h / 3.0).clamp(2.0, h - 3.0) as u32,
    )
}

// The capital's buildings, citizens and farmers at the start are `wizard` in `rules.ron`.

/// A wizard's first view of the world: only the land within `radius` of their fortress (what
/// founding the capital and creating its army revealed is replaced).
fn reveal_start(world: &mut World, player: PlayerId, radius: HalfInt) {
    let Some(fortress) = world.wizard(player).and_then(|w| w.fortress()) else {
        return;
    };
    world.restrict_view(player, fortress, radius);
}

/// Found a wizard's fortress near their start point, as the original's `Generate_Home_Cities`
/// does: a capital of four with the race's spearmen and swordsmen (two swordsmen for a race
/// without spearmen), a smithy, barracks and builders hall, and three citizens on the farms.
/// The site has to be land that can feed eight (seven for the later wizards), clear of other
/// fortresses and of nodes, towers and lairs; those distances relax if nothing fits.
/// A start town on a speck of land gets the island built out (`island::MIN_LAND`), so its wizard
/// has ground to build on.
fn build_out_start_island(world: &mut World, player: PlayerId, rng: &mut GameRng) {
    let Some(at) = world.wizard(player).and_then(|w| w.fortress()) else {
        return;
    };
    crate::island::build_out(world, at, rng);
}

fn start_player(
    world: &mut World,
    player: PlayerId,
    count: u8,
    race: u16,
    site: Option<Position>,
    rng: &mut GameRng,
) -> Result<(), NewGameError> {
    let rules = &world.content().rules;
    let race_def = rules
        .races
        .by_num(race)
        .ok_or(NewGameError::NoRace { race })?;
    let race_id = rules.races.id_numbered(race_def.num);
    let name = world.names().city(race_def.num, &race_def.id, 0);
    let army = race_def.start_army.clone();
    let buildings: Vec<_> = world
        .content()
        .rules
        .config
        .wizard
        .capital_buildings
        .clone();
    let plane = if is_myrran(world, player) {
        Plane::Myrror
    } else {
        Plane::Arcanus
    };
    let (width, height) = world
        .map()
        .map(|m| (m.width(), m.height()))
        .ok_or(NewGameError::NoMap)?;
    // Wizards on the same plane are spread round it, whatever the others do on the other.
    let (mates, index) = {
        let same: Vec<u8> = (0..count)
            .filter(|&k| is_myrran(world, PlayerId(k)) == (plane == Plane::Myrror))
            .collect();
        let index = same.iter().position(|&k| k == player.0).unwrap_or(0);
        (same.len().max(1) as u8, index as u8)
    };
    let (cx, cy) = anchor(width, height, index, mates);
    let wrap = |a: u32, b: u32| {
        let d = a.abs_diff(b);
        d.min(width - d)
    };
    let mut spots: Vec<(u32, u32)> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .collect();
    spots.sort_by_key(|&(x, y)| (wrap(x, cx).pow(2) + y.abs_diff(cy).pow(2), y, x));
    let fortresses: Vec<Position> = world
        .player_ids()
        .into_iter()
        .filter_map(|p| world.wizard(p)?.fortress())
        .filter(|f| f.plane == plane)
        .collect();
    let need = 8 - u32::from(player.0) / 3;
    // Each pass relaxes the last: fortress distance, site distance, the size the land must feed.
    let passes: &[(u32, u32, u32)] = if site.is_some() {
        &[(0, 0, 0)]
    } else {
        &[
            (16, 3, need),
            (10, 3, need),
            (10, 1, need),
            // Last, any distance but still land that feeds five (1.60: a starting city always
            // has a maximum population of at least 5); only then anywhere at all.
            (0, 0, MIN_START_SIZE),
            (0, 0, 0),
        ]
    };
    if let Some(site) = site {
        spots = vec![(site.x, site.y)];
    }
    for &(apart, clear, size) in passes {
        for &(x, y) in &spots {
            let pos = Position { plane, x, y };
            if fortresses
                .iter()
                .any(|f| wrap(f.x, x).max(f.y.abs_diff(y)) < apart)
                || world.can_found_city(pos).is_err()
                || !crate::neutral::clear_of_sites(world, pos, clear)
                || world.economy().site_max_size(pos) < size
            {
                continue;
            }
            let events = world.apply(
                Command::PlaceFortress {
                    player,
                    pos,
                    name: name.clone(),
                    race: race_id,
                    population: world.content().rules.config.wizard.capital_figures,
                },
                FIRST_ROUND,
                rng,
            );
            let Some(city) = events.iter().find_map(|e| match e {
                f4x_core::world::Event::FortressPlaced { .. } => None,
                f4x_core::world::Event::CityFounded { city, .. } => Some(*city),
                _ => None,
            }) else {
                continue;
            };
            world.apply(
                Command::SetFarmers {
                    city,
                    farmers: world.content().rules.config.wizard.capital_farmers,
                },
                FIRST_ROUND,
                rng,
            );
            for building in buildings {
                world.scenario_set_building(city, building, true);
            }
            for &unit in &army {
                world.apply(
                    Command::CreateUnit {
                        owner: player,
                        pos,
                        unit,
                    },
                    FIRST_ROUND,
                    rng,
                );
            }
            return Ok(());
        }
    }
    Err(NewGameError::NoFortressPlace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_def::testdir::shipped;
    use f4x_def::testdir::TestResult;

    fn spec(seed: u64, wizards: u8) -> NewGame {
        let mut spec = NewGame::with_wizards(wizards);
        spec.seed = seed;
        spec.all_ai = true;
        spec
    }

    #[test]
    fn a_game_is_dealt_its_names_from_the_bundle_and_falls_back_to_ids_without_one() -> TestResult {
        let content = shipped();
        let capital_race = |w: &World| {
            w.economy()
                .cities()
                .into_iter()
                .find(|(_, c, _)| c.owner == PlayerId(0))
                .map(|(_, c, _)| c.name.clone())
        };
        let bare = new_game(content, &spec(3, 2))?.0;
        let race_id = |w: &World| {
            w.wizard(PlayerId(0))
                .and_then(|wizard| wizard.capital_race())
                .map(|r| w.content().rules.races.get(r).id.clone())
        };
        let id = race_id(&bare).ok_or("no capital race")?;
        assert_eq!(capital_race(&bare), Some(format!("{id}_city_00")));
        assert_eq!(bare.units().hero_type_name(0).as_deref(), Some("dwarf"));
        // The same game with name lists for every race and the dwarf hero.
        let mut named = spec(3, 2);
        for race in content.rules.races.defs() {
            named
                .names
                .cities
                .insert(race.num, vec![format!("Town of {}", race.id)]);
        }
        named.names.heroes.insert(0, vec!["Brax".into()]);
        let w = new_game(content, &named)?.0;
        assert_eq!(capital_race(&w), Some(format!("Town of {id}")));
        assert_eq!(w.units().hero_type_name(0).as_deref(), Some("Brax"));
        assert_eq!(w.names().heroes[&0], ["Brax"]);
        Ok(())
    }

    #[test]
    fn above_normal_arcanus_computer_wizards_are_limited_to_five_races_unless_the_switch_is_off(
    ) -> TestResult {
        let content = shipped();
        let arcanus_races = |difficulty: u8, limit: bool| -> Result<Vec<u16>, String> {
            let mut out = Vec::new();
            for seed in 0..30 {
                let mut sp = spec(seed, 5);
                sp.settings.basic.difficulty = difficulty;
                sp.settings.rules.limit_ai_races = limit;
                let w = new_game(content, &sp)?.0;
                for k in 0..5 {
                    let race = w
                        .wizard(PlayerId(k))
                        .and_then(|wizard| wizard.capital_race())
                        .ok_or("no race")?;
                    let def = w.content().rules.races.get(race);
                    if !def.myrror {
                        out.push(def.num);
                    }
                }
            }
            Ok(out)
        };
        let limited = arcanus_races(3, true)?;
        assert!(!limited.is_empty());
        assert!(
            limited
                .iter()
                .all(|n| content.rules.config.limited_ai_races.contains(n)),
            "{limited:?}"
        );
        assert!(arcanus_races(3, false)?.iter().any(|n| !content
            .rules
            .config
            .limited_ai_races
            .contains(n)));
        assert!(arcanus_races(2, true)?.iter().any(|n| !content
            .rules
            .config
            .limited_ai_races
            .contains(n)));
        Ok(())
    }

    #[test]
    fn a_retort_is_found_by_name_whatever_the_case_and_punctuation() {
        let rules = &shipped().rules;
        // Each retort's alias in the case and with the separators a player may type.
        for (id, def) in rules.retorts.iter() {
            for typed in [
                def.id.to_uppercase(),
                def.id.replace('_', " "),
                def.id.replace('_', "-"),
                def.id.replace('_', " ").to_uppercase(),
            ] {
                assert_eq!(parse_retort(rules, &typed), Some(id), "{typed}");
            }
        }
        assert_eq!(parse_retort(rules, "no such retort"), None);
        assert_eq!(parse_retort(rules, ""), None);
        for (id, def) in rules.retorts.iter() {
            assert_eq!(parse_retort(rules, &def.id), Some(id), "{}", def.id);
        }
    }

    /// The sizes the settings allow and the numbers of wizards the content allows: every case.
    fn every_start_spot_case() -> impl Iterator<Item = (u32, u32, u8, u8)> {
        let most = shipped().settings.max_opponents + 1;
        f4x_def::settings::MAP_WIDTHS.flat_map(move |w| {
            f4x_def::settings::MAP_HEIGHTS.flat_map(move |h| {
                (1..=most)
                    .flat_map(move |n| (0..n).map(move |k| (u32::from(w), u32::from(h), k, n)))
            })
        })
    }

    #[test]
    fn the_start_spots_are_inside_the_map_and_apart_for_every_size_and_number_of_wizards() {
        let most = shipped().settings.max_opponents + 1;
        for w in f4x_def::settings::MAP_WIDTHS.map(u32::from) {
            for h in f4x_def::settings::MAP_HEIGHTS.map(u32::from) {
                for n in 1..=most {
                    let spots: Vec<_> = (0..n).map(|k| anchor(w, h, k, n)).collect();
                    assert!(
                        spots.iter().all(|&(x, y)| x < w && (2..h - 2).contains(&y)),
                        "{w}x{h} {n} wizards: {spots:?}"
                    );
                    let mut unique = spots.clone();
                    unique.sort_unstable();
                    unique.dedup();
                    assert_eq!(unique.len(), spots.len(), "{w}x{h} {n} wizards: {spots:?}");
                }
            }
        }
        // The first of two starts in the west and the second in the east.
        assert!(anchor(60, 40, 0, 2).0 < anchor(60, 40, 1, 2).0);
    }

    /// The start spot must not depend on the platform's trigonometry. A value is either exactly
    /// halfway between two whole numbers (a quarter turn: then it rounds up, as stated), or a
    /// thousandth or more away from it, so that `f32`, `f64` and any maths library agree.
    #[test]
    fn the_start_spot_is_stable_over_every_size() {
        for (w, h, k, n) in every_start_spot_case() {
            let angle = std::f64::consts::PI + std::f64::consts::TAU * f64::from(k) / f64::from(n);
            let (fw, fh) = (f64::from(w), f64::from(h));
            let values = [
                fw / 2.0 + exact(angle.cos()) * fw / 4.0,
                fh / 2.0 + exact(angle.sin()) * fh / 3.0,
            ];
            for v in values {
                let off = (v - v.floor() - 0.5).abs();
                assert!(
                    off < 1e-9 || off > 1e-4,
                    "{w}x{h} {k}/{n}: {v} is {off} from a half"
                );
            }
            // Away from a half, the single-precision answer is the same.
            let a32 = std::f32::consts::PI + std::f32::consts::TAU * f32::from(k) / f32::from(n);
            let (w32, h32) = (w as f32, h as f32);
            let x32 = (w32 / 2.0 + a32.cos() * w32 / 4.0).round() as u32 % w;
            let y32 = (h32 / 2.0 + a32.sin() * h32 / 3.0)
                .round()
                .clamp(2.0, h32 - 3.0) as u32;
            let (x, y) = anchor(w, h, k, n);
            let near_half = |v: f64| (v - v.floor() - 0.5).abs() < 1e-9;
            if !near_half(values[0]) {
                assert_eq!(x, x32, "{w}x{h} {k}/{n}");
            }
            if !near_half(values[1]) {
                assert_eq!(y, y32, "{w}x{h} {k}/{n}");
            }
        }
    }

    #[test]
    fn a_start_row_that_is_exactly_a_half_goes_up_on_every_odd_height() {
        // The first of any number of wizards sits at the angle PI: a quarter turn, sine 0, so the
        // row is the middle of the map, x.5 on an odd height.
        for h in f4x_def::settings::MAP_HEIGHTS
            .map(u32::from)
            .filter(|h| h % 2 == 1)
        {
            let y = anchor(60, h, 0, 5).1;
            assert_eq!(y, h / 2 + 1, "height {h}");
        }
    }

    #[test]
    fn the_rivals_names_come_from_the_seed_and_the_slot_and_are_not_empty() {
        let list = |tag: &str| -> Vec<(String, f4x_def::text::Gender)> {
            (0..10)
                .map(|i| (format!("{tag}{i}"), Default::default()))
                .collect()
        };
        let names = f4x_def::names::NameBundle {
            rivals: vec![list("a"), list("b"), list("c"), list("d")],
            ..Default::default()
        };
        let name = |seed, k| fallback_name(&names, seed, k).0;
        assert_eq!(name(5, 1), name(5, 1));
        assert!(!name(5, 1).is_empty());
        let distinct: std::collections::BTreeSet<_> = (0..40).map(|s| name(s, 1)).collect();
        assert!(distinct.len() > 1, "the seed picks among the ten");
        assert!(name(3, 1).starts_with('a') && name(3, 2).starts_with('b'));
        assert!(
            name(3, 9).starts_with('d'),
            "the last list serves the later slots"
        );
        // No lists: the slot's number.
        let none = f4x_def::names::NameBundle::default();
        assert_eq!(fallback_name(&none, 5, 2).0, "rival_2");
    }

    #[test]
    fn the_plain_computer_wizards_alternate_between_two_builds() {
        let one = plain_computer_build(&Default::default(), 4, 1);
        let two = plain_computer_build(&Default::default(), 4, 2);
        assert!(
            one.books.contains_key(&RealmId::new(2) /* chaos */)
                && one.retorts.contains(&Retort::new(1) /* warlord */)
        );
        assert!(
            two.books.contains_key(&RealmId::new(3) /* life */)
                && two.retorts.contains(&Retort::new(16) /* charismatic */)
        );
        assert!(one.preset && two.preset);
        assert_eq!(
            plain_computer_build(&Default::default(), 4, 3).books,
            one.books
        );
    }

    #[test]
    fn a_game_is_refused_for_bad_settings_or_too_many_rivals() {
        let c = shipped();
        let mut bad = spec(1, 3);
        bad.settings.basic.difficulty = 9;
        assert!(new_game(c, &bad).is_err());
        let mut many = spec(1, 3);
        many.settings.basic.opponents = c.settings.max_opponents + 1;
        let err = new_game(c, &many).err();
        // The setting's own range refuses it first; the content's most is the second line.
        let refused = match &err {
            Some(NewGameError::TooManyOpponents { .. }) => true,
            Some(NewGameError::Settings(issues)) => issues
                .iter()
                .any(|i| i.detail.first().map(String::as_str) == Some("opponents")),
            _ => false,
        };
        assert!(refused, "{err:?}");
    }

    /// With `extend_tiny_islands` on no wizard starts on a speck of land: the town stands on a land
    /// mass of at least `island::MIN_LAND` squares, whatever the seed puts it on.
    #[test]
    fn with_the_setting_every_start_town_stands_on_land_enough_to_play_on() -> TestResult {
        for seed in 0..40 {
            let mut spec = spec(seed, 4);
            spec.settings.rules.extend_tiny_islands = true;
            let w = new_game(shipped(), &spec)?.0;
            let map = w.map().ok_or("no map")?;
            for k in 0..4 {
                let at = w
                    .wizard(PlayerId(k))
                    .and_then(|wizard| wizard.fortress())
                    .ok_or("no fortress")?;
                let mass = crate::island::land_mass(map, w.content(), at, usize::MAX);
                assert!(
                    mass.len() >= crate::island::MIN_LAND,
                    "seed {seed}, wizard {k}: {} squares",
                    mass.len()
                );
            }
        }
        Ok(())
    }

    /// With `extend_tiny_islands` on no city of the start, a neutral one either, stands on a speck
    /// of land: its land mass has at least `island::MIN_LAND` squares and at least four squares to
    /// work lie within two squares of it (a city's work radius).
    #[test]
    fn with_the_setting_no_city_at_the_start_is_cramped() -> TestResult {
        for seed in 0..30 {
            let mut spec = spec(seed, 3);
            spec.settings.rules.extend_tiny_islands = true;
            let w = new_game(shipped(), &spec)?.0;
            let map = w.map().ok_or("no map")?;
            let cities: Vec<Position> = w
                .economy()
                .cities()
                .iter()
                .map(|(_, _, c)| Position {
                    plane: c.plane,
                    x: c.x,
                    y: c.y,
                })
                .collect();
            assert!(cities.len() > 3, "seed {seed}: neutral cities were placed");
            for at in cities {
                let mass = crate::island::land_mass(map, w.content(), at, usize::MAX);
                assert!(
                    mass.len() >= crate::island::MIN_LAND,
                    "seed {seed}, {at:?}: {} squares",
                    mass.len()
                );
                let near = crate::island::land_near(map, w.content(), at);
                assert!(
                    near >= crate::island::MIN_NEAR,
                    "seed {seed}, {at:?}: {near} squares to work"
                );
            }
        }
        Ok(())
    }

    /// Off (the default and both rule sets), the map is what the generator made: the same game
    /// with the setting on differs only where an island was built out.
    #[test]
    fn without_the_setting_the_map_is_left_as_generated() -> TestResult {
        let content = shipped();
        let def = &content.settings;
        let base = f4x_def::settings::GameSettings::for_difficulty(def, 2);
        for set in [
            f4x_def::settings::RuleSet::V131,
            f4x_def::settings::RuleSet::V160,
        ] {
            assert!(!set.applied(base, def).rules.extend_tiny_islands, "{set:?}");
        }
        let land = |on: bool| -> Result<usize, Box<dyn std::error::Error>> {
            let mut total = 0;
            for seed in 0..10 {
                let mut spec = spec(seed, 4);
                spec.settings.rules.extend_tiny_islands = on;
                let w = new_game(content, &spec)?.0;
                let map = w.map().ok_or("no map")?;
                for plane in [Plane::Arcanus, Plane::Myrror] {
                    for y in 0..map.height() {
                        for x in 0..map.width() {
                            let t = map.plane(plane).get(x as i32, y as i32);
                            let class = t.and_then(|t| content.terrains.get(t)).map(|d| d.class);
                            total += usize::from(
                                class != Some(f4x_core::map::terrain::TerrainClass::Ocean),
                            );
                        }
                    }
                }
            }
            Ok(total)
        };
        assert!(
            land(true)? > land(false)?,
            "building out adds land somewhere"
        );
        Ok(())
    }

    #[test]
    fn every_wizard_starts_with_a_capital_an_army_and_a_small_view() -> TestResult {
        let w = new_game(shipped(), &spec(2, 3))?.0;
        for k in 0..3 {
            let p = PlayerId(k);
            let fortress = w
                .wizard(p)
                .and_then(|wizard| wizard.fortress())
                .ok_or("no fortress")?;
            let city = w.economy().city_at(fortress).ok_or("no capital")?;
            let start = &w.content().rules.config.wizard;
            assert_eq!(w.city_population(city).figures, start.capital_figures);
            assert_eq!(w.city_farmers(city), start.capital_farmers);
            let built = w.city_buildings(city);
            for &b in &start.capital_buildings {
                assert!(built.contains(&b), "wizard {k} lacks building {}", b.num());
            }
            let army = w.units().units_at(fortress);
            assert!(!army.is_empty() && army.len() <= 2, "{} units", army.len());
            assert!(w.is_explored(p, fortress));
            let seen = w.explored_squares(p);
            let all = 2
                * w.map().ok_or("no map")?.width() as usize
                * w.map().ok_or("no map")?.height() as usize;
            assert!(
                seen > 0 && seen < all / 4,
                "wizard {k} sees {seen} of {all}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_myrran_wizard_starts_on_myrror_and_the_others_on_arcanus() -> TestResult {
        let mut s = spec(2, 3);
        s.retorts.insert(Retort::new(9) /* myrran */);
        // Player 0 is the human who chose Myrran; the computer's wizards are not rolled into it.
        s.all_ai = false;
        s.books.insert(RealmId::new(0 /* nature */), 8);
        s.settings.ai.wizards.randomise = false;
        let w = new_game(shipped(), &s)?.0;
        let plane = |k| {
            w.wizard(PlayerId(k))
                .and_then(|wizard| wizard.fortress())
                .map(|f| f.plane)
        };
        assert_eq!(plane(0), Some(Plane::Myrror));
        assert_eq!(plane(1), Some(Plane::Arcanus));
        assert_eq!(plane(2), Some(Plane::Arcanus));
        Ok(())
    }

    #[test]
    fn the_same_seed_makes_the_same_world_and_another_seed_another() -> TestResult {
        let game = |seed| -> Result<(String, u64), Box<dyn std::error::Error>> {
            let (world, rng) = new_game(shipped(), &spec(seed, 3))?;
            Ok((world.snapshot().to_text()?, rng.draws()))
        };
        assert_eq!(game(4)?, game(4)?);
        assert_ne!(game(4)?.0, game(5)?.0);
        Ok(())
    }
}
