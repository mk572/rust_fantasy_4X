//! The unit statistics sheet: what the original's `Unit_Statistics_Popup` shows about
//! one unit, built from the game (this file) and drawn by `unit_screen.rs`.
//!
//! The layout, the icon rows and the order of the abilities list follow ReMoM (`UnitStat.c`
//! `Thing_View_Draw`, `UV_Draw_Stat_Icons`, `UV_Draw_Hits_Icons`; `UnitView.c`
//! `USW_Build_Effect_List`). The numbers in the tables below are picture indexes into the
//! original's `special.lbx` (and `special2.lbx` from 111 on), not art.
use crate::text::{Args, Term, Texts};
use f4x_battle::battle::{Battle, RangedKind};
use f4x_battle::BattleUnit;
use f4x_core::rules::unit_flag::UnitFlag;
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
    /// The picture of an item power, an entry of `itemisc.lbx` too (Merging).
    Power(usize),
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
    /// The item this line is, for its card (a right-click) and the item screen (a click).
    pub card: Option<f4x_core::item::Item>,
}

/// What a unit costs each turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SheetUpkeep {
    pub gold: f4x_def::half_int::HalfInt,
    pub mana: i32,
    pub food: f4x_def::half_int::HalfInt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sheet {
    pub source: Source,
    /// The unit's whole name, its race included ("Dwarven Swordsmen"): a language words it as it likes.
    pub name: String,
    /// The same name as a term, for the messages that need a form of it.
    pub term: Term,
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
    /// The rank and experience line: drawn in the top right corner of the sheet.
    pub level: Option<Special>,
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
    if has(Ability::new(20 /* fire_breath */))
        || has(Ability::new(37 /* lightning_breath */))
        || has(Ability::new(64 /* stoning_gaze */))
        || has(Ability::new(15 /* death_gaze */))
        || has(Ability::new(46 /* doom_gaze */))
    {
        7
    } else if has(Ability::new(69 /* thrown */)) {
        8
    } else if has(Ability::new(57 /* rock_throwing */)) || has(Ability::new(59 /* sling */)) {
        6
    } else if has(Ability::new(3 /* bow */)) {
        5
    } else if [
        Ability::new(22 /* fireball_attack */), // fireball attack
        Ability::new(36 /* lightning_bolt */),  // lightning bolt
        Ability::new(29 /* ice_bolt */),        // ice bolt
        Ability::new(58 /* shaman_bolt */),     // shaman bolt
        Ability::new(60 /* sorcery_bolt */),    // sorcery bolt
        Ability::new(14 /* death_bolt */),      // death bolt
        Ability::new(18 /* drow_bolt */),       // drow bolt
        Ability::new(63 /* sprite_bolt */),     // sprite bolt
        Ability::new(47 /* nature_bolt */),     // nature bolt
        Ability::new(30 /* illusion_attack */), // illusion attack
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
    let caster = has(Ability::new(61 /* spellcaster */))
        || has(Ability::new(62 /* spellcaster_major */));
    let key = if has(Ability::new(57 /* rock_throwing */)) {
        "unit.quiver_rock"
    } else if has(Ability::new(3 /* bow */)) {
        "unit.quiver_arrow"
    } else if has(Ability::new(59 /* sling */)) {
        "unit.quiver_bullet"
    } else if !caster && ranged_icon(abilities) == 1 {
        "unit.quiver_spell"
    } else {
        return None;
    };
    Some(texts.get(key, &Args::new().count("n", ammo)))
}

const LISTED: &[(Ability, usize)] = &[
    // Movement and special unit abilities (`USW_Abilities`).
    (Ability::new(68 /* teleport */), 0),
    (Ability::new(25 /* forester */), 1),
    (Ability::new(45 /* mountaineer */), 2),
    (Ability::new(50 /* pathfinding */), 20),
    (Ability::new(34 /* large_shield */), 14),
    (Ability::new(70 /* wall_crusher */), 15),
    (Ability::new(27 /* healer */), 16),
    (Ability::new(12 /* create_outpost */), 17),
    (Ability::new(33 /* invisibility */), 18),
    (Ability::new(13 /* create_undead */), 19),
    (Ability::new(38 /* long_range */), 129),
    (Ability::new(42 /* meld */), 40),
    (Ability::new(49 /* non_corporeal */), 22),
    (Ability::new(74 /* wind_walking */), 23),
    (Ability::new(55 /* regeneration */), 24),
    (Ability::new(54 /* purify */), 25),
    (Ability::new(48 /* negate_first_strike */), 48),
    (Ability::new(51 /* planar_travel */), 91),
    (Ability::new(71 /* water_walking */), 71),
    // Summoning and spell casting.
    (Ability::new(67 /* summon_demon */), 139),
    (Ability::new(61 /* spellcaster */), 55),
    (Ability::new(62 /* spellcaster_major */), 55),
    // Immunities and bonuses (`USW_Attributes_1` and `_2`).
    (Ability::new(39 /* lucky */), 127),
    (Ability::new(52 /* poison_immunity */), 5),
    (Ability::new(21 /* fire_immunity */), 6),
    (Ability::new(65 /* stoning_immunity */), 7),
    (Ability::new(72 /* weapon_immunity */), 8),
    (Ability::new(44 /* missile_immunity */), 9),
    (Ability::new(31 /* illusion_immunity */), 10),
    (Ability::new(10 /* cold_immunity */), 11),
    (Ability::new(40 /* magic_immunity */), 12),
    (Ability::new(16 /* death_immunity */), 49),
    (Ability::new(6 /* casts_healing */), 38),
    (Ability::new(5 /* casts_fireball */), 39),
    (Ability::new(4 /* casts_doom_bolt */), 41),
    (Ability::new(32 /* immolation */), 32),
    (Ability::new(8 /* cause_fear */), 132),
    (Ability::new(7 /* casts_web */), 131),
    (Ability::new(56 /* resist_all */), 33),
    (Ability::new(28 /* holy_bonus */), 34),
    // Attack flags (`USW_ATK_Flags`) and the special ranged attacks (`USW_SR_Attacks`).
    (Ability::new(1 /* armor_piercing */), 28),
    (Ability::new(53 /* poison_touch */), 29),
    (Ability::new(35 /* life_steal */), 30),
    (Ability::new(101 /* doom_damage */), 31),
    (Ability::new(100 /* destruction */), 140),
    (Ability::new(30 /* illusion_attack */), 116),
    (Ability::new(66 /* stoning_touch */), 35),
    (Ability::new(99 /* death_touch */), 27),
    (Ability::new(103 /* power_drain */), 141),
    (Ability::new(17 /* dispel_evil */), 142),
    (Ability::new(23 /* first_strike */), 133),
    (Ability::new(69 /* thrown */), 130),
    (Ability::new(20 /* fire_breath */), 138),
    (Ability::new(37 /* lightning_breath */), 137),
    (Ability::new(64 /* stoning_gaze */), 26),
    (Ability::new(15 /* death_gaze */), 135),
    (Ability::new(46 /* doom_gaze */), 134),
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
    /// The rules, for what a hero's abilities give per level (`abilities.ron`).
    pub rules: &'a f4x_def::rules::Rules,
    /// The unit's special level (poison strength, the penalty of its touches and gazes) and its
    /// ranged strength, which the abilities that have a number show.
    pub special_level: i32,
    pub ranged: i32,
    /// The abilities an item or a spell gave (their touches cost the target's save more).
    pub granted: &'a [Ability],
    /// The experience level of a hero (0-based): its abilities are listed, with their values.
    pub hero_level: Option<usize>,
    /// Lasting spells on the unit: spell, id, name, and whether the viewer may turn it off.
    pub enchantments: Vec<(f4x_def::rules::SpellId, String, bool, Option<u16>)>,
}

/// The unit's rank and experience ("Veteran (65 ep)") with the picture of its level; the sheet
/// draws it in the top right corner, not in the list.
pub(crate) fn level_special(level: Option<(&str, u32, usize)>, texts: &Texts) -> Option<Special> {
    let (name, xp, level) = level?;
    let args = Args::new().text("name", name).count("xp", xp);
    Some(Special {
        name: texts.get("unit.level", &args),
        icon: Some(102 + level.min(5)),
        spell: None,
        removable: false,
        help: None,
        item: None,
        card: None,
    })
}

/// The picture of the Merging item power in `itemisc.lbx`.
const MERGING_POWER_ICON: usize = 18;

/// A hero's abilities with their values for its level (`f4x_def::hero`): `(ability number of the
/// plain one, its picture in the original's `special_seg`, the value)`; the doubled ability is the
/// next number. Lucky, Charmed and Noble have no value.
fn hero_abilities(input: &SpecialsInput, level: usize, out: &mut Vec<Special>) {
    use f4x_def::hero::{self, HeroAbilities, Tier};
    let abilities = HeroAbilities::from_ids(input.abilities);
    // The ability and its picture in the original's `special_seg`.
    let tiered: &[(&str, usize)] = &[
        ("leadership", 43),
        ("legendary", 45),
        ("blademaster", 47),
        ("armsmaster", 46),
        ("constitution", 50),
        ("might", 52),
        ("arcane_power", 54),
        ("sage", 61),
        ("prayermaster", 57),
        ("agility", 143),
    ];
    for &(name, icon) in tiered {
        let Some(tier) = abilities.tier(name).filter(|&t| t != Tier::None) else {
            continue;
        };
        let id = Ability::new(hero::tiered_number(name) + u16::from(tier == Tier::Super));
        let shown = (input.ability_name)(id);
        let n = input.rules.hero_value(name, tier, level);
        out.push(Special {
            name: if n > 0 {
                input.texts.get(
                    "unit.ability_plus",
                    &Args::new().text("name", shown).count("n", n),
                )
            } else {
                shown
            },
            icon: Some(icon),
            help: (input.ability_help)(id),
            spell: None,
            removable: false,
            item: None,
            card: None,
        });
    }
    for (on, id, icon) in [
        (abilities.lucky, 39, 58),
        (abilities.charmed, 95, 59),
        (abilities.noble, 96, 60),
    ] {
        if on {
            let id = Ability::new(id);
            out.push(Special {
                name: (input.ability_name)(id),
                icon: Some(icon),
                help: (input.ability_help)(id),
                spell: None,
                removable: false,
                item: None,
                card: None,
            });
        }
    }
}

pub(crate) fn specials(input: &SpecialsInput) -> Vec<Special> {
    let mut out = Vec::new();
    let plain = |name: String, icon: Option<usize>| Special {
        name,
        icon,
        spell: None,
        removable: false,
        help: None,
        item: None,
        card: None,
    };
    if let Some(level) = input.hero_level {
        hero_abilities(input, level, &mut out);
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
    if has(Ability::new(43 /* merging */)) {
        out.push(Special {
            help: (input.ability_help)(Ability::new(43 /* merging */)),
            item: Some(ItemIcon::Power(MERGING_POWER_ICON)),
            ..plain((input.ability_name)(Ability::new(43 /* merging */)), None)
        });
    }
    // Some abilities show a number: poison strength, the strength of a thrown weapon or a
    // breath, and what a touch or a gaze takes off the target's save (Life Steal and Death
    // Touch since 1.50).
    let special = f4x_battle::special::SpecialAttacks::of_unit(
        input.abilities,
        input.granted,
        input.special_level,
    );
    let number = |id: Ability| -> Option<i32> {
        match id.num() {
            53 => Some(special.poison),
            69 | 20 | 37 => Some(input.ranged),
            35 | 64 | 15 | 46 => Some(-special.penalty),
            99 => Some(-(special.penalty + special.death_extra)),
            66 => Some(-(special.penalty + special.stoning_extra)),
            _ => None,
        }
        .filter(|&n| n != 0)
    };
    for &(id, icon) in LISTED {
        if !has(id) {
            continue;
        }
        // A hero's Lucky is one of its hero abilities, listed with them.
        if id == Ability::new(39 /* lucky */) && input.hero_level.is_some() {
            continue;
        }
        // Pathfinding replaces the forester and mountaineer lines it contains.
        if has(Ability::new(50 /* pathfinding */))
            && (id == Ability::new(25 /* forester */)
                || id == Ability::new(45 /* mountaineer */))
        {
            continue;
        }
        let mut name = (input.ability_name)(id);
        if let Some(n) = number(id) {
            let args = Args::new().text("name", name).count("n", n);
            name = input.texts.get("unit.ability_value", &args);
        }
        if (id == Ability::new(61 /* spellcaster */)
            || id == Ability::new(62 /* spellcaster_major */))
            && input.mana > 0
        {
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
        (
            Ability::new(104 /* mutation_armor */),
            "unit.mutation_armor",
        ),
        (
            Ability::new(105 /* mutation_wings */),
            "unit.mutation_wings",
        ),
        (
            Ability::new(106 /* mutation_breath */),
            "unit.mutation_breath",
        ),
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
            card: None,
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
        let granted = units.unit_granted_abilities(unit);
        let stasis_held = game.unit_in_stasis(unit);
        let stat = |s: Stat| units.unit_stat(unit, s);
        let base = |s: Stat| def.stats.get(&s).map_or(0, |v| v.floor());
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
            .filter(|_| {
                matches!(def.kind, UnitKind::Normal | UnitKind::Hero)
                    && !game.units().unit_is_undead(unit)
            })
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
        // The lasting spells the unit's items put on it come after its own (`USW_Item_UEs`); they
        // go with the item and cannot be turned off.
        let from_items = game
            .items()
            .spells(unit)
            .into_iter()
            .map(|spell| (spell, crate::view::HUMAN));
        let own = units.unit_enchantments(unit);
        let enchantments = own
            .iter()
            .copied()
            .chain(from_items.filter(|(s, _)| !own.iter().any(|(o, _)| o == s)))
            .map(|(spell, caster)| {
                let def = content.rules.spells.get(spell);
                (
                    spell,
                    self.names.spell(def.num).to_string(),
                    caster == crate::view::HUMAN && own.iter().any(|(o, _)| *o == spell),
                    def.help,
                )
            })
            .collect();
        let mana = if abilities.contains(&Ability::new(62 /* spellcaster_major */)) {
            40
        } else if abilities.contains(&Ability::new(61 /* spellcaster */)) {
            20
        } else {
            0
        };
        let mut specials = specials(&SpecialsInput {
            abilities: &abilities,
            stasis_held,
            ammo: stat(Stat::Ammo),
            to_hit: stat(Stat::ToHit),
            construction: stat(Stat::Construction) > 0,
            scouting: units.unit_sight(unit).floor().max(0),
            mana,
            undead: units.unit_is_undead(unit),
            transport: stat(Stat::Transport),
            ability_name: &ability_name,
            ability_help: &ability_help,
            texts: self.texts(),
            rules: &content.rules,
            enchantments,
            special_level: stat(Stat::SpecialLevel),
            ranged: stat(Stat::Ranged),
            granted: &granted,
            hero_level: def.hero.is_some().then(|| units.unit_level(unit)),
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
                    card: None,
                }),
        );
        let hero = self.hero_view(unit, &mut specials);
        let art = self.art.unit_entry(&def.id);
        Some(Sheet {
            source: Source::Overland(unit),
            name: self.names.unit(def.num).to_string(),
            term: self.names.unit_term(def.num),
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
            level: level_special(level, self.texts()),
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
                card: item.clone(),
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
                        ..
                    } = rule.effect
                    {
                        *stats
                            .entry(stat)
                            .or_insert(f4x_def::half_int::HalfInt::ZERO) += value;
                    }
                }
            }
            if let Some(l) = path.levels.get(level) {
                level_line = Some((self.names.level(path.num, level), l.xp, level));
            }
        }
        let stat = |s: Stat| stats.get(&s).map_or(0, |v| v.floor());
        let base = |s: Stat| def.stats.get(&s).map_or(0, |v| v.floor());
        let abilities = &def.abilities;
        let ability_name = |id: Ability| {
            self.names
                .ability(content.rules.abilities.get(id).num)
                .to_string()
        };
        let ability_help = |id: Ability| content.rules.abilities.get(id).help;
        let mana = if abilities.contains(&Ability::new(62 /* spellcaster_major */)) {
            40
        } else if abilities.contains(&Ability::new(61 /* spellcaster */)) {
            20
        } else {
            0
        };
        let specials = specials(&SpecialsInput {
            abilities,
            stasis_held: false,
            ammo: stat(Stat::Ammo),
            to_hit: stat(Stat::ToHit),
            construction: stat(Stat::Construction) > 0,
            scouting: stats.get(&Stat::Sight).map_or(0, |v| v.floor().max(0)),
            mana,
            // No unit definition lists "undead" among its abilities: an offered unit is never undead.
            undead: false,
            transport: stat(Stat::Transport),
            ability_name: &ability_name,
            ability_help: &ability_help,
            texts: self.texts(),
            rules: &content.rules,
            enchantments: Vec::new(),
            special_level: stat(Stat::SpecialLevel),
            ranged: stat(Stat::Ranged),
            granted: &[],
            hero_level: None,
        });
        let ranged = stat(Stat::Ranged);
        Sheet {
            source: Source::Offer,
            name: self.names.unit(def.num).to_string(),
            term: self.names.unit_term(def.num),
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
                gold: def.upkeep.gold,
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
            level: level_special(level_line, self.texts()),
            can_dismiss: false,
            hero: None,
        }
    }

    /// The portrait of a hero type for a sheet that does not belong to an overland unit; `name`
    /// is the hero's own name when one is known, else empty. `None` for any other unit.
    pub(crate) fn hero_portrait_view(
        &self,
        def: &f4x_def::rules::UnitDef,
        name: &str,
    ) -> Option<HeroView> {
        def.hero.as_ref()?;
        Some(HeroView {
            portrait: self.art.hero_portrait(&def.id)?,
            name: name.to_string(),
        })
    }

    /// The sheet of a unit in the battle on screen.
    pub(crate) fn battle_sheet(&self, battle: &Battle, id: usize) -> Option<Sheet> {
        let u: &BattleUnit = battle.unit(id)?;
        let now = battle.now_of(u);
        let content = self.game.content();
        let def = content.rules.units.get(u.unit_id?);
        let base = |s: Stat| def.stats.get(&s).map_or(0, |v| v.floor());
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
            .chain(u.lasting().iter().map(|l| l.spell))
            .map(|spell| {
                let s = content.rules.spells.get(spell);
                (spell, self.names.spell(s.num).to_string(), false, s.help)
            })
            .collect();
        let mut specials = specials(&SpecialsInput {
            abilities: &u.abilities,
            stasis_held: false,
            ammo: u.ammo,
            to_hit: now.to_hit(),
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
            rules: &content.rules,
            enchantments,
            special_level: u.special_level,
            ranged: now.ranged(),
            granted: &u.granted,
            hero_level: def.hero.is_some().then(|| {
                self.game
                    .battle_unit_source(id)
                    .map_or(0, |e| self.game.units().unit_level(e))
            }),
        });
        // A hero's three item slots head the list, as on the overland sheet; the portrait and
        // the name come from the battle unit, which kept the name when the battle began.
        if let Some(unit) = self.game.battle_unit_source(id) {
            let _ = self.hero_view(unit, &mut specials);
        }
        let owner = battle.player_of(u.side).0;
        let art = self.art.unit_entry(&def.id);
        Some(Sheet {
            source: Source::Battle(id),
            name: self.names.unit(def.num).to_string(),
            term: self.names.unit_term(def.num),
            unit_type: art
                .as_ref()
                .and_then(|(lbx, entry)| crate::art::unit_type_number(lbx, *entry)),
            banner: self.banner_index(owner),
            figures: u.figures,
            water: matches!(def.movement, MovementType::Sailing | MovementType::Swimming),
            moves: u.moves.floor().max(0) as u32,
            move_icon: if u.flag(UnitFlag::Flies) {
                12
            } else {
                move_icon_of(def.movement)
            },
            upkeep: None,
            damage: Some(u.front_damage),
            melee: Row::stat(melee_icon(u.weapon_quality), now.melee(), base(Stat::Melee)),
            ranged: (now.ranged() > 0)
                .then(|| Row::stat(ranged_icon(&u.abilities), now.ranged(), base(Stat::Ranged))),
            defense: Row::stat(9, now.defense(), base(Stat::Defense)),
            resist: Row::stat(14, now.resistance(), base(Stat::Resistance)),
            hits: Row {
                grey: 0,
                ..Row::stat(10, u.hits, base(Stat::MaxHp))
            },
            hurt: u.front_damage,
            specials,
            level: None,
            can_dismiss: false,
            hero: self.hero_portrait_view(def, &u.hero_name),
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
        // Black Sleep only greys the figure.
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

    /// What the heroes' abilities give, from the shipped content.
    fn formulas() -> &'static f4x_def::rules::Rules {
        &f4x_def::testdir::shipped().rules
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
        let abilities = [
            Ability::new(3 /* bow */),
            Ability::new(23 /* first_strike */),
            Ability::new(52 /* poison_immunity */),
        ];
        let name = |id: Ability| format!("ability {}", id.num());
        let no_help = |_: Ability| None;
        let list = specials(&SpecialsInput {
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
            rules: formulas(),
            enchantments: Vec::new(),
            special_level: 0,
            ranged: 0,
            granted: &[],
            hero_level: None,
        });
        assert_eq!(
            names(&list),
            ["Arrows x8", "+1 To Hit", "ability 52", "ability 23"]
        );
        // The rank is no line of the list: the sheet draws it in its top right corner.
        let rank = level_special(Some(("Veteran", 65, 2)), en()).expect("a level has a line");
        assert_eq!(rank.name, "Veteran (65 ep)");
        assert_eq!(rank.icon, Some(104));
        assert!(level_special(None, en()).is_none());
    }

    #[test]
    fn pathfinding_stands_in_for_forester_and_mountaineer() {
        // forester, mountaineer, pathfinding
        let abilities = [
            Ability::new(25 /* forester */),
            Ability::new(45 /* mountaineer */),
            Ability::new(50 /* pathfinding */),
        ];
        let name = |id: Ability| format!("ability {}", id.num());
        let no_help = |_: Ability| None;
        let list = specials(&SpecialsInput {
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
            rules: formulas(),
            enchantments: Vec::new(),
            special_level: 0,
            ranged: 0,
            granted: &[],
            hero_level: None,
        });
        assert_eq!(names(&list), ["ability 50"]);
    }

    #[test]
    fn spell_casters_show_their_mana_and_no_quiver() {
        // spellcaster, fireball attack
        let abilities = [
            Ability::new(61 /* spellcaster */),
            Ability::new(22 /* fireball_attack */),
        ];
        let name = |id: Ability| format!("ability {}", id.num());
        let no_help = |_: Ability| None;
        let list = specials(&SpecialsInput {
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
            rules: formulas(),
            enchantments: Vec::new(),
            special_level: 0,
            ranged: 0,
            granted: &[],
            hero_level: None,
        });
        assert_eq!(names(&list), ["Caster 20 mp"]);
    }

    #[test]
    fn life_steal_and_death_touch_show_what_they_take_off_the_save() {
        let name = |id: Ability| match id.num() {
            35 => "Life Steal".to_string(),
            99 => "Death Touch".to_string(),
            n => format!("ability {n}"),
        };
        let no_help = |_: Ability| None;
        // Life steal, destruction, death touch, power drain and doom damage.
        let abilities = [
            Ability::new(103 /* power_drain */),
            Ability::new(99 /* death_touch */),
            Ability::new(100 /* destruction */),
            Ability::new(101 /* doom_damage */),
            Ability::new(35 /* life_steal */),
        ];
        // Special level 2; the unit's own life steal costs 2, a death touch an item gave 2 + 3.
        let granted = [Ability::new(99 /* death_touch */)];
        let list = specials(&SpecialsInput {
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
            rules: formulas(),
            special_level: 2,
            ranged: 0,
            granted: &granted,
            hero_level: None,
            enchantments: Vec::new(),
        });
        assert_eq!(
            names(&list),
            [
                "Life Steal -2",
                "ability 101",
                "ability 100",
                "Death Touch -5",
                "ability 103"
            ]
        );
    }

    fn listed(
        abilities: &[Ability],
        special_level: i32,
        ranged: i32,
        hero: Option<usize>,
    ) -> Vec<Special> {
        let name = |id: Ability| format!("ability {}", id.num());
        let no_help = |_: Ability| None;
        specials(&SpecialsInput {
            abilities,
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
            rules: formulas(),
            special_level,
            ranged,
            granted: &[],
            hero_level: hero,
            enchantments: Vec::new(),
        })
    }

    #[test]
    fn a_hero_lists_its_abilities_with_their_values_for_its_level() {
        // Super Might (86), Leadership (75), Armsmaster (81), Sage (89), Lucky (39), Charmed (95),
        // Noble (96), at level 2 (the third): x = 3.
        let abilities = [
            Ability::new(86 /* super_might */),
            Ability::new(75 /* leadership */),
            Ability::new(81 /* armsmaster */),
            Ability::new(89 /* sage */),
            Ability::new(39 /* lucky */),
            Ability::new(95 /* charmed */),
            Ability::new(96 /* noble */),
        ];
        let list = listed(&abilities, 0, 0, Some(2));
        assert_eq!(
            names(&list),
            [
                "ability 75 +1",
                "ability 81 +6",
                "ability 86 +4",
                "ability 89 +6",
                "ability 39",
                "ability 95",
                "ability 96"
            ]
        );
        assert_eq!(list[5].icon, Some(59));
        assert_eq!(list[6].icon, Some(60));
        // A unit that is no hero lists none of them (they are not its abilities).
        assert_eq!(listed(&abilities, 0, 0, None).len(), 1, "only Lucky");
    }

    #[test]
    fn merging_is_listed_with_the_picture_of_its_item_power() {
        let list = listed(
            &[
                Ability::new(43 /* merging */),
                Ability::new(68 /* teleport */),
            ],
            0,
            0,
            None,
        );
        assert_eq!(names(&list), ["ability 43", "ability 68"]);
        assert_eq!(list[0].item, Some(ItemIcon::Power(18)));
    }

    #[test]
    fn strengths_and_saves_follow_the_name_of_the_ability_that_has_one() {
        // Poison touch 4, thrown 3, fire breath 5, a stoning gaze -2 and a stoning touch -1.
        assert_eq!(
            names(&listed(&[Ability::new(53 /* poison_touch */)], 4, 0, None)),
            ["ability 53 4"]
        );
        assert_eq!(
            names(&listed(&[Ability::new(69 /* thrown */)], 0, 3, None)),
            ["ability 69 3"]
        );
        assert_eq!(
            names(&listed(&[Ability::new(20 /* fire_breath */)], 0, 5, None)),
            ["ability 20 5"]
        );
        assert_eq!(
            names(&listed(&[Ability::new(64 /* stoning_gaze */)], 2, 1, None)),
            ["ability 64 -2"]
        );
        assert_eq!(
            names(&listed(&[Ability::new(66 /* stoning_touch */)], 1, 0, None)),
            ["ability 66 -1"]
        );
        // Nothing to show: a touch with no penalty stays plain.
        assert_eq!(
            names(&listed(&[Ability::new(66 /* stoning_touch */)], 0, 0, None)),
            ["ability 66"]
        );
    }

    #[test]
    fn a_scout_lists_its_scouting_level() {
        let name = |id: Ability| format!("ability {}", id.num());
        let no_help = |_: Ability| None;
        let input = |scouting| SpecialsInput {
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
            rules: formulas(),
            special_level: 0,
            ranged: 0,
            granted: &[],
            hero_level: None,
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
        assert_eq!(ranged_icon(&[Ability::new(3 /* bow */)]), 5);
        assert_eq!(ranged_icon(&[Ability::new(20 /* fire_breath */)]), 7);
        assert_eq!(ranged_icon(&[Ability::new(57 /* rock_throwing */)]), 6);
        assert_eq!(ranged_icon(&[Ability::new(29 /* ice_bolt */)]), 1);
    }

    #[test]
    fn a_heros_sheet_names_its_level_and_keeps_the_item_cards() -> Result<(), String> {
        let mut app = crate::view::App::for_test();
        app.select_first_stack();
        let at = app
            .game
            .units()
            .unit_position(app.selection[0])
            .ok_or("the stack has no position")?;
        let hero = app
            .game
            .spawn_hero(crate::view::HUMAN, at, 0)
            .map_err(|e| format!("{e:?}"))?;
        let sheet = app.overland_sheet(hero).ok_or("the hero has no sheet")?;
        assert!(sheet.hero.is_some());
        let items = sheet.specials.iter().filter(|s| s.item.is_some()).count();
        assert_eq!(items, 3, "the three item slots head the list");
        assert!(
            sheet
                .level
                .as_ref()
                .is_some_and(|l| l.name.starts_with("Hero")),
            "the level title is in the corner: {:?}",
            sheet.level
        );
        assert!(
            sheet.specials.iter().all(|s| !s.name.starts_with("Hero (")),
            "and not a line of the list: {:?}",
            names(&sheet.specials)
        );
        assert!(
            sheet.specials.iter().all(|s| s.card.is_none()),
            "empty slots have no card"
        );
        Ok(())
    }

    #[test]
    fn a_heros_sheet_lists_the_abilities_rolled_for_its_game() -> Result<(), String> {
        let mut app = crate::view::App::for_test();
        app.select_first_stack();
        let at = app
            .game
            .units()
            .unit_position(app.selection[0])
            .ok_or("the stack has no position")?;
        // Hero type 0 (Brax) has Constitution as a fixed ability; at level 0 it is worth +1.
        let hero = app
            .game
            .spawn_hero(crate::view::HUMAN, at, 0)
            .map_err(|e| format!("{e:?}"))?;
        let sheet = app.overland_sheet(hero).ok_or("the hero has no sheet")?;
        let all = names(&sheet.specials);
        assert!(
            all.iter()
                .any(|n| n.starts_with("Constitution") && n.ends_with("+1")),
            "{all:?}"
        );
        Ok(())
    }

    #[test]
    fn a_hero_type_gets_a_portrait_on_the_sheets_of_offers_and_battles() -> Result<(), String> {
        let app = crate::view::App::for_test();
        let rules = app.game.content().rules.clone();
        let template = rules.hero_templates.first().ok_or("no hero type")?;
        let def = rules.units.get(template.unit);
        let view = app
            .hero_portrait_view(def, "Rolf")
            .ok_or("a hero type has a portrait")?;
        assert_eq!(view.name, "Rolf");
        let plain = rules
            .units
            .iter()
            .find(|(_, d)| d.hero.is_none())
            .map(|(_, d)| d)
            .ok_or("no plain unit")?;
        assert!(app.hero_portrait_view(plain, "").is_none());
        Ok(())
    }

    #[test]
    fn a_hero_in_a_battle_shows_its_own_name_and_the_sheet_is_an_overlay() -> Result<(), String> {
        let mut app = crate::view::App::for_test();
        app.select_first_stack();
        let at = app
            .game
            .units()
            .unit_position(app.selection[0])
            .ok_or("the stack has no position")?;
        let hero = app
            .game
            .spawn_hero(crate::view::HUMAN, at, 0)
            .map_err(|e| format!("{e:?}"))?;
        let name = app.game.units().hero_name(hero).ok_or("no hero name")?;
        app.open_test_battle();
        // Renaming afterwards changes nothing in the battle: the name was stored at the start.
        let battle = app.game.battle().cloned().ok_or("no battle")?;
        let id = battle
            .units
            .iter()
            .map(|u| u.id)
            .find(|&id| app.game.battle_unit_source(id) == Some(hero))
            .ok_or("the hero is not in the battle")?;
        let over = app
            .battle_sheet_over(&battle, id)
            .ok_or("the hero has no battle sheet")?;
        assert!(over.over && !over.closed);
        let items = over.sheet.specials.iter().filter(|l| l.item.is_some());
        assert_eq!(
            items.count(),
            3,
            "the item slots head the list, as overland"
        );
        let view = over.sheet.hero.ok_or("a hero sheet has a portrait")?;
        assert_eq!(view.name, name);
        Ok(())
    }
}
