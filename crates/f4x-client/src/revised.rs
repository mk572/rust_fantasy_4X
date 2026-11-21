//! A value whose every mutable access moves a revision counter (task 692).
//!
//! The client holds its [`Game`](f4x_core::game::Game) in a [`Revised`]. A cache of something derived
//! from the game (the budget, the human's vision, the path under the pointer: tasks 693–695) keeps the
//! revision it was made at and is stale when [`revision`] differs. The counter moves in `DerefMut`, so a
//! call site that changes the game, now or later, cannot forget it; a read through `Deref` does not
//! move it. A new wrapper (a load, a new game) moves it too. One counter for the whole process, so a
//! replaced game never repeats a revision of the game it replaces.
//!
//! The revision may move without a change (a `&mut` taken by a call that refuses, say): a cache then
//! rebuilds once more than needed, never too little.

use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};

static REVISION: AtomicU64 = AtomicU64::new(0);

fn bump() -> u64 {
    REVISION.fetch_add(1, Ordering::Relaxed) + 1
}

/// The current revision of the held value.
pub struct Revised<T> {
    value: T,
    revision: u64,
}

impl<T> Revised<T> {
    /// Wraps `value` at a revision no earlier value had.
    pub fn new(value: T) -> Self {
        Self {
            value,
            revision: bump(),
        }
    }

    // Read by the caches of tasks 693-695; until they land only the tests ask.
    #[cfg_attr(not(test), allow(dead_code))]
    /// The revision: differs from an earlier one if the value was borrowed mutably or replaced since.
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

impl<T> Deref for Revised<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for Revised<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.revision = bump();
        &mut self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_keeps_the_revision_and_a_mutable_borrow_moves_it() {
        let mut v = Revised::new(vec![1, 2]);
        let r = v.revision();
        assert_eq!(v.len(), 2);
        assert_eq!(v.revision(), r, "a read does not move it");
        v.push(3);
        assert_ne!(v.revision(), r, "a change moves it");
    }

    #[test]
    fn a_replaced_value_never_repeats_a_revision() {
        let mut seen = vec![];
        let mut v = Revised::new(0);
        for n in 1..4 {
            seen.push(v.revision());
            v = Revised::new(n);
        }
        seen.push(v.revision());
        let mut sorted = seen.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), seen.len());
    }
}
