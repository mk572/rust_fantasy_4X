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
        settings: settings(),
        ..Default::default()
    }
}

/// The shipped `content/settings.ron`, for tests that play a difficulty level.
pub fn settings() -> crate::game::setting::SettingsDef {
    ron::from_str(include_str!("../../../../content/settings.ron")).expect("settings.ron parses")
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
        (Stat::Sight, 3),
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
        unit("strider", MovementType::Walking, 2, &["water_walking"]),
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
        "water_walking",
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
    use crate::rules::{BuildingDef, ModOp, RaceDef, RealmDef};
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
                coastal: false,
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
    c.rules.races = Registry::new(vec![elf, dwarf]);
    c.rules.units = Registry::new(units);
    c.rules.buildings = Registry::new(buildings);
    c.rules.spells = Registry::new(real_spells());
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

/// The game's real spells (`content/spells.ron`), for tests that cast or look at them.
pub fn real_spells() -> Vec<crate::rules::SpellDef> {
    ron::from_str(include_str!("../../../../content/spells.ron")).expect("the game's spells parse")
}

/// Economy content plus the game's real spells and realms (`content/spells.ron` and
/// `content/realms.ron`), so a test casts the same spell the game does.
pub fn content_with_magic() -> Content {
    use crate::rules::RealmDef;
    let mut c = content_with_economy();
    let realms: Vec<RealmDef> = ron::from_str(include_str!("../../../../content/realms.ron"))
        .expect("the game's realms parse");
    c.rules.realms = Registry::new(realms);
    c
}

/// A [`crate::game::view::WorldView`] answered from lists, so a domain's rules can be tested with no
/// world, map or fog. `sightings` holds what each viewer sees; `units` what each player has.
pub struct FakeView {
    pub turn: u32,
    pub human: Option<crate::player::PlayerId>,
    pub difficulty: u8,
    pub content: Content,
    pub players: Vec<crate::player::PlayerId>,
    pub sightings: Vec<(crate::player::PlayerId, crate::game::view::Sighting)>,
    pub units: Vec<(crate::player::PlayerId, crate::game::view::UnitInfo)>,
    /// The owner of each city.
    pub cities: Vec<crate::player::PlayerId>,
    /// The players that are neutral.
    pub neutral: Vec<crate::player::PlayerId>,
    /// Each player's gold and mana; a player not listed has the starting ones.
    pub treasuries: Vec<(crate::player::PlayerId, crate::player::Treasury)>,
    /// The players that are defeated.
    pub defeated: Vec<crate::player::PlayerId>,
    /// The wizards' entities (one for each player of `new`), with their sheets.
    pub world: hecs::World,
    pub wizards: Vec<(crate::player::PlayerId, hecs::Entity)>,
}

impl FakeView {
    /// A view with a wizard for each id (and no content: use `with_content` for spells).
    pub fn new(ids: &[crate::player::PlayerId]) -> Self {
        let mut world = hecs::World::new();
        let wizards = ids
            .iter()
            .map(|&id| (id, world.spawn(crate::wizard::sheet::bundle())))
            .collect();
        Self {
            world,
            wizards,
            turn: 1,
            human: None,
            difficulty: 2,
            content: Content::default(),
            players: ids.to_vec(),
            sightings: Vec::new(),
            units: Vec::new(),
            cities: Vec::new(),
            neutral: Vec::new(),
            treasuries: Vec::new(),
            defeated: Vec::new(),
        }
    }

    /// A write handle on a wizard's sheet.
    ///
    /// # Panics
    /// If `id` was not given to [`FakeView::new`].
    pub fn wizard_mut(
        &mut self,
        id: crate::player::PlayerId,
    ) -> crate::wizard::sheet::WizardMut<'_> {
        let e = self
            .wizards
            .iter()
            .find(|(p, _)| *p == id)
            .expect("a wizard of the view")
            .1;
        crate::wizard::sheet::WizardMut::new(&mut self.world, e)
    }

    pub fn with_content(mut self, content: Content) -> Self {
        self.content = content;
        self
    }

    /// A player's gold and mana, to change (the starting ones until then).
    pub fn treasury_mut(
        &mut self,
        id: crate::player::PlayerId,
    ) -> Option<&mut crate::player::Treasury> {
        self.players.iter().find(|p| **p == id)?;
        let at = match self.treasuries.iter().position(|(p, _)| *p == id) {
            Some(at) => at,
            None => {
                self.treasuries
                    .push((id, crate::player::Treasury::default()));
                self.treasuries.len() - 1
            }
        };
        Some(&mut self.treasuries[at].1)
    }

    pub fn set_human(&mut self, human: Option<crate::player::PlayerId>) {
        self.human = human;
    }
}

impl crate::game::view::WorldView for FakeView {
    fn turn(&self) -> u32 {
        self.turn
    }

    fn human(&self) -> Option<crate::player::PlayerId> {
        self.human
    }

    fn difficulty(&self) -> u8 {
        self.difficulty
    }

    fn content(&self) -> &Content {
        &self.content
    }

    fn player_ids(&self) -> Vec<crate::player::PlayerId> {
        self.players.clone()
    }

    fn is_neutral(&self, id: crate::player::PlayerId) -> bool {
        self.neutral.contains(&id)
    }

    fn treasury(&self, id: crate::player::PlayerId) -> Option<crate::player::Treasury> {
        self.players.iter().find(|p| **p == id)?;
        Some(
            self.treasuries
                .iter()
                .find(|(p, _)| *p == id)
                .map_or_else(crate::player::Treasury::default, |(_, t)| *t),
        )
    }

    fn is_defeated(&self, id: crate::player::PlayerId) -> bool {
        self.defeated.contains(&id)
    }

    fn has_player(&self, id: crate::player::PlayerId) -> bool {
        self.players.contains(&id)
    }

    fn wizard(&self, id: crate::player::PlayerId) -> Option<crate::wizard::entity::Wizard<'_>> {
        let e = self.wizards.iter().find(|(p, _)| *p == id)?.1;
        Some(crate::wizard::entity::Wizard::new(&self.world, e))
    }

    fn foreign_sightings(
        &self,
        viewer: crate::player::PlayerId,
    ) -> Vec<crate::game::view::Sighting> {
        self.sightings
            .iter()
            .filter(|(v, s)| *v == viewer && s.owner != viewer)
            .map(|(_, s)| *s)
            .collect()
    }

    fn foreign_sightings_near(
        &self,
        viewer: crate::player::PlayerId,
        at: crate::map::Position,
        radius: u32,
    ) -> Vec<crate::game::view::Sighting> {
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

    fn units_of(&self, player: crate::player::PlayerId) -> Vec<crate::game::view::UnitInfo> {
        self.units
            .iter()
            .filter(|(p, _)| *p == player)
            .map(|(_, u)| *u)
            .collect()
    }

    fn city_count(&self, player: crate::player::PlayerId) -> usize {
        self.cities.iter().filter(|&&c| c == player).count()
    }
}

/// An item power for tests that is not a plain bonus: the lasting spell and the attack ability
/// it gives the carrier (either may be none).
pub fn item_power(
    id: &str,
    spell: Option<&str>,
    attack: Option<&str>,
) -> crate::rules::ItemPowerDef {
    crate::rules::ItemPowerDef {
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
