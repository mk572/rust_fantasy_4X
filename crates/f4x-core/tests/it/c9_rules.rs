//! Class 9, the effect executor: every effect rule of the shipped content reaches the host as the
//! rule says. The host here writes down what it is asked to do; the world's own host is the
//! business of the spell tests.

use crate::support::content;
use f4x_core::map::{Plane, Position};
use f4x_core::player::PlayerId;
use f4x_core::rules::damage_type::DamageTypes;
use f4x_core::rules::effects::{Effect, EffectRule, TargetSel, Trigger};
use f4x_core::rules::exec::{
    resolve_targets, run_rules, Context, EffectHost, Facts, Outcome, Target,
};
use f4x_core::rules::realm::Realm;
use f4x_core::rules::stats::{Modifier, Source, Stat};
use f4x_core::rules::{Ability, UnitId};
use f4x_core::world::rng::GameRng;
use f4x_def::rules::Rules;
use rand::RngCore;
use std::collections::BTreeSet;

const ME: PlayerId = PlayerId(1);
const FOE: PlayerId = PlayerId(2);

fn square(x: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y: 0,
    }
}

/// Units on squares, and a note of every change asked for.
struct Host {
    units: Vec<(hecs::Entity, PlayerId, Position)>,
    /// Whether every target is immune to damage, or protected from the realm.
    immune: bool,
    protected: bool,
    /// Whether every resistance check succeeds.
    resists: bool,
    asked: Vec<String>,
    rng: GameRng,
}

impl Host {
    fn new() -> Host {
        // The entities only name the units: nothing else is asked of the store that makes them.
        let mut ids = hecs::World::new();
        let units = [(ME, 1), (ME, 2), (FOE, 2), (FOE, 3)]
            .map(|(owner, x)| (ids.spawn(()), owner, square(x)))
            .to_vec();
        Host {
            units,
            immune: false,
            protected: false,
            resists: false,
            asked: Vec::new(),
            rng: GameRng::new(1),
        }
    }

    fn unit(&self, who: Target) -> Option<&(hecs::Entity, PlayerId, Position)> {
        match who {
            Target::Unit(e) => self.units.iter().find(|u| u.0 == e),
            _ => None,
        }
    }
}

impl EffectHost for Host {
    fn facts(&self, _who: Target) -> Facts {
        Facts::default()
    }
    fn position(&self, who: Target) -> Option<Position> {
        match who {
            Target::Tile(p) => Some(p),
            _ => self.unit(who).map(|u| u.2),
        }
    }
    fn owner_of(&self, who: Target) -> Option<PlayerId> {
        self.unit(who).map(|u| u.1)
    }
    fn units(&self) -> Vec<hecs::Entity> {
        self.units.iter().map(|u| u.0).collect()
    }
    fn city_at(&self, _pos: Position) -> Option<hecs::Entity> {
        None
    }
    fn stat(&self, _who: Target, _stat: Stat) -> i32 {
        0
    }
    fn protected_from(&self, _who: Target, _realm: Option<Realm>) -> bool {
        self.protected
    }
    fn immune_to(&self, _who: Target, _types: DamageTypes) -> bool {
        self.immune
    }
    fn add_modifier(&mut self, who: Target, m: Modifier) {
        self.asked.push(format!("modify {who:?} {m:?}"));
    }
    fn grant_ability(&mut self, who: Target, ability: Ability) {
        self.asked.push(format!("grant {who:?} {ability:?}"));
    }
    fn damage(&mut self, who: Target, amount: i32, _types: DamageTypes) -> i32 {
        self.asked.push(format!("damage {who:?} {amount}"));
        amount
    }
    fn heal(&mut self, who: Target, amount: i32) -> i32 {
        self.asked.push(format!("heal {who:?} {amount}"));
        amount
    }
    fn summon(&mut self, owner: PlayerId, at: Position, unit: UnitId, count: u8) {
        self.asked
            .push(format!("summon {owner:?} {at:?} {unit:?} {count}"));
    }
    fn custom(&mut self, hook: &str, args: &[String], who: Target, _ctx: &Context) {
        self.asked.push(format!("custom {who:?} {hook} {args:?}"));
    }
    fn resistance_failure(&mut self, _resistance: i32, _modifier: i32) -> i32 {
        i32::from(!self.resists)
    }
    fn rng(&mut self) -> &mut dyn RngCore {
        &mut self.rng
    }
}

fn context(host: &Host, chosen: Option<Target>) -> Context {
    Context {
        this: Target::Unit(host.units[0].0),
        owner: ME,
        chosen,
        source: Source::Other,
    }
}

#[test]
fn each_selector_picks_the_targets_it_names() {
    let host = Host::new();
    let unit = |i: usize| Target::Unit(host.units[i].0);
    let chosen = Target::Tile(square(2));
    let ctx = context(&host, Some(chosen));
    let pick = |sel| resolve_targets(sel, &ctx, &host);
    assert_eq!(pick(TargetSel::This), vec![unit(0)]);
    assert_eq!(pick(TargetSel::Owner), vec![Target::Player(ME)]);
    assert_eq!(pick(TargetSel::AllOwnUnits), vec![unit(0), unit(1)]);
    assert_eq!(pick(TargetSel::AllEnemyUnits), vec![unit(2), unit(3)]);
    assert_eq!(pick(TargetSel::Tile), vec![Target::Tile(square(1))]);
    assert_eq!(pick(TargetSel::Chosen), vec![chosen]);
    assert_eq!(pick(TargetSel::UnitsOnChosenSquare), vec![unit(1), unit(2)]);
    // A unit chosen stands for its square.
    let by_unit = context(&host, Some(unit(3)));
    assert_eq!(
        resolve_targets(TargetSel::UnitsOnChosenSquare, &by_unit, &host),
        vec![unit(3)]
    );
    // With nothing chosen, nothing is picked.
    let none = context(&host, None);
    for sel in [TargetSel::Chosen, TargetSel::UnitsOnChosenSquare] {
        assert!(resolve_targets(sel, &none, &host).is_empty(), "{sel:?}");
    }
}

/// What a host is asked to do for an effect, as the note it writes begins.
fn verb(effect: &Effect) -> Option<&'static str> {
    Some(match effect {
        Effect::Modify { .. } => "modify",
        Effect::GrantAbility(_) => "grant",
        Effect::Damage { .. } => "damage",
        Effect::Heal(_) => "heal",
        Effect::Summon { .. } => "summon",
        Effect::Custom { .. } => "custom",
        Effect::Slay { .. } => return None,
    })
}

/// Every effect rule of the shipped definitions, with the name of what carries it.
fn shipped_rules(rules: &Rules) -> Vec<(String, &EffectRule)> {
    macro_rules! of {
        ($table:ident) => {
            rules.$table.defs().iter().flat_map(|d| {
                d.effects
                    .iter()
                    .map(move |r| (format!("{} {:?}", stringify!($table), d.id), r))
            })
        };
    }
    of!(races)
        .chain(of!(abilities))
        .chain(of!(buildings))
        .chain(of!(spells))
        .chain(of!(items))
        .collect()
}

#[test]
fn every_shipped_rule_reaches_the_host_once_for_each_target_that_passes() {
    let content = content();
    let mut seen = BTreeSet::new();
    for (carrier, rule) in shipped_rules(&content.rules) {
        let what = format!("{carrier}: {rule:?}");
        let mut host = Host::new();
        let target = match rule.target {
            TargetSel::UnitsOnChosenSquare => Target::Tile(square(2)),
            _ if matches!(rule.effect, Effect::Summon { .. }) => Target::Tile(square(5)),
            _ => Target::Unit(host.units[2].0),
        };
        let ctx = context(&host, Some(target));
        let targets = resolve_targets(rule.target, &ctx, &host);
        let passing = targets
            .into_iter()
            .filter(|&t| f4x_core::rules::exec::eval(&rule.condition, &host.facts(t)))
            .count();
        let outcomes = run_rules(std::slice::from_ref(rule), rule.when, &ctx, &mut host);
        match verb(&rule.effect) {
            // The battle carries these out itself.
            None => {
                assert!(outcomes.is_empty() && host.asked.is_empty(), "{what}");
            }
            Some(verb) => {
                // A summons needs a square to put the creatures on.
                let there = !matches!(rule.effect, Effect::Summon { .. })
                    || matches!(target, Target::Tile(_));
                let expected = if there { passing } else { 0 };
                assert_eq!(outcomes.len(), expected, "{what}: {outcomes:?}");
                assert_eq!(host.asked.len(), expected, "{what}: {:?}", host.asked);
                assert!(
                    host.asked.iter().all(|a| a.starts_with(verb)),
                    "{what}: {:?}",
                    host.asked
                );
                if expected > 0 {
                    seen.insert(verb);
                }
            }
        }
        // A rule runs only when its own trigger is asked for.
        let mut idle = Host::new();
        let wrong = if rule.when == Trigger::OnCast {
            Trigger::Passive
        } else {
            Trigger::OnCast
        };
        let other = run_rules(std::slice::from_ref(rule), wrong, &ctx, &mut idle);
        assert!(other.is_empty() && idle.asked.is_empty(), "{what}");
    }
    let all: BTreeSet<&str> = ["modify", "grant", "damage", "heal", "summon", "custom"].into();
    assert_eq!(seen, all, "an effect kind no shipped rule shows");
}

#[test]
fn damage_is_stopped_by_immunity_protection_or_a_resistance_check_that_succeeds() {
    let content = content();
    let (mut resistible, mut sure) = (0, 0);
    for spell in content.rules.spells.defs() {
        for rule in &spell.effects {
            let Effect::Damage {
                amount,
                resistible: can_resist,
                ..
            } = rule.effect
            else {
                continue;
            };
            let what = format!("spell {:?}", spell.id);
            let run = |change: fn(&mut Host)| {
                let mut host = Host::new();
                change(&mut host);
                let target = Target::Tile(square(2));
                let target = match rule.target {
                    TargetSel::UnitsOnChosenSquare => target,
                    _ => Target::Unit(host.units[2].0),
                };
                let ctx = context(&host, Some(target));
                let outcomes = run_rules(std::slice::from_ref(rule), rule.when, &ctx, &mut host);
                (outcomes, host.asked)
            };
            let (hit, asked) = run(|_| {});
            assert!(
                !hit.is_empty()
                    && hit
                        .iter()
                        .all(|o| matches!(o, Outcome::Damaged { amount: a, .. } if *a == amount)),
                "{what}: {hit:?}"
            );
            assert_eq!(asked.len(), hit.len(), "{what}");
            let stops: [fn(&mut Host); 2] = [|h| h.immune = true, |h| h.protected = true];
            for stop in stops {
                let (outcomes, asked) = run(stop);
                assert!(
                    outcomes.iter().all(|o| matches!(o, Outcome::Immune { .. }))
                        && asked.is_empty(),
                    "{what}: {outcomes:?}"
                );
            }
            let (outcomes, asked) = run(|h| h.resists = true);
            if can_resist {
                resistible += 1;
                assert!(
                    outcomes
                        .iter()
                        .all(|o| matches!(o, Outcome::Resisted { .. }))
                        && asked.is_empty(),
                    "{what}: {outcomes:?}"
                );
            } else {
                sure += 1;
                assert_eq!(outcomes, hit, "{what}");
            }
        }
    }
    assert!(resistible > 0 && sure > 0, "{resistible} and {sure}");
}
