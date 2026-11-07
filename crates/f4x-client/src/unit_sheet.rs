//! The unit statistics sheet: what the original's `Unit_Statistics_Popup` shows about
//! one unit, built from the game (this file) and drawn by `unit_screen.rs`.
//!
//! The layout, the icon rows and the order of the abilities list follow ReMoM (`UnitStat.c`
//! `Thing_View_Draw`, `UV_Draw_Stat_Icons`, `UV_Draw_Hits_Icons`; `UnitView.c`
//! `USW_Build_Effect_List`). The numbers in the tables below are picture indexes into the
//! original's `special.lbx` (and `special2.lbx` from 111 on), not art.

use crate::text::{Args, Texts};
use f4x_core::battle::{Battle, BattleUnit, RangedKind};
use f4x_def::rules::Ability;
use f4x_def::rules::SpellId;
use f4x_def::rules::{MovementType, Stat, UnitKind};

use crate::view::App;

/// Where a sheet's unit lives, for Dismiss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    Overland(hecs::Entity),
    Battle(usize),
    /// A hero or a company of mercenaries on offer: not a unit yet.
    Offer,
}

/// One row of stat icons: `value` icons, of which the last `gold` are bonus ones, plus `grey`
/// icons for what a curse took away (drawn darkened).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Row {
    pub icon: usize,
    pub value: i32,
    pub gold: i32,
    pub grey: i32,
}

impl Row {
    /// A stat now `value` that was `base` before bonuses and curses.
    pub fn stat(icon: usize, value: i32, base: i32) -> Self {
        Self {
            icon,
            value: value.max(0),
            gold: (value - base).max(0),
            grey: (base - value).max(0),
        }
    }
}

/// The picture of an item line of a hero's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemIcon {
    /// The item's own picture, an entry of `items.lbx`.
    Held(usize),
    /// An empty slot: the picture of the kind of item it takes, an entry of `itemisc.lbx`.
    Empty(usize),
}

/// What a hero's sheet shows in place of the unit on its diamond.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeroView {
    /// An entry of `portrait.lbx`.
    pub portrait: usize,
    /// The hero's own name; the unit type's name goes under it ("The Dwarf").
    pub name: String,
}

/// One line of the abilities list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Special {
    pub name: String,
    /// Picture index (see [`special_picture`]); `None` draws no icon.
    pub icon: Option<usize>,
    /// The lasting spell this line is, when it is one.
    pub spell: Option<f4x_def::rules::SpellId>,
    /// A click on the line turns the spell off: it is the viewer's own and not in a battle.
    pub removable: bool,
    /// The help topic a right-click on the line opens: the `help` of its ability or spell.
    pub help: Option<u16>,
    /// The line is one of a hero's item slots, drawn with this picture instead of `icon`.
    pub item: Option<ItemIcon>,
}

/// What a unit costs each turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SheetUpkeep {
    pub gold: i32,
    pub mana: i32,
    pub food: f4x_def::half_int::HalfInt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sheet {
    pub source: Source,
    pub race: Option<String>,
    pub name: String,
    /// The unit type number the figure pictures use, and the owner's banner.
    pub unit_type: Option<usize>,
    pub banner: u8,
    pub figures: u32,
    pub water: bool,
    pub moves: u32,
    pub move_icon: usize,
    /// Gold, mana and food upkeep (the map sheet).
    pub upkeep: Option<SheetUpkeep>,
    /// Damage on the front figure (the battle sheet).
    pub damage: Option<i32>,
    pub melee: Row,
    pub ranged: Option<Row>,
    pub defense: Row,
    pub resist: Row,
    /// Hit points of one figure; `grey` is not used. `damage` is on the front figure.
    pub hits: Row,
    pub hurt: i32,
    pub specials: Vec<Special>,
    pub can_dismiss: bool,
    /// A hero: its portrait and name, and its item slots head the list.
    pub hero: Option<HeroView>,
}

/// The `(lbx, entry)` of a picture index of the original's `special_seg`.
pub(crate) fn special_picture(index: usize) -> (&'static str, usize) {
    if index >= 111 {
        ("special2", index - 111)
    } else {
        ("special", index)
    }
}

/// The stat-icon number of the melee row for a weapon quality (0 normal, 1 magic, 2 mithril,
/// 3 adamantium).
pub(crate) fn melee_icon(quality: u8) -> usize {
    match quality {
        1 => 3,
        2 => 2,
        3 => 4,
        _ => 0,
    }
}

/// The stat-icon number of the ranged row for a unit's abilities.
pub(crate) fn ranged_icon(abilities: &[Ability]) -> usize {
    let has = |a: Ability| abilities.contains(&a);
    if has(Ability::new(20))
        || has(Ability::new(37))
        || has(Ability::new(64))
        || has(Ability::new(15))
        || has(Ability::new(46))
    {
        7
    } else if has(Ability::new(69)) {
        8
    } else if has(Ability::new(57)) || has(Ability::new(59)) {
        6
    } else if has(Ability::new(3)) {
        5
    } else if [
        Ability::new(22), // fireball attack
        Ability::new(36), // lightning bolt
        Ability::new(29), // ice bolt
        Ability::new(58), // shaman bolt
        Ability::new(60), // sorcery bolt
        Ability::new(14), // death bolt
        Ability::new(18), // drow bolt
        Ability::new(63), // sprite bolt
        Ability::new(47), // nature bolt
        Ability::new(30), // illusion attack
    ]
    .iter()
    .any(|&a| has(a))
    {
        1
    } else {
        5
    }
}

/// The quiver line ("Arrows x8", "Spells x3") for a ranged unit that is not a spell caster.
pub(crate) fn quiver_name(texts: &Texts, abilities: &[Ability], ammo: i32) -> Option<String> {
    let has = |a: Ability| abilities.contains(&a);
    let caster = has(Ability::new(61)) || has(Ability::new(62));
    let key = if has(Ability::new(57)) {
        "unit.quiver_rock"
    } else if has(Ability::new(3)) {
        "unit.quiver_arrow"
    } else if has(Ability::new(59)) {
        "unit.quiver_bullet"
    } else if !caster && ranged_icon(abilities) == 1 {
        "unit.quiver_spell"
    } else {
        return None;
    };
    Some(texts.get(key, &Args::new().count("n", ammo)))
}

/// Abilities the list shows, in the original's order, with their picture.
const LISTED: &[(Ability, usize)] = &[
    // Movement and special unit abilities (`USW_Abilities`).
    (Ability::new(68), 0),   // teleport
    (Ability::new(25), 1),   // forester
    (Ability::new(45), 2),   // mountaineer
    (Ability::new(50), 20),  // pathfinding
    (Ability::new(34), 14),  // large_shield
    (Ability::new(70), 15),  // wall_crusher
    (Ability::new(27), 16),  // healer
    (Ability::new(12), 17),  // create_outpost
    (Ability::new(33), 18),  // invisibility
    (Ability::new(13), 19),  // create_undead
    (Ability::new(38), 129), // long_range
    (Ability::new(42), 40),  // meld
    (Ability::new(49), 22),  // non_corporeal
    (Ability::new(74), 23),  // wind_walking
    (Ability::new(55), 24),  // regeneration
    (Ability::new(54), 25),  // purify
    (Ability::new(48), 48),  // negate_first_strike
    (Ability::new(51), 91),  // planar_travel
    (Ability::new(71), 71),  // water_walking
    // Summoning and spell casting.
    (Ability::new(67), 139), // summon_demon
    (Ability::new(61), 55),  // spellcaster
    (Ability::new(62), 55),  // spellcaster_major
    // Immunities and bonuses (`USW_Attributes_1` and `_2`).
    (Ability::new(39), 127), // lucky
    (Ability::new(52), 5),   // poison_immunity
    (Ability::new(21), 6),   // fire_immunity
    (Ability::new(65), 7),   // stoning_immunity
    (Ability::new(72), 8),   // weapon_immunity
    (Ability::new(44), 9),   // missile_immunity
    (Ability::new(31), 10),  // illusion_immunity
    (Ability::new(10), 11),  // cold_immunity
    (Ability::new(40), 12),  // magic_immunity
    (Ability::new(16), 49),  // death_immunity
    (Ability::new(6), 38),   // casts_healing
    (Ability::new(5), 39),   // casts_fireball
    (Ability::new(4), 41),   // casts_doom_bolt
    (Ability::new(32), 32),  // immolation
    (Ability::new(8), 132),  // cause_fear
    (Ability::new(7), 131),  // casts_web
    (Ability::new(56), 33),  // resist_all
    (Ability::new(28), 34),  // holy_bonus
    // Attack flags (`USW_ATK_Flags`) and the special ranged attacks (`USW_SR_Attacks`).
    (Ability::new(1), 28),   // armor_piercing
    (Ability::new(53), 29),  // poison_touch
    (Ability::new(35), 30),  // life_steal
    (Ability::new(30), 116), // illusion_attack
    (Ability::new(66), 35),  // stoning_touch
    (Ability::new(17), 142), // dispel_evil
    (Ability::new(23), 133), // first_strike
    (Ability::new(69), 130), // thrown
    (Ability::new(20), 138), // fire_breath
    (Ability::new(37), 137), // lightning_breath
    (Ability::new(64), 26),  // stoning_gaze
    (Ability::new(15), 135), // death_gaze
    (Ability::new(46), 134), // multi_gaze
];

/// The pictures of the lasting spells on a unit: spell number, the icon of the unit sheet
/// (`_unit_enchantment_data`, `USW_Combat_FX`; a picture of `special.lbx`) and how the spell
/// shows over the unit in a battle. Whirlwind and Mind Twist, which the original has
/// pictures for, are no spells in the rules (task 465).
const ENCHANTMENT_ART: &[(u16, usize, Option<usize>)] = &[
    (99 /* immolation */, 117, None),
    (44 /* guardian_wind */, 118, None),
    (179 /* berserk */, 128, None),
    (164 /* cloak_of_fear */, 97, None),
    (175 /* black_channels */, 67, None),
    (181 /* wraith_form */, 68, None),
    (31 /* regeneration */, 69, None),
    (16 /* path_finding */, 70, None),
    (8 /* water_walking */, 71, None),
    (2 /* resist_elements */, 72, None),
    (21 /* elemental_armor */, 73, None),
    (7 /* stone_skin */, 74, None),
    (24 /* iron_skin */, 75, None),
    (123 /* endurance */, 76, None),
    (54 /* spell_lock */, 119, None),
    (62 /* invisibility */, 78, None),
    (63 /* wind_walking */, 79, None),
    (56 /* flight */, 80, None),
    (41 /* resist_magic */, 81, None),
    (69 /* magic_immunity */, 82, None),
    (94 /* flame_blade */, 83, None),
    (86 /* eldritch_weapon */, 84, None),
    (131 /* true_sight */, 85, None),
    (124 /* holy_weapon */, 86, None),
    (130 /* heroism */, 87, None),
    (121 /* bless */, 88, None),
    (141 /* lionheart */, 89, None),
    (4 /* giant_strength */, 65, None),
    (138 /* planar_travel */, 91, None),
    (126 /* holy_armor */, 92, None),
    (144 /* righteousness */, 93, None),
    (143 /* invulnerability */, 94, None),
    (53 /* vertigo */, 101, Some(16)),
    (46 /* confusion */, 111, Some(17)),
    (67 /* mind_storm */, 113, Some(19)),
    (88 /* shatter */, 95, Some(20)),
    (162 /* weakness */, 96, Some(21)),
    (165 /* black_sleep */, 114, None),
    (89 /* warp_creature */, 122, None),
    (170 /* mana_leak */, 121, None),
    (70 /* haste */, 77, None),
    (5 /* web */, 99, Some(23)),
    (73 /* creature_binding */, 100, None),
    (172 /* possession */, 98, None),
];

/// The picture of a lasting spell on a unit in the unit sheet.
pub(crate) fn enchantment_icon(spell: SpellId) -> Option<usize> {
    ENCHANTMENT_ART
        .iter()
        .find(|(n, _, _)| *n == spell.num())
        .map(|&(_, icon, _)| icon)
}

/// The animated curse picture over a unit with these lasting spells in a battle: the `chriver`
/// entry (16 to 23, `Battle_Unit_Curse_Effects`) of the lowest one. Black Sleep has none, as in
/// the original, which only greys the figure.
pub(crate) fn curse_picture(spells: &[SpellId]) -> Option<usize> {
    ENCHANTMENT_ART
        .iter()
        .filter(|&&(n, _, _)| spells.iter().any(|s| s.num() == n))
        .filter_map(|&(_, _, entry)| entry)
        .min()
}

/// What the abilities list is built from.
pub(crate) struct SpecialsInput<'a> {
    /// The level's name, the experience points and the level (0-based), for normal units.
    pub level: Option<(&'a str, u32, usize)>,
    pub abilities: &'a [Ability],
    /// The unit is held in stasis (a line of its own, not an ability).
    pub stasis_held: bool,
    pub ammo: i32,
    pub to_hit: i32,
    pub construction: bool,
    /// The scouting level: how far the unit sees, when that is more than one square.
    pub scouting: i32,
    /// The mana of a spell caster unit.
    pub mana: i32,
    pub undead: bool,
    /// How many land units a ship carries (0 for everything else).
    pub transport: i32,
    /// Display name of an ability.
    pub ability_name: &'a dyn Fn(Ability) -> String,
    /// The help topic of an ability (its `help`).
    pub ability_help: &'a dyn Fn(Ability) -> Option<u16>,
    pub texts: &'a Texts,
    /// Lasting spells on the unit: spell, id, name, and whether the viewer may turn it off.
    pub enchantments: Vec<(f4x_def::rules::SpellId, String, bool, Option<u16>)>,
}

/// The abilities list in the original's order: level, quiver, to hit, construction, the listed
/// abilities, then the lasting spells.
pub(crate) fn specials(input: &SpecialsInput) -> Vec<Special> {
    let mut out = Vec::new();
    let plain = |name: String, icon: Option<usize>| Special {
        name,
        icon,
        spell: None,
        removable: false,
        help: None,
        item: None,
    };
    if let Some((name, xp, level)) = input.level {
        let args = Args::new().text("name", name).count("xp", xp);
        out.push(plain(
            input.texts.get("unit.level", &args),
            Some(102 + level.min(5)),
        ));
    }
    if let Some(name) = quiver_name(input.texts, input.abilities, input.ammo) {
        out.push(plain(name, Some(136)));
    }
    if input.to_hit != 0 {
        let (icon, key) = if input.to_hit > 0 {
            (125, "unit.to_hit_plus")
        } else {
            (126, "unit.to_hit_minus")
        };
        let args = Args::new().count("n", input.to_hit);
        out.push(plain(input.texts.get(key, &args), Some(icon)));
    }
    if input.construction {
        out.push(plain(
            input.texts.get("unit.construction", &Args::new()),
            Some(36),
        ));
    }
    if input.scouting > 1 {
        let args = Args::new().count("n", input.scouting);
        out.push(plain(input.texts.get("unit.scouting", &args), Some(37)));
    }
    if input.transport > 0 {
        let args = Args::new().count("n", input.transport);
        out.push(plain(input.texts.get("unit.carries", &args), None));
    }
    let has = |a: Ability| input.abilities.contains(&a);
    for &(id, icon) in LISTED {
        if !has(id) {
            continue;
        }
        // Pathfinding replaces the forester and mountaineer lines it contains.
        if has(Ability::new(50)) && (id == Ability::new(25) || id == Ability::new(45)) {
            continue;
        }
        let mut name = (input.ability_name)(id);
        if (id == Ability::new(61) || id == Ability::new(62)) && input.mana > 0 {
            name = input
                .texts
                .get("unit.caster", &Args::new().count("n", input.mana));
        }
        out.push(Special {
            help: (input.ability_help)(id),
            ..plain(name, Some(icon))
        });
    }
    if input.undead {
        out.push(plain(
            input.texts.get("unit.undead", &Args::new()),
            Some(120),
        ));
    }
    if input.stasis_held {
        out.push(plain(
            input.texts.get("unit.stasis_held", &Args::new()),
            None,
        ));
    }
    for (marker, key) in [
        (Ability::new(104), "unit.mutation_armor"),
        (Ability::new(105), "unit.mutation_wings"),
        (Ability::new(106), "unit.mutation_breath"),
    ] {
        if has(marker) {
            out.push(plain(input.texts.get(key, &Args::new()), None));
        }
    }
    for (spell, name, removable, help) in &input.enchantments {
        out.push(Special {
            name: name.clone(),
            icon: enchantment_icon(*spell),
            spell: Some(*spell),
            removable: *removable,
            help: *help,
            item: None,
        });
    }
    out
}

/// One icon of a row: where it goes (pixels from the row's start), and how it looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cell {
    pub x: i32,
    pub y: i32,
    pub gold: bool,
    pub dark: bool,
}

/// The icons of a stat row (`UV_Draw_Stat_Icons`): icons ten pixels apart, a gap after every
/// fifth, at most two rows of `row_length`, the second row a little to the right and below.
pub(crate) fn stat_cells(row: Row, row_length: i32) -> Vec<Cell> {
    let mut total = row.value + row.grey;
    let mut grey = row.grey;
    if total < 1 {
        return Vec::new();
    }
    if row_length * 2 < total {
        if row_length * 2 < row.value {
            total = row_length * 2;
            grey = 0;
        } else {
            grey = row_length * 2 - row.value;
            total = row.value + grey;
        }
    }
    cells(total, row_length, |i| {
        (i >= total - row.gold, i >= total - grey)
    })
}

/// The hearts of the hits row (`UV_Draw_Hits_Icons`): the last `damage` are darkened.
pub(crate) fn hit_cells(per_figure: i32, bonus: i32, damage: i32, row_length: i32) -> Vec<Cell> {
    if per_figure < 1 {
        return Vec::new();
    }
    let base = per_figure - bonus;
    let total = per_figure.min(row_length * 2);
    cells(total, row_length, |i| (i >= base, i >= total - damage))
}

fn cells(total: i32, row_length: i32, look: impl Fn(i32) -> (bool, bool)) -> Vec<Cell> {
    (0..total)
        .map(|i| {
            let (col, second) = if i < row_length {
                (i, false)
            } else {
                (i - row_length, true)
            };
            let (gold, dark) = look(i);
            Cell {
                x: 10 * col + 2 * (col / 5) + if second { 3 } else { 0 },
                y: if second { 2 } else { 0 },
                gold,
                dark,
            }
        })
        .collect()
}

/// Where figure `index` of `count` stands on the sheet's diamond, from the figure's top left
/// (`Unit_Figure_Position`).
pub(crate) fn figure_spot(count: usize, index: usize) -> (i32, i32) {
    const TABLE: &[&[(i32, i32)]] = &[
        &[(1, 8)],
        &[(-8, 9), (8, 9)],
        &[(0, 4), (-7, 10), (8, 10)],
        &[(1, 3), (-9, 8), (11, 8), (1, 13)],
        &[(1, 3), (-9, 8), (1, 8), (9, 8), (1, 14)],
        &[(1, 3), (4, 7), (-10, 8), (10, 10), (-3, 11), (1, 15)],
        &[(1, 3), (6, 6), (-9, 8), (1, 8), (10, 8), (-3, 11), (1, 14)],
        &[
            (1, 3),
            (6, 6),
            (-2, 7),
            (-10, 8),
            (10, 8),
            (3, 9),
            (-5, 11),
            (1, 14),
        ],
    ];
    let row = TABLE[count.clamp(1, 8) - 1];
    row[index.min(row.len() - 1)]
}

/// The movement icon for a unit's movement type.
fn move_icon_of(movement: MovementType) -> usize {
    match movement {
        MovementType::Walking => 11,
        MovementType::Flying => 12,
        MovementType::Swimming | MovementType::Sailing => 13,
    }
}

impl App {
    /// The sheet of a unit on the map.
    pub(crate) fn overland_sheet(&self, unit: hecs::Entity) -> Option<Sheet> {
        let game = &self.game;
        let units = game.units();
        let def = units.unit_def(unit)?;
        let content = game.content();
        let abilities = units.unit_abilities(unit);
        let stasis_held = game.unit_in_stasis(unit);
        let stat = |s: Stat| units.unit_stat(unit, s);
        let base = |s: Stat| def.stats.get(&s).copied().unwrap_or(0);
        let hp = stat(Stat::MaxHp).max(1);
        let (cur, _) = units.unit_hits(unit);
        let cur = cur as i32;
        let figures = if cur > 0 { (cur + hp - 1) / hp } else { 0 };
        let front = if cur > 0 {
            hp - ((cur - 1) % hp + 1)
        } else {
            0
        };
        let owner = units.unit_owner(unit)?;
        let ranged = stat(Stat::Ranged);
        let level_path = def.level_path.map(|p| content.rules.level_paths.get(p));
        let level = level_path
            .filter(|_| def.kind == UnitKind::Normal && !game.units().unit_is_undead(unit))
            .and_then(|p| {
                let i = units.unit_level(unit);
                p.level(i)
                    .map(|_| (self.names.level(p.num, i), units.unit_experience(unit), i))
            });
        let ability_name = |id: Ability| {
            self.names
                .ability(content.rules.abilities.get(id).num)
                .to_string()
        };
        let ability_help = |id: Ability| content.rules.abilities.get(id).help;
        let enchantments = units
            .unit_enchantments(unit)
            .into_iter()
            .map(|(spell, caster)| {
                let def = content.rules.spells.get(spell);
                (
                    spell,
                    self.names.spell(def.num).to_string(),
                    caster == crate::view::HUMAN,
                    def.help,
                )
            })
            .collect();
        let mana = if abilities.contains(&Ability::new(62)) {
            40
        } else if abilities.contains(&Ability::new(61)) {
            20
        } else {
            0
        };
        let mut specials = specials(&SpecialsInput {
            level,
            abilities: &abilities,
            stasis_held,
            ammo: stat(Stat::Ammo),
            to_hit: stat(Stat::ToHit),
            construction: stat(Stat::Construction) > 0,
            scouting: (stat(Stat::Sight).max(1) - 1) / 2,
            mana,
            undead: units.unit_is_undead(unit),
            transport: stat(Stat::Transport),
            ability_name: &ability_name,
            ability_help: &ability_help,
            texts: self.texts(),
            enchantments,
        });
        // What temporary effects do to the unit, after its own abilities.
        specials.extend(
            self.temp_effect_lines(unit)
                .into_iter()
                .map(|name| Special {
                    name,
                    icon: None,
                    spell: None,
                    removable: false,
                    help: None,
                    item: None,
                }),
        );
        let hero = self.hero_view(unit, &mut specials);
        let art = self.art.unit_entry(&def.id);
        Some(Sheet {
            source: Source::Overland(unit),
            race: self.sheet_race(def),
            name: self.names.unit(def.num).to_string(),
            unit_type: art
                .as_ref()
                .and_then(|(lbx, entry)| crate::art::unit_type_number(lbx, *entry)),
            banner: self.banner_index(owner.0),
            figures: figures as u32,
            water: matches!(def.movement, MovementType::Sailing | MovementType::Swimming)
                || stat(Stat::Transport) > 0,
            moves: stat(Stat::Moves).max(0) as u32,
            move_icon: move_icon_of(def.movement),
            upkeep: Some(SheetUpkeep {
                gold: units.unit_gold_upkeep(unit),
                mana: def.upkeep.mana as i32,
                food: units.unit_food(unit),
            }),
            damage: None,
            melee: Row::stat(
                melee_icon(units.unit_weapon_quality(unit)),
                stat(Stat::Melee),
                base(Stat::Melee),
            ),
            ranged: (ranged > 0)
                .then(|| Row::stat(ranged_icon(&abilities), ranged, base(Stat::Ranged))),
            defense: Row::stat(9, stat(Stat::Defense), base(Stat::Defense)),
            resist: Row::stat(14, stat(Stat::Resistance), base(Stat::Resistance)),
            hits: Row {
                grey: 0,
                ..Row::stat(10, hp, base(Stat::MaxHp))
            },
            hurt: front,
            specials,
            can_dismiss: owner == crate::view::HUMAN,
            hero,
        })
    }

    /// A hero's portrait and name, with its three item slots put at the head of `specials` (the
    /// original lists the items first, then the abilities); `None` for any other unit.
    fn hero_view(&self, unit: hecs::Entity, specials: &mut Vec<Special>) -> Option<HeroView> {
        let units = self.game.units();
        let def = units.unit_def(unit)?;
        let hero = def.hero.as_ref()?;
        let held = self.game.items().hero_items(unit);
        let lines = hero
            .slots()
            .into_iter()
            .zip(held.0)
            .map(|(kind, item)| Special {
                name: item.as_ref().map(|i| i.name.clone()).unwrap_or_default(),
                icon: None,
                spell: None,
                removable: false,
                help: None,
                item: Some(match &item {
                    Some(i) => ItemIcon::Held(self.art.item_icon(i)),
                    None => ItemIcon::Empty(crate::item_screen::slot_icon(kind)),
                }),
            });
        specials.splice(0..0, lines);
        Some(HeroView {
            portrait: self.art.hero_portrait(&def.id).unwrap_or(0),
            name: units.hero_name(unit).unwrap_or_default(),
        })
    }

    /// The sheet of a unit that is only offered (a hero for hire, mercenaries): the unit type's
    /// own numbers, with what its level adds when it has one (`level` is 0-based).
    pub(crate) fn offer_sheet(
        &self,
        def: &f4x_def::rules::UnitDef,
        level: Option<usize>,
        owner: u8,
    ) -> Sheet {
        use f4x_def::rules::effects::Effect;
        use f4x_def::rules::ModOp;
        let content = self.game.content();
        let mut stats = def.stats.clone();
        let mut level_line = None;
        let path = def.level_path.map(|p| content.rules.level_paths.get(p));
        if let (Some(path), Some(level)) = (path, level) {
            // Every level up to this one adds its bonuses.
            for l in path.levels.iter().take(level + 1) {
                for rule in &l.effects {
                    if let Effect::Modify {
                        stat,
                        op: ModOp::Add,
                        value,
                    } = rule.effect
                    {
                        *stats.entry(stat).or_insert(0) += value;
                    }
                }
            }
            if let Some(l) = path.levels.get(level) {
                level_line = Some((self.names.level(path.num, level), l.xp, level));
            }
        }
        let stat = |s: Stat| stats.get(&s).copied().unwrap_or(0);
        let base = |s: Stat| def.stats.get(&s).copied().unwrap_or(0);
        let abilities = &def.abilities;
        let ability_name = |id: Ability| {
            self.names
                .ability(content.rules.abilities.get(id).num)
                .to_string()
        };
        let ability_help = |id: Ability| content.rules.abilities.get(id).help;
        let mana = if abilities.contains(&Ability::new(62)) {
            40
        } else if abilities.contains(&Ability::new(61)) {
            20
        } else {
            0
        };
        let specials = specials(&SpecialsInput {
            level: level_line,
            abilities,
            stasis_held: false,
            ammo: stat(Stat::Ammo),
            to_hit: stat(Stat::ToHit),
            construction: stat(Stat::Construction) > 0,
            scouting: (stat(Stat::Sight).max(1) - 1) / 2,
            mana,
            // No unit definition lists "undead" among its abilities: an offered unit is never undead.
            undead: false,
            transport: stat(Stat::Transport),
            ability_name: &ability_name,
            ability_help: &ability_help,
            texts: self.texts(),
            enchantments: Vec::new(),
        });
        let ranged = stat(Stat::Ranged);
        Sheet {
            source: Source::Offer,
            race: self.sheet_race(def),
            name: self.names.unit(def.num).to_string(),
            unit_type: self
                .art
                .unit_entry(&def.id)
                .and_then(|(lbx, entry)| crate::art::unit_type_number(&lbx, entry)),
            banner: self.banner_index(owner),
            figures: stat(Stat::Figures).max(1) as u32,
            water: matches!(def.movement, MovementType::Sailing | MovementType::Swimming)
                || stat(Stat::Transport) > 0,
            moves: stat(Stat::Moves).max(0) as u32,
            move_icon: move_icon_of(def.movement),
            upkeep: Some(SheetUpkeep {
                gold: def.upkeep.gold as i32,
                mana: def.upkeep.mana as i32,
                food: def.upkeep.food,
            }),
            damage: None,
            melee: Row::stat(melee_icon(0), stat(Stat::Melee), base(Stat::Melee)),
            ranged: (ranged > 0)
                .then(|| Row::stat(ranged_icon(abilities), ranged, base(Stat::Ranged))),
            defense: Row::stat(9, stat(Stat::Defense), base(Stat::Defense)),
            resist: Row::stat(14, stat(Stat::Resistance), base(Stat::Resistance)),
            hits: Row {
                grey: 0,
                ..Row::stat(10, stat(Stat::MaxHp).max(1), base(Stat::MaxHp))
            },
            hurt: 0,
            specials,
            can_dismiss: false,
            hero: None,
        }
    }

    /// The race prefix of a race-specific unit's name ("High Elf" in "High Elf Magicians"):
    /// units of a race show it, creatures and heroes do not.
    fn sheet_race(&self, def: &f4x_def::rules::UnitDef) -> Option<String> {
        let race = def.race?;
        Some(
            self.names
                .race(self.game.content().rules.races.get(race).num)
                .to_string(),
        )
    }

    /// The sheet of a unit in the battle on screen.
    pub(crate) fn battle_sheet(&self, battle: &Battle, id: usize) -> Option<Sheet> {
        let u: &BattleUnit = battle.unit(id)?;
        let content = self.game.content();
        let def = content.rules.units.get(u.unit_id?);
        let base = |s: Stat| def.stats.get(&s).copied().unwrap_or(0);
        let ability_name = |id: Ability| {
            self.names
                .ability(content.rules.abilities.get(id).num)
                .to_string()
        };
        let ability_help = |id: Ability| content.rules.abilities.get(id).help;
        let enchantments = u
            .enchantments
            .iter()
            .copied()
            .chain(u.combat_spells.iter().map(|&(spell, _)| spell))
            .map(|spell| {
                let s = content.rules.spells.get(spell);
                (spell, self.names.spell(s.num).to_string(), false, s.help)
            })
            .collect();
        let specials = specials(&SpecialsInput {
            level: None,
            abilities: &u.abilities,
            stasis_held: false,
            ammo: u.ammo,
            to_hit: u.to_hit,
            construction: false,
            scouting: 0,
            mana: if u.ranged_kind == RangedKind::Magic {
                u.mana
            } else {
                0
            },
            undead: u.undead,
            transport: 0,
            ability_name: &ability_name,
            ability_help: &ability_help,
            texts: self.texts(),
            enchantments,
        });
        let owner = battle.player_of(u.side).0;
        let art = self.art.unit_entry(&def.id);
        Some(Sheet {
            source: Source::Battle(id),
            race: self.sheet_race(def),
            name: self.names.unit(def.num).to_string(),
            unit_type: art
                .as_ref()
                .and_then(|(lbx, entry)| crate::art::unit_type_number(lbx, *entry)),
            banner: self.banner_index(owner),
            figures: u.figures,
            water: matches!(def.movement, MovementType::Sailing | MovementType::Swimming),
            moves: u.moves / 2,
            move_icon: if u.flies {
                12
            } else {
                move_icon_of(def.movement)
            },
            upkeep: None,
            damage: Some(u.front_damage),
            melee: Row::stat(melee_icon(u.weapon_quality), u.melee, base(Stat::Melee)),
            ranged: (u.ranged > 0)
                .then(|| Row::stat(ranged_icon(&u.abilities), u.ranged, base(Stat::Ranged))),
            defense: Row::stat(9, u.defense, base(Stat::Defense)),
            resist: Row::stat(14, u.resistance, base(Stat::Resistance)),
            hits: Row {
                grey: 0,
                ..Row::stat(10, u.hits, base(Stat::MaxHp))
            },
            hurt: u.front_damage,
            specials,
            can_dismiss: false,
            hero: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curse_pictures_follow_the_lowest_entry_and_black_sleep_has_none() {
        let s = |n: u16| SpellId::new(n);
        // Web (23) and Vertigo (16): the lower entry wins.
        assert_eq!(
            curse_picture(&[s(5) /* web */, s(53) /* vertigo */]),
            Some(16)
        );
        // Black Sleep only greys the figure, as in the original.
        assert_eq!(curse_picture(&[s(165) /* black_sleep */]), None);
        assert_eq!(
            curse_picture(&[s(165) /* black_sleep */, s(162) /* weakness */]),
            Some(21)
        );
        assert_eq!(curse_picture(&[s(70) /* haste */]), None);
    }

    #[test]
    fn invisibility_spell_has_its_icon() {
        // The spell's id was once misspelled ("invisiblity"), which hid its icon while the
        // table was keyed by alias.
        assert_eq!(
            enchantment_icon(SpellId::new(62) /* invisibility */),
            Some(78)
        );
    }

    /// The English messages of the shipped content.
    fn en() -> &'static Texts {
        crate::test_app::shipped::texts()
    }

    fn names(list: &[Special]) -> Vec<&str> {
        list.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn a_stat_row_marks_the_bonus_icons_gold_and_leaves_a_gap_after_five() {
        let row = Row::stat(0, 7, 5);
        let cells = stat_cells(row, 15);
        assert_eq!(cells.len(), 7);
        assert_eq!(cells[0].x, 0);
        assert_eq!(cells[4].x, 40);
        // The sixth icon follows the gap.
        assert_eq!(cells[5].x, 52);
        assert!(!cells[4].gold && cells[5].gold && cells[6].gold);
    }

    #[test]
    fn a_curse_leaves_darkened_icons_and_a_long_row_wraps_to_a_second_one() {
        let curse = Row::stat(0, 3, 5);
        let cells = stat_cells(curse, 15);
        assert_eq!(cells.len(), 5);
        assert!(cells[3].dark && cells[4].dark && !cells[2].dark);
        let long = stat_cells(Row::stat(0, 20, 20), 15);
        assert_eq!(long.len(), 20);
        assert_eq!((long[15].x, long[15].y), (3, 2));
        // More than two rows are cut off.
        assert_eq!(stat_cells(Row::stat(0, 40, 40), 15).len(), 30);
    }

    #[test]
    fn hearts_darken_from_the_end_and_the_bonus_ones_are_gold() {
        let cells = hit_cells(5, 1, 2, 15);
        assert_eq!(cells.len(), 5);
        assert!(cells[4].gold && !cells[3].gold);
        assert!(cells[3].dark && cells[4].dark && !cells[2].dark);
        assert!(hit_cells(0, 0, 0, 15).is_empty());
    }

    #[test]
    fn the_list_runs_level_quiver_to_hit_abilities_then_spells() {
        // bow, first strike, poison immunity
        let abilities = [Ability::new(3), Ability::new(23), Ability::new(52)];
        let name = |id: Ability| format!("ability {}", id.num());
        let no_help = |_: Ability| None;
        let list = specials(&SpecialsInput {
            level: Some(("Veteran", 65, 2)),
            abilities: &abilities,
            ammo: 8,
            to_hit: 1,
            construction: false,
            scouting: 0,
            mana: 0,
            undead: false,
            transport: 0,
            ability_name: &name,
            ability_help: &no_help,
            stasis_held: false,
            texts: en(),
            enchantments: Vec::new(),
        });
        assert_eq!(
            names(&list),
            [
                "Veteran (65 ep)",
                "Arrows x8",
                "+1 To Hit",
                "ability 52",
                "ability 23"
            ]
        );
        assert_eq!(list[0].icon, Some(104));
    }

    #[test]
    fn pathfinding_stands_in_for_forester_and_mountaineer() {
        // forester, mountaineer, pathfinding
        let abilities = [Ability::new(25), Ability::new(45), Ability::new(50)];
        let name = |id: Ability| format!("ability {}", id.num());
        let no_help = |_: Ability| None;
        let list = specials(&SpecialsInput {
            level: None,
            abilities: &abilities,
            ammo: 0,
            to_hit: 0,
            construction: false,
            scouting: 0,
            mana: 0,
            undead: false,
            transport: 0,
            ability_name: &name,
            ability_help: &no_help,
            stasis_held: false,
            texts: en(),
            enchantments: Vec::new(),
        });
        assert_eq!(names(&list), ["ability 50"]);
    }

    #[test]
    fn spell_casters_show_their_mana_and_no_quiver() {
        // spellcaster, fireball attack
        let abilities = [Ability::new(61), Ability::new(22)];
        let name = |id: Ability| format!("ability {}", id.num());
        let no_help = |_: Ability| None;
        let list = specials(&SpecialsInput {
            level: None,
            abilities: &abilities,
            ammo: 4,
            to_hit: 0,
            construction: false,
            scouting: 0,
            mana: 20,
            undead: false,
            transport: 0,
            ability_name: &name,
            ability_help: &no_help,
            stasis_held: false,
            texts: en(),
            enchantments: Vec::new(),
        });
        assert_eq!(names(&list), ["Caster 20 mp"]);
    }

    #[test]
    fn a_scout_lists_its_scouting_level() {
        let name = |id: Ability| format!("ability {}", id.num());
        let no_help = |_: Ability| None;
        let input = |scouting| SpecialsInput {
            level: None,
            abilities: &[],
            ammo: 0,
            to_hit: 0,
            construction: false,
            scouting,
            mana: 0,
            undead: false,
            transport: 0,
            ability_name: &name,
            ability_help: &no_help,
            stasis_held: false,
            texts: en(),
            enchantments: Vec::new(),
        };
        assert!(specials(&input(1)).is_empty());
        assert_eq!(names(&specials(&input(3))), ["Scouting 3"]);
        let mut ship = input(1);
        ship.transport = 5;
        assert_eq!(names(&specials(&ship)), ["Carries 5"]);
    }

    #[test]
    fn figures_stand_on_the_diamond_where_the_original_puts_them() {
        assert_eq!(figure_spot(1, 0), (1, 8));
        assert_eq!(figure_spot(4, 3), (1, 13));
        assert_eq!(figure_spot(8, 7), (1, 14));
    }

    #[test]
    fn the_weapon_and_ranged_icons_follow_the_unit() {
        assert_eq!(melee_icon(0), 0);
        assert_eq!(melee_icon(2), 2);
        // bow, fire breath, rock throwing, ice bolt
        assert_eq!(ranged_icon(&[Ability::new(3)]), 5);
        assert_eq!(ranged_icon(&[Ability::new(20)]), 7);
        assert_eq!(ranged_icon(&[Ability::new(57)]), 6);
        assert_eq!(ranged_icon(&[Ability::new(29)]), 1);
    }
}
