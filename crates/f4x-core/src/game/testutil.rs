//! Content for unit tests: [`shipped`] is the game's own `content/`, and [`num`] names the numbers
//! of the terrains and map features tests use.
//!
//! Tests: none, because this is test support.

use crate::content::Content;
use crate::feature::def::{FeatureDef, Features};
use crate::map::terrain::{TerrainDef, Terrains};
use crate::rules::{Registry, Rules, UnitDef};

/// The numbers (`num` in `content/`) of the terrains and map features that tests name. The number
/// is the one source of truth, as in a save; the constant's name only tells the reader which
/// English-named terrain or feature the shipped content has under it today. If a name is
/// changed or translated, the number still holds.
pub mod num {
    pub mod terrain {
        pub const CHAOS_NODE: u16 = 1;
        pub const FOREST: u16 = 3;
        pub const GRASSLAND: u16 = 4;
        pub const HILLS: u16 = 5;
        pub const MOUNTAINS: u16 = 6;
        pub const NATURE_NODE: u16 = 7;
        pub const OCEAN: u16 = 8;
        pub const SORCERY_NODE: u16 = 9;
        pub const SWAMP: u16 = 10;
    }

    pub mod feature {
        pub const ADAMANTIUM_ORE: u16 = 2;
        pub const COAL: u16 = 5;
        pub const GEM: u16 = 9;
        pub const GOLD_ORE: u16 = 10;
        pub const IRON_ORE: u16 = 11;
        pub const MITHRIL_ORE: u16 = 12;
        pub const MONSTER_LAIR: u16 = 13;
        pub const NIGHTSHADE: u16 = 16;
        pub const SILVER_ORE: u16 = 19;
        pub const SORCERY_NODE: u16 = 20;
        pub const TOWER: u16 = 21;
        pub const WILD_GAME: u16 = 22;
    }
}

/// The shipped `content/settings.ron`, for tests that play a difficulty level.
pub fn settings() -> crate::game::setting::SettingsDef {
    ron::from_str(include_str!("../../../../content/settings.ron")).expect("settings.ron parses")
}

/// The game's own `content/` folder, read as `f4x_data::load_content` reads it (the names from
/// `text/en/name/`, the city names, the ready-made wizards' names), without its checks and
/// without the messages. It is parsed once per test process; `f4x-data` tests the loader.
pub fn shipped() -> Content {
    static SHIPPED: std::sync::OnceLock<Content> = std::sync::OnceLock::new();
    SHIPPED.get_or_init(load_shipped).clone()
}

fn load_shipped() -> Content {
    use serde::de::DeserializeOwned;
    use std::collections::BTreeMap;

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    fn parse<T: DeserializeOwned>(dir: &std::path::Path, name: &str) -> T {
        let text = std::fs::read_to_string(dir.join(name))
            .unwrap_or_else(|e| panic!("content/{name}: {e}"));
        ron::from_str(&text).unwrap_or_else(|e| panic!("content/{name}: {e}"))
    }
    /// Put `text/en/name/<file>.ron` on the definitions, by their numbers.
    fn named<T, V: DeserializeOwned + Clone>(
        dir: &std::path::Path,
        file: &str,
        defs: &mut [T],
        num: fn(&T) -> u16,
        set: fn(&mut T, V),
    ) {
        let names: BTreeMap<u16, V> = parse(dir, &format!("text/en/name/{file}.ron"));
        for d in defs {
            let name = names
                .get(&num(d))
                .unwrap_or_else(|| panic!("text/en/name/{file}.ron has no name for {}", num(d)));
            set(d, name.clone());
        }
    }

    let mut terrains: Vec<TerrainDef> = parse(&dir, "terrain.ron");
    named(
        &dir,
        "terrain",
        &mut terrains,
        |d| d.num,
        |d, n: String| d.name = n,
    );
    let mut features: Vec<FeatureDef> = parse(&dir, "features.ron");
    named(
        &dir,
        "feature",
        &mut features,
        |d| d.num,
        |d, n: String| d.name = n,
    );
    let mut races: Vec<crate::rules::RaceDef> = parse(&dir, "races.ron");
    named(
        &dir,
        "race",
        &mut races,
        |d| d.num,
        |d, n: (String, String)| {
            d.name = n.0;
            d.plural = n.1;
        },
    );
    #[derive(serde::Deserialize)]
    struct CityNames {
        race: String,
        names: Vec<String>,
    }
    for list in parse::<Vec<CityNames>>(&dir, "city_names.ron") {
        if let Some(race) = races.iter_mut().find(|r| r.id == list.race) {
            race.city_names = list.names;
        }
    }
    let mut units: Vec<UnitDef> = parse(&dir, "units.ron");
    named(
        &dir,
        "unit",
        &mut units,
        |d| d.num,
        |d, n: String| d.name = n,
    );
    let mut abilities: Vec<crate::rules::AbilityDef> = parse(&dir, "abilities.ron");
    named(
        &dir,
        "ability",
        &mut abilities,
        |d| d.num,
        |d, n: String| d.name = n,
    );
    let mut buildings: Vec<crate::rules::BuildingDef> = parse(&dir, "buildings.ron");
    named(
        &dir,
        "building",
        &mut buildings,
        |d| d.num,
        |d, n: String| d.name = n,
    );
    let mut realms: Vec<crate::rules::RealmDef> = parse(&dir, "realms.ron");
    named(
        &dir,
        "realm",
        &mut realms,
        |d| d.num,
        |d, n: String| d.name = n,
    );
    let mut spells: Vec<crate::rules::SpellDef> = parse(&dir, "spells.ron");
    named(
        &dir,
        "spell",
        &mut spells,
        |d| d.num,
        |d, n: String| d.name = n,
    );
    let mut items: Vec<crate::rules::ItemDef> = parse(&dir, "items.ron");
    named(
        &dir,
        "item",
        &mut items,
        |d| d.num,
        |d, n: String| d.name = n,
    );
    let mut powers: Vec<crate::rules::ItemPowerDef> = parse(&dir, "item_powers.ron");
    named(
        &dir,
        "item_power",
        &mut powers,
        |d| d.num,
        |d, n: String| d.name = n,
    );
    let mut wizards: Vec<crate::rules::WizardDef> = parse(&dir, "wizards.ron");
    for w in wizards.iter_mut() {
        w.name =
            w.id.split('_')
                .map(|word| {
                    let mut c = word.chars();
                    c.next()
                        .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
                })
                .collect::<Vec<_>>()
                .join(" ");
    }
    Content {
        terrains: Terrains::new(terrains),
        features: Features::new(features),
        rules: Rules {
            config: parse(&dir, "rules.ron"),
            races: Registry::new(races),
            units: Registry::new(units),
            abilities: Registry::new(abilities),
            buildings: Registry::new(buildings),
            realms: Registry::new(realms),
            spells: Registry::new(spells),
            items: Registry::new(items),
            item_powers: Registry::new(powers),
            level_paths: Registry::new(parse(&dir, "level_paths.ron")),
            wizards: Registry::new(wizards),
            hero_templates: parse(&dir, "heroes.ron"),
            lairs: parse(&dir, "lairs.ron"),
            wizard_names: parse(&dir, "wizard_names.ron"),
        },
        settings: parse(&dir, "settings.ron"),
        texts: Default::default(),
    }
}
