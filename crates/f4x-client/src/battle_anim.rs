//! What a battle looks like while it plays: where each figure stands in its square, which way
//! it faces, and the walk, blow, shot and death animations that play after an order.
//!
//! The core finishes an order at once. The screen keeps a copy of the battle from before it and
//! plays the order's events on that copy, one after another, so a unit walks its path before
//! the next thing happens; when the last one ends the real battle is shown. Everything here is
//! pure (times in, positions out) so it can be tested.

use f4x_core::battle::map::Pos;

/// Seconds to cross one square.
pub const STEP_TIME: f32 = 0.2;
const ATTACK_TIME: f32 = 0.6;
const SHOT_TIME: f32 = 0.55;
const DEATH_TIME: f32 = 0.5;
/// Walking figures cycle through these animation frames.
const WALK_FRAMES: [u8; 4] = [0, 1, 2, 1];
const WALK_RATE: f32 = 7.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anim {
    /// Walk along the squares after the unit's starting one.
    Move {
        unit: usize,
        path: Vec<Pos>,
    },
    Attack {
        attacker: usize,
        defender: usize,
    },
    Shot {
        attacker: usize,
        defender: usize,
    },
    Death {
        unit: usize,
    },
}

impl Anim {
    pub fn duration(&self) -> f32 {
        match self {
            Anim::Move { path, .. } => path.len() as f32 * STEP_TIME,
            Anim::Attack { .. } => ATTACK_TIME,
            Anim::Shot { .. } => SHOT_TIME,
            Anim::Death { .. } => DEATH_TIME,
        }
    }
}

/// How one unit is drawn this instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Visual {
    /// Position in squares; fractional while walking or lunging.
    pub at: (f32, f32),
    pub facing: u8,
    pub frame: u8,
    /// 1 = solid, fading to 0 as the unit dies.
    pub alpha: f32,
    /// How red to flash it (a blow just landed), 0 to 1.
    pub hurt: f32,
}

impl Visual {
    pub fn still(cell: Pos, facing: u8) -> Self {
        Self {
            at: (cell.0 as f32, cell.1 as f32),
            facing,
            frame: 1,
            alpha: 1.0,
            hurt: 0.0,
        }
    }
}

/// Which of the eight pictures of a figure shows it looking from `from` towards `to` (0 north,
/// clockwise; the original's `BATTLE_UNIT_FACING_DRECTION`, where grid x grows to the lower
/// right and y to the lower left). Facing nowhere gives 2, as the original does.
pub fn facing(from: Pos, to: Pos) -> u8 {
    use std::cmp::Ordering::*;
    match (from.0.cmp(&to.0), from.1.cmp(&to.1)) {
        (Greater, Greater) => 0,
        (Equal, Greater) => 1,
        (Less, Greater) => 2,
        (Less, Equal) => 3,
        (Less, Less) => 4,
        (Equal, Less) => 5,
        (Greater, Less) => 6,
        (Greater, Equal) => 7,
        (Equal, Equal) => 2,
    }
}

/// Where a figure of a unit that started with `count` figures stands, in pixels from the
/// middle of its square's cell (`Battle_Unit_Figure_Position`; the picture's top left is this
/// point minus (14, 23)).
pub fn figure_offset(count: usize, index: usize) -> (i32, i32) {
    const TABLE: [&[(i32, i32)]; 8] = [
        &[(1, 8)],
        &[(-7, 9), (7, 9)],
        &[(0, 4), (-6, 10), (7, 10)],
        &[(1, 4), (8, 8), (1, 12), (-7, 8)],
        &[(1, 4), (8, 8), (1, 12), (-7, 8), (1, 8)],
        &[(1, 4), (9, 8), (-8, 8), (1, 12), (-3, 9), (4, 7)],
        &[(1, 4), (10, 8), (-8, 8), (1, 12), (6, 6), (-3, 11), (1, 8)],
        &[
            (1, 4),
            (10, 8),
            (-8, 8),
            (2, 12),
            (6, 6),
            (-3, 12),
            (-2, 7),
            (3, 9),
        ],
    ];
    let row = TABLE[count.clamp(1, 8) - 1];
    row[index.min(row.len() - 1)]
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn walk_frame(time: f32) -> u8 {
    WALK_FRAMES[(time * WALK_RATE) as usize % WALK_FRAMES.len()]
}

/// How `unit`, standing on `cell` before the animation, looks `t` seconds into `anim`.
/// `None` if the animation does not touch it.
pub fn visual_in(anim: &Anim, t: f32, unit: usize, cell: Pos, base_facing: u8) -> Option<Visual> {
    let mut v = Visual::still(cell, base_facing);
    match anim {
        Anim::Move { unit: u, path } if *u == unit => {
            let steps: Vec<Pos> = std::iter::once(cell).chain(path.iter().copied()).collect();
            let i = ((t / STEP_TIME) as usize).min(steps.len().saturating_sub(2));
            let (a, b) = (steps[i], steps[(i + 1).min(steps.len() - 1)]);
            let f = ((t - i as f32 * STEP_TIME) / STEP_TIME).clamp(0.0, 1.0);
            v.at = (
                lerp(a.0 as f32, b.0 as f32, f),
                lerp(a.1 as f32, b.1 as f32, f),
            );
            v.facing = facing(a, b);
            v.frame = walk_frame(t);
            Some(v)
        }
        Anim::Death { unit: u } if *u == unit => {
            v.alpha = (1.0 - t / DEATH_TIME).clamp(0.0, 1.0);
            v.frame = 3;
            Some(v)
        }
        _ => None,
    }
}

/// The look of an attacker or a defender in a blow or shot between two squares.
pub fn visual_between(
    anim: &Anim,
    t: f32,
    unit: usize,
    cell: Pos,
    other: Pos,
    base_facing: u8,
) -> Option<Visual> {
    let (attacker, defender, melee, total) = match anim {
        Anim::Attack { attacker, defender } => (*attacker, *defender, true, ATTACK_TIME),
        Anim::Shot { attacker, defender } => (*attacker, *defender, false, SHOT_TIME),
        _ => return None,
    };
    let mut v = Visual::still(cell, base_facing);
    let phase = (t / total).clamp(0.0, 1.0);
    if unit == attacker {
        v.facing = facing(cell, other);
        if melee {
            // Lunge towards the target and back.
            let reach = (phase * std::f32::consts::PI).sin() * 0.35;
            v.at = (
                cell.0 as f32 + (other.0 - cell.0) as f32 * reach,
                cell.1 as f32 + (other.1 - cell.1) as f32 * reach,
            );
            v.frame = if phase < 0.5 { 1 } else { 3 };
        } else {
            v.frame = 1;
        }
        Some(v)
    } else if unit == defender {
        v.facing = facing(cell, other);
        // The blow lands at about two fifths of the way through.
        let hit_at = 0.4;
        if phase > hit_at {
            let left = 1.0 - (phase - hit_at) / (1.0 - hit_at);
            v.hurt = left.clamp(0.0, 1.0);
            v.frame = 3;
        }
        Some(v)
    } else {
        None
    }
}

/// Where a shot's missile is `t` seconds in, as a fraction of the way from the shooter to the
/// target (it flies for the first two thirds of the animation), or `None` while it is not in
/// the air.
pub fn missile_progress(t: f32) -> Option<f32> {
    let flight = SHOT_TIME * 0.66;
    (t < flight).then(|| (t / flight).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facing_follows_the_original_table() {
        assert_eq!(facing((5, 5), (4, 4)), 0);
        assert_eq!(facing((5, 5), (5, 4)), 1);
        assert_eq!(facing((5, 5), (6, 4)), 2);
        assert_eq!(facing((5, 5), (6, 5)), 3);
        assert_eq!(facing((5, 5), (6, 6)), 4);
        assert_eq!(facing((5, 5), (5, 6)), 5);
        assert_eq!(facing((5, 5), (4, 6)), 6);
        assert_eq!(facing((5, 5), (4, 5)), 7);
        assert_eq!(facing((5, 5), (5, 5)), 2);
    }

    #[test]
    fn figures_stand_where_the_original_puts_them() {
        assert_eq!(figure_offset(1, 0), (1, 8));
        assert_eq!(figure_offset(2, 1), (7, 9));
        assert_eq!(figure_offset(8, 7), (3, 9));
        assert_eq!(figure_offset(6, 5), (4, 7));
        // More than eight is shown as eight; an index past the end repeats the last spot.
        assert_eq!(figure_offset(12, 0), figure_offset(8, 0));
        for n in 1..=8 {
            for i in 0..n {
                let _ = figure_offset(n, i);
            }
        }
    }

    #[test]
    fn a_walker_is_between_two_squares_half_way_through_a_step() {
        let anim = Anim::Move {
            unit: 3,
            path: vec![(5, 5), (6, 5)],
        };
        let v = visual_in(&anim, STEP_TIME * 0.5, 3, (4, 5), 2).unwrap();
        assert!((v.at.0 - 4.5).abs() < 1e-4 && (v.at.1 - 5.0).abs() < 1e-4);
        assert_eq!(v.facing, 3);
        let v = visual_in(&anim, STEP_TIME * 1.5, 3, (4, 5), 2).unwrap();
        assert!((v.at.0 - 5.5).abs() < 1e-4);
        // Other units are not touched.
        assert!(visual_in(&anim, 0.1, 4, (4, 5), 2).is_none());
        assert_eq!(anim.duration(), 2.0 * STEP_TIME);
    }

    #[test]
    fn the_end_of_a_walk_is_the_last_square() {
        let anim = Anim::Move {
            unit: 0,
            path: vec![(1, 0), (2, 0)],
        };
        let v = visual_in(&anim, anim.duration(), 0, (0, 0), 2).unwrap();
        assert!((v.at.0 - 2.0).abs() < 1e-4);
    }

    #[test]
    fn a_blow_lunges_and_returns_and_hurts_the_defender_late() {
        let anim = Anim::Attack {
            attacker: 0,
            defender: 1,
        };
        let a = |t| visual_between(&anim, t, 0, (5, 5), (6, 5), 2).unwrap();
        assert_eq!(a(0.0).at, (5.0, 5.0));
        assert!(a(ATTACK_TIME * 0.5).at.0 > 5.2);
        assert!((a(ATTACK_TIME).at.0 - 5.0).abs() < 1e-3);
        let d = |t| visual_between(&anim, t, 1, (6, 5), (5, 5), 7).unwrap();
        assert_eq!(d(0.0).hurt, 0.0);
        assert!(d(ATTACK_TIME * 0.5).hurt > 0.5);
        assert_eq!(d(0.0).facing, 7);
    }

    #[test]
    fn the_dying_fade_out() {
        let anim = Anim::Death { unit: 2 };
        let start = visual_in(&anim, 0.0, 2, (3, 3), 2).unwrap();
        let end = visual_in(&anim, DEATH_TIME, 2, (3, 3), 2).unwrap();
        assert_eq!(start.alpha, 1.0);
        assert_eq!(end.alpha, 0.0);
    }

    #[test]
    fn a_missile_flies_then_lands() {
        assert_eq!(missile_progress(0.0), Some(0.0));
        assert!(missile_progress(SHOT_TIME * 0.3).unwrap() > 0.4);
        assert_eq!(missile_progress(SHOT_TIME), None);
    }
}
