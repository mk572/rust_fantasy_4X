//! Unit instances: components on map entities, and stats computed from their
//! definition, abilities and experience level through the modifier engine.

use crate::city::{PlayerId, Position};
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

/// Movement left this turn, in half-moves (a terrain cost of 1 is 2 half-moves).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MovesLeft(pub u32);

/// A standing order carried out at the start of each turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Order {
    /// Walk this path (the first entry is the next tile).
    GoTo(Vec<Position>),
}

/// Owner of monsters and unclaimed sites.
pub const NEUTRAL: PlayerId = PlayerId(u8::MAX);

/// Most units that may share one tile.
pub const MAX_STACK: usize = 9;

impl Game {
    pub fn unit_def(&self, unit: hecs::Entity) -> Option<&UnitDef> {
        let id = self.world.get::<&Unit>(unit).ok()?.0;
        Some(self.content.rules.units.get(id))
    }

    pub fn unit_owner(&self, unit: hecs::Entity) -> Option<PlayerId> {
        self.world.get::<&Owner>(unit).ok().map(|o| o.0)
    }

    pub fn unit_position(&self, unit: hecs::Entity) -> Option<Position> {
        self.world.get::<&Position>(unit).ok().map(|p| *p)
    }

    pub fn unit_has_ability(&self, unit: hecs::Entity, ability: &str) -> bool {
        self.unit_def(unit)
            .is_some_and(|d| d.abilities.iter().any(|a| a == ability))
    }

    /// Experience level, 0-based, from the unit's level path (0 without one).
    pub fn unit_level(&self, unit: hecs::Entity) -> usize {
        let Some(def) = self.unit_def(unit) else {
            return 0;
        };
        let xp = self.world.get::<&Experience>(unit).map_or(0, |x| x.0);
        let Some(path) = def
            .level_path
            .as_deref()
            .and_then(|p| self.content.rules.level_paths.by_name(p))
        else {
            return 0;
        };
        path.levels.iter().rposition(|l| l.xp <= xp).unwrap_or(0)
    }

    /// Every modifier currently affecting the unit: its abilities and level effects.
    pub fn unit_modifiers(&self, unit: hecs::Entity) -> Vec<Modifier> {
        let rules = &self.content.rules;
        let Some(def) = self.unit_def(unit) else {
            return Vec::new();
        };
        let mut mods = Vec::new();
        for name in &def.abilities {
            if let Some(id) = rules.abilities.id_of(name) {
                mods.extend(passive_modifiers(
                    &rules.abilities.get(id).effects,
                    Source::Ability(id),
                ));
            }
        }
        if let Some(path) = def
            .level_path
            .as_deref()
            .and_then(|p| rules.level_paths.by_name(p))
        {
            let level = self.unit_level(unit);
            for (i, lvl) in path.levels.iter().enumerate().take(level + 1) {
                mods.extend(passive_modifiers(&lvl.effects, Source::Level(i as u8)));
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
        self.explore(owner, pos, sight);
        e
    }
}
