//! Special overland spells with no home of their own: Transmute, Earth Lore, Nature's Cures,
//! Word of Recall, Plane Shift and Floating Island. Each is a hook the spell runs; the targets they
//! accept are checked by `check_target` in `spell/cast.rs`.

use crate::map::{Plane, Position};

use crate::spell::target_error::TargetError;
use crate::unit::Hurt;
use crate::world::{Event, World};

pub use f4x_def::rules::hook_name::{RECALL, RECALL_ANYONE, RECALL_HERO};
/// The hook of the Spell of Mastery, which wins the game when the casting is done.
pub const SPELL_OF_MASTERY: &str = "spell_of_mastery";
/// The hook of the Spell of Return, which brings a banished wizard home.
pub const SPELL_OF_RETURN: &str = "spell_of_return";
/// The hook of Plane Shift.
pub const PLANE_SHIFT: &str = "plane_shift";
/// The hook of the spells that move a building of the wizard to a city: the argument is the
/// building, `fortress` or `summoning_circle`.
pub const MOVE_BUILDING: &str = "move_building";
pub const MOVE_FORTRESS: &str = "fortress";
pub const MOVE_CIRCLE: &str = "summoning_circle";

/// The block Earth Lore reveals, in tiles.
const LORE_WIDTH: u32 = 12;
const LORE_HEIGHT: u32 = 10;

impl World {
    /// The mineral Transmute makes of the one on `at`, or why there is none it can change.
    pub fn transmute_result(&self, at: Position) -> Result<crate::map::FeatureId, TargetError> {
        let map = self.map.as_ref().ok_or(TargetError::OffMap)?;
        (|| {
            let here = map.plane(at.plane).mineral(at.x as i32, at.y as i32)?;
            // The mineral's row says what Transmute makes of it; adamantium and the crystals have none.
            let to = self.content.features.get(here)?.transmute?;
            self.content.features.id_numbered(to)
        })()
        .ok_or(TargetError::NothingToTransmute)
    }

    /// Floating Island needs a water square with no enemy units on it and room for the island:
    /// it is raised where the wizard aims.
    pub fn check_floating_island(
        &self,
        owner: crate::player::PlayerId,
        at: Position,
    ) -> Result<(), TargetError> {
        match self.economy().terrain_class_at(at) {
            None => return Err(TargetError::OffMap),
            Some(crate::map::terrain::TerrainClass::Ocean) => {}
            Some(_) => return Err(TargetError::NotWater),
        }
        if self.economy().hostile_at(at, owner).is_some() {
            return Err(TargetError::EnemyOnSquare);
        }
        Ok(())
    }

    pub(crate) fn floating_island(&mut self, owner: crate::player::PlayerId, at: Position) {
        if self.check_floating_island(owner, at).is_err() {
            return;
        }
        let island = crate::rules::UnitId::new(190) /* floating_island */;
        if let Some(id) = self.content.rules.units.id_numbered(island.num()) {
            self.spawn_unit(owner, at, id);
        }
    }

    pub(crate) fn transmute(&mut self, at: Position) -> Vec<Event> {
        let Ok(to) = self.transmute_result(at) else {
            return Vec::new();
        };
        if let Some(map) = self.map.as_mut() {
            map.plane_mut(at.plane).set_mineral(at.x, at.y, Some(to));
        }
        vec![Event::TerrainChanged { pos: at }]
    }

    /// Earth Lore: the wizard explores the whole block around the target tile.
    pub(crate) fn earth_lore(&mut self, caster: crate::player::PlayerId, at: Position) {
        self.fog.explore_block(caster, at, LORE_WIDTH, LORE_HEIGHT);
    }

    /// Nature's Cures: every unit on the target's tile is healed in full, except creatures
    /// of the Death realm and the undead.
    pub(crate) fn natures_cures(&mut self, unit: hecs::Entity) {
        let Some(at) = self.units().unit_position(unit) else {
            return;
        };
        let healable: Vec<_> = self
            .units()
            .units_at(at)
            .into_iter()
            .filter(|&u| !self.units().unit_is_death(u))
            .collect();
        for u in healable {
            self.put(u, Hurt(0));
        }
    }

    /// Where Word of Recall would send a unit of `player`: the summoning circle, if the wizard has
    /// one. With several circles (`multiple_summoning_circles`) `at` names the one, and is asked for. A
    /// full circle takes the unit all the same (`unit/sanity.rs`).
    pub fn recall_destination(
        &self,
        player: crate::player::PlayerId,
        at: Option<Position>,
    ) -> Result<Position, TargetError> {
        let circles = self.summon_circles(player);
        match (at, circles.as_slice()) {
            (_, []) => Err(TargetError::NoCircle),
            (Some(at), circles) if circles.contains(&at) => Ok(at),
            (Some(_), _) => Err(TargetError::WrongCircle),
            (None, [only]) => Ok(*only),
            (None, _) => Err(TargetError::NeedsCircle),
        }
    }

    /// Send the unit to the summoning circle, or say why it cannot go (nothing is changed then).
    pub(crate) fn word_of_recall(
        &mut self,
        caster: crate::player::PlayerId,
        unit: hecs::Entity,
        at: Option<Position>,
    ) -> Result<(), TargetError> {
        let to = self.recall_destination(caster, at)?;
        self.put(unit, to);
        self.explore(caster, to, crate::map::fog::sight_for(1));
        Ok(())
    }

    /// Where Plane Shift would take the stack `unit` stands in, or why it cannot go:
    /// the Planar Seal is up, an enemy holds the square on the other plane, the stack would
    /// not fit, or one of its units could not stand there.
    pub fn plane_shift_destination(&self, unit: hecs::Entity) -> Result<Position, TargetError> {
        if self.units().planar_seal_active() {
            return Err(TargetError::PlanarSeal);
        }
        let from = self
            .units()
            .unit_position(unit)
            .ok_or(TargetError::WrongTarget)?;
        let owner = self
            .units()
            .unit_owner(unit)
            .ok_or(TargetError::WrongTarget)?;
        let to = Position {
            plane: match from.plane {
                Plane::Arcanus => Plane::Myrror,
                Plane::Myrror => Plane::Arcanus,
            },
            ..from
        };
        let stack = self.units().units_at(from);
        if self.economy().hostile_at(to, owner).is_some() {
            return Err(TargetError::EnemyOnOtherPlane);
        }
        if self.units().units_at(to).len() + stack.len() > self.tile_limit(to) {
            return Err(TargetError::OtherPlaneFull);
        }
        let traits = self.economy().stack_traits(&stack);
        stack
            .iter()
            .all(|&u| self.economy().step_cost(u, to, to, traits).is_ok())
            .then_some(to)
            .ok_or(TargetError::CannotStandThere)
    }

    /// Moves the whole stack `unit` stands in to the other plane; the event names the stack and
    /// the two squares. `None` when it cannot go.
    pub(crate) fn plane_shift(
        &mut self,
        caster: crate::player::PlayerId,
        unit: hecs::Entity,
    ) -> Option<crate::world::Event> {
        let from = self.units().unit_position(unit)?;
        let to = self.plane_shift_destination(unit).ok()?;
        let units = self.units().units_at(from);
        for &u in &units {
            self.put(u, to);
        }
        self.explore(caster, to, crate::map::fog::sight_for(1));
        Some(crate::world::Event::PlaneShifted {
            player: caster,
            from,
            to,
            units,
        })
    }
}
