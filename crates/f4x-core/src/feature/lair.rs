//! Guarded sites: who guards towers, magic nodes and encounter sites, what treasure they hold,
//! and what clearing one does.
//!
//! The original gives every tower, node and encounter site a record with up to two kinds of
//! guard and a hoard, made from a budget when the map is generated (ReMoM `Create_Lair`).
//! The guards are not on the map: they are created as neutral units when a stack enters the
//! site (`Lair_Make_Guardians`), fight the stack, and are removed again afterwards. A stack that
//! wins takes the hoard and the site is cleared; an encounter site then disappears from the map.
//!
//! Differences from the original are listed in `docs/lairs.md`.
//!
//! Tests: in feature/lair/tests.rs.

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::feature::def::FeatureKind;
use crate::feature::entity::LairInfo;
use crate::item::make::{make_item, Grade};
use crate::map::Position;
use crate::player::{PlayerId, MONSTERS};
use f4x_def::content::Content;

use crate::rules::{Rarity, RealmId, SpellId, Stat, UnitId, UnitKind};
use crate::unit::movement::MoveError;

use crate::game::{Event, Game};
use crate::wizard::{all_retorts, retort_available, retort_cost};

/// Units that can guard a site of `realm`: summoned creatures of that realm that are not
/// transports, with their costs, in the data's order.
pub fn candidates(content: &Content, realm: RealmId) -> Vec<(UnitId, i32)> {
    content
        .rules
        .units
        .defs()
        .iter()
        .filter(|d| {
            d.kind == UnitKind::Summoned
                && d.realm == Some(realm)
                && d.stats.get(&Stat::Transport).map_or(0, |v| v.floor()) == 0
        })
        .map(|d| (UnitId::new(d.num), d.cost as i32))
        .collect()
}

/// What the loot a winner cannot use turns into: the hoard's own table over `value`, without
/// the kinds a winner has no use for. Gold and mana are always of use; an item only when `items`
/// has room (at most that many, and each of 300 or more); no spell, book, retort or prisoner.
/// Returns the gold, the mana and the values of the items.
pub(crate) fn reroll_hoard(rng: &mut impl Rng, value: i32, items: usize) -> (u32, u32, Vec<u32>) {
    let mut loot = value;
    let (mut gold, mut mana) = (0, 0);
    let mut made = Vec::new();
    for _ in 0..2000 {
        if loot < 50 {
            break;
        }
        match rng.gen_range(0..15) {
            0 | 1 => {
                let price = rng.gen_range(1..=20) * 10;
                loot -= 200;
                gold += price.min(loot.max(0) + 200) as u32 / 10 * 10;
            }
            2 | 3 => {
                let price = rng.gen_range(1..=20) * 10;
                loot -= 200;
                mana += price.min(loot.max(0) + 200) as u32 / 10 * 10;
            }
            4..=8 if made.len() < items && loot >= 300 => {
                let price = (300 + rng.gen_range(1..=27) * 100).min(loot) / 10 * 10;
                loot -= price;
                made.push(price as u32);
            }
            _ => {}
        }
    }
    // A value too small for the table pays out as gold, so that nothing is lost to the roll.
    if gold + mana == 0 && made.is_empty() {
        gold = (value.max(0) / 4) as u32;
    }
    (gold, mana, made)
}

/// What a guarded site holds: its guards and its treasure. Towers, magic nodes and the
/// encounter sites each have one (the original's "lair" table), a component of their entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lair {
    /// False once cleared.
    pub intact: bool,
    /// Guard units with counts; empty for an unguarded site.
    pub guards: Vec<(UnitId, u32)>,
    pub gold: u32,
    pub mana: u32,
    /// The value of each magic item inside (items themselves wait for the hero tasks).
    pub items: Vec<u32>,
    /// A hero is held prisoner here.
    pub prisoner: bool,
    /// 0 nothing, 1 to 4 a spell of that rarity (1 common), 5 a spell book or a retort,
    /// 6 a spell book or retort worth two picks.
    pub special: u8,
    /// The players who have fought for this site once: they know what guards it.
    #[serde(default)]
    pub fought_by: Vec<PlayerId>,
}

impl Lair {
    /// Number of guard units.
    pub fn guard_count(&self) -> u32 {
        self.guards.iter().map(|g| g.1).sum()
    }
}

/// What a cleared site gave its conqueror.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Loot {
    pub gold: u32,
    pub mana: u32,
    /// The magic items found, made from the values in the hoard; they are in the armory.
    pub items: Vec<crate::item::Item>,
    /// A prisoner hero was found: a free offer of the hero waits on the wizard's offers.
    pub prisoner: bool,
    pub spell: Option<SpellId>,
    /// Spell books gained, by realm.
    pub books: Vec<RealmId>,
    pub retorts: Vec<crate::wizard::magic::Retort>,
}

/// Why a stack could not enter a site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteError {
    Move(MoveError),
    /// No guarded site on that square.
    NoSite,
    /// A battle is already being fought.
    Busy,
}

impl SiteError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Move(e) => e.key(),
            Self::NoSite => "refuse_map.site_error_no_site",
            Self::Busy => "refuse_map.site_error_busy",
        }
    }
}

/// A random spell of about `rarity` from a realm the wizard has books in, or arcane, that the
/// wizard does not know yet; the wizard learns it. A realm with one book gives up to uncommon
/// spells, two books up to rare, more up to very rare; arcane up to rare (ReMoM
/// `WIZ_RollSpellReward`, whose realm test is inverted and whose index arithmetic is muddled:
/// this follows its rarity caps and gives only spells the wizard can use). Dearer
/// rarities are tried when the first has nothing new; none at all gives `None`.
pub(crate) fn award_spell(
    wizard: &mut crate::wizard::sheet::WizardMut<'_>,
    content: &Content,
    rng: &mut crate::game::rng::GameRng,
    rarity: i32,
) -> Option<SpellId> {
    let mut realms: Vec<RealmId> = crate::rules::BOOK_REALMS
        .iter()
        .copied()
        .filter(|&r| wizard.view().books_in(r) > 0)
        .collect();
    realms.push(RealmId::new(5) /* arcane */);
    while !realms.is_empty() {
        let r = rng.roll(realms.len() as u32) as usize - 1;
        let realm = realms.remove(r);
        let cap = match (realm.num(), wizard.view().books_in(realm)) {
            (5 /* arcane */, _) => 3,
            (_, 1) => 2,
            (_, 2) => 3,
            _ => 4,
        };
        for step in 0..4 {
            let level = match (rarity + step).min(cap) {
                1 => Rarity::Common,
                2 => Rarity::Uncommon,
                3 => Rarity::Rare,
                _ => Rarity::VeryRare,
            };
            let pool: Vec<SpellId> = crate::wizard::magic::spells_of(content, realm, level)
                .into_iter()
                .filter(|s| !wizard.view().knows(*s))
                .filter(|s| {
                    !matches!(
                        content.rules.spells.get(*s).id.as_str(),
                        "spell_of_mastery" | "spell_of_return"
                    )
                })
                .collect();
            if !pool.is_empty() {
                let spell = pool[rng.roll(pool.len() as u32) as usize - 1];
                let mut spells = wizard.get_mut::<crate::wizard::sheet::KnownSpells>();
                spells.researchable.remove(&spell);
                spells.known.insert(spell);
                return Some(spell);
            }
        }
    }
    None
}

/// Spell book and retort treasure: three times in four a book of the site's realm (never life
/// with death or the reverse, nor to a wizard with more than twelve books already), otherwise a retort. A two-pick hoard
/// gives a two-pick retort or two books. The original's version loops and awards oddly (its
/// own comments call these bugs); this keeps the odds and the restrictions.
pub(crate) fn award_special(
    rules: &crate::rules::Rules,
    wizard: &mut crate::wizard::sheet::WizardMut<'_>,
    rng: &mut crate::game::rng::GameRng,
    lair: &LairInfo,
    picks: i32,
    loot: &mut Loot,
) {
    let (life, death) = (
        RealmId::new(3), /* life */
        RealmId::new(4), /* death */
    );
    let realm = match lair.reward {
        Some(realm) => realm,
        None => crate::rules::BOOK_REALMS[rng.roll(5) as usize - 1],
    };
    // Three times in four a book; the original turns to the other kind when the wizard is at
    // the limit of six retorts or more than twelve books.
    let wants_retort = if wizard.view().retorts().len() >= crate::wizard::MAX_RETORTS {
        false
    } else if wizard.view().total_books() > crate::wizard::sheet::BOOKS_BEFORE_LIMIT {
        true
    } else {
        rng.roll(100) > 75
    };
    if wants_retort {
        let double = |r| retort_cost(rules, r) == 2;
        let (view, books) = (wizard.view(), wizard.view().books());
        let options: Vec<_> = all_retorts(rules)
            .filter(|r| !view.has(*r) && retort_available(rules, *r, &books))
            .filter(
                |r| *r != crate::wizard::magic::Retort::new(9), /* myrran */
            )
            .filter(|&r| if picks == 2 { double(r) } else { !double(r) })
            .collect();
        if !options.is_empty() {
            let r = options[rng.roll(options.len() as u32) as usize - 1];
            wizard
                .get_mut::<crate::wizard::sheet::Retorts>()
                .0
                .insert(r);
            loot.retorts.push(r);
            return;
        }
    }
    for _ in 0..picks {
        let clash = (realm == life && wizard.view().books_in(death) > 0)
            || (realm == death && wizard.view().books_in(life) > 0);
        if clash || wizard.view().total_books() > crate::wizard::sheet::BOOKS_BEFORE_LIMIT {
            continue;
        }
        *wizard
            .get_mut::<crate::wizard::sheet::Books>()
            .0
            .entry(realm)
            .or_insert(0) += 1;
        loot.books.push(realm);
    }
}

/// Where a freed prisoner serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prisoner {
    /// A free offer of the hero on the wizard's offers.
    Offer,
    /// The hero stands in this city of a neutral player.
    Join(Position),
}

impl Game {
    /// Fight the guards of the site on `to`, which a stack next to it has been stopped at.
    /// The guards are created as neutral units on the site for the battle.
    pub fn enter_site(
        &mut self,
        units: &[hecs::Entity],
        to: Position,
    ) -> Result<Vec<Event>, SiteError> {
        if self.battle.is_some() {
            return Err(SiteError::Busy);
        }
        let lair = self.economy().lair_at(to).ok_or(SiteError::NoSite)?;
        let site = lair.entity;
        if lair.guards.is_empty() {
            return Err(SiteError::NoSite);
        }
        // The stack must be able to make the step (adjacent, moves left, and so on).
        let (_, entry) = self
            .economy()
            .plan_step(units, to)
            .map_err(SiteError::Move)?;
        if !matches!(entry, crate::unit::movement::Entry::Site(e) if e == site) {
            return Err(SiteError::NoSite);
        }
        let guards: Vec<(UnitId, u32)> = lair.guards.clone();
        let mut defenders = Vec::new();
        for (unit, count) in guards {
            for _ in 0..count {
                if defenders.len() < crate::unit::MAX_STACK {
                    defenders.push(self.spawn_unit(MONSTERS, to, unit));
                }
            }
        }
        match self.start_battle(units, &defenders) {
            Ok(events) => {
                // Whoever fights for a site, win or lose, knows what guards it from then on.
                if let Some(player) = units.first().and_then(|&u| self.units().unit_owner(u)) {
                    if let Ok(mut l) = self.world.get::<&mut Lair>(site) {
                        if !l.fought_by.contains(&player) {
                            l.fought_by.push(player);
                        }
                    }
                }
                if let Some(active) = self.battle.as_mut() {
                    active.lair = Some(site);
                }
                Ok(events)
            }
            Err(_) => {
                for d in defenders {
                    self.despawn_with_spells(d);
                }
                Err(SiteError::Busy)
            }
        }
    }

    /// A stack that has just arrived on a square with an unguarded site takes the treasure.
    pub(crate) fn collect_site(&mut self, units: &[hecs::Entity], to: Position) -> Vec<Event> {
        let Some(owner) = units.first().and_then(|&u| self.units().unit_owner(u)) else {
            return Vec::new();
        };
        if owner == MONSTERS {
            return self.occupy_zone(units, to);
        }
        match self.economy().lair_at(to) {
            Some(l) if l.guards.is_empty() => self.claim_lair(l.entity, owner),
            _ => Vec::new(),
        }
    }

    /// Clear the guarded site on an entity: it stops being intact, and an encounter site vanishes
    /// from the map (towers and nodes stay). Returns the site as it was, with its treasure;
    /// `None` if there is none or it is cleared already.
    pub(crate) fn clear_site(&mut self, entity: hecs::Entity) -> Option<LairInfo> {
        let lair = self.economy().lair_of(entity).filter(|l| l.intact)?;
        if let Ok(mut l) = self.world.get::<&mut Lair>(entity) {
            l.intact = false;
        }
        let is_encounter =
            self.content.features.get(lair.site).map(|d| d.kind) == Some(FeatureKind::Encounter);
        if is_encounter {
            self.despawn_feature(entity);
        }
        Some(lair)
    }

    /// Where a freed prisoner can serve `player`: a wizard gets a free offer; a neutral player
    /// gets the hero in the nearest of its cities that has room. `None` when no hero is left to
    /// free (every one serves, or is gone for good) or there is no room: the prisoner is then
    /// rerolled into the hoard.
    fn prisoner_place(&self, player: PlayerId, near: Position) -> Option<Prisoner> {
        if !self
            .units()
            .hero_summonable(player, crate::hero::kind::Class::Any)
        {
            return None;
        }
        if self.wizard(player).is_some() {
            return Some(Prisoner::Offer);
        }
        let economy = self.economy();
        let mut homes: Vec<Position> = economy
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == player)
            .map(|(_, _, p)| p)
            .filter(|&p| self.units().units_at(p).len() < self.max_tile())
            .collect();
        homes.sort_by_key(|p| {
            (
                p.plane != near.plane,
                p.x.abs_diff(near.x) + p.y.abs_diff(near.y),
            )
        });
        homes.first().map(|&p| Prisoner::Join(p))
    }

    /// Clear a lair and give `player` its treasure. What the winner cannot use is rerolled from
    /// its value (`reroll_hoard`).
    pub(crate) fn claim_lair(&mut self, site: hecs::Entity, player: PlayerId) -> Vec<Event> {
        let Some(mut lair) = self.clear_site(site) else {
            return Vec::new();
        };
        let pos = lair.pos;
        let wizard = self.wizard(player).is_some();
        let mut unused = 0i32;
        let mut prisoner = None;
        if lair.prisoner {
            prisoner = self.prisoner_place(player, pos);
            if prisoner.is_none() {
                unused += 1000;
            }
        }
        if !wizard {
            unused += match lair.special {
                1..=4 => i32::from(lair.special) * i32::from(lair.special) * 50,
                5 | 6 => 3000,
                _ => 0,
            };
            lair.special = 0;
        }
        // A wizard's spell comes first: a hoard with no spell the wizard can use pays its value.
        let mut spell = None;
        if wizard && matches!(lair.special, 1..=4) {
            spell = self.award_lair_spell(player, lair.special);
            if spell.is_none() {
                unused += i32::from(lair.special) * i32::from(lair.special) * 50;
            }
            lair.special = 0;
        }
        let has_hero = wizard
            || matches!(prisoner, Some(Prisoner::Join(_)))
            || !self.units().hero_units(player).is_empty();
        if !has_hero {
            unused += lair.items.iter().map(|&v| v as i32).sum::<i32>();
            lair.items.clear();
        }
        if unused > 0 {
            let room = if has_hero {
                3 - lair.items.len().min(3)
            } else {
                0
            };
            let (gold, mana, items) = reroll_hoard(&mut self.rng, unused, room);
            lair.gold += gold;
            lair.mana += mana;
            lair.items.extend(items);
        }
        let mut loot = Loot {
            gold: lair.gold,
            mana: lair.mana,
            spell,
            ..Loot::default()
        };
        let mut items: Vec<crate::item::Item> = Vec::new();
        for &value in &lair.items {
            items.push(make_item(
                &mut self.rng,
                &self.content.rules,
                Grade::Any,
                value,
                &mut self.prefab_taken,
            ));
        }
        if let Some(t) = self.treasury_mut(player) {
            t.gold += lair.gold as i32;
            t.mana += lair.mana as i32;
        }
        if let Some(mut w) = self
            .player_index
            .get(&player)
            .and_then(|&e| crate::wizard::entity::of_player(&self.world, e))
            .map(|e| crate::wizard::sheet::WizardMut::new(&mut self.world, e))
        {
            match lair.special {
                5 | 6 => {
                    let picks = i32::from(lair.special) - 4;
                    award_special(
                        &self.content.rules,
                        &mut w,
                        &mut self.rng,
                        &lair,
                        picks,
                        &mut loot,
                    );
                    // A book found opens more of its realm's spells to research.
                    for &realm in &loot.books {
                        w.open_book_spells(&self.content, &mut self.rng, realm);
                    }
                    // A retort found may bring fame with it (Famous: ten points).
                    let fame = crate::wizard::retort::stat(
                        &self.content.rules,
                        &loot.retorts.iter().copied().collect(),
                        crate::rules::Stat::StartingFame,
                        0,
                    );
                    w.get_mut::<crate::wizard::sheet::Fame>().0 += fame;
                }
                _ => {}
            }
        }
        if !loot.books.is_empty() {
            self.refill_research(player);
        }
        // A hoard of a book or a retort that gave nothing (nothing left to give) pays an item
        // worth 1200 instead, or 1000 mana when the hoard already has three items (the original
        // writes the item past the end of its list: ReMoM marks it a bug).
        if matches!(lair.special, 5 | 6) && loot.books.is_empty() && loot.retorts.is_empty() {
            if items.len() < 3 {
                items.push(make_item(
                    &mut self.rng,
                    &self.content.rules,
                    Grade::Any,
                    1200,
                    &mut self.prefab_taken,
                ));
            } else {
                loot.mana = 1000;
                if let Some(t) = self.treasury_mut(player) {
                    t.mana += 1000;
                }
            }
        }
        for item in &items {
            self.give_item(player, item.clone());
        }
        loot.items = items;
        // A prisoner is a hero for nothing: offered to a wizard, in service at once for a
        // neutral player.
        if let Some(place) = prisoner {
            if let Some(kind) = self.pick_hero(player, true, crate::hero::kind::Class::Any) {
                match place {
                    Prisoner::Offer => {
                        if let Some(o) = self.player_component_mut::<crate::player::Offers>(player)
                        {
                            o.0.insert(0, crate::player::offer::Offer::Hero { kind, price: 0 });
                            loot.prisoner = true;
                        }
                    }
                    Prisoner::Join(at) => {
                        loot.prisoner = self.spawn_hero(player, at, kind).is_ok();
                    }
                }
            }
        }
        let mut events = vec![Event::SiteCleared {
            player,
            pos,
            site: lair.site,
            guarded: !lair.guards.is_empty(),
            loot,
        }];
        if let Some(Event::SiteCleared { loot, .. }) = events.first() {
            if let Some(spell) = loot.spell {
                events.push(Event::SpellLearned { player, spell });
            }
        }
        events
    }

    /// A spell for the wizard `player` from a hoard of `rarity` (`award_spell`).
    fn award_lair_spell(&mut self, player: PlayerId, rarity: u8) -> Option<SpellId> {
        let entity = self.player_index.get(&player).copied()?;
        let wizard = crate::wizard::entity::of_player(&self.world, entity)?;
        let mut w = crate::wizard::sheet::WizardMut::new(&mut self.world, wizard);
        award_spell(&mut w, &self.content, &mut self.rng, i32::from(rarity))
    }

    /// After a battle at a site: the guards go, and a win clears the site.
    pub(crate) fn finish_site_battle(
        &mut self,
        site: hecs::Entity,
        winner: Option<PlayerId>,
        attackers: &[hecs::Entity],
        guards: &[hecs::Entity],
    ) -> Vec<Event> {
        let Some(lair) = self.economy().lair_of(site) else {
            return Vec::new();
        };
        let pos = lair.pos;
        // The guards were made for the fight and go with it. Other neutral units on the site
        // (the undead raised for the monsters) stay, as do guards the victors took as undead.
        for &u in guards {
            if self.units().unit_owner(u) == Some(MONSTERS) {
                self.despawn_with_spells(u);
            }
        }
        let Some(winner) = winner.filter(|&w| w != MONSTERS) else {
            return Vec::new();
        };
        let survivors: Vec<hecs::Entity> = attackers
            .iter()
            .copied()
            .filter(|&u| self.world.contains(u))
            .collect();
        let mut events = self.claim_lair(site, winner);
        if !survivors.is_empty() {
            let from = self.units().unit_position(survivors[0]);
            if self.move_stack(&survivors, pos).is_ok() {
                if let Some(from) = from {
                    events.push(Event::UnitsMoved {
                        units: survivors,
                        from,
                        to: pos,
                    });
                }
            }
        }
        events
    }
}

#[cfg(test)]
mod tests;
