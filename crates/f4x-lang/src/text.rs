//! Player-facing messages: looked up by key, with count, plural and gender.
//!
//! The words live in `content/text/<lang>/*.ron` of the client (one file per screen or domain,
//! see `docs/text.md`); code only names a key and gives the values to put in.
//!
//! Tests: in text/tests.rs.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::Path;

use f4x_def::text::{Gender, FALLBACK_LANGUAGE};
use serde::de::{self, MapAccess, Visitor};
use serde::Deserialize;
use serde::Deserializer;

/// A value given to a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    Text(String),
    Count(i64),
    /// A name with a gender (a hero or a wizard): prints as the name and selects `Gender` forms.
    Named(String, Gender),
    /// A name of the game's own tables (a unit, a race) with its forms and gender: `{name}` prints
    /// its text, `{name:dat}` the form of that name the language wrote, and a `gender` choice
    /// selects by its gender.
    Term(Term),
    /// Items joined by the language's list patterns (`list.two`, `list.start`, `list.middle`,
    /// `list.end`; each has `{a}` and `{b}`).
    List(Vec<String>),
}

/// A name that a language may spell differently by the place it stands in a sentence: its plain
/// text, its grammatical gender, and the forms (by a name the language chooses: `dat`, `gen`,
/// `dat_pl`, ...) a message asks for with `{name:form}`. A language that has no use for forms
/// writes the text only; a form the term lacks prints as the text.
///
/// In a `name/*.ron` table a term is a plain string (`76: "Dwarven Swordsmen"`) or a record
/// (`76: (text: "Zwergische Schwertkämpfer", gender: Masculine, dat: "Zwergischen Schwertkämpfern")`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Term {
    pub text: String,
    pub gender: Option<Gender>,
    /// A noun that is plural in form ("City Walls", "Spearmen", "Trade Goods"): a `number` choice
    /// takes its plural wording ("fall", "are") and a singular one ("falls", "is") for the rest.
    pub plural: bool,
    /// The name starts with a vowel sound, so English says "an Armory" (the default is "a").
    pub an: bool,
    pub forms: BTreeMap<String, String>,
}

impl Term {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// The name with its indefinite article (`{name:a}`; `{name:A}` capitalised for a sentence's
    /// start). A form `a` the language wrote wins; else English says "a"/"an" (`an` tag) and
    /// nothing before a plural noun; a language with no rule here prints the plain text.
    pub fn indefinite(&self, lang: &str) -> String {
        if let Some(form) = self.forms.get("a") {
            return form.clone();
        }
        match (lang, self.plural, self.an) {
            ("en", false, false) => format!("a {}", self.text),
            ("en", false, true) => format!("an {}", self.text),
            _ => self.text.clone(),
        }
    }

    /// A form as a message asks for it (`{name:form}`): `a` and `A` are the indefinite article's
    /// forms (`indefinite`), any other the language's own form or the plain text.
    pub fn render_form(&self, form: &str, lang: &str) -> String {
        match form {
            "a" => self.indefinite(lang),
            "A" => {
                let a = self.indefinite(lang);
                let mut chars = a.chars();
                chars
                    .next()
                    .into_iter()
                    .flat_map(char::to_uppercase)
                    .chain(chars)
                    .collect()
            }
            // A form name with a capital (`Def`) is the form (`def`) with its first letter capitalised,
            // for a sentence that starts with the name: "Die Stadt Waldheim ...".
            other if other.chars().next().is_some_and(char::is_uppercase) => {
                let mut name = other.chars();
                let lower: String = name
                    .next()
                    .into_iter()
                    .flat_map(char::to_lowercase)
                    .chain(name)
                    .collect();
                let form = self.form(&lower);
                let mut chars = form.chars();
                chars
                    .next()
                    .into_iter()
                    .flat_map(char::to_uppercase)
                    .chain(chars)
                    .collect()
            }
            other => self.form(other).to_string(),
        }
    }

    /// The named form, else the plain text.
    pub fn form(&self, name: &str) -> &str {
        self.forms
            .get(name)
            .map_or(self.text.as_str(), String::as_str)
    }
}

/// A field name of a record term: RON reads it as an identifier.
struct Field(String);

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Field;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("text, gender, plural, an or the name of a form")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Field, E> {
                Ok(Field(v.to_string()))
            }
        }
        d.deserialize_identifier(V)
    }
}

impl<'de> Deserialize<'de> for Term {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Term;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a text, or (text: .., gender: .., plural: .., an: .., form: ..)")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Term, E> {
                Ok(Term::plain(v))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Term, A::Error> {
                let mut term = Term::default();
                let mut has_text = false;
                while let Some(Field(key)) = m.next_key::<Field>()? {
                    match key.as_str() {
                        "text" => {
                            term.text = m.next_value()?;
                            has_text = true;
                        }
                        "gender" => term.gender = Some(m.next_value()?),
                        "plural" => term.plural = m.next_value()?,
                        "an" => term.an = m.next_value()?,
                        _ => {
                            term.forms.insert(key, m.next_value()?);
                        }
                    }
                }
                if has_text {
                    Ok(term)
                } else {
                    Err(de::Error::missing_field("text"))
                }
            }
        }
        d.deserialize_any(V)
    }
}

/// The values of one message, by the placeholder's name.
#[derive(Debug, Clone, Default)]
pub struct Args(Vec<(&'static str, Arg)>);

impl Args {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.0.push((name, Arg::Text(value.into())));
        self
    }

    pub fn count(mut self, name: &'static str, value: impl TryInto<i64>) -> Self {
        let n = value.try_into().unwrap_or(i64::MAX);
        self.0.push((name, Arg::Count(n)));
        self
    }

    pub fn list(mut self, name: &'static str, items: Vec<String>) -> Self {
        self.0.push((name, Arg::List(items)));
        self
    }

    pub fn named(mut self, name: &'static str, value: impl Into<String>, gender: Gender) -> Self {
        self.0.push((name, Arg::Named(value.into(), gender)));
        self
    }

    /// A name with its forms (`Names::unit_term` and the like); see `Arg::Term`.
    pub fn term(mut self, name: &'static str, value: Term) -> Self {
        self.0.push((name, Arg::Term(value)));
        self
    }

    fn get(&self, name: &str) -> Option<&Arg> {
        self.0.iter().find(|(n, _)| *n == name).map(|(_, a)| a)
    }
}

/// The plural forms a language tells apart (the CLDR categories that the game uses).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plural {
    One,
    Few,
    Many,
    Other,
}

/// The plural category of `n` in a language (two-letter code). Languages not listed have one
/// form, which is `Other`.
pub fn plural(lang: &str, n: i64) -> Plural {
    let n = n.unsigned_abs();
    match lang {
        "en" | "de" | "nl" | "sv" | "da" | "nb" | "es" | "it" | "fi" | "el" | "hu" | "tr" => {
            if n == 1 {
                Plural::One
            } else {
                Plural::Other
            }
        }
        "fr" | "pt" => {
            if n < 2 {
                Plural::One
            } else {
                Plural::Other
            }
        }
        "ru" | "uk" => match (n % 10, n % 100) {
            (1, r) if r != 11 => Plural::One,
            (2..=4, r) if !(12..=14).contains(&r) => Plural::Few,
            _ => Plural::Many,
        },
        "pl" => match (n, n % 10, n % 100) {
            (1, ..) => Plural::One,
            (_, 2..=4, r) if !(12..=14).contains(&r) => Plural::Few,
            _ => Plural::Many,
        },
        "cs" | "sk" => match n {
            1 => Plural::One,
            2..=4 => Plural::Few,
            _ => Plural::Other,
        },
        _ => Plural::Other,
    }
}

/// One message: a text, or the choice between texts by a count or by a gender. The forms of a
/// choice are entries themselves, so a gendered form may hold a count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Text(String),
    /// Chosen by the plural category of the count argument `by`.
    Count {
        by: String,
        zero: Option<Box<Entry>>,
        one: Option<Box<Entry>>,
        few: Option<Box<Entry>>,
        many: Option<Box<Entry>>,
        other: Box<Entry>,
    },
    /// One of the lines, chosen by the count argument `by` modulo the number of lines: a wizard's
    /// own way of saying the same thing.
    Pick {
        by: String,
        lines: Vec<Entry>,
    },
    /// Chosen by the number of the named term `by`: `plural` when it is tagged as a plural noun.
    Number {
        by: String,
        singular: Box<Entry>,
        plural: Box<Entry>,
    },
    /// Chosen by the gender of the named argument `by`.
    Gender {
        by: String,
        masculine: Box<Entry>,
        feminine: Box<Entry>,
        neuter: Box<Entry>,
    },
}

/// The fields of an entry that has forms.
#[derive(PartialEq, Eq, Hash)]
enum Key {
    Count,
    Gender,
    Number,
    Pick,
    Lines,
    Zero,
    One,
    Few,
    Many,
    Other,
    Masculine,
    Feminine,
    Neuter,
    Singular,
    Plural,
}

/// RON reads the names of a struct's fields as identifiers, which a derived enum would not.
impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Key;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("count, gender or the name of a form")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Key, E> {
                Ok(match v {
                    "count" => Key::Count,
                    "gender" => Key::Gender,
                    "number" => Key::Number,
                    "singular" => Key::Singular,
                    "plural" => Key::Plural,
                    "pick" => Key::Pick,
                    "lines" => Key::Lines,
                    "zero" => Key::Zero,
                    "one" => Key::One,
                    "few" => Key::Few,
                    "many" => Key::Many,
                    "other" => Key::Other,
                    "masculine" => Key::Masculine,
                    "feminine" => Key::Feminine,
                    "neuter" => Key::Neuter,
                    other => return Err(E::unknown_field(other, &["count", "gender"])),
                })
            }
        }
        d.deserialize_identifier(V)
    }
}

impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Entry;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a text, or (count: ..) / (gender: ..) with its forms")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Entry, E> {
                Ok(Entry::Text(v.to_string()))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Entry, A::Error> {
                let mut count = None;
                let mut gender = None;
                let mut number = None;
                let mut pick = None;
                let mut lines = None;
                let mut forms: HashMap<Key, Entry> = HashMap::new();
                while let Some(key) = m.next_key::<Key>()? {
                    match key {
                        Key::Count => count = Some(m.next_value::<String>()?),
                        Key::Gender => gender = Some(m.next_value::<String>()?),
                        Key::Number => number = Some(m.next_value::<String>()?),
                        Key::Pick => pick = Some(m.next_value::<String>()?),
                        Key::Lines => lines = Some(m.next_value::<Vec<Entry>>()?),
                        form => {
                            forms.insert(form, m.next_value()?);
                        }
                    }
                }
                let mut take = |key: Key| forms.remove(&key).map(Box::new);
                match (count, gender, pick, number) {
                    (None, None, None, Some(by)) => Ok(Entry::Number {
                        by,
                        singular: take(Key::Singular)
                            .ok_or_else(|| de::Error::missing_field("singular"))?,
                        plural: take(Key::Plural)
                            .ok_or_else(|| de::Error::missing_field("plural"))?,
                    }),
                    (None, None, Some(by), None) => {
                        let lines = lines
                            .filter(|l| !l.is_empty())
                            .ok_or_else(|| de::Error::missing_field("lines"))?;
                        Ok(Entry::Pick { by, lines })
                    }
                    (Some(by), None, None, None) => Ok(Entry::Count {
                        by,
                        zero: take(Key::Zero),
                        one: take(Key::One),
                        few: take(Key::Few),
                        many: take(Key::Many),
                        other: take(Key::Other).ok_or_else(|| de::Error::missing_field("other"))?,
                    }),
                    (None, Some(by), None, None) => Ok(Entry::Gender {
                        by,
                        masculine: take(Key::Masculine)
                            .ok_or_else(|| de::Error::missing_field("masculine"))?,
                        feminine: take(Key::Feminine)
                            .ok_or_else(|| de::Error::missing_field("feminine"))?,
                        neuter: take(Key::Neuter)
                            .ok_or_else(|| de::Error::missing_field("neuter"))?,
                    }),
                    _ => Err(de::Error::custom(
                        "an entry has exactly one of count, gender, number and pick",
                    )),
                }
            }
        }
        d.deserialize_any(V)
    }
}

impl Entry {
    /// The names of the arguments the entry uses: the placeholders of its texts (without a form,
    /// `{unit:dat}` is `unit`) and the arguments its choices read. A translation must use the same
    /// ones as the original (`german_messages_*` in tests).
    #[cfg(test)]
    pub(crate) fn arguments(&self) -> std::collections::BTreeSet<String> {
        fn in_text(t: &str, out: &mut std::collections::BTreeSet<String>) {
            let mut rest = t;
            while let Some(open) = rest.find('{') {
                if rest[open..].starts_with("{{") {
                    rest = &rest[open + 2..];
                    continue;
                }
                let Some(close) = rest[open..].find('}') else {
                    return;
                };
                let name = &rest[open + 1..open + close];
                out.insert(name.split(':').next().unwrap_or(name).to_string());
                rest = &rest[open + close + 1..];
            }
        }
        fn walk(e: &Entry, out: &mut std::collections::BTreeSet<String>) {
            match e {
                Entry::Text(t) => in_text(t, out),
                Entry::Count {
                    by,
                    zero,
                    one,
                    few,
                    many,
                    other,
                } => {
                    out.insert(by.clone());
                    for f in [zero, one, few, many].into_iter().flatten() {
                        walk(f, out);
                    }
                    walk(other, out);
                }
                Entry::Pick { by, lines } => {
                    out.insert(by.clone());
                    lines.iter().for_each(|l| walk(l, out));
                }
                Entry::Number {
                    by,
                    singular,
                    plural,
                } => {
                    out.insert(by.clone());
                    walk(singular, out);
                    walk(plural, out);
                }
                Entry::Gender {
                    by,
                    masculine,
                    feminine,
                    neuter,
                } => {
                    out.insert(by.clone());
                    for f in [masculine, feminine, neuter] {
                        walk(f, out);
                    }
                }
            }
        }
        let mut out = std::collections::BTreeSet::new();
        walk(self, &mut out);
        out
    }

    fn render(&self, lang: &str, args: &Args, out: &mut String) {
        match self {
            Entry::Text(t) => substitute(lang, t, args, out),
            Entry::Count {
                by,
                zero,
                one,
                few,
                many,
                other,
            } => {
                let n = match args.get(by) {
                    Some(Arg::Count(n)) => *n,
                    _ => 0,
                };
                let form = match plural(lang, n) {
                    _ if n == 0 && zero.is_some() => zero,
                    Plural::One => one,
                    Plural::Few => few,
                    Plural::Many => many,
                    _ => &None,
                };
                form.as_deref().unwrap_or(other).render(lang, args, out);
            }
            Entry::Pick { by, lines } => {
                let n = match args.get(by) {
                    Some(Arg::Count(n)) => n.unsigned_abs() as usize,
                    _ => 0,
                };
                lines[n % lines.len()].render(lang, args, out);
            }
            Entry::Number {
                by,
                singular,
                plural,
            } => match args.get(by) {
                Some(Arg::Term(t)) if t.plural => plural,
                _ => singular,
            }
            .render(lang, args, out),
            Entry::Gender {
                by,
                masculine,
                feminine,
                neuter,
            } => {
                let gender = match args.get(by) {
                    Some(Arg::Named(_, g)) => *g,
                    Some(Arg::Term(t)) => t.gender.unwrap_or_default(),
                    _ => Gender::Neuter,
                };
                match gender {
                    Gender::Masculine => masculine,
                    Gender::Feminine => feminine,
                    Gender::Neuter => neuter,
                }
                .render(lang, args, out);
            }
        }
    }
}

fn substitute(lang: &str, text: &str, args: &Args, out: &mut String) {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '{' => {
                let name: String = chars.by_ref().take_while(|c| *c != '}').collect();
                let (name, form) = match name.split_once(':') {
                    Some((n, f)) => (n.to_string(), Some(f.to_string())),
                    None => (name, None),
                };
                match args.get(&name) {
                    Some(Arg::Term(t)) => match form.as_deref() {
                        Some(f) => out.push_str(&t.render_form(f, lang)),
                        None => out.push_str(&t.text),
                    },
                    Some(Arg::Text(t)) | Some(Arg::Named(t, _)) => out.push_str(t),
                    Some(Arg::Count(n)) => out.push_str(&n.to_string()),
                    Some(Arg::List(items)) => out.push_str(&items.join(", ")),
                    None => {
                        out.push('{');
                        out.push_str(&name);
                        out.push('}');
                    }
                }
            }
            c => out.push(c),
        }
    }
}

/// All the messages of one language, with the fallback language behind them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Texts {
    lang: String,
    entries: HashMap<String, Entry>,
    fallback_lang: String,
    fallback: HashMap<String, Entry>,
}

impl Texts {
    /// A language's messages, and the fallback's (usually `en`) for the keys it lacks.
    pub fn new(
        lang: &str,
        entries: HashMap<String, Entry>,
        fallback_lang: &str,
        fallback: HashMap<String, Entry>,
    ) -> Self {
        Self {
            lang: lang.to_string(),
            entries,
            fallback_lang: fallback_lang.to_string(),
            fallback,
        }
    }

    /// Items as a list: "a", "a and b", "a, b and c" in English; the patterns are messages.
    pub fn join(&self, items: &[String]) -> String {
        let pair =
            |key: &str, a: &str, b: &str| self.get(key, &Args::new().text("a", a).text("b", b));
        match items {
            [] => String::new(),
            [one] => one.clone(),
            [a, b] => pair("list.two", a, b),
            [first, middle @ .., last] => {
                let mut tail = middle.iter().fold(String::new(), |acc, item| {
                    if acc.is_empty() {
                        item.clone()
                    } else {
                        pair("list.middle", &acc, item)
                    }
                });
                if tail.is_empty() {
                    tail = last.clone();
                } else {
                    tail = pair("list.end", &tail, last);
                }
                pair("list.start", first, &tail)
            }
        }
    }

    /// The language's code (`en`), for what renders a term outside a message.
    pub fn language(&self) -> &str {
        &self.lang
    }

    /// Whether the language itself (not only the fallback) has a message for the key.
    pub fn has_own(&self, key: &str) -> bool {
        self.entries.contains_key(key)
            || (self.lang == self.fallback_lang && self.fallback.contains_key(key))
    }

    /// Whether the language or the fallback has a message for the key.
    pub fn has(&self, key: &str) -> bool {
        self.entries.contains_key(key) || self.fallback.contains_key(key)
    }

    /// The message with the values put in. A key nobody wrote prints as the key.
    /// An amount counted in halves as text: `3`, `½` or `3½`.
    pub fn half_amount(&self, amount: f4x_def::half_int::HalfInt) -> String {
        if amount < f4x_def::half_int::HalfInt::ZERO {
            return format!("-{}", self.half_amount(-amount));
        }
        let whole = amount.floor();
        match (amount.is_whole(), whole) {
            (true, _) => whole.to_string(),
            (false, 0) => self.get("ui.amount_half", &Args::new()),
            (false, _) => self.get("ui.amount_whole_and_half", &Args::new().count("n", whole)),
        }
    }

    pub fn get(&self, key: &str, args: &Args) -> String {
        let joined;
        let args = if args.0.iter().any(|(_, a)| matches!(a, Arg::List(_))) {
            joined = Args(
                args.0
                    .iter()
                    .map(|(n, a)| match a {
                        Arg::List(items) => (*n, Arg::Text(self.join(items))),
                        other => (*n, other.clone()),
                    })
                    .collect(),
            );
            &joined
        } else {
            args
        };
        let mut out = String::new();
        if let Some(e) = self.entries.get(key) {
            e.render(&self.lang, args, &mut out);
        } else if let Some(e) = self.fallback.get(key) {
            e.render(&self.fallback_lang, args, &mut out);
        } else {
            out.push_str(key);
        }
        out
    }
}

/// One language's messages: every `*.ron` file of `text/<lang>/` in each directory. A key may be
/// in one file only; a directory earlier in `dirs` wins over a later one for a key both have.
fn load_language(dirs: &[&Path], lang: &str) -> Result<HashMap<String, Entry>, String> {
    let mut all = HashMap::new();
    for dir in dirs.iter().rev() {
        let folder = dir.join("text").join(lang);
        let Ok(files) = std::fs::read_dir(&folder) else {
            continue;
        };
        let mut paths: Vec<_> = files
            .filter_map(|f| f.ok().map(|f| f.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "ron"))
            .collect();
        paths.sort();
        let mut here: HashMap<String, Entry> = HashMap::new();
        let mut problems = Vec::new();
        for path in paths {
            let src =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let entries: HashMap<String, Entry> =
                ron::from_str(&src).map_err(|e| format!("{}: {e}", path.display()))?;
            for (key, entry) in entries {
                if here.insert(key.clone(), entry).is_some() {
                    problems.push(format!(
                        "{}: the key {key} is in two files",
                        folder.display()
                    ));
                }
            }
        }
        if !problems.is_empty() {
            return Err(problems.join("; "));
        }
        all.extend(here);
    }
    Ok(all)
}

/// The messages in `lang`, with those of the fallback language behind them.
pub fn load_texts(dirs: &[&Path], lang: &str) -> Result<Texts, String> {
    let fallback = load_language(dirs, FALLBACK_LANGUAGE)?;
    let entries = if lang == FALLBACK_LANGUAGE {
        HashMap::new()
    } else {
        load_language(dirs, lang)?
    };
    Ok(Texts::new(lang, entries, FALLBACK_LANGUAGE, fallback))
}

#[cfg(test)]
mod tests;
