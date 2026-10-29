//! The modal stack: questions and notices that wait over the screen below until they are answered.
//!
//! A modal is drawn after the screen below it (dimmed, if the player wants it) and, while one is
//! open, gets all the input of the frame (`App::frame` returns after the modal's frame, so
//! nothing below sees a click or a key). Each kind is a struct with its own state that implements
//! [`Modal`]; its frame takes the state and returns what is left of it (`None` once it is
//! answered).
//!
//! Two kinds of modal come in two ways:
//! - A question the player asked for (a spell's target, a confirmation) is `open`ed on top; one of
//!   the same kind that is already open is replaced.
//! - A notice that turn processing produces (an event, a site's report) is `enqueue`d: it waits
//!   until nothing is open, and the waiting ones run first in, first out (lower `priority` first
//!   among those that wait).

use std::collections::VecDeque;

use crate::audio::Track;
use crate::input::Input;
use crate::view::App;

/// The order the waiting notices run in when several kinds wait together. Lower runs first.
pub(crate) mod priority {
    pub(crate) const MASTERY: u8 = 20;
}

/// The names of the kinds that other code asks about (`ModalStack::has`).
pub(crate) mod kind {
    pub(crate) const MASTERY: &str = "mastery";
    pub(crate) const UPKEEP: &str = "upkeep";
    pub(crate) const PROPOSAL: &str = "proposal";
    pub(crate) const SITE: &str = "site";
    pub(crate) const LEARNED: &str = "learned";
    pub(crate) const ATTACK: &str = "attack";
    pub(crate) const LEVEL_UP: &str = "level_up";
    pub(crate) const EVENT: &str = "event";
    pub(crate) const GLOBAL_CAST: &str = "global_cast";
    pub(crate) const SUMMON_SCENE: &str = "summon_scene";
    pub(crate) const CITY_SCROLL: &str = "city_scroll";
    pub(crate) const SQUARE_ANIM: &str = "square_anim";
    pub(crate) const WIZARD_ANIM: &str = "wizard_anim";
    pub(crate) const SPELL_NOTICE: &str = "spell_notice";
    pub(crate) const OFFER: &str = "offer";
    pub(crate) const BUILT: &str = "built";
    pub(crate) const SELECTION: &str = "selection";
    pub(crate) const VIZIER: &str = "vizier";
    pub(crate) const HELP: &str = "help";
    pub(crate) const COMBAT_RESULT: &str = "combat_result";
    pub(crate) const CHANCELLOR: &str = "chancellor";
}

pub(crate) trait Modal {
    /// What kind of box this is: opening one replaces an open one of the same kind.
    fn kind(&self) -> &'static str;

    /// Among the waiting notices the lower number runs first; equal ones keep their order.
    fn priority(&self) -> u8 {
        0
    }

    /// Whether the turn change must wait until this is answered.
    fn blocks_turn(&self) -> bool {
        true
    }

    /// Whether this fills the whole screen itself, so the screen below is not drawn and not dimmed.
    fn covers_screen(&self) -> bool {
        false
    }

    /// The piece that plays while this is shown, if it has one.
    fn music(&self) -> Option<Track> {
        None
    }

    /// One frame: draw the box and take the input; what is left of the modal, `None` once it is
    /// answered.
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>>;
}

/// Box what is left of a modal's state, for `Modal::frame`.
pub(crate) fn keep<M: Modal + 'static>(left: Option<M>) -> Option<Box<dyn Modal>> {
    left.map(|m| Box::new(m) as Box<dyn Modal>)
}

/// The open modals, the newest last, and the notices waiting for their turn.
#[derive(Default)]
pub(crate) struct ModalStack {
    open: Vec<Box<dyn Modal>>,
    waiting: VecDeque<Box<dyn Modal>>,
}

impl ModalStack {
    /// Open a modal on top; one of the same kind that is already open is replaced.
    pub(crate) fn open(&mut self, modal: impl Modal + 'static) {
        let kind = modal.kind();
        self.open.retain(|m| m.kind() != kind);
        self.open.push(Box::new(modal));
    }

    /// Let a notice wait for its turn, behind the ones that wait already.
    pub(crate) fn enqueue(&mut self, modal: impl Modal + 'static) {
        let at = self
            .waiting
            .iter()
            .position(|m| m.priority() > modal.priority())
            .unwrap_or(self.waiting.len());
        self.waiting.insert(at, Box::new(modal));
    }

    /// Whether a modal of this kind is open or waiting.
    pub(crate) fn has(&self, kind: &str) -> bool {
        self.open
            .iter()
            .chain(&self.waiting)
            .any(|m| m.kind() == kind)
    }

    /// Drop every modal of this kind, open or waiting.
    pub(crate) fn remove(&mut self, kind: &str) {
        self.open.retain(|m| m.kind() != kind);
        self.waiting.retain(|m| m.kind() != kind);
    }

    /// The piece of the modal that is on top, or else of the one that waits first.
    pub(crate) fn music(&self) -> Option<Track> {
        self.open
            .iter()
            .rev()
            .chain(&self.waiting)
            .find_map(|m| m.music())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.open.is_empty() && self.waiting.is_empty()
    }

    /// Whether the turn change has to wait for an answer.
    pub(crate) fn blocks_turn(&self) -> bool {
        self.open
            .iter()
            .chain(&self.waiting)
            .any(|m| m.blocks_turn())
    }

    /// Take the top modal to run its frame; give what is left of it back with `restore`. With
    /// nothing open, the first waiting notice becomes the open one.
    pub(crate) fn take_top(&mut self) -> Option<(usize, Box<dyn Modal>)> {
        if self.open.is_empty() {
            let next = self.waiting.pop_front()?;
            self.open.push(next);
        }
        let modal = self.open.pop()?;
        Some((self.open.len(), modal))
    }

    /// Put back what is left of a modal taken from `at`; modals opened meanwhile stay above it.
    pub(crate) fn restore(&mut self, at: usize, left: Option<Box<dyn Modal>>) {
        if let Some(modal) = left {
            self.open.insert(at.min(self.open.len()), modal);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A modal that only has a name, to look at the order they come in.
    struct Named(&'static str, u8);

    impl Modal for Named {
        fn kind(&self) -> &'static str {
            self.0
        }
        fn priority(&self) -> u8 {
            self.1
        }
        fn frame(self: Box<Self>, _: &mut App, _: &Input) -> Option<Box<dyn Modal>> {
            None
        }
    }

    fn top_kind(stack: &mut ModalStack) -> Option<&'static str> {
        stack.take_top().map(|(_, m)| m.kind())
    }

    #[test]
    fn a_kind_opened_twice_is_kept_once() {
        let mut stack = ModalStack::default();
        stack.open(Named("resign", 0));
        stack.open(Named("resign", 0));
        assert_eq!(top_kind(&mut stack), Some("resign"));
        assert!(stack.is_empty());
    }

    #[test]
    fn the_newest_opened_modal_is_on_top_and_the_one_below_waits() {
        let mut stack = ModalStack::default();
        stack.open(Named("no_target", 0));
        stack.open(Named("resign", 0));
        let (at, top) = stack.take_top().unwrap();
        assert_eq!(top.kind(), "resign");
        // Still unanswered: it goes back where it was, below nothing.
        stack.restore(at, Some(top));
        assert_eq!(top_kind(&mut stack), Some("resign"));
        assert_eq!(top_kind(&mut stack), Some("no_target"));
        assert_eq!(top_kind(&mut stack), None);
    }

    #[test]
    fn a_modal_opened_while_one_runs_ends_up_above_it() {
        let mut stack = ModalStack::default();
        stack.open(Named("resign", 0));
        let (at, running) = stack.take_top().unwrap();
        stack.open(Named("no_target", 0));
        stack.restore(at, Some(running));
        assert_eq!(top_kind(&mut stack), Some("no_target"));
        assert_eq!(top_kind(&mut stack), Some("resign"));
    }

    #[test]
    fn waiting_notices_run_first_in_first_out_and_a_lower_priority_number_goes_first() {
        let mut stack = ModalStack::default();
        stack.enqueue(Named("event", 5));
        stack.enqueue(Named("site", 3));
        stack.enqueue(Named("event", 5));
        stack.enqueue(Named("level_up", 5));
        let order: Vec<_> = std::iter::from_fn(|| top_kind(&mut stack)).collect();
        assert_eq!(order, ["site", "event", "event", "level_up"]);
    }

    #[test]
    fn an_opened_question_runs_before_the_waiting_notices() {
        let mut stack = ModalStack::default();
        stack.enqueue(Named("event", 5));
        stack.open(Named("no_target", 0));
        assert_eq!(top_kind(&mut stack), Some("no_target"));
        assert_eq!(top_kind(&mut stack), Some("event"));
    }

    #[test]
    fn a_notice_that_does_not_block_the_turn_leaves_it_free() {
        struct Quiet;
        impl Modal for Quiet {
            fn kind(&self) -> &'static str {
                "quiet"
            }
            fn blocks_turn(&self) -> bool {
                false
            }
            fn frame(self: Box<Self>, _: &mut App, _: &Input) -> Option<Box<dyn Modal>> {
                None
            }
        }
        let mut stack = ModalStack::default();
        stack.enqueue(Quiet);
        assert!(!stack.is_empty() && !stack.blocks_turn());
        stack.open(Named("resign", 0));
        assert!(stack.blocks_turn());
    }
}
