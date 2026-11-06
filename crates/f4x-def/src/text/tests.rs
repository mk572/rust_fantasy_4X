use super::*;
use crate::testdir::TestResult;

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
