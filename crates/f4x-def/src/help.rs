//! In-game help: topics and which screen areas open them. Content, not rules.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct HelpField {
    pub help: i32,
    /// `(x1, y1, x2, y2)` in 320x200 screen coordinates.
    pub rect: (i32, i32, i32, i32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

    /// The topic an entity's `help` names, if it names one.
    pub fn topic_of(&self, index: Option<u16>) -> Option<&HelpTopic> {
        self.topic(index?)
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
        // A chain of includes that comes back to where it began would never end.
        for t in &self.topics {
            let mut at = t;
            for _ in 0..self.topics.len() {
                match at.include.and_then(|i| self.topic(i)) {
                    Some(next) if next.index == t.index => {
                        p.push(format!("help topic {} includes itself", t.index));
                        break;
                    }
                    Some(next) => at = next,
                    None => break,
                }
            }
        }
        // The picture is a file name, not a path: it is looked up in one folder.
        for t in &self.topics {
            if let Some((file, _)) = &t.picture {
                let plain = !file.is_empty()
                    && !file.contains(['/', '\\', '\0', ':'])
                    && !file.starts_with('.');
                if !plain {
                    p.push(format!(
                        "help topic {} has the picture {file:?}, which is not a plain file name",
                        t.index
                    ));
                }
            }
        }
        p
    }
}
