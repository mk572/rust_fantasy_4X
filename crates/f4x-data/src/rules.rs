//! Loading and cross-reference validation of the rules definitions.

use std::collections::{BTreeMap, HashSet};

use f4x_core::content::Features;
use f4x_core::rules::effects::{Condition, Effect, EffectRule, Trigger, CUSTOM_HOOKS};
use f4x_core::rules::{Def, Duration, Registry, Rules, Stat, UnitKind};

use crate::id_problem;

fn check_ids<T: Def>(kind: &str, reg: &Registry<T>, problems: &mut Vec<String>) {
    let mut seen = HashSet::new();
    for d in reg.defs() {
        problems.extend(id_problem(kind, d.id()));
        if !seen.insert(d.id()) {
            problems.push(format!("duplicate {kind} id {:?}", d.id()));
        }
    }
}

/// Returns a human-readable list of problems; empty means valid.
pub fn validate_rules(r: &Rules, features: &Features) -> Vec<String> {
    let mut p = Vec::new();
    check_ids("race", &r.races, &mut p);
    check_ids("unit", &r.units, &mut p);
    check_ids("ability", &r.abilities, &mut p);
    check_ids("building", &r.buildings, &mut p);
    check_ids("realm", &r.realms, &mut p);
    check_ids("spell", &r.spells, &mut p);
    check_ids("item", &r.items, &mut p);
    check_ids("level path", &r.level_paths, &mut p);

    check_races(r, &mut p);
    check_units(r, &mut p);
    check_buildings(r, &mut p);
    check_spells(r, &mut p);
    check_level_paths(r, &mut p);
    for item in r.items.defs() {
        if item.effects.is_empty() {
            p.push(format!("item {:?} has no effects", item.id));
        }
    }

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
        for level in &path.levels {
            carriers.push((
                format!("level path {:?} level {:?}", path.id, level.name),
                &level.effects[..],
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
        let has_unit = r
            .units
            .defs()
            .iter()
            .any(|u| u.kind == UnitKind::Normal && u.race.as_deref() == Some(race.id.as_str()));
        if !has_unit {
            p.push(format!("race {:?} has no units", race.id));
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
        if let Some(race) = &u.race {
            if r.races.by_name(race).is_none() {
                p.push(format!("{who} has unknown race {race:?}"));
            }
        }
        if let Some(realm) = &u.realm {
            if r.realms.by_name(realm).is_none() {
                p.push(format!("{who} has unknown realm {realm:?}"));
            }
        }
        for a in &u.abilities {
            if r.abilities.by_name(a).is_none() {
                p.push(format!("{who} has unknown ability {a:?}"));
            }
        }
        for b in &u.requires {
            match r.buildings.by_name(b) {
                None => p.push(format!("{who} requires unknown building {b:?}")),
                Some(bd) => {
                    if let Some(race) = &u.race {
                        if !bd.races.is_empty() && !bd.races.contains(race) {
                            p.push(format!(
                                "{who} requires building {b:?}, which its race cannot build"
                            ));
                        }
                    }
                }
            }
        }
        if let Some(path) = &u.level_path {
            if r.level_paths.by_name(path).is_none() {
                p.push(format!("{who} has unknown level path {path:?}"));
            }
        }
        match u.kind {
            UnitKind::Normal => {
                if u.race.is_none() {
                    p.push(format!("{who} is Normal and needs a race"));
                }
                if u.hero.is_some() {
                    p.push(format!("{who} is Normal but has hero data"));
                }
            }
            UnitKind::Hero => {
                if u.race.is_some() {
                    p.push(format!("{who} is a Hero and must not have a race"));
                }
                match &u.hero {
                    None => p.push(format!("{who} is a Hero and needs hero data")),
                    Some(h) => {
                        let unique: HashSet<_> = h.slots.iter().collect();
                        if h.slots.is_empty() || unique.len() != h.slots.len() {
                            p.push(format!("{who}: hero slots must be non-empty and unique"));
                        }
                    }
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
        for req in &b.requires {
            if r.buildings.by_name(req).is_none() {
                p.push(format!(
                    "building {:?} requires unknown building {req:?}",
                    b.id
                ));
            }
        }
        for race in &b.races {
            if r.races.by_name(race).is_none() {
                p.push(format!("building {:?} lists unknown race {race:?}", b.id));
            }
        }
    }
    // Requirement cycles: depth-first search with white/grey/black colouring.
    let graph: BTreeMap<&str, Vec<&str>> = r
        .buildings
        .defs()
        .iter()
        .map(|b| {
            (
                b.id.as_str(),
                b.requires.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    let mut state: BTreeMap<&str, u8> = BTreeMap::new(); // 1 = on stack, 2 = done
    fn visit<'a>(
        n: &'a str,
        g: &BTreeMap<&'a str, Vec<&'a str>>,
        state: &mut BTreeMap<&'a str, u8>,
        p: &mut Vec<String>,
    ) {
        match state.get(n) {
            Some(2) => return,
            Some(_) => {
                p.push(format!("building requirement cycle involving {n:?}"));
                return;
            }
            None => {}
        }
        state.insert(n, 1);
        for &m in g.get(n).into_iter().flatten() {
            if g.contains_key(m) {
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
        if r.realms.by_name(&s.realm).is_none() {
            p.push(format!("{who} has unknown realm {:?}", s.realm));
        }
        if s.cost == 0 || s.research_cost == 0 {
            p.push(format!("{who} needs a nonzero cost and research_cost"));
        }
        if s.contexts.is_empty() {
            p.push(format!("{who} can be cast in no context"));
        }
        if s.effects.is_empty() {
            p.push(format!("{who} has no effects"));
        }
        if s.duration == Duration::Turns(0) {
            p.push(format!("{who} has a duration of 0 turns"));
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
    let ability = |name: &str, p: &mut Vec<String>| {
        if r.abilities.by_name(name).is_none() {
            p.push(format!("{owner}: unknown ability {name:?}"));
        }
    };
    match &rule.effect {
        Effect::GrantAbility(a) | Effect::RemoveAbility(a) => ability(a, p),
        Effect::Summon { unit, count } => {
            if r.units.by_name(unit).is_none() {
                p.push(format!("{owner}: summons unknown unit {unit:?}"));
            }
            if *count == 0 {
                p.push(format!("{owner}: summon count must be at least 1"));
            }
        }
        Effect::SetFeature(f) => {
            if features.id_of(f).is_none() {
                p.push(format!("{owner}: unknown feature {f:?}"));
            }
        }
        Effect::ApplyEnchantment(s) => match r.spells.by_name(s) {
            None => p.push(format!("{owner}: applies unknown enchantment {s:?}")),
            Some(sd) if sd.duration == Duration::Instant => {
                p.push(format!(
                    "{owner}: {s:?} is an instant spell, not an enchantment"
                ));
            }
            Some(_) => {}
        },
        Effect::Damage { amount, .. } if *amount <= 0 => {
            p.push(format!("{owner}: damage must be positive"))
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
        Condition::HasAbility(a) if r.abilities.by_name(a).is_none() => {
            p.push(format!("{owner}: condition names unknown ability {a:?}"));
        }
        Condition::RaceIs(x) if r.races.by_name(x).is_none() => {
            p.push(format!("{owner}: condition names unknown race {x:?}"));
        }
        Condition::RealmIs(x) if r.realms.by_name(x).is_none() => {
            p.push(format!("{owner}: condition names unknown realm {x:?}"));
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
    use crate::load_content;
    use crate::testutil::{base_dir, has, load_edited_base, problems_of};
    use f4x_core::rules::stats::{compute, passive_modifiers};
    use f4x_core::rules::{Source, Stat};

    #[test]
    fn shipped_rules_are_valid() {
        assert_eq!(problems_of(&[]), Vec::<String>::new());
        let c = load_content(&base_dir()).unwrap();
        assert_eq!(c.rules.realms.len(), 5);
        assert!(c
            .rules
            .units
            .by_name("captain_mira")
            .unwrap()
            .hero
            .is_some());
    }

    #[test]
    fn typed_ids_round_trip() {
        let c = load_content(&base_dir()).unwrap();
        let id = c.rules.units.id_of("archer").unwrap();
        assert_eq!(c.rules.units.get(id).name, "Archer");
        assert!(c.rules.units.id_of("nope").is_none());
    }

    #[test]
    fn catches_unknown_references() {
        let p = problems_of(&[
            (
                "units.ron",
                "abilities: [\"armoured\"]",
                "abilities: [\"nope\"]",
            ),
            (
                "spells.ron",
                "realm: \"chaos\", rarity: Common",
                "realm: \"void\", rarity: Common",
            ),
            (
                "units.ron",
                "race: Some(\"ironclans\"), cost: 55",
                "race: Some(\"ghosts\"), cost: 55",
            ),
        ]);
        assert!(has(&p, "unknown ability \"nope\""), "{p:?}");
        assert!(has(&p, "unknown realm \"void\""), "{p:?}");
        assert!(has(&p, "unknown race \"ghosts\""), "{p:?}");
    }

    #[test]
    fn catches_unit_rule_violations() {
        let p = problems_of(&[
            ("units.ron", "hero: Some((slots: [Weapon, Armor, Misc], hire_cost: 150))", "hero: None"),
            ("units.ron", "kind: Summoned, realm: Some(\"chaos\")", "kind: Summoned"),
            ("units.ron", "Ranged: 2, Ammo: 4", "Ranged: 2"),
            (
                "units.ron",
                "Figures: 4, Moves: 1, Sight: 1},\n     movement: Walking, abilities: [\"armoured\"]",
                "Figures: 0, Moves: 1, Sight: 1},\n     movement: Walking, abilities: [\"armoured\"]",
            ),
        ]);
        assert!(has(&p, "needs hero data"), "{p:?}");
        assert!(has(&p, "Summoned and needs a realm"), "{p:?}");
        assert!(has(&p, "ranged attack but no ammo"), "{p:?}");
        assert!(has(&p, "Figures must be at least 1"), "{p:?}");
    }

    #[test]
    fn catches_building_problems() {
        let p = problems_of(&[
            (
                "buildings.ron",
                "requires: [\"barracks\"], effects: [\n        (target: City, effect: Modify(stat: UnitCostPercent",
                "requires: [\"forge\"], effects: [\n        (target: City, effect: Modify(stat: UnitCostPercent",
            ),
            ("buildings.ron", "requires: [\"barracks\"], races: [\"ironclans\"]", "requires: [\"armoury\"], races: [\"ironclans\"]"),
        ]);
        assert!(has(&p, "requirement cycle"), "{p:?}");
        let q = problems_of(&[(
            "units.ron",
            "requires: [\"barracks\"], level_path: Some(\"standard_xp\")),\n    (id: \"marksman\"",
            "requires: [\"granary\"], level_path: Some(\"standard_xp\")),\n    (id: \"marksman\"",
        )]);
        assert!(has(&q, "which its race cannot build"), "{q:?}");
    }

    #[test]
    fn catches_spell_and_effect_problems() {
        let p = problems_of(&[
            (
                "spells.ron",
                "duration: Turns(3),\n     effects: [(effect: Modify(stat: Defense",
                "duration: Instant,\n     effects: [(effect: Modify(stat: Defense",
            ),
            (
                "spells.ron",
                "Summon(unit: \"fire_sprite\", count: 1)",
                "Summon(unit: \"fire_sprite\", count: 0)",
            ),
            ("spells.ron", "Heal(8)", "Custom(\"mystery\")"),
            ("abilities.ron", "Heal(2)", "ApplyEnchantment(\"fireball\")"),
        ]);
        assert!(has(&p, "instant but has Passive"), "{p:?}");
        assert!(has(&p, "summon count"), "{p:?}");
        assert!(has(&p, "unknown custom hook"), "{p:?}");
        assert!(has(&p, "instant spell, not an enchantment"), "{p:?}");
    }

    #[test]
    fn catches_bad_level_paths() {
        let p = problems_of(&[(
            "level_paths.ron",
            "xp: 20, effects: [(effect",
            "xp: 0, effects: [(effect",
        )]);
        assert!(has(&p, "strictly increase"), "{p:?}");
    }

    #[test]
    fn veteran_spearman_stats_come_from_def_plus_level_modifiers() {
        let c = load_edited_base(&[]).unwrap();
        let rules = &c.rules;
        let unit = rules.units.by_name("spearman").unwrap();
        let path = rules
            .level_paths
            .by_name(unit.level_path.as_deref().unwrap())
            .unwrap();
        let mods = passive_modifiers(&path.levels[2].effects, Source::Level(2));
        assert_eq!(compute(Stat::Melee, unit.stats[&Stat::Melee], &mods), 4);
        assert_eq!(compute(Stat::Defense, unit.stats[&Stat::Defense], &mods), 4);
        let spell = rules.spells.by_name("stoneskin").unwrap();
        let id = rules.spells.id_of("stoneskin").unwrap();
        let mut all = mods.clone();
        all.extend(passive_modifiers(&spell.effects, Source::Spell(id)));
        assert_eq!(compute(Stat::Defense, unit.stats[&Stat::Defense], &all), 6);
    }
}
