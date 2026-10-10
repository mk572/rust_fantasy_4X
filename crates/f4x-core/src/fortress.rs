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

    fn defeat(&mut self, loser: PlayerId, winner: PlayerId) -> Vec<Event> {
        if let Some(p) = self.player_mut(loser) {
            p.defeated = true;
            p.banished = false;
            p.fortress = None;
            p.summon_at = None;
            p.magic.casting = None;
        }
        self.aftermath(loser, winner);
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
mod tests {
    use super::*;
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::rules::exec::Target;
    use crate::save::{LoadError, Snapshot};
    use crate::testutil::content_with_magic;
    use crate::{Command, Rejection};

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    fn world() -> Game {
        let c = content_with_magic();
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(3);
        g.content = c;
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        for id in [P0, P1] {
            let mut p = Player::new(id, "W");
            p.mana = 100;
            g.players.push(p);
        }
        g.init_wizard(P0, &[("life", 2)], &[]).unwrap();
        g
    }

    fn place(g: &mut Game, player: PlayerId, pos: Position) -> hecs::Entity {
        let race = g.content.rules.races.id_of("elf");
        let events = g.apply(Command::PlaceFortress {
            player,
            pos,
            name: "Capital".into(),
            race,
            population: 4,
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::FortressPlaced { .. })),
            "{events:?}"
        );
        g.city_at(pos).unwrap()
    }

    fn city(g: &mut Game, player: PlayerId, pos: Position) -> hecs::Entity {
        let race = g.content.rules.races.id_of("elf");
        g.apply(Command::FoundCity {
            owner: player,
            pos,
            name: "Town".into(),
            race,
            population: 3,
        });
        g.city_at(pos).unwrap()
    }

    fn spell(g: &Game, id: &str) -> crate::rules::SpellId {
        g.content.rules.spells.id_of(id).unwrap()
    }

    fn cast(g: &mut Game, id: &str, city: hecs::Entity) -> Vec<Event> {
        let s = spell(g, id);
        g.magic_mut_for_tests(P0).known.insert(s);
        g.magic_mut_for_tests(P0).skill_left = 500;
        g.player_mut(P0).unwrap().mana = 500;
        g.apply(Command::CastSpell {
            player: P0,
            spell: s,
            target: Some(Target::City(city)),
        })
    }

    #[test]
    fn placing_the_fortress_founds_the_capital_once() {
        let mut g = world();
        let capital = place(&mut g, P0, at(5, 5));
        let p = g.player(P0).unwrap();
        assert_eq!(p.fortress, Some(at(5, 5)));
        assert_eq!(p.summon_at, Some(at(5, 5)));
        assert_eq!(p.capital_race, g.content.rules.races.id_of("elf"));
        assert_eq!(g.fortress_city(P0), Some(capital));
        assert!(g.is_fortress_city(capital));
        // A second fortress, or one on a bad site, is refused.
        let again = g.apply(Command::PlaceFortress {
            player: P0,
            pos: at(12, 5),
            name: "x".into(),
            race: None,
            population: 1,
        });
        assert_eq!(
            again,
            vec![Event::Rejected(Rejection::Fortress(
                FortressError::NotAllowed
            ))]
        );
        // The fortress feeds the power base: one mana per book, plus the city's own.
        assert_eq!(g.power_base(P0), 2 + 2);
    }

    #[test]
    fn summoned_creatures_appear_at_the_circle() {
        let mut g = world();
        place(&mut g, P0, at(5, 5));
        let town = city(&mut g, P0, at(15, 5));
        let call = spell(&g, "call_spears");
        g.magic_mut_for_tests(P0).known.insert(call);
        g.magic_mut_for_tests(P0).skill_left = 99;
        // No target is needed: the creatures come to the circle.
        assert!(!g.spell_needs_target(P0, call));
        g.apply(Command::CastSpell {
            player: P0,
            spell: call,
            target: None,
        });
        assert_eq!(g.units_at(at(5, 5)).len(), 2);
        // Move the circle with the spell, and they come there instead.
        let events = cast(&mut g, "summoning_circle", town);
        assert!(events.contains(&Event::SummoningCircleMoved {
            player: P0,
            pos: at(15, 5)
        }));
        assert_eq!(
            g.player(P0).unwrap().fortress,
            Some(at(5, 5)),
            "fortress stays"
        );
        g.magic_mut_for_tests(P0).skill_left = 99;
        g.player_mut(P0).unwrap().mana = 500;
        g.apply(Command::CastSpell {
            player: P0,
            spell: call,
            target: None,
        });
        assert_eq!(g.units_at(at(15, 5)).len(), 2);
    }

    #[test]
    fn the_fortress_spells_check_their_target_before_charging() {
        let mut g = world();
        let capital = place(&mut g, P0, at(5, 5));
        let foreign = place(&mut g, P1, at(15, 5));
        let town = city(&mut g, P0, at(10, 9));
        for (id, bad) in [
            ("move_fortress", foreign),
            ("move_fortress", capital),
            ("summoning_circle", foreign),
            ("spell_of_return", town),
        ] {
            let events = cast(&mut g, id, bad);
            assert!(
                matches!(events.as_slice(), [Event::Rejected(Rejection::Magic(_))]),
                "{id}: {events:?}"
            );
        }
        assert_eq!(g.player(P0).unwrap().mana, 500, "nothing was paid");
        // A legal move works, and Myrror adds five to the fortress's power.
        let events = cast(&mut g, "move_fortress", town);
        assert!(events.contains(&Event::FortressMoved {
            player: P0,
            pos: at(10, 9)
        }));
        assert_eq!(g.fortress_city(P0), Some(town));
        assert_eq!(g.player(P0).unwrap().summon_at, Some(at(10, 9)));
        let myrror = Position {
            plane: Plane::Myrror,
            x: 3,
            y: 3,
        };
        let far = city(&mut g, P0, myrror);
        let before = g.power_base(P0);
        cast(&mut g, "move_fortress", far);
        assert_eq!(g.power_base(P0), before + 5, "plus five on Myrror");
    }

    #[test]
    fn taking_an_ordinary_city_changes_its_owner_only() {
        let mut g = world();
        place(&mut g, P0, at(5, 5));
        let town = city(&mut g, P0, at(15, 5));
        let events = g.capture_city(town, P1);
        assert_eq!(
            events,
            vec![Event::CityCaptured {
                city: town,
                from: P0,
                to: P1
            }]
        );
        assert_eq!(g.city_owner(town), Some(P1));
        assert!(!g.player(P0).unwrap().banished);
    }

    #[test]
    fn losing_the_fortress_banishes_and_the_spell_of_return_brings_the_wizard_back() {
        let mut g = world();
        let capital = place(&mut g, P0, at(5, 5));
        let town = city(&mut g, P0, at(15, 5));
        let w = g.player(P1).unwrap().clone();
        let events = g.capture_city(capital, P1);
        assert!(
            events.contains(&Event::WizardBanished { player: P0, by: P1 }),
            "{events:?}"
        );
        let p = g.player(P0).unwrap();
        assert!(p.banished);
        assert_eq!((p.fortress, p.summon_at), (None, None));
        assert_eq!(p.mana, 50, "half the mana goes to the captor");
        assert_eq!(g.player(P1).unwrap().mana, w.mana + 50);
        assert_eq!(g.player(P1).unwrap().fame, w.fame + 5);
        // Casting the Spell of Return has begun, with no income of any kind meanwhile.
        let ret = spell(&g, "spell_of_return");
        assert_eq!(g.magic(P0).unwrap().casting.unwrap().spell, ret);
        assert_eq!(g.power_split(P0), crate::magic::PowerSplit::default());
        assert_eq!(
            g.cancel_cast(P0),
            Err(crate::magic::MagicError::NotAvailable)
        );
        assert_eq!(
            g.start_cast(P0, ret, Some(Target::City(town))),
            Err(crate::magic::MagicError::NotAvailable)
        );
        // It is paid over the following turns, then needs a city.
        let mut ready = false;
        for _ in 0..20 {
            let events = g.apply(Command::EndTurn);
            if events.contains(&Event::SpellReady {
                player: P0,
                spell: ret,
            }) {
                ready = true;
                break;
            }
        }
        assert!(ready, "the Spell of Return completes");
        let events = g.apply(Command::ResolveCast {
            player: P0,
            target: Target::City(town),
        });
        assert!(
            events.contains(&Event::WizardReturned {
                player: P0,
                pos: at(15, 5)
            }),
            "{events:?}"
        );
        let p = g.player(P0).unwrap();
        assert!(!p.banished);
        assert_eq!(p.fortress, Some(at(15, 5)));
        assert_eq!(p.summon_at, Some(at(15, 5)));
        assert_eq!(g.magic(P0).unwrap().casting, None);
    }

    #[test]
    fn losing_the_last_city_defeats_the_wizard() {
        let mut g = world();
        let capital = place(&mut g, P0, at(5, 5));
        let events = g.capture_city(capital, P1);
        assert!(events.contains(&Event::WizardDefeated { player: P0 }));
        let p = g.player(P0).unwrap();
        assert!(p.defeated && !p.banished);
        assert_eq!(p.magic.casting, None);
        // A defeated wizard cannot place a new fortress.
        let r = g.place_fortress(P0, at(9, 9), "x".into(), None, 1);
        assert_eq!(r, Err(FortressError::NotAllowed));
    }

    #[test]
    fn a_fortress_city_cannot_be_destroyed() {
        let mut g = world();
        let capital = place(&mut g, P0, at(5, 5));
        let town = city(&mut g, P0, at(15, 5));
        g.world.get::<&mut Population>(capital).unwrap().figures = 0;
        assert_eq!(
            g.destroy_city(capital),
            Err(FortressError::CannotDestroyFortress)
        );
        assert_eq!(g.world.get::<&Population>(capital).unwrap().figures, 1);
        // Destroying the city that held the circle sends it back to the fortress.
        g.move_summoning_circle(P0, town).unwrap();
        let events = g.destroy_city(town).unwrap();
        assert_eq!(events, vec![Event::CityDestroyed { pos: at(15, 5) }]);
        assert!(g.city_at(at(15, 5)).is_none());
        assert_eq!(g.player(P0).unwrap().summon_at, Some(at(5, 5)));
    }

    #[test]
    fn saves_keep_the_fortress_and_reject_a_misplaced_one() {
        let mut g = world();
        let capital = place(&mut g, P0, at(5, 5));
        city(&mut g, P0, at(15, 5));
        g.capture_city(capital, P1);
        let snap = g.snapshot();
        let loaded = Snapshot::from_text(&snap.to_text().unwrap())
            .unwrap()
            .restore(content_with_magic())
            .unwrap();
        assert_eq!(loaded.snapshot(), g.snapshot());
        assert!(loaded.player(P0).unwrap().banished);
        // A fortress where the wizard has no city is refused.
        let mut bad = g.snapshot();
        bad.players[1].fortress = Some(at(9, 9));
        let err = bad.restore(content_with_magic()).err().unwrap();
        assert!(matches!(err, LoadError::Inconsistent(_)), "{err:?}");
    }

    #[test]
    fn taking_the_last_rivals_last_city_wins_by_conquest() {
        let mut g = world();
        let capital = place(&mut g, P0, at(5, 5));
        g.player_mut(P0).unwrap().gold = 90;
        let events = g.capture_city(capital, P1);
        assert!(events.contains(&Event::GameOver {
            winner: P1,
            kind: crate::endgame::EndKind::Conquest,
            turn: 1
        }));
        assert_eq!(g.player(P0).unwrap().defeated_by, Some(P1));
        // Half the loser's gold goes to the winner.
        assert_eq!(g.player(P0).unwrap().gold, 45);
        assert_eq!(g.player(P1).unwrap().gold, 145);
        assert_eq!(
            g.apply(Command::EndTurn),
            vec![Event::Rejected(crate::Rejection::GameOver)]
        );
        // The winner scores 50 for the conquest.
        assert_eq!(g.score(P1).conquests, 50);
        assert_eq!(g.score(P0).conquests, 0);
    }

    #[test]
    fn the_game_goes_on_while_two_wizards_stand_unless_the_human_falls() {
        let mut g = world();
        g.players.push(Player::new(PlayerId(2), "Third"));
        let capital = place(&mut g, P1, at(5, 5));
        let events = g.capture_city(capital, P0);
        assert!(events.contains(&Event::WizardDefeated { player: P1 }));
        assert!(g.game_over.is_none(), "wizards 0 and 2 still stand");

        let mut g = world();
        g.players.push(Player::new(PlayerId(2), "Third"));
        g.human = Some(P0);
        let capital = place(&mut g, P0, at(5, 5));
        let events = g.capture_city(capital, P1);
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::GameOver { winner, .. } if *winner == P1)));
    }

    #[test]
    fn finishing_the_spell_of_mastery_wins_and_scores() {
        let mut g = world();
        let capital = place(&mut g, P0, at(5, 5));
        g.player_mut(P0).unwrap().fame = 10;
        let s = spell(&g, "spell_of_mastery");
        g.magic_mut_for_tests(P0).known.insert(s);
        g.magic_mut_for_tests(P0).skill_left = 9000;
        g.player_mut(P0).unwrap().mana = 9000;
        let _ = capital;
        let events = g.apply(Command::CastSpell {
            player: P0,
            spell: s,
            target: None,
        });
        assert!(
            events.contains(&Event::GameOver {
                winner: P0,
                kind: crate::endgame::EndKind::SpellOfMastery,
                turn: 1
            }),
            "{events:?}"
        );
        let score = g.score(P0);
        assert_eq!(score.mastery, 250);
        assert_eq!(score.fame, 20);
        // The rival still stood, so it counts as beaten.
        assert_eq!(score.conquests, 50);
        assert_eq!(score.time, 2000 - 2);
        assert_eq!(score.townsfolk, 2);
        assert_eq!(score.spells, 2);
        assert_eq!(score.total(), 2 + 2 + 50 + 20 + 1998 + 250);
        // The rival gets no time bonus: the mastery winner is someone else.
        assert_eq!(g.score(P1).time, 0);
    }

    #[test]
    fn a_defeat_clears_the_losers_enchantments_and_nodes() {
        let mut g = world();
        let capital = place(&mut g, P0, at(5, 5));
        let town = city(&mut g, P1, at(15, 5));
        let ward = spell(&g, "move_fortress");
        g.world
            .insert_one(town, crate::economy::Enchantments(vec![(ward, P0)]))
            .unwrap();
        g.player_mut(P0).unwrap().magic.global.push(ward);
        g.capture_city(capital, P1);
        assert!(g
            .world
            .get::<&crate::economy::Enchantments>(town)
            .unwrap()
            .0
            .is_empty());
        assert!(g.player(P0).unwrap().magic.global.is_empty());
    }

    #[test]
    fn the_end_of_the_game_is_saved() {
        let mut g = world();
        g.human = Some(P0);
        let capital = place(&mut g, P0, at(5, 5));
        g.capture_city(capital, P1);
        let loaded = Snapshot::from_text(&g.snapshot().to_text().unwrap())
            .unwrap()
            .restore(content_with_magic())
            .unwrap();
        assert_eq!(loaded.game_over, g.game_over);
        assert_eq!(loaded.human, Some(P0));
        assert_eq!(loaded.player(P0).unwrap().defeated_by, Some(P1));
    }
}
