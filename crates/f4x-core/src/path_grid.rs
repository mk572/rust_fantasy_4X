//! Shortest ways over a grid of numbers: a cost to enter each square, roads, squares nothing can
//! enter, and a plane that wraps east and west. It knows nothing of units or rules; a caller
//! turns what a walker can do into a [`PathGrid`] (one grid for each kind of mover, and
//! [`PathGrid::slowest`] for a group of them) and what stops it into a closure.
//!
//! Tests: random grids against a plain Dijkstra, at the bottom of this file.

/// A square of the grid: `y * width + x`.
pub type Square = u32;

/// A cost in half moves.
pub type Cost = u32;

/// The entry cost of a square nothing can enter.
pub const BLOCKED: u8 = u8::MAX;

/// What a square's road lets a step cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Road {
    #[default]
    None,
    Plain,
    /// A step between two enchanted squares costs nothing.
    Enchanted,
}

/// No square (the start of a path, the region of a blocked square).
const NONE: u32 = u32::MAX;

/// The eight squares round one: the count is the point.
const AROUND: &[(i32, i32)] = &[
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathGridError {
    Empty,
    /// A table has not one entry for each square.
    Size {
        expected: usize,
        got: usize,
    },
    /// Grids that are not of one plane cannot be put together.
    Mismatch,
}

/// What it costs a walker, in half moves, to enter each square (row by row, `BLOCKED` where it
/// cannot), and to enter it along a road: from a square with a road, or two with enchanted roads.
/// A road bears on a step only when both squares have one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Costs {
    pub enter: Vec<u8>,
    pub by_road: Vec<u8>,
    pub by_enchanted_road: Vec<u8>,
}

impl Costs {
    /// The costs of a walker for whom a road step costs half a move and an enchanted one nothing.
    pub fn with_ordinary_roads(enter: Vec<u8>) -> Self {
        let by = |cost: u8| {
            enter
                .iter()
                .map(|&e| if e == BLOCKED { BLOCKED } else { cost })
                .collect()
        };
        Self {
            by_road: by(1),
            by_enchanted_road: by(0),
            enter,
        }
    }
}

/// The cost of entering each square, fixed until the map or the walker changes.
#[derive(Debug, Clone)]
pub struct PathGrid {
    width: u32,
    height: u32,
    costs: Costs,
    roads: Vec<Road>,
    diagonal_extra: bool,
    /// The connected piece of the squares a walker can enter each square lies in (not counting
    /// what stops a walk); `NONE` for a blocked square.
    region: Vec<u32>,
    /// The least and the most one step costs, for the heuristic and the size of the queue.
    min_step: Cost,
    max_step: Cost,
}

impl PathGrid {
    /// `costs` and `roads` have one entry per square, row by row. With `diagonal_extra` a
    /// diagonal step costs one and a half times as much, rounded up.
    pub fn new(
        width: u32,
        height: u32,
        costs: Costs,
        roads: Vec<Road>,
        diagonal_extra: bool,
    ) -> Result<Self, PathGridError> {
        if width == 0 || height == 0 {
            return Err(PathGridError::Empty);
        }
        let expected = width as usize * height as usize;
        for got in [
            costs.enter.len(),
            costs.by_road.len(),
            costs.by_enchanted_road.len(),
            roads.len(),
        ] {
            if got != expected {
                return Err(PathGridError::Size { expected, got });
            }
        }
        let mut grid = Self {
            width,
            height,
            costs,
            roads,
            diagonal_extra,
            region: Vec::new(),
            min_step: 0,
            max_step: 0,
        };
        grid.measure();
        grid.label_regions();
        Ok(grid)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn squares(&self) -> usize {
        self.costs.enter.len()
    }

    pub fn square(&self, x: u32, y: u32) -> Square {
        y * self.width + x
    }

    pub fn xy(&self, square: Square) -> (u32, u32) {
        (square % self.width, square / self.width)
    }

    pub fn is_blocked(&self, square: Square) -> bool {
        self.costs.enter[square as usize] == BLOCKED
    }

    /// The cost of the step from `from` to the square next to it, `None` when it cannot be
    /// taken.
    pub fn step_cost(&self, from: Square, to: Square) -> Option<Cost> {
        let ((fx, fy), (tx, ty)) = (self.xy(from), self.xy(to));
        self.step(from, to, fx != tx && fy != ty)
    }

    fn step(&self, from: Square, to: Square, diagonal: bool) -> Option<Cost> {
        let i = to as usize;
        if self.costs.enter[i] == BLOCKED {
            return None;
        }
        let cost = Cost::from(match (self.roads[from as usize], self.roads[i]) {
            (Road::None, _) | (_, Road::None) => self.costs.enter[i],
            (Road::Enchanted, Road::Enchanted) => self.costs.by_enchanted_road[i],
            _ => self.costs.by_road[i],
        });
        Some(if diagonal && self.diagonal_extra {
            (cost * 3).div_ceil(2)
        } else {
            cost
        })
    }

    /// The region a square lies in: squares in different regions cannot be walked between, with
    /// nothing in the way. `None` for a square nothing can enter.
    pub fn region(&self, square: Square) -> Option<u32> {
        let r = self.region[square as usize];
        (r != NONE).then_some(r)
    }

    /// `false` only when there is no way from `from` to `to` even with nothing in the way. A
    /// walker that stands on a square it could not enter has no region, and then this cannot say.
    pub fn maybe_reachable(&self, from: Square, to: Square) -> bool {
        if self.is_blocked(to) {
            return false;
        }
        match (self.region(from), self.region(to)) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
    }

    fn measure(&mut self) {
        let (mut min, mut max) = (Cost::MAX, 0);
        let mut note = |cost: u8| {
            min = min.min(Cost::from(cost));
            max = max.max(Cost::from(cost));
        };
        for (i, &enter) in self.costs.enter.iter().enumerate() {
            if enter == BLOCKED {
                continue;
            }
            note(enter);
            // A road step needs a road at both ends: the cheaper kind only where there is one.
            match self.roads[i] {
                Road::None => {}
                Road::Plain => note(self.costs.by_road[i]),
                Road::Enchanted => {
                    note(self.costs.by_road[i]);
                    note(self.costs.by_enchanted_road[i]);
                }
            }
        }
        if min == Cost::MAX {
            min = 0;
        }
        self.min_step = min;
        self.max_step = if self.diagonal_extra {
            (max * 3).div_ceil(2)
        } else {
            max
        };
    }

    /// The grid of a group that moves together, at the pace of the slowest: a step costs what the
    /// dearest of the parts pays for it, and a square any part cannot enter is blocked to all.
    /// The parts must share the plane (size, roads, the diagonal rule).
    pub fn slowest(parts: &[&PathGrid]) -> Result<Self, PathGridError> {
        let Some((first, rest)) = parts.split_first() else {
            return Err(PathGridError::Empty);
        };
        let mut costs = first.costs.clone();
        for part in rest {
            if (part.width, part.height, part.diagonal_extra)
                != (first.width, first.height, first.diagonal_extra)
                || part.roads != first.roads
            {
                return Err(PathGridError::Mismatch);
            }
            for (mine, theirs) in [
                (&mut costs.enter, &part.costs.enter),
                (&mut costs.by_road, &part.costs.by_road),
                (&mut costs.by_enchanted_road, &part.costs.by_enchanted_road),
            ] {
                for (m, &t) in mine.iter_mut().zip(theirs) {
                    *m = (*m).max(t);
                }
            }
        }
        Self::new(
            first.width,
            first.height,
            costs,
            first.roads.clone(),
            first.diagonal_extra,
        )
    }

    fn label_regions(&mut self) {
        let mut region = vec![NONE; self.squares()];
        let mut next = 0;
        let mut stack = Vec::new();
        for first in 0..self.squares() as Square {
            if self.is_blocked(first) || region[first as usize] != NONE {
                continue;
            }
            region[first as usize] = next;
            stack.push(first);
            while let Some(here) = stack.pop() {
                for (there, _) in self.around(here) {
                    if !self.is_blocked(there) && region[there as usize] == NONE {
                        region[there as usize] = next;
                        stack.push(there);
                    }
                }
            }
            next += 1;
        }
        self.region = region;
    }

    /// The squares round `square` (the plane wraps east and west and stops at the north and the
    /// south) with whether the step to each is diagonal.
    fn around(&self, square: Square) -> impl Iterator<Item = (Square, bool)> + '_ {
        let (x, y) = self.xy(square);
        AROUND.iter().filter_map(move |&(dx, dy)| {
            let ny = y.checked_add_signed(dy).filter(|&ny| ny < self.height)?;
            let nx = (x + self.width).wrapping_add_signed(dx) % self.width;
            let next = self.square(nx, ny);
            (next != square).then_some((next, dx != 0 && dy != 0))
        })
    }

    /// The least a walk from `from` to `to` can cost: the squares in between, each at the least a
    /// step costs.
    fn estimate(&self, from: Square, to: (u32, u32)) -> Cost {
        let (x, y) = self.xy(from);
        let dx = x.abs_diff(to.0);
        dx.min(self.width - dx).max(y.abs_diff(to.1)) * self.min_step
    }
}

/// The queue of a search whose costs are small whole numbers: one list for each cost, in a ring
/// as long as the greatest jump from one cost to the next, so a push and a pop take no
/// searching.
#[derive(Default)]
struct Buckets {
    ring: Vec<Vec<(Square, Cost)>>,
    key: u32,
    len: usize,
}

impl Buckets {
    fn reset(&mut self, size: usize) {
        if self.ring.len() == size {
            self.ring.iter_mut().for_each(Vec::clear);
        } else {
            self.ring = vec![Vec::new(); size];
        }
        self.key = 0;
        self.len = 0;
    }

    fn push(&mut self, key: u32, entry: (Square, Cost)) {
        let key = key.max(self.key);
        let size = self.ring.len();
        self.ring[key as usize % size].push(entry);
        self.len += 1;
    }

    fn pop(&mut self) -> Option<(Square, Cost)> {
        if self.len == 0 {
            return None;
        }
        let size = self.ring.len();
        loop {
            if let Some(entry) = self.ring[self.key as usize % size].pop() {
                self.len -= 1;
                return Some(entry);
            }
            self.key += 1;
        }
    }
}

/// What a search looks for.
enum Goal<'a> {
    /// One square, which stops do not apply to; the search is an A*.
    Square(Square),
    /// Any of some squares.
    Any(&'a [Square]),
    /// Everything that can be walked to.
    Everything,
}

/// The tables of searches, kept between them: nothing is cleared, a square's entry counts only
/// when it carries the stamp of the search.
#[derive(Default)]
pub struct Searcher {
    stamp: u32,
    mark: Vec<u32>,
    cost: Vec<Cost>,
    came: Vec<Square>,
    stop_mark: Vec<u32>,
    stop: Vec<bool>,
    settled: Vec<Square>,
    queue: Buckets,
}

impl Searcher {
    pub fn new() -> Self {
        Self::default()
    }

    fn begin(&mut self, grid: &PathGrid) {
        let squares = grid.squares();
        if self.mark.len() != squares {
            self.mark = vec![0; squares];
            self.cost = vec![0; squares];
            self.came = vec![NONE; squares];
            self.stop_mark = vec![0; squares];
            self.stop = vec![false; squares];
            self.stamp = 0;
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.mark.fill(0);
            self.stop_mark.fill(0);
            self.stamp = 1;
        }
        self.settled.clear();
        self.queue
            .reset((grid.max_step + grid.min_step) as usize + 1);
    }

    fn known(&self, square: Square) -> Option<Cost> {
        (self.mark[square as usize] == self.stamp).then(|| self.cost[square as usize])
    }

    fn set(&mut self, square: Square, cost: Cost, came: Square) {
        self.mark[square as usize] = self.stamp;
        self.cost[square as usize] = cost;
        self.came[square as usize] = came;
    }

    /// Whether `stops` holds for the square, asked once in a search.
    fn stopped(&mut self, square: Square, stops: &impl Fn(Square) -> bool) -> bool {
        let i = square as usize;
        if self.stop_mark[i] != self.stamp {
            self.stop[i] = stops(square);
            self.stop_mark[i] = self.stamp;
        }
        self.stop[i]
    }

    /// The cheapest way from `from` to `to`, without `from`; `None` when there is none, or
    /// `from == to`. A square that `stops` holds for cannot be walked through, only ended on.
    pub fn path(
        &mut self,
        grid: &PathGrid,
        from: Square,
        to: Square,
        stops: impl Fn(Square) -> bool,
    ) -> Option<Vec<Square>> {
        if from == to || !grid.maybe_reachable(from, to) {
            return None;
        }
        let end = self.run(grid, from, &Goal::Square(to), &stops)?;
        let mut path = vec![end];
        let mut at = end;
        while self.came[at as usize] != from {
            at = self.came[at as usize];
            path.push(at);
        }
        path.reverse();
        Some(path)
    }

    /// Whether any of `targets` can be walked to (`from` among them counts). A square that
    /// `stops` holds for is not entered, targets included.
    pub fn reaches_any(
        &mut self,
        grid: &PathGrid,
        from: Square,
        targets: &[Square],
        stops: impl Fn(Square) -> bool,
    ) -> bool {
        if targets.contains(&from) {
            return true;
        }
        if !targets.iter().any(|&t| grid.maybe_reachable(from, t)) {
            return false;
        }
        self.run(grid, from, &Goal::Any(targets), &stops).is_some()
    }

    /// Walk over everything that can be reached from `from`, or until one of `targets` is; the
    /// squares walked over are [`Searcher::settled`]. Whether a target was found.
    pub fn walk(
        &mut self,
        grid: &PathGrid,
        from: Square,
        targets: &[Square],
        stops: impl Fn(Square) -> bool,
    ) -> bool {
        let goal = if targets.is_empty() {
            Goal::Everything
        } else {
            Goal::Any(targets)
        };
        self.run(grid, from, &goal, &stops).is_some()
    }

    /// The squares the last [`Searcher::walk`] went over, the start first.
    pub fn settled(&self) -> &[Square] {
        &self.settled
    }

    fn run(
        &mut self,
        grid: &PathGrid,
        from: Square,
        goal: &Goal,
        stops: &impl Fn(Square) -> bool,
    ) -> Option<Square> {
        self.begin(grid);
        let aim = match goal {
            Goal::Square(to) => Some(grid.xy(*to)),
            _ => None,
        };
        let guess = |square: Square| aim.map_or(0, |to| grid.estimate(square, to));
        self.set(from, 0, NONE);
        self.queue.push(guess(from), (from, 0));
        while let Some((here, spent)) = self.queue.pop() {
            if self.known(here).is_some_and(|best| spent > best) {
                continue;
            }
            match goal {
                Goal::Square(to) if *to == here => return Some(here),
                Goal::Square(_) => {}
                Goal::Any(targets) => {
                    self.settled.push(here);
                    if targets.contains(&here) {
                        return Some(here);
                    }
                }
                Goal::Everything => self.settled.push(here),
            }
            for (next, diagonal) in grid.around(here) {
                let is_goal = matches!(goal, Goal::Square(to) if *to == next);
                if !is_goal && self.stopped(next, stops) {
                    continue;
                }
                let Some(step) = grid.step(here, next, diagonal) else {
                    continue;
                };
                let total = spent + step;
                if self.known(next).is_none_or(|best| total < best) {
                    self.set(next, total, here);
                    self.queue.push(total + guess(next), (next, total));
                }
            }
        }
        None
    }

    /// What every square costs to walk to `targets`, which a lookup with [`Searcher::distance`]
    /// gives: one search for any number of walkers. The cost is that of the steps a walker takes
    /// from the square on, so a walker standing on a square that `stops` holds for gets its cost
    /// too, and only squares it has no way through are left out.
    pub fn distance_field(
        &mut self,
        grid: &PathGrid,
        targets: &[Square],
        stops: impl Fn(Square) -> bool,
    ) {
        self.begin(grid);
        for &t in targets {
            self.set(t, 0, NONE);
            self.queue.push(0, (t, 0));
        }
        while let Some((here, spent)) = self.queue.pop() {
            if self.known(here).is_some_and(|best| spent > best) {
                continue;
            }
            // Only a target can be the end of a walk that is stopped there.
            if self.came[here as usize] != NONE && self.stopped(here, &stops) {
                continue;
            }
            for (before, diagonal) in grid.around(here) {
                let Some(step) = grid.step(before, here, diagonal) else {
                    continue;
                };
                let total = spent + step;
                if self.known(before).is_none_or(|best| total < best) {
                    self.set(before, total, here);
                    self.queue.push(total, (before, total));
                }
            }
        }
    }

    /// The cost from `square` to the targets of the last [`Searcher::distance_field`].
    pub fn distance(&self, square: Square) -> Option<Cost> {
        self.known(square)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};
    use rand_pcg::Pcg64Mcg;
    use std::cmp::Reverse;
    use std::collections::{BinaryHeap, HashSet};

    /// A random plane: costs 1 to 8 half moves, some water that nothing enters, roads on some
    /// squares, some of them enchanted.
    struct Plane {
        grid: PathGrid,
        costs: Costs,
        roads: Vec<Road>,
        stops: HashSet<Square>,
        diagonal_extra: bool,
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
            enter: vec![0; n],
            by_road: vec![0; n],
            by_enchanted_road: vec![0; n],
        };
        for i in 0..n {
            if rng.gen_range(0..100) < blocked {
                costs.enter[i] = BLOCKED;
                costs.by_road[i] = BLOCKED;
                costs.by_enchanted_road[i] = BLOCKED;
            } else {
                costs.enter[i] = rng.gen_range(1..=8) * if rng.gen_bool(0.5) { 2 } else { 1 };
                costs.by_road[i] = rng.gen_range(0..=3);
                costs.by_enchanted_road[i] = rng.gen_range(0..=2);
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
        let grid = PathGrid::new(width, height, costs.clone(), roads.clone(), diagonal_extra)
            .expect("a grid of the right size");
        Plane {
            grid,
            costs,
            roads,
            stops,
            diagonal_extra,
        }
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
            if self.costs.enter[to_i] == BLOCKED {
                return None;
            }
            let (road_from, road_to) = (self.roads[from as usize], self.roads[to_i]);
            let cost = u32::from(
                if road_from == Road::Enchanted && road_to == Road::Enchanted {
                    self.costs.by_enchanted_road[to_i]
                } else if road_from != Road::None && road_to != Road::None {
                    self.costs.by_road[to_i]
                } else {
                    self.costs.enter[to_i]
                },
            );
            Some(if self.diagonal_extra && dx == 1 && dy == 1 {
                (cost * 3).div_ceil(2)
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
            best[from as usize] = Some(0);
            open.push(Reverse((0, from)));
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
            let mut sum = 0;
            for &next in path {
                sum += self
                    .step(at, next)
                    .expect("the path takes only steps the rule allows");
                at = next;
            }
            sum
        }
    }

    const CASES: u64 = 60;

    fn planes() -> impl Iterator<Item = Plane> {
        (0..CASES).map(|seed| {
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
                    let found = searcher.path(&plane.grid, from, to, |s| plane.stops.contains(&s));
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
                    .filter(|&s| {
                        best[s as usize].is_some() && (s == from || !plane.stops.contains(&s))
                    })
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
                let expected = targets.iter().any(|&t| {
                    t == from || (best[t as usize].is_some() && !plane.stops.contains(&t))
                });
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
                    assert_eq!(field.distance(from), Some(0));
                    continue;
                }
                let found = forward
                    .path(&plane.grid, from, target, |s| plane.stops.contains(&s))
                    .map(|p| plane.cost_of(from, &p));
                assert_eq!(field.distance(from), found, "from {from} to {target}");
            }
        }
    }

    #[test]
    fn a_group_walks_at_the_pace_of_its_slowest_part_over_what_every_part_can_enter() {
        let mut searcher = Searcher::new();
        for seed in 0..20u64 {
            let mut rng = Pcg64Mcg::seed_from_u64(seed + 1000);
            let (width, height, diagonal_extra) =
                (9 + seed as u32 % 4, 6 + seed as u32 % 3, seed % 2 == 0);
            let n = (width * height) as usize;
            let roads = random_roads(&mut rng, n);
            let planes: Vec<Plane> = (0..3)
                .map(|_| {
                    let costs = random_costs(&mut rng, n);
                    Plane {
                        grid: PathGrid::new(
                            width,
                            height,
                            costs.clone(),
                            roads.clone(),
                            diagonal_extra,
                        )
                        .expect("a grid of the right size"),
                        costs,
                        roads: roads.clone(),
                        stops: HashSet::new(),
                        diagonal_extra,
                    }
                })
                .collect();
            let parts: Vec<&PathGrid> = planes.iter().map(|p| &p.grid).collect();
            let group = PathGrid::slowest(&parts).expect("parts of one plane");
            for from in 0..n as Square {
                for (to, _) in group.around(from) {
                    let each: Option<Vec<Cost>> = planes.iter().map(|p| p.step(from, to)).collect();
                    assert_eq!(
                        group.step_cost(from, to),
                        each.map(|c| c.into_iter().max().unwrap_or(0)),
                        "from {from} to {to}"
                    );
                }
            }
            let (from, to) = (1, n as Square - 2);
            let found = searcher.path(&group, from, to, |_| false);
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
            Costs::with_ordinary_roads(vec![2; 4]),
            vec![Road::None; 4],
            false,
        )
        .expect("a grid of the right size");
        let big = PathGrid::new(
            3,
            2,
            Costs::with_ordinary_roads(vec![2; 6]),
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
                Costs::with_ordinary_roads(vec![1; 5]),
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
            Costs::with_ordinary_roads(vec![2; 15]),
            vec![Road::None; 15],
            false,
        )
        .expect("a grid of the right size");
        let mut searcher = Searcher::new();
        let west_to_east = searcher.path(&grid, grid.square(0, 1), grid.square(4, 1), |_| false);
        assert_eq!(west_to_east, Some(vec![grid.square(4, 1)]));
        let north_to_south = searcher.path(&grid, grid.square(0, 0), grid.square(0, 2), |_| false);
        assert_eq!(north_to_south.map(|p| p.len()), Some(2));
    }

    /// A heap-queue A* with tables made for each search, like the game's search before it was
    /// kept between searches.
    fn heap_search(plane: &Plane, from: Square, to: Square) -> Option<Cost> {
        let grid = &plane.grid;
        let mut best: Vec<Option<Cost>> = vec![None; grid.squares()];
        let mut open = BinaryHeap::new();
        best[from as usize] = Some(0);
        open.push(Reverse((0, 0, from)));
        while let Some(Reverse((_, cost, here))) = open.pop() {
            if here == to {
                return Some(cost);
            }
            if best[here as usize] != Some(cost) {
                continue;
            }
            for (next, diagonal) in grid.around(here) {
                if next != to && plane.stops.contains(&next) {
                    continue;
                }
                let Some(step) = grid.step(here, next, diagonal) else {
                    continue;
                };
                if best[next as usize].is_none_or(|b| cost + step < b) {
                    best[next as usize] = Some(cost + step);
                    open.push(Reverse((
                        cost + step + grid.estimate(next, grid.xy(to)),
                        cost + step,
                        next,
                    )));
                }
            }
        }
        None
    }

    /// Timings against a plain Dijkstra that allocates its tables for each search (about the
    /// shape of what the game does now); run with `--release -- --ignored --nocapture`.
    #[test]
    #[ignore = "a timing, not a check"]
    fn timing_on_a_map_of_the_game_size() {
        use std::time::Instant;
        let plane = random_plane(7, 60, 40, true);
        let n = plane.grid.squares() as Square;
        let pairs: Vec<(Square, Square)> = (0..2000)
            .map(|i| ((i * 37) % n, (i * 101 + 17) % n))
            .collect();
        let mut searcher = Searcher::new();

        let start = Instant::now();
        let mut sum = 0u64;
        for &(a, b) in &pairs {
            let found = searcher.path(&plane.grid, a, b, |s| plane.stops.contains(&s));
            sum += found.map_or(0, |p| p.len() as u64);
        }
        println!("helper: {:?} ({sum})", start.elapsed());

        let start = Instant::now();
        let mut sum = 0u64;
        for &(a, b) in &pairs {
            sum += u64::from(heap_search(&plane, a, b).is_some());
        }
        println!("heap search: {:?} ({sum})", start.elapsed());
    }
}
