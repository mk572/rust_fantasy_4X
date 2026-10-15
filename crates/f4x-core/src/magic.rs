//! Wizards: spell books, retorts, the power split, research, casting and mana upkeep.
//!
//! Formulas follow the original game's code (see `docs/magic.md` for the list and for where
//! this differs). The wizard is not an entity: it is a [`Magic`] record on the [`Player`].
//!
//! This file is the wizard's own side (task 180, `architecture-plan.md`): [`Magic`] holds the
//! state and the rules that need nothing but the content and the random numbers, as methods
//! that take what they use (`&Content`, `&mut GameRng`, numbers the game worked out). What
//! needs the world (power from the cities, the upkeep items, resolving a spell, the turn
//! change) is in `magic_game.rs`, which calls these.
//!
//! [`Player`]: crate::player::Player

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::cast::UpkeepItem;

use crate::content::Content;
use crate::player::Player;
use crate::rng::GameRng;
use crate::rules::effects::Effect;
use crate::rules::{Rarity, SpellDef, SpellId};

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

/// How many spells are offered for research at once.
pub const RESEARCH_SLOTS: usize = 8;

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
    /// The spells on offer for research now (at most [`RESEARCH_SLOTS`]), fastest first:
    /// the wizard picks the next research from these (`Magic::researchable` is what
    /// the books allow in all).
    pub candidates: Vec<SpellId>,
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
            candidates: Vec::new(),
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

/// Spells of one realm and rarity in file order (the order the data lists them in).
pub fn spells_of(content: &Content, realm: &str, rarity: Rarity) -> Vec<SpellId> {
    content
        .rules
        .spells
        .iter()
        .filter(|(_, d)| d.realm == realm && d.rarity == rarity)
        .map(|(id, _)| id)
        .collect()
}

/// Pick `count` distinct spells from `pool` at random, skipping any in `have`.
fn pick_spells(
    rng: &mut GameRng,
    pool: &[SpellId],
    have: &BTreeSet<SpellId>,
    count: usize,
) -> Vec<SpellId> {
    let mut left: Vec<SpellId> = pool.iter().copied().filter(|s| !have.contains(s)).collect();
    let mut out = Vec::new();
    while out.len() < count && !left.is_empty() {
        let i = rng.roll(left.len() as u32) as usize - 1;
        out.push(left.remove(i));
    }
    out
}

/// The turn's income reaches the wizard: mana goes to the reserve, research to the spell being
/// researched and skill points to casting skill (`Magic::take_power`). `split` and `research`
/// are what the wizard's cities and fortress make, worked out by the game.
pub(crate) fn collect_power(player: &mut Player, split: &PowerSplit, research: i32) {
    let mana = player.magic.take_power(split, research);
    player.mana = player.mana.saturating_add(mana);
}

/// Spend casting skill and mana on the spell being cast (`Magic::pay_casting`).
pub(crate) fn pay_for_casting(player: &mut Player) {
    player.mana = player.magic.pay_casting(player.mana);
}

/// What to do about the mana upkeep: the items to give up (newest first) so that the reserve
/// can pay the rest, and what is then owed. A Channeler's items count half (`divisor` 2).
pub(crate) fn upkeep_plan(
    items: &[UpkeepItem],
    divisor: i32,
    reserve: i32,
) -> (Vec<UpkeepItem>, i32) {
    let mut owed: i32 = items.iter().map(|i| i.mana).sum::<i32>() / divisor;
    let mut rest = items.to_vec();
    let mut dropped = Vec::new();
    while owed > reserve {
        let Some(item) = rest.pop() else {
            break;
        };
        owed -= item.mana / divisor;
        dropped.push(item);
    }
    (dropped, owed)
}

/// How a spell cast on the overland map begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastStart {
    /// The skill and mana on hand pay for all of it: it takes effect now.
    Now,
    /// It is paid over turns.
    Paid,
}

impl Magic {
    /// A new wizard's record: books and retorts, and the starting casting skill (twice the books,
    /// plus 5 books' worth for an archmage), found by growing the invested total until the skill
    /// reaches it. The spell library comes with [`Magic::init_library`].
    pub fn new(books: &[(&str, u8)], retorts: &[Retort]) -> Magic {
        let mut magic = Magic::default();
        for &(realm, n) in books {
            if n > 0 {
                magic.books.insert(realm.to_string(), n.min(11));
            }
        }
        magic.retorts = retorts.iter().copied().collect();
        let archmage = magic.has(Retort::Archmage);
        let target = 2 * (magic.total_books() + if archmage { 5 } else { 0 });
        magic.skill_points = skill_points_for(target, archmage);
        magic.skill_left = base_casting_skill(magic.skill_points, archmage);
        magic
    }

    /// Known and researchable spells from books: with `n` books in a realm the wizard
    /// starts knowing `n - 1` common spells and may research a growing share of every
    /// rarity (all uncommon from 8 books, all rare and very rare from 10). The offer for
    /// research is emptied; fill it with [`Magic::refill_research`].
    pub fn init_library(
        &mut self,
        content: &Content,
        rng: &mut GameRng,
        start_spells: Option<&[SpellId]>,
    ) {
        let books: Vec<(String, u8)> = self.books.iter().map(|(r, &n)| (r.clone(), n)).collect();
        let mut known = BTreeSet::new();
        let mut available = BTreeSet::new();
        for (realm, n) in books {
            let n = usize::from(n).clamp(1, 11);
            let common = spells_of(content, &realm, Rarity::Common);
            if n == 11 {
                // Everything is open; all the common spells are known, and so are the two
                // uncommon and the one rare spell the wizard picked.
                known.extend(common.iter().copied());
                for (rarity, count) in [(Rarity::Uncommon, 2), (Rarity::Rare, 1)] {
                    let pool = spells_of(content, &realm, rarity);
                    let picked: Vec<SpellId> = match start_spells {
                        Some(chosen) => chosen
                            .iter()
                            .copied()
                            .filter(|s| pool.contains(s))
                            .take(count)
                            .collect(),
                        None => pick_spells(rng, &pool, &known, count),
                    };
                    known.extend(picked);
                }
                for rarity in [
                    Rarity::Common,
                    Rarity::Uncommon,
                    Rarity::Rare,
                    Rarity::VeryRare,
                ] {
                    available.extend(spells_of(content, &realm, rarity));
                }
                continue;
            }
            let start = match start_spells {
                Some(chosen) => chosen
                    .iter()
                    .copied()
                    .filter(|s| common.contains(s))
                    .take(n - 1)
                    .collect(),
                None => pick_spells(rng, &common, &known, n - 1),
            };
            known.extend(start.iter().copied());
            available.extend(start);
            let limit = COMMON_LIMIT[n - 1].min(common.len());
            let have_common = common.iter().filter(|s| available.contains(s)).count();
            let more = pick_spells(rng, &common, &available, limit.saturating_sub(have_common));
            available.extend(more);
            let tiers = [
                (Rarity::Uncommon, n > 7, UNCOMMON_LIMIT[(n - 1).min(6)]),
                (Rarity::Rare, n > 9, RARE_LIMIT[(n - 1).min(8)]),
                (Rarity::VeryRare, n > 9, VERY_RARE_LIMIT[(n - 1).min(8)]),
            ];
            for (rarity, all, limit) in tiers {
                let pool = spells_of(content, &realm, rarity);
                if all {
                    available.extend(pool);
                } else {
                    let picked = pick_spells(rng, &pool, &available, limit);
                    available.extend(picked);
                }
            }
        }
        // Arcane spells are open to everyone.
        let arcane: Vec<SpellId> = content
            .rules
            .spells
            .iter()
            .filter(|(_, d)| d.realm == "arcane" && !NOT_RESEARCHABLE.contains(&d.id.as_str()))
            .map(|(id, _)| id)
            .collect();
        available.extend(arcane);
        self.researchable = available.difference(&known).copied().collect();
        self.known = known;
        self.candidates.clear();
    }

    /// Casting skill now.
    pub fn casting_skill(&self) -> i32 {
        base_casting_skill(self.skill_points, self.has(Retort::Archmage))
    }

    /// Mana needed to cast `def`. Overland casting of a spell that can also be cast in battle
    /// costs five times the battle price; reductions come from books beyond seven, masteries,
    /// artificer, conjurer and runemaster. `evil_omens`: life and nature spells cost half as
    /// much again (the help does not say; the caster is assumed to be spared, so the game says
    /// whether Evil Omens is against this wizard).
    pub fn casting_cost(&self, def: &SpellDef, in_battle: bool, evil_omens: bool) -> i32 {
        let mut cost = def.cost as i32;
        if !in_battle && def.overland_x5 {
            cost *= 5;
        }
        cost -= cost * self.cost_reduction(def) / 100;
        if matches!(def.realm.as_str(), "life" | "nature") && evil_omens {
            cost += cost / 2;
        }
        cost
    }

    /// Percent off the cost of `def`.
    pub fn cost_reduction(&self, def: &SpellDef) -> i32 {
        let mut cut = 0;
        if def.realm == "arcane" {
            if self.has(Retort::Runemaster) {
                cut += 25;
            }
        } else {
            cut += (self.books_in(&def.realm) - 7).max(0) * 10;
        }
        if self.has_mastery(&def.realm) {
            cut += 15;
        }
        if self.has(Retort::Artificer) && is_crafting(def) {
            cut += 50;
        }
        if self.has(Retort::Conjurer) && is_summoning(def) {
            cut += 25;
        }
        cut
    }

    /// Whether the wizard has the mastery of this realm.
    fn has_mastery(&self, realm: &str) -> bool {
        match realm {
            "nature" => self.has(Retort::NatureMastery),
            "sorcery" => self.has(Retort::SorceryMastery),
            "chaos" => self.has(Retort::ChaosMastery),
            _ => false,
        }
    }

    /// Percent added to research of `def`: sage master, conjurer for summoning, realm
    /// masteries, books beyond seven in the realm, runemaster for arcane.
    pub fn research_bonus(&self, def: &SpellDef) -> i32 {
        let mut bonus = 0;
        if self.has(Retort::SageMaster) {
            bonus += 25;
        }
        if self.has(Retort::Conjurer) && is_summoning(def) {
            bonus += 25;
        }
        if self.has_mastery(&def.realm) {
            bonus += 15;
        }
        if def.realm == "arcane" {
            if self.has(Retort::Runemaster) {
                bonus += 25;
            }
        } else {
            bonus += (self.books_in(&def.realm) - 7).max(0) * 10;
        }
        bonus
    }

    /// The power base split by the wizard's percentages. Rounding goes to research; if the
    /// research share is switched off, what it would have been goes to skill (or to mana
    /// when skill is off too). Mana Focusing adds a quarter to mana. `city_research` is what
    /// the wizard's cities produce, added to the research share.
    pub fn split_power(&self, power: i32, city_research: i32) -> PowerSplit {
        let mut mana = (power * i32::from(self.mana_pct) + 50) / 100;
        let mut skill = (power * i32::from(self.skill_pct) + 50) / 100;
        let mut research = power - mana - skill;
        if self.research_pct == 0 && research > 0 {
            if self.skill_pct == 0 {
                mana += research;
            } else {
                skill += research;
            }
            research = 0;
        }
        if self.has(Retort::ManaFocusing) {
            mana += mana / 4;
        }
        PowerSplit {
            mana,
            skill,
            research: research + city_research,
        }
    }

    /// Research points that will be paid next turn change, with bonuses.
    pub fn research_income(&self, content: &Content, split: &PowerSplit) -> i32 {
        let bonus = self
            .researching
            .map_or(0, |s| self.research_bonus(content.rules.spells.get(s)));
        let research = split.research.max(0);
        research + research * bonus / 100
    }

    /// Change how power is split. The three shares must add up to 100.
    pub fn set_split(&mut self, mana: u8, research: u8, skill: u8) -> Result<(), MagicError> {
        if u32::from(mana) + u32::from(research) + u32::from(skill) != 100 {
            return Err(MagicError::BadSplit);
        }
        (self.mana_pct, self.research_pct, self.skill_pct) = (mana, research, skill);
        Ok(())
    }

    /// What researching `spell` costs this wizard. The Spell of Mastery's cost is its listed
    /// 60,000 less half the research cost of every spell the wizard already knows (the original
    /// takes that off as each spell is gained; counting the starting spells too is our
    /// simplification, so the sum needs no saved field). Any other spell costs its listed
    /// amount.
    pub fn research_cost(&self, content: &Content, spell: SpellId) -> i32 {
        let spells = &content.rules.spells;
        let listed = spells.get(spell).research_cost as i32;
        if spells.get(spell).id != "spell_of_mastery" {
            return listed;
        }
        let known: i32 = self
            .known
            .iter()
            .map(|&s| spells.get(s).research_cost as i32 / 2)
            .sum();
        (listed - known).max(0)
    }

    /// Choose what to research. The choice stands until the spell is learned.
    pub fn choose_research(&mut self, content: &Content, spell: SpellId) -> Result<(), MagicError> {
        if self.researching.is_some() {
            return Err(MagicError::AlreadyResearching);
        }
        if !self.researchable.contains(&spell) || !self.candidates.contains(&spell) {
            return Err(MagicError::NotAvailable);
        }
        self.researching = Some(spell);
        self.research_remaining = self.research_cost(content, spell);
        Ok(())
    }

    /// The spells the books would let the wizard add to the offer next (the original's
    /// `Build_Research_List`): in each realm every researchable spell of the lowest rarity that
    /// still has one, so a realm's commons are offered before its uncommons. Arcane spells go
    /// in three steps (the first three, the next five, then the rest).
    fn research_pool(&self, content: &Content) -> Vec<SpellId> {
        let rules = &content.rules;
        let open = |s: &SpellId| self.researchable.contains(s) && !self.candidates.contains(s);
        let mut pool = Vec::new();
        for realm in ["nature", "sorcery", "chaos", "life", "death"] {
            let mut of: Vec<(Rarity, SpellId)> = rules
                .spells
                .iter()
                .filter(|(id, d)| d.realm == realm && open(id))
                .map(|(id, d)| (d.rarity, id))
                .collect();
            if let Some(lowest) = of.iter().map(|&(r, _)| r).min() {
                of.retain(|&(r, _)| r == lowest);
                pool.extend(of.into_iter().map(|(_, id)| id));
            }
        }
        let arcane: Vec<SpellId> = rules
            .spells
            .iter()
            .filter(|(_, d)| d.realm == "arcane")
            .map(|(id, _)| id)
            .collect();
        let step = |at: usize| match at {
            0..=2 => 0,
            3..=7 => 1,
            _ => 2,
        };
        let open_arcane: Vec<(usize, SpellId)> = arcane
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, s)| open(s))
            .collect();
        if let Some(first) = open_arcane.iter().map(|&(i, _)| step(i)).min() {
            pool.extend(
                open_arcane
                    .into_iter()
                    .filter(|&(i, _)| step(i) == first)
                    .map(|(_, s)| s),
            );
        }
        pool
    }

    /// Top the offer for research up to [`RESEARCH_SLOTS`] spells from the pool (the pool is
    /// worked out afresh after each pick, so a realm's next rarity opens as its last cheaper
    /// spell goes on offer), drop spells that are no longer researchable, and sort the offer by
    /// the time each would take at `income` research points a turn. Called each turn, when a
    /// wizard is made and when books are added.
    pub fn refill_research(&mut self, content: &Content, rng: &mut GameRng, income: i32) {
        let keep: Vec<SpellId> = self
            .candidates
            .iter()
            .copied()
            .filter(|s| self.researchable.contains(s))
            .collect();
        self.candidates = keep;
        while self.candidates.len() < RESEARCH_SLOTS {
            let pool = self.research_pool(content);
            if pool.is_empty() {
                break;
            }
            let pick = pool[rng.roll(pool.len() as u32) as usize - 1];
            self.candidates.push(pick);
        }
        // With nothing left to offer, the Spell of Mastery is: the last thing to research.
        if self.candidates.len() < RESEARCH_SLOTS {
            if let Some(som) = content.rules.spells.id_of("spell_of_mastery") {
                if !self.known.contains(&som)
                    && !self.candidates.contains(&som)
                    && self.research_pool(content).is_empty()
                {
                    self.researchable.insert(som);
                    self.candidates.push(som);
                }
            }
        }
        let income = income.max(0);
        let time = |s: SpellId| {
            let def = content.rules.spells.get(s);
            let bonus = self.research_bonus(def);
            let cost = self.research_cost(content, s) * 100 / (100 + bonus);
            if income == 0 {
                cost
            } else {
                cost / income
            }
        };
        let mut sorted = self.candidates.clone();
        sorted.sort_by_key(|&s| time(s));
        self.candidates = sorted;
    }

    /// The spell becomes known, as if researched (a gift or an exchange between wizards).
    /// The offer is refilled by the caller.
    pub fn learn(&mut self, spell: SpellId) {
        self.researchable.remove(&spell);
        self.candidates.retain(|&s| s != spell);
        if self.researching == Some(spell) {
            self.researching = None;
            self.research_remaining = 0;
        }
        self.known.insert(spell);
    }

    /// The turn's income of research and skill: research goes to the spell, skill points to
    /// casting skill (an archmage gets half as much again). Returns the mana for the reserve.
    pub fn take_power(&mut self, split: &PowerSplit, research: i32) -> i32 {
        if self.researching.is_some() {
            self.research_remaining = (self.research_remaining - research).max(0);
        }
        let skill = split.skill.max(0);
        self.skill_points += if self.has(Retort::Archmage) {
            skill * 3 / 2
        } else {
            skill
        };
        split.mana.max(0)
    }

    /// The spell being researched is learned if its research is paid for.
    pub fn finish_research(&mut self) -> Option<SpellId> {
        match self.researching {
            Some(spell) if self.research_remaining == 0 => {
                self.researching = None;
                self.researchable.remove(&spell);
                self.candidates.retain(|&s| s != spell);
                self.known.insert(spell);
                Some(spell)
            }
            _ => None,
        }
    }

    /// 2 for a Channeler (half the spell maintenance), else 1.
    pub fn upkeep_divisor(&self) -> i32 {
        if self.has(Retort::Channeler) {
            2
        } else {
            1
        }
    }

    /// Spend casting skill and mana on the spell being cast. Returns the reserve left.
    pub fn pay_casting(&mut self, reserve: i32) -> i32 {
        let nominal = self.casting_skill();
        let mut reserve = reserve;
        if let Some(c) = self.casting.as_mut() {
            let units = self.skill_left.min(reserve).min(c.remaining).max(0);
            c.remaining -= units;
            reserve -= units;
            self.skill_left = 0;
            if c.remaining > 0 {
                let units = nominal.min(reserve).min(c.remaining).max(0);
                if c.remaining - units == 0 {
                    c.remaining = 0;
                    reserve -= units;
                    self.skill_left = -units;
                }
            }
        }
        if self.skill_left < 0 {
            self.skill_left += nominal;
        } else {
            self.skill_left = nominal;
        }
        reserve
    }

    /// The spell that has been paid for, if there is one.
    pub fn casting_paid(&self) -> Option<SpellId> {
        self.casting.filter(|c| c.remaining <= 0).map(|c| c.spell)
    }

    /// May the wizard begin casting `spell` now?
    pub fn check_can_cast(&self, spell: SpellId) -> Result<(), MagicError> {
        if !self.known.contains(&spell) {
            return Err(MagicError::NotAvailable);
        }
        if self.casting.is_some() {
            return Err(MagicError::AlreadyCasting);
        }
        Ok(())
    }

    /// Skill and mana the wizard can put into a spell at once.
    pub fn instant_budget(&self, reserve: i32) -> i32 {
        self.skill_left.min(reserve).max(0)
    }

    /// Pay for a spell cast at once out of this turn's skill.
    pub fn spend_skill(&mut self, cost: i32) {
        self.skill_left -= cost;
    }

    /// Begin paying for `spell` over turns.
    pub fn begin_casting(&mut self, spell: SpellId, cost: i32) {
        self.casting = Some(Casting {
            spell,
            remaining: cost,
        });
    }

    /// Give up the spell being cast; the mana already paid is lost.
    pub fn cancel_casting(&mut self) {
        self.casting = None;
    }

    /// Turn off one of the wizard's own global enchantments.
    pub fn turn_off(&mut self, spell: SpellId) -> Result<(), MagicError> {
        if !self.global.contains(&spell) {
            return Err(MagicError::NotAvailable);
        }
        self.global.retain(|&s| s != spell);
        Ok(())
    }
}

/// `Magic` without a game: only the content and a generator.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::content_with_magic;

    fn spell(c: &Content, id: &str) -> SpellId {
        c.rules.spells.id_of(id).unwrap()
    }

    #[test]
    fn a_new_wizard_has_skill_from_books_and_an_archmage_more() {
        let plain = Magic::new(&[("life", 3), ("chaos", 2)], &[]);
        assert_eq!(plain.total_books(), 5);
        // Twice the books: ten points of target, reached by the triangular steps.
        assert_eq!(plain.casting_skill(), 10);
        assert_eq!(plain.skill_left, 10);
        let arch = Magic::new(&[("life", 3)], &[Retort::Archmage]);
        assert_eq!(arch.casting_skill(), 2 * (3 + 5));
        let capped = Magic::new(&[("life", 15)], &[]);
        assert_eq!(capped.books_in("life"), 11, "no more than eleven");
    }

    #[test]
    fn the_library_depends_on_the_seed_and_the_books_alone() {
        let c = content_with_magic();
        let library = |seed| {
            let mut m = Magic::new(&[("life", 4), ("chaos", 2)], &[]);
            let mut rng = GameRng::new(seed);
            m.init_library(&c, &mut rng, None);
            m.refill_research(&c, &mut rng, 1);
            m
        };
        let (a, b) = (library(5), library(5));
        assert_eq!(a, b);
        assert_eq!(a.known.len(), 3 + 1, "n - 1 commons in each realm");
        assert!(a.candidates.len() <= RESEARCH_SLOTS);
        assert!(a.candidates.iter().all(|s| a.researchable.contains(s)));
        assert!(a.known.is_disjoint(&a.researchable));
    }

    #[test]
    fn the_cost_of_a_spell_follows_the_books_the_retorts_and_where_it_is_cast() {
        let c = content_with_magic();
        let strength = c.rules.spells.get(spell(&c, "giant_strength"));
        let bare = Magic::new(&[("nature", 1)], &[]);
        let base = strength.cost as i32;
        assert_eq!(bare.casting_cost(strength, true, false), base);
        assert_eq!(
            bare.casting_cost(strength, false, false),
            base * 5,
            "five times as much away from a battle"
        );
        // Nine books of its realm: two steps of ten percent; the mastery adds fifteen.
        let deep = Magic::new(&[("nature", 9)], &[Retort::NatureMastery]);
        assert_eq!(deep.cost_reduction(strength), 20 + 15);
        let cost = deep.casting_cost(strength, true, false);
        assert_eq!(cost, base - base * 35 / 100);
        // Evil Omens: half as much again for life and nature.
        assert_eq!(deep.casting_cost(strength, true, true), cost + cost / 2);
    }

    #[test]
    fn the_power_is_split_by_the_percentages() {
        let mut m = Magic::new(&[("life", 3)], &[]);
        m.set_split(40, 30, 30).unwrap();
        let split = m.split_power(100, 6);
        assert_eq!((split.mana, split.skill, split.research), (40, 30, 30 + 6));
        assert_eq!(m.set_split(50, 50, 50), Err(MagicError::BadSplit));
        // With research off its share goes to skill, with Mana Focusing mana grows a quarter.
        m.set_split(50, 0, 50).unwrap();
        m.retorts.insert(Retort::ManaFocusing);
        let split = m.split_power(10, 0);
        assert_eq!((split.mana, split.skill, split.research), (5 + 1, 5, 0));
    }

    #[test]
    fn the_spell_of_mastery_is_offered_last_and_costs_less_for_each_spell_known() {
        let c = content_with_magic();
        let mastery = spell(&c, "spell_of_mastery");
        let mut rng = GameRng::new(3);
        let mut m = Magic::new(&[("life", 1)], &[]);
        m.init_library(&c, &mut rng, None);
        m.refill_research(&c, &mut rng, 5);
        assert!(
            !m.candidates.contains(&mastery),
            "not while other spells remain"
        );
        let mut learned = 0;
        while m.candidates != [mastery] {
            let next = *m.candidates.first().expect("something on offer");
            m.learn(next);
            learned += 1;
            assert!(learned < 500, "the pool runs dry");
            m.refill_research(&c, &mut rng, 5);
        }
        let listed = c.rules.spells.get(mastery).research_cost as i32;
        let cost = m.research_cost(&c, mastery);
        assert!(
            cost < listed,
            "known spells cut it: {cost} against {listed}"
        );
        let half: i32 = m
            .known
            .iter()
            .map(|&s| c.rules.spells.get(s).research_cost as i32 / 2)
            .sum();
        assert_eq!(cost, (listed - half).max(0));
        m.choose_research(&c, mastery).unwrap();
        assert_eq!(m.research_remaining, cost);
        assert_ne!(
            m.research_cost(&c, spell(&c, "mend")),
            cost,
            "other spells cost their listed amount"
        );
    }

    #[test]
    fn research_runs_from_choice_to_a_known_spell() {
        let c = content_with_magic();
        let mut rng = GameRng::new(3);
        let mut m = Magic::new(&[("life", 3)], &[Retort::SageMaster]);
        m.init_library(&c, &mut rng, None);
        m.refill_research(&c, &mut rng, 5);
        let pick = *m.candidates.first().expect("something on offer");
        assert_eq!(
            m.choose_research(&c, spell(&c, "mend")).is_ok(),
            m.candidates.contains(&spell(&c, "mend"))
        );
        m = {
            let mut fresh = m.clone();
            fresh.researching = None;
            fresh
        };
        m.choose_research(&c, pick).unwrap();
        assert_eq!(
            m.choose_research(&c, pick),
            Err(MagicError::AlreadyResearching)
        );
        let cost = c.rules.spells.get(pick).research_cost as i32;
        assert_eq!(m.research_remaining, cost);
        // A sage master's research goes a quarter further.
        let split = PowerSplit {
            mana: 0,
            skill: 0,
            research: 100,
        };
        assert!(m.research_income(&c, &split) >= 125);
        let mut turns = 0;
        while m.finish_research().is_none() {
            m.take_power(&split, m.research_income(&c, &split));
            turns += 1;
            assert!(turns < 100);
        }
        assert!(m.known.contains(&pick));
        assert!(!m.researchable.contains(&pick) && !m.candidates.contains(&pick));
        assert!(m.researching.is_none());
    }

    #[test]
    fn casting_is_paid_over_turns_and_cancelled_without_a_refund() {
        let c = content_with_magic();
        let strength = spell(&c, "giant_strength");
        let mut m = Magic::new(&[("life", 3)], &[]);
        assert_eq!(m.check_can_cast(strength), Err(MagicError::NotAvailable));
        m.known.insert(strength);
        assert_eq!(m.check_can_cast(strength), Ok(()));
        assert_eq!(m.instant_budget(100), m.skill_left);
        assert_eq!(m.instant_budget(2), 2, "the reserve limits it");
        m.begin_casting(strength, 40);
        assert_eq!(m.check_can_cast(strength), Err(MagicError::AlreadyCasting));
        let skill = m.casting_skill();
        // The skill left from last turn pays first; the nominal skill pays only if it finishes the
        // spell, which it cannot here. What is paid comes out of the reserve.
        let reserve = m.pay_casting(1000);
        assert_eq!(m.casting.unwrap().remaining, 40 - skill);
        assert_eq!(reserve, 1000 - skill);
        assert_eq!(
            m.skill_left, skill,
            "the nominal skill is there for next turn"
        );
        assert_eq!(m.casting_paid(), None);
        // With one turn's skill to go the second part finishes it, borrowed from the next turn.
        m.casting.as_mut().unwrap().remaining = skill;
        m.skill_left = 0;
        let reserve = m.pay_casting(1000);
        assert_eq!(m.casting_paid(), Some(strength));
        assert_eq!(reserve, 1000 - skill);
        assert_eq!(m.skill_left, 0, "borrowed and paid back at once");
        m.cancel_casting();
        assert!(m.casting.is_none());
    }

    #[test]
    fn upkeep_gives_up_the_newest_items_until_the_reserve_pays() {
        use crate::cast::UpkeepKind;
        let c = content_with_magic();
        let any = spell(&c, "giant_strength");
        let item = |mana| UpkeepItem {
            kind: UpkeepKind::Global(any),
            mana,
        };
        let items = [item(10), item(6), item(4)];
        assert_eq!(upkeep_plan(&items, 1, 20), (vec![], 20));
        let (dropped, owed) = upkeep_plan(&items, 1, 15);
        assert_eq!(
            dropped,
            vec![item(4), item(6)],
            "the newest first, until it fits"
        );
        assert_eq!(owed, 10);
        assert_eq!(upkeep_plan(&items, 1, 16).0, vec![item(4)]);
        // A channeler pays half, so the same reserve covers the lot.
        assert_eq!(upkeep_plan(&items, 2, 15), (vec![], 10));
        assert_eq!(Magic::new(&[], &[Retort::Channeler]).upkeep_divisor(), 2);
        assert_eq!(Magic::default().upkeep_divisor(), 1);
    }

    #[test]
    fn global_spells_can_be_turned_off_only_if_they_are_on() {
        let c = content_with_magic();
        let s = spell(&c, "giant_strength");
        let mut m = Magic::default();
        assert_eq!(m.turn_off(s), Err(MagicError::NotAvailable));
        m.global.push(s);
        assert_eq!(m.turn_off(s), Ok(()));
        assert!(m.global.is_empty());
    }

    #[test]
    fn power_reaches_the_reserve_the_research_and_the_skill() {
        use crate::city::PlayerId;
        let c = content_with_magic();
        let mut p = Player::new(PlayerId(0), "Merlin");
        p.magic = Magic::new(&[("life", 2)], &[Retort::Archmage]);
        let target = *p.magic.candidates.first().unwrap_or(&spell(&c, "mend"));
        p.magic.researching = Some(target);
        p.magic.research_remaining = 10;
        p.mana = 5;
        let points = p.magic.skill_points;
        let split = PowerSplit {
            mana: 7,
            research: 4,
            skill: 4,
        };
        collect_power(&mut p, &split, 4);
        assert_eq!(p.mana, 12);
        assert_eq!(p.magic.research_remaining, 6);
        assert_eq!(
            p.magic.skill_points - points,
            6,
            "an archmage gets half as much again"
        );
        p.mana = i32::MAX;
        collect_power(&mut p, &split, 0);
        assert_eq!(p.mana, i32::MAX, "the reserve does not overflow");
    }

    #[test]
    fn paying_for_a_cast_takes_mana_from_the_reserve() {
        use crate::city::PlayerId;
        let c = content_with_magic();
        let mut p = Player::new(PlayerId(0), "Merlin");
        p.magic = Magic::new(&[("life", 2)], &[]);
        p.magic.begin_casting(spell(&c, "mend"), 30);
        p.mana = 50;
        let skill = p.magic.skill_left;
        pay_for_casting(&mut p);
        assert_eq!(
            p.mana,
            50 - skill,
            "the skill on hand is paid for with mana"
        );
        assert_eq!(p.magic.casting.unwrap().remaining, 30 - skill);
    }
}
