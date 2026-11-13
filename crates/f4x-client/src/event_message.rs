//! The message of a random event: the client composes it from the game's sentences
//! (`text/<lang>/message/event_texts.ron`) and the notice the game sends.
//! A count of nothing reads "no", the people lost are given in thousands, and the plural ending
//! is dropped for exactly one.

use std::path::Path;

use crate::text::{Args, Texts};
use f4x_core::world_event::EventNotice;

/// One sentence of `event_texts.ron`: the event, its voice (see [`Voice`](f4x_core::world_event::Voice))
/// and the text with `{city}`, `{size}`, `{player}`, `{number}`, `{ore}`, `{item}`, `{units}`,
/// `{people}`, `{buildings}` and the plural endings `{units_s}` and `{buildings_s}`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct EventText {
    pub event: u32,
    pub voice: u32,
    pub text: String,
}

/// The event sentences (`event_texts.ron`) of a folder. A folder without
/// them yields none, and the events are told without words.
pub fn load(dir: &Path) -> Result<Vec<EventText>, String> {
    let path = dir.join("event_texts.ron");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    ron::from_str(&text).map_err(|e| format!("event_texts.ron: {e}"))
}

/// The keys of the names of the city size classes, outpost to capital.
const CITY_SIZE_KEYS: &[&str] = &[
    "city.size_outpost",
    "city.size_hamlet",
    "city.size_village",
    "city.size_town",
    "city.size_city",
    "city.size_capital",
];

/// The minerals an event can put on the map: the feature number and the key of the word for it,
/// what follows the article in "a{ore} deposit".
const ORE_KEYS: &[(u16, &str)] = &[
    (11 /* iron_ore */, "event.ore_iron_ore"),
    (5 /* coal */, "event.ore_coal"),
    (19 /* silver_ore */, "event.ore_silver_ore"),
    (10 /* gold_ore */, "event.ore_gold_ore"),
    (9 /* gem */, "event.ore_gem"),
    (12 /* mithril_ore */, "event.ore_mithril_ore"),
    (2 /* adamantium_ore */, "event.ore_adamantium_ore"),
    (17 /* quork_crystal */, "event.ore_quork_crystal"),
    (6 /* crysx_crystal */, "event.ore_crysx_crystal"),
];

/// The message of a notice, if the lines have one for it; `item` is the name of the gift.
pub fn compose(
    lines: &[EventText],
    words: &Texts,
    notice: &EventNotice,
    item: &str,
) -> Option<String> {
    let line = lines
        .iter()
        .find(|t| t.event == notice.kind.number() && t.voice == notice.voice as u32)?;
    let count = |n: u32, key: &str| {
        if n == 0 {
            words.get("event.none", &Args::new())
        } else {
            words.get(key, &Args::new().count("n", n as i32))
        }
    };
    let plural = |n: u32| words.get("event.plural_ending", &Args::new().count("n", n as i32));
    Some(
        line.text
            .replace("{city}", &notice.city_name)
            .replace(
                "{size}",
                &words.get(
                    CITY_SIZE_KEYS[notice.city_size.min(5) as usize],
                    &Args::new(),
                ),
            )
            .replace("{player}", &notice.player_name)
            .replace("{number}", &notice.number.to_string())
            .replace(
                "{ore}",
                &notice
                    .ore
                    .and_then(|num| ORE_KEYS.iter().find(|&&(n, _)| n == num))
                    .map_or(String::new(), |&(_, key)| words.get(key, &Args::new())),
            )
            .replace("{item}", item)
            .replace("{units}", &count(notice.lost_units, "event.number"))
            .replace("{people}", &count(notice.lost_population, "event.thousand"))
            .replace("{buildings}", &count(notice.lost_buildings, "event.number"))
            .replace("{units_s}", &plural(notice.lost_units))
            .replace("{buildings_s}", &plural(notice.lost_buildings)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::world_event::{EventKind, Voice};
    use f4x_def::testdir::TestResult;

    /// The words the messages are filled with, as `content/text/en/event.ron` has them.
    fn words() -> Texts {
        let entries: std::collections::HashMap<String, crate::text::Entry> = ron::from_str(
            r#"{
                "event.none": "no",
                "event.number": "{n}",
                "event.thousand": "{n} thousand",
                "event.plural_ending": (count: "n", one: "", other: "s"),
            }"#,
        )
        .expect("value succeeds");
        Texts::new("en", entries, "en", Default::default())
    }

    fn text(event: u32, voice: u32, text: &str) -> EventText {
        EventText {
            event,
            voice,
            text: text.into(),
        }
    }

    #[test]
    fn a_message_fills_in_the_names_and_counts() -> TestResult {
        let texts = [text(
            1,
            0,
            "Hit {city} killing {people} townsfolk and {units} unit{units_s}, destroying {buildings} building{buildings_s}.",
        )];
        let mut n = EventNotice::new(EventKind::GreatMeteor, Voice::Started);
        n.city_name = "Rome".into();
        n.lost_population = 3;
        n.lost_units = 1;
        n.lost_buildings = 0;
        assert_eq!(
            compose(&texts, &words(), &n, "").ok_or("compose is missing")?,
            "Hit Rome killing 3 thousand townsfolk and 1 unit, destroying no buildings."
        );
        n.lost_units = 4;
        n.lost_buildings = 1;
        assert_eq!(
            compose(&texts, &words(), &n, "").ok_or("compose is missing")?,
            "Hit Rome killing 3 thousand townsfolk and 4 units, destroying 1 building."
        );
        Ok(())
    }

    #[test]
    fn the_message_is_the_one_of_the_events_voice() -> TestResult {
        let texts = [
            text(7, 0, "yours {size} of {city}"),
            text(7, 1, "over in {city}"),
            text(7, 4, "{player}'s"),
        ];
        let mut n = EventNotice::new(EventKind::Plague, Voice::Ended);
        n.city_name = "Oslo".into();
        assert_eq!(
            compose(&texts, &words(), &n, "").ok_or("compose is missing")?,
            "over in Oslo"
        );
        n.voice = Voice::Elsewhere;
        n.player_name = "Rjak".into();
        assert_eq!(
            compose(&texts, &words(), &n, "").ok_or("compose is missing")?,
            "Rjak's"
        );
        n.voice = Voice::Happened;
        assert_eq!(compose(&texts, &words(), &n, ""), None);
        Ok(())
    }

    /// The shipped English sentences give every notice the game can send a message, and use only
    /// the placeholders `compose` fills in.
    #[test]
    fn the_shipped_sentences_cover_every_notice() -> TestResult {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../f4x-lang/content/text/en/message");
        let lines = load(&dir)?;
        assert!(!lines.is_empty(), "the shipped event sentences are missing");
        let known = [
            "city",
            "size",
            "player",
            "number",
            "ore",
            "item",
            "units",
            "people",
            "buildings",
            "units_s",
            "buildings_s",
        ];
        for kind in EventKind::all() {
            // The voices the game gives each kind: see `world_event::game`.
            let voices: &[Voice] = if kind == EventKind::Disjunction {
                &[Voice::Happened]
            } else if kind.is_world() {
                &[Voice::Happened, Voice::Ended]
            } else if kind.lasts() {
                &[Voice::Started, Voice::Ended, Voice::Elsewhere]
            } else {
                &[Voice::Started, Voice::Elsewhere]
            };
            for &voice in voices {
                let n = lines
                    .iter()
                    .filter(|t| t.event == kind.number() && t.voice == voice as u32)
                    .count();
                assert_eq!(n, 1, "{kind:?} {voice:?}: one sentence expected, found {n}");
            }
        }
        for t in &lines {
            let mut rest = t.text.as_str();
            while let Some(open) = rest.find('{') {
                let close = rest[open..].find('}').ok_or("an unclosed placeholder")? + open;
                let name = &rest[open + 1..close];
                assert!(
                    known.contains(&name),
                    "event {}: unknown {{{name}}}",
                    t.event
                );
                rest = &rest[close + 1..];
            }
        }
        Ok(())
    }
}
