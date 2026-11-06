//! Player-facing messages: looked up by key, with count, plural and gender.
//!
//! The words live in `content/text/<lang>/*.ron` (one file per screen or domain, see
//! `docs/text.md`); code only names a key and gives the values to put in.
//!
//! Tests: in text/tests.rs.

use std::collections::HashMap;
use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

/// The language that fills in the keys another language lacks.
pub const FALLBACK_LANGUAGE: &str = "en";

/// The grammatical gender of a hero or a wizard, for the messages that name one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Gender {
    Masculine,
    Feminine,
    #[default]
    Neuter,
}

/// A value given to a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    Text(String),
    Count(i64),
    /// A name with a gender (a hero or a wizard): prints as the name and selects `Gender` forms.
    Named(String, Gender),
    /// Items joined by the language's list patterns (`list.two`, `list.start`, `list.middle`,
    /// `list.end`; each has `{a}` and `{b}`).
    List(Vec<String>),
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

    fn get(&self, name: &str) -> Option<&Arg> {
        self.0.iter().find(|(n, _)| *n == name).map(|(_, a)| a)
    }
}

/// The plural forms a language tells apart (the CLDR categories that the game uses).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plural {
    Zero,
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
                let mut pick = None;
                let mut lines = None;
                let mut forms: HashMap<Key, Entry> = HashMap::new();
                while let Some(key) = m.next_key::<Key>()? {
                    match key {
                        Key::Count => count = Some(m.next_value::<String>()?),
                        Key::Gender => gender = Some(m.next_value::<String>()?),
                        Key::Pick => pick = Some(m.next_value::<String>()?),
                        Key::Lines => lines = Some(m.next_value::<Vec<Entry>>()?),
                        form => {
                            forms.insert(form, m.next_value()?);
                        }
                    }
                }
                let mut take = |key: Key| forms.remove(&key).map(Box::new);
                match (count, gender, pick) {
                    (None, None, Some(by)) => {
                        let lines = lines
                            .filter(|l| !l.is_empty())
                            .ok_or_else(|| de::Error::missing_field("lines"))?;
                        Ok(Entry::Pick { by, lines })
                    }
                    (Some(by), None, None) => Ok(Entry::Count {
                        by,
                        zero: take(Key::Zero),
                        one: take(Key::One),
                        few: take(Key::Few),
                        many: take(Key::Many),
                        other: take(Key::Other).ok_or_else(|| de::Error::missing_field("other"))?,
                    }),
                    (None, Some(by), None) => Ok(Entry::Gender {
                        by,
                        masculine: take(Key::Masculine)
                            .ok_or_else(|| de::Error::missing_field("masculine"))?,
                        feminine: take(Key::Feminine)
                            .ok_or_else(|| de::Error::missing_field("feminine"))?,
                        neuter: take(Key::Neuter)
                            .ok_or_else(|| de::Error::missing_field("neuter"))?,
                    }),
                    _ => Err(de::Error::custom(
                        "an entry has exactly one of count, gender and pick",
                    )),
                }
            }
        }
        d.deserialize_any(V)
    }
}

impl Entry {
    fn render(&self, lang: &str, args: &Args, out: &mut String) {
        match self {
            Entry::Text(t) => substitute(t, args, out),
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
            Entry::Gender {
                by,
                masculine,
                feminine,
                neuter,
            } => {
                let gender = match args.get(by) {
                    Some(Arg::Named(_, g)) => *g,
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

fn substitute(text: &str, args: &Args, out: &mut String) {
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
                match args.get(&name) {
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

    pub fn has(&self, key: &str) -> bool {
        self.entries.contains_key(key) || self.fallback.contains_key(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.keys().chain(self.fallback.keys())
    }

    /// The message with the values put in. A key nobody wrote prints as the key.
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

#[cfg(test)]
mod tests;
