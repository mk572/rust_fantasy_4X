//! Starting a game without the client: a map, the wizards, a fortress for each spread round
//! the map and a few starting units. The client's new-game screen, the console, the AI soak
//! runs and tests all start games here.
//!
//! Tests: below, and in setup/original.rs for the original's fortress sites.

use f4x_def::half_int::HalfInt;
use std::collections::{BTreeMap, BTreeSet};

mod original;

use crate::mapgen::{generate, GenParams};
use f4x_core::game::setting::GameSettings;
use f4x_core::map::{Plane, Position};
use f4x_core::player::PlayerId;
use f4x_core::rules::{BuildingId, RealmId};
use f4x_def::content::Content;

use f4x_core::game::{Command, Game};
use f4x_core::wizard::magic::Retort;
use f4x_core::wizard::WizardBuild;

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
pub use f4x_core::game::setting::{LAND_LARGE, LAND_MEDIUM, LAND_SMALL};

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

pub fn new_game(content: &Content, spec: &NewGame) -> Result<Game, String> {
    let problems = spec.settings.validate();
    if !problems.is_empty() {
        return Err(problems.join("; "));
    }
    // The hooks and special spells the code implements are the only ones the content may name.
    let hooks = f4x_core::hook::problems(&content.rules);
    if !hooks.is_empty() {
        return Err(hooks.join("; "));
    }
    if spec.settings.basic.opponents > content.settings.max_opponents {
        return Err(format!(
            "{} opponents is over the most this content allows, {}",
            spec.settings.basic.opponents, content.settings.max_opponents
        ));
    }
    let settings = spec.settings;
    let wizards = settings.basic.opponents + 1;
    let params = GenParams {
        land_percent: settings.basic.land_percent,
        difficulty: settings.basic.difficulty,
        magic_strength: settings.basic.magic_strength,
        original_generator: settings.basic.original_map_generator,
        lair_guard_percent: settings.world.lair_guard_percent,
        lair_treasure_percent: settings.world.lair_treasure_percent,
        ..GenParams::sized(
            u32::from(settings.basic.map_width),
            u32::from(settings.basic.map_height),
        )
    };
    let params = GenParams {
        tower_count: settings.basic.tower_count(),
        ..params
    };
    let map = generate(spec.seed, &params, content).map_err(|e| e.to_string())?;
    let mut game = Game::begin_world(
        spec.seed,
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
        game.add_player(PlayerId(k), name);
    }
    let fixed = |books: &[(RealmId, u8)], retort| WizardBuild {
        name: String::new(),
        gender: Default::default(),
        books: books.iter().map(|&(r, n)| (r, n)).collect(),
        retorts: [retort].into_iter().collect(),
        spells: Vec::new(),
        banner: None,
        portrait: None,
        preset: true,
    };
    let preset_ids: Vec<u16> = game
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
                game.preset_build(num)
                    .ok_or_else(|| format!("no ready-made wizard {num}"))?
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
                fixed(
                    &[
                        (RealmId::new(1) /* sorcery */, 3),
                        (RealmId::new(0) /* nature */, 2),
                    ],
                    Retort::new(15), /* conjurer */
                )
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
                    let presets = game.content().rules.wizards.defs().to_vec();
                    crate::computer_wizard::make(
                        &mut game,
                        &presets,
                        &settings,
                        &mut taken,
                        computers,
                        human_myrran,
                        &|i| plain[i].clone(),
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
                .get((spec.seed as usize + k) % free.len().max(1))
                .and_then(|&&id| game.preset_build(id).map(|b| (id, b)));
            match pick {
                Some((id, b)) => {
                    taken.push(id);
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
            &mut game,
            &mut builds[first..],
            human_myrran,
            usize::from(wizards),
            settings.wizards.picks.ai,
        );
    }
    for (k, build) in builds.iter().enumerate() {
        game.create_wizard(PlayerId(k as u8), build)
            .map_err(|e| format!("{e:?}"))?;
    }
    // Every wizard has a banner of their own.
    let mut used = BTreeSet::new();
    for k in 0..wizards {
        let id = PlayerId(k);
        let mut banner = game.player(id).map_or(0, |p| p.banner());
        // Each wizard has a colour of their own while there are colours left.
        while used.len() < usize::from(f4x_core::player::BANNER_CHOICES) && used.contains(&banner) {
            banner = (banner + 1) % f4x_core::player::BANNER_CHOICES;
        }
        used.insert(banner);
        if let Some(mut w) = game.wizard_mut(id) {
            w.put(Some(f4x_core::wizard::entity::Banner(banner)));
        }
    }
    // And a picture of their own.
    let mut shown = BTreeSet::new();
    for k in 0..wizards {
        let id = PlayerId(k);
        let mut portrait = game.player(id).map_or(0, |p| p.portrait());
        while shown.contains(&portrait) {
            portrait = (portrait + 1) % f4x_core::player::PORTRAITS;
        }
        shown.insert(portrait);
        if let Some(mut w) = game.wizard_mut(id) {
            w.put(Some(f4x_core::wizard::entity::Portrait(portrait)));
        }
    }
    game.set_human((!spec.all_ai).then_some(HUMAN));
    game.apply_wizard_settings();
    for k in 0..wizards {
        crate::main_realm_spell::give_main_realm_spell(&mut game, PlayerId(k));
    }
    for k in 0..wizards {
        let id = PlayerId(k);
        let human = game.human() == Some(id);
        let (gold, mana) = (
            settings.wizards.starting_gold.of(human),
            settings.wizards.starting_mana.of(human),
        );
        if let Some(t) = game.treasury_mut(id) {
            t.gold = HalfInt::from_int(gold);
            t.mana = HalfInt::from_int(mana);
        }
    }
    // Races: player 0's choice, then the others in turn, skipping races already taken. A
    // Myrran wizard has a Myrror race and starts there; everyone else an Arcanus one.
    let race_defs = game.content().rules.races.defs().to_vec();
    let mut races: Vec<u16> = Vec::new();
    for k in 0..wizards as usize {
        let myrran = is_myrran(&game, PlayerId(k as u8));
        let on_plane = |r: &&f4x_core::rules::RaceDef| r.myrror == myrran;
        let id = match spec.race.as_ref().filter(|_| k == 0) {
            Some(want) => {
                let ok = race_defs.iter().filter(on_plane).any(|r| r.num == *want);
                if !ok {
                    return Err(format!(
                        "race {want} is not one of the {} races",
                        if myrran { "Myrror" } else { "Arcanus" }
                    ));
                }
                *want
            }
            None => {
                // The original's limit: above Normal an Arcanus computer wizard has one of five races.
                let limited =
                    settings.rules.limit_ai_races && !myrran && settings.basic.difficulty > 2;
                let all: Vec<u16> = race_defs
                    .iter()
                    .filter(on_plane)
                    .map(|r| r.num)
                    .filter(|n| {
                        !limited
                            || game.human() == Some(PlayerId(k as u8))
                            || f4x_def::settings::LIMITED_AI_RACES.contains(n)
                    })
                    .collect();
                let free: Vec<u16> = all.iter().copied().filter(|r| !races.contains(r)).collect();
                let pool = if free.is_empty() { all } else { free };
                pool.get((spec.seed as usize + k * 3) % pool.len().max(1))
                    .copied()
                    .ok_or("no races")?
            }
        };
        races.push(id);
    }
    // The original's rolled fortress sites, when the setting asks for them; if no pass finds
    // them all the wizards start where `start_player` looks for them.
    let rolled = settings.basic.original_map_generator.then(|| {
        let planes: Vec<Plane> = (0..wizards)
            .map(|k| {
                if is_myrran(&game, PlayerId(k)) {
                    Plane::Myrror
                } else {
                    Plane::Arcanus
                }
            })
            .collect();
        original::fortress_sites(&mut game, &planes)
    });
    for k in 0..wizards {
        let site = rolled.clone().flatten().map(|v| v[k as usize]);
        start_player(&mut game, PlayerId(k), wizards, races[k as usize], site)?;
        reveal_start(&mut game, PlayerId(k), settings.world.start_reveal_radius);
    }
    // Independent cities on both planes, once the wizards' fortresses are down.
    for plane in [Plane::Arcanus, Plane::Myrror] {
        crate::neutral::generate_neutral_cities(&mut game, plane);
    }
    crate::temperament::assign_temperaments(&mut game);
    // The heroes' abilities are rolled last, so the map and the wizards' start stay as they were.
    init_heroes(&mut game);
    // Undo takes a snapshot of the whole game for every command: only a person needs it.
    game.set_undo(!spec.all_ai);
    Ok(game)
}

/// The plain build of computer wizard `k` when no ready-made wizard is free: a chaos and death
/// wizard or a life and nature one, with a name from the lists.
/// Make the game's heroes (`Init_Heroes`): one of each hero type, in the template order, which is
/// the order the original draws their abilities in.
fn init_heroes(game: &mut Game) {
    for kind in 0..game.content().rules.hero_templates.len() {
        game.add_hero(kind);
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
fn is_myrran(game: &Game, player: PlayerId) -> bool {
    game.wizard(player)
        .is_some_and(|w| w.has(Retort::new(9) /* myrran */))
}

/// Where wizard `k` of `n` starts: spread round the middle of the map.
fn anchor(width: u32, height: u32, k: u8, n: u8) -> (u32, u32) {
    let (w, h) = (width as f32, height as f32);
    let angle = std::f32::consts::PI + std::f32::consts::TAU * f32::from(k) / f32::from(n);
    (
        (w / 2.0 + angle.cos() * w / 4.0).round() as u32 % width,
        (h / 2.0 + angle.sin() * h / 3.0)
            .round()
            .clamp(2.0, h - 3.0) as u32,
    )
}

/// The buildings a wizard's capital starts with.
const CAPITAL_BUILDINGS: &[BuildingId] = &[
    BuildingId::new(8),  // smithy
    BuildingId::new(3),  // barracks
    BuildingId::new(32), // builders_hall
];

/// Citizens in the capital at the start, and how many of them farm.
const CAPITAL_FIGURES: u32 = 4;
const CAPITAL_FARMERS: u32 = 3;

/// A wizard's first view of the world: only the land within `radius` of their fortress (what
/// founding the capital and creating its army revealed is replaced).
fn reveal_start(game: &mut Game, player: PlayerId, radius: HalfInt) {
    let Some(fortress) = game.wizard(player).and_then(|w| w.fortress()) else {
        return;
    };
    game.restrict_view(player, fortress, radius);
}

/// Found a wizard's fortress near their start point, as the original's `Generate_Home_Cities`
/// does: a capital of four with the race's spearmen and swordsmen (two swordsmen for a race
/// without spearmen), a smithy, barracks and builders hall, and three citizens on the farms.
/// The site has to be land that can feed eight (seven for the later wizards), clear of other
/// fortresses and of nodes, towers and lairs; those distances relax if nothing fits.
fn start_player(
    game: &mut Game,
    player: PlayerId,
    count: u8,
    race: u16,
    site: Option<Position>,
) -> Result<(), String> {
    let rules = &game.content().rules;
    let race_def = rules
        .races
        .by_num(race)
        .ok_or_else(|| format!("no race {race}"))?;
    let race_id = rules.races.id_numbered(race_def.num);
    let name = game.names().city(race_def.num, &race_def.id, 0);
    let army = race_def.start_army.clone();
    let buildings: Vec<_> = CAPITAL_BUILDINGS.to_vec();
    let plane = if is_myrran(game, player) {
        Plane::Myrror
    } else {
        Plane::Arcanus
    };
    let (width, height) = game
        .map()
        .map(|m| (m.width(), m.height()))
        .ok_or("no map")?;
    // Wizards on the same plane are spread round it, whatever the others do on the other.
    let (mates, index) = {
        let same: Vec<u8> = (0..count)
            .filter(|&k| is_myrran(game, PlayerId(k)) == (plane == Plane::Myrror))
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
    let fortresses: Vec<Position> = game
        .player_ids()
        .into_iter()
        .filter_map(|p| game.wizard(p)?.fortress())
        .filter(|f| f.plane == plane)
        .collect();
    let need = 8 - u32::from(player.0) / 3;
    // Each pass relaxes the last: fortress distance, site distance, the size the land must feed.
    let passes: &[(u32, u32, u32)] = if site.is_some() {
        &[(0, 0, 0)]
    } else {
        &[(16, 3, need), (10, 3, need), (10, 1, need), (0, 0, 0)]
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
                || game.can_found_city(pos).is_err()
                || !crate::neutral::clear_of_sites(game, pos, clear)
                || game.economy().site_max_size(pos) < size
            {
                continue;
            }
            let events = game.apply(Command::PlaceFortress {
                player,
                pos,
                name: name.clone(),
                race: race_id,
                population: CAPITAL_FIGURES,
            });
            let Some(city) = events.iter().find_map(|e| match e {
                f4x_core::game::Event::FortressPlaced { .. } => None,
                f4x_core::game::Event::CityFounded { city, .. } => Some(*city),
                _ => None,
            }) else {
                continue;
            };
            game.apply(Command::SetFarmers {
                city,
                farmers: CAPITAL_FARMERS,
            });
            for building in buildings {
                game.scenario_set_building(city, building, true);
            }
            for &unit in &army {
                game.apply(Command::CreateUnit {
                    owner: player,
                    pos,
                    unit,
                });
            }
            return Ok(());
        }
    }
    Err("no place for a fortress".into())
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
        let capital_race = |g: &Game| {
            g.economy()
                .cities()
                .into_iter()
                .find(|(_, c, _)| c.owner == PlayerId(0))
                .map(|(_, c, _)| c.name.clone())
        };
        let bare = new_game(content, &spec(3, 2))?;
        let race_id = |g: &Game| {
            g.wizard(PlayerId(0))
                .and_then(|w| w.capital_race())
                .map(|r| g.content().rules.races.get(r).id.clone())
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
        let g = new_game(content, &named)?;
        assert_eq!(capital_race(&g), Some(format!("Town of {id}")));
        assert_eq!(g.units().hero_type_name(0).as_deref(), Some("Brax"));
        assert_eq!(g.names().heroes[&0], ["Brax"]);
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
                let g = new_game(content, &sp)?;
                for k in 0..5 {
                    let race = g
                        .wizard(PlayerId(k))
                        .and_then(|w| w.capital_race())
                        .ok_or("no race")?;
                    let def = g.content().rules.races.get(race);
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
                .all(|n| f4x_def::settings::LIMITED_AI_RACES.contains(n)),
            "{limited:?}"
        );
        assert!(arcanus_races(3, false)?
            .iter()
            .any(|n| !f4x_def::settings::LIMITED_AI_RACES.contains(n)));
        assert!(arcanus_races(2, true)?
            .iter()
            .any(|n| !f4x_def::settings::LIMITED_AI_RACES.contains(n)));
        Ok(())
    }

    #[test]
    fn a_retort_is_found_by_name_whatever_the_case_and_punctuation() {
        let rules = &shipped().rules;
        assert_eq!(
            parse_retort(rules, "alchemy"),
            Some(Retort::new(0) /* alchemy */)
        );
        assert_eq!(
            parse_retort(rules, "Sage_Master"),
            Some(Retort::new(7) /* sage_master */)
        );
        assert_eq!(
            parse_retort(rules, "sage master"),
            Some(Retort::new(7) /* sage_master */)
        );
        assert_eq!(
            parse_retort(rules, "MANA-FOCUSING"),
            Some(Retort::new(11) /* mana_focusing */)
        );
        assert_eq!(parse_retort(rules, "no such retort"), None);
        assert_eq!(parse_retort(rules, ""), None);
        for (id, def) in rules.retorts.iter() {
            assert_eq!(parse_retort(rules, &def.id), Some(id), "{}", def.id);
        }
    }

    #[test]
    fn the_start_points_are_spread_round_the_middle_and_inside_the_map() {
        let (w, h) = (60, 40);
        for n in 1..=10u8 {
            let spots: Vec<_> = (0..n).map(|k| anchor(w, h, k, n)).collect();
            assert!(spots.iter().all(|&(x, y)| x < w && (2..h - 2).contains(&y)));
            let mut unique = spots.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique.len(), spots.len(), "{n} wizards, {spots:?}");
        }
        // The first of two starts in the west and the second in the east.
        assert!(anchor(w, h, 0, 2).0 < anchor(w, h, 1, 2).0);
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
        let err = new_game(c, &many).err().unwrap_or_default();
        assert!(err.contains("opponents"), "{err}");
    }

    #[test]
    fn every_wizard_starts_with_a_capital_an_army_and_a_small_view() -> TestResult {
        let g = new_game(shipped(), &spec(2, 3))?;
        for k in 0..3 {
            let p = PlayerId(k);
            let fortress = g
                .wizard(p)
                .and_then(|w| w.fortress())
                .ok_or("no fortress")?;
            let city = g.economy().city_at(fortress).ok_or("no capital")?;
            assert_eq!(g.city_population(city).figures, CAPITAL_FIGURES);
            assert_eq!(g.city_farmers(city), CAPITAL_FARMERS);
            let built = g.city_buildings(city);
            for &b in CAPITAL_BUILDINGS {
                assert!(built.contains(&b), "wizard {k} lacks building {}", b.num());
            }
            let army = g.units().units_at(fortress);
            assert!(!army.is_empty() && army.len() <= 2, "{} units", army.len());
            assert!(g.is_explored(p, fortress));
            let seen = g.explored_squares(p);
            let all = 2
                * g.map().ok_or("no map")?.width() as usize
                * g.map().ok_or("no map")?.height() as usize;
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
        // Every wizard is the computer's here, and the computer's retorts are rolled.
        s.settings.ai.wizards.randomise = false;
        let g = new_game(shipped(), &s)?;
        let plane = |k| {
            g.wizard(PlayerId(k))
                .and_then(|w| w.fortress())
                .map(|f| f.plane)
        };
        assert_eq!(plane(0), Some(Plane::Myrror));
        assert_eq!(plane(1), Some(Plane::Arcanus));
        assert_eq!(plane(2), Some(Plane::Arcanus));
        Ok(())
    }

    #[test]
    fn the_same_seed_makes_the_same_world_and_another_seed_another() -> TestResult {
        let text = |seed| -> Result<String, Box<dyn std::error::Error>> {
            Ok(new_game(shipped(), &spec(seed, 3))?.snapshot().to_text()?)
        };
        assert_eq!(text(4)?, text(4)?);
        assert_ne!(text(4)?, text(5)?);
        Ok(())
    }
}
