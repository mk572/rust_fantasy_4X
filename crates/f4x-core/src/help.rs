//! In-game help: topics and which screen areas open them. Content, not rules.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelpTopic {
    pub index: u16,
    pub title: String,
    pub text: String,
    /// `(picture file, entry)` shown beside the text.
    #[serde(default)]
    pub picture: Option<(String, u16)>,
    /// Another topic whose text is shown with this one.
    #[serde(default)]
    pub include: Option<u16>,
    /// What the topic explains when no screen area opens it: `ability:<id>` or `spell:<id>`.
    /// Found with [`Help::topic_for`]; a subject has one topic.
    #[serde(default)]
    pub subject: Option<String>,
}

/// Left, top, right and bottom in 320 by 200 screen coordinates.
pub type Rect = (i32, i32, i32, i32);

/// A clickable area of a screen. `help` is a topic index, or -1 for "no help here".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelpField {
    pub help: i32,
    /// `(x1, y1, x2, y2)` in 320x200 screen coordinates.
    pub rect: (i32, i32, i32, i32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelpScreen {
    pub id: u16,
    pub name: String,
    pub fields: Vec<HelpField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Help {
    pub topics: Vec<HelpTopic>,
    pub screens: Vec<HelpScreen>,
}

/// The help subject of an ability, as a topic's `subject` writes it.
pub fn ability_subject(id: &str) -> String {
    format!("ability:{id}")
}

/// The help subject of a spell.
pub fn spell_subject(id: &str) -> String {
    format!("spell:{id}")
}

impl Help {
    pub fn topic(&self, index: u16) -> Option<&HelpTopic> {
        self.topics.iter().find(|t| t.index == index)
    }

    /// The topic that explains `subject` (`ability:forester`, `spell:web`).
    pub fn topic_for(&self, subject: &str) -> Option<&HelpTopic> {
        self.topics
            .iter()
            .find(|t| t.subject.as_deref() == Some(subject))
    }

    /// The areas of `screen` (a [`HelpScreen::id`]).
    pub fn screen(&self, id: u16) -> Option<&HelpScreen> {
        self.screens.iter().find(|s| s.id == id)
    }

    /// The topic a click at `(x, y)` (320 by 200 coordinates) opens on `screen`: the first area
    /// that holds the point, if it names a topic. An area of -1 holds the point and opens none,
    /// so a click there is left to the game.
    pub fn at(&self, screen: u16, x: i32, y: i32) -> Option<(&HelpTopic, Rect)> {
        self.at_where(screen, x, y, |_| true)
    }

    /// [`Help::at`], looking only at the areas whose topic number `allowed` accepts: a screen
    /// that shows one button instead of others (the main screen's Next Turn over Done, Patrol,
    /// Wait and Build) leaves the covered areas out.
    pub fn at_where(
        &self,
        screen: u16,
        x: i32,
        y: i32,
        allowed: impl Fn(i32) -> bool,
    ) -> Option<(&HelpTopic, Rect)> {
        let field = self.screen(screen)?.fields.iter().find(|f| {
            let (x1, y1, x2, y2) = f.rect;
            allowed(f.help) && (x1..=x2).contains(&x) && (y1..=y2).contains(&y)
        })?;
        let topic = self.topic(u16::try_from(field.help).ok()?)?;
        Some((topic, field.rect))
    }

    /// The topics the screens' areas open that have no text yet, in order, once each.
    pub fn unwritten(&self) -> Vec<u16> {
        let mut out: Vec<u16> = self
            .screens
            .iter()
            .flat_map(|s| s.fields.iter())
            .filter_map(|f| u16::try_from(f.help).ok())
            .filter(|&t| self.topic(t).is_none())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Cross-reference problems; empty means consistent. An area may open a topic that is not
    /// written yet (see [`Help::unwritten`]); it then opens nothing. A topic number is used once,
    /// and no two topics repeat the same title and text.
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for t in &self.topics {
            if !seen.insert(t.index) {
                p.push(format!("help topic {} is there twice", t.index));
            }
        }
        let mut subjects = std::collections::HashSet::new();
        for t in &self.topics {
            if let Some(subject) = &t.subject {
                if !subjects.insert(subject) {
                    p.push(format!(
                        "help topic {} repeats the subject {subject}",
                        t.index
                    ));
                }
            }
        }
        for (i, t) in self.topics.iter().enumerate() {
            if let Some(other) = self.topics[..i]
                .iter()
                .find(|o| o.title.eq_ignore_ascii_case(&t.title) && o.text == t.text)
            {
                p.push(format!(
                    "help topics {} and {} have the same title and text",
                    other.index, t.index
                ));
            }
        }
        for t in &self.topics {
            if let Some(inc) = t.include {
                if self.topic(inc).is_none() {
                    p.push(format!(
                        "help topic {} includes unknown topic {inc}",
                        t.index
                    ));
                }
            }
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topic(index: u16, include: Option<u16>) -> HelpTopic {
        HelpTopic {
            index,
            title: format!("T{index}"),
            text: String::new(),
            picture: None,
            include,
            subject: None,
        }
    }

    #[test]
    fn a_subject_finds_its_topic_and_has_one() {
        let mut a = topic(1000, None);
        a.subject = Some("ability:forester".into());
        let mut b = topic(1001, None);
        b.subject = Some("spell:web".into());
        b.title = "Web".into();
        let mut help = Help {
            topics: vec![a.clone(), b],
            screens: Vec::new(),
        };
        assert_eq!(
            help.topic_for("ability:forester").map(|t| t.index),
            Some(1000)
        );
        assert_eq!(help.topic_for("spell:web").map(|t| t.index), Some(1001));
        assert!(help.topic_for("spell:none").is_none());
        assert!(help.problems().is_empty());
        let mut again = a;
        again.index = 1002;
        again.title = "Other".into();
        help.topics.push(again);
        assert!(help
            .problems()
            .iter()
            .any(|p| p.contains("repeats the subject")));
    }

    #[test]
    fn repeated_topics_are_problems() {
        let help = Help {
            topics: vec![topic(1, None), topic(1, None), topic(2, None)],
            screens: Vec::new(),
        };
        let problems = help.problems();
        assert!(problems
            .iter()
            .any(|p| p.contains("topic 1 is there twice")));
        let mut twin = topic(3, None);
        twin.title = "T2".into();
        let help = Help {
            topics: vec![topic(2, None), twin],
            screens: Vec::new(),
        };
        assert!(help
            .problems()
            .iter()
            .any(|p| p.contains("same title and text")));
    }

    #[test]
    fn finds_dangling_references() {
        let help = Help {
            topics: vec![topic(0, None), topic(1, Some(9))],
            screens: vec![HelpScreen {
                id: 0,
                name: "main".into(),
                fields: vec![
                    HelpField {
                        help: 0,
                        rect: (0, 0, 1, 1),
                    },
                    HelpField {
                        help: -1,
                        rect: (0, 0, 1, 1),
                    },
                    HelpField {
                        help: 7,
                        rect: (0, 0, 1, 1),
                    },
                ],
            }],
        };
        // Only the broken include is a problem; the area that opens topic 7 is just unwritten.
        let problems = help.problems();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(help.unwritten(), vec![7]);
        assert_eq!(help.at(0, 0, 0).map(|(t, _)| t.index), Some(0));
        assert!(help.at(0, 5, 5).is_none());
    }
}
