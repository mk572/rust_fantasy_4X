//! The ceiling on the units of one square. The rules limit the units that walk onto a square
//! ([`Economy::tile_limit`](crate::city::economy::Economy::tile_limit)), but a unit can also be put
//! there without walking: hired, summoned, built, freed, raised, recalled. Those are never refused
//! for a full square, so a square can hold more than the rules allow, and the player sorts it out
//! by moving units off it. This is the one hard limit, [`TILE_SANITY_LIMIT`]: past it the game
//! disbands the cheapest unit of the square (a cheat, or a computer wizard looping, is the only way
//! to get there) and tells the player (`LossCause::Crowded`).
//!
//! It runs after every command (`Game::execute`), so whatever put the units there is covered
//! without each of them knowing.

use std::collections::HashMap;

use crate::game::{Event, Game, LossCause};
use crate::map::Position;
use crate::unit::TILE_SANITY_LIMIT;

impl Game {
    /// Disband the cheapest unit on each square that holds more than [`TILE_SANITY_LIMIT`] units,
    /// until none does. The newest unit goes first among equals.
    pub(crate) fn enforce_tile_sanity(&mut self) -> Vec<Event> {
        if self.battle.is_some() {
            return Vec::new();
        }
        let mut count: HashMap<Position, usize> = HashMap::new();
        for (_, (_, at)) in self.world.query::<(&crate::unit::Unit, &Position)>().iter() {
            *count.entry(*at).or_default() += 1;
        }
        let mut over: Vec<Position> = count
            .into_iter()
            .filter(|&(_, n)| n > TILE_SANITY_LIMIT)
            .map(|(p, _)| p)
            .collect();
        over.sort_by_key(|p| (p.plane as u8, p.y, p.x));
        let mut events = Vec::new();
        for at in over {
            let mut here = self.units().units_at(at);
            self.units().sort_by_serial(&mut here);
            while here.len() > TILE_SANITY_LIMIT {
                let cheapest = here
                    .iter()
                    .enumerate()
                    .min_by_key(|&(i, &u)| {
                        let cost = self.units().unit_def(u).map_or(0, |d| d.cost);
                        (cost, std::cmp::Reverse(i))
                    })
                    .map(|(i, _)| i)
                    .expect("a square that is over the limit holds units");
                let unit = here.remove(cheapest);
                let (owner, kind) = (
                    self.units().unit_owner(unit),
                    self.units().unit_kind_id(unit),
                );
                if self
                    .units()
                    .unit_def(unit)
                    .is_some_and(|d| d.kind == crate::rules::UnitKind::Hero)
                {
                    if let Some(owner) = owner {
                        self.pass_items(unit, owner);
                    }
                }
                self.remove_unit(unit, crate::hero::Fate::Released);
                if let (Some(player), Some(unit)) = (owner, kind) {
                    events.push(Event::UnitLost {
                        player,
                        unit,
                        cause: LossCause::Crowded,
                    });
                }
            }
        }
        events
    }
}
