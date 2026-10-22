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

use crate::city::economy::{Buildings, CityRace, Farmers, Population};
use crate::city::production::{Production, ProductionItem};
use crate::city::City;
use crate::content::Content;
use crate::game::serial::Serial;
use crate::map::fog::Fog;
use crate::map::{FeatureId, Grid, Map, Position, TerrainId};
use crate::player::{Defeat, History, Offers, Owner, PlayerId, Tax, Treasury};
use crate::wizard::entity::{Banner, CapitalRace, Portrait};
use crate::wizard::fortress::{Banishment, Fortress, SummonAt};

use crate::rules::SpellId;
use crate::unit::{Experience, Hurt, MovesLeft, Order, Unit};

use crate::game::Game;
use crate::wizard::entity::Wizard;
use crate::wizard::magic::Retort;
use crate::wizard::sheet::{
    Books, Candidates, KnownSpells, PowerSplit, Research, Retorts, Skill, WizardMut,
};

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
    pub settings: crate::game::setting::GameSettings,
    /// Set once the game has ended.
    pub game_over: Option<crate::game::endgame::GameOver>,
    pub human: Option<u8>,
    /// Contact, relations, treaties and pending proposals between wizards.
    pub diplomacy: crate::diplomacy::DiplomacySave,
    /// The world events going on, with the turns each has run.
    pub events: crate::world_event::EventsSave,
    /// Override (cheat) commands used in this game.
    pub override_log: Vec<String>,
    /// The prefab items already given out (`Make_Item` gives each once a game).
    pub prefab_taken: Vec<String>,
    pub map: Option<MapSave>,
    pub fog: FogSave,
    pub players: Vec<PlayerSave>,
    pub units: Vec<UnitSave>,
    pub cities: Vec<CitySave>,
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
    pub nodes: Vec<crate::feature::node::Node>,
    /// Tower owners `(x, y, player)`.
    pub tower_owners: Vec<(u32, u32, PlayerId)>,
    /// Guards and treasure of towers, nodes and encounter sites.
    pub lairs: Vec<crate::feature::lair::Lair>,
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
    pub armory: crate::item::armory::Armory,
    /// The hero records.
    pub roster: crate::hero::roster::Roster,
    /// The offers waiting for an answer.
    pub offers: Vec<crate::player::offer::Offer>,
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
    /// `(spell, mana still to pay, creation number)`.
    pub casting: Option<(String, i32, u64)>,
    /// The item being made by Enchant Item or Create Artifact.
    pub crafting: Option<crate::item::Item>,
    /// The dead hero Resurrection is bringing back.
    pub raising: Option<usize>,
    /// `(spell, creation number)` of the lasting spells on the world.
    pub global: Vec<(String, u64)>,
    pub skill_left: i32,
}

impl Default for MagicSave {
    fn default() -> Self {
        let split = PowerSplit::default();
        Self {
            books: Vec::new(),
            retorts: Vec::new(),
            skill_points: 0,
            mana_pct: split.mana,
            research_pct: split.research,
            skill_pct: split.skill,
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

/// A spell of a wizard read from a save, made once the creation numbers are checked: a lasting
/// spell on the world, or the cast in progress with `(mana owed, item, hero)`.
struct PendingSpell {
    spell: SpellId,
    serial: u64,
    cast: Option<(i32, Option<crate::item::Item>, Option<usize>)>,
}

impl MagicSave {
    /// What the wizard's components hold (the default for a player without a wizard).
    fn of(wizard: Option<Wizard<'_>>, rules: &crate::rules::Rules) -> Self {
        let Some(w) = wizard else {
            return Self::default();
        };
        let name = |s: &SpellId| rules.spells.get(*s).id.clone();
        let split = w.power_split();
        Self {
            books: w.books().into_iter().collect(),
            retorts: w.retorts().into_iter().collect(),
            skill_points: w.skill_points(),
            mana_pct: split.mana,
            research_pct: split.research,
            skill_pct: split.skill,
            known: w.known().iter().map(name).collect(),
            researchable: w.researchable().iter().map(name).collect(),
            candidates: w.candidates().iter().map(name).collect(),
            researching: w.researching().as_ref().map(name),
            research_remaining: w.research_remaining(),
            casting: w.casting().and_then(|c| {
                let cast = w.cast_instance()?;
                let serial = *w.world().get::<&Serial>(cast).ok()?;
                Some((name(&c.spell), c.remaining, serial.0))
            }),
            crafting: w.crafting(),
            raising: w.raising(),
            global: w
                .global_instances()
                .iter()
                .map(|(s, serial)| (name(s), serial.0))
                .collect(),
            skill_left: w.skill_left(),
        }
    }

    /// Put the saved magic on the wizard's components.
    /// Returns the lasting spells on the world, which the caller makes once the creation
    /// numbers are checked.
    fn put(
        self,
        mut wizard: WizardMut<'_>,
        rules: &crate::rules::Rules,
    ) -> Result<Vec<PendingSpell>, LoadError> {
        let m = self;
        let id = |s: &String| {
            rules
                .spells
                .id_of(s)
                .ok_or_else(|| LoadError::UnknownId("spell", s.clone()))
        };
        let ids = |v: &[String]| v.iter().map(id).collect::<Result<Vec<_>, _>>();
        let mut pending = m
            .global
            .iter()
            .map(|(s, serial)| {
                id(s).map(|spell| PendingSpell {
                    spell,
                    serial: *serial,
                    cast: None,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        match m.casting {
            Some((s, owed, serial)) => pending.push(PendingSpell {
                spell: id(&s)?,
                serial,
                cast: Some((owed, m.crafting, m.raising)),
            }),
            None if m.crafting.is_some() || m.raising.is_some() => {
                return Err(LoadError::Inconsistent(
                    "an item or hero is waiting for a spell that is not being cast".into(),
                ));
            }
            None => {}
        }
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
        *wizard.get_mut::<Books>() = Books(m.books.into_iter().collect());
        *wizard.get_mut::<Retorts>() = Retorts(m.retorts.into_iter().collect());
        *wizard.get_mut::<Skill>() = Skill {
            points: m.skill_points,
            left: m.skill_left,
        };
        *wizard.get_mut::<PowerSplit>() = PowerSplit {
            mana: m.mana_pct,
            research: m.research_pct,
            skill: m.skill_pct,
        };
        *wizard.get_mut::<KnownSpells>() = KnownSpells {
            known,
            researchable,
        };
        *wizard.get_mut::<Candidates>() = Candidates(ids(&m.candidates)?);
        *wizard.get_mut::<Research>() = Research {
            spell: m.researching.as_ref().map(id).transpose()?,
            remaining: m.research_remaining,
        };
        Ok(pending)
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
    /// Weapon quality (`unit::WeaponQuality`).
    pub weapon: u8,
    pub moves: u32,
    pub order: Option<Vec<Position>>,
    /// `(spell, caster, creation number)`.
    pub enchantments: Vec<(String, u8, u64)>,
    /// Road work in progress: `(square, turns left, squares still to do)`.
    pub road_work: Option<(Position, u8, Vec<Position>)>,
    /// Creation number of the ship the unit is aboard.
    pub aboard: Option<u64>,
    /// The unit was made undead (`unit/undead.rs`).
    pub undead: bool,
    /// Done or Patrol (the main screen's buttons).
    pub standby: Option<crate::unit::Standby>,
    /// Held in Stasis: 0 no, 1 just put there, 2 lingering.
    pub stasis: u8,
    /// Temporary modifiers and abilities.
    pub effects: Vec<TempSave>,
    /// A hero's three items.
    pub items: Option<crate::item::HeroItems>,
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
    use crate::spell::temp_effects::TempKind;
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
    use crate::spell::temp_effects::{TempEffect, TempEffects, TempKind};
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
    /// `(spell, caster, creation number)`.
    pub enchantments: Vec<(String, u8, u64)>,
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
    pub autobuild: Option<crate::city::autobuild::AutoBuild>,
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
            .player_ids()
            .iter()
            .map(|id| PlayerSave {
                id: id.0,
                name: self.player_name(*id),
                gold: self.treasury(*id).map_or(0, |t| t.gold),
                mana: self.treasury(*id).map_or(0, |t| t.mana),
                tax_rate: self.player_component::<Tax>(*id).map_or(0, |t| t.0),
                grand_vizier: self.has_grand_vizier(*id),
                neutral: self.is_neutral_player(*id),
                history: self
                    .player_component::<History>(*id)
                    .map_or_else(Vec::new, |h| h.0.clone()),
                capital_race: self
                    .wizard(*id)
                    .and_then(|w| w.capital_race())
                    .map(|r| rules.races.get(r).id.clone()),
                fame: self.wizard(*id).map_or(0, |w| w.fame()),
                magic: MagicSave::of(self.wizard(*id), rules),
                armory: self
                    .player_component::<crate::item::armory::Armory>(*id)
                    .map(|a| crate::item::armory::Armory::clone(&a))
                    .unwrap_or_default(),
                roster: self
                    .player_component::<crate::hero::roster::Roster>(*id)
                    .map(|r| crate::hero::roster::Roster::clone(&r))
                    .unwrap_or_default(),
                offers: self.offers(*id),
                fortress: self.wizard(*id).and_then(|w| w.fortress()),
                summon_at: self.wizard(*id).and_then(|w| w.summon_at()),
                banished: self.wizard(*id).is_some_and(|w| w.is_banished()),
                banished_by: self.wizard(*id).and_then(|w| w.banished_by()).map(|d| d.0),
                defeated: self.is_defeated(*id),
                defeated_by: self.defeat_by(*id).map(|d| d.0),
                banner: Some(self.banner(*id)),
                portrait: Some(self.portrait(*id)),
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
                        .spells()
                        .unit_instances(u)
                        .into_iter()
                        .map(|(s, p, serial)| (rules.spells.get(s).id.clone(), p.0, serial.0))
                        .collect(),
                    aboard: self
                        .world
                        .get::<&crate::unit::transport::Aboard>(u)
                        .ok()
                        .map(|a| a.0 .0),
                    road_work: self
                        .world
                        .get::<&crate::map::road::RoadWork>(u)
                        .ok()
                        .map(|w| (w.at, w.left, w.rest.clone())),
                    effects: effects_save(self, u),
                    items: self
                        .world
                        .get::<&crate::item::HeroItems>(u)
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
                    marooned: self
                        .world
                        .get::<&crate::unit::transport::Marooned>(u)
                        .is_ok(),
                    stasis: match self
                        .world
                        .get::<&crate::unit::stasis::Stasis>(u)
                        .ok()
                        .map(|s| *s)
                    {
                        None => 0,
                        Some(crate::unit::stasis::Stasis::Fresh) => 1,
                        Some(crate::unit::stasis::Stasis::Lingering) => 2,
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
                    owner: self.economy().city_owner(e)?.0,
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
                        .spells()
                        .city_instances(e)
                        .into_iter()
                        .map(|(s, p, serial)| (rules.spells.get(s).id.clone(), p.0, serial.0))
                        .collect(),
                    wards: self.economy().city_wards(e),
                    item: production.item.map(item_save),
                    progress: production.progress,
                    effects: effects_save(self, e),
                    conquered: self
                        .world
                        .get::<&crate::city::conquest::Conquered>(e)
                        .ok()
                        .and_then(|c| crate::player::id_of(&self.world, c.0))
                        .map(|p| p.0),
                    sold_building: self.economy().sold_this_turn(e),
                    autobuild: self
                        .world
                        .get::<&crate::city::autobuild::AutoBuild>(e)
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
            human: self.human().map(|h| h.0),
            diplomacy: self.diplomacy_to_save(),
            events: self
                .events
                .to_save(|e| crate::player::id_of(&self.world, e)),
            override_log: self.override_log.clone(),
            prefab_taken: self.prefab_taken.clone(),
            map,
            fog,
            players,
            units,
            cities,
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
        game.override_log = self.override_log;
        game.prefab_taken = self.prefab_taken;
        game.next_serial = self.next_serial;
        // Resume the random stream where it stopped.
        game.rng = crate::game::rng::GameRng::resume(self.seed, self.draws);
        let mut pending = Vec::new();

        game.map = self
            .map
            .map(|m| restore_map(m, &game.content))
            .transpose()?;
        game.fog = restore_fog(self.fog, game.map.as_ref())?;
        for p in self.players {
            if game.player_entity(PlayerId(p.id)).is_some() {
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
            let id = PlayerId(p.id);
            let (name, banner, portrait) = (
                p.name,
                p.banner
                    .unwrap_or(if p.id == u8::MAX {
                        crate::player::NEUTRAL_BANNER
                    } else {
                        p.id % crate::player::BANNER_CHOICES
                    })
                    .min(crate::player::NEUTRAL_BANNER),
                p.portrait
                    .unwrap_or(p.id % crate::player::PORTRAITS)
                    .min(crate::player::PORTRAITS - 1),
            );
            let neutral = p.neutral;
            let (gold, mana, tax_rate, grand_vizier) = (p.gold, p.mana, p.tax_rate, p.grand_vizier);
            let (history, offers) = (p.history, p.offers);
            let (defeated, defeated_by) = (p.defeated, p.defeated_by.map(PlayerId));
            let (fame, magic) = (p.fame, p.magic);
            let (armory, roster) = (p.armory, p.roster);
            let (fortress, summon_at) = (p.fortress, p.summon_at);
            let banishment = p.banished.then(|| Banishment {
                by: p.banished_by.map(PlayerId),
            });
            let entity = if neutral {
                game.add_neutral_player(id)
            } else {
                game.add_player(id, name)
            };
            let _ = game.world.insert(
                entity,
                (
                    Treasury { gold, mana },
                    Tax(tax_rate),
                    History(history),
                    Offers(offers),
                    armory,
                    roster,
                ),
            );
            if !neutral {
                game.set_fame(id, fame);
                let rules = &game.content.rules;
                let entity = crate::wizard::entity::of_player(&game.world, entity)
                    .ok_or_else(|| LoadError::Inconsistent("a wizard without its entity".into()))?;
                let mut wizard = WizardMut::new(&mut game.world, entity);
                wizard.put(fortress.map(Fortress));
                wizard.put(summon_at.map(SummonAt));
                wizard.put(banishment);
                for spell in magic.put(wizard, rules)? {
                    pending.push((entity, spell));
                }
            }
            game.set_grand_vizier(id, grand_vizier);
            if defeated {
                let _ = game.world.insert_one(entity, Defeat { by: defeated_by });
            }
            game.wizard_set(id, Banner(banner));
            game.wizard_set(id, Portrait(portrait));
            game.wizard_set(id, CapitalRace(capital_race));
        }
        game.set_human(self.human.map(PlayerId));

        let mut serials = std::collections::BTreeSet::new();
        let mut take = |serial: u64, what: &str| -> Result<Serial, LoadError> {
            if serial == 0 || serial >= game.next_serial || !serials.insert(serial) {
                return Err(LoadError::Inconsistent(format!(
                    "{what} has a bad or repeated creation number {serial}"
                )));
            }
            Ok(Serial(serial))
        };
        for (caster, p) in pending {
            let serial = take(p.serial, "a spell of a wizard")?;
            let target = match p.cast {
                None => crate::spell::instance::SpellTarget::Global,
                Some(_) => crate::spell::instance::SpellTarget::Wizard(caster),
            };
            let instance = game.world.spawn((
                crate::spell::instance::SpellCast { def: p.spell },
                crate::spell::instance::Caster(caster),
                target,
                serial,
            ));
            if let Some((owed, crafting, raising)) = p.cast {
                let _ = game
                    .world
                    .insert_one(instance, crate::spell::instance::Owed(owed));
                if let Some(item) = crafting {
                    let _ = game
                        .world
                        .insert_one(instance, crate::spell::instance::Crafting(item));
                }
                if let Some(kind) = raising {
                    let _ = game
                        .world
                        .insert_one(instance, crate::spell::instance::Raising(kind));
                }
            }
        }
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
            let owner = game.player_entity(PlayerId(u.owner)).ok_or_else(|| {
                LoadError::Inconsistent(format!("a unit belongs to the missing player {}", u.owner))
            })?;
            let entity = game.world.spawn((
                Unit(id),
                Owner(owner),
                u.pos,
                Hurt(u.hurt),
                Experience(u.xp),
                MovesLeft(u.moves),
                serial,
            ));
            if u.weapon > 0 {
                let _ = game
                    .world
                    .insert_one(entity, crate::unit::WeaponQuality(u.weapon));
            }
            if let Some(ship) = u.aboard {
                let _ = game
                    .world
                    .insert_one(entity, crate::unit::transport::Aboard(Serial(ship)));
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
                let _ = game.world.insert_one(entity, crate::unit::undead::Undead);
            }
            if u.marooned {
                let _ = game
                    .world
                    .insert_one(entity, crate::unit::transport::Marooned);
            }
            match u.stasis {
                1 => {
                    let _ = game
                        .world
                        .insert_one(entity, crate::unit::stasis::Stasis::Fresh);
                }
                2 => {
                    let _ = game
                        .world
                        .insert_one(entity, crate::unit::stasis::Stasis::Lingering);
                }
                _ => {}
            }
            if let Some((at, left, rest)) = u.road_work {
                let _ = game
                    .world
                    .insert_one(entity, crate::map::road::RoadWork { at, left, rest });
            }
            if let Some(path) = u.order {
                let _ = game.world.insert_one(entity, Order::GoTo(path));
            }
            for (s, p, serial) in &u.enchantments {
                let def = game
                    .content
                    .rules
                    .spells
                    .id_of(s)
                    .ok_or_else(|| LoadError::UnknownId("spell", s.clone()))?;
                let serial = take(*serial, "a unit spell")?;
                let wizard = game
                    .wizard(PlayerId(*p))
                    .map(|w| w.entity())
                    .ok_or_else(|| {
                        LoadError::Inconsistent(format!(
                            "a spell on a unit cast by the missing player {p}"
                        ))
                    })?;
                game.world.spawn((
                    crate::spell::instance::SpellCast { def },
                    crate::spell::instance::Caster(wizard),
                    crate::spell::instance::SpellTarget::Unit(entity),
                    serial,
                ));
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
                .map(|(s, p, serial)| {
                    rules
                        .spells
                        .id_of(s)
                        .map(|id| (id, PlayerId(*p), *serial))
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
            let owner = game.player_entity(PlayerId(c.owner)).ok_or_else(|| {
                LoadError::Inconsistent(format!("a city belongs to the missing player {}", c.owner))
            })?;
            let entity = game.world.spawn((
                serial,
                City { name: c.name },
                Owner(owner),
                c.pos,
                Population {
                    figures: c.figures,
                    tens: c.tens,
                },
                Farmers(c.farmers),
                Buildings(buildings),
                Production {
                    item,
                    progress: c.progress,
                },
            ));
            if let Some(r) = race {
                let _ = game.world.insert_one(entity, CityRace(r));
            }
            for (spell, caster, serial) in enchantments {
                let serial = take(serial, "a city spell")?;
                let wizard = game.wizard(caster).map(|w| w.entity()).ok_or_else(|| {
                    LoadError::Inconsistent(format!(
                        "a spell on a city cast by the missing player {}",
                        caster.0
                    ))
                })?;
                game.world.spawn((
                    crate::spell::instance::SpellCast { def: spell },
                    crate::spell::instance::Caster(wizard),
                    crate::spell::instance::SpellTarget::City(entity),
                    serial,
                ));
            }
            effects_load(&mut game.world, entity, c.effects);
            if !c.wards.is_empty() {
                let _ = game
                    .world
                    .insert_one(entity, crate::spell::city::SpellWards(c.wards));
            }
            if c.sold_building {
                let _ = game
                    .world
                    .insert_one(entity, crate::city::production::SoldBuilding);
            }
            if let Some(a) = c.autobuild {
                let _ = game.world.insert_one(entity, a);
            }
            if let Some(by) = c.conquered.and_then(|p| game.player_entity(PlayerId(p))) {
                let _ = game
                    .world
                    .insert_one(entity, crate::city::conquest::Conquered(by));
            }
        }
        for id in game.player_ids() {
            let Some(w) = game.wizard(id) else {
                continue;
            };
            for pos in [w.fortress(), w.summon_at()].into_iter().flatten() {
                let ok = game
                    .economy()
                    .city_at(pos)
                    .is_some_and(|c| game.economy().city_owner(c) == Some(id));
                if !ok {
                    return Err(LoadError::Inconsistent(format!(
                        "player {} has a fortress or summoning circle where they have no city",
                        id.0
                    )));
                }
            }
            if w.is_banished() && w.fortress().is_some() {
                return Err(LoadError::Inconsistent(
                    "a banished wizard has a fortress".into(),
                ));
            }
        }
        game.events =
            crate::world_event::Events::from_save(&self.events, |p| game.player_entity(p))
                .ok_or_else(|| LoadError::Inconsistent("an unknown event".into()))?;
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
        if n.owner.is_some_and(|o| o.0 > 250) {
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
