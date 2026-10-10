//! Wizards: spell books, retorts, the power split, research, casting and mana upkeep.
//!
//! Formulas follow the original game's code (see `docs/magic.md` for the list and for where
//! this differs). The wizard is not an entity: it is a [`Magic`] record on the [`Player`].
//!
//! [`Player`]: crate::player::Player

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::city::PlayerId;
use crate::rules::effects::Effect;
use crate::rules::{CastContext, Rarity, SpellDef, SpellId};
use crate::{Event, Game};

/// Special abilities a wizard can pick ("retorts" in the original's manual).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Retort {
    Alchemy,
    Warlord,
    ChaosMastery,
    NatureMastery,
    SorceryMastery,
    Channeler,
    Archmage,
    Artificer,
    Conjurer,
    SageMaster,
    Myrran,
    DivinePower,
    InfernalPower,
    Famous,
    Runemaster,
    Charismatic,
    ManaFocusing,
    NodeMastery,
}

/// A spell the wizard is casting overland: the mana still to be paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Casting {
    pub spell: SpellId,
    pub remaining: i32,
}

/// Everything about a wizard beyond gold and the mana reserve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Magic {
    /// Spell books per realm id.
    pub books: BTreeMap<String, u8>,
    pub retorts: BTreeSet<Retort>,
    /// Casting skill is not stored directly: this total of invested points maps to it
    /// through triangular numbers (see [`base_casting_skill`]).
    pub skill_points: i32,
    /// Share of the power base (percent) going to mana, research and casting skill.
    pub mana_pct: u8,
    pub research_pct: u8,
    pub skill_pct: u8,
    /// Spells the wizard can cast.
    pub known: BTreeSet<SpellId>,
    /// Spells the wizard may research, not yet known.
    pub researchable: BTreeSet<SpellId>,
    pub researching: Option<SpellId>,
    /// Research points still to pay for `researching`.
    pub research_remaining: i32,
    pub casting: Option<Casting>,
    /// Lasting spells on the wizard (or the whole world) that cost mana every turn.
    pub global: Vec<SpellId>,
    /// Casting skill still available this turn for instant casting (the original's
    /// "Skill_Left"); it can be negative after a spell was finished on next turn's skill.
    pub skill_left: i32,
}

impl Default for Magic {
    fn default() -> Self {
        Self {
            books: BTreeMap::new(),
            retorts: BTreeSet::new(),
            skill_points: 0,
            mana_pct: 33,
            research_pct: 34,
            skill_pct: 33,
            known: BTreeSet::new(),
            researchable: BTreeSet::new(),
            researching: None,
            research_remaining: 0,
            casting: None,
            global: Vec::new(),
            skill_left: 0,
        }
    }
}

impl Magic {
    pub fn has(&self, retort: Retort) -> bool {
        self.retorts.contains(&retort)
    }

    pub fn books_in(&self, realm: &str) -> i32 {
        self.books.get(realm).map_or(0, |&n| i32::from(n))
    }

    pub fn total_books(&self) -> i32 {
        self.books.values().map(|&n| i32::from(n)).sum()
    }
}

/// Casting skill for an invested total: the number of triangular steps (2, 6, 12, 20...)
/// the total has passed, plus 10 for an archmage.
pub fn base_casting_skill(points: i32, archmage: bool) -> i32 {
    let (mut skill, mut step, mut threshold) = (0, 0, 0);
    while points > threshold {
        skill += 1;
        step += 2;
        threshold += step;
    }
    skill + if archmage { 10 } else { 0 }
}

/// Smallest invested total whose casting skill reaches `target`.
pub fn skill_points_for(target: i32, archmage: bool) -> i32 {
    let mut points = 0;
    while base_casting_skill(points, archmage) < target {
        points += 1;
    }
    points
}

/// The power base split three ways, as it will be applied at the turn change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PowerSplit {
    pub mana: i32,
    pub skill: i32,
    /// Includes research points from the wizard's cities.
    pub research: i32,
}

/// Why a magic command was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagicError {
    UnknownPlayer,
    /// The spell is not known (cast) or not researchable (research).
    NotAvailable,
    AlreadyResearching,
    AlreadyCasting,
    /// Percentages must add up to 100.
    BadSplit,
    /// This spell cannot be cast on the overland map.
    NotOverland,
    NoSuchTarget,
    /// The spell needs a target and none was given.
    NeedsTarget,
    NothingToResolve,
    NotEnoughGold,
    NotEnoughMana,
}

/// Data-table limits from the original's library set-up, indexed by books - 1.
const COMMON_LIMIT: [usize; 10] = [3, 5, 6, 7, 8, 9, 10, 10, 10, 10];
const UNCOMMON_LIMIT: [usize; 7] = [1, 2, 3, 4, 5, 6, 8];
const RARE_LIMIT: [usize; 9] = [0, 1, 2, 3, 4, 5, 6, 7, 9];
const VERY_RARE_LIMIT: [usize; 9] = [0, 0, 1, 2, 3, 4, 5, 6, 7];

/// Arcane spells a wizard never starts with the right to research.
const NOT_RESEARCHABLE: [&str; 2] = ["spell_of_mastery", "spell_of_return"];

fn is_summoning(def: &SpellDef) -> bool {
    def.effects
        .iter()
        .any(|r| matches!(r.effect, Effect::Summon { .. }))
}

fn is_crafting(def: &SpellDef) -> bool {
    matches!(def.id.as_str(), "enchant_item" | "create_artifact")
}

impl Game {
    pub fn magic(&self, player: PlayerId) -> Option<&Magic> {
        self.player(player).map(|p| &p.magic)
    }

    #[cfg(test)]
    pub(crate) fn magic_mut_for_tests(&mut self, player: PlayerId) -> &mut Magic {
        self.magic_mut(player).unwrap()
    }

    pub(crate) fn magic_mut(&mut self, player: PlayerId) -> Option<&mut Magic> {
        self.player_mut(player).map(|p| &mut p.magic)
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
        let mut magic = Magic::default();
        for &(realm, n) in books {
            if n > 0 {
                magic.books.insert(realm.to_string(), n.min(10));
            }
        }
        magic.retorts = retorts.iter().copied().collect();
        // Starting skill: twice the books (plus 5 for an archmage), found by growing the
        // invested total until the skill reaches it.
        let archmage = magic.has(Retort::Archmage);
        let target = 2 * (magic.total_books() + if archmage { 5 } else { 0 });
        magic.skill_points = skill_points_for(target, archmage);
        magic.skill_left = base_casting_skill(magic.skill_points, archmage);
        self.player_mut(player)
            .ok_or(MagicError::UnknownPlayer)?
            .magic = magic;
        self.init_spell_library(player, start_spells);
        Ok(())
    }

    /// Spells of one realm and rarity in file order (the order the data lists them in).
    pub(crate) fn spells_of(&self, realm: &str, rarity: Rarity) -> Vec<SpellId> {
        self.content
            .rules
            .spells
            .iter()
            .filter(|(_, d)| d.realm == realm && d.rarity == rarity)
            .map(|(id, _)| id)
            .collect()
    }

    /// Pick `count` distinct spells from `pool` at random, skipping any in `have`.
    fn pick_spells(
        &mut self,
        pool: &[SpellId],
        have: &BTreeSet<SpellId>,
        count: usize,
    ) -> Vec<SpellId> {
        let mut left: Vec<SpellId> = pool.iter().copied().filter(|s| !have.contains(s)).collect();
        let mut out = Vec::new();
        while out.len() < count && !left.is_empty() {
            let i = self.roll(left.len() as u32) as usize - 1;
            out.push(left.remove(i));
        }
        out
    }

    /// Known and researchable spells from books: with `n` books in a realm the wizard
    /// starts knowing `n - 1` common spells and may research a growing share of every
    /// rarity (all uncommon from 8 books, all rare and very rare from 10).
    fn init_spell_library(&mut self, player: PlayerId, start_spells: Option<&[SpellId]>) {
        let books: Vec<(String, u8)> = self
            .magic(player)
            .map(|m| m.books.iter().map(|(r, &n)| (r.clone(), n)).collect())
            .unwrap_or_default();
        let mut known = BTreeSet::new();
        let mut available = BTreeSet::new();
        for (realm, n) in books {
            let n = usize::from(n).clamp(1, 10);
            let common = self.spells_of(&realm, Rarity::Common);
            let start = match start_spells {
                Some(chosen) => chosen
                    .iter()
                    .copied()
                    .filter(|s| common.contains(s))
                    .take(n - 1)
                    .collect(),
                None => self.pick_spells(&common, &known, n - 1),
            };
            known.extend(start.iter().copied());
            available.extend(start);
            let limit = COMMON_LIMIT[n - 1].min(common.len());
            let have_common = common.iter().filter(|s| available.contains(s)).count();
            let more = self.pick_spells(&common, &available, limit.saturating_sub(have_common));
            available.extend(more);
            let tiers = [
                (Rarity::Uncommon, n > 7, UNCOMMON_LIMIT[(n - 1).min(6)]),
                (Rarity::Rare, n > 9, RARE_LIMIT[(n - 1).min(8)]),
                (Rarity::VeryRare, n > 9, VERY_RARE_LIMIT[(n - 1).min(8)]),
            ];
            for (rarity, all, limit) in tiers {
                let pool = self.spells_of(&realm, rarity);
                if all {
                    available.extend(pool);
                } else {
                    let picked = self.pick_spells(&pool, &available, limit);
                    available.extend(picked);
                }
            }
        }
        // Arcane spells are open to everyone.
        let arcane: Vec<SpellId> = self
            .content
            .rules
            .spells
            .iter()
            .filter(|(_, d)| d.realm == "arcane" && !NOT_RESEARCHABLE.contains(&d.id.as_str()))
            .map(|(id, _)| id)
            .collect();
        available.extend(arcane);
        if let Some(m) = self.magic_mut(player) {
            m.researchable = available.difference(&known).copied().collect();
            m.known = known;
        }
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

    /// The power base split by the wizard's percentages. Rounding goes to research; if the
    /// research share is switched off, what it would have been goes to skill (or to mana
    /// when skill is off too). Mana Focusing adds a quarter to mana.
    pub fn power_split(&self, player: PlayerId) -> PowerSplit {
        if self.casting_return(player) {
            return PowerSplit::default();
        }
        let Some(m) = self.magic(player) else {
            return PowerSplit::default();
        };
        let power = self.power_base(player);
        let mut mana = (power * i32::from(m.mana_pct) + 50) / 100;
        let mut skill = (power * i32::from(m.skill_pct) + 50) / 100;
        let mut research = power - mana - skill;
        if m.research_pct == 0 && research > 0 {
            if m.skill_pct == 0 {
                mana += research;
            } else {
                skill += research;
            }
            research = 0;
        }
        if m.has(Retort::ManaFocusing) {
            mana += mana / 4;
        }
        PowerSplit {
            mana,
            skill,
            research: research + self.city_research(player),
        }
    }

    /// Percent added to research of `spell`: sage master, conjurer for summoning, realm
    /// masteries, books beyond seven in the realm, runemaster for arcane.
    pub fn research_bonus(&self, player: PlayerId, spell: SpellId) -> i32 {
        let (Some(m), def) = (self.magic(player), self.content.rules.spells.get(spell)) else {
            return 0;
        };
        let mut bonus = 0;
        if m.has(Retort::SageMaster) {
            bonus += 25;
        }
        if m.has(Retort::Conjurer) && is_summoning(def) {
            bonus += 25;
        }
        let mastery = match def.realm.as_str() {
            "nature" => m.has(Retort::NatureMastery),
            "sorcery" => m.has(Retort::SorceryMastery),
            "chaos" => m.has(Retort::ChaosMastery),
            _ => false,
        };
        if mastery {
            bonus += 15;
        }
        if def.realm == "arcane" {
            if m.has(Retort::Runemaster) {
                bonus += 25;
            }
        } else {
            bonus += (m.books_in(&def.realm) - 7).max(0) * 10;
        }
        bonus
    }

    /// Research points that will be paid next turn change, with bonuses.
    pub fn research_income(&self, player: PlayerId) -> i32 {
        let split = self.power_split(player);
        let bonus = self
            .magic(player)
            .and_then(|m| m.researching)
            .map_or(0, |s| self.research_bonus(player, s));
        let research = split.research.max(0);
        research + research * bonus / 100
    }

    /// Mana needed to cast `spell`. Overland casting of a spell that can also be cast in
    /// battle costs five times the battle price; reductions come from books beyond seven,
    /// masteries, artificer, conjurer and runemaster.
    pub fn casting_cost(&self, player: PlayerId, spell: SpellId, in_battle: bool) -> i32 {
        let def = self.content.rules.spells.get(spell);
        let mut cost = def.cost as i32;
        if !in_battle && def.contexts.contains(&CastContext::Battle) {
            cost *= 5;
        }
        cost -= cost * self.casting_cost_reduction(player, spell) / 100;
        // Evil Omens: life and nature spells cost half as much again for every wizard who does
        // not hold the spell (the help does not say; the caster is assumed to be spared).
        if matches!(def.realm.as_str(), "life" | "nature") && self.evil_omens_against(player) {
            cost += cost / 2;
        }
        cost
    }

    fn casting_cost_reduction(&self, player: PlayerId, spell: SpellId) -> i32 {
        let (Some(m), def) = (self.magic(player), self.content.rules.spells.get(spell)) else {
            return 0;
        };
        let mut cut = 0;

        if def.realm == "arcane" {
            if m.has(Retort::Runemaster) {
                cut += 25;
            }
        } else {
            cut += (m.books_in(&def.realm) - 7).max(0) * 10;
        }
        let mastery = match def.realm.as_str() {
            "nature" => m.has(Retort::NatureMastery),
            "sorcery" => m.has(Retort::SorceryMastery),
            "chaos" => m.has(Retort::ChaosMastery),
            _ => false,
        };
        if mastery {
            cut += 15;
        }
        if m.has(Retort::Artificer) && is_crafting(def) {
            cut += 50;
        }
        if m.has(Retort::Conjurer) && is_summoning(def) {
            cut += 25;
        }
        cut
    }

    /// Casting skill now.
    pub fn casting_skill(&self, player: PlayerId) -> i32 {
        self.magic(player).map_or(0, |m| {
            base_casting_skill(m.skill_points, m.has(Retort::Archmage))
        })
    }

    /// Mana per turn needed to keep lasting spells and summoned creatures going.
    pub fn mana_upkeep(&self, player: PlayerId) -> i32 {
        self.upkeep_items(player).iter().map(|i| i.mana).sum()
    }

    // ---- commands ----------------------------------------------------------------

    /// Change how power is split. The three shares must add up to 100.
    pub fn set_power_split(
        &mut self,
        player: PlayerId,
        mana: u8,
        research: u8,
        skill: u8,
    ) -> Result<(), MagicError> {
        if u32::from(mana) + u32::from(research) + u32::from(skill) != 100 {
            return Err(MagicError::BadSplit);
        }
        let m = self.magic_mut(player).ok_or(MagicError::UnknownPlayer)?;
        (m.mana_pct, m.research_pct, m.skill_pct) = (mana, research, skill);
        Ok(())
    }

    /// Choose what to research. The choice stands until the spell is learned.
    pub fn set_research(&mut self, player: PlayerId, spell: SpellId) -> Result<(), MagicError> {
        let cost = self.content.rules.spells.get(spell).research_cost as i32;
        let m = self.magic_mut(player).ok_or(MagicError::UnknownPlayer)?;
        if m.researching.is_some() {
            return Err(MagicError::AlreadyResearching);
        }
        if !m.researchable.contains(&spell) {
            return Err(MagicError::NotAvailable);
        }
        m.researching = Some(spell);
        m.research_remaining = cost;
        Ok(())
    }

    /// Spells the wizard could pick to research now, cheapest first.
    pub fn research_choices(&self, player: PlayerId) -> Vec<SpellId> {
        let Some(m) = self.magic(player) else {
            return Vec::new();
        };
        let mut v: Vec<SpellId> = m.researchable.iter().copied().collect();
        v.sort_by_key(|&s| (self.content.rules.spells.get(s).research_cost, s));
        v
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

    /// All wizards' magic at the turn change: income, research, upkeep, casting.
    pub(crate) fn process_magic(&mut self) -> Vec<Event> {
        let mut events = self.tick_timers();
        self.process_globals();
        let ids: Vec<PlayerId> = self.players.iter().map(|p| p.id).collect();
        for &player in &ids {
            self.apply_magic_power(player);
        }
        for &player in &ids {
            events.extend(self.check_research(player));
        }
        for &player in &ids {
            events.extend(self.apply_mana_upkeep(player));
        }
        for &player in &ids {
            self.apply_casting(player);
        }
        for &player in &ids {
            events.extend(self.finish_casting(player));
        }
        events
    }

    /// Mana goes to the reserve, research to the spell, skill points to casting skill
    /// (an archmage gets half as much again).
    fn apply_magic_power(&mut self, player: PlayerId) {
        let split = self.power_split(player);
        let research = self.research_income(player);
        let Some(p) = self.player_mut(player) else {
            return;
        };
        p.mana = p.mana.saturating_add(split.mana.max(0));
        let m = &mut p.magic;
        if m.researching.is_some() {
            m.research_remaining = (m.research_remaining - research).max(0);
        }
        let skill = split.skill.max(0);
        m.skill_points += if m.has(Retort::Archmage) {
            skill * 3 / 2
        } else {
            skill
        };
    }

    fn check_research(&mut self, player: PlayerId) -> Vec<Event> {
        let Some(m) = self.magic_mut(player) else {
            return Vec::new();
        };
        match m.researching {
            Some(spell) if m.research_remaining == 0 => {
                m.researching = None;
                m.researchable.remove(&spell);
                m.known.insert(spell);
                vec![Event::SpellLearned { player, spell }]
            }
            _ => Vec::new(),
        }
    }

    /// Pay the mana upkeep. If the reserve cannot cover it, the newest lasting spells and
    /// summoned creatures are given up until it can.
    fn apply_mana_upkeep(&mut self, player: PlayerId) -> Vec<Event> {
        let mut events = Vec::new();
        let mut items = self.upkeep_items(player);
        let mut owed: i32 = items.iter().map(|i| i.mana).sum();
        let reserve = self.player(player).map_or(0, |p| p.mana);
        while owed > reserve {
            let Some(item) = items.pop() else {
                break;
            };
            owed -= item.mana;
            events.extend(self.drop_upkeep_item(player, item));
        }
        if let Some(p) = self.player_mut(player) {
            p.mana = (p.mana - owed).max(0);
        }
        events
    }

    /// Spend casting skill and mana on the spell being cast.
    fn apply_casting(&mut self, player: PlayerId) {
        let nominal = self.casting_skill(player);
        let reserve = self.player(player).map_or(0, |p| p.mana);
        let Some(m) = self.magic_mut(player) else {
            return;
        };
        let mut reserve = reserve;
        if let Some(c) = m.casting.as_mut() {
            let units = m.skill_left.min(reserve).min(c.remaining).max(0);
            c.remaining -= units;
            reserve -= units;
            m.skill_left = 0;
            if c.remaining > 0 {
                let units = nominal.min(reserve).min(c.remaining).max(0);
                if c.remaining - units == 0 {
                    c.remaining = 0;
                    reserve -= units;
                    m.skill_left = -units;
                }
            }
        }
        if m.skill_left < 0 {
            m.skill_left += nominal;
        } else {
            m.skill_left = nominal;
        }
        if let Some(p) = self.player_mut(player) {
            p.mana = reserve;
        }
    }

    /// A spell that has been paid for takes effect: at once if it needs no target, else
    /// the wizard is asked for one.
    fn finish_casting(&mut self, player: PlayerId) -> Vec<Event> {
        let Some(c) = self.magic(player).and_then(|m| m.casting) else {
            return Vec::new();
        };
        if c.remaining > 0 {
            return Vec::new();
        }
        if self.spell_needs_target(player, c.spell) {
            return vec![Event::SpellReady {
                player,
                spell: c.spell,
            }];
        }
        self.resolve_cast(player, c.spell, None)
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
        let m = self.magic_mut(player).ok_or(MagicError::UnknownPlayer)?;
        if !m.known.contains(&spell) {
            return Err(MagicError::NotAvailable);
        }
        if m.casting.is_some() {
            return Err(MagicError::AlreadyCasting);
        }
        let instant = m.skill_left.min(reserve).max(0);
        if cost <= instant {
            if needs_target && target.is_none() {
                return Err(MagicError::NeedsTarget);
            }
            self.check_target(player, spell, target)?;
            let m = self.magic_mut(player).ok_or(MagicError::UnknownPlayer)?;
            m.skill_left -= cost;
            if let Some(p) = self.player_mut(player) {
                p.mana -= cost;
            }
            return Ok(self.resolve_cast(player, spell, target));
        }
        m.casting = Some(Casting {
            spell,
            remaining: cost,
        });
        Ok(Vec::new())
    }

    /// Give up the spell being cast; the mana already paid is lost.
    pub fn cancel_cast(&mut self, player: PlayerId) -> Result<(), MagicError> {
        if self.casting_return(player) {
            return Err(MagicError::NotAvailable);
        }
        let m = self.magic_mut(player).ok_or(MagicError::UnknownPlayer)?;
        m.casting = None;
        Ok(())
    }

    /// Choose the target of a spell that has been fully paid for.
    pub fn finish_cast_with_target(
        &mut self,
        player: PlayerId,
        target: crate::rules::exec::Target,
    ) -> Result<Vec<Event>, MagicError> {
        let c = self
            .magic(player)
            .and_then(|m| m.casting)
            .filter(|c| c.remaining <= 0)
            .ok_or(MagicError::NothingToResolve)?;
        self.check_target(player, c.spell, Some(target))?;
        Ok(self.resolve_cast(player, c.spell, Some(target)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::Position;
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::rules::exec::Target;
    use crate::rules::Stat;
    use crate::save::Snapshot;
    use crate::testutil::content_with_magic;
    use crate::Command;

    const P0: PlayerId = PlayerId(0);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    fn spell(g: &Game, id: &str) -> SpellId {
        g.content.rules.spells.id_of(id).unwrap()
    }

    /// 24x14 grassland, one wizard with 2 life books and 100 mana.
    fn world() -> Game {
        let c = content_with_magic();
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut game = Game::new(7);
        game.content = c;
        game.content.rules.config.tax_unrest_percent = vec![0, 0, 0, 30, 45, 60, 75];
        game.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        let mut p = Player::new(P0, "Merlin");
        p.gold = 500;
        p.tax_rate = 2;
        p.mana = 100;
        game.players.push(p);
        game.init_wizard(P0, &[("life", 2)], &[]).unwrap();
        game
    }

    /// The first city is the wizard's fortress, later ones are ordinary cities.
    fn found(g: &mut Game, pos: Position, pop: u32) -> hecs::Entity {
        let race = g.content.rules.races.id_of("elf");
        let name = "Town".into();
        if g.player(P0).unwrap().fortress.is_none() {
            g.apply(Command::PlaceFortress {
                player: P0,
                pos,
                name,
                race,
                population: pop,
            });
        } else {
            g.apply(Command::FoundCity {
                owner: P0,
                pos,
                name,
                race,
                population: pop,
            });
        }
        g.city_at(pos).unwrap()
    }

    fn add_building(g: &mut Game, city: hecs::Entity, id: &str) {
        let b = g.content.rules.buildings.id_of(id).unwrap();
        g.world
            .get::<&mut crate::economy::Buildings>(city)
            .unwrap()
            .0
            .push(b);
    }

    #[test]
    fn casting_skill_follows_triangular_numbers() {
        // Skill k needs more than (k-1)*k points: 2, 6, 12, 20...
        assert_eq!(base_casting_skill(0, false), 0);
        assert_eq!(base_casting_skill(1, false), 1);
        assert_eq!(base_casting_skill(2, false), 1);
        assert_eq!(base_casting_skill(3, false), 2);
        assert_eq!(base_casting_skill(56, false), 7);
        assert_eq!(base_casting_skill(57, false), 8);
        assert_eq!(base_casting_skill(0, true), 10, "archmage adds ten");
        assert_eq!(skill_points_for(8, false), 57);
        assert_eq!(base_casting_skill(skill_points_for(14, true), true), 14);
    }

    #[test]
    fn a_new_wizard_starts_with_books_spells_and_skill() {
        let mut g = world();
        g.init_wizard(P0, &[("life", 3), ("chaos", 2)], &[Retort::Archmage])
            .unwrap();
        let m = g.magic(P0).unwrap();
        // Three books and two books: 2 + 1 common spells known.
        assert_eq!(m.known.len(), 3);
        assert!(m.known.is_disjoint(&m.researchable));
        let in_realm = |set: &BTreeSet<SpellId>, realm: &str| {
            set.iter()
                .filter(|&&s| g.content.rules.spells.get(s).realm == realm)
                .count()
        };
        // Life, 3 books: 6 common + 3 uncommon + 2 rare + 1 very rare, of which 2 known.
        assert_eq!(in_realm(&m.known, "life"), 2);
        assert_eq!(in_realm(&m.researchable, "life"), 12 - 2);
        // Chaos, 2 books: 5 + 2 + 1 + 0, of which 1 known.
        assert_eq!(in_realm(&m.researchable, "chaos"), 8 - 1);
        // Arcane is open to everyone, except the Spell of Mastery.
        assert!(m.researchable.contains(&spell(&g, "magic_spirit")));
        assert!(!m.researchable.contains(&spell(&g, "spell_of_mastery")));
        // 5 books + 5 for the archmage, doubled.
        assert_eq!(g.casting_skill(P0), 20);
    }

    #[test]
    fn the_same_seed_gives_the_same_spell_library() {
        let (a, b) = (world(), world());
        assert_eq!(a.magic(P0), b.magic(P0));
    }

    #[test]
    fn power_comes_from_cities_and_is_split_by_percentages() {
        let mut g = world();
        assert_eq!(g.power_split(P0), PowerSplit::default());
        let city = found(&mut g, at(10, 6), 4);
        add_building(&mut g, city, "temple");
        add_building(&mut g, city, "library");
        // Four elves make two mana, the temple two; the library makes two research.
        // Plus two books' worth from the fortress city.
        assert_eq!(g.power_base(P0), 4 + 2);
        assert_eq!(g.city_research(P0), 2);
        assert_eq!(
            g.power_split(P0),
            PowerSplit {
                mana: 2,
                skill: 2,
                research: 2 + 2
            }
        );
        g.set_power_split(P0, 100, 0, 0).unwrap();
        assert_eq!(
            g.power_split(P0),
            PowerSplit {
                mana: 6,
                skill: 0,
                research: 2
            }
        );
        assert_eq!(g.set_power_split(P0, 50, 40, 20), Err(MagicError::BadSplit));
        g.magic_mut(P0)
            .unwrap()
            .retorts
            .insert(Retort::ManaFocusing);
        assert_eq!(g.power_split(P0).mana, 7, "a quarter more mana");
    }

    #[test]
    fn retorts_and_extra_books_change_research_and_cost() {
        let mut g = world();
        g.init_wizard(P0, &[("life", 9)], &[Retort::SageMaster])
            .unwrap();
        let mend = spell(&g, "mend");
        // Sage master 25 plus two books over seven, 10 each.
        assert_eq!(g.research_bonus(P0, mend), 45);
        // Two books over seven take a fifth off the cost.
        assert_eq!(g.casting_cost(P0, mend, false), 8);
        let runes = spell(&g, "magic_spirit");
        assert_eq!(g.research_bonus(P0, runes), 25, "arcane gets no book bonus");
        assert_eq!(g.casting_cost(P0, runes, false), 30);
        // A spell that can also be cast in battle costs five times as much overland.
        let strength = spell(&g, "giant_strength");
        assert_eq!(g.casting_cost(P0, strength, true), 8);
        assert_eq!(g.casting_cost(P0, strength, false), 40);
    }

    #[test]
    fn research_takes_turns_and_ends_with_a_new_spell() {
        let mut g = world();
        let target = spell(&g, "magic_spirit");
        let other = spell(&g, "dispel_magic");
        found(&mut g, at(10, 6), 4);
        g.set_power_split(P0, 0, 100, 0).unwrap();
        assert_eq!(
            g.set_research(P0, spell(&g, "spell_of_mastery")),
            Err(MagicError::NotAvailable)
        );
        g.set_research(P0, target).unwrap();
        assert_eq!(g.magic(P0).unwrap().research_remaining, 20);
        assert_eq!(
            g.set_research(P0, other),
            Err(MagicError::AlreadyResearching)
        );
        // A 4-citizen city makes 2 mana (elves) and the fortress 2: all of it research.
        let mut learned = None;
        for turn in 1..=30 {
            let events = g.apply(Command::EndTurn);
            if events
                .iter()
                .any(|e| matches!(e, Event::SpellLearned { spell, .. } if *spell == target))
            {
                learned = Some(turn);
                break;
            }
        }
        let turns = learned.expect("the spell is learned");
        assert!(
            (3..=8).contains(&turns),
            "20 points at about 6 a turn: {turns}"
        );
        let m = g.magic(P0).unwrap();
        assert!(m.known.contains(&target));
        assert!(!m.researchable.contains(&target));
        assert_eq!(m.researching, None);
        g.set_research(P0, other).unwrap();
    }

    #[test]
    fn mana_income_fills_the_reserve() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), 4);
        add_building(&mut g, city, "temple");
        g.set_power_split(P0, 100, 0, 0).unwrap();
        let before = g.player(P0).unwrap().mana;
        g.apply(Command::EndTurn);
        assert_eq!(g.player(P0).unwrap().mana, before + 6);
    }

    #[test]
    fn an_affordable_spell_takes_effect_at_once() {
        let mut g = world();
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let unit = g.spawn_unit(P0, at(3, 3), spear);
        g.world.insert_one(unit, crate::units::Hurt(3)).unwrap();
        let mend = spell(&g, "mend");
        g.magic_mut(P0).unwrap().known.insert(mend);
        g.magic_mut(P0).unwrap().skill_left = 50;
        let mana = g.player(P0).unwrap().mana;
        // A spell with a target needs one.
        assert_eq!(g.start_cast(P0, mend, None), Err(MagicError::NeedsTarget));
        let events = g.apply(Command::CastSpell {
            player: P0,
            spell: mend,
            target: Some(Target::Unit(unit)),
        });
        assert!(events.contains(&Event::SpellCast {
            player: P0,
            spell: mend
        }));
        assert_eq!(g.unit_hits(unit).0, g.unit_hits(unit).1, "healed");
        assert_eq!(g.player(P0).unwrap().mana, mana - 10);
        assert_eq!(g.magic(P0).unwrap().skill_left, 40);
        assert_eq!(g.magic(P0).unwrap().casting, None);
    }

    #[test]
    fn spells_need_a_known_spell_and_a_valid_target() {
        let mut g = world();
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let mine = g.spawn_unit(P0, at(3, 3), spear);
        let theirs = g.spawn_unit(PlayerId(1), at(4, 3), spear);
        let (mend, zap) = (spell(&g, "mend"), spell(&g, "zap"));
        g.magic_mut(P0).unwrap().skill_left = 50;
        assert_eq!(
            g.start_cast(P0, mend, Some(Target::Unit(mine))),
            Err(MagicError::NotAvailable),
            "not known yet"
        );
        g.magic_mut(P0).unwrap().known.extend([mend, zap]);
        assert_eq!(
            g.check_target(P0, mend, Some(Target::Unit(theirs))),
            Err(MagicError::NoSuchTarget),
            "healing is for your own units"
        );
        assert_eq!(g.check_target(P0, zap, Some(Target::Unit(theirs))), Ok(()));
        assert_eq!(
            g.check_target(P0, zap, Some(Target::Unit(mine))),
            Err(MagicError::NoSuchTarget)
        );
        // Three damage kills a spear-sized unit with that few hit points or hurts it.
        g.start_cast(P0, zap, Some(Target::Unit(theirs))).unwrap();
        let (left, max) = g.unit_hits(theirs);
        assert!(left < max || !g.world.contains(theirs));
    }

    #[test]
    fn a_dear_spell_is_paid_over_turns_then_aimed_and_costs_upkeep() {
        let mut g = world();
        // A galley eats no food and costs no gold, so it outlives the long wait.
        let spear = g.content.rules.units.id_of("galley").unwrap();
        found(&mut g, at(10, 6), 8);
        // All power to research, so the mana reserve only moves by what is spent.
        g.set_power_split(P0, 0, 100, 0).unwrap();
        let unit = g.spawn_unit(P0, at(3, 3), spear);
        g.balance_farmers(P0);
        let strength = spell(&g, "giant_strength");
        g.magic_mut(P0).unwrap().known.insert(strength);
        let melee = g.unit_stat(unit, Stat::Melee);
        // 40 mana overland; the wizard's skill is 4, so it takes many turns.
        assert_eq!(g.casting_skill(P0), 4);
        assert_eq!(g.start_cast(P0, strength, None), Ok(vec![]));
        assert_eq!(g.magic(P0).unwrap().casting.unwrap().remaining, 40);
        assert_eq!(
            g.start_cast(P0, strength, None),
            Err(MagicError::AlreadyCasting)
        );
        let mut ready_after = None;
        for turn in 1..=20 {
            let events = g.apply(Command::EndTurn);
            if events.contains(&Event::SpellReady {
                player: P0,
                spell: strength,
            }) {
                ready_after = Some(turn);
                break;
            }
        }
        // Forty mana at four a turn is ten payments, but the last is borrowed from the next
        // turn's skill, as in the original.
        assert_eq!(ready_after, Some(9));
        assert_eq!(g.player(P0).unwrap().mana, 60, "the 40 mana was paid");
        let events = g.apply(Command::ResolveCast {
            player: P0,
            target: Target::Unit(unit),
        });
        assert!(events.contains(&Event::SpellCast {
            player: P0,
            spell: strength
        }));
        assert_eq!(g.unit_stat(unit, Stat::Melee), melee + 1);
        assert_eq!(g.mana_upkeep(P0), 2);
        let mana = g.player(P0).unwrap().mana;
        g.apply(Command::EndTurn);
        assert_eq!(g.player(P0).unwrap().mana, mana - 2);

        // With no mana left the enchantment lapses.
        g.player_mut(P0).unwrap().mana = 1;
        let events = g.apply(Command::EndTurn);
        assert!(events.contains(&Event::EnchantmentLapsed {
            player: P0,
            spell: strength
        }));
        assert_eq!(g.unit_stat(unit, Stat::Melee), melee);
        assert_eq!(g.mana_upkeep(P0), 0);
    }

    #[test]
    fn cancelling_loses_what_was_paid() {
        let mut g = world();
        let strength = spell(&g, "giant_strength");
        g.magic_mut(P0).unwrap().known.insert(strength);
        g.start_cast(P0, strength, None).unwrap();
        g.apply(Command::EndTurn);
        let mana = g.player(P0).unwrap().mana;
        assert!(mana < 100);
        g.apply(Command::CancelCast { player: P0 });
        assert_eq!(g.magic(P0).unwrap().casting, None);
        assert_eq!(g.player(P0).unwrap().mana, mana, "no refund");
    }

    #[test]
    fn summoning_creates_units_at_the_chosen_tile() {
        let mut g = world();
        let call = spell(&g, "call_spears");
        g.magic_mut(P0).unwrap().known.insert(call);
        g.magic_mut(P0).unwrap().skill_left = 99;
        g.apply(Command::CastSpell {
            player: P0,
            spell: call,
            target: Some(Target::Tile(at(5, 5))),
        });
        assert_eq!(g.units_at(at(5, 5)).len(), 2);
    }

    #[test]
    fn a_city_enchantment_is_cast_and_paid_for() {
        let mut g = world();
        let city = found(&mut g, at(10, 6), 4);
        let famine = spell(&g, "famine");
        g.magic_mut(P0).unwrap().known.insert(famine);
        g.magic_mut(P0).unwrap().skill_left = 99;
        let food = g.city_economy(city).unwrap().food;
        g.apply(Command::CastSpell {
            player: P0,
            spell: famine,
            target: Some(Target::City(city)),
        });
        assert!(g.city_economy(city).unwrap().food < food);
        assert_eq!(g.mana_upkeep(P0), 1);
    }

    #[test]
    fn dispelling_removes_enchantments_by_strength() {
        let mut g = world();
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let unit = g.spawn_unit(P0, at(3, 3), spear);
        let strength = spell(&g, "giant_strength");
        let melee = g.unit_stat(unit, Stat::Melee);
        g.attach_enchantment(Target::Unit(unit), strength, P0);
        assert_eq!(g.unit_stat(unit, Stat::Melee), melee + 1);
        {
            use crate::rules::exec::EffectHost;
            let mut host = crate::cast::tests_support::host(&mut g);
            let mut rng = rand_pcg::Pcg64Mcg::new(1);
            // An overwhelming dispel always works.
            assert_eq!(host.dispel(Target::Unit(unit), 100_000, &mut rng), 1);
        }
        assert_eq!(g.unit_stat(unit, Stat::Melee), melee);
    }

    #[test]
    fn saves_keep_the_wizard_the_casting_and_unit_spells() {
        let mut g = world();
        let spear = g.content.rules.units.id_of("spear").unwrap();
        found(&mut g, at(10, 6), 8);
        g.balance_farmers(P0);
        let unit = g.spawn_unit(P0, at(3, 3), spear);
        let strength = spell(&g, "giant_strength");
        g.attach_enchantment(Target::Unit(unit), strength, P0);
        g.set_research(P0, spell(&g, "magic_spirit")).unwrap();
        g.set_power_split(P0, 20, 70, 10).unwrap();
        g.magic_mut(P0).unwrap().known.insert(strength);
        g.start_cast(P0, strength, None).unwrap();
        g.apply(Command::EndTurn);
        let text = g.snapshot().to_text().unwrap();
        let back = Snapshot::from_text(&text).unwrap();
        let loaded = back.restore(content_with_magic()).unwrap();
        assert_eq!(loaded.magic(P0), g.magic(P0));
        assert_eq!(loaded.snapshot(), g.snapshot());
        let unit = loaded.all_units()[0];
        assert_eq!(
            loaded.unit_stat(unit, Stat::Melee),
            g.unit_stat(g.all_units()[0], Stat::Melee)
        );
    }

    #[test]
    fn old_saves_without_wizards_still_load() {
        let mut g = world();
        g.players[0].magic = Magic::default();
        let text = g.snapshot().to_text().unwrap();
        // Take the wizard out of the text the way an old save would lack it.
        let start = text.find("magic:").unwrap();
        let end = text[start..].find("skill_left").unwrap();
        let close = text[start + end..].find("),").unwrap();
        let old = format!("{}{}", &text[..start], &text[start + end + close + 2..]);
        let back = Snapshot::from_text(&old).unwrap();
        let loaded = back.restore(content_with_magic()).unwrap();
        assert_eq!(loaded.magic(P0), Some(&Magic::default()));
    }
}
