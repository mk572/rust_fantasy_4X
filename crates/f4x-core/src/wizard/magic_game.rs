//! The game's side of magic: what a wizard's power and upkeep need from the world (the cities,
//! the fortress, the nodes, the units and city spells that cost mana), the commands, the turn
//! change, and the glue to the wizard's sheet (`wizard/sheet.rs`), which holds the wizard's components and their rules.
//! Spell resolution is in `spell/cast.rs`, `spell/dispel.rs` and `spell/city.rs`.

use crate::game::{Event, Game};
use crate::player::PlayerId;
use crate::rules::{CastContext, SpellId};
use crate::wizard::magic::{upkeep_plan, CastStart, MagicError, PowerShares, Retort};

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
        books: &[(&str, u8)],
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
        w.found(books, retorts);
        w.init_library(content, rng, start_spells);
        self.refill_research(player);
        Ok(())
    }

    /// A computer wizard also knows one spell of its main realm, the realm with most books (the
    /// first in the realms' order on a tie), from `spell_library.ai_main_realm_known`. The
    /// human wizard and the independent players get none.
    pub(crate) fn give_main_realm_spell(&mut self, player: PlayerId) {
        if self.human() == Some(player) {
            return;
        }
        let guaranteed = self.settings.ai.wizards.guaranteed_summoning_spell;
        let Some(w) = self.wizard(player) else {
            return;
        };
        let books = w.books();
        let main = self
            .content
            .rules
            .realms
            .defs()
            .iter()
            .filter_map(|r| Some((r.id.as_str(), *books.get(&r.id)?)))
            .fold(None, |best: Option<(&str, u8)>, (id, n)| match best {
                Some((_, b)) if b >= n => best,
                _ => Some((id, n)),
            });
        let Some((realm, _)) = main else {
            return;
        };
        let library = &self.content.settings.spell_library;
        let spells = &self.content.rules.spells;
        let known = guaranteed
            .then(|| library.ai_main_realm_known.get(realm))
            .flatten()
            .and_then(|id| spells.id_of(id));
        let open = (self.settings.basic.difficulty >= library.ai_main_realm_researchable_from)
            .then(|| library.ai_main_realm_researchable.get(realm))
            .flatten()
            .and_then(|id| spells.id_of(id));
        if let Some(open) = open {
            let known = self.wizard(player).is_some_and(|w| w.knows(open));
            if let Some(mut w) = self.wizard_mut(player).filter(|_| !known) {
                w.get_mut::<crate::wizard::sheet::KnownSpells>()
                    .researchable
                    .insert(open);
            }
        }
        match known {
            Some(spell) => self.learn_spell(player, spell),
            None => self.refill_research(player),
        }
    }

    // ---- power -------------------------------------------------------------------

    /// Is the wizard casting the Spell of Return? Then they get no income of any kind.
    pub fn casting_return(&self, player: PlayerId) -> bool {
        self.wizard(player)
            .and_then(|w| w.casting())
            .is_some_and(|c| self.spell_has_hook(c.spell, crate::wizard::fortress::RETURN))
    }

    /// Mana needed to cast `spell` (`sheet::casting_cost`).
    pub fn casting_cost(&self, player: PlayerId, spell: SpellId, in_battle: bool) -> i32 {
        let def = self.content.rules.spells.get(spell);
        let evil_omens = self.evil_omens_active();
        crate::wizard::sheet::casting_cost(self.wizard(player).as_ref(), def, in_battle, evil_omens)
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

    /// Top the offer for research up (`Magic::refill_research`), at the income the wizard's
    /// research has now. Called each turn, when a wizard is made and when books are added.
    pub fn refill_research(&mut self, player: PlayerId) {
        let income = self.income(player).research;
        let (content, rng) = (&self.content, &mut self.rng);
        if let Some(mut w) = self
            .player_index
            .get(&player)
            .and_then(|&e| crate::wizard::entity::of_player(&self.world, e))
            .map(|e| crate::wizard::sheet::WizardMut::new(&mut self.world, e))
        {
            w.refill_research(content, rng, income);
        }
    }

    // ---- turn change -------------------------------------------------------------

    /// All wizards' magic at the turn change: income, research, upkeep, casting. The steps run
    /// over all wizards in turn, as in the original (power for all, then the research check for
    /// all, and so on). The temporary effects, global spells and experience that used to
    /// start here are the turn's (`turn.rs`).
    pub(crate) fn process_magic(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let ids: Vec<PlayerId> = crate::player::ids(&self.world);
        // While Time Stop lasts, its wizard has no income and only the others' time is stopped
        // for casting: no spell of theirs gets any further.
        let stopper = self.time_stopper();
        for &player in &ids {
            if stopper == Some(player) {
                continue;
            }
            let income = self.income(player);
            self.collect_power(player, &income.split, income.research);
        }
        for &player in &ids {
            if let Some(spell) = self
                .wizard_mut(player)
                .and_then(|mut w| w.finish_research())
            {
                events.push(Event::SpellLearned { player, spell });
            }
        }
        for &player in &ids {
            self.refill_research(player);
        }
        for &player in &ids {
            events.extend(self.apply_mana_upkeep(player));
        }
        for &player in ids.iter().filter(|&&p| stopper.is_none_or(|s| s == p)) {
            // Heroes at the fortress add to the skill that pays for a spell over turns.
            let heroes = self.hero_casting_skill(player);
            self.pay_for_casting(player, heroes);
        }
        for &player in ids.iter().filter(|&&p| stopper.is_none_or(|s| s == p)) {
            events.extend(self.finish_casting(player));
        }
        events
    }

    /// The turn's income reaches the wizard: mana goes to the reserve, research to the spell being
    /// researched and skill points to casting skill (`WizardMut::take_power`). `split` and
    /// `research` are what the wizard's cities and fortress make, worked out by the game.
    fn collect_power(&mut self, player: PlayerId, split: &PowerShares, research: i32) {
        let Some(mana) = self
            .wizard_mut(player)
            .map(|mut w| w.take_power(split, research))
        else {
            return;
        };
        if let Some(t) = self.treasury_mut(player) {
            t.mana = t.mana.saturating_add(mana);
        }
    }

    /// Heroes at the fortress add their skill, then skill and mana are spent on the spell being
    /// cast (`WizardMut::pay_casting`).
    fn pay_for_casting(&mut self, player: PlayerId, heroes: i32) {
        let Some(reserve) = self.treasury(player).map(|t| t.mana) else {
            return;
        };
        let Some(mut w) = self.wizard_mut(player) else {
            return;
        };
        w.add_skill_left(heroes);
        let reserve = w.pay_casting(reserve);
        if let Some(t) = self.treasury_mut(player) {
            t.mana = reserve;
        }
    }

    /// Pay the mana upkeep. If the reserve cannot cover it, the newest lasting spells and
    /// summoned creatures are given up until it can (`upkeep_plan`).
    fn apply_mana_upkeep(&mut self, player: PlayerId) -> Vec<Event> {
        let items = self.upkeep_items(player);
        let reserve = self.treasury(player).map_or(0, |p| p.mana);
        let (dropped, owed) = upkeep_plan(
            &items,
            self.wizard(player).map_or(1, |w| w.upkeep_divisor()),
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
            t.mana = (t.mana - owed).max(0);
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
            return vec![Event::SpellReady { player, spell }];
        }
        self.resolve_cast(player, spell, None)
    }

    /// Start casting `spell` overland. If the skill and mana on hand pay for all of it, it
    /// takes effect now (and needs its target now); otherwise it is paid over turns.
    pub fn start_cast(
        &mut self,
        player: PlayerId,
        spell: SpellId,
        target: Option<crate::rules::exec::Target>,
    ) -> Result<Vec<Event>, MagicError> {
        let cost = self.casting_cost(player, spell, false);
        let reserve = self.treasury(player).ok_or(MagicError::UnknownPlayer)?.mana;
        let def = self.content.rules.spells.get(spell);
        if !def.contexts.contains(&CastContext::Overland) {
            return Err(MagicError::NotOverland);
        }
        let needs_target = self.spell_needs_target(player, spell);
        if self.spell_has_hook(spell, crate::item::crafting::CRAFT_ITEM) {
            // The items are designed first: `Command::CraftItem`.
            return Err(MagicError::NotAvailable);
        }
        if self.spell_has_hook(spell, crate::spell::hero::RESURRECTION) {
            // The dead hero is chosen first: `Command::ResurrectHero`.
            return Err(MagicError::NotAvailable);
        }
        for hook in crate::spell::hero::HOOKS {
            if self.spell_has_hook(spell, hook)
                && self.units().hero_spell_blocked(player, hook).is_some()
            {
                // Six heroes already, no circle to appear in, or nobody left to bring.
                return Err(MagicError::NotAvailable);
            }
        }
        if self.spell_has_hook(spell, crate::wizard::fortress::RETURN) {
            // Cast only through banishment, never chosen.
            return Err(MagicError::NotAvailable);
        }
        let start = {
            let w = self.wizard(player).ok_or(MagicError::UnknownPlayer)?;
            w.check_can_cast(spell)?;
            w.cast_start(cost, reserve)
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
                events.extend(self.resolve_cast(player, spell, target));
                Ok(events)
            }
            CastStart::Paid => {
                if let Some(mut w) = self.wizard_mut(player) {
                    w.begin_casting(spell, cost);
                }
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
