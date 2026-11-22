//! Class 4, the small rules the definitions answer with: which features can spawn where, what a
//! condition says, what a unit's defence rules do to an attack, what a retort needs, which spells
//! stack. Each is held against its definition read straight off the shipped rows, not against a copy
//! of the code.

use f4x_def::feature::FeatureKind;
use f4x_def::half_int::HalfInt;
use f4x_def::help::{Help, HelpField, HelpScreen, HelpTopic};
use f4x_def::map::{Plane, TerrainClass};
use f4x_def::rules::damage_type::{DamageType, DamageTypes, DefenceEffect};
use f4x_def::rules::effects::{Condition, Effect, EffectRule, SpellTag, TargetSel, Trigger};
use f4x_def::rules::facts::{eval, Facts};
use f4x_def::rules::immunity::Immunities;
use f4x_def::rules::stacking::{stacking_refusal, Carrier, Stacking};
use f4x_def::rules::stats::{passive_modifiers, ModOp, Source, Stat};
use f4x_def::rules::unit_effect::UnitEffects;
use f4x_def::rules::{Ability, Id, RealmId, SpellId, UnitKind};
use f4x_def::settings::{MeleeVsFliers, Rarities, SpellLibraryDef};
use f4x_def::testdir::shipped;
use strum::IntoEnumIterator;

// ---- features ----

#[test]
fn a_feature_spawns_where_a_rule_of_it_says_and_weights_add_up() {
    let f = &shipped().features;
    let kinds = [
        FeatureKind::Mineral,
        FeatureKind::Encounter,
        FeatureKind::Node,
        FeatureKind::Tower,
    ];
    for kind in kinds {
        if let Some(id) = f.first_of_kind(kind) {
            assert_eq!(f.get(id).map(|d| d.kind), Some(kind));
            assert_eq!(
                f.defs().iter().find(|d| d.kind == kind).map(|d| d.num),
                Some(id.0),
                "the first of its kind"
            );
        }
        for terrain in [
            TerrainClass::Grassland,
            TerrainClass::Hills,
            TerrainClass::Ocean,
            TerrainClass::Swamp,
        ] {
            for plane in Plane::all() {
                let got = f.weighted(kind, terrain, plane);
                for d in f.defs().iter().filter(|d| d.kind == kind) {
                    let want: u32 = d
                        .spawn
                        .iter()
                        .filter(|r| r.terrain.contains(&terrain) && r.planes.contains(&plane))
                        .map(|r| r.weight)
                        .sum();
                    let found = got.iter().find(|(id, _)| id.0 == d.num).map(|(_, w)| *w);
                    assert_eq!(
                        found,
                        (want > 0).then_some(want),
                        "{} on {terrain:?} {plane:?}",
                        d.id
                    );
                }
                assert!(got
                    .iter()
                    .all(|(id, _)| f.get(*id).is_some_and(|d| d.kind == kind)));
            }
        }
    }
    assert!(f.first_of_kind(FeatureKind::Tower).is_some());
}

// ---- terrain classes ----

#[test]
fn a_node_realm_has_its_own_node_terrain_and_no_other_realm_has_one() {
    let classes: Vec<(u16, Option<TerrainClass>)> = shipped()
        .rules
        .realms
        .defs()
        .iter()
        .map(|r| (r.num, TerrainClass::of_node_realm(RealmId::new(r.num))))
        .collect();
    let node = |alias: &str| {
        shipped()
            .rules
            .realms
            .defs()
            .iter()
            .find(|r| r.id == alias)
            .map(|r| r.num)
            .expect("a realm")
    };
    let of = |alias: &str| {
        classes
            .iter()
            .find(|(n, _)| *n == node(alias))
            .and_then(|(_, c)| *c)
    };
    assert_eq!(of("nature"), Some(TerrainClass::NatureNode));
    assert_eq!(of("sorcery"), Some(TerrainClass::SorceryNode));
    assert_eq!(of("chaos"), Some(TerrainClass::ChaosNode));
    for other in ["life", "death", "arcane"] {
        assert_eq!(of(other), None, "{other}");
    }
    assert!(
        TerrainClass::NatureNode.is_forest()
            && TerrainClass::Forest.is_forest()
            && !TerrainClass::Hills.is_forest()
    );
    assert!(
        TerrainClass::ChaosNode.is_mountain()
            && TerrainClass::Mountains.is_mountain()
            && !TerrainClass::Hills.is_mountain()
    );
    let nodes = [
        TerrainClass::NatureNode,
        TerrainClass::SorceryNode,
        TerrainClass::ChaosNode,
    ];
    for c in [
        TerrainClass::Ocean,
        TerrainClass::Grassland,
        TerrainClass::Forest,
        TerrainClass::Hills,
        TerrainClass::Mountains,
        TerrainClass::Swamp,
        TerrainClass::Desert,
        TerrainClass::Tundra,
    ]
    .into_iter()
    .chain(nodes)
    {
        assert_eq!(c.is_node(), nodes.contains(&c), "{c:?}");
    }
}

// ---- conditions ----

#[test]
fn a_condition_is_true_of_the_facts_it_names() {
    let realm = RealmId::new(shipped().rules.realms.defs()[0].num);
    let other = RealmId::new(shipped().rules.realms.defs()[1].num);
    let facts = Facts {
        realm: Some(realm),
        kind: Some(UnitKind::Hero),
        spell_tags: vec![SpellTag::Summoning],
        abilities: vec![],
    };
    let none = Facts::default();
    assert!(eval(&Condition::Always, &facts) && eval(&Condition::Always, &none));
    assert!(
        eval(&Condition::RealmIs(realm), &facts)
            && !eval(&Condition::RealmIs(other), &facts)
            && !eval(&Condition::RealmIs(realm), &none)
    );
    assert!(
        eval(&Condition::KindIs(UnitKind::Hero), &facts)
            && !eval(&Condition::KindIs(UnitKind::Normal), &facts)
            && !eval(&Condition::KindIs(UnitKind::Hero), &none)
    );
    assert!(
        eval(&Condition::SpellIs(SpellTag::Summoning), &facts)
            && !eval(&Condition::SpellIs(SpellTag::Crafting), &facts)
            && !eval(&Condition::SpellIs(SpellTag::Summoning), &none)
    );
    for c in [
        Condition::Always,
        Condition::RealmIs(realm),
        Condition::KindIs(UnitKind::Hero),
        Condition::SpellIs(SpellTag::Crafting),
    ] {
        assert_eq!(
            eval(&Condition::Not(Box::new(c.clone())), &facts),
            !eval(&c, &facts)
        );
        assert_eq!(
            eval(
                &Condition::Not(Box::new(Condition::Not(Box::new(c.clone())))),
                &facts
            ),
            eval(&c, &facts)
        );
    }
}

// ---- passive modifiers ----

#[test]
fn only_a_passive_unconditional_modify_rule_is_a_modifier_and_it_keeps_its_amount_and_source() {
    let modify = |when, condition, value| EffectRule {
        when,
        condition,
        target: TargetSel::This,
        effect: Effect::Modify {
            stat: Stat::Melee,
            op: ModOp::Add,
            value: HalfInt::from_int(value),
            types: DamageTypes::of(&[DamageType::Fire]),
        },
    };
    let realm = RealmId::new(shipped().rules.realms.defs()[0].num);
    let rules = vec![
        modify(Trigger::Passive, Condition::Always, 1),
        modify(Trigger::OnCast, Condition::Always, 2),
        modify(Trigger::Passive, Condition::RealmIs(realm), 3),
        EffectRule {
            when: Trigger::Passive,
            condition: Condition::Always,
            target: TargetSel::This,
            effect: Effect::Heal(4),
        },
        modify(Trigger::Passive, Condition::Always, 5),
    ];
    let mods = passive_modifiers(&rules, Source::Hero);
    assert_eq!(
        mods.iter().map(|m| m.value.floor()).collect::<Vec<_>>(),
        [1, 5]
    );
    assert!(mods.iter().all(|m| m.source == Source::Hero
        && m.types.contains(DamageType::Fire)
        && m.stat == Stat::Melee));
}

// ---- unit effects ----

#[test]
fn a_set_of_unit_effects_covers_what_it_holds() {
    let spells: Vec<UnitEffects> = shipped()
        .rules
        .spells
        .defs()
        .iter()
        .map(UnitEffects::of)
        .filter(|e| !e.is_empty())
        .collect();
    assert!(
        spells.len() > 3,
        "the content has spells that change a unit"
    );
    for a in &spells {
        assert!(
            a.covers(*a)
                && a.covers(UnitEffects::NONE)
                && UnitEffects::NONE.is_empty()
                && !a.is_empty()
        );
        for b in &spells {
            let mut u = *a;
            u.union(*b);
            assert!(u.covers(*a) && u.covers(*b));
            assert_eq!(a.covers(*b), u == *a);
        }
    }
}

// ---- retorts ----

#[test]
fn a_retort_is_available_when_the_books_meet_every_need() {
    use f4x_def::rules::RetortNeed;
    use std::collections::BTreeMap;
    let rules = &shipped().rules;
    let realms: Vec<RealmId> = f4x_def::rules::BOOK_REALMS.to_vec();
    for retort in rules.retorts.defs() {
        let none: BTreeMap<RealmId, u8> = BTreeMap::new();
        assert_eq!(
            retort.available(&none),
            retort.requires.is_empty(),
            "{} with no books",
            retort.id
        );
        for need in &retort.requires {
            match need {
                RetortNeed::Books(realm, min) => {
                    let exactly: BTreeMap<RealmId, u8> = [(*realm, *min)].into();
                    let one_short: BTreeMap<RealmId, u8> = [(*realm, min.saturating_sub(1))].into();
                    if retort.requires.len() == 1 {
                        assert!(
                            retort.available(&exactly),
                            "{}: exactly the books",
                            retort.id
                        );
                        assert_eq!(
                            retort.available(&one_short),
                            *min == 0,
                            "{}: one book short",
                            retort.id
                        );
                    }
                }
                RetortNeed::Realms(count, min) => {
                    let enough: BTreeMap<RealmId, u8> = realms
                        .iter()
                        .take(usize::from(*count))
                        .map(|r| (*r, *min))
                        .collect();
                    let short: BTreeMap<RealmId, u8> = realms
                        .iter()
                        .take(usize::from(*count).saturating_sub(1))
                        .map(|r| (*r, *min))
                        .collect();
                    if retort.requires.len() == 1 {
                        assert!(retort.available(&enough), "{}: enough realms", retort.id);
                        assert!(!retort.available(&short), "{}: one realm short", retort.id);
                    }
                }
            }
        }
    }
    assert!(
        rules.retorts.defs().iter().any(|r| !r.requires.is_empty()),
        "some retort has a need"
    );
}

// ---- defence rules ----

#[test]
fn what_a_unit_takes_from_an_attack_is_what_its_defence_rules_say() {
    let rules = &shipped().rules;
    let attacks: Vec<DamageTypes> = DamageType::iter()
        .map(|t| DamageTypes::of(&[t]))
        .chain([
            DamageTypes::of(&[DamageType::Melee, DamageType::MundaneWeapon]),
            DamageTypes::of(&[DamageType::Ranged, DamageType::Magic, DamageType::Fire]),
        ])
        .collect();
    let mut with_rules = 0;
    for a in rules
        .abilities
        .defs()
        .iter()
        .filter(|a| !a.defence.is_empty())
    {
        with_rules += 1;
        let im = Immunities::of(&[Id::new(a.num)], &[], rules);
        assert!(!im.is_empty());
        for attack in &attacks {
            let hit: Vec<_> = a.defence.iter().filter(|r| r.matches(*attack)).collect();
            let strong = |e: &DefenceEffect| {
                matches!(
                    e,
                    DefenceEffect::Immune
                        | DefenceEffect::SeeThrough
                        | DefenceEffect::DefenceFloor(_)
                )
            };
            assert_eq!(
                im.stops(*attack),
                hit.iter().any(|r| r.effect == DefenceEffect::Immune),
                "{} stops",
                a.id
            );
            assert_eq!(
                im.sees_through(*attack),
                hit.iter().any(|r| r.effect == DefenceEffect::SeeThrough),
                "{} sees through",
                a.id
            );
            assert_eq!(
                im.meets(*attack),
                hit.iter().any(|r| strong(&r.effect)),
                "{} meets",
                a.id
            );
            assert_eq!(im.met_by(*attack).is_some(), im.meets(*attack));
            assert_eq!(im.stopped_by(*attack).is_some(), im.stops(*attack));
            if let Some(t) = im.stopped_by(*attack) {
                assert!(attack.contains(t));
            }
            let floors = hit.iter().filter_map(|r| match r.effect {
                DefenceEffect::DefenceFloor(n) => Some(n),
                _ => None,
            });
            assert_eq!(im.floor_against(*attack), floors.max(), "{} floor", a.id);
            let sum = |pick: fn(&DefenceEffect) -> Option<i32>| {
                hit.iter().filter_map(|r| pick(&r.effect)).sum::<i32>()
            };
            assert_eq!(
                im.defence_bonus(*attack),
                sum(|e| match e {
                    DefenceEffect::DefenceBonus(n) => Some(*n),
                    _ => None,
                }),
                "{} defence",
                a.id
            );
            assert_eq!(
                im.resistance_bonus(*attack),
                sum(|e| match e {
                    DefenceEffect::ResistanceBonus(n) => Some(*n),
                    _ => None,
                }),
                "{} resistance",
                a.id
            );
            let converted = im.convert(*attack);
            match hit.iter().find_map(|r| match r.effect {
                DefenceEffect::BecomesDoom(k) => Some(k),
                _ => None,
            }) {
                Some(keeps) => assert_eq!(
                    converted,
                    attack.both(keeps).with(DamageType::Doom),
                    "{} converts",
                    a.id
                ),
                None => assert_eq!(converted, *attack, "{} leaves it", a.id),
            }
        }
        for t in DamageType::iter() {
            let has = a
                .defence
                .iter()
                .any(|r| r.against.contains(t) && strong_effect(&r.effect));
            assert_eq!(im.has(t), has, "{} has {t:?}", a.id);
        }
    }
    assert!(
        with_rules > 5,
        "the content has abilities with defence rules"
    );
    assert!(Immunities::of(&[], &[], rules).is_empty());
    // A unit's lasting spells add their rules to its abilities'.
    let spell = rules
        .spells
        .defs()
        .iter()
        .find(|s| !s.defence.is_empty())
        .expect("a spell with defence rules");
    let both = Immunities::of(&[], &[SpellId::new(spell.num)], rules);
    assert!(!both.is_empty());
}

fn strong_effect(e: &DefenceEffect) -> bool {
    matches!(
        e,
        DefenceEffect::Immune | DefenceEffect::SeeThrough | DefenceEffect::DefenceFloor(_)
    )
}

// ---- stacking ----

#[test]
fn a_spell_a_unit_already_has_or_cannot_go_with_is_refused_in_the_order_the_rules_name() {
    let rules = &shipped().rules;
    let none = |_: SpellId| false;
    let no_ability = |_: Ability| false;
    let mut seen = [false; 3];
    for s in rules.spells.defs() {
        let id = SpellId::new(s.num);
        let carrier = |has: &dyn Fn(SpellId) -> bool| {
            let has: Box<dyn Fn(SpellId) -> bool> = Box::new({
                let f: Vec<SpellId> = rules
                    .spells
                    .defs()
                    .iter()
                    .map(|d| SpellId::new(d.num))
                    .filter(|i| has(*i))
                    .collect();
                move |i| f.contains(&i)
            });
            has
        };
        let free = Carrier {
            has_spell: &none,
            has_ability: &no_ability,
            effects: UnitEffects::NONE,
        };
        assert_eq!(
            stacking_refusal(id, s, &free, &none),
            None,
            "{} on a bare unit",
            s.id
        );
        let has_it = carrier(&|i| i == id);
        let with = Carrier {
            has_spell: &*has_it,
            has_ability: &no_ability,
            effects: UnitEffects::NONE,
        };
        assert_eq!(
            stacking_refusal(id, s, &with, &none),
            Some(Stacking::AlreadyEnchanted),
            "{}",
            s.id
        );
        let gives = UnitEffects::of(s);
        if !gives.is_empty() {
            let by_effect = Carrier {
                has_spell: &none,
                has_ability: &no_ability,
                effects: gives,
            };
            assert_eq!(
                stacking_refusal(id, s, &by_effect, &none),
                Some(Stacking::AlreadyEnchanted),
                "{} by its effect",
                s.id
            );
            seen[0] = true;
        }
        if let Some(other) = s.not_with.first().copied() {
            let on_unit = carrier(&|i| i == other);
            let c = Carrier {
                has_spell: &*on_unit,
                has_ability: &no_ability,
                effects: UnitEffects::NONE,
            };
            assert_eq!(
                stacking_refusal(id, s, &c, &none),
                Some(Stacking::NotCumulative),
                "{} with {}",
                s.id,
                other.num()
            );
            assert_eq!(
                stacking_refusal(id, s, &free, &|i| i == other),
                Some(Stacking::NotCumulative),
                "{}: the caster has it",
                s.id
            );
            seen[1] = true;
        }
        if let Some(a) = s.already_able.first().copied() {
            let able = |x: Ability| x == a;
            let c = Carrier {
                has_spell: &none,
                has_ability: &able,
                effects: UnitEffects::NONE,
            };
            assert_eq!(
                stacking_refusal(id, s, &c, &none),
                Some(Stacking::AlreadyAble),
                "{}",
                s.id
            );
            seen[2] = true;
        }
    }
    assert_eq!(
        seen, [true; 3],
        "the content has a spell for each way of being refused"
    );
    assert!(
        Stacking::AlreadyEnchanted.is_pointless()
            && Stacking::AlreadyAble.is_pointless()
            && !Stacking::NotCumulative.is_pointless()
    );
}

// ---- settings ----

#[test]
fn melee_against_a_flier_is_nothing_half_rounded_up_or_all() {
    for full in 0..=40 {
        assert_eq!(MeleeVsFliers::Zero.strength(full), 0);
        assert_eq!(MeleeVsFliers::Full.strength(full), full);
        let half = MeleeVsFliers::Half.strength(full);
        assert!((0..=1).contains(&(2 * half - full)), "{full}: {half}");
    }
}

#[test]
fn the_spell_library_row_of_a_number_of_books_is_that_row_and_the_last_for_more() {
    let lib = &shipped().settings.spell_library;
    let rows = &lib.known;
    for books in 0..=u8::MAX {
        let want = rows
            .get(usize::from(books).min(SpellLibraryDef::MAX_BOOKS))
            .copied();
        assert_eq!(
            SpellLibraryDef::row(rows, books),
            want.expect("a row"),
            "{books} books"
        );
    }
    let zero = Rarities {
        common: 0,
        uncommon: 0,
        rare: 0,
        very_rare: 0,
    };
    assert_eq!(SpellLibraryDef::row(&[], 3), zero, "no rows");
    assert_eq!(
        SpellLibraryDef::row(&rows[..2], 5),
        zero,
        "a table too short"
    );
    assert!(rows.iter().any(|r| *r != zero), "the table has figures");
}

#[test]
fn a_setting_a_file_does_not_name_takes_the_fallback_and_else_the_built_in_value() {
    use f4x_def::settings::{with_fallback, GameSettings};
    let text =
        ron::ser::to_string_pretty(&GameSettings::default(), ron::ser::PrettyConfig::default())
            .expect("written");
    let key = "combat_forecast";
    assert!(text.contains(key));
    let without: String = text
        .lines()
        .filter(|l| !l.trim_start().starts_with(key))
        .collect::<Vec<_>>()
        .join("\n");
    let mut chosen = GameSettings::default();
    chosen.rules.combat_forecast = !GameSettings::default().rules.combat_forecast;
    let named: GameSettings = ron::from_str(&text).expect("read");
    let read =
        |t: &str| ron::from_str::<GameSettings>(t).expect("a file without the setting still reads");
    assert!(!read(&without).rules.combat_forecast, "the built-in value");
    assert_eq!(
        with_fallback(chosen, || read(&without))
            .rules
            .combat_forecast,
        chosen.rules.combat_forecast,
        "the fallback's"
    );
    assert_eq!(
        with_fallback(chosen, || read(&text)).rules.combat_forecast,
        named.rules.combat_forecast,
        "a named setting wins"
    );
    // The fallback is only for the one read.
    assert!(!read(&without).rules.combat_forecast);
}

// ---- help ----

#[test]
fn a_click_opens_the_first_area_that_holds_it_and_an_area_without_help_opens_nothing() {
    let topic = |i| HelpTopic {
        index: i,
        title: format!("t{i}"),
        text: format!("x{i}"),
        picture: None,
        include: None,
    };
    let field = |help, rect| HelpField { help, rect };
    let help = Help {
        topics: vec![topic(1), topic(2)],
        screens: vec![HelpScreen {
            id: 7,
            name: "s".into(),
            fields: vec![
                field(-1, (0, 0, 9, 9)),
                field(1, (5, 5, 20, 20)),
                field(2, (10, 10, 30, 30)),
                field(9, (40, 40, 50, 50)),
            ],
        }],
    };
    assert!(
        help.at(7, 3, 3).is_none(),
        "an area of -1 holds the point and opens nothing"
    );
    assert_eq!(
        help.at(7, 10, 10).map(|(t, r)| (t.index, r)),
        Some((1, (5, 5, 20, 20))),
        "the first area, edges inside"
    );
    assert_eq!(help.at(7, 20, 20).map(|(t, _)| t.index), Some(1));
    assert_eq!(help.at(7, 21, 21).map(|(t, _)| t.index), Some(2));
    assert_eq!(help.at(7, 30, 30).map(|(t, _)| t.index), Some(2));
    assert!(
        help.at(7, 31, 31).is_none()
            && help.at(7, 4, 15).is_none()
            && help.at(7, 15, 4).is_none()
            && help.at(7, 21, 4).is_none()
            && help.at(7, 4, 21).is_none()
    );
    assert!(
        help.at(7, 45, 45).is_none(),
        "a topic that is not written opens nothing"
    );
    assert!(help.at(8, 10, 10).is_none(), "no such screen");
    assert_eq!(
        help.at_where(7, 10, 10, |h| h != 1).map(|(t, _)| t.index),
        Some(2),
        "a covered area is left out"
    );
    assert!(help.at_where(7, 10, 10, |_| false).is_none());
    assert_eq!(help.unwritten(), vec![9]);
    assert_eq!(help.topic(2).map(|t| t.index), Some(2));
    assert!(
        help.topic(3).is_none()
            && help.topic_of(None).is_none()
            && help.topic_of(Some(1)).is_some()
    );
    assert_eq!(help.screen(7).map(|s| s.id), Some(7));
}

// ---- hero abilities ----

#[test]
fn a_hero_ability_gives_nothing_without_a_tier_and_more_or_equal_at_a_higher_level() {
    use f4x_def::hero::Tier;
    let rules = &shipped().rules;
    for name in [
        "leadership",
        "legendary",
        "blademaster",
        "armsmaster",
        "constitution",
        "might",
        "arcane_power",
        "sage",
        "prayermaster",
        "agility",
    ] {
        for level in 0..6 {
            assert_eq!(rules.hero_value(name, Tier::None, level), 0);
            let (n, s) = (
                rules.hero_value(name, Tier::Normal, level),
                rules.hero_value(name, Tier::Super, level),
            );
            assert!(n >= 0 && s >= n, "{name} {level}: {n} {s}");
            if level > 0 {
                assert!(n >= rules.hero_value(name, Tier::Normal, level - 1));
            }
        }
        assert!(
            rules.hero_value(name, Tier::Super, 5) > 0,
            "{name} gives something"
        );
    }
    assert_eq!(rules.hero_value("not_an_ability", Tier::Super, 3), 0);
}

// ---- display, forecast, attack nature, defence floors, effect names ----

#[test]
fn a_half_int_is_shown_with_its_sign_only_when_negative_or_asked_for() {
    for (raw, plain, plus) in [
        (0, "0", "+0"),
        (1, "0.5", "+0.5"),
        (-1, "-0.5", "-0.5"),
        (4, "2", "+2"),
        (-5, "-2.5", "-2.5"),
    ] {
        let v = HalfInt::from_raw(raw);
        assert_eq!(format!("{v}"), plain);
        assert_eq!(format!("{v:+}"), plus);
    }
}

#[test]
fn a_worth_exactly_on_a_band_edge_is_not_beyond_it() {
    use f4x_def::rules::forecast::Forecast;
    let bands = [25, 50, 75]; // exact in binary, so the edge is exactly hit
    assert_eq!(
        Forecast::of_worths(4.0, 1.0, bands),
        Some(Forecast::MuchStronger),
        "4 to 1 is on the edge of 25 percent"
    );
    assert_eq!(
        Forecast::of_worths(1.0, 4.0, bands),
        Some(Forecast::MuchWeaker)
    );
    assert_eq!(
        Forecast::of_worths(5.0, 1.0, bands),
        Some(Forecast::OtherOutmatched)
    );
    assert_eq!(
        Forecast::of_worths(1.0, 5.0, bands),
        Some(Forecast::CompletelyOutmatched)
    );
}

#[test]
fn the_nature_of_a_unit_attack_is_what_its_abilities_add_for_that_mode() {
    use f4x_def::rules::attack_nature::{AttackNature, Mode};
    use f4x_def::rules::damage_type::Delivery;
    use f4x_def::rules::realm::Realm;
    let rules = &shipped().rules;
    let magical = AttackNature::magical(Realm::Chaos, &[DamageType::Fire]);
    assert!(
        !magical.armor_piercing
            && magical.types.contains(DamageType::Magic)
            && magical.types.contains(DamageType::Fire)
    );
    let mut seen = [false; 3];
    for a in rules.abilities.defs().iter().filter(|a| a.attack.is_some()) {
        let at = a.attack.as_ref().expect("attack");
        for mode in [Mode::Melee, Mode::ThrownOrBreath, Mode::Ranged] {
            let counts = at.delivery == Delivery::Any || mode != Mode::Melee;
            for exceptions in [false, true] {
                let got =
                    AttackNature::of_unit_attack(mode, &[Id::new(a.num)], false, exceptions, rules);
                let mut want = DamageTypes::default();
                if mode == Mode::Melee {
                    want.insert(DamageType::MundaneWeapon);
                }
                if counts {
                    want.union(DamageTypes::of(&at.types));
                    if exceptions && at.magic_immunity_exception {
                        want = want.without(DamageType::Magic);
                    }
                }
                assert_eq!(got.types, want, "{} {mode:?} exceptions {exceptions}", a.id);
                assert_eq!(
                    got.armor_piercing,
                    counts && at.armor_piercing,
                    "{} {mode:?}",
                    a.id
                );
                seen[0] |= counts && at.armor_piercing;
                seen[1] |= !counts;
                seen[2] |= counts && exceptions && at.magic_immunity_exception;
            }
            let weapon = AttackNature::of_unit_attack(mode, &[], true, false, rules);
            assert!(!weapon.types.contains(DamageType::MundaneWeapon) && !weapon.armor_piercing);
        }
    }
    assert_eq!(
        seen, [true; 3],
        "the content has an armor piercing, a ranged only and a magic exception ability"
    );
}

#[test]
fn the_defence_floor_against_an_attack_is_the_highest_floor_of_the_rules_that_match_it() {
    let rules = &shipped().rules;
    // Every rule that sets a floor, from abilities and from lasting spells.
    let mut sources: Vec<(
        Vec<Ability>,
        Vec<SpellId>,
        Vec<f4x_def::rules::damage_type::DefenceRule>,
    )> = Vec::new();
    for a in rules.abilities.defs().iter().filter(|a| {
        a.defence
            .iter()
            .any(|r| matches!(r.effect, DefenceEffect::DefenceFloor(_)))
    }) {
        sources.push((vec![Id::new(a.num)], vec![], a.defence.clone()));
    }
    for s in rules.spells.defs().iter().filter(|s| {
        s.defence
            .iter()
            .any(|r| matches!(r.effect, DefenceEffect::DefenceFloor(_)))
    }) {
        sources.push((vec![], vec![SpellId::new(s.num)], s.defence.clone()));
    }
    assert!(!sources.is_empty(), "the content has defence floors");
    let mut attacks: Vec<DamageTypes> = DamageType::iter().map(|t| DamageTypes::of(&[t])).collect();
    for r in sources.iter().flat_map(|s| s.2.iter()) {
        let mut both = r.against;
        both.union(r.with);
        attacks.push(both);
    }
    let mut two_floors_met = false;
    for (i, a) in sources.iter().enumerate() {
        for b in &sources[i..] {
            let abilities: Vec<Ability> = a.0.iter().chain(&b.0).copied().collect();
            let spells: Vec<SpellId> = a.1.iter().chain(&b.1).copied().collect();
            let im = Immunities::of(&abilities, &spells, rules);
            let rules_of_both: Vec<_> =
                a.2.iter()
                    .chain(if std::ptr::eq(a, b) {
                        [].iter()
                    } else {
                        b.2.iter()
                    })
                    .collect();
            for attack in &attacks {
                let want = rules_of_both
                    .iter()
                    .filter(|r| r.matches(*attack))
                    .filter_map(|r| match r.effect {
                        DefenceEffect::DefenceFloor(n) => Some(n),
                        _ => None,
                    })
                    .max();
                assert_eq!(im.floor_against(*attack), want);
                let n = rules_of_both
                    .iter()
                    .filter(|r| {
                        r.matches(*attack) && matches!(r.effect, DefenceEffect::DefenceFloor(_))
                    })
                    .count();
                two_floors_met |= n >= 2;
            }
        }
    }
    // The shipped content never puts two floors on one attack, a mod may: the highest holds.
    if !two_floors_met {
        use crate::support::{copy_of_shipped, edit, load};
        use f4x_def::rules::AbilityDef;
        let dir = copy_of_shipped("c6-floors");
        let mut low_high = (0, 0);
        edit::<Vec<AbilityDef>>(&dir, "abilities.ron", |rows| {
            let a = rows
                .iter_mut()
                .find(|a| {
                    a.defence
                        .iter()
                        .any(|r| matches!(r.effect, DefenceEffect::DefenceFloor(_)))
                })
                .expect("a floor");
            let mut rule = *a
                .defence
                .iter()
                .find(|r| matches!(r.effect, DefenceEffect::DefenceFloor(_)))
                .expect("a floor");
            let DefenceEffect::DefenceFloor(n) = rule.effect else {
                unreachable!()
            };
            low_high = (n, n + 3);
            rule.effect = DefenceEffect::DefenceFloor(n + 3);
            a.defence.insert(0, rule);
            let mut again = rule;
            again.effect = DefenceEffect::DefenceFloor(n - 1);
            a.defence.push(again);
            low_high.0 = n;
        });
        let content = load(&dir).expect("a mod with two floors loads");
        let a = content
            .rules
            .abilities
            .defs()
            .iter()
            .find(|a| {
                a.defence.len() >= 3
                    && a.defence
                        .iter()
                        .any(|r| r.effect == DefenceEffect::DefenceFloor(low_high.1))
            })
            .expect("the edited ability");
        let rule = a
            .defence
            .iter()
            .find(|r| r.effect == DefenceEffect::DefenceFloor(low_high.1))
            .expect("rule");
        let attack = if rule.against.is_empty() {
            rule.with
        } else {
            rule.against
        };
        let im = Immunities::of(&[Id::new(a.num)], &[], &content.rules);
        assert_eq!(
            im.floor_against(attack),
            Some(low_high.1),
            "the highest of {}, {}, {}",
            low_high.0,
            low_high.1,
            low_high.0 - 1
        );
    }
}

#[test]
fn a_unit_effect_is_in_exactly_the_spells_that_name_it() {
    use f4x_def::rules::unit_effect::UnitEffect;
    let spells = shipped().rules.spells.defs();
    for name in UnitEffect::NAMES {
        let e = UnitEffect::named(name).expect("a name");
        let carriers = spells.iter().filter(|s| e.is_in(s)).count();
        assert!(carriers >= 1, "{name}");
        assert!(carriers < spells.len(), "{name} is not in every spell");
        for s in spells.iter().filter(|s| e.is_in(s)) {
            assert!(s
                .hook_args("unit_effect")
                .is_some_and(|a| a.iter().any(|x| x == name)));
        }
    }
}

#[test]
fn a_spell_access_allows_the_wizards_it_names_and_nobody_when_off() {
    use f4x_def::settings::SpellAccess;
    for human in [false, true] {
        assert!(SpellAccess::All.allows(human));
        assert!(!SpellAccess::Off.allows(human));
    }
    assert!(SpellAccess::HumanOnly.allows(true) && !SpellAccess::HumanOnly.allows(false));
}

#[test]
fn the_limits_of_a_record_hold_at_their_edge_and_fail_one_past_it() {
    use f4x_def::settings::{BasicSettings, GameSettings, MIN_SQUARES_PER_PLAYER};
    let ok = || GameSettings::default();
    assert!(ok().validate().is_empty());
    // Raiders: the edge is allowed, one past it is not.
    let mut g = ok();
    let edge = (0..=u16::MAX)
        .rev()
        .find(|n| {
            g.world.raider_percent = *n;
            g.validate().is_empty()
        })
        .expect("an allowed value");
    g.world.raider_percent = edge;
    assert!(g.validate().is_empty());
    g.world.raider_percent = edge + 1;
    assert!(
        !g.validate().is_empty(),
        "one past the largest raider percent"
    );
    // Crowding: with one opponent each wizard has width times height squares.
    let mut b = BasicSettings {
        opponents: 1,
        map_width: 30,
        ..BasicSettings::default()
    };
    b.map_height = u16::try_from(MIN_SQUARES_PER_PLAYER / 30).expect("fits");
    assert_eq!(
        b.squares_per_player(),
        MIN_SQUARES_PER_PLAYER,
        "30 times a row of the minimum"
    );
    assert!(b.crowding().is_none(), "exactly the minimum is not crowded");
    b.map_height -= 1;
    assert_eq!(b.crowding(), Some(b.squares_per_player()));
}

#[test]
fn a_hero_with_an_ability_has_its_modifier_and_a_modifier_of_nothing_is_left_out() {
    use f4x_def::hero::HeroAbilities;
    let rules = &shipped().rules;
    let none = HeroAbilities::default();
    for level in 0..6 {
        assert!(
            none.modifiers(rules, level, true)
                .iter()
                .all(|m| m.value != HalfInt::ZERO),
            "level {level}"
        );
    }
    let might = f4x_def::hero::tiered_number("might");
    let with = HeroAbilities::from_ids(&[Id::new(might)]);
    let mods = with.modifiers(rules, 3, false);
    let melee = mods
        .iter()
        .find(|m| m.stat == Stat::Melee)
        .expect("a Might modifier");
    assert_eq!(
        melee.value.floor(),
        rules.hero_value("might", with.tier("might").expect("tier"), 3)
    );
    assert!(melee.value != HalfInt::ZERO && mods.iter().all(|m| m.value != HalfInt::ZERO));
}
