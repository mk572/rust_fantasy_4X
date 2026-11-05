//! Loading and cross-reference validation of the rules definitions.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use f4x_core::feature::def::Features;
use f4x_core::rules::effects::{Condition, Effect, EffectRule, Trigger, CUSTOM_HOOKS};
use f4x_core::rules::{
    AbilityDef, AbilityId, BuildingDef, Def, Duration, HeroTemplate, ItemDef, ItemPowerDef,
    LevelPathDef, RaceDef, RealmDef, Registry, RetortDef, Rules, RulesConfig, SpellDef, Stat,
    UnitDef, UnitKind, WizardDef,
};

use crate::{id_problem, ContentError};

/// The first of `dirs` that has the file `name`.
fn find(dirs: &[&Path], name: &str) -> Option<std::path::PathBuf> {
    dirs.iter().map(|d| d.join(name)).find(|p| p.exists())
}

fn read(dirs: &[&Path], name: &str) -> Result<String, ContentError> {
    let path = find(dirs, name)
        .unwrap_or_else(|| dirs.first().map_or_else(|| name.into(), |d| d.join(name)));
    std::fs::read_to_string(&path).map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))
}

fn parse<T: serde::de::DeserializeOwned>(dirs: &[&Path], name: &str) -> Result<T, ContentError> {
    ron::from_str(&read(dirs, name)?).map_err(|e| ContentError::Parse(format!("{name}: {e}")))
}

/// The race's city names, kept apart from the race table so a local file can replace them.
#[derive(serde::Deserialize)]
struct CityNames {
    race: String,
    names: Vec<String>,
}

/// Fill in `city_names` from `city_names.ron`, if any directory has it. A race the file does
/// not mention keeps the names in the race table.
fn apply_city_names(dirs: &[&Path], races: &mut [RaceDef]) -> Result<(), ContentError> {
    if find(dirs, "city_names.ron").is_none() {
        return Ok(());
    }
    let lists = parse::<Vec<CityNames>>(dirs, "city_names.ron")?;
    for list in lists {
        let Some(race) = races.iter_mut().find(|r| r.id == list.race) else {
            return Err(ContentError::Invalid(vec![format!(
                "city_names.ron names unknown race {:?}",
                list.race
            )]));
        };
        race.city_names = list.names;
    }
    Ok(())
}

/// Fill in each hero template's `names` from `text/<lang>/name/hero.ron` (by the hero unit's
/// number, five names for each; `lang` over the fallback language). A hero without names, and
/// names for a number that is no hero, are problems.
fn apply_hero_names(
    (dirs, lang): (&[&Path], &str),
    templates: &mut [HeroTemplate],
    problems: &mut Vec<String>,
) -> Result<(), ContentError> {
    let names = name_table::<Vec<String>>(dirs, lang, "hero")?;
    for t in templates.iter_mut() {
        match names.get(&t.unit.num()) {
            Some(list) => t.names = list.clone(),
            None => problems.push(format!(
                "hero {} has no names in text/{lang}/name/hero.ron",
                t.unit.num()
            )),
        }
    }
    let known: std::collections::BTreeSet<u16> = templates.iter().map(|t| t.unit.num()).collect();
    for n in names.keys().filter(|n| !known.contains(n)) {
        problems.push(format!(
            "text/{lang}/name/hero.ron names the unknown hero {n}"
        ));
    }
    Ok(())
}

/// The names of one table in `lang`, by definition number: `text/<lang>/name/<file>.ron` of the
/// first directory that has it, over the same file of the fallback language.
fn name_table<V: serde::de::DeserializeOwned>(
    dirs: &[&Path],
    lang: &str,
    file: &str,
) -> Result<std::collections::BTreeMap<u16, V>, ContentError> {
    let mut all = std::collections::BTreeMap::new();
    for l in [f4x_core::text::FALLBACK_LANGUAGE, lang] {
        let name = format!("text/{l}/name/{file}.ron");
        if find(dirs, &name).is_some() {
            all.extend(parse::<std::collections::BTreeMap<u16, V>>(dirs, &name)?);
        }
    }
    Ok(all)
}

/// The names for rival wizards: `text/<lang>/name/rival.ron` of the first directory that has
/// it, else the fallback language's (one list for each rival slot).
fn rival_names(
    dirs: &[&Path],
    lang: &str,
) -> Result<Vec<Vec<(String, f4x_core::text::Gender)>>, ContentError> {
    let own = format!("text/{lang}/name/rival.ron");
    let name = if find(dirs, &own).is_some() {
        own
    } else {
        format!("text/{}/name/rival.ron", f4x_core::text::FALLBACK_LANGUAGE)
    };
    parse(dirs, &name)
}

/// Load and validate every rules file. Each file comes from the first of `dirs` that has it,
/// so a local directory can replace single files of the committed one.
pub fn load_rules(dirs: &[&Path], lang: &str, features: &Features) -> Result<Rules, ContentError> {
    let dir = dirs;
    let mut races = parse::<Vec<RaceDef>>(dir, "races.ron")?;
    apply_city_names(dir, &mut races)?;
    let mut hero_templates = if find(dir, "heroes.ron").is_some() {
        parse::<Vec<HeroTemplate>>(dir, "heroes.ron")?
    } else {
        Vec::new()
    };
    let wizards = if find(dir, "wizards.ron").is_some() {
        parse::<Vec<WizardDef>>(dir, "wizards.ron")?
    } else {
        Vec::new()
    };
    let units = parse::<Vec<UnitDef>>(dir, "units.ron")?;
    let abilities = parse::<Vec<AbilityDef>>(dir, "abilities.ron")?;
    let buildings = parse::<Vec<BuildingDef>>(dir, "buildings.ron")?;
    let realms = parse::<Vec<RealmDef>>(dir, "realms.ron")?;
    let retorts = parse::<Vec<RetortDef>>(dir, "retorts.ron")?;
    let spells = parse::<Vec<SpellDef>>(dir, "spells.ron")?;
    let items = parse::<Vec<ItemDef>>(dir, "items.ron")?;
    let item_powers = parse::<Vec<ItemPowerDef>>(dir, "item_powers.ron")?;
    let mut problems = Vec::new();
    let at = (dir, lang);
    apply_hero_names(at, &mut hero_templates, &mut problems)?;
    let level_paths = parse::<Vec<LevelPathDef>>(dir, "level_paths.ron")?;
    if !problems.is_empty() {
        return Err(ContentError::Invalid(problems));
    }
    let config = parse::<RulesConfig>(dir, "rules.ron")?;
    let abilities = Registry::new(abilities);
    let rules = Rules {
        config,
        races: Registry::new(races),
        units: Registry::new(units),
        abilities,
        buildings: Registry::new(buildings),
        realms: Registry::new(realms),
        spells: Registry::new(spells),
        items: Registry::new(items),
        item_powers: Registry::new(item_powers),
        level_paths: Registry::new(level_paths),
        // Ready-made wizards are optional: a rule set without them just has no presets.
        wizards: Registry::new(wizards),
        retorts: Registry::new(retorts),
        // The starting records of the hero types, also optional.
        hero_templates,
        // Fallback names for rival wizards, also optional.
        // The lair rules, also optional: a rule set without the file uses the original's.
        lairs: if find(dir, "lairs.ron").is_some() {
            parse::<f4x_core::rules::lair::LairRules>(dir, "lairs.ron")?
        } else {
            f4x_core::rules::lair::LairRules::default()
        },
        wizard_names: rival_names(dir, lang)?,
    };
    let problems = validate_rules(&rules, features);
    if problems.is_empty() {
        Ok(rules)
    } else {
        Err(ContentError::Invalid(problems))
    }
}

fn check_ids<T: Def>(kind: &str, reg: &Registry<T>, problems: &mut Vec<String>) {
    let mut seen = HashSet::new();
    for d in reg.defs() {
        problems.extend(id_problem(kind, d.id()));
        if !seen.insert(d.id()) {
            problems.push(format!("duplicate {kind} id {:?}", d.id()));
        }
    }
}

/// The lair rules: real realms, rolls that can be made and a table that is not empty.
fn check_lairs(r: &Rules, p: &mut Vec<String>) {
    let l = &r.lairs;
    let realm = |what: &str, num: u16, p: &mut Vec<String>| {
        if r.realms.id_numbered(num).is_none() {
            p.push(format!("lairs.ron: {what} names unknown realm {num}"));
        }
    };
    for (what, list) in [
        ("tower_realms", &l.tower_realms),
        ("holy_realms", &l.holy_realms),
        ("wild_realms", &l.wild_realms),
    ] {
        if list.iter().map(|(_, w)| w).sum::<u32>() == 0 {
            p.push(format!("lairs.ron: {what} has no weight"));
        }
        for &(num, _) in list {
            realm(what, num, p);
        }
    }
    for &num in l.node_own_realms.iter().chain([&l.node_other]) {
        realm("the node realms", num, p);
    }
    if l.hoard.iter().map(|(_, w)| w).sum::<u32>() == 0 {
        p.push("lairs.ron: the hoard table has no weight".into());
    }
    let rolls = [
        ("tower_budget", l.tower_budget.rolls),
        (
            "strong_budget",
            l.strong_budget[0].rolls.min(l.strong_budget[1].rolls),
        ),
        (
            "weak_budget",
            l.weak_budget[0].rolls.min(l.weak_budget[1].rolls),
        ),
        (
            "plane_loot",
            l.plane_loot[0].rolls.min(l.plane_loot[1].rolls),
        ),
        ("node_rolls", l.node_rolls),
        ("coin_rolls", l.coin_rolls),
        ("item_rolls", l.item_rolls),
        ("spell_rarities", l.spell_rarities),
        ("double_chance", l.double_chance),
        ("first_spread", l.first_spread),
    ];
    for (what, n) in rolls {
        if n < 1 {
            p.push(format!("lairs.ron: {what} must roll at least 1"));
        }
    }
    if l.second_spread - l.max_first_guards < 1 {
        p.push("lairs.ron: second_spread must exceed max_first_guards".into());
    }
    if l.max_first_guards > l.max_guards as i32 {
        p.push("lairs.ron: max_first_guards is over max_guards".into());
    }
    if !(1..=4).contains(&l.spell_total) {
        p.push("lairs.ron: spell_total must be 1 to 4 (a spell's rarity)".into());
    }
}

/// The hero templates: one for each hero unit, no more, with real abilities and spells.
fn check_hero_templates(r: &Rules, p: &mut Vec<String>) {
    if r.hero_templates.is_empty() {
        return;
    }
    let mut seen = HashSet::new();
    for (row, t) in r.hero_templates.iter().enumerate() {
        let who = format!("hero template for unit {}", t.unit.num());
        match r.units.try_get(t.unit) {
            Some(u) if u.kind == UnitKind::Hero => {
                // The hero type is the number of the hero's unit, so the row is that number.
                if usize::from(u.num) != row {
                    p.push(format!(
                        "{who} is row {row} but its unit is numbered {}",
                        u.num
                    ));
                }
            }
            Some(_) => p.push(format!("{who} is not a Hero unit")),
            None => p.push(format!("{who} names an unknown unit")),
        }
        if !seen.insert(t.unit) {
            p.push(format!("{who} appears twice"));
        }
        for &a in &t.abilities {
            if !f4x_core::hero::HeroAbilities::is_hero_ability(a) {
                p.push(format!("{who} has unknown hero ability {}", a.num()));
            }
        }
        for &s in &t.spells {
            if r.spells.try_get(s).is_none() {
                p.push(format!("{who} has unknown spell {}", s.num()));
            }
        }
        if t.spells.len() > 4 {
            p.push(format!("{who} has more than four innate spells"));
        }
    }
    for u in r.units.defs().iter().filter(|u| u.kind == UnitKind::Hero) {
        if !seen.contains(&f4x_core::rules::UnitId::new(u.num)) {
            p.push(format!("hero {:?} has no hero template", u.id));
        }
    }
}

fn check_items(r: &Rules, p: &mut Vec<String>) {
    for power in r.item_powers.defs() {
        let who = format!("item power {:?}", power.id);
        if let Some(realm) = power.realm {
            if r.realms.try_get(realm).is_none() {
                p.push(format!("{who} has unknown realm {}", realm.num()));
            }
        }
        if let Some(spell) = power.spell {
            if r.spells.try_get(spell).is_none() {
                p.push(format!("{who} has unknown spell {}", spell.num()));
            }
        }
        if let Some(attack) = power.attack {
            if r.abilities.try_get(attack).is_none() {
                p.push(format!("{who} has unknown attack ability {}", attack.num()));
            }
        }
        if power.kinds.is_empty() {
            p.push(format!("{who} fits no item type"));
        }
    }
    for item in r.items.defs() {
        let who = format!("item {:?}", item.id);
        if item.kind.slot() != item.slot {
            p.push(format!(
                "{who}: a {:?} does not go in the {:?} slot",
                item.kind, item.slot
            ));
        }
        if item.effects.is_empty()
            && item.powers.is_empty()
            && item.charges.is_none()
            && item.spell_skill == 0
            && item.spell_save == 0
        {
            p.push(format!("{who} has no effects"));
        }
        for &power in &item.powers {
            if r.item_powers.try_get(power).is_none() {
                p.push(format!("{who} has unknown power {}", power.num()));
            }
        }
        if let Some((spell, count)) = item.charges {
            if r.spells.try_get(spell).is_none() {
                p.push(format!("{who} has unknown spell {}", spell.num()));
            }
            if count == 0 {
                p.push(format!("{who} has a spell with no charges"));
            }
        }
    }
}

/// Returns a human-readable list of problems; empty means valid.
pub fn validate_rules(r: &Rules, features: &Features) -> Vec<String> {
    let mut p = Vec::new();
    check_lairs(r, &mut p);
    check_ids("race", &r.races, &mut p);
    check_ids("unit", &r.units, &mut p);
    check_ids("ability", &r.abilities, &mut p);
    check_ids("building", &r.buildings, &mut p);
    check_ids("realm", &r.realms, &mut p);
    check_ids("spell", &r.spells, &mut p);
    check_ids("item", &r.items, &mut p);
    check_ids("item power", &r.item_powers, &mut p);
    check_ids("level path", &r.level_paths, &mut p);
    check_ids("wizard", &r.wizards, &mut p);
    check_ids("retort", &r.retorts, &mut p);

    check_races(r, &mut p);
    check_units(r, &mut p);
    check_buildings(r, &mut p);
    check_spells(r, &mut p);
    check_level_paths(r, &mut p);
    check_wizards(r, &mut p);
    check_items(r, &mut p);
    check_hero_templates(r, &mut p);

    // Effect and condition references, for everything that carries effect rules.
    let mut carriers: Vec<(String, &[EffectRule])> = Vec::new();
    carriers.extend(
        r.races
            .defs()
            .iter()
            .map(|d| (format!("race {:?}", d.id), &d.effects[..])),
    );
    carriers.extend(
        r.abilities
            .defs()
            .iter()
            .map(|d| (format!("ability {:?}", d.id), &d.effects[..])),
    );
    carriers.extend(
        r.buildings
            .defs()
            .iter()
            .map(|d| (format!("building {:?}", d.id), &d.effects[..])),
    );
    carriers.extend(
        r.spells
            .defs()
            .iter()
            .map(|d| (format!("spell {:?}", d.id), &d.effects[..])),
    );
    carriers.extend(
        r.items
            .defs()
            .iter()
            .map(|d| (format!("item {:?}", d.id), &d.effects[..])),
    );
    for path in r.level_paths.defs() {
        for (i, level) in path.levels.iter().enumerate() {
            carriers.push((
                format!("level path {:?} level {i}", path.id),
                &level.effects[..],
            ));
        }
    }
    for spell in r.spells.defs() {
        let special = spell
            .effects
            .iter()
            .any(|e| matches!(e.effect, Effect::Special));
        if special && !f4x_core::rules::effects::SPECIAL_SPELLS.contains(&spell.num) {
            p.push(format!(
                "spell {:?} has Special code but no Rust code is written for its number",
                spell.id
            ));
        }
    }
    for (owner, rules) in carriers {
        for rule in rules {
            check_rule(r, features, &owner, rule, &mut p);
        }
    }
    p
}

fn check_races(r: &Rules, p: &mut Vec<String>) {
    for race in r.races.defs() {
        let this = f4x_core::rules::RaceId::new(race.num);
        if race.units.is_empty() {
            p.push(format!("race {:?} has no units", race.id));
        }
        for &u in &race.units {
            match r.units.try_get(u) {
                None => p.push(format!("race {:?} lists unknown unit {}", race.id, u.num())),
                Some(unit) if unit.race != Some(this) => {
                    p.push(format!(
                        "race {:?} lists unit {:?}, which belongs to another race",
                        race.id, unit.id
                    ));
                }
                Some(_) => {}
            }
        }
        for &b in &race.buildings {
            match r.buildings.try_get(b) {
                None => p.push(format!(
                    "race {:?} lists unknown building {}",
                    race.id,
                    b.num()
                )),
                Some(bd) if !bd.buildable_by(this) => {
                    p.push(format!(
                        "race {:?} lists building {:?}, which its races list excludes",
                        race.id, bd.id
                    ));
                }
                Some(_) => {}
            }
        }
        if race.start_army.len() != 2 || race.start_army.iter().any(|u| !race.units.contains(u)) {
            p.push(format!(
                "race {:?} must start with two of its own units",
                race.id
            ));
        }
    }
}

fn check_units(r: &Rules, p: &mut Vec<String>) {
    for u in r.units.defs() {
        let who = format!("unit {:?}", u.id);
        for stat in [
            Stat::Melee,
            Stat::Defense,
            Stat::Resistance,
            Stat::MaxHp,
            Stat::Figures,
            Stat::Moves,
        ] {
            if !u.stats.contains_key(&stat) {
                p.push(format!("{who} is missing required stat {stat:?}"));
            }
        }
        for (stat, &v) in &u.stats {
            if !(0..=99).contains(&v) {
                p.push(format!("{who}: {stat:?} = {v} is outside 0..=99"));
            }
        }
        let stat = |s: Stat| u.stats.get(&s).copied().unwrap_or(0);
        for s in [Stat::MaxHp, Stat::Figures, Stat::Moves] {
            if stat(s) < 1 {
                p.push(format!("{who}: {s:?} must be at least 1"));
            }
        }
        if stat(Stat::Ranged) > 0 && stat(Stat::Ammo) < 1 {
            p.push(format!("{who} has a ranged attack but no ammo"));
        }
        if let Some(race) = u.race {
            if r.races.try_get(race).is_none() {
                p.push(format!("{who} has unknown race {}", race.num()));
            }
        }
        if let Some(realm) = u.realm {
            if r.realms.try_get(realm).is_none() {
                p.push(format!("{who} has unknown realm {}", realm.num()));
            }
        }
        for &a in &u.abilities {
            if r.abilities.try_get(a).is_none() {
                p.push(format!("{who} has unknown ability {}", a.num()));
            }
        }
        for &b in &u.requires {
            match r.buildings.try_get(b) {
                None => p.push(format!("{who} requires unknown building {}", b.num())),
                Some(bd) => {
                    if let Some(race) = u.race {
                        if !bd.races.is_empty() && !bd.races.contains(&race) {
                            p.push(format!(
                                "{who} requires building {:?}, which its race cannot build",
                                bd.id
                            ));
                        }
                    }
                }
            }
        }
        if let Some(path) = u.level_path {
            if r.level_paths.try_get(path).is_none() {
                p.push(format!("{who} has unknown level path {}", path.num()));
            }
        }
        match u.kind {
            UnitKind::Normal => {
                match &u.race {
                    None => p.push(format!("{who} is Normal and needs a race")),
                    Some(race) => {
                        let listed = r.races.try_get(*race).is_some_and(|rd| {
                            rd.units.contains(&f4x_core::rules::UnitId::new(u.num))
                        });
                        if !listed {
                            p.push(format!(
                                "{who} is not in its race's unit list, so nothing can build it"
                            ));
                        }
                    }
                }
                if u.hero.is_some() {
                    p.push(format!("{who} is Normal but has hero data"));
                }
            }
            UnitKind::Hero => {
                if u.race.is_some() {
                    p.push(format!("{who} is a Hero and must not have a race"));
                }
                if u.hero.is_none() {
                    p.push(format!("{who} is a Hero and needs hero data"));
                }
            }
            UnitKind::Summoned => {
                if u.realm.is_none() {
                    p.push(format!("{who} is Summoned and needs a realm"));
                }
                if u.cost == 0 {
                    p.push(format!("{who} is Summoned and needs a mana cost"));
                }
            }
            UnitKind::Fantastic => {
                if u.race.is_some() || u.hero.is_some() {
                    p.push(format!(
                        "{who} is Fantastic and must have no race or hero data"
                    ));
                }
            }
        }
    }
}

fn check_buildings(r: &Rules, p: &mut Vec<String>) {
    for b in r.buildings.defs() {
        if b.cost == 0 {
            p.push(format!("building {:?} has no cost", b.id));
        }
        for &req in &b.requires {
            if r.buildings.try_get(req).is_none() {
                p.push(format!(
                    "building {:?} requires unknown building {}",
                    b.id,
                    req.num()
                ));
            }
        }
        for race in &b.races {
            if r.races.try_get(*race).is_none() {
                p.push(format!(
                    "building {:?} lists unknown race {}",
                    b.id,
                    race.num()
                ));
            }
        }
        if let Some(old) = b.replaces {
            if r.buildings.try_get(old).is_none() {
                p.push(format!(
                    "building {:?} replaces unknown building {}",
                    b.id,
                    old.num()
                ));
            }
        }
    }
    // Requirement cycles: depth-first search with white/grey/black colouring.
    let graph: BTreeMap<u16, Vec<u16>> = r
        .buildings
        .defs()
        .iter()
        .map(|b| (b.num, b.requires.iter().map(|id| id.num()).collect()))
        .collect();
    let mut state: BTreeMap<u16, u8> = BTreeMap::new(); // 1 = on stack, 2 = done
    fn visit(
        n: u16,
        g: &BTreeMap<u16, Vec<u16>>,
        state: &mut BTreeMap<u16, u8>,
        p: &mut Vec<String>,
    ) {
        match state.get(&n) {
            Some(2) => return,
            Some(_) => {
                p.push(format!("building requirement cycle involving building {n}"));
                return;
            }
            None => {}
        }
        state.insert(n, 1);
        for &m in g.get(&n).into_iter().flatten() {
            if g.contains_key(&m) {
                visit(m, g, state, p);
            }
        }
        state.insert(n, 2);
    }
    for &n in graph.keys() {
        visit(n, &graph, &mut state, p);
    }
}

fn check_spells(r: &Rules, p: &mut Vec<String>) {
    for s in r.spells.defs() {
        let who = format!("spell {:?}", s.id);
        if r.realms.try_get(s.realm).is_none() {
            p.push(format!("{who} has unknown realm {}", s.realm.num()));
        }
        if s.cost == 0 || s.research_cost == 0 {
            p.push(format!("{who} needs a nonzero cost and research_cost"));
        }
        if s.contexts.is_empty() {
            p.push(format!("{who} can be cast in no context"));
        }
        // A lasting spell may carry no effects of its own when the engine reads it by number
        // (Resist Elements adds to resistance against some realms only).
        if s.effects.is_empty() && s.duration == Duration::Instant {
            p.push(format!("{who} has no effects"));
        }
        for other in &s.not_with {
            if r.spells.try_get(*other).is_none() {
                p.push(format!(
                    "{who} is not cumulative with unknown spell {}",
                    other.num()
                ));
            }
        }
        let unknown_abilities = s
            .already_able
            .iter()
            .chain(s.save.iter().flat_map(|save| &save.immune))
            .filter(|a| r.abilities.try_get(**a).is_none());
        for a in unknown_abilities {
            p.push(format!("{who} names unknown ability {}", a.num()));
        }
        if s.duration == Duration::Instant {
            if s.upkeep != 0 {
                p.push(format!("{who} is instant but has upkeep"));
            }
            if s.effects.iter().any(|e| e.when == Trigger::Passive) {
                p.push(format!(
                    "{who} is instant but has Passive effects; use OnCast"
                ));
            }
        }
    }
}

fn check_wizards(r: &Rules, p: &mut Vec<String>) {
    for retort in r.retorts.defs() {
        for need in &retort.requires {
            if let f4x_core::rules::RetortNeed::Books(realm, _) = need {
                if r.realms.try_get(*realm).is_none() {
                    p.push(format!(
                        "retort {:?} needs books in unknown realm {}",
                        retort.id,
                        realm.num()
                    ));
                }
            }
        }
    }
    for realm in r.realms.defs() {
        for s in &realm.default_spells {
            if r.spells.try_get(*s).is_none() {
                p.push(format!(
                    "realm {:?} defaults to unknown spell {}",
                    realm.id,
                    s.num()
                ));
            }
        }
    }
    for w in r.wizards.defs() {
        let mut picks: u32 = 0;
        for (realm, n) in &w.books {
            if r.realms.try_get(*realm).is_none() {
                p.push(format!(
                    "wizard {:?} has books in unknown realm {}",
                    w.id,
                    realm.num()
                ));
            }
            picks += u32::from(*n);
        }
        for &retort in &w.retorts {
            match r.retorts.try_get(retort) {
                Some(def) => picks += u32::from(def.cost),
                None => p.push(format!(
                    "wizard {:?} has unknown retort {}",
                    w.id,
                    retort.num()
                )),
            }
        }
        let budget = u32::from(f4x_core::wizard::PICKS);
        if picks != budget {
            p.push(format!(
                "wizard {:?} spends {picks} picks, not {budget}",
                w.id
            ));
        }
    }
}

fn check_level_paths(r: &Rules, p: &mut Vec<String>) {
    for path in r.level_paths.defs() {
        let who = format!("level path {:?}", path.id);
        match path.levels.first() {
            None => p.push(format!("{who} has no levels")),
            Some(first) if first.xp != 0 => p.push(format!("{who}: first level must need 0 xp")),
            Some(_) => {}
        }
        if path.levels.windows(2).any(|w| w[1].xp <= w[0].xp) {
            p.push(format!("{who}: xp thresholds must strictly increase"));
        }
    }
}

fn check_rule(r: &Rules, features: &Features, owner: &str, rule: &EffectRule, p: &mut Vec<String>) {
    check_condition(r, owner, &rule.condition, p);
    let ability = |a: &AbilityId, p: &mut Vec<String>| {
        if r.abilities.try_get(*a).is_none() {
            p.push(format!("{owner}: unknown ability {}", a.num()));
        }
    };
    let realm = |m: &f4x_core::rules::RealmId, p: &mut Vec<String>| {
        if r.realms.try_get(*m).is_none() {
            p.push(format!("{owner}: unknown realm {}", m.num()));
        }
    };
    match &rule.effect {
        Effect::GrantAbility(a) | Effect::RemoveAbility(a) => ability(a, p),
        Effect::Summon { unit, count } => {
            if r.units.try_get(*unit).is_none() {
                p.push(format!("{owner}: summons unknown unit {}", unit.num()));
            }
            if *count == 0 {
                p.push(format!("{owner}: summon count must be at least 1"));
            }
        }
        Effect::SetFeature(f) => {
            if features.get(*f).is_none() {
                p.push(format!("{owner}: unknown feature {}", f.0));
            }
        }
        Effect::ApplyEnchantment(s) => match r.spells.try_get(*s) {
            None => p.push(format!("{owner}: applies unknown enchantment {}", s.num())),
            Some(sd) if sd.duration == Duration::Instant => {
                p.push(format!(
                    "{owner}: {:?} is an instant spell, not an enchantment",
                    sd.id
                ));
            }
            Some(_) => {}
        },
        Effect::Damage {
            amount,
            immune,
            realm: m,
            ..
        } => {
            immune.iter().for_each(|a| ability(a, p));
            m.iter().for_each(|m| realm(m, p));
            if *amount <= 0 {
                p.push(format!("{owner}: damage must be positive"));
            }
        }
        Effect::Slay {
            immune, realm: m, ..
        } => {
            immune.iter().for_each(|a| ability(a, p));
            realm(m, p);
        }
        Effect::Heal(n) if *n <= 0 => p.push(format!("{owner}: heal must be positive")),
        Effect::Dispel { strength: 0 } => {
            p.push(format!("{owner}: dispel strength must be positive"))
        }
        Effect::Custom(name) if !CUSTOM_HOOKS.contains(&name.as_str()) => {
            p.push(format!("{owner}: unknown custom hook {name:?}"));
        }
        _ => {}
    }
    if let f4x_core::rules::effects::TargetSel::UnitsInArea(0) = rule.target {
        p.push(format!("{owner}: area radius must be at least 1"));
    }
}

fn check_condition(r: &Rules, owner: &str, c: &Condition, p: &mut Vec<String>) {
    match c {
        Condition::HasAbility(a) if r.abilities.try_get(*a).is_none() => {
            p.push(format!(
                "{owner}: condition names unknown ability {}",
                a.num()
            ));
        }
        Condition::RaceIs(x) if r.races.try_get(*x).is_none() => {
            p.push(format!("{owner}: condition names unknown race {}", x.num()));
        }
        Condition::RealmIs(x) if r.realms.try_get(*x).is_none() => {
            p.push(format!(
                "{owner}: condition names unknown realm {}",
                x.num()
            ));
        }
        Condition::All(cs) | Condition::Any(cs) => {
            cs.iter().for_each(|c| check_condition(r, owner, c, p))
        }
        Condition::Not(c) => check_condition(r, owner, c, p),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::rules::stats::{compute, passive_modifiers};
    use f4x_core::rules::Source;
    use f4x_core::testdir::TestResult;

    #[test]
    fn the_committed_wizards_have_their_names_and_books() -> TestResult {
        let dir = crate::testutil::content_dir();
        let features = crate::parse_features(&std::fs::read_to_string(dir.join("features.ron"))?)?;
        let rules = load_rules(&[dir.as_path(), &client_dir()], "en", &features)?;
        let wizards = rules.wizards.defs();
        assert_eq!(wizards.len(), 14);
        let lo_pan = wizards
            .iter()
            .find(|w| w.id == "lo_pan")
            .ok_or("the value is missing")?;
        assert_eq!(lo_pan.books.iter().map(|b| u32::from(b.1)).sum::<u32>(), 9);
        Ok(())
    }

    fn content_dir() -> std::path::PathBuf {
        crate::testutil::content_dir()
    }

    fn client_dir() -> std::path::PathBuf {
        crate::testutil::client_dir()
    }

    fn features() -> Features {
        let src = std::fs::read_to_string(content_dir().join("features.ron"))
            .expect("content/features.ron exists");
        crate::parse_features(&src).expect("the shipped features parse")
    }

    /// The shipped files after applying `(file, from, to)` text replacements (for the files the
    /// tests edit as text: the lair table).
    fn rules_with(edits: &[(&str, &str, &str)]) -> Rules {
        fn shipped<T: serde::de::DeserializeOwned>(name: &str, edits: &[(&str, &str, &str)]) -> T {
            let mut s = std::fs::read_to_string(content_dir().join(name))
                .unwrap_or_else(|e| panic!("content/{name}: {e}"));
            for (f, from, to) in edits.iter().filter(|(f, ..)| *f == name) {
                assert!(s.contains(from), "{f} does not contain {from:?}");
                s = s.replace(from, to);
            }
            ron::from_str(&s).unwrap_or_else(|e| panic!("content/{name}: {e}"))
        }
        let races: Vec<f4x_core::rules::RaceDef> = shipped("races.ron", edits);
        let units: Vec<UnitDef> = shipped("units.ron", edits);
        let abilities: Vec<f4x_core::rules::AbilityDef> = shipped("abilities.ron", edits);
        let buildings: Vec<f4x_core::rules::BuildingDef> = shipped("buildings.ron", edits);
        let realms: Vec<RealmDef> = shipped("realms.ron", edits);
        let retorts: Vec<RetortDef> = shipped("retorts.ron", edits);
        let spells: Vec<SpellDef> = shipped("spells.ron", edits);
        let item_powers: Vec<ItemPowerDef> = shipped("item_powers.ron", edits);
        let level_paths: Vec<LevelPathDef> = shipped("level_paths.ron", edits);
        let wizards: Vec<f4x_core::rules::WizardDef> = Vec::new();
        let items: Vec<f4x_core::rules::ItemDef> = shipped("items.ron", edits);
        let hero_templates: Vec<f4x_core::rules::HeroTemplate> = Vec::new();
        let config: f4x_core::rules::RulesConfig = shipped("rules.ron", edits);
        Rules {
            config,
            races: Registry::new(races),
            units: Registry::new(units),
            abilities: Registry::new(abilities),
            buildings: Registry::new(buildings),
            realms: Registry::new(realms),
            retorts: Registry::new(retorts),
            spells: Registry::new(spells),
            items: Registry::new(items),
            item_powers: Registry::new(item_powers),
            level_paths: Registry::new(level_paths),
            wizards: Registry::new(wizards),
            hero_templates,
            wizard_names: Vec::new(),
            lairs: shipped("lairs.ron", edits),
        }
    }

    /// The shipped definitions, to change one by one.
    struct Lists {
        races: Vec<f4x_core::rules::RaceDef>,
        units: Vec<f4x_core::rules::UnitDef>,
        abilities: Vec<f4x_core::rules::AbilityDef>,
        buildings: Vec<f4x_core::rules::BuildingDef>,
        spells: Vec<f4x_core::rules::SpellDef>,
        items: Vec<f4x_core::rules::ItemDef>,
        item_powers: Vec<f4x_core::rules::ItemPowerDef>,
        level_paths: Vec<f4x_core::rules::LevelPathDef>,
    }

    /// Find a definition by its id, to change it.
    fn find<'a, T>(list: &'a mut [T], id: &str, id_of: fn(&T) -> &str) -> &'a mut T {
        list.iter_mut()
            .find(|d| id_of(d) == id)
            .unwrap_or_else(|| panic!("the shipped content has no {id:?}"))
    }

    /// What the validation says about the shipped definitions once `change` has altered them.
    fn problems_after(change: impl FnOnce(&mut Lists)) -> Vec<String> {
        let shipped = rules_with(&[]);
        let mut lists = Lists {
            races: shipped.races.defs().to_vec(),
            units: shipped.units.defs().to_vec(),
            abilities: shipped.abilities.defs().to_vec(),
            buildings: shipped.buildings.defs().to_vec(),
            spells: shipped.spells.defs().to_vec(),
            items: shipped.items.defs().to_vec(),
            item_powers: shipped.item_powers.defs().to_vec(),
            level_paths: shipped.level_paths.defs().to_vec(),
        };
        change(&mut lists);
        let rules = Rules {
            races: Registry::new(lists.races),
            units: Registry::new(lists.units),
            abilities: Registry::new(lists.abilities),
            buildings: Registry::new(lists.buildings),
            spells: Registry::new(lists.spells),
            items: Registry::new(lists.items),
            item_powers: Registry::new(lists.item_powers),
            level_paths: Registry::new(lists.level_paths),
            ..shipped
        };
        validate_rules(&rules, &features())
    }

    fn problems(edits: &[(&str, &str, &str)]) -> Vec<String> {
        validate_rules(&rules_with(edits), &features())
    }

    fn has(problems: &[String], needle: &str) -> bool {
        problems.iter().any(|m| m.contains(needle))
    }

    #[test]
    fn the_shipped_lair_table_is_the_default_and_bad_ones_are_refused() {
        assert_eq!(
            rules_with(&[]).lairs,
            f4x_core::rules::lair::LairRules::default()
        );
        assert!(has(
            &problems(&[("lairs.ron", "(4, 3), (3, 1)", "(99, 3)")]),
            "unknown realm"
        ));
        assert!(has(
            &problems(&[(
                "lairs.ron",
                "(Gold, 2), (Mana, 2), (Item, 5), (Prisoner, 1), (Spell, 3), (Special, 2)",
                ""
            )]),
            "no weight"
        ));
        assert!(has(
            &problems(&[("lairs.ron", "coin_rolls: 20", "coin_rolls: 0")]),
            "coin_rolls"
        ));
    }

    #[test]
    fn shipped_rules_are_valid() -> TestResult {
        assert_eq!(problems(&[]), Vec::<String>::new());
        let rules = load_rules(&[&content_dir(), &client_dir()], "en", &features())?;
        for realm in ["life", "death", "chaos", "nature", "sorcery"] {
            assert!(rules.realms.by_name(realm).is_some(), "{realm}");
        }
        assert!(rules
            .units
            .by_num(0)
            .ok_or("the value is missing")?
            .hero
            .is_some());
        Ok(())
    }

    #[test]
    fn typed_ids_round_trip() -> TestResult {
        let rules = load_rules(&[&content_dir(), &client_dir()], "en", &features())?;
        let id = rules.units.id_numbered(39).ok_or("the value is missing")?;
        assert_eq!(rules.units.get(id).id, "barbarian_spearmen");
        assert!(rules.units.by_num(9999).is_none());
        Ok(())
    }

    #[test]
    fn catches_item_problems() {
        let p = problems_after(|l| {
            let item = find(&mut l.items, "black_asp", |i| &i.id);
            item.slot = f4x_core::rules::ItemSlot::Armor;
            item.powers = vec![f4x_core::rules::ItemPowerId::new(9990)];
            item.charges = Some((f4x_core::rules::SpellId::new(9991), 0));
            find(&mut l.item_powers, "flaming", |i| &i.id).realm =
                Some(f4x_core::rules::RealmId::new(9992));
        });
        assert!(has(&p, "a Sword does not go in the Armor slot"), "{p:?}");
        assert!(has(&p, "unknown power 9990"), "{p:?}");
        assert!(has(&p, "unknown spell 9991"), "{p:?}");
        assert!(has(&p, "spell with no charges"), "{p:?}");
        assert!(has(&p, "unknown realm 9992"), "{p:?}");
    }

    #[test]
    fn catches_unknown_references() -> TestResult {
        let p = problems_after(|l| {
            find(&mut l.units, "dwarf", |u| &u.id)
                .abilities
                .push(f4x_core::rules::Ability::new(9990));
            find(&mut l.spells, "web", |s| &s.id).realm = f4x_core::rules::RealmId::new(9991);
            let race = find(&mut l.races, "barbarian", |r| &r.id);
            // The barbarians' bowmen (unit 41) become a unit nobody has.
            let bowmen = race.units.iter_mut().find(|u| u.num() == 41);
            *bowmen.expect("the barbarians build bowmen") = f4x_core::rules::UnitId::new(9992);
        });
        assert!(has(&p, "unknown ability 9990"), "{p:?}");
        assert!(has(&p, "unknown realm 9991"), "{p:?}");
        assert!(has(&p, "unknown unit 9992"), "{p:?}");
        assert!(
            has(&p, "barbarian_bowmen"),
            "barbarian_bowmen is no longer buildable by anyone: {p:?}"
        );
        Ok(())
    }

    #[test]
    fn catches_hero_template_problems() {
        use f4x_core::rules::{HeroTemplate, PickType};
        let mut rules = rules_with(&[]);
        let template = |unit: u16| HeroTemplate {
            unit: f4x_core::rules::UnitId::new(unit),
            picks: 1,
            pick_type: PickType::Any,
            abilities: vec![f4x_core::rules::Ability::new(85 /* might */)],
            casting_skill: 0,
            spells: Vec::new(),
            names: Vec::new(),
            gender: Default::default(),
        };
        // A hero without a template, once there are templates at all.
        rules.hero_templates = vec![template(39 /* barbarian_spearmen */), template(9990)];
        rules.hero_templates[0]
            .abilities
            .push(f4x_core::rules::Ability::new(24 /* flying */));
        // Flying is an ability, but not a hero one.
        rules.hero_templates[0]
            .spells
            .push(f4x_core::rules::SpellId::new(9991));
        let p = validate_rules(&rules, &features());
        assert!(
            has(&p, "hero template for unit 39 is not a Hero unit"),
            "{p:?}"
        );
        assert!(has(&p, "names an unknown unit"), "{p:?}");
        assert!(has(&p, "unknown hero ability 24"), "{p:?}");
        assert!(has(&p, "unknown spell 9991"), "{p:?}");
        assert!(has(&p, "dwarf\" has no hero template"), "{p:?}");
    }

    #[test]
    fn catches_unit_rule_violations() {
        let p = problems_after(|l| {
            // hero without hero data, summoned unit without a realm
            find(&mut l.units, "dwarf", |u| &u.id).hero = None;
            find(&mut l.units, "hell_hounds", |u| &u.id).realm = None;
            // ranged unit without ammo
            let spearmen = find(&mut l.units, "barbarian_spearmen", |u| &u.id);
            spearmen.stats.remove(&Stat::Ammo);
            spearmen.stats.insert(Stat::Figures, 0);
        });
        assert!(has(&p, "needs hero data"), "{p:?}");
        assert!(has(&p, "Summoned and needs a realm"), "{p:?}");
        assert!(has(&p, "ranged attack but no ammo"), "{p:?}");
        assert!(has(&p, "Figures must be at least 1"), "{p:?}");
    }

    #[test]
    fn catches_a_building_replacing_nothing() {
        let p = problems_after(|l| {
            find(&mut l.buildings, "armory", |b| &b.id).replaces =
                Some(f4x_core::rules::BuildingId::new(9990));
        });
        assert!(has(&p, "replaces unknown building 9990"), "{p:?}");
    }

    #[test]
    fn catches_building_problems() -> TestResult {
        // The armory needs the barracks; make the barracks need the armory.
        let p = problems_after(|l| {
            find(&mut l.buildings, "barracks", |b| &b.id).requires =
                vec![f4x_core::rules::BuildingId::new(4 /* armory */)];
        });
        assert!(has(&p, "requirement cycle"), "{p:?}");
        // A unit that needs a building its race cannot build.
        let q = problems_after(|l| {
            let walls = find(&mut l.buildings, "armorers_guild", |b| &b.id);
            let guild = f4x_core::rules::BuildingId::new(walls.num);
            let gnoll = f4x_core::rules::RaceId::new(5 /* gnoll */);
            assert!(!walls.races.contains(&gnoll));
            let unit = l
                .units
                .iter_mut()
                .find(|u| u.race == Some(gnoll))
                .expect("a gnoll unit exists");
            unit.requires = vec![guild];
        });
        assert!(has(&q, "which its race cannot build"), "{q:?}");
        Ok(())
    }

    #[test]
    fn catches_spell_and_effect_problems() {
        use f4x_core::rules::effects::Effect;
        use f4x_core::rules::Duration;
        let p = problems_after(|l| {
            // a passive modifier on an instant spell
            find(&mut l.spells, "giant_strength", |s| &s.id).duration = Duration::Instant;
            // a summon of nothing
            let sprites = find(&mut l.spells, "sprites", |s| &s.id);
            for rule in &mut sprites.effects {
                if let Effect::Summon { count, .. } = &mut rule.effect {
                    *count = 0;
                }
            }
            // a custom hook that does not exist
            let healing = find(&mut l.spells, "healing", |s| &s.id);
            healing.effects[0].effect = Effect::Custom("mystery".into());
            // an ability that applies an instant spell as an enchantment
            let mut rule = healing.effects[0].clone();
            let fireball =
                f4x_core::rules::SpellId::new(find(&mut l.spells, "fireball", |s| &s.id).num);
            rule.effect = Effect::ApplyEnchantment(fireball);
            l.abilities[0].effects = vec![rule];
        });
        assert!(has(&p, "instant but has Passive"), "{p:?}");
        assert!(has(&p, "summon count"), "{p:?}");
        assert!(has(&p, "unknown custom hook"), "{p:?}");
        assert!(has(&p, "instant spell, not an enchantment"), "{p:?}");
    }

    #[test]
    fn catches_bad_level_paths() {
        let p = problems_after(|l| {
            find(&mut l.level_paths, "standard_xp", |p| &p.id).levels[1].xp = 0;
        });
        assert!(has(&p, "strictly increase"), "{p:?}");
    }

    #[test]
    fn veteran_stats_come_from_def_plus_level_modifiers() -> TestResult {
        use f4x_core::rules::effects::Effect;
        use f4x_core::rules::ModOp;
        // What a list of effects adds to a stat.
        let added = |effects: &[f4x_core::rules::effects::EffectRule], stat: Stat| -> i32 {
            effects
                .iter()
                .map(|r| match r.effect {
                    Effect::Modify {
                        stat: s,
                        op: ModOp::Add,
                        value,
                    } if s == stat => value,
                    _ => 0,
                })
                .sum()
        };
        let rules = load_rules(&[&content_dir(), &client_dir()], "en", &features())?;
        let unit = rules.units.by_num(39).ok_or("the value is missing")?;
        let path = rules
            .level_paths
            .try_get(unit.level_path.ok_or("the value is missing")?)
            .ok_or("the value is missing")?;
        let level = &path.levels[2];
        let mods = passive_modifiers(&level.effects, Source::Level(2));
        for stat in [Stat::Melee, Stat::Defense, Stat::Resistance] {
            assert_eq!(
                compute(stat, unit.stats[&stat], &mods),
                unit.stats[&stat] + added(&level.effects, stat),
                "{stat:?}"
            );
        }
        // The level gives something, or the loop above proves nothing.
        assert!(added(&level.effects, Stat::Defense) > 0);
        // A spell enchantment stacks with it.
        let spell = rules.spells.by_num(7).ok_or("the value is missing")?;
        let id = rules.spells.id_numbered(7).ok_or("the value is missing")?;
        let mut all = mods.clone();
        all.extend(passive_modifiers(&spell.effects, Source::Spell(id)));
        assert_eq!(
            compute(Stat::Defense, unit.stats[&Stat::Defense], &all),
            unit.stats[&Stat::Defense]
                + added(&level.effects, Stat::Defense)
                + added(&spell.effects, Stat::Defense)
        );
        Ok(())
    }

    #[test]
    fn city_names_come_from_the_first_directory_that_has_the_file() -> TestResult {
        let over = f4x_core::testdir::TestDir::new("names");
        std::fs::write(
            over.join("city_names.ron"),
            r#"[(race: "barbarian", names: ["Alpha", "Beta"])]"#,
        )?;
        let features = features();
        let shipped = load_rules(&[&content_dir(), &client_dir()], "en", &features)?;
        let rules = load_rules(&[&over, &content_dir(), &client_dir()], "en", &features)?;
        let barbarian = rules.races.by_num(0).ok_or("the value is missing")?;
        assert_eq!(barbarian.city_names, ["Alpha", "Beta"]);
        // The file is read whole from the first directory: the shipped table has names for
        // another race, the overriding one has none.
        assert!(!shipped
            .races
            .by_num(1)
            .ok_or("the value is missing")?
            .city_names
            .is_empty());
        assert!(rules
            .races
            .by_num(1)
            .ok_or("the value is missing")?
            .city_names
            .is_empty());
        std::fs::write(
            over.join("city_names.ron"),
            r#"[(race: "nobody", names: ["X"])]"#,
        )?;
        assert!(load_rules(&[&over, &content_dir(), &client_dir()], "en", &features).is_err());
        Ok(())
    }
}
