//! The wizard's fortress and summoning circle, banishment and return.
//!
//! In the original the fortress is a special building in the wizard's capital; here it is a
//! location on the [`Player`] (the city standing there is the fortress city). The summoning
//! circle is where summoned creatures appear; it starts in the fortress city and can be moved
//! by a spell. When the fortress city is taken the wizard is banished and starts casting the
//! Spell of Return, which puts the fortress in a city of their choice.
//!
//! [`Player`]: crate::player::Player

use crate::city::{City, FoundCityError, PlayerId, Position};
use crate::economy::{CityRace, Population};
use crate::rules::RaceId;
use crate::{Event, Game};

/// Why a fortress operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FortressError {
    UnknownPlayer,
    /// The wizard already has a fortress, or was defeated.
    NotAllowed,
    Found(FoundCityError),
    /// The city does not exist or belongs to someone else.
    NotYourCity,
    /// The spell only works while banished (or only while not banished).
    WrongState,
    /// A fortress city cannot be destroyed.
    CannotDestroyFortress,
}

/// Hook names used by spell data for the fortress spells.
pub const MOVE_FORTRESS: &str = "move_fortress";
pub const MOVE_CIRCLE: &str = "move_summoning_circle";
pub const RETURN: &str = "return_to_fortress";

impl Game {
    /// The city standing on the wizard's fortress, if any.
    pub fn fortress_city(&self, player: PlayerId) -> Option<hecs::Entity> {
        let pos = self.player(player)?.fortress?;
        self.city_at(pos)
            .filter(|&c| self.city_owner(c) == Some(player))
    }

    pub fn is_fortress_city(&self, city: hecs::Entity) -> bool {
        self.city_owner(city)
            .is_some_and(|o| self.fortress_city(o) == Some(city))
    }

    /// Where summoned creatures appear.
    pub fn summon_point(&self, player: PlayerId) -> Option<Position> {
        let p = self.player(player)?;
        p.summon_at.or(p.fortress)
    }

    fn own_city_pos(
        &self,
        player: PlayerId,
        city: hecs::Entity,
    ) -> Result<Position, FortressError> {
        if self.city_owner(city) != Some(player) {
            return Err(FortressError::NotYourCity);
        }
        self.world
            .get::<&Position>(city)
            .map(|p| *p)
            .map_err(|_| FortressError::NotYourCity)
    }

    /// Found the wizard's first city and put the fortress and summoning circle in it.
    pub fn place_fortress(
        &mut self,
        player: PlayerId,
        pos: Position,
        name: String,
        race: Option<RaceId>,
        population: u32,
    ) -> Result<Vec<Event>, FortressError> {
        let p = self.player(player).ok_or(FortressError::UnknownPlayer)?;
        if p.fortress.is_some() || p.banished || p.defeated {
            return Err(FortressError::NotAllowed);
        }
        self.can_found_city(pos).map_err(FortressError::Found)?;
        let city = self.spawn_city(player, pos, name, race, population);
        if let Some(p) = self.player_mut(player) {
            p.fortress = Some(pos);
            p.summon_at = Some(pos);
            p.capital_race = race;
        }
        Ok(vec![
            Event::CityFounded { city, pos },
            Event::FortressPlaced { player, pos },
        ])
    }

    /// Move the fortress (and the summoning circle with it) to another of the wizard's cities.
    pub fn move_fortress(
        &mut self,
        player: PlayerId,
        city: hecs::Entity,
    ) -> Result<Vec<Event>, FortressError> {
        let pos = self.own_city_pos(player, city)?;
        if self.player(player).and_then(|p| p.fortress).is_none() {
            return Err(FortressError::WrongState);
        }
        if let Some(p) = self.player_mut(player) {
            p.fortress = Some(pos);
            p.summon_at = Some(pos);
        }
        Ok(vec![Event::FortressMoved { player, pos }])
    }

    /// Put the summoning circle in another of the wizard's cities.
    pub fn move_summoning_circle(
        &mut self,
        player: PlayerId,
        city: hecs::Entity,
    ) -> Result<Vec<Event>, FortressError> {
        let pos = self.own_city_pos(player, city)?;
        if let Some(p) = self.player_mut(player) {
            p.summon_at = Some(pos);
        }
        Ok(vec![Event::SummoningCircleMoved { player, pos }])
    }

    /// The Spell of Return: a banished wizard puts the fortress in a city of their choice and
    /// takes that city's race as their capital race.
    pub fn return_to_fortress(
        &mut self,
        player: PlayerId,
        city: hecs::Entity,
    ) -> Result<Vec<Event>, FortressError> {
        let pos = self.own_city_pos(player, city)?;
        if !self.player(player).is_some_and(|p| p.banished) {
            return Err(FortressError::WrongState);
        }
        let race = self.world.get::<&CityRace>(city).ok().map(|r| r.0);
        if let Some(p) = self.player_mut(player) {
            p.banished = false;
            p.fortress = Some(pos);
            p.summon_at = Some(pos);
            if race.is_some() {
                p.capital_race = race;
            }
        }
        Ok(vec![Event::WizardReturned { player, pos }])
    }

    fn city_count(&self, player: PlayerId) -> usize {
        self.cities()
            .iter()
            .filter(|(_, c, _)| c.owner == player)
            .count()
    }

    /// A city changes hands (the combat layer calls this when a city is taken). Taking a
    /// wizard's fortress city banishes them; taking their last city defeats them.
    pub fn capture_city(&mut self, city: hecs::Entity, winner: PlayerId) -> Vec<Event> {
        let Some(loser) = self.city_owner(city) else {
            return Vec::new();
        };
        if loser == winner {
            return Vec::new();
        }
        let was_fortress = self.is_fortress_city(city);
        let pos = self.world.get::<&Position>(city).map(|p| *p).ok();
        if let Ok(mut c) = self.world.get::<&mut City>(city) {
            c.owner = winner;
        }
        let mut events = vec![Event::CityCaptured {
            city,
            from: loser,
            to: winner,
        }];
        if let (Some(pos), Some(p)) = (pos, self.player_mut(loser)) {
            if p.summon_at == Some(pos) {
                p.summon_at = p.fortress.filter(|&f| f != pos);
            }
        }
        if was_fortress {
            events.extend(self.banish(loser, winner));
        } else if self.is_neutral_player(loser) {
            // An independent city's player goes with its city: the garrison is gone or
            // dismissed, and the treasury stays behind.
            for u in self.player_units(loser) {
                let _ = self.world.despawn(u);
            }
            self.players.retain(|p| p.id != loser);
        } else if self.city_count(loser) == 0 {
            events.extend(self.defeat(loser, winner));
        }
        events
    }

    /// The fortress falls: the wizard loses it and starts casting the Spell of Return; the
    /// winner gains 5 fame and half the loser's mana. With no cities left, they are defeated.
    fn banish(&mut self, loser: PlayerId, winner: PlayerId) -> Vec<Event> {
        let half = self.player(loser).map_or(0, |p| p.mana / 2);
        if let Some(p) = self.player_mut(loser) {
            p.mana -= half;
            p.fortress = None;
            p.summon_at = None;
        }
        if let Some(w) = self.player_mut(winner) {
            w.fame += 5;
            w.mana += half;
        }
        if self.city_count(loser) == 0 {
            return self.defeat(loser, winner);
        }
        self.conquest_relations(winner, loser, false);
        let spell = self.content.rules.spells.id_of("spell_of_return");
        let cost = spell.map(|s| self.casting_cost(loser, s, false));
        if let Some(p) = self.player_mut(loser) {
            p.banished = true;
            if let (Some(spell), Some(cost)) = (spell, cost) {
                p.magic.casting = Some(crate::magic::Casting {
                    spell,
                    remaining: cost,
                });
            }
        }
        vec![Event::WizardBanished {
            player: loser,
            by: winner,
        }]
    }

    pub(crate) fn defeat(&mut self, loser: PlayerId, winner: PlayerId) -> Vec<Event> {
        if let Some(p) = self.player_mut(loser) {
            p.defeated = true;
            p.banished = false;
            p.fortress = None;
            p.summon_at = None;
            p.magic.casting = None;
        }
        self.aftermath(loser, winner);
        self.conquest_relations(winner, loser, true);
        let mut events = vec![Event::WizardDefeated { player: loser }];
        events.extend(self.check_conquest(winner));
        events
    }

    /// Raze a city. A fortress city cannot be destroyed: it survives with at least one
    /// citizen, as in the original.
    pub fn destroy_city(&mut self, city: hecs::Entity) -> Result<Vec<Event>, FortressError> {
        let owner = self.city_owner(city).ok_or(FortressError::NotYourCity)?;
        if self.is_fortress_city(city) {
            if let Ok(mut pop) = self.world.get::<&mut Population>(city) {
                pop.figures = pop.figures.max(1);
            }
            return Err(FortressError::CannotDestroyFortress);
        }
        let pos = self.world.get::<&Position>(city).map(|p| *p).ok();
        let _ = self.world.despawn(city);
        if let (Some(pos), Some(p)) = (pos, self.player_mut(owner)) {
            if p.summon_at == Some(pos) {
                p.summon_at = p.fortress;
            }
        }
        let mut events = vec![Event::CityDestroyed {
            pos: pos.unwrap_or(Position {
                plane: crate::map::Plane::Arcanus,
                x: 0,
                y: 0,
            }),
        }];
        if self.city_count(owner) == 0 {
            events.extend(self.defeat(owner, owner));
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests;
