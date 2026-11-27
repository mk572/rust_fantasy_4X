//! The wizard's fortress and summoning circle, banishment and return.
//!
//! The fortress is a location on the wizard (the city standing there is the fortress city). The
//! summoning circle is where summoned creatures appear; it starts in the fortress city and can be
//! moved by a spell. When the fortress city is taken the wizard is banished and starts casting the
//! Spell of Return, which puts the fortress in a city of their choice.
//!
//! Tests: in wizard/fortress/tests.rs.

use crate::city::economy::{CityRace, Economy, Population};
use crate::city::FoundCityError;
use crate::map::Position;
use crate::player::PlayerId;
use crate::wizard::entity::{CapitalRace, Wizard};
use crate::wizard::sheet::WizardMut;
use f4x_def::half_int::HalfInt;

use crate::rules::RaceId;
use crate::world::{Event, World};

/// Why a fortress operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FortressError {
    UnknownPlayer,
    /// The wizard already has a fortress, or was defeated.
    NotAllowed,
    Found(FoundCityError),
    /// The city does not exist, has no owner or no square, or belongs to someone else.
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

/// On a wizard: where its fortress stands (a city). Absent before it is placed and while the
/// wizard is banished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fortress(pub Position);

/// On a wizard: where summoned creatures appear, the cities with a summoning circle in the order they
/// were made (one, unless the setting `multiple_summoning_circles` is on). Absent: the fortress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummonAt(pub Vec<Position>);

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

    /// Where the wizard's summoning circle stands (the first, when there are several).
    pub fn summon_at(&self) -> Option<Position> {
        self.summon_circles().first().copied()
    }

    /// Where the wizard's summoning circles stand, in the order they were made.
    pub fn summon_circles(&self) -> Vec<Position> {
        self.component::<SummonAt>()
            .map(|s| s.0.clone())
            .unwrap_or_default()
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
    wizard.put(Some(SummonAt(vec![pos])));
}

/// A summoning circle in the city at `pos`: it replaces the circles the wizard has, or, with `keep`
/// (the setting `multiple_summoning_circles`), is added to them.
pub(crate) fn put_circle(wizard: &mut WizardMut, pos: Position, keep: bool) {
    let mut circles = if keep {
        wizard.view().summon_circles()
    } else {
        Vec::new()
    };
    if !circles.contains(&pos) {
        circles.push(pos);
    }
    wizard.put(Some(SummonAt(circles)));
}

/// Move the fortress (and a summoning circle with it) to the city at `pos`; with `keep` the circles the
/// wizard has stay.
pub(crate) fn move_fortress_to(
    wizard: &mut WizardMut,
    pos: Position,
    keep: bool,
) -> Result<(), FortressError> {
    if wizard.view().fortress().is_none() {
        return Err(FortressError::WrongState);
    }
    wizard.put(Some(Fortress(pos)));
    put_circle(wizard, pos, keep);
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

/// The summoning circle in the city at `pos` is gone. When it was the last one, creatures appear at the
/// fortress again, unless that is the city (`falls_back`).
fn circle_lost(wizard: &mut WizardMut, pos: Position, falls_back: bool) {
    let view = wizard.view();
    let mut circles = view.summon_circles();
    if !circles.contains(&pos) {
        return;
    }
    circles.retain(|&c| c != pos);
    if circles.is_empty() && falls_back {
        circles.extend(view.fortress().filter(|&f| f != pos));
    }
    wizard.put((!circles.is_empty()).then_some(SummonAt(circles)));
}

/// A city of the wizard's was taken: a summoning circle in it falls back to the fortress,
/// unless that is the city.
pub(crate) fn circle_lost_to_capture(wizard: &mut WizardMut, pos: Position) {
    circle_lost(wizard, pos, true);
}

/// A city of the wizard's was destroyed: a circle in it falls back to the fortress.
pub(crate) fn circle_lost_to_destruction(wizard: &mut WizardMut, pos: Position) {
    circle_lost(wizard, pos, true);
}

/// The fortress falls: the wizard loses it and the summoning circle. (Half their mana goes to the
/// conqueror: [`Treasury::take_half_mana`].)
///
/// [`Treasury::take_half_mana`]: crate::player::Treasury::take_half_mana
pub(crate) fn fortress_falls(wizard: &mut WizardMut) {
    wizard.put::<Fortress>(None);
    wizard.put::<SummonAt>(None);
}

/// How a wizard fell, which decides what the winner takes ([`World::take_spoils`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fall {
    /// The fortress fell and the wizard may come back: fame, half the mana, spells.
    Banished,
    /// Defeated at once (the last city fell, or there is no way back): also half the gold.
    Defeated,
    /// A banished wizard gave up: as defeated, but the fame was paid with the banishment.
    Resigned,
}

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
        self.ecs
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

impl World {
    /// Where summoned creatures, hired heroes and mercenaries appear: a wizard's summoning
    /// circle, else the fortress; a player without a wizard (a neutral race) has its seat, the
    /// biggest city.
    pub fn summon_point(&self, player: PlayerId) -> Option<Position> {
        match self.wizard(player) {
            Some(w) => w.summon_point(),
            None => self.seat_of(player),
        }
    }

    /// Where the wizard's summoning circles stand (empty for a player without a wizard).
    pub fn summon_circles(&self, player: PlayerId) -> Vec<Position> {
        self.wizard(player)
            .map(|w| w.summon_circles())
            .unwrap_or_default()
    }

    /// Does the wizard have more than one summoning circle, so that a summoning asks for the city?
    pub fn has_several_circles(&self, player: PlayerId) -> bool {
        self.summon_circles(player).len() > 1
    }

    /// The position of the summoning circle in `city`, if it is the wizard's and holds one.
    pub fn circle_in(&self, player: PlayerId, city: hecs::Entity) -> Option<Position> {
        let at = self.economy().own_city_pos(player, city).ok()?;
        self.summon_circles(player).contains(&at).then_some(at)
    }

    /// The seat of a player without a wizard: its biggest city by citizens, the one made first
    /// when they tie. It is the new seat when the old one is lost.
    pub fn seat_of(&self, player: PlayerId) -> Option<Position> {
        self.economy()
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == player)
            .max_by_key(|&(e, ..)| {
                (
                    self.economy().citizens(e),
                    std::cmp::Reverse(self.units().serial_of(e)),
                )
            })
            .and_then(|(e, ..)| self.ecs.get::<&Position>(e).ok().map(|p| *p))
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
        let keep = self.settings.rules.multiple_summoning_circles;
        match self.wizard_mut(player) {
            Some(mut w) => move_fortress_to(&mut w, pos, keep)?,
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
        let keep = self.settings.rules.multiple_summoning_circles;
        if let Some(mut w) = self.wizard_mut(player) {
            put_circle(&mut w, pos, keep);
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
        let race = self.ecs.get::<&CityRace>(city).ok().map(|r| r.0);
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
        let pos = self.ecs.get::<&Position>(city).map(|p| *p).ok();
        self.set_owner(city, winner);
        // Whoever takes a city ends the old owner's enchantments on it (`Change_City_Ownership`).
        for (spell, caster) in self.spells().city_enchantments(city) {
            if caster == loser {
                self.remove_city_enchantment(city, spell, caster);
            }
        }
        if self.is_neutral_player(loser) {
            // The plan the independent city's AI set was its own; the new owner plans anew.
            self.strip::<crate::city::autobuild::AutoBuild>(city);
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
        } else if self.is_wizard(loser) && self.economy().city_count(loser) == 0 {
            // The last city fell, and it was not the fortress: the prize is the same
            // (`Resolve_Wizard_Conquest`).
            events.extend(self.defeat(loser, winner, Fall::Defeated));
        }
        events
    }

    /// Everything the winner takes from a wizard who falls, in one place (`Banish_Wizard` and
    /// `Resolve_Wizard_Conquest`): 5 fame (not again from a wizard who gives up after a
    /// banishment, which paid it), half the loser's mana, half their gold when they are
    /// defeated, and up to two of their spells ([`World::conquest_spells`]). The mana and the gold
    /// are only taken when the setting `defeat_loot` is on; off, they are lost with the wizard.
    fn take_spoils(&mut self, loser: PlayerId, winner: PlayerId, fall: Fall) -> Vec<Event> {
        if fall != Fall::Resigned {
            self.add_fame(winner, self.content.rules.config.renown.fortress_prize_fame);
        }
        if self.settings.rules.defeat_loot {
            let (gold, mana) =
                self.treasury_mut(loser)
                    .map_or((HalfInt::ZERO, HalfInt::ZERO), |t| {
                        let gold = match fall {
                            Fall::Banished => HalfInt::ZERO,
                            Fall::Defeated | Fall::Resigned => t.take_half_gold(),
                        };
                        (gold, t.take_half_mana())
                    });
            if let Some(t) = self.treasury_mut(winner) {
                t.gold += gold;
                t.mana += mana;
            }
        }
        self.conquest_spells(winner, loser)
    }

    /// `Conquest_Spells`: the winner takes up to two spells chosen at random from those the loser
    /// knows and it does not, that its books allow. Nothing happens unless both are wizards. The event is made even when
    /// nothing is found, for the human winner's box.
    fn conquest_spells(&mut self, winner: PlayerId, loser: PlayerId) -> Vec<Event> {
        if winner == loser || self.wizard(winner).is_none() || self.wizard(loser).is_none() {
            return Vec::new();
        }
        let mine = self.wizard(winner).map(|w| w.known()).unwrap_or_default();
        // Only a spell the winner has the books for (`Get_Differential_Spell_List`).
        let spells = &self.content.rules.spells;
        let winner_sheet = self.wizard(winner);
        let mut pool: Vec<_> = self
            .wizard(loser)
            .map(|w| w.known())
            .unwrap_or_default()
            .into_iter()
            .filter(|s| !mine.contains(s))
            .filter(|&s| winner_sheet.is_some_and(|w| w.books_allow(spells.get(s))))
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
        let skill = self
            .wizard(loser)
            .map_or(0, |m| m.casting_skill(&self.content.rules))
            + self.hero_casting_skill(loser);
        let mana = self
            .player(loser)
            .map_or(HalfInt::ZERO, |p| p.treasury().mana);
        if let Some(mut w) = self.wizard_mut(loser) {
            fortress_falls(&mut w);
        }
        // The game's rules may leave this wizard no Spell of Return (`RuleSettings::spell_of_return`).
        if self.economy().city_count(loser) == 0
            || skill < self.content.rules.config.wizard.resign_below_skill
            || mana < 1
            || !self.return_open_to(loser)
        {
            return self.defeat(loser, winner, Fall::Defeated);
        }
        self.conquest_relations(winner, loser, false);
        let spell = self.content.rules.spells.id_numbered(214) /* spell_of_return */;
        let cost = spell.map(|s| self.casting_cost(loser, s, false));
        if let Some(mut w) = self.wizard_mut(loser) {
            // The Return takes the place of what the wizard was casting.
            w.clear_casting();
            banish_player(&mut w, winner);
        }
        if let Some((spell, cost)) = spell.zip(cost) {
            self.begin_casting(loser, spell, cost);
        }
        let mut events = vec![Event::WizardBanished {
            player: loser,
            by: winner,
        }];
        // The spoils of a banishment go to the winner whoever it is and whoever loses: the game
        // does not tell the human from a computer wizard (1.50 note 502; the original took them
        // from the banished wizard unless it was the one whose turn it was).
        events.extend(self.take_spoils(loser, winner, Fall::Banished));
        events
    }

    /// A banished wizard that is casting nothing (its Spell of Return was ended by a Spell Blast)
    /// begins the Spell of Return again: a banished wizard casts nothing else and is always
    /// casting it until it is cast, so no wizard, the computer's or the human's, is left banished
    /// with nothing to do. Called at the start of the wizard's turn end (`process_magic`).
    pub(crate) fn resume_return(&mut self, player: PlayerId) {
        let idle = self
            .wizard(player)
            .is_some_and(|w| w.is_banished() && w.casting().is_none());
        if !idle || !self.return_open_to(player) {
            return;
        }
        let Some(spell) = self.content.rules.spells.id_numbered(214)
        /* spell_of_return */
        else {
            return;
        };
        let cost = self.casting_cost(player, spell, false);
        self.begin_casting(player, spell, cost);
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
        Ok(self.defeat(player, winner, Fall::Resigned))
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
        let Some(pos) = self.ecs.get::<&Position>(city).ok().map(|p| *p) else {
            return;
        };
        let race = self
            .ecs
            .get::<&CityRace>(city)
            .map_or(crate::rules::RaceId::new(0), |r| r.0);
        let neutral = self.race_player(race);
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

    pub(crate) fn defeat(&mut self, loser: PlayerId, winner: PlayerId, fall: Fall) -> Vec<Event> {
        if let Some(mut w) = self.wizard_mut(loser) {
            defeat_player(&mut w);
            w.clear_casting();
        }
        if let Some(e) = self.player_entity(loser) {
            self.put(e, crate::player::Defeat { by: Some(winner) });
        }
        self.cities_go_neutral(loser);
        self.aftermath(loser);
        self.conquest_relations(winner, loser, true);
        let mut events = vec![Event::WizardDefeated { player: loser }];
        events.extend(self.take_spoils(loser, winner, fall));
        events
    }

    /// A city that the rules have decided is gone (an outpost that is lost, a city taken with no
    /// citizens, one a monster horde razes): [`World::destroy_city`], and what to do when it says
    /// no. A wizard's fortress city survives with a citizen, so no event. A city that has no owner
    /// or no square cannot be destroyed by the book and is removed all the same, so that none is
    /// left standing as a ghost; the event is there when it had a square to name.
    pub(crate) fn remove_city(&mut self, city: hecs::Entity) -> Vec<Event> {
        match self.destroy_city(city) {
            Ok(events) => events,
            Err(FortressError::CannotDestroyFortress) => Vec::new(),
            Err(_) => {
                let pos = self.ecs.get::<&Position>(city).map(|p| *p).ok();
                self.despawn_with_spells(city);
                pos.map(|pos| Event::CityDestroyed { pos })
                    .into_iter()
                    .collect()
            }
        }
    }

    /// Raze a city. A fortress city cannot be destroyed: it survives with at least one citizen.
    pub fn destroy_city(&mut self, city: hecs::Entity) -> Result<Vec<Event>, FortressError> {
        let owner = self
            .economy()
            .city_owner(city)
            .ok_or(FortressError::NotYourCity)?;
        if self.economy().is_fortress_city(city) {
            if let Ok(mut pop) = self.ecs.get::<&mut Population>(city) {
                pop.figures = pop.figures.max(1);
            }
            return Err(FortressError::CannotDestroyFortress);
        }
        // A city with no square cannot be reported or taken off the map by its square: refused
        // here, so no event ever names a made-up square.
        let pos = self
            .ecs
            .get::<&Position>(city)
            .map(|p| *p)
            .map_err(|_| FortressError::NotYourCity)?;
        self.despawn_with_spells(city);
        if let Some(mut w) = self.wizard_mut(owner) {
            circle_lost_to_destruction(&mut w, pos);
        }
        let mut events = vec![Event::CityDestroyed { pos }];
        if self.economy().city_count(owner) == 0 {
            events.extend(self.defeat(owner, owner, Fall::Defeated));
        }
        Ok(events)
    }
}
