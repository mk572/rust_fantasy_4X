//! Class 5: code and content disagree. Rust enums, constants and fixed numbers that must match
//! rows of the shipped files. Each test is generic over the enum (every variant, every row), so a
//! new variant or a new row is covered without a new test.

use f4x_def::hero::HeroAbilities;
use f4x_def::map::{Plane, TerrainClass};
use f4x_def::rules::counter_effect::{CounterEffect, COUNTER_EFFECT};
use f4x_def::rules::damage_type::DamageType;
use f4x_def::rules::global_effect::{GlobalEffect, GLOBAL_EFFECT};
use f4x_def::rules::realm::Realm;
use f4x_def::rules::unit_effect::{UnitEffect, UNIT_EFFECT};
use f4x_def::rules::{Ability, AbilityId, Id, RealmId, SpellDef, UnitKind};
use f4x_def::settings::{
    BasicSettings, GameSettings, SpellLibraryDef, DEFAULT_DIFFICULTY, LEVELS, MAX_DIFFICULTY,
};
use f4x_def::testdir::shipped;
use strum::IntoEnumIterator;

// ---- the effects a spell hook names ----

/// Every spell of the shipped rules that runs the hook, with its arguments.
fn hook_args(hook: &'static str) -> impl Iterator<Item = (&'static SpellDef, &'static [String])> {
    shipped()
        .rules
        .spells
        .defs()
        .iter()
        .filter_map(move |s| s.hook_args(hook).map(|a| (s, a)))
}

fn names_are_unique_and_round_trip(names: &[&str], named: impl Fn(&str) -> Option<String>) {
    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "a name is twice in {names:?}");
    for n in names {
        assert_eq!(named(n).as_deref(), Some(*n), "{n} does not round trip");
    }
}

#[test]
fn every_unit_effect_is_given_by_a_spell_and_every_spell_argument_is_an_effect() {
    names_are_unique_and_round_trip(UnitEffect::NAMES, |n| {
        UnitEffect::named(n).map(|e| e.name().to_string())
    });
    for name in UnitEffect::NAMES {
        let effect = UnitEffect::named(name).expect("a name");
        assert!(
            effect.spell_in(&shipped().rules).is_some(),
            "no spell gives the unit effect {name}"
        );
    }
    for (spell, args) in hook_args(UNIT_EFFECT) {
        for a in args {
            assert!(
                UnitEffect::named(a).is_some(),
                "spell {:?} names the unit effect {a}, which does not exist",
                spell.id
            );
        }
    }
}

#[test]
fn every_global_effect_is_given_by_a_spell_and_every_spell_argument_is_an_effect() {
    names_are_unique_and_round_trip(GlobalEffect::NAMES, |n| {
        GlobalEffect::named(n).map(|e| e.name().to_string())
    });
    for name in GlobalEffect::NAMES {
        let effect = GlobalEffect::named(name).expect("a name");
        assert!(
            effect.spell_in(&shipped().rules).is_some(),
            "no spell gives the global effect {name}"
        );
    }
    for (spell, args) in hook_args(GLOBAL_EFFECT) {
        for a in args {
            assert!(
                GlobalEffect::named(a).is_some(),
                "spell {:?} names the global effect {a}, which does not exist",
                spell.id
            );
        }
    }
}

#[test]
fn every_counter_effect_is_given_by_a_spell_and_every_spell_has_a_known_one() {
    names_are_unique_and_round_trip(CounterEffect::NAMES, |n| {
        CounterEffect::named(n).map(|e| e.name().to_string())
    });
    for name in CounterEffect::NAMES {
        let effect = CounterEffect::named(name).expect("a name");
        assert!(
            hook_args(COUNTER_EFFECT)
                .any(|(s, _)| CounterEffect::of(s).is_some_and(|(e, _)| e == effect)),
            "no spell gives the counter effect {name}"
        );
    }
    for (spell, args) in hook_args(COUNTER_EFFECT) {
        let first = args
            .first()
            .unwrap_or_else(|| panic!("spell {:?} has the hook without an argument", spell.id));
        assert!(
            CounterEffect::named(first).is_some(),
            "spell {:?}: {first} is no counter effect",
            spell.id
        );
    }
}

// ---- the realms ----

#[test]
fn the_realm_enum_is_the_realm_rows_in_order_and_by_name() {
    let rows = shipped().rules.realms.defs();
    for row in rows {
        let realm =
            Realm::from_num(row.num).unwrap_or_else(|| panic!("realm row {} has no Realm", row.id));
        assert_eq!(
            format!("{realm:?}").to_lowercase(),
            row.id,
            "the alias of realm {}",
            row.num
        );
        assert_eq!(realm.num(), row.num);
        assert_eq!(realm.id(), RealmId::new(row.num));
        assert_eq!(Realm::of(realm.id()), Some(realm));
        assert_eq!(realm.damage_type().realm_num(), Some(row.num));
    }
    // And no Realm without a row.
    let with_realm = (0..=u16::MAX).filter_map(Realm::from_num).count();
    assert_eq!(
        with_realm,
        rows.len(),
        "the Realm enum and realms.ron have different counts"
    );
}

#[test]
fn the_damage_types_of_the_realms_are_the_realm_rows() {
    let rows = shipped().rules.realms.defs();
    let realm_types = DamageType::iter()
        .filter(|t| t.realm_num().is_some())
        .count();
    assert_eq!(realm_types, rows.len());
    for row in rows {
        assert!(
            DamageType::of_realm(row.num).is_some(),
            "realm {} has no damage type",
            row.id
        );
    }
}

#[test]
fn the_realms_with_books_are_realms_of_the_content_and_arcane_has_none() {
    let rules = &shipped().rules;
    for realm in f4x_def::rules::BOOK_REALMS {
        assert!(
            rules.realms.try_get(*realm).is_some(),
            "book realm {} is not in realms.ron",
            realm.num()
        );
    }
    assert!(!f4x_def::rules::BOOK_REALMS.contains(&Realm::Arcane.id()));
}

#[test]
fn the_node_realms_have_a_node_terrain_in_the_content_on_both_planes() {
    let terrains = &shipped().terrains;
    for row in shipped().rules.realms.defs() {
        let class = TerrainClass::of_node_realm(RealmId::new(row.num));
        if let Some(class) = class {
            assert!(class.is_node());
            for plane in Plane::all() {
                assert!(
                    terrains.first_of_class(class, plane).is_some(),
                    "{} node has no terrain on {plane:?}",
                    row.id
                );
            }
        }
    }
}

// ---- abilities and spells the code names by number ----

fn ability_alias(n: u16) -> &'static str {
    &shipped()
        .rules
        .abilities
        .by_num(n)
        .unwrap_or_else(|| panic!("ability {n} is not in abilities.ron"))
        .id
}

#[test]
fn the_abilities_the_code_asks_for_by_number_are_those_it_says() {
    // The unit helpers `is_settler` and `is_worker` ask for these two by number.
    assert_eq!(ability_alias(12), "create_outpost");
    assert_eq!(ability_alias(42), "meld");
    let rules = &shipped().rules;
    let settlers = rules.units.defs().iter().filter(|u| u.is_settler()).count();
    assert!(settlers > 0, "no unit of the shipped content is a settler");
    for u in rules.units.defs().iter().filter(|u| u.is_settler()) {
        assert!(u.abilities.contains(&Id::new(12)));
    }
}

#[test]
fn the_hero_abilities_the_code_numbers_are_the_rows_of_their_names() {
    const TIERED: &[&str] = &[
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
    ];
    for name in TIERED {
        let n = f4x_def::hero::tiered_number(name);
        assert_eq!(ability_alias(n), *name, "the plain form of {name}");
        assert_eq!(
            ability_alias(n + 1),
            format!("super_{name}"),
            "the doubled form of {name}"
        );
        assert!(HeroAbilities::default().tier(name).is_some());
    }
    for (n, name) in [
        (39, "lucky"),
        (95, "charmed"),
        (96, "noble"),
        (97, "female"),
    ] {
        assert_eq!(ability_alias(n), name);
    }
    // The numbers of the ten abilities are the only ones that are hero abilities (besides the four).
    let heroish = shipped()
        .rules
        .abilities
        .defs()
        .iter()
        .filter(|a| HeroAbilities::is_hero_ability(AbilityId::new(a.num)))
        .count();
    assert_eq!(heroish, TIERED.len() * 2 + 4);
}

#[test]
fn hero_abilities_go_to_ids_and_back() {
    for a in shipped().rules.abilities.defs() {
        let id: Ability = Id::new(a.num);
        if HeroAbilities::is_hero_ability(id) {
            assert_eq!(HeroAbilities::from_ids(&[id]).ids(), vec![id], "{}", a.id);
        } else {
            assert!(HeroAbilities::from_ids(&[id]).ids().is_empty(), "{}", a.id);
        }
    }
}

#[test]
fn every_hero_unit_of_the_content_has_a_template_row_of_its_number() {
    let rules = &shipped().rules;
    assert!(rules.units.defs().iter().any(|u| u.kind == UnitKind::Hero));
    for u in rules
        .units
        .defs()
        .iter()
        .filter(|u| u.kind == UnitKind::Hero)
    {
        let t = rules
            .hero_templates
            .get(usize::from(u.num))
            .unwrap_or_else(|| panic!("hero {} has no template row", u.id));
        assert_eq!(t.unit.num(), u.num);
    }
}

// ---- the settings ----

#[test]
fn the_number_of_levels_the_code_counts_is_the_number_the_file_has() {
    let def = &shipped().settings;
    assert_eq!(def.levels.len(), LEVELS);
    assert_eq!(usize::from(MAX_DIFFICULTY) + 1, LEVELS);
    assert!(usize::from(DEFAULT_DIFFICULTY) < LEVELS);
}

#[test]
fn the_default_game_is_the_default_level_of_the_shipped_file() {
    let def = &shipped().settings;
    assert_eq!(
        GameSettings::default(),
        GameSettings::for_difficulty(def, DEFAULT_DIFFICULTY)
    );
    assert!(GameSettings::default().is_level(def));
}

#[test]
fn the_fallbacks_of_the_code_are_what_the_shipped_file_says() {
    let def = &shipped().settings;
    assert_eq!(
        &SpellLibraryDef::default(),
        &def.spell_library,
        "SpellLibraryDef::default"
    );
    assert_eq!(
        BasicSettings::default(),
        def.new_game,
        "BasicSettings::default"
    );
}

#[test]
fn every_spell_retort_and_realm_the_builtin_spell_library_names_exists_in_the_content() {
    let rules = &shipped().rules;
    let lib = SpellLibraryDef::default();
    let spells = lib
        .always_known
        .iter()
        .chain(lib.retort_known.values().flatten())
        .chain(&lib.arcane_closed)
        .chain(lib.ai_main_realm_known.values())
        .chain(lib.ai_main_realm_researchable.values());
    for s in spells {
        assert!(rules.spells.try_get(*s).is_some(), "spell {}", s.num());
    }
    for r in lib.retort_known.keys() {
        assert!(rules.retorts.try_get(*r).is_some(), "retort {}", r.num());
    }
    for r in lib
        .ai_main_realm_known
        .keys()
        .chain(lib.ai_main_realm_researchable.keys())
    {
        assert!(rules.realms.try_get(*r).is_some(), "realm {}", r.num());
    }
}

#[test]
fn the_built_in_rules_a_missing_optional_file_falls_back_to_are_the_shipped_files() {
    use f4x_def::rules::ai_site::SiteRules;
    use f4x_def::rules::lair::LairRules;
    let r = &shipped().rules;
    // battle.ron is not here: it may differ from the original's numbers on purpose (docs/divergences.md).
    assert!(
        r.lairs == LairRules::default(),
        "lairs.ron against LairRules::default"
    );
    assert!(
        r.ai_sites == SiteRules::default(),
        "ai_sites.ron against SiteRules::default"
    );
}
