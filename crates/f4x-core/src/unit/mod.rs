//! Unit instances: components on map entities, and stats computed from their
//! definition, abilities and experience level through the modifier engine.
//!
//! The rules of units are in [`UnitView`] (task 185): a read handle that borrows only the world,
//! the content and the players (for the owner's retorts and global spells), with the queries as
//! its methods. They are spread over the files of the unit rules (`spell/temp_effect.rs`,
//! `unit/undead.rs`, `unit/experience.rs`, `spell/global_enchantment.rs`), each an `impl UnitView` block. What changes units
//! is a function over `&mut World` (`spawn`, `experience::gain_xp`, the temporary effects).
//! `Game` keeps thin methods with the old names that make a handle and ask it (`Game::units`).

pub mod experience;
pub(crate) mod healing;
pub mod movement;
pub mod mutation;
pub mod pathing;
pub mod stasis;
pub mod transport;
pub mod undead;

use hecs::World;

use crate::content::Content;
use crate::game::serial::Serial;
use crate::map::Position;
use crate::player::{Owner, PlayerId};

use crate::game::{Event, Game};
use crate::rules::stats::{compute, passive_modifiers, Modifier, Source};
use crate::rules::{LevelPathDef, Stat, UnitDef, UnitId, UnitKind};
use crate::wizard::magic::Retort;

/// Marks an entity as a unit of this definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unit(pub UnitId);

/// Hit points lost, summed over all figures of the unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Hurt(pub u32);

/// On a unit its owner pays no upkeep for, neither gold nor food: the troops a neutral city
/// raises when it goes raiding (`threat/raider.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unpaid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Experience(pub u32);

/// The quality of a unit's weapons, set where it is trained (`Game::city_weapon_quality`):
/// 0 normal, 1 magic (an alchemists guild or the Alchemy retort), 2 mithril, 3 adamantium.
/// Mithril and adamantium add to the unit's attacks and defence in battle; every quality above
/// normal adds a plus to hit and makes the weapons magical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WeaponQuality(pub u8);

/// The level Heroism lifts a unit to (the original's level 3: Elite, or a hero's Commander).
pub(crate) const HEROISM_LEVEL: usize = 3;
/// The spell that does it.
pub(crate) const HEROISM: &str = "heroism";

pub const WEAPON_MAGIC: u8 = 1;
pub const WEAPON_MITHRIL: u8 = 2;
pub const WEAPON_ADAMANTIUM: u8 = 3;

/// Movement left this turn, in half-moves (a terrain cost of 1 is 2 half-moves).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MovesLeft(pub u32);

/// A standing order carried out at the start of each turn.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Order {
    /// Walk this path (the first entry is the next tile).
    GoTo(Vec<Position>),
}

/// What a player has told an idle unit to do about being asked for orders (the main screen's
/// Done and Patrol buttons): `Done` skips it for this turn, `Patrol` until it moves again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Standby {
    Done,
    Patrol,
}

impl UnitView<'_> {
    /// The unit's standby, if it has one.
    pub fn unit_standby(&self, unit: hecs::Entity) -> Option<Standby> {
        self.world.get::<&Standby>(unit).ok().map(|s| *s)
    }
}

/// Most units that may share one tile.
pub const MAX_STACK: usize = 9;

/// The unit rules' view of the game: the world, the content and the players.
#[derive(Clone, Copy)]
pub struct UnitView<'a> {
    pub(crate) world: &'a World,
    pub(crate) content: &'a Content,
}

impl<'a> UnitView<'a> {
    pub fn new(world: &'a World, content: &'a Content) -> Self {
        UnitView { world, content }
    }

    /// A player's magic (retorts and global spells), for what the owner adds to its units.
    pub(crate) fn wizard(&self, player: PlayerId) -> Option<crate::wizard::entity::Wizard<'a>> {
        crate::wizard::entity::find(self.world, player)
    }

    /// Does `player` keep the global spell with this id going?
    pub(crate) fn has_global(&self, player: PlayerId, id: &str) -> bool {
        self.wizard(player).is_some_and(|m| {
            crate::spell::global_enchantment::magic_has_global(&m, self.content, id)
        })
    }

    /// A wizard's fame as the rules count it: what they have earned, what their Legendary heroes
    /// add, and ten more while Just Cause is kept going.
    pub fn fame(&self, player: PlayerId) -> i32 {
        let earned = self.wizard(player).map_or(0, |w| w.fame());
        let legendary: i32 = self
            .hero_units(player)
            .into_iter()
            .filter_map(|h| {
                let tier = self
                    .world
                    .get::<&crate::hero::Hero>(h)
                    .ok()?
                    .abilities
                    .legendary;
                Some(crate::hero::legendary(tier, self.unit_level(h)))
            })
            .sum();
        earned
            + legendary
            + if self.has_global(player, "just_cause") {
                crate::spell::global_enchantment::JUST_CAUSE_FAME
            } else {
                0
            }
    }

    /// Research points the wizard's Sage heroes add.
    pub fn sage_research(&self, player: PlayerId) -> i32 {
        self.hero_units(player)
            .into_iter()
            .filter_map(|h| {
                let tier = self.world.get::<&crate::hero::Hero>(h).ok()?.abilities.sage;
                Some(crate::hero::sage(tier, self.unit_level(h)))
            })
            .sum()
    }

    /// The entity with this creation number, if it is still alive.
    pub fn entity_of_serial(&self, serial: Serial) -> Option<hecs::Entity> {
        self.world
            .query::<&Serial>()
            .iter()
            .find(|(_, s)| **s == serial)
            .map(|(e, _)| e)
    }

    /// Does any wizard keep the global spell with this id going?
    pub(crate) fn any_global(&self, id: &str) -> bool {
        crate::player::ids(self.world)
            .into_iter()
            .any(|p| self.has_global(p, id))
    }

    /// Planar Seal closes every junction between the planes while anyone keeps it up.
    pub fn planar_seal_active(&self) -> bool {
        self.any_global("planar_seal")
    }

    /// An entity's creation number (entities without one sort first, by id).
    pub(crate) fn serial_of(&self, entity: hecs::Entity) -> Serial {
        self.world.get::<&Serial>(entity).map_or(Serial(0), |s| *s)
    }

    /// Sort entities oldest first.
    pub(crate) fn sort_by_serial(&self, entities: &mut [hecs::Entity]) {
        entities.sort_by_cached_key(|&e| (self.serial_of(e), e.id()));
    }

    /// The kind of unit this is, as the rules know it.
    pub fn unit_kind_id(&self, unit: hecs::Entity) -> Option<UnitId> {
        self.world.get::<&Unit>(unit).ok().map(|u| u.0)
    }

    pub fn unit_def(&self, unit: hecs::Entity) -> Option<&'a UnitDef> {
        let id = self.world.get::<&Unit>(unit).ok()?.0;
        Some(self.content.rules.units.get(id))
    }

    /// The quality of the unit's weapons (see [`WeaponQuality`]).
    pub fn unit_weapon_quality(&self, unit: hecs::Entity) -> u8 {
        self.world.get::<&WeaponQuality>(unit).map_or(0, |w| w.0)
    }

    pub fn unit_owner(&self, unit: hecs::Entity) -> Option<PlayerId> {
        crate::player::id_of(self.world, self.unit_owner_entity(unit)?)
    }

    /// The player entity a unit belongs to.
    pub fn unit_owner_entity(&self, unit: hecs::Entity) -> Option<hecs::Entity> {
        self.world.get::<&Owner>(unit).ok().map(|o| o.0)
    }

    pub fn unit_position(&self, unit: hecs::Entity) -> Option<Position> {
        self.world.get::<&Position>(unit).ok().map(|p| *p)
    }

    pub fn unit_has_ability(&self, unit: hecs::Entity, ability: &str) -> bool {
        self.unit_def(unit).is_some() && self.unit_abilities(unit).iter().any(|a| a == ability)
    }

    pub(crate) fn unit_level_path(&self, unit: hecs::Entity) -> Option<&'a LevelPathDef> {
        self.unit_def(unit)?
            .level_path
            .as_deref()
            .and_then(|p| self.content.rules.level_paths.by_name(p))
    }

    /// The level the unit's experience points alone give, 0-based (0 without a level path).
    pub fn unit_xp_level(&self, unit: hecs::Entity) -> usize {
        let Some(path) = self.unit_level_path(unit) else {
            return 0;
        };
        let xp = self.world.get::<&Experience>(unit).map_or(0, |x| x.0);
        path.levels.iter().rposition(|l| l.xp <= xp).unwrap_or(0)
    }

    /// The lasting spells on a unit, with the wizard who cast each.
    pub fn unit_enchantments(&self, unit: hecs::Entity) -> Vec<(crate::rules::SpellId, PlayerId)> {
        crate::spell::instance::Spells::new(self.world).unit_enchantments(unit)
    }

    /// Whether the Heroism spell is on the unit.
    pub fn unit_has_heroism(&self, unit: hecs::Entity) -> bool {
        let Some(spell) = self.content.rules.spells.id_of(HEROISM) else {
            return false;
        };
        crate::spell::instance::Spells::new(self.world).unit_has(unit, spell)
    }

    /// Experience level, 0-based, from the unit's level path (0 without one): the level its
    /// experience gives, raised to Elite by Heroism, then by one each for its owner's Warlord
    /// retort and Crusade spell (which leave the undead and creatures alone). Battles use the
    /// same level as the map (the original's battle code ignores Warlord, Crusade and xp when
    /// Heroism is on: B9 in `help-vs-implementation-conflicts.md`, we follow the help).
    pub fn unit_level(&self, unit: hecs::Entity) -> usize {
        self.level_with(unit, self.unit_has_heroism(unit))
    }

    /// The level the unit has with Heroism on it or without.
    fn level_with(&self, unit: hecs::Entity, heroism: bool) -> usize {
        let Some(path) = self.unit_level_path(unit) else {
            return 0;
        };
        let mut level = self.unit_xp_level(unit);
        if level < HEROISM_LEVEL && heroism {
            level = HEROISM_LEVEL;
        }
        if let Some(owner) = self.unit_owner(unit) {
            if self.wizard(owner).is_some_and(|m| m.has(Retort::Warlord)) {
                level += 1;
            }
            let normal = self.unit_def(unit).is_some_and(|d| {
                matches!(
                    d.kind,
                    crate::rules::UnitKind::Normal | crate::rules::UnitKind::Hero
                )
            });
            if normal && !self.unit_is_undead(unit) && self.has_global(owner, "crusade") {
                level += 1;
            }
        }
        level.min(path.level_count().saturating_sub(1))
    }

    /// What Heroism adds to a unit that has not got it yet: the bonuses of the levels it would
    /// then reach (a battle unit is made once, so a Heroism cast in the battle adds them).
    pub fn heroism_modifiers(&self, unit: hecs::Entity) -> Vec<Modifier> {
        let (from, to) = (self.level_with(unit, false), self.level_with(unit, true));
        let mut mods = self.level_modifiers(unit, from + 1..to + 1);
        // A hero's abilities grow with its level too; the change is added as a difference.
        if let Ok(hero) = self.world.get::<&crate::hero::Hero>(unit) {
            let magic = crate::battle::game::ranged_kind_of(&self.unit_abilities(unit))
                == crate::battle::RangedKind::Magic;
            let before = hero.abilities.modifiers(from, magic);
            for m in hero.abilities.modifiers(to, magic) {
                let had = before
                    .iter()
                    .find(|b| b.stat == m.stat)
                    .map_or(0, |b| b.value);
                if m.value != had {
                    mods.push(Modifier::new(m.stat, m.op, m.value - had, m.source));
                }
            }
        }
        mods
    }

    /// The modifiers of the unit's level path for these levels.
    fn level_modifiers(&self, unit: hecs::Entity, levels: std::ops::Range<usize>) -> Vec<Modifier> {
        let Some(def) = self.unit_def(unit) else {
            return Vec::new();
        };
        let Some(path) = def
            .level_path
            .as_deref()
            .and_then(|p| self.content.rules.level_paths.by_name(p))
        else {
            return Vec::new();
        };
        let mut mods = Vec::new();
        for i in levels {
            if let Some((_, effects)) = path.level(i) {
                // A level's attack plus only counts for a unit that has the attack
                // (`Battle_Unit_Level_Stats`).
                mods.extend(
                    passive_modifiers(effects, Source::Level(i as u8))
                        .into_iter()
                        .filter(|m| {
                            !matches!(m.stat, Stat::Melee | Stat::Ranged)
                                || def.stats.get(&m.stat).copied().unwrap_or(0) > 0
                        }),
                );
            }
        }
        mods
    }

    /// Every modifier currently affecting the unit: its abilities and level effects.
    pub fn unit_modifiers(&self, unit: hecs::Entity) -> Vec<Modifier> {
        let rules = &self.content.rules;
        if self.unit_def(unit).is_none() {
            return Vec::new();
        }
        let mut mods = Vec::new();
        mods.extend(self.ability_modifiers(unit));
        if let Ok(hero) = self.world.get::<&crate::hero::Hero>(unit) {
            let magic = crate::battle::game::ranged_kind_of(&self.unit_abilities(unit))
                == crate::battle::RangedKind::Magic;
            mods.extend(hero.abilities.modifiers(self.unit_level(unit), magic));
        }
        mods.extend(self.level_modifiers(unit, 0..self.unit_level(unit) + 1));
        for (spell, _) in crate::spell::instance::Spells::new(self.world).unit_enchantments(unit) {
            if self.unit_spell_superseded(unit, spell) {
                continue;
            }
            mods.extend(passive_modifiers(
                &rules.spells.get(spell).effects,
                Source::Spell(spell),
            ));
        }
        for spell in self.items().spells(unit) {
            if self.unit_spell_superseded(unit, spell) {
                continue;
            }
            mods.extend(passive_modifiers(
                &rules.spells.get(spell).effects,
                Source::Spell(spell),
            ));
        }
        mods.extend(self.items().modifiers(unit));
        mods.extend(self.temp_modifiers(unit));
        mods.extend(self.global_unit_modifiers(unit));
        mods
    }

    pub fn unit_stat(&self, unit: hecs::Entity, stat: Stat) -> i32 {
        let base = self
            .unit_def(unit)
            .and_then(|d| d.stats.get(&stat).copied())
            .unwrap_or(0);
        compute(stat, base, &self.unit_modifiers(unit))
    }

    /// Full movement allowance in half-moves.
    pub fn unit_max_moves(&self, unit: hecs::Entity) -> u32 {
        let whole = self.unit_stat(unit, Stat::Moves).max(0) as u32;
        if whole == 0 {
            return 0;
        }
        let moves = whole * 2 + self.unit_stat(unit, Stat::HalfMoves).max(0) as u32;
        // Wind Mastery (our rule, not the original's): ships and fliers of a wizard who keeps it
        // go half as far again; those of a wizard without it, while anyone else keeps it, go
        // half as far. Nothing stacks.
        if self.is_ship_or_flier(unit) {
            if let Some(owner) = self.unit_owner(unit) {
                if self.has_global(owner, "wind_mastery") {
                    return moves * 3 / 2;
                }
                if crate::player::ids(self.world)
                    .into_iter()
                    .any(|p| self.has_global(p, "wind_mastery"))
                {
                    return moves / 2;
                }
            }
        }
        moves
    }

    /// A ship (sails, or carries others) or a flier (flies by nature or by a spell).
    fn is_ship_or_flier(&self, unit: hecs::Entity) -> bool {
        use crate::rules::MovementType;
        self.unit_def(unit)
            .is_some_and(|d| matches!(d.movement, MovementType::Sailing | MovementType::Flying))
            || self.unit_stat(unit, Stat::Transport) > 0
            || self.unit_has_ability(unit, "flying")
    }

    pub fn unit_moves_left(&self, unit: hecs::Entity) -> u32 {
        self.world.get::<&MovesLeft>(unit).map_or(0, |m| m.0)
    }

    /// How far the unit sees on the map, in half-squares like movement (`map::fog::in_sight`):
    /// its `Sight` stat, at least 5 (the original's scouting level 2) for a flier or a Wind
    /// Walker (`Update_Scouted_And_Contacted`).
    pub fn unit_sight(&self, unit: hecs::Entity) -> i32 {
        let sight = self.unit_stat(unit, Stat::Sight).max(3);
        let flies = self
            .unit_def(unit)
            .is_some_and(|d| d.movement == crate::rules::MovementType::Flying)
            || self.unit_has_ability(unit, "flying")
            || self.unit_has_ability(unit, "wind_walking");
        if flies {
            sight.max(5)
        } else {
            sight
        }
    }

    /// The unit's experience points.
    pub fn unit_experience(&self, unit: hecs::Entity) -> u32 {
        self.world.get::<&Experience>(unit).map_or(0, |x| x.0)
    }

    /// `(current, maximum)` hit points over all figures.
    pub fn unit_hits(&self, unit: hecs::Entity) -> (u32, u32) {
        let max = (self.unit_stat(unit, Stat::MaxHp).max(0)
            * self.unit_stat(unit, Stat::Figures).max(0)) as u32;
        let hurt = self.world.get::<&Hurt>(unit).map_or(0, |h| h.0);
        (max.saturating_sub(hurt), max)
    }

    /// Units standing on `pos`, in a stable order.
    pub fn units_at(&self, pos: Position) -> Vec<hecs::Entity> {
        let mut out: Vec<_> = self
            .world
            .query::<(&Unit, &Position)>()
            .iter()
            .filter(|(_, (_, p))| **p == pos)
            .map(|(e, _)| e)
            .collect();
        self.sort_by_serial(&mut out);
        out
    }

    /// All units, in a stable order.
    pub fn all_units(&self) -> Vec<hecs::Entity> {
        let mut out: Vec<_> = self.world.query::<&Unit>().iter().map(|(e, _)| e).collect();
        self.sort_by_serial(&mut out);
        out
    }

    /// Whether the unit pays its upkeep: a basic troop of the setting `half_upkeep` pays only
    /// when its serial number is even, so about every second unit made is free. The number is
    /// the unit's own, so letting one go changes nothing for the others.
    fn pays_upkeep(&self, unit: hecs::Entity) -> bool {
        let half = &self.content.settings.half_upkeep;
        if !half.enabled {
            return true;
        }
        let basic = self
            .unit_def(unit)
            .is_some_and(|d| d.kind == UnitKind::Normal && half.units.contains(&d.name));
        !basic || self.serial_of(unit).0.is_multiple_of(2)
    }

    /// Food a unit eats per turn.
    pub fn unit_food_upkeep(&self, unit: hecs::Entity) -> i32 {
        if self.unit_is_undead(unit) || self.unit_is_unpaid(unit) || !self.pays_upkeep(unit) {
            return 0;
        }
        match self.unit_def(unit).map(|d| d.kind) {
            Some(UnitKind::Normal) => 1,
            _ => 0,
        }
    }

    /// Whether the owner pays nothing for the unit (see [`Unpaid`]).
    pub fn unit_is_unpaid(&self, unit: hecs::Entity) -> bool {
        self.world.get::<&Unpaid>(unit).is_ok()
    }

    /// Gold a unit costs per turn (heroes with the noble trait cost nothing).
    pub fn unit_gold_upkeep(&self, unit: hecs::Entity) -> i32 {
        let Some(def) = self.unit_def(unit) else {
            return 0;
        };
        if self.unit_is_undead(unit) || self.unit_is_unpaid(unit) || !self.pays_upkeep(unit) {
            return 0;
        }
        let gold = match def.kind {
            UnitKind::Normal => def.upkeep.gold as i32,
            UnitKind::Hero if !self.unit_has_ability(unit, "noble") => def.upkeep.gold as i32,
            _ => 0,
        };
        // A computer wizard's difficulty cuts its units' upkeep (task 247).
        let cut = self
            .unit_owner(unit)
            .and_then(|o| crate::player::component::<crate::player::Bonus>(self.world, o))
            .map_or(100, |b| b.unit_upkeep);
        crate::player::Bonus::of(cut, gold)
    }

    /// A player's units, in the order of `all_units`.
    pub fn player_units(&self, player: PlayerId) -> Vec<hecs::Entity> {
        self.all_units()
            .into_iter()
            .filter(|&u| self.unit_owner(u) == Some(player))
            .collect()
    }

    /// Whether the unit counts towards a city's garrison for keeping order.
    pub fn is_garrison(&self, unit: hecs::Entity) -> bool {
        self.unit_def(unit)
            .is_some_and(|d| matches!(d.kind, UnitKind::Normal | UnitKind::Hero))
    }
}

/// Create a unit with full moves and health.
pub(crate) fn spawn(
    world: &mut World,
    content: &Content,
    serial: Serial,
    owner: hecs::Entity,
    pos: Position,
    unit: UnitId,
) -> hecs::Entity {
    let e = world.spawn((
        serial,
        Unit(unit),
        Owner(owner),
        pos,
        Hurt(0),
        Experience(0),
    ));
    let moves = UnitView::new(world, content).unit_max_moves(e);
    let _ = world.insert_one(e, MovesLeft(moves));
    e
}

impl Game {
    /// The unit rules' view of the game: the world, the content and the players. The methods
    /// below with the old names ask it.
    pub fn units(&self) -> UnitView<'_> {
        UnitView::new(&self.world, &self.content)
    }

    /// Create a unit with full moves and health, and let its owner see around it.
    pub fn spawn_unit(&mut self, owner: PlayerId, pos: Position, unit: UnitId) -> hecs::Entity {
        let serial = self.take_serial();
        let player = self
            .player_entity(owner)
            .expect("a unit is spawned for a player that exists");
        let e = spawn(&mut self.world, &self.content, serial, player, pos, unit);
        let sight = self.units().unit_sight(e) as u32;
        if !self.is_neutral_player(owner) {
            self.explore(owner, pos, sight);
        }
        e
    }

    /// The unit sheet's Dismiss button: the unit leaves its owner's service for good. Heroes
    /// are refused until the items they carry have somewhere to go (task 221).
    pub fn dismiss_unit(&mut self, unit: hecs::Entity) -> Vec<Event> {
        let Some(owner) = self.units().unit_owner(unit) else {
            return Vec::new();
        };
        if self
            .units()
            .unit_def(unit)
            .is_none_or(|d| d.kind == crate::rules::UnitKind::Hero)
        {
            return Vec::new();
        }
        self.remove_unit(unit, crate::hero::Fate::Released);
        vec![Event::UnitsDisbanded {
            player: owner,
            units: 1,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::content_with_magic;
    use crate::map::Plane;

    const AT: Position = Position {
        plane: Plane::Arcanus,
        x: 2,
        y: 2,
    };

    #[test]
    fn half_moves_add_to_the_allowance_but_not_to_a_unit_that_cannot_move() {
        use crate::game::testutil::unit;
        use crate::rules::{MovementType, Registry, Stat};
        let mut content = content_with_magic();
        let mut boots = unit("boots", MovementType::Walking, 2, &[]);
        boots.stats.insert(Stat::HalfMoves, 1);
        let mut rooted = unit("rooted", MovementType::Walking, 0, &[]);
        rooted.stats.insert(Stat::HalfMoves, 1);
        content.rules.units = Registry::new(vec![boots, rooted]);
        let mut world = World::new();
        let owner = world.spawn(crate::player::bundle(PlayerId(0)));
        let mut max = |id: &str, n: u64| {
            let def = content.rules.units.id_of(id).unwrap();
            let e = spawn(&mut world, &content, Serial(n), owner, AT, def);
            UnitView::new(&world, &content).unit_max_moves(e)
        };
        assert_eq!(max("boots", 1), 5, "two moves and a half-move");
        assert_eq!(max("rooted", 2), 0);
    }

    #[test]
    fn basic_troops_cost_half_the_upkeep_when_the_setting_is_on() {
        use crate::game::testutil::unit;
        use crate::rules::{MovementType, Registry};
        let mut content = content_with_magic();
        let mut spear = unit("Spearmen", MovementType::Walking, 1, &[]);
        spear.kind = UnitKind::Normal;
        spear.upkeep.gold = 1;
        let mut sword = unit("Swordsmen", MovementType::Walking, 1, &[]);
        sword.kind = UnitKind::Normal;
        sword.upkeep.gold = 1;
        let mut knight = unit("Knights", MovementType::Walking, 1, &[]);
        knight.kind = UnitKind::Normal;
        knight.upkeep.gold = 3;
        content.rules.units = Registry::new(vec![spear, sword, knight]);
        let mut world = World::new();
        let (a, b) = (
            world.spawn(crate::player::bundle(PlayerId(0))),
            world.spawn(crate::player::bundle(PlayerId(1))),
        );
        let id = |n: &str, c: &Content| c.rules.units.id_of(n).unwrap();
        let made: Vec<_> = [
            (a, "Spearmen"),
            (a, "Swordsmen"),
            (a, "Spearmen"),
            (a, "Knights"),
            (b, "Spearmen"),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (owner, name))| {
            spawn(
                &mut world,
                &content,
                Serial(i as u64 + 1),
                owner,
                AT,
                id(name, &content),
            )
        })
        .collect();
        let costs = |content: &Content| {
            let units = UnitView::new(&world, content);
            made.iter()
                .map(|&u| (units.unit_gold_upkeep(u), units.unit_food_upkeep(u)))
                .collect::<Vec<_>>()
        };
        // Off: every unit pays in full.
        content.settings.half_upkeep.enabled = false;
        assert_eq!(costs(&content), [(1, 1), (1, 1), (1, 1), (3, 1), (1, 1)]);
        content.settings.half_upkeep.enabled = true;
        // A basic troop with an odd serial number is free (serials 1 to 5 here); the knight, with
        // its own upkeep, is not touched.
        assert_eq!(costs(&content), [(0, 0), (1, 1), (0, 0), (3, 1), (0, 0)]);
    }

    #[test]
    fn a_units_figures_come_from_a_world_and_content_alone() {
        let content = content_with_magic();
        let spear = content.rules.units.id_of("spear").unwrap();
        let mut world = World::new();
        let owner = world.spawn(crate::player::bundle(PlayerId(0)));
        let e = spawn(&mut world, &content, Serial(1), owner, AT, spear);
        let units = UnitView::new(&world, &content);
        assert_eq!(units.unit_owner(e), Some(PlayerId(0)));
        assert_eq!(units.unit_position(e), Some(AT));
        assert_eq!(units.unit_max_moves(e), 2, "one move is two half-moves");
        assert_eq!(units.unit_moves_left(e), 2);
        let (left, max) = units.unit_hits(e);
        assert!(max > 0 && left == max);
        assert_eq!(units.unit_level(e), 0);
        assert!(!units.unit_has_ability(e, "flying"));
        assert_eq!(units.units_at(AT), vec![e]);
    }

    #[test]
    fn units_come_in_the_order_they_were_made() {
        let content = content_with_magic();
        let spear = content.rules.units.id_of("spear").unwrap();
        let mut world = World::new();
        let owner = world.spawn(crate::player::bundle(PlayerId(0)));
        let second = spawn(&mut world, &content, Serial(2), owner, AT, spear);
        let first = spawn(&mut world, &content, Serial(1), owner, AT, spear);
        let units = UnitView::new(&world, &content);
        assert_eq!(units.all_units(), vec![first, second]);
        assert_eq!(units.units_at(AT), vec![first, second]);
    }
}
