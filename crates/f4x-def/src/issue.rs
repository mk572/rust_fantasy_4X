//! What is wrong with a content file, as data: the file, where in it, what kind of thing, the values
//! the message needs, and what the game does about it (`docs/content-policy.md`).
//!
//! `f4x-def` never writes the sentence the player reads: the client turns an issue into text with the
//! key of its kind (`IssueKind::text_key`) and the details as parameters (task 768). [`Display`] is the
//! plain English form for the tools and the log.
//!
//! Tests: `tests/it/` (`docs/f4x-def-test-design.md`).

use std::fmt;

/// What sort of thing is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IssueKind {
    /// A file, a row or a field that must be there is not.
    Missing,
    /// A file that is there cannot be read (a folder, no permission, not UTF-8).
    Unreadable,
    /// The text is not the format at all (a cut file, binary, a syntax error).
    Corrupt,
    /// A value of another type than the field's.
    WrongType,
    /// A value outside what the field allows.
    OutOfRange,
    /// A reference to a row that does not exist.
    Dangling,
    /// A number, an id or a name used twice.
    Duplicate,
    /// A key that no type has (a typo).
    UnknownKey,
    /// Values that are each fine but contradict each other.
    Conflict,
}

impl IssueKind {
    /// The key of the sentence for this kind in the client's texts (`content_issue.<kind>`).
    pub fn text_key(self) -> &'static str {
        match self {
            IssueKind::Missing => "content_issue.missing",
            IssueKind::Unreadable => "content_issue.unreadable",
            IssueKind::Corrupt => "content_issue.corrupt",
            IssueKind::WrongType => "content_issue.wrong_type",
            IssueKind::OutOfRange => "content_issue.out_of_range",
            IssueKind::Dangling => "content_issue.dangling",
            IssueKind::Duplicate => "content_issue.duplicate",
            IssueKind::UnknownKey => "content_issue.unknown_key",
            IssueKind::Conflict => "content_issue.conflict",
        }
    }

    fn label(self) -> &'static str {
        match self {
            IssueKind::Missing => "missing",
            IssueKind::Unreadable => "cannot be read",
            IssueKind::Corrupt => "is not valid",
            IssueKind::WrongType => "has the wrong type",
            IssueKind::OutOfRange => "is out of range",
            IssueKind::Dangling => "refers to nothing",
            IssueKind::Duplicate => "is used twice",
            IssueKind::UnknownKey => "is a key nothing has",
            IssueKind::Conflict => "contradicts the rest",
        }
    }
}

/// The parts of the content that a bad row may be dropped from without refusing the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Element {
    Wizard,
    Rating,
    Efficiency,
    Help,
    Item,
}

/// What the game does about an issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// The game does not start (or the save does not load).
    Refuse,
    /// The game plays without that element.
    Degrade(Element),
}

/// One problem with the content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentIssue {
    /// The file, as the content folder names it (`units.ron`), or the path when it is outside it.
    pub file: String,
    /// Where in the file: a row and a field (`units[12].cost`, `unit 12: cost`), a line and a column
    /// for a syntax error, empty for the whole file.
    pub at: String,
    pub kind: IssueKind,
    /// The values the sentence of the kind needs, in order: what was found, what is allowed, the name
    /// that does not exist.
    pub detail: Vec<String>,
    pub outcome: Outcome,
}

impl ContentIssue {
    /// An issue that refuses the game.
    pub fn new(file: impl Into<String>, at: impl Into<String>, kind: IssueKind) -> Self {
        ContentIssue {
            file: file.into(),
            at: at.into(),
            kind,
            detail: Vec::new(),
            outcome: Outcome::Refuse,
        }
    }

    /// The same with its values.
    pub fn with(mut self, detail: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.detail = detail.into_iter().map(Into::into).collect();
        self
    }

    /// An issue from a problem sentence of a validator that does not give the parts yet: the file is
    /// the leading `name.ron: ` when there is one, the sentence is the detail, and the kind is
    /// `Conflict`. (Transitional: each validator moves to the typed constructors, task 764.)
    pub fn from_problem(problem: &str) -> Self {
        let (file, rest) = match problem.split_once(": ") {
            Some((f, r)) if f.ends_with(".ron") && !f.contains(' ') => (f, r),
            _ => ("", problem),
        };
        ContentIssue::new(file, "", IssueKind::Conflict).with([rest])
    }
}

impl fmt::Display for ContentIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.file.is_empty() {
            write!(f, "{}: ", self.file)?;
        }
        if !self.at.is_empty() {
            write!(f, "{}: ", self.at)?;
        }
        match (self.kind, self.detail.as_slice()) {
            (IssueKind::Conflict, [sentence]) => write!(f, "{sentence}"),
            (kind, []) => write!(f, "{}", kind.label()),
            (kind, detail) => write!(f, "{} ({})", kind.label(), detail.join("; ")),
        }
    }
}

/// A load that did not give content: every issue found, not only the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentError {
    pub issues: Vec<ContentIssue>,
}

impl ContentError {
    pub fn new(issues: Vec<ContentIssue>) -> Self {
        ContentError { issues }
    }

    /// The error of a validator that still gives sentences.
    pub fn invalid(problems: Vec<String>) -> Self {
        ContentError::new(
            problems
                .iter()
                .map(|p| ContentIssue::from_problem(p))
                .collect(),
        )
    }

    /// A file that could not be read; `path` is as the caller names it.
    pub fn io(path: &std::path::Path, e: &std::io::Error) -> Self {
        let kind = if e.kind() == std::io::ErrorKind::NotFound {
            IssueKind::Missing
        } else {
            IssueKind::Unreadable
        };
        ContentError::new(vec![ContentIssue::new(
            path.display().to_string(),
            "",
            kind,
        )
        .with([e.to_string()])])
    }

    /// A file whose text does not read as its type. The place is the line and column the parser gives.
    pub fn parse(file: &str, e: &ron::error::SpannedError) -> Self {
        use ron::error::Error as E;
        let message = e.code.to_string();
        let kind = match &e.code {
            E::NoSuchStructField { .. } | E::NoSuchEnumVariant { .. } => IssueKind::UnknownKey,
            E::MissingStructField { .. } => IssueKind::Missing,
            E::DuplicateStructField { .. } => IssueKind::Duplicate,
            E::ExpectedInteger
            | E::ExpectedFloat
            | E::ExpectedBoolean
            | E::ExpectedString
            | E::ExpectedChar
            | E::ExpectedArray
            | E::ExpectedMap
            | E::ExpectedOption
            | E::ExpectedUnit
            | E::ExpectedStructLike
            | E::ExpectedNamedStructLike(_)
            | E::ExpectedDifferentStructName { .. }
            | E::ExpectedDifferentLength { .. }
            | E::InvalidValueForType { .. } => IssueKind::WrongType,
            E::Message(m) if m.starts_with("unknown field") || m.starts_with("unknown variant") => {
                IssueKind::UnknownKey
            }
            E::Message(m) if m.starts_with("missing field") => IssueKind::Missing,
            E::Message(m) if m.starts_with("duplicate field") => IssueKind::Duplicate,
            E::Message(m) if m.starts_with("invalid type") => IssueKind::WrongType,
            E::Message(m) if m.starts_with("invalid value") => IssueKind::OutOfRange,
            E::IntegerOutOfBounds => IssueKind::OutOfRange,
            _ => IssueKind::Corrupt,
        };
        let at = format!("line {}, column {}", e.position.line, e.position.col);
        ContentError::new(vec![ContentIssue::new(file, at, kind).with([message])])
    }

    /// Whether some issue is of this kind.
    pub fn has(&self, kind: IssueKind) -> bool {
        self.issues.iter().any(|i| i.kind == kind)
    }

    /// Whether some issue is about this file (a path that ends in it counts).
    pub fn mentions(&self, file: &str) -> bool {
        self.issues
            .iter()
            .any(|i| i.file.ends_with(file) || i.at.contains(file))
            || self.to_string().contains(file)
    }
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.issues.as_slice() {
            [one] => write!(f, "content: {one}"),
            many => {
                write!(f, "content has {} problems:", many.len())?;
                many.iter().try_for_each(|i| write!(f, "\n  - {i}"))
            }
        }
    }
}

impl std::error::Error for ContentError {}
