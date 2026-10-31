//! Raiders (`Make_Raiders`, `AIDATA.c`): a neutral city on a landmass with a player's city gets
//! a raiding party of its own kind of troops. Unlike the original, which takes the troops out
//! of the garrison, the troops are new and the city pays no upkeep for them, and the city goes
//! to war: its AI (`f4x_ai::play_neutrals`) sets out to conquer the lairs and cities near it.
//!
//! Tests: in threat/tests.rs (the raider tests and `the_party_is_...`, `a_party_on_myrror_...`, `the_raiders_are_as_experienced_...`).

use crate::game::{Event, Game};
use crate::map::Position;
use crate::player::{Aggressive, PlayerId, MONSTERS};
use crate::rules::{Stat, UnitId, UnitKind};
use crate::unit::{Experience, Unit, Unpaid};

use super::RAIDER_LIMIT;

/// A neutral city that can raise a party: where it stands, who owns it and the kinds of troops
/// it has (one entry for each unit, so a common kind is drawn more often).
struct Source {
    city: hecs::Entity,
    owner: PlayerId,
    pos: Position,
    troops: Vec<UnitId>,
    room: usize,
    ai_fortress: bool,
}

impl Game {
    /// Run the raider clock and, when it runs out, raise a party in a random neutral city.
    pub(super) fn make_raiders(&mut self) -> Vec<Event> {
        let d = u32::from(self.difficulty());
        if !self
            .economy()
            .cities()
            .iter()
            .any(|(_, c, _)| self.is_raider_owner(c.owner))
        {
            return Vec::new();
        }
        self.threat.raider_clock += self.roll(d + 1);
        if self.threat.raider_clock < RAIDER_LIMIT {
            return Vec::new();
        }
        self.threat.raider_clock = 0;
        let sources = self.raider_sources();
        if sources.is_empty() {
            // The original lets a failed raid feed the monsters' clock instead.
            self.threat.monster_clock += 15;
            return Vec::new();
        }
        let source = &sources[self.roll(sources.len() as u32) as usize - 1];
        let (city, owner, pos) = (source.city, source.owner, source.pos);
        let mut count =
            source.troops.len() as u32 * u32::from(self.settings.world.raider_percent) / 100;
        if source.ai_fortress {
            count = count * 2 / 3;
        }
        if pos.plane == crate::map::Plane::Myrror && self.turn < 200 {
            count /= 2;
        }
        let count = (count.max(1) as usize).min(source.room);
        let troops = source.troops.clone();
        let xp_level = match self.turn {
            251.. => 3,
            121.. => 2,
            41.. => 1,
            _ => 0,
        };
        let mut made = 0;
        for _ in 0..count {
            let kind = troops[self.roll(troops.len() as u32) as usize - 1];
            let unit = self.spawn_unit(owner, pos, kind);
            let _ = self.world.insert_one(unit, Unpaid);
            if let Some(xp) = self.level_xp(unit, xp_level) {
                let _ = self.world.insert_one(unit, Experience(xp));
            }
            made += 1;
        }
        if let Some(e) = self.player_entity(owner) {
            let _ = self.world.insert_one(e, Aggressive);
        }
        vec![Event::RaidersRaised {
            city,
            owner,
            count: made,
        }]
    }

    /// Whether the player is an independent city's.
    fn is_raider_owner(&self, owner: PlayerId) -> bool {
        owner != MONSTERS && self.is_neutral_player(owner)
    }

    /// The experience that makes the unit reach `level` (0-based; the highest it has when it
    /// has fewer), if it has levels.
    fn level_xp(&self, unit: hecs::Entity, level: usize) -> Option<u32> {
        let path = self.units().unit_level_path(unit)?;
        path.levels
            .get(level.min(path.levels.len().checked_sub(1)?))
            .map(|l| l.xp)
    }

    /// The neutral cities that may raise a party: a player's city stands on their landmass, they
    /// have troops of the kinds that raid and room for more in the city.
    fn raider_sources(&self) -> Vec<Source> {
        let lands = self.all_landmasses();
        let mut cities: Vec<(hecs::Entity, PlayerId, Position)> = self
            .economy()
            .cities()
            .into_iter()
            .map(|(e, c, p)| (e, c.owner, p))
            .collect();
        cities.sort_by_key(|&(e, ..)| (self.serial_of(e), e.id()));
        let human = self.human();
        let mut out = Vec::new();
        for &(city, owner, pos) in &cities {
            if !self.is_raider_owner(owner) {
                continue;
            }
            let land = self.landmass_at(&lands, pos);
            let on_land = |p: Position| land != 0 && self.landmass_at(&lands, p) == land;
            if !cities
                .iter()
                .any(|&(_, o, p)| !self.is_neutral_player(o) && on_land(p))
            {
                continue;
            }
            let ai_fortress = cities.iter().any(|&(e, o, p)| {
                !self.is_neutral_player(o)
                    && Some(o) != human
                    && on_land(p)
                    && self.economy().is_fortress_city(e)
            });
            let present = self.units().units_at(pos);
            let room = self.max_tile().saturating_sub(present.len());
            let troops: Vec<UnitId> = present
                .iter()
                .copied()
                .filter(|&u| self.units().unit_owner(u) == Some(owner) && self.raids(u))
                .filter_map(|u| self.world.get::<&Unit>(u).ok().map(|k| k.0))
                .collect();
            if troops.is_empty() || room == 0 {
                continue;
            }
            out.push(Source {
                city,
                owner,
                pos,
                troops,
                room,
                ai_fortress,
            });
        }
        out
    }

    /// Whether a unit is of a kind a party is made of: an ordinary fighter, neither a settler nor
    /// a ship.
    fn raids(&self, unit: hecs::Entity) -> bool {
        let units = self.units();
        units
            .unit_def(unit)
            .is_some_and(|d| d.kind == UnitKind::Normal)
            && !units.unit_has_ability(unit, "create_outpost")
            && units.unit_stat(unit, Stat::Transport) == 0
            && units
                .unit_stat(unit, Stat::Melee)
                .max(units.unit_stat(unit, Stat::Ranged))
                > 0
    }
}
