//! Why a stack has no way to a square (task 668): the facts a refused go-to order carries, so that
//! the log says "the target is water" or "cut off at (12, 7), a lair on (13, 7)" and not only
//! "impassable".
//!
//! Tests: at the bottom of this file, on the shipped rules.

use crate::game::Game;
use crate::map::Position;

/// What keeps a stack out of a square.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocker {
    /// Open sea, and nothing in the stack can cross it (no ship with room, no flight).
    Water,
    /// Dry land to a ship that has no city there.
    Land,
    /// Ground the stack cannot enter (mountains and the like for a walker).
    Terrain,
    /// An enemy unit or city stands there.
    Hostile,
    /// A guarded site: its guards have to be fought first.
    Lair,
    /// `present` units stand there and the stack would pass the `limit` of a square.
    Full { present: u32, limit: u32 },
}

impl Blocker {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Water => "refuse_map.blocker_water",
            Self::Land => "refuse_map.blocker_land",
            Self::Terrain => "refuse_map.blocker_terrain",
            Self::Hostile => "refuse_map.blocker_hostile",
            Self::Lair => "refuse_map.blocker_lair",
            Self::Full { .. } => "refuse_map.blocker_full",
        }
    }
}

/// Why `Command::GoTo` found no path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoPath {
    /// No unit, or one that is not on the map.
    NoUnits,
    /// The units already stand on the target square.
    AlreadyThere,
    /// The target is on the other plane (a tower is needed first).
    OtherPlane,
    /// The target square itself cannot be entered.
    Target(Blocker),
    /// A way exists only through squares that are full for this stack.
    OnlyThroughFullSquares,
    /// The target can be entered but the squares the stack walks to do not touch it: the search
    /// stopped at `stopped` (the square it reached nearest to the target, `region` squares in all);
    /// `beyond` is the square next to it, towards the target, that it could not enter.
    CutOff {
        stopped: Position,
        region: u32,
        beyond: Option<(Position, Blocker)>,
    },
}

impl NoPath {
    /// The name of the kind, without the squares: what the logs group by.
    pub fn name(self) -> &'static str {
        match self {
            Self::NoUnits => "NoUnits",
            Self::AlreadyThere => "AlreadyThere",
            Self::OtherPlane => "OtherPlane",
            Self::Target(_) => "Target",
            Self::OnlyThroughFullSquares => "OnlyThroughFullSquares",
            Self::CutOff { .. } => "CutOff",
        }
    }

    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::NoUnits => "refuse_map.no_path_no_units",
            Self::AlreadyThere => "refuse_map.no_path_already_there",
            Self::OtherPlane => "refuse_map.no_path_other_plane",
            Self::Target(_) => "refuse_map.no_path_target",
            Self::OnlyThroughFullSquares => "refuse_map.no_path_only_through_full_squares",
            Self::CutOff { .. } => "refuse_map.no_path_cut_off",
        }
    }
}

fn square(p: Position) -> String {
    format!("({}, {})", p.x, p.y)
}

/// A short line for the logs of the tools (not the player's text, which is `key`).
impl std::fmt::Display for Blocker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Water => write!(f, "water"),
            Self::Land => write!(f, "land for a ship"),
            Self::Terrain => write!(f, "terrain the units cannot enter"),
            Self::Hostile => write!(f, "enemy"),
            Self::Lair => write!(f, "guarded site"),
            Self::Full { present, limit } => write!(f, "full ({present} of {limit})"),
        }
    }
}

/// A short line for the logs of the tools.
impl std::fmt::Display for NoPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoUnits => write!(f, "no units on the map"),
            Self::AlreadyThere => write!(f, "already on the target square"),
            Self::OtherPlane => write!(f, "target on the other plane"),
            Self::Target(b) => write!(f, "target is {b}"),
            Self::OnlyThroughFullSquares => write!(f, "only through full squares"),
            Self::CutOff {
                stopped,
                region,
                beyond,
            } => {
                write!(f, "cut off at {} after {region} squares", square(*stopped))?;
                if let Some((at, b)) = beyond {
                    write!(f, ", {} is {b}", square(*at))?;
                }
                Ok(())
            }
        }
    }
}

impl Game {
    /// What keeps the stack out of `at`, or `None` when it can step in.
    pub fn blocker_at(&self, units: &[hecs::Entity], at: Position) -> Option<Blocker> {
        let economy = self.economy();
        let owner = units.first().and_then(|&u| self.units().unit_owner(u))?;
        let traits = economy.stack_traits(units);
        for profile in economy.step_profiles(units)? {
            if economy.step_entry(&profile, at, traits).is_err() {
                return Some(if economy.is_sea(at) {
                    Blocker::Water
                } else if economy.terrain_class_at(at).is_some() && !traits.flies && traits.sea_lift
                {
                    Blocker::Land
                } else {
                    Blocker::Terrain
                });
            }
        }
        if economy.hostile_at(at, owner).is_some() {
            return Some(Blocker::Hostile);
        }
        if owner != crate::player::MONSTERS
            && economy.lair_at(at).is_some_and(|l| !l.guards.is_empty())
        {
            return Some(Blocker::Lair);
        }
        let present = economy.army_at(at).len();
        (present + units.len() > economy.max_tile).then_some(Blocker::Full {
            present: present as u32,
            limit: economy.max_tile as u32,
        })
    }

    /// Why [`Game::find_path`] found no way for the stack to `to`.
    pub fn why_no_path(&self, units: &[hecs::Entity], to: Position) -> NoPath {
        let Some(start) = units.first().and_then(|&u| self.units().unit_position(u)) else {
            return NoPath::NoUnits;
        };
        if start == to {
            return NoPath::AlreadyThere;
        }
        if start.plane != to.plane {
            return NoPath::OtherPlane;
        }
        // A full target is entered by the last step only when there is room; a path to it
        // does not exist otherwise.
        if let Some(blocker) = self.blocker_at(units, to) {
            if !matches!(blocker, Blocker::Hostile | Blocker::Lair) {
                return NoPath::Target(blocker);
            }
        }
        if self.grid_path(units, to, false).is_some() {
            return NoPath::OnlyThroughFullSquares;
        }
        let Some(map) = self.map.as_ref() else {
            return NoPath::NoUnits;
        };
        let region = self.reachable_squares(units);
        let apart = |p: &Position| map.plane(to.plane).chebyshev((p.x, p.y), (to.x, to.y));
        let stopped = region
            .iter()
            .copied()
            .min_by_key(|p| (apart(p), p.y, p.x))
            .unwrap_or(start);
        let beyond = crate::unit::pathing::neighbours(self, stopped)
            .into_iter()
            .filter(|p| !region.contains(p))
            .min_by_key(|p| (apart(p), p.y, p.x))
            .and_then(|p| self.blocker_at(units, p).map(|b| (p, b)));
        NoPath::CutOff {
            stopped,
            region: region.len() as u32,
            beyond,
        }
    }
}
