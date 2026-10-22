//! What a wizard is as components of its entity (task 315): spell books, retorts, casting skill,
//! the split of power, research, the spells known and researchable, the offer for research and
//! fame. [`Wizard`] reads them (the cost of a spell, the research income, may the wizard cast it)
//! and [`WizardMut`] changes them (research, casting, power). The casting in progress, the item
//! being made, the hero being raised and the lasting spells on the wizard stay in the transitional
//! [`Magic`] component until the spell instances (tasks 319 to 322).
//!
//! Formulas follow the original game's code (see `docs/magic.md` for the list and for where this
//! differs).

use std::collections::{BTreeMap, BTreeSet};

use crate::content::Content;
use crate::game::rng::GameRng;
use crate::game::setting::SpellLibraryDef;
use crate::rules::{Rarity, SpellDef, SpellId};
use crate::wizard::entity::Wizard;
use crate::wizard::magic::{
    base_casting_skill, default_spells, is_crafting, is_summoning, pick_spells, skill_points_for,
    spells_of, CastStart, Casting, Magic, MagicError, PowerShares, Retort, RESEARCH_SLOTS,
};

/// Spell books per realm id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Books(pub BTreeMap<String, u8>);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Retorts(pub BTreeSet<Retort>);

/// Casting skill is not stored directly: the total of invested `points` maps to it through
/// triangular numbers (see [`base_casting_skill`]). `left` is the skill still available this turn
/// for instant casting (the original's "Skill_Left"); it can be negative after a spell was
/// finished on next turn's skill.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Skill {
    pub points: i32,
    pub left: i32,
}

/// Share of the power base (percent) going to mana, research and casting skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerSplit {
    pub mana: u8,
    pub research: u8,
    pub skill: u8,
}

impl Default for PowerSplit {
    fn default() -> Self {
        Self {
            mana: 33,
            research: 34,
            skill: 33,
        }
    }
}

/// The spell being researched and the research points still to pay for it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Research {
    pub spell: Option<SpellId>,
    pub remaining: i32,
}

/// Spells the wizard can cast, and spells it may research (never both).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownSpells {
    pub known: BTreeSet<SpellId>,
    pub researchable: BTreeSet<SpellId>,
}

/// The spells on offer for research now (at most [`RESEARCH_SLOTS`]), fastest first: the wizard
/// picks the next research from these (`KnownSpells::researchable` is what the books allow in
/// all).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Candidates(pub Vec<SpellId>);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Fame(pub i32);

/// The components of a wizard's sheet, empty.
pub(crate) fn bundle() -> (
    Books,
    Retorts,
    Skill,
    PowerSplit,
    Research,
    KnownSpells,
    Candidates,
    Fame,
    Magic,
) {
    Default::default()
}

impl<'a> Wizard<'a> {
    pub fn has(&self, retort: Retort) -> bool {
        self.component::<Retorts>()
            .is_some_and(|r| r.0.contains(&retort))
    }

    pub fn books_in(&self, realm: &str) -> i32 {
        self.component::<Books>()
            .and_then(|b| b.0.get(realm).copied())
            .map_or(0, i32::from)
    }

    pub fn total_books(&self) -> i32 {
        self.component::<Books>()
            .map_or(0, |b| b.0.values().map(|&n| i32::from(n)).sum())
    }

    /// Books per realm id.
    pub fn books(&self) -> BTreeMap<String, u8> {
        self.component::<Books>()
            .map(|b| b.0.clone())
            .unwrap_or_default()
    }

    pub fn retorts(&self) -> BTreeSet<Retort> {
        self.component::<Retorts>()
            .map(|r| r.0.clone())
            .unwrap_or_default()
    }

    pub fn skill_points(&self) -> i32 {
        self.component::<Skill>().map_or(0, |s| s.points)
    }

    /// Casting skill still available this turn for instant casting.
    pub fn skill_left(&self) -> i32 {
        self.component::<Skill>().map_or(0, |s| s.left)
    }

    /// Casting skill now.
    pub fn casting_skill(&self) -> i32 {
        base_casting_skill(self.skill_points(), self.has(Retort::Archmage))
    }

    pub fn power_split(&self) -> PowerSplit {
        self.component::<PowerSplit>()
            .map(|s| *s)
            .unwrap_or_default()
    }

    pub fn knows(&self, spell: SpellId) -> bool {
        self.component::<KnownSpells>()
            .is_some_and(|k| k.known.contains(&spell))
    }

    /// May the wizard research `spell` (it is not known yet)?
    pub fn can_research(&self, spell: SpellId) -> bool {
        self.component::<KnownSpells>()
            .is_some_and(|k| k.researchable.contains(&spell))
    }

    /// The spells the wizard can cast.
    pub fn known(&self) -> BTreeSet<SpellId> {
        self.component::<KnownSpells>()
            .map(|k| k.known.clone())
            .unwrap_or_default()
    }

    /// The spells the wizard may research, not yet known.
    pub fn researchable(&self) -> BTreeSet<SpellId> {
        self.component::<KnownSpells>()
            .map(|k| k.researchable.clone())
            .unwrap_or_default()
    }

    /// The offer for research, fastest first.
    pub fn candidates(&self) -> Vec<SpellId> {
        self.component::<Candidates>()
            .map(|c| c.0.clone())
            .unwrap_or_default()
    }

    pub fn researching(&self) -> Option<SpellId> {
        self.component::<Research>().and_then(|r| r.spell)
    }

    /// Research points still to pay for the spell being researched.
    pub fn research_remaining(&self) -> i32 {
        self.component::<Research>().map_or(0, |r| r.remaining)
    }

    pub fn fame(&self) -> i32 {
        self.component::<Fame>().map_or(0, |f| f.0)
    }

    /// The spell being cast overland, with the mana still to pay.
    pub fn casting(&self) -> Option<Casting> {
        self.component::<Magic>().and_then(|m| m.casting)
    }

    /// The item Enchant Item or Create Artifact is making while its cost is paid.
    pub fn crafting(&self) -> Option<crate::item::Item> {
        self.component::<Magic>().and_then(|m| m.crafting.clone())
    }

    /// The dead hero (a roster index) Resurrection is bringing back while its cost is paid.
    pub fn raising(&self) -> Option<usize> {
        self.component::<Magic>().and_then(|m| m.raising)
    }

    /// Lasting spells on the wizard (or the whole world) that cost mana every turn.
    pub fn global(&self) -> Vec<SpellId> {
        self.component::<Magic>()
            .map(|m| m.global.clone())
            .unwrap_or_default()
    }

    pub fn has_global(&self, spell: SpellId) -> bool {
        self.component::<Magic>()
            .is_some_and(|m| m.global.contains(&spell))
    }

    /// Mana needed to cast `def`. Overland casting of a spell that can also be cast in battle
    /// costs five times the battle price; reductions come from books beyond seven, masteries,
    /// artificer, conjurer and runemaster. `evil_omens`: life and nature spells cost half as
    /// much again (for every wizard, the caster too; the game says whether Evil Omens is in
    /// force).
    pub fn casting_cost(&self, def: &SpellDef, in_battle: bool, evil_omens: bool) -> i32 {
        casting_cost(Some(self), def, in_battle, evil_omens)
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
    pub fn split_power(&self, power: i32, city_research: i32) -> PowerShares {
        let split = self.power_split();
        let mut mana = (power * i32::from(split.mana) + 50) / 100;
        let mut skill = (power * i32::from(split.skill) + 50) / 100;
        let mut research = power - mana - skill;
        if split.research == 0 && research > 0 {
            if split.skill == 0 {
                mana += research;
            } else {
                skill += research;
            }
            research = 0;
        }
        if self.has(Retort::ManaFocusing) {
            mana += mana / 4;
        }
        PowerShares {
            mana,
            skill,
            research: research + city_research,
        }
    }

    /// Research points that will be paid next turn change, with bonuses.
    pub fn research_income(&self, content: &Content, split: &PowerShares) -> i32 {
        let bonus = self
            .researching()
            .map_or(0, |s| self.research_bonus(content.rules.spells.get(s)));
        let research = split.research.max(0);
        research + research * bonus / 100
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
        let known: i32 = self.component::<KnownSpells>().map_or(0, |k| {
            k.known
                .iter()
                .map(|&s| spells.get(s).research_cost as i32 / 2)
                .sum()
        });
        (listed - known).max(0)
    }

    /// The spells the books would let the wizard add to the offer next (the original's
    /// `Build_Research_List`): in each realm every researchable spell of the lowest rarity that
    /// still has one, so a realm's commons are offered before its uncommons. Arcane spells go
    /// in three steps (the first three, the next five, then the rest).
    fn research_pool(&self, content: &Content) -> Vec<SpellId> {
        let rules = &content.rules;
        let researchable = self.researchable();
        let candidates = self.candidates();
        let open = |s: &SpellId| researchable.contains(s) && !candidates.contains(s);
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

    /// The spell that has been paid for, if there is one.
    pub fn casting_paid(&self) -> Option<SpellId> {
        self.casting().filter(|c| c.remaining <= 0).map(|c| c.spell)
    }

    /// May the wizard begin casting `spell` now?
    pub fn check_can_cast(&self, spell: SpellId) -> Result<(), MagicError> {
        if !self.knows(spell) {
            return Err(MagicError::NotAvailable);
        }
        if self.casting().is_some() {
            return Err(MagicError::AlreadyCasting);
        }
        Ok(())
    }

    /// Skill and mana the wizard can put into a spell at once.
    pub fn instant_budget(&self, reserve: i32) -> i32 {
        self.skill_left().min(reserve).max(0)
    }

    /// How a spell costing `cost` begins: at once if the skill and mana on hand pay for it.
    pub fn cast_start(&self, cost: i32, reserve: i32) -> CastStart {
        if cost <= self.instant_budget(reserve) {
            CastStart::Now
        } else {
            CastStart::Paid
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
}

/// The mana a spell costs `wizard` (a player without a wizard pays the listed cost).
pub fn casting_cost(
    wizard: Option<&Wizard<'_>>,
    def: &SpellDef,
    in_battle: bool,
    evil_omens: bool,
) -> i32 {
    let mut cost = def.cost as i32;
    if !in_battle && def.overland_x5 {
        cost *= 5;
    }
    if let Some(w) = wizard {
        cost -= cost * w.cost_reduction(def) / 100;
    }
    if matches!(def.realm.as_str(), "life" | "nature") && evil_omens {
        cost += cost / 2;
    }
    cost
}

/// A write handle on one wizard.
pub struct WizardMut<'a> {
    world: &'a mut hecs::World,
    entity: hecs::Entity,
}

impl<'a> WizardMut<'a> {
    pub(crate) fn new(world: &'a mut hecs::World, entity: hecs::Entity) -> Self {
        Self { world, entity }
    }

    /// A component of the wizard, to change.
    ///
    /// # Panics
    /// If the wizard has no such component (every wizard has all of the sheet's).
    pub fn get_mut<T: hecs::Component>(&mut self) -> hecs::RefMut<'_, T> {
        self.world
            .get::<&mut T>(self.entity)
            .expect("a wizard has all the components of its sheet")
    }

    /// Give the wizard a component, or take it away (`None`) when it is an optional one.
    pub fn put<T: hecs::Component>(&mut self, value: Option<T>) {
        match value {
            Some(v) => {
                let _ = self.world.insert_one(self.entity, v);
            }
            None => {
                let _ = self.world.remove_one::<T>(self.entity);
            }
        }
    }

    /// The wizard's read handle (for a moment: it ends before the next change).
    pub fn view(&self) -> Wizard<'_> {
        Wizard::new(self.world, self.entity)
    }

    /// Set the sheet up for a new wizard: books and retorts, and the starting casting skill
    /// (twice the books, plus 5 books' worth for an archmage), found by growing the invested
    /// total until the skill reaches it; a Famous wizard starts with a fame of 10. The spell
    /// library comes with [`WizardMut::init_library`].
    pub fn found(&mut self, books: &[(&str, u8)], retorts: &[Retort]) {
        let mut sheet = Books::default();
        for &(realm, n) in books {
            if n > 0 {
                sheet.0.insert(realm.to_string(), n.min(11));
            }
        }
        let total: i32 = sheet.0.values().map(|&n| i32::from(n)).sum();
        let retorts = Retorts(retorts.iter().copied().collect());
        let archmage = retorts.0.contains(&Retort::Archmage);
        let famous = retorts.0.contains(&Retort::Famous);
        let target = 2 * (total + if archmage { 5 } else { 0 });
        let points = skill_points_for(target, archmage);
        *self.get_mut::<Books>() = sheet;
        *self.get_mut::<Retorts>() = retorts;
        *self.get_mut::<Skill>() = Skill {
            points,
            left: base_casting_skill(points, archmage),
        };
        *self.get_mut::<PowerSplit>() = PowerSplit::default();
        *self.get_mut::<Fame>() = Fame(if famous { 10 } else { 0 });
    }

    /// Known and researchable spells from books and retorts, by the tables in `settings.ron`
    /// (`spell_library`, the original's `Init_Computer_Players_Spell_Library`). In each realm the
    /// known spells are the chosen ones (`start_spells`), then the realm's default list; without
    /// `start_spells` they are drawn at random. The researchable ones are drawn at random up to
    /// the table's count, the known ones included. The arcane spells are open to everyone except
    /// the closed ones; the spells every wizard or a retort starts with are known. The offer
    /// for research is emptied; fill it with [`WizardMut::refill_research`].
    pub fn init_library(
        &mut self,
        content: &Content,
        rng: &mut GameRng,
        start_spells: Option<&[SpellId]>,
    ) {
        let library = &content.settings.spell_library;
        let (books, retorts) = {
            let w = self.view();
            (w.books(), w.retorts())
        };
        let mut known = BTreeSet::new();
        let mut available = BTreeSet::new();
        for (realm, n) in books {
            let n = n.clamp(1, SpellLibraryDef::MAX_BOOKS as u8);
            let (want, open) = (
                SpellLibraryDef::row(&library.known, n),
                SpellLibraryDef::row(&library.researchable, n),
            );
            for (rarity, count, limit) in [
                (Rarity::Common, want.common, open.common),
                (Rarity::Uncommon, want.uncommon, open.uncommon),
                (Rarity::Rare, want.rare, open.rare),
                (Rarity::VeryRare, want.very_rare, open.very_rare),
            ] {
                let pool = spells_of(content, &realm, rarity);
                let count = usize::from(count).min(pool.len());
                let start: Vec<SpellId> = match start_spells {
                    Some(chosen) => {
                        let mut start: Vec<SpellId> = chosen
                            .iter()
                            .copied()
                            .filter(|s| pool.contains(s))
                            .take(count)
                            .collect();
                        let defaults = default_spells(content, &realm, rarity);
                        for s in defaults.into_iter().chain(pool.iter().copied()) {
                            if start.len() >= count {
                                break;
                            }
                            if !start.contains(&s) {
                                start.push(s);
                            }
                        }
                        start
                    }
                    None => pick_spells(rng, &pool, &known, count),
                };
                known.extend(start.iter().copied());
                available.extend(start);
                let limit = usize::from(limit).min(pool.len());
                let have = pool.iter().filter(|s| available.contains(s)).count();
                let more = pick_spells(rng, &pool, &available, limit.saturating_sub(have));
                available.extend(more);
            }
        }
        // Arcane spells are open to everyone, but the closed ones.
        let arcane: Vec<SpellId> = content
            .rules
            .spells
            .iter()
            .filter(|(_, d)| d.realm == "arcane" && !library.arcane_closed.contains(&d.id))
            .map(|(id, _)| id)
            .collect();
        available.extend(arcane);
        // What everyone, or a retort, starts with.
        let given = library.always_known.iter().chain(
            library
                .retort_known
                .iter()
                .filter(|(r, _)| retorts.contains(*r))
                .flat_map(|(_, spells)| spells),
        );
        for id in given {
            if let Some(spell) = content.rules.spells.id_of(id) {
                known.insert(spell);
            }
        }
        *self.get_mut::<KnownSpells>() = KnownSpells {
            researchable: available.difference(&known).copied().collect(),
            known,
        };
        self.get_mut::<Candidates>().0.clear();
    }

    /// Change how power is split. The three shares must add up to 100.
    pub fn set_split(&mut self, mana: u8, research: u8, skill: u8) -> Result<(), MagicError> {
        if u32::from(mana) + u32::from(research) + u32::from(skill) != 100 {
            return Err(MagicError::BadSplit);
        }
        *self.get_mut::<PowerSplit>() = PowerSplit {
            mana,
            research,
            skill,
        };
        Ok(())
    }

    /// Choose what to research. The choice stands until the spell is learned.
    pub fn choose_research(&mut self, content: &Content, spell: SpellId) -> Result<(), MagicError> {
        let cost = {
            let w = self.view();
            if w.researching().is_some() {
                return Err(MagicError::AlreadyResearching);
            }
            if !w.can_research(spell) || !w.candidates().contains(&spell) {
                return Err(MagicError::NotAvailable);
            }
            w.research_cost(content, spell)
        };
        *self.get_mut::<Research>() = Research {
            spell: Some(spell),
            remaining: cost,
        };
        Ok(())
    }

    /// Top the offer for research up to [`RESEARCH_SLOTS`] spells from the pool (the pool is
    /// worked out afresh after each pick, so a realm's next rarity opens as its last cheaper
    /// spell goes on offer), drop spells that are no longer researchable, and sort the offer by
    /// the time each would take at `income` research points a turn. Called each turn, when a
    /// wizard is made and when books are added.
    pub fn refill_research(&mut self, content: &Content, rng: &mut GameRng, income: i32) {
        let researchable = self.view().researchable();
        let mut candidates: Vec<SpellId> = self
            .view()
            .candidates()
            .into_iter()
            .filter(|s| researchable.contains(s))
            .collect();
        *self.get_mut::<Candidates>() = Candidates(candidates.clone());
        while candidates.len() < RESEARCH_SLOTS {
            let pool = self.view().research_pool(content);
            if pool.is_empty() {
                break;
            }
            let pick = pool[rng.roll(pool.len() as u32) as usize - 1];
            candidates.push(pick);
            *self.get_mut::<Candidates>() = Candidates(candidates.clone());
        }
        // With nothing left to offer, the Spell of Mastery is: the last thing to research.
        if candidates.len() < RESEARCH_SLOTS {
            if let Some(som) = content.rules.spells.id_of("spell_of_mastery") {
                if !self.view().knows(som)
                    && !candidates.contains(&som)
                    && self.view().research_pool(content).is_empty()
                {
                    self.get_mut::<KnownSpells>().researchable.insert(som);
                    candidates.push(som);
                }
            }
        }
        let income = income.max(0);
        let w = self.view();
        let time = |s: SpellId| {
            let def = content.rules.spells.get(s);
            let bonus = w.research_bonus(def);
            let cost = w.research_cost(content, s) * 100 / (100 + bonus);
            if income == 0 {
                cost
            } else {
                cost / income
            }
        };
        candidates.sort_by_key(|&s| time(s));
        *self.get_mut::<Candidates>() = Candidates(candidates);
    }

    /// The spell becomes known, as if researched (a gift or an exchange between wizards).
    /// The offer is refilled by the caller.
    pub fn learn(&mut self, spell: SpellId) {
        self.get_mut::<KnownSpells>().researchable.remove(&spell);
        self.get_mut::<Candidates>().0.retain(|&s| s != spell);
        {
            let mut research = self.get_mut::<Research>();
            if research.spell == Some(spell) {
                *research = Research::default();
            }
        }
        self.get_mut::<KnownSpells>().known.insert(spell);
    }

    /// The turn's income of research and skill: research goes to the spell, skill points to
    /// casting skill (an archmage gets half as much again). Returns the mana for the reserve.
    pub fn take_power(&mut self, split: &PowerShares, research: i32) -> i32 {
        let archmage = self.view().has(Retort::Archmage);
        self.add_research(research);
        let skill = split.skill.max(0);
        self.get_mut::<Skill>().points += if archmage { skill * 3 / 2 } else { skill };
        split.mana.max(0)
    }

    /// The spell being researched is learned if its research is paid for.
    pub fn finish_research(&mut self) -> Option<SpellId> {
        let research = *self.get_mut::<Research>();
        match research.spell {
            Some(spell) if research.remaining == 0 => {
                *self.get_mut::<Research>() = Research::default();
                self.get_mut::<KnownSpells>().researchable.remove(&spell);
                self.get_mut::<Candidates>().0.retain(|&s| s != spell);
                self.get_mut::<KnownSpells>().known.insert(spell);
                Some(spell)
            }
            _ => None,
        }
    }

    /// Spend casting skill and mana on the spell being cast. Returns the reserve left.
    pub fn pay_casting(&mut self, reserve: i32) -> i32 {
        let nominal = self.view().casting_skill();
        let mut reserve = reserve;
        let mut skill = *self.get_mut::<Skill>();
        {
            let mut magic = self.get_mut::<Magic>();
            if let Some(c) = magic.casting.as_mut() {
                let units = skill.left.min(reserve).min(c.remaining).max(0);
                c.remaining -= units;
                reserve -= units;
                skill.left = 0;
                if c.remaining > 0 {
                    let units = nominal.min(reserve).min(c.remaining).max(0);
                    if c.remaining - units == 0 {
                        c.remaining = 0;
                        reserve -= units;
                        skill.left = -units;
                    }
                }
            }
        }
        if skill.left < 0 {
            skill.left += nominal;
        } else {
            skill.left = nominal;
        }
        *self.get_mut::<Skill>() = skill;
        reserve
    }

    /// Research points reach the spell being researched (nothing when none is).
    pub fn add_research(&mut self, points: i32) {
        let mut r = self.get_mut::<Research>();
        if r.spell.is_some() {
            r.remaining = (r.remaining - points).max(0);
        }
    }

    /// A lasting spell starts on the wizard (or the whole world); it is there once.
    pub fn add_global(&mut self, spell: SpellId) {
        let mut magic = self.get_mut::<Magic>();
        if !magic.global.contains(&spell) {
            magic.global.push(spell);
        }
    }

    /// A lasting spell on the wizard ends.
    pub fn remove_global(&mut self, spell: SpellId) {
        self.get_mut::<Magic>().global.retain(|&s| s != spell);
    }

    /// The spell being cast is cast or given up (the item and the hero stay).
    pub fn clear_casting(&mut self) {
        self.get_mut::<Magic>().casting = None;
    }

    /// Pay for a spell cast at once out of this turn's skill.
    pub fn spend_skill(&mut self, cost: i32) {
        self.get_mut::<Skill>().left -= cost;
    }

    /// Skill added to this turn's (heroes at the fortress add to it).
    pub fn add_skill_left(&mut self, skill: i32) {
        self.get_mut::<Skill>().left += skill;
    }

    /// Begin paying for `spell` over turns.
    pub fn begin_casting(&mut self, spell: SpellId, cost: i32) {
        self.get_mut::<Magic>().casting = Some(Casting {
            spell,
            remaining: cost,
        });
    }

    /// Give up the spell being cast; the mana already paid is lost.
    pub fn cancel_casting(&mut self) {
        let mut magic = self.get_mut::<Magic>();
        magic.casting = None;
        magic.crafting = None;
        magic.raising = None;
    }

    /// Turn off one of the wizard's own global enchantments.
    pub fn turn_off(&mut self, spell: SpellId) -> Result<(), MagicError> {
        let mut magic = self.get_mut::<Magic>();
        if !magic.global.contains(&spell) {
            return Err(MagicError::NotAvailable);
        }
        magic.global.retain(|&s| s != spell);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
