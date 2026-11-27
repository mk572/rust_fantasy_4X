//! Unit instances: components on map entities, and stats computed from their
//! definition, abilities and experience level through the modifier engine.
//!
//! The rules of units are in [`UnitView`]: a read handle that borrows only the world,
//! the content and the players (for the owner's retorts and global spells), with the queries as
//! its methods. They are spread over the files of the unit rules (`spell/temp_effect.rs`,
//! `unit/undead.rs`, `unit/experience.rs`, `spell/global_enchantment.rs`), each an `impl UnitView` block. What changes units
//! is a function over `&mut hecs::World` (`spawn`, `experience::gain_xp`, the temporary effects).
//! `World` has thin methods that make a handle and ask it (`World::units`).

pub mod experience;
pub mod handling;
pub(crate) mod healing;
pub(crate) mod kept;
pub mod ledger;
pub mod mobility;
pub mod movement;
pub mod mutation;
pub mod order;
pub mod pathing;
pub mod sanity;
pub mod scouting;
pub mod stasis;
pub mod transport;
pub mod undead;

use crate::rules::damage_type::DamageTypes;
use crate::rules::unit_flag::{UnitFlag, UnitFlags};
use crate::rules::Ability;
use f4x_def::half_int::HalfInt;

use crate::map::Position;
use crate::player::{Owner, PlayerId};
use crate::world::serial::Serial;
use f4x_def::content::Content;

use crate::rules::stats::{compute, compute_half, passive_modifiers, Modifier, Source};
use crate::rules::{LevelPathDef, ModOp, Stat, UnitDef, UnitId, UnitKind};
use crate::world::{Event, World};

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

/// The quality of a unit's weapons, set where it is trained (`World::city_weapon_quality`):
/// 0 normal, 1 magic (an alchemists guild or the Alchemy retort), 2 mithril, 3 adamantium.
/// Mithril and adamantium add to the unit's attacks and defence in battle; every quality above
/// normal adds a plus to hit and makes the weapons magical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WeaponQuality(pub u8);

pub(crate) use f4x_def::rules::hook_name::HEROISM;

pub const WEAPON_MAGIC: u8 = 1;
pub const WEAPON_MITHRIL: u8 = 2;
pub const WEAPON_ADAMANTIUM: u8 = 3;

/// Movement left this turn (a half move is 0.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MovesLeft(pub HalfInt);

pub use order::{Errand, Order, OrderEnd, OrderError, Scouting, Trip};

/// What a player has told an idle unit to do about being asked for orders (the main screen's
/// Done and Patrol buttons): `Done` skips it for this turn, `Patrol` until it moves one step
/// (`Command::MoveStack`) or another player's unit comes into its sight
/// (`World::end_patrols_on_sight`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Standby {
    Done,
    Patrol,
}

/// The units of other players a patrol knew of when it was set: they do not end it. Read only
/// while the unit's standby is `Patrol`, and written anew each time that is set.
#[derive(Debug, Clone, Default)]
pub(crate) struct PatrolSeen(pub Vec<crate::world::serial::Serial>);

impl UnitView<'_> {
    /// The unit's standby, if it has one.
    pub fn unit_standby(&self, unit: hecs::Entity) -> Option<Standby> {
        self.ecs.get::<&Standby>(unit).ok().map(|s| *s)
    }
}

pub use f4x_def::settings::{MAX_STACK, MAX_TILE_LIMIT, TILE_SANITY_LIMIT};

/// The unit rules' view of the game: the world, the content and the players.
#[derive(Clone, Copy)]
pub struct UnitView<'a> {
    pub(crate) ecs: &'a hecs::World,
    pub(crate) content: &'a Content,
}

impl<'a> UnitView<'a> {
    pub fn new(ecs: &'a hecs::World, content: &'a Content) -> Self {
        UnitView { ecs, content }
    }

    /// A player's magic (retorts and global spells), for what the owner adds to its units.
    pub(crate) fn wizard(&self, player: PlayerId) -> Option<crate::wizard::entity::Wizard<'a>> {
        crate::wizard::entity::find(self.ecs, player)
    }

    /// Does `player` keep a global spell with this effect going?
    pub(crate) fn has_global_effect(
        &self,
        player: PlayerId,
        effect: crate::spell::global_enchantment::GlobalEffect,
    ) -> bool {
        self.wizard(player).is_some_and(|m| {
            crate::spell::global_enchantment::magic_has_effect(&m, &self.content.rules, effect)
        })
    }

    /// A wizard's fame as the rules count it: what they have earned, what their Legendary heroes
    /// add, and ten more while Just Cause is kept going.
    ///
    /// A neutral race player (task 522) has no wizard and earns no fame: it has the average of
    /// the living wizards' (rounded down), so what it is offered keeps pace with the game.
    pub fn fame(&self, player: PlayerId) -> i32 {
        if let Some(average) = self.average_wizard_fame(player) {
            return average;
        }
        let earned = self.wizard(player).map_or(0, |w| w.fame());
        let legendary: i32 = self
            .hero_units(player)
            .into_iter()
            .filter_map(|h| {
                let tier = self
                    .ecs
                    .get::<&crate::hero::Hero>(h)
                    .ok()?
                    .abilities
                    .legendary;
                Some(
                    self.content
                        .rules
                        .hero_value("legendary", tier, self.unit_level(h)),
                )
            })
            .sum();
        earned
            + legendary
            + if self.has_global_effect(
                player,
                crate::spell::global_enchantment::GlobalEffect::JustCause,
            ) {
                self.content.rules.config.renown.just_cause_fame
            } else {
                0
            }
    }

    /// The average fame of the living wizards, for a player that has no wizard and is the
    /// neutral player of a race; `None` for anyone else.
    fn average_wizard_fame(&self, player: PlayerId) -> Option<i32> {
        use crate::player::race_player::FIRST_RACE_PLAYER;
        if player == crate::player::MONSTERS
            || player.0 < FIRST_RACE_PLAYER
            || self.wizard(player).is_some()
        {
            return None;
        }
        let wizards: Vec<PlayerId> = crate::player::ids(self.ecs)
            .into_iter()
            .filter(|&p| self.wizard(p).is_some())
            .filter(|&p| {
                crate::player::find_entity(self.ecs, p)
                    .is_some_and(|e| self.ecs.get::<&crate::player::Defeat>(e).is_err())
            })
            .collect();
        let count = i32::try_from(wizards.len()).ok().filter(|&n| n > 0)?;
        Some(
            wizards
                .iter()
                .map(|&w| self.fame(w))
                .sum::<i32>()
                .div_euclid(count),
        )
    }

    /// Research points the wizard's Sage heroes add.
    pub fn sage_research(&self, player: PlayerId) -> i32 {
        self.hero_units(player)
            .into_iter()
            .filter_map(|h| {
                let tier = self.ecs.get::<&crate::hero::Hero>(h).ok()?.abilities.sage;
                Some(
                    self.content
                        .rules
                        .hero_value("sage", tier, self.unit_level(h)),
                )
            })
            .sum()
    }

    /// The entity with this creation number, if it is still alive.
    pub fn entity_of_serial(&self, serial: Serial) -> Option<hecs::Entity> {
        self.ecs
            .query::<&Serial>()
            .iter()
            .find(|(_, s)| **s == serial)
            .map(|(e, _)| e)
    }

    /// Does any wizard keep a global spell with this effect going?
    pub(crate) fn any_global_effect(
        &self,
        effect: crate::spell::global_enchantment::GlobalEffect,
    ) -> bool {
        let spells = crate::spell::instance::Spells::new(self.ecs);
        self.content.rules.spells.iter().any(|(id, def)| {
            effect.is_in(def) && spells.has(crate::spell::instance::SpellTarget::Global, id)
        })
    }

    /// Planar Seal closes every junction between the planes while anyone keeps it up.
    pub fn planar_seal_active(&self) -> bool {
        self.any_global_effect(crate::spell::global_enchantment::GlobalEffect::PlanarSeal)
    }

    /// An entity's creation number (entities without one sort first, by id).
    pub fn serial_of(&self, entity: hecs::Entity) -> Serial {
        self.ecs.get::<&Serial>(entity).map_or(Serial(0), |s| *s)
    }

    /// Sort entities oldest first.
    pub fn sort_by_serial(&self, entities: &mut [hecs::Entity]) {
        entities.sort_by_cached_key(|&e| (self.serial_of(e), e.id()));
    }

    /// The kind of unit this is, as the rules know it.
    pub fn unit_kind_id(&self, unit: hecs::Entity) -> Option<UnitId> {
        self.ecs.get::<&Unit>(unit).ok().map(|u| u.0)
    }

    pub fn unit_def(&self, unit: hecs::Entity) -> Option<&'a UnitDef> {
        let id = self.ecs.get::<&Unit>(unit).ok()?.0;
        Some(self.content.rules.units.get(id))
    }

    /// The quality of the unit's weapons (see [`WeaponQuality`]).
    pub fn unit_weapon_quality(&self, unit: hecs::Entity) -> u8 {
        self.ecs.get::<&WeaponQuality>(unit).map_or(0, |w| w.0)
    }

    pub fn unit_owner(&self, unit: hecs::Entity) -> Option<PlayerId> {
        crate::player::id_of(self.ecs, self.unit_owner_entity(unit)?)
    }

    /// The player entity a unit belongs to.
    pub fn unit_owner_entity(&self, unit: hecs::Entity) -> Option<hecs::Entity> {
        self.ecs.get::<&Owner>(unit).ok().map(|o| o.0)
    }

    pub fn unit_position(&self, unit: hecs::Entity) -> Option<Position> {
        self.ecs.get::<&Position>(unit).ok().map(|p| *p)
    }

    pub fn unit_has_ability(&self, unit: hecs::Entity, ability: Ability) -> bool {
        self.unit_def(unit).is_some() && self.abilities_shared(unit).contains(&ability)
    }

    /// The flags the abilities set (`flags` in `abilities.ron`).
    pub fn flags_of(&self, abilities: &[Ability]) -> UnitFlags {
        let mut flags = UnitFlags::NONE;
        for &a in abilities {
            for &f in &self.content.rules.abilities.get(a).flags {
                flags.insert(f);
            }
        }
        flags
    }

    /// Whether one of the unit's abilities (its own, its enchantments' and its items') sets the
    /// flag.
    pub fn unit_has_flag(&self, unit: hecs::Entity, flag: UnitFlag) -> bool {
        self.unit_def(unit).is_some() && self.flags_of(&self.abilities_shared(unit)).contains(flag)
    }

    /// Whether the unit can found a city.
    pub fn unit_is_settler(&self, unit: hecs::Entity) -> bool {
        self.unit_has_ability(unit, Ability::new(12 /* create_outpost */))
    }

    pub(crate) fn unit_level_path(&self, unit: hecs::Entity) -> Option<&'a LevelPathDef> {
        self.unit_def(unit)?
            .level_path
            .map(|p| self.content.rules.level_paths.get(p))
    }

    /// The level the unit's experience points alone give, 0-based (0 without a level path).
    pub fn unit_xp_level(&self, unit: hecs::Entity) -> usize {
        let Some(def) = self.unit_def(unit) else {
            return 0;
        };
        let xp = self.ecs.get::<&Experience>(unit).map_or(0, |x| x.0);
        self.content.rules.xp_level(def, xp)
    }

    /// The lasting spells on a unit, with the wizard who cast each.
    pub fn unit_enchantments(&self, unit: hecs::Entity) -> Vec<(crate::rules::SpellId, PlayerId)> {
        crate::spell::instance::Spells::new(self.ecs).unit_enchantments(unit)
    }

    /// Whether the Heroism spell is on the unit.
    pub fn unit_has_heroism(&self, unit: hecs::Entity) -> bool {
        self.unit_enchantments(unit)
            .iter()
            .any(|&(spell, _)| self.content.rules.spells.get(spell).has_hook(HEROISM))
    }

    /// Experience level, 0-based, from the unit's level path (0 without one): the level its
    /// experience gives, raised to Elite by Heroism, then by one each for its owner's Warlord
    /// retort and Crusade spell (which leave the undead and creatures alone). Battles use the
    /// same level as the map (the original's battle code ignores Warlord, Crusade and xp when
    /// Heroism is on: B9 in `help-vs-implementation-conflicts.md`, we follow the help).
    pub fn unit_level(&self, unit: hecs::Entity) -> usize {
        if self.unit_level_path(unit).is_none() {
            return 0;
        }
        self.level_with(unit, self.unit_has_heroism(unit))
    }

    /// The level the unit has with Heroism on it or without.
    fn level_with(&self, unit: hecs::Entity, heroism: bool) -> usize {
        let Some(path) = self.unit_level_path(unit) else {
            return 0;
        };
        let mut level = self.unit_xp_level(unit);
        let lifted = self.content.rules.config.renown.heroism_level;
        if level < lifted && heroism {
            level = lifted;
        }
        if let Some(owner) = self.unit_owner(unit) {
            if let Some(m) = self.wizard(owner) {
                level +=
                    usize::try_from(m.stat(&self.content.rules, crate::rules::Stat::UnitLevels, 0))
                        .unwrap_or(0);
            }
            let normal = self.unit_def(unit).is_some_and(|d| {
                matches!(
                    d.kind,
                    crate::rules::UnitKind::Normal | crate::rules::UnitKind::Hero
                )
            });
            if normal
                && self.has_global_effect(
                    owner,
                    crate::spell::global_enchantment::GlobalEffect::Crusade,
                )
                && !self.unit_is_undead(unit)
            {
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
        if let Ok(hero) = self.ecs.get::<&crate::hero::Hero>(unit) {
            let magic = crate::battle::world::ranged_kind_of(
                &self.unit_abilities(unit),
                &self.content.rules,
            ) == crate::battle::RangedKind::Magic;
            let formulas = &self.content.rules;
            let before = hero.abilities.modifiers(formulas, from, magic);
            for m in hero.abilities.modifiers(formulas, to, magic) {
                let had = before
                    .iter()
                    .find(|b| b.stat == m.stat)
                    .map_or(HalfInt::ZERO, |b| b.value);
                if m.value != had {
                    mods.push(Modifier::amount(m.stat, m.op, m.value - had, m.source));
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
            .map(|p| self.content.rules.level_paths.get(p))
        else {
            return Vec::new();
        };
        let mut mods = Vec::new();
        for i in levels {
            if let Some(effects) = path.level(i) {
                // A level's attack plus only counts for a unit that has the attack
                // (`Battle_Unit_Level_Stats`).
                mods.extend(
                    passive_modifiers(effects, Source::Level(i as u8))
                        .into_iter()
                        .filter(|m| {
                            !matches!(m.stat, Stat::Melee | Stat::Ranged)
                                || def.stats.get(&m.stat).map_or(0, |v| v.floor()) > 0
                        }),
                );
            }
        }
        mods
    }

    /// Every modifier currently affecting the unit: its abilities and level effects. Shared: kept
    /// for a computer wizard's turn (see `unit::kept`).
    pub(crate) fn modifiers_shared(&self, unit: hecs::Entity) -> std::rc::Rc<Vec<Modifier>> {
        if let Some(kept) = kept::modifiers(self.ecs, unit) {
            if kept::checking() {
                assert_eq!(
                    *kept,
                    self.compute_modifiers(unit),
                    "kept modifiers of {unit:?}"
                );
            }
            return kept;
        }
        let mods = std::rc::Rc::new(self.compute_modifiers(unit));
        kept::keep_modifiers(self.ecs, unit, &mods);
        mods
    }

    /// The modifiers the unit has of its own: its abilities', its lasting spells', its items' and
    /// the temporary ones. Not its level's, a hero's own abilities' or its owner's global
    /// enchantments': a battle works those out from the unit's experience and the ids of the
    /// wizard's enchantments (`f4x-battle`'s unit/stats.rs).
    pub fn own_modifiers(&self, unit: hecs::Entity) -> Vec<Modifier> {
        if self.unit_def(unit).is_none() {
            return Vec::new();
        }
        let mut mods = self.ability_modifiers(unit);
        mods.extend(self.spell_modifiers(unit));
        mods.extend(self.items().modifiers(unit));
        mods.extend(self.temp_modifiers(unit));
        mods
    }

    /// What the unit's lasting spells and the spells of its items do to its stats.
    fn spell_modifiers(&self, unit: hecs::Entity) -> Vec<Modifier> {
        let rules = &self.content.rules;
        let mut mods = Vec::new();
        for (spell, _) in crate::spell::instance::Spells::new(self.ecs).unit_enchantments(unit) {
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
        mods
    }

    fn compute_modifiers(&self, unit: hecs::Entity) -> Vec<Modifier> {
        let _rules = &self.content.rules;
        if self.unit_def(unit).is_none() {
            return Vec::new();
        }
        let mut mods = Vec::new();
        mods.extend(self.ability_modifiers(unit));
        let level = self.unit_level(unit);
        if let Ok(hero) = self.ecs.get::<&crate::hero::Hero>(unit) {
            let magic = crate::battle::world::ranged_kind_of(
                &self.unit_abilities(unit),
                &self.content.rules,
            ) == crate::battle::RangedKind::Magic;
            mods.extend(hero.abilities.modifiers(&self.content.rules, level, magic));
        }
        mods.extend(self.level_modifiers(unit, 0..level + 1));
        mods.extend(self.spell_modifiers(unit));
        mods.extend(self.items().modifiers(unit));
        mods.extend(self.temp_modifiers(unit));
        mods.extend(self.global_unit_modifiers(unit));
        mods
    }

    pub fn unit_stat(&self, unit: hecs::Entity, stat: Stat) -> i32 {
        if stat == Stat::Moves {
            return self.unit_moves(unit).floor();
        }
        let base = self
            .unit_def(unit)
            .and_then(|d| d.stats.get(&stat).map(|v| v.floor()))
            .unwrap_or(0);
        let value = compute(stat, base, &self.modifiers_shared(unit));
        // Charm of Life comes last, on the hits with all the other bonuses in.
        if stat == Stat::MaxHp
            && self.unit_owner(unit).is_some_and(|o| {
                self.has_global_effect(
                    o,
                    crate::spell::global_enchantment::GlobalEffect::CharmOfLife,
                )
            })
        {
            return f4x_def::rules::global_effect::charm_of_life_hits(value);
        }
        value
    }

    /// The dice of an attack stat that are made of damage types (what a Flame Blade or a Flaming
    /// weapon adds): each added amount with its types. They are part of `unit_stat`, not on top.
    pub fn unit_typed_dice(&self, unit: hecs::Entity, stat: Stat) -> Vec<(i32, DamageTypes)> {
        self.modifiers_shared(unit)
            .iter()
            .filter(|m| m.stat == stat && m.op == ModOp::Add && !m.types.is_empty())
            .map(|m| (m.value.floor(), m.types))
            .collect()
    }

    /// The `Moves` stat of the unit with everything on it, in halves.
    pub fn unit_moves(&self, unit: hecs::Entity) -> HalfInt {
        let base = self
            .unit_def(unit)
            .and_then(|d| d.stats.get(&Stat::Moves).copied())
            .unwrap_or(HalfInt::ZERO);
        compute_half(Stat::Moves, base, &self.modifiers_shared(unit))
    }

    /// Full movement allowance.
    pub fn unit_max_moves(&self, unit: hecs::Entity) -> HalfInt {
        let moves = self.unit_moves(unit).max(HalfInt::ZERO);
        // Wind Mastery (our rule): ships and fliers of a wizard who keeps it go half as far again;
        // those of a wizard without it, while anyone else keeps it, go half as far. Nothing stacks.
        if self.is_ship_or_flier(unit) {
            if let Some(owner) = self.unit_owner(unit) {
                if self.has_global_effect(
                    owner,
                    crate::spell::global_enchantment::GlobalEffect::WindMastery,
                ) {
                    return moves.percent(150);
                }
                if self
                    .any_global_effect(crate::spell::global_enchantment::GlobalEffect::WindMastery)
                {
                    return moves.percent(50);
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
            || self.unit_has_flag(unit, UnitFlag::Flies)
    }

    pub fn unit_moves_left(&self, unit: hecs::Entity) -> HalfInt {
        self.ecs
            .get::<&MovesLeft>(unit)
            .map_or(HalfInt::ZERO, |m| m.0)
    }

    /// How far the unit sees on the map, in squares (`map::fog::in_sight`): its `Sight` stat, at
    /// least 2.5 (the original's scouting level 2) for a flier or a Wind Walker
    /// (`Update_Scouted_And_Contacted`).
    pub fn unit_sight(&self, unit: hecs::Entity) -> HalfInt {
        let base = self
            .unit_def(unit)
            .and_then(|d| d.stats.get(&Stat::Sight).copied())
            .unwrap_or(HalfInt::ZERO);
        let sight = compute_half(Stat::Sight, base, &self.modifiers_shared(unit))
            .max(crate::map::fog::sight_for(1));
        let flies = self
            .unit_def(unit)
            .is_some_and(|d| d.movement == crate::rules::MovementType::Flying)
            || self.unit_has_flag(unit, UnitFlag::Flies)
            || self.unit_has_flag(unit, UnitFlag::WindWalking);
        if flies {
            sight.max(crate::map::fog::sight_for(2))
        } else {
            sight
        }
    }

    /// The unit's experience points.
    pub fn unit_experience(&self, unit: hecs::Entity) -> u32 {
        self.ecs.get::<&Experience>(unit).map_or(0, |x| x.0)
    }

    /// `(current, maximum)` hit points over all figures.
    pub fn unit_hits(&self, unit: hecs::Entity) -> (u32, u32) {
        let max = (self.unit_stat(unit, Stat::MaxHp).max(0)
            * self.unit_stat(unit, Stat::Figures).max(0)) as u32;
        let hurt = self.ecs.get::<&Hurt>(unit).map_or(0, |h| h.0);
        (max.saturating_sub(hurt), max)
    }

    /// How quickly the unit gets to a square (`unit::mobility`): its movement rate times what
    /// its kind of movement and its abilities do for it.
    pub fn unit_mobility(&self, unit: hecs::Entity) -> f32 {
        let Some(def) = self.unit_def(unit) else {
            return 0.0;
        };
        mobility::unit_mobility(
            self.unit_stat(unit, Stat::Moves),
            def.movement,
            |a| self.unit_has_ability(unit, a),
            &self.content.rules.config.abilities,
        )
    }

    /// How quickly a party of units gets to a square: as quickly as the slowest, unless one of
    /// them carries the whole stack (pathfinding, wind walking).
    pub fn party_mobility(&self, units: &[hecs::Entity]) -> f32 {
        let members: Vec<(f32, i32)> = units
            .iter()
            .map(|&u| (self.unit_mobility(u), self.unit_stat(u, Stat::Moves)))
            .collect();
        let carried = units.iter().any(|&u| {
            mobility::carries_stack(
                |a| self.unit_has_ability(u, a),
                &self.content.rules.config.abilities,
            )
        });
        let flying = self.content.rules.config.abilities.flying_movement_pct as f32 / 100.0;
        mobility::party_mobility(&members, carried, flying)
    }

    /// What the unit adds to a battle at full health: the one measure of strength. A unit with a
    /// row in the rating file is worth its measured combat rating at its experience level
    /// (`Rules::combat_power`), times what it carries beyond that level: the formula of
    /// `battle::power` on its stats and abilities now over the formula on the stats and abilities of
    /// its level alone (an item or a spell raises it, a curse lowers it; a hero's rolled abilities are in its rating).
    /// A unit with no row is worth the formula. Kept for a computer wizard's turn (see
    /// `unit::kept`).
    pub fn unit_power(&self, unit: hecs::Entity) -> f32 {
        if let Some(kept) = kept::power(self.ecs, unit) {
            if kept::checking() {
                assert_eq!(kept, self.compute_power(unit), "kept power of {unit:?}");
            }
            return kept;
        }
        let power = self.compute_power(unit);
        kept::keep_power(self.ecs, unit, power);
        power
    }

    fn compute_power(&self, unit: hecs::Entity) -> f32 {
        let now = crate::battle::power::unit_power(
            |s| self.unit_stat(unit, s),
            |a| self.unit_has_ability(unit, a),
        );
        let Some(def) = self.unit_def(unit) else {
            return now;
        };
        let level = self.unit_level(unit);
        let Some(rated) = self.content.rules.combat_power(def, level) else {
            return now;
        };
        // The stats and abilities of the level alone: the row's own, the unit's innate abilities
        // and the bonuses of its level path.
        let mut mods = self.level_modifiers(unit, 0..level + 1);
        for &id in &def.abilities {
            mods.extend(passive_modifiers(
                &self.content.rules.abilities.get(id).effects,
                Source::Ability(id),
            ));
        }
        // A hero's rating was measured with the abilities the game rolls for it (task 655), so
        // they are part of the level alone: only items, spells and curses move it from there.
        let mut rolled = Vec::new();
        if let Ok(hero) = self.ecs.get::<&crate::hero::Hero>(unit) {
            let magic = crate::battle::world::ranged_kind_of(
                &self.unit_abilities(unit),
                &self.content.rules,
            ) == crate::battle::RangedKind::Magic;
            mods.extend(hero.abilities.modifiers(&self.content.rules, level, magic));
            rolled = hero.abilities.ids();
        }
        let base = crate::battle::power::unit_power(
            |s| compute(s, def.stats.get(&s).map_or(0, |v| v.floor()), &mods),
            |a| def.abilities.contains(&a) || rolled.contains(&a),
        );
        if base > 0.0 {
            rated * now / base
        } else {
            rated
        }
    }

    /// What the unit would add to a battle with `spell` lasting on it, at full health: its
    /// [`UnitView::unit_power`] times what the formula of `battle::power` makes of its stats and
    /// abilities with the spell's modifiers and granted abilities added over what it has now. A
    /// spell that adds nothing (an ability the unit has already) gives its present power. This is
    /// the effect of the spell on this unit, for the AI to weigh a buff against another buff or
    /// another unit.
    pub fn unit_power_with_spell(&self, unit: hecs::Entity, spell: crate::rules::SpellId) -> f32 {
        let now = self.unit_power(unit);
        let Some(def) = self.unit_def(unit) else {
            return now;
        };
        let spell_def = self.content.rules.spells.get(spell);
        // Heroism lifts the level: what it adds is the rating of the higher level over the
        // present one's (a unit already as high gains nothing).
        if spell_def.has_hook(HEROISM) {
            let level = self.unit_level(unit);
            let lifted = self.level_with(unit, true);
            let rules = &self.content.rules;
            return match (
                rules.combat_power(def, level),
                rules.combat_power(def, lifted),
            ) {
                (Some(now_rated), Some(lifted_rated)) if lifted > level && now_rated > 0.0 => {
                    now * lifted_rated / now_rated
                }
                _ => now,
            };
        }
        let effects = &spell_def.effects;
        let mut mods = (*self.modifiers_shared(unit)).clone();
        mods.extend(passive_modifiers(effects, Source::Spell(spell)));
        let granted: Vec<Ability> = effects
            .iter()
            .filter_map(|r| match r.effect {
                f4x_def::rules::effects::Effect::GrantAbility(a) => Some(a),
                _ => None,
            })
            .collect();
        let formula = |mods: &[Modifier], extra: &[Ability]| {
            crate::battle::power::unit_power(
                |s| compute(s, def.stats.get(&s).map_or(0, |v| v.floor()), mods),
                |a| self.unit_has_ability(unit, a) || extra.contains(&a),
            )
        };
        let before = formula(&self.modifiers_shared(unit), &[]);
        if before <= 0.0 {
            return now;
        }
        now * formula(&mods, &granted) / before
    }

    /// Units standing on `pos`, in a stable order.
    pub fn units_at(&self, pos: Position) -> Vec<hecs::Entity> {
        let mut out: Vec<_> = self
            .ecs
            .query::<(&Unit, &Position)>()
            .iter()
            .filter(|(_, (_, p))| **p == pos)
            .map(|(e, _)| e)
            .collect();
        self.sort_by_serial(&mut out);
        out
    }

    /// All units, in no particular order (for callers that filter before they sort).
    pub fn unsorted_units(&self) -> Vec<hecs::Entity> {
        self.ecs.query::<&Unit>().iter().map(|(e, _)| e).collect()
    }

    /// All units, in a stable order.
    pub fn all_units(&self) -> Vec<hecs::Entity> {
        let mut out: Vec<_> = self.ecs.query::<&Unit>().iter().map(|(e, _)| e).collect();
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
        let basic = self.unit_def(unit).is_some_and(|d| {
            d.kind == UnitKind::Normal
                && half.units.iter().any(|u| d.id.ends_with(&format!("_{u}")))
        });
        !basic || self.serial_of(unit).0.is_multiple_of(2)
    }

    /// Food a unit eats per turn: the `food` of its row in the unit table, or nothing when it is
    /// undead, unpaid or free by the half-upkeep setting.
    pub fn unit_food(&self, unit: hecs::Entity) -> HalfInt {
        if self.unit_is_undead(unit) || self.unit_is_unpaid(unit) || !self.pays_upkeep(unit) {
            return HalfInt::ZERO;
        }
        self.unit_def(unit).map_or(HalfInt::ZERO, |d| d.upkeep.food)
    }

    /// Food these units eat together per turn.
    pub fn food_of(&self, units: &[hecs::Entity]) -> HalfInt {
        units.iter().map(|&u| self.unit_food(u)).sum()
    }

    /// Whether the owner pays nothing for the unit (see [`Unpaid`]).
    pub fn unit_is_unpaid(&self, unit: hecs::Entity) -> bool {
        self.ecs.get::<&Unpaid>(unit).is_ok()
    }

    /// Gold a unit costs per turn (heroes with the noble trait cost nothing).
    pub fn unit_gold_upkeep(&self, unit: hecs::Entity) -> HalfInt {
        let Some(def) = self.unit_def(unit) else {
            return HalfInt::ZERO;
        };
        if self.unit_is_undead(unit) || self.unit_is_unpaid(unit) || !self.pays_upkeep(unit) {
            return HalfInt::ZERO;
        }
        let gold = match def.kind {
            UnitKind::Normal => def.upkeep.gold,
            UnitKind::Hero if !self.unit_has_ability(unit, Ability::new(96 /* noble */)) => {
                def.upkeep.gold
            }
            _ => HalfInt::ZERO,
        };
        // A computer wizard's difficulty cuts its units' upkeep.
        let cut = self
            .unit_owner(unit)
            .and_then(|o| crate::player::component::<crate::player::Bonus>(self.ecs, o))
            .map_or(100, |b| b.unit_upkeep);
        crate::player::Bonus::of_half(cut, gold)
    }

    /// The army ledger of a player; `None` for an id that is no player's.
    fn army(&self, player: PlayerId) -> Option<hecs::Ref<'a, ledger::ArmyLedger>> {
        let entity = crate::player::find_entity(self.ecs, player)?;
        self.ecs.get::<&ledger::ArmyLedger>(entity).ok()
    }

    /// A player's units, in the order of `all_units`.
    pub fn player_units(&self, player: PlayerId) -> Vec<hecs::Entity> {
        self.army(player)
            .map_or_else(Vec::new, |army| army.living().collect())
    }

    /// The record of the units that left a player's army, in the order they did.
    pub fn departed_units(&self, player: PlayerId) -> Vec<ledger::DepartedUnit> {
        self.army(player)
            .map_or_else(Vec::new, |army| army.departed().to_vec())
    }

    /// How many units a player has (what `player_units` would count, without the list).
    pub fn player_unit_count(&self, player: PlayerId) -> usize {
        self.army(player).map_or(0, |army| army.len())
    }

    /// Whether the unit counts towards a city's garrison for keeping order.
    pub fn is_garrison(&self, unit: hecs::Entity) -> bool {
        self.unit_def(unit)
            .is_some_and(|d| matches!(d.kind, UnitKind::Normal | UnitKind::Hero))
    }
}

/// Create a unit with full moves and health.
pub(crate) fn spawn(
    ecs: &mut hecs::World,
    content: &Content,
    serial: Serial,
    owner: hecs::Entity,
    pos: Position,
    unit: UnitId,
) -> hecs::Entity {
    let e = ecs.spawn((
        serial,
        Unit(unit),
        Owner(owner),
        pos,
        Hurt(0),
        Experience(0),
    ));
    let moves = UnitView::new(ecs, content).unit_max_moves(e);
    crate::world::put_in(ecs, e, MovesLeft(moves));
    ledger::enlist(ecs, e);
    e
}

/// Take every part of a unit off an entity, which stays: what a hero's entry loses when the hero
/// leaves service. Every component a unit can carry is listed here, except `Serial` and
/// `Experience`, which are a hero's own.
pub(crate) fn strip_body(ecs: &mut hecs::World, entity: hecs::Entity) {
    use crate::world::strip_in;
    strip_in::<Unit>(ecs, entity);
    strip_in::<Owner>(ecs, entity);
    strip_in::<Position>(ecs, entity);
    strip_in::<Hurt>(ecs, entity);
    strip_in::<MovesLeft>(ecs, entity);
    strip_in::<Unpaid>(ecs, entity);
    strip_in::<WeaponQuality>(ecs, entity);
    strip_in::<Standby>(ecs, entity);
    strip_in::<PatrolSeen>(ecs, entity);
    strip_in::<crate::spell::temp_effect::TempEffects>(ecs, entity);
    strip_in::<undead::Undead>(ecs, entity);
    strip_in::<stasis::Stasis>(ecs, entity);
    strip_in::<transport::Aboard>(ecs, entity);
    strip_in::<transport::Marooned>(ecs, entity);
    strip_in::<transport::Bridge>(ecs, entity);
}

impl World {
    /// The unit rules' view of the game: the world, the content and the players. The methods
    /// below ask it.
    pub fn units(&self) -> UnitView<'_> {
        UnitView::new(&self.ecs, &self.content)
    }

    /// Create a unit with full moves and health, and let its owner see around it.
    pub fn spawn_unit(&mut self, owner: PlayerId, pos: Position, unit: UnitId) -> hecs::Entity {
        let serial = self.take_serial();
        let player = self
            .player_entity(owner)
            .expect("a unit is spawned for a player that exists");
        let e = spawn(&mut self.ecs, &self.content, serial, player, pos, unit);
        self.watch_tile(pos);
        let sight = self.units().unit_sight(e);
        if owner != crate::player::MONSTERS {
            self.explore(owner, pos, sight);
        }
        e
    }

    /// The unit sheet's Dismiss button: the unit leaves its owner's service for good. A hero
    /// is let go (it can be hired again, with its experience) and its items go to the armory.
    pub fn dismiss_unit(&mut self, unit: hecs::Entity) -> Vec<Event> {
        let Some(owner) = self.units().unit_owner(unit) else {
            return Vec::new();
        };
        let Some(def) = self.units().unit_def(unit) else {
            return Vec::new();
        };
        if def.kind == crate::rules::UnitKind::Hero {
            self.pass_items(unit, owner);
        }
        self.remove_unit(unit, crate::hero::Fate::Released);
        vec![Event::UnitsDisbanded {
            player: owner,
            units: 1,
        }]
    }
}
