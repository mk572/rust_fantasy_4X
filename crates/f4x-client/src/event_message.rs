//! The message of a random event (`Get_Event_Message`): the client composes it from the
//! original's lines (`event_texts.ron`, from `EVENTMSG.LBX`) and the notice the game sends.
//! A count of nothing reads "no", the people lost are given in thousands, and the plural ending
//! is dropped for exactly one.

use std::path::Path;

use f4x_core::text::{Args, Texts};
use f4x_core::world_event::EventNotice;

/// One line of `EVENTMSG.LBX`, as the importer writes it to `event_texts.ron`: the event, its
/// voice (the line within the event's seven) and the text with `{city}`, `{size}`, `{player}`,
/// `{number}`, `{ore}`, `{item}`, `{units}`, `{people}`, `{buildings}` and the plural endings
/// `{units_s}` and `{buildings_s}` where the original has its data bytes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct EventText {
    pub event: u32,
    pub voice: u32,
    pub text: String,
}

/// The optional event messages (`event_texts.ron`) of a content directory. A directory without
/// them yields none, and the events are told without words.
pub fn load(dir: &Path) -> Result<Vec<EventText>, String> {
    let path = dir.join("event_texts.ron");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    ron::from_str(&text).map_err(|e| format!("event_texts.ron: {e}"))
}

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
            .replace("{size}", &notice.city_size)
            .replace("{player}", &notice.player_name)
            .replace("{number}", &notice.number.to_string())
            .replace("{ore}", &notice.ore)
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
    use f4x_core::testdir::TestResult;
    use f4x_core::world_event::{EventKind, Voice};

    /// The words the messages are filled with, as `content/text/en/event.ron` has them.
    fn words() -> Texts {
        let entries: std::collections::HashMap<String, f4x_core::text::Entry> = ron::from_str(
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
}
