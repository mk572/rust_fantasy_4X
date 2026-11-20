//! A window of rows over a longer list, for the spell screens that choose among the other wizards
//! (`wizard_pick.rs`, `global_pick.rs`).

/// `shown` rows of `total`, the first of them `first`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Window {
    pub(crate) first: usize,
    pub(crate) shown: usize,
    pub(crate) total: usize,
}

impl Window {
    pub(crate) fn new(first: usize, shown: usize, total: usize) -> Self {
        let mut window = Self {
            first,
            shown,
            total,
        };
        window.first = first.min(window.last_first());
        window
    }

    /// The first row of the last window.
    fn last_first(&self) -> usize {
        self.total.saturating_sub(self.shown)
    }

    pub(crate) fn can_up(&self) -> bool {
        self.first > 0
    }

    pub(crate) fn can_down(&self) -> bool {
        self.first < self.last_first()
    }

    /// Move by `rows` (negative: up), as far as the list goes.
    pub(crate) fn moved(self, rows: i32) -> Self {
        let first = (self.first as i64 + i64::from(rows)).clamp(0, self.last_first() as i64);
        Self {
            first: first as usize,
            ..self
        }
    }

    /// The rows in view, as indices into the list.
    pub(crate) fn rows(&self) -> std::ops::Range<usize> {
        self.first..(self.first + self.shown).min(self.total)
    }

    /// Whether there is more than fits (the arrows are drawn at all).
    pub(crate) fn scrolls(&self) -> bool {
        self.total > self.shown
    }
}

/// The move a wheel turn or a key asks for: the wheel's `y` is up when positive.
pub(crate) fn step(wheel_y: f32, up_key: bool, down_key: bool) -> i32 {
    let by_wheel = if wheel_y > 0.0 {
        -1
    } else if wheel_y < 0.0 {
        1
    } else {
        0
    };
    by_wheel - i32::from(up_key) + i32::from(down_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_that_fits_does_not_scroll() {
        let w = Window::new(3, 4, 3);
        assert_eq!(
            (w.first, w.scrolls(), w.can_up(), w.can_down()),
            (0, false, false, false)
        );
        assert_eq!(w.moved(1), w);
        assert_eq!(w.rows(), 0..3);
    }

    #[test]
    fn a_longer_list_moves_a_row_at_a_time_and_stops_at_both_ends() {
        let mut w = Window::new(0, 2, 5);
        assert!(w.scrolls() && !w.can_up() && w.can_down());
        w = w.moved(1);
        assert_eq!((w.first, w.rows()), (1, 1..3));
        w = w.moved(10);
        assert_eq!((w.first, w.can_down(), w.can_up()), (3, false, true));
        assert_eq!(w.rows(), 3..5);
        assert_eq!(w.moved(-10).first, 0);
    }

    #[test]
    fn a_first_row_past_the_end_is_pulled_back_when_the_list_shrinks() {
        assert_eq!(Window::new(9, 2, 4).first, 2);
    }

    #[test]
    fn the_wheel_and_the_keys_agree_on_the_direction() {
        assert_eq!(step(1.0, false, false), -1);
        assert_eq!(step(-1.0, false, false), 1);
        assert_eq!(step(0.0, true, false), -1);
        assert_eq!(step(0.0, false, true), 1);
        assert_eq!(step(0.0, true, true), 0);
    }
}
