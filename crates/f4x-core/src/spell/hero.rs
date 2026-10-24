//! Getting heroes into the army (task 48): hiring with gold, and the four spells that bring a
//! hero in: Summon Hero, Summon Champion, Incarnation and Resurrection. A hero always appears at
//! the wizard's summoning circle. The hero types and the entities are in `hero/kind.rs` and
//! `hero/game.rs`.
//!
//! Resurrection needs the wizard to choose which dead hero returns: the hero is the spell's
//! target (`Target::Hero`), given with the cast when it takes effect at once, or when the cost
//! has been paid over turns (`Command::ResolveCast`), as for any spell that wants a target.

use crate::game::rng::GameRng;
use crate::game::{Event, Game};
use crate::hero::kind::{self, Class, Standing, HEROES};
use crate::hero::{HeroError, HeroState};
use crate::player::PlayerId;
use crate::unit::UnitView;

pub const SUMMON_HERO: &str = "summon_hero";
pub const SUMMON_CHAMPION: &str = "summon_champion";
pub const INCARNATION: &str = "incarnation";
pub const RESURRECTION: &str = "resurrection";

/// Torin the Chosen, the last hero type: only Incarnation brings him.
pub fn torin(rules: &crate::rules::Rules) -> usize {
    rules.hero_templates.len().saturating_sub(1)
}

/// The hooks that bring a hero in.
pub const HOOKS: [&str; 4] = [SUMMON_HERO, SUMMON_CHAMPION, INCARNATION, RESURRECTION];

impl UnitView<'_> {
    /// Could a summoning of this class find a hero for the wizard? (`Pick_Random_Hero` with a
    /// zero cost: fame does not matter, the books do; a champion summoning falls back on any.)
    pub fn hero_summonable(&self, player: PlayerId, class: Class) -> bool {
        let states = self.hero_states();
        let standing = Standing {
            fame: 200,
            ..self.hero_standing(player)
        };
        let rules = &self.content.rules;
        let mut range = match class {
            Class::Hero => 0..HEROES,
            Class::Champion | Class::Any => 0..torin(rules),
        };
        range.any(|k| kind::can_offer(rules, &states, k, &standing))
    }

    /// The dead heroes Resurrection can raise, whoever's they were: every dead one but Torin, whom only Incarnation
    /// brings back (`Cast_Resurrection` lists types 0 to 33).
    pub fn raisable_heroes(&self) -> Vec<usize> {
        let last = torin(&self.content.rules);
        self.hero_states()
            .into_iter()
            .enumerate()
            .filter(|&(k, state)| state == HeroState::Dead && k != last)
            .map(|(k, _)| k)
            .collect()
    }

    /// The hero a computer wizard raises: the last of the raisable ones, as the original does.
    pub fn ai_resurrection_pick(&self) -> Option<usize> {
        self.raisable_heroes().last().copied()
    }

    /// Why the wizard cannot bring a hero in by this hook now, if so: no summoning
    /// circle, or nobody to bring (Incarnation: Torin is already serving or
    /// gone). `Resurrection` is checked against the hero chosen, not here.
    pub fn hero_spell_blocked(&self, player: PlayerId, hook: &str) -> Option<HeroError> {
        let states = self.hero_states();
        if crate::wizard::entity::find(self.world, player)
            .is_none_or(|w| w.summon_point().is_none())
        {
            return Some(HeroError::NoCircle);
        }
        let record = |k: usize| states.get(k).copied();
        let possible = match hook {
            SUMMON_HERO => self.hero_summonable(player, Class::Hero),
            SUMMON_CHAMPION => self.hero_summonable(player, Class::Champion),
            INCARNATION => matches!(
                record(torin(&self.content.rules)),
                Some(HeroState::Free | HeroState::Dead)
            ),
            _ => true,
        };
        (!possible).then_some(HeroError::NotAvailable)
    }
}

impl Game {
    /// Hire the free hero `kind` for gold: it joins at the summoning circle.
    pub fn hire_hero(&mut self, player: PlayerId, kind: usize) -> Result<hecs::Entity, HeroError> {
        let hero = self.units().hero_entity(kind).ok_or(HeroError::Unknown)?;
        if self.units().hero_state(hero) != Some(HeroState::Free) {
            return Err(HeroError::NotAvailable);
        }
        let price = self
            .units()
            .hero_hire_price(player, kind)
            .ok_or(HeroError::Unknown)?;
        if self.player(player).map_or(0, |p| p.treasury().gold) < price {
            return Err(HeroError::NoGold);
        }
        let unit = self.arrive(player, kind)?;
        if let Some(t) = self.treasury_mut(player) {
            t.gold -= price;
        }
        Ok(unit)
    }

    /// Bring the hero `kind` in at the summoning circle, with nothing charged.
    pub(crate) fn arrive(
        &mut self,
        player: PlayerId,
        kind: usize,
    ) -> Result<hecs::Entity, HeroError> {
        let at = self.summon_point(player).ok_or(HeroError::NoCircle)?;
        self.spawn_hero(player, at, kind)
    }

    /// Run one of the hero hooks for a spell that has been paid for. `None` if `hook` is not
    /// one of them. A summoning that finds nobody (the roster changed while the cost was paid
    /// over turns) is lost.
    ///
    /// The cast has lent the game's generator to the effect host, so the pick draws from `rng`.
    pub(crate) fn hero_hook(
        &mut self,
        hook: &str,
        player: PlayerId,
        who: crate::rules::exec::Target,
        rng: &mut GameRng,
    ) -> Option<Vec<Event>> {
        let spell = self.content.rules.spells.id_of(hook)?;
        let standing = self.units().hero_standing(player);
        let states = self.units().hero_states();
        let mut pick =
            |class| kind::pick(rng, &self.content.rules, &states, &standing, true, class);
        let kind = match hook {
            SUMMON_HERO => pick(Class::Hero),
            SUMMON_CHAMPION => pick(Class::Champion),
            INCARNATION => Some(torin(&self.content.rules)),
            RESURRECTION => {
                // Nobody to raise if the hero is not dead any more: the spell is lost, with a warning.
                let dead = self.units().raisable_heroes();
                match who {
                    crate::rules::exec::Target::Hero(k) if dead.contains(&k) => Some(k),
                    _ => None,
                }
            }
            _ => return None,
        };
        let joined = kind.and_then(|k| self.arrive(player, k).ok().map(|u| (u, k)));
        Some(match joined {
            Some((unit, kind)) => vec![Event::HeroJoined { player, unit, kind }],
            None => vec![Event::HeroSpellFailed { player, spell }],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Command;
    use crate::hero::game::tests::{at, set_state, state, world, P0, P1};
    use crate::hero::Fate;
    use crate::map::Plane;
    use crate::unit::Experience;

    /// The hero world, with the four spells known to wizard 0, a circle at (3, 3), 100 gold
    /// and plenty of mana and skill. The last of the seven hero types is "Torin".
    fn caster() -> Game {
        let mut g = world();
        g.init_wizard(P0, &[("life", 3)], &[], None).unwrap();
        for id in HOOKS {
            let s = g.content.rules.spells.id_of(id).unwrap();
            let mut m = g.wizard_mut(P0).unwrap();
            m.get_mut::<crate::wizard::sheet::KnownSpells>()
                .researchable
                .remove(&s);
            m.get_mut::<crate::wizard::sheet::Candidates>()
                .0
                .retain(|c| *c != s);
            if m.get_mut::<crate::wizard::sheet::Research>().spell == Some(s) {
                m.get_mut::<crate::wizard::sheet::Research>().spell = None;
            }
            m.get_mut::<crate::wizard::sheet::KnownSpells>()
                .known
                .insert(s);
        }
        g.wizard_mut(P0)
            .unwrap()
            .get_mut::<crate::wizard::sheet::Skill>()
            .left = 99_999;
        let t = g.treasury_mut(P0).unwrap();
        t.mana = 1000;
        t.gold = 100;
        let mut w = g.wizard_mut(P0).unwrap();
        w.put(Some(crate::wizard::fortress::Fortress(at(3, 3))));
        w.put(Some(crate::wizard::fortress::SummonAt(at(3, 3))));
        g
    }

    fn cast(g: &mut Game, hook: &str) -> Vec<Event> {
        let spell = g.content.rules.spells.id_of(hook).unwrap();
        g.apply(Command::CastSpell {
            player: P0,
            spell,
            target: None,
        })
    }

    fn joined(events: &[Event]) -> Option<(hecs::Entity, usize)> {
        events.iter().find_map(|e| match e {
            Event::HeroJoined { unit, kind, .. } => Some((*unit, *kind)),
            _ => None,
        })
    }

    #[test]
    fn hiring_charges_the_price_and_the_hero_appears_at_the_circle() {
        let mut g = caster();
        let h = g.hire_hero(P0, 0).unwrap();
        assert_eq!(g.player(P0).unwrap().treasury().gold, 0);
        assert_eq!(g.units().unit_position(h), Some(at(3, 3)));
        assert_eq!(g.units().hero_kind(h), Some(0));
        // Not twice, not by the other wizard while it serves, not without the gold, not without
        // a circle.
        assert_eq!(g.hire_hero(P0, 0), Err(HeroError::NotAvailable));
        assert_eq!(g.hire_hero(P1, 0), Err(HeroError::NotAvailable));
        assert_eq!(g.hire_hero(P0, 1), Err(HeroError::NoGold));
        assert_eq!(g.hire_hero(P0, 99), Err(HeroError::Unknown));
        g.treasury_mut(P0).unwrap().gold = 500;
        g.wizard_mut(P0)
            .unwrap()
            .put::<crate::wizard::fortress::SummonAt>(None);
        g.wizard_mut(P0)
            .unwrap()
            .put::<crate::wizard::fortress::Fortress>(None);
        assert_eq!(g.hire_hero(P0, 1), Err(HeroError::NoCircle));
        assert_eq!(g.player(P0).unwrap().treasury().gold, 500);
        assert!(g.units().heroes(P1).is_empty());
    }

    #[test]
    fn a_dead_hero_is_not_hired_and_there_is_no_limit_on_how_many_serve() {
        let mut g = caster();
        g.treasury_mut(P0).unwrap().gold = 5000;
        let first = g.hire_hero(P0, 0).unwrap();
        g.remove_unit(first, Fate::Slain);
        assert_eq!(g.hire_hero(P0, 0), Err(HeroError::NotAvailable));
        for k in 1..=6 {
            g.hire_hero(P0, k).unwrap();
        }
        assert_eq!(g.units().hero_units(P0).len(), 6);
        assert_eq!(g.units().raisable_heroes(), vec![0]);
    }

    #[test]
    fn summon_hero_brings_a_hero_for_mana_only() {
        let mut g = caster();
        let events = cast(&mut g, SUMMON_HERO);
        let (unit, kind) = joined(&events).unwrap_or_else(|| panic!("{events:?}"));
        assert!(kind < HEROES);
        assert_eq!(g.units().unit_position(unit), Some(at(3, 3)));
        assert_eq!(g.player(P0).unwrap().treasury().gold, 100);
        assert!(g.player(P0).unwrap().treasury().mana < 1000);
        assert_eq!(state(&g, kind), HeroState::Serving);
        // The necromancer (a death book) is not among those a life wizard can summon.
        for _ in 0..5 {
            if let Some((_, k)) = joined(&cast(&mut g, SUMMON_HERO)) {
                assert_ne!(k, 6);
            }
        }
    }

    #[test]
    fn summoning_is_refused_when_nobody_is_left_or_without_a_circle_and_costs_nothing() {
        let mut g = caster();
        g.treasury_mut(P0).unwrap().gold = 5000;
        // Every hero serves: nobody is left to summon.
        for k in 0..=torin(&g.content.rules) {
            g.hire_hero(P0, k).unwrap();
        }
        let mana = g.player(P0).unwrap().treasury().mana;
        for hook in [SUMMON_HERO, SUMMON_CHAMPION, INCARNATION] {
            let events = cast(&mut g, hook);
            assert!(
                matches!(events.as_slice(), [Event::Rejected(_)]),
                "{hook}: {events:?}"
            );
        }
        assert_eq!(g.player(P0).unwrap().treasury().mana, mana);
        let h = g.units().hero_units(P0)[0];
        g.remove_unit(h, Fate::Released);
        g.wizard_mut(P0)
            .unwrap()
            .put::<crate::wizard::fortress::SummonAt>(None);
        g.wizard_mut(P0)
            .unwrap()
            .put::<crate::wizard::fortress::Fortress>(None);
        assert!(matches!(
            cast(&mut g, SUMMON_HERO).as_slice(),
            [Event::Rejected(_)]
        ));
        assert_eq!(g.player(P0).unwrap().treasury().mana, mana);
    }

    #[test]
    fn summon_champion_is_refused_when_nobody_is_left() {
        let mut g = caster();
        // The test roster has no champion; "any" finds the free ones until none remain.
        for k in 0..6 {
            set_state(&mut g, k, HeroState::Gone);
        }
        let events = cast(&mut g, SUMMON_CHAMPION);
        assert!(
            matches!(events.as_slice(), [Event::Rejected(_)]),
            "{events:?}"
        );
    }

    #[test]
    fn incarnation_brings_torin_back_with_his_experience() {
        let mut g = caster();
        let last = torin(&g.content.rules);
        assert_eq!(last, 6);
        let events = cast(&mut g, INCARNATION);
        let (unit, kind) = joined(&events).unwrap_or_else(|| panic!("{events:?}"));
        assert_eq!(kind, last);
        let _ = g.world.insert_one(unit, Experience(120));
        // There is only one Torin at a time.
        assert!(matches!(
            cast(&mut g, INCARNATION).as_slice(),
            [Event::Rejected(_)]
        ));
        g.remove_unit(unit, Fate::Slain);
        let (again, _) = joined(&cast(&mut g, INCARNATION)).unwrap();
        assert_eq!(g.world.get::<&Experience>(again).unwrap().0, 120);
        // Not once he is gone for good.
        g.remove_unit(again, Fate::Gone);
        assert!(matches!(
            cast(&mut g, INCARNATION).as_slice(),
            [Event::Rejected(_)]
        ));
    }

    fn raise(g: &mut Game, kind: usize) -> Vec<Event> {
        let spell = g.content.rules.spells.id_of(RESURRECTION).unwrap();
        g.apply(Command::CastSpell {
            player: P0,
            spell,
            target: Some(crate::rules::exec::Target::Hero(kind)),
        })
    }

    #[test]
    fn resurrection_raises_the_chosen_dead_hero() {
        let mut g = caster();
        g.treasury_mut(P0).unwrap().gold = 5000;
        let a = g.hire_hero(P0, 0).unwrap();
        let b = g.hire_hero(P0, 1).unwrap();
        let _ = g.world.insert_one(b, Experience(75));
        g.remove_unit(a, Fate::Slain);
        g.remove_unit(b, Fate::Slain);
        // The spell is not cast at once without saying whom.
        assert!(matches!(
            cast(&mut g, RESURRECTION).as_slice(),
            [Event::Rejected(_)]
        ));
        let mana = g.player(P0).unwrap().treasury().mana;
        let events = raise(&mut g, 1);
        let (unit, kind) = joined(&events).unwrap_or_else(|| panic!("{events:?}"));
        assert_eq!(kind, 1);
        assert_eq!(g.world.get::<&Experience>(unit).unwrap().0, 75);
        assert!(g.player(P0).unwrap().treasury().mana < mana);
        assert_eq!(g.units().raisable_heroes(), vec![0]);
        // A hero who is not dead, or never was, cannot be raised.
        for kind in [1, 5, 99] {
            let events = raise(&mut g, kind);
            assert!(
                matches!(events.as_slice(), [Event::Rejected(_)]),
                "{kind}: {events:?}"
            );
        }
    }

    #[test]
    fn resurrection_picks_its_hero_when_the_cost_is_paid_and_survives_a_save() {
        let mut g = caster();
        g.treasury_mut(P0).unwrap().gold = 5000;
        let a = g.hire_hero(P0, 3).unwrap();
        g.remove_unit(a, Fate::Slain);
        let mut skill = g.wizard_mut(P0).unwrap();
        skill.get_mut::<crate::wizard::sheet::Skill>().left = 0;
        skill.get_mut::<crate::wizard::sheet::Skill>().points = 0;
        // Started with no hero named: the cost is paid over turns first.
        let spell = g.content.rules.spells.id_of(RESURRECTION).unwrap();
        let events = cast(&mut g, RESURRECTION);
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
        assert_eq!(
            g.wizard(P0).unwrap().casting().map(|c| c.spell),
            Some(spell)
        );
        // (The test world has no city for the fortress to stand in, which a save refuses.)
        g.wizard_mut(P0)
            .unwrap()
            .put::<crate::wizard::fortress::Fortress>(None);
        g.wizard_mut(P0)
            .unwrap()
            .put::<crate::wizard::fortress::SummonAt>(None);
        let text = ron::to_string(&g.snapshot()).unwrap();
        let mut back = crate::game::save::Snapshot::from_text(&text)
            .unwrap()
            .restore(g.content.clone())
            .unwrap();
        back.wizard_mut(P0)
            .unwrap()
            .put(Some(crate::wizard::fortress::SummonAt(at(3, 3))));
        assert_eq!(
            back.wizard(P0).unwrap().casting().map(|c| c.spell),
            Some(spell)
        );
        // The cost is paid; the hero is named then.
        back.wizard_mut(P0).unwrap().cancel_casting();
        back.begin_casting(P0, spell, 0);
        let wrong = back.apply(Command::ResolveCast {
            player: P0,
            target: crate::rules::exec::Target::Hero(5),
        });
        assert!(
            matches!(wrong.as_slice(), [Event::Rejected(_)]),
            "{wrong:?}"
        );
        let events = back.apply(Command::ResolveCast {
            player: P0,
            target: crate::rules::exec::Target::Hero(3),
        });
        assert_eq!(joined(&events).map(|(_, k)| k), Some(3), "{events:?}");
    }

    #[test]
    fn resurrection_warns_when_it_is_paid_and_no_hero_lies_dead() {
        let mut g = caster();
        let spell = g.content.rules.spells.id_of(RESURRECTION).unwrap();
        assert!(!g.spell_has_target(P0, spell));
        let mut skill = g.wizard_mut(P0).unwrap();
        skill.get_mut::<crate::wizard::sheet::Skill>().left = 0;
        skill.get_mut::<crate::wizard::sheet::Skill>().points = 0;
        let started = cast(&mut g, RESURRECTION);
        assert!(
            !started.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{started:?}"
        );
        // The turn ends with the cast still on and nobody to raise: the wizard is asked.
        let events = g.apply(Command::EndTurn);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::NoTargetForCast { .. })),
            "{events:?}"
        );
        // Torin is raised by Incarnation only; a computer wizard raises the last dead hero.
        let mut g = caster();
        g.treasury_mut(P0).unwrap().gold = 5000;
        for k in [1, 4, 6] {
            let h = g.hire_hero(P0, k).unwrap();
            g.remove_unit(h, Fate::Slain);
        }
        assert_eq!(g.units().raisable_heroes(), vec![1, 4]);
        assert_eq!(g.units().ai_resurrection_pick(), Some(4));
        assert!(matches!(raise(&mut g, 6).as_slice(), [Event::Rejected(_)]));
        let _ = (P1, Plane::Arcanus);
    }
}
