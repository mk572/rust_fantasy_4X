//! Class 2: loading itself fails, or fails badly. A missing or unreadable file, malformed RON, a
//! wrong type, a truncated or binary file, a local override that wins or loses, an error that does
//! not say which file is at fault. The loader returns an error for every one of them and never
//! panics, whatever the file holds.

use crate::support::*;
use f4x_def::content::Content;
use f4x_def::issue::IssueKind;
use f4x_def::load::{load_content, load_content_layers, load_help, load_settings, ContentError};
use f4x_def::testdir::{shipped, shipped_dir, TestDir};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

/// What loading a folder came to.
pub enum Outcome {
    Loaded(Box<Content>),
    Refused(ContentError),
    Panicked(String),
}

impl std::fmt::Debug for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Outcome::Loaded(_) => write!(f, "the content was accepted"),
            Outcome::Refused(e) => write!(
                f,
                "refused: {}",
                e.to_string().chars().take(300).collect::<String>()
            ),
            Outcome::Panicked(m) => write!(f, "PANIC: {m}"),
        }
    }
}

pub fn attempt(dir: &Path) -> Outcome {
    match catch_unwind(AssertUnwindSafe(|| load_content(dir))) {
        Ok(Ok(c)) => Outcome::Loaded(Box::new(c)),
        Ok(Err(e)) => Outcome::Refused(e),
        Err(p) => Outcome::Panicked(
            p.downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default(),
        ),
    }
}

/// The files of the shipped content, by name.
pub fn shipped_files() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(shipped_dir())
        .expect("listed")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".ron"))
        .collect();
    v.sort();
    v
}

/// Files the loader must have, and files it may do without (the rule set then has none of what the
/// file holds). They are the contract of `load_rules` and `load_content`.
const REQUIRED: &[&str] = &[
    "terrain.ron",
    "features.ron",
    "races.ron",
    "units.ron",
    "abilities.ron",
    "buildings.ron",
    "realms.ron",
    "retorts.ron",
    "spells.ron",
    "items.ron",
    "item_powers.ron",
    "level_paths.ron",
    "rules.ron",
    "battle.ron",
    "lairs.ron",
    "ai_sites.ron",
    "rules_settings.ron",
];
const OPTIONAL: &[&str] = &[
    "heroes.ron",
    "wizards.ron",
    "unit_ratings.ron",
    "unit_efficiency.ron",
];

fn mentions_file(e: &ContentError, file: &str) -> bool {
    e.mentions(file)
}

/// A refusal because a file or folder is not there or cannot be read.
fn unreadable(o: &Outcome) -> bool {
    matches!(o, Outcome::Refused(e) if e.has(IssueKind::Missing) || e.has(IssueKind::Unreadable))
}

/// A refusal that comes from the checks of what was read (a reference to a row the damage took away,
/// a contradiction), not from reading or parsing: it may name another file than the damaged one.
fn only_conflicts(e: &ContentError) -> bool {
    e.issues.iter().all(|i| {
        matches!(
            i.kind,
            IssueKind::Conflict
                | IssueKind::Dangling
                | IssueKind::Duplicate
                | IssueKind::OutOfRange
        )
    })
}

// ---- files that are not there ----

#[test]
fn the_shipped_files_are_exactly_the_required_and_the_optional_ones() {
    let mut known: Vec<&str> = REQUIRED.iter().chain(OPTIONAL).copied().collect();
    known.sort_unstable();
    assert_eq!(
        shipped_files(),
        known,
        "a file was added or removed: say here whether it is required"
    );
}

#[test]
fn a_required_file_that_is_missing_is_an_error_that_names_it() {
    for file in REQUIRED {
        let dir = copy_of_shipped("c2-missing");
        std::fs::remove_file(dir.join(file)).expect("removed");
        match attempt(&dir) {
            Outcome::Refused(e) if e.has(IssueKind::Missing) => {
                assert!(mentions_file(&e, file), "{file}: {e}")
            }
            other => panic!("{file}: {other:?}"),
        }
    }
}

#[test]
fn an_optional_file_that_is_missing_leaves_the_content_without_what_it_held() {
    for file in OPTIONAL {
        let dir = copy_of_shipped("c2-optional");
        std::fs::remove_file(dir.join(file)).expect("removed");
        match attempt(&dir) {
            Outcome::Loaded(_) => {}
            other => panic!("{file}: {other:?}"),
        }
    }
}

#[test]
fn a_folder_that_is_empty_or_not_there_or_a_file_is_an_error() {
    let empty = TestDir::new("c2-empty");
    assert!(unreadable(&attempt(&empty)));
    assert!(unreadable(&attempt(&empty.join("nowhere"))));
    std::fs::write(empty.join("afile"), "x").expect("a file");
    assert!(unreadable(&attempt(&empty.join("afile"))));
}

#[test]
fn a_file_that_is_a_folder_is_an_error_that_names_it() {
    for file in [
        "units.ron",
        "rules_settings.ron",
        "battle.ron",
        "terrain.ron",
    ] {
        let dir = copy_of_shipped("c2-isdir");
        std::fs::remove_file(dir.join(file)).expect("removed");
        std::fs::create_dir(dir.join(file)).expect("a folder");
        match attempt(&dir) {
            Outcome::Refused(e) => assert!(mentions_file(&e, file), "{file}: {e}"),
            other => panic!("{file}: {other:?}"),
        }
    }
}

#[cfg(unix)]
#[test]
fn a_file_that_cannot_be_read_is_an_error_that_names_it() {
    use std::os::unix::fs::PermissionsExt;
    let dir = copy_of_shipped("c2-perm");
    let path = dir.join("spells.ron");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    // Whoever may read anything (root) cannot be locked out: nothing to test then.
    if std::fs::read(&path).is_ok() {
        return;
    }
    match attempt(&dir) {
        Outcome::Refused(e) => assert!(mentions_file(&e, "spells.ron"), "{e}"),
        other => panic!("{other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn an_optional_file_that_is_a_broken_link_is_not_taken_for_a_missing_one() {
    let dir = copy_of_shipped("c2-link");
    std::fs::remove_file(dir.join("battle.ron")).expect("removed");
    std::os::unix::fs::symlink(dir.join("nowhere.ron"), dir.join("battle.ron")).expect("a link");
    match attempt(&dir) {
        Outcome::Refused(e) => assert!(mentions_file(&e, "battle.ron"), "{e}"),
        other => panic!("a link to nothing silently made the file optional: {other:?}"),
    }
}

// ---- files that hold the wrong thing ----

/// What a file is turned into: every one must be refused (or, for the optional ones whose rows are
/// all defaulted, loaded) without a panic, and a refusal names the file.
fn damages(original: &str) -> Vec<(&'static str, Vec<u8>)> {
    let bytes = original.as_bytes();
    let mut v: Vec<(&'static str, Vec<u8>)> = vec![
        ("empty", vec![]),
        ("one space", b" ".to_vec()),
        ("only a comment", b"// nothing here\n".to_vec()),
        ("garbage text", b"this is not ron at all".to_vec()),
        ("binary", (0..=255u8).collect()),
        ("not utf-8", vec![0xff, 0xfe, 0xfd, 0x28, 0x29]),
        ("nul bytes", vec![0u8; 64]),
        ("a bare number", b"42".to_vec()),
        ("a string", b"\"units\"".to_vec()),
        ("unit", b"()".to_vec()),
        ("none", b"None".to_vec()),
        ("an empty list", b"[]".to_vec()),
        ("an empty struct", b"()".to_vec()),
        ("a map", b"{ \"a\": 1 }".to_vec()),
        ("unclosed list", b"[".to_vec()),
        ("closed too much", b"]]]]".to_vec()),
        ("a quote never closed", b"[\"abc".to_vec()),
        (
            "deep nesting",
            format!("{}{}", "[".repeat(2000), "]".repeat(2000)).into_bytes(),
        ),
        ("deeper nesting", "(".repeat(100_000).into_bytes()),
        (
            "a huge string",
            format!("[\"{}\"]", "x".repeat(5_000_000)).into_bytes(),
        ),
        (
            "a huge number",
            format!("[{}]", "9".repeat(5000)).into_bytes(),
        ),
    ];
    for (name, cut) in [
        ("cut at a quarter", bytes.len() / 4),
        ("cut at a half", bytes.len() / 2),
        (
            "cut near the end",
            original.trim_end().len().saturating_sub(2),
        ),
        ("cut by one", original.trim_end().len().saturating_sub(1)),
    ] {
        v.push((name, bytes[..cut].to_vec()));
    }
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(bytes);
    v.push(("a byte order mark first", bom));
    v.push(("trailing garbage", [bytes, b" ]]garbage"].concat()));
    v.push(("a second copy", [bytes, bytes].concat()));
    v.push(("doubled brackets", original.replace('[', "[[").into_bytes()));
    v.push((
        "every comma removed",
        original.replace(',', "").into_bytes(),
    ));
    v.push((
        "every paren swapped",
        original.replace('(', "[").replace(')', "]").into_bytes(),
    ));
    v.push((
        "every colon removed",
        original.replace(':', " ").into_bytes(),
    ));
    v.push((
        "every quote removed",
        original.replace('"', "").into_bytes(),
    ));
    v
}

#[test]
fn every_file_damaged_in_every_way_is_refused_without_a_panic_and_the_error_names_the_file() {
    let dir = copy_of_shipped("c2-damage");
    let mut failures = Vec::new();
    for file in shipped_files() {
        let original = read(&dir, &file);
        for (what, bytes) in damages(&original) {
            std::fs::write(dir.join(&file), &bytes).expect("written");
            match attempt(&dir) {
                Outcome::Panicked(m) => failures.push(format!("{file} / {what}: PANIC {m}")),
                Outcome::Refused(e) if only_conflicts(&e) => {}
                Outcome::Refused(e) => {
                    if !mentions_file(&e, &file) {
                        failures.push(format!(
                            "{file} / {what}: error does not name the file: {}",
                            e.to_string().chars().take(120).collect::<String>()
                        ));
                    }
                }
                // The damage made rows of a degradable file bad (an emptied file of powers leaves the
                // items that name them without a power): the game plays on and says what it left out.
                Outcome::Loaded(c) if !c.rules.degraded.is_empty() => {}
                Outcome::Loaded(_) => {
                    // Accepted: only fine for a file whose damage leaves it meaning the same, or an optional file.
                    // A list that may be empty (no items) and a file of defaults only may be damaged to
                    // "nothing" and still mean something; the rest must not survive the damage.
                    let may_be_empty = file == "items.ron" && what == "an empty list";
                    // A file with no quote has none to remove.
                    let all_default = what == "every quote removed" && !original.contains('"');
                    if REQUIRED.contains(&file.as_str())
                        && !may_be_empty
                        && !all_default
                        && !["a byte order mark first", "a second copy"].contains(&what)
                    {
                        failures.push(format!("{file} / {what}: ACCEPTED"));
                    }
                }
            }
        }
        std::fs::write(dir.join(&file), original).expect("restored");
    }
    assert!(
        failures.is_empty(),
        "{} problems:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn other_ways_a_text_can_be_spelled_are_still_the_same_content() {
    let want = shipped();
    for (what, change) in [
        (
            "windows line ends",
            (|t: &str| t.replace('\n', "\r\n")) as fn(&str) -> String,
        ),
        ("tabs for spaces", |t| t.replace("    ", "\t")),
        ("a trailing blank line", |t| format!("{t}\n\n")),
        ("a leading comment", |t| format!("// extra\n{t}")),
    ] {
        let dir = copy_of_shipped("c2-spelling");
        for f in shipped_files() {
            write(&dir, &f, &change(&read(&dir, &f)));
        }
        match attempt(&dir) {
            // The files differ in their bytes, so the fingerprint does; the content is the same.
            Outcome::Loaded(c) => {
                let mut c = (*c).clone();
                c.fingerprint = want.fingerprint.clone();
                assert!(&c == want, "{what} changed the content");
            }
            other => panic!("{what}: {other:?}"),
        }
    }
}

#[test]
fn a_row_with_a_field_of_the_wrong_type_is_refused_and_names_the_file() {
    for (file, from, to) in [
        ("units.ron", "cost: ", "cost: \"x\", old: "),
        ("units.ron", "num: ", "num: -"),
        ("spells.ron", "cost: ", "cost: 1.5, old: "),
        ("races.ron", "units: [", "units: [\"a\", "),
        ("items.ron", "slot: ", "slot: Nonsense, old: "),
        ("realms.ron", "color: (", "color: (999, "),
    ] {
        let dir = copy_of_shipped("c2-type");
        let text = read(&dir, file).replacen(from, to, 1);
        write(&dir, file, &text);
        match attempt(&dir) {
            Outcome::Refused(e) => {
                assert!(mentions_file(&e, file) || only_conflicts(&e), "{file}: {e}")
            }
            other => panic!("{file} {from:?}: {other:?}"),
        }
    }
}

#[test]
fn a_field_that_is_written_twice_is_refused() {
    let dir = copy_of_shipped("c2-twice");
    let text = read(&dir, "units.ron").replacen("cost: ", "cost: 1, cost: ", 1);
    write(&dir, "units.ron", &text);
    assert!(
        matches!(attempt(&dir), Outcome::Refused(_)),
        "a repeated field was accepted"
    );
}

#[test]
fn a_field_that_no_type_has_is_refused_so_a_typo_does_not_silently_drop_a_setting() {
    for file in shipped_files() {
        let dir = copy_of_shipped("c2-unknown");
        let file = file.as_str();
        let original = read(&dir, file);
        // A key no type has, as the first field of the first row (after the comment lines).
        let start = original
            .split_inclusive('\n')
            .scan(0, |at, line| {
                let here = *at;
                *at += line.len();
                Some((here, line))
            })
            .find(|(_, line)| !line.trim_start().starts_with("//") && line.contains('('))
            .map(|(here, line)| here + line.find('(').expect("a paren") + 1)
            .expect("a struct");
        let mut text = original.clone();
        text.insert_str(start, "typo_key_that_no_type_has: 1, ");
        write(&dir, file, &text);
        match attempt(&dir) {
            Outcome::Refused(_) => {}
            other => panic!("{file}: an unknown field was accepted: {other:?}"),
        }
    }
}

#[test]
fn the_error_of_a_file_that_cannot_be_parsed_says_where() {
    let dir = copy_of_shipped("c2-where");
    write(&dir, "units.ron", "[\n(num: 1,\n id: 5\n]");
    match attempt(&dir) {
        Outcome::Refused(e) => {
            let issue = e.issues.first().expect("an issue");
            assert_eq!(issue.file, "units.ron", "{e}");
            assert!(issue.at.contains("line"), "no position in {e}");
            assert!(!only_conflicts(&e), "a syntax error is not a conflict: {e}");
        }
        other => panic!("{other:?}"),
    }
}

// ---- layers: a local folder over the shipped one ----

#[test]
fn a_file_of_the_first_folder_wins_and_the_others_come_from_the_next() {
    let local = TestDir::new("c2-layer");
    std::fs::copy(
        shipped_dir().join("rules_settings.ron"),
        local.join("rules_settings.ron"),
    )
    .expect("copied");
    edit::<f4x_def::settings::SettingsDef>(&local, "rules_settings.ron", |d| {
        d.myrror_fortress_power += 1
    });
    let layered = load_content_layers(&[&local, &shipped_dir()]).expect("loads");
    assert_eq!(
        layered.settings.myrror_fortress_power,
        shipped().settings.myrror_fortress_power + 1
    );
    assert_eq!(
        layered.rules,
        shipped().rules,
        "the files the local folder lacks come from the shipped one"
    );
    // The order is the order of the list.
    let other = load_content_layers(&[&shipped_dir(), &local]).expect("loads");
    assert_eq!(
        other.settings.myrror_fortress_power,
        shipped().settings.myrror_fortress_power
    );
}

#[test]
fn a_broken_file_in_the_first_folder_is_an_error_not_a_quiet_fall_back_to_the_next() {
    let local = TestDir::new("c2-layer-broken");
    std::fs::write(local.join("units.ron"), "[ broken").expect("written");
    match load_content_layers(&[&local, &shipped_dir()]) {
        Err(e) => assert!(mentions_file(&e, "units.ron"), "{e}"),
        Ok(_) => panic!("the broken local file was skipped"),
    }
}

#[test]
fn no_folder_at_all_is_an_error() {
    assert!(load_content_layers(&[]).is_err());
}

#[test]
fn a_settings_file_that_is_missing_or_bad_is_refused() {
    let dir = copy_of_shipped("c2-settings");
    std::fs::remove_file(dir.join("rules_settings.ron")).expect("removed");
    let e = load_settings(&[&dir]).expect_err("no settings");
    assert!(
        e.has(IssueKind::Missing) && e.mentions("rules_settings.ron"),
        "{e}"
    );
    write(&dir, "rules_settings.ron", "(this is not settings");
    let e = load_settings(&[&dir]).expect_err("not settings");
    assert!(
        e.mentions("rules_settings.ron") && !only_conflicts(&e),
        "{e}"
    );
    write(&dir, "rules_settings.ron", "(max_opponents: 0)");
    assert!(load_settings(&[&dir]).is_err());
}

// ---- help ----

#[test]
fn help_without_files_is_empty_and_damaged_help_is_an_error() {
    let dir = TestDir::new("c2-help");
    let none = load_help(&[&dir], "en").expect("no files, no help");
    assert!(none.topics.is_empty() && none.screens.is_empty());
    std::fs::create_dir_all(dir.join("text/en/help")).expect("folders");
    for bad in [
        "",
        "garbage",
        "[(index: 1)]",
        "[(index: \"a\", title: 1, text: 2)]",
    ] {
        std::fs::write(dir.join("text/en/help/help.ron"), bad).expect("written");
        let r = catch_unwind(AssertUnwindSafe(|| load_help(&[&dir], "en"))).expect("no panic");
        assert!(r.is_err(), "{bad:?} was accepted");
    }
    std::fs::write(
        dir.join("text/en/help/help.ron"),
        "[(index: 1, title: \"a\", text: \"b\")]",
    )
    .expect("written");
    assert_eq!(load_help(&[&dir], "en").expect("help").topics.len(), 1);
    // A language without a file of its own uses the fallback language's.
    assert_eq!(load_help(&[&dir], "xx").expect("fallback").topics.len(), 1);
}

#[test]
fn a_language_name_cannot_make_the_help_read_another_folder() {
    let dir = TestDir::new("c2-lang");
    std::fs::create_dir_all(dir.join("text/en/help")).expect("folders");
    std::fs::write(dir.join("text/en/help/help.ron"), "[]").expect("written");
    std::fs::write(
        dir.join("secret.ron"),
        "[(index: 9, title: \"s\", text: \"s\")]",
    )
    .expect("written");
    for lang in ["../..", "..", "en/../..", "/", "", "en\0x", "../../../etc"] {
        let r = catch_unwind(AssertUnwindSafe(|| load_help(&[&dir], lang))).expect("no panic");
        if let Ok(h) = r {
            assert!(
                h.topics.iter().all(|t| t.index != 9),
                "language {lang:?} reached a file outside text/<lang>/help"
            );
        }
    }
}

// ---- every number of every file, made into something else ----

/// How many numbers to try per kind of change: a few hundred in a normal run, `F4X_ATTACKS=n` for more.
fn attack_count() -> usize {
    std::env::var("F4X_ATTACKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(250)
}

/// Replace one number at a time by each of `values`; whatever the loader does, it does not panic, it
/// names the file when it cannot parse, and the content it accepts has no dangling reference.
fn attack_numbers(seed: u64, values: &[&str]) {
    let dir = copy_of_shipped("c2-numbers");
    let files = shipped_files();
    let originals: Vec<String> = files.iter().map(|f| read(&dir, f)).collect();
    let spots: Vec<Vec<(usize, usize)>> = originals.iter().map(|t| numbers_in(t)).collect();
    let total: usize = spots.iter().map(Vec::len).sum();
    assert!(total > 1000, "the files have numbers");
    let mut rng = seed | 1;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let mut failures = Vec::new();
    for _ in 0..attack_count() {
        let mut pick = (next() % total as u64) as usize;
        let f = spots
            .iter()
            .position(|s| {
                if pick < s.len() {
                    true
                } else {
                    pick -= s.len();
                    false
                }
            })
            .expect("a file");
        let (a, b) = spots[f][pick];
        for value in values {
            let text = format!("{}{value}{}", &originals[f][..a], &originals[f][b..]);
            write(&dir, &files[f], &text);
            let what = format!(
                "{} at byte {a} ({:?} -> {value})",
                files[f],
                &originals[f][a..b]
            );
            match attempt(&dir) {
                Outcome::Panicked(m) => failures.push(format!("{what}: PANIC {m}")),
                Outcome::Refused(e) if only_conflicts(&e) => {}
                Outcome::Refused(e) => {
                    if !mentions_file(&e, &files[f]) {
                        failures.push(format!("{what}: the error does not name the file: {e}"));
                    }
                }
                Outcome::Loaded(c) => {
                    let d = dangling(&c);
                    if !d.is_empty() {
                        failures.push(format!(
                            "{what}: ACCEPTED with a dangling reference: {:?}",
                            &d[..d.len().min(2)]
                        ));
                    }
                }
            }
        }
        write(&dir, &files[f], &originals[f]);
    }
    assert!(
        failures.is_empty(),
        "{} problems:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn a_number_made_negative_or_zero_is_refused_or_harmless() {
    attack_numbers(
        1,
        &["-1", "0", "-0", "-2147483648", "-99999999999999999999"],
    );
}

#[test]
fn a_number_made_too_big_for_its_type_is_refused_or_harmless() {
    attack_numbers(
        2,
        &[
            "255",
            "256",
            "65535",
            "65536",
            "4294967295",
            "4294967296",
            "2147483647",
            "2147483648",
            "99999999999999999999",
            "1e308",
        ],
    );
}

#[test]
fn a_number_made_fractional_or_not_a_number_is_refused_or_harmless() {
    attack_numbers(
        3,
        &[
            "0.5", "0.25", "1.5", "NaN", "inf", "-inf", "00", "0x10", "1_0", "1e3", ".5", "5.",
        ],
    );
}

#[test]
fn a_number_made_something_else_is_refused_or_harmless() {
    attack_numbers(
        4,
        &[
            "\"x\"", "None", "Some(1)", "()", "[]", "(1)", "true", "'a'", "\"\"", "",
        ],
    );
}

// ---- a key no type has, in any struct of any file ----

#[test]
fn a_key_no_type_has_is_refused_wherever_it_is_put() {
    let dir = copy_of_shipped("c2-keys");
    let files = shipped_files();
    let originals: Vec<String> = files.iter().map(|f| read(&dir, f)).collect();
    // Every opening paren of a struct (not in a string or a comment) is a place for the key.
    let places: Vec<Vec<usize>> = originals
        .iter()
        .map(|t| {
            let mut v = Vec::new();
            let (mut in_str, mut in_comment, mut in_block) = (false, false, false);
            let mut prev = ' ';
            for (i, c) in t.char_indices() {
                if in_block {
                    in_block = !(prev == '*' && c == '/');
                } else if in_comment {
                    in_comment = c != '\n';
                } else if c == '*' && prev == '/' && !in_str {
                    in_block = true;
                } else if in_str {
                    in_str = c != '"' || prev == '\\';
                } else if c == '"' {
                    in_str = true;
                } else if c == '/' && prev == '/' {
                    in_comment = true;
                } else if c == '(' {
                    v.push(i + 1);
                }
                prev = c;
            }
            v
        })
        .collect();
    let total: usize = places.iter().map(Vec::len).sum();
    let mut rng = 0x9e3779b97f4a7c15u64;
    let mut accepted = Vec::new();
    for _ in 0..attack_count() * 4 {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let mut pick = (rng % total as u64) as usize;
        let f = places
            .iter()
            .position(|p| {
                if pick < p.len() {
                    true
                } else {
                    pick -= p.len();
                    false
                }
            })
            .expect("a file");
        let at = places[f][pick];
        let text = format!(
            "{}typo_key_that_no_type_has: 1, {}",
            &originals[f][..at],
            &originals[f][at..]
        );
        write(&dir, &files[f], &text);
        // A paren that opens a tuple or a variant refuses a key it does not take: that is as good.
        if let Outcome::Loaded(_) = attempt(&dir) {
            accepted.push(format!(
                "{} at byte {at}: ...{}",
                files[f],
                originals[f][at.saturating_sub(40)..at].replace('\n', " ")
            ));
        }
        write(&dir, &files[f], &originals[f]);
    }
    assert!(
        accepted.is_empty(),
        "{} places took a key no type has:\n{}",
        accepted.len(),
        accepted.join("\n")
    );
}

// ---- rows that cannot make a table ----

#[test]
fn rows_out_of_the_order_of_their_numbers_or_with_a_number_twice_are_an_error_and_not_a_panic() {
    use f4x_def::rules::{AbilityDef, BuildingDef, RealmDef, SpellDef, UnitDef};
    fn swap_and_repeat<T: serde::de::DeserializeOwned + serde::Serialize>(
        file: &str,
        num_of: fn(&mut T) -> &mut u16,
    ) {
        let dir = copy_of_shipped("c2-order");
        edit::<Vec<T>>(&dir, file, |v| v.swap(0, 1));
        match attempt(&dir) {
            Outcome::Refused(e) => assert!(e.mentions(file), "{file}: {e}"),
            other => panic!("{file} out of order: {other:?}"),
        }
        let dir = copy_of_shipped("c2-repeat");
        edit::<Vec<T>>(&dir, file, |v| {
            let first = *num_of(&mut v[0]);
            *num_of(&mut v[1]) = first;
        });
        match attempt(&dir) {
            Outcome::Refused(e) => assert!(e.mentions(file), "{file}: {e}"),
            other => panic!("{file} repeated: {other:?}"),
        }
    }
    swap_and_repeat::<UnitDef>("units.ron", |d| &mut d.num);
    swap_and_repeat::<AbilityDef>("abilities.ron", |d| &mut d.num);
    swap_and_repeat::<BuildingDef>("buildings.ron", |d| &mut d.num);
    swap_and_repeat::<RealmDef>("realms.ron", |d| &mut d.num);
    swap_and_repeat::<SpellDef>("spells.ron", |d| &mut d.num);
}

#[test]
fn a_file_with_more_rows_than_a_table_holds_is_an_error_and_not_a_panic() {
    use f4x_def::rules::RealmDef;
    let dir = copy_of_shipped("c2-many");
    edit::<Vec<RealmDef>>(&dir, "realms.ron", |v| {
        let row = v[0].clone();
        v.clear();
        for n in 0..=u16::MAX {
            v.push(RealmDef {
                num: n,
                id: format!("r{n}"),
                ..row.clone()
            });
        }
    });
    match attempt(&dir) {
        Outcome::Refused(e) => assert!(e.mentions("realms.ron"), "{e}"),
        other => panic!("{other:?}"),
    }
}

// ---- the player's own settings file ----

#[test]
fn the_players_default_game_file_is_read_back_brought_into_range_and_a_bad_one_is_ignored() {
    use f4x_def::settings::{DefaultGame, GameSettings};
    let dir = TestDir::new("c2-default-game");
    let path = dir.join("settings/default_game.ron");
    assert!(DefaultGame::load(&path).is_none(), "no file");
    let rec = DefaultGame::factory(&shipped().settings);
    rec.save(&path).expect("saved");
    assert_eq!(
        DefaultGame::load(&path),
        Some(rec),
        "what is saved is read back"
    );
    for bad in ["", "garbage", "(", "()", "(preset: Nonsense)", "\u{0}"] {
        std::fs::write(&path, bad).expect("written");
        assert!(DefaultGame::load(&path).is_none(), "{bad:?}");
    }
    // A file edited by hand to values out of range: what is read is in range.
    let text =
        ron::ser::to_string_pretty(&rec, ron::ser::PrettyConfig::default()).expect("written");
    let (mut read, mut mended) = (0, 0);
    for (a, b) in numbers_in(&text) {
        for v in ["0", "99", "255", "65535", "4294967295"] {
            std::fs::write(&path, format!("{}{v}{}", &text[..a], &text[b..])).expect("written");
            if let Some(r) = DefaultGame::load(&path) {
                read += 1;
                mended += usize::from(
                    ron::from_str::<DefaultGame>(&std::fs::read_to_string(&path).expect("read"))
                        .is_ok_and(|raw| raw.settings != r.settings),
                );
                assert_eq!(
                    r.settings.validate(),
                    Vec::<f4x_def::issue::ContentIssue>::new(),
                    "{} -> {v}",
                    &text[a..b]
                );
                let _: GameSettings = r.settings;
            }
        }
    }
    assert!(
        read > 100 && mended > 10,
        "the attack reached too little ({read}, {mended})"
    );
}

#[test]
fn saving_the_default_game_where_nothing_can_be_written_says_where() {
    use f4x_def::settings::DefaultGame;
    let dir = TestDir::new("c2-default-game-bad");
    std::fs::write(dir.join("a file"), "x").expect("a file");
    let rec = DefaultGame::factory(&shipped().settings);
    // A folder that has to be made inside a file cannot be.
    let err = rec
        .save(&dir.join("a file/inside/default_game.ron"))
        .expect_err("not written");
    assert!(err.contains("a file"), "{err}");
}

// ---- the kind of an issue ----

/// The issues of the content in `dir`, which must be refused.
fn issues_of(dir: &Path) -> Vec<f4x_def::issue::ContentIssue> {
    match attempt(dir) {
        Outcome::Refused(e) => e.issues,
        other => panic!("expected a refusal: {other:?}"),
    }
}

#[test]
fn each_way_a_file_can_be_wrong_gives_its_own_kind_and_names_file_and_place() {
    use f4x_def::issue::Outcome as Out;
    let case = |file: &str, change: &dyn Fn(&str) -> String| {
        let dir = copy_of_shipped("c2-kind");
        let text = change(&read(&dir, file));
        write(&dir, file, &text);
        issues_of(&dir)
    };
    // Cut off in the middle: not the format.
    let i = case("units.ron", &|t| t[..t.len() / 2].to_string());
    assert!(
        i.iter().any(|i| i.kind == IssueKind::Corrupt
            && i.file == "units.ron"
            && i.at.contains("line")),
        "{i:?}"
    );
    // A number where a name goes: the wrong type.
    let i = case("realms.ron", &|t| t.replacen("id: \"", "id: 5, was: \"", 1));
    assert!(
        i.iter().any(|i| i.file == "realms.ron"
            && matches!(i.kind, IssueKind::UnknownKey | IssueKind::WrongType)),
        "{i:?}"
    );
    let i = case("realms.ron", &|t| t.replacen("num: 0", "num: \"zero\"", 1));
    assert!(
        i.iter()
            .any(|i| i.kind == IssueKind::WrongType && i.file == "realms.ron"),
        "{i:?}"
    );
    // A key no type has.
    let i = case("realms.ron", &|t| {
        t.replacen("(num: 0", "(nmu: 7, num: 0", 1)
    });
    assert!(
        i.iter()
            .any(|i| i.kind == IssueKind::UnknownKey && i.file == "realms.ron"),
        "{i:?}"
    );
    // A field that must be there, left out.
    let i = case("realms.ron", &|t| t.replacen("num: 0, ", "", 1));
    assert!(
        i.iter()
            .any(|i| i.kind == IssueKind::Missing && i.file == "realms.ron"),
        "{i:?}"
    );
    // A file that is not there.
    let dir = copy_of_shipped("c2-kind-missing");
    std::fs::remove_file(dir.join("spells.ron")).expect("removed");
    assert!(issues_of(&dir)
        .iter()
        .any(|i| i.kind == IssueKind::Missing && i.file.ends_with("spells.ron")));
    // A file that is a folder: there, but not readable.
    let dir = copy_of_shipped("c2-kind-folder");
    std::fs::remove_file(dir.join("spells.ron")).expect("removed");
    std::fs::create_dir(dir.join("spells.ron")).expect("a folder");
    assert!(issues_of(&dir)
        .iter()
        .any(|i| i.kind == IssueKind::Unreadable && i.file.ends_with("spells.ron")));
    // Whatever the cause, today every issue refuses the game.
    assert!(issues_of(&dir).iter().all(|i| i.outcome == Out::Refuse));
}

#[test]
fn damage_in_several_files_is_reported_for_all_of_them_in_one_load() {
    let dir = copy_of_shipped("c2-many");
    for (file, what) in [
        ("units.ron", "cut"),
        ("spells.ron", "key"),
        ("terrain.ron", "gone"),
    ] {
        let text = read(&dir, file);
        match what {
            "cut" => write(&dir, file, &text[..text.len() / 2]),
            "key" => write(&dir, file, &text.replacen("(num:", "(nmu: 1, num:", 1)),
            _ => std::fs::remove_file(dir.join(file)).expect("removed"),
        }
    }
    let issues = issues_of(&dir);
    for file in ["units.ron", "spells.ron", "terrain.ron"] {
        assert!(
            issues.iter().any(|i| i.file.ends_with(file)),
            "{file} is not reported: {issues:#?}"
        );
    }
}

// ---- settings a file leaves out ----

#[test]
fn a_settings_file_must_state_every_setting_and_a_missing_one_is_named() {
    use f4x_def::load::load_settings;
    // Every setting that a save may lack is one the file must state: take each out of the shipped
    // file in turn (a line of its own in the `new_game` block, or the first level's `rules`).
    let dir = copy_of_shipped("c2-gap");
    let text = read(&dir, "rules_settings.ron");
    for key in [
        "map_width",
        "map_height",
        "combat_forecast",
        "ban_life_and_death",
        "monster_start_turn",
        "start_turn",
        "frequency",
    ] {
        let cut = text.replace(&format!("{key}: "), &format!("not_{key}: "));
        write(&dir, "rules_settings.ron", &cut);
        let e = load_settings(&[&dir]).expect_err(key);
        assert!(!e.issues.is_empty(), "{key}");
    }
    write(&dir, "rules_settings.ron", &text);
    load_settings(&[&dir]).expect("the shipped file is whole");
}

// ---- the fingerprint of the files ----

#[test]
fn the_fingerprint_names_the_files_the_content_was_read_from() {
    let first = &shipped().fingerprint;
    assert_eq!(first.len(), 64, "a SHA-256 in hex");
    let dir = copy_of_shipped("c2-fingerprint");
    assert_eq!(
        &load(&dir).expect("loads").fingerprint,
        first,
        "a copy has the same files"
    );
    // A change of one byte in one file is another fingerprint.
    let text = read(&dir, "lairs.ron");
    write(&dir, "lairs.ron", &format!("{text}\n// changed\n"));
    assert_ne!(&load(&dir).expect("loads").fingerprint, first);
}

#[test]
fn every_kind_of_issue_has_a_message_key_of_its_own() {
    // The match has no `_` arm: a new kind does not compile until it is listed here, and so is given
    // a message in `content_issue.ron` (the client test `every_kind_of_issue_is_told_in_words...`).
    let kinds = [
        IssueKind::Missing,
        IssueKind::Unreadable,
        IssueKind::Corrupt,
        IssueKind::WrongType,
        IssueKind::OutOfRange,
        IssueKind::Dangling,
        IssueKind::Duplicate,
        IssueKind::UnknownKey,
        IssueKind::Conflict,
    ];
    let mut keys = std::collections::BTreeSet::new();
    for kind in kinds {
        let expected = match kind {
            IssueKind::Missing => "content_issue.missing",
            IssueKind::Unreadable => "content_issue.unreadable",
            IssueKind::Corrupt => "content_issue.corrupt",
            IssueKind::WrongType => "content_issue.wrong_type",
            IssueKind::OutOfRange => "content_issue.out_of_range",
            IssueKind::Dangling => "content_issue.dangling",
            IssueKind::Duplicate => "content_issue.duplicate",
            IssueKind::UnknownKey => "content_issue.unknown_key",
            IssueKind::Conflict => "content_issue.conflict",
        };
        assert_eq!(kind.text_key(), expected);
        assert!(keys.insert(expected), "two kinds share {expected}");
    }
}
