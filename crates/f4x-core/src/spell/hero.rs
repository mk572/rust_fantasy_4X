//! Getting heroes into the army: hiring with gold, and the four spells that bring a
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
use crate::player::offer::Offer;
use crate::player::PlayerId;
use crate::rules::SpellId;
use crate::unit::UnitView;

/// The hook of the spells that bring a free hero in: Summon Hero, Summon Champion, Incarnation.
pub const SUMMON_HERO: &str = "summon_hero";

crate::hook::effect_enum! {
    /// Which hero a spell with the `summon_hero` hook brings: the argument of the hook.
    pub enum HeroSummon in SUMMON_HERO;
    Hero = "hero",
    Champion = "champion",
    Torin = "torin",
}

impl HeroSummon {
    /// What the spell summons, if it has the hook.
    pub fn of(def: &crate::rules::SpellDef) -> Option<HeroSummon> {
        HeroSummon::named(def.hook_args(SUMMON_HERO)?.first()?)
    }
}

/// Torin the Chosen, the last hero type: only Incarnation brings him.
pub fn torin(rules: &crate::rules::Rules) -> usize {
    rules.hero_templates.len().saturating_sub(1)
}

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
    pub fn hero_spell_blocked(&self, player: PlayerId, spell: SpellId) -> Option<HeroError> {
        let states = self.hero_states();
        if crate::wizard::entity::find(self.world, player)
            .is_none_or(|w| w.summon_point().is_none())
        {
            return Some(HeroError::NoCircle);
        }
        let record = |k: usize| states.get(k).copied();
        let possible = match HeroSummon::of(self.content.rules.spells.get(spell)) {
            Some(HeroSummon::Hero) => self.hero_summonable(player, Class::Hero),
            Some(HeroSummon::Champion) => self.hero_summonable(player, Class::Champion),
            Some(HeroSummon::Torin) => matches!(
                record(torin(&self.content.rules)),
                Some(HeroState::Free | HeroState::Dead)
            ),
            None => true,
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
        spell: SpellId,
        player: PlayerId,
        who: crate::rules::exec::Target,
        rng: &mut GameRng,
    ) -> Option<Vec<Event>> {
        let standing = self.units().hero_standing(player);
        let states = self.units().hero_states();
        let summon = HeroSummon::of(self.content.rules.spells.get(spell));
        let mut pick =
            |class| kind::pick(rng, &self.content.rules, &states, &standing, true, class);
        let kind = match summon {
            Some(HeroSummon::Hero) => pick(Class::Hero),
            Some(HeroSummon::Champion) => pick(Class::Champion),
            Some(HeroSummon::Torin) => Some(torin(&self.content.rules)),
            None if spell.num() == 133 /* resurrection */ => {
                // Nobody to raise if the hero is not dead any more: the spell is lost, with a warning.
                let dead = self.units().raisable_heroes();
                match who {
                    crate::rules::exec::Target::Hero(k) if dead.contains(&k) => Some(k),
                    _ => None,
                }
            }
            None => return None,
        };
        // The human wizard is asked to accept a summoned hero (`Hire_Hero_Popup`, types 1 and 3).
        if matches!(summon, Some(HeroSummon::Hero | HeroSummon::Champion))
            && self.human() == Some(player)
        {
            if let Some(kind) = kind {
                let champion = summon == Some(HeroSummon::Champion);
                if let Some(o) = self.player_component_mut::<crate::player::Offers>(player) {
                    o.0.push(Offer::Summoned { kind, champion });
                    return Some(vec![Event::HeroSummoned { player, kind }]);
                }
            }
        }
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
    use crate::rules::RealmId;
    use crate::unit::Experience;
    use f4x_def::testdir::TestResult;

    /// The hero world, with the four spells known to wizard 0, a circle at (3, 3), 100 gold
    /// and plenty of mana and skill. The last of the seven hero types is "Torin".
    fn caster() -> Game {
        let mut g = world();
        g.init_wizard(P0, &[(RealmId::new(3 /* life */), 3)], &[], None)
            .expect("init_wizard succeeds");
        for number in [208, 212, 142, 133] {
            // summon_hero, summon_champion, incarnation, resurrection
            let s = SpellId::new(number);
            let mut m = g.wizard_mut(P0).expect("wizard_mut succeeds");
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
            .expect("value succeeds")
            .get_mut::<crate::wizard::sheet::Skill>()
            .left = 99_999;
        let t = g.treasury_mut(P0).expect("treasury_mut succeeds");
        t.mana = 1000;
        t.gold = 100;
        let mut w = g.wizard_mut(P0).expect("wizard_mut succeeds");
        w.put(Some(crate::wizard::fortress::Fortress(at(3, 3))));
        w.put(Some(crate::wizard::fortress::SummonAt(at(3, 3))));
        g
    }

    fn cast(g: &mut Game, number: u16) -> Vec<Event> {
        let spell = SpellId::new(number);
        g.apply(Command::CastSpell {
            extra: 0,
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
    fn hiring_charges_the_price_and_the_hero_appears_at_the_circle() -> TestResult {
        let mut g = caster();
        let h = g.hire_hero(P0, 0).map_err(|e| format!("{e:?}"))?;
        assert_eq!(g.player(P0).ok_or("player is missing")?.treasury().gold, 0);
        assert_eq!(g.units().unit_position(h), Some(at(3, 3)));
        assert_eq!(g.units().hero_kind(h), Some(0));
        // Not twice, not by the other wizard while it serves, not without the gold, not without
        // a circle.
        assert_eq!(g.hire_hero(P0, 0), Err(HeroError::NotAvailable));
        assert_eq!(g.hire_hero(P1, 0), Err(HeroError::NotAvailable));
        assert_eq!(g.hire_hero(P0, 1), Err(HeroError::NoGold));
        assert_eq!(g.hire_hero(P0, 99), Err(HeroError::Unknown));
        g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold = 500;
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .put::<crate::wizard::fortress::SummonAt>(None);
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .put::<crate::wizard::fortress::Fortress>(None);
        assert_eq!(g.hire_hero(P0, 1), Err(HeroError::NoCircle));
        assert_eq!(
            g.player(P0).ok_or("player is missing")?.treasury().gold,
            500
        );
        assert!(g.units().heroes(P1).is_empty());
        Ok(())
    }

    #[test]
    fn a_dead_hero_is_not_hired_and_there_is_no_limit_on_how_many_serve() -> TestResult {
        let mut g = caster();
        g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold = 5000;
        let first = g.hire_hero(P0, 0).map_err(|e| format!("{e:?}"))?;
        g.remove_unit(first, Fate::Slain);
        assert_eq!(g.hire_hero(P0, 0), Err(HeroError::NotAvailable));
        for k in 1..=6 {
            g.hire_hero(P0, k).map_err(|e| format!("{e:?}"))?;
        }
        assert_eq!(g.units().hero_units(P0).len(), 6);
        assert_eq!(g.units().raisable_heroes(), vec![0]);
        Ok(())
    }

    #[test]
    fn summon_hero_brings_a_hero_for_mana_only() -> TestResult {
        let mut g = caster();
        let events = cast(&mut g, 208 /* summon_hero */);
        let (unit, kind) = joined(&events).ok_or_else(|| format!("{events:?}"))?;
        assert!(kind < HEROES);
        assert_eq!(g.units().unit_position(unit), Some(at(3, 3)));
        assert_eq!(
            g.player(P0).ok_or("player is missing")?.treasury().gold,
            100
        );
        assert!(g.player(P0).ok_or("player is missing")?.treasury().mana < 1000);
        assert_eq!(state(&g, kind), HeroState::Serving);
        // The necromancer (a death book) is not among those a life wizard can summon.
        let necromancer = crate::hero::game::tests::kind_of(&g, 32 /* necromancer */);
        for _ in 0..5 {
            if let Some((_, k)) = joined(&cast(&mut g, 208 /* summon_hero */)) {
                assert_ne!(k, necromancer);
            }
        }
        Ok(())
    }

    /// The summoned hero of the human wizard's cast, waiting to be accepted.
    fn waiting(g: &Game) -> Option<(usize, bool)> {
        g.offers(P0).iter().find_map(|o| match o {
            Offer::Summoned { kind, champion } => Some((*kind, *champion)),
            _ => None,
        })
    }

    #[test]
    fn the_human_wizard_is_asked_to_accept_a_summoned_hero_and_pays_the_mana_either_way(
    ) -> TestResult {
        let mut g = caster();
        g.set_human(Some(P0));
        let events = cast(&mut g, 212 /* summon_champion */);
        let (player, kind) = events
            .iter()
            .find_map(|e| match e {
                Event::HeroSummoned { player, kind } => Some((*player, kind)),
                _ => None,
            })
            .ok_or_else(|| format!("{events:?}"))?;
        assert_eq!(player, P0);
        assert!(joined(&events).is_none(), "the hero does not join yet");
        assert_eq!(waiting(&g), Some((*kind, true)));
        assert_eq!(state(&g, *kind), HeroState::Free);
        assert!(g.player(P0).ok_or("player is missing")?.treasury().mana < 1000);
        // The turn change does not take the offer away.
        g.determine_offers_all();
        assert_eq!(waiting(&g), Some((*kind, true)));
        // Accepting names the hero, costs no gold and brings it to the circle.
        let events = g
            .accept_offer(P0, 0, Some("Rolf".to_string()))
            .map_err(|e| format!("{e:?}"))?;
        let (unit, joined_kind) = joined(&events).ok_or("no HeroJoined")?;
        assert_eq!(joined_kind, *kind);
        assert_eq!(g.units().unit_position(unit), Some(at(3, 3)));
        assert_eq!(state(&g, *kind), HeroState::Serving);
        assert_eq!(g.units().hero_name(unit).as_deref(), Some("Rolf"));
        assert_eq!(
            g.player(P0).ok_or("player is missing")?.treasury().gold,
            100
        );
        assert_eq!(waiting(&g), None);
        Ok(())
    }

    #[test]
    fn a_refused_summoned_hero_stays_free_and_the_computer_wizards_get_theirs_at_once() -> TestResult
    {
        let mut g = caster();
        g.set_human(Some(P0));
        let events = cast(&mut g, 208 /* summon_hero */);
        let kind = events
            .iter()
            .find_map(|e| match e {
                Event::HeroSummoned { kind, .. } => Some(*kind),
                _ => None,
            })
            .ok_or_else(|| format!("{events:?}"))?;
        g.reject_offer(P0, 0).map_err(|e| format!("{e:?}"))?;
        assert_eq!(state(&g, kind), HeroState::Free);
        assert_eq!(waiting(&g), None);
        assert!(g.units().hero_units(P0).is_empty());
        // No human: the hero joins without a question.
        g.set_human(None);
        let events = cast(&mut g, 208 /* summon_hero */);
        assert!(joined(&events).is_some(), "{events:?}");
        assert_eq!(waiting(&g), None);
        Ok(())
    }

    #[test]
    fn summoning_is_refused_when_nobody_is_left_or_without_a_circle_and_costs_nothing() -> TestResult
    {
        let mut g = caster();
        g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold = 100_000;
        // Every hero serves: nobody is left to summon.
        for k in 0..=torin(&g.content.rules) {
            g.hire_hero(P0, k).map_err(|e| format!("{e:?}"))?;
        }
        let mana = g.player(P0).ok_or("player is missing")?.treasury().mana;
        for number in [208, 212, 142] {
            // summon_hero, summon_champion, incarnation
            let events = cast(&mut g, number);
            assert!(
                matches!(events.as_slice(), [Event::Rejected(_)]),
                "{number}: {events:?}"
            );
        }
        assert_eq!(
            g.player(P0).ok_or("player is missing")?.treasury().mana,
            mana
        );
        let h = g.units().hero_units(P0)[0];
        g.remove_unit(h, Fate::Released);
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .put::<crate::wizard::fortress::SummonAt>(None);
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .put::<crate::wizard::fortress::Fortress>(None);
        assert!(matches!(
            cast(&mut g, 208 /* summon_hero */).as_slice(),
            [Event::Rejected(_)]
        ));
        assert_eq!(
            g.player(P0).ok_or("player is missing")?.treasury().mana,
            mana
        );
        Ok(())
    }

    #[test]
    fn summon_champion_is_refused_when_nobody_is_left() {
        let mut g = caster();
        // A champion summoning finds the free champions, then any free hero, until none remain.
        for k in 0..g.content.rules.hero_templates.len() {
            set_state(&mut g, k, HeroState::Gone);
        }
        let events = cast(&mut g, 212 /* summon_champion */);
        assert!(
            matches!(events.as_slice(), [Event::Rejected(_)]),
            "{events:?}"
        );
    }

    #[test]
    fn incarnation_brings_torin_back_with_his_experience() -> TestResult {
        let mut g = caster();
        let last = torin(&g.content.rules);
        assert_eq!(last, 34, "Torin the Chosen");
        let events = cast(&mut g, 142 /* incarnation */);
        let (unit, kind) = joined(&events).ok_or_else(|| format!("{events:?}"))?;
        assert_eq!(kind, last);
        let _ = g.world.insert_one(unit, Experience(120));
        // There is only one Torin at a time.
        assert!(matches!(
            cast(&mut g, 142 /* incarnation */).as_slice(),
            [Event::Rejected(_)]
        ));
        g.remove_unit(unit, Fate::Slain);
        let (again, _) = joined(&cast(&mut g, 142 /* incarnation */)).ok_or("joined is missing")?;
        assert_eq!(g.world.get::<&Experience>(again)?.0, 120);
        // Not once he is gone for good.
        g.remove_unit(again, Fate::Gone);
        assert!(matches!(
            cast(&mut g, 142 /* incarnation */).as_slice(),
            [Event::Rejected(_)]
        ));
        Ok(())
    }

    #[test]
    fn torin_alone_is_a_hero_and_a_life_creature_and_has_his_three_extra_spells() -> TestResult {
        let rules = &caster().content.rules;
        let last = torin(rules);
        for t in &rules.hero_templates {
            let def = rules.units.get(t.unit);
            let torin = rules.hero_templates[last].unit == t.unit;
            assert_eq!(
                def.realm.map(|r| r.num()),
                torin.then_some(3 /* life */),
                "{}",
                t.unit.num()
            );
        }
        let spells = &rules.hero_templates[last].spells;
        for s in [
            crate::rules::SpellId::new(125), // healing
            crate::rules::SpellId::new(126), // holy_armor
            crate::rules::SpellId::new(141), // lionheart
        ] {
            assert!(spells.contains(&s), "{}", s.num());
        }
        Ok(())
    }

    fn raise(g: &mut Game, kind: usize) -> Vec<Event> {
        let spell = crate::rules::SpellId::new(133) /* resurrection */;
        g.apply(Command::CastSpell {
            extra: 0,
            player: P0,
            spell,
            target: Some(crate::rules::exec::Target::Hero(kind)),
        })
    }

    #[test]
    fn resurrection_raises_the_chosen_dead_hero() -> TestResult {
        let mut g = caster();
        g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold = 5000;
        let a = g.hire_hero(P0, 0).map_err(|e| format!("{e:?}"))?;
        let b = g.hire_hero(P0, 1).map_err(|e| format!("{e:?}"))?;
        let _ = g.world.insert_one(b, Experience(75));
        g.remove_unit(a, Fate::Slain);
        g.remove_unit(b, Fate::Slain);
        // The spell is not cast at once without saying whom.
        assert!(matches!(
            cast(&mut g, 133 /* resurrection */).as_slice(),
            [Event::Rejected(_)]
        ));
        let mana = g.player(P0).ok_or("player is missing")?.treasury().mana;
        let events = raise(&mut g, 1);
        let (unit, kind) = joined(&events).ok_or_else(|| format!("{events:?}"))?;
        assert_eq!(kind, 1);
        assert_eq!(g.world.get::<&Experience>(unit)?.0, 75);
        assert!(g.player(P0).ok_or("player is missing")?.treasury().mana < mana);
        assert_eq!(g.units().raisable_heroes(), vec![0]);
        // A hero who is not dead, or never was, cannot be raised.
        for kind in [1, 5, 99] {
            let events = raise(&mut g, kind);
            assert!(
                matches!(events.as_slice(), [Event::Rejected(_)]),
                "{kind}: {events:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn resurrection_picks_its_hero_when_the_cost_is_paid_and_survives_a_save() -> TestResult {
        let mut g = caster();
        g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold = 5000;
        let a = g.hire_hero(P0, 3).map_err(|e| format!("{e:?}"))?;
        g.remove_unit(a, Fate::Slain);
        let mut skill = g.wizard_mut(P0).ok_or("wizard_mut is missing")?;
        skill.get_mut::<crate::wizard::sheet::Skill>().left = 0;
        skill.get_mut::<crate::wizard::sheet::Skill>().points = 0;
        // Started with no hero named: the cost is paid over turns first.
        let spell = crate::rules::SpellId::new(133) /* resurrection */;
        let events = cast(&mut g, 133 /* resurrection */);
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
        assert_eq!(
            g.wizard(P0)
                .ok_or("wizard is missing")?
                .casting()
                .map(|c| c.spell),
            Some(spell)
        );
        // (The test world has no city for the fortress to stand in, which a save refuses.)
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .put::<crate::wizard::fortress::Fortress>(None);
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .put::<crate::wizard::fortress::SummonAt>(None);
        let text = ron::to_string(&g.snapshot())?;
        let mut back = crate::game::save::Snapshot::from_text(&text)?.restore(g.content.clone())?;
        back.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .put(Some(crate::wizard::fortress::SummonAt(at(3, 3))));
        assert_eq!(
            back.wizard(P0)
                .ok_or("wizard is missing")?
                .casting()
                .map(|c| c.spell),
            Some(spell)
        );
        // The cost is paid; the hero is named then.
        back.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .cancel_casting();
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
        Ok(())
    }

    #[test]
    fn resurrection_warns_when_it_is_paid_and_no_hero_lies_dead() -> TestResult {
        let mut g = caster();
        let spell = crate::rules::SpellId::new(133) /* resurrection */;
        assert!(!g.spell_has_target(P0, spell));
        let mut skill = g.wizard_mut(P0).ok_or("wizard_mut is missing")?;
        skill.get_mut::<crate::wizard::sheet::Skill>().left = 0;
        skill.get_mut::<crate::wizard::sheet::Skill>().points = 0;
        let started = cast(&mut g, 133 /* resurrection */);
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
        g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold = 5000;
        for k in [1, 4, 34] {
            let h = g.hire_hero(P0, k).map_err(|e| format!("{e:?}"))?;
            g.remove_unit(h, Fate::Slain);
        }
        assert_eq!(g.units().raisable_heroes(), vec![1, 4]);
        assert_eq!(g.units().ai_resurrection_pick(), Some(4));
        assert!(matches!(raise(&mut g, 34).as_slice(), [Event::Rejected(_)]));
        let _ = (P1, Plane::Arcanus);
        Ok(())
    }
}
