//! The raid from the sea (setting `sea_raids`, `rules.ron` `threat.sea_raid`): in place of a
//! raid out of a city, a party of a random race lands on a far coast. A ship (a trireme, a galley
//! later), one to four fighters that grow in number and rank with the turn (the cheapest fighter
//! of the race first, the dearest at the end; experience as the other raiders'), and a settler.
//! Four coast squares are rolled and the one furthest from any town is the landing. All of it is
//! free of upkeep and belongs to the neutral player of the race (made when the race has none);
//! what it does then is the neutral AI's (`f4x_ai::NeutralSeat`).
//!
//! Tests: in threat/tests.rs (`a_sea_raid_...`).

use crate::game::{Event, Game};
use crate::map::terrain::TerrainClass;
use crate::map::{Plane, Position};
use crate::rules::{Ability, RaceId, Stat, UnitId, UnitKind};
use crate::unit::{Experience, Unpaid};

/// The eight neighbours of a square.
const AROUND: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// A coast square to land on, with the sea square the ship lies on beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Landing {
    pub land: Position,
    pub sea: Position,
}

impl Game {
    /// Whether this raid is a sea raid: the setting is on and the chance comes up. No roll is
    /// made with the setting off, so a game without it plays as it did.
    pub(super) fn sea_raid_comes(&mut self) -> bool {
        self.settings.world.sea_raids
            && self.roll(100) <= self.content.rules.config.threat.sea_raid.chance_percent
    }

    /// Raise the party. Nothing happens (and false is returned) when no race has a weight on the
    /// plane or no coast is free; the caller then raids as usual.
    pub(super) fn make_sea_raid(&mut self) -> Option<Vec<Event>> {
        let plane = if self.roll(2) == 1 {
            Plane::Arcanus
        } else {
            Plane::Myrror
        };
        let race = self.sea_raid_race(plane)?;
        let landing = self.sea_raid_landing(plane)?;
        let (fighters, settler) = self.sea_raid_troops(race)?;
        let ship = self.sea_raid_ship()?;
        let owner = self.race_player(race);
        let ship = self.spawn_unit(owner, landing.sea, ship);
        self.put(ship, Unpaid);
        let xp_level = match self.turn {
            251.. => 3,
            121.. => 2,
            41.. => 1,
            _ => 0,
        };
        let mut count = 0;
        for kind in fighters.into_iter().chain(std::iter::once(settler)) {
            let unit = self.spawn_unit(owner, landing.land, kind);
            self.put(unit, Unpaid);
            if kind != settler {
                if let Some(xp) = self.level_xp(unit, xp_level) {
                    self.put(unit, Experience(xp));
                }
                count += 1;
            }
        }
        Some(vec![Event::SeaRaidLanded {
            owner,
            at: landing.land,
            count,
        }])
    }

    /// A race rolled for the plane, by the weights the races have for a neutral city there.
    fn sea_raid_race(&mut self, plane: Plane) -> Option<RaceId> {
        let rules = &self.content.rules;
        let weights: Vec<(RaceId, u32)> = rules
            .races
            .defs()
            .iter()
            .filter_map(|r| {
                let w = match plane {
                    Plane::Arcanus => r.neutral_weight.arcanus,
                    Plane::Myrror => r.neutral_weight.myrror,
                };
                Some((rules.races.id_numbered(r.num)?, w)).filter(|&(_, w)| w > 0)
            })
            .collect();
        let total: u32 = weights.iter().map(|&(_, w)| w).sum();
        if total == 0 {
            return None;
        }
        let mut left = self.roll(total);
        for (race, w) in weights {
            if left <= w {
                return Some(race);
            }
            left -= w;
        }
        None
    }

    /// The ship for the turn: the last of the list that has begun.
    fn sea_raid_ship(&self) -> Option<UnitId> {
        let rules = &self.content.rules;
        rules
            .config
            .threat
            .sea_raid
            .ships
            .iter()
            .rfind(|&&(from, _)| self.turn >= from)
            .and_then(|&(_, num)| rules.units.id_numbered(num))
    }

    /// The fighters of the party and its settler. The fighters of the race run from the cheapest
    /// to the dearest, and how far up the turn takes the party sets their rank and their number.
    fn sea_raid_troops(&self, race: RaceId) -> Option<(Vec<UnitId>, UnitId)> {
        let rules = &self.content.rules;
        let cfg = &rules.config.threat.sea_raid;
        let mut fighters: Vec<(u32, UnitId)> = Vec::new();
        let mut settler = None;
        for &u in &rules.races.get(race).units {
            let def = rules.units.get(u);
            if def.kind != UnitKind::Normal {
                continue;
            }
            if def.abilities.contains(&Ability::new(12)) {
                settler = settler.or(Some(u));
            } else if def.stat(Stat::Transport) == 0.0
                && def.stat(Stat::Melee).max(def.stat(Stat::Ranged)) > 0.0
            {
                fighters.push((def.cost, u));
            }
        }
        fighters.sort_by_key(|&(cost, _)| cost);
        let settler = settler?;
        if fighters.is_empty() {
            return None;
        }
        let progress_of = |n: u32| {
            u64::from(self.turn.min(cfg.full_strength_turn)) * u64::from(n)
                / u64::from(cfg.full_strength_turn.max(1))
        };
        let rank = progress_of(fighters.len() as u32 - 1) as usize;
        let count = 1 + progress_of(cfg.most_fighters.saturating_sub(1)) as usize;
        Some((vec![fighters[rank].1; count], settler))
    }

    /// Roll coast squares of the plane and take the one furthest from every town.
    pub(super) fn sea_raid_landing(&mut self, plane: Plane) -> Option<Landing> {
        let coast = self.free_coast(plane);
        if coast.is_empty() {
            return None;
        }
        let towns: Vec<Position> = self
            .economy()
            .cities()
            .into_iter()
            .map(|(.., p)| p)
            .filter(|p| p.plane == plane)
            .collect();
        let rolls = self.content.rules.config.threat.sea_raid.coast_rolls.max(1);
        let picks: Vec<Landing> = (0..rolls)
            .map(|_| coast[self.roll(coast.len() as u32) as usize - 1])
            .collect();
        let grid = self.map.as_ref()?.plane(plane);
        let far = |l: &Landing| {
            towns
                .iter()
                .map(|t| grid.chebyshev((t.x, t.y), (l.land.x, l.land.y)))
                .min()
                .unwrap_or(u32::MAX)
        };
        let mut best: Option<(u32, Landing)> = None;
        for pick in picks {
            let d = far(&pick);
            if best.is_none_or(|(b, _)| d > b) {
                best = Some((d, pick));
            }
        }
        best.map(|(_, l)| l)
    }

    /// Every land square of the plane with open sea beside it (not a lake) that holds no unit,
    /// feature or city, each with its sea square, in the order of the map.
    pub(super) fn free_coast(&self, plane: Plane) -> Vec<Landing> {
        let Some(map) = self.map.as_ref() else {
            return Vec::new();
        };
        let grid = map.plane(plane);
        let sites = self.economy().site_squares();
        let class = |x: i32, y: i32| {
            grid.get(x, y)
                .and_then(|t| self.content.terrains.get(t))
                .map(|t| t.class)
        };
        let mut out = Vec::new();
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                let land = Position { plane, x, y };
                if class(x as i32, y as i32).is_none_or(|c| c == TerrainClass::Ocean)
                    || sites.contains(&land)
                    || !self.units().units_at(land).is_empty()
                    || self.economy().city_at(land).is_some()
                {
                    continue;
                }
                let sea = AROUND.iter().find_map(|&(dx, dy)| {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    (class(nx, ny) == Some(TerrainClass::Ocean) && !grid.is_lake(nx, ny)).then(
                        || Position {
                            plane,
                            x: grid.wrap_x(nx),
                            y: ny as u32,
                        },
                    )
                });
                if let Some(sea) = sea {
                    out.push(Landing { land, sea });
                }
            }
        }
        out
    }
}
