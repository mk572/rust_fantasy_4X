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
    pub fn problems(&self) -> Vec<crate::issue::ContentIssue> {
        let mut p = crate::issue::Issues::new();
        for i in self.twice() {
            p.push(duplicate_number(self.topics[i].index));
        }
        for (i, o) in self.same_words() {
            p.push(same_words(self.topics[i].index, self.topics[o].index));
        }
        for i in self.unknown_includes() {
            p.push(unknown_include(&self.topics[i]));
        }
        for i in self.circles() {
            p.push(circle(self.topics[i].index));
        }
        for i in self.bad_pictures() {
            p.push(bad_picture(&self.topics[i]));
        }
        p.into_vec()
    }

    /// The help with every bad part left out, and what was left out (each a `Degrade` issue of
    /// [`Element::Help`]): a topic that repeats a number or another topic's words is dropped, the first
    /// stays; an `include` of nothing or in a circle, and a picture that is not a plain file name, are
    /// cleared. The area of a screen that opens a topic that is gone opens nothing.
    pub fn repaired(mut self) -> (Help, Vec<crate::issue::ContentIssue>) {
        use crate::issue::Element;
        let mut out = Vec::new();
        let degrade = |i: crate::issue::ContentIssue| i.degrading(Element::Help);
        // Topics out: repeated numbers and repeated words (the later of the two).
        let twice = self.twice();
        let same: Vec<usize> = self.same_words().into_iter().map(|(i, _)| i).collect();
        for &i in &twice {
            out.push(degrade(duplicate_number(self.topics[i].index)));
        }
        for (i, o) in self.same_words() {
            if !twice.contains(&i) {
                out.push(degrade(same_words(
                    self.topics[i].index,
                    self.topics[o].index,
                )));
            }
        }
        let mut at = 0;
        self.topics.retain(|_| {
            at += 1;
            !twice.contains(&(at - 1)) && !same.contains(&(at - 1))
        });
        // Includes of nothing (also of a topic that has just gone).
        for i in self.unknown_includes() {
            out.push(degrade(unknown_include(&self.topics[i])));
            self.topics[i].include = None;
        }
        // Circles: clear the include of one topic of a circle until none is left.
        while let Some(&i) = self.circles().first() {
            out.push(degrade(circle(self.topics[i].index)));
            self.topics[i].include = None;
        }
        for i in self.bad_pictures() {
            out.push(degrade(bad_picture(&self.topics[i])));
            self.topics[i].picture = None;
        }
        (self, out)
    }

    /// The topics (by position) whose number an earlier topic has.
    fn twice(&self) -> Vec<usize> {
        let mut seen = std::collections::HashSet::new();
        (0..self.topics.len())
            .filter(|&i| !seen.insert(self.topics[i].index))
            .collect()
    }

    /// `(later, earlier)` topics that have the same title and text.
    fn same_words(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (i, t) in self.topics.iter().enumerate() {
            if let Some(o) = self.topics[..i]
                .iter()
                .position(|o| o.title.eq_ignore_ascii_case(&t.title) && o.text == t.text)
            {
                out.push((i, o));
            }
        }
        out
    }

    fn unknown_includes(&self) -> Vec<usize> {
        (0..self.topics.len())
            .filter(|&i| {
                self.topics[i]
                    .include
                    .is_some_and(|inc| self.topic(inc).is_none())
            })
            .collect()
    }

    /// The topics whose chain of includes comes back to them.
    fn circles(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for (i, t) in self.topics.iter().enumerate() {
            let mut at = t;
            for _ in 0..self.topics.len() {
                match at.include.and_then(|n| self.topic(n)) {
                    Some(next) if next.index == t.index => {
                        out.push(i);
                        break;
                    }
                    Some(next) => at = next,
                    None => break,
                }
            }
        }
        out
    }

    /// The picture is a file name, not a path: it is looked up in one folder.
    fn bad_pictures(&self) -> Vec<usize> {
        (0..self.topics.len())
            .filter(|&i| {
                self.topics[i].picture.as_ref().is_some_and(|(file, _)| {
                    file.is_empty()
                        || file.contains(['/', '\\', '\0', ':'])
                        || file.starts_with('.')
                })
            })
            .collect()
    }
}

const F: &str = "help.ron";

fn duplicate_number(index: u16) -> crate::issue::ContentIssue {
    crate::issue::ContentIssue::new(
        F,
        format!("help topic {index}"),
        crate::issue::IssueKind::Duplicate,
    )
    .with(["the topic number".to_string(), index.to_string()])
    .about_row(index)
}

fn same_words(index: u16, other: u16) -> crate::issue::ContentIssue {
    crate::issue::ContentIssue::new(
        F,
        format!("help topic {index}"),
        crate::issue::IssueKind::Conflict,
    )
    .with([format!("it has the same title and text as topic {other}")])
    .about_row(index)
}

fn unknown_include(t: &HelpTopic) -> crate::issue::ContentIssue {
    let inc = t.include.unwrap_or_default();
    crate::issue::ContentIssue::new(
        F,
        format!("help topic {}", t.index),
        crate::issue::IssueKind::Dangling,
    )
    .with([
        "include".to_string(),
        "help topic".to_string(),
        inc.to_string(),
    ])
    .about_row(t.index)
}

fn circle(index: u16) -> crate::issue::ContentIssue {
    crate::issue::ContentIssue::new(
        F,
        format!("help topic {index}"),
        crate::issue::IssueKind::Conflict,
    )
    .with(["it includes itself".to_string()])
    .about_row(index)
}

fn bad_picture(t: &HelpTopic) -> crate::issue::ContentIssue {
    let file = t
        .picture
        .as_ref()
        .map(|(f, _)| f.as_str())
        .unwrap_or_default();
    crate::issue::ContentIssue::new(
        F,
        format!("help topic {}", t.index),
        crate::issue::IssueKind::OutOfRange,
    )
    .with([
        "picture".to_string(),
        format!("{file:?}"),
        "a plain file name".to_string(),
    ])
    .about_row(t.index)
}
