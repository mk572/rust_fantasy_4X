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
}

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

impl Help {
    pub fn topic(&self, index: u16) -> Option<&HelpTopic> {
        self.topics.iter().find(|t| t.index == index)
    }

    /// Cross-reference problems; empty means consistent.
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
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
        for s in &self.screens {
            for f in &s.fields {
                if f.help >= 0 && self.topic(f.help as u16).is_none() {
                    p.push(format!(
                        "help screen {:?} points at unknown topic {}",
                        s.name, f.help
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
        }
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
        let problems = help.problems();
        assert_eq!(problems.len(), 2, "{problems:?}");
    }
}
