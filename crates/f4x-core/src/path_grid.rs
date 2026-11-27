//! Shortest ways over a grid of numbers: a cost to enter each square, roads, squares nothing can
//! enter, and a plane that wraps east and west. It knows nothing of units or rules; a caller
//! turns what a walker can do into a [`PathGrid`] (one grid for each kind of mover, and
//! [`PathGrid::slowest`] for a group of them) and what stops it into a closure.
//!
//! Tests: random grids against a plain Dijkstra, `tests/it/c9_path.rs`.

use f4x_def::half_int::HalfInt;

/// A square of the grid: `y * width + x`.
pub type Square = u32;

/// A cost in moves.
pub type Cost = HalfInt;

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

/// What it costs a walker, in moves, to enter each square (row by row, `None` where it
/// cannot), and to enter it along a road: from a square with a road, or two with enchanted roads.
/// A road bears on a step only when both squares have one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Costs {
    pub enter: Vec<Option<Cost>>,
    pub by_road: Vec<Option<Cost>>,
    pub by_enchanted_road: Vec<Option<Cost>>,
}

impl Costs {
    /// The costs of a walker for whom a road step costs half a move and an enchanted one nothing.
    pub fn with_ordinary_roads(enter: Vec<Option<Cost>>) -> Self {
        let by = |cost: Cost| enter.iter().map(|e| e.map(|_| cost)).collect();
        Self {
            by_road: by(Cost::HALF),
            by_enchanted_road: by(Cost::ZERO),
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
    /// A river runs through the square (empty: the rivers do not help a step, `with_rivers`).
    rivers: Vec<bool>,
    diagonal_extra: bool,
    /// The connected piece of the squares a walker can enter each square lies in (not counting
    /// what stops a walk); `NONE` for a blocked square. Labelled at the first question, as a grid
    /// that only walks never asks.
    region: std::cell::OnceCell<Vec<u32>>,
    /// The least and the most one step costs, for the heuristic and the size of the queue.
    min_step: Cost,
    max_step: Cost,
}

impl PartialEq for PathGrid {
    /// Equal when the squares cost the same; the labels follow from the costs.
    fn eq(&self, other: &Self) -> bool {
        (
            self.width,
            self.height,
            self.diagonal_extra,
            &self.costs,
            &self.roads,
            &self.rivers,
        ) == (
            other.width,
            other.height,
            other.diagonal_extra,
            &other.costs,
            &other.roads,
            &other.rivers,
        )
    }
}

impl Eq for PathGrid {}

impl PathGrid {
    /// `costs` and `roads` have one entry per square, row by row. With `diagonal_extra` a
    /// diagonal step costs one and a half times as much, rounded up to a half.
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
            rivers: Vec::new(),
            diagonal_extra,
            region: std::cell::OnceCell::new(),
            min_step: Cost::ZERO,
            max_step: Cost::ZERO,
        };
        grid.measure();
        Ok(grid)
    }

    /// The same grid where a step between two river squares costs what a road step does, never
    /// more than entering the square (`rivers` has one entry per square; the wrong size is
    /// ignored).
    pub fn with_rivers(mut self, rivers: Vec<bool>) -> Self {
        if rivers.len() == self.squares() {
            self.rivers = rivers;
            self.measure();
        }
        self
    }

    fn river(&self, square: Square) -> bool {
        self.rivers.get(square as usize).copied().unwrap_or(false)
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
        self.costs.enter[square as usize].is_none()
    }

    /// The cost of the step from `from` to the square next to it, `None` when it cannot be
    /// taken.
    pub fn step_cost(&self, from: Square, to: Square) -> Option<Cost> {
        let ((fx, fy), (tx, ty)) = (self.xy(from), self.xy(to));
        self.step(from, to, fx != tx && fy != ty)
    }

    fn step(&self, from: Square, to: Square, diagonal: bool) -> Option<Cost> {
        let i = to as usize;
        self.costs.enter[i]?;
        let cost = match (self.roads[from as usize], self.roads[i]) {
            (Road::None, _) | (_, Road::None) if self.river(from) && self.river(to) => {
                self.costs.enter[i]
                    .zip(self.costs.by_road[i])
                    .map(|(a, b)| a.min(b))
            }
            (Road::None, _) | (_, Road::None) => self.costs.enter[i],
            (Road::Enchanted, Road::Enchanted) => self.costs.by_enchanted_road[i],
            _ => self.costs.by_road[i],
        }?;
        Some(if diagonal && self.diagonal_extra {
            cost + cost.half()
        } else {
            cost
        })
    }

    /// The region a square lies in: squares in different regions cannot be walked between, with
    /// nothing in the way. `None` for a square nothing can enter.
    pub fn region(&self, square: Square) -> Option<u32> {
        let r = self.regions()[square as usize];
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
        let (mut min, mut max) = (Cost::MAX, Cost::ZERO);
        let mut note = |cost: Option<Cost>| {
            if let Some(cost) = cost {
                min = min.min(cost);
                max = max.max(cost);
            }
        };
        for (i, &enter) in self.costs.enter.iter().enumerate() {
            if enter.is_none() {
                continue;
            }
            note(enter);
            if self.river(i as Square) {
                note(self.costs.by_road[i]);
            }
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
            min = Cost::ZERO;
        }
        self.min_step = min;
        self.max_step = if self.diagonal_extra {
            max + max.half()
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
                || part.rivers != first.rivers
            {
                return Err(PathGridError::Mismatch);
            }
            for (mine, theirs) in [
                (&mut costs.enter, &part.costs.enter),
                (&mut costs.by_road, &part.costs.by_road),
                (&mut costs.by_enchanted_road, &part.costs.by_enchanted_road),
            ] {
                for (m, &t) in mine.iter_mut().zip(theirs) {
                    // A square either part cannot enter is shut; otherwise the dearer cost.
                    *m = m.zip(t).map(|(a, b)| a.max(b));
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
        .map(|grid| {
            if first.rivers.is_empty() {
                grid
            } else {
                grid.with_rivers(first.rivers.clone())
            }
        })
    }

    fn regions(&self) -> &[u32] {
        self.region.get_or_init(|| self.label_regions())
    }

    fn label_regions(&self) -> Vec<u32> {
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
        region
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
        self.min_step * dx.min(self.width - dx).max(y.abs_diff(to.1)) as i32
    }
}

/// The queue of a search whose costs are few: one list for each cost (a bucket for each half
/// move), in a ring as long as the greatest jump from one cost to the next, so a push and a pop
/// take no searching. The bucket numbers are the only place a cost is counted in halves.
#[derive(Default)]
struct Buckets {
    ring: Vec<Vec<(Square, Cost)>>,
    key: u32,
    len: usize,
}

impl Buckets {
    fn bucket(cost: Cost) -> u32 {
        cost.raw().max(0) as u32
    }

    /// Empty, with room for jumps of up to `jump`.
    fn reset(&mut self, jump: Cost) {
        let size = Self::bucket(jump) as usize + 1;
        if self.ring.len() == size {
            self.ring.iter_mut().for_each(Vec::clear);
        } else {
            self.ring = vec![Vec::new(); size];
        }
        self.key = 0;
        self.len = 0;
    }

    fn push(&mut self, key: Cost, entry: (Square, Cost)) {
        let key = Self::bucket(key).max(self.key);
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
            self.cost = vec![Cost::ZERO; squares];
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
        self.queue.reset(grid.max_step + grid.min_step);
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
    /// `from == to`. A square that `stops` holds for cannot be walked through, only ended on;
    /// one that `closed` holds for cannot be entered at all.
    pub fn path(
        &mut self,
        grid: &PathGrid,
        from: Square,
        to: Square,
        stops: impl Fn(Square) -> bool,
        closed: impl Fn(Square) -> bool,
    ) -> Option<Vec<Square>> {
        if from == to || !grid.maybe_reachable(from, to) {
            return None;
        }
        let end = self.run(grid, from, &Goal::Square(to), &stops, &closed)?;
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
        self.run(grid, from, &Goal::Any(targets), &stops, &|_| false)
            .is_some()
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
        self.run(grid, from, &goal, &stops, &|_| false).is_some()
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
        closed: &impl Fn(Square) -> bool,
    ) -> Option<Square> {
        self.begin(grid);
        let aim = match goal {
            Goal::Square(to) => Some(grid.xy(*to)),
            _ => None,
        };
        let guess = |square: Square| aim.map_or(Cost::ZERO, |to| grid.estimate(square, to));
        self.set(from, Cost::ZERO, NONE);
        self.queue.push(guess(from), (from, Cost::ZERO));
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
                if closed(next) {
                    continue;
                }
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
            self.set(t, Cost::ZERO, NONE);
            self.queue.push(Cost::ZERO, (t, Cost::ZERO));
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
