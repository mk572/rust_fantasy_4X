//! Loading and cross-reference validation of the rules definitions.

use crate::half_int::HalfInt;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use crate::rules::effects::{Condition, Effect, EffectRule, Trigger};
use crate::rules::{
    AbilityDef, AbilityId, BuildingDef, Def, Duration, HeroTemplate, ItemDef, ItemPowerDef,
    LevelPathDef, RaceDef, RealmDef, Registry, RetortDef, Rules, RulesConfig, SpellDef, Stat,
    UnitDef, UnitKind, WizardDef,
};

use super::{id_problem, ContentError};
use crate::issue::{ContentIssue, Element, IssueKind, Issues};

/// The first of `dirs` that has the file `name`.
fn find(dirs: &[&Path], name: &str) -> Option<std::path::PathBuf> {
    dirs.iter().map(|d| d.join(name)).find(|p| is_there(p))
}

/// Whether there is something at the path, a link that leads nowhere included: that is a file that
/// cannot be read, an error to report, and not a file that is missing.
pub(super) fn is_there(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn read(dirs: &[&Path], name: &str) -> Result<String, ContentError> {
    let path = find(dirs, name)
        .unwrap_or_else(|| dirs.first().map_or_else(|| name.into(), |d| d.join(name)));
    std::fs::read_to_string(&path).map_err(|e| ContentError::io(&path, &e))
}

fn parse<T: serde::de::DeserializeOwned>(dirs: &[&Path], name: &str) -> Result<T, ContentError> {
    ron::from_str(&read(dirs, name)?).map_err(|e| ContentError::parse(name, &e))
}

/// The registry of the rows of a file; the problem is noted when the rows cannot make one.
fn table<T: Def>(file: &str, defs: Vec<T>, problems: &mut Issues) -> Registry<T> {
    Registry::try_new(defs).unwrap_or_else(|e| {
        problems.duplicate(file, "", "the number of a row (rows must rise)", e);
        Registry::default()
    })
}

/// A file that must be there: its rows, or the issue that says why not.
fn parse_into<T: serde::de::DeserializeOwned>(
    dirs: &[&Path],
    name: &str,
    issues: &mut Issues,
) -> Option<T> {
    match parse(dirs, name) {
        Ok(v) => Some(v),
        Err(e) => {
            issues.extend_error(e);
            None
        }
    }
}

/// A file that may be left out: the type's default when it is not there.
fn optional<T: serde::de::DeserializeOwned + Default>(
    dirs: &[&Path],
    name: &str,
    issues: &mut Issues,
) -> Option<T> {
    if find(dirs, name).is_some() {
        parse_into(dirs, name, issues)
    } else {
        Some(T::default())
    }
}

/// Load and validate every rules file. Each file comes from the first of `dirs` that has it,
/// so a local directory can replace single files of the committed one.
pub fn load_rules(dirs: &[&Path]) -> Result<Rules, ContentError> {
    let dir = dirs;
    // Every file is read and parsed, and every failure is kept: the player sees all of them at once.
    let mut issues = Issues::new();
    let races = parse_into::<Vec<RaceDef>>(dir, "races.ron", &mut issues);
    let hero_templates = optional::<Vec<HeroTemplate>>(dir, "heroes.ron", &mut issues);
    let wizards = optional::<Vec<WizardDef>>(dir, "wizards.ron", &mut issues);
    let units = parse_into::<Vec<UnitDef>>(dir, "units.ron", &mut issues);
    let abilities = parse_into::<Vec<AbilityDef>>(dir, "abilities.ron", &mut issues);
    let buildings = parse_into::<Vec<BuildingDef>>(dir, "buildings.ron", &mut issues);
    let realms = parse_into::<Vec<RealmDef>>(dir, "realms.ron", &mut issues);
    let retorts = parse_into::<Vec<RetortDef>>(dir, "retorts.ron", &mut issues);
    let spells = parse_into::<Vec<SpellDef>>(dir, "spells.ron", &mut issues);
    let items = parse_into::<Vec<ItemDef>>(dir, "items.ron", &mut issues);
    let item_powers = parse_into::<Vec<ItemPowerDef>>(dir, "item_powers.ron", &mut issues);
    let level_paths = parse_into::<Vec<LevelPathDef>>(dir, "level_paths.ron", &mut issues);
    let config = parse_into::<RulesConfig>(dir, "rules.ron", &mut issues);
    let ratings = optional::<crate::rules::UnitRatings>(dir, "unit_ratings.ron", &mut issues);
    let efficiency =
        optional::<crate::rules::UnitEfficiencies>(dir, "unit_efficiency.ron", &mut issues);
    let battle = parse_into::<crate::rules::BattleRules>(dir, "battle.ron", &mut issues);
    let lairs = parse_into::<crate::rules::lair::LairRules>(dir, "lairs.ron", &mut issues);
    let ai_sites = parse_into::<crate::rules::ai_site::SiteRules>(dir, "ai_sites.ron", &mut issues);
    if let Some(range) = config.as_ref().map(|c| &c.casting_range) {
        if range.beyond.windows(2).any(|w| w[0] >= w[1])
            || range.beyond.last().is_some_and(|&l| l >= range.far)
        {
            issues.conflict(
                "rules.ron",
                "casting_range",
                "beyond must rise and end below `far`",
            );
        }
    }
    // A file that failed to read left an issue; with no issue every required file is there.
    let (
        Some(config),
        Some(races),
        Some(units),
        Some(abilities),
        Some(buildings),
        Some(realms),
        Some(spells),
        Some(items),
        Some(item_powers),
        Some(level_paths),
        Some(retorts),
        Some(battle),
        Some(lairs),
        Some(ai_sites),
    ) = (
        config,
        races,
        units,
        abilities,
        buildings,
        realms,
        spells,
        items,
        item_powers,
        level_paths,
        retorts,
        battle,
        lairs,
        ai_sites,
    )
    else {
        return Err(ContentError::new(issues.into_vec()));
    };
    if !issues.is_empty() {
        return Err(ContentError::new(issues.into_vec()));
    }
    let parts = Parts {
        config,
        races,
        units,
        abilities,
        buildings,
        realms,
        spells,
        items,
        item_powers,
        level_paths,
        wizards: wizards.unwrap_or_default(),
        retorts,
        hero_templates: hero_templates.unwrap_or_default(),
        ratings: ratings.unwrap_or_default(),
        efficiency: efficiency.unwrap_or_default(),
        battle,
        lairs,
        ai_sites,
    };
    assemble(parts)
}

/// What the files hold, before the tables are built.
struct Parts {
    config: RulesConfig,
    races: Vec<RaceDef>,
    units: Vec<UnitDef>,
    abilities: Vec<AbilityDef>,
    buildings: Vec<BuildingDef>,
    realms: Vec<RealmDef>,
    spells: Vec<SpellDef>,
    items: Vec<ItemDef>,
    item_powers: Vec<ItemPowerDef>,
    level_paths: Vec<LevelPathDef>,
    wizards: Vec<WizardDef>,
    retorts: Vec<RetortDef>,
    hero_templates: Vec<HeroTemplate>,
    ratings: crate::rules::UnitRatings,
    efficiency: crate::rules::UnitEfficiencies,
    battle: crate::rules::BattleRules,
    lairs: crate::rules::lair::LairRules,
    ai_sites: crate::rules::ai_site::SiteRules,
}

/// The part of the content a file's rows are, when a bad row of the file may be dropped.
fn element_of(file: &str) -> Option<Element> {
    match file {
        "wizards.ron" => Some(Element::Wizard),
        "unit_ratings.ron" => Some(Element::Rating),
        "unit_efficiency.ron" => Some(Element::Efficiency),
        "items.ron" => Some(Element::Item),
        _ => None,
    }
}

/// The rows of a degradable file that cannot stay in a table: the table needs rising numbers, so the
/// rows that stay are the longest run of rising numbers (the first of two rows with one number stays),
/// and every other row is dropped. One row with a number far too big goes alone, not everything after it.
fn out_of_order<T>(
    file: &str,
    element: Element,
    rows: &mut Vec<T>,
    num: impl Fn(&T) -> u16,
    degraded: &mut Vec<ContentIssue>,
) {
    let nums: Vec<u16> = rows.iter().map(&num).collect();
    // `tails[k]`: the row that ends the best rising run of length k + 1; `before[i]`: the row before row i in its run.
    let mut tails: Vec<usize> = Vec::new();
    let mut before: Vec<Option<usize>> = vec![None; nums.len()];
    for (i, &n) in nums.iter().enumerate() {
        let k = tails.partition_point(|&t| nums[t] < n);
        before[i] = k.checked_sub(1).map(|k| tails[k]);
        if k == tails.len() {
            tails.push(i);
        } else {
            tails[k] = i;
        }
    }
    let mut stays = vec![false; nums.len()];
    let mut at = tails.last().copied();
    while let Some(i) = at {
        stays[i] = true;
        at = before[i];
    }
    for (i, &n) in nums.iter().enumerate().filter(|(i, _)| !stays[*i]) {
        degraded.push(
            ContentIssue::new(
                file,
                format!("row {} (numbered {n})", i + 1),
                IssueKind::Duplicate,
            )
            .with([
                "the number of a row (rows must rise)".to_string(),
                n.to_string(),
            ])
            .about_row(n)
            .degrading(element),
        );
    }
    let mut i = 0;
    rows.retain(|_| {
        i += 1;
        stays[i - 1]
    });
}

/// Build the rules from the files' rows, drop what may be dropped, and check.
///
/// A bad row of a degradable file (`element_of`) is dropped and reported with a `Degrade` outcome; the
/// game plays without it. A row is dropped only when nothing refers to it: the rules are checked
/// again without it, and an issue that appears (a reference to the dropped row) refuses the game.
/// Every other issue refuses it at once.
fn assemble(mut parts: Parts) -> Result<Rules, ContentError> {
    let mut dropped: BTreeSet<(Element, u16)> = BTreeSet::new();
    let mut degraded: Vec<ContentIssue> = Vec::new();
    out_of_order(
        "wizards.ron",
        Element::Wizard,
        &mut parts.wizards,
        |w| w.num,
        &mut degraded,
    );
    out_of_order(
        "items.ron",
        Element::Item,
        &mut parts.items,
        |i| i.num,
        &mut degraded,
    );
    out_of_order(
        "unit_ratings.ron",
        Element::Rating,
        &mut parts.ratings.units,
        |r| r.num,
        &mut degraded,
    );
    out_of_order(
        "unit_efficiency.ron",
        Element::Efficiency,
        &mut parts.efficiency.units,
        |r| r.num,
        &mut degraded,
    );
    let parts = &parts;
    let rules = build(parts, &dropped)?;
    let found = validate_rules(&rules);
    let (droppable, fatal): (Vec<ContentIssue>, Vec<ContentIssue>) = found
        .into_iter()
        .partition(|i| i.row.is_some() && element_of(&i.file).is_some());
    if !fatal.is_empty() {
        // Refused: the whole list, the rows that could have been dropped with it.
        let all = degraded.into_iter().chain(droppable).chain(fatal).collect();
        return Err(ContentError::new(all));
    }
    if droppable.is_empty() {
        let mut rules = rules;
        rules.degraded = degraded;
        return Ok(rules);
    }
    for issue in droppable {
        if let (Some(element), Some(row)) = (element_of(&issue.file), issue.row) {
            dropped.insert((element, row));
            degraded.push(issue.degrading(element));
        }
    }
    let mut rules = build(parts, &dropped)?;
    // Without the rows, something that referred to one has nothing to point at: refuse.
    let again = validate_rules(&rules);
    if !again.is_empty() {
        return Err(ContentError::new(again));
    }
    rules.degraded = degraded;
    Ok(rules)
}

/// The tables from the rows, less the rows dropped.
fn build(p: &Parts, dropped: &BTreeSet<(Element, u16)>) -> Result<Rules, ContentError> {
    let mut problems = Issues::new();
    let keep = |element: Element, num: u16| !dropped.contains(&(element, num));
    let races = table("races.ron", p.races.clone(), &mut problems);
    let units = table("units.ron", p.units.clone(), &mut problems);
    let abilities = table("abilities.ron", p.abilities.clone(), &mut problems);
    let buildings = table("buildings.ron", p.buildings.clone(), &mut problems);
    let realms = table("realms.ron", p.realms.clone(), &mut problems);
    let spells = table("spells.ron", p.spells.clone(), &mut problems);
    let items = table(
        "items.ron",
        p.items
            .iter()
            .filter(|i| keep(Element::Item, i.num))
            .cloned()
            .collect(),
        &mut problems,
    );
    let item_powers = table("item_powers.ron", p.item_powers.clone(), &mut problems);
    let level_paths = table("level_paths.ron", p.level_paths.clone(), &mut problems);
    let wizards = table(
        "wizards.ron",
        p.wizards
            .iter()
            .filter(|w| keep(Element::Wizard, w.num))
            .cloned()
            .collect(),
        &mut problems,
    );
    let retorts = table("retorts.ron", p.retorts.clone(), &mut problems);
    if !problems.is_empty() {
        return Err(ContentError::new(problems.into_vec()));
    }
    let mut ratings = p.ratings.clone();
    ratings.units.retain(|r| keep(Element::Rating, r.num));
    let mut efficiency = p.efficiency.clone();
    efficiency
        .units
        .retain(|r| keep(Element::Efficiency, r.num));
    Ok(Rules {
        config: p.config.clone(),
        races,
        units,
        abilities,
        buildings,
        realms,
        spells,
        items,
        item_powers,
        level_paths,
        wizards,
        retorts,
        hero_templates: p.hero_templates.clone(),
        ratings,
        efficiency,
        degraded: Vec::new(),
        battle: p.battle.clone(),
        lairs: p.lairs.clone(),
        ai_sites: p.ai_sites.clone(),
    })
}

fn check_ids<T: Def>(kind: &str, reg: &Registry<T>, problems: &mut Issues) {
    let file = file_of(kind);
    let mut seen = HashSet::new();
    for d in reg.defs() {
        problems.row(Some(d.num()));
        problems.extend_opt(id_problem(file, kind, d.id()));
        if !seen.insert(d.id()) {
            problems.duplicate(
                file,
                format!("{kind} {:?}", d.id()),
                "the id",
                format!("{:?}", d.id()),
            );
        }
    }
    problems.row(None);
}

/// The file a kind of row comes from.
fn file_of(kind: &str) -> &'static str {
    match kind {
        "race" => "races.ron",
        "unit" => "units.ron",
        "ability" => "abilities.ron",
        "building" => "buildings.ron",
        "realm" => "realms.ron",
        "spell" => "spells.ron",
        "item" => "items.ron",
        "item power" => "item_powers.ron",
        "level path" => "level_paths.ron",
        "wizard" => "wizards.ron",
        "retort" => "retorts.ron",
        _ => "",
    }
}

/// The lair rules: real realms, rolls that can be made and a table that is not empty.
fn check_lairs(r: &Rules, p: &mut Issues) {
    const F: &str = "lairs.ron";
    let l = &r.lairs;
    let realm = |what: &str, num: u16, p: &mut Issues| {
        if r.realms.id_numbered(num).is_none() {
            p.dangling(F, "", what, "realm", num);
        }
    };
    for (what, list) in [
        ("tower_realms", &l.tower_realms),
        ("holy_realms", &l.holy_realms),
        ("wild_realms", &l.wild_realms),
    ] {
        if list.iter().map(|(_, w)| u64::from(*w)).sum::<u64>() == 0 {
            p.out_of_range(F, "", what, "all weights 0", "at least one weight above 0");
        }
        for &(num, _) in list {
            realm(what, num, p);
        }
    }
    for &num in l.node_own_realms.iter().chain([&l.node_other]) {
        realm("the node realms", num, p);
    }
    if l.hoard.iter().map(|(_, w)| u64::from(*w)).sum::<u64>() == 0 {
        p.out_of_range(
            F,
            "",
            "hoard",
            "all weights 0",
            "at least one weight above 0",
        );
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
            p.out_of_range(F, "", what, n, "at least 1 roll");
        }
    }
    if l.second_spread - l.max_first_guards < 1 {
        p.out_of_range(
            F,
            "",
            "second_spread",
            l.second_spread,
            format!("above max_first_guards ({})", l.max_first_guards),
        );
    }
    if l.max_first_guards > l.max_guards as i32 {
        p.out_of_range(
            F,
            "",
            "max_first_guards",
            l.max_first_guards,
            format!("at most max_guards ({})", l.max_guards),
        );
    }
    if !(1..=4).contains(&l.spell_total) {
        p.out_of_range(
            F,
            "",
            "spell_total",
            l.spell_total,
            "1 to 4 (a spell's rarity)",
        );
    }
}

/// The numbers `rules.ron` keeps that name rows of other files.
fn check_config(r: &Rules, p: &mut Issues) {
    const F: &str = "rules.ron";
    let c = &r.config;
    for b in c.autobuild_order.iter() {
        if r.buildings.try_get(*b).is_none() {
            p.dangling(F, "", "autobuild_order", "building", b.num());
        }
    }
    for (_, b) in &c.starting_buildings {
        if r.buildings.try_get(*b).is_none() {
            p.dangling(F, "", "starting_buildings", "building", b.num());
        }
    }
    for &n in &c.limited_ai_races {
        if r.races.by_num(n).is_none() {
            p.dangling(F, "", "limited_ai_races", "race", n);
        }
    }
    for (i, &(a, b)) in c.mergeable_realms.iter().enumerate() {
        for n in [a, b] {
            if r.realms.id_numbered(n).is_none() {
                p.dangling(F, "", "mergeable_realms", "realm", n);
            }
        }
        if a == b {
            p.conflict(
                F,
                "mergeable_realms",
                format!("realm {a} cannot be merged with itself"),
            );
        }
        if c.mergeable_realms[..i]
            .iter()
            .any(|&(x, y)| (x, y) == (a, b) || (x, y) == (b, a))
        {
            p.duplicate(
                F,
                "mergeable_realms",
                "the pair of realms",
                format!("{a} and {b}"),
            );
        }
    }
}

/// The unit ratings: rows in the order of the unit numbers, each for a unit that exists, with at
/// least one positive number.
fn check_ratings(r: &Rules, p: &mut Issues) {
    let positive = |v: &f32| v.is_finite() && *v > 0.0;
    let unit_exists = |num: u16| r.units.iter().any(|(_, u)| u.num == num);
    const E: &str = "unit_efficiency.ron";
    for w in r.efficiency.units.windows(2) {
        if w[0].num >= w[1].num {
            p.row(Some(w[1].num));
            p.duplicate(
                E,
                format!("unit {}", w[1].num),
                "the unit number (a row is out of order or twice)",
                w[1].num,
            );
            p.row(None);
        }
    }
    for row in &r.efficiency.units {
        p.row(Some(row.num));
        let at = format!("unit {}", row.num);
        if !unit_exists(row.num) {
            p.dangling(E, &at, "num", "unit", row.num);
        }
        for (field, v) in [
            ("rating", row.rating),
            ("cost", row.cost),
            ("per_100", row.per_100),
            ("relative", row.relative),
        ] {
            if !positive(&v) {
                p.out_of_range(E, &at, field, v, "a number above 0");
            }
        }
    }
    p.row(None);
    const R: &str = "unit_ratings.ron";
    // Without a file of ratings there is nothing the factor multiplies.
    if !r.ratings.units.is_empty() && !positive(&r.ratings.hero_factor) {
        p.out_of_range(
            R,
            "",
            "hero_factor",
            r.ratings.hero_factor,
            "a number above 0",
        );
    }
    for w in r.ratings.units.windows(2) {
        if w[0].num >= w[1].num {
            p.row(Some(w[1].num));
            p.duplicate(
                R,
                format!("unit {}", w[1].num),
                "the unit number (a row is out of order or twice)",
                w[1].num,
            );
            p.row(None);
        }
    }
    for row in &r.ratings.units {
        p.row(Some(row.num));
        let at = format!("unit {}", row.num);
        if !unit_exists(row.num) {
            p.dangling(R, &at, "num", "unit", row.num);
        }
        if row.levels.is_empty() {
            p.missing(R, &at, "levels");
        }
        for v in row.levels.iter().filter(|v| !positive(v)) {
            p.out_of_range(R, &at, "levels", v, "numbers above 0");
        }
    }
    p.row(None);
}

/// The hero templates: one for each hero unit, no more, with real abilities and spells.
fn check_hero_templates(r: &Rules, p: &mut Issues) {
    const F: &str = "heroes.ron";
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
                    p.conflict(
                        F,
                        &who,
                        format!("it is row {row} but its unit is numbered {}", u.num),
                    );
                }
            }
            Some(_) => p.conflict(F, &who, "the unit is not a Hero unit"),
            None => p.dangling(F, &who, "unit", "unit", t.unit.num()),
        }
        if !seen.insert(t.unit) {
            p.duplicate(F, &who, "the template of unit", t.unit.num());
        }
        for &a in &t.abilities {
            if !crate::hero::HeroAbilities::is_hero_ability(a) {
                p.dangling(F, &who, "abilities", "hero ability", a.num());
            }
        }
        for &s in &t.spells {
            if r.spells.try_get(s).is_none() {
                p.dangling(F, &who, "spells", "spell", s.num());
            }
        }
        if t.spells.len() > 4 {
            p.out_of_range(
                F,
                &who,
                "spells",
                t.spells.len(),
                "at most four innate spells",
            );
        }
    }
    for u in r.units.defs().iter().filter(|u| u.kind == UnitKind::Hero) {
        if !seen.contains(&crate::rules::UnitId::new(u.num)) {
            p.missing(F, format!("hero {:?}", u.id), "its hero template");
        }
    }
}

fn check_items(r: &Rules, p: &mut Issues) {
    const P: &str = "item_powers.ron";
    const I: &str = "items.ron";
    for power in r.item_powers.defs() {
        let who = format!("item power {:?}", power.id);
        if let Some(realm) = power.realm {
            if r.realms.try_get(realm).is_none() {
                p.dangling(P, &who, "realm", "realm", realm.num());
            }
        }
        if let Some(spell) = power.spell {
            if r.spells.try_get(spell).is_none() {
                p.dangling(P, &who, "spell", "spell", spell.num());
            }
        }
        if let Some(attack) = power.attack {
            if r.abilities.try_get(attack).is_none() {
                p.dangling(P, &who, "attack", "ability", attack.num());
            }
        }
        if power.kinds.is_empty() {
            p.missing(P, &who, "kinds (it fits no item type)");
        }
    }
    for item in r.items.defs() {
        p.row(Some(item.num));
        let who = format!("item {:?}", item.id);
        if item.kind.slot() != item.slot {
            p.conflict(
                I,
                &who,
                format!("a {:?} does not go in the {:?} slot", item.kind, item.slot),
            );
        }
        if item.effects.is_empty()
            && item.powers.is_empty()
            && item.charges.is_none()
            && item.spell_skill == 0
            && item.spell_save == 0
        {
            p.missing(I, &who, "effects (the item does nothing)");
        }
        for &power in &item.powers {
            if r.item_powers.try_get(power).is_none() {
                p.dangling(I, &who, "powers", "item power", power.num());
            }
        }
        if let Some((spell, count)) = item.charges {
            if r.spells.try_get(spell).is_none() {
                p.dangling(I, &who, "charges", "spell", spell.num());
            }
            if count == 0 {
                p.out_of_range(I, &who, "charges", 0, "at least 1 charge");
            }
        }
    }
    p.row(None);
}

/// Returns a human-readable list of problems; empty means valid.
pub fn validate_rules(r: &Rules) -> Vec<ContentIssue> {
    let mut p = Issues::new();
    check_lairs(r, &mut p);
    for i in r.ai_sites.problems() {
        p.push(i);
    }
    for i in crate::hero::problems(r) {
        p.push(i);
    }
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
    check_ratings(r, &mut p);
    check_config(r, &mut p);

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
        r.retorts
            .defs()
            .iter()
            .map(|d| (format!("retort {:?}", d.id), &d.effects[..])),
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
    for retort in r.retorts.defs() {
        for rule in &retort.effects {
            let fine = rule.when == crate::rules::effects::Trigger::Passive
                && matches!(
                    rule.effect,
                    Effect::Modify {
                        stat: Stat::SpellCostPercent
                            | Stat::SpellUpkeepPercent
                            | Stat::UnitManaUpkeepPercent
                            | Stat::ManaIncome
                            | Stat::ResearchIncome
                            | Stat::CastingSkill
                            | Stat::StartingSkill
                            | Stat::SkillGainPercent
                            | Stat::DispelMultiplier
                            | Stat::DispelPower
                            | Stat::NodeImmunity
                            | Stat::CastingDistance
                            | Stat::UnitLevels
                            | Stat::AlchemyPercent
                            | Stat::StartingWeapons
                            | Stat::StartingFame
                            | Stat::DiplomacyGainPercent
                            | Stat::DiplomacyLossPercent
                            | Stat::HireCostPercent
                            | Stat::OfferChancePercent
                            | Stat::NodePowerDoublings
                            | Stat::ReligiousPercent,
                        ..
                    }
                );
            if !fine {
                p.conflict(
                    "retorts.ron",
                    format!("retort {:?}", retort.id),
                    "only passive changes to the wizard stats (SpellCostPercent, SpellUpkeepPercent, UnitManaUpkeepPercent, ManaIncome, ResearchIncome, CastingSkill and the others a wizard has) are understood",
                );
            }
        }
    }
    for path in r.level_paths.defs() {
        for (i, level) in path.levels.iter().enumerate() {
            carriers.push((
                format!("level path {:?} level {i}", path.id),
                &level.effects[..],
            ));
        }
    }
    for (owner, rules) in carriers {
        // The effects of a premade item are the item's: a bad one drops the item.
        if owner.starts_with("item ") {
            p.row(
                r.items
                    .defs()
                    .iter()
                    .find(|i| owner == format!("item {:?}", i.id))
                    .map(|i| i.num),
            );
        }
        for rule in rules {
            check_rule(r, &owner, rule, &mut p);
        }
        p.row(None);
    }
    p.into_vec()
}

fn check_races(r: &Rules, p: &mut Issues) {
    const F: &str = "races.ron";
    for race in r.races.defs() {
        let at = format!("race {:?}", race.id);
        let this = crate::rules::RaceId::new(race.num);
        if race.units.is_empty() {
            p.missing(F, &at, "units");
        }
        for &u in &race.units {
            match r.units.try_get(u) {
                None => p.dangling(F, &at, "units", "unit", u.num()),
                Some(unit) if unit.race != Some(this) => {
                    p.conflict(
                        F,
                        &at,
                        format!("it lists unit {:?}, which belongs to another race", unit.id),
                    );
                }
                Some(_) => {}
            }
        }
        for &b in &race.buildings {
            match r.buildings.try_get(b) {
                None => p.dangling(F, &at, "buildings", "building", b.num()),
                Some(bd) if !bd.buildable_by(this) => {
                    p.conflict(
                        F,
                        &at,
                        format!(
                            "it lists building {:?}, which its races list excludes",
                            bd.id
                        ),
                    );
                }
                Some(_) => {}
            }
        }
        if race.start_army.len() != 2 || race.start_army.iter().any(|u| !race.units.contains(u)) {
            p.conflict(F, &at, "it must start with two of its own units");
        }
    }
}

fn check_units(r: &Rules, p: &mut Issues) {
    const F: &str = "units.ron";
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
                p.missing(F, &who, &format!("stats: {stat:?}"));
            }
        }
        for (stat, &v) in &u.stats {
            if !(HalfInt::ZERO..=HalfInt::from_int(99)).contains(&v) {
                p.out_of_range(F, &who, &format!("{stat:?}"), v, "0 to 99");
            }
        }
        let stat = |s: Stat| u.stats.get(&s).map_or(0, |v| v.floor());
        for s in [Stat::MaxHp, Stat::Figures, Stat::Moves] {
            if stat(s) < 1 {
                p.out_of_range(F, &who, &format!("{s:?}"), stat(s), "at least 1");
            }
        }
        if stat(Stat::Ranged) > 0 && stat(Stat::Ammo) < 1 {
            p.conflict(F, &who, "it has a ranged attack but no ammo");
        }
        if let Some(race) = u.race {
            if r.races.try_get(race).is_none() {
                p.dangling(F, &who, "race", "race", race.num());
            }
        }
        if let Some(realm) = u.realm {
            if r.realms.try_get(realm).is_none() {
                p.dangling(F, &who, "realm", "realm", realm.num());
            }
        }
        for &a in &u.abilities {
            if r.abilities.try_get(a).is_none() {
                p.dangling(F, &who, "abilities", "ability", a.num());
            }
        }
        for &b in &u.requires {
            match r.buildings.try_get(b) {
                None => p.dangling(F, &who, "requires", "building", b.num()),
                Some(bd) => {
                    if let Some(race) = u.race {
                        if !bd.races.contains(&race) {
                            p.conflict(
                                F,
                                &who,
                                format!(
                                    "it requires building {:?}, which its race cannot build",
                                    bd.id
                                ),
                            );
                        }
                    }
                }
            }
        }
        if let Some(path) = u.level_path {
            if r.level_paths.try_get(path).is_none() {
                p.dangling(F, &who, "level_path", "level path", path.num());
            }
        }
        match u.kind {
            UnitKind::Normal => {
                match &u.race {
                    None => p.missing(F, &who, "race (a Normal unit needs one)"),
                    Some(race) => {
                        let listed = r
                            .races
                            .try_get(*race)
                            .is_some_and(|rd| rd.units.contains(&crate::rules::UnitId::new(u.num)));
                        if !listed {
                            p.conflict(
                                F,
                                &who,
                                "it is not in its race's unit list, so nothing can build it",
                            );
                        }
                    }
                }
                if u.hero.is_some() {
                    p.conflict(F, &who, "it is Normal but has hero data");
                }
            }
            UnitKind::Hero => {
                if u.race.is_some() {
                    p.conflict(F, &who, "it is a Hero and must not have a race");
                }
                if u.hero.is_none() {
                    p.missing(F, &who, "hero (a Hero unit needs hero data)");
                }
            }
            UnitKind::Summoned => {
                if u.realm.is_none() {
                    p.missing(F, &who, "realm (a Summoned unit needs one)");
                }
                if u.cost == 0 {
                    p.out_of_range(
                        F,
                        &who,
                        "cost",
                        0,
                        "above 0 (a Summoned unit needs a mana cost)",
                    );
                }
            }
            UnitKind::Fantastic => {
                if u.race.is_some() || u.hero.is_some() {
                    p.conflict(
                        F,
                        &who,
                        "it is Fantastic and must have no race or hero data",
                    );
                }
            }
        }
    }
}

fn check_buildings(r: &Rules, p: &mut Issues) {
    const F: &str = "buildings.ron";
    for b in r.buildings.defs() {
        let at = format!("building {:?}", b.id);
        if b.cost == 0 {
            p.out_of_range(F, &at, "cost", 0, "above 0");
        }
        for &req in &b.requires {
            if r.buildings.try_get(req).is_none() {
                p.dangling(F, &at, "requires", "building", req.num());
            }
        }
        for race in &b.races {
            if r.races.try_get(*race).is_none() {
                p.dangling(F, &at, "races", "race", race.num());
            }
            // The list is complete: a race that cannot build what this needs cannot build this.
            for &req in &b.requires {
                if r.buildings
                    .try_get(req)
                    .is_some_and(|needed| !needed.races.contains(race))
                {
                    p.conflict(
                        F,
                        &at,
                        format!(
                            "it lists race {}, which cannot build {} that it requires",
                            race.num(),
                            req.num()
                        ),
                    );
                }
            }
        }
        if let Some(old) = b.replaces {
            if r.buildings.try_get(old).is_none() {
                p.dangling(F, &at, "replaces", "building", old.num());
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
    fn visit(n: u16, g: &BTreeMap<u16, Vec<u16>>, state: &mut BTreeMap<u16, u8>, p: &mut Issues) {
        match state.get(&n) {
            Some(2) => return,
            Some(_) => {
                p.conflict(
                    "buildings.ron",
                    format!("building {n}"),
                    "it is part of a requirement cycle",
                );
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

fn check_spells(r: &Rules, p: &mut Issues) {
    const F: &str = "spells.ron";
    for s in r.spells.defs() {
        let who = format!("spell {:?}", s.id);
        if r.realms.try_get(s.realm).is_none() {
            p.dangling(F, &who, "realm", "realm", s.realm.num());
        }
        if s.cost == 0 {
            p.out_of_range(F, &who, "cost", 0, "above 0");
        }
        if s.research_cost == 0 {
            p.out_of_range(F, &who, "research_cost", 0, "above 0");
        }
        if s.contexts.is_empty() {
            p.missing(F, &who, "contexts (it can be cast nowhere)");
        }
        // A lasting spell may carry no effects of its own when the engine reads it by number
        // (Resist Elements adds to resistance against some realms only).
        if s.effects.is_empty() && s.duration == Duration::Instant {
            p.missing(F, &who, "effects (it does nothing)");
        }
        for other in &s.not_with {
            if r.spells.try_get(*other).is_none() {
                p.dangling(F, &who, "not_with", "spell", other.num());
            }
        }
        let unknown_abilities = s
            .already_able
            .iter()
            .filter(|a| r.abilities.try_get(**a).is_none());
        for a in unknown_abilities {
            p.dangling(F, &who, "already_able", "ability", a.num());
        }
        if s.duration == Duration::Instant {
            if s.upkeep != 0 {
                p.conflict(F, &who, "it is instant but has upkeep");
            }
            if s.effects.iter().any(|e| e.when == Trigger::Passive) {
                p.conflict(F, &who, "it is instant but has Passive effects; use OnCast");
            }
        }
    }
}

fn check_wizards(r: &Rules, p: &mut Issues) {
    for retort in r.retorts.defs() {
        for need in &retort.requires {
            if let crate::rules::RetortNeed::Books(realm, _) = need {
                if r.realms.try_get(*realm).is_none() {
                    p.dangling(
                        "retorts.ron",
                        format!("retort {:?}", retort.id),
                        "requires",
                        "realm",
                        realm.num(),
                    );
                }
            }
        }
    }
    for realm in r.realms.defs() {
        for s in &realm.default_spells {
            if r.spells.try_get(*s).is_none() {
                p.dangling(
                    "realms.ron",
                    format!("realm {:?}", realm.id),
                    "default_spells",
                    "spell",
                    s.num(),
                );
            }
        }
    }
    const F: &str = "wizards.ron";
    for w in r.wizards.defs() {
        p.row(Some(w.num));
        let at = format!("wizard {:?}", w.id);
        let mut picks: u32 = 0;
        for (realm, n) in &w.books {
            if r.realms.try_get(*realm).is_none() {
                p.dangling(F, &at, "books", "realm", realm.num());
            }
            picks += u32::from(*n);
        }
        for &retort in &w.retorts {
            match r.retorts.try_get(retort) {
                Some(def) => picks += u32::from(def.cost),
                None => p.dangling(F, &at, "retorts", "retort", retort.num()),
            }
        }
        let budget = u32::from(crate::settings::PICKS);
        if picks != budget {
            p.out_of_range(
                F,
                &at,
                "picks spent on books and retorts",
                picks,
                format!("exactly {budget}"),
            );
        }
    }
    p.row(None);
}

fn check_level_paths(r: &Rules, p: &mut Issues) {
    const F: &str = "level_paths.ron";
    for path in r.level_paths.defs() {
        let who = format!("level path {:?}", path.id);
        match path.levels.first() {
            None => p.missing(F, &who, "levels"),
            Some(first) if first.xp != 0 => {
                p.out_of_range(F, &who, "the xp of the first level", first.xp, "0")
            }
            Some(_) => {}
        }
        if path.levels.windows(2).any(|w| w[1].xp <= w[0].xp) {
            p.conflict(F, &who, "the xp thresholds must strictly increase");
        }
    }
}

/// The file the rows of an owner (`race "orc"`, `level path "x" level 2`) are in.
fn file_of_owner(owner: &str) -> &'static str {
    [
        ("race ", "races.ron"),
        ("ability ", "abilities.ron"),
        ("building ", "buildings.ron"),
        ("retort ", "retorts.ron"),
        ("spell ", "spells.ron"),
        ("item ", "items.ron"),
        ("level path ", "level_paths.ron"),
    ]
    .iter()
    .find(|(prefix, _)| owner.starts_with(prefix))
    .map_or("", |(_, file)| file)
}

fn check_rule(r: &Rules, owner: &str, rule: &EffectRule, p: &mut Issues) {
    let file = file_of_owner(owner);
    check_condition(r, owner, &rule.condition, p);
    let ability = |a: &AbilityId, p: &mut Issues| {
        if r.abilities.try_get(*a).is_none() {
            p.dangling(file, owner, "effects", "ability", a.num());
        }
    };
    let realm = |m: &crate::rules::RealmId, p: &mut Issues| {
        if r.realms.try_get(*m).is_none() {
            p.dangling(file, owner, "effects", "realm", m.num());
        }
    };
    match &rule.effect {
        Effect::GrantAbility(a) => ability(a, p),
        Effect::Summon { unit, count } => {
            if r.units.try_get(*unit).is_none() {
                p.dangling(file, owner, "effects (summon)", "unit", unit.num());
            }
            if *count == 0 {
                p.out_of_range(file, owner, "the summon count", 0, "at least 1");
            }
        }
        Effect::Damage { amount, types, .. } => {
            if types.is_empty() {
                p.missing(file, owner, "the damage type");
            }
            if types.iter().filter(|t| t.realm_num().is_some()).count() > 1 {
                p.conflict(file, owner, "the damage has more than one realm");
            }
            if *amount <= 0 {
                p.out_of_range(file, owner, "the damage", amount, "above 0");
            }
        }
        Effect::Slay { realm: m, .. } => {
            realm(m, p);
        }
        Effect::Heal(n) if *n <= 0 => p.out_of_range(file, owner, "the heal", n, "above 0"),
        _ => {}
    }
}

fn check_condition(r: &Rules, owner: &str, c: &Condition, p: &mut Issues) {
    match c {
        Condition::RealmIs(x) if r.realms.try_get(*x).is_none() => {
            p.dangling(file_of_owner(owner), owner, "condition", "realm", x.num());
        }
        Condition::Not(c) => check_condition(r, owner, c, p),
        _ => {}
    }
}
