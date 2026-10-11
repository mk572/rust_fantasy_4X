//! Starting a game without the client: the same setup the client's new-game screen makes
//! (a map, the human wizard and a rival, a fortress near the middle and a few units).

use std::collections::{BTreeMap, BTreeSet};

use f4x_core::city::{PlayerId, Position};
use f4x_core::content::Content;
use f4x_core::magic::Retort;
use f4x_core::map::Plane;
use f4x_core::mapgen::{generate, GenParams};
use f4x_core::player::Player;
use f4x_core::wizard::WizardBuild;
use f4x_core::{Command, Game};

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
#[derive(Debug, Clone, Default)]
pub struct NewGame {
    pub seed: u64,
    /// A ready-made wizard's id; the default is a fixed sorcery and nature wizard.
    pub preset: Option<String>,
    /// A custom wizard (used when no preset is given).
    pub name: Option<String>,
    pub books: BTreeMap<String, u8>,
    pub retorts: BTreeSet<Retort>,
    /// The human's race id; the default is the first race with city names.
    pub race: Option<String>,
}

pub fn new_game(content: &Content, spec: &NewGame) -> Result<Game, String> {
    let params = GenParams::default();
    let map = generate(spec.seed, &params, content).map_err(|e| e.to_string())?;
    let mut game = Game::new(spec.seed);
    game.map = Some(map);
    game.content = content.clone();
    game.players.push(Player::new(HUMAN, "You"));
    game.players.push(Player::new(RIVAL, "Rival"));
    let fixed = |books: &[(&str, u8)], retort| WizardBuild {
        name: String::new(),
        books: books.iter().map(|&(r, n)| (r.to_string(), n)).collect(),
        retorts: [retort].into_iter().collect(),
        spells: Vec::new(),
        banner: None,
        preset: true,
    };
    let human = if let Some(id) = &spec.preset {
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
    };
    let rival = {
        let defs = game.content.rules.wizards.defs();
        let others: Vec<String> = defs
            .iter()
            .filter(|w| w.name != human.name)
            .map(|w| w.id.clone())
            .collect();
        others
            .get(spec.seed as usize % others.len().max(1))
            .and_then(|id| game.preset_build(id))
            .unwrap_or_else(|| fixed(&[("chaos", 3), ("death", 2)], Retort::Warlord))
    };
    for (player, build) in [(HUMAN, human), (RIVAL, rival)] {
        game.create_wizard(player, &build)
            .map_err(|e| format!("{e:?}"))?;
    }
    let human_banner = game.player(HUMAN).map_or(0, |p| p.banner);
    if let Some(p) = game.player_mut(RIVAL) {
        if p.banner == human_banner {
            p.banner = (human_banner + 1) % f4x_core::player::BANNER_CHOICES;
        }
    }
    game.human = Some(HUMAN);
    start_human(&mut game, &params, spec.race.as_deref())?;
    game.set_undo(true);
    Ok(game)
}

/// Found the human's fortress near the middle and hand out two soldiers and a settler.
fn start_human(game: &mut Game, params: &GenParams, race: Option<&str>) -> Result<(), String> {
    let rules = &game.content.rules;
    let race_def = match race {
        Some(id) => rules
            .races
            .by_name(id)
            .ok_or_else(|| format!("no race {id:?}"))?,
        None => rules
            .races
            .defs()
            .iter()
            .find(|r| !r.city_names.is_empty())
            .or_else(|| rules.races.defs().first())
            .ok_or("no races")?,
    };
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
    let (cx, cy) = (params.width / 2, params.height / 2);
    let mut spots: Vec<(u32, u32)> = (0..params.height)
        .flat_map(|y| (0..params.width).map(move |x| (x, y)))
        .collect();
    spots.sort_by_key(|&(x, y)| x.abs_diff(cx).pow(2) + y.abs_diff(cy).pow(2));
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
            player: HUMAN,
            pos,
            name: name.clone(),
            race: race_id,
            population: 3,
        });
        if events
            .iter()
            .any(|e| matches!(e, f4x_core::Event::FortressPlaced { .. }))
        {
            for unit in army.into_iter().chain(settlers) {
                game.apply(Command::CreateUnit {
                    owner: HUMAN,
                    pos,
                    unit,
                });
            }
            return Ok(());
        }
    }
    Err("no place for a fortress".into())
}
