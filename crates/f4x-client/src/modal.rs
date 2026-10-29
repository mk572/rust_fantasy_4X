//! The modal stack: questions that wait over the screen below until they are answered.
//!
//! A modal is drawn after the screen below it and, while one is open, gets all the input of the
//! frame (`App::frame` returns after the modal's frame, so nothing below sees a click or a key).
//! The newest modal is on top. Each kind holds its own state; its frame returns what is left of
//! it (`None` once it is answered). Opening a kind that is already open replaces it.

use f4x_core::rules::SpellId;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Modal {
    /// The fortress has fallen: resign, or cast the Spell of Return.
    Resign,
    /// Which realm a Spell Ward guards a city against.
    Ward {
        spell: SpellId,
        city: hecs::Entity,
        resolve: bool,
    },
    /// The cast has nothing to aim at: keep the spell or give it up.
    NoTarget(SpellId),
    /// The dead hero to raise, the row picked at `row`.
    Resurrect {
        spell: SpellId,
        resolve: bool,
        row: usize,
    },
    /// The rival's global enchantment to dispel, the row picked at `row`.
    Global {
        spell: SpellId,
        resolve: bool,
        row: usize,
    },
    /// The units of a square the player cannot command, listed over the screen.
    UnitList(Vec<hecs::Entity>),
    /// Turn 1: the box that lets the player rename the capital, with the name typed so far.
    CapitalName { city: hecs::Entity, name: String },
}

/// The open modals, the newest last.
#[derive(Default)]
pub(crate) struct ModalStack {
    open: Vec<Modal>,
}

impl ModalStack {
    /// Open a modal on top; one of the same kind that is already open is replaced.
    pub(crate) fn open(&mut self, modal: Modal) {
        self.open
            .retain(|m| std::mem::discriminant(m) != std::mem::discriminant(&modal));
        self.open.push(modal);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    /// Take the top modal to run its frame; give it back with `restore`.
    pub(crate) fn take_top(&mut self) -> Option<(usize, Modal)> {
        let modal = self.open.pop()?;
        Some((self.open.len(), modal))
    }

    /// Put back what is left of a modal taken from `at`; modals opened meanwhile stay above it.
    pub(crate) fn restore(&mut self, at: usize, left: Option<Modal>) {
        if let Some(modal) = left {
            self.open.insert(at.min(self.open.len()), modal);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real spell of the shipped rules.
    fn some_spell() -> SpellId {
        let content = f4x_data::load_content(std::path::Path::new("../../content")).unwrap();
        let (id, _) = content.rules.spells.iter().next().unwrap();
        id
    }

    #[test]
    fn a_kind_opened_twice_is_kept_once_with_the_newer_state() {
        let mut stack = ModalStack::default();
        stack.open(Modal::Resign);
        stack.open(Modal::Resign);
        assert_eq!(stack.take_top(), Some((0, Modal::Resign)));
        assert!(stack.is_empty());
    }

    #[test]
    fn the_newest_modal_is_on_top_and_the_one_below_waits() {
        let mut stack = ModalStack::default();
        let spell = some_spell();
        stack.open(Modal::NoTarget(spell));
        stack.open(Modal::Resign);
        let (at, top) = stack.take_top().unwrap();
        assert_eq!(top, Modal::Resign);
        // Still unanswered: it goes back where it was, below nothing.
        stack.restore(at, Some(top));
        assert_eq!(stack.take_top().unwrap().1, Modal::Resign);
        assert_eq!(stack.take_top().unwrap().1, Modal::NoTarget(spell));
        assert!(stack.take_top().is_none());
    }

    #[test]
    fn a_modal_opened_while_one_runs_ends_up_above_it() {
        let mut stack = ModalStack::default();
        let spell = some_spell();
        stack.open(Modal::Resign);
        let (at, running) = stack.take_top().unwrap();
        stack.open(Modal::NoTarget(spell));
        stack.restore(at, Some(running));
        assert_eq!(stack.take_top().unwrap().1, Modal::NoTarget(spell));
        assert_eq!(stack.take_top().unwrap().1, Modal::Resign);
    }
}
