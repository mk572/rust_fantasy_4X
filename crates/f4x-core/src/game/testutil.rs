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

/// The shipped `content/rules_settings.ron`, for tests that play a difficulty level.
pub fn settings() -> crate::game::setting::SettingsDef {
    ron::from_str(include_str!("../../../../content/rules_settings.ron"))
        .expect("rules_settings.ron parses")
}

/// The game's own `content/` folder, read as `f4x_data::load_content` reads it (the city names, the
/// ready-made wizards' names), without its checks and
/// without the messages. It is parsed once per test process; `f4x-data` tests the loader.
pub fn shipped() -> Content {
    static SHIPPED: std::sync::OnceLock<Content> = std::sync::OnceLock::new();
    SHIPPED.get_or_init(load_shipped).clone()
}

fn load_shipped() -> Content {
    use serde::de::DeserializeOwned;

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    // The names are the client's files.
    let client = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../f4x-client/content");
    fn parse<T: DeserializeOwned>(dir: &std::path::Path, name: &str) -> T {
        let text = std::fs::read_to_string(dir.join(name))
            .unwrap_or_else(|e| panic!("content/{name}: {e}"));
        ron::from_str(&text).unwrap_or_else(|e| panic!("content/{name}: {e}"))
    }
    let terrains: Vec<TerrainDef> = parse(&dir, "terrain.ron");
    let features: Vec<FeatureDef> = parse(&dir, "features.ron");
    let mut races: Vec<crate::rules::RaceDef> = parse(&dir, "races.ron");
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
    let units: Vec<UnitDef> = parse(&dir, "units.ron");
    let abilities: Vec<crate::rules::AbilityDef> = parse(&dir, "abilities.ron");
    let buildings: Vec<crate::rules::BuildingDef> = parse(&dir, "buildings.ron");
    let realms: Vec<crate::rules::RealmDef> = parse(&dir, "realms.ron");
    let spells: Vec<crate::rules::SpellDef> = parse(&dir, "spells.ron");
    let items: Vec<crate::rules::ItemDef> = parse(&dir, "items.ron");
    let powers: Vec<crate::rules::ItemPowerDef> = parse(&dir, "item_powers.ron");
    let retorts: Vec<crate::rules::RetortDef> = parse(&dir, "retorts.ron");
    let wizards: Vec<crate::rules::WizardDef> = parse(&dir, "wizards.ron");
    let level_paths: Vec<crate::rules::LevelPathDef> = parse(&dir, "level_paths.ron");
    let hero_templates: Vec<crate::rules::HeroTemplate> = parse(&dir, "heroes.ron");
    let config: crate::rules::RulesConfig = parse(&dir, "rules.ron");
    let settings: crate::game::setting::SettingsDef = parse(&dir, "rules_settings.ron");
    let abilities = Registry::new(abilities);
    Content {
        terrains: Terrains::new(terrains),
        features: Features::new(features),
        rules: Rules {
            config,
            races: Registry::new(races),
            units: Registry::new(units),
            abilities,
            buildings: Registry::new(buildings),
            realms: Registry::new(realms),
            spells: Registry::new(spells),
            items: Registry::new(items),
            item_powers: Registry::new(powers),
            level_paths: Registry::new(level_paths),
            wizards: Registry::new(wizards),
            retorts: Registry::new(retorts),
            hero_templates,
            lairs: parse(&dir, "lairs.ron"),
            wizard_names: parse(&client, "text/en/name/rival.ron"),
        },
        settings,
        texts: Default::default(),
    }
}

/// The number of the ability the content file calls `alias`, for tests that name abilities in
/// lists and loops (the game code itself refers to abilities by number).
pub fn ability(alias: u16) -> crate::rules::Ability {
    shipped()
        .rules
        .abilities
        .id_numbered(alias)
        .unwrap_or_else(|| panic!("no ability {alias:?} in the shipped content"))
}

/// The number of the realm the content file calls `alias`, for tests that name realms (the game
/// code itself refers to realms by number).
pub fn realm(alias: u16) -> crate::rules::RealmId {
    shipped()
        .rules
        .realms
        .id_numbered(alias)
        .unwrap_or_else(|| panic!("no realm {alias:?} in the shipped content"))
}

/// The number of the realm the content file calls `alias`, for tests that set a definition's
/// realm by hand.
pub fn realm_ref(alias: u16) -> crate::rules::RealmId {
    realm(alias)
}

/// The number of the ability the content file calls `alias`, for tests that name abilities in a
/// condition or an effect.
pub fn ability_ref(alias: u16) -> crate::rules::Ability {
    ability(alias)
}

/// The number of the spell the content file calls `alias`.
pub fn spell_ref(alias: u16) -> crate::rules::SpellId {
    shipped()
        .rules
        .spells
        .id_numbered(alias)
        .unwrap_or_else(|| panic!("no spell {alias:?} in the shipped content"))
}

/// The number of the race the content file calls `alias`.
pub fn race_ref(alias: u16) -> crate::rules::RaceId {
    shipped()
        .rules
        .races
        .id_numbered(alias)
        .unwrap_or_else(|| panic!("no race {alias:?} in the shipped content"))
}

/// [`ability`] for a list of aliases.
pub fn abilities(aliases: &[u16]) -> Vec<crate::rules::Ability> {
    aliases.iter().map(|&a| ability(a)).collect()
}
