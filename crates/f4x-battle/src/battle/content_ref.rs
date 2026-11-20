//! The handle a battle keeps to the game content, shared with the game that started it.

use f4x_def::content::Content;
use std::sync::Arc;

/// The content of the game, read by the battle where it needs a definition. It is not saved with
/// the battle (the game that loads the battle attaches its own), and it does not count in
/// the battle's equality: two battles are equal when their state is.
#[derive(Clone, Default)]
pub struct ContentRef(Arc<Content>);

impl ContentRef {
    pub fn new(content: Arc<Content>) -> ContentRef {
        ContentRef(content)
    }
}

impl std::ops::Deref for ContentRef {
    type Target = Content;

    fn deref(&self) -> &Content {
        &self.0
    }
}

impl std::fmt::Debug for ContentRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContentRef")
    }
}

impl PartialEq for ContentRef {
    fn eq(&self, _other: &ContentRef) -> bool {
        true
    }
}

impl Eq for ContentRef {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_reads_the_content_it_was_given_and_clones_share_it() {
        let content = Arc::new(f4x_def::testdir::shipped().clone());
        let a = ContentRef::new(content.clone());
        let b = a.clone();
        assert_eq!(a.rules.units.defs().len(), content.rules.units.defs().len());
        assert!(std::ptr::eq(&*a, &*b));
    }

    #[test]
    fn battles_with_different_content_are_still_compared_by_state() {
        let shipped = ContentRef::new(Arc::new(f4x_def::testdir::shipped().clone()));
        assert_eq!(shipped, ContentRef::default());
    }
}
