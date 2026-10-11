//! Conditions, targets and effects: the primitives and a small executor.
//!
//! The executor never touches the ECS directly. It talks to an [`EffectHost`],
//! which answers queries about the world ("who is in this tile?") and carries out
//! the primitive changes. The real game implements the host over `hecs`; tests
//! implement it over a tiny synthetic world. All randomness comes from the RNG
//! passed in, so a run is reproducible.

use rand::RngCore;

use super::effects::{Condition, DamageKind, Effect, EffectRule, TargetSel, Trigger};
use super::stats::{ModOp, Modifier, Source, Stat};
use crate::battle::immunity::{resistance_failure, Realm};
use crate::city::{PlayerId, Position};
use crate::content::TerrainClass;
use crate::map::Plane;

/// Something an effect can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    Unit(hecs::Entity),
    City(hecs::Entity),
    Tile(Position),
    Player(PlayerId),
    /// An enemy wizard's global enchantment: its owner and the spell.
    Global(PlayerId, crate::rules::SpellId),
}

/// What the rules need to know about a target to evaluate a [`Condition`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    pub abilities: Vec<String>,
    pub terrain: Option<TerrainClass>,
    pub plane: Option<Plane>,
    pub race: Option<String>,
    pub realm: Option<String>,
    pub kind: Option<crate::rules::UnitKind>,
    pub in_battle: bool,
    pub in_city: bool,
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
    fn grant_ability(&mut self, who: Target, ability: &str);
    fn remove_ability(&mut self, who: Target, ability: &str);
    /// Returns the damage actually dealt (after hp clamping), for the event log.
    fn damage(&mut self, who: Target, amount: i32, kind: DamageKind) -> i32;
    fn heal(&mut self, who: Target, amount: i32) -> i32;
    fn add_resources(&mut self, who: Target, gold: i32, food: i32, mana: i32, research: i32);
    fn summon(&mut self, owner: PlayerId, at: Position, unit: &str, count: u8);
    fn change_terrain(&mut self, at: Position, class: TerrainClass);
    fn set_feature(&mut self, at: Position, feature: &str);
    fn apply_enchantment(&mut self, who: Target, spell: &str, caster: PlayerId);
    /// Remove other wizards' enchantments (cast by anyone but `by`) that the strength beats;
    /// returns how many were removed.
    fn dispel(&mut self, who: Target, strength: u32, by: PlayerId, rng: &mut dyn RngCore) -> u32;
    fn custom(&mut self, hook: &str, who: Target, ctx: &Context, rng: &mut dyn RngCore);
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
        ability: String,
    },
    AbilityRemoved {
        who: Target,
        ability: String,
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
    ResourcesAdded {
        who: Target,
    },
    Summoned {
        unit: String,
        count: u8,
    },
    TerrainChanged {
        at: Position,
        class: TerrainClass,
    },
    FeatureSet {
        at: Position,
        feature: String,
    },
    Enchanted {
        who: Target,
        spell: String,
    },
    Dispelled {
        who: Target,
        removed: u32,
    },
    Custom {
        hook: String,
    },
}

/// Evaluate a condition against the facts about the subject.
pub fn eval(cond: &Condition, facts: &Facts) -> bool {
    match cond {
        Condition::Always => true,
        Condition::HasAbility(a) => facts.abilities.iter().any(|x| x == a),
        Condition::TerrainIs(c) => facts.terrain == Some(*c),
        Condition::OnPlane(p) => facts.plane == Some(*p),
        Condition::RaceIs(r) => facts.race.as_deref() == Some(r.as_str()),
        Condition::RealmIs(r) => facts.realm.as_deref() == Some(r.as_str()),
        Condition::KindIs(k) => facts.kind == Some(*k),
        Condition::InBattle => facts.in_battle,
        Condition::InCity => facts.in_city,
        Condition::All(cs) => cs.iter().all(|c| eval(c, facts)),
        Condition::Any(cs) => cs.iter().any(|c| eval(c, facts)),
        Condition::Not(c) => !eval(c, facts),
    }
}

/// Chebyshev distance on one plane; `None` across planes.
fn distance(a: Position, b: Position) -> Option<u32> {
    (a.plane == b.plane).then(|| a.x.abs_diff(b.x).max(a.y.abs_diff(b.y)))
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
        TargetSel::UnitsInTile => match here {
            Some(p) => units()
                .map(Target::Unit)
                .filter(|t| host.position(*t) == Some(p))
                .collect(),
            None => Vec::new(),
        },
        TargetSel::UnitsInArea(r) => match here {
            Some(p) => units()
                .map(Target::Unit)
                .filter(|t| {
                    host.position(*t)
                        .and_then(|q| distance(p, q))
                        .is_some_and(|d| d <= r as u32)
                })
                .collect(),
            None => Vec::new(),
        },
        TargetSel::City => match ctx.this {
            Target::City(_) => vec![ctx.this],
            _ => here
                .and_then(|p| host.city_at(p))
                .map(Target::City)
                .into_iter()
                .collect(),
        },
        TargetSel::Tile => here.map(Target::Tile).into_iter().collect(),
        TargetSel::Chosen => ctx.chosen.into_iter().collect(),
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
    rng: &mut dyn RngCore,
    out: &mut Vec<Outcome>,
) {
    match effect {
        Effect::Modify { stat, op, value } => {
            host.add_modifier(who, Modifier::new(*stat, *op, *value, ctx.source));
            out.push(Outcome::Modified { who, stat: *stat });
        }
        Effect::GrantAbility(a) => {
            host.grant_ability(who, a);
            out.push(Outcome::AbilityGranted {
                who,
                ability: a.clone(),
            });
        }
        Effect::RemoveAbility(a) => {
            host.remove_ability(who, a);
            out.push(Outcome::AbilityRemoved {
                who,
                ability: a.clone(),
            });
        }
        Effect::Damage {
            amount,
            kind,
            resistible,
            immune,
            realm,
            resist_modifier,
        } => {
            let realm = realm.as_deref().and_then(Realm::from_id);
            let abilities = host.facts(who).abilities;
            if immune.iter().any(|i| abilities.contains(i)) || host.protected_from(who, realm) {
                out.push(Outcome::Immune { who });
            } else if *resistible
                && resistance_failure(host.resistance_against(who, realm), *resist_modifier, rng)
                    == 0
            {
                out.push(Outcome::Resisted { who });
            } else {
                let dealt = host.damage(who, *amount, *kind);
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
        Effect::AddResource {
            gold,
            food,
            mana,
            research,
        } => {
            host.add_resources(who, *gold, *food, *mana, *research);
            out.push(Outcome::ResourcesAdded { who });
        }
        Effect::Summon { unit, count } => {
            if let Some(at) = tile_of(who, host) {
                host.summon(ctx.owner, at, unit, *count);
                out.push(Outcome::Summoned {
                    unit: unit.clone(),
                    count: *count,
                });
            }
        }
        Effect::ChangeTerrain(class) => {
            if let Some(at) = tile_of(who, host) {
                host.change_terrain(at, *class);
                out.push(Outcome::TerrainChanged { at, class: *class });
            }
        }
        Effect::SetFeature(f) => {
            if let Some(at) = tile_of(who, host) {
                host.set_feature(at, f);
                out.push(Outcome::FeatureSet {
                    at,
                    feature: f.clone(),
                });
            }
        }
        Effect::ApplyEnchantment(spell) => {
            host.apply_enchantment(who, spell, ctx.owner);
            out.push(Outcome::Enchanted {
                who,
                spell: spell.clone(),
            });
        }
        Effect::Dispel { strength } => {
            let removed = host.dispel(who, *strength, ctx.owner, rng);
            out.push(Outcome::Dispelled { who, removed });
        }
        Effect::Custom(hook) => {
            host.custom(hook, who, ctx, rng);
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
    rng: &mut dyn RngCore,
) -> Vec<Outcome> {
    let mut out = Vec::new();
    for rule in rules.iter().filter(|r| r.when == when) {
        for who in resolve_targets(rule.target, ctx, host) {
            if eval(&rule.condition, &host.facts(who)) {
                run_effect(&rule.effect, who, ctx, host, rng, &mut out);
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
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_pcg::Pcg64Mcg;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct MockUnit {
        owner: u8,
        pos: Option<Position>,
        facts: Facts,
        hp: i32,
        resistance: i32,
        mods: Vec<Modifier>,
        enchants: Vec<String>,
    }

    /// A tiny synthetic world: units keyed by entity, one optional city.
    #[derive(Default)]
    struct MockHost {
        units: BTreeMap<u32, (hecs::Entity, MockUnit)>,
        city: Option<(hecs::Entity, Position)>,
        summoned: Vec<(String, u8, Position)>,
        terrain: Vec<(Position, TerrainClass)>,
        features: Vec<(Position, String)>,
        resources: Vec<(i32, i32, i32, i32)>,
        customs: Vec<String>,
        ids: hecs::World,
        next: u32,
    }

    impl MockHost {
        fn spawn(&mut self, owner: u8, pos: Position, hp: i32, resistance: i32) -> hecs::Entity {
            // A world owned by the mock hands out valid, distinct entity ids.
            let e = self.ids.spawn(());
            self.next += 1;
            self.units.insert(
                e.id(),
                (
                    e,
                    MockUnit {
                        owner,
                        pos: Some(pos),
                        hp,
                        resistance,
                        facts: Facts {
                            plane: Some(pos.plane),
                            ..Facts::default()
                        },
                        ..MockUnit::default()
                    },
                ),
            );
            e
        }
        fn unit(&self, e: hecs::Entity) -> &MockUnit {
            &self.units[&e.id()].1
        }
        fn unit_mut(&mut self, who: Target) -> Option<&mut MockUnit> {
            match who {
                Target::Unit(e) => self.units.get_mut(&e.id()).map(|(_, u)| u),
                _ => None,
            }
        }
    }

    impl EffectHost for MockHost {
        fn facts(&self, who: Target) -> Facts {
            match who {
                Target::Unit(e) => self.unit(e).facts.clone(),
                _ => Facts::default(),
            }
        }
        fn position(&self, who: Target) -> Option<Position> {
            match who {
                Target::Unit(e) => self.unit(e).pos,
                Target::City(_) => self.city.map(|c| c.1),
                Target::Tile(p) => Some(p),
                Target::Player(_) | Target::Global(..) => None,
            }
        }
        fn owner_of(&self, who: Target) -> Option<PlayerId> {
            match who {
                Target::Unit(e) => Some(PlayerId(self.unit(e).owner)),
                _ => None,
            }
        }
        fn units(&self) -> Vec<hecs::Entity> {
            self.units.values().map(|(e, _)| *e).collect()
        }
        fn city_at(&self, pos: Position) -> Option<hecs::Entity> {
            self.city.filter(|c| c.1 == pos).map(|c| c.0)
        }
        fn stat(&self, who: Target, stat: Stat) -> i32 {
            match (who, stat) {
                (Target::Unit(e), Stat::Resistance) => self.unit(e).resistance,
                _ => 0,
            }
        }
        fn add_modifier(&mut self, who: Target, m: Modifier) {
            if let Some(u) = self.unit_mut(who) {
                u.mods.push(m);
            }
        }
        fn grant_ability(&mut self, who: Target, a: &str) {
            if let Some(u) = self.unit_mut(who) {
                u.facts.abilities.push(a.to_string());
            }
        }
        fn remove_ability(&mut self, who: Target, a: &str) {
            if let Some(u) = self.unit_mut(who) {
                u.facts.abilities.retain(|x| x != a);
            }
        }
        fn damage(&mut self, who: Target, amount: i32, _k: DamageKind) -> i32 {
            let u = self.unit_mut(who).unwrap();
            let dealt = amount.min(u.hp);
            u.hp -= dealt;
            dealt
        }
        fn heal(&mut self, who: Target, amount: i32) -> i32 {
            let u = self.unit_mut(who).unwrap();
            u.hp += amount;
            amount
        }
        fn add_resources(&mut self, _w: Target, g: i32, f: i32, m: i32, r: i32) {
            self.resources.push((g, f, m, r));
        }
        fn summon(&mut self, _o: PlayerId, at: Position, unit: &str, count: u8) {
            self.summoned.push((unit.to_string(), count, at));
        }
        fn change_terrain(&mut self, at: Position, class: TerrainClass) {
            self.terrain.push((at, class));
        }
        fn set_feature(&mut self, at: Position, f: &str) {
            self.features.push((at, f.to_string()));
        }
        fn apply_enchantment(&mut self, who: Target, spell: &str, _c: PlayerId) {
            if let Some(u) = self.unit_mut(who) {
                u.enchants.push(spell.to_string());
            }
        }
        fn dispel(
            &mut self,
            who: Target,
            strength: u32,
            _by: PlayerId,
            _rng: &mut dyn RngCore,
        ) -> u32 {
            let u = self.unit_mut(who).unwrap();
            let n = if strength > 0 {
                u.enchants.len() as u32
            } else {
                0
            };
            u.enchants.clear();
            n
        }
        fn custom(&mut self, hook: &str, _w: Target, _c: &Context, _r: &mut dyn RngCore) {
            self.customs.push(hook.to_string());
        }
    }

    fn pos(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    fn rng() -> Pcg64Mcg {
        Pcg64Mcg::seed_from_u64(7)
    }

    fn ctx(this: Target, owner: u8) -> Context {
        Context {
            this,
            owner: PlayerId(owner),
            chosen: None,
            source: Source::Other,
        }
    }

    fn rule(when: Trigger, condition: Condition, target: TargetSel, effect: Effect) -> EffectRule {
        EffectRule {
            when,
            condition,
            target,
            effect,
        }
    }

    #[test]
    fn conditions_combine() {
        let f = Facts {
            abilities: vec!["flying".into()],
            terrain: Some(TerrainClass::Hills),
            plane: Some(Plane::Myrror),
            race: Some("orc".into()),
            in_battle: true,
            ..Facts::default()
        };
        assert!(eval(&Condition::Always, &f));
        assert!(eval(&Condition::HasAbility("flying".into()), &f));
        assert!(!eval(&Condition::HasAbility("swimming".into()), &f));
        assert!(eval(&Condition::TerrainIs(TerrainClass::Hills), &f));
        assert!(eval(&Condition::OnPlane(Plane::Myrror), &f));
        assert!(eval(&Condition::RaceIs("orc".into()), &f));
        assert!(!eval(&Condition::RealmIs("life".into()), &f));
        assert!(eval(&Condition::InBattle, &f) && !eval(&Condition::InCity, &f));
        let all = Condition::All(vec![
            Condition::InBattle,
            Condition::HasAbility("flying".into()),
        ]);
        assert!(eval(&all, &f));
        let any = Condition::Any(vec![Condition::InCity, Condition::OnPlane(Plane::Arcanus)]);
        assert!(!eval(&any, &f));
        assert!(eval(&Condition::Not(Box::new(any)), &f));
    }

    #[test]
    fn selectors_pick_the_right_units() {
        let mut h = MockHost::default();
        let a = h.spawn(0, pos(5, 5), 5, 0);
        let b = h.spawn(0, pos(5, 5), 5, 0);
        let c = h.spawn(0, pos(6, 6), 5, 0);
        let far = h.spawn(0, pos(20, 20), 5, 0);
        let enemy = h.spawn(1, pos(5, 5), 5, 0);
        let cx = ctx(Target::Unit(a), 0);
        let units = |sel| resolve_targets(sel, &cx, &h);
        assert_eq!(units(TargetSel::This), vec![Target::Unit(a)]);
        assert_eq!(units(TargetSel::Owner), vec![Target::Player(PlayerId(0))]);
        assert_eq!(
            units(TargetSel::AllOwnUnits),
            vec![a, b, c, far]
                .into_iter()
                .map(Target::Unit)
                .collect::<Vec<_>>()
        );
        assert_eq!(units(TargetSel::AllEnemyUnits), vec![Target::Unit(enemy)]);
        assert_eq!(
            units(TargetSel::UnitsInTile),
            vec![a, b, enemy]
                .into_iter()
                .map(Target::Unit)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            units(TargetSel::UnitsInArea(1)),
            vec![a, b, c, enemy]
                .into_iter()
                .map(Target::Unit)
                .collect::<Vec<_>>()
        );
        assert_eq!(units(TargetSel::Tile), vec![Target::Tile(pos(5, 5))]);
        assert!(units(TargetSel::Chosen).is_empty());
        let picked = Context {
            chosen: Some(Target::Unit(far)),
            ..cx
        };
        assert_eq!(
            resolve_targets(TargetSel::Chosen, &picked, &h),
            vec![Target::Unit(far)]
        );
    }

    #[test]
    fn modify_is_tagged_with_the_source() {
        let mut h = MockHost::default();
        let a = h.spawn(0, pos(1, 1), 5, 0);
        let cx = Context {
            source: Source::Level(2),
            ..ctx(Target::Unit(a), 0)
        };
        let rules = [rule(
            Trigger::Passive,
            Condition::Always,
            TargetSel::This,
            add(Stat::Melee, 2),
        )];
        let out = run_rules(&rules, Trigger::Passive, &cx, &mut h, &mut rng());
        assert_eq!(
            out,
            vec![Outcome::Modified {
                who: Target::Unit(a),
                stat: Stat::Melee
            }]
        );
        let m = h.unit(a).mods[0];
        assert_eq!(
            (m.stat, m.value, m.source),
            (Stat::Melee, 2, Source::Level(2))
        );
    }

    #[test]
    fn only_matching_trigger_and_condition_run() {
        let mut h = MockHost::default();
        let a = h.spawn(0, pos(1, 1), 5, 0);
        let cx = ctx(Target::Unit(a), 0);
        let rules = [
            rule(
                Trigger::OnAttack,
                Condition::Always,
                TargetSel::This,
                add(Stat::Melee, 1),
            ),
            rule(
                Trigger::Passive,
                Condition::HasAbility("blessed".into()),
                TargetSel::This,
                add(Stat::Defense, 1),
            ),
        ];
        assert!(run_rules(&rules, Trigger::Passive, &cx, &mut h, &mut rng()).is_empty());
        h.grant_ability(Target::Unit(a), "blessed");
        let out = run_rules(&rules, Trigger::Passive, &cx, &mut h, &mut rng());
        assert_eq!(
            out,
            vec![Outcome::Modified {
                who: Target::Unit(a),
                stat: Stat::Defense
            }]
        );
    }

    #[test]
    fn damage_respects_hp_and_resistance() {
        let mut h = MockHost::default();
        let caster = h.spawn(0, pos(0, 0), 5, 0);
        let soft = h.spawn(1, pos(1, 0), 4, 0);
        let immune = h.spawn(1, pos(2, 0), 4, 10);
        let dmg = |resistible| {
            [rule(
                Trigger::OnCast,
                Condition::Always,
                TargetSel::AllEnemyUnits,
                Effect::Damage {
                    amount: 6,
                    kind: DamageKind::Fire,
                    resistible,
                    immune: Vec::new(),
                    realm: None,
                    resist_modifier: 0,
                },
            )]
        };
        let cx = ctx(Target::Unit(caster), 0);
        let out = run_rules(&dmg(true), Trigger::OnCast, &cx, &mut h, &mut rng());
        assert_eq!(
            out,
            vec![
                Outcome::Damaged {
                    who: Target::Unit(soft),
                    amount: 4,
                    kind: DamageKind::Fire
                },
                Outcome::Resisted {
                    who: Target::Unit(immune)
                },
            ]
        );
        assert_eq!(h.unit(soft).hp, 0);
        assert_eq!(h.unit(immune).hp, 4);
        // Not resistible: the immune unit takes it.
        run_rules(&dmg(false), Trigger::OnCast, &cx, &mut h, &mut rng());
        assert_eq!(h.unit(immune).hp, 0);
    }

    fn resists(resistance: i32, rng: &mut dyn RngCore) -> bool {
        resistance_failure(resistance, 0, rng) == 0
    }

    #[test]
    fn immune_targets_take_no_damage_and_a_spell_can_be_hard_to_resist() {
        let mut h = MockHost::default();
        let caster = h.spawn(0, pos(0, 0), 5, 0);
        let salamander = h.spawn(1, pos(1, 0), 9, 0);
        h.grant_ability(Target::Unit(salamander), "fire_immunity");
        let plain = h.spawn(1, pos(2, 0), 9, 0);
        let bolt = |immune: &[&str], resist_modifier| {
            [rule(
                Trigger::OnCast,
                Condition::Always,
                TargetSel::AllEnemyUnits,
                Effect::Damage {
                    amount: 4,
                    kind: DamageKind::Fire,
                    resistible: true,
                    immune: immune.iter().map(|s| s.to_string()).collect(),
                    realm: Some("chaos".into()),
                    resist_modifier,
                },
            )]
        };
        let cx = ctx(Target::Unit(caster), 0);
        let out = run_rules(
            &bolt(&["fire_immunity"], 0),
            Trigger::OnCast,
            &cx,
            &mut h,
            &mut rng(),
        );
        assert_eq!(
            out,
            vec![
                Outcome::Immune {
                    who: Target::Unit(salamander)
                },
                Outcome::Damaged {
                    who: Target::Unit(plain),
                    amount: 4,
                    kind: DamageKind::Fire
                },
            ]
        );
        assert_eq!(h.unit(salamander).hp, 9);
        // Resistance 5 resists half the time; a penalty of 5 leaves nothing to resist with.
        let resisted = |modifier| {
            (0..400u64)
                .filter(|&seed| {
                    let mut h = MockHost::default();
                    let c = h.spawn(0, pos(0, 0), 5, 0);
                    h.spawn(1, pos(1, 0), 9, 5);
                    let mut r = <rand_pcg::Pcg64Mcg as rand::SeedableRng>::seed_from_u64(seed);
                    let out = run_rules(
                        &bolt(&[], modifier),
                        Trigger::OnCast,
                        &ctx(Target::Unit(c), 0),
                        &mut h,
                        &mut r,
                    );
                    matches!(out[0], Outcome::Resisted { .. })
                })
                .count()
        };
        assert!((150..250).contains(&resisted(0)), "{}", resisted(0));
        assert_eq!(resisted(-5), 0);
    }

    #[test]
    fn resistance_odds_follow_the_stat() {
        let mut r = rng();
        let resisted = (0..10_000).filter(|_| resists(3, &mut r)).count();
        assert!((2_700..3_300).contains(&resisted), "{resisted}");
        assert!(!(0..100).any(|_| resists(0, &mut r)));
        assert!((0..100).all(|_| resists(10, &mut r)));
    }

    #[test]
    fn world_changing_effects_reach_the_host() {
        let mut h = MockHost::default();
        let a = h.spawn(0, pos(3, 4), 5, 0);
        let cx = ctx(Target::Unit(a), 0);
        let rules = [
            rule(
                Trigger::OnCast,
                Condition::Always,
                TargetSel::Tile,
                Effect::ChangeTerrain(TerrainClass::Forest),
            ),
            rule(
                Trigger::OnCast,
                Condition::Always,
                TargetSel::Tile,
                Effect::SetFeature("ruin".into()),
            ),
            rule(
                Trigger::OnCast,
                Condition::Always,
                TargetSel::Tile,
                Effect::Summon {
                    unit: "wolf".into(),
                    count: 2,
                },
            ),
            rule(
                Trigger::OnCast,
                Condition::Always,
                TargetSel::Owner,
                Effect::AddResource {
                    gold: 5,
                    food: 0,
                    mana: 3,
                    research: 0,
                },
            ),
            rule(
                Trigger::OnCast,
                Condition::Always,
                TargetSel::This,
                Effect::ApplyEnchantment("haste".into()),
            ),
            rule(
                Trigger::OnCast,
                Condition::Always,
                TargetSel::This,
                Effect::Custom("hook".into()),
            ),
        ];
        run_rules(&rules, Trigger::OnCast, &cx, &mut h, &mut rng());
        assert_eq!(h.terrain, vec![(pos(3, 4), TerrainClass::Forest)]);
        assert_eq!(h.features, vec![(pos(3, 4), "ruin".to_string())]);
        assert_eq!(h.summoned, vec![("wolf".to_string(), 2, pos(3, 4))]);
        assert_eq!(h.resources, vec![(5, 0, 3, 0)]);
        assert_eq!(h.unit(a).enchants, vec!["haste".to_string()]);
        assert_eq!(h.customs, vec!["hook".to_string()]);
    }

    #[test]
    fn grant_remove_heal_and_dispel() {
        let mut h = MockHost::default();
        let a = h.spawn(0, pos(0, 0), 2, 0);
        let cx = ctx(Target::Unit(a), 0);
        let on_cast = |e| [rule(Trigger::OnCast, Condition::Always, TargetSel::This, e)];
        run_rules(
            &on_cast(Effect::GrantAbility("flying".into())),
            Trigger::OnCast,
            &cx,
            &mut h,
            &mut rng(),
        );
        assert_eq!(h.unit(a).facts.abilities, vec!["flying".to_string()]);
        run_rules(
            &on_cast(Effect::RemoveAbility("flying".into())),
            Trigger::OnCast,
            &cx,
            &mut h,
            &mut rng(),
        );
        assert!(h.unit(a).facts.abilities.is_empty());
        run_rules(
            &on_cast(Effect::Heal(3)),
            Trigger::OnCast,
            &cx,
            &mut h,
            &mut rng(),
        );
        assert_eq!(h.unit(a).hp, 5);
        run_rules(
            &on_cast(Effect::ApplyEnchantment("x".into())),
            Trigger::OnCast,
            &cx,
            &mut h,
            &mut rng(),
        );
        let out = run_rules(
            &on_cast(Effect::Dispel { strength: 50 }),
            Trigger::OnCast,
            &cx,
            &mut h,
            &mut rng(),
        );
        assert_eq!(
            out,
            vec![Outcome::Dispelled {
                who: Target::Unit(a),
                removed: 1
            }]
        );
    }

    #[test]
    fn same_seed_same_outcomes() {
        let run = || {
            let mut h = MockHost::default();
            let c = h.spawn(0, pos(0, 0), 5, 0);
            for i in 0..6 {
                h.spawn(1, pos(i, 1), 9, 5);
            }
            let rules = [rule(
                Trigger::OnCast,
                Condition::Always,
                TargetSel::AllEnemyUnits,
                Effect::Damage {
                    amount: 3,
                    kind: DamageKind::Cold,
                    resistible: true,
                    immune: Vec::new(),
                    realm: None,
                    resist_modifier: 0,
                },
            )];
            run_rules(
                &rules,
                Trigger::OnCast,
                &ctx(Target::Unit(c), 0),
                &mut h,
                &mut rng(),
            )
            .iter()
            .map(|o| matches!(o, Outcome::Resisted { .. }))
            .collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }
}
