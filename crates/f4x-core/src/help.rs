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

impl Help {
    pub fn topic(&self, index: u16) -> Option<&HelpTopic> {
        self.topics.iter().find(|t| t.index == index)
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
    /// written yet (see [`Help::unwritten`]); it then opens nothing.
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
        // Only the broken include is a problem; the area that opens topic 7 is just unwritten.
        let problems = help.problems();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(help.unwritten(), vec![7]);
        assert_eq!(help.at(0, 0, 0).map(|(t, _)| t.index), Some(0));
        assert!(help.at(0, 5, 5).is_none());
    }
}
