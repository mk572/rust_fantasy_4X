//! The shipped content through the loader, in the tests that need the game code too (the map
//! generator, the item maker, the hero roll); the loader's own tests are in `f4x-def`.

use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Plane;
use f4x_def::content::Content;
use f4x_def::feature::FeatureKind;
use f4x_def::load::*;
use f4x_def::testdir::TestResult;
use std::path::PathBuf;

/// The game's own `content/` folder.
fn content_dir() -> PathBuf {
    f4x_def::testdir::shipped_dir()
}

/// The game's own `content/` folder, loaded.
fn real_content() -> Content {
    f4x_def::testdir::shipped().clone()
}

#[test]
fn rolled_heroes_follow_init_heroes() -> TestResult {
    use f4x_core::game::rng::GameRng;
    use f4x_core::hero::kind::roll;
    use f4x_core::hero::{HeroAbilities, Tier};
    use f4x_def::rules::PickType;
    let c = load_content_layers(&[&content_dir()])?;
    let rules = &c.rules;
    // Steps of ability a record has beyond what its template gives: each tier is a step.
    let steps = |a: &HeroAbilities| -> i32 {
        [
            a.leadership,
            a.legendary,
            a.blademaster,
            a.armsmaster,
            a.constitution,
            a.might,
            a.arcane_power,
            a.prayermaster,
            a.agility,
            a.sage,
        ]
        .iter()
        .map(|t| match t {
            Tier::None => 0,
            Tier::Normal => 1,
            Tier::Super => 2,
        })
        .sum::<i32>()
            + i32::from(a.lucky)
            + i32::from(a.charmed)
            + i32::from(a.noble)
    };
    let mut distinct = std::collections::BTreeSet::new();
    for seed in 0..200 {
        // The game's heroes are rolled once from its stream, not once for each wizard.
        let roster = roll(&mut GameRng::new(seed), rules, &Default::default());
        assert_eq!(
            roster,
            roll(&mut GameRng::new(seed), rules, &Default::default())
        );
        assert_eq!(roster.len(), 35);
        for (r, t) in roster.iter().zip(&rules.hero_templates) {
            let fixed = HeroAbilities::from_ids(&t.abilities);
            let gained = steps(&r.abilities) - steps(&fixed)
                + (i32::from(r.casting_skill) - t.casting_skill as i32);
            // A pick that finds nothing to raise (a second Arcane Power on a hero
            // without a magic attack) still counts, so never more than the picks.
            assert!(
                (0..=t.picks as i32).contains(&gained),
                "{} gained {gained} of {} picks",
                t.unit.num(),
                t.picks
            );
            assert_eq!(r.spells, t.spells);
            // Fixed abilities stay (a pick may double one).
            let a = &r.abilities;
            for name in [
                "leadership",
                "legendary",
                "blademaster",
                "armsmaster",
                "constitution",
                "might",
                "arcane_power",
                "prayermaster",
                "agility",
                "sage",
            ] {
                assert!(
                    a.tier(name) >= fixed.tier(name),
                    "{} lost {name}",
                    t.unit.num()
                );
            }
            assert!(a.lucky >= fixed.lucky && a.charmed >= fixed.charmed);
            assert!(a.noble >= fixed.noble && a.female == fixed.female);
            let martial = [
                a.leadership,
                a.legendary,
                a.blademaster,
                a.armsmaster,
                a.constitution,
                a.might,
                a.agility,
            ];
            let magic = [a.arcane_power, a.prayermaster, a.sage];
            let (f_martial, f_magic) = (
                [
                    fixed.leadership,
                    fixed.legendary,
                    fixed.blademaster,
                    fixed.armsmaster,
                    fixed.constitution,
                    fixed.might,
                    fixed.agility,
                ],
                [fixed.arcane_power, fixed.prayermaster, fixed.sage],
            );
            match t.pick_type {
                PickType::Mage => {
                    assert_eq!(
                        martial,
                        f_martial,
                        "{} drew a warrior ability",
                        t.unit.num()
                    )
                }
                PickType::Warrior => {
                    assert_eq!(magic, f_magic, "{} drew a mage ability", t.unit.num());
                    assert_eq!(i32::from(r.casting_skill), t.casting_skill as i32);
                }
                PickType::Any => {}
            }
            // The original's exclusions.
            // Arcane Power only goes to a hero with a magic ranged attack.
            let unit = rules.units.get(t.unit);
            if !unit
                .abilities
                .iter()
                .any(|x| f4x_core::hero::kind::MAGIC_RANGED.contains(x))
            {
                assert_eq!(a.arcane_power, fixed.arcane_power, "{}", t.unit.num());
            }
            if unit.abilities.contains(&f4x_def::rules::Ability::new(40)) {
                assert_eq!(
                    a.charmed,
                    fixed.charmed,
                    "{} is immune already",
                    t.unit.num()
                );
            }
        }
        distinct.insert(format!(
            "{:?}",
            roster.iter().map(|r| r.abilities).collect::<Vec<_>>()
        ));
    }
    assert!(
        distinct.len() > 100,
        "the draws are not varied: {}",
        distinct.len()
    );
    Ok(())
}

#[test]
fn shipped_content_generates_a_map() -> TestResult {
    let g = f4x_core::game::Game::with_generated_map(1, &Default::default(), &real_content())?;
    assert!(g.map().is_some());
    Ok(())
}

/// The minerals and the placed features on one plane of a generated map (a tower is on both).
fn feature_ids(
    generated: &f4x_core::map::mapgen::Generated,
    plane: Plane,
    c: &Content,
) -> Vec<(u32, u32, u16)> {
    let grid = generated.map.plane(plane);
    let mut out = Vec::new();
    for y in 0..grid.height() {
        for x in 0..grid.width() {
            if let Some(f) = grid.mineral(x as i32, y as i32) {
                out.push((x, y, f.0));
            }
        }
    }
    for p in &generated.placements {
        let def = c.features.get(p.feature).expect("a placed feature exists");
        if p.at.plane == plane || def.kind == FeatureKind::Tower {
            out.push((p.at.x, p.at.y, def.num));
        }
    }
    out
}

#[test]
fn the_buildings_that_allow_nightshade_exist() -> TestResult {
    // The real rules data, not the small test set.
    let c = load_content(&content_dir())?;
    for &id in f4x_core::city::nightshade::NIGHTSHADE_BUILDINGS {
        assert!(
            c.rules.buildings.id_numbered(id.num()).is_some(),
            "no building {}",
            id.num()
        );
    }
    Ok(())
}

#[test]
fn every_special_item_power_does_something_and_its_spell_exists() -> TestResult {
    let c = load_content(&content_dir())?;
    for power in c.rules.item_powers.defs() {
        if power.bonus.is_none() {
            assert!(
                f4x_core::item::power_is_handled(power),
                "item power {} has no effect in the rules",
                power.id
            );
        }
        if let Some(spell) = power.spell {
            assert!(
                c.rules.spells.try_get(spell).is_some(),
                "item power {} names the missing spell {}",
                power.id,
                spell.num()
            );
        }
    }
    Ok(())
}

#[test]
fn any_wizard_can_be_given_any_prefab_item_once() {
    use f4x_core::item::make::{make_item, Grade};
    let c = real_content();
    let rules = &c.rules;
    // The wizard's books play no part in generating an item, only in crafting.
    let mut rng = f4x_core::game::rng::GameRng::new(3);
    let mut taken = Vec::new();
    let mut ids = Vec::new();
    for _ in 0..2000 {
        let item = make_item(&mut rng, rules, Grade::Any, 0, &mut taken);
        assert!(item.cost > 0 && (item.prefab.is_some() || item.made.is_some()));
        if let Some(id) = item.prefab {
            assert!(!ids.contains(&id), "{id:?} given twice");
            ids.push(id);
        }
    }
    assert!(ids.len() > 150, "{}", ids.len());
    // A prefab item of a value lies between half of it and all of it.
    for seed in 0..100 {
        let mut taken = Vec::new();
        let item = make_item(
            &mut f4x_core::game::rng::GameRng::new(seed),
            rules,
            Grade::Artifact,
            1200,
            &mut taken,
        );
        if item.prefab.is_some() {
            assert!((600..=1200).contains(&item.cost), "{item:?}");
        }
    }
}

#[test]
fn random_items_stay_within_their_grade() {
    use f4x_core::item::make::{random_item, Grade};
    let c = real_content();
    let rules = &c.rules;
    for seed in 0..300 {
        let mut rng = f4x_core::game::rng::GameRng::new(seed);
        // An Enchant Item grade item has at most two plain powers, each of 200 or less.
        let item = random_item(&mut rng, rules, Grade::Enchant, 0);
        assert!(
            item.powers.is_empty() && item.cost < 300 + 2 * 400 + 100,
            "{item:?}"
        );
        let item = random_item(&mut rng, rules, Grade::Artifact, 0);
        assert!(
            item.powers.is_empty() && (!item.name.is_empty() || item.made.is_some()),
            "{item:?}"
        );
    }
}

#[test]
fn generated_maps_obey_placement_rules() -> TestResult {
    let c = real_content();
    let p = f4x_core::map::mapgen::GenParams::default();
    for seed in 0..5 {
        let map = f4x_core::map::mapgen::generate(seed, &p, &c)?;
        let towers = |plane| -> Vec<(u32, u32)> {
            feature_ids(&map, plane, &c)
                .into_iter()
                .filter(|&(_, _, id)| id == 21 /* tower */)
                .map(|(x, y, _)| (x, y))
                .collect()
        };
        // Towers: right number, identical coordinates on both planes.
        assert_eq!(
            towers(Plane::Arcanus).len(),
            p.tower_count as usize,
            "seed {seed}"
        );
        assert_eq!(towers(Plane::Arcanus), towers(Plane::Myrror), "seed {seed}");
        for plane in Plane::all() {
            let grid = map.map.plane(plane);
            let nodes = feature_ids(&map, plane, &c)
                .into_iter()
                .filter(|&(_, _, id)| {
                    c.features
                        .id_numbered(id)
                        .and_then(|f| c.features.get(f))
                        .is_some_and(|d| d.kind == FeatureKind::Node)
                })
                .count();
            assert_eq!(
                nodes,
                p.planes[plane.index()].node_count as usize,
                "seed {seed}"
            );
            // No feature sits on ocean; every feature is on terrain its rules allow.
            for (x, y, id) in feature_ids(&map, plane, &c) {
                let class = c
                    .terrains
                    .get(grid.get(x as i32, y as i32).ok_or("the value is missing")?)
                    .ok_or("the value is missing")?
                    .class;
                assert_ne!(class, TerrainClass::Ocean, "{id} on ocean, seed {seed}");
                let def = c
                    .features
                    .get(c.features.id_numbered(id).ok_or("the value is missing")?)
                    .ok_or("the value is missing")?;
                if def.kind == FeatureKind::Node {
                    // A node's square has its own base terrain, and no river runs there.
                    let realm = def.realm.ok_or("a node has a realm")?;
                    assert_eq!(
                        Some(class),
                        TerrainClass::of_node_realm(realm),
                        "{id}'s square, seed {seed}"
                    );
                    assert!(!grid.has_river(x as i32, y as i32), "{id}, seed {seed}");
                } else if def.kind != FeatureKind::Tower {
                    assert!(
                        c.features
                            .weighted(def.kind, class, plane)
                            .iter()
                            .any(|(f, _)| f.0 == id),
                        "{id} not allowed on {class:?}/{plane:?}, seed {seed}"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn myrror_has_its_own_minerals_and_more_of_them() -> TestResult {
    let c = real_content();
    let p = f4x_core::map::mapgen::GenParams::default();
    let (mut arc_minerals, mut myr_minerals) = (0, 0);
    for seed in 0..10 {
        let map = f4x_core::map::mapgen::generate(seed, &p, &c)?;
        let arcanus = feature_ids(&map, Plane::Arcanus, &c);
        let myrror = feature_ids(&map, Plane::Myrror, &c);
        let is_myrror_only = |id: u16| id == 2 /* adamantium_ore */ || id == 6 /* crysx_crystal */;
        assert!(
            arcanus.iter().all(|&(_, _, id)| !is_myrror_only(id)),
            "seed {seed}"
        );
        let is_mineral = |id: u16| {
            c.features
                .id_numbered(id)
                .and_then(|f| c.features.get(f))
                .is_some_and(|f| f.kind == FeatureKind::Mineral)
        };
        arc_minerals += arcanus.iter().filter(|&&(_, _, id)| is_mineral(id)).count();
        myr_minerals += myrror.iter().filter(|&&(_, _, id)| is_mineral(id)).count();
    }
    assert!(
        myr_minerals > arc_minerals,
        "myrror {myr_minerals} vs arcanus {arc_minerals}"
    );
    Ok(())
}

#[test]
fn myrror_only_minerals_appear_across_many_seeds() -> TestResult {
    let c = real_content();
    let p = f4x_core::map::mapgen::GenParams::default();
    let mut found = false;
    for seed in 0..10 {
        let map = f4x_core::map::mapgen::generate(seed, &p, &c)?;
        found |= feature_ids(&map, Plane::Myrror, &c)
            .iter()
            .any(|&(_, _, id)| id == 2 /* adamantium_ore */);
        if found {
            break;
        }
    }
    assert!(found, "adamantium never generated on Myrror");
    Ok(())
}
