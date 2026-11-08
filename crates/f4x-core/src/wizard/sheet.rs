//! What a wizard is as components of its entity: spell books, retorts, casting skill,
//! the split of power, research, the spells known and researchable, the offer for research and
//! fame. [`Wizard`] reads them (the cost of a spell, the research income, may the wizard cast it)
//! and [`WizardMut`] changes them (research, casting, power). The cast in progress, with the item
//! being made and the hero being raised, is a spell instance (`spell/instance.rs`).
//!
//! Formulas follow the original game's code (see `docs/magic.md` for the list and for where this
//! differs).
//!
//! Tests: in wizard/sheet/tests.rs.

use f4x_def::half_int::HalfInt;
use std::collections::{BTreeMap, BTreeSet};

use crate::game::rng::GameRng;
use crate::game::setting::SpellLibraryDef;
use crate::rules::{Rarity, RealmId, Rules, SpellDef, SpellId, Stat};
use crate::spell::instance::{Owed, Spells};
use crate::wizard::entity::Wizard;
use crate::wizard::magic::{
    base_casting_skill, default_spells, pick_spells, skill_points_for, spells_of, CastStart,
    Casting, MagicError, PowerShares, Retort, RESEARCH_SLOTS,
};
use crate::wizard::retort;
use f4x_def::content::Content;

/// Spell books per realm id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Books(pub BTreeMap<RealmId, u8>);

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

/// Books of one realm that take [`MASTERY_DISCOUNT`] off the Spell of Mastery's research cost.
pub const MASTERY_DISCOUNT_BOOKS: i32 = 11;
/// Research points off the Spell of Mastery for a wizard with [`MASTERY_DISCOUNT_BOOKS`] books
/// of one realm.
pub const MASTERY_DISCOUNT: i32 = 3000;
/// A wizard with more books than this finds no more in treasure, so the most is one above it
/// (the original refuses a book at `Book_Count > 12`).
pub const BOOKS_BEFORE_LIMIT: i32 = 12;

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
    /// The spells the wizard started with (the original sets them directly, without the gain
    /// that lowers the Spell of Mastery's cost, so they do not count towards it).
    pub starting: BTreeSet<SpellId>,
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
) {
    Default::default()
}

impl<'a> Wizard<'a> {
    /// `base` changed by what the wizard's retorts say about `stat` (`retorts.ron`).
    pub fn stat(&self, rules: &crate::rules::Rules, stat: crate::rules::Stat, base: i32) -> i32 {
        match self.component::<Retorts>() {
            Some(r) => crate::wizard::retort::stat(rules, &r.0, stat, base),
            None => crate::wizard::retort::stat(rules, &BTreeSet::new(), stat, base),
        }
    }

    pub fn has(&self, retort: Retort) -> bool {
        self.component::<Retorts>()
            .is_some_and(|r| r.0.contains(&retort))
    }

    pub fn books_in(&self, realm: RealmId) -> i32 {
        self.component::<Books>()
            .and_then(|b| b.0.get(&realm).copied())
            .map_or(0, i32::from)
    }

    pub fn total_books(&self) -> i32 {
        self.component::<Books>()
            .map_or(0, |b| b.0.values().map(|&n| i32::from(n)).sum())
    }

    /// Books per realm id.
    pub fn books(&self) -> BTreeMap<RealmId, u8> {
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
    pub fn casting_skill(&self, rules: &crate::rules::Rules) -> i32 {
        base_casting_skill(
            self.skill_points(),
            self.stat(rules, crate::rules::Stat::CastingSkill, 0),
        )
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

    /// The spells the wizard started with.
    pub fn starting_spells(&self) -> BTreeSet<SpellId> {
        self.component::<KnownSpells>()
            .map(|k| k.starting.clone())
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
        let spells = Spells::new(self.world());
        let cast = spells.cast_of(self.entity())?;
        Some(Casting {
            spell: spells.def(cast)?,
            remaining: self.world().get::<&Owed>(cast).ok()?.0,
        })
    }

    /// The item Enchant Item or Create Artifact is making while its cost is paid.
    pub fn crafting(&self) -> Option<crate::item::Item> {
        let player = crate::wizard::entity::player_id_of(self.world(), self.entity())?;
        crate::item::entity::crafting_of(self.world(), player).map(|(_, item)| item)
    }

    /// Lasting spells on the wizard (or the whole world) that cost mana every turn.
    pub fn global(&self) -> Vec<SpellId> {
        self.global_instances()
            .into_iter()
            .map(|(s, _)| s)
            .collect()
    }

    pub fn has_global(&self, spell: SpellId) -> bool {
        self.global_instances().iter().any(|&(s, _)| s == spell)
    }

    /// Mana needed to cast `def`. Overland casting of a spell that can also be cast in battle
    /// costs five times the battle price; reductions come from books beyond seven and from the
    /// retorts (`retorts.ron`). `evil_omens`: life and nature spells cost half as much again (for
    /// every wizard, the caster too; the game says whether Evil Omens is in force).
    pub fn casting_cost(
        &self,
        rules: &Rules,
        def: &SpellDef,
        in_battle: bool,
        evil_omens: bool,
    ) -> i32 {
        casting_cost(rules, Some(self), def, in_battle, evil_omens)
    }

    /// Percent off the cost of `def`: the books beyond seven in its realm, and the retorts.
    pub fn cost_reduction(&self, rules: &Rules, def: &SpellDef) -> i32 {
        self.extra_books(def) * 10 + retort::cost_cut(rules, &self.retorts(), def)
    }

    /// Books beyond the seventh in the realm of `def` (none for the arcane spells).
    fn extra_books(&self, def: &SpellDef) -> i32 {
        if def.realm == RealmId::new(5)
        /* arcane */
        {
            0
        } else {
            (self.books_in(def.realm) - 7).max(0)
        }
    }

    /// Percent added to research of `def`: the books beyond seven in its realm, and the retorts.
    pub fn research_bonus(&self, rules: &Rules, def: &SpellDef) -> i32 {
        self.extra_books(def) * 10 + retort::research_bonus(rules, &self.retorts(), def)
    }

    /// The power base split by the wizard's percentages. Rounding goes to research; if the
    /// research share is switched off, what it would have been goes to skill (or to mana
    /// when skill is off too). Mana Focusing adds a quarter to mana. `city_research` is what
    /// the wizard's cities produce, added to the research share.
    pub fn split_power(&self, rules: &Rules, power: HalfInt, city_research: i32) -> PowerShares {
        let split = self.power_split();
        // The mana share keeps halves; the skill and research points are whole, so the halves
        // they would have had stay with the research (or with the mana, when it takes the rest).
        let mut mana = HalfInt::from_raw((power.raw() * i32::from(split.mana) + 50) / 100);
        let mut skill = (power.raw() * i32::from(split.skill) + 50) / 100 / 2;
        let mut research = (power - mana - HalfInt::from_int(skill)).floor();
        if split.research == 0 && research > 0 {
            if split.skill == 0 {
                mana += HalfInt::from_int(research);
            } else {
                skill += research;
            }
            research = 0;
        }
        mana = retort::stat_half(rules, &self.retorts(), Stat::ManaIncome, mana);
        PowerShares {
            mana,
            skill,
            research: research + city_research,
        }
    }

    /// Research points that will be paid next turn change, with bonuses.
    pub fn research_income(&self, content: &Content, split: &PowerShares) -> i32 {
        let bonus = self.researching().map_or(0, |s| {
            self.research_bonus(&content.rules, content.rules.spells.get(s))
        });
        let research = split.research.max(0);
        research + research * bonus / 100
    }

    /// What researching `spell` costs this wizard. The Spell of Mastery's cost is its listed
    /// 60,000 less half the research cost of every spell gained since the start (the original
    /// takes that off as each spell is gained; the starting ones do not count, as the wiki
    /// says), and 3,000 less for a wizard with eleven books of one realm (the wiki; in the
    /// original the test needs eleven in all five realms, so it never applies). Any other
    /// spell costs its listed amount.
    pub fn research_cost(&self, content: &Content, spell: SpellId) -> i32 {
        let spells = &content.rules.spells;
        let listed = spells.get(spell).research_cost as i32;
        if !spells
            .get(spell)
            .has_hook(crate::spell::special::SPELL_OF_MASTERY)
        {
            return listed;
        }
        let gained: i32 = self.component::<KnownSpells>().map_or(0, |k| {
            k.known
                .difference(&k.starting)
                .map(|&s| spells.get(s).research_cost as i32 / 2)
                .sum()
        });
        let full_realm = crate::rules::BOOK_REALMS
            .iter()
            .copied()
            .any(|r| self.books_in(r) >= MASTERY_DISCOUNT_BOOKS);
        let discount = if full_realm { MASTERY_DISCOUNT } else { 0 };
        (listed - gained - discount).max(0)
    }

    /// May the wizard hold `def`? A realm needs a book: with one book up to uncommon spells,
    /// with two up to rare ones (`Calc_Spell_Value`); arcane spells need none. Conquest and
    /// the exchange between wizards give only such spells.
    pub fn books_allow(&self, def: &SpellDef) -> bool {
        if def.realm == RealmId::new(5)
        /* arcane */
        {
            return true;
        }
        match self.books_in(def.realm) {
            0 => false,
            1 => def.rarity <= Rarity::Uncommon,
            2 => def.rarity <= Rarity::Rare,
            _ => true,
        }
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
        let cap = usize::from(content.settings.spell_library.arcane_candidate_cap);
        let mut pool = Vec::new();
        let realms = crate::rules::BOOK_REALMS
            .iter()
            .copied()
            .chain([RealmId::new(5) /* arcane */]);
        for realm in realms {
            let mut of: Vec<(Rarity, SpellId)> = rules
                .spells
                .iter()
                .filter(|(id, d)| d.realm == (realm) && open(id))
                .map(|(id, d)| (d.rarity, id))
                .collect();
            // Patch 1.40: while another realm has something to research, only so many arcane
            // spells are on offer at once (a setting, off by default).
            let arcane_on_offer = candidates
                .iter()
                .filter(|&&c| rules.spells.get(c).realm == realm)
                .count();
            if cap > 0 && realm == RealmId::new(5) && arcane_on_offer >= cap && !pool.is_empty() {
                continue;
            }
            if let Some(lowest) = of.iter().map(|&(r, _)| r).min() {
                of.retain(|&(r, _)| r == lowest);
                pool.extend(of.into_iter().map(|(_, id)| id));
            }
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
        if self.is_banished() {
            return Err(MagicError::Banished);
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

    /// Percent of the spell upkeep the wizard pays: 100, half of it for a Channeler.
    pub fn upkeep_percent(&self, rules: &Rules) -> i32 {
        retort::stat(rules, &self.retorts(), Stat::SpellUpkeepPercent, 100)
    }
}

/// The mana a spell costs `wizard` (a player without a wizard pays the listed cost).
pub fn casting_cost(
    rules: &Rules,
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
        cost -= cost * w.cost_reduction(rules, def) / 100;
    }
    if matches!(Some(def.realm.num()), Some(3 /* life */ | 0 /* nature */)) && evil_omens {
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
    /// (twice the books, plus what the retorts add: `StartingSkill`), found by growing the invested
    /// total until the skill reaches it; a Famous wizard starts with a fame of 10. The spell
    /// library comes with [`WizardMut::init_library`].
    pub fn found(
        &mut self,
        rules: &crate::rules::Rules,
        books: &[(RealmId, u8)],
        retorts: &[Retort],
    ) {
        let mut sheet = Books::default();
        for &(realm, n) in books {
            if n > 0 {
                sheet.0.insert(realm, n.min(11));
            }
        }
        let total: i32 = sheet.0.values().map(|&n| i32::from(n)).sum();
        let retorts = Retorts(retorts.iter().copied().collect());
        let stat = |s, base| crate::wizard::retort::stat(rules, &retorts.0, s, base);
        let bonus = stat(crate::rules::Stat::CastingSkill, 0);
        let target = stat(crate::rules::Stat::StartingSkill, 2 * total);
        let points = skill_points_for(target, bonus);
        let fame = stat(crate::rules::Stat::StartingFame, 0);
        *self.get_mut::<Books>() = sheet;
        *self.get_mut::<Retorts>() = retorts;
        *self.get_mut::<Skill>() = Skill {
            points,
            left: base_casting_skill(points, bonus),
        };
        *self.get_mut::<PowerSplit>() = PowerSplit::default();
        *self.get_mut::<Fame>() = Fame(fame);
    }

    /// Known and researchable spells from books and retorts, by the tables in `rules_settings.ron`
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
                let pool = spells_of(content, realm, rarity);
                let count = usize::from(count).min(pool.len());
                let start: Vec<SpellId> = match start_spells {
                    Some(chosen) => {
                        let mut start: Vec<SpellId> = chosen
                            .iter()
                            .copied()
                            .filter(|s| pool.contains(s))
                            .take(count)
                            .collect();
                        let defaults = default_spells(content, realm, rarity);
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
            .filter(|&(id, d)| {
                d.realm == (RealmId::new(5)/* arcane */) && !library.arcane_closed.contains(&id)
            })
            .map(|(id, _)| id)
            .collect();
        available.extend(arcane);
        // What everyone, or a retort, starts with.
        let given = library.always_known.iter().copied().chain(
            library
                .retort_known
                .iter()
                .filter(|(r, _)| retorts.contains(r))
                .flat_map(|(_, spells)| spells.iter().copied()),
        );
        known.extend(given);
        *self.get_mut::<KnownSpells>() = KnownSpells {
            researchable: available.difference(&known).copied().collect(),
            starting: known.clone(),
            known,
        };
        self.get_mut::<Candidates>().0.clear();
    }

    /// A book of `realm` was found: the realm's table row for its new number of books
    /// (`spell_library.researchable`) lets more of its spells be researched, picked at random
    /// from those not yet known or open. The wizard's offer is refilled by the caller.
    pub fn open_book_spells(&mut self, content: &Content, rng: &mut GameRng, realm: RealmId) {
        let open = SpellLibraryDef::row(
            &content.settings.spell_library.researchable,
            self.view().books_in(realm).clamp(0, 255) as u8,
        );
        let mut taken: BTreeSet<SpellId> = {
            let spells = self.get_mut::<KnownSpells>();
            spells.known.union(&spells.researchable).copied().collect()
        };
        for (rarity, limit) in [
            (Rarity::Common, open.common),
            (Rarity::Uncommon, open.uncommon),
            (Rarity::Rare, open.rare),
            (Rarity::VeryRare, open.very_rare),
        ] {
            let pool = spells_of(content, realm, rarity);
            let have = pool.iter().filter(|s| taken.contains(s)).count();
            let more = pick_spells(rng, &pool, &taken, usize::from(limit).saturating_sub(have));
            taken.extend(&more);
            self.get_mut::<KnownSpells>().researchable.extend(more);
        }
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
            if let Some(som) = content.rules.spells.id_numbered(213)
            /* spell_of_mastery */
            {
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
            let bonus = w.research_bonus(&content.rules, def);
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
    /// casting skill (an archmage gets half as much again: `SkillGainPercent`). Returns the mana for the reserve.
    pub fn take_power(
        &mut self,
        rules: &crate::rules::Rules,
        split: &PowerShares,
        research: i32,
    ) -> HalfInt {
        let percent = self
            .view()
            .stat(rules, crate::rules::Stat::SkillGainPercent, 100);
        self.add_research(research);
        let skill = split.skill.max(0);
        self.get_mut::<Skill>().points += skill * percent / 100;
        split.mana.max(HalfInt::ZERO)
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
    pub fn pay_casting(&mut self, rules: &crate::rules::Rules, reserve: HalfInt) -> HalfInt {
        let nominal = self.view().casting_skill(rules);
        // Skill and mana trade one for one, in whole points; a half point of mana stays.
        let half = reserve - HalfInt::from_int(reserve.floor());
        let mut reserve = reserve.floor();
        let mut skill = *self.get_mut::<Skill>();
        if let Some(cast) = self.cast() {
            if let Ok(mut c) = self.world.get::<&mut Owed>(cast) {
                let units = skill.left.min(reserve).min(c.0).max(0);
                c.0 -= units;
                reserve -= units;
                skill.left = 0;
                if c.0 > 0 {
                    let units = nominal.min(reserve).min(c.0).max(0);
                    if c.0 - units == 0 {
                        c.0 = 0;
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
        HalfInt::from_int(reserve) + half
    }

    /// Research points reach the spell being researched (nothing when none is).
    pub fn add_research(&mut self, points: i32) {
        let mut r = self.get_mut::<Research>();
        if r.spell.is_some() {
            r.remaining = (r.remaining - points).max(0);
        }
    }

    /// The spell being cast is cast or given up (the hero stays; an item being made that was not
    /// handed over is lost).
    pub fn clear_casting(&mut self) {
        if let Some(cast) = self.cast() {
            let _ = self.world.despawn(cast);
        }
        if let Some(player) = crate::wizard::entity::player_id_of(self.world, self.entity) {
            if let Some((item, _)) = crate::item::entity::crafting_of(self.world, player) {
                let _ = self.world.despawn(item);
            }
        }
    }

    /// The cast being paid for.
    fn cast(&self) -> Option<hecs::Entity> {
        Spells::new(self.world).cast_of(self.entity)
    }

    /// Pay for a spell cast at once out of this turn's skill.
    pub fn spend_skill(&mut self, cost: i32) {
        self.get_mut::<Skill>().left -= cost;
    }

    /// Skill added to this turn's (heroes at the fortress add to it).
    pub fn add_skill_left(&mut self, skill: i32) {
        self.get_mut::<Skill>().left += skill;
    }

    /// Give up the spell being cast; the mana already paid is lost.
    pub fn cancel_casting(&mut self) {
        self.clear_casting();
    }
}

#[cfg(test)]
mod tests;
