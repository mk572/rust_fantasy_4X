//! Unit instances: components on map entities, and stats computed from their
//! definition, abilities and experience level through the modifier engine.

use crate::city::{PlayerId, Position};
use crate::magic::Retort;
use crate::rules::stats::{compute, passive_modifiers, Modifier, Source};
use crate::rules::{Stat, UnitDef, UnitId};
use crate::Game;

/// Marks an entity as a unit of this definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unit(pub UnitId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner(pub PlayerId);

/// Hit points lost, summed over all figures of the unit.
/// Lasting spells on a unit, with who cast each.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnitEnchantments(pub Vec<(crate::rules::SpellId, PlayerId)>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Hurt(pub u32);

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Order {
    /// Walk this path (the first entry is the next tile).
    GoTo(Vec<Position>),
}

/// Owner of monsters and unclaimed sites. Its units last until someone kills them (the lair
/// guards are made for a fight and removed after it; undead raised for the monsters stay).
pub const NEUTRAL: PlayerId = PlayerId(u8::MAX);

/// Most units that may share one tile.
pub const MAX_STACK: usize = 9;

impl Game {
    pub fn unit_def(&self, unit: hecs::Entity) -> Option<&UnitDef> {
        let id = self.world.get::<&Unit>(unit).ok()?.0;
        Some(self.content.rules.units.get(id))
    }

    /// The quality of the unit's weapons (see [`WeaponQuality`]).
    pub fn unit_weapon_quality(&self, unit: hecs::Entity) -> u8 {
        self.world.get::<&WeaponQuality>(unit).map_or(0, |w| w.0)
    }

    pub fn unit_owner(&self, unit: hecs::Entity) -> Option<PlayerId> {
        self.world.get::<&Owner>(unit).ok().map(|o| o.0)
    }

    pub fn unit_position(&self, unit: hecs::Entity) -> Option<Position> {
        self.world.get::<&Position>(unit).ok().map(|p| *p)
    }

    pub fn unit_has_ability(&self, unit: hecs::Entity, ability: &str) -> bool {
        self.unit_def(unit).is_some() && self.unit_abilities(unit).iter().any(|a| a == ability)
    }

    fn unit_level_path(&self, unit: hecs::Entity) -> Option<&crate::rules::LevelPathDef> {
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

    /// Whether the Heroism spell is on the unit.
    pub fn unit_has_heroism(&self, unit: hecs::Entity) -> bool {
        let Some(spell) = self.content.rules.spells.id_of(HEROISM) else {
            return false;
        };
        self.world
            .get::<&UnitEnchantments>(unit)
            .is_ok_and(|list| list.0.iter().any(|&(s, _)| s == spell))
    }

    /// Experience level, 0-based, from the unit's level path (0 without one): the level its
    /// experience gives, raised to Elite by Heroism, then by one each for its owner's Warlord
    /// retort and Crusade spell (which leave the undead and creatures alone). Battles use the
    /// same level as the map (the original's battle code ignores Warlord, Crusade and xp when
    /// Heroism is on: B9 in `help-vs-implementation-conflicts.md`, we follow the help).
    pub fn unit_level(&self, unit: hecs::Entity) -> usize {
        let Some(path) = self.unit_level_path(unit) else {
            return 0;
        };
        let mut level = self.unit_xp_level(unit);
        if level < HEROISM_LEVEL && self.unit_has_heroism(unit) {
            level = HEROISM_LEVEL;
        }
        if let Some(owner) = self.unit_owner(unit) {
            if self.magic(owner).is_some_and(|m| m.has(Retort::Warlord)) {
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

    /// Every modifier currently affecting the unit: its abilities and level effects.
    pub fn unit_modifiers(&self, unit: hecs::Entity) -> Vec<Modifier> {
        let rules = &self.content.rules;
        let Some(def) = self.unit_def(unit) else {
            return Vec::new();
        };
        let mut mods = Vec::new();
        mods.extend(self.ability_modifiers(unit));
        if let Some(path) = def
            .level_path
            .as_deref()
            .and_then(|p| rules.level_paths.by_name(p))
        {
            let level = self.unit_level(unit);
            for i in 0..=level {
                if let Some((_, effects)) = path.level(i) {
                    mods.extend(passive_modifiers(effects, Source::Level(i as u8)));
                }
            }
        }
        if let Ok(list) = self.world.get::<&UnitEnchantments>(unit) {
            for &(spell, _) in &list.0 {
                mods.extend(passive_modifiers(
                    &rules.spells.get(spell).effects,
                    Source::Spell(spell),
                ));
            }
        }
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
        self.unit_stat(unit, Stat::Moves).max(0) as u32 * 2
    }

    pub fn unit_moves_left(&self, unit: hecs::Entity) -> u32 {
        self.world.get::<&MovesLeft>(unit).map_or(0, |m| m.0)
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

    /// Create a unit with full moves and health, and let its owner see around it.
    pub fn spawn_unit(&mut self, owner: PlayerId, pos: Position, unit: UnitId) -> hecs::Entity {
        let serial = self.take_serial();
        let e = self.world.spawn((
            serial,
            Unit(unit),
            Owner(owner),
            pos,
            Hurt(0),
            Experience(0),
        ));
        let moves = self.unit_max_moves(e);
        let _ = self.world.insert_one(e, MovesLeft(moves));
        let sight = self.unit_stat(e, Stat::Sight).max(1) as u32;
        if !self.is_neutral_player(owner) {
            self.explore(owner, pos, sight);
        }
        e
    }
}
