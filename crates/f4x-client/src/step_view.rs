//! Stepping the view through the player's cities and armies (patch 1.60: Find Next/Previous Army,
//! Next/Previous City). The squares are taken in reading order, plane by plane, row by row; a
//! step goes on from the square the last step stopped at and wraps round at the end of the list.
//!
//! Tests: below.

use f4x_core::map::Position;

fn order(p: Position) -> (u8, u32, u32) {
    (p.plane as u8, p.y, p.x)
}

/// The square after `from` in the list (before it when `forward` is false), wrapping round.
/// With no `from`, or one that is not on the list any more, the first (or last) square of the
/// part of the list that lies after (or before) it: a square that has gone does not restart the
/// walk. `None` for an empty list.
pub(crate) fn step(
    squares: &[Position],
    from: Option<Position>,
    forward: bool,
) -> Option<Position> {
    let mut list: Vec<Position> = squares.to_vec();
    list.sort_by_key(|&p| order(p));
    list.dedup();
    let pick = match (from, forward) {
        (None, true) => list.first(),
        (None, false) => list.last(),
        (Some(f), true) => list
            .iter()
            .find(|&&p| order(p) > order(f))
            .or_else(|| list.first()),
        (Some(f), false) => list
            .iter()
            .rev()
            .find(|&&p| order(p) < order(f))
            .or_else(|| list.last()),
    };
    pick.copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::map::Plane;

    fn at(plane: Plane, x: u32, y: u32) -> Position {
        Position { plane, x, y }
    }

    #[test]
    fn a_step_goes_on_in_reading_order_and_wraps_round() {
        let a = at(Plane::Arcanus, 5, 2);
        let b = at(Plane::Arcanus, 1, 3);
        let c = at(Plane::Myrror, 0, 0);
        let list = [c, a, b, a];
        assert_eq!(step(&list, None, true), Some(a));
        assert_eq!(step(&list, Some(a), true), Some(b));
        assert_eq!(step(&list, Some(b), true), Some(c));
        assert_eq!(step(&list, Some(c), true), Some(a), "wraps to the first");
        assert_eq!(step(&list, None, false), Some(c));
        assert_eq!(step(&list, Some(c), false), Some(b));
        assert_eq!(step(&list, Some(a), false), Some(c), "wraps to the last");
    }

    #[test]
    fn a_square_that_has_gone_does_not_restart_the_walk() {
        let a = at(Plane::Arcanus, 1, 1);
        let c = at(Plane::Arcanus, 9, 9);
        let gone = at(Plane::Arcanus, 4, 4);
        assert_eq!(step(&[a, c], Some(gone), true), Some(c));
        assert_eq!(step(&[a, c], Some(gone), false), Some(a));
    }

    #[test]
    fn one_square_is_its_own_next_and_an_empty_list_has_none() {
        let a = at(Plane::Arcanus, 1, 1);
        assert_eq!(step(&[a], Some(a), true), Some(a));
        assert_eq!(step(&[a], Some(a), false), Some(a));
        assert_eq!(step(&[], None, true), None);
    }
}
