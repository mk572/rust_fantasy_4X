//! The game's side of magic: what a wizard's power and upkeep need from the world (the cities,
//! the fortress, the nodes, the units and city spells that cost mana), the commands, the turn
//! change, and the glue to `Magic` (`magic.rs`), which holds the wizard's own state and rules.
//! Spell resolution is in `cast.rs`, `dispel.rs` and `city_spells.rs`.

use crate::city::PlayerId;
use crate::magic::{
    collect_power, pay_for_casting, upkeep_plan, CastStart, Magic, MagicError, Retort,
};
use crate::magic_commands::found_wizard;
use crate::rules::{CastContext, SpellId};
use crate::{Event, Game};

impl Game {
    pub fn magic(&self, player: PlayerId) -> Option<&Magic> {
        self.player(player).map(|p| &p.magic)
    }

    pub(crate) fn magic_mut(&mut self, player: PlayerId) -> Option<&mut Magic> {
        self.player_mut(player).map(|p| &mut p.magic)
    }

    /// Set up a new wizard (see [`found_wizard`]), with the starting spells given instead of
    /// drawn at random when `start_spells` is `Some`, and the first offer for research.
    pub fn init_wizard(
        &mut self,
        player: PlayerId,
        books: &[(&str, u8)],
        retorts: &[Retort],
        start_spells: Option<&[SpellId]>,
    ) -> Result<(), MagicError> {
        let p = self
            .players
            .iter_mut()
            .find(|p| p.id == player)
            .ok_or(MagicError::UnknownPlayer)?;
        found_wizard(
            p,
            &self.content,
            &mut self.rng,
            books,
            retorts,
            start_spells,
        );
        self.refill_research(player);
        Ok(())
    }

    // ---- power -------------------------------------------------------------------

    /// Is the wizard casting the Spell of Return? Then they get no income of any kind.
    pub fn casting_return(&self, player: PlayerId) -> bool {
        self.magic(player)
            .and_then(|m| m.casting)
            .is_some_and(|c| self.spell_has_hook(c.spell, crate::fortress::RETURN))
    }

    /// Mana needed to cast `spell` (`Magic::casting_cost`).
    pub fn casting_cost(&self, player: PlayerId, spell: SpellId, in_battle: bool) -> i32 {
        let def = self.content.rules.spells.get(spell);
        let evil_omens = self.evil_omens_against(player);
        match self.magic(player) {
            Some(m) => m.casting_cost(def, in_battle, evil_omens),
            None => Magic::default().casting_cost(def, in_battle, evil_omens),
        }
    }

    // ---- commands ----------------------------------------------------------------

    /// Choose what to research. The choice stands until the spell is learned.
    pub fn set_research(&mut self, player: PlayerId, spell: SpellId) -> Result<(), MagicError> {
        let p = self
            .players
            .iter_mut()
            .find(|p| p.id == player)
            .ok_or(MagicError::UnknownPlayer)?;
        p.magic.choose_research(&self.content, spell)
    }

    /// The spell becomes known to `player`, as if researched (a gift or an exchange between
    /// wizards, `diplomacy.rs`).
    pub(crate) fn learn_spell(&mut self, player: PlayerId, spell: SpellId) {
        let Some(m) = self.magic_mut(player) else {
            return;
        };
        m.learn(spell);
        self.refill_research(player);
    }

    /// Top the offer for research up (`Magic::refill_research`), at the income the wizard's
    /// research has now. Called each turn, when a wizard is made and when books are added.
    pub fn refill_research(&mut self, player: PlayerId) {
        let income = self.income(player).research;
        if let Some(p) = self.players.iter_mut().find(|p| p.id == player) {
            p.magic
                .refill_research(&self.content, &mut self.rng, income);
        }
    }

    // ---- turn change -------------------------------------------------------------

    /// All wizards' magic at the turn change: income, research, upkeep, casting. The steps run
    /// over all wizards in turn, as in the original (power for all, then the research check for
    /// all, and so on). The timers, temporary effects, global spells and experience that used to
    /// start here are the turn's (`turn.rs`).
    pub(crate) fn process_magic(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let ids: Vec<PlayerId> = self.players.iter().map(|p| p.id).collect();
        for &player in &ids {
            let income = self.income(player);
            if let Some(p) = self.player_mut(player) {
                collect_power(p, &income.split, income.research);
            }
        }
        for &player in &ids {
            if let Some(spell) = self.magic_mut(player).and_then(|m| m.finish_research()) {
                events.push(Event::SpellLearned { player, spell });
            }
        }
        for &player in &ids {
            self.refill_research(player);
        }
        for &player in &ids {
            events.extend(self.apply_mana_upkeep(player));
        }
        for &player in &ids {
            if let Some(p) = self.player_mut(player) {
                pay_for_casting(p);
            }
        }
        for &player in &ids {
            events.extend(self.finish_casting(player));
        }
        events
    }

    /// Pay the mana upkeep. If the reserve cannot cover it, the newest lasting spells and
    /// summoned creatures are given up until it can (`upkeep_plan`).
    fn apply_mana_upkeep(&mut self, player: PlayerId) -> Vec<Event> {
        let items = self.upkeep_items(player);
        let reserve = self.player(player).map_or(0, |p| p.mana);
        let (dropped, owed) = upkeep_plan(
            &items,
            self.magic(player).map_or(1, |m| m.upkeep_divisor()),
            reserve,
        );
        let mut events = Vec::new();
        for item in dropped {
            events.extend(self.drop_upkeep_item(player, item));
        }
        if let Some(p) = self.player_mut(player) {
            p.mana = (p.mana - owed).max(0);
        }
        events
    }

    /// A spell that has been paid for takes effect: at once if it needs no target, else
    /// the wizard is asked for one.
    fn finish_casting(&mut self, player: PlayerId) -> Vec<Event> {
        let Some(spell) = self.magic(player).and_then(|m| m.casting_paid()) else {
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
        let reserve = self.player(player).ok_or(MagicError::UnknownPlayer)?.mana;
        let def = self.content.rules.spells.get(spell);
        if !def.contexts.contains(&CastContext::Overland) {
            return Err(MagicError::NotOverland);
        }
        let needs_target = self.spell_needs_target(player, spell);
        if self.spell_has_hook(spell, crate::items::crafting::CRAFT_ITEM) {
            // The items are designed first: `Command::CraftItem`.
            return Err(MagicError::NotAvailable);
        }
        if self.spell_has_hook(spell, crate::fortress::RETURN) {
            // Cast only through banishment, never chosen.
            return Err(MagicError::NotAvailable);
        }
        let m = self.magic(player).ok_or(MagicError::UnknownPlayer)?;
        m.check_can_cast(spell)?;
        let start = if cost <= m.instant_budget(reserve) {
            CastStart::Now
        } else {
            CastStart::Paid
        };
        match start {
            CastStart::Now => {
                if needs_target && target.is_none() {
                    return Err(MagicError::NeedsTarget);
                }
                self.check_target(player, spell, target)?;
                if let Some(p) = self.player_mut(player) {
                    p.magic.spend_skill(cost);
                    p.mana -= cost;
                }
                self.begin_cast_relations(player, spell);
                Ok(self.resolve_cast(player, spell, target))
            }
            CastStart::Paid => {
                if let Some(m) = self.magic_mut(player) {
                    m.begin_casting(spell, cost);
                }
                self.begin_cast_relations(player, spell);
                Ok(Vec::new())
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
            .magic(player)
            .and_then(|m| m.casting_paid())
            .ok_or(MagicError::NothingToResolve)?;
        self.check_target(player, spell, Some(target))?;
        Ok(self.resolve_cast(player, spell, Some(target)))
    }
}

#[cfg(test)]
mod tests;
