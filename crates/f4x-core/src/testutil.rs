//! Hand-built content for unit tests (the shipped files are tested in f4x-data).

use std::collections::BTreeMap;

use crate::content::*;
use crate::map::Plane;
use crate::rules::{
    AbilityDef, LevelDef, LevelPathDef, MovementType, Registry, Rules, Stat, UnitDef, UnitKind,
};

pub fn content() -> Content {
    let terrain = |id: &str, class: TerrainClass, walkable, can_found_city| TerrainDef {
        id: id.into(),
        name: id.into(),
        class,
        walkable,
        can_found_city,
        move_cost: match class {
            TerrainClass::Forest | TerrainClass::Hills | TerrainClass::Swamp => 2,
            TerrainClass::Mountains => 3,
            _ => 1,
        },
        food: match class {
            TerrainClass::Grassland => 3,
            TerrainClass::Forest | TerrainClass::Hills | TerrainClass::Swamp => 1,
            _ => 0,
        },
        production_percent: match class {
            TerrainClass::Forest | TerrainClass::Hills => 3,
            TerrainClass::Mountains => 5,
            _ => 0,
        },
        planes: Plane::ALL.to_vec(),
    };
    let terrains = Terrains::new(vec![
        terrain("ocean", TerrainClass::Ocean, false, false),
        terrain("grassland", TerrainClass::Grassland, true, true),
        terrain("forest", TerrainClass::Forest, true, true),
        terrain("hills", TerrainClass::Hills, true, true),
        terrain("mountains", TerrainClass::Mountains, true, false),
        terrain("swamp", TerrainClass::Swamp, true, true),
        terrain("desert", TerrainClass::Desert, true, true),
        terrain("tundra", TerrainClass::Tundra, true, true),
    ]);
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
    let features = Features::new(vec![
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
        feature("node", FeatureKind::Node, land, Plane::ALL.to_vec()),
    ]);
    Content {
        terrains,
        features,
        ..Default::default()
    }
}

/// A unit definition with the stats movement and combat tests need.
pub fn unit(id: &str, movement: MovementType, moves: i32, abilities: &[&str]) -> UnitDef {
    let mut stats = BTreeMap::new();
    for (stat, v) in [
        (Stat::Melee, 3),
        (Stat::Defense, 2),
        (Stat::Resistance, 4),
        (Stat::MaxHp, 2),
        (Stat::Figures, 3),
        (Stat::Moves, moves),
        (Stat::Sight, 1),
    ] {
        stats.insert(stat, v);
    }
    UnitDef {
        id: id.into(),
        name: id.into(),
        kind: UnitKind::Fantastic,
        race: None,
        realm: None,
        cost: 10,
        upkeep: Default::default(),
        stats,
        movement,
        abilities: abilities.iter().map(|a| a.to_string()).collect(),
        requires: vec![],
        level_path: Some("xp".into()),
        hero: None,
        art: None,
    }
}

/// Test content plus a few units: walker (2 moves), scout (flying, 3), boat (sailing, 4),
/// ranger (forester + mountaineer), pathfinder, spirit (non-corporeal), mage (wind walking),
/// traveller (planar travel).
pub fn content_with_units() -> Content {
    let mut c = content();
    let units = vec![
        unit("walker", MovementType::Walking, 2, &[]),
        unit("slow", MovementType::Walking, 1, &[]),
        unit("scout", MovementType::Flying, 3, &[]),
        unit("boat", MovementType::Sailing, 4, &[]),
        unit(
            "ranger",
            MovementType::Walking,
            2,
            &["forester", "mountaineer"],
        ),
        unit("pathfinder", MovementType::Walking, 2, &["pathfinding"]),
        unit("spirit", MovementType::Walking, 2, &["non_corporeal"]),
        unit("mage", MovementType::Walking, 2, &["wind_walking"]),
        unit("traveller", MovementType::Walking, 2, &["planar_travel"]),
        unit("hardy", MovementType::Walking, 2, &["tough"]),
    ];
    let ability = |id: &str, effects| AbilityDef {
        id: id.into(),
        name: id.into(),
        description: String::new(),
        effects,
    };
    let abilities = [
        "forester",
        "mountaineer",
        "pathfinding",
        "non_corporeal",
        "wind_walking",
        "planar_travel",
        "flying",
    ]
    .into_iter()
    .map(|a| ability(a, vec![]))
    .chain([ability(
        "tough",
        vec![crate::rules::effects::EffectRule {
            when: crate::rules::effects::Trigger::Passive,
            condition: crate::rules::effects::Condition::Always,
            target: crate::rules::effects::TargetSel::This,
            effect: crate::rules::effects::Effect::Modify {
                stat: Stat::Defense,
                op: crate::rules::ModOp::Add,
                value: 2,
            },
        }],
    )])
    .collect();
    let level_path = LevelPathDef {
        id: "xp".into(),
        levels: vec![
            LevelDef {
                name: "Recruit".into(),
                xp: 0,
                effects: vec![],
            },
            LevelDef {
                name: "Veteran".into(),
                xp: 20,
                effects: vec![crate::rules::effects::EffectRule {
                    when: crate::rules::effects::Trigger::Passive,
                    condition: crate::rules::effects::Condition::Always,
                    target: crate::rules::effects::TargetSel::This,
                    effect: crate::rules::effects::Effect::Modify {
                        stat: Stat::Melee,
                        op: crate::rules::ModOp::Add,
                        value: 1,
                    },
                }],
            },
        ],
        bonus: vec![],
    };
    c.rules = Rules {
        units: Registry::new(units),
        abilities: Registry::new(abilities),
        level_paths: Registry::new(vec![level_path]),
        ..Default::default()
    };
    c
}

/// Test content with a few races, buildings, mines and city enchantments, for economy tests.
pub fn content_with_economy() -> Content {
    use crate::rules::effects::{Condition, Effect, EffectRule, TargetSel, Trigger};
    use crate::rules::{
        BuildingDef, CastContext, Duration, ModOp, RaceDef, Rarity, RealmDef, SpellDef, SpellTarget,
    };
    let mut c = content_with_units();
    let modify = |stat, op, value| EffectRule {
        when: Trigger::Passive,
        condition: Condition::Always,
        target: TargetSel::This,
        effect: Effect::Modify { stat, op, value },
    };
    let race = |id: &str, units: &[&str], effects: Vec<EffectRule>| RaceDef {
        id: id.into(),
        name: id.into(),
        units: units.iter().map(|u| u.to_string()).collect(),
        buildings: vec![],
        home_terrain: vec![],
        effects,
        city_names: vec![],
        farmer_food: 2,
        worker_production_x2: 4,
        mana_per_citizen_x2: 0,
        double_mining: false,
        trade_percent: 0,
        outpost_growth: 10,
        house_type: 0,
        myrror: false,
    };
    let mut dwarf = race(
        "dwarf",
        &["spear"],
        vec![modify(Stat::CityGrowth, ModOp::Add, -20)],
    );
    dwarf.double_mining = true;
    dwarf.worker_production_x2 = 6;
    let mut elf = race("elf", &["spear"], vec![]);
    elf.mana_per_citizen_x2 = 1;
    elf.farmer_food = 3;
    let building =
        |id: &str, cost: u32, upkeep: u32, requires: &[&str], effects: Vec<EffectRule>| {
            BuildingDef {
                id: id.into(),
                name: id.into(),
                cost,
                upkeep_gold: upkeep,
                requires: requires.iter().map(|r| r.to_string()).collect(),
                replaces: None,
                races: vec![],
                effects,
                shape: 0,
            }
        };
    let mut hall = building("builders_hall", 60, 1, &[], vec![]);
    hall.replaces = None;
    let mut smithy = building("smithy", 40, 1, &[], vec![]);
    smithy.replaces = None;
    let mut armory = building("armory", 80, 2, &["barracks", "smithy"], vec![]);
    armory.replaces = Some("barracks".into());
    let mut dwarf_only = building("forge", 100, 2, &[], vec![]);
    dwarf_only.races = vec!["dwarf".into()];
    let buildings = vec![
        building("barracks", 30, 0, &[], vec![]),
        building(
            "fighters_guild",
            200,
            3,
            &["barracks"],
            vec![modify(Stat::StartingXp, ModOp::Min, 20)],
        ),
        building("war_college", 500, 5, &["fighters_guild"], vec![]),
        building("alchemists_guild", 250, 3, &[], vec![]),
        building(
            "marketplace",
            100,
            1,
            &[],
            vec![modify(Stat::CityGold, ModOp::Percent, 50)],
        ),
        building(
            "temple",
            200,
            2,
            &[],
            vec![
                modify(Stat::CityUnrest, ModOp::Add, -1),
                modify(Stat::CityMana, ModOp::Add, 2),
            ],
        ),
        building(
            "granary",
            40,
            1,
            &[],
            vec![
                modify(Stat::CityFoodFlat, ModOp::Add, 2),
                modify(Stat::CityGrowth, ModOp::Add, 20),
            ],
        ),
        building(
            "library",
            60,
            1,
            &[],
            vec![modify(Stat::CityResearch, ModOp::Add, 2)],
        ),
        building(
            "miners_guild",
            300,
            3,
            &[],
            vec![modify(Stat::CityProduction, ModOp::Percent, 50)],
        ),
        building("shipyard", 200, 2, &[], vec![]),
        building(
            "sawmill",
            100,
            2,
            &[],
            vec![modify(Stat::CityProduction, ModOp::Percent, 25)],
        ),
        hall,
        smithy,
        armory,
        dwarf_only,
    ];
    let mut spear = unit("spear", MovementType::Walking, 1, &[]);
    spear.kind = UnitKind::Normal;
    spear.race = Some("elf".into());
    spear.cost = 20;
    spear.upkeep.gold = 1;
    spear.requires = vec!["barracks".into()];
    let mut settler = unit("settler", MovementType::Walking, 1, &["create_outpost"]);
    settler.kind = UnitKind::Normal;
    settler.race = Some("elf".into());
    let mut galley = unit("galley", MovementType::Sailing, 4, &[]);
    galley.requires = vec!["shipyard".into()];
    galley.cost = 50;
    let mut units: Vec<UnitDef> = c.rules.units.defs().to_vec();
    units.push(spear);
    units.push(galley);
    units.push(settler);
    elf.units = vec!["spear".into(), "settler".into()];
    dwarf.units = vec!["spear".into()];
    // The spear is the elves' unit; dwarves list it too for the test, so give the check a race.
    let spell = |id: &str, effects| SpellDef {
        id: id.into(),
        name: id.into(),
        realm: "life".into(),
        rarity: Rarity::Common,
        cost: 10,
        research_cost: 10,
        contexts: vec![CastContext::Overland],
        overland_x5: false,
        target: SpellTarget::OwnCity,
        duration: Duration::Permanent,
        upkeep: 1,
        dispel_difficulty: 10,
        save: None,
        effects,
    };
    let spells = vec![
        spell("famine", vec![modify(Stat::CityFood, ModOp::Percent, -50)]),
        spell("gaias_blessing", vec![]),
    ];
    c.rules.races = Registry::new(vec![elf, dwarf]);
    c.rules.units = Registry::new(units);
    c.rules.buildings = Registry::new(buildings);
    c.rules.spells = Registry::new(spells);
    c.rules.realms = Registry::new(vec![RealmDef {
        id: "life".into(),
        name: "Life".into(),
        color: (255, 255, 255),
        default_spells: Vec::new(),
    }]);
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
    c.rules.config.racial_unrest = vec![vec![0, 2], vec![1, 0]];
    c
}

/// Economy content plus a spell library: ten spells of each rarity in life and chaos,
/// two arcane spells, and a few with real effects (a unit enchantment, a heal, a summon).
pub fn content_with_magic() -> Content {
    use crate::rules::effects::{Condition, Effect, EffectRule, TargetSel, Trigger};
    use crate::rules::{CastContext, Duration, ModOp, Rarity, RealmDef, SpellDef, SpellTarget};
    let mut c = content_with_economy();
    let rule = |when, effect| EffectRule {
        when,
        condition: Condition::Always,
        target: TargetSel::Chosen,
        effect,
    };
    let mut spells: Vec<SpellDef> = c.rules.spells.defs().to_vec();
    let base = |id: &str, realm: &str, rarity, cost, research, target| SpellDef {
        id: id.into(),
        name: id.into(),
        realm: realm.into(),
        rarity,
        cost,
        research_cost: research,
        contexts: vec![CastContext::Overland],
        overland_x5: false,
        target,
        duration: Duration::Instant,
        upkeep: 0,
        dispel_difficulty: 0,
        save: None,
        effects: vec![],
    };
    for realm in ["life", "chaos"] {
        for (rarity, tag) in [
            (Rarity::Common, "c"),
            (Rarity::Uncommon, "u"),
            (Rarity::Rare, "r"),
            (Rarity::VeryRare, "v"),
        ] {
            for i in 0..10 {
                let mut s = base(
                    &format!("{realm}_{tag}{i}"),
                    realm,
                    rarity,
                    10 + i,
                    20 + 10 * i,
                    SpellTarget::None,
                );
                s.effects = vec![rule(
                    Trigger::OnCast,
                    Effect::Custom("unimplemented".into()),
                )];
                spells.push(s);
            }
        }
    }
    spells.push(base(
        "magic_spirit",
        "arcane",
        Rarity::Common,
        30,
        20,
        SpellTarget::None,
    ));
    spells.push(base(
        "dispel_magic",
        "arcane",
        Rarity::Common,
        10,
        100,
        SpellTarget::None,
    ));
    let mut mastery = base(
        "spell_of_mastery",
        "arcane",
        Rarity::Uncommon,
        5000,
        1,
        SpellTarget::None,
    );
    mastery.effects = vec![rule(
        Trigger::OnCast,
        Effect::Custom(crate::endgame::SPELL_OF_MASTERY.into()),
    )];
    spells.push(mastery);
    let mut strength = base(
        "giant_strength",
        "nature",
        Rarity::Common,
        8,
        80,
        SpellTarget::OwnUnit,
    );
    strength.duration = Duration::Permanent;
    strength.upkeep = 2;
    strength.dispel_difficulty = 8;
    strength.contexts = vec![CastContext::Battle, CastContext::Overland];
    strength.overland_x5 = true;
    strength.effects = vec![EffectRule {
        when: Trigger::Passive,
        condition: Condition::Always,
        target: TargetSel::This,
        effect: Effect::Modify {
            stat: crate::rules::Stat::Melee,
            op: ModOp::Add,
            value: 1,
        },
    }];
    // Lasting spells that grant an ability, or that the engine reads by name for resistance.
    for (id, grant) in [
        ("magic_immunity", Some("magic_immunity")),
        ("flame_blade", Some("magical_weapon")),
        ("resist_elements", None),
        ("righteousness", None),
    ] {
        let mut s = base(id, "life", Rarity::Rare, 30, 100, SpellTarget::OwnUnit);
        s.duration = Duration::Permanent;
        s.upkeep = 2;
        s.dispel_difficulty = 30;
        s.effects = grant
            .map(|a| EffectRule {
                when: Trigger::Passive,
                condition: Condition::Always,
                target: TargetSel::This,
                effect: Effect::GrantAbility(a.into()),
            })
            .into_iter()
            .collect();
        spells.push(s);
    }
    let mut brief = strength.clone();
    brief.id = "brief_strength".into();
    brief.duration = Duration::Turns(2);
    brief.upkeep = 0;
    spells.push(strength);
    spells.push(brief);
    let mut heal = base("mend", "life", Rarity::Common, 10, 40, SpellTarget::OwnUnit);
    heal.effects = vec![rule(Trigger::OnCast, Effect::Heal(5))];
    spells.push(heal);
    let mut bolt = base(
        "zap",
        "chaos",
        Rarity::Common,
        10,
        40,
        SpellTarget::EnemyUnit,
    );
    bolt.effects = vec![rule(
        Trigger::OnCast,
        Effect::Damage {
            amount: 3,
            kind: crate::rules::effects::DamageKind::Fire,
            resistible: false,
            immune: Vec::new(),
            realm: None,
            resist_modifier: 0,
            traits: Default::default(),
        },
    )];
    spells.push(bolt);
    let mut summon = base(
        "call_spears",
        "life",
        Rarity::Common,
        40,
        40,
        SpellTarget::Tile,
    );
    summon.effects = vec![rule(
        Trigger::OnCast,
        Effect::Summon {
            unit: "spear".into(),
            count: 2,
        },
    )];
    spells.push(summon);
    // The fortress spells, which run hooks instead of effects.
    for (id, cost, hook) in [
        ("move_fortress", 100, crate::fortress::MOVE_FORTRESS),
        ("summoning_circle", 50, crate::fortress::MOVE_CIRCLE),
        ("spell_of_return", 10, crate::fortress::RETURN),
    ] {
        let mut s = base(
            id,
            "arcane",
            Rarity::Uncommon,
            cost,
            100,
            SpellTarget::OwnCity,
        );
        s.effects = vec![rule(Trigger::OnCast, Effect::Custom(hook.into()))];
        spells.push(s);
    }
    // City gates (task 12): the effect lives in `gates.rs`, the spell only enchants the city.
    for id in ["earth_gate", "astral_gate"] {
        let mut s = base(
            id,
            "arcane",
            Rarity::VeryRare,
            100,
            100,
            SpellTarget::OwnCity,
        );
        s.duration = Duration::Permanent;
        s.effects = vec![rule(
            Trigger::OnCast,
            Effect::Custom(crate::gates::CITY_GATE.into()),
        )];
        spells.push(s);
    }
    // Global enchantments the engine reads from the wizard's list.
    for id in [
        "evil_omens",
        "herb_mastery",
        "planar_seal",
        "awareness",
        "nature_awareness",
        "detect_magic",
        "charm_of_life",
        "holy_arms",
        "chaos_surge",
        "crusade",
    ] {
        let mut s = base(id, "arcane", Rarity::VeryRare, 100, 100, SpellTarget::None);
        s.duration = Duration::Permanent;
        s.effects = vec![rule(
            Trigger::OnCast,
            Effect::Custom(crate::globals::GLOBAL_EFFECT.into()),
        )];
        let modify = |cond, stat, value| EffectRule {
            when: Trigger::Passive,
            condition: cond,
            target: TargetSel::This,
            effect: Effect::Modify {
                stat,
                op: ModOp::Add,
                value,
            },
        };
        match id {
            "holy_arms" => s.effects.push(modify(
                Condition::KindIs(crate::rules::UnitKind::Normal),
                Stat::Melee,
                1,
            )),
            "chaos_surge" => {
                s.effects
                    .push(modify(Condition::RealmIs("chaos".into()), Stat::Melee, 2))
            }
            _ => {}
        }
        spells.push(s);
    }
    // Heroism: a lasting unit enchantment the level code looks for by name.
    let mut heroism = base(
        "heroism",
        "life",
        Rarity::Common,
        20,
        100,
        SpellTarget::OwnUnit,
    );
    heroism.duration = Duration::Permanent;
    heroism.upkeep = 2;
    spells.push(heroism);
    // The map-square spells.
    for (id, cost, hook) in [
        ("raise_volcano", 200, crate::terraform::RAISE_VOLCANO),
        ("change_terrain", 50, crate::terraform::CHANGE_TERRAIN),
        ("corruption", 40, crate::corruption::CORRUPT_LAND),
        ("warp_node", 75, crate::corruption::WARP_NODE),
    ] {
        let mut s = base(id, "arcane", Rarity::Uncommon, cost, 100, SpellTarget::Tile);
        s.effects = vec![rule(Trigger::OnCast, Effect::Custom(hook.into()))];
        spells.push(s);
    }
    // The special spells (`special_spells.rs`).
    for (id, hook, target) in [
        (
            "transmute",
            crate::special_spells::TRANSMUTE,
            SpellTarget::Tile,
        ),
        (
            "earth_lore",
            crate::special_spells::EARTH_LORE,
            SpellTarget::Tile,
        ),
        (
            "natures_cures",
            crate::special_spells::NATURES_CURES,
            SpellTarget::OwnUnit,
        ),
        (
            "word_of_recall",
            crate::special_spells::WORD_OF_RECALL,
            SpellTarget::OwnUnit,
        ),
        (
            "plane_shift",
            crate::special_spells::PLANE_SHIFT,
            SpellTarget::OwnUnit,
        ),
    ] {
        let mut s = base(id, "arcane", Rarity::Uncommon, 20, 100, target);
        s.effects = vec![rule(Trigger::OnCast, Effect::Custom(hook.into()))];
        spells.push(s);
    }
    c.rules.spells = Registry::new(spells);
    c.rules.realms = Registry::new(
        ["life", "chaos", "nature"]
            .iter()
            .map(|r| RealmDef {
                id: (*r).into(),
                name: (*r).into(),
                color: (255, 255, 255),
                default_spells: Vec::new(),
            })
            .collect(),
    );
    c
}

/// A [`crate::view::WorldView`] answered from lists, so a domain's rules can be tested with no
/// world, map or fog. `sightings` holds what each viewer sees; `units` what each player has.
pub struct FakeView {
    pub turn: u32,
    pub human: Option<crate::city::PlayerId>,
    pub difficulty: u8,
    pub content: Content,
    pub players: Vec<crate::player::Player>,
    pub sightings: Vec<(crate::city::PlayerId, crate::view::Sighting)>,
    pub units: Vec<(crate::city::PlayerId, crate::view::UnitInfo)>,
    /// The owner of each city.
    pub cities: Vec<crate::city::PlayerId>,
}

impl FakeView {
    /// A view with a wizard for each id (and no content: use `with_content` for spells).
    pub fn new(ids: &[crate::city::PlayerId]) -> Self {
        Self {
            turn: 1,
            human: None,
            difficulty: 2,
            content: Content::default(),
            players: ids
                .iter()
                .map(|&id| crate::player::Player::new(id, "W"))
                .collect(),
            sightings: Vec::new(),
            units: Vec::new(),
            cities: Vec::new(),
        }
    }

    pub fn with_content(mut self, content: Content) -> Self {
        self.content = content;
        self
    }
}

impl crate::view::WorldView for FakeView {
    fn turn(&self) -> u32 {
        self.turn
    }

    fn human(&self) -> Option<crate::city::PlayerId> {
        self.human
    }

    fn difficulty(&self) -> u8 {
        self.difficulty
    }

    fn content(&self) -> &Content {
        &self.content
    }

    fn players(&self) -> &[crate::player::Player] {
        &self.players
    }

    fn foreign_sightings(&self, viewer: crate::city::PlayerId) -> Vec<crate::view::Sighting> {
        self.sightings
            .iter()
            .filter(|(v, s)| *v == viewer && s.owner != viewer)
            .map(|(_, s)| *s)
            .collect()
    }

    fn foreign_sightings_near(
        &self,
        viewer: crate::city::PlayerId,
        at: crate::city::Position,
        radius: u32,
    ) -> Vec<crate::view::Sighting> {
        self.sightings
            .iter()
            .filter(|(v, s)| *v == viewer && s.owner != viewer)
            .map(|(_, s)| *s)
            .filter(|s| {
                s.pos.plane == at.plane
                    && s.pos.x.abs_diff(at.x).max(s.pos.y.abs_diff(at.y)) <= radius
            })
            .collect()
    }

    fn units_of(&self, player: crate::city::PlayerId) -> Vec<crate::view::UnitInfo> {
        self.units
            .iter()
            .filter(|(p, _)| *p == player)
            .map(|(_, u)| *u)
            .collect()
    }

    fn city_count(&self, player: crate::city::PlayerId) -> usize {
        self.cities.iter().filter(|&&c| c == player).count()
    }
}
