//! Class 5 (`docs/f4x-newgame-test-design.md`): the small algorithms with a definition, tested for
//! what the definition promises, over their whole input where it is small and over many random
//! inputs where it is not. The private ones are tested next to their code.

use crate::support::*;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Grid, Map, Plane, Position};
use f4x_core::rules::RealmId;
use f4x_core::wizard::magic::Retort;
use f4x_newgame::island::{growth, land_mass, land_near, MIN_LAND, MIN_NEAR};
use f4x_newgame::neutral::{neutral_city_count, range};
use f4x_newgame::setup::parse_retort;
use f4x_newgame::temperament::{
    objective_weights, personality_weights, pick_objective, pick_personality,
};

// ---- the range between two squares ----

#[test]
fn the_range_between_squares_is_a_distance_on_a_map_that_wraps() {
    for width in [8u32, 45, 60, 90] {
        let at = |i: u32| (i * 7919) % width;
        for i in 0..40u32 {
            let (x1, y1, x2, y2) = (at(i), i % 11, at(i * 3 + 1), (i * 5) % 13);
            let r = range(width, x1, y1, x2, y2);
            assert_eq!(r, range(width, x2, y2, x1, y1), "symmetric");
            assert_eq!(range(width, x1, y1, x1, y1), 0, "zero on itself");
            // The same as the definition: the longer side plus half the shorter, the short way
            // round in the width.
            let d = x1.abs_diff(x2);
            let dx = d.min(width - d);
            let dy = y1.abs_diff(y2);
            assert_eq!(r, dx.max(dy) + dx.min(dy) / 2);
            // Never shorter than the larger side, never longer than one and a half of it.
            assert!(r >= dx.max(dy) && r <= dx.max(dy) * 3 / 2 + 1);
        }
    }
}

#[test]
fn a_map_wraps_east_and_west_in_the_range() {
    assert_eq!(range(60, 0, 5, 59, 5), 1);
    assert_eq!(range(60, 2, 5, 58, 5), 4);
}

#[test]
fn the_number_of_neutral_cities_grows_with_the_area_and_never_shrinks() {
    let mut last = 0;
    for width in [45u32, 60, 75, 90] {
        let mut row = 0;
        for height in [30u32, 40, 50, 60] {
            let n = neutral_city_count(width, height);
            assert!(n >= row, "{width}x{height}");
            row = n;
        }
        assert!(row >= last);
        last = row;
    }
    assert!(neutral_city_count(45, 30) >= 1);
}

// ---- retort names ----

#[test]
fn every_retort_is_found_by_its_name_whatever_the_case_and_punctuation() {
    let rules = &content().rules;
    for def in rules.retorts.defs() {
        let want = Some(rules.retorts.id_numbered(def.num).expect("the row"));
        let id = def.id.clone();
        for spelling in [
            id.clone(),
            id.to_uppercase(),
            id.replace('_', ""),
            id.replace('_', " "),
            id.replace('_', "-"),
            format!("  {id}  "),
            id.replace('_', ".").to_uppercase(),
        ] {
            assert_eq!(parse_retort(rules, &spelling), want, "{spelling:?}");
        }
    }
    for nothing in ["", "   ", "not a retort", "myrran2", "💥"] {
        assert_eq!(parse_retort(rules, nothing), None, "{nothing:?}");
    }
}

// ---- the temperament rolls ----

fn realms() -> Vec<RealmId> {
    f4x_def::rules::BOOK_REALMS.to_vec()
}

fn retorts() -> Vec<Retort> {
    f4x_newgame::coupling::RETORTS
        .iter()
        .map(|&(n, _)| Retort::new(n))
        .collect()
}

/// Every wizard shape of a small kind: a realm with the most books, and any subset of the retorts
/// the temperament reads (all of them, a few at a time).
type Shape = (Vec<(RealmId, i32)>, Vec<Retort>);

fn shapes() -> Vec<Shape> {
    let mut out = Vec::new();
    let all = retorts();
    for (i, _) in realms().iter().enumerate() {
        for second in [None, Some((i + 1) % 5)] {
            let mut books: Vec<(RealmId, i32)> = realms().into_iter().map(|r| (r, 0)).collect();
            books[i].1 = 5;
            if let Some(j) = second {
                books[j].1 = 5; // a tie: the earlier realm is the primary
            }
            for mask in 0u32..(1 << all.len()).min(1 << 11) {
                let held: Vec<Retort> = all
                    .iter()
                    .enumerate()
                    .filter(|(b, _)| mask >> b & 1 == 1)
                    .map(|(_, r)| *r)
                    .collect();
                out.push((books.clone(), held));
            }
        }
    }
    out
}

#[test]
fn the_weights_are_never_negative_and_always_leave_somewhere_for_a_roll_to_land() {
    for (books, held) in shapes() {
        let b = |r: RealmId| books.iter().find(|x| x.0 == r).map_or(0, |x| x.1);
        let h = |r: Retort| held.contains(&r);
        let (p, o) = (personality_weights(b, h), objective_weights(b, h));
        assert!(
            p.iter().all(|&w| w >= 0) && p.iter().sum::<i32>() > 0,
            "{p:?}"
        );
        assert!(
            o.iter().all(|&w| w >= 0) && o.iter().sum::<i32>() > 0,
            "{o:?}"
        );
    }
}

#[test]
fn a_roll_lands_on_each_kind_as_often_as_its_weight_and_never_on_a_weight_of_nothing() {
    for (books, held) in shapes().into_iter().step_by(37) {
        let b = |r: RealmId| books.iter().find(|x| x.0 == r).map_or(0, |x| x.1);
        let h = |r: Retort| held.contains(&r);
        let p = personality_weights(b, h);
        let mut hits = vec![0; p.len()];
        for roll in 1..=p.iter().sum::<i32>() {
            let kind = pick_personality(&p, roll);
            let at = kind as usize;
            hits[at] += 1;
        }
        assert_eq!(hits, p.to_vec(), "{p:?}");
        let o = objective_weights(b, h);
        let mut hits = vec![0; o.len()];
        for roll in 1..=o.iter().sum::<i32>() {
            let kind = pick_objective(&o, roll);
            let at = f4x_core::wizard::objective::Objective::all()
                .position(|k| k == kind)
                .expect("an objective");
            hits[at] += 1;
        }
        assert_eq!(hits, o.to_vec(), "{o:?}");
    }
}

#[test]
fn a_roll_outside_the_weights_still_gives_a_kind() {
    // A roll of nothing or over the total (a wizard whose weights summed to nothing is drawn 0):
    // never a panic.
    let p = [0, 1, 1, 1, 5, 2];
    for roll in [-100, -1, 0, 1, 10, 11, 12, 1000, i32::MAX] {
        let _ = pick_personality(&p, roll);
        let _ = pick_objective(&[1, 1, 1, 1, 1], roll);
    }
}

#[test]
fn a_life_wizard_is_never_maniacal_and_a_death_wizard_never_peaceful() {
    use f4x_core::wizard::objective::Personality;
    let life = RealmId::new(
        f4x_newgame::coupling::REALMS
            .iter()
            .find(|e| e.1 == "life")
            .expect("life")
            .0,
    );
    let death = RealmId::new(
        f4x_newgame::coupling::REALMS
            .iter()
            .find(|e| e.1 == "death")
            .expect("death")
            .0,
    );
    for (primary, never) in [
        (life, Personality::Maniacal),
        (death, Personality::Peaceful),
    ] {
        for (_, held) in shapes().into_iter().step_by(13) {
            let b = |r: RealmId| if r == primary { 9 } else { 0 };
            let h = |r: Retort| held.contains(&r);
            let w = personality_weights(b, h);
            for roll in 1..=w.iter().sum::<i32>() {
                assert_ne!(pick_personality(&w, roll), never);
            }
        }
    }
}

#[test]
fn a_retort_never_lowers_a_weight_it_does_not_zero() {
    for (books, held) in shapes().into_iter().step_by(11) {
        let b = |r: RealmId| books.iter().find(|x| x.0 == r).map_or(0, |x| x.1);
        let without = |r: Retort| held.contains(&r);
        let base = (
            personality_weights(b, without),
            objective_weights(b, without),
        );
        for extra in retorts() {
            let with = |r: Retort| r == extra || held.contains(&r);
            let (p, o) = (personality_weights(b, with), objective_weights(b, with));
            assert!(
                p.iter().zip(base.0).all(|(n, old)| *n >= old),
                "{extra:?} {p:?} {:?}",
                base.0
            );
            assert!(
                o.iter().zip(base.1).all(|(n, old)| *n >= old),
                "{extra:?} {o:?} {:?}",
                base.1
            );
        }
    }
}

// ---- building out an island ----

struct Sea {
    map: Map,
    land: f4x_core::map::TerrainId,
}

/// An ocean `w` by `h`, with the squares of `land` turned to grassland, both planes alike.
fn sea(w: u32, h: u32, land: &[(u32, u32)]) -> Sea {
    let c = content();
    let ocean = c
        .terrains
        .first_of_class(TerrainClass::Ocean, Plane::Arcanus)
        .expect("ocean");
    let grass = c
        .terrains
        .first_of_class(TerrainClass::Grassland, Plane::Arcanus)
        .expect("grass");
    let mut a = Grid::new(w, h, ocean);
    for &(x, y) in land {
        a.set(x, y, grass);
    }
    let b = a.clone();
    Sea {
        map: Map::new(a, b),
        land: grass,
    }
}

/// A small land mass grown from `at` by a walk of `steps` squares, from a seed.
fn blob(seed: u64, w: u32, h: u32, at: (u32, u32), steps: u32) -> Vec<(u32, u32)> {
    let mut z = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move || {
        z ^= z << 13;
        z ^= z >> 7;
        z ^= z << 17;
        z
    };
    let mut cells = vec![at];
    let mut here = at;
    for _ in 0..steps {
        let (dx, dy) = [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)][(next() % 4) as usize];
        here = (
            (here.0 as i32 + dx).rem_euclid(w as i32) as u32,
            (here.1 as i32 + dy).clamp(1, h as i32 - 2) as u32,
        );
        cells.push(here);
    }
    cells
}

#[test]
fn growth_gives_an_island_its_ground_and_touches_nothing_it_may_not() {
    let c = content();
    let (w, h) = (24u32, 20u32);
    let at = Position {
        plane: Plane::Arcanus,
        x: 12,
        y: 10,
    };
    for seed in 0..300u64 {
        let steps = (seed % 14) as u32;
        let cells = blob(seed, w, h, (at.x, at.y), steps);
        let mut s = sea(w, h, &cells);
        // A feature stands on some square beside the island: it must stay as it is.
        let taken = |x: i32, y: i32| (x, y) == (at.x as i32 + 2, at.y as i32);
        let before = land_mass(&s.map, c, at, MIN_LAND);
        let added = growth(&s.map, c, seed, at, &taken);
        assert_eq!(added, growth(&s.map, c, seed, at, &taken), "deterministic");
        let mut once = added.clone();
        once.sort_unstable();
        once.dedup();
        assert_eq!(once.len(), added.len(), "a square twice");
        for &(x, y) in &added {
            assert!(y < h && x < w);
            assert_ne!(
                (x as i32, y as i32),
                (at.x as i32 + 2, at.y as i32),
                "a taken square"
            );
            assert!(!cells.contains(&(x, y)), "land that was land");
            s.map.plane_mut(Plane::Arcanus).set(x, y, s.land);
        }
        let after = land_mass(&s.map, c, at, MIN_LAND);
        // Whatever it grew is one land mass with the town, and the mass has what it needs unless
        // there was nowhere to grow (the taken square and the edges can leave none).
        assert!(after.len() >= before.len());
        assert!(
            after.len() >= MIN_LAND.min(w as usize * h as usize / 4),
            "{seed}: {}",
            after.len()
        );
        assert!(
            land_near(&s.map, c, at) >= MIN_NEAR,
            "seed {seed}: the town has no ground to work"
        );
        for &(x, y) in &added {
            assert!(
                after.contains(&(x, y)) || after.len() >= MIN_LAND,
                "{seed}: ({x},{y}) is cut off"
            );
        }
    }
}

#[test]
fn an_island_that_is_big_enough_is_left_alone_and_the_sea_has_nothing_to_grow() {
    let c = content();
    let at = Position {
        plane: Plane::Arcanus,
        x: 10,
        y: 10,
    };
    // A block of land: more squares than the least, all of it ground to work.
    let side = 5;
    let block: Vec<(u32, u32)> = (0..side)
        .flat_map(|y| (0..side).map(move |x| (8 + x, 8 + y)))
        .collect();
    let s = sea(24, 20, &block);
    assert!(block.len() >= MIN_LAND);
    assert!(growth(&s.map, c, 1, at, &|_, _| false).is_empty());
    // A town in the open sea has no land to build out from: nothing is made up.
    let s = sea(24, 20, &[]);
    assert!(growth(&s.map, c, 1, at, &|_, _| false).is_empty());
}

#[test]
fn growth_does_not_turn_a_lake_into_ground() {
    let c = content();
    let at = Position {
        plane: Plane::Arcanus,
        x: 10,
        y: 10,
    };
    let mut s = sea(24, 20, &[(10, 10)]);
    for (x, y) in [(11, 10), (9, 10), (10, 11), (10, 9)] {
        s.map.plane_mut(Plane::Arcanus).set_lake(x, y, true);
    }
    for seed in 0..50 {
        let added = growth(&s.map, c, seed, at, &|_, _| false);
        for sq in [(11, 10), (9, 10), (10, 11), (10, 9)] {
            assert!(!added.contains(&sq), "seed {seed}: grew over a lake");
        }
    }
}

// ---- the maps of other sizes grow the same way ----

#[test]
fn growth_on_a_map_that_wraps_reaches_across_the_edge() {
    let c = content();
    // A town on the west edge with its only land on the east edge (the same island).
    let (w, h) = (24u32, 20u32);
    let s = sea(w, h, &[(0, 10), (w - 1, 10)]);
    let at = Position {
        plane: Plane::Arcanus,
        x: 0,
        y: 10,
    };
    assert_eq!(land_mass(&s.map, c, at, 4).len(), 2);
    let added = growth(&s.map, c, 3, at, &|_, _| false);
    assert!(!added.is_empty());
}

#[test]
fn a_mass_too_small_is_built_out_even_when_there_is_ground_to_work_round_it() {
    // The town's own land is a pair of squares, but islets lie all round it within the work
    // radius: the ground to work is enough and the mass joined to the town is not.
    let c = content();
    let at = Position {
        plane: Plane::Arcanus,
        x: 12,
        y: 10,
    };
    let radius = f4x_newgame::island::WORK_RADIUS as i32;
    let mut land = vec![(12, 10), (13, 10)];
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let (x, y) = (12 + dx, 10 + dy);
            // Both offsets even and further than a square away: nothing touches the town or
            // another islet by a side.
            if dx % 2 == 0 && dy % 2 == 0 && (dx, dy) != (0, 0) && (dx, dy) != (1, 0) {
                land.push((x as u32, y as u32));
            }
        }
    }
    let s = sea(24, 20, &land);
    assert!(
        land_near(&s.map, c, at) >= MIN_NEAR,
        "the test's islets are too few"
    );
    assert!(land_mass(&s.map, c, at, MIN_LAND).len() < MIN_LAND);
    assert!(!growth(&s.map, c, 1, at, &|_, _| false).is_empty());
}

#[test]
fn ground_to_work_comes_first_and_nothing_more_than_needed_is_added() {
    // A town at the end of a long strip: the mass is big enough, the ground within the work
    // radius is not. Exactly the missing squares are added, all of them in reach of the town.
    let c = content();
    let at = Position {
        plane: Plane::Arcanus,
        x: 4,
        y: 10,
    };
    let strip: Vec<(u32, u32)> = (4..4 + MIN_LAND as u32 + 2).map(|x| (x, 10)).collect();
    // The strip runs east from the town: two squares of it are in reach (the radius is the same
    // either way round, but the walk of the strip's far end is not).
    let s = sea(30, 20, &strip);
    let near = land_near(&s.map, c, at);
    assert!(
        near < MIN_NEAR,
        "the strip gives {near} squares of ground to work"
    );
    assert!(land_mass(&s.map, c, at, MIN_LAND).len() >= MIN_LAND);
    let radius = f4x_newgame::island::WORK_RADIUS;
    for seed in 0..100 {
        let added = growth(&s.map, c, seed, at, &|_, _| false);
        assert_eq!(added.len(), MIN_NEAR - near, "seed {seed}: {added:?}");
        for &(x, y) in &added {
            let reach = (x as i32 - at.x as i32)
                .unsigned_abs()
                .max((y as i32 - at.y as i32).unsigned_abs());
            assert!(reach <= radius, "seed {seed}: ({x},{y}) is out of reach");
        }
    }
}

#[test]
fn growth_may_use_the_top_row_of_the_map() {
    // The town stands on the second row: some seed builds out onto the first.
    let c = content();
    let at = Position {
        plane: Plane::Arcanus,
        x: 10,
        y: 1,
    };
    let s = sea(24, 20, &[(10, 1)]);
    let top = (0..200).any(|seed| {
        growth(&s.map, c, seed, at, &|_, _| false)
            .iter()
            .any(|&(_, y)| y == 0)
    });
    assert!(top, "no seed used the top row");
}
