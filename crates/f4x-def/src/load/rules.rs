//! Loading and cross-reference validation of the rules definitions.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use crate::rules::effects::{Condition, Effect, EffectRule, Trigger};
use crate::rules::{
    AbilityDef, AbilityId, BuildingDef, Def, Duration, HeroTemplate, ItemDef, ItemPowerDef,
    LevelPathDef, RaceDef, RealmDef, Registry, RetortDef, Rules, RulesConfig, SpellDef, Stat,
    UnitDef, UnitKind, WizardDef,
};

use super::{id_problem, ContentError};

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

/// Load and validate every rules file. Each file comes from the first of `dirs` that has it,
/// so a local directory can replace single files of the committed one.
pub fn load_rules(dirs: &[&Path]) -> Result<Rules, ContentError> {
    let dir = dirs;
    let races = parse::<Vec<RaceDef>>(dir, "races.ron")?;
    let hero_templates = if find(dir, "heroes.ron").is_some() {
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
    let problems = Vec::new();
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
        // The lair rules, also optional: a rule set without the file uses the original's.
        lairs: if find(dir, "lairs.ron").is_some() {
            parse::<crate::rules::lair::LairRules>(dir, "lairs.ron")?
        } else {
            crate::rules::lair::LairRules::default()
        },
    };
    let problems = validate_rules(&rules);
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
            if !crate::hero::HeroAbilities::is_hero_ability(a) {
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
        if !seen.contains(&crate::rules::UnitId::new(u.num)) {
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
pub fn validate_rules(r: &Rules) -> Vec<String> {
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
                            | Stat::ManaIncome
                            | Stat::ResearchIncome
                            | Stat::CastingSkill
                            | Stat::StartingSkill
                            | Stat::SkillGainPercent
                            | Stat::DispelMultiplier
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
                p.push(format!(
                    "retort {:?}: only passive changes to the wizard stats (SpellCostPercent, SpellUpkeepPercent, ManaIncome, ResearchIncome, CastingSkill and the others a wizard has) are understood",
                    retort.id
                ));
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
        for rule in rules {
            check_rule(r, &owner, rule, &mut p);
        }
    }
    p
}

fn check_races(r: &Rules, p: &mut Vec<String>) {
    for race in r.races.defs() {
        let this = crate::rules::RaceId::new(race.num);
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
                        let listed = r
                            .races
                            .try_get(*race)
                            .is_some_and(|rd| rd.units.contains(&crate::rules::UnitId::new(u.num)));
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
            if let crate::rules::RetortNeed::Books(realm, _) = need {
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
        let budget = u32::from(crate::settings::PICKS);
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

fn check_rule(r: &Rules, owner: &str, rule: &EffectRule, p: &mut Vec<String>) {
    check_condition(r, owner, &rule.condition, p);
    let ability = |a: &AbilityId, p: &mut Vec<String>| {
        if r.abilities.try_get(*a).is_none() {
            p.push(format!("{owner}: unknown ability {}", a.num()));
        }
    };
    let realm = |m: &crate::rules::RealmId, p: &mut Vec<String>| {
        if r.realms.try_get(*m).is_none() {
            p.push(format!("{owner}: unknown realm {}", m.num()));
        }
    };
    match &rule.effect {
        Effect::GrantAbility(a) => ability(a, p),
        Effect::Summon { unit, count } => {
            if r.units.try_get(*unit).is_none() {
                p.push(format!("{owner}: summons unknown unit {}", unit.num()));
            }
            if *count == 0 {
                p.push(format!("{owner}: summon count must be at least 1"));
            }
        }
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
        _ => {}
    }
}

fn check_condition(r: &Rules, owner: &str, c: &Condition, p: &mut Vec<String>) {
    match c {
        Condition::RealmIs(x) if r.realms.try_get(*x).is_none() => {
            p.push(format!(
                "{owner}: condition names unknown realm {}",
                x.num()
            ));
        }
        Condition::Not(c) => check_condition(r, owner, c, p),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::stats::{compute, passive_modifiers};
    use crate::rules::Source;
    use crate::testdir::TestResult;

    #[test]
    fn the_committed_wizards_have_their_names_and_books() -> TestResult {
        let dir = crate::load::testutil::content_dir();
        let rules = load_rules(&[dir.as_path()])?;
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
        crate::load::testutil::content_dir()
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
        let races: Vec<crate::rules::RaceDef> = shipped("races.ron", edits);
        let units: Vec<UnitDef> = shipped("units.ron", edits);
        let abilities: Vec<crate::rules::AbilityDef> = shipped("abilities.ron", edits);
        let buildings: Vec<crate::rules::BuildingDef> = shipped("buildings.ron", edits);
        let realms: Vec<RealmDef> = shipped("realms.ron", edits);
        let retorts: Vec<RetortDef> = shipped("retorts.ron", edits);
        let spells: Vec<SpellDef> = shipped("spells.ron", edits);
        let item_powers: Vec<ItemPowerDef> = shipped("item_powers.ron", edits);
        let level_paths: Vec<LevelPathDef> = shipped("level_paths.ron", edits);
        let wizards: Vec<crate::rules::WizardDef> = Vec::new();
        let items: Vec<crate::rules::ItemDef> = shipped("items.ron", edits);
        let hero_templates: Vec<crate::rules::HeroTemplate> = Vec::new();
        let config: crate::rules::RulesConfig = shipped("rules.ron", edits);
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
            lairs: shipped("lairs.ron", edits),
        }
    }

    /// The shipped definitions, to change one by one.
    struct Lists {
        races: Vec<crate::rules::RaceDef>,
        units: Vec<crate::rules::UnitDef>,
        abilities: Vec<crate::rules::AbilityDef>,
        buildings: Vec<crate::rules::BuildingDef>,
        spells: Vec<crate::rules::SpellDef>,
        items: Vec<crate::rules::ItemDef>,
        item_powers: Vec<crate::rules::ItemPowerDef>,
        level_paths: Vec<crate::rules::LevelPathDef>,
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
        validate_rules(&rules)
    }

    fn problems(edits: &[(&str, &str, &str)]) -> Vec<String> {
        validate_rules(&rules_with(edits))
    }

    fn has(problems: &[String], needle: &str) -> bool {
        problems.iter().any(|m| m.contains(needle))
    }

    #[test]
    fn the_shipped_lair_table_is_the_default_and_bad_ones_are_refused() {
        assert_eq!(
            rules_with(&[]).lairs,
            crate::rules::lair::LairRules::default()
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
        let rules = load_rules(&[&content_dir()])?;
        for realm in 0..=4 {
            assert!(rules.realms.by_num(realm).is_some(), "{realm}");
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
        let rules = load_rules(&[&content_dir()])?;
        let id = rules.units.id_numbered(39).ok_or("the value is missing")?;
        assert_eq!(rules.units.get(id).id, "barbarian_spearmen");
        assert!(rules.units.by_num(9999).is_none());
        Ok(())
    }

    #[test]
    fn catches_item_problems() {
        let p = problems_after(|l| {
            let item = find(&mut l.items, "black_asp", |i| &i.id);
            item.slot = crate::rules::ItemSlot::Armor;
            item.powers = vec![crate::rules::ItemPowerId::new(9990)];
            item.charges = Some((crate::rules::SpellId::new(9991), 0));
            find(&mut l.item_powers, "flaming", |i| &i.id).realm =
                Some(crate::rules::RealmId::new(9992));
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
                .push(crate::rules::Ability::new(9990));
            find(&mut l.spells, "web", |s| &s.id).realm = crate::rules::RealmId::new(9991);
            let race = find(&mut l.races, "barbarian", |r| &r.id);
            // The barbarians' bowmen (unit 41) become a unit nobody has.
            let bowmen = race.units.iter_mut().find(|u| u.num() == 41);
            *bowmen.expect("the barbarians build bowmen") = crate::rules::UnitId::new(9992);
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
        use crate::rules::{HeroTemplate, PickType};
        let mut rules = rules_with(&[]);
        let template = |unit: u16| HeroTemplate {
            unit: crate::rules::UnitId::new(unit),
            picks: 1,
            pick_type: PickType::Any,
            abilities: vec![crate::rules::Ability::new(85 /* might */)],
            casting_skill: 0,
            spells: Vec::new(),
            gender: Default::default(),
        };
        // A hero without a template, once there are templates at all.
        rules.hero_templates = vec![template(39 /* barbarian_spearmen */), template(9990)];
        rules.hero_templates[0]
            .abilities
            .push(crate::rules::Ability::new(24 /* flying */));
        // Flying is an ability, but not a hero one.
        rules.hero_templates[0]
            .spells
            .push(crate::rules::SpellId::new(9991));
        let p = validate_rules(&rules);
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
                Some(crate::rules::BuildingId::new(9990));
        });
        assert!(has(&p, "replaces unknown building 9990"), "{p:?}");
    }

    #[test]
    fn catches_building_problems() -> TestResult {
        // The armory needs the barracks; make the barracks need the armory.
        let p = problems_after(|l| {
            find(&mut l.buildings, "barracks", |b| &b.id).requires =
                vec![crate::rules::BuildingId::new(4 /* armory */)];
        });
        assert!(has(&p, "requirement cycle"), "{p:?}");
        // A unit that needs a building its race cannot build.
        let q = problems_after(|l| {
            let walls = find(&mut l.buildings, "armorers_guild", |b| &b.id);
            let guild = crate::rules::BuildingId::new(walls.num);
            let gnoll = crate::rules::RaceId::new(5 /* gnoll */);
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
        use crate::rules::effects::Effect;
        use crate::rules::Duration;
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
        });
        assert!(has(&p, "instant but has Passive"), "{p:?}");
        assert!(has(&p, "summon count"), "{p:?}");
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
        use crate::rules::effects::Effect;
        use crate::rules::ModOp;
        // What a list of effects adds to a stat.
        let added = |effects: &[crate::rules::effects::EffectRule], stat: Stat| -> i32 {
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
        let rules = load_rules(&[&content_dir()])?;
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

    /// Which races can build each building, as the wiki's "Urban Tech by Race" table gives it
    /// (`reference/Races _ Master of Magic Wiki _ Fandom.html`; it counts the prerequisite
    /// buildings in, so a University shows the 7 races that can have a Sages' Guild and a
    /// University). A building not listed has no race restriction. The one cell not followed: the
    /// table lets Gnolls build a Sages' Guild, but it has no Gnoll Alchemists' Guild or University,
    /// which need one, and the original's table (`RACETYPE.c`) bars Gnolls from it.
    #[test]
    fn the_races_that_can_build_each_building_follow_the_wiki() -> TestResult {
        let rules = &crate::testdir::shipped().rules;
        let wiki: &[(&str, &[&str])] = &[
            (
                "sawmill",
                &[
                    "barbarian",
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "dwarven",
                    "gnoll",
                    "halfling",
                    "high_elf",
                    "high_men",
                    "klackon",
                    "nomad",
                    "orc",
                    "troll",
                ],
            ),
            (
                "foresters_guild",
                &[
                    "barbarian",
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "dwarven",
                    "gnoll",
                    "halfling",
                    "high_elf",
                    "high_men",
                    "klackon",
                    "nomad",
                    "orc",
                    "troll",
                ],
            ),
            (
                "miners_guild",
                &[
                    "barbarian",
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "dwarven",
                    "gnoll",
                    "halfling",
                    "high_elf",
                    "high_men",
                    "klackon",
                    "nomad",
                    "orc",
                ],
            ),
            (
                "mechanicians_guild",
                &[
                    "beastmen", "dark_elf", "high_elf", "high_men", "nomad", "orc",
                ],
            ),
            (
                "bank",
                &[
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "high_elf",
                    "high_men",
                    "nomad",
                    "orc",
                ],
            ),
            (
                "merchants_guild",
                &[
                    "dark_elf",
                    "draconian",
                    "high_elf",
                    "high_men",
                    "nomad",
                    "orc",
                ],
            ),
            (
                "armorers_guild",
                &[
                    "barbarian",
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "dwarven",
                    "high_elf",
                    "high_men",
                    "klackon",
                    "lizardman",
                    "nomad",
                    "orc",
                    "troll",
                ],
            ),
            (
                "war_college",
                &[
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "high_elf",
                    "high_men",
                    "nomad",
                    "orc",
                ],
            ),
            (
                "temple",
                &[
                    "barbarian",
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "dwarven",
                    "gnoll",
                    "halfling",
                    "high_elf",
                    "high_men",
                    "lizardman",
                    "nomad",
                    "orc",
                    "troll",
                ],
            ),
            (
                "parthenon",
                &[
                    "barbarian",
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "halfling",
                    "high_men",
                    "nomad",
                    "orc",
                    "troll",
                ],
            ),
            (
                "cathedral",
                &[
                    "beastmen",
                    "draconian",
                    "halfling",
                    "high_men",
                    "nomad",
                    "orc",
                    "troll",
                ],
            ),
            (
                "oracle",
                &[
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "high_men",
                    "nomad",
                    "orc",
                ],
            ),
            (
                "sages_guild",
                &[
                    "barbarian",
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "dwarven",
                    "halfling",
                    "high_elf",
                    "high_men",
                    "nomad",
                    "orc",
                ],
            ),
            (
                "alchemists_guild",
                &[
                    "barbarian",
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "dwarven",
                    "halfling",
                    "high_elf",
                    "high_men",
                    "nomad",
                    "orc",
                ],
            ),
            (
                "university",
                &[
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "high_elf",
                    "high_men",
                    "nomad",
                    "orc",
                ],
            ),
            (
                "wizards_guild",
                &[
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "high_elf",
                    "high_men",
                    "orc",
                ],
            ),
            (
                "stables",
                &[
                    "barbarian",
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "gnoll",
                    "high_elf",
                    "high_men",
                    "klackon",
                    "lizardman",
                    "nomad",
                    "orc",
                    "troll",
                ],
            ),
            (
                "animists_guild",
                &[
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "high_elf",
                    "high_men",
                    "nomad",
                    "orc",
                    "troll",
                ],
            ),
            (
                "fantastic_stable",
                &["dark_elf", "high_elf", "nomad", "orc"],
            ),
            (
                "ship_wrights_guild",
                &[
                    "barbarian",
                    "beastmen",
                    "dark_elf",
                    "draconian",
                    "dwarven",
                    "gnoll",
                    "halfling",
                    "high_elf",
                    "high_men",
                    "klackon",
                    "nomad",
                    "orc",
                    "troll",
                ],
            ),
            (
                "ship_yard",
                &[
                    "barbarian",
                    "dark_elf",
                    "draconian",
                    "gnoll",
                    "high_elf",
                    "high_men",
                    "nomad",
                    "orc",
                ],
            ),
            ("maritime_guild", &["barbarian", "high_men", "orc"]),
        ];
        // The races that can build a building: its own list, and for each building it needs, theirs.
        fn builders(rules: &Rules, b: &BuildingDef) -> Vec<String> {
            let mut races: Vec<String> = rules
                .races
                .defs()
                .iter()
                .filter(|r| {
                    b.buildable_by(
                        rules
                            .races
                            .id_numbered(r.num)
                            .expect("the race is in the table"),
                    )
                })
                .map(|r| r.id.clone())
                .collect();
            for need in &b.requires {
                let needed = builders(rules, rules.buildings.get(*need));
                races.retain(|r| needed.contains(r));
            }
            races
        }
        for (id, expected) in wiki {
            let def = rules
                .buildings
                .defs()
                .iter()
                .find(|b| b.id == *id)
                .ok_or_else(|| format!("building {id} is missing"))?;
            let mut got = builders(rules, def);
            got.sort();
            let mut want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
            want.sort();
            assert_eq!(got, want, "the races that can build the {id}");
        }
        Ok(())
    }
}
