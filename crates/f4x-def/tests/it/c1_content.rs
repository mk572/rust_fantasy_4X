//! Class 1: bad content reaches the game unnoticed.
//!
//! (a) The shipped content passes every check. (b) For each rule of the validator, one test breaks
//! exactly that thing in a copy of a shipped file and expects the loader to refuse it and to name the
//! row. A rule that never fires is a rule that is not there. Rows are changed through their types
//! (`support::edit`), so a test needs no knowledge of how the file is spelled.

use crate::support::*;
use f4x_def::feature::{FeatureDef, FeatureKind};
use f4x_def::half_int::HalfInt;
use f4x_def::issue::IssueKind;
use f4x_def::load::rules::validate_rules;
use f4x_def::load::{validate_features, validate_terrains};
use f4x_def::map::{Plane, TerrainClass, TerrainDef};
use f4x_def::rules::effects::{Condition, Effect, EffectRule, Trigger};
use f4x_def::rules::lair::LairRules;
use f4x_def::rules::*;
use f4x_def::testdir::shipped;
use serde::{de::DeserializeOwned, Serialize};

/// The problems the loader names for a copy of the shipped content with one file changed.
fn broken<T: DeserializeOwned + Serialize>(file: &str, change: impl FnOnce(&mut T)) -> Vec<String> {
    let dir = copy_of_shipped("c1");
    edit::<T>(&dir, file, change);
    problems(load(&dir))
}

/// Refused, and a problem names `row`.
fn refused<T: DeserializeOwned + Serialize>(
    file: &str,
    row: &str,
    change: impl FnOnce(&mut T),
) -> Vec<String> {
    let p = broken(file, change);
    assert!(
        p.iter().any(|l| l.contains(row)),
        "no problem names {row:?}: {p:#?}"
    );
    p
}

/// Refused with an issue of this kind in this file (and, when `detail` is given, with this word
/// among its values): the test reads the issue's parts, not its wording.
fn refused_as<T: DeserializeOwned + Serialize>(
    file: &str,
    kind: f4x_def::issue::IssueKind,
    detail: Option<&str>,
    change: impl FnOnce(&mut T),
) {
    let dir = copy_of_shipped("c1-kind");
    edit::<T>(&dir, file, change);
    let e = load(&dir).expect_err("the content should be refused");
    assert!(
        e.issues.iter().any(|i| i.kind == kind
            && i.file == file
            && detail.is_none_or(|d| i.detail.iter().any(|x| x == d))),
        "no {kind:?} issue in {file} with {detail:?}: {:#?}",
        e.issues
    );
}

/// Refused, and a problem names one of the two rows that clash.
fn refused_either<T: DeserializeOwned + Serialize>(
    file: &str,
    rows: [&str; 2],
    change: impl FnOnce(&mut T),
) {
    let p = broken(file, change);
    assert!(
        p.iter().any(|l| rows.iter().any(|r| l.contains(r))),
        "no problem names {rows:?}: {p:#?}"
    );
}

const NOWHERE: u16 = 64000;

// ---- (a) the shipped content ----

#[test]
fn the_shipped_content_has_no_problem() {
    let c = shipped();
    assert!(validate_rules(&c.rules).is_empty());
    assert!(validate_terrains(c.terrains.defs()).is_empty());
    assert!(validate_features(c.features.defs()).is_empty());
    assert!(c.settings.validate().is_empty());
}

#[test]
fn every_reference_in_the_shipped_files_resolves_even_where_the_validator_does_not_look() {
    assert_eq!(dangling(shipped()), Vec::<String>::new());
}

#[test]
fn numbers_and_ids_are_unique_in_every_shipped_table() {
    let c = shipped();
    fn unique<T: Def>(kind: &str, reg: &Registry<T>) {
        let nums: std::collections::BTreeSet<_> = reg.defs().iter().map(|d| d.num()).collect();
        let ids: std::collections::BTreeSet<_> =
            reg.defs().iter().map(|d| d.id().to_string()).collect();
        assert_eq!(nums.len(), reg.len(), "{kind}: a number is twice");
        assert_eq!(ids.len(), reg.len(), "{kind}: an id is twice");
    }
    unique("race", &c.rules.races);
    unique("unit", &c.rules.units);
    unique("ability", &c.rules.abilities);
    unique("building", &c.rules.buildings);
    unique("realm", &c.rules.realms);
    unique("spell", &c.rules.spells);
    unique("item", &c.rules.items);
    unique("item power", &c.rules.item_powers);
    unique("level path", &c.rules.level_paths);
    unique("wizard", &c.rules.wizards);
    unique("retort", &c.rules.retorts);
}

// ---- terrain and features ----

#[test]
fn a_terrain_with_a_bad_id_is_refused() {
    for bad in ["", "Grass", "gras s", "grass-land", "grüne", "a\nb", "../x"] {
        let p = broken::<Vec<TerrainDef>>("terrain.ron", |t| t[0].id = bad.to_string());
        assert!(!p.is_empty(), "{bad:?}");
    }
}

#[test]
fn a_terrain_id_or_number_used_twice_or_zero_is_refused() {
    // The file is not in the order of the numbers: the rows are found by their ids.
    let (a, b) = (
        shipped().terrains.defs()[0].id.clone(),
        shipped().terrains.defs()[1].id.clone(),
    );
    let row = |t: &[TerrainDef], id: &str| t.iter().position(|d| d.id == id).expect("a row");
    refused::<Vec<TerrainDef>>("terrain.ron", &a, |t| {
        let (i, j) = (row(t, &a), row(t, &b));
        t[j].id = t[i].id.clone();
    });
    refused_either::<Vec<TerrainDef>>("terrain.ron", [&a, &b], |t| {
        let (i, j) = (row(t, &a), row(t, &b));
        t[j].num = t[i].num;
    });
    refused::<Vec<TerrainDef>>("terrain.ron", &a, |t| {
        let i = row(t, &a);
        t[i].num = 0;
    });
}

#[test]
fn a_terrain_that_contradicts_itself_is_refused() {
    let land = shipped()
        .terrains
        .defs()
        .iter()
        .find(|t| t.class != TerrainClass::Ocean && t.walkable)
        .expect("land")
        .clone();
    let sea = shipped()
        .terrains
        .defs()
        .iter()
        .find(|t| t.class == TerrainClass::Ocean)
        .expect("sea")
        .clone();
    let of = |t: &mut Vec<TerrainDef>, id: &str| t.iter().position(|d| d.id == id).expect("a row");
    refused::<Vec<TerrainDef>>("terrain.ron", &land.id, |t| {
        let i = of(t, &land.id);
        t[i].planes.clear();
    });
    refused::<Vec<TerrainDef>>("terrain.ron", &land.id, |t| {
        let i = of(t, &land.id);
        t[i].walkable = false;
    });
    refused::<Vec<TerrainDef>>("terrain.ron", &land.id, |t| {
        let i = of(t, &land.id);
        t[i].move_cost = 0;
    });
    refused::<Vec<TerrainDef>>("terrain.ron", &sea.id, |t| {
        let i = of(t, &sea.id);
        t[i].walkable = true;
    });
}

#[test]
fn a_plane_without_a_terrain_class_the_generator_needs_is_refused() {
    for class in f4x_def::map::REQUIRED_CLASSES {
        for plane in Plane::all() {
            let p = broken::<Vec<TerrainDef>>("terrain.ron", |t| {
                t.iter_mut()
                    .filter(|d| d.class == *class)
                    .for_each(|d| d.planes.retain(|p| *p != plane));
                t.retain(|d| !d.planes.is_empty());
            });
            assert!(!p.is_empty(), "no {class:?} on {plane:?} was accepted");
        }
    }
}

#[test]
fn a_feature_that_contradicts_itself_is_refused() {
    let f = shipped()
        .features
        .defs()
        .iter()
        .find(|f| f.kind != FeatureKind::Tower)
        .expect("a feature")
        .clone();
    let other = shipped()
        .features
        .defs()
        .iter()
        .find(|d| d.id != f.id)
        .expect("another")
        .clone();
    let on = |change: &dyn Fn(&mut FeatureDef), row: &str| {
        refused::<Vec<FeatureDef>>("features.ron", row, |v| {
            let i = v.iter().position(|d| d.id == f.id).expect("a row");
            change(&mut v[i]);
        });
    };
    on(&|d| d.id = "BAD ID".into(), "BAD ID");
    on(&|d| d.num = 0, &f.id);
    on(&|d| d.spawn.clear(), &f.id);
    on(&|d| d.spawn[0].weight = 0, &f.id);
    on(&|d| d.spawn[0].planes.clear(), &f.id);
    on(&|d| d.spawn[0].terrain.clear(), &f.id);
    on(&|d| d.spawn[0].terrain = vec![TerrainClass::Ocean], &f.id);
    on(&|d| d.id = other.id.clone(), &other.id);
    on(&|d| d.num = other.num, &f.id);
}

#[test]
fn a_content_without_a_tower_or_a_feature_kind_a_plane_needs_is_refused() {
    assert!(!broken::<Vec<FeatureDef>>("features.ron", |v| v
        .retain(|f| f.kind != FeatureKind::Tower))
    .is_empty());
    for kind in [
        FeatureKind::Mineral,
        FeatureKind::Encounter,
        FeatureKind::Node,
    ] {
        for plane in Plane::all() {
            let p = broken::<Vec<FeatureDef>>("features.ron", |v| {
                for f in v.iter_mut().filter(|f| f.kind == kind) {
                    f.spawn
                        .iter_mut()
                        .for_each(|r| r.planes.retain(|p| *p != plane));
                    f.spawn.retain(|r| !r.planes.is_empty());
                }
                v.retain(|f| f.kind != kind || !f.spawn.is_empty());
            });
            assert!(!p.is_empty(), "no {kind:?} on {plane:?} was accepted");
        }
    }
}

// ---- ids of the rules ----

#[test]
fn a_rule_row_with_a_bad_or_repeated_id_is_refused() {
    let first = shipped().rules.units.defs()[0].id.clone();
    refused::<Vec<UnitDef>>("units.ron", "Not Lower", |u| u[0].id = "Not Lower".into());
    assert!(!broken::<Vec<UnitDef>>("units.ron", |u| u[0].id = String::new()).is_empty());
    let second = shipped().rules.units.defs()[1].id.clone();
    refused_either::<Vec<UnitDef>>("units.ron", [&first, &second], |u| {
        u[1].id = u[0].id.clone()
    });
    refused::<Vec<SpellDef>>("spells.ron", "", |u| u[1].id = u[0].id.clone());
    refused::<Vec<AbilityDef>>("abilities.ron", "", |u| u[1].id = u[0].id.clone());
    refused::<Vec<BuildingDef>>("buildings.ron", "", |u| u[1].id = u[0].id.clone());
    refused::<Vec<RaceDef>>("races.ron", "", |u| u[1].id = u[0].id.clone());
    refused::<Vec<RealmDef>>("realms.ron", "", |u| u[1].id = u[0].id.clone());
    refused::<Vec<ItemDef>>("items.ron", "", |u| u[1].id = u[0].id.clone());
    refused::<Vec<ItemPowerDef>>("item_powers.ron", "", |u| u[1].id = u[0].id.clone());
    refused::<Vec<LevelPathDef>>("level_paths.ron", "", |u| u[1].id = u[0].id.clone());
    refused::<Vec<RetortDef>>("retorts.ron", "", |u| u[1].id = u[0].id.clone());
    refused::<Vec<WizardDef>>("wizards.ron", "", |u| u[1].id = u[0].id.clone());
}

// ---- races ----

fn race() -> RaceDef {
    shipped().rules.races.defs()[0].clone()
}

#[test]
fn a_race_that_cannot_field_its_units_is_refused() {
    let r = race();
    refused::<Vec<RaceDef>>("races.ron", &r.id, |v| v[0].units.clear());
    refused::<Vec<RaceDef>>("races.ron", &r.id, |v| v[0].units.push(Id::new(NOWHERE)));
    refused::<Vec<RaceDef>>("races.ron", &r.id, |v| {
        v[0].buildings.push(Id::new(NOWHERE))
    });
    refused::<Vec<RaceDef>>("races.ron", &r.id, |v| {
        v[0].start_army.pop().map(|_| ()).unwrap_or(())
    });
    refused::<Vec<RaceDef>>("races.ron", &r.id, |v| {
        v[0].start_army = vec![Id::new(NOWHERE); 2]
    });
    refused::<Vec<RaceDef>>("races.ron", &r.id, |v| {
        let first = v[0].start_army[0];
        v[0].start_army.push(first);
    });
    // A unit that belongs to another race.
    let alien = shipped().rules.races.defs()[1].units[0];
    refused::<Vec<RaceDef>>("races.ron", &r.id, |v| v[0].units.push(alien));
}

#[test]
fn a_race_that_lists_a_building_its_list_of_races_excludes_is_refused() {
    let r = race();
    let this = RaceId::new(r.num);
    let b = shipped()
        .rules
        .buildings
        .defs()
        .iter()
        .find(|b| !b.races.contains(&this))
        .map(|b| (b.num, b.id.clone()));
    if let Some((num, _)) = b {
        refused::<Vec<RaceDef>>("races.ron", &r.id, |v| v[0].buildings.push(Id::new(num)));
    }
}

// ---- units ----

fn normal_unit() -> UnitDef {
    shipped()
        .rules
        .units
        .defs()
        .iter()
        .find(|u| u.kind == UnitKind::Normal && u.stats.contains_key(&Stat::Melee))
        .expect("a unit")
        .clone()
}

fn unit_at(id: &str) -> usize {
    shipped()
        .rules
        .units
        .defs()
        .iter()
        .position(|u| u.id == id)
        .expect("a row")
}

#[test]
fn a_unit_with_missing_or_impossible_stats_is_refused() {
    let u = normal_unit();
    let at = unit_at(&u.id);
    for stat in [
        Stat::Melee,
        Stat::Defense,
        Stat::Resistance,
        Stat::MaxHp,
        Stat::Figures,
        Stat::Moves,
    ] {
        refused::<Vec<UnitDef>>("units.ron", &u.id, |v| {
            v[at].stats.remove(&stat);
        });
    }
    for stat in [Stat::MaxHp, Stat::Figures, Stat::Moves] {
        refused::<Vec<UnitDef>>("units.ron", &u.id, |v| {
            v[at].stats.insert(stat, HalfInt::ZERO);
        });
    }
    refused::<Vec<UnitDef>>("units.ron", &u.id, |v| {
        v[at].stats.insert(Stat::Melee, HalfInt::from_int(-1));
    });
    refused::<Vec<UnitDef>>("units.ron", &u.id, |v| {
        v[at].stats.insert(Stat::Melee, HalfInt::from_int(100));
    });
    refused::<Vec<UnitDef>>("units.ron", &u.id, |v| {
        v[at].stats.insert(Stat::Ranged, HalfInt::from_int(3));
        v[at].stats.remove(&Stat::Ammo);
    });
}

#[test]
fn a_unit_that_names_something_that_does_not_exist_is_refused() {
    let u = normal_unit();
    let at = unit_at(&u.id);
    refused::<Vec<UnitDef>>("units.ron", &u.id, |v| v[at].race = Some(Id::new(NOWHERE)));
    refused::<Vec<UnitDef>>("units.ron", &u.id, |v| v[at].realm = Some(Id::new(NOWHERE)));
    refused::<Vec<UnitDef>>("units.ron", &u.id, |v| {
        v[at].abilities.push(Id::new(NOWHERE))
    });
    refused::<Vec<UnitDef>>("units.ron", &u.id, |v| {
        v[at].requires.push(Id::new(NOWHERE))
    });
    refused::<Vec<UnitDef>>("units.ron", &u.id, |v| {
        v[at].level_path = Some(Id::new(NOWHERE))
    });
}

#[test]
fn a_unit_of_a_kind_that_does_not_fit_its_data_is_refused() {
    let normal = normal_unit();
    let n = unit_at(&normal.id);
    let hero = shipped()
        .rules
        .units
        .defs()
        .iter()
        .find(|u| u.kind == UnitKind::Hero)
        .expect("a hero")
        .clone();
    let h = unit_at(&hero.id);
    let summoned = shipped()
        .rules
        .units
        .defs()
        .iter()
        .find(|u| u.kind == UnitKind::Summoned)
        .expect("a summoned")
        .clone();
    let s = unit_at(&summoned.id);
    let fantastic = shipped()
        .rules
        .units
        .defs()
        .iter()
        .find(|u| u.kind == UnitKind::Fantastic)
        .expect("a fantastic")
        .clone();
    let f = unit_at(&fantastic.id);
    refused::<Vec<UnitDef>>("units.ron", &normal.id, |v| v[n].race = None);
    refused::<Vec<UnitDef>>("units.ron", &normal.id, |v| v[n].hero = hero.hero.clone());
    refused::<Vec<UnitDef>>("units.ron", &hero.id, |v| v[h].race = normal.race);
    refused::<Vec<UnitDef>>("units.ron", &hero.id, |v| v[h].hero = None);
    refused::<Vec<UnitDef>>("units.ron", &summoned.id, |v| v[s].realm = None);
    refused::<Vec<UnitDef>>("units.ron", &summoned.id, |v| v[s].cost = 0);
    refused::<Vec<UnitDef>>("units.ron", &fantastic.id, |v| v[f].race = normal.race);
    refused::<Vec<UnitDef>>("units.ron", &fantastic.id, |v| {
        v[f].hero = hero.hero.clone()
    });
}

#[test]
fn a_unit_its_race_does_not_list_is_refused() {
    let u = normal_unit();
    let race_at = usize::from(u.race.expect("a race").num());
    let race_row = shipped()
        .rules
        .races
        .defs()
        .iter()
        .position(|r| r.num == race_at as u16)
        .expect("a race row");
    refused::<Vec<RaceDef>>("races.ron", &u.id, |v| {
        v[race_row].units.retain(|x| x.num() != u.num);
        v[race_row].start_army.retain(|x| x.num() != u.num);
        v[race_row].start_army = v[race_row].units.iter().take(2).copied().collect();
    });
}

// ---- buildings ----

fn building_with_a_requirement() -> BuildingDef {
    shipped()
        .rules
        .buildings
        .defs()
        .iter()
        .find(|b| !b.requires.is_empty())
        .expect("a building")
        .clone()
}

fn building_at(id: &str) -> usize {
    shipped()
        .rules
        .buildings
        .defs()
        .iter()
        .position(|b| b.id == id)
        .expect("a row")
}

#[test]
fn a_building_that_makes_no_sense_is_refused() {
    let b = building_with_a_requirement();
    let at = building_at(&b.id);
    refused::<Vec<BuildingDef>>("buildings.ron", &b.id, |v| v[at].cost = 0);
    refused::<Vec<BuildingDef>>("buildings.ron", &b.id, |v| {
        v[at].requires.push(Id::new(NOWHERE))
    });
    refused::<Vec<BuildingDef>>("buildings.ron", &b.id, |v| {
        v[at].races.push(Id::new(NOWHERE))
    });
    refused::<Vec<BuildingDef>>("buildings.ron", &b.id, |v| {
        v[at].replaces = Some(Id::new(NOWHERE))
    });
}

#[test]
fn a_building_that_requires_itself_or_a_loop_is_refused() {
    let b = building_with_a_requirement();
    let at = building_at(&b.id);
    refused::<Vec<BuildingDef>>("buildings.ron", "cycle", |v| {
        v[at].requires.push(Id::new(b.num))
    });
    let needed = b.requires[0].num();
    let other = building_at(&shipped().rules.buildings.by_num(needed).expect("needed").id);
    refused::<Vec<BuildingDef>>("buildings.ron", "cycle", |v| {
        v[other].requires.push(Id::new(b.num))
    });
}

#[test]
fn a_building_that_lists_a_race_which_cannot_build_what_it_needs_is_refused() {
    let b = building_with_a_requirement();
    let needed = b.requires[0].num();
    let needed_at = building_at(&shipped().rules.buildings.by_num(needed).expect("needed").id);
    let race = b.races.first().copied();
    if let Some(race) = race {
        refused::<Vec<BuildingDef>>("buildings.ron", &b.id, |v| {
            v[needed_at].races.retain(|r| *r != race)
        });
    }
}

// ---- spells ----

fn instant_spell() -> SpellDef {
    shipped()
        .rules
        .spells
        .defs()
        .iter()
        .find(|s| s.duration == Duration::Instant && !s.effects.is_empty())
        .expect("a spell")
        .clone()
}

fn spell_at(id: &str) -> usize {
    shipped()
        .rules
        .spells
        .defs()
        .iter()
        .position(|s| s.id == id)
        .expect("a row")
}

#[test]
fn a_spell_that_makes_no_sense_is_refused() {
    let s = instant_spell();
    let at = spell_at(&s.id);
    refused::<Vec<SpellDef>>("spells.ron", &s.id, |v| v[at].realm = Id::new(NOWHERE));
    refused::<Vec<SpellDef>>("spells.ron", &s.id, |v| v[at].cost = 0);
    refused::<Vec<SpellDef>>("spells.ron", &s.id, |v| v[at].research_cost = 0);
    refused::<Vec<SpellDef>>("spells.ron", &s.id, |v| v[at].contexts.clear());
    refused::<Vec<SpellDef>>("spells.ron", &s.id, |v| v[at].effects.clear());
    refused::<Vec<SpellDef>>("spells.ron", &s.id, |v| {
        v[at].not_with.push(Id::new(NOWHERE))
    });
    refused::<Vec<SpellDef>>("spells.ron", &s.id, |v| {
        v[at].already_able.push(Id::new(NOWHERE))
    });
    refused::<Vec<SpellDef>>("spells.ron", &s.id, |v| v[at].upkeep = 3);
    refused::<Vec<SpellDef>>("spells.ron", &s.id, |v| {
        v[at].effects.push(EffectRule {
            when: Trigger::Passive,
            condition: Condition::Always,
            target: f4x_def::rules::effects::TargetSel::This,
            effect: Effect::Heal(1),
        })
    });
}

// ---- effects ----

fn rule(effect: Effect) -> EffectRule {
    EffectRule {
        when: Trigger::OnCast,
        condition: Condition::Always,
        target: f4x_def::rules::effects::TargetSel::This,
        effect,
    }
}

#[test]
fn an_effect_that_names_something_that_does_not_exist_or_makes_no_sense_is_refused() {
    let s = instant_spell();
    let at = spell_at(&s.id);
    let realm = shipped().rules.realms.defs()[0].num;
    let with = |e: Effect| {
        refused::<Vec<SpellDef>>("spells.ron", &s.id, move |v| v[at].effects.push(rule(e)));
    };
    with(Effect::GrantAbility(Id::new(NOWHERE)));
    with(Effect::Summon {
        unit: Id::new(NOWHERE),
        count: 1,
    });
    with(Effect::Summon {
        unit: Id::new(shipped().rules.units.defs()[0].num),
        count: 0,
    });
    with(Effect::Damage {
        amount: 5,
        types: vec![],
        resistible: false,
        resist_modifier: 0,
        traits: Default::default(),
    });
    with(Effect::Damage {
        amount: 0,
        types: vec![f4x_def::rules::damage_type::DamageType::Fire],
        resistible: false,
        resist_modifier: 0,
        traits: Default::default(),
    });
    with(Effect::Damage {
        amount: -3,
        types: vec![f4x_def::rules::damage_type::DamageType::Fire],
        resistible: false,
        resist_modifier: 0,
        traits: Default::default(),
    });
    with(Effect::Damage {
        amount: 5,
        types: vec![
            f4x_def::rules::damage_type::DamageType::ChaosRealm,
            f4x_def::rules::damage_type::DamageType::LifeRealm,
        ],
        resistible: false,
        resist_modifier: 0,
        traits: Default::default(),
    });
    with(Effect::Slay {
        realm: Id::new(NOWHERE),
        modifier: 0,
        types: vec![],
        only: Default::default(),
        below: None,
        mana_per_step: 0,
    });
    with(Effect::Heal(0));
    with(Effect::Heal(-4));
    let _ = realm;
    refused::<Vec<SpellDef>>("spells.ron", &s.id, |v| {
        v[at].effects.push(EffectRule {
            condition: Condition::RealmIs(Id::new(NOWHERE)),
            ..rule(Effect::Heal(1))
        })
    });
    refused::<Vec<SpellDef>>("spells.ron", &s.id, |v| {
        v[at].effects.push(EffectRule {
            condition: Condition::Not(Box::new(Condition::RealmIs(Id::new(NOWHERE)))),
            ..rule(Effect::Heal(1))
        })
    });
}

#[test]
fn a_bad_effect_is_found_on_every_kind_of_carrier() {
    let bad = || rule(Effect::GrantAbility(Id::new(NOWHERE)));
    refused::<Vec<AbilityDef>>(
        "abilities.ron",
        &shipped().rules.abilities.defs()[0].id,
        |v| v[0].effects.push(bad()),
    );
    refused::<Vec<BuildingDef>>(
        "buildings.ron",
        &shipped().rules.buildings.defs()[0].id,
        |v| v[0].effects.push(bad()),
    );
    refused::<Vec<RaceDef>>("races.ron", &race().id, |v| v[0].effects.push(bad()));
    refused::<Vec<ItemDef>>("items.ron", &shipped().rules.items.defs()[0].id, |v| {
        v[0].effects.push(bad())
    });
    refused::<Vec<LevelPathDef>>(
        "level_paths.ron",
        &shipped().rules.level_paths.defs()[0].id,
        |v| v[0].levels[0].effects.push(bad()),
    );
    refused::<Vec<RetortDef>>("retorts.ron", &shipped().rules.retorts.defs()[0].id, |v| {
        v[0].effects.push(bad())
    });
}

// ---- wizards, retorts, realms ----

#[test]
fn a_wizard_with_unknown_picks_or_the_wrong_number_of_picks_is_refused() {
    let w = shipped().rules.wizards.defs()[0].clone();
    refused::<Vec<WizardDef>>("wizards.ron", &w.id, |v| {
        v[0].books.push((Id::new(NOWHERE), 1))
    });
    refused::<Vec<WizardDef>>("wizards.ron", &w.id, |v| {
        v[0].retorts.push(Id::new(NOWHERE))
    });
    refused::<Vec<WizardDef>>("wizards.ron", &w.id, |v| {
        let realm = v[0].books[0].0;
        v[0].books.push((realm, 1));
    });
    refused::<Vec<WizardDef>>("wizards.ron", &w.id, |v| v[0].books.clear());
}

#[test]
fn a_retort_or_realm_that_names_something_unknown_is_refused() {
    let r = shipped()
        .rules
        .retorts
        .defs()
        .iter()
        .find(|r| !r.requires.is_empty())
        .expect("a retort")
        .clone();
    let at = shipped()
        .rules
        .retorts
        .defs()
        .iter()
        .position(|x| x.id == r.id)
        .expect("row");
    refused::<Vec<RetortDef>>("retorts.ron", &r.id, |v| {
        v[at].requires = vec![RetortNeed::Books(Id::new(NOWHERE), 2)]
    });
    let realm = shipped().rules.realms.defs()[0].clone();
    refused::<Vec<RealmDef>>("realms.ron", &realm.id, |v| {
        v[0].default_spells.push(Id::new(NOWHERE))
    });
}

#[test]
fn a_retort_that_does_something_the_game_does_not_read_is_refused() {
    let r = shipped().rules.retorts.defs()[0].clone();
    refused::<Vec<RetortDef>>("retorts.ron", &r.id, |v| {
        v[0].effects.push(EffectRule {
            when: Trigger::Passive,
            condition: Condition::Always,
            target: f4x_def::rules::effects::TargetSel::This,
            effect: Effect::Modify {
                stat: Stat::Melee,
                op: ModOp::Add,
                value: HalfInt::from_int(1),
                types: Default::default(),
            },
        })
    });
}

// ---- level paths ----

#[test]
fn a_level_path_with_no_levels_or_bad_thresholds_is_refused() {
    let p = shipped().rules.level_paths.defs()[0].clone();
    refused::<Vec<LevelPathDef>>("level_paths.ron", &p.id, |v| v[0].levels.clear());
    refused::<Vec<LevelPathDef>>("level_paths.ron", &p.id, |v| v[0].levels[0].xp = 1);
    refused::<Vec<LevelPathDef>>("level_paths.ron", &p.id, |v| {
        let l = v[0].levels[1].xp;
        v[0].levels[2].xp = l;
    });
}

// ---- items ----

#[test]
fn an_item_or_a_power_that_makes_no_sense_is_refused() {
    let item = shipped()
        .rules
        .items
        .defs()
        .iter()
        .find(|i| !i.powers.is_empty())
        .expect("an item")
        .clone();
    let at = shipped()
        .rules
        .items
        .defs()
        .iter()
        .position(|i| i.id == item.id)
        .expect("row");
    refused::<Vec<ItemDef>>("items.ron", &item.id, |v| {
        v[at].powers.push(Id::new(NOWHERE))
    });
    refused::<Vec<ItemDef>>("items.ron", &item.id, |v| {
        v[at].charges = Some((Id::new(NOWHERE), 1))
    });
    refused::<Vec<ItemDef>>("items.ron", &item.id, |v| {
        v[at].charges = Some((Id::new(shipped().rules.spells.defs()[0].num), 0))
    });
    refused::<Vec<ItemDef>>("items.ron", &item.id, |v| {
        v[at].effects.clear();
        v[at].powers.clear();
        v[at].charges = None;
        v[at].spell_skill = 0;
        v[at].spell_save = 0;
    });
    let power = shipped().rules.item_powers.defs()[0].clone();
    refused::<Vec<ItemPowerDef>>("item_powers.ron", &power.id, |v| {
        v[0].realm = Some(Id::new(NOWHERE))
    });
    refused::<Vec<ItemPowerDef>>("item_powers.ron", &power.id, |v| {
        v[0].spell = Some(Id::new(NOWHERE))
    });
    refused::<Vec<ItemPowerDef>>("item_powers.ron", &power.id, |v| {
        v[0].attack = Some(Id::new(NOWHERE))
    });
    refused::<Vec<ItemPowerDef>>("item_powers.ron", &power.id, |v| v[0].kinds.clear());
}

// ---- hero templates ----

#[test]
fn a_hero_template_that_does_not_match_its_hero_is_refused() {
    let t = &shipped().rules.hero_templates[0];
    let unit = t.unit.num().to_string();
    let _ = unit;
    let normal = normal_unit();
    refused::<Vec<HeroTemplate>>("heroes.ron", "hero template", |v| {
        v[0].unit = Id::new(NOWHERE)
    });
    refused::<Vec<HeroTemplate>>("heroes.ron", "hero template", |v| {
        v[0].unit = Id::new(normal.num)
    });
    refused::<Vec<HeroTemplate>>("heroes.ron", "hero template", |v| v[1].unit = v[0].unit);
    refused::<Vec<HeroTemplate>>("heroes.ron", "hero template", |v| {
        v[0].abilities.push(Id::new(NOWHERE))
    });
    refused::<Vec<HeroTemplate>>("heroes.ron", "hero template", |v| {
        v[0].spells.push(Id::new(NOWHERE))
    });
    refused::<Vec<HeroTemplate>>("heroes.ron", "hero template", |v| {
        v[0].spells = vec![Id::new(shipped().rules.spells.defs()[0].num); 5]
    });
    refused_as::<Vec<HeroTemplate>>("heroes.ron", IssueKind::Missing, None, |v| {
        v.pop();
    });
}

// ---- ratings and efficiency ----

#[test]
fn ratings_and_efficiencies_that_are_out_of_order_unknown_or_not_positive_are_refused() {
    refused::<UnitRatings>("unit_ratings.ron", "unit_ratings", |r| r.hero_factor = 0.0);
    refused::<UnitRatings>("unit_ratings.ron", "unit_ratings", |r| {
        r.hero_factor = f32::NAN
    });
    refused::<UnitRatings>("unit_ratings.ron", "unit_ratings", |r| {
        r.hero_factor = f32::INFINITY
    });
    refused::<UnitRatings>("unit_ratings.ron", "unit_ratings", |r| {
        r.units[1].num = r.units[0].num
    });
    refused::<UnitRatings>("unit_ratings.ron", "unit_ratings", |r| {
        r.units[0].num = NOWHERE
    });
    refused::<UnitRatings>("unit_ratings.ron", "unit_ratings", |r| {
        r.units[0].levels.clear()
    });
    refused::<UnitRatings>("unit_ratings.ron", "unit_ratings", |r| {
        r.units[0].levels[0] = -1.0
    });
    refused::<UnitRatings>("unit_ratings.ron", "unit_ratings", |r| {
        r.units[0].levels[0] = f32::NAN
    });
    refused::<UnitEfficiencies>("unit_efficiency.ron", "unit_efficiency", |r| {
        r.units[1].num = r.units[0].num
    });
    refused::<UnitEfficiencies>("unit_efficiency.ron", "unit_efficiency", |r| {
        r.units[0].num = NOWHERE
    });
    refused::<UnitEfficiencies>("unit_efficiency.ron", "unit_efficiency", |r| {
        r.units[0].cost = 0.0
    });
    refused::<UnitEfficiencies>("unit_efficiency.ron", "unit_efficiency", |r| {
        r.units[0].per_100 = f32::NAN
    });
}

// ---- the other files of the rules ----

#[test]
fn lair_rules_that_cannot_be_rolled_are_refused() {
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.tower_realms.clear());
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| {
        l.tower_realms.iter_mut().for_each(|r| r.1 = 0)
    });
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.tower_realms[0].0 = NOWHERE);
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.node_other = NOWHERE);
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.hoard.clear());
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.coin_rolls = 0);
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.item_rolls = -1);
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.tower_budget.rolls = 0);
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| {
        l.second_spread = l.max_first_guards
    });
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| {
        l.max_first_guards = l.max_guards as i32 + 1
    });
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.spell_total = 0);
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.spell_total = 5);
}

#[test]
fn each_of_a_pair_of_lair_rolls_must_roll_and_the_spreads_must_differ_by_at_least_one() {
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.strong_budget[0].rolls = 0);
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.strong_budget[1].rolls = 0);
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.weak_budget[0].rolls = 0);
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.weak_budget[1].rolls = 0);
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.plane_loot[0].rolls = 0);
    refused::<LairRules>("lairs.ron", "lairs.ron", |l| l.plane_loot[1].rolls = 0);
    // One more than the first guards is enough; the same is not.
    let dir = copy_of_shipped("c1-spread");
    edit::<LairRules>(&dir, "lairs.ron", |l| {
        l.second_spread = l.max_first_guards + 1
    });
    load(&dir).expect("a spread of one is a spread");
}

#[test]
fn site_rules_that_are_not_numbers_are_refused() {
    use f4x_def::rules::ai_site::SiteRules;
    refused::<SiteRules>("ai_sites.ron", "ai_sites.ron", |s| s.mana = -1.0);
    refused::<SiteRules>("ai_sites.ron", "ai_sites.ron", |s| s.gold = f32::NAN);
    refused::<SiteRules>("ai_sites.ron", "ai_sites.ron", |s| {
        s.sea_cost = f32::INFINITY
    });
    refused::<SiteRules>("ai_sites.ron", "ai_sites.ron", |s| s.shared_share = 1.5);
    refused::<SiteRules>("ai_sites.ron", "ai_sites.ron", |s| s.enemy_range = 0);
    refused::<SiteRules>("ai_sites.ron", "ai_sites.ron", |s| {
        s.land_values[0].1 = -2.0
    });
}

#[test]
fn a_casting_range_that_does_not_rise_is_refused() {
    refused::<RulesConfig>("rules.ron", "casting_range", |c| {
        let far = c.casting_range.far;
        c.casting_range.beyond.push(far);
    });
    refused::<RulesConfig>("rules.ron", "casting_range", |c| {
        c.casting_range.beyond = vec![5, 5];
    });
    refused::<RulesConfig>("rules.ron", "casting_range", |c| {
        c.casting_range.beyond = vec![9, 3];
    });
}

// ---- settings ----

#[test]
fn settings_out_of_range_are_refused_and_named() {
    use f4x_def::settings::{SettingsDef, LEVELS, MAX_OPPONENTS};
    let s = |change: &dyn Fn(&mut SettingsDef)| {
        broken::<SettingsDef>("rules_settings.ron", |d| change(d))
    };
    assert!(!s(&|d| d.levels.truncate(LEVELS - 1)).is_empty());
    assert!(!s(&|d| d.max_opponents = 0).is_empty());
    assert!(!s(&|d| d.max_opponents = MAX_OPPONENTS + 1).is_empty());
    assert!(!s(&|d| d.new_game.opponents = d.max_opponents.saturating_add(1)).is_empty());
    assert!(!s(&|d| d.land_sizes.clear()).is_empty());
    assert!(!s(&|d| d.land_sizes[0].land_percent = 0).is_empty());
    assert!(!s(&|d| d.land_sizes[0].land_percent = 100).is_empty());
    assert!(!s(&|d| d.land_sizes[0].id = "  ".into()).is_empty());
    assert!(!s(&|d| d.land_sizes[1].id = d.land_sizes[0].id.clone()).is_empty());
    assert!(!s(&|d| d.map_sizes[1].id = d.map_sizes[0].id.clone()).is_empty());
    // The id ends a text key: no space, dot, slash, newline, capital or sign in it.
    for bad in [
        "a b",
        "a.b",
        "../x",
        "a/b",
        "a\nb",
        "Small",
        "a-b",
        "é",
        "a\0b",
        "options.land_x",
    ] {
        assert!(
            !s(&|d| d.land_sizes[0].id = bad.into()).is_empty(),
            "land size {bad:?}"
        );
        assert!(
            !s(&|d| d.map_sizes[0].id = bad.into()).is_empty(),
            "map size {bad:?}"
        );
    }
    assert!(!s(&|d| d.map_sizes.clear()).is_empty());
    assert!(!s(&|d| d.map_sizes[0].width = 0).is_empty());
    assert!(!s(&|d| d.map_sizes[0].height = u16::MAX).is_empty());
    assert!(!s(&|d| d.new_game.map_width = 0).is_empty());
    assert!(!s(&|d| d.new_game.land_percent = 0).is_empty());
    assert!(!s(&|d| d.levels[2].scoring.score_percent = 0).is_empty());
    assert!(!s(&|d| d.levels[2].rules.max_battle_side = 0).is_empty());
}

#[test]
fn a_spell_library_that_names_an_unknown_spell_realm_or_retort_is_refused() {
    use f4x_def::settings::SettingsDef;
    let p = broken::<SettingsDef>("rules_settings.ron", |d| {
        d.spell_library.always_known.push(Id::new(NOWHERE))
    });
    assert!(p.iter().any(|l| l.contains(&NOWHERE.to_string())), "{p:?}");
    let p = broken::<SettingsDef>("rules_settings.ron", |d| {
        d.spell_library
            .retort_known
            .insert(Id::new(NOWHERE), vec![]);
    });
    assert!(p.iter().any(|l| l.contains(&NOWHERE.to_string())), "{p:?}");
    let p = broken::<SettingsDef>("rules_settings.ron", |d| {
        d.spell_library.ai_main_realm_known.insert(
            Id::new(NOWHERE),
            Id::new(shipped().rules.spells.defs()[0].num),
        );
    });
    assert!(p.iter().any(|l| l.contains(&NOWHERE.to_string())), "{p:?}");
}

#[test]
fn every_problem_of_a_content_with_several_is_reported_not_only_the_first() {
    let dir = copy_of_shipped("c1-many");
    edit::<Vec<UnitDef>>(&dir, "units.ron", |v| {
        v[0].id = "Bad One".into();
        v[1].abilities.push(Id::new(NOWHERE));
        v[2].requires.push(Id::new(NOWHERE));
    });
    let p = problems(load(&dir));
    assert!(p.len() >= 3, "{p:#?}");
}

// ---- help ----

#[test]
fn a_help_with_a_repeated_topic_or_a_dangling_include_is_reported() {
    use f4x_def::help::{Help, HelpTopic};
    let topic = |index, title: &str, text: &str, include| HelpTopic {
        index,
        title: title.into(),
        text: text.into(),
        picture: None,
        include,
    };
    let fine = Help {
        topics: vec![topic(1, "a", "x", None), topic(2, "b", "y", Some(1))],
        screens: vec![],
    };
    assert!(fine.problems().is_empty());
    let twice = Help {
        topics: vec![topic(1, "a", "x", None), topic(1, "b", "y", None)],
        screens: vec![],
    };
    assert!(!twice.problems().is_empty());
    let same = Help {
        topics: vec![topic(1, "A", "x", None), topic(2, "a", "x", None)],
        screens: vec![],
    };
    assert!(!same.problems().is_empty());
    let dangling = Help {
        topics: vec![topic(1, "a", "x", Some(9))],
        screens: vec![],
    };
    assert!(!dangling.problems().is_empty());
}

// ---- what other files say about each other ----

#[test]
fn the_numbers_rules_ron_keeps_for_rows_of_other_files_must_name_rows() {
    refused_as::<RulesConfig>("rules.ron", IssueKind::Dangling, Some("building"), |c| {
        c.autobuild_order.push(Id::new(NOWHERE))
    });
    refused_as::<RulesConfig>("rules.ron", IssueKind::Dangling, Some("building"), |c| {
        c.starting_buildings.push((1, Id::new(NOWHERE)))
    });
    refused_as::<RulesConfig>("rules.ron", IssueKind::Dangling, Some("race"), |c| {
        c.limited_ai_races.push(NOWHERE)
    });
    refused_as::<RulesConfig>("rules.ron", IssueKind::Dangling, Some("feature"), |c| {
        c.event_ores.push(NOWHERE)
    });
}

#[test]
fn a_feature_that_names_a_realm_or_a_feature_that_is_not_there_is_refused() {
    let f = shipped()
        .features
        .defs()
        .iter()
        .find(|f| f.kind == FeatureKind::Mineral && f.transmute.is_some())
        .expect("a mineral")
        .clone();
    let on = |change: &dyn Fn(&mut FeatureDef)| {
        refused::<Vec<FeatureDef>>("features.ron", &f.id, |v| {
            let i = v.iter().position(|d| d.id == f.id).expect("a row");
            change(&mut v[i]);
        });
    };
    on(&|d| d.transmute = Some(NOWHERE));
    on(&|d| d.realm = Some(Id::new(NOWHERE)));
    on(&|d| d.reward = Some(Id::new(NOWHERE)));
}

#[test]
fn lair_weights_that_add_up_past_what_a_number_holds_do_not_stop_the_check() {
    let dir = copy_of_shipped("c1-weights");
    edit::<LairRules>(&dir, "lairs.ron", |l| {
        l.tower_realms.iter_mut().for_each(|r| r.1 = u32::MAX);
        l.hoard.iter_mut().for_each(|h| h.1 = u32::MAX);
    });
    // Big weights are weights: the check adds them without a panic, and the content loads.
    load(&dir).expect("big weights are fine");
}

#[test]
fn a_help_whose_includes_run_in_a_circle_or_whose_picture_is_a_path_is_reported() {
    use f4x_def::help::{Help, HelpTopic};
    let topic = |index, text: &str, include, picture: Option<&str>| HelpTopic {
        index,
        title: format!("t{index}"),
        text: text.into(),
        picture: picture.map(|f| (f.to_string(), 0)),
        include,
    };
    let own = Help {
        topics: vec![topic(1, "a", Some(1), None)],
        screens: vec![],
    };
    assert!(!own.problems().is_empty(), "a topic that includes itself");
    let circle = Help {
        topics: vec![
            topic(1, "a", Some(2), None),
            topic(2, "b", Some(3), None),
            topic(3, "c", Some(1), None),
        ],
        screens: vec![],
    };
    assert!(!circle.problems().is_empty(), "a circle of three");
    let chain = Help {
        topics: vec![
            topic(1, "a", Some(2), None),
            topic(2, "b", Some(3), None),
            topic(3, "c", None, None),
        ],
        screens: vec![],
    };
    assert!(chain.problems().is_empty(), "a chain that ends");
    for bad in [
        "../x.png",
        "a/b.png",
        "a\\b.png",
        "/etc/passwd",
        "",
        ".hidden",
        "c:x",
        "a\0b",
    ] {
        let h = Help {
            topics: vec![topic(1, "a", None, Some(bad))],
            screens: vec![],
        };
        assert!(!h.problems().is_empty(), "picture {bad:?}");
    }
    let fine = Help {
        topics: vec![topic(1, "a", None, Some("picture.png"))],
        screens: vec![],
    };
    assert!(fine.problems().is_empty());
}

#[test]
fn a_per_level_fraction_of_a_hero_ability_must_have_a_positive_denominator_a_nonnegative_numerator_and_offset_0_or_1(
) {
    let bad = |change: fn(&mut PerLevel)| {
        refused::<Vec<AbilityDef>>("abilities.ron", "might", |rows| {
            let a = rows.iter_mut().find(|a| a.id == "might").expect("might");
            let mut f = a.per_level.expect("tiered");
            change(&mut f);
            a.per_level = Some(f);
        });
    };
    bad(|f| f.denominator = 0);
    bad(|f| f.denominator = -1);
    bad(|f| f.numerator = -1);
    bad(|f| f.level_offset = 2);
    bad(|f| f.level_offset = -1);
    // The edges are fine.
    let dir = copy_of_shipped("c1-fraction");
    edit::<Vec<AbilityDef>>(&dir, "abilities.ron", |rows| {
        let a = rows.iter_mut().find(|a| a.id == "might").expect("might");
        a.per_level = Some(PerLevel {
            numerator: 0,
            denominator: 1,
            level_offset: 1,
        });
    });
    load(&dir).expect("0 over 1 with offset 1 is a fraction");
    edit::<Vec<AbilityDef>>(&dir, "abilities.ron", |rows| {
        let a = rows.iter_mut().find(|a| a.id == "might").expect("might");
        a.per_level = Some(PerLevel {
            numerator: 1,
            denominator: 1,
            level_offset: 0,
        });
    });
    load(&dir).expect("1 over 1 with offset 0 is a fraction");
}

#[test]
fn a_unit_rating_must_name_a_unit_and_hold_positive_finite_numbers() {
    let first = shipped().rules.ratings.units[0].num;
    refused::<UnitRatings>("unit_ratings.ron", "unit_ratings.ron", |r| {
        r.units.last_mut().expect("a row").num = u16::MAX;
    });
    for bad in [0.0_f32, -1.0, f32::INFINITY] {
        refused::<UnitRatings>("unit_ratings.ron", &first.to_string(), |r| {
            r.units[0].levels = vec![bad]
        });
    }
    refused::<UnitRatings>("unit_ratings.ron", &first.to_string(), |r| {
        r.units[0].levels = vec![]
    });
    let dir = copy_of_shipped("c1-rating");
    edit::<UnitRatings>(&dir, "unit_ratings.ron", |r| {
        r.units[0].levels = vec![f32::MIN_POSITIVE]
    });
    load(&dir).expect("a small positive worth is a worth");
}

#[test]
fn a_check_gives_file_row_kind_and_the_values_and_not_only_a_sentence() {
    let dir = copy_of_shipped("c1-parts");
    let unit_id = shipped().rules.units.defs()[0].id.clone();
    edit::<Vec<UnitDef>>(&dir, "units.ron", |rows| {
        rows[0].abilities.push(Id::new(NOWHERE))
    });
    let e = load(&dir).expect_err("refused");
    let issue = e
        .issues
        .iter()
        .find(|i| i.kind == IssueKind::Dangling)
        .expect("a dangling reference");
    assert_eq!(issue.file, "units.ron");
    assert!(issue.at.contains(&unit_id), "the row: {}", issue.at);
    assert_eq!(issue.detail, ["abilities", "ability", &NOWHERE.to_string()]);
    assert_eq!(issue.outcome, f4x_def::issue::Outcome::Refuse);
    // The same through the number out of range and the twice used.
    let dir = copy_of_shipped("c1-parts2");
    edit::<Vec<BuildingDef>>(&dir, "buildings.ron", |rows| rows[0].cost = 0);
    let e = load(&dir).expect_err("refused");
    let issue = e
        .issues
        .iter()
        .find(|i| i.kind == IssueKind::OutOfRange)
        .expect("out of range");
    assert_eq!(
        (
            issue.file.as_str(),
            issue.detail[0].as_str(),
            issue.detail[1].as_str()
        ),
        ("buildings.ron", "cost", "0")
    );
}
