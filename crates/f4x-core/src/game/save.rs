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

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::city::economy::{Buildings, CityRace, Farmers, Population};
use crate::city::production::{Production, ProductionItem};
use crate::city::City;
use crate::content::Content;
use crate::game::serial::Serial;
use crate::map::fog::Fog;
use crate::map::{FeatureId, Grid, Map, Position, TerrainId};
use crate::player::{Defeat, History, Offers, Owner, PlayerId, Tax, Treasury};
use crate::spell::instance::{Caster, Owed, SpellCast, SpellTarget};
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

/// Bumped when the snapshot layout changes in a way old files cannot be read. Version 2 saves each
/// kind of entity as its own list (`docs/ecs-structure.md` section 3.9).
pub const SAVE_VERSION: u32 = 2;

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
    /// The player entities, in the order of their ids.
    pub players: Vec<PlayerSave>,
    /// The wizard entities, one for each player that is no neutral player.
    pub wizards: Vec<WizardSave>,
    /// The game's heroes, one of each type.
    pub heroes: Vec<HeroSave>,
    /// The units, oldest first. A unit with the creation number of a hero is that hero's body.
    pub units: Vec<UnitSave>,
    pub cities: Vec<CitySave>,
    /// The towers, nodes, encounter sites and volcanoes, oldest first.
    pub features: Vec<FeatureSave>,
    /// The magic items, oldest first, with who holds each.
    pub items: Vec<MagicItemSave>,
    /// The spells that last or are being paid for, oldest first.
    pub spells: Vec<SpellSave>,
    /// The battle being fought, if the game was saved in one. The world above is the world the
    /// battle is fought in.
    #[serde(default)]
    pub battle: Option<crate::battle::save::ActiveBattleSave>,
}

/// An item entity: its creation number, what it is and who holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MagicItemSave {
    pub serial: u64,
    pub item: crate::item::Item,
    pub held: HeldSave,
}

/// Who holds an item, with players by id and a hero by the creation number of its entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeldSave {
    Hero(u64, u8),
    Vault(u8, u16),
    Award(u8),
    Crafting(u8),
    Offered(u8),
}

/// A terrain feature entity: its creation number, what it is, where it stands and what it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureSave {
    pub serial: u64,
    /// The id of the feature in the content; `None` for a volcano.
    pub feature: Option<String>,
    pub at: Position,
    pub node: Option<crate::feature::node::Node>,
    pub lair: Option<crate::feature::lair::Lair>,
    /// The player who owns it (a node, a tower or a volcano).
    pub owner: Option<u8>,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaneSave {
    /// Index into `MapSave::terrains` for every tile, row by row.
    pub tiles: Vec<u16>,
    /// `(tile index, index into MapSave::features)` for tiles that have a mineral.
    pub features: Vec<(u32, u16)>,
    /// Tile indexes that have a road.
    pub roads: Vec<u32>,
    /// Tile indexes that have a river.
    pub rivers: Vec<u32>,
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

/// A player entity: what it owns and does. Its wizard, if it has one, is a [`WizardSave`] with
/// the same id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSave {
    pub id: u8,
    pub gold: i32,
    pub mana: i32,
    pub tax_rate: u8,
    /// The Grand Vizier chooses what the cities build.
    pub grand_vizier: bool,
    /// An independent city's player: it has no wizard.
    pub neutral: bool,
    /// The wizard's power over the turns.
    pub history: Vec<u8>,
    /// The offers waiting for an answer.
    pub offers: Vec<crate::player::offer::Offer>,
    pub defeated: bool,
    pub defeated_by: Option<u8>,
}

/// A wizard entity: what the creation screen makes and what follows from it. Spells are saved by
/// id; the wizard's lasting spells and the cast in progress are [`SpellSave`]s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardSave {
    /// The id of the player the wizard plays as.
    pub player: u8,
    pub name: String,
    #[serde(default)]
    pub gender: crate::text::Gender,
    pub portrait: u8,
    pub banner: u8,
    pub capital_race: Option<String>,
    pub fame: i32,
    pub books: Vec<(String, u8)>,
    pub retorts: Vec<Retort>,
    pub skill_points: i32,
    pub skill_left: i32,
    pub mana_pct: u8,
    pub research_pct: u8,
    pub skill_pct: u8,
    pub known: Vec<String>,
    pub researchable: Vec<String>,
    /// The spells on offer for research.
    pub candidates: Vec<String>,
    pub researching: Option<String>,
    pub research_remaining: i32,
    pub fortress: Option<Position>,
    pub summon_at: Option<Position>,
    pub banished: bool,
    pub banished_by: Option<u8>,
}

/// A spell instance: a spell that lasts, or the cast a wizard is paying for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellSave {
    pub serial: u64,
    pub spell: String,
    /// The player of the wizard who cast it.
    pub caster: u8,
    pub target: SpellTargetSave,
    /// The mana still to pay, on a cast in progress (which is aimed at its own caster).
    pub owed: Option<i32>,
}

/// What a spell is on: a player id for a wizard, a creation number for the other entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpellTargetSave {
    Global,
    Wizard(u8),
    City(u64),
    Unit(u64),
    Feature(u64),
}

impl WizardSave {
    /// What the wizard's components hold.
    fn of(id: PlayerId, w: Wizard<'_>, rules: &crate::rules::Rules) -> Self {
        let name = |s: &SpellId| rules.spells.get(*s).id.clone();
        let split = w.power_split();
        Self {
            player: id.0,
            name: w.name(),
            gender: w.gender(),
            portrait: w.portrait(),
            banner: w.banner(),
            capital_race: w.capital_race().map(|r| rules.races.get(r).id.clone()),
            fame: w.fame(),
            books: w.books().into_iter().collect(),
            retorts: w.retorts().into_iter().collect(),
            skill_points: w.skill_points(),
            skill_left: w.skill_left(),
            mana_pct: split.mana,
            research_pct: split.research,
            skill_pct: split.skill,
            known: w.known().iter().map(name).collect(),
            researchable: w.researchable().iter().map(name).collect(),
            candidates: w.candidates().iter().map(name).collect(),
            researching: w.researching().as_ref().map(name),
            research_remaining: w.research_remaining(),
            fortress: w.fortress(),
            summon_at: w.summon_at(),
            banished: w.is_banished(),
            banished_by: w.banished_by().map(|d| d.0),
        }
    }

    /// Put the saved sheet on the wizard's components.
    fn put(self, mut wizard: WizardMut<'_>, rules: &crate::rules::Rules) -> Result<(), LoadError> {
        let id = |s: &String| {
            rules
                .spells
                .id_of(s)
                .ok_or_else(|| LoadError::UnknownId("spell", s.clone()))
        };
        let ids = |v: &[String]| v.iter().map(id).collect::<Result<Vec<_>, _>>();
        if u32::from(self.mana_pct) + u32::from(self.research_pct) + u32::from(self.skill_pct)
            != 100
        {
            return Err(LoadError::Inconsistent(
                "power split does not add up to 100".into(),
            ));
        }
        let known: std::collections::BTreeSet<SpellId> = ids(&self.known)?.into_iter().collect();
        let researchable: std::collections::BTreeSet<SpellId> =
            ids(&self.researchable)?.into_iter().collect();
        if known.intersection(&researchable).next().is_some() {
            return Err(LoadError::Inconsistent(
                "a spell is both known and researchable".into(),
            ));
        }
        wizard.put(self.fortress.map(Fortress));
        wizard.put(self.summon_at.map(SummonAt));
        wizard.put(self.banished.then(|| Banishment {
            by: self.banished_by.map(PlayerId),
        }));
        *wizard.get_mut::<Books>() = Books(self.books.into_iter().collect());
        *wizard.get_mut::<Retorts>() = Retorts(self.retorts.into_iter().collect());
        *wizard.get_mut::<Skill>() = Skill {
            points: self.skill_points,
            left: self.skill_left,
        };
        *wizard.get_mut::<PowerSplit>() = PowerSplit {
            mana: self.mana_pct,
            research: self.research_pct,
            skill: self.skill_pct,
        };
        *wizard.get_mut::<KnownSpells>() = KnownSpells {
            known,
            researchable,
        };
        *wizard.get_mut::<Candidates>() = Candidates(ids(&self.candidates)?);
        *wizard.get_mut::<Research>() = Research {
            spell: self.researching.as_ref().map(id).transpose()?,
            remaining: self.research_remaining,
        };
        Ok(())
    }
}

/// One of the game's heroes. A hero that serves also has a `UnitSave` with the same serial, which
/// says whose it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroSave {
    pub serial: u64,
    pub state: crate::hero::HeroState,
    pub hero: crate::hero::Hero,
    /// Experience kept while the hero is free or dead.
    pub xp: u32,
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
    pub order: Option<Order>,
    /// Road work in progress.
    pub road_work: Option<crate::map::road::RoadWork>,
    /// Creation number of the ship the unit is aboard.
    pub aboard: Option<u64>,
    /// The unit was made undead (`unit/undead.rs`).
    pub undead: bool,
    /// Done or Patrol (the main screen's buttons).
    pub standby: Option<crate::unit::Standby>,
    pub stasis: Option<crate::unit::stasis::Stasis>,
    /// Temporary modifiers and abilities.
    pub effects: Vec<crate::spell::temp_effects::TempEffect>,
    /// A ship let go with passengers aboard, sailing for the shore (`transport::Marooned`).
    pub marooned: bool,
}

fn effects_load(
    world: &mut hecs::World,
    e: hecs::Entity,
    list: Vec<crate::spell::temp_effects::TempEffect>,
) {
    if !list.is_empty() {
        let _ = world.insert_one(e, crate::spell::temp_effects::TempEffects(list));
    }
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
    /// The realms a Spell Ward guards the city against.
    pub wards: Vec<String>,
    pub item: Option<ItemSave>,
    pub progress: u32,
    /// Temporary modifiers.
    pub effects: Vec<crate::spell::temp_effects::TempEffect>,
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
    /// The file was written with a different save layout.
    Version { found: u32, supported: u32 },
    /// The file was written by a different version of the game.
    GameVersion {
        found: String,
        supported: &'static str,
    },
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
            LoadError::GameVersion { found, supported } => {
                write!(f, "saved by {found}, this is {supported}")
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
    /// The item entities, oldest first.
    fn item_saves(&self) -> Vec<MagicItemSave> {
        use crate::item::entity::HeldBy;
        let mut items: Vec<MagicItemSave> = self
            .world
            .query::<(&crate::item::Item, &HeldBy, &Serial)>()
            .iter()
            .map(|(_, (item, by, serial))| MagicItemSave {
                serial: serial.0,
                item: item.clone(),
                held: match *by {
                    HeldBy::Hero(unit, slot) => HeldSave::Hero(self.serial_of(unit).0, slot),
                    HeldBy::Vault(p, slot) => HeldSave::Vault(p.0, slot),
                    HeldBy::Award(p) => HeldSave::Award(p.0),
                    HeldBy::Crafting(p) => HeldSave::Crafting(p.0),
                    HeldBy::Offered(p) => HeldSave::Offered(p.0),
                },
            })
            .collect();
        items.sort_by_key(|i| i.serial);
        items
    }

    /// The spell instances, oldest first.
    fn spell_saves(&self) -> Vec<SpellSave> {
        let spells = self.spells();
        spells
            .all()
            .into_iter()
            .filter_map(|i| {
                let target = match spells.target(i)? {
                    SpellTarget::Global => SpellTargetSave::Global,
                    SpellTarget::Wizard(w) => SpellTargetSave::Wizard(
                        crate::wizard::entity::player_id_of(&self.world, w)?.0,
                    ),
                    SpellTarget::City(e) => SpellTargetSave::City(self.serial_of(e).0),
                    SpellTarget::Unit(e) => SpellTargetSave::Unit(self.serial_of(e).0),
                    SpellTarget::Feature(e) => SpellTargetSave::Feature(self.serial_of(e).0),
                    SpellTarget::Battlefield => return None,
                };
                Some(SpellSave {
                    serial: self.serial_of(i).0,
                    spell: self.content.rules.spells.get(spells.def(i)?).id.clone(),
                    caster: spells.caster_player(i)?.0,
                    target,
                    owed: self.world.get::<&Owed>(i).ok().map(|o| o.0),
                })
            })
            .collect()
    }

    /// The feature entities, oldest first.
    fn feature_saves(&self) -> Vec<FeatureSave> {
        let eco = self.economy();
        let mut found: Vec<(Serial, hecs::Entity)> = self
            .world
            .query::<&Serial>()
            .iter()
            .filter(|(e, _)| {
                self.world
                    .satisfies::<&crate::feature::entity::TerrainFeature>(*e)
                    .unwrap_or(false)
                    || self
                        .world
                        .satisfies::<&crate::feature::entity::Volcano>(*e)
                        .unwrap_or(false)
            })
            .map(|(e, s)| (*s, e))
            .collect();
        found.sort_by_key(|&(s, e)| (s, e.id()));
        found
            .into_iter()
            .filter_map(|(serial, e)| {
                let at = *self.world.get::<&Position>(e).ok()?;
                let feature = match self.world.get::<&crate::feature::entity::TerrainFeature>(e) {
                    Ok(f) => Some(self.content.features.get(f.0)?.id.clone()),
                    Err(_) => None,
                };
                let owner = self
                    .world
                    .get::<&crate::player::Owner>(e)
                    .ok()
                    .and_then(|o| crate::player::id_of(&self.world, o.0))
                    .map(|p| p.0);
                let node = self
                    .world
                    .get::<&crate::feature::node::Node>(e)
                    .ok()
                    .map(|n| (*n).clone());
                let lair = self
                    .world
                    .get::<&crate::feature::lair::Lair>(e)
                    .ok()
                    .map(|l| (*l).clone());
                let _ = &eco;
                Some(FeatureSave {
                    serial: serial.0,
                    feature,
                    at,
                    node,
                    lair,
                    owner,
                })
            })
            .collect()
    }

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
                        .minerals()
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
                gold: self.player(*id).map_or(0, |p| p.treasury().gold),
                mana: self.player(*id).map_or(0, |p| p.treasury().mana),
                tax_rate: self.player_component::<Tax>(*id).map_or(0, |t| t.0),
                grand_vizier: self.player(*id).is_some_and(|p| p.has_grand_vizier()),
                neutral: self.is_neutral_player(*id),
                history: self
                    .player_component::<History>(*id)
                    .map_or_else(Vec::new, |h| h.0.clone()),
                offers: self.offers(*id),
                defeated: self.player(*id).is_some_and(|p| p.is_defeated()),
                defeated_by: self.player(*id).and_then(|p| p.defeated_by()).map(|d| d.0),
            })
            .collect();
        let wizards = self
            .player_ids()
            .iter()
            .filter_map(|id| Some(WizardSave::of(*id, self.wizard(*id)?, rules)))
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
                    order: self.world.get::<&Order>(u).ok().map(|o| (*o).clone()),
                    aboard: self
                        .world
                        .get::<&crate::unit::transport::Aboard>(u)
                        .ok()
                        .map(|a| a.0 .0),
                    road_work: self
                        .world
                        .get::<&crate::map::road::RoadWork>(u)
                        .ok()
                        .map(|w| (*w).clone()),
                    effects: self.units().temp_effects(u),
                    undead: self.units().unit_is_undead(u),
                    standby: self.units().unit_standby(u),
                    marooned: self
                        .world
                        .get::<&crate::unit::transport::Marooned>(u)
                        .is_ok(),
                    stasis: self
                        .world
                        .get::<&crate::unit::stasis::Stasis>(u)
                        .ok()
                        .map(|s| *s),
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
                    wards: self.economy().city_wards(e),
                    item: production.item.map(item_save),
                    progress: production.progress,
                    effects: self.units().temp_effects(e),
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
            wizards,
            items: self.item_saves(),
            features: self.feature_saves(),
            spells: self.spell_saves(),
            heroes: self
                .units()
                .all_heroes()
                .into_iter()
                .filter_map(|h| {
                    Some(HeroSave {
                        serial: self.serial_of(h).0,
                        state: self.units().hero_state(h)?,
                        hero: (*self.world.get::<&crate::hero::Hero>(h).ok()?).clone(),
                        xp: self.units().hero_xp(h),
                    })
                })
                .collect(),
            units,
            cities,
            battle: self.battle_save(),
        }
    }
}

/// Just the version fields of a save, read before the rest so that a file of another layout is
/// refused for its version and not for whatever it fails to parse as. The structs carry the names
/// of the ones they stand for, which RON checks.
mod header {
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct Snapshot {
        pub version: u32,
        #[serde(default)]
        pub game_version: String,
    }

    #[derive(Deserialize)]
    pub struct Replay {
        pub version: u32,
        pub start: Snapshot,
    }
}

/// Whether a save is as the game wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Integrity {
    Intact,
    /// The text differs from what the checksum in its first line was made from: someone edited it.
    Edited,
    /// The first line carries no checksum.
    Unsigned,
}

/// A 64-bit FNV-1a hash of the text. It catches an edit, not a forger: the game does not stop
/// anyone from editing a save, it only says so.
fn checksum(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Refuse a save or replay made by another save layout or another version of the game.
fn check_versions(version: u32, game_version: &str) -> Result<(), LoadError> {
    if version != SAVE_VERSION {
        return Err(LoadError::Version {
            found: version,
            supported: SAVE_VERSION,
        });
    }
    match game_version {
        GAME_VERSION => Ok(()),
        "" => Err(LoadError::GameVersion {
            found: "an unknown version".into(),
            supported: GAME_VERSION,
        }),
        other => Err(LoadError::GameVersion {
            found: other.to_string(),
            supported: GAME_VERSION,
        }),
    }
}

pub(crate) fn check_snapshot_text(text: &str) -> Result<(), LoadError> {
    let h: header::Snapshot = ron::from_str(text).map_err(|e| LoadError::Parse(e.to_string()))?;
    check_versions(h.version, &h.game_version)
}

pub(crate) fn check_replay_text(text: &str) -> Result<(), LoadError> {
    let h: header::Replay = ron::from_str(text).map_err(|e| LoadError::Parse(e.to_string()))?;
    check_versions(h.version, &h.start.game_version)
}

impl Snapshot {
    /// The snapshot as readable text. The first line names the versions and carries the checksum
    /// of the rest.
    pub fn to_text(&self) -> Result<String, LoadError> {
        let body =
            ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default().depth_limit(8))
                .map_err(|e| LoadError::Parse(e.to_string()))?;
        let body = format!("{body}\n");
        Ok(format!(
            "// f4x save, version {SAVE_VERSION}, game {GAME_VERSION}, checksum {:016x}\n{body}",
            checksum(&body)
        ))
    }

    /// Whether `text` is as `to_text` wrote it. A save that was edited still loads; the caller
    /// says so.
    pub fn integrity(text: &str) -> Integrity {
        let Some((first, body)) = text.split_once('\n') else {
            return Integrity::Unsigned;
        };
        match first.rsplit_once("checksum ") {
            Some((_, sum)) if sum.trim() == format!("{:016x}", checksum(body)) => Integrity::Intact,
            Some(_) => Integrity::Edited,
            None => Integrity::Unsigned,
        }
    }

    pub fn from_text(text: &str) -> Result<Self, LoadError> {
        check_snapshot_text(text)?;
        ron::from_str(text).map_err(|e| LoadError::Parse(e.to_string()))
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

        game.map = self
            .map
            .map(|m| restore_map(m, &game.content))
            .transpose()?;
        game.fog = restore_fog(self.fog, game.map.as_ref())?;
        let mut wizards: std::collections::BTreeMap<u8, WizardSave> = BTreeMap::new();
        for w in self.wizards {
            let id = w.player;
            if wizards.insert(id, w).is_some() {
                return Err(LoadError::Inconsistent(format!(
                    "player {id} has two wizards"
                )));
            }
        }
        for p in self.players {
            let id = PlayerId(p.id);
            if game.player_entity(id).is_some() {
                return Err(LoadError::Inconsistent(format!(
                    "player {} appears twice",
                    p.id
                )));
            }
            let wizard = wizards.remove(&p.id);
            let entity = match (&wizard, p.neutral) {
                (None, true) => game.add_neutral_player(id),
                (Some(w), false) => game.add_player(id, w.name.clone()),
                (Some(_), true) => {
                    return Err(LoadError::Inconsistent(format!(
                        "neutral player {} has a wizard",
                        p.id
                    )))
                }
                (None, false) => {
                    return Err(LoadError::Inconsistent(format!(
                        "player {} has no wizard",
                        p.id
                    )))
                }
            };
            let _ = game.world.insert(
                entity,
                (
                    Treasury {
                        gold: p.gold,
                        mana: p.mana,
                    },
                    Tax(p.tax_rate),
                    History(p.history),
                    Offers(p.offers),
                ),
            );
            if let Some(w) = wizard {
                let capital_race = w
                    .capital_race
                    .as_ref()
                    .map(|r| {
                        game.content
                            .rules
                            .races
                            .id_of(r)
                            .ok_or_else(|| LoadError::UnknownId("race", r.clone()))
                    })
                    .transpose()?;
                let (fame, banner, portrait, gender) = (
                    w.fame,
                    w.banner.min(crate::player::NEUTRAL_BANNER),
                    w.portrait.min(crate::player::PORTRAITS - 1),
                    w.gender,
                );
                game.set_fame(id, fame);
                let rules = &game.content.rules;
                let entity = crate::wizard::entity::of_player(&game.world, entity)
                    .ok_or_else(|| LoadError::Inconsistent("a wizard without its entity".into()))?;
                w.put(WizardMut::new(&mut game.world, entity), rules)?;
                game.wizard_set(id, Banner(banner));
                game.wizard_set(id, Portrait(portrait));
                game.wizard_set(id, gender);
                game.wizard_set(id, CapitalRace(capital_race));
            }
            game.set_grand_vizier(id, p.grand_vizier);
            if p.defeated {
                let _ = game.world.insert_one(
                    entity,
                    Defeat {
                        by: p.defeated_by.map(PlayerId),
                    },
                );
            }
        }
        if let Some(id) = wizards.keys().next() {
            return Err(LoadError::Inconsistent(format!(
                "a wizard plays as the missing player {id}"
            )));
        }
        game.set_human(self.human.map(PlayerId));

        let mut serials = std::collections::BTreeSet::new();
        let next_serial = game.next_serial;
        let mut take = |serial: u64, what: &str| -> Result<Serial, LoadError> {
            if serial == 0 || serial >= next_serial || !serials.insert(serial) {
                return Err(LoadError::Inconsistent(format!(
                    "{what} has a bad or repeated creation number {serial}"
                )));
            }
            Ok(Serial(serial))
        };
        for f in self.features {
            let serial = take(f.serial, "a feature")?;
            if game
                .map
                .as_ref()
                .is_none_or(|m| f.at.x >= m.width() || f.at.y >= m.height())
            {
                return Err(LoadError::Inconsistent("a feature is off the map".into()));
            }
            if let Some(n) = &f.node {
                let m = game.map.as_ref().expect("checked above");
                if n.aura
                    .iter()
                    .any(|&(x, y)| x >= m.width() || y >= m.height())
                {
                    return Err(LoadError::Inconsistent("a node is off the map".into()));
                }
            }
            let entity = match &f.feature {
                Some(id) => {
                    let feature = game
                        .content
                        .features
                        .id_of(id)
                        .ok_or(LoadError::UnknownId("feature", id.clone()))?;
                    game.spawn_feature_with(
                        serial,
                        crate::feature::entity::Placement {
                            at: f.at,
                            feature,
                            node: f.node,
                            lair: f.lair,
                        },
                    )
                }
                None => {
                    let owner = f
                        .owner
                        .map(PlayerId)
                        .ok_or_else(|| LoadError::Inconsistent("a volcano has no owner".into()))?;
                    game.spawn_volcano_with(serial, owner, f.at)
                        .ok_or_else(|| {
                            LoadError::Inconsistent("a volcano belongs to a missing player".into())
                        })?
                }
            };
            if let (Some(owner), true) = (f.owner, f.feature.is_some()) {
                if game.player_entity(PlayerId(owner)).is_none() {
                    return Err(LoadError::Inconsistent("a feature has a bad owner".into()));
                }
                game.set_feature_owner(entity, PlayerId(owner));
            }
        }
        for h in self.heroes {
            let serial = take(h.serial, "a hero")?;
            if h.hero.kind >= game.content.rules.hero_templates.len()
                || game.units().hero_entity(h.hero.kind).is_some()
            {
                return Err(LoadError::Inconsistent(format!(
                    "a hero of type {} is missing or twice",
                    h.hero.kind
                )));
            }
            game.make_hero_with(serial, h.hero, h.state, h.xp);
        }
        let in_map = |game: &Game, pos: Position| -> bool {
            game.map
                .as_ref()
                .is_none_or(|m| pos.x < m.width() && pos.y < m.height())
        };
        let mut units = self.units;
        units.sort_by_key(|u| u.serial);
        for u in units {
            // A hero's body is its own entity already, made with its serial.
            let hero_entity = game
                .units()
                .all_heroes()
                .into_iter()
                .find(|&h| game.serial_of(h).0 == u.serial);
            let serial = match hero_entity {
                Some(_) => Serial(u.serial),
                None => take(u.serial, "a unit")?,
            };
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
            let entity = match hero_entity {
                Some(hero) => {
                    let _ = game.world.insert(
                        hero,
                        (
                            Owner(owner),
                            Unit(id),
                            u.pos,
                            Hurt(u.hurt),
                            Experience(u.xp),
                            MovesLeft(u.moves),
                        ),
                    );
                    hero
                }
                None => game.world.spawn((
                    Unit(id),
                    Owner(owner),
                    u.pos,
                    Hurt(u.hurt),
                    Experience(u.xp),
                    MovesLeft(u.moves),
                    serial,
                )),
            };
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
            if let Some(stasis) = u.stasis {
                let _ = game.world.insert_one(entity, stasis);
            }
            if let Some(work) = u.road_work {
                let _ = game.world.insert_one(entity, work);
            }
            if let Some(order) = u.order {
                let _ = game.world.insert_one(entity, order);
            }
        }
        // A hero serves exactly when it has its body.
        for hero in game.units().all_heroes() {
            let serving = game.units().hero_state(hero) == Some(crate::hero::HeroState::Serving);
            if serving != game.world.get::<&Unit>(hero).is_ok() {
                return Err(LoadError::Inconsistent(
                    "a hero's state and its unit do not agree".into(),
                ));
            }
        }
        let mut items = self.items;
        items.sort_by_key(|i| i.serial);
        for i in items {
            use crate::item::entity::HeldBy;
            let serial = take(i.serial, "an item")?;
            let player = |p: u8| -> Result<PlayerId, LoadError> {
                game.player_entity(PlayerId(p))
                    .map(|_| PlayerId(p))
                    .ok_or_else(|| {
                        LoadError::Inconsistent(format!(
                            "an item belongs to the missing player {p}"
                        ))
                    })
            };
            let held = match i.held {
                HeldSave::Hero(unit, slot) => {
                    let found = game
                        .world
                        .query::<&Serial>()
                        .iter()
                        .find(|(_, s)| s.0 == unit)
                        .map(|(e, _)| e);
                    let hero = found
                        .filter(|&e| game.world.get::<&Unit>(e).is_ok())
                        .filter(|_| slot < 3)
                        .ok_or_else(|| {
                            LoadError::Inconsistent(
                                "an item is held by something that is no unit".into(),
                            )
                        })?;
                    HeldBy::Hero(hero, slot)
                }
                HeldSave::Vault(p, slot) => HeldBy::Vault(player(p)?, slot),
                HeldSave::Award(p) => HeldBy::Award(player(p)?),
                HeldSave::Crafting(p) => HeldBy::Crafting(player(p)?),
                HeldSave::Offered(p) => HeldBy::Offered(player(p)?),
            };
            game.spawn_item_with(serial, i.item, held);
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
        let by_serial: std::collections::HashMap<u64, hecs::Entity> = game
            .world
            .query::<&Serial>()
            .iter()
            .map(|(e, s)| (s.0, e))
            .collect();
        let mut owing = std::collections::BTreeSet::new();
        let mut spells = self.spells;
        spells.sort_by_key(|i| i.serial);
        for i in spells {
            let serial = take(i.serial, "a spell")?;
            let def = game
                .content
                .rules
                .spells
                .id_of(&i.spell)
                .ok_or_else(|| LoadError::UnknownId("spell", i.spell.clone()))?;
            let caster = game
                .wizard(PlayerId(i.caster))
                .map(|w| w.entity())
                .ok_or_else(|| {
                    LoadError::Inconsistent(format!(
                        "a spell cast by the missing player {}",
                        i.caster
                    ))
                })?;
            let on = |serial: u64, what: &str, has: &dyn Fn(hecs::Entity) -> bool| {
                by_serial
                    .get(&serial)
                    .copied()
                    .filter(|&e| has(e))
                    .ok_or_else(|| {
                        LoadError::Inconsistent(format!("a spell is on {what} that is not there"))
                    })
            };
            let has =
                |e: hecs::Entity, c: &dyn Fn(&hecs::World, hecs::Entity) -> bool| c(&game.world, e);
            let target = match i.target {
                SpellTargetSave::Global => SpellTarget::Global,
                SpellTargetSave::Wizard(p) => {
                    SpellTarget::Wizard(game.wizard(PlayerId(p)).map(|w| w.entity()).ok_or_else(
                        || {
                            LoadError::Inconsistent(
                                "a spell is on a wizard who is not there".into(),
                            )
                        },
                    )?)
                }
                SpellTargetSave::City(n) => SpellTarget::City(on(n, "a city", &|e| {
                    has(e, &|w, e| w.satisfies::<&City>(e).unwrap_or(false))
                })?),
                SpellTargetSave::Unit(n) => SpellTarget::Unit(on(n, "a unit", &|e| {
                    has(e, &|w, e| w.satisfies::<&Unit>(e).unwrap_or(false))
                })?),
                SpellTargetSave::Feature(n) => SpellTarget::Feature(on(n, "a feature", &|e| {
                    has(e, &|w, e| {
                        w.satisfies::<&Position>(e).unwrap_or(false)
                            && (w
                                .satisfies::<&crate::feature::entity::TerrainFeature>(e)
                                .unwrap_or(false)
                                || w.satisfies::<&crate::feature::entity::Volcano>(e)
                                    .unwrap_or(false))
                    })
                })?),
            };
            let instance = game
                .world
                .spawn((SpellCast { def }, Caster(caster), target, serial));
            if let Some(owed) = i.owed {
                if target != SpellTarget::Wizard(caster) || !owing.insert(caster) {
                    return Err(LoadError::Inconsistent(
                        "a cast in progress is not the one of its own wizard".into(),
                    ));
                }
                let _ = game.world.insert_one(instance, Owed(owed));
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
        if let Some(battle) = self.battle {
            game.restore_battle(battle)?;
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
                .with_enchanted_roads(enchanted)
                .with_lakes(lakes)
                .with_corruption(corrupted),
        );
    }
    let myrror = grids.pop().expect("two planes");
    let arcanus = grids.pop().expect("two planes");
    Ok(Map::new(arcanus, myrror))
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
