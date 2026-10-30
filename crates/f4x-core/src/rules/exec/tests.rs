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
    rng: SeededRng,
}

/// The mock host's generator: the same seed every test starts from.
struct SeededRng(Pcg64Mcg);

impl Default for SeededRng {
    fn default() -> Self {
        SeededRng(Pcg64Mcg::seed_from_u64(7))
    }
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
            Target::City(_) | Target::CityRealm(..) => self.city.map(|c| c.1),
            Target::Tile(p) => Some(p),
            Target::Player(_) | Target::Spell(_) | Target::Hero(_) => None,
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
        let u = self.unit_mut(who).expect("unit_mut succeeds");
        let dealt = amount.min(u.hp);
        u.hp -= dealt;
        dealt
    }
    fn heal(&mut self, who: Target, amount: i32) -> i32 {
        let u = self.unit_mut(who).expect("unit_mut succeeds");
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
    fn dispel(&mut self, who: Target, strength: u32, _by: PlayerId) -> u32 {
        let u = self.unit_mut(who).expect("unit_mut succeeds");
        let n = if strength > 0 {
            u.enchants.len() as u32
        } else {
            0
        };
        u.enchants.clear();
        n
    }
    fn custom(&mut self, hook: &str, _w: Target, _c: &Context) {
        self.customs.push(hook.to_string());
    }
    fn rng(&mut self) -> &mut dyn RngCore {
        &mut self.rng.0
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
    let out = run_rules(&rules, Trigger::Passive, &cx, &mut h);
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
    assert!(run_rules(&rules, Trigger::Passive, &cx, &mut h).is_empty());
    h.grant_ability(Target::Unit(a), "blessed");
    let out = run_rules(&rules, Trigger::Passive, &cx, &mut h);
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
                traits: Default::default(),
            },
        )]
    };
    let cx = ctx(Target::Unit(caster), 0);
    let out = run_rules(&dmg(true), Trigger::OnCast, &cx, &mut h);
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
    run_rules(&dmg(false), Trigger::OnCast, &cx, &mut h);
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
                traits: Default::default(),
            },
        )]
    };
    let cx = ctx(Target::Unit(caster), 0);
    let out = run_rules(&bolt(&["fire_immunity"], 0), Trigger::OnCast, &cx, &mut h);
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
                h.rng = SeededRng(Pcg64Mcg::seed_from_u64(seed));
                let out = run_rules(
                    &bolt(&[], modifier),
                    Trigger::OnCast,
                    &ctx(Target::Unit(c), 0),
                    &mut h,
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
    run_rules(&rules, Trigger::OnCast, &cx, &mut h);
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
    );
    assert_eq!(h.unit(a).facts.abilities, vec!["flying".to_string()]);
    run_rules(
        &on_cast(Effect::RemoveAbility("flying".into())),
        Trigger::OnCast,
        &cx,
        &mut h,
    );
    assert!(h.unit(a).facts.abilities.is_empty());
    run_rules(&on_cast(Effect::Heal(3)), Trigger::OnCast, &cx, &mut h);
    assert_eq!(h.unit(a).hp, 5);
    run_rules(
        &on_cast(Effect::ApplyEnchantment("x".into())),
        Trigger::OnCast,
        &cx,
        &mut h,
    );
    let out = run_rules(
        &on_cast(Effect::Dispel { strength: 50 }),
        Trigger::OnCast,
        &cx,
        &mut h,
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
                traits: Default::default(),
            },
        )];
        run_rules(&rules, Trigger::OnCast, &ctx(Target::Unit(c), 0), &mut h)
            .iter()
            .map(|o| matches!(o, Outcome::Resisted { .. }))
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}
