//! Starting a game without the client: a map, the wizards, a fortress for each spread round
//! the map and a few starting units. The client's new-game screen, the console, the AI soak
//! runs and tests all start games here.

use std::collections::{BTreeMap, BTreeSet};

use crate::content::Content;
use crate::game::setting::GameSettings;
use crate::map::mapgen::{generate, GenParams};
use crate::map::{Plane, Position};
use crate::player::{Player, PlayerId};

use crate::game::{Command, Game};
use crate::wizard::magic::Retort;
use crate::wizard::WizardBuild;

pub const HUMAN: PlayerId = PlayerId(0);
pub const RIVAL: PlayerId = PlayerId(1);

const RETORTS: [Retort; 18] = [
    Retort::Alchemy,
    Retort::Warlord,
    Retort::ChaosMastery,
    Retort::NatureMastery,
    Retort::SorceryMastery,
    Retort::Channeler,
    Retort::Archmage,
    Retort::Artificer,
    Retort::Conjurer,
    Retort::SageMaster,
    Retort::Myrran,
    Retort::DivinePower,
    Retort::InfernalPower,
    Retort::Famous,
    Retort::Runemaster,
    Retort::Charismatic,
    Retort::ManaFocusing,
    Retort::NodeMastery,
];

/// A retort by name (case and punctuation ignored): `alchemy`, `sage_master`, `sagemaster`.
pub fn parse_retort(text: &str) -> Option<Retort> {
    let key: String = text
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    RETORTS
        .iter()
        .copied()
        .find(|r| format!("{r:?}").to_lowercase() == key)
}

/// The original's land sizes, as a share of each plane's tiles (`NEWG_CreateLands`: 360, 480 and
/// 720 of 2400).
pub use crate::game::setting::{LAND_LARGE, LAND_MEDIUM, LAND_SMALL};

/// What a new game is made from. Everything but the seed is optional.
#[derive(Debug, Clone)]
pub struct NewGame {
    pub seed: u64,
    /// Every wizard is a ready-made one for an AI to play (no human): `human` stays unset.
    pub all_ai: bool,
    /// A ready-made wizard's id for player 0; the default is a fixed sorcery and nature wizard.
    pub preset: Option<String>,
    /// A custom wizard for player 0 (used when no preset is given).
    pub name: Option<String>,
    pub books: BTreeMap<String, u8>,
    pub retorts: BTreeSet<Retort>,
    /// Player 0's race id; the default is the first race with city names.
    pub race: Option<String>,
    /// A finished wizard for player 0 (name, books, retorts, spells, banner); it wins over
    /// `preset`, `name`, `books` and `retorts`.
    pub build: Option<WizardBuild>,
    /// The settings the game is played with: difficulty, the number of opponents (2 to 5
    /// wizards play in all, the first is player 0), land size, node strength and the rest.
    pub settings: GameSettings,
}

impl NewGame {
    /// A new game with `wizards` playing (2 to 5) and every other choice at its default.
    pub fn with_wizards(wizards: u8) -> Self {
        let mut spec = Self::default();
        spec.settings.basic.opponents = wizards.saturating_sub(1);
        spec
    }
}

impl Default for NewGame {
    fn default() -> Self {
        Self {
            seed: 0,
            all_ai: false,
            preset: None,
            name: None,
            books: BTreeMap::new(),
            retorts: BTreeSet::new(),
            race: None,
            build: None,
            // The symmetric game: tests, the soak and the learning AI. The client and the
            // console's `difficulty=` start from a level instead.
            settings: GameSettings::flat(),
        }
    }
}

pub fn new_game(content: &Content, spec: &NewGame) -> Result<Game, String> {
    let problems = spec.settings.validate();
    if !problems.is_empty() {
        return Err(problems.join("; "));
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
        lair_guard_percent: settings.world.lair_guard_percent,
        lair_treasure_percent: settings.world.lair_treasure_percent,
        ..GenParams::default()
    };
    let map = generate(spec.seed, &params, content).map_err(|e| e.to_string())?;
    let mut game = Game::new(spec.seed);
    game.settings = settings;
    game.map = Some(map);
    game.content = content.clone();
    for k in 0..wizards {
        let name = match (k, spec.all_ai) {
            (0, false) => "You".to_string(),
            _ => format!("Wizard {}", k + 1),
        };
        game.add_player(PlayerId(k), name, Player::new());
    }
    let fixed = |books: &[(&str, u8)], retort| WizardBuild {
        name: String::new(),
        books: books.iter().map(|&(r, n)| (r.to_string(), n)).collect(),
        retorts: [retort].into_iter().collect(),
        spells: Vec::new(),
        banner: None,
        portrait: None,
        preset: true,
    };
    let preset_ids: Vec<String> = game
        .content
        .rules
        .wizards
        .defs()
        .iter()
        .map(|w| w.id.clone())
        .collect();
    let mut taken: Vec<String> = Vec::new();
    let mut builds = Vec::new();
    // The computer wizards, made together when the settings ask for it (see `ai_wizards`).
    let mut made: Option<std::collections::VecDeque<WizardBuild>> = None;
    for k in 0..wizards as usize {
        let build = if k == 0 && !spec.all_ai {
            if let Some(build) = &spec.build {
                if let Some(id) = build.preset.then(|| spec.preset.clone()).flatten() {
                    taken.push(id);
                }
                build.clone()
            } else if let Some(id) = &spec.preset {
                taken.push(id.clone());
                game.preset_build(id)
                    .ok_or_else(|| format!("no ready-made wizard {id:?}"))?
            } else if !spec.books.is_empty() {
                WizardBuild {
                    name: spec.name.clone().unwrap_or_else(|| "Wizard".into()),
                    books: spec.books.clone(),
                    retorts: spec.retorts.clone(),
                    spells: Vec::new(),
                    banner: None,
                    portrait: None,
                    preset: false,
                }
            } else {
                fixed(&[("sorcery", 3), ("nature", 2)], Retort::Conjurer)
            }
        } else if let Some(build) = settings
            .ai
            .wizards
            .randomise
            .then(|| {
                made.get_or_insert_with(|| {
                    let computers = usize::from(wizards) - usize::from(!spec.all_ai);
                    let first = usize::from(!spec.all_ai);
                    let plain =
                        |i: usize| plain_computer_build(&game.content, spec.seed, first + i);
                    let plain: Vec<WizardBuild> = (0..computers).map(plain).collect();
                    crate::wizard::ai::make(&mut game, &settings, &mut taken, computers, &|i| {
                        plain[i].clone()
                    })
                    .into()
                })
                .pop_front()
            })
            .flatten()
        {
            build
        } else {
            // The next ready-made wizard nobody has yet, starting from a seed-chosen one.
            let free: Vec<&String> = preset_ids.iter().filter(|id| !taken.contains(id)).collect();
            let pick = free
                .get((spec.seed as usize + k) % free.len().max(1))
                .and_then(|id| game.preset_build(id).map(|b| ((*id).clone(), b)));
            match pick {
                Some((id, b)) => {
                    taken.push(id);
                    b
                }
                None => plain_computer_build(&game.content, spec.seed, k),
            }
        };
        builds.push(build);
    }
    for (k, build) in builds.iter().enumerate() {
        game.create_wizard(PlayerId(k as u8), build)
            .map_err(|e| format!("{e:?}"))?;
    }
    // Every wizard has a banner of their own.
    let mut used = BTreeSet::new();
    for k in 0..wizards {
        let id = PlayerId(k);
        let mut banner = game.banner(id);
        while used.contains(&banner) {
            banner = (banner + 1) % crate::player::BANNER_CHOICES;
        }
        used.insert(banner);
        game.wizard_set(id, crate::wizard::entity::Banner(banner));
    }
    // And a picture of their own.
    let mut shown = BTreeSet::new();
    for k in 0..wizards {
        let id = PlayerId(k);
        let mut portrait = game.portrait(id);
        while shown.contains(&portrait) {
            portrait = (portrait + 1) % crate::player::PORTRAITS;
        }
        shown.insert(portrait);
        game.wizard_set(id, crate::wizard::entity::Portrait(portrait));
    }
    game.set_human((!spec.all_ai).then_some(HUMAN));
    game.apply_wizard_settings();
    for k in 0..wizards {
        game.give_main_realm_spell(PlayerId(k));
    }
    for k in 0..wizards {
        let id = PlayerId(k);
        let human = game.human() == Some(id);
        let (gold, mana) = (
            settings.wizards.starting_gold.of(human),
            settings.wizards.starting_mana.of(human),
        );
        if let Some(t) = game.treasury_mut(id) {
            t.gold = gold;
            t.mana = mana;
        }
    }
    // Races: player 0's choice, then the others in turn, skipping races already taken. A
    // Myrran wizard has a Myrror race and starts there; everyone else an Arcanus one.
    let race_defs = game.content.rules.races.defs().to_vec();
    let mut races: Vec<String> = Vec::new();
    for k in 0..wizards as usize {
        let myrran = is_myrran(&game, PlayerId(k as u8));
        let on_plane = |r: &&crate::rules::RaceDef| !r.city_names.is_empty() && r.myrror == myrran;
        let id = match spec.race.as_ref().filter(|_| k == 0) {
            Some(want) => {
                let ok = race_defs.iter().filter(on_plane).any(|r| &r.id == want);
                if !ok {
                    return Err(format!(
                        "race {want:?} is not one of the {} races",
                        if myrran { "Myrror" } else { "Arcanus" }
                    ));
                }
                want.clone()
            }
            None => {
                let all: Vec<&String> = race_defs.iter().filter(on_plane).map(|r| &r.id).collect();
                let free: Vec<&String> =
                    all.iter().copied().filter(|r| !races.contains(r)).collect();
                let pool = if free.is_empty() { all } else { free };
                pool.get((spec.seed as usize + k * 3) % pool.len().max(1))
                    .map(|r| (*r).clone())
                    .ok_or("no races")?
            }
        };
        races.push(id);
    }
    for k in 0..wizards {
        start_player(&mut game, PlayerId(k), wizards, &races[k as usize])?;
        reveal_start(&mut game, PlayerId(k), settings.world.start_reveal_radius.0);
    }
    // Independent cities on both planes, once the wizards' fortresses are down.
    for plane in [Plane::Arcanus, Plane::Myrror] {
        game.generate_neutral_cities(plane);
    }
    game.assign_temperaments();
    // The heroes' abilities are rolled last, so the map and the wizards' start stay as they were.
    game.init_heroes();
    game.set_undo(true);
    Ok(game)
}

/// The plain build of computer wizard `k` when no ready-made wizard is free: a chaos and death
/// wizard or a life and nature one, with a name from the lists.
fn plain_computer_build(content: &Content, seed: u64, k: usize) -> WizardBuild {
    let (books, retort): (&[(&str, u8)], Retort) = if k % 2 == 1 {
        (&[("chaos", 3), ("death", 2)], Retort::Warlord)
    } else {
        (&[("life", 3), ("nature", 2)], Retort::Charismatic)
    };
    WizardBuild {
        name: fallback_name(content, seed, k),
        books: books.iter().map(|&(r, n)| (r.to_string(), n)).collect(),
        retorts: [retort].into_iter().collect(),
        spells: Vec::new(),
        banner: None,
        portrait: None,
        preset: true,
    }
}

/// A name for rival `k` when no ready-made wizards are loaded: one of the ten in that slot's list
/// (`content/wizard_names.ron`), picked from the seed without touching the game's random stream.
/// Empty if there is no list, which leaves the plain "Wizard N".
fn fallback_name(content: &Content, seed: u64, k: usize) -> String {
    let lists = &content.rules.wizard_names;
    let Some(list) = lists.get(k.saturating_sub(1).min(lists.len().saturating_sub(1))) else {
        return String::new();
    };
    if list.is_empty() {
        return String::new();
    }
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
    game.wizard(player).is_some_and(|w| w.has(Retort::Myrran))
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
const CAPITAL_BUILDINGS: [&str; 3] = ["smithy", "barracks", "builders_hall"];

/// Citizens in the capital at the start, and how many of them farm.
const CAPITAL_FIGURES: u32 = 4;
const CAPITAL_FARMERS: u32 = 3;

/// A wizard's first view of the world: only the land within `radius` of their fortress (what
/// founding the capital and creating its army revealed is replaced).
fn reveal_start(game: &mut Game, player: PlayerId, radius: f32) {
    let Some(fortress) = game.player(player).and_then(|p| p.fortress) else {
        return;
    };
    game.fog.forget(player);
    game.fog.explore_within(player, fortress, radius);
}

/// Found a wizard's fortress near their start point, as the original's `Generate_Home_Cities`
/// does: a capital of four with the race's spearmen and swordsmen (two swordsmen for a race
/// without spearmen), a smithy, barracks and builders hall, and three citizens on the farms.
/// The site has to be land that can feed eight (seven for the later wizards), clear of other
/// fortresses and of nodes, towers and lairs; those distances relax if nothing fits.
fn start_player(game: &mut Game, player: PlayerId, count: u8, race: &str) -> Result<(), String> {
    let rules = &game.content.rules;
    let race_def = rules
        .races
        .by_name(race)
        .ok_or_else(|| format!("no race {race:?}"))?;
    let race_id = rules.races.id_of(&race_def.id);
    let name = race_def
        .city_names
        .first()
        .cloned()
        .unwrap_or_else(|| "Capital".into());
    let named = |what: &str| {
        race_def
            .units
            .iter()
            .filter_map(|u| rules.units.id_of(u))
            .find(|&id| rules.units.get(id).name == what)
    };
    let swordsmen = named("Swordsmen");
    let army: Vec<_> = [named("Spearmen").or(swordsmen), swordsmen]
        .into_iter()
        .flatten()
        .collect();
    let buildings: Vec<_> = CAPITAL_BUILDINGS
        .iter()
        .filter_map(|b| rules.buildings.id_of(b))
        .collect();
    let plane = if is_myrran(game, player) {
        Plane::Myrror
    } else {
        Plane::Arcanus
    };
    let (width, height) = game
        .map
        .as_ref()
        .map(|m| (m.width(), m.height()))
        .ok_or("no map")?;
    let (cx, cy) = anchor(width, height, player.0, count);
    let wrap = |a: u32, b: u32| {
        let d = a.abs_diff(b);
        d.min(width - d)
    };
    let mut spots: Vec<(u32, u32)> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .collect();
    spots.sort_by_key(|&(x, y)| (wrap(x, cx).pow(2) + y.abs_diff(cy).pow(2), y, x));
    let fortresses: Vec<Position> = game
        .players()
        .iter()
        .filter_map(|(_, p)| p.fortress)
        .filter(|f| f.plane == plane)
        .collect();
    let need = 8 - u32::from(player.0) / 3;
    // Each pass relaxes the last: fortress distance, site distance, the size the land must feed.
    for (apart, clear, size) in [(16, 3, need), (10, 3, need), (10, 1, need), (0, 0, 0)] {
        for &(x, y) in &spots {
            let pos = Position { plane, x, y };
            if fortresses
                .iter()
                .any(|f| wrap(f.x, x).max(f.y.abs_diff(y)) < apart)
                || game.can_found_city(pos).is_err()
                || !game.clear_of_sites(pos, clear)
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
                crate::game::Event::FortressPlaced { .. } => None,
                crate::game::Event::CityFounded { city, .. } => Some(*city),
                _ => None,
            }) else {
                continue;
            };
            let _ = game
                .world
                .insert_one(city, crate::city::economy::Farmers(CAPITAL_FARMERS));
            let _ = game
                .world
                .insert_one(city, crate::city::economy::Buildings(buildings));
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
