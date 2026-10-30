//! Hand-built content for unit tests (the shipped files are tested in f4x-data).

use crate::content::Content;
use crate::feature::def::{CityBonus, FeatureDef, FeatureKind, Features, SpawnRule};
use crate::map::terrain::{TerrainClass, TerrainDef, Terrains};
use crate::map::Plane;
use crate::rules::{Registry, Rules, UnitDef};

pub fn content() -> Content {
    let terrain = |id: &str, class: TerrainClass, walkable, can_found_city| TerrainDef {
        id: id.into(),
        num: 0,
        name: id.into(),
        class,
        walkable,
        can_found_city,
        move_cost: match class {
            TerrainClass::Forest
            | TerrainClass::NatureNode
            | TerrainClass::Hills
            | TerrainClass::Swamp => 2,
            TerrainClass::Mountains | TerrainClass::ChaosNode => 3,
            _ => 1,
        },
        food: match class {
            TerrainClass::Grassland => 3,
            TerrainClass::SorceryNode => 4,
            TerrainClass::NatureNode => 5,
            TerrainClass::Forest | TerrainClass::Hills | TerrainClass::Swamp => 1,
            _ => 0,
        },
        production_percent: match class {
            TerrainClass::Forest | TerrainClass::NatureNode | TerrainClass::Hills => 3,
            TerrainClass::Mountains | TerrainClass::ChaosNode => 5,
            _ => 0,
        },
        planes: Plane::ALL.to_vec(),
    };
    let mut terrains = Terrains::new(vec![
        terrain("ocean", TerrainClass::Ocean, false, false),
        terrain("grassland", TerrainClass::Grassland, true, true),
        terrain("forest", TerrainClass::Forest, true, true),
        terrain("hills", TerrainClass::Hills, true, true),
        terrain("mountains", TerrainClass::Mountains, true, false),
        terrain("swamp", TerrainClass::Swamp, true, true),
        terrain("desert", TerrainClass::Desert, true, true),
        terrain("tundra", TerrainClass::Tundra, true, true),
        terrain("sorcery_node", TerrainClass::SorceryNode, true, true),
        terrain("nature_node", TerrainClass::NatureNode, true, true),
        terrain("chaos_node", TerrainClass::ChaosNode, true, true),
    ]);
    for (i, d) in terrains.defs_mut().iter_mut().enumerate() {
        d.num = i as u16 + 1;
    }
    let land = vec![
        TerrainClass::Grassland,
        TerrainClass::Forest,
        TerrainClass::Hills,
        TerrainClass::Mountains,
        TerrainClass::Swamp,
        TerrainClass::Desert,
        TerrainClass::Tundra,
    ];
    let feature = |id: &str, kind, terrain: Vec<TerrainClass>, planes: Vec<Plane>| FeatureDef {
        id: id.into(),
        num: 0,
        name: id.into(),
        kind,
        city: Default::default(),
        spawn: if terrain.is_empty() {
            vec![]
        } else {
            vec![SpawnRule {
                terrain,
                planes,
                weight: 1,
            }]
        },
    };
    let mut features = Features::new(vec![
        feature(
            "iron",
            FeatureKind::Mineral,
            vec![TerrainClass::Hills, TerrainClass::Mountains],
            Plane::ALL.to_vec(),
        ),
        feature(
            "mithril_ore",
            FeatureKind::Mineral,
            vec![TerrainClass::Hills, TerrainClass::Mountains],
            Plane::ALL.to_vec(),
        ),
        feature(
            "adamantium_ore",
            FeatureKind::Mineral,
            vec![TerrainClass::Hills, TerrainClass::Mountains],
            vec![Plane::Myrror],
        ),
        feature(
            "lair",
            FeatureKind::Encounter,
            land.clone(),
            Plane::ALL.to_vec(),
        ),
        feature("tower", FeatureKind::Tower, vec![], vec![]),
        feature("sorcery_node", FeatureKind::Node, land, Plane::ALL.to_vec()),
    ]);
    for (i, d) in features.defs_mut().iter_mut().enumerate() {
        d.num = i as u16 + 1;
    }
    Content {
        terrains,
        features,
        settings: settings(),
        ..Default::default()
    }
}

/// The game's real abilities (`content/abilities.ron`).
pub fn real_abilities() -> Vec<crate::rules::AbilityDef> {
    let mut abilities: Vec<crate::rules::AbilityDef> =
        ron::from_str(include_str!("../../../../content/abilities.ron"))
            .expect("the game's abilities parse");
    let names = name_table(include_str!("../../../../content/text/en/name/ability.ron"));
    for d in &mut abilities {
        d.name = names[&d.num].clone();
    }
    abilities
}

/// The shipped `content/settings.ron`, for tests that play a difficulty level.
pub fn settings() -> crate::game::setting::SettingsDef {
    ron::from_str(include_str!("../../../../content/settings.ron")).expect("settings.ron parses")
}

/// Test content plus the game's real units, abilities and experience levels.
pub fn content_with_units() -> Content {
    let mut c = content();
    c.rules = Rules {
        units: Registry::new(real_units()),
        abilities: Registry::new(real_abilities()),
        level_paths: Registry::new(
            ron::from_str(include_str!("../../../../content/level_paths.ron"))
                .expect("the game's level paths parse"),
        ),
        ..Default::default()
    };
    c
}

/// Test content with a few races, buildings, mines and city enchantments, for economy tests.
pub fn content_with_economy() -> Content {
    let mut c = content_with_units();
    let buildings = real_buildings();
    c.rules.races = Registry::new(real_races());
    c.rules.buildings = Registry::new_sorted(buildings);
    c.rules.spells = Registry::new(real_spells());
    c.rules.realms = Registry::new(real_realms());
    // Features with city bonuses.
    let mut defs: Vec<FeatureDef> = c.features.defs().to_vec();
    for d in defs.iter_mut() {
        match d.id.as_str() {
            "iron" => {
                d.city = CityBonus {
                    unit_cost_percent: 5,
                    outpost_growth: 5,
                    ..Default::default()
                }
            }
            "mithril_ore" => {
                d.city = CityBonus {
                    mana: 1,
                    outpost_growth: 10,
                    ..Default::default()
                }
            }
            "adamantium_ore" => {
                d.city = CityBonus {
                    mana: 2,
                    outpost_growth: 10,
                    ..Default::default()
                }
            }
            _ => {}
        }
    }
    defs.push(FeatureDef {
        num: defs.len() as u16 + 1,
        id: "gold".into(),
        name: "gold".into(),
        kind: FeatureKind::Mineral,
        city: CityBonus {
            gold: 3,
            outpost_growth: 10,
            ..Default::default()
        },
        spawn: vec![],
    });
    defs.push(FeatureDef {
        num: defs.len() as u16 + 1,
        id: "game".into(),
        name: "game".into(),
        kind: FeatureKind::Mineral,
        city: CityBonus {
            food: 2,
            outpost_growth: 10,
            ..Default::default()
        },
        spawn: vec![],
    });
    c.features = Features::new(defs);
    let rules: crate::rules::RulesConfig =
        ron::from_str(include_str!("../../../../content/rules.ron")).expect("rules.ron parses");
    c.rules.config.racial_unrest = rules.racial_unrest;
    c
}

/// The game's real spells (`content/spells.ron`), for tests that cast or look at them.
pub fn real_spells() -> Vec<crate::rules::SpellDef> {
    let mut spells: Vec<crate::rules::SpellDef> =
        ron::from_str(include_str!("../../../../content/spells.ron"))
            .expect("the game's spells parse");
    let names = name_table(include_str!("../../../../content/text/en/name/spell.ron"));
    for d in &mut spells {
        d.name = names[&d.num].clone();
    }
    spells
}

/// The names of one table (`content/text/en/name/*.ron`) by definition number.
fn name_table(src: &str) -> std::collections::BTreeMap<u16, String> {
    ron::from_str(src).expect("the game's names parse")
}

/// All the game's real units (`content/units.ron`), in the original's order.
pub fn real_units() -> Vec<UnitDef> {
    let mut units: Vec<UnitDef> = ron::from_str(include_str!("../../../../content/units.ron"))
        .expect("the game's units parse");
    let names = name_table(include_str!("../../../../content/text/en/name/unit.ron"));
    for d in &mut units {
        d.name = names[&d.num].clone();
    }
    units
}

/// Put one of the game's real units among `units` unless it is already there.
pub fn add_real_unit(units: &mut Vec<UnitDef>, id: &str) {
    if !units.iter().any(|u| u.id == id) {
        units.push(real_unit(id));
    }
}

/// One of the game's real units (`content/units.ron`) by its alias, for a test that needs the unit
/// itself rather than a stand-in. It carries its own number, so push it among the fixtures with
/// `Registry::new_sorted`.
pub fn real_unit(id: &str) -> UnitDef {
    real_units()
        .into_iter()
        .find(|u| u.id == id)
        .unwrap_or_else(|| panic!("no real unit {id}"))
}

/// The game's 35 real hero templates (`content/heroes.ron`).
pub fn real_hero_templates() -> Vec<crate::rules::HeroTemplate> {
    ron::from_str(include_str!("../../../../content/heroes.ron")).expect("the game's heroes parse")
}

/// The game's real races (`content/races.ron`).
pub fn real_races() -> Vec<crate::rules::RaceDef> {
    let mut races: Vec<crate::rules::RaceDef> =
        ron::from_str(include_str!("../../../../content/races.ron"))
            .expect("the game's races parse");
    let names: std::collections::BTreeMap<u16, (String, String)> =
        ron::from_str(include_str!("../../../../content/text/en/name/race.ron"))
            .expect("the game's race names parse");
    for d in &mut races {
        (d.name, d.plural) = names[&d.num].clone();
    }
    races
}

/// The game's real buildings (`content/buildings.ron`).
pub fn real_buildings() -> Vec<crate::rules::BuildingDef> {
    let mut buildings: Vec<crate::rules::BuildingDef> =
        ron::from_str(include_str!("../../../../content/buildings.ron"))
            .expect("the game's buildings parse");
    let names = name_table(include_str!(
        "../../../../content/text/en/name/building.ron"
    ));
    for d in &mut buildings {
        d.name = names[&d.num].clone();
    }
    buildings
}

/// Economy content plus the game's real spells and realms (`content/spells.ron` and
/// `content/realms.ron`), so a test casts the same spell the game does.
pub fn content_with_magic() -> Content {
    content_with_economy()
}

/// The game's real realms (`content/realms.ron`).
pub fn real_realms() -> Vec<crate::rules::RealmDef> {
    let mut realms: Vec<crate::rules::RealmDef> =
        ron::from_str(include_str!("../../../../content/realms.ron"))
            .expect("the game's realms parse");
    let names = name_table(include_str!("../../../../content/text/en/name/realm.ron"));
    for d in &mut realms {
        d.name = names[&d.num].clone();
    }
    realms
}

/// Fixture item powers of a test, numbered from 0 in the order given.
pub fn numbered_powers(
    mut powers: Vec<crate::rules::ItemPowerDef>,
) -> Vec<crate::rules::ItemPowerDef> {
    for (i, p) in powers.iter_mut().enumerate() {
        p.num = i as u16;
    }
    powers
}

/// Fixture items of a test, numbered from 0 in the order given.
pub fn numbered_items(mut items: Vec<crate::rules::ItemDef>) -> Vec<crate::rules::ItemDef> {
    for (i, p) in items.iter_mut().enumerate() {
        p.num = i as u16;
    }
    items
}

/// An item power for tests that is not a plain bonus: the lasting spell and the attack ability
/// it gives the carrier (either may be none).
pub fn item_power(
    id: &str,
    spell: Option<&str>,
    attack: Option<&str>,
) -> crate::rules::ItemPowerDef {
    crate::rules::ItemPowerDef {
        num: 0,
        id: id.into(),
        name: id.into(),
        group: 9,
        cost: 0,
        kinds: Vec::new(),
        realm: None,
        books: 0,
        bonus: None,
        spell: spell.map(Into::into),
        attack: attack.map(Into::into),
    }
}
