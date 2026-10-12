//! The game's side of magic: what a wizard's power and upkeep need from the world (the cities,
//! the fortress, the nodes, the units and city spells that cost mana), the commands, the turn
//! change, and the glue to `Magic` (`magic.rs`), which holds the wizard's own state and rules.
//! Spell resolution is in `cast.rs`, `dispel.rs` and `city_spells.rs`.

use crate::city::PlayerId;
use crate::content::Content;
use crate::magic::{
    collect_power, pay_for_casting, upkeep_plan, CastStart, Magic, MagicError, PowerSplit, Retort,
};
use crate::rng::GameRng;
use crate::rules::{CastContext, SpellId};
use crate::{Event, Game};

impl Game {
    pub fn magic(&self, player: PlayerId) -> Option<&Magic> {
        self.player(player).map(|p| &p.magic)
    }

    pub(crate) fn magic_mut(&mut self, player: PlayerId) -> Option<&mut Magic> {
        self.player_mut(player).map(|p| &mut p.magic)
    }

    /// A wizard's magic with the content and the random numbers, borrowed apart so the rules of
    /// `Magic` can use all three.
    fn magic_parts(&mut self, player: PlayerId) -> Option<(&mut Magic, &Content, &mut GameRng)> {
        let p = self.players.iter_mut().find(|p| p.id == player)?;
        Some((&mut p.magic, &self.content, &mut self.rng))
    }

    /// Set up a new wizard: books and retorts, starting casting skill, and the spell
    /// library (what is known and what can be researched) from the number of books.
    pub fn init_wizard(
        &mut self,
        player: PlayerId,
        books: &[(&str, u8)],
        retorts: &[Retort],
    ) -> Result<(), MagicError> {
        self.init_wizard_with(player, books, retorts, None)
    }

    /// Like [`Game::init_wizard`], with the starting spells given instead of drawn at random.
    pub fn init_wizard_with(
        &mut self,
        player: PlayerId,
        books: &[(&str, u8)],
        retorts: &[Retort],
        start_spells: Option<&[SpellId]>,
    ) -> Result<(), MagicError> {
        let magic = Magic::new(books, retorts);
        // A famous wizard starts with 10 fame, any other with none.
        let fame = if magic.has(Retort::Famous) { 10 } else { 0 };
        let p = self.player_mut(player).ok_or(MagicError::UnknownPlayer)?;
        p.magic = magic;
        p.fame = fame;
        if let Some((m, content, rng)) = self.magic_parts(player) {
            m.init_library(content, rng, start_spells);
        }
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

    /// Magic power before the split: mana from the wizard's cities (minerals, citizens,
    /// religious buildings) and the fortress. Owned nodes and volcanoes add to it.
    pub fn power_base(&self, player: PlayerId) -> i32 {
        if self.casting_return(player) {
            return 0;
        }
        let total: i32 = self
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == player)
            .filter_map(|(e, ..)| self.city_economy(e))
            .map(|eco| eco.mana)
            .sum();
        (total
            + self.fortress_power(player)
            + self.volcano_count(player)
            + self.node_income(player))
        .max(0)
    }

    fn fortress_power(&self, player: PlayerId) -> i32 {
        let Some(city) = self.fortress_city(player) else {
            return 0;
        };
        let books = self.magic(player).map_or(0, |m| m.total_books());
        let myrror = self
            .world
            .get::<&crate::city::Position>(city)
            .is_ok_and(|p| p.plane == crate::map::Plane::Myrror);
        books + if myrror { 5 } else { 0 }
    }

    /// Research points the wizard's cities produce.
    pub fn city_research(&self, player: PlayerId) -> i32 {
        self.cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == player)
            .filter_map(|(e, ..)| self.city_economy(e))
            .map(|eco| eco.research)
            .sum()
    }

    /// The power base split by the wizard's percentages (`Magic::split_power`).
    pub fn power_split(&self, player: PlayerId) -> PowerSplit {
        if self.casting_return(player) {
            return PowerSplit::default();
        }
        let Some(m) = self.magic(player) else {
            return PowerSplit::default();
        };
        m.split_power(self.power_base(player), self.city_research(player))
    }

    /// Research points that will be paid next turn change, with bonuses.
    pub fn research_income(&self, player: PlayerId) -> i32 {
        let split = self.power_split(player);
        self.magic(player).map_or(split.research.max(0), |m| {
            m.research_income(&self.content, &split)
        })
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

    /// Mana per turn needed to keep lasting spells and summoned creatures going: a Channeler
    /// pays half of the total.
    pub fn mana_upkeep(&self, player: PlayerId) -> i32 {
        self.upkeep_items(player)
            .iter()
            .map(|i| i.mana)
            .sum::<i32>()
            / self.magic(player).map_or(1, |m| m.upkeep_divisor())
    }

    // ---- commands ----------------------------------------------------------------

    /// Choose what to research. The choice stands until the spell is learned.
    pub fn set_research(&mut self, player: PlayerId, spell: SpellId) -> Result<(), MagicError> {
        let (m, content, _) = self.magic_parts(player).ok_or(MagicError::UnknownPlayer)?;
        m.choose_research(content, spell)
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
        let income = self.research_income(player);
        if let Some((m, content, rng)) = self.magic_parts(player) {
            m.refill_research(content, rng, income);
        }
    }

    /// Convert gold to mana or mana to gold (alchemy): one for one with the retort, two for
    /// one without. `gold_to_mana` converts that much gold; negative converts mana to gold.
    pub fn alchemy(
        &mut self,
        player: PlayerId,
        amount: i32,
        gold_to_mana: bool,
    ) -> Result<(), MagicError> {
        let skilled = self
            .magic(player)
            .ok_or(MagicError::UnknownPlayer)?
            .has(Retort::Alchemy);
        let p = self.player_mut(player).ok_or(MagicError::UnknownPlayer)?;
        let amount = amount.max(0);
        if gold_to_mana {
            if p.gold < amount {
                return Err(MagicError::NotEnoughGold);
            }
            p.gold -= amount;
            p.mana += if skilled { amount } else { amount / 2 };
        } else {
            if p.mana < amount {
                return Err(MagicError::NotEnoughMana);
            }
            p.mana -= amount;
            p.gold += if skilled { amount } else { amount / 2 };
        }
        Ok(())
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
            let split = self.power_split(player);
            let research = self.research_income(player);
            if let Some(p) = self.player_mut(player) {
                collect_power(p, &split, research);
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

    /// Give up the spell being cast; the mana already paid is lost.
    pub fn cancel_cast(&mut self, player: PlayerId) -> Result<(), MagicError> {
        if self.casting_return(player) {
            return Err(MagicError::NotAvailable);
        }
        let m = self.magic_mut(player).ok_or(MagicError::UnknownPlayer)?;
        m.cancel_casting();
        Ok(())
    }

    /// Turn off one of the wizard's own global enchantments.
    pub fn turn_off_spell(
        &mut self,
        player: PlayerId,
        spell: SpellId,
    ) -> Result<Vec<Event>, MagicError> {
        let m = self.magic_mut(player).ok_or(MagicError::UnknownPlayer)?;
        m.turn_off(spell)?;
        Ok(vec![Event::EnchantmentLapsed { player, spell }])
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
