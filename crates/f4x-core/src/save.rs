//! Saving and loading: a snapshot of the whole game that can be written as text and
//! restored later, even into a build with different or extra content.
//!
//! A snapshot never stores definitions. Everything that refers to content (a unit's
//! definition, a building, a terrain) is saved by its string id and looked up again on load,
//! so reordering or adding to the content files does not break old saves, and a save that
//! needs content that no longer exists is refused with a clear error rather than loaded
//! wrongly.
//!
//! Entities are saved in creation order with their creation numbers (`Serial`), not their
//! `hecs` ids, so a loaded game orders everything exactly as the original did and continues
//! identically: the same commands from a loaded game and from the game that was saved give
//! the same snapshots. The random stream is resumed from the seed and the number of values
//! drawn so far.

use std::collections::BTreeMap;

use rand::{RngCore, SeedableRng};
use rand_pcg::Pcg64Mcg;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub seed: u64,
    /// Values drawn from the random generator so far.
    pub draws: u64,
    pub turn: u32,
    pub next_serial: u64,
    /// The node setting: 0 weak, 1 normal, 2 powerful.
    #[serde(default = "normal_magic")]
    pub magic_strength: u8,
    /// The difficulty, 0 intro to 4 impossible.
    #[serde(default = "normal_difficulty")]
    pub difficulty: u8,
    /// Set once the game has ended.
    #[serde(default)]
    pub game_over: Option<crate::endgame::GameOver>,
    #[serde(default)]
    pub human: Option<u8>,
    /// Override (cheat) commands used in this game.
    #[serde(default)]
    pub override_log: Vec<String>,
    pub map: Option<MapSave>,
    pub fog: FogSave,
    pub players: Vec<PlayerSave>,
    pub units: Vec<UnitSave>,
    pub cities: Vec<CitySave>,
    /// Countdowns for timed spells.
    #[serde(default)]
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

fn normal_magic() -> u8 {
    1
}

fn normal_difficulty() -> u8 {
    2
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
    #[serde(default)]
    pub nodes: Vec<crate::map::Node>,
    /// Tower owners `(x, y, player)`.
    #[serde(default)]
    pub tower_owners: Vec<(u32, u32, u8)>,
    /// Guards and treasure of towers, nodes and encounter sites.
    #[serde(default)]
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
    #[serde(default)]
    pub rivers: Vec<u32>,
    /// `(tile index, owner's player number)` for every volcano.
    #[serde(default)]
    pub volcanoes: Vec<(u32, u8)>,
    /// Tile indexes whose road is enchanted.
    #[serde(default)]
    pub enchanted_roads: Vec<u32>,
    /// Tile indexes that are lakes.
    #[serde(default)]
    pub lakes: Vec<u32>,
    /// Tile indexes that are corrupted.
    #[serde(default)]
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
    #[serde(default)]
    pub grand_vizier: bool,
    pub capital_race: Option<String>,
    pub fame: i32,
    /// Absent in saves from before wizards existed.
    #[serde(default)]
    pub magic: MagicSave,
    #[serde(default)]
    pub fortress: Option<Position>,
    #[serde(default)]
    pub summon_at: Option<Position>,
    #[serde(default)]
    pub banished: bool,
    #[serde(default)]
    pub defeated: bool,
    #[serde(default)]
    pub defeated_by: Option<u8>,
    /// Absent in older saves: the player's number picks a colour.
    #[serde(default)]
    pub banner: Option<u8>,
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
    pub researching: Option<String>,
    pub research_remaining: i32,
    /// `(spell, mana still to pay)`.
    pub casting: Option<(String, i32)>,
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
            researching: None,
            research_remaining: 0,
            casting: None,
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
            researching: self.researching.as_ref().map(name),
            research_remaining: self.research_remaining,
            casting: self.casting.map(|c| (name(&c.spell), c.remaining)),
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
            researching: m.researching.as_ref().map(id).transpose()?,
            research_remaining: m.research_remaining,
            casting: m
                .casting
                .map(|(s, remaining)| id(&s).map(|spell| Casting { spell, remaining }))
                .transpose()?,
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
    pub moves: u32,
    pub order: Option<Vec<Position>>,
    /// `(spell, caster)`; absent in older saves.
    #[serde(default)]
    pub enchantments: Vec<(String, u8)>,
    /// Road work in progress: `(square, turns left, squares still to do)`.
    #[serde(default)]
    pub road_work: Option<(Position, u8, Vec<Position>)>,
    /// Creation number of the ship the unit is aboard.
    #[serde(default)]
    pub aboard: Option<u64>,
    /// Temporary modifiers and abilities.
    #[serde(default)]
    pub effects: Vec<TempSave>,
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
    game.temp_effects(e)
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
                TempKindSave::Modifier { stat, op, value } => TempKind::Modifier { stat, op, value },
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
    pub item: Option<ItemSave>,
    pub progress: u32,
    /// Temporary modifiers.
    #[serde(default)]
    pub effects: Vec<TempSave>,
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
                capital_race: p.capital_race.map(|r| rules.races.get(r).id.clone()),
                fame: p.fame,
                magic: p.magic.to_save(rules),
                fortress: p.fortress,
                summon_at: p.summon_at,
                banished: p.banished,
                defeated: p.defeated,
                defeated_by: p.defeated_by.map(|d| d.0),
                banner: Some(p.banner),
            })
            .collect();
        let units = self
            .all_units()
            .into_iter()
            .filter_map(|u| {
                Some(UnitSave {
                    serial: self.serial_of(u).0,
                    unit: self.unit_def(u)?.id.clone(),
                    owner: self.unit_owner(u)?.0,
                    pos: self.unit_position(u)?,
                    hurt: self.world.get::<&Hurt>(u).map_or(0, |h| h.0),
                    xp: self.world.get::<&Experience>(u).map_or(0, |x| x.0),
                    moves: self.unit_moves_left(u),
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
                })
            })
            .collect();
        let item_save = |item: ProductionItem| match item {
            ProductionItem::Building(b) => ItemSave::Building(rules.buildings.get(b).id.clone()),
            ProductionItem::Unit(u) => ItemSave::Unit(rules.units.get(u).id.clone()),
            ProductionItem::TradeGoods => ItemSave::TradeGoods,
            ProductionItem::Housing => ItemSave::Housing,
        };
        let mut city_entities: Vec<hecs::Entity> =
            self.cities().into_iter().map(|(e, ..)| e).collect();
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
                let production = self.city_production(e);
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
                    item: production.item.map(item_save),
                    progress: production.progress,
                    effects: effects_save(self, e),
                })
            })
            .collect();
        Snapshot {
            version: SAVE_VERSION,
            seed: self.seed,
            draws: self.draws,
            turn: self.turn,
            next_serial: self.next_serial,
            magic_strength: self.magic_strength,
            difficulty: self.difficulty,
            game_over: self.game_over,
            human: self.human.map(|h| h.0),
            override_log: self.override_log.clone(),
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
        Ok(format!("// f4x save, version {SAVE_VERSION}\n{body}\n"))
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
        game.magic_strength = self.magic_strength.min(2);
        game.difficulty = self.difficulty.min(4);
        game.game_over = self.game_over;
        game.human = self.human.map(PlayerId);
        game.override_log = self.override_log;
        game.next_serial = self.next_serial;
        // Resume the random stream where it stopped.
        let mut rng = Pcg64Mcg::seed_from_u64(self.seed);
        for _ in 0..self.draws {
            rng.next_u64();
        }
        game.rng = rng;
        game.draws = self.draws;

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
                id: PlayerId(p.id),
                name: p.name,
                gold: p.gold,
                mana: p.mana,
                tax_rate: p.tax_rate,
                grand_vizier: p.grand_vizier,
                capital_race,
                fame: p.fame,
                magic: Magic::from_save(p.magic, &game.content.rules)?,
                fortress: p.fortress,
                summon_at: p.summon_at,
                banished: p.banished,
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
            if let Some(ship) = u.aboard {
                let _ = game
                    .world
                    .insert_one(entity, crate::transport::Aboard(Serial(ship)));
            }
            effects_load(&mut game.world, entity, u.effects);
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
        }
        for p in &game.players {
            for pos in [p.fortress, p.summon_at].into_iter().flatten() {
                let ok = game
                    .city_at(pos)
                    .is_some_and(|c| game.city_owner(c) == Some(p.id));
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
    let mut players = BTreeMap::new();
    for (id, a, m) in f.players {
        let layers = [unpack(&a, len), unpack(&m, len)];
        let [Some(a), Some(m)] = layers else {
            return Err(LoadError::Inconsistent(
                "an explored layer has the wrong length".into(),
            ));
        };
        players.insert(PlayerId(id), [a, m]);
    }
    Fog::from_parts(f.width, f.height, players.into_iter().collect())
        .ok_or_else(|| LoadError::Inconsistent("the explored area is malformed".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Grid, Plane};
    use crate::testutil::content_with_economy;
    use crate::Command;

    const P0: PlayerId = PlayerId(0);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    /// A small lived-in world: two cities and an outpost, an army with an order,
    /// roads, a mine, explored land, and some turns played.
    fn game() -> Game {
        let c = content_with_economy();
        let grass = c.terrains.id_of("grassland").unwrap();
        let ocean = c.terrains.id_of("ocean").unwrap();
        let hills = c.terrains.id_of("hills").unwrap();
        let gold = c.features.id_of("gold").unwrap();
        let build = || {
            let mut g = Grid::new(24, 14, grass);
            for y in 0..14 {
                g.set(0, y, ocean);
            }
            g.set(5, 5, hills);
            g.set_feature(10, 8, Some(gold));
            g.set_road(12, 6, true);
            g.set_road(13, 6, true);
            g.set_river(14, 6, true);
            g.set_volcano(15, 6, Some(0));
            g.set_lake(16, 6, true);
            g.set_corrupted(17, 6, true);
            g
        };
        let mut map = Map::new(build(), build());
        map.set_nodes(vec![crate::map::Node {
            plane: Plane::Myrror,
            x: 3,
            y: 3,
            realm: "chaos".into(),
            power: 12,
            aura: vec![(3, 3), (4, 3), (3, 4)],
            owner: Some(0),
            guardian: true,
            warped: false,
        }]);
        let mut g = Game::new(99);
        g.content = c;
        g.magic_strength = 2;
        g.difficulty = 4;
        g.map = Some(map);
        let mut p = Player::new(P0, "Merlin");
        p.gold = 300;
        p.tax_rate = 2;
        p.grand_vizier = true;
        g.players.push(p);
        g.players.push(Player::new(PlayerId(1), "Rival"));
        let elf = g.content.rules.races.id_of("elf");
        let dwarf = g.content.rules.races.id_of("dwarf");
        g.apply(Command::FoundCity {
            owner: P0,
            pos: at(10, 6),
            name: "Home".into(),
            race: elf,
            population: 4,
        });
        g.apply(Command::FoundCity {
            owner: P0,
            pos: at(18, 6),
            name: "Delve".into(),
            race: dwarf,
            population: 2,
        });
        g.apply(Command::FoundCity {
            owner: P0,
            pos: at(10, 11),
            name: "Camp".into(),
            race: elf,
            population: 0,
        });
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let walker = g.content.rules.units.id_of("walker").unwrap();
        for _ in 0..2 {
            g.spawn_unit(P0, at(10, 6), spear);
        }
        let scout = g.spawn_unit(P0, at(10, 6), walker);
        g.apply(Command::GoTo {
            units: vec![scout],
            to: at(22, 2),
        });
        let hall = g.content.rules.buildings.id_of("builders_hall").unwrap();
        let home = g.city_at(at(10, 6)).unwrap();
        g.apply(Command::SetProduction {
            city: home,
            item: ProductionItem::Building(hall),
        });
        for _ in 0..4 {
            g.apply(Command::EndTurn);
        }
        g
    }

    fn city_named(g: &Game, name: &str) -> hecs::Entity {
        g.cities()
            .into_iter()
            .find(|(_, c, _)| c.name == name)
            .unwrap()
            .0
    }

    #[test]
    fn a_snapshot_survives_text() {
        let snap = game().snapshot();
        let text = snap.to_text().unwrap();
        assert!(text.starts_with("// f4x save, version 1"));
        assert_eq!(Snapshot::from_text(&text).unwrap(), snap);
    }

    #[test]
    fn restoring_gives_the_same_game() {
        let g = game();
        let snap = g.snapshot();
        let back = Snapshot::from_text(&snap.to_text().unwrap())
            .unwrap()
            .restore(g.content.clone())
            .unwrap();
        assert_eq!(back.snapshot(), snap);
        assert_eq!(back.turn, g.turn);
        assert_eq!(back.cities().len(), 3);
        assert_eq!(back.all_units().len(), g.all_units().len());
        assert_eq!(back.player(P0).unwrap().gold, g.player(P0).unwrap().gold);
        assert!(back.player(P0).unwrap().grand_vizier);
        // Details that are easy to lose: roads, features, exploration, orders, the outpost.
        let grid = back.map.as_ref().unwrap().plane(Plane::Arcanus);
        assert!(grid.has_road(12, 6) && grid.has_road(13, 6) && !grid.has_road(14, 6));
        assert!(grid.has_river(14, 6) && !grid.has_river(13, 6));
        assert_eq!(grid.volcano(15, 6), Some(0));
        assert_eq!(grid.volcano(14, 6), None);
        assert!(grid.is_lake(16, 6) && !grid.is_lake(15, 6));
        assert!(grid.is_corrupted(17, 6) && !grid.is_corrupted(16, 6));
        assert_eq!(
            back.map.as_ref().unwrap().nodes(),
            g.map.as_ref().unwrap().nodes()
        );
        assert_eq!(back.magic_strength, 2);
        assert_eq!(back.difficulty, 4);
        assert!(grid.feature(10, 8).is_some());
        assert_eq!(
            back.fog.explored_count(P0, Plane::Arcanus),
            g.fog.explored_count(P0, Plane::Arcanus)
        );
        let walkers_with_orders = back
            .all_units()
            .into_iter()
            .filter(|&u| back.world.get::<&Order>(u).is_ok())
            .count();
        assert_eq!(walkers_with_orders, 1);
        let camp = city_named(&back, "Camp");
        assert_eq!(back.world.get::<&Population>(camp).unwrap().figures, 0);
    }

    #[test]
    fn the_random_stream_resumes_where_it_stopped() {
        let mut g = game();
        assert!(
            g.draws > 0,
            "the world used random numbers (outposts, healing)"
        );
        let mut back = g.snapshot().restore(g.content.clone()).unwrap();
        for _ in 0..5 {
            assert_eq!(g.next_u64(), back.next_u64());
        }
    }

    #[test]
    fn a_loaded_game_continues_exactly_like_the_original() {
        let mut original = game();
        // Make entity ids get reused: remove a unit, then create another.
        let spear = original.content.rules.units.id_of("spear").unwrap();
        let victim = original.units_at(at(10, 6))[0];
        original.world.despawn(victim).unwrap();
        original.spawn_unit(P0, at(11, 6), spear);
        let mut loaded = original
            .snapshot()
            .restore(original.content.clone())
            .unwrap();
        for turn in 0..12 {
            let (a, b) = (
                original.apply(Command::EndTurn),
                loaded.apply(Command::EndTurn),
            );
            assert_eq!(a.len(), b.len(), "same events on turn {turn}");
            // Same command to both games, found by name rather than entity id.
            if turn == 3 {
                for g in [&mut original, &mut loaded] {
                    let delve = city_named(g, "Delve");
                    let hall = g.content.rules.buildings.id_of("builders_hall").unwrap();
                    g.apply(Command::SetProduction {
                        city: delve,
                        item: ProductionItem::Building(hall),
                    });
                    g.apply(Command::BuyProduction { city: delve });
                }
            }
            assert_eq!(
                original.snapshot(),
                loaded.snapshot(),
                "diverged on turn {turn}"
            );
        }
    }

    #[test]
    fn saves_do_not_depend_on_the_order_of_the_content() {
        let g = game();
        let snap = g.snapshot();
        let mut shuffled = g.content.clone();
        let mut units = shuffled.rules.units.defs().to_vec();
        units.reverse();
        shuffled.rules.units = crate::rules::Registry::new(units);
        let mut buildings = shuffled.rules.buildings.defs().to_vec();
        buildings.reverse();
        shuffled.rules.buildings = crate::rules::Registry::new(buildings);
        let back = snap.clone().restore(shuffled).unwrap();
        assert_eq!(
            back.snapshot(),
            snap,
            "the same game under a different table order"
        );
    }

    #[test]
    fn missing_content_is_reported_by_name() {
        let g = game();
        let snap = g.snapshot();
        let mut without = g.content.clone();
        let units: Vec<_> = without
            .rules
            .units
            .defs()
            .iter()
            .filter(|u| u.id != "spear")
            .cloned()
            .collect();
        without.rules.units = crate::rules::Registry::new(units);
        assert_eq!(
            snap.clone().restore(without).err(),
            Some(LoadError::UnknownId("unit", "spear".into()))
        );
        let mut no_terrain = g.content.clone();
        let terrains: Vec<_> = no_terrain
            .terrains
            .defs()
            .iter()
            .filter(|t| t.id != "hills")
            .cloned()
            .collect();
        no_terrain.terrains = crate::content::Terrains::new(terrains);
        assert_eq!(
            snap.restore(no_terrain).err(),
            Some(LoadError::UnknownId("terrain", "hills".into()))
        );
    }

    #[test]
    fn unreadable_and_foreign_saves_are_refused() {
        assert!(matches!(
            Snapshot::from_text("not a save"),
            Err(LoadError::Parse(_))
        ));
        let mut snap = game().snapshot();
        snap.version = 99;
        let text = ron::to_string(&snap).unwrap();
        assert_eq!(
            Snapshot::from_text(&text).err(),
            Some(LoadError::Version {
                found: 99,
                supported: SAVE_VERSION
            })
        );
    }

    #[test]
    fn contradictory_saves_are_refused() {
        let g = game();
        let content = g.content.clone();
        let base = g.snapshot();

        let mut dup = base.clone();
        dup.units[1].serial = dup.units[0].serial;
        assert!(
            matches!(dup.restore(content.clone()), Err(LoadError::Inconsistent(m)) if m.contains("creation number"))
        );

        let mut off = base.clone();
        off.units[0].pos = at(500, 3);
        assert!(
            matches!(off.restore(content.clone()), Err(LoadError::Inconsistent(m)) if m.contains("off the map"))
        );

        let mut farm = base.clone();
        farm.cities[0].farmers = farm.cities[0].figures + 5;
        assert!(matches!(
            farm.restore(content.clone()),
            Err(LoadError::Inconsistent(_))
        ));

        let mut fog = base.clone();
        fog.fog.players[0].1.pop();
        assert!(matches!(
            fog.restore(content.clone()),
            Err(LoadError::Inconsistent(_))
        ));

        let mut twice = base;
        twice.players.push(twice.players[0].clone());
        assert!(matches!(
            twice.restore(content),
            Err(LoadError::Inconsistent(_))
        ));
    }

    #[test]
    fn an_empty_game_round_trips() {
        let g = Game::new(5);
        let snap = g.snapshot();
        let back = Snapshot::from_text(&snap.to_text().unwrap())
            .unwrap()
            .restore(Default::default())
            .unwrap();
        assert_eq!(back.snapshot(), snap);
    }

    #[test]
    fn creation_numbers_keep_counting_after_a_load() {
        let g = game();
        let next = g.next_serial;
        let mut back = g.snapshot().restore(g.content.clone()).unwrap();
        let walker = back.content.rules.units.id_of("walker").unwrap();
        let fresh = back.spawn_unit(P0, at(11, 6), walker);
        assert_eq!(back.serial_of(fresh), Serial(next));
    }

    #[test]
    fn bit_packing_round_trips() {
        for len in [0usize, 1, 7, 8, 9, 100] {
            let bits: Vec<bool> = (0..len).map(|i| i % 3 == 0).collect();
            assert_eq!(unpack(&pack(&bits), len), Some(bits));
        }
        assert_eq!(unpack(&[0, 0], 8), None);
    }
}
