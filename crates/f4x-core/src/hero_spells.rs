//! Getting heroes into the army (task 48): hiring with gold, and the four spells that bring a
//! hero in: Summon Hero, Summon Champion, Incarnation and Resurrection. A hero always appears at
//! the wizard's summoning circle. The roster and the unit are in `roster.rs` and `hero_game.rs`.
//!
//! Resurrection needs the wizard to choose which dead hero returns, so it is cast through
//! `Command::ResurrectHero` (as Enchant Item is through `Command::CraftItem`) and the chosen hero
//! waits in `Magic::raising` while the cost is paid.

use crate::city::PlayerId;
use crate::hero_game::HeroError;
use crate::rng::GameRng;
use crate::roster::{self, Availability, Class, Standing, HEROES};
use crate::units::Units;
use crate::{Event, Game, Rejection};

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

impl Units<'_> {
    /// Could a summoning of this class find a hero for the wizard? (`Pick_Random_Hero` with a
    /// zero cost: fame does not matter, the books do; a champion summoning falls back on any.)
    pub fn hero_summonable(&self, player: PlayerId, class: Class) -> bool {
        let Some(p) = self.players.iter().find(|p| p.id == player) else {
            return false;
        };
        let standing = Standing {
            fame: 200,
            ..self.hero_standing(player)
        };
        let rules = &self.content.rules;
        let mut range = match class {
            Class::Hero => 0..HEROES,
            Class::Champion | Class::Any => 0..torin(rules),
        };
        range.any(|k| roster::can_offer(rules, &p.roster, k, &standing))
    }

    /// The dead heroes Resurrection can raise: every dead one but Torin, whom only Incarnation
    /// brings back (`Cast_Resurrection` lists types 0 to 33).
    pub fn raisable_heroes(&self, player: PlayerId) -> Vec<usize> {
        let last = torin(&self.content.rules);
        self.players
            .iter()
            .find(|p| p.id == player)
            .map(|p| p.roster.dead())
            .unwrap_or_default()
            .into_iter()
            .filter(|&k| k != last)
            .collect()
    }

    /// The hero a computer wizard raises: the last of the raisable ones, as the original does.
    pub fn ai_resurrection_pick(&self, player: PlayerId) -> Option<usize> {
        self.raisable_heroes(player).last().copied()
    }

    /// Why the wizard cannot bring a hero in by this hook now, if so: no free place among the
    /// six, no summoning circle, or nobody to bring (Incarnation: Torin is already serving or
    /// gone). `Resurrection` is checked against the hero chosen, not here.
    pub fn hero_spell_blocked(&self, player: PlayerId, hook: &str) -> Option<HeroError> {
        if self.open_hero_slots(player) == 0 {
            return Some(HeroError::NoRoom);
        }
        let Some(p) = self.players.iter().find(|p| p.id == player) else {
            return Some(HeroError::Unknown);
        };
        if crate::fortress::summon_point(p).is_none() {
            return Some(HeroError::NoCircle);
        }
        let record = |k: usize| p.roster.get(k).map(|r| r.state);
        let possible = match hook {
            SUMMON_HERO => self.hero_summonable(player, Class::Hero),
            SUMMON_CHAMPION => self.hero_summonable(player, Class::Champion),
            INCARNATION => matches!(
                record(torin(&self.content.rules)),
                Some(Availability::Free | Availability::Dead)
            ),
            _ => true,
        };
        (!possible).then_some(HeroError::NotAvailable)
    }
}

impl Game {
    /// Hire the free hero `kind` for gold: it joins at the summoning circle.
    pub fn hire_hero(&mut self, player: PlayerId, kind: usize) -> Result<hecs::Entity, HeroError> {
        let record = self
            .player(player)
            .and_then(|p| p.roster.get(kind))
            .ok_or(HeroError::Unknown)?;
        if record.state != Availability::Free {
            return Err(HeroError::NotAvailable);
        }
        let price = self
            .units()
            .hero_hire_price(player, kind)
            .ok_or(HeroError::Unknown)?;
        if self.player(player).map_or(0, |p| p.gold) < price {
            return Err(HeroError::NoGold);
        }
        let unit = self.arrive(player, kind)?;
        if let Some(p) = self.player_mut(player) {
            p.gold -= price;
        }
        Ok(unit)
    }

    /// Bring the hero `kind` in at the summoning circle, with nothing charged.
    fn arrive(&mut self, player: PlayerId, kind: usize) -> Result<hecs::Entity, HeroError> {
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
        rng: &mut GameRng,
    ) -> Option<Vec<Event>> {
        let spell = self.content.rules.spells.id_of(hook)?;
        let standing = self.units().hero_standing(player);
        let mut pick = |class| {
            let roster = &self.player(player)?.roster;
            roster::pick(rng, &self.content.rules, roster, &standing, true, class)
        };
        let kind = match hook {
            SUMMON_HERO => pick(Class::Hero),
            SUMMON_CHAMPION => pick(Class::Champion),
            INCARNATION => Some(torin(&self.content.rules)),
            RESURRECTION => self.magic_mut(player).and_then(|m| m.raising.take()),
            _ => return None,
        };
        let joined = kind.and_then(|k| self.arrive(player, k).ok().map(|u| (u, k)));
        Some(match joined {
            Some((unit, kind)) => vec![Event::HeroJoined { player, unit, kind }],
            None => vec![Event::HeroSpellFailed { player, spell }],
        })
    }

    /// Cast Resurrection on the dead hero `kind`. Like any spell it takes effect now if the
    /// casting skill and mana on hand pay for it, else it is paid over turns.
    pub fn resurrect_hero(
        &mut self,
        player: PlayerId,
        kind: usize,
    ) -> Result<Vec<Event>, Rejection> {
        use crate::magic::MagicError;
        let spell = self
            .content
            .rules
            .spells
            .id_of(RESURRECTION)
            .ok_or(Rejection::Magic(MagicError::NotAvailable))?;
        let state = self
            .player(player)
            .and_then(|p| p.roster.get(kind))
            .ok_or(Rejection::Hero(HeroError::Unknown))?
            .state;
        if state != Availability::Dead || kind == torin(&self.content.rules) {
            return Err(Rejection::Hero(HeroError::NotAvailable));
        }
        if let Some(e) = self.units().hero_spell_blocked(player, RESURRECTION) {
            return Err(Rejection::Hero(e));
        }
        let cost = self.casting_cost(player, spell, false);
        let reserve = self.player(player).map_or(0, |p| p.mana);
        let m = self
            .magic(player)
            .ok_or(Rejection::Magic(MagicError::UnknownPlayer))?;
        m.check_can_cast(spell).map_err(Rejection::Magic)?;
        let now = cost <= m.instant_budget(reserve);
        if let Some(p) = self.player_mut(player) {
            p.magic.raising = Some(kind);
            if now {
                p.magic.spend_skill(cost);
                p.mana -= cost;
            } else {
                p.magic.begin_casting(spell, cost);
            }
        }
        self.begin_cast_relations(player, spell);
        if !now {
            return Ok(Vec::new());
        }
        let events = self.resolve_cast(player, spell, None);
        if let Some(m) = self.magic_mut(player) {
            m.raising = None;
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hero_game::tests::{at, world, P0, P1};
    use crate::map::Plane;
    use crate::roster::Fate;
    use crate::rules::effects::{Condition, Effect, EffectRule, TargetSel, Trigger};
    use crate::rules::{CastContext, Duration, Rarity, Registry, SpellDef, SpellTarget};
    use crate::units::Experience;
    use crate::Command;

    fn hero_spell(id: &str, cost: u32) -> SpellDef {
        SpellDef {
            id: id.into(),
            name: id.into(),
            realm: "life".into(),
            rarity: Rarity::Common,
            cost,
            research_cost: cost,
            contexts: vec![CastContext::Overland],
            overland_x5: false,
            target: SpellTarget::None,
            duration: Duration::Instant,
            upkeep: 0,
            dispel_difficulty: 0,
            save: None,
            effects: vec![EffectRule {
                when: Trigger::OnCast,
                target: TargetSel::Chosen,
                condition: Condition::Always,
                effect: Effect::Custom(id.into()),
            }],
        }
    }

    /// The hero world, with the four spells known to wizard 0, a circle at (3, 3), 100 gold
    /// and plenty of mana and skill. The last of the seven hero types is "Torin".
    fn caster() -> Game {
        let mut g = world();
        let mut spells = g.content.rules.spells.defs().to_vec();
        for (id, cost) in [
            (SUMMON_HERO, 30),
            (SUMMON_CHAMPION, 75),
            (INCARNATION, 50),
            (RESURRECTION, 25),
        ] {
            spells.push(hero_spell(id, cost));
        }
        g.content.rules.spells = Registry::new(spells);
        g.init_wizard(P0, &[("life", 3)], &[], None).unwrap();
        for id in HOOKS {
            let s = g.content.rules.spells.id_of(id).unwrap();
            g.magic_mut(P0).unwrap().known.insert(s);
        }
        g.magic_mut(P0).unwrap().skill_left = 99_999;
        let p = g.player_mut(P0).unwrap();
        p.mana = 1000;
        p.gold = 100;
        p.fortress = Some(at(3, 3));
        p.summon_at = Some(at(3, 3));
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
        assert_eq!(g.player(P0).unwrap().gold, 0);
        assert_eq!(g.units().unit_position(h), Some(at(3, 3)));
        assert_eq!(g.units().hero_kind(h), Some(0));
        // Not twice, not without the gold, not without a circle, not the other wizard's record.
        assert_eq!(g.hire_hero(P0, 0), Err(HeroError::NotAvailable));
        assert_eq!(g.hire_hero(P0, 1), Err(HeroError::NoGold));
        assert_eq!(g.hire_hero(P0, 99), Err(HeroError::Unknown));
        g.player_mut(P0).unwrap().gold = 500;
        g.player_mut(P0).unwrap().summon_at = None;
        g.player_mut(P0).unwrap().fortress = None;
        assert_eq!(g.hire_hero(P0, 1), Err(HeroError::NoCircle));
        assert_eq!(g.player(P0).unwrap().gold, 500);
        assert_eq!(
            g.players[1].roster.get(0).unwrap().state,
            Availability::Free
        );
    }

    #[test]
    fn a_dead_hero_is_not_hired_and_six_is_the_most() {
        let mut g = caster();
        g.player_mut(P0).unwrap().gold = 5000;
        let first = g.hire_hero(P0, 0).unwrap();
        g.remove_unit(first, Fate::Slain);
        assert_eq!(g.hire_hero(P0, 0), Err(HeroError::NotAvailable));
        for k in 1..=6 {
            g.hire_hero(P0, k).unwrap();
        }
        assert_eq!(g.units().open_hero_slots(P0), 0);
        assert_eq!(g.player(P0).unwrap().roster.dead(), vec![0]);
    }

    #[test]
    fn summon_hero_brings_a_hero_for_mana_only() {
        let mut g = caster();
        let events = cast(&mut g, SUMMON_HERO);
        let (unit, kind) = joined(&events).unwrap_or_else(|| panic!("{events:?}"));
        assert!(kind < HEROES);
        assert_eq!(g.units().unit_position(unit), Some(at(3, 3)));
        assert_eq!(g.player(P0).unwrap().gold, 100);
        assert!(g.player(P0).unwrap().mana < 1000);
        assert_eq!(
            g.player(P0).unwrap().roster.get(kind).unwrap().state,
            Availability::Serving
        );
        // The necromancer (a death book) is not among those a life wizard can summon.
        for _ in 0..5 {
            if let Some((_, k)) = joined(&cast(&mut g, SUMMON_HERO)) {
                assert_ne!(k, 6);
            }
        }
    }

    #[test]
    fn summoning_is_refused_without_room_or_a_circle_and_costs_nothing() {
        let mut g = caster();
        g.player_mut(P0).unwrap().gold = 5000;
        for k in 0..6 {
            g.hire_hero(P0, k).unwrap();
        }
        let mana = g.player(P0).unwrap().mana;
        for hook in [SUMMON_HERO, SUMMON_CHAMPION, INCARNATION] {
            let events = cast(&mut g, hook);
            assert!(
                matches!(events.as_slice(), [Event::Rejected(_)]),
                "{hook}: {events:?}"
            );
        }
        assert_eq!(g.player(P0).unwrap().mana, mana);
        let h = g.units().hero_units(P0)[0];
        g.remove_unit(h, Fate::Released);
        g.player_mut(P0).unwrap().summon_at = None;
        g.player_mut(P0).unwrap().fortress = None;
        assert!(matches!(
            cast(&mut g, SUMMON_HERO).as_slice(),
            [Event::Rejected(_)]
        ));
        assert_eq!(g.player(P0).unwrap().mana, mana);
    }

    #[test]
    fn summon_champion_is_refused_when_nobody_is_left() {
        let mut g = caster();
        // The test roster has no champion; "any" finds the free ones until none remain.
        for k in 0..6 {
            g.player_mut(P0).unwrap().roster.get_mut(k).unwrap().state = Availability::Gone;
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

    #[test]
    fn resurrection_raises_the_chosen_dead_hero() {
        let mut g = caster();
        g.player_mut(P0).unwrap().gold = 5000;
        let a = g.hire_hero(P0, 0).unwrap();
        let b = g.hire_hero(P0, 1).unwrap();
        let _ = g.world.insert_one(b, Experience(75));
        g.remove_unit(a, Fate::Slain);
        g.remove_unit(b, Fate::Slain);
        // The spell is not cast without saying whom.
        assert!(matches!(
            cast(&mut g, RESURRECTION).as_slice(),
            [Event::Rejected(_)]
        ));
        let mana = g.player(P0).unwrap().mana;
        let events = g.apply(Command::ResurrectHero {
            player: P0,
            kind: 1,
        });
        let (unit, kind) = joined(&events).unwrap_or_else(|| panic!("{events:?}"));
        assert_eq!(kind, 1);
        assert_eq!(g.world.get::<&Experience>(unit).unwrap().0, 75);
        assert!(g.player(P0).unwrap().mana < mana);
        assert_eq!(g.magic(P0).unwrap().raising, None);
        assert_eq!(g.player(P0).unwrap().roster.dead(), vec![0]);
        // A hero who is not dead, or never was, cannot be raised.
        for kind in [1, 5, 99] {
            let events = g.apply(Command::ResurrectHero { player: P0, kind });
            assert!(
                matches!(events.as_slice(), [Event::Rejected(_)]),
                "{kind}: {events:?}"
            );
        }
    }

    #[test]
    fn resurrection_can_be_paid_over_turns_and_survives_a_save() {
        let mut g = caster();
        g.player_mut(P0).unwrap().gold = 5000;
        let a = g.hire_hero(P0, 3).unwrap();
        g.remove_unit(a, Fate::Slain);
        g.magic_mut(P0).unwrap().skill_left = 0;
        g.magic_mut(P0).unwrap().skill_points = 0;
        let events = g.apply(Command::ResurrectHero {
            player: P0,
            kind: 3,
        });
        assert!(events.is_empty(), "{events:?}");
        assert_eq!(g.magic(P0).unwrap().raising, Some(3));
        // (The test world has no city for the fortress to stand in, which a save refuses.)
        g.player_mut(P0).unwrap().fortress = None;
        g.player_mut(P0).unwrap().summon_at = None;
        let text = ron::to_string(&g.snapshot()).unwrap();
        let mut back = crate::save::Snapshot::from_text(&text)
            .unwrap()
            .restore(g.content.clone())
            .unwrap();
        back.player_mut(P0).unwrap().summon_at = Some(at(3, 3));
        assert_eq!(back.magic(P0).unwrap().raising, Some(3));
        // The cost is paid; the spell takes effect.
        let spell = back.content.rules.spells.id_of(RESURRECTION).unwrap();
        let events = back.resolve_cast(P0, spell, None);
        assert_eq!(joined(&events).map(|(_, k)| k), Some(3));
        // Torin is raised by Incarnation only; a computer wizard raises the last dead hero.
        let mut g = caster();
        g.player_mut(P0).unwrap().gold = 5000;
        for k in [1, 4, 6] {
            let h = g.hire_hero(P0, k).unwrap();
            g.remove_unit(h, Fate::Slain);
        }
        assert_eq!(g.units().raisable_heroes(P0), vec![1, 4]);
        assert_eq!(g.units().ai_resurrection_pick(P0), Some(4));
        let events = g.apply(Command::ResurrectHero {
            player: P0,
            kind: 6,
        });
        assert!(matches!(events.as_slice(), [Event::Rejected(_)]));
        // Giving the spell up forgets the hero.
        let mut g = caster();
        g.magic_mut(P0).unwrap().raising = Some(2);
        g.magic_mut(P0).unwrap().cancel_casting();
        assert_eq!(g.magic(P0).unwrap().raising, None);
        let _ = (P1, Plane::Arcanus);
    }
}
