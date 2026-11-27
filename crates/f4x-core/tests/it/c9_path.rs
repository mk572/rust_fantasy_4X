//! Class 9, shortest ways: the searches of `path_grid` held to a plain Dijkstra that has the step
//! rule written out again. The grids are random on purpose: the module knows nothing of units or
//! rules, and every grid is one it must handle.

use f4x_core::path_grid::{Cost, Costs, PathGrid, PathGridError, Road, Searcher, Square};
use f4x_def::testdir::sample;
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

/// A random plane: costs from half a move to four moves, some water that nothing enters, roads on some
/// squares, some of them enchanted, and rivers.
struct Plane {
    grid: PathGrid,
    costs: Costs,
    roads: Vec<Road>,
    rivers: Vec<bool>,
    stops: HashSet<Square>,
    diagonal_extra: bool,
}

fn random_rivers(rng: &mut Pcg64Mcg, n: usize) -> Vec<bool> {
    let share = rng.gen_range(0..40);
    (0..n).map(|_| rng.gen_range(0..100) < share).collect()
}

fn random_roads(rng: &mut Pcg64Mcg, n: usize) -> Vec<Road> {
    let road_share = rng.gen_range(0..60);
    let magic = rng.gen_bool(0.3);
    (0..n)
        .map(|_| match rng.gen_range(0..100) {
            r if r >= road_share => Road::None,
            _ if magic => Road::Enchanted,
            r if r % 5 == 0 => Road::Enchanted,
            _ => Road::Plain,
        })
        .collect()
}

/// The costs of one random walker: the squares it cannot enter, what each costs to enter and
/// to enter along a road.
fn random_costs(rng: &mut Pcg64Mcg, n: usize) -> Costs {
    let blocked = rng.gen_range(0..40);
    let mut costs = Costs {
        enter: vec![None; n],
        by_road: vec![None; n],
        by_enchanted_road: vec![None; n],
    };
    for i in 0..n {
        if rng.gen_range(0..100) >= blocked {
            costs.enter[i] = Some(Cost::HALF * rng.gen_range(1..=8));
            costs.by_road[i] = Some(Cost::HALF * rng.gen_range(0..=3));
            costs.by_enchanted_road[i] = Some(Cost::HALF * rng.gen_range(0..=2));
        }
    }
    costs
}

fn random_plane(seed: u64, width: u32, height: u32, diagonal_extra: bool) -> Plane {
    let mut rng = Pcg64Mcg::seed_from_u64(seed);
    let n = (width * height) as usize;
    let roads = random_roads(&mut rng, n);
    let costs = random_costs(&mut rng, n);
    let stops = (0..n as Square)
        .filter(|_| rng.gen_range(0..100) < 6)
        .collect();
    let rivers = random_rivers(&mut rng, n);
    let grid = PathGrid::new(width, height, costs.clone(), roads.clone(), diagonal_extra)
        .expect("a grid of the right size")
        .with_rivers(rivers.clone());
    Plane {
        grid,
        costs,
        roads,
        rivers,
        stops,
        diagonal_extra,
    }
}

/// The squares beside `from`: the plane wraps east and west only.
fn around(grid: &PathGrid, from: Square) -> Vec<Square> {
    let (w, h) = (grid.width() as i32, grid.height() as i32);
    let (x, y) = grid.xy(from);
    (-1..=1)
        .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
        .filter(|&d| d != (0, 0))
        .filter(|&(_, dy)| (0..h).contains(&(y as i32 + dy)))
        .map(|(dx, dy)| grid.square((x as i32 + dx).rem_euclid(w) as u32, (y as i32 + dy) as u32))
        .collect()
}

impl Plane {
    /// The rule of a step, written out again from the rule and not from the grid.
    fn step(&self, from: Square, to: Square) -> Option<Cost> {
        let (w, h) = (self.grid.width() as i32, self.grid.height() as i32);
        let at = |s: Square| ((s % w as u32) as i32, (s / w as u32) as i32);
        let (a, b) = (at(from), at(to));
        let dx = (a.0 - b.0).rem_euclid(w).min((b.0 - a.0).rem_euclid(w));
        let dy = (a.1 - b.1).abs();
        if dx > 1 || dy > 1 || (dx == 0 && dy == 0) || b.1 >= h {
            return None;
        }
        let to_i = to as usize;
        self.costs.enter[to_i]?;
        let (road_from, road_to) = (self.roads[from as usize], self.roads[to_i]);
        let cost = if road_from == Road::Enchanted && road_to == Road::Enchanted {
            self.costs.by_enchanted_road[to_i]
        } else if road_from != Road::None && road_to != Road::None {
            self.costs.by_road[to_i]
        } else if self.rivers[from as usize] && self.rivers[to_i] {
            // A river step costs what a road step does, never more than entering.
            self.costs.enter[to_i]
                .zip(self.costs.by_road[to_i])
                .map(|(a, b)| a.min(b))
        } else {
            self.costs.enter[to_i]
        }?;
        Some(if self.diagonal_extra && dx == 1 && dy == 1 {
            cost + cost.half()
        } else {
            cost
        })
    }

    /// A plain Dijkstra over every square: the least cost from `from` to each square, with
    /// `stops` squares ended on but not left (the start is left whatever it is).
    fn reference(&self, from: Square, stops: &HashSet<Square>) -> Vec<Option<Cost>> {
        let n = self.grid.squares();
        let mut best: Vec<Option<Cost>> = vec![None; n];
        let mut open = BinaryHeap::new();
        best[from as usize] = Some(Cost::ZERO);
        open.push(Reverse((Cost::ZERO, from)));
        while let Some(Reverse((cost, here))) = open.pop() {
            if best[here as usize] != Some(cost) {
                continue;
            }
            if here != from && stops.contains(&here) {
                continue;
            }
            for next in 0..n as Square {
                if let Some(step) = self.step(here, next) {
                    if best[next as usize].is_none_or(|b| cost + step < b) {
                        best[next as usize] = Some(cost + step);
                        open.push(Reverse((cost + step, next)));
                    }
                }
            }
        }
        best
    }

    fn cost_of(&self, from: Square, path: &[Square]) -> Cost {
        let mut at = from;
        let mut sum = Cost::ZERO;
        for &next in path {
            sum += self
                .step(at, next)
                .expect("the path takes only steps the rule allows");
            at = next;
        }
        sum
    }
}

fn planes() -> impl Iterator<Item = Plane> {
    (0..sample(600, 60) as u64).map(|seed| {
        random_plane(
            seed,
            7 + (seed % 5) as u32 * 3,
            5 + (seed % 4) as u32 * 2,
            seed % 2 == 0,
        )
    })
}

#[test]
fn a_path_costs_what_the_cheapest_walk_costs_and_never_goes_through_a_stop() {
    let mut searcher = Searcher::new();
    for plane in planes() {
        let n = plane.grid.squares() as Square;
        for from in (0..n).step_by(3) {
            let best = plane.reference(from, &plane.stops);
            for to in (0..n).step_by(2) {
                let found = searcher.path(
                    &plane.grid,
                    from,
                    to,
                    |s| plane.stops.contains(&s),
                    |_| false,
                );
                let expected = if from == to { None } else { best[to as usize] };
                assert_eq!(
                    found.as_deref().map(|p| plane.cost_of(from, p)),
                    expected,
                    "from {from} to {to}"
                );
                if let Some(path) = found {
                    assert_eq!(path.last(), Some(&to));
                    assert!(
                        path[..path.len() - 1]
                            .iter()
                            .all(|s| !plane.stops.contains(s)),
                        "the path goes through a stop"
                    );
                }
            }
        }
    }
}

#[test]
fn a_walk_goes_over_what_the_reference_reaches() {
    let mut searcher = Searcher::new();
    for plane in planes() {
        let n = plane.grid.squares() as Square;
        for from in (0..n).step_by(4) {
            let best = plane.reference(from, &plane.stops);
            // The start is never a stop of its own walk, and a stop is not entered.
            let found = searcher.walk(&plane.grid, from, &[], |s| plane.stops.contains(&s));
            assert!(!found, "a walk with no target finds none");
            let walked: HashSet<Square> = searcher.settled().iter().copied().collect();
            let expected: HashSet<Square> = (0..n)
                .filter(|&s| best[s as usize].is_some() && (s == from || !plane.stops.contains(&s)))
                .collect();
            assert_eq!(walked, expected, "from {from}");
        }
    }
}

#[test]
fn a_target_is_reached_when_the_reference_reaches_it() {
    let mut searcher = Searcher::new();
    for plane in planes() {
        let n = plane.grid.squares() as Square;
        for from in (0..n).step_by(5) {
            let best = plane.reference(from, &plane.stops);
            let targets: Vec<Square> = (0..n).filter(|s| s % 11 == from % 11).collect();
            let expected = targets
                .iter()
                .any(|&t| t == from || (best[t as usize].is_some() && !plane.stops.contains(&t)));
            let found =
                searcher.reaches_any(&plane.grid, from, &targets, |s| plane.stops.contains(&s));
            assert_eq!(found, expected, "from {from}");
        }
    }
}

#[test]
fn a_square_of_another_region_cannot_be_reached_by_any_walk() {
    for plane in planes() {
        let n = plane.grid.squares() as Square;
        for from in 0..n {
            let best = plane.reference(from, &HashSet::new());
            for to in 0..n {
                if from != to && !plane.grid.maybe_reachable(from, to) {
                    assert_eq!(best[to as usize], None, "from {from} to {to}");
                }
            }
        }
    }
}

#[test]
fn the_distance_field_gives_what_a_search_from_each_square_gives() {
    let mut field = Searcher::new();
    let mut forward = Searcher::new();
    for plane in planes() {
        let n = plane.grid.squares() as Square;
        let target = n / 3;
        field.distance_field(&plane.grid, &[target], |s| plane.stops.contains(&s));
        for from in 0..n {
            if from == target {
                assert_eq!(field.distance(from), Some(Cost::ZERO));
                continue;
            }
            let found = forward
                .path(
                    &plane.grid,
                    from,
                    target,
                    |s| plane.stops.contains(&s),
                    |_| false,
                )
                .map(|p| plane.cost_of(from, &p));
            assert_eq!(field.distance(from), found, "from {from} to {target}");
        }
    }
}

#[test]
fn a_group_walks_at_the_pace_of_its_slowest_part_over_what_every_part_can_enter() {
    let mut searcher = Searcher::new();
    for seed in 0..sample(200, 20) as u64 {
        let mut rng = Pcg64Mcg::seed_from_u64(seed + 1000);
        let (width, height, diagonal_extra) =
            (9 + seed as u32 % 4, 6 + seed as u32 % 3, seed % 2 == 0);
        let n = (width * height) as usize;
        let roads = random_roads(&mut rng, n);
        let rivers = random_rivers(&mut rng, n);
        let planes: Vec<Plane> = (0..3)
            .map(|_| {
                let mut costs = random_costs(&mut rng, n);
                // As a walker's costs are in the game: a road never dearer than entering (the
                // step along a river counts on it).
                for (road, enter) in costs.by_road.iter_mut().zip(&costs.enter) {
                    *road = road.zip(*enter).map(|(r, e)| r.min(e));
                }
                Plane {
                    grid: PathGrid::new(
                        width,
                        height,
                        costs.clone(),
                        roads.clone(),
                        diagonal_extra,
                    )
                    .expect("a grid of the right size")
                    .with_rivers(rivers.clone()),
                    costs,
                    roads: roads.clone(),
                    rivers: rivers.clone(),
                    stops: HashSet::new(),
                    diagonal_extra,
                }
            })
            .collect();
        let parts: Vec<&PathGrid> = planes.iter().map(|p| &p.grid).collect();
        let group = PathGrid::slowest(&parts).expect("parts of one plane");
        for from in 0..n as Square {
            for to in around(&group, from) {
                let each: Option<Vec<Cost>> = planes.iter().map(|p| p.step(from, to)).collect();
                assert_eq!(
                    group.step_cost(from, to),
                    each.map(|c| c.into_iter().max().unwrap_or(Cost::ZERO)),
                    "from {from} to {to}"
                );
            }
        }
        let (from, to) = (1, n as Square - 2);
        let found = searcher.path(&group, from, to, |_| false, |_| false);
        let stack: Vec<Option<Cost>> = planes
            .iter()
            .map(|p| p.reference(from, &HashSet::new())[to as usize])
            .collect();
        // Reachable for the group only where it is for every part.
        if found.is_some() {
            assert!(stack.iter().all(Option::is_some), "seed {seed}");
        }
    }
}

#[test]
fn parts_of_different_planes_make_no_group() {
    let small = PathGrid::new(
        2,
        2,
        Costs::with_ordinary_roads(vec![Some(Cost::from_int(1)); 4]),
        vec![Road::None; 4],
        false,
    )
    .expect("a grid of the right size");
    let big = PathGrid::new(
        3,
        2,
        Costs::with_ordinary_roads(vec![Some(Cost::from_int(1)); 6]),
        vec![Road::None; 6],
        false,
    )
    .expect("a grid of the right size");
    assert_eq!(
        PathGrid::slowest(&[&small, &big]).err(),
        Some(PathGridError::Mismatch)
    );
    assert_eq!(PathGrid::slowest(&[]).err(), Some(PathGridError::Empty));
}

#[test]
fn a_grid_must_have_a_cost_for_each_square() {
    assert_eq!(
        PathGrid::new(
            3,
            2,
            Costs::with_ordinary_roads(vec![Some(Cost::HALF); 5]),
            vec![Road::None; 6],
            false
        )
        .err(),
        Some(PathGridError::Size {
            expected: 6,
            got: 5
        })
    );
    assert_eq!(
        PathGrid::new(0, 2, Costs::with_ordinary_roads(vec![]), vec![], false).err(),
        Some(PathGridError::Empty)
    );
}

#[test]
fn the_plane_wraps_east_and_west_but_not_north_and_south() {
    let grid = PathGrid::new(
        5,
        3,
        Costs::with_ordinary_roads(vec![Some(Cost::from_int(1)); 15]),
        vec![Road::None; 15],
        false,
    )
    .expect("a grid of the right size");
    let mut searcher = Searcher::new();
    let west_to_east = searcher.path(
        &grid,
        grid.square(0, 1),
        grid.square(4, 1),
        |_| false,
        |_| false,
    );
    assert_eq!(west_to_east, Some(vec![grid.square(4, 1)]));
    let north_to_south = searcher.path(
        &grid,
        grid.square(0, 0),
        grid.square(0, 2),
        |_| false,
        |_| false,
    );
    assert_eq!(north_to_south.map(|p| p.len()), Some(2));
}

#[test]
fn a_closed_square_is_never_entered_not_even_as_the_goal() {
    let mut searcher = Searcher::new();
    for plane in planes() {
        let n = plane.grid.squares() as Square;
        let closed: HashSet<Square> = (0..n).filter(|s| s % 7 == 3).collect();
        let from = 0;
        for to in 1..n {
            let found = searcher.path(&plane.grid, from, to, |_| false, |s| closed.contains(&s));
            if closed.contains(&to) {
                assert_eq!(found, None, "to {to}");
            } else if let Some(path) = found {
                assert!(
                    path.iter().all(|s| !closed.contains(s)),
                    "to {to}: {path:?}"
                );
            }
        }
    }
}
