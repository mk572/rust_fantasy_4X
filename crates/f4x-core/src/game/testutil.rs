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
    let mut retorts: Vec<crate::rules::RetortDef> = parse(&dir, "retorts.ron");
    named(
        &dir,
        "retort",
        &mut retorts,
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
    let mut level_paths: Vec<crate::rules::LevelPathDef> = parse(&dir, "level_paths.ron");
    let mut hero_templates: Vec<crate::rules::HeroTemplate> = parse(&dir, "heroes.ron");
    let mut config: crate::rules::RulesConfig = parse(&dir, "rules.ron");
    crate::rules::resolve_references(&mut crate::rules::Tables {
        races: &mut races,
        units: &mut units,
        abilities: &mut abilities,
        buildings: &mut buildings,
        realms: &mut realms,
        retorts: &mut retorts,
        spells: &mut spells,
        item_powers: &mut powers,
        level_paths: &mut level_paths,
        wizards: &mut wizards,
        items: &mut items,
        hero_templates: &mut hero_templates,
        config: &mut config,
    });
    let settings: crate::game::setting::SettingsDef = parse(&dir, "rules_settings.ron");
    let abilities = Registry::new(abilities);
    crate::rules::resolve_unit_abilities(&mut units, &abilities);
    crate::rules::resolve_building_links(&mut buildings, &mut units);
    crate::rules::resolve_summons(&mut spells, &units);
    crate::rules::resolve_hero_units(&mut hero_templates, &units);
    let mut content = Content {
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
            wizard_names: parse(&dir, "text/en/name/rival.ron"),
        },
        settings,
        client_settings: parse(&dir, "client_settings.ron"),
        texts: Default::default(),
    };
    content.settings.spell_library.resolve(
        content.rules.spells.defs(),
        &content.rules.retorts,
        &content.rules.realms,
    );
    content
}

/// The number of the ability the content file calls `alias`, for tests that name abilities in
/// lists and loops (the game code itself refers to abilities by number).
pub fn ability(alias: &str) -> crate::rules::Ability {
    shipped()
        .rules
        .abilities
        .id_of(alias)
        .unwrap_or_else(|| panic!("no ability {alias:?} in the shipped content"))
}

/// The number of the realm the content file calls `alias`, for tests that name realms (the game
/// code itself refers to realms by number).
pub fn realm(alias: &str) -> crate::rules::RealmId {
    shipped()
        .rules
        .realms
        .id_of(alias)
        .unwrap_or_else(|| panic!("no realm {alias:?} in the shipped content"))
}

/// A resolved reference to the realm the content file calls `alias`, for tests that set a
/// definition's realm by hand (a name alone stays unresolved).
pub fn realm_ref(alias: &str) -> crate::rules::Ref<crate::rules::RealmDef> {
    let mut r = crate::rules::Ref::new(alias);
    r.set(Some(realm(alias)));
    r
}

/// A resolved reference to the ability the content file calls `alias`, for tests that name
/// abilities in a condition or an effect (a name alone stays unresolved).
pub fn ability_ref(alias: &str) -> crate::rules::Ref<crate::rules::AbilityDef> {
    let mut r = crate::rules::Ref::new(alias);
    r.set(Some(ability(alias)));
    r
}

/// A resolved reference to the spell the content file calls `alias`.
pub fn spell_ref(alias: &str) -> crate::rules::Ref<crate::rules::SpellDef> {
    let mut r = crate::rules::Ref::new(alias);
    r.set(shipped().rules.spells.id_of(alias));
    r
}

/// A resolved reference to the race the content file calls `alias`, for tests that name races in
/// a condition or a definition (a name alone stays unresolved).
pub fn race_ref(alias: &str) -> crate::rules::Ref<crate::rules::RaceDef> {
    let mut r = crate::rules::Ref::new(alias);
    r.set(shipped().rules.races.id_of(alias));
    r
}

/// [`ability`] for a list of aliases.
pub fn abilities(aliases: &[&str]) -> Vec<crate::rules::Ability> {
    aliases.iter().map(|a| ability(a)).collect()
}
