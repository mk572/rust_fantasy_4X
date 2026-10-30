//! The wizard's fortress and summoning circle, banishment and return.
//!
//! In the original the fortress is a special building in the wizard's capital; here it is a
//! location on the wizard (the city standing there is the fortress city). The summoning
//! circle is where summoned creatures appear; it starts in the fortress city and can be moved
//! by a spell. When the fortress city is taken the wizard is banished and starts casting the
//! Spell of Return, which puts the fortress in a city of their choice.
//!
//! Tests: in wizard/fortress/tests.rs.

use crate::city::economy::{CityRace, Economy, Population};
use crate::city::FoundCityError;
use crate::map::Position;
use crate::player::PlayerId;
use crate::wizard::entity::{CapitalRace, Wizard};
use crate::wizard::sheet::WizardMut;

use crate::game::{Event, Game};
use crate::rules::RaceId;

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

impl FortressError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::UnknownPlayer => "refuse_map.fortress_error_unknown_player",
            Self::NotAllowed => "refuse_map.fortress_error_not_allowed",
            Self::Found(e) => e.key(),
            Self::NotYourCity => "refuse_map.fortress_error_not_your_city",
            Self::WrongState => "refuse_map.fortress_error_wrong_state",
            Self::CannotDestroyFortress => "refuse_map.fortress_error_cannot_destroy_fortress",
        }
    }
}

/// A wizard whose casting skill (with heroes) is below this is defeated when banished rather
/// than casting the Spell of Return.
const RESIGN_BELOW_SKILL: i32 = 40;

/// Hook names used by spell data for the fortress spells.
pub const MOVE_FORTRESS: &str = "move_fortress";
pub const MOVE_CIRCLE: &str = "move_summoning_circle";
pub const RETURN: &str = "return_to_fortress";

/// On a wizard: where its fortress stands (a city). Absent before it is placed and while the
/// wizard is banished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fortress(pub Position);

/// On a wizard: where summoned creatures appear. Absent: the fortress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummonAt(pub Position);

/// On a wizard whose fortress fell: it is casting the Spell of Return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Banishment {
    /// Who took the fortress (the one a resignation is to).
    pub by: Option<PlayerId>,
}

impl Wizard<'_> {
    /// Where the wizard's fortress stands.
    pub fn fortress(&self) -> Option<Position> {
        self.component::<Fortress>().map(|f| f.0)
    }

    /// Where the wizard's summoning circle stands.
    pub fn summon_at(&self) -> Option<Position> {
        self.component::<SummonAt>().map(|s| s.0)
    }

    /// Where summoned creatures appear: the summoning circle, else the fortress.
    pub fn summon_point(&self) -> Option<Position> {
        self.summon_at().or(self.fortress())
    }

    /// Has the wizard's fortress fallen?
    pub fn is_banished(&self) -> bool {
        self.component::<Banishment>().is_some()
    }

    /// Who took the fortress of a banished wizard.
    pub fn banished_by(&self) -> Option<PlayerId> {
        self.component::<Banishment>().and_then(|b| b.by)
    }
}

/// Whether the wizard may found a fortress: they have none, are not banished and not defeated.
pub(crate) fn can_place(wizard: &Wizard, defeated: bool) -> Result<(), FortressError> {
    if wizard.fortress().is_some() || wizard.is_banished() || defeated {
        return Err(FortressError::NotAllowed);
    }
    Ok(())
}

/// The fortress and the summoning circle stand in the wizard's first city at `pos`.
pub(crate) fn place_at(wizard: &mut WizardMut, pos: Position) {
    wizard.put(Some(Fortress(pos)));
    wizard.put(Some(SummonAt(pos)));
}

/// Move the fortress (and the summoning circle with it) to the city at `pos`.
pub(crate) fn move_fortress_to(wizard: &mut WizardMut, pos: Position) -> Result<(), FortressError> {
    if wizard.view().fortress().is_none() {
        return Err(FortressError::WrongState);
    }
    place_at(wizard, pos);
    Ok(())
}

/// The Spell of Return: a banished wizard puts the fortress at `pos` and takes the city's race
/// (if it has one) as their capital race.
pub(crate) fn return_to(wizard: &mut WizardMut, pos: Position) -> Result<(), FortressError> {
    if !wizard.view().is_banished() {
        return Err(FortressError::WrongState);
    }
    wizard.put::<Banishment>(None);
    place_at(wizard, pos);
    Ok(())
}

/// A city of the wizard's was taken: a summoning circle in it falls back to the fortress,
/// unless that is the city.
pub(crate) fn circle_lost_to_capture(wizard: &mut WizardMut, pos: Position) {
    let view = wizard.view();
    if view.summon_at() == Some(pos) {
        let back = view.fortress().filter(|&f| f != pos);
        wizard.put(back.map(SummonAt));
    }
}

/// A city of the wizard's was destroyed: a circle in it falls back to the fortress.
pub(crate) fn circle_lost_to_destruction(wizard: &mut WizardMut, pos: Position) {
    let view = wizard.view();
    if view.summon_at() == Some(pos) {
        let back = view.fortress();
        wizard.put(back.map(SummonAt));
    }
}

/// The fortress falls: the wizard loses it and the summoning circle. (Half their mana goes to the
/// conqueror: [`Treasury::take_half_mana`].)
///
/// [`Treasury::take_half_mana`]: crate::player::Treasury::take_half_mana
pub(crate) fn fortress_falls(wizard: &mut WizardMut) {
    wizard.put::<Fortress>(None);
    wizard.put::<SummonAt>(None);
}

/// The fame the conqueror of a fortress city gains (and the loser's half of the mana).
pub(crate) const FORTRESS_PRIZE_FAME: i32 = 5;

/// The wizard is banished by `by`; the game starts the Spell of Return.
pub(crate) fn banish_player(wizard: &mut WizardMut, by: PlayerId) {
    wizard.put(Some(Banishment { by: Some(by) }));
}

/// The wizard is out of the game.
pub(crate) fn defeat_player(wizard: &mut WizardMut) {
    wizard.put::<Banishment>(None);
    fortress_falls(wizard);
}

impl Economy<'_> {
    /// The city standing on the wizard's fortress, if any.
    pub fn fortress_city(&self, player: PlayerId) -> Option<hecs::Entity> {
        let pos = self.wizard(player)?.fortress()?;
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
        self.wizard(player).and_then(|w| w.summon_point())
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
        can_place(
            &self.wizard(player).ok_or(FortressError::UnknownPlayer)?,
            self.player(player).is_some_and(|p| p.is_defeated()),
        )?;
        self.can_found_city(pos).map_err(FortressError::Found)?;
        let city = self.spawn_city(player, pos, name, race, population);
        if let Some(mut w) = self.wizard_mut(player) {
            place_at(&mut w, pos);
        }
        self.wizard_set(player, CapitalRace(race));
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
        match self.wizard_mut(player) {
            Some(mut w) => move_fortress_to(&mut w, pos)?,
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
        if let Some(mut w) = self.wizard_mut(player) {
            w.put(Some(SummonAt(pos)));
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
        match self.wizard_mut(player) {
            Some(mut w) => return_to(&mut w, pos)?,
            None => return Err(FortressError::WrongState),
        }
        if race.is_some() {
            self.wizard_set(player, CapitalRace(race));
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
        self.set_owner(city, winner);
        // Whoever takes a city ends the old owner's enchantments on it (`Change_City_Ownership`).
        for (spell, caster) in self.spells().city_enchantments(city) {
            if caster == loser {
                self.remove_city_enchantment(city, spell, caster);
            }
        }
        if self.is_neutral_player(loser) {
            // The plan the independent city's AI set was its own; the new owner plans anew.
            let _ = self
                .world
                .remove_one::<crate::city::autobuild::AutoBuild>(city);
        }
        let mut events = vec![Event::CityCaptured {
            city,
            from: loser,
            to: winner,
        }];
        if let (Some(pos), Some(mut w)) = (pos, self.wizard_mut(loser)) {
            circle_lost_to_capture(&mut w, pos);
        }
        if was_fortress {
            events.extend(self.banish(loser, winner));
        } else if self.is_neutral_player(loser) {
            // An independent city's player goes with its city: the garrison is gone or
            // dismissed, and the treasury stays behind.
            for u in self.units().player_units(loser) {
                self.remove_unit(u, crate::hero::Fate::Released);
            }
            self.remove_player(loser);
        } else if self.economy().city_count(loser) == 0 {
            // The last city fell, and it was not the fortress: the prize is the same
            // (`Resolve_Wizard_Conquest`).
            self.take_fortress_prize(loser, winner);
            events.extend(self.defeat(loser, winner));
        }
        events
    }

    /// What the winner gets from a wizard who loses the fortress or the last city: 5 fame and
    /// half the loser's mana.
    fn take_fortress_prize(&mut self, loser: PlayerId, winner: PlayerId) {
        let half = self.treasury_mut(loser).map_or(0, |t| t.take_half_mana());
        self.add_fame(winner, FORTRESS_PRIZE_FAME);
        if let Some(t) = self.treasury_mut(winner) {
            t.mana += half;
        }
    }

    /// `Conquest_Spells`: the winner takes up to two spells chosen at random from those the loser
    /// knows and it does not. Nothing happens unless both are wizards. The event is made even when
    /// nothing is found, for the human winner's box.
    fn conquest_spells(&mut self, winner: PlayerId, loser: PlayerId) -> Vec<Event> {
        if winner == loser || self.wizard(winner).is_none() || self.wizard(loser).is_none() {
            return Vec::new();
        }
        let mine = self.wizard(winner).map(|w| w.known()).unwrap_or_default();
        let mut pool: Vec<_> = self
            .wizard(loser)
            .map(|w| w.known())
            .unwrap_or_default()
            .into_iter()
            .filter(|s| !mine.contains(s))
            .collect();
        let mut found = Vec::new();
        while found.len() < 2 && !pool.is_empty() {
            let pick = self.rng.roll(pool.len() as u32) as usize - 1;
            let spell = pool.remove(pick);
            self.learn_spell(winner, spell);
            found.push(spell);
        }
        vec![Event::ConquestSpells {
            winner,
            loser,
            spells: found,
        }]
    }

    /// The fortress falls: the wizard loses it and starts casting the Spell of Return; the
    /// winner gains 5 fame and half the loser's mana. With no cities left, they are defeated.
    /// A wizard who could not pay for the spell (casting skill under 40, or no mana) is
    /// defeated on the spot, the human too (`Banish_Wizard`).
    fn banish(&mut self, loser: PlayerId, winner: PlayerId) -> Vec<Event> {
        let skill =
            self.wizard(loser).map_or(0, |m| m.casting_skill()) + self.hero_casting_skill(loser);
        let mana = self.player(loser).map_or(0, |p| p.treasury().mana);
        if let Some(mut w) = self.wizard_mut(loser) {
            fortress_falls(&mut w);
        }
        self.take_fortress_prize(loser, winner);
        if self.economy().city_count(loser) == 0 || skill < RESIGN_BELOW_SKILL || mana < 1 {
            return self.defeat(loser, winner);
        }
        self.conquest_relations(winner, loser, false);
        let spell = self.content.rules.spells.id_of("spell_of_return");
        let cost = spell.map(|s| self.casting_cost(loser, s, false));
        if let Some(mut w) = self.wizard_mut(loser) {
            banish_player(&mut w, winner);
        }
        if let Some((spell, cost)) = spell.zip(cost) {
            self.begin_casting(loser, spell, cost);
        }
        let mut events = vec![Event::WizardBanished {
            player: loser,
            by: winner,
        }];
        // As in the original, a banished human's spells stay with them: the winner takes spells
        // only from a computer wizard (the human is asked whether to resign instead).
        if self.human() != Some(loser) {
            events.extend(self.conquest_spells(winner, loser));
        }
        events
    }

    /// A banished wizard gives up (the original asks the human at the moment of banishment,
    /// "Do you wish to resign?"): they are defeated by whoever took their fortress, their
    /// cities turn neutral and everything else of theirs goes (`Resolve_Wizard_Conquest`).
    pub fn resign(&mut self, player: PlayerId) -> Result<Vec<Event>, FortressError> {
        let w = self.wizard(player).ok_or(FortressError::UnknownPlayer)?;
        if !w.is_banished() || self.player(player).is_some_and(|p| p.is_defeated()) {
            return Err(FortressError::WrongState);
        }
        let banished_by = w.banished_by();
        let winner = banished_by
            .filter(|w| self.has_player(*w))
            .or_else(|| self.living_wizards().into_iter().find(|w| *w != player))
            .unwrap_or(player);
        Ok(self.defeat(player, winner))
    }

    /// A defeated wizard's cities that are left (a wizard who resigned with cities) turn
    /// neutral, each into a player of its own as the neutral cities are. Heroes and conjured
    /// creatures in them are dismissed; the normal units garrisoned there go with the city.
    fn cities_go_neutral(&mut self, loser: PlayerId) {
        let cities: Vec<_> = self
            .economy()
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == loser)
            .collect();
        for (city, ..) in cities {
            self.make_city_neutral(city, loser);
        }
    }

    /// A city of `owner` turns neutral, a player of its own as the neutral cities are: the
    /// normal units garrisoned there stay with it, the heroes and conjured creatures are
    /// dismissed.
    pub(crate) fn make_city_neutral(&mut self, city: hecs::Entity, owner: PlayerId) {
        let Some(pos) = self.world.get::<&Position>(city).ok().map(|p| *p) else {
            return;
        };
        let neutral = self.new_city_player();
        let garrison: Vec<_> = self
            .units()
            .units_at(pos)
            .into_iter()
            .filter(|&u| self.units().unit_owner(u) == Some(owner))
            .collect();
        for u in garrison {
            let normal = self
                .units()
                .unit_def(u)
                .is_some_and(|d| d.kind == crate::rules::UnitKind::Normal);
            if normal {
                self.set_owner(u, neutral);
            } else {
                self.remove_unit(u, crate::hero::Fate::Gone);
            }
        }
        self.set_owner(city, neutral);
    }

    pub(crate) fn defeat(&mut self, loser: PlayerId, winner: PlayerId) -> Vec<Event> {
        if let Some(mut w) = self.wizard_mut(loser) {
            defeat_player(&mut w);
            w.clear_casting();
        }
        if let Some(e) = self.player_entity(loser) {
            let _ = self
                .world
                .insert_one(e, crate::player::Defeat { by: Some(winner) });
        }
        self.cities_go_neutral(loser);
        self.aftermath(loser, winner);
        self.conquest_relations(winner, loser, true);
        let mut events = vec![Event::WizardDefeated { player: loser }];
        events.extend(self.conquest_spells(winner, loser));
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
        self.despawn_with_spells(city);
        if let (Some(pos), Some(mut w)) = (pos, self.wizard_mut(owner)) {
            circle_lost_to_destruction(&mut w, pos);
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
