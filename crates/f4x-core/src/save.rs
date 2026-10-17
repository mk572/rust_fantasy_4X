//! Saving and loading: a snapshot of the whole game that can be written as text and
//! restored later, even into a build with different or extra content.
//!
//! A snapshot never stores definitions. Everything that refers to content (a unit's
//! definition, a building, a terrain) is saved by its string id and looked up again on load,
//! so reordering or adding to the content files does not break a save, and a save that
//! needs content that no longer exists is refused with a clear error rather than loaded
//! wrongly.
//!
//! Entities are saved in creation order with their creation numbers (`Serial`), not their
//! `hecs` ids, so a loaded game orders everything exactly as the original did and continues
//! identically: the same commands from a loaded game and from the game that was saved give
//! the same snapshots. The random stream is resumed from the seed and the number of values
//! drawn so far.

use serde::{Deserialize, Serialize};

use crate::city::{City, PlayerId, Position};
use crate::content::Content;
use crate::economy::{Buildings, CityRace, Enchantments, Farmers, Population};
use crate::fog::Fog;
use crate::magic::{Casting, Magic, Retort};
use crate::map::{FeatureId, Grid, Map, TerrainId};
use crate::player::Player;
use crate::production::{Production, ProductionItem};
use crate::rules::SpellId;
use crate::serial::Serial;
use crate::units::{Experience, Hurt, MovesLeft, Order, Owner, Unit, UnitEnchantments};
use crate::Game;

/// Bumped when the snapshot layout changes in a way old files cannot be read.
pub const SAVE_VERSION: u32 = 1;

/// The version of the game itself (the workspace version in `Cargo.toml`), written into every save.
pub const GAME_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    /// The game version that wrote the save.
    pub game_version: String,
    pub seed: u64,
    /// Values drawn from the random generator so far.
    pub draws: u64,
    pub turn: u32,
    pub next_serial: u64,
    /// The settings the game is played with.
    pub settings: crate::settings::GameSettings,
    /// Set once the game has ended.
    pub game_over: Option<crate::endgame::GameOver>,
    pub human: Option<u8>,
    /// Contact, relations, treaties and pending proposals between wizards.
    pub diplomacy: crate::diplomacy::DiplomacySave,
    /// Override (cheat) commands used in this game.
    pub override_log: Vec<String>,
    /// The prefab items already given out (`Make_Item` gives each once a game).
    pub prefab_taken: Vec<String>,
    pub map: Option<MapSave>,
    pub fog: FogSave,
    pub players: Vec<PlayerSave>,
    pub units: Vec<UnitSave>,
    pub cities: Vec<CitySave>,
    /// Countdowns for timed spells.
    pub timers: Vec<TimerSave>,
}

/// What carries a timed enchantment, by creation number or player number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimerTargetSave {
    Unit(u64),
    City(u64),
    Wizard(u8),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerSave {
    pub target: TimerTargetSave,
    pub spell: String,
    pub caster: u8,
    pub left: u16,
}

/// The map with terrain and features saved as indexes into name lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapSave {
    pub width: u32,
    pub height: u32,
    pub terrains: Vec<String>,
    pub features: Vec<String>,
    /// Arcanus, then Myrror.
    pub planes: Vec<PlaneSave>,
    /// Magic nodes with their power, aura and owner.
    pub nodes: Vec<crate::map::Node>,
    /// Tower owners `(x, y, player)`.
    pub tower_owners: Vec<(u32, u32, u8)>,
    /// Guards and treasure of towers, nodes and encounter sites.
    pub lairs: Vec<crate::map::Lair>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaneSave {
    /// Index into `MapSave::terrains` for every tile, row by row.
    pub tiles: Vec<u16>,
    /// `(tile index, index into MapSave::features)` for tiles that have a feature.
    pub features: Vec<(u32, u16)>,
    /// Tile indexes that have a road.
    pub roads: Vec<u32>,
    /// Tile indexes that have a river.
    pub rivers: Vec<u32>,
    /// `(tile index, owner's player number)` for every volcano.
    pub volcanoes: Vec<(u32, u8)>,
    /// Tile indexes whose road is enchanted.
    pub enchanted_roads: Vec<u32>,
    /// Tile indexes that are lakes.
    pub lakes: Vec<u32>,
    /// Tile indexes that are corrupted.
    pub corrupted: Vec<u32>,
}

/// Explored flags, bit-packed (least significant bit first), per player and plane.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FogSave {
    pub width: u32,
    pub height: u32,
    pub players: Vec<(u8, Vec<u8>, Vec<u8>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSave {
    pub id: u8,
    pub name: String,
    pub gold: i32,
    pub mana: i32,
    pub tax_rate: u8,
    /// The Grand Vizier chooses what the cities build.
    pub grand_vizier: bool,
    /// An independent city's player.
    pub neutral: bool,
    /// The wizard's power over the turns.
    pub history: Vec<u8>,
    pub capital_race: Option<String>,
    pub fame: i32,
    pub magic: MagicSave,
    /// The vault and the award list.
    pub armory: crate::items::armory::Armory,
    /// The hero records.
    pub roster: crate::roster::Roster,
    /// The offers waiting for an answer.
    pub offers: Vec<crate::offers::Offer>,
    pub fortress: Option<Position>,
    pub summon_at: Option<Position>,
    pub banished: bool,
    pub banished_by: Option<u8>,
    pub defeated: bool,
    pub defeated_by: Option<u8>,
    /// The player's colour.
    pub banner: Option<u8>,
    /// The player's picture.
    pub portrait: Option<u8>,
}

/// A wizard's books, retorts, spells and casting; spells are saved by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MagicSave {
    pub books: Vec<(String, u8)>,
    pub retorts: Vec<Retort>,
    pub skill_points: i32,
    pub mana_pct: u8,
    pub research_pct: u8,
    pub skill_pct: u8,
    pub known: Vec<String>,
    pub researchable: Vec<String>,
    /// The spells on offer for research.
    pub candidates: Vec<String>,
    pub researching: Option<String>,
    pub research_remaining: i32,
    /// `(spell, mana still to pay)`.
    pub casting: Option<(String, i32)>,
    /// The item being made by Enchant Item or Create Artifact.
    pub crafting: Option<crate::items::Item>,
    /// The dead hero Resurrection is bringing back.
    pub raising: Option<usize>,
    pub global: Vec<String>,
    pub skill_left: i32,
}

impl Default for MagicSave {
    fn default() -> Self {
        let m = Magic::default();
        Self {
            books: Vec::new(),
            retorts: Vec::new(),
            skill_points: 0,
            mana_pct: m.mana_pct,
            research_pct: m.research_pct,
            skill_pct: m.skill_pct,
            known: Vec::new(),
            researchable: Vec::new(),
            candidates: Vec::new(),
            researching: None,
            research_remaining: 0,
            casting: None,
            crafting: None,
            raising: None,
            global: Vec::new(),
            skill_left: 0,
        }
    }
}

impl Magic {
    fn to_save(&self, rules: &crate::rules::Rules) -> MagicSave {
        let name = |s: &SpellId| rules.spells.get(*s).id.clone();
        MagicSave {
            books: self.books.iter().map(|(r, &n)| (r.clone(), n)).collect(),
            retorts: self.retorts.iter().copied().collect(),
            skill_points: self.skill_points,
            mana_pct: self.mana_pct,
            research_pct: self.research_pct,
            skill_pct: self.skill_pct,
            known: self.known.iter().map(name).collect(),
            researchable: self.researchable.iter().map(name).collect(),
            candidates: self.candidates.iter().map(name).collect(),
            researching: self.researching.as_ref().map(name),
            research_remaining: self.research_remaining,
            casting: self.casting.map(|c| (name(&c.spell), c.remaining)),
            crafting: self.crafting.clone(),
            raising: self.raising,
            global: self.global.iter().map(name).collect(),
            skill_left: self.skill_left,
        }
    }

    fn from_save(m: MagicSave, rules: &crate::rules::Rules) -> Result<Self, LoadError> {
        let id = |s: &String| {
            rules
                .spells
                .id_of(s)
                .ok_or_else(|| LoadError::UnknownId("spell", s.clone()))
        };
        let ids = |v: &[String]| v.iter().map(id).collect::<Result<Vec<_>, _>>();
        if u32::from(m.mana_pct) + u32::from(m.research_pct) + u32::from(m.skill_pct) != 100 {
            return Err(LoadError::Inconsistent(
                "power split does not add up to 100".into(),
            ));
        }
        let known: std::collections::BTreeSet<SpellId> = ids(&m.known)?.into_iter().collect();
        let researchable: std::collections::BTreeSet<SpellId> =
            ids(&m.researchable)?.into_iter().collect();
        if known.intersection(&researchable).next().is_some() {
            return Err(LoadError::Inconsistent(
                "a spell is both known and researchable".into(),
            ));
        }
        Ok(Magic {
            books: m.books.into_iter().collect(),
            retorts: m.retorts.into_iter().collect(),
            skill_points: m.skill_points,
            mana_pct: m.mana_pct,
            research_pct: m.research_pct,
            skill_pct: m.skill_pct,
            known,
            researchable,
            candidates: ids(&m.candidates)?,
            researching: m.researching.as_ref().map(id).transpose()?,
            research_remaining: m.research_remaining,
            casting: m
                .casting
                .map(|(s, remaining)| id(&s).map(|spell| Casting { spell, remaining }))
                .transpose()?,
            crafting: m.crafting,
            raising: m.raising,
            global: ids(&m.global)?,
            skill_left: m.skill_left,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitSave {
    pub serial: u64,
    pub unit: String,
    pub owner: u8,
    pub pos: Position,
    pub hurt: u32,
    pub xp: u32,
    /// Weapon quality (`units::WeaponQuality`).
    pub weapon: u8,
    pub moves: u32,
    pub order: Option<Vec<Position>>,
    /// `(spell, caster)`.
    pub enchantments: Vec<(String, u8)>,
    /// Road work in progress: `(square, turns left, squares still to do)`.
    pub road_work: Option<(Position, u8, Vec<Position>)>,
    /// Creation number of the ship the unit is aboard.
    pub aboard: Option<u64>,
    /// The unit was made undead (`undead.rs`).
    pub undead: bool,
    /// Done or Patrol (the main screen's buttons).
    pub standby: Option<crate::units::Standby>,
    /// Held in Stasis: 0 no, 1 just put there, 2 lingering.
    pub stasis: u8,
    /// Temporary modifiers and abilities.
    pub effects: Vec<TempSave>,
    /// A hero's three items.
    pub items: Option<crate::items::HeroItems>,
    /// A hero's own record; none for other units.
    pub hero: Option<crate::hero::Hero>,
    /// A ship let go with passengers aboard, sailing for the shore (`transport::Marooned`).
    pub marooned: bool,
}

/// A temporary effect as saved; stats are named by their own serialisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempSave {
    pub kind: TempKindSave,
    pub turns: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TempKindSave {
    Modifier {
        stat: crate::rules::Stat,
        op: crate::rules::ModOp,
        value: i32,
    },
    Grant(String),
    Remove(String),
}

fn effects_save(game: &Game, e: hecs::Entity) -> Vec<TempSave> {
    use crate::temp_effects::TempKind;
    game.units()
        .temp_effects(e)
        .into_iter()
        .map(|t| TempSave {
            kind: match t.kind {
                TempKind::Modifier { stat, op, value } => {
                    TempKindSave::Modifier { stat, op, value }
                }
                TempKind::Grant(a) => TempKindSave::Grant(a),
                TempKind::Remove(a) => TempKindSave::Remove(a),
            },
            turns: t.turns,
        })
        .collect()
}

fn effects_load(world: &mut hecs::World, e: hecs::Entity, list: Vec<TempSave>) {
    use crate::temp_effects::{TempEffect, TempEffects, TempKind};
    if list.is_empty() {
        return;
    }
    let list = list
        .into_iter()
        .map(|t| TempEffect {
            kind: match t.kind {
                TempKindSave::Modifier { stat, op, value } => {
                    TempKind::Modifier { stat, op, value }
                }
                TempKindSave::Grant(a) => TempKind::Grant(a),
                TempKindSave::Remove(a) => TempKind::Remove(a),
            },
            turns: t.turns,
        })
        .collect();
    let _ = world.insert_one(e, TempEffects(list));
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemSave {
    Building(String),
    Unit(String),
    TradeGoods,
    Housing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitySave {
    pub serial: u64,
    pub name: String,
    pub owner: u8,
    pub pos: Position,
    pub figures: u32,
    pub tens: u32,
    pub farmers: u32,
    pub race: Option<String>,
    pub buildings: Vec<String>,
    /// `(spell, caster)`.
    pub enchantments: Vec<(String, u8)>,
    /// The realms a Spell Ward guards the city against.
    pub wards: Vec<String>,
    pub item: Option<ItemSave>,
    pub progress: u32,
    /// Temporary modifiers.
    pub effects: Vec<TempSave>,
    /// Taken this turn by this player, who may still raze it.
    pub conquered: Option<u8>,
    /// A building was sold this turn.
    pub sold_building: bool,
    /// The city's auto-builder plan and what it has spent so far.
    pub autobuild: Option<crate::autobuild::AutoBuild>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// The text is not a valid snapshot.
    Parse(String),
    /// The file was written by a different version of the game.
    Version { found: u32, supported: u32 },
    /// The save refers to content that is not loaded: `(kind, id)`.
    UnknownId(&'static str, String),
    /// The save contradicts itself.
    Inconsistent(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Parse(m) => write!(f, "not a readable save: {m}"),
            LoadError::Version { found, supported } => {
                write!(
                    f,
                    "save version {found} is not supported (this game reads version {supported})"
                )
            }
            LoadError::UnknownId(kind, id) => write!(
                f,
                "the save needs {kind} {id:?}, which is not in the loaded content"
            ),
            LoadError::Inconsistent(m) => write!(f, "the save is inconsistent: {m}"),
        }
    }
}

impl std::error::Error for LoadError {}

fn pack(bits: &[bool]) -> Vec<u8> {
    let mut out = vec![0u8; bits.len().div_ceil(8)];
    for (i, &b) in bits.iter().enumerate() {
        if b {
            out[i / 8] |= 1 << (i % 8);
        }
    }
    out
}

fn unpack(bytes: &[u8], len: usize) -> Option<Vec<bool>> {
    (bytes.len() == len.div_ceil(8)).then(|| {
        (0..len)
            .map(|i| bytes[i / 8] & (1 << (i % 8)) != 0)
            .collect()
    })
}

/// Index of `name` in `list`, adding it if new.
fn intern(list: &mut Vec<String>, name: &str) -> u16 {
    match list.iter().position(|n| n == name) {
        Some(i) => i as u16,
        None => {
            list.push(name.to_string());
            (list.len() - 1) as u16
        }
    }
}

impl Game {
    /// Capture everything needed to continue this game later.
    pub fn snapshot(&self) -> Snapshot {
        let rules = &self.content.rules;
        let map = self.map.as_ref().map(|map| {
            let (mut terrains, mut features) = (Vec::new(), Vec::new());
            let planes = crate::map::Plane::ALL
                .iter()
                .map(|&plane| {
                    let grid = map.plane(plane);
                    let tiles = grid
                        .tiles()
                        .iter()
                        .map(|t| {
                            intern(
                                &mut terrains,
                                self.content.terrains.get(*t).map_or("", |d| d.id.as_str()),
                            )
                        })
                        .collect();
                    let features = grid
                        .features()
                        .iter()
                        .enumerate()
                        .filter_map(|(i, f)| {
                            let def = self.content.features.get((*f)?)?;
                            Some((i as u32, intern(&mut features, &def.id)))
                        })
                        .collect();
                    let roads = grid
                        .roads()
                        .iter()
                        .enumerate()
                        .filter(|(_, &r)| r)
                        .map(|(i, _)| i as u32)
                        .collect();
                    let rivers = grid
                        .rivers()
                        .iter()
                        .enumerate()
                        .filter(|(_, &r)| r)
                        .map(|(i, _)| i as u32)
                        .collect();
                    let volcanoes = grid
                        .volcanoes()
                        .iter()
                        .enumerate()
                        .filter(|(_, &v)| v > 0)
                        .map(|(i, &v)| (i as u32, v - 1))
                        .collect();
                    let enchanted_roads = grid
                        .enchanted_roads()
                        .iter()
                        .enumerate()
                        .filter(|(_, &r)| r)
                        .map(|(i, _)| i as u32)
                        .collect();
                    PlaneSave {
                        tiles,
                        features,
                        roads,
                        rivers,
                        volcanoes,
                        enchanted_roads,
                        corrupted: grid
                            .corrupted()
                            .iter()
                            .enumerate()
                            .filter(|(_, &c)| c)
                            .map(|(i, _)| i as u32)
                            .collect(),
                        lakes: grid
                            .lakes()
                            .iter()
                            .enumerate()
                            .filter(|(_, &l)| l)
                            .map(|(i, _)| i as u32)
                            .collect(),
                    }
                })
                .collect();
            MapSave {
                width: map.width(),
                height: map.height(),
                terrains,
                features,
                planes,
                nodes: map.nodes().to_vec(),
                tower_owners: map.tower_owners().to_vec(),
                lairs: map.lairs().to_vec(),
            }
        });
        let (width, height, fog_players) = self.fog.to_parts();
        let fog = FogSave {
            width,
            height,
            players: fog_players
                .into_iter()
                .map(|(p, layers)| (p.0, pack(&layers[0]), pack(&layers[1])))
                .collect(),
        };
        let players = self
            .players
            .iter()
            .map(|p| PlayerSave {
                id: p.id.0,
                name: p.name.clone(),
                gold: p.gold,
                mana: p.mana,
                tax_rate: p.tax_rate,
                grand_vizier: p.grand_vizier,
                neutral: p.neutral,
                history: p.history.clone(),
                capital_race: p.capital_race.map(|r| rules.races.get(r).id.clone()),
                fame: p.fame,
                magic: p.magic.to_save(rules),
                armory: p.armory.clone(),
                roster: p.roster.clone(),
                offers: p.offers.clone(),
                fortress: p.fortress,
                summon_at: p.summon_at,
                banished: p.banished,
                banished_by: p.banished_by.map(|d| d.0),
                defeated: p.defeated,
                defeated_by: p.defeated_by.map(|d| d.0),
                banner: Some(p.banner),
                portrait: Some(p.portrait),
            })
            .collect();
        let units = self
            .units()
            .all_units()
            .into_iter()
            .filter_map(|u| {
                Some(UnitSave {
                    serial: self.serial_of(u).0,
                    unit: self.units().unit_def(u)?.id.clone(),
                    owner: self.units().unit_owner(u)?.0,
                    pos: self.units().unit_position(u)?,
                    hurt: self.world.get::<&Hurt>(u).map_or(0, |h| h.0),
                    xp: self.world.get::<&Experience>(u).map_or(0, |x| x.0),
                    weapon: self.units().unit_weapon_quality(u),
                    moves: self.units().unit_moves_left(u),
                    order: self.world.get::<&Order>(u).ok().map(|o| {
                        let Order::GoTo(path) = &*o;
                        path.clone()
                    }),
                    enchantments: self
                        .world
                        .get::<&UnitEnchantments>(u)
                        .map(|en| {
                            en.0.iter()
                                .map(|(s, p)| (rules.spells.get(*s).id.clone(), p.0))
                                .collect()
                        })
                        .unwrap_or_default(),
                    aboard: self
                        .world
                        .get::<&crate::transport::Aboard>(u)
                        .ok()
                        .map(|a| a.0 .0),
                    road_work: self
                        .world
                        .get::<&crate::roads::RoadWork>(u)
                        .ok()
                        .map(|w| (w.at, w.left, w.rest.clone())),
                    effects: effects_save(self, u),
                    items: self
                        .world
                        .get::<&crate::items::HeroItems>(u)
                        .ok()
                        .filter(|i| !i.is_empty())
                        .map(|i| (*i).clone()),
                    hero: self
                        .world
                        .get::<&crate::hero::Hero>(u)
                        .ok()
                        .map(|h| (*h).clone()),
                    undead: self.units().unit_is_undead(u),
                    standby: self.units().unit_standby(u),
                    marooned: self.world.get::<&crate::transport::Marooned>(u).is_ok(),
                    stasis: match self.world.get::<&crate::stasis::Stasis>(u).ok().map(|s| *s) {
                        None => 0,
                        Some(crate::stasis::Stasis::Fresh) => 1,
                        Some(crate::stasis::Stasis::Lingering) => 2,
                    },
                })
            })
            .collect();
        let item_save = |item: ProductionItem| match item {
            ProductionItem::Building(b) => ItemSave::Building(rules.buildings.get(b).id.clone()),
            ProductionItem::Unit(u) => ItemSave::Unit(rules.units.get(u).id.clone()),
            ProductionItem::TradeGoods => ItemSave::TradeGoods,
            ProductionItem::Housing => ItemSave::Housing,
        };
        let mut city_entities: Vec<hecs::Entity> = self
            .economy()
            .cities()
            .into_iter()
            .map(|(e, ..)| e)
            .collect();
        self.sort_by_serial(&mut city_entities);
        let cities = city_entities
            .into_iter()
            .filter_map(|e| {
                let city = self.world.get::<&City>(e).ok()?;
                let pos = *self.world.get::<&Position>(e).ok()?;
                let pop = self
                    .world
                    .get::<&Population>(e)
                    .map(|p| *p)
                    .unwrap_or_default();
                let production = self.economy().city_production(e);
                Some(CitySave {
                    serial: self.serial_of(e).0,
                    name: city.name.clone(),
                    owner: city.owner.0,
                    pos,
                    figures: pop.figures,
                    tens: pop.tens,
                    farmers: self.world.get::<&Farmers>(e).map_or(0, |f| f.0),
                    race: self
                        .world
                        .get::<&CityRace>(e)
                        .ok()
                        .map(|r| rules.races.get(r.0).id.clone()),
                    buildings: self
                        .world
                        .get::<&Buildings>(e)
                        .map(|b| {
                            b.0.iter()
                                .map(|&b| rules.buildings.get(b).id.clone())
                                .collect()
                        })
                        .unwrap_or_default(),
                    enchantments: self
                        .world
                        .get::<&Enchantments>(e)
                        .map(|en| {
                            en.0.iter()
                                .map(|(s, p)| (rules.spells.get(*s).id.clone(), p.0))
                                .collect()
                        })
                        .unwrap_or_default(),
                    wards: self.economy().city_wards(e),
                    item: production.item.map(item_save),
                    progress: production.progress,
                    effects: effects_save(self, e),
                    conquered: self
                        .world
                        .get::<&crate::conquest::Conquered>(e)
                        .ok()
                        .map(|c| c.0 .0),
                    sold_building: self.economy().sold_this_turn(e),
                    autobuild: self
                        .world
                        .get::<&crate::autobuild::AutoBuild>(e)
                        .ok()
                        .map(|a| *a),
                })
            })
            .collect();
        Snapshot {
            version: SAVE_VERSION,
            game_version: GAME_VERSION.to_string(),
            seed: self.rng.seed(),
            draws: self.rng.draws(),
            turn: self.turn,
            next_serial: self.next_serial,
            settings: self.settings,
            game_over: self.game_over,
            human: self.human.map(|h| h.0),
            diplomacy: self.diplomacy_to_save(),
            override_log: self.override_log.clone(),
            prefab_taken: self.prefab_taken.clone(),
            map,
            fog,
            players,
            units,
            cities,
            timers: self
                .timers
                .iter()
                .map(|t| TimerSave {
                    target: match t.target {
                        crate::timers::TimerTarget::Unit(s) => TimerTargetSave::Unit(s.0),
                        crate::timers::TimerTarget::City(s) => TimerTargetSave::City(s.0),
                        crate::timers::TimerTarget::Wizard(p) => TimerTargetSave::Wizard(p.0),
                    },
                    spell: self.content.rules.spells.get(t.spell).id.clone(),
                    caster: t.caster.0,
                    left: t.left,
                })
                .collect(),
        }
    }
}

impl Snapshot {
    /// The snapshot as readable text.
    pub fn to_text(&self) -> Result<String, LoadError> {
        let body =
            ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default().depth_limit(8))
                .map_err(|e| LoadError::Parse(e.to_string()))?;
        Ok(format!(
            "// f4x save, version {SAVE_VERSION}, game {GAME_VERSION}\n{body}\n"
        ))
    }

    pub fn from_text(text: &str) -> Result<Self, LoadError> {
        let snapshot: Snapshot =
            ron::from_str(text).map_err(|e| LoadError::Parse(e.to_string()))?;
        if snapshot.version != SAVE_VERSION {
            return Err(LoadError::Version {
                found: snapshot.version,
                supported: SAVE_VERSION,
            });
        }
        Ok(snapshot)
    }

    /// Rebuild the game, resolving every id against `content`.
    pub fn restore(self, content: Content) -> Result<Game, LoadError> {
        if self.version != SAVE_VERSION {
            return Err(LoadError::Version {
                found: self.version,
                supported: SAVE_VERSION,
            });
        }
        let mut game = Game::new(self.seed);
        game.content = content;
        game.turn = self.turn;
        game.settings = self.settings.normalised();
        game.game_over = self.game_over;
        game.human = self.human.map(PlayerId);
        game.override_log = self.override_log;
        game.prefab_taken = self.prefab_taken;
        game.next_serial = self.next_serial;
        // Resume the random stream where it stopped.
        game.rng = crate::rng::GameRng::resume(self.seed, self.draws);

        game.map = self
            .map
            .map(|m| restore_map(m, &game.content))
            .transpose()?;
        game.fog = restore_fog(self.fog, game.map.as_ref())?;
        for p in self.players {
            if game.players.iter().any(|q| q.id.0 == p.id) {
                return Err(LoadError::Inconsistent(format!(
                    "player {} appears twice",
                    p.id
                )));
            }
            let capital_race = p
                .capital_race
                .map(|r| {
                    game.content
                        .rules
                        .races
                        .id_of(&r)
                        .ok_or(LoadError::UnknownId("race", r))
                })
                .transpose()?;
            game.players.push(Player {
                bonus: crate::player::Bonus::default(),
                history: p.history,
                id: PlayerId(p.id),
                name: p.name,
                gold: p.gold,
                mana: p.mana,
                tax_rate: p.tax_rate,
                grand_vizier: p.grand_vizier,
                neutral: p.neutral,
                capital_race,
                fame: p.fame,
                magic: Magic::from_save(p.magic, &game.content.rules)?,
                armory: p.armory,
                roster: p.roster,
                offers: p.offers,
                fortress: p.fortress,
                summon_at: p.summon_at,
                banished: p.banished,
                banished_by: p.banished_by.map(PlayerId),
                defeated: p.defeated,
                defeated_by: p.defeated_by.map(PlayerId),
                banner: p
                    .banner
                    .unwrap_or(if p.id == u8::MAX {
                        crate::player::NEUTRAL_BANNER
                    } else {
                        p.id % crate::player::BANNER_CHOICES
                    })
                    .min(crate::player::NEUTRAL_BANNER),
                portrait: p
                    .portrait
                    .unwrap_or(p.id % crate::player::PORTRAITS)
                    .min(crate::player::PORTRAITS - 1),
            });
        }

        let mut serials = std::collections::BTreeSet::new();
        let mut take = |serial: u64, what: &str| -> Result<Serial, LoadError> {
            if serial == 0 || serial >= game.next_serial || !serials.insert(serial) {
                return Err(LoadError::Inconsistent(format!(
                    "{what} has a bad or repeated creation number {serial}"
                )));
            }
            Ok(Serial(serial))
        };
        let in_map = |game: &Game, pos: Position| -> bool {
            game.map
                .as_ref()
                .is_none_or(|m| pos.x < m.width() && pos.y < m.height())
        };
        let mut units = self.units;
        units.sort_by_key(|u| u.serial);
        for u in units {
            let serial = take(u.serial, "a unit")?;
            let id = game
                .content
                .rules
                .units
                .id_of(&u.unit)
                .ok_or(LoadError::UnknownId("unit", u.unit.clone()))?;
            if !in_map(&game, u.pos) {
                return Err(LoadError::Inconsistent(format!(
                    "unit {} is off the map",
                    u.unit
                )));
            }
            let entity = game.world.spawn((
                Unit(id),
                Owner(PlayerId(u.owner)),
                u.pos,
                Hurt(u.hurt),
                Experience(u.xp),
                MovesLeft(u.moves),
                serial,
            ));
            if u.weapon > 0 {
                let _ = game
                    .world
                    .insert_one(entity, crate::units::WeaponQuality(u.weapon));
            }
            if let Some(ship) = u.aboard {
                let _ = game
                    .world
                    .insert_one(entity, crate::transport::Aboard(Serial(ship)));
            }
            effects_load(&mut game.world, entity, u.effects);
            if let Some(items) = u.items {
                let _ = game.world.insert_one(entity, items);
            }
            if let Some(hero) = u.hero {
                let _ = game.world.insert_one(entity, hero);
            }
            if let Some(standby) = u.standby {
                let _ = game.world.insert_one(entity, standby);
            }
            if u.undead {
                let _ = game.world.insert_one(entity, crate::undead::Undead);
            }
            if u.marooned {
                let _ = game.world.insert_one(entity, crate::transport::Marooned);
            }
            match u.stasis {
                1 => {
                    let _ = game.world.insert_one(entity, crate::stasis::Stasis::Fresh);
                }
                2 => {
                    let _ = game
                        .world
                        .insert_one(entity, crate::stasis::Stasis::Lingering);
                }
                _ => {}
            }
            if let Some((at, left, rest)) = u.road_work {
                let _ = game
                    .world
                    .insert_one(entity, crate::roads::RoadWork { at, left, rest });
            }
            if let Some(path) = u.order {
                let _ = game.world.insert_one(entity, Order::GoTo(path));
            }
            if !u.enchantments.is_empty() {
                let list = u
                    .enchantments
                    .iter()
                    .map(|(s, p)| {
                        game.content
                            .rules
                            .spells
                            .id_of(s)
                            .map(|id| (id, PlayerId(*p)))
                            .ok_or(LoadError::UnknownId("spell", s.clone()))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let _ = game.world.insert_one(entity, UnitEnchantments(list));
            }
        }
        let mut cities = self.cities;
        cities.sort_by_key(|c| c.serial);
        for c in cities {
            let serial = take(c.serial, "a city")?;
            if !in_map(&game, c.pos) {
                return Err(LoadError::Inconsistent(format!(
                    "city {} is off the map",
                    c.name
                )));
            }
            if c.farmers > c.figures || c.tens >= 100 {
                return Err(LoadError::Inconsistent(format!(
                    "city {} has impossible population figures",
                    c.name
                )));
            }
            let rules = &game.content.rules;
            let buildings = c
                .buildings
                .iter()
                .map(|b| {
                    rules
                        .buildings
                        .id_of(b)
                        .ok_or(LoadError::UnknownId("building", b.clone()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let enchantments = c
                .enchantments
                .iter()
                .map(|(s, p)| {
                    rules
                        .spells
                        .id_of(s)
                        .map(|id| (id, PlayerId(*p)))
                        .ok_or(LoadError::UnknownId("spell", s.clone()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let item = c
                .item
                .map(|i| match i {
                    ItemSave::Building(b) => rules
                        .buildings
                        .id_of(&b)
                        .map(ProductionItem::Building)
                        .ok_or(LoadError::UnknownId("building", b)),
                    ItemSave::Unit(u) => rules
                        .units
                        .id_of(&u)
                        .map(ProductionItem::Unit)
                        .ok_or(LoadError::UnknownId("unit", u)),
                    ItemSave::TradeGoods => Ok(ProductionItem::TradeGoods),
                    ItemSave::Housing => Ok(ProductionItem::Housing),
                })
                .transpose()?;
            let race = c
                .race
                .map(|r| rules.races.id_of(&r).ok_or(LoadError::UnknownId("race", r)))
                .transpose()?;
            let entity = game.world.spawn((
                serial,
                City {
                    name: c.name,
                    owner: PlayerId(c.owner),
                },
                c.pos,
                Population {
                    figures: c.figures,
                    tens: c.tens,
                },
                Farmers(c.farmers),
                Buildings(buildings),
                Enchantments(enchantments),
                Production {
                    item,
                    progress: c.progress,
                },
            ));
            if let Some(r) = race {
                let _ = game.world.insert_one(entity, CityRace(r));
            }
            effects_load(&mut game.world, entity, c.effects);
            if !c.wards.is_empty() {
                let _ = game
                    .world
                    .insert_one(entity, crate::city_spells::SpellWards(c.wards));
            }
            if c.sold_building {
                let _ = game
                    .world
                    .insert_one(entity, crate::production::SoldBuilding);
            }
            if let Some(a) = c.autobuild {
                let _ = game.world.insert_one(entity, a);
            }
            if let Some(p) = c.conquered {
                let _ = game
                    .world
                    .insert_one(entity, crate::conquest::Conquered(PlayerId(p)));
            }
        }
        for p in &game.players {
            for pos in [p.fortress, p.summon_at].into_iter().flatten() {
                let ok = game
                    .economy()
                    .city_at(pos)
                    .is_some_and(|c| game.economy().city_owner(c) == Some(p.id));
                if !ok {
                    return Err(LoadError::Inconsistent(format!(
                        "player {} has a fortress or summoning circle where they have no city",
                        p.id.0
                    )));
                }
            }
            if p.banished && p.fortress.is_some() {
                return Err(LoadError::Inconsistent(
                    "a banished wizard has a fortress".into(),
                ));
            }
        }
        for t in self.timers {
            let spell = game
                .content
                .rules
                .spells
                .id_of(&t.spell)
                .ok_or(LoadError::UnknownId("spell", t.spell.clone()))?;
            let target = match t.target {
                TimerTargetSave::Unit(s) => crate::timers::TimerTarget::Unit(Serial(s)),
                TimerTargetSave::City(s) => crate::timers::TimerTarget::City(Serial(s)),
                TimerTargetSave::Wizard(p) => crate::timers::TimerTarget::Wizard(PlayerId(p)),
            };
            game.timers.push(crate::timers::Timer {
                target,
                spell,
                caster: PlayerId(t.caster),
                left: t.left,
            });
        }
        game.diplomacy_from_save(self.diplomacy);
        game.apply_wizard_settings();
        Ok(game)
    }
}

fn restore_map(m: MapSave, content: &Content) -> Result<Map, LoadError> {
    if m.planes.len() != 2 {
        return Err(LoadError::Inconsistent(
            "a map needs exactly two planes".into(),
        ));
    }
    let terrains = m
        .terrains
        .iter()
        .map(|t| {
            content
                .terrains
                .id_of(t)
                .ok_or(LoadError::UnknownId("terrain", t.clone()))
        })
        .collect::<Result<Vec<TerrainId>, _>>()?;
    let features = m
        .features
        .iter()
        .map(|f| {
            content
                .features
                .id_of(f)
                .ok_or(LoadError::UnknownId("feature", f.clone()))
        })
        .collect::<Result<Vec<FeatureId>, _>>()?;
    let len = (m.width as usize) * (m.height as usize);
    let mut grids = Vec::new();
    for plane in m.planes {
        let tiles = plane
            .tiles
            .iter()
            .map(|&i| terrains.get(i as usize).copied())
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                LoadError::Inconsistent("a tile names a terrain that is not in the palette".into())
            })?;
        let mut feature_layer = vec![None; len];
        for (i, f) in plane.features {
            let slot = feature_layer
                .get_mut(i as usize)
                .ok_or_else(|| LoadError::Inconsistent("a feature is off the map".into()))?;
            *slot = Some(*features.get(f as usize).ok_or_else(|| {
                LoadError::Inconsistent("a feature is not in the palette".into())
            })?);
        }
        let mut roads = vec![false; len];
        for i in plane.roads {
            *roads
                .get_mut(i as usize)
                .ok_or_else(|| LoadError::Inconsistent("a road is off the map".into()))? = true;
        }
        let mut rivers = vec![false; len];
        for i in plane.rivers {
            *rivers
                .get_mut(i as usize)
                .ok_or_else(|| LoadError::Inconsistent("a river is off the map".into()))? = true;
        }
        let mut volcanoes = vec![0u8; len];
        for (i, owner) in plane.volcanoes {
            *volcanoes
                .get_mut(i as usize)
                .ok_or_else(|| LoadError::Inconsistent("a volcano is off the map".into()))? =
                owner.saturating_add(1);
        }
        let mut corrupted = vec![false; len];
        for i in plane.corrupted {
            *corrupted
                .get_mut(i as usize)
                .ok_or_else(|| LoadError::Inconsistent("corruption is off the map".into()))? = true;
        }
        let mut lakes = vec![false; len];
        for i in plane.lakes {
            *lakes
                .get_mut(i as usize)
                .ok_or_else(|| LoadError::Inconsistent("a lake is off the map".into()))? = true;
        }
        let mut enchanted = vec![false; len];
        for i in plane.enchanted_roads {
            *enchanted.get_mut(i as usize).ok_or_else(|| {
                LoadError::Inconsistent("an enchanted road is off the map".into())
            })? = true;
        }
        grids.push(
            Grid::from_parts(m.width, m.height, tiles, feature_layer, roads)
                .ok_or_else(|| {
                    LoadError::Inconsistent("a plane does not match the map size".into())
                })?
                .with_rivers(rivers)
                .with_volcanoes(volcanoes)
                .with_enchanted_roads(enchanted)
                .with_lakes(lakes)
                .with_corruption(corrupted),
        );
    }
    let myrror = grids.pop().expect("two planes");
    let arcanus = grids.pop().expect("two planes");
    let mut map = Map::new(arcanus, myrror);
    for n in &m.nodes {
        let on_map = n.x < m.width && n.y < m.height;
        if !on_map || n.aura.iter().any(|&(x, y)| x >= m.width || y >= m.height) {
            return Err(LoadError::Inconsistent("a node is off the map".into()));
        }
        if n.owner.is_some_and(|o| o > 250) {
            return Err(LoadError::Inconsistent("a node has a bad owner".into()));
        }
    }
    map.set_nodes(m.nodes);
    map.set_tower_owners(m.tower_owners);
    map.set_lairs(m.lairs);
    Ok(map)
}

fn restore_fog(f: FogSave, map: Option<&Map>) -> Result<Fog, LoadError> {
    if f.width == 0 && f.players.is_empty() {
        return Ok(Fog::default());
    }
    if let Some(m) = map {
        if (m.width(), m.height()) != (f.width, f.height) {
            return Err(LoadError::Inconsistent(
                "the explored area does not match the map size".into(),
            ));
        }
    }
    let len = f.width as usize * f.height as usize;
    let mut players = Vec::new();
    for (id, a, m) in f.players {
        let (Some(a), Some(m)) = (unpack(&a, len), unpack(&m, len)) else {
            return Err(LoadError::Inconsistent(
                "an explored layer has the wrong length".into(),
            ));
        };
        players.push((PlayerId(id), [a, m]));
    }
    Fog::from_parts(f.width, f.height, players)
        .ok_or_else(|| LoadError::Inconsistent("the explored area is malformed".into()))
}

#[cfg(test)]
mod tests;
