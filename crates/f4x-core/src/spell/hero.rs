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
use crate::map::Position;
use crate::player::offer::Offer;
use crate::player::PlayerId;
use crate::rules::SpellId;
use crate::unit::UnitView;
use f4x_def::half_int::HalfInt;

/// The hook of the spells that bring a free hero in: Summon Hero, Summon Champion, Incarnation.
pub const SUMMON_HERO: &str = "summon_hero";

crate::hook::effect_enum! {
    /// Which hero a spell with the `summon_hero` hook brings: the argument of the hook.
    pub enum HeroSummon in SUMMON_HERO;
    Hero = "hero",
    Champion = "champion",
    Torin = "torin",
    // Raises a dead hero the wizard chooses.
    Resurrection = "resurrection",
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

    /// The hero a computer wizard raises: the last of the raisable ones.
    pub fn ai_resurrection_pick(&self) -> Option<usize> {
        self.raisable_heroes().last().copied()
    }

    /// Why the wizard cannot bring a hero in by this hook now, if so: no summoning
    /// circle, or nobody to bring (Incarnation: Torin is already serving or
    /// gone). `Resurrection` is checked against the hero chosen, not here.
    pub fn hero_spell_blocked(&self, player: PlayerId, spell: SpellId) -> Option<HeroError> {
        if crate::wizard::entity::find(self.world, player)
            .is_none_or(|w| w.summon_point().is_none())
        {
            return Some(HeroError::NoCircle);
        }
        (!self.hero_to_summon(player, spell)).then_some(HeroError::NotAvailable)
    }

    /// Is there a hero this hero-bringing spell could find for the player (whatever the circle
    /// says)? Always so for a spell that does not bring a free hero. A computer wizard asks before
    /// it plans a summoning, so that it does not cast one every turn when the 35 heroes are all
    /// serving (the neutral players share them, task 522).
    pub fn hero_to_summon(&self, player: PlayerId, spell: SpellId) -> bool {
        let states = self.hero_states();
        let record = |k: usize| states.get(k).copied();
        match HeroSummon::of(self.content.rules.spells.get(spell)) {
            Some(HeroSummon::Hero) => self.hero_summonable(player, Class::Hero),
            Some(HeroSummon::Champion) => self.hero_summonable(player, Class::Champion),
            Some(HeroSummon::Torin) => matches!(
                record(torin(&self.content.rules)),
                Some(HeroState::Free | HeroState::Dead)
            ),
            Some(HeroSummon::Resurrection) | None => true,
        }
    }
}

impl Game {
    /// Hire the free hero `kind` for gold: it joins at the summoning circle.
    pub fn hire_hero(&mut self, player: PlayerId, kind: usize) -> Result<hecs::Entity, HeroError> {
        self.hire_hero_at(player, kind, None)
    }

    /// [`hire_hero`](Self::hire_hero), joining at the summoning circle in `at` when the wizard has one
    /// there (the first circle otherwise).
    pub fn hire_hero_at(
        &mut self,
        player: PlayerId,
        kind: usize,
        at: Option<Position>,
    ) -> Result<hecs::Entity, HeroError> {
        let hero = self.units().hero_entity(kind).ok_or(HeroError::Unknown)?;
        if self.units().hero_state(hero) != Some(HeroState::Free) {
            return Err(HeroError::NotAvailable);
        }
        let price = self
            .units()
            .hero_hire_price(player, kind)
            .ok_or(HeroError::Unknown)?;
        if self
            .player(player)
            .map_or(HalfInt::ZERO, |p| p.treasury().gold)
            < price
        {
            return Err(HeroError::NoGold);
        }
        let unit = self.arrive_at(player, kind, at)?;
        if let Some(t) = self.treasury_mut(player) {
            t.gold -= price;
        }
        Ok(unit)
    }

    /// Bring the hero `kind` in at the summoning circle in `at`, when the wizard has one there (else the
    /// first circle), with nothing charged.
    pub(crate) fn arrive_at(
        &mut self,
        player: PlayerId,
        kind: usize,
        at: Option<Position>,
    ) -> Result<hecs::Entity, HeroError> {
        let at = at
            .filter(|a| self.summon_circles(player).contains(a))
            .or_else(|| self.summon_point(player))
            .ok_or(HeroError::NoCircle)?;
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
        // The circle the wizard chose, when they have several.
        let at = match who {
            crate::rules::exec::Target::City(c) => self.circle_in(player, c),
            crate::rules::exec::Target::HeroAt(_, at) => Some(at),
            _ => None,
        };
        let standing = self.units().hero_standing(player);
        let states = self.units().hero_states();
        let summon = HeroSummon::of(self.content.rules.spells.get(spell));
        let mut pick =
            |class| kind::pick(rng, &self.content.rules, &states, &standing, true, class);
        let kind = match summon {
            Some(HeroSummon::Hero) => pick(Class::Hero),
            Some(HeroSummon::Champion) => pick(Class::Champion),
            Some(HeroSummon::Torin) => Some(torin(&self.content.rules)),
            Some(HeroSummon::Resurrection) => {
                // Nobody to raise if the hero is not dead any more: the spell is lost, with a warning.
                let dead = self.units().raisable_heroes();
                match who {
                    crate::rules::exec::Target::Hero(k)
                    | crate::rules::exec::Target::HeroAt(k, _)
                        if dead.contains(&k) =>
                    {
                        Some(k)
                    }
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
                    o.0.push(Offer::Summoned { kind, champion, at });
                    return Some(vec![Event::HeroSummoned { player, kind }]);
                }
            }
        }
        let joined = kind.and_then(|k| self.arrive_at(player, k, at).ok().map(|u| (u, k)));
        Some(match joined {
            Some((unit, kind)) => vec![Event::HeroJoined { player, unit, kind }],
            None => vec![Event::HeroSpellFailed { player, spell }],
        })
    }
}
