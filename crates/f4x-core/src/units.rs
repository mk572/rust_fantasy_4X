//! Unit instances: components, derived stats, movement and equipment.
//!
//! A unit entity stores only what changes (damage, experience, moves left,
//! equipped items); everything else is computed from its definition plus the
//! modifiers currently applying, via [`crate::rules::stats`].

use hecs::Entity;

use crate::city::{PlayerId, Position};
use crate::content::TerrainClass;
use crate::rules::stats::{compute, passive_modifiers, Modifier, Source, Stat};
use crate::rules::{ItemId, ItemSlot, MovementType, UnitDef, UnitId, UnitKind};
use crate::Game;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unit {
    pub def: UnitId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner(pub PlayerId);

/// Hit points lost, as one pool across all figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Damage(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Experience(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MovesLeft(pub i32);

/// Item slots of a hero and what is in each.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Equipment(pub Vec<(ItemSlot, Option<ItemId>)>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Health {
    pub max_total: i32,
    pub current_total: i32,
    pub figures_alive: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveError {
    NoSuchUnit,
    NotYours,
    NoMap,
    OutOfBounds,
    /// Plane changes go through towers and spells, not ordinary moves.
    WrongPlane,
    NotAdjacent,
    NoMovesLeft,
    /// This unit cannot enter that terrain.
    ImpassableTerrain,
    StackFull,
    /// An enemy unit or city is there; resolving that is combat's job.
    EnemyPresent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquipError {
    NoSuchUnit,
    NotYours,
    NotAHero,
    NoSuchItem,
    NoFreeSlot,
    /// The item is not equipped on this unit.
    NotEquipped,
}

/// Index of the highest level whose xp threshold has been reached.
pub fn level_for_xp(levels: &[crate::rules::LevelDef], xp: u32) -> usize {
    levels.iter().rposition(|l| l.xp <= xp).unwrap_or(0)
}

impl Game {
    pub fn unit_def(&self, unit: Entity) -> Option<&UnitDef> {
        let u = self.world.get::<&Unit>(unit).ok()?;
        Some(self.content.rules.units.get(u.def))
    }

    pub fn unit_owner(&self, unit: Entity) -> Option<PlayerId> {
        self.world.get::<&Owner>(unit).ok().map(|o| o.0)
    }

    /// Experience level (0-based) of a unit, or `None` for a non-unit.
    pub fn unit_level(&self, unit: Entity) -> Option<usize> {
        let def = self.unit_def(unit)?;
        let path = self
            .content
            .rules
            .level_paths
            .by_name(def.level_path.as_deref()?)?;
        let xp = self.world.get::<&Experience>(unit).ok().map_or(0, |x| x.0);
        Some(level_for_xp(&path.levels, xp))
    }

    /// Everything modifying this unit's stats right now: abilities, experience
    /// levels and equipped items. (Lasting spells join here once casting exists.)
    pub fn unit_modifiers(&self, unit: Entity) -> Vec<Modifier> {
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
        if let (Some(level), Some(path)) = (
            self.unit_level(unit),
            def.level_path
                .as_deref()
                .and_then(|p| rules.level_paths.by_name(p)),
        ) {
            for (i, l) in path.levels.iter().enumerate().take(level + 1) {
                mods.extend(passive_modifiers(&l.effects, Source::Level(i as u8)));
            }
        }
        if let Ok(eq) = self.world.get::<&Equipment>(unit) {
            for item in eq.0.iter().filter_map(|(_, i)| *i) {
                mods.extend(passive_modifiers(
                    &rules.items.get(item).effects,
                    Source::Item(item),
                ));
            }
        }
        mods
    }

    /// A unit's final value for `stat`.
    pub fn unit_stat(&self, unit: Entity, stat: Stat) -> Option<i32> {
        let def = self.unit_def(unit)?;
        let base = def.stats.get(&stat).copied().unwrap_or(0);
        Some(compute(stat, base, &self.unit_modifiers(unit)))
    }

    pub fn unit_health(&self, unit: Entity) -> Option<Health> {
        let hp = self.unit_stat(unit, Stat::MaxHp)?;
        let figures = self.unit_stat(unit, Stat::Figures)?;
        let damage = self.world.get::<&Damage>(unit).map_or(0, |d| d.0 as i32);
        let max_total = hp * figures;
        let current_total = (max_total - damage).max(0);
        // Figures die from the back: any partial figure still counts as alive.
        let figures_alive = if hp > 0 {
            (current_total + hp - 1) / hp
        } else {
            0
        };
        Some(Health {
            max_total,
            current_total,
            figures_alive,
        })
    }

    pub fn units_at(&self, pos: Position) -> Vec<Entity> {
        let mut v: Vec<Entity> = self
            .world
            .query::<(&Unit, &Position)>()
            .iter()
            .filter(|(_, (_, p))| **p == pos)
            .map(|(e, _)| e)
            .collect();
        v.sort_by_key(|e| e.id());
        v
    }

    pub fn units_of(&self, player: PlayerId) -> Vec<Entity> {
        let mut v: Vec<Entity> = self
            .world
            .query::<(&Unit, &Owner)>()
            .iter()
            .filter(|(_, (_, o))| o.0 == player)
            .map(|(e, _)| e)
            .collect();
        v.sort_by_key(|e| e.id());
        v
    }

    /// Create a unit. Used by production, summoning, hiring and tests.
    pub fn spawn_unit(&mut self, owner: PlayerId, def: UnitId, pos: Position) -> Entity {
        let hero = self.content.rules.units.get(def).hero.clone();
        let e = self.world.spawn((
            Unit { def },
            Owner(owner),
            pos,
            Damage::default(),
            Experience::default(),
            MovesLeft(0),
        ));
        if let Some(h) = hero {
            let slots = h.slots.iter().map(|&s| (s, None)).collect();
            self.world
                .insert_one(e, Equipment(slots))
                .expect("entity just spawned");
        }
        let moves = self.unit_stat(e, Stat::Moves).unwrap_or(0);
        self.world
            .insert_one(e, MovesLeft(moves))
            .expect("entity just spawned");
        e
    }

    /// Give every unit its full movement back (start of turn).
    pub(crate) fn refresh_moves(&mut self) {
        let units: Vec<Entity> = self.world.query::<&Unit>().iter().map(|(e, _)| e).collect();
        for e in units {
            let moves = self.unit_stat(e, Stat::Moves).unwrap_or(0);
            let _ = self.world.insert_one(e, MovesLeft(moves));
        }
    }

    /// Check and perform a one-tile move; returns the position moved from.
    pub(crate) fn move_unit(
        &mut self,
        by: PlayerId,
        unit: Entity,
        to: Position,
    ) -> Result<Position, MoveError> {
        let def = self.unit_def(unit).ok_or(MoveError::NoSuchUnit)?.clone();
        if self.unit_owner(unit) != Some(by) {
            return Err(MoveError::NotYours);
        }
        let from = *self
            .world
            .get::<&Position>(unit)
            .map_err(|_| MoveError::NoSuchUnit)?;
        let map = self.map.as_ref().ok_or(MoveError::NoMap)?;
        if from.plane != to.plane {
            return Err(MoveError::WrongPlane);
        }
        let grid = map.plane(to.plane);
        if to.x >= grid.width() || to.y >= grid.height() {
            return Err(MoveError::OutOfBounds);
        }
        if grid.chebyshev((from.x, from.y), (to.x, to.y)) != 1 {
            return Err(MoveError::NotAdjacent);
        }
        let left = self.world.get::<&MovesLeft>(unit).map_or(0, |m| m.0);
        if left <= 0 {
            return Err(MoveError::NoMovesLeft);
        }
        let terrain = grid
            .get(to.x as i32, to.y as i32)
            .and_then(|t| self.content.terrains.get(t))
            .ok_or(MoveError::OutOfBounds)?;
        let is_ocean = terrain.class == TerrainClass::Ocean;
        let cost = match def.movement {
            MovementType::Flying => 1,
            MovementType::Walking if terrain.walkable => i32::from(terrain.move_cost.max(1)),
            MovementType::Swimming | MovementType::Sailing if is_ocean => 1,
            _ => return Err(MoveError::ImpassableTerrain),
        };
        // Enemy city or units on the tile: combat's job, not a plain move.
        if let Some(city) = self.city_at(to) {
            if self
                .world
                .get::<&crate::city::City>(city)
                .is_ok_and(|c| c.owner != by)
            {
                return Err(MoveError::EnemyPresent);
            }
        }
        let there = self.units_at(to);
        if there
            .iter()
            .any(|&u| self.unit_owner(u).is_some_and(|o| o != by))
        {
            return Err(MoveError::EnemyPresent);
        }
        if there.len() as u32 >= self.content.rules.config.max_stack {
            return Err(MoveError::StackFull);
        }
        // A unit with any movement left may always enter an adjacent tile; the
        // move uses up whatever the tile costs, down to zero.
        let _ = self.world.insert_one(unit, MovesLeft((left - cost).max(0)));
        let _ = self.world.insert_one(unit, to);
        Ok(from)
    }

    pub(crate) fn equip_item(
        &mut self,
        by: PlayerId,
        unit: Entity,
        item: ItemId,
    ) -> Result<(), EquipError> {
        let def = self.unit_def(unit).ok_or(EquipError::NoSuchUnit)?;
        if def.kind != UnitKind::Hero {
            return Err(EquipError::NotAHero);
        }
        if self.unit_owner(unit) != Some(by) {
            return Err(EquipError::NotYours);
        }
        if item.index() >= self.content.rules.items.len() {
            return Err(EquipError::NoSuchItem);
        }
        let slot = self.content.rules.items.get(item).slot;
        let mut eq = self
            .world
            .get::<&mut Equipment>(unit)
            .map_err(|_| EquipError::NotAHero)?;
        let free =
            eq.0.iter_mut()
                .find(|(s, i)| *s == slot && i.is_none())
                .ok_or(EquipError::NoFreeSlot)?;
        free.1 = Some(item);
        Ok(())
    }

    pub(crate) fn unequip_item(
        &mut self,
        by: PlayerId,
        unit: Entity,
        item: ItemId,
    ) -> Result<(), EquipError> {
        if self.unit_def(unit).ok_or(EquipError::NoSuchUnit)?.kind != UnitKind::Hero {
            return Err(EquipError::NotAHero);
        }
        if self.unit_owner(unit) != Some(by) {
            return Err(EquipError::NotYours);
        }
        let mut eq = self
            .world
            .get::<&mut Equipment>(unit)
            .map_err(|_| EquipError::NotAHero)?;
        let held =
            eq.0.iter_mut()
                .find(|(_, i)| *i == Some(item))
                .ok_or(EquipError::NotEquipped)?;
        held.1 = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Plane::{Arcanus, Myrror};
    use crate::testutil::*;
    use crate::{Command, Event, Rejection};

    fn spawn(g: &mut Game, owner: PlayerId, name: &str, pos: Position) -> Entity {
        let def = unit_id(g, name);
        g.spawn_unit(owner, def, pos)
    }

    fn mv(g: &mut Game, by: PlayerId, unit: Entity, to: Position) -> Result<(), MoveError> {
        match g.apply(Command::MoveUnit { by, unit, to })[..] {
            [Event::UnitMoved { .. }] => Ok(()),
            [Event::Rejected(Rejection::Move(e))] => Err(e),
            ref other => panic!("unexpected {other:?}"),
        }
    }

    fn moves_left(g: &Game, u: Entity) -> i32 {
        g.world.get::<&MovesLeft>(u).unwrap().0
    }

    #[test]
    fn stats_combine_definition_ability_level_and_items() {
        let mut g = game();
        let s = spawn(&mut g, P0, "spearman", at(Arcanus, 3, 3));
        assert_eq!(g.unit_stat(s, Stat::Melee), Some(3));
        assert_eq!(
            g.unit_stat(s, Stat::Defense),
            Some(4),
            "armoured ability adds 1"
        );
        g.world.insert_one(s, Experience(10)).unwrap();
        assert_eq!(g.unit_level(s), Some(1));
        assert_eq!(g.unit_stat(s, Stat::Melee), Some(4));
        g.world.insert_one(s, Experience(30)).unwrap();
        assert_eq!(
            g.unit_stat(s, Stat::Melee),
            Some(5),
            "level modifiers accumulate"
        );
        assert!(g
            .unit_modifiers(s)
            .iter()
            .any(|m| m.source == Source::Level(2)));
        assert_eq!(g.unit_stat(Entity::DANGLING, Stat::Melee), None);
    }

    #[test]
    fn health_counts_figures_from_a_shared_pool() {
        let mut g = game();
        let s = spawn(&mut g, P0, "spearman", at(Arcanus, 3, 3)); // 3 figures x 2 hp
        let alive = |g: &Game| g.unit_health(s).unwrap();
        assert_eq!((alive(&g).max_total, alive(&g).figures_alive), (6, 3));
        g.world.insert_one(s, Damage(3)).unwrap();
        assert_eq!((alive(&g).current_total, alive(&g).figures_alive), (3, 2));
        g.world.insert_one(s, Damage(5)).unwrap();
        assert_eq!(alive(&g).figures_alive, 1, "a wounded figure still counts");
        g.world.insert_one(s, Damage(99)).unwrap();
        assert_eq!((alive(&g).current_total, alive(&g).figures_alive), (0, 0));
    }

    #[test]
    fn heroes_equip_by_slot() {
        let mut g = game();
        let hero = spawn(&mut g, P0, "hero", at(Arcanus, 3, 3));
        let item = |g: &Game, n: &str| g.content.rules.items.id_of(n).unwrap();
        let (sword, axe, shield, ring) = (
            item(&g, "sword"),
            item(&g, "axe"),
            item(&g, "shield"),
            item(&g, "ring"),
        );
        let equip =
            |g: &mut Game, by, unit, item| match g.apply(Command::EquipItem { by, unit, item })[..]
            {
                [Event::ItemEquipped { .. }] => Ok(()),
                [Event::Rejected(Rejection::Equip(e))] => Err(e),
                ref o => panic!("{o:?}"),
            };
        assert_eq!(g.unit_stat(hero, Stat::Melee), Some(4));
        assert_eq!(equip(&mut g, P0, hero, sword), Ok(()));
        assert_eq!(g.unit_stat(hero, Stat::Melee), Some(6));
        assert_eq!(
            equip(&mut g, P0, hero, axe),
            Err(EquipError::NoFreeSlot),
            "one weapon slot"
        );
        assert_eq!(
            equip(&mut g, P0, hero, ring),
            Err(EquipError::NoFreeSlot),
            "no misc slot"
        );
        assert_eq!(equip(&mut g, P1, hero, shield), Err(EquipError::NotYours));
        assert_eq!(equip(&mut g, P0, hero, shield), Ok(()));
        assert_eq!(g.unit_stat(hero, Stat::Defense), Some(5));
        let events = g.apply(Command::UnequipItem {
            by: P0,
            unit: hero,
            item: sword,
        });
        assert_eq!(
            events,
            vec![Event::ItemUnequipped {
                unit: hero,
                item: sword
            }]
        );
        assert_eq!(g.unit_stat(hero, Stat::Melee), Some(4));
        assert_eq!(equip(&mut g, P0, hero, axe), Ok(()));
        let s = spawn(&mut g, P0, "spearman", at(Arcanus, 4, 3));
        assert_eq!(equip(&mut g, P0, s, sword), Err(EquipError::NotAHero));
    }

    #[test]
    fn movement_costs_follow_terrain_and_refresh_each_turn() {
        let mut g = game();
        let s = spawn(&mut g, P0, "spearman", at(Arcanus, 7, 5)); // 2 moves
        assert_eq!(mv(&mut g, P0, s, at(Arcanus, 7, 4)), Ok(()));
        assert_eq!(moves_left(&g, s), 1);
        // Forest costs 2 but a unit with any movement left may still enter.
        assert_eq!(mv(&mut g, P0, s, at(Arcanus, 6, 5)), Ok(()));
        assert_eq!(moves_left(&g, s), 0);
        assert_eq!(
            mv(&mut g, P0, s, at(Arcanus, 6, 4)),
            Err(MoveError::NoMovesLeft)
        );
        g.apply(Command::EndTurn);
        assert_eq!(moves_left(&g, s), 2);
        assert_eq!(g.unit_stat(s, Stat::Moves), Some(2));
    }

    #[test]
    fn movement_rules() {
        let mut g = game();
        let s = spawn(&mut g, P0, "spearman", at(Arcanus, 3, 3));
        assert_eq!(
            mv(&mut g, P1, s, at(Arcanus, 4, 3)),
            Err(MoveError::NotYours)
        );
        assert_eq!(
            mv(&mut g, P0, s, at(Arcanus, 5, 3)),
            Err(MoveError::NotAdjacent)
        );
        assert_eq!(
            mv(&mut g, P0, s, at(Arcanus, 3, 3)),
            Err(MoveError::NotAdjacent)
        );
        assert_eq!(
            mv(&mut g, P0, s, at(Myrror, 4, 3)),
            Err(MoveError::WrongPlane)
        );
        assert_eq!(
            mv(&mut g, P0, s, at(Arcanus, 4, 99)),
            Err(MoveError::OutOfBounds)
        );
        let w = spawn(&mut g, P0, "spearman", at(Arcanus, 1, 3));
        assert_eq!(
            mv(&mut g, P0, w, at(Arcanus, 0, 3)),
            Err(MoveError::ImpassableTerrain),
            "ocean"
        );
        assert_eq!(
            mv(&mut g, P0, Entity::DANGLING, at(Arcanus, 1, 3)),
            Err(MoveError::NoSuchUnit)
        );
        // Diagonal moves are fine.
        assert_eq!(mv(&mut g, P0, s, at(Arcanus, 4, 4)), Ok(()));
    }

    #[test]
    fn movement_types() {
        let mut g = game();
        let bat = spawn(&mut g, P0, "bat", at(Arcanus, 19, 3));
        // Flying crosses the wrap seam onto ocean, at cost 1 per tile.
        assert_eq!(mv(&mut g, P0, bat, at(Arcanus, 0, 3)), Ok(()));
        assert_eq!(moves_left(&g, bat), 3);
        let raft = spawn(&mut g, P0, "raft", at(Arcanus, 0, 5));
        assert_eq!(mv(&mut g, P0, raft, at(Arcanus, 0, 6)), Ok(()));
        assert_eq!(
            mv(&mut g, P0, raft, at(Arcanus, 1, 6)),
            Err(MoveError::ImpassableTerrain),
            "boats stay on water"
        );
    }

    #[test]
    fn enemies_cities_and_stacks_block_movement() {
        let mut g = game();
        let s = spawn(&mut g, P0, "spearman", at(Arcanus, 3, 3));
        spawn(&mut g, P1, "spearman", at(Arcanus, 4, 3));
        assert_eq!(
            mv(&mut g, P0, s, at(Arcanus, 4, 3)),
            Err(MoveError::EnemyPresent)
        );
        found(&mut g, P1, "sunfolk", at(Arcanus, 3, 5));
        let s2 = spawn(&mut g, P0, "spearman", at(Arcanus, 3, 4));
        assert_eq!(
            mv(&mut g, P0, s2, at(Arcanus, 3, 5)),
            Err(MoveError::EnemyPresent),
            "enemy city"
        );
        found(&mut g, P0, "sunfolk", at(Arcanus, 8, 8));
        let s3 = spawn(&mut g, P0, "spearman", at(Arcanus, 8, 7));
        assert_eq!(mv(&mut g, P0, s3, at(Arcanus, 8, 8)), Ok(()), "own city");
        for _ in 0..8 {
            spawn(&mut g, P0, "pikeman", at(Arcanus, 10, 10));
        }
        let s4 = spawn(&mut g, P0, "spearman", at(Arcanus, 10, 9));
        assert_eq!(
            mv(&mut g, P0, s4, at(Arcanus, 10, 10)),
            Ok(()),
            "ninth unit fits"
        );
        let s5 = spawn(&mut g, P0, "spearman", at(Arcanus, 10, 9));
        assert_eq!(
            mv(&mut g, P0, s5, at(Arcanus, 10, 10)),
            Err(MoveError::StackFull)
        );
    }

    #[test]
    fn disbanding_and_queries() {
        let mut g = game();
        let a = spawn(&mut g, P0, "spearman", at(Arcanus, 3, 3));
        let b = spawn(&mut g, P1, "spearman", at(Arcanus, 3, 3));
        assert_eq!(g.units_at(at(Arcanus, 3, 3)), vec![a, b]);
        assert_eq!(g.units_of(P1), vec![b]);
        assert_eq!(
            g.apply(Command::DisbandUnit { by: P1, unit: a }),
            vec![Event::Rejected(Rejection::NotYours)]
        );
        assert_eq!(
            g.apply(Command::DisbandUnit { by: P0, unit: a }),
            vec![Event::UnitDisbanded { unit: a }]
        );
        assert_eq!(
            g.apply(Command::DisbandUnit { by: P0, unit: a }),
            vec![Event::Rejected(Rejection::NoSuchUnit)]
        );
        assert_eq!(g.units_at(at(Arcanus, 3, 3)), vec![b]);
    }
}
