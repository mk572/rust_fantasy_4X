//! The game's side of magic: what a wizard's power and upkeep need from the world (the cities,
//! the fortress, the nodes, the units and city spells that cost mana), the commands, the turn
//! change, and the glue to the wizard's sheet (`wizard/sheet.rs`), which holds the wizard's components and their rules.
//! Spell resolution is in `spell/cast.rs`, `spell/dispel.rs` and `spell/city.rs`.
//!
//! Tests: in wizard/magic_game/tests.rs.

use crate::game::{Event, Game};
use crate::player::PlayerId;
use crate::rules::{CastContext, RealmId, SpellId};
use crate::wizard::magic::{upkeep_plan, CastStart, MagicError, PowerShares, Retort};
use f4x_def::half_int::HalfInt;

impl Game {
    /// Set up a new wizard (see [`WizardMut::found`] and [`WizardMut::init_library`]), with the
    /// starting spells given instead of drawn at random when `start_spells` is `Some`, and the
    /// first offer for research.
    ///
    /// [`WizardMut::found`]: crate::wizard::sheet::WizardMut::found
    /// [`WizardMut::init_library`]: crate::wizard::sheet::WizardMut::init_library
    pub fn init_wizard(
        &mut self,
        player: PlayerId,
        books: &[(RealmId, u8)],
        retorts: &[Retort],
        start_spells: Option<&[SpellId]>,
    ) -> Result<(), MagicError> {
        let (content, rng) = (&self.content, &mut self.rng);
        let entity = self
            .player_index
            .get(&player)
            .and_then(|&e| crate::wizard::entity::of_player(&self.world, e))
            .ok_or(MagicError::UnknownPlayer)?;
        let mut w = crate::wizard::sheet::WizardMut::new(&mut self.world, entity);
        w.found(&content.rules, books, retorts);
        w.init_library(content, rng, start_spells);
        self.refill_research(player);
        Ok(())
    }

    // ---- power -------------------------------------------------------------------

    /// Is the wizard casting the Spell of Return? Then they get no income of any kind.
    pub fn casting_return(&self, player: PlayerId) -> bool {
        self.wizard(player)
            .and_then(|w| w.casting())
            .is_some_and(|c| {
                self.content
                    .rules
                    .spells
                    .get(c.spell)
                    .has_hook(crate::spell::special::SPELL_OF_RETURN)
            })
    }

    /// Whether mana put into `spell` beyond its cost makes it stronger overland: the spells
    /// that dispel (Disenchant Area and True) and those that attack a rival's global enchantment
    /// (Disjunction, Disjunction True and Spell Binding). Dispel strength is the mana, so one
    /// extra mana adds one point of strength, three for a True spell.
    pub fn spell_takes_extra_mana(&self, spell: SpellId) -> bool {
        let def = self.content.rules.spells.get(spell);
        def.contexts.contains(&CastContext::Overland)
            && crate::spell::dispel::CounterEffect::of(def)
                .is_some_and(|(e, _)| e.takes_extra_mana())
    }

    /// The mana `spell` costs with `extra` mana put into it.
    pub fn casting_cost_with_extra(&self, player: PlayerId, spell: SpellId, extra: i32) -> i32 {
        let mut def = self.content.rules.spells.get(spell).clone();
        def.cost += extra.max(0) as u32;
        let evil_omens = self.evil_omens_active();
        crate::wizard::sheet::casting_cost(
            &self.content.rules,
            self.wizard(player).as_ref(),
            &def,
            false,
            evil_omens,
        )
    }

    /// Mana needed to cast `spell` (`sheet::casting_cost`).
    pub fn casting_cost(&self, player: PlayerId, spell: SpellId, in_battle: bool) -> i32 {
        let def = self.content.rules.spells.get(spell);
        let evil_omens = self.evil_omens_active();
        crate::wizard::sheet::casting_cost(
            &self.content.rules,
            self.wizard(player).as_ref(),
            def,
            in_battle,
            evil_omens,
        )
    }

    // ---- commands ----------------------------------------------------------------

    /// Choose what to research. The choice stands until the spell is learned.
    pub fn set_research(&mut self, player: PlayerId, spell: SpellId) -> Result<(), MagicError> {
        let content = &self.content;
        let mut w = self
            .player_index
            .get(&player)
            .and_then(|&e| crate::wizard::entity::of_player(&self.world, e))
            .map(|e| crate::wizard::sheet::WizardMut::new(&mut self.world, e))
            .ok_or(MagicError::UnknownPlayer)?;
        w.choose_research(content, spell)
    }

    /// The spell becomes known to `player`, as if researched (a gift or an exchange between
    /// wizards, `diplomacy.rs`).
    pub(crate) fn learn_spell(&mut self, player: PlayerId, spell: SpellId) {
        let Some(mut w) = self.wizard_mut(player) else {
            return;
        };
        w.learn(spell);
        self.refill_research(player);
    }

    /// Top the offer for research up (`refill_research`), at the income the wizard's
    /// research has now. Called each turn, when a wizard is made and when books are added.
    pub fn refill_research(&mut self, player: PlayerId) {
        let income = self.income(player).research;
        let mastery_open = self.mastery_open_to(player);
        let (content, rng) = (&self.content, &mut self.rng);
        if let Some(mut w) = self
            .player_index
            .get(&player)
            .and_then(|&e| crate::wizard::entity::of_player(&self.world, e))
            .map(|e| crate::wizard::sheet::WizardMut::new(&mut self.world, e))
        {
            w.refill_research(content, rng, income, mastery_open);
        }
    }

    // ---- turn change -------------------------------------------------------------

    /// The research that finished at the last turn end is learned and the next spell is chosen,
    /// at the start of the wizard's turn (`collect_power` added the points at the end).
    pub(crate) fn process_research(&mut self, player: PlayerId) -> Vec<Event> {
        let mut events = Vec::new();
        if let Some(spell) = self
            .wizard_mut(player)
            .and_then(|mut w| w.finish_research())
        {
            events.push(Event::SpellLearned { player, spell });
        }
        self.refill_research(player);
        events
    }

    /// One wizard's magic at the end of their turn: income (mana, research points, skill),
    /// mana upkeep, casting (the original does each step for all wizards in turn; the turn is
    /// now the player's own). The research is learned at the next turn's start
    /// ([`Game::process_research`]); the temporary effects, global spells and experience that
    /// used to start here are the round's (`turn.rs`).
    pub(crate) fn process_magic(&mut self, player: PlayerId) -> Vec<Event> {
        let mut events = Vec::new();
        // While Time Stop lasts, its wizard has no income and only the others' time is stopped
        // for casting: no spell of theirs gets any further.
        let stopper = self.time_stopper();
        if stopper != Some(player) {
            let income = self.income(player);
            self.collect_power(player, &income.split, income.research);
        }
        events.extend(self.apply_mana_upkeep(player));
        if stopper.is_none_or(|s| s == player) {
            // A spell with nothing to aim at is not paid for; its wizard is asked whether to stop.
            if let Some((spell, paid)) = self.aimless_cast(player) {
                if !paid {
                    events.push(Event::NoTargetForCast { player, spell });
                }
            } else {
                // Heroes at the fortress add to the skill that pays for a spell over turns.
                let heroes = self.hero_casting_skill(player);
                self.pay_for_casting(player, heroes);
                events.extend(self.finish_casting(player));
            }
        }
        events
    }

    /// The turn's income reaches the wizard: mana goes to the reserve, research to the spell being
    /// researched and skill points to casting skill (`WizardMut::take_power`). `split` and
    /// `research` are what the wizard's cities and fortress make, worked out by the game.
    fn collect_power(&mut self, player: PlayerId, split: &PowerShares, research: i32) {
        // A player without a wizard (a neutral race, task 522) has all its power as mana.
        let mana = match self.wizard_mut_with_rules(player) {
            Some((mut w, rules)) => w.take_power(rules, split, research),
            None => split.mana,
        };
        if let Some(t) = self.treasury_mut(player) {
            t.mana += mana;
        }
    }

    /// Heroes at the fortress add their skill, then skill and mana are spent on the spell being
    /// cast (`WizardMut::pay_casting`).
    fn pay_for_casting(&mut self, player: PlayerId, heroes: i32) {
        let Some(reserve) = self.player(player).map(|p| p.treasury().mana) else {
            return;
        };
        let Some((mut w, rules)) = self.wizard_mut_with_rules(player) else {
            return;
        };
        w.add_skill_left(heroes);
        let reserve = w.pay_casting(rules, reserve);
        if let Some(t) = self.treasury_mut(player) {
            t.mana = reserve;
        }
    }

    /// Pay the mana upkeep. If the reserve cannot cover it, the newest lasting spells and
    /// summoned creatures are given up until it can (`upkeep_plan`).
    fn apply_mana_upkeep(&mut self, player: PlayerId) -> Vec<Event> {
        let items = self.upkeep_items(player);
        let reserve = self
            .player(player)
            .map_or(HalfInt::ZERO, |p| p.treasury().mana);
        let (dropped, owed) = upkeep_plan(
            &items,
            self.wizard(player)
                .map_or(100, |w| w.upkeep_percent(&self.content.rules)),
            reserve,
        );
        let mut events = Vec::new();
        if !dropped.is_empty() {
            events.push(Event::Shortage {
                player,
                what: crate::game::Shortage::Mana,
            });
        }
        for item in dropped {
            events.extend(self.drop_upkeep_item(player, item));
        }
        if let Some(t) = self.treasury_mut(player) {
            t.mana = (t.mana - owed).max(HalfInt::ZERO);
        }
        events
    }

    /// A spell that has been paid for takes effect: at once if it needs no target, else
    /// the wizard is asked for one.
    fn finish_casting(&mut self, player: PlayerId) -> Vec<Event> {
        let Some(spell) = self.wizard(player).and_then(|w| w.casting_paid()) else {
            return Vec::new();
        };
        if self.spell_needs_target(player, spell) {
            return vec![if self.spell_has_target(player, spell) {
                Event::SpellReady { player, spell }
            } else {
                Event::NoTargetForCast { player, spell }
            }];
        }
        self.resolve_cast(player, spell, None)
    }

    /// The spell the wizard is casting if it needs a target and there is none to aim it at, and
    /// whether it is already paid for.
    fn aimless_cast(&self, player: PlayerId) -> Option<(SpellId, bool)> {
        let c = self.wizard(player)?.casting()?;
        let aimless =
            self.spell_needs_target(player, c.spell) && !self.spell_has_target(player, c.spell);
        aimless.then_some((c.spell, c.remaining <= 0))
    }

    /// Is there anything `player` could aim `spell` at now? It tries every unit, city, wizard,
    /// rival global enchantment and square against the same check a cast goes through.
    pub fn spell_has_target(&self, player: PlayerId, spell: SpellId) -> bool {
        use crate::map::{Plane, Position};
        use crate::rules::exec::Target;
        if !self.spell_needs_target(player, spell) {
            return true;
        }
        if crate::spell::hero::HeroSummon::Resurrection.is_in(self.content.rules.spells.get(spell))
        {
            return !self.units().raisable_heroes().is_empty();
        }
        let ok = |t: Target| self.check_target(player, spell, Some(t)).is_ok();
        if self
            .units()
            .all_units()
            .into_iter()
            .any(|u| ok(Target::Unit(u)))
            || self.player_ids().into_iter().any(|p| ok(Target::Player(p)))
            || self
                .rival_globals(player)
                .into_iter()
                .any(|i| ok(Target::Spell(i)))
        {
            return true;
        }
        let cities = self.economy().cities();
        let realms = &self.content.rules.realms;
        let realms: Vec<_> = realms.iter().map(|(id, _)| id).collect();
        if cities.iter().any(|&(c, ..)| {
            ok(Target::City(c)) || realms.iter().any(|&r| ok(Target::CityRealm(c, r)))
        }) {
            return true;
        }
        let Some(map) = self.map.as_ref() else {
            return false;
        };
        [Plane::Arcanus, Plane::Myrror].into_iter().any(|plane| {
            (0..map.height())
                .any(|y| (0..map.width()).any(|x| ok(Target::Tile(Position { plane, x, y }))))
        })
    }

    /// Start casting `spell` overland. If the skill and mana on hand pay for all of it, it
    /// takes effect now (and needs its target now); otherwise it is paid over turns.
    pub fn start_cast(
        &mut self,
        player: PlayerId,
        spell: SpellId,
        target: Option<crate::rules::exec::Target>,
    ) -> Result<Vec<Event>, MagicError> {
        self.start_cast_with(player, spell, target, 0)
    }

    /// `start_cast` with `extra` mana put into a spell that takes it.
    pub fn start_cast_with(
        &mut self,
        player: PlayerId,
        spell: SpellId,
        target: Option<crate::rules::exec::Target>,
        extra: i32,
    ) -> Result<Vec<Event>, MagicError> {
        if extra < 0 || (extra > 0 && !self.spell_takes_extra_mana(spell)) {
            return Err(MagicError::NotAvailable);
        }
        // The extra mana is part of the price, so that cost reductions apply to it too, and at
        // most as much again as the spell costs.
        let extra = extra.min(self.content.rules.spells.get(spell).cost as i32);
        let cost = self.casting_cost_with_extra(player, spell, extra);
        let reserve = self
            .player(player)
            .ok_or(MagicError::UnknownPlayer)?
            .treasury()
            .mana;
        let def = self.content.rules.spells.get(spell);
        if !def.contexts.contains(&CastContext::Overland) {
            return Err(MagicError::NotOverland);
        }
        // The game's rules may close the Spell of Mastery to this wizard.
        if def.has_hook(crate::spell::special::SPELL_OF_MASTERY) && !self.mastery_open_to(player) {
            return Err(MagicError::NotAvailable);
        }
        let needs_target = self.spell_needs_target(player, spell);
        if self.spell_has_hook(spell, crate::item::crafting::CRAFT_ITEM) {
            // The items are designed first: `Command::CraftItem`.
            return Err(MagicError::NotAvailable);
        }
        // Summon Hero, Summon Champion, Incarnation and Resurrection.
        if crate::spell::hero::HeroSummon::of(self.content.rules.spells.get(spell)).is_some()
            && self.units().hero_spell_blocked(player, spell).is_some()
        {
            // No circle to appear in, or nobody left to bring.
            return Err(MagicError::NotAvailable);
        }
        if self
            .content
            .rules
            .spells
            .get(spell)
            .has_hook(crate::spell::special::SPELL_OF_RETURN)
        {
            // Cast only through banishment, never chosen.
            return Err(MagicError::NotAvailable);
        }
        let start = {
            let w = self.wizard(player).ok_or(MagicError::UnknownPlayer)?;
            w.check_can_cast(spell)?;
            w.cast_start(cost, reserve.floor())
        };
        match start {
            CastStart::Now => {
                if needs_target && target.is_none() {
                    return Err(MagicError::NeedsTarget);
                }
                self.check_target(player, spell, target)?;
                if let Some(mut w) = self.wizard_mut(player) {
                    w.spend_skill(cost);
                }
                if let Some(t) = self.treasury_mut(player) {
                    t.mana -= cost;
                }
                self.begin_cast_relations(player, spell);
                let mut events = self.mastery_announcement(player, spell);
                events.extend(self.resolve_cast_with(player, spell, target, extra));
                Ok(events)
            }
            CastStart::Paid => {
                self.begin_casting_with(player, spell, cost, extra);
                self.begin_cast_relations(player, spell);
                Ok(self.mastery_announcement(player, spell))
            }
        }
    }

    /// Choose the target of a spell that has been fully paid for.
    pub fn finish_cast_with_target(
        &mut self,
        player: PlayerId,
        target: crate::rules::exec::Target,
    ) -> Result<Vec<Event>, MagicError> {
        let spell = self
            .wizard(player)
            .and_then(|w| w.casting_paid())
            .ok_or(MagicError::NothingToResolve)?;
        self.check_target(player, spell, Some(target))?;
        Ok(self.resolve_cast(player, spell, Some(target)))
    }
}

#[cfg(test)]
mod tests;
