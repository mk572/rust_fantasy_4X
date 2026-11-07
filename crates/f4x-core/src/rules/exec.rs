//! Conditions, targets and effects: the primitives and a small executor.
//!
//! The executor never touches the ECS directly. It talks to an [`EffectHost`],
//! which answers queries about the world ("who is in this tile?") and carries out
//! the primitive changes. The real game implements the host over `hecs`; tests
//! implement it over a tiny synthetic world. All randomness comes from the RNG
//! passed in, so a run is reproducible.
//!
//! Tests: in rules/exec/tests.rs.

use crate::rules::{Ability, RealmId, UnitId};
use rand::RngCore;

use super::effects::{Condition, DamageKind, Effect, EffectRule, TargetSel, Trigger};
use super::stats::{ModOp, Modifier, Source, Stat};
use crate::battle::immunity::{resistance_failure, Realm};
use crate::map::Position;
use crate::player::PlayerId;

/// Something an effect can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Target {
    Unit(#[serde(with = "crate::game::replay::stable_ref::one")] hecs::Entity),
    City(#[serde(with = "crate::game::replay::stable_ref::one")] hecs::Entity),
    /// A city and a realm: what a Spell Ward is aimed at.
    CityRealm(
        #[serde(with = "crate::game::replay::stable_ref::one")] hecs::Entity,
        crate::rules::RealmId,
    ),
    Tile(Position),
    Player(PlayerId),
    /// A dead hero, by its type (a roster index): what Resurrection is aimed at.
    Hero(usize),
    /// A spell instance: an enemy wizard's global enchantment (`docs/ecs-structure.md` 3.4).
    Spell(#[serde(with = "crate::game::replay::stable_ref::one")] hecs::Entity),
}

/// What the rules need to know about a target to evaluate a [`Condition`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// The target's abilities, which the executor reads for the immunities of a damage effect.
    pub abilities: Vec<Ability>,
    pub realm: Option<RealmId>,
    pub kind: Option<crate::rules::UnitKind>,
    /// The kinds the spell in question is (for a wizard's retorts).
    pub spell_tags: Vec<crate::rules::effects::SpellTag>,
}

/// Who is acting, and on what, when an effect rule runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    /// The entity the rule belongs to (the unit with the ability, the city with
    /// the building, the spell's target holder...). `This` resolves to it.
    pub this: Target,
    /// The player who owns the source (caster, unit owner, city owner).
    pub owner: PlayerId,
    /// What the caster picked, for `TargetSel::Chosen`.
    pub chosen: Option<Target>,
    /// What produced the effect, used to tag the modifiers it adds.
    pub source: Source,
}

/// Queries and primitive changes the executor needs from the world.
pub trait EffectHost {
    fn facts(&self, who: Target) -> Facts;
    fn position(&self, who: Target) -> Option<Position>;
    fn owner_of(&self, who: Target) -> Option<PlayerId>;
    /// Every unit currently in the world, in a stable order.
    fn units(&self) -> Vec<hecs::Entity>;
    fn city_at(&self, pos: Position) -> Option<hecs::Entity>;
    fn stat(&self, who: Target, stat: Stat) -> i32;
    /// The target's resistance against a spell of `realm`: its stat, plus whatever its
    /// immunities and enchantments add against that realm.
    fn resistance_against(&self, who: Target, _realm: Option<Realm>) -> i32 {
        self.stat(who, Stat::Resistance)
    }
    /// Whether the target is wholly protected from spells of `realm` (Righteousness against
    /// chaos and death).
    fn protected_from(&self, _who: Target, _realm: Option<Realm>) -> bool {
        false
    }

    fn add_modifier(&mut self, who: Target, modifier: Modifier);
    fn grant_ability(&mut self, who: Target, ability: Ability);
    /// Returns the damage actually dealt (after hp clamping), for the event log.
    fn damage(&mut self, who: Target, amount: i32, kind: DamageKind) -> i32;
    /// A damage spell of `strength` struck by the dice of a battle (every figure of the unit is
    /// attacked), when the effect asks for it with `traits.area`; otherwise plain damage. Returns
    /// the hit points lost.
    fn strike(
        &mut self,
        who: Target,
        strength: i32,
        kind: DamageKind,
        _realm: Option<Realm>,
        _traits: &crate::rules::effects::DamageTraits,
    ) -> i32 {
        self.damage(who, strength, kind)
    }
    fn heal(&mut self, who: Target, amount: i32) -> i32;
    fn summon(&mut self, owner: PlayerId, at: Position, unit: UnitId, count: u8);
    fn custom(&mut self, hook: &str, args: &[String], who: Target, ctx: &Context);
    /// The random numbers of the rules being run. The host owns the stream (the game's own, in
    /// play), so the executor and the hooks it calls draw from one generator.
    fn rng(&mut self) -> &mut dyn RngCore;
}

/// What happened when a rule ran, for the event log and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Modified {
        who: Target,
        stat: Stat,
    },
    AbilityGranted {
        who: Target,
        ability: Ability,
    },
    Damaged {
        who: Target,
        amount: i32,
        kind: DamageKind,
    },
    /// The target's resistance check negated the effect.
    Resisted {
        who: Target,
    },
    /// The target is immune to the effect.
    Immune {
        who: Target,
    },
    Healed {
        who: Target,
        amount: i32,
    },
    Summoned {
        unit: UnitId,
        count: u8,
    },
    Custom {
        hook: String,
    },
}

/// Evaluate a condition against the facts about the subject.
pub fn eval(cond: &Condition, facts: &Facts) -> bool {
    match cond {
        Condition::Always => true,
        Condition::RealmIs(r) => facts.realm == Some(*r),
        Condition::KindIs(k) => facts.kind == Some(*k),
        Condition::SpellIs(t) => facts.spell_tags.contains(t),
        Condition::Not(c) => !eval(c, facts),
    }
}

/// The targets a selector picks out, in a stable order.
pub fn resolve_targets(sel: TargetSel, ctx: &Context, host: &dyn EffectHost) -> Vec<Target> {
    let here = host.position(ctx.this);
    let units = || host.units().into_iter();
    match sel {
        TargetSel::This => vec![ctx.this],
        TargetSel::Owner => vec![Target::Player(ctx.owner)],
        TargetSel::AllOwnUnits => units()
            .map(Target::Unit)
            .filter(|t| host.owner_of(*t) == Some(ctx.owner))
            .collect(),
        TargetSel::AllEnemyUnits => units()
            .map(Target::Unit)
            .filter(|t| host.owner_of(*t).is_some_and(|o| o != ctx.owner))
            .collect(),
        TargetSel::Tile => here.map(Target::Tile).into_iter().collect(),
        TargetSel::Chosen => ctx.chosen.into_iter().collect(),
        TargetSel::UnitsOnChosenSquare => match ctx.chosen.and_then(|t| tile_of(t, host)) {
            Some(p) => units()
                .map(Target::Unit)
                .filter(|t| host.position(*t) == Some(p))
                .collect(),
            None => Vec::new(),
        },
    }
}

fn tile_of(who: Target, host: &dyn EffectHost) -> Option<Position> {
    match who {
        Target::Tile(p) => Some(p),
        _ => host.position(who),
    }
}

fn run_effect(
    effect: &Effect,
    who: Target,
    ctx: &Context,
    host: &mut dyn EffectHost,
    out: &mut Vec<Outcome>,
) {
    match effect {
        Effect::Modify { stat, op, value } => {
            host.add_modifier(who, Modifier::new(*stat, *op, *value, ctx.source));
            out.push(Outcome::Modified { who, stat: *stat });
        }
        Effect::GrantAbility(ability) => {
            let ability = *ability;
            host.grant_ability(who, ability);
            out.push(Outcome::AbilityGranted { who, ability });
        }
        Effect::Damage {
            amount,
            kind,
            resistible,
            immune,
            realm,
            resist_modifier,
            traits,
        } => {
            let realm = realm.and_then(Realm::of);
            let abilities = host.facts(who).abilities;
            let immune_ids = immune.clone();
            if immune_ids.iter().any(|i| abilities.contains(i)) || host.protected_from(who, realm) {
                out.push(Outcome::Immune { who });
            } else if *resistible && {
                let resistance = host.resistance_against(who, realm);
                resistance_failure(resistance, *resist_modifier, host.rng()) == 0
            } {
                out.push(Outcome::Resisted { who });
            } else {
                let dealt = host.strike(who, *amount, *kind, realm, traits);
                out.push(Outcome::Damaged {
                    who,
                    amount: dealt,
                    kind: *kind,
                });
            }
        }
        Effect::Heal(n) => {
            let healed = host.heal(who, *n);
            out.push(Outcome::Healed {
                who,
                amount: healed,
            });
        }
        // Battle spells: the battle carries them out (`battle/spells.rs`).
        Effect::Slay { .. } => {}
        Effect::Summon { unit, count } => {
            if let Some(at) = tile_of(who, host) {
                host.summon(ctx.owner, at, *unit, *count);
                out.push(Outcome::Summoned {
                    unit: *unit,
                    count: *count,
                });
            }
        }
        Effect::Custom { name: hook, args } => {
            host.custom(hook, args, who, ctx);
            out.push(Outcome::Custom { hook: hook.clone() });
        }
    }
}

/// Run every rule in `rules` whose trigger is `when`: check the condition against
/// the rule's targets, then apply the effect to each target that passes.
///
/// Passive rules run once when their source is attached (modifiers are tagged
/// with `ctx.source` so they can be removed later); the other triggers run each
/// time the event happens.
pub fn run_rules(
    rules: &[EffectRule],
    when: Trigger,
    ctx: &Context,
    host: &mut dyn EffectHost,
) -> Vec<Outcome> {
    let mut out = Vec::new();
    for rule in rules.iter().filter(|r| r.when == when) {
        for who in resolve_targets(rule.target, ctx, host) {
            if eval(&rule.condition, &host.facts(who)) {
                run_effect(&rule.effect, who, ctx, host, &mut out);
            }
        }
    }
    out
}

/// Convenience for tests and tools: an `Add` modifier effect.
pub fn add(stat: Stat, value: i32) -> Effect {
    Effect::Modify {
        stat,
        op: ModOp::Add,
        value,
    }
}

#[cfg(test)]
mod tests;
