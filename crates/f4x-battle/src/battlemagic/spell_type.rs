//! What a spell does in a battle, read from its definition: the type of a spell, the hook of the
//! battle effects, the menu and legality records. Casting itself is in `f4x-core`'s `battle/spell.rs`.
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use f4x_def::rules::counter_effect::{CounterEffect, COUNTER_EFFECT};
use f4x_def::rules::damage_type::{DamageType, DamageTypes};
use f4x_def::rules::defs::{CastContext, Duration, SpellDef, SpellSave, SpellTarget};
use f4x_def::rules::effects::{Condition, DamageTraits, Effect, SlayOnly, TargetSel, Trigger};
use f4x_def::rules::hook_name::{HEROISM, RECALL, RECALL_ANYONE, RECALL_HERO, WALL};
use f4x_def::rules::realm::Realm;
use f4x_def::rules::stats::{ModOp, Modifier, Source, Stat};
use f4x_def::rules::unit_effect::{UnitEffect, UNIT_EFFECT};
use f4x_def::rules::{Ability, SpellId, UnitId};

use super::caster::SpellAim;
use super::field::{Field, BATTLEFIELD};
use crate::unit::WarpKind;

/// The changes of one of Warp Creature's three results.
pub fn warp_mods(kind: WarpKind, spell: SpellId) -> Vec<Modifier> {
    let source = Source::Spell(spell);
    match kind {
        WarpKind::Attack => vec![
            Modifier::new(Stat::Melee, ModOp::Percent, -50, source),
            Modifier::new(Stat::Ranged, ModOp::Percent, -50, source),
        ],
        WarpKind::Defense => vec![Modifier::new(Stat::Defense, ModOp::Percent, -50, source)],
        WarpKind::Resistance => vec![Modifier::new(Stat::Resistance, ModOp::Set, 0, source)],
    }
}

/// The walls a defender can raise at its city in a battle (`Wall_Rise`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CityWall {
    Stone,
    Fire,
    Darkness,
}

/// What a spell does in a battle, from its effects.
#[derive(Debug, Clone, PartialEq)]
pub enum SpellType {
    Damage {
        strength: i32,
        types: DamageTypes,
        traits: DamageTraits,
        /// Every enemy unit, not one chosen.
        all: bool,
    },
    Heal {
        points: i32,
        /// Every unit of the caster's, not one chosen.
        all: bool,
        /// Creatures of this realm are not healed (Mass Healing does not help the dead).
        except: Option<Realm>,
    },
    Summon(UnitId),
    /// A lasting spell on one unit; a curse has a save.
    Enchant {
        mods: Vec<Modifier>,
        grants: Vec<Ability>,
        save: Option<SpellSave>,
    },
    /// A dispel of this strength for each mana put in.
    Dispel(u32),
    CounterMagic,
    /// Figures that fail a resistance check are destroyed.
    Slay {
        realm: Realm,
        modifier: i32,
        types: Vec<DamageType>,
        only: SlayOnly,
        below: Option<i32>,
        mana_per_step: u32,
        all: bool,
    },
    /// A battlefield enchantment.
    Field(Field),
    CallChaos,
    /// Raise one of the wizard's own dead units as undead (Animate Dead).
    Animate,
    /// Bring a fallen normal unit of the wizard's back with half its figures (Raise Dead).
    Raise,
    /// Destroy a missile unit's ammunition (Warp Wood).
    WarpWood,
    /// Take one of the wizard's units off the field to the summoning circle: only a hero for
    /// Recall Hero, any unit for Word of Recall.
    Recall {
        anyone: bool,
    },
    /// A Magic Vortex on a free square.
    Vortex,
    /// Mud on a 5 by 5 area (Earth to Mud).
    EarthToMud,
    /// A fissure on a square (Cracks Call).
    CracksCall,
    /// A section of the city wall is destroyed (Disrupt).
    Disrupt,
    /// The defender raises a wall at its city for the battle: stone, fire or darkness.
    CityWall(CityWall),
    /// Life from the target to the caster (Life Drain); extra mana makes it harder to resist.
    LifeDrain,
    /// A unit changes sides after a resistance check: for good until a dispel (Creature
    /// Binding, fantastic creatures) or just for the battle (Possession, normal units).
    Turn {
        bind: bool,
        save: Option<SpellSave>,
    },
    /// A normal unit becomes a creature of chaos with a random mutation, for good (Chaos
    /// Channels): the same change as overland.
    Channel,
    /// After a resistance check, one of three lasting harms to a normal unit (Warp Creature).
    Warp {
        save: Option<SpellSave>,
    },
}

impl SpellType {
    /// What the spell does in a battle, or `None` if it cannot be cast there (yet).
    pub fn of(spell: SpellId, def: &SpellDef) -> Option<SpellType> {
        if !def.contexts.contains(&CastContext::Battle) {
            return None;
        }
        if let [r] = def.effects.as_slice() {
            // Creature Binding or Possession.
            let own = UnitEffect::of(def).filter(
                |_| matches!(&r.effect, Effect::Custom { name, .. } if name == UNIT_EFFECT),
            );
            let bind = own == Some(UnitEffect::CreatureBinding);
            if bind || own == Some(UnitEffect::Possession) {
                return Some(SpellType::Turn {
                    bind,
                    save: def.save.clone(),
                });
            }
        }
        // A lasting spell on one unit: every rule is a plain modifier or granted ability.
        if matches!(def.target, SpellTarget::OwnUnit | SpellTarget::EnemyUnit)
            && def.duration != Duration::Instant
        {
            let mut mods = Vec::new();
            let mut grants = Vec::new();
            for r in &def.effects {
                if r.when != Trigger::Passive || r.condition != Condition::Always {
                    return None;
                }
                match &r.effect {
                    Effect::Modify {
                        stat,
                        op,
                        value,
                        types,
                    } => {
                        mods.push(
                            Modifier::amount(*stat, *op, *value, Source::Spell(spell))
                                .of_types(*types),
                        );
                    }
                    Effect::GrantAbility(a) => {
                        grants.push(*a);
                    }
                    // Heroism's tag: the level it gives is read where the unit's level is.
                    Effect::Custom { name, .. } if name == HEROISM => {}
                    // What the engine reads from the spell by its effect.
                    Effect::Custom { name, .. } if name == UNIT_EFFECT => {}
                    _ => return None,
                }
            }
            // A spell with no rules is read by its name where it matters (Bless, Righteousness,
            // the resistance spells, Spell Lock, Heroism): the unit just carries it.
            return Some(SpellType::Enchant {
                mods,
                grants,
                save: def.save.clone(),
            });
        }
        let [rule] = def.effects.as_slice() else {
            return None;
        };
        if rule.when != Trigger::OnCast {
            return None;
        }
        let all_enemies = rule.target == TargetSel::AllEnemyUnits;
        match &rule.effect {
            Effect::Damage {
                amount,
                types,
                traits,
                ..
            } => Some(SpellType::Damage {
                strength: *amount,
                types: DamageTypes::of(types),
                traits: *traits,
                all: all_enemies,
            }),
            Effect::Heal(n) => Some(SpellType::Heal {
                points: *n,
                all: rule.target == TargetSel::AllOwnUnits,
                except: match &rule.condition {
                    Condition::Not(c) => match &**c {
                        Condition::RealmIs(r) => Realm::of(*r),
                        _ => None,
                    },
                    _ => None,
                },
            }),
            Effect::Summon { unit, .. } => Some(SpellType::Summon(*unit)),
            Effect::Custom { name, .. } if name == COUNTER_EFFECT => match CounterEffect::of(def) {
                Some((CounterEffect::Dispel, strength)) => {
                    Some(SpellType::Dispel(strength / def.cost.max(1)))
                }
                Some((CounterEffect::CounterMagic, _)) => Some(SpellType::CounterMagic),
                _ => None,
            },
            Effect::Slay {
                realm,
                modifier,
                types,
                only,
                below,
                mana_per_step,
            } => Some(SpellType::Slay {
                realm: Realm::of(*realm)?,
                modifier: *modifier,
                types: types.clone(),
                only: *only,
                below: *below,
                mana_per_step: *mana_per_step,
                all: all_enemies,
            }),
            Effect::Custom { name, .. } if name == BATTLEFIELD => {
                Field::of(def).map(SpellType::Field)
            }
            Effect::Custom { name, .. } if name == BATTLE_EFFECT => {
                BattleEffect::of(def).map(BattleEffect::kind)
            }
            Effect::Custom { name, .. } if name == UNIT_EFFECT => match UnitEffect::of(def) {
                Some(UnitEffect::ChaosChannels) => Some(SpellType::Channel),
                Some(UnitEffect::WarpCreature) => Some(SpellType::Warp {
                    save: def.save.clone(),
                }),
                _ => None,
            },
            Effect::Custom { name, args } if name == RECALL => {
                match args.first().map(String::as_str) {
                    Some(RECALL_ANYONE) => Some(SpellType::Recall { anyone: true }),
                    Some(RECALL_HERO) => Some(SpellType::Recall { anyone: false }),
                    _ => None,
                }
            }
            Effect::Custom { name, args } if name == WALL => {
                match args.first().map(String::as_str) {
                    Some("stone") => Some(SpellType::CityWall(CityWall::Stone)),
                    Some("fire") => Some(SpellType::CityWall(CityWall::Fire)),
                    Some("darkness") => Some(SpellType::CityWall(CityWall::Darkness)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Whether extra mana makes the spell stronger.
    pub fn takes_extra_mana(&self) -> bool {
        match self {
            SpellType::Damage { traits, .. } => traits.mana_per_point > 0,
            SpellType::Slay { mana_per_step, .. } => *mana_per_step > 0,
            SpellType::Dispel(_) | SpellType::CounterMagic | SpellType::LifeDrain => true,
            _ => false,
        }
    }
}

/// A spell the wizard could cast in the battle, for a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BattleSpell {
    pub spell: SpellId,
    /// Casting skill it costs at least.
    pub cost: i32,
    /// More mana can be put in to make it stronger.
    pub takes_extra_mana: bool,
    /// What it must be aimed at.
    pub target: SpellTarget,
    /// Whether the wizard can cast it now (skill, reserve, and the one spell a turn).
    pub castable: bool,
}

/// What a damage spell would do to a unit (see `Game::preview_spell_damage`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageOdds {
    /// Hit points taken on average.
    pub mean: f32,
    /// The share of tries in which the unit is destroyed.
    pub kill_chance: f32,
}

/// A spell the wizard may cast now, with where it may be aimed and how much mana may go in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalCast {
    pub spell: SpellId,
    /// Every aim the spell accepts: `None` for a spell with no target, a unit, or a square.
    pub aims: Vec<SpellAim>,
    /// The mana the spell takes at least (its cost) and at most (what the wizard can afford).
    /// They are equal for a spell that takes no extra mana. Any amount between is legal; `0`
    /// asks for the least.
    pub least_mana: u32,
    pub most_mana: u32,
}

/// The hook of the battle spells that act in a way of their own.
pub const BATTLE_EFFECT: &str = "battle_effect";

f4x_def::effect_enum! {
    /// What a spell with the `battle_effect` hook does in a battle: the argument of the hook
    /// (`Custom(name: "battle_effect", args: ["earth_to_mud"])`).
    pub enum BattleEffect in BATTLE_EFFECT;
    CallChaos = "call_chaos",
    Animate = "animate",
    Raise = "raise",
    WarpWood = "warp_wood",
    EarthToMud = "earth_to_mud",
    CracksCall = "cracks_call",
    Vortex = "vortex",
    Disrupt = "disrupt",
    LifeDrain = "life_drain",
}

impl BattleEffect {
    /// The effect a spell's definition gives, if it has the hook.
    pub fn of(def: &SpellDef) -> Option<BattleEffect> {
        BattleEffect::named(def.hook_args(BATTLE_EFFECT)?.first()?)
    }

    pub fn kind(self) -> SpellType {
        match self {
            BattleEffect::CallChaos => SpellType::CallChaos,
            BattleEffect::Animate => SpellType::Animate,
            BattleEffect::Raise => SpellType::Raise,
            BattleEffect::WarpWood => SpellType::WarpWood,
            BattleEffect::EarthToMud => SpellType::EarthToMud,
            BattleEffect::CracksCall => SpellType::CracksCall,
            BattleEffect::Vortex => SpellType::Vortex,
            BattleEffect::Disrupt => SpellType::Disrupt,
            BattleEffect::LifeDrain => SpellType::LifeDrain,
        }
    }
}

/// Creatures a spell may only be thrown at, by realm: Star Fires burns chaos and death
/// creatures and nothing else.
pub fn only_against(spell: SpellId) -> Option<&'static [Realm]> {
    match spell.num() {
        122 /* star_fires */ => Some(&[Realm::Chaos, Realm::Death]),
        _ => None,
    }
}

/// How much harder the undead find it to resist a spell, on top of its own modifier: Dispel Evil
/// and Holy Word (the help's "much more difficult time").
pub fn undead_penalty(spell: SpellId) -> i32 {
    match spell.num() {
        134 /* dispel_evil */ | 150 /* holy_word */ => 5,
        _ => 0,
    }
}

/// Whether a slaying spell of this kind can destroy the unit.
pub fn slay_allows(only: SlayOnly, unit: &crate::unit::BattleUnit) -> bool {
    match only {
        SlayOnly::Any => true,
        SlayOnly::Fantastic => unit.is_fantastic(),
        SlayOnly::ChaosOrDeath => matches!(unit.realm, Some(Realm::Chaos | Realm::Death)),
    }
}
