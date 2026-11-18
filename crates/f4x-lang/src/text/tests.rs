use super::*;
use f4x_def::testdir::TestResult;

fn texts(src: &str) -> Texts {
    let entries: HashMap<String, Entry> = ron::from_str(src).expect("from_str succeeds");
    Texts::new("en", entries, "en", HashMap::new())
}

#[test]
fn a_plain_text_takes_its_values() {
    let t = texts(r#"{ "spell.cast": "{wizard} casts {spell} for {mana} mana." }"#);
    let args = Args::new()
        .text("wizard", "Merlin")
        .text("spell", "Fire Bolt")
        .count("mana", 15);
    assert_eq!(
        t.get("spell.cast", &args),
        "Merlin casts Fire Bolt for 15 mana."
    );
}

#[test]
fn braces_can_be_written_twice_to_print_one() {
    let t = texts(r#"{ "a": "{{{n}}}" }"#);
    assert_eq!(t.get("a", &Args::new().count("n", 3)), "{3}");
}

#[test]
fn a_missing_key_prints_as_the_key() {
    assert_eq!(texts("{}").get("no.such", &Args::new()), "no.such");
}

#[test]
fn a_key_missing_from_the_language_comes_from_the_fallback() -> TestResult {
    let fallback: HashMap<String, Entry> = ron::from_str(r#"{ "a": "fallback" }"#)?;
    let t = Texts::new("de", HashMap::new(), "en", fallback);
    assert_eq!(t.get("a", &Args::new()), "fallback");
    Ok(())
}

const UNITS: &str = r#"{
    "city.units": (count: "n", zero: "no units", one: "{n} unit", other: "{n} units"),
}"#;

#[test]
fn a_count_picks_the_plural_form() {
    let t = texts(UNITS);
    let say = |n| t.get("city.units", &Args::new().count("n", n));
    assert_eq!(say(0), "no units");
    assert_eq!(say(1), "1 unit");
    assert_eq!(say(2), "2 units");
}

#[test]
fn languages_tell_plurals_apart_by_their_own_rules() {
    assert_eq!(plural("fr", 0), Plural::One);
    assert_eq!(plural("en", 0), Plural::Other);
    assert_eq!(plural("ru", 21), Plural::One);
    assert_eq!(plural("ru", 22), Plural::Few);
    assert_eq!(plural("ru", 11), Plural::Many);
    assert_eq!(plural("pl", 12), Plural::Many);
    assert_eq!(plural("cs", 3), Plural::Few);
    assert_eq!(plural("ja", 1), Plural::Other);
}

const SLAIN: &str = r#"{
    "hero.slain": (gender: "hero",
        masculine: "{hero} fell; his items are lost.",
        feminine: "{hero} fell; her items are lost.",
        neuter: "{hero} fell; its items are lost."),
    "hero.slain_with": (count: "n",
        one: (gender: "hero", masculine: "He took {n} foe.", feminine: "She took {n} foe.", neuter: "It took {n} foe."),
        other: "{hero} took {n} foes."),
}"#;

#[test]
fn a_gender_picks_the_form_of_the_named_argument() {
    let t = texts(SLAIN);
    let say = |g| t.get("hero.slain", &Args::new().named("hero", "Serena", g));
    assert_eq!(say(Gender::Feminine), "Serena fell; her items are lost.");
    assert_eq!(say(Gender::Masculine), "Serena fell; his items are lost.");
    assert_eq!(say(Gender::Neuter), "Serena fell; its items are lost.");
}

#[test]
fn a_form_can_hold_a_choice_of_its_own() {
    let t = texts(SLAIN);
    let one = Args::new()
        .named("hero", "Brax", Gender::Masculine)
        .count("n", 1);
    assert_eq!(t.get("hero.slain_with", &one), "He took 1 foe.");
    let two = Args::new()
        .named("hero", "Brax", Gender::Masculine)
        .count("n", 2);
    assert_eq!(t.get("hero.slain_with", &two), "Brax took 2 foes.");
}

#[test]
fn an_entry_with_both_choices_or_without_other_is_refused() {
    assert!(ron::from_str::<HashMap<String, Entry>>(r#"{ "a": (count: "n", one: "x") }"#).is_err());
    assert!(ron::from_str::<HashMap<String, Entry>>(
        r#"{ "a": (count: "n", gender: "h", other: "x") }"#
    )
    .is_err());
}

#[test]
fn a_list_is_joined_by_the_language_patterns() {
    let t = texts(
        r#"{ "list.two": "{a} and {b}", "list.start": "{a}, {b}", "list.middle": "{a}, {b}",
              "list.end": "{a} and {b}", "found": "You find {things}." }"#,
    );
    let say = |items: &[&str]| {
        let items = items.iter().map(|s| s.to_string()).collect();
        t.get("found", &Args::new().list("things", items))
    };
    assert_eq!(say(&["gold"]), "You find gold.");
    assert_eq!(say(&["gold", "a sword"]), "You find gold and a sword.");
    assert_eq!(
        say(&["gold", "a sword", "mana"]),
        "You find gold, a sword and mana."
    );
    assert_eq!(say(&["a", "b", "c", "d"]), "You find a, b, c and d.");
}

#[test]
fn a_pick_takes_the_line_the_number_names() {
    let t = texts(r#"{ "hello": (pick: "salt", lines: ["Hi.", "Hello.", "Greetings."]) }"#);
    let say = |n| t.get("hello", &Args::new().count("salt", n));
    assert_eq!(say(0), "Hi.");
    assert_eq!(say(4), "Hello.");
    assert_eq!(say(5), "Greetings.");
    assert!(ron::from_str::<HashMap<String, Entry>>(r#"{ "a": (pick: "s", lines: []) }"#).is_err());
}

fn german_texts(src: &str) -> Texts {
    let entries: HashMap<String, Entry> = ron::from_str(src).expect("from_str succeeds");
    Texts::new("de", entries, "en", HashMap::new())
}

fn swordsmen() -> Term {
    ron::from_str(
        r#"(text: "Zwergische Schwertkämpfer", gender: Masculine,
            dat_pl: "Zwergischen Schwertkämpfern", gen_pl: "Zwergischer Schwertkämpfer")"#,
    )
    .expect("a record term reads")
}

#[test]
fn a_term_is_a_plain_string_or_a_record_with_forms() -> TestResult {
    let plain: Term = ron::from_str(r#""Dwarven Swordsmen""#)?;
    assert_eq!(plain, Term::plain("Dwarven Swordsmen"));
    let term = swordsmen();
    assert_eq!(term.text, "Zwergische Schwertkämpfer");
    assert_eq!(term.gender, Some(Gender::Masculine));
    assert_eq!(term.form("dat_pl"), "Zwergischen Schwertkämpfern");
    // A form the term lacks is its plain text.
    assert_eq!(term.form("acc"), "Zwergische Schwertkämpfer");
    assert!(
        ron::from_str::<Term>(r#"(dat: "x")"#).is_err(),
        "a record needs its text"
    );
    Ok(())
}

#[test]
fn a_message_asks_for_the_form_of_a_name_it_needs() {
    let t = german_texts(
        r#"{ "log.damage": "Den {unit:dat_pl} wurden {n} Schadenspunkte zugefügt.",
             "log.plain": "{unit} greifen an.",
             "log.missing": "Die {unit:acc_pl} greifen an." }"#,
    );
    let args = Args::new().term("unit", swordsmen()).count("n", 3);
    assert_eq!(
        t.get("log.damage", &args),
        "Den Zwergischen Schwertkämpfern wurden 3 Schadenspunkte zugefügt."
    );
    assert_eq!(
        t.get("log.plain", &args),
        "Zwergische Schwertkämpfer greifen an."
    );
    assert_eq!(
        t.get("log.missing", &args),
        "Die Zwergische Schwertkämpfer greifen an."
    );
}

#[test]
fn a_gender_choice_reads_the_gender_of_a_name() {
    let t = german_texts(
        r#"{ "log.the": (gender: "unit", masculine: "Der {unit}", feminine: "Die {unit}", neuter: "Das {unit}") }"#,
    );
    let masculine = Args::new().term("unit", swordsmen());
    assert_eq!(
        t.get("log.the", &masculine),
        "Der Zwergische Schwertkämpfer"
    );
    let feminine = Args::new().term(
        "unit",
        Term {
            gender: Some(Gender::Feminine),
            ..Term::plain("Kanone")
        },
    );
    assert_eq!(t.get("log.the", &feminine), "Die Kanone");
}

#[test]
fn a_number_choice_reads_the_plural_tag_of_a_name() -> TestResult {
    let t = texts(
        r#"{ "log.falls": (number: "b", singular: "the {b} falls", plural: "the {b} fall") }"#,
    );
    let walls: Term = ron::from_str(r#"(text: "City Walls", plural: true)"#)?;
    let singular = Args::new().term("b", Term::plain("Library"));
    assert_eq!(t.get("log.falls", &singular), "the Library falls");
    assert_eq!(
        t.get("log.falls", &Args::new().term("b", walls)),
        "the City Walls fall"
    );
    // A value that is no term has no tag: the singular wording.
    assert_eq!(
        t.get("log.falls", &Args::new().text("b", "x")),
        "the x falls"
    );
    assert!(
        ron::from_str::<Entry>(r#"(number: "b", singular: "x")"#).is_err(),
        "both wordings are needed"
    );
    Ok(())
}

#[test]
fn the_shipped_messages_agree_with_the_number_of_the_name() -> TestResult {
    let dirs = [std::path::Path::new("content")];
    let texts = load_texts(&dirs, "en")?;
    let names = crate::names::Names::load(&dirs, "en")?;
    let rules = f4x_def::testdir::shipped();
    let building = |id: &str| {
        let def = rules.rules.buildings.defs().iter().find(|d| d.id == id);
        names.building_term(def.map_or(0, |d| d.num))
    };
    let rift = |id: &str| {
        let args = Args::new()
            .text("city", "Ur")
            .term("building", building(id));
        texts.get("log.rift_building", &args)
    };
    assert_eq!(
        rift("library"),
        "The chaos rift over Ur strikes, the Library falls"
    );
    assert_eq!(
        rift("city_walls"),
        "The chaos rift over Ur strikes, the City Walls fall"
    );
    let spearmen = names.unit_term(39);
    assert!(spearmen.plural, "{spearmen:?}");
    let blocked = texts.get(
        "log.production_blocked",
        &Args::new().text("city", "Ur").term("item", spearmen),
    );
    assert!(
        blocked.contains("the Barbarian Spearmen are ready"),
        "{blocked}"
    );
    Ok(())
}

#[test]
fn an_indefinite_article_follows_the_tags_and_a_language_form() -> TestResult {
    let t = texts(r#"{ "found": "You found {x:a}.", "start": "{x:A} appears." }"#);
    let library = Args::new().term("x", Term::plain("Library"));
    assert_eq!(t.get("found", &library), "You found a Library.");
    let armory: Term = ron::from_str(r#"(text: "Armory", an: true)"#)?;
    assert_eq!(
        t.get("found", &Args::new().term("x", armory.clone())),
        "You found an Armory."
    );
    assert_eq!(
        t.get("start", &Args::new().term("x", armory)),
        "An Armory appears."
    );
    // A plural noun takes no article.
    let walls: Term = ron::from_str(r#"(text: "City Walls", plural: true)"#)?;
    assert_eq!(
        t.get("found", &Args::new().term("x", walls)),
        "You found City Walls."
    );
    // German writes the whole phrase as a form; a language with no rule prints the text.
    let de: Term = ron::from_str(r#"(text: "Zwerg", a: "ein Zwerg")"#)?;
    let german = german_texts(r#"{ "found": "Du findest {x:a}." }"#);
    assert_eq!(
        german.get("found", &Args::new().term("x", de)),
        "Du findest ein Zwerg."
    );
    let bare = Args::new().term("x", Term::plain("Zwerg"));
    assert_eq!(german.get("found", &bare), "Du findest Zwerg.");
    Ok(())
}

#[test]
fn the_shipped_messages_choose_a_or_an_for_the_name_they_name() -> TestResult {
    let dirs = [std::path::Path::new("content")];
    let texts = load_texts(&dirs, "en")?;
    let names = crate::names::Names::load(&dirs, "en")?;
    let rules = f4x_def::testdir::shipped();
    let realm = |id: &str| {
        let def = rules.rules.realms.defs().iter().find(|d| d.id == id);
        names.realm_term(def.map_or(0, |d| d.num))
    };
    let book = |id: &str| texts.get("log.loot_book", &Args::new().term("realm", realm(id)));
    assert_eq!(book("nature"), "a Nature book");
    assert_eq!(book("arcane"), "an Arcane book");
    let spell = |num| names.spell_term(num);
    let found = |first| texts.get("spell.conquest_one", &Args::new().term("first", first));
    assert!(found(spell(
        rules
            .rules
            .spells
            .defs()
            .iter()
            .find(|d| d.id == "ice_bolt")
            .map_or(0, |d| d.num)
    ))
    .contains("an Ice Bolt spell"));
    // A unit that is a plural noun takes no article.
    let goods = texts.get(
        "diplomacy.goods_unit",
        &Args::new()
            .term("name", names.unit_term(39))
            .count("level", 2),
    );
    assert_eq!(goods, "Barbarian Spearmen (level 2)");
    Ok(())
}

#[test]
fn a_retort_takes_a_or_an_by_its_tag() -> TestResult {
    let dirs = [std::path::Path::new("content")];
    let texts = texts(r#"{ "got": "You have the retort of {r:a}." }"#);
    let names = crate::names::Names::load(&dirs, "en")?;
    let rules = f4x_def::testdir::shipped();
    let retort = |id: &str| {
        let def = rules.rules.retorts.defs().iter().find(|d| d.id == id);
        names.retort_term(def.map_or(0, |d| d.num))
    };
    let say = |id: &str| texts.get("got", &Args::new().term("r", retort(id)));
    assert_eq!(say("archmage"), "You have the retort of an Archmage.");
    assert_eq!(say("warlord"), "You have the retort of a Warlord.");
    Ok(())
}

/// The messages of a language: every `*.ron` file of `text/<lang>/` (not the subfolders), by key.
fn language_files(
    lang: &str,
) -> Result<std::collections::BTreeMap<String, (String, Entry)>, Box<dyn std::error::Error>> {
    let folder = std::path::Path::new("content/text").join(lang);
    let mut all = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(&folder)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "ron") {
            let name = path
                .file_stem()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
            let rows: std::collections::BTreeMap<String, Entry> =
                ron::from_str(&std::fs::read_to_string(&path)?)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
            for (key, entry) in rows {
                all.insert(key, (name.clone(), entry));
            }
        }
    }
    Ok(all)
}

#[test]
fn german_messages_name_english_keys_and_use_the_same_arguments() -> TestResult {
    let en = language_files("en")?;
    let de = language_files("de")?;
    let mut problems = Vec::new();
    for (key, (file, entry)) in &de {
        let Some((en_file, original)) = en.get(key) else {
            problems.push(format!("{key}: no such key in English"));
            continue;
        };
        if file != en_file {
            problems.push(format!(
                "{key}: in de/{file}.ron, English has it in {en_file}.ron"
            ));
        }
        let (want, have) = (original.arguments(), entry.arguments());
        if want != have {
            problems.push(format!("{key}: English uses {want:?}, German {have:?}"));
        }
    }
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
    Ok(())
}

/// Lists what German still lacks; it is a progress report, so it only fails when asked for with
/// `--ignored` and something is missing.
#[test]
#[ignore = "progress report of the German translation"]
fn german_has_every_english_message() -> TestResult {
    let en = language_files("en")?;
    let de = language_files("de")?;
    let missing: Vec<_> = en.keys().filter(|k| !de.contains_key(*k)).collect();
    assert!(
        missing.is_empty(),
        "{} of {} keys missing:\n{}",
        missing.len(),
        en.len(),
        missing
            .iter()
            .map(|k| k.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    Ok(())
}

#[test]
fn a_case_form_is_chosen_by_name_and_a_capital_starts_a_sentence() -> TestResult {
    let library: Term = ron::from_str(
        r#"(text: "Bibliothek", gender: Feminine, def: "die Bibliothek", def_dat: "der Bibliothek")"#,
    )?;
    let t = german_texts(
        r#"{ "built": "Waldheim hat {x:def} gebaut.", "in": "In {x:def_dat} brennt es.",
             "start": "{x:Def} brennt.", "missing": "Mit {x:def_gen}." }"#,
    );
    let args = Args::new().term("x", library);
    assert_eq!(t.get("built", &args), "Waldheim hat die Bibliothek gebaut.");
    assert_eq!(t.get("in", &args), "In der Bibliothek brennt es.");
    assert_eq!(t.get("start", &args), "Die Bibliothek brennt.");
    // A form the name lacks prints as the plain text.
    assert_eq!(t.get("missing", &args), "Mit Bibliothek.");
    Ok(())
}
