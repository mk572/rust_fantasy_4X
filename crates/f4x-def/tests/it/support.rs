//! What the tests share: a private copy of the shipped content to break one file of, and the loader.

use f4x_def::content::Content;
use f4x_def::load::{load_content, ContentError};
use f4x_def::testdir::{shipped_dir, TestDir};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

/// A new folder holding a copy of every file of the shipped content.
pub fn copy_of_shipped(label: &str) -> TestDir {
    let dir = TestDir::new(label);
    for entry in std::fs::read_dir(shipped_dir()).expect("the shipped content can be listed") {
        let path = entry.expect("an entry").path();
        if path.is_file() {
            let name = path.file_name().expect("a file name");
            std::fs::copy(&path, dir.join(name)).expect("the file can be copied");
        }
    }
    dir
}

/// Load the content of a folder.
pub fn load(dir: &Path) -> Result<Content, ContentError> {
    load_content(dir)
}

/// The text of a file of the copy.
pub fn read(dir: &Path, file: &str) -> String {
    std::fs::read_to_string(dir.join(file)).expect("the file is there")
}

/// Replace the file of the copy with `text`.
pub fn write(dir: &Path, file: &str, text: &str) {
    std::fs::write(dir.join(file), text).expect("the file can be written");
}

/// Change the rows of a file of the copy through their type and write the file again: the way to
/// break one thing in a file without knowing how it is spelled. Whatever `change` does, the file
/// stays what the loader would read, so the loader's own checks are what answers.
pub fn edit<T: DeserializeOwned + Serialize>(dir: &Path, file: &str, change: impl FnOnce(&mut T)) {
    let mut rows: T = ron::from_str(&read(dir, file)).expect("the shipped file parses as its type");
    change(&mut rows);
    let text = ron::ser::to_string_pretty(&rows, ron::ser::PrettyConfig::default())
        .expect("the rows can be written");
    write(dir, file, &text);
}

/// An issue as its parts, not its sentence: the file, the place and the values, separated by `|`.
/// A test asks whether a name is among them; the wording of the message is not part of the contract.
pub fn parts(i: &f4x_def::issue::ContentIssue) -> String {
    let mut out = vec![i.file.clone(), i.at.clone()];
    out.extend(i.detail.iter().cloned());
    out.join(" | ")
}

/// The problems a refused content names (each as its [`parts`]), or a failure when it was not
/// refused with a list.
pub fn problems(result: Result<Content, ContentError>) -> Vec<String> {
    match result {
        Err(e) if !e.issues.is_empty() => e.issues.iter().map(parts).collect(),
        Err(other) => panic!("expected a list of problems, got: {other}"),
        Ok(_) => panic!("the content was accepted but should have been refused"),
    }
}

/// Every reference of a loaded content that does not lead to a row, found by walking the content's
/// own types, without the loader's validator (the test must not trust what it checks). It is what
/// the attacks ask of content the loader accepted: whatever got through must still resolve, so
/// nothing in the game looks a row up and finds none.
pub fn dangling(c: &Content) -> Vec<String> {
    use f4x_def::rules::effects::{Condition, Effect, EffectRule};
    use f4x_def::rules::Def;
    let r = &c.rules;
    let mut out = Vec::new();
    let mut need = |ok: bool, what: String| {
        if !ok {
            out.push(what);
        }
    };
    let rules_of = |rules: &[EffectRule], owner: &str, need: &mut dyn FnMut(bool, String)| {
        fn cond(
            r: &f4x_def::rules::Rules,
            c: &Condition,
            owner: &str,
            need: &mut dyn FnMut(bool, String),
        ) {
            match c {
                Condition::RealmIs(x) => need(
                    r.realms.try_get(*x).is_some(),
                    format!("{owner}: realm {}", x.num()),
                ),
                Condition::Not(c) => cond(r, c, owner, need),
                _ => {}
            }
        }
        for rule in rules {
            cond(r, &rule.condition, owner, need);
            match &rule.effect {
                Effect::GrantAbility(a) => need(
                    r.abilities.try_get(*a).is_some(),
                    format!("{owner}: ability {}", a.num()),
                ),
                Effect::Summon { unit, .. } => need(
                    r.units.try_get(*unit).is_some(),
                    format!("{owner}: unit {}", unit.num()),
                ),
                Effect::Slay { realm, .. } => need(
                    r.realms.try_get(*realm).is_some(),
                    format!("{owner}: realm {}", realm.num()),
                ),
                _ => {}
            }
        }
    };
    for u in r.units.defs() {
        let who = format!("unit {}", u.id());
        if let Some(x) = u.race {
            need(
                r.races.try_get(x).is_some(),
                format!("{who}: race {}", x.num()),
            );
        }
        if let Some(x) = u.realm {
            need(
                r.realms.try_get(x).is_some(),
                format!("{who}: realm {}", x.num()),
            );
        }
        if let Some(x) = u.level_path {
            need(
                r.level_paths.try_get(x).is_some(),
                format!("{who}: level path {}", x.num()),
            );
        }
        for x in &u.abilities {
            need(
                r.abilities.try_get(*x).is_some(),
                format!("{who}: ability {}", x.num()),
            );
        }
        for x in &u.requires {
            need(
                r.buildings.try_get(*x).is_some(),
                format!("{who}: building {}", x.num()),
            );
        }
    }
    for x in r.races.defs() {
        let who = format!("race {}", x.id());
        for u in x.units.iter().chain(&x.start_army) {
            need(
                r.units.try_get(*u).is_some(),
                format!("{who}: unit {}", u.num()),
            );
        }
        for b in &x.buildings {
            need(
                r.buildings.try_get(*b).is_some(),
                format!("{who}: building {}", b.num()),
            );
        }
        rules_of(&x.effects, &who, &mut need);
    }
    for b in r.buildings.defs() {
        let who = format!("building {}", b.id());
        for x in b.requires.iter().chain(&b.replaces) {
            need(
                r.buildings.try_get(*x).is_some(),
                format!("{who}: building {}", x.num()),
            );
        }
        for x in &b.races {
            need(
                r.races.try_get(*x).is_some(),
                format!("{who}: race {}", x.num()),
            );
        }
        rules_of(&b.effects, &who, &mut need);
    }
    for s in r.spells.defs() {
        let who = format!("spell {}", s.id());
        need(
            r.realms.try_get(s.realm).is_some(),
            format!("{who}: realm {}", s.realm.num()),
        );
        for x in &s.not_with {
            need(
                r.spells.try_get(*x).is_some(),
                format!("{who}: spell {}", x.num()),
            );
        }
        for x in &s.already_able {
            need(
                r.abilities.try_get(*x).is_some(),
                format!("{who}: ability {}", x.num()),
            );
        }
        rules_of(&s.effects, &who, &mut need);
    }
    for a in r.abilities.defs() {
        rules_of(&a.effects, &format!("ability {}", a.id()), &mut need);
    }
    for x in r.items.defs() {
        let who = format!("item {}", x.id());
        for p in &x.powers {
            need(
                r.item_powers.try_get(*p).is_some(),
                format!("{who}: power {}", p.num()),
            );
        }
        if let Some((s, _)) = x.charges {
            need(
                r.spells.try_get(s).is_some(),
                format!("{who}: spell {}", s.num()),
            );
        }
        rules_of(&x.effects, &who, &mut need);
    }
    for p in r.item_powers.defs() {
        let who = format!("item power {}", p.id());
        if let Some(x) = p.realm {
            need(
                r.realms.try_get(x).is_some(),
                format!("{who}: realm {}", x.num()),
            );
        }
        if let Some(x) = p.spell {
            need(
                r.spells.try_get(x).is_some(),
                format!("{who}: spell {}", x.num()),
            );
        }
        if let Some(x) = p.attack {
            need(
                r.abilities.try_get(x).is_some(),
                format!("{who}: ability {}", x.num()),
            );
        }
    }
    for x in r.retorts.defs() {
        rules_of(&x.effects, &format!("retort {}", x.id()), &mut need);
    }
    for x in r.level_paths.defs() {
        for l in &x.levels {
            rules_of(&l.effects, &format!("level path {}", x.id()), &mut need);
        }
    }
    for w in r.wizards.defs() {
        let who = format!("wizard {}", w.id());
        for (realm, _) in &w.books {
            need(
                r.realms.try_get(*realm).is_some(),
                format!("{who}: realm {}", realm.num()),
            );
        }
        for x in &w.retorts {
            need(
                r.retorts.try_get(*x).is_some(),
                format!("{who}: retort {}", x.num()),
            );
        }
    }
    for x in r.realms.defs() {
        for s in &x.default_spells {
            need(
                r.spells.try_get(*s).is_some(),
                format!("realm {}: spell {}", x.id(), s.num()),
            );
        }
    }
    for t in &r.hero_templates {
        let who = format!("hero template {}", t.unit.num());
        need(r.units.try_get(t.unit).is_some(), format!("{who}: unit"));
        for a in &t.abilities {
            need(
                r.abilities.try_get(*a).is_some(),
                format!("{who}: ability {}", a.num()),
            );
        }
        for s in &t.spells {
            need(
                r.spells.try_get(*s).is_some(),
                format!("{who}: spell {}", s.num()),
            );
        }
    }
    for x in r
        .ratings
        .units
        .iter()
        .map(|u| u.num)
        .chain(r.efficiency.units.iter().map(|u| u.num))
    {
        need(
            r.units.by_num(x).is_some(),
            format!("rating or efficiency row: unit {x}"),
        );
    }
    for b in &r.config.autobuild_order {
        need(
            r.buildings.try_get(*b).is_some(),
            format!("config.autobuild_order: building {}", b.num()),
        );
    }
    for (_, b) in &r.config.starting_buildings {
        need(
            r.buildings.try_get(*b).is_some(),
            format!("config.starting_buildings: building {}", b.num()),
        );
    }
    for n in &r.config.limited_ai_races {
        need(
            r.races.by_num(*n).is_some(),
            format!("config.limited_ai_races: race {n}"),
        );
    }
    for &(a, b) in &r.config.mergeable_realms {
        for n in [a, b] {
            need(
                r.realms.id_numbered(n).is_some(),
                format!("config.mergeable_realms: realm {n}"),
            );
        }
    }
    for n in &r.config.event_ores {
        need(
            c.features.defs().iter().any(|f| f.num == *n),
            format!("config.event_ores: feature {n}"),
        );
    }
    for f in c.features.defs() {
        let who = format!("feature {}", f.id);
        for x in f.realm.iter().chain(&f.reward) {
            need(
                r.realms.try_get(*x).is_some(),
                format!("{who}: realm {}", x.num()),
            );
        }
        if let Some(n) = f.transmute {
            need(
                c.features.defs().iter().any(|o| o.num == n),
                format!("{who}: transmute feature {n}"),
            );
        }
    }
    out
}

/// Where the numbers are in a text: not inside a string or a comment.
pub fn numbers_in(text: &str) -> Vec<(usize, usize)> {
    let b = text.as_bytes();
    let (mut out, mut i, mut in_str) = (Vec::new(), 0, false);
    while i < b.len() {
        let c = b[i];
        if in_str {
            if c == b'\\' {
                i += 1;
            } else if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            in_str = true;
            i += 1;
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
        } else if c.is_ascii_digit()
            && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_'))
        {
            let start = if i > 0 && b[i - 1] == b'-' { i - 1 } else { i };
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            out.push((start, i));
        } else {
            i += 1;
        }
    }
    out
}
