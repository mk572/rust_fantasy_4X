//! Whether a spell may be aimed at a unit or a square of a battle. The facts of the world that the
//! check needs (what a unit was overland, the wizard's circle and enchantments) come from the game
//! through [`AimWorld`]; everything about the units, squares and sides of the battle is decided here.
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use f4x_def::id::PlayerId;
use f4x_def::map::Position;
use f4x_def::rules::damage_type::{DamageType, RangedKind};
use f4x_def::rules::defs::{SpellDef, SpellSave, SpellTarget};
use f4x_def::rules::stacking::{stacking_refusal, unit_only_refusal, Carrier};
use f4x_def::rules::target_error::TargetError;
use f4x_def::rules::unit_effect::UnitEffect;
use f4x_def::rules::unit_flag::UnitFlag;
use f4x_def::rules::{SpellId, UnitKind};

use super::caster::SpellAim;
use super::error::SpellError;
use super::spell_type::{only_against, slay_allows, CityWall, SpellType};
use crate::battle::Battle;
use crate::side::Side;
use crate::unit::{BattleUnit, Fate, Status};
use f4x_def::rules::realm::Realm;
use f4x_def::rules::stacking::Stacking;

/// What the check of an aim asks of the world outside the battle.
pub trait AimWorld {
    /// The kind of the overland unit a battle unit was copied from, and whether it counts as undead
    /// there; `None` for a unit made in the battle (a conjured creature) or one whose source is gone.
    fn source_kind(&self, unit: &BattleUnit) -> Option<(UnitKind, bool)>;
    /// Why the overland unit behind a battle unit cannot be mutated (Chaos Channels), if it cannot.
    fn mutation_refusal(&self, unit: &BattleUnit) -> Option<TargetError>;
    /// The summoning circle of a player.
    fn summon_point(&self, player: PlayerId) -> Option<Position>;
    /// Whether the player keeps the global enchantment.
    fn caster_has_global(&self, player: PlayerId, spell: SpellId) -> bool;
    /// Whether the caster has confirmed a cast that would do nothing.
    fn confirmed(&self) -> bool;
    /// The rule setting: immunity to illusion protects against curses with a save.
    fn illusion_immunity_protects(&self) -> bool;
}

/// The damage types a curse's target may be immune to: the spell's `save` types, less illusion when
/// illusion immunity does not protect.
pub fn save_types(save: &SpellSave, illusion_immunity_protects: bool) -> Vec<DamageType> {
    save.types
        .iter()
        .copied()
        .filter(|t| illusion_immunity_protects || *t != DamageType::Illusion)
        .collect()
}

impl From<Stacking> for SpellError {
    fn from(why: Stacking) -> Self {
        match why {
            Stacking::AlreadyEnchanted => Self::AlreadyEnchanted,
            Stacking::NotCumulative => Self::NotCumulative,
            Stacking::AlreadyAble => Self::AlreadyAble,
        }
    }
}

impl Battle {
    /// Whether the spell may be aimed here.
    pub fn check_aim(
        &self,
        world: &dyn AimWorld,
        side: Side,
        spell: SpellId,
        def: &SpellDef,
        kind: &SpellType,
        aim: SpellAim,
    ) -> Result<(), SpellError> {
        match (def.target, aim) {
            (SpellTarget::None | SpellTarget::Battlefield, SpellAim::None) => {
                // A spell on every enemy needs one it can touch.
                let hits = |ok: &dyn Fn(&BattleUnit) -> bool| {
                    self.units
                        .iter()
                        .any(|u| u.is_active() && u.side != side && ok(u))
                };
                match kind {
                    SpellType::Slay {
                        only, all: true, ..
                    } if !hits(&|u| slay_allows(*only, u)) => {
                        return Err(SpellError::Target(TargetError::NothingToHit));
                    }
                    SpellType::Damage { all: true, .. }
                        if only_against(spell).is_some_and(|realms| {
                            !hits(&|u| u.realm.is_some_and(|r| realms.contains(&r)))
                        }) =>
                    {
                        return Err(SpellError::Target(TargetError::NothingToHit));
                    }
                    _ => {}
                }
                if let SpellType::Field(f) = kind {
                    if self.has_field(side, *f) {
                        return Err(SpellError::AlreadyUp);
                    }
                }
                Ok(())
            }
            (SpellTarget::OwnCity, SpellAim::None) => match kind {
                // Only the defender, at its own city, and not what already stands.
                SpellType::CityWall(wall) if side == Side::Defender && self.map.is_city() => {
                    let up = match wall {
                        CityWall::Stone => self.map.is_fully_walled(),
                        CityWall::Fire => self.wall_of_fire,
                        CityWall::Darkness => self.wall_of_darkness,
                    };
                    if up {
                        Err(SpellError::AlreadyUp)
                    } else {
                        Ok(())
                    }
                }
                _ => Err(SpellError::Unsupported),
            },
            (SpellTarget::Tile, SpellAim::Tile(cell)) if matches!(kind, SpellType::Disrupt) => {
                if self.map.has_wall(cell) {
                    Ok(())
                } else {
                    Err(SpellError::Target(TargetError::NoWall))
                }
            }
            (SpellTarget::Tile, SpellAim::Tile(cell)) if matches!(kind, SpellType::Vortex) => {
                if self.can_make_vortex(cell) {
                    Ok(())
                } else {
                    Err(SpellError::Target(TargetError::NoVortexRoom))
                }
            }
            (SpellTarget::Tile, SpellAim::Tile(cell))
                if matches!(kind, SpellType::EarthToMud | SpellType::CracksCall) =>
            {
                if self.map.is_playable(cell) {
                    Ok(())
                } else {
                    Err(SpellError::Target(TargetError::BadSquare))
                }
            }
            (SpellTarget::Tile, SpellAim::Tile(cell)) => {
                if self.active_units(side).count() >= self.max_side
                    || !self.is_summon_square(side, cell)
                {
                    return Err(SpellError::NoRoom);
                }
                Ok(())
            }
            (SpellTarget::AnyUnit, SpellAim::Unit(id)) if matches!(kind, SpellType::Raise) => {
                if self.active_units(side).count() >= self.max_side
                    || self.first_summon_square(side).is_none()
                {
                    return Err(SpellError::NoRoom);
                }
                if self.can_raise(world, side, id) {
                    Ok(())
                } else {
                    Err(SpellError::Target(TargetError::NothingToRaise))
                }
            }
            (SpellTarget::AnyUnit, SpellAim::Unit(id)) if matches!(kind, SpellType::Animate) => {
                if self.active_units(side).count() >= self.max_side
                    || self.first_summon_square(side).is_none()
                {
                    return Err(SpellError::NoRoom);
                }
                if self.can_animate(world, side, id) {
                    Ok(())
                } else {
                    Err(SpellError::Target(TargetError::NothingToAnimate))
                }
            }
            (
                target @ (SpellTarget::OwnUnit | SpellTarget::EnemyUnit | SpellTarget::AnyUnit),
                SpellAim::Unit(id),
            ) => {
                let u = self
                    .unit(id)
                    .filter(|u| u.is_active())
                    .ok_or(SpellError::Target(TargetError::NotInBattle))?;
                let own = u.side == side;
                if target == SpellTarget::OwnUnit && !own {
                    return Err(SpellError::Target(TargetError::NotYourUnit));
                }
                if target == SpellTarget::EnemyUnit && own {
                    return Err(SpellError::Target(TargetError::NotEnemyUnit));
                }
                // An invisible enemy that cannot be seen cannot be picked.
                if !self.seen_by(side, id) {
                    return Err(SpellError::Target(TargetError::Unseen));
                }
                // Death creatures, born so or undead, cannot be healed.
                if matches!(kind, SpellType::Heal { all: false, .. })
                    && u.realm == Some(Realm::Death)
                {
                    return Err(SpellError::Target(TargetError::Deathly));
                }
                if let SpellType::Turn { bind, .. } = kind {
                    // Binding takes fantastic creatures, Possession normal units and heroes.
                    if *bind && !u.is_fantastic() {
                        return Err(SpellError::Target(TargetError::CannotBind));
                    }
                    if !*bind && u.is_fantastic() {
                        return Err(SpellError::Target(TargetError::NotNormalUnit));
                    }
                    if u.bound_from.is_some() {
                        return Err(SpellError::Target(TargetError::AlreadyBound));
                    }
                    if self.active_units(side).count() >= self.max_side {
                        return Err(SpellError::NoRoom);
                    }
                }
                // Recall Hero takes one of the wizard's own heroes, and needs a place to send it.
                if let SpellType::Recall { anyone } = kind {
                    if !own {
                        return Err(SpellError::Target(TargetError::NotYourUnit));
                    }
                    if !anyone
                        && !world
                            .source_kind(u)
                            .is_some_and(|(kind, _)| kind == UnitKind::Hero)
                    {
                        return Err(SpellError::Target(TargetError::RecallOnlyHeroes));
                    }
                    let Some(circle) = world.summon_point(self.player_of(side)) else {
                        return Err(SpellError::Target(TargetError::NoCircle));
                    };
                    // A fight at the circle itself has nowhere to recall to (1.50 note 704).
                    if self.site == Some(circle) {
                        return Err(SpellError::Target(TargetError::CircleHere));
                    }
                }
                if matches!(kind, SpellType::Channel) {
                    if let Some(why) = world.mutation_refusal(u) {
                        return Err(SpellError::Target(why));
                    }
                }
                if self.spares(u, spell) {
                    return Err(SpellError::Target(
                        if matches!(kind, SpellType::LifeDrain) {
                            TargetError::Deathly
                        } else {
                            TargetError::NotAffected
                        },
                    ));
                }
                // A curse the target is immune to is refused before it costs anything
                // (`Combat_Spell_Target_Screen`); the types it is stopped by are in the spell's
                // data (`save` in `spells.ron`: illusion for Sorcery, death for Death).
                if let Some(save) = &def.save {
                    if u.is_immune_to(
                        &save_types(save, world.illusion_immunity_protects()),
                        &self.content.rules,
                    ) {
                        return Err(SpellError::Target(TargetError::NotAffected));
                    }
                }
                if matches!(kind, SpellType::Enchant { .. }) {
                    // A unit made in the battle has no source: a conjured creature.
                    let (unit_kind, undead) =
                        world.source_kind(u).unwrap_or((UnitKind::Summoned, false));
                    if let Some(why) =
                        unit_only_refusal(def.unit_only, unit_kind, undead || u.undead)
                    {
                        return Err(SpellError::Target(why));
                    }
                    self.check_stacking(world, self.player_of(side), spell, def, u)?;
                }
                if UnitEffect::Web.is_in(def) && u.flag(UnitFlag::NonCorporeal) {
                    return Err(SpellError::Target(TargetError::NonCorporeal));
                }
                // Warp Wood needs a missile unit, whatever its kind, and a unit is warped once.
                if matches!(kind, SpellType::WarpWood)
                    && (u.ranged_kind != RangedKind::Missile || u.ammo <= 0)
                {
                    return Err(SpellError::Target(TargetError::NeedsMissile));
                }
                if matches!(kind, SpellType::Warp { .. }) && u.warp.is_some() {
                    return Err(SpellError::Target(TargetError::AlreadyWarped));
                }
                if let Some(realms) = only_against(spell) {
                    if !u.realm.is_some_and(|r| realms.contains(&r)) {
                        return Err(SpellError::Target(TargetError::NotAffected));
                    }
                }
                if let SpellType::Slay {
                    only, all: false, ..
                } = kind
                {
                    if !slay_allows(*only, u) {
                        return Err(SpellError::Target(TargetError::NotAffected));
                    }
                }
                Ok(())
            }
            _ => Err(SpellError::Target(TargetError::WrongTarget)),
        }
    }

    /// The refusals for a lasting spell on a unit that cannot take it: it has the spell
    /// already, one the spell is not cumulative with, or the ability by its nature
    /// (`spell/stacking.rs` has the same rules for the map).
    pub fn check_stacking(
        &self,
        world: &dyn AimWorld,
        player: PlayerId,
        spell: SpellId,
        def: &SpellDef,
        unit: &BattleUnit,
    ) -> Result<(), SpellError> {
        // A web that has been broken may be cast again; the curse that is left changes nothing.
        if UnitEffect::Web.is_in(def) && !unit.held_by_web(&self.content.rules) {
            return Ok(());
        }
        let carrier = Carrier {
            has_spell: &|id| unit.enchantments.contains(&id),
            has_ability: &|a| unit.has(a),
            effects: unit.effects(&self.content.rules),
        };
        match stacking_refusal(spell, def, &carrier, &|id| {
            world.caster_has_global(player, id)
        }) {
            Some(why) if !(why.is_pointless() && world.confirmed()) => Err(why.into()),
            _ => Ok(()),
        }
    }

    /// Whether the dead unit may be raised as undead by `side`.
    pub fn can_animate(&self, world: &dyn AimWorld, side: Side, id: usize) -> bool {
        let Some(u) = self.unit(id) else {
            return false;
        };
        u.side == side
            && u.status == Status::Dead
            && u.fate() != Fate::Gone
            // Death Immunity keeps a unit from rising as undead (1.50 notes 496, 498).
            && !u.is_immune_to(&[DamageType::Death], &self.content.rules)
            && world
                .source_kind(u)
                .is_some_and(|(kind, _)| kind != UnitKind::Hero)
    }

    /// Whether the dead unit may be raised whole (a normal unit, not a creature or undead).
    pub fn can_raise(&self, world: &dyn AimWorld, side: Side, id: usize) -> bool {
        self.unit(id)
            .is_some_and(|u| !u.is_fantastic() && !u.undead)
            && self.can_animate(world, side, id)
    }
}
