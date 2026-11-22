//! What a player can see right now (as opposed to merely having explored), worked out by the
//! `Economy` handle (world, map, content, players) without a `Game`.

use crate::city::economy::Economy;
use crate::fast::FastMap;
use crate::map::{Plane, Position};
use crate::player::PlayerId;
use crate::rules::Ability;
use f4x_def::half_int::HalfInt;

use crate::rules::stats::compute_half;
use crate::rules::Stat;

/// Sight of a city (squares) before walls, an Oracle or Nature's Eye raise it.
const CITY_SIGHT: HalfInt = HalfInt::from_raw(5);

impl Economy<'_> {
    /// Can `viewer` make out `unit`? One's own units always, others unless invisible; an
    /// invisible unit shows to a viewer who has a unit immune to illusions in sight of it.
    pub fn unit_visible_to(&self, unit: hecs::Entity, viewer: PlayerId) -> bool {
        let units = self.units();
        if units.unit_owner(unit) == Some(viewer)
            || !units.unit_has_ability(unit, Ability::new(33 /* invisibility */))
        {
            return true;
        }
        let Some(at) = units.unit_position(unit) else {
            return true;
        };
        let Some(map) = self.map else {
            return true;
        };
        let grid = map.plane(at.plane);
        units.player_units(viewer).into_iter().any(|u| {
            units.unit_has_ability(u, Ability::new(31 /* illusion_immunity */))
                && units.unit_position(u).is_some_and(|p| {
                    p.plane == at.plane
                        && HalfInt::from_int(grid.chebyshev((p.x, p.y), (at.x, at.y)) as i32)
                            <= units.unit_sight(u)
                })
        })
    }

    /// How far a city sees, in squares: 2.5, or more with an Oracle (4.5), City Walls (3.5) or
    /// Nature's Eye (4.5).
    pub fn city_sight(&self, city: hecs::Entity) -> HalfInt {
        compute_half(Stat::CitySight, CITY_SIGHT, &self.city_modifiers(city))
    }

    /// Visible tiles on one plane as a row-major grid of `width * height` flags: the
    /// squares around each of the player's units and cities.
    pub fn visibility(&self, player: PlayerId, plane: Plane) -> Vec<bool> {
        self.vision_grid(player, Some(plane)).flags(plane)
    }

    /// What `player` sees on `only` plane (both when `None`), with the sight of each of its units
    /// kept so that [`VisionGrid::restamp`] can move one without a new count.
    pub fn vision_grid(&self, player: PlayerId, only: Option<Plane>) -> VisionGrid {
        let Some(map) = self.map else {
            return VisionGrid::default();
        };
        let (w, h) = (map.width() as i32, map.height() as i32);
        let mut grid = VisionGrid {
            w,
            h,
            diagonal_extra: self.diagonal_extra,
            all: false,
            counts: [Vec::new(), Vec::new()],
            sights: FastMap::default(),
        };
        if self.units().has_global_effect(
            player,
            crate::spell::global_enchantment::GlobalEffect::NatureAwareness,
        ) {
            grid.all = true;
            return grid;
        }
        for plane in Plane::all() {
            if only.is_none_or(|p| p == plane) {
                grid.counts[plane.index()] = vec![0; (w * h) as usize];
            }
        }
        let units = self.units();
        for unit in units.player_units(player) {
            // The sight is the dear part (it adds up the unit's items and spells): ask only for
            // the units on a plane that is wanted.
            if let Some(at) = units
                .unit_position(unit)
                .filter(|at| only.is_none_or(|p| p == at.plane))
            {
                let sight = units.unit_sight(unit);
                grid.stamp(at, sight, true);
                grid.sights.insert(unit, (at, sight));
            }
        }
        for (entity, city, at) in self.cities() {
            if city.owner == player && only.is_none_or(|p| p == at.plane) {
                grid.stamp(at, self.city_sight(entity), true);
            }
        }
        grid
    }
}

/// How many of a player's units and cities see each square, per plane. A square is in sight when
/// the count is above 0. Keeping counts and not flags is what lets one unit's sight be taken off
/// where it stood and put on where it stands, leaving the rest alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VisionGrid {
    w: i32,
    h: i32,
    diagonal_extra: bool,
    /// Nature Awareness: everything is in sight and nothing is counted.
    all: bool,
    counts: [Vec<u16>; 2],
    /// Each counted unit: where it was put on and with what sight.
    sights: FastMap<hecs::Entity, (Position, HalfInt)>,
}

impl VisionGrid {
    /// Put the sight of one source on the counts (`add`), or take it off.
    fn stamp(&mut self, at: Position, sight: HalfInt, add: bool) {
        let counts = &mut self.counts[at.plane.index()];
        if counts.is_empty() {
            return;
        }
        let (w, h) = (self.w, self.h);
        let radius = crate::map::fog::squares_seen(sight) as i32;
        for dy in -radius..=radius {
            let y = at.y as i32 + dy;
            if !(0..h).contains(&y) {
                continue;
            }
            for dx in (-radius..=radius)
                .filter(|&dx| crate::map::fog::in_sight(dx, dy, sight, self.diagonal_extra))
            {
                let x = (at.x as i32 + dx).rem_euclid(w);
                let n = &mut counts[(y * w + x) as usize];
                *n = if add { *n + 1 } else { n.saturating_sub(1) };
            }
        }
    }

    /// Is the square in sight?
    pub fn sees(&self, at: Position) -> bool {
        if self.all {
            return true;
        }
        self.counts[at.plane.index()]
            .get((at.y as i32 * self.w + at.x as i32) as usize)
            .is_some_and(|&n| n > 0)
    }

    /// The squares in sight on a plane, row by row.
    pub fn flags(&self, plane: Plane) -> Vec<bool> {
        if self.all {
            return vec![true; (self.w * self.h) as usize];
        }
        self.counts[plane.index()].iter().map(|&n| n > 0).collect()
    }

    /// Where a unit was put on the counts, if it was.
    pub(crate) fn tracked_at(&self, unit: hecs::Entity) -> Option<Position> {
        self.sights.get(&unit).map(|&(p, _)| p)
    }

    /// The units that were put on the counts.
    pub(crate) fn tracked(&self) -> impl Iterator<Item = (hecs::Entity, Position)> + '_ {
        self.sights.iter().map(|(&u, &(p, _))| (u, p))
    }

    /// Take a unit's sight off where it was put on; `false` when it was not on.
    pub(crate) fn lift(&mut self, unit: hecs::Entity) -> bool {
        let Some((at, sight)) = self.sights.remove(&unit) else {
            return false;
        };
        if !self.all {
            self.stamp(at, sight, false);
        }
        true
    }

    /// Put a unit's sight on at `at`.
    pub(crate) fn put(&mut self, unit: hecs::Entity, at: Position, sight: HalfInt) {
        if !self.all {
            self.stamp(at, sight, true);
        }
        self.sights.insert(unit, (at, sight));
    }
}

/// What a command does to what the player sees, as far as the cache of the player's vision is
/// concerned (see [`VisionCache`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VisionEffect {
    /// Nothing: no unit or city comes, goes, moves or changes its sight.
    None,
    /// Units walk: take each unit's sight off where it stood and put it on where it stands.
    Walk,
    /// Anything else: count again at the next look.
    Recount,
}

impl VisionEffect {
    /// The effect a command can have, by its kind alone. Only commands that cannot touch a unit,
    /// a city or a spell on either are `None`; the walking commands are `Walk`; every other
    /// command, and any command not named here, is `Recount`.
    pub(crate) fn of(command: &crate::game::Command) -> Self {
        use crate::game::Command as C;
        match command {
            C::ClearOrders { .. }
            | C::SetStandby { .. }
            | C::SetProduction { .. }
            | C::BuyProduction { .. }
            | C::SetFarmers { .. }
            | C::SetAutoBuild { .. }
            | C::RenameCity { .. }
            | C::SetTaxRate { .. }
            | C::SetGrandVizier { .. }
            | C::SetPowerSplit { .. }
            | C::SetResearch { .. }
            | C::Alchemy { .. }
            | C::RejectOffer { .. }
            | C::CancelCast { .. }
            | C::Diplomacy(_) => Self::None,
            C::MoveStack { .. } | C::GoTo { .. } => Self::Walk,
            _ => Self::Recount,
        }
    }

    /// The effect once the events of the command are known: a command that raised an event
    /// outside the few that cannot change the sight is a `Recount` whatever its kind says.
    pub(crate) fn with_events(self, events: &[crate::game::Event]) -> Self {
        use crate::game::Event as E;
        let harmless = |e: &E| match self {
            Self::None => matches!(e, E::Rejected(_) | E::Diplomacy(_)),
            Self::Walk => matches!(
                e,
                E::Rejected(_)
                    | E::UnitsMoved { .. }
                    | E::AttackRequested { .. }
                    | E::SiteEncounter { .. }
            ),
            Self::Recount => false,
        };
        if events.iter().all(harmless) {
            self
        } else {
            Self::Recount
        }
    }
}

/// The vision of one player, kept while a computer wizard plays its turn. Its views are made over
/// and over, each wanting what the player sees; counting it afresh for each is a tenth of a whole
/// game. Between two of the wizard's commands nothing else changes the game, so the count is kept,
/// patched after a command that only moved units ([`VisionEffect::Walk`]), and dropped after any
/// other that is not harmless ([`VisionEffect`]). Off unless [`Game::keep_for_turn`] is called.
#[derive(Default)]
pub(crate) struct VisionCache {
    player: Option<PlayerId>,
    grid: Option<VisionGrid>,
}

impl crate::game::Game {
    /// Keep what `player` sees, and what its units' stats are made of (`unit::kept`), from now
    /// until [`Game::drop_kept`]: for the length of a computer wizard's turn, in which every
    /// change of the game is a command it sends.
    pub fn keep_for_turn(&self, player: PlayerId) {
        crate::unit::kept::arm(&self.world, player);
        *self.vision.borrow_mut() = VisionCache {
            player: Some(player),
            grid: None,
        };
    }

    /// Stop keeping.
    pub fn drop_kept(&self) {
        crate::unit::kept::disarm();
        *self.vision.borrow_mut() = VisionCache::default();
    }

    /// Whether `player` sees the square, when its vision is kept; `None` when it is not.
    pub(crate) fn cached_sees(&self, player: PlayerId, at: Position) -> Option<bool> {
        let mut cache = self.vision.borrow_mut();
        if cache.player != Some(player) {
            return None;
        }
        let grid = cache
            .grid
            .get_or_insert_with(|| self.economy().vision_grid(player, None));
        Some(grid.sees(at))
    }

    /// Bring the kept vision up to date after a command.
    pub(crate) fn vision_after(&mut self, effect: VisionEffect, events: &[crate::game::Event]) {
        let effect = effect.with_events(events);
        // A walk or a harmless command leaves what a unit's stats are made of as it was.
        if effect == VisionEffect::Recount {
            crate::unit::kept::clear();
        }
        let (Some(player), Some(mut grid)) = ({
            let mut cache = self.vision.borrow_mut();
            (cache.player, cache.grid.take())
        }) else {
            return;
        };
        let keep = match effect {
            VisionEffect::None => true,
            VisionEffect::Recount => false,
            VisionEffect::Walk => {
                // Every unit of the player now, where it stands (no order is needed).
                let now: FastMap<hecs::Entity, Option<Position>> = match crate::player::find_entity(
                    &self.world,
                    player,
                ) {
                    Some(me) => self
                        .world
                        .query::<(&crate::unit::Unit, &crate::player::Owner, Option<&Position>)>()
                        .iter()
                        .filter(|(_, (_, o, _))| o.0 == me)
                        .map(|(e, (_, _, p))| (e, p.copied()))
                        .collect(),
                    None => FastMap::default(),
                };
                // A unit that was counted and is no longer one of the player's: the command did
                // more than walk, so count again.
                let intact = grid.tracked().all(|(u, _)| now.contains_key(&u));
                if intact {
                    let moved: Vec<(hecs::Entity, Option<Position>)> = now
                        .iter()
                        .filter(|&(&u, &at)| at != grid.tracked_at(u))
                        .map(|(&u, &at)| (u, at))
                        .collect();
                    let units = self.units();
                    for (u, at) in moved {
                        grid.lift(u);
                        if let Some(at) = at {
                            grid.put(u, at, units.unit_sight(u));
                        }
                    }
                }
                intact
            }
        };
        if !keep {
            return;
        }
        if check_vision() {
            let fresh = self.economy().vision_grid(player, None);
            for plane in Plane::all() {
                assert_eq!(
                    grid.flags(plane),
                    fresh.flags(plane),
                    "kept vision is out of date after {effect:?}: {events:?}"
                );
            }
        }
        self.vision.borrow_mut().grid = Some(grid);
    }
}

/// Set `F4X_CHECK_VISION` to count the vision again after every command and compare it with the
/// kept one (a test of the cache, slow).
fn check_vision() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("F4X_CHECK_VISION").is_some())
}
