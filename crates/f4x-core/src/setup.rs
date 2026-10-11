//! Starting a game without the client: a map, the wizards, a fortress for each spread round
//! the map and a few starting units. The client's new-game screen, the console, the AI soak
//! runs and tests all start games here.

use std::collections::{BTreeMap, BTreeSet};

use crate::city::{PlayerId, Position};
use crate::content::Content;
use crate::magic::Retort;
use crate::map::Plane;
use crate::mapgen::{generate, GenParams};
use crate::player::Player;
use crate::wizard::WizardBuild;
use crate::{Command, Game};

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

/// What a new game is made from. Everything but the seed is optional.
#[derive(Debug, Clone)]
pub struct NewGame {
    pub seed: u64,
    /// How many wizards play (2 to 5); the first is player 0.
    pub wizards: u8,
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
}

impl Default for NewGame {
    fn default() -> Self {
        Self {
            seed: 0,
            wizards: 2,
            all_ai: false,
            preset: None,
            name: None,
            books: BTreeMap::new(),
            retorts: BTreeSet::new(),
            race: None,
        }
    }
}

pub fn new_game(content: &Content, spec: &NewGame) -> Result<Game, String> {
    if !(2..=5).contains(&spec.wizards) {
        return Err("a game has 2 to 5 wizards".into());
    }
    let params = GenParams::default();
    let map = generate(spec.seed, &params, content).map_err(|e| e.to_string())?;
    let mut game = Game::new(spec.seed);
    game.map = Some(map);
    game.content = content.clone();
    for k in 0..spec.wizards {
        let name = match (k, spec.all_ai) {
            (0, false) => "You".to_string(),
            _ => format!("Wizard {}", k + 1),
        };
        game.players.push(Player::new(PlayerId(k), name));
    }
    let fixed = |books: &[(&str, u8)], retort| WizardBuild {
        name: String::new(),
        books: books.iter().map(|&(r, n)| (r.to_string(), n)).collect(),
        retorts: [retort].into_iter().collect(),
        spells: Vec::new(),
        banner: None,
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
    for k in 0..spec.wizards as usize {
        let build = if k == 0 && !spec.all_ai {
            if let Some(id) = &spec.preset {
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
                    preset: false,
                }
            } else {
                fixed(&[("sorcery", 3), ("nature", 2)], Retort::Conjurer)
            }
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
                None if k % 2 == 1 => fixed(&[("chaos", 3), ("death", 2)], Retort::Warlord),
                None => fixed(&[("life", 3), ("nature", 2)], Retort::Charismatic),
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
    for k in 0..spec.wizards {
        let id = PlayerId(k);
        let mut banner = game.player(id).map_or(0, |p| p.banner);
        while used.contains(&banner) {
            banner = (banner + 1) % crate::player::BANNER_CHOICES;
        }
        used.insert(banner);
        if let Some(p) = game.player_mut(id) {
            p.banner = banner;
        }
    }
    game.human = (!spec.all_ai).then_some(HUMAN);
    // Races: player 0's choice, then the others in turn, skipping races already taken.
    let race_ids: Vec<String> = game
        .content
        .rules
        .races
        .defs()
        .iter()
        .filter(|r| !r.city_names.is_empty())
        .map(|r| r.id.clone())
        .collect();
    let mut races: Vec<String> = Vec::new();
    for k in 0..spec.wizards as usize {
        let id = if k == 0 && spec.race.is_some() {
            spec.race.clone().unwrap_or_default()
        } else {
            let free: Vec<&String> = race_ids.iter().filter(|r| !races.contains(r)).collect();
            free.get((spec.seed as usize + k * 3) % free.len().max(1))
                .map(|r| (*r).clone())
                .or_else(|| race_ids.first().cloned())
                .ok_or("no races")?
        };
        races.push(id);
    }
    for k in 0..spec.wizards {
        start_player(&mut game, &params, PlayerId(k), spec.wizards, &races[k as usize])?;
    }
    game.set_undo(true);
    Ok(game)
}

/// Where wizard `k` of `n` starts: spread round the middle of the map.
fn anchor(params: &GenParams, k: u8, n: u8) -> (u32, u32) {
    let (w, h) = (params.width as f32, params.height as f32);
    let angle = std::f32::consts::PI + std::f32::consts::TAU * f32::from(k) / f32::from(n);
    (
        (w / 2.0 + angle.cos() * w / 4.0).round() as u32 % params.width,
        (h / 2.0 + angle.sin() * h / 3.0).round().clamp(2.0, h - 3.0) as u32,
    )
}

/// Found a wizard's fortress near their start point and hand out two soldiers and a settler.
fn start_player(
    game: &mut Game,
    params: &GenParams,
    player: PlayerId,
    count: u8,
    race: &str,
) -> Result<(), String> {
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
    let (mut army, mut settlers) = (Vec::new(), None);
    for unit in &race_def.units {
        let Some(id) = rules.units.id_of(unit) else {
            continue;
        };
        let def = rules.units.get(id);
        if def.abilities.iter().any(|a| a == "create_outpost") {
            settlers.get_or_insert(id);
        } else if army.len() < 2 && def.cost > 0 {
            army.push(id);
        }
    }
    let (cx, cy) = anchor(params, player.0, count);
    let wrap = |a: u32, b: u32| {
        let d = a.abs_diff(b);
        d.min(params.width - d)
    };
    let mut spots: Vec<(u32, u32)> = (0..params.height)
        .flat_map(|y| (0..params.width).map(move |x| (x, y)))
        .collect();
    spots.sort_by_key(|&(x, y)| (wrap(x, cx).pow(2) + y.abs_diff(cy).pow(2), y, x));
    for (x, y) in spots {
        let pos = Position {
            plane: Plane::Arcanus,
            x,
            y,
        };
        if game.can_found_city(pos).is_err() {
            continue;
        }
        let events = game.apply(Command::PlaceFortress {
            player,
            pos,
            name: name.clone(),
            race: race_id,
            population: 3,
        });
        if events
            .iter()
            .any(|e| matches!(e, crate::Event::FortressPlaced { .. }))
        {
            for unit in army.into_iter().chain(settlers) {
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
