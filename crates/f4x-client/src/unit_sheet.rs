//! The unit statistics sheet (task 140): what the original's `Unit_Statistics_Popup` shows about
//! one unit, built from the game (this file) and drawn by `unit_screen.rs`.
//!
//! The layout, the icon rows and the order of the abilities list follow ReMoM (`UnitStat.c`
//! `Thing_View_Draw`, `UV_Draw_Stat_Icons`, `UV_Draw_Hits_Icons`; `UnitView.c`
//! `USW_Build_Effect_List`). The numbers in the tables below are picture indexes into the
//! original's `special.lbx` (and `special2.lbx` from 111 on), not art.

use f4x_core::battle::{Battle, BattleUnit, RangedKind};
use f4x_core::rules::{MovementType, Stat, UnitKind};

use crate::view::App;

/// Where a sheet's unit lives, for Dismiss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    Overland(hecs::Entity),
    Battle(usize),
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

/// One line of the abilities list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Special {
    pub name: String,
    /// Picture index (see [`special_picture`]); `None` draws no icon.
    pub icon: Option<usize>,
    /// The lasting spell this line is, when it is one.
    pub spell: Option<f4x_core::rules::SpellId>,
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
    pub upkeep: Option<[i32; 3]>,
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
pub(crate) fn ranged_icon(abilities: &[String]) -> usize {
    let has = |a: &str| abilities.iter().any(|x| x == a);
    if has("fire_breath")
        || has("lightning_breath")
        || has("stoning_gaze")
        || has("death_gaze")
        || has("multi_gaze")
    {
        7
    } else if has("thrown") {
        8
    } else if has("rock_throwing") || has("sling") {
        6
    } else if has("bow") {
        5
    } else if [
        "fireball_attack",
        "lightning_bolt",
        "ice_bolt",
        "shaman_bolt",
        "sorcery_bolt",
        "death_bolt",
        "drow_bolt",
        "sprite_bolt",
        "nature_bolt",
        "illusion_attack",
    ]
    .iter()
    .any(|a| has(a))
    {
        1
    } else {
        5
    }
}

/// The quiver line ("Arrows x8", "Spells x3") for a ranged unit that is not a spell caster.
pub(crate) fn quiver_name(abilities: &[String], ammo: i32) -> Option<String> {
    let has = |a: &str| abilities.iter().any(|x| x == a);
    let caster = has("spellcaster") || has("spellcaster_major");
    let kind = if has("rock_throwing") {
        "Rock"
    } else if has("bow") {
        "Arrow"
    } else if has("sling") {
        "Bullet"
    } else if !caster && ranged_icon(abilities) == 1 {
        "Spell"
    } else {
        return None;
    };
    let plural = if ammo > 1 { "s" } else { "" };
    Some(format!("{kind}{plural} x{ammo}"))
}

/// Abilities the list shows, in the original's order, with their picture.
const LISTED: &[(&str, usize)] = &[
    // Movement and special unit abilities (`USW_Abilities`).
    ("teleport", 0),
    ("forester", 1),
    ("mountaineer", 2),
    ("pathfinding", 20),
    ("large_shield", 14),
    ("wall_crusher", 15),
    ("healer", 16),
    ("create_outpost", 17),
    ("invisibility", 18),
    ("create_undead", 19),
    ("long_range", 129),
    ("meld", 40),
    ("non_corporeal", 22),
    ("wind_walking", 23),
    ("regeneration", 24),
    ("purify", 25),
    ("negate_first_strike", 48),
    ("planar_travel", 91),
    ("water_walking", 71),
    // Summoning and spell casting.
    ("summon_demon", 139),
    ("spellcaster", 55),
    ("spellcaster_major", 55),
    // Immunities and bonuses (`USW_Attributes_1` and `_2`).
    ("lucky", 127),
    ("poison_immunity", 5),
    ("fire_immunity", 6),
    ("stoning_immunity", 7),
    ("weapon_immunity", 8),
    ("missile_immunity", 9),
    ("illusion_immunity", 10),
    ("cold_immunity", 11),
    ("magic_immunity", 12),
    ("death_immunity", 49),
    ("casts_healing", 38),
    ("casts_fireball", 39),
    ("casts_doom_bolt", 41),
    ("immolation", 32),
    ("cause_fear", 132),
    ("casts_web", 131),
    ("resist_all", 33),
    ("holy_bonus", 34),
    // Attack flags (`USW_ATK_Flags`) and the special ranged attacks (`USW_SR_Attacks`).
    ("armor_piercing", 28),
    ("poison_touch", 29),
    ("life_steal", 30),
    ("illusion_attack", 116),
    ("stoning_touch", 35),
    ("dispel_evil", 142),
    ("first_strike", 133),
    ("thrown", 130),
    ("fire_breath", 138),
    ("lightning_breath", 137),
    ("stoning_gaze", 26),
    ("death_gaze", 135),
    ("multi_gaze", 134),
];

/// The picture of a lasting spell on a unit (`_unit_enchantment_data`, `USW_Combat_FX`).
pub(crate) fn enchantment_icon(spell: &str) -> Option<usize> {
    Some(match spell {
        "immolation" => 117,
        "guardian_wind" => 118,
        "berserk" => 128,
        "cloak_of_fear" => 97,
        "black_channels" => 67,
        "wraith_form" => 68,
        "regeneration" => 69,
        "path_finding" => 70,
        "water_walking" => 71,
        "resist_elements" => 72,
        "elemental_armor" => 73,
        "stone_skin" => 74,
        "iron_skin" => 75,
        "endurance" => 76,
        "spell_lock" => 119,
        "invisibility" => 78,
        "wind_walking" => 79,
        "flight" => 80,
        "resist_magic" => 81,
        "magic_immunity" => 82,
        "flame_blade" => 83,
        "eldritch_weapon" => 84,
        "true_sight" => 85,
        "holy_weapon" => 86,
        "heroism" => 87,
        "bless" => 88,
        "lionheart" => 89,
        "giant_strength" => 65,
        "planar_travel" => 91,
        "holy_armor" => 92,
        "righteousness" => 93,
        "invulnerability" => 94,
        // Spells of the battle.
        "vertigo" => 101,
        "confusion" => 111,
        "mind_storm" => 113,
        "shatter" => 95,
        "weakness" => 96,
        "black_sleep" => 114,
        "warp_creature" => 122,
        "mana_leak" => 121,
        "haste" => 77,
        "web" => 99,
        "creature_binding" => 100,
        "possession" => 98,
        _ => return None,
    })
}

/// What the abilities list is built from.
pub(crate) struct SpecialsInput<'a> {
    /// The level's name, the experience points and the level (0-based), for normal units.
    pub level: Option<(&'a str, u32, usize)>,
    pub abilities: &'a [String],
    pub ammo: i32,
    pub to_hit: i32,
    pub construction: bool,
    /// The scouting level: how far the unit sees, when that is more than one square.
    pub scouting: i32,
    /// The mana of a spell caster unit.
    pub mana: i32,
    pub undead: bool,
    /// Display name of an ability id.
    pub ability_name: &'a dyn Fn(&str) -> String,
    /// Lasting spells on the unit: spell, id, name.
    pub enchantments: Vec<(f4x_core::rules::SpellId, String, String)>,
}

/// The abilities list in the original's order: level, quiver, to hit, construction, the listed
/// abilities, then the lasting spells.
pub(crate) fn specials(input: &SpecialsInput) -> Vec<Special> {
    let mut out = Vec::new();
    let plain = |name: String, icon: Option<usize>| Special {
        name,
        icon,
        spell: None,
    };
    if let Some((name, xp, level)) = input.level {
        out.push(plain(format!("{name} ({xp} ep)"), Some(102 + level.min(5))));
    }
    if let Some(name) = quiver_name(input.abilities, input.ammo) {
        out.push(plain(name, Some(136)));
    }
    if input.to_hit != 0 {
        let (icon, sign) = if input.to_hit > 0 {
            (125, "+")
        } else {
            (126, "")
        };
        out.push(plain(format!("{sign}{} To Hit", input.to_hit), Some(icon)));
    }
    if input.construction {
        out.push(plain("Construction".to_string(), Some(36)));
    }
    if input.scouting > 1 {
        out.push(plain(format!("Scouting {}", input.scouting), Some(37)));
    }
    let has = |a: &str| input.abilities.iter().any(|x| x == a);
    for &(id, icon) in LISTED {
        if !has(id) {
            continue;
        }
        // Pathfinding replaces the forester and mountaineer lines it contains.
        if has("pathfinding") && (id == "forester" || id == "mountaineer") {
            continue;
        }
        let mut name = (input.ability_name)(id);
        if matches!(id, "spellcaster" | "spellcaster_major") && input.mana > 0 {
            name = format!("Caster {} mp", input.mana);
        }
        out.push(plain(name, Some(icon)));
    }
    if input.undead {
        out.push(plain("Undead".to_string(), Some(120)));
    }
    for (spell, id, name) in &input.enchantments {
        out.push(Special {
            name: name.clone(),
            icon: enchantment_icon(id),
            spell: Some(*spell),
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
    const TABLE: [&[(i32, i32)]; 8] = [
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
        let level_path = def
            .level_path
            .as_deref()
            .and_then(|p| content.rules.level_paths.by_name(p));
        let level = level_path
            .filter(|_| def.kind == UnitKind::Normal && !game.units().unit_is_undead(unit))
            .and_then(|p| {
                let i = units.unit_level(unit);
                p.level(i)
                    .map(|(name, _)| (name, units.unit_experience(unit), i))
            });
        let ability_name = |id: &str| {
            content
                .rules
                .abilities
                .by_name(id)
                .map_or_else(|| id.to_string(), |a| a.name.clone())
        };
        let enchantments = units
            .unit_enchantments(unit)
            .into_iter()
            .map(|(spell, _)| {
                let def = content.rules.spells.get(spell);
                (spell, def.id.clone(), def.name.clone())
            })
            .collect();
        let mana = if abilities.iter().any(|a| a == "spellcaster_major") {
            40
        } else if abilities.iter().any(|a| a == "spellcaster") {
            20
        } else {
            0
        };
        let specials = specials(&SpecialsInput {
            level,
            abilities: &abilities,
            ammo: stat(Stat::Ammo),
            to_hit: stat(Stat::ToHit),
            construction: stat(Stat::Construction) > 0,
            scouting: stat(Stat::Sight),
            mana,
            undead: units.unit_is_undead(unit),
            ability_name: &ability_name,
            enchantments,
        });
        let art = def.art.clone();
        Some(Sheet {
            source: Source::Overland(unit),
            race: self.sheet_race(def),
            name: def.name.clone(),
            unit_type: art
                .as_ref()
                .and_then(|(lbx, entry)| crate::art::unit_type_number(lbx, *entry as usize)),
            banner: self.banner_index(owner.0),
            figures: figures as u32,
            water: matches!(def.movement, MovementType::Sailing | MovementType::Swimming)
                || stat(Stat::Transport) > 0,
            moves: stat(Stat::Moves).max(0) as u32,
            move_icon: move_icon_of(def.movement),
            upkeep: Some([
                units.unit_gold_upkeep(unit),
                def.upkeep.mana as i32,
                units.unit_food_upkeep(unit),
            ]),
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
            can_dismiss: owner == crate::view::HUMAN && def.kind != UnitKind::Hero,
        })
    }

    /// The race prefix of a race-specific unit's name ("High Elf" in "High Elf Magicians"):
    /// units of a race show it, creatures and heroes do not.
    fn sheet_race(&self, def: &f4x_core::rules::UnitDef) -> Option<String> {
        let race = def.race.as_deref()?;
        let races = &self.game.content().rules.races;
        races.by_name(race).map(|r| r.name.clone())
    }

    /// The sheet of a unit in the battle on screen.
    pub(crate) fn battle_sheet(&self, battle: &Battle, id: usize) -> Option<Sheet> {
        let u: &BattleUnit = battle.unit(id)?;
        let content = self.game.content();
        let def = content
            .rules
            .units
            .defs()
            .iter()
            .find(|d| d.id == u.unit_id)?;
        let base = |s: Stat| def.stats.get(&s).copied().unwrap_or(0);
        let ability_name = |id: &str| {
            content
                .rules
                .abilities
                .by_name(id)
                .map_or_else(|| id.to_string(), |a| a.name.clone())
        };
        let enchantments = u
            .enchantments
            .iter()
            .filter_map(|id| {
                let spell = content.rules.spells.id_of(id)?;
                Some((
                    spell,
                    id.clone(),
                    content.rules.spells.get(spell).name.clone(),
                ))
            })
            .chain(u.combat_spells.iter().map(|&(spell, _)| {
                let s = content.rules.spells.get(spell);
                (spell, s.id.clone(), s.name.clone())
            }))
            .collect();
        let specials = specials(&SpecialsInput {
            level: None,
            abilities: &u.abilities,
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
            ability_name: &ability_name,
            enchantments,
        });
        let owner = battle.player_of(u.side).0;
        let art = def.art.clone();
        Some(Sheet {
            source: Source::Battle(id),
            race: self.sheet_race(def),
            name: def.name.clone(),
            unit_type: art
                .as_ref()
                .and_then(|(lbx, entry)| crate::art::unit_type_number(lbx, *entry as usize)),
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let abilities: Vec<String> = ["bow", "first_strike", "poison_immunity"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let name = |id: &str| id.to_string();
        let list = specials(&SpecialsInput {
            level: Some(("Veteran", 65, 2)),
            abilities: &abilities,
            ammo: 8,
            to_hit: 1,
            construction: false,
            scouting: 0,
            mana: 0,
            undead: false,
            ability_name: &name,
            enchantments: Vec::new(),
        });
        assert_eq!(
            names(&list),
            [
                "Veteran (65 ep)",
                "Arrows x8",
                "+1 To Hit",
                "poison_immunity",
                "first_strike"
            ]
        );
        assert_eq!(list[0].icon, Some(104));
    }

    #[test]
    fn pathfinding_stands_in_for_forester_and_mountaineer() {
        let abilities: Vec<String> = ["forester", "mountaineer", "pathfinding"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let name = |id: &str| id.to_string();
        let list = specials(&SpecialsInput {
            level: None,
            abilities: &abilities,
            ammo: 0,
            to_hit: 0,
            construction: false,
            scouting: 0,
            mana: 0,
            undead: false,
            ability_name: &name,
            enchantments: Vec::new(),
        });
        assert_eq!(names(&list), ["pathfinding"]);
    }

    #[test]
    fn spell_casters_show_their_mana_and_no_quiver() {
        let abilities: Vec<String> = ["spellcaster", "fireball_attack"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let name = |id: &str| id.to_string();
        let list = specials(&SpecialsInput {
            level: None,
            abilities: &abilities,
            ammo: 4,
            to_hit: 0,
            construction: false,
            scouting: 0,
            mana: 20,
            undead: false,
            ability_name: &name,
            enchantments: Vec::new(),
        });
        assert_eq!(names(&list), ["Caster 20 mp"]);
    }

    #[test]
    fn a_scout_lists_its_scouting_level() {
        let name = |id: &str| id.to_string();
        let input = |scouting| SpecialsInput {
            level: None,
            abilities: &[],
            ammo: 0,
            to_hit: 0,
            construction: false,
            scouting,
            mana: 0,
            undead: false,
            ability_name: &name,
            enchantments: Vec::new(),
        };
        assert!(specials(&input(1)).is_empty());
        assert_eq!(names(&specials(&input(3))), ["Scouting 3"]);
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
        let s = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(ranged_icon(&s(&["bow"])), 5);
        assert_eq!(ranged_icon(&s(&["fire_breath"])), 7);
        assert_eq!(ranged_icon(&s(&["rock_throwing"])), 6);
        assert_eq!(ranged_icon(&s(&["ice_bolt"])), 1);
    }
}
