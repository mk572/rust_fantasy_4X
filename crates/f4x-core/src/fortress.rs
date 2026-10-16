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
use crate::economy::{CityRace, Economy, Population};
use crate::player::Player;
use crate::rules::{RaceId, SpellId};
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

/// Where summoned creatures appear: the summoning circle, else the fortress.
pub fn summon_point(player: &Player) -> Option<Position> {
    player.summon_at.or(player.fortress)
}

/// Whether the wizard may found a fortress: they have none, are not banished and not defeated.
pub(crate) fn can_place(player: &Player) -> Result<(), FortressError> {
    if player.fortress.is_some() || player.banished || player.defeated {
        return Err(FortressError::NotAllowed);
    }
    Ok(())
}

/// The fortress and the summoning circle stand in the wizard's first city at `pos`.
pub(crate) fn place_at(player: &mut Player, pos: Position, race: Option<RaceId>) {
    player.fortress = Some(pos);
    player.summon_at = Some(pos);
    player.capital_race = race;
}

/// Move the fortress (and the summoning circle with it) to the city at `pos`.
pub(crate) fn move_fortress_to(player: &mut Player, pos: Position) -> Result<(), FortressError> {
    if player.fortress.is_none() {
        return Err(FortressError::WrongState);
    }
    player.fortress = Some(pos);
    player.summon_at = Some(pos);
    Ok(())
}

/// The Spell of Return: a banished wizard puts the fortress at `pos` and takes the city's race
/// (if it has one) as their capital race.
pub(crate) fn return_to(
    player: &mut Player,
    pos: Position,
    race: Option<RaceId>,
) -> Result<(), FortressError> {
    if !player.banished {
        return Err(FortressError::WrongState);
    }
    player.banished = false;
    player.fortress = Some(pos);
    player.summon_at = Some(pos);
    if race.is_some() {
        player.capital_race = race;
    }
    Ok(())
}

/// A city of the wizard's was taken: a summoning circle in it falls back to the fortress,
/// unless that is the city.
pub(crate) fn circle_lost_to_capture(player: &mut Player, pos: Position) {
    if player.summon_at == Some(pos) {
        player.summon_at = player.fortress.filter(|&f| f != pos);
    }
}

/// A city of the wizard's was destroyed: a circle in it falls back to the fortress.
pub(crate) fn circle_lost_to_destruction(player: &mut Player, pos: Position) {
    if player.summon_at == Some(pos) {
        player.summon_at = player.fortress;
    }
}

/// The fortress falls: the wizard loses it and half their mana, which is returned.
pub(crate) fn fortress_falls(loser: &mut Player) -> i32 {
    let half = loser.mana / 2;
    loser.mana -= half;
    loser.fortress = None;
    loser.summon_at = None;
    half
}

/// The conqueror of a fortress city gains 5 fame and the loser's half of the mana.
pub(crate) fn fortress_prize(winner: &mut Player, half_mana: i32) {
    winner.fame += 5;
    winner.mana += half_mana;
}

/// The wizard is banished and starts casting the Spell of Return, at the cost given.
pub(crate) fn banish_player(player: &mut Player, spell_of_return: Option<(SpellId, i32)>) {
    player.banished = true;
    if let Some((spell, cost)) = spell_of_return {
        player.magic.casting = Some(crate::magic::Casting {
            spell,
            remaining: cost,
        });
    }
}

/// The wizard is out of the game.
pub(crate) fn defeat_player(player: &mut Player) {
    player.defeated = true;
    player.banished = false;
    player.fortress = None;
    player.summon_at = None;
    player.magic.casting = None;
}

impl Economy<'_> {
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

    /// The position of a city of the player's own.
    pub(crate) fn own_city_pos(
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

    pub(crate) fn city_count(&self, player: PlayerId) -> usize {
        self.cities()
            .iter()
            .filter(|(_, c, _)| c.owner == player)
            .count()
    }
}

impl Game {
    /// Where summoned creatures appear.
    pub fn summon_point(&self, player: PlayerId) -> Option<Position> {
        self.player(player).and_then(summon_point)
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
        can_place(p)?;
        self.can_found_city(pos).map_err(FortressError::Found)?;
        let city = self.spawn_city(player, pos, name, race, population);
        if let Some(p) = self.player_mut(player) {
            place_at(p, pos, race);
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
        let pos = self.economy().own_city_pos(player, city)?;
        match self.player_mut(player) {
            Some(p) => move_fortress_to(p, pos)?,
            None => return Err(FortressError::WrongState),
        }
        Ok(vec![Event::FortressMoved { player, pos }])
    }

    /// Put the summoning circle in another of the wizard's cities.
    pub fn move_summoning_circle(
        &mut self,
        player: PlayerId,
        city: hecs::Entity,
    ) -> Result<Vec<Event>, FortressError> {
        let pos = self.economy().own_city_pos(player, city)?;
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
        let pos = self.economy().own_city_pos(player, city)?;
        let race = self.world.get::<&CityRace>(city).ok().map(|r| r.0);
        match self.player_mut(player) {
            Some(p) => return_to(p, pos, race)?,
            None => return Err(FortressError::WrongState),
        }
        Ok(vec![Event::WizardReturned { player, pos }])
    }

    /// A city changes hands (the combat layer calls this when a city is taken). Taking a
    /// wizard's fortress city banishes them; taking their last city defeats them.
    pub fn capture_city(&mut self, city: hecs::Entity, winner: PlayerId) -> Vec<Event> {
        let Some(loser) = self.economy().city_owner(city) else {
            return Vec::new();
        };
        if loser == winner {
            return Vec::new();
        }
        let was_fortress = self.economy().is_fortress_city(city);
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
            circle_lost_to_capture(p, pos);
        }
        if was_fortress {
            events.extend(self.banish(loser, winner));
        } else if self.is_neutral_player(loser) {
            // An independent city's player goes with its city: the garrison is gone or
            // dismissed, and the treasury stays behind.
            for u in self.units().player_units(loser) {
                self.remove_unit(u, crate::roster::Fate::Released);
            }
            self.players.retain(|p| p.id != loser);
        } else if self.economy().city_count(loser) == 0 {
            events.extend(self.defeat(loser, winner));
        }
        events
    }

    /// The fortress falls: the wizard loses it and starts casting the Spell of Return; the
    /// winner gains 5 fame and half the loser's mana. With no cities left, they are defeated.
    fn banish(&mut self, loser: PlayerId, winner: PlayerId) -> Vec<Event> {
        let half = self.player_mut(loser).map_or(0, fortress_falls);
        if let Some(w) = self.player_mut(winner) {
            fortress_prize(w, half);
        }
        if self.economy().city_count(loser) == 0 {
            return self.defeat(loser, winner);
        }
        self.conquest_relations(winner, loser, false);
        let spell = self.content.rules.spells.id_of("spell_of_return");
        let cost = spell.map(|s| self.casting_cost(loser, s, false));
        if let Some(p) = self.player_mut(loser) {
            banish_player(p, spell.zip(cost));
        }
        vec![Event::WizardBanished {
            player: loser,
            by: winner,
        }]
    }

    pub(crate) fn defeat(&mut self, loser: PlayerId, winner: PlayerId) -> Vec<Event> {
        if let Some(p) = self.player_mut(loser) {
            defeat_player(p);
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
        let owner = self
            .economy()
            .city_owner(city)
            .ok_or(FortressError::NotYourCity)?;
        if self.economy().is_fortress_city(city) {
            if let Ok(mut pop) = self.world.get::<&mut Population>(city) {
                pop.figures = pop.figures.max(1);
            }
            return Err(FortressError::CannotDestroyFortress);
        }
        let pos = self.world.get::<&Position>(city).map(|p| *p).ok();
        let _ = self.world.despawn(city);
        if let (Some(pos), Some(p)) = (pos, self.player_mut(owner)) {
            circle_lost_to_destruction(p, pos);
        }
        let mut events = vec![Event::CityDestroyed {
            pos: pos.unwrap_or(Position {
                plane: crate::map::Plane::Arcanus,
                x: 0,
                y: 0,
            }),
        }];
        if self.economy().city_count(owner) == 0 {
            events.extend(self.defeat(owner, owner));
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests;
