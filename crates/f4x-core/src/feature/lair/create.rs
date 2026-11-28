//! The guards and treasure of a tower, a magic node or an encounter site, made from a budget as
//! the original's `Create_Lair` does. A new world calls it once for each site on the map
//! (`f4x_newgame::lair`); a ruin left by a rampage is made in `threat/monster.rs`.
//!
//! Differences from the original are listed in `docs/lairs.md`.

use rand_pcg::Pcg64Mcg;

use super::Lair;
use crate::feature::def::FeatureKind;
use crate::map::Plane;
use crate::rules::lair::{HoardKind, LairRules, LairStyle, Roll};
use crate::rules::{RealmId, UnitId};
use crate::world::rng::random;
use f4x_def::content::Content;

/// What kind of site a lair guards, for choosing its guards' realm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteType {
    Tower,
    Node(RealmId),
    /// Temples and ruins draw on death and life.
    Holy,
    /// Caves, lairs, dungeons and keeps draw on death, chaos and nature.
    Wild,
}

pub fn site_type(def: &crate::feature::def::FeatureDef) -> SiteType {
    match def.kind {
        FeatureKind::Tower => SiteType::Tower,
        FeatureKind::Node => def.realm.map_or(SiteType::Wild, SiteType::Node),
        _ if def.holy => SiteType::Holy,
        _ => SiteType::Wild,
    }
}

/// The realm of a site's guards.
fn guard_realm(rng: &mut Pcg64Mcg, site: SiteType, content: &Content) -> Option<RealmId> {
    let rules = &content.rules.lairs;
    let realms = &content.rules.realms;
    let table = |rng: &mut Pcg64Mcg, weights| {
        LairRules::realm_for(
            weights,
            random(rng, LairRules::total(weights) as i32) as u32,
        )
    };
    let num = match site {
        SiteType::Node(realm) => Some(realm.num())
            .filter(|n| rules.node_own_realms.contains(n))
            .unwrap_or(rules.node_other),
        SiteType::Tower => table(rng, &rules.tower_realms),
        SiteType::Holy => table(rng, &rules.holy_realms),
        // The original's switch falls through to nature in every case; the intended split
        // (ReMoM marks it a bug) is two parts death, two chaos, one nature.
        SiteType::Wild => table(rng, &rules.wild_realms),
    };
    realms.id_numbered(num)
}

/// The dearest candidate costing less than `limit` (the first of equals), skipping `not`.
fn dearest_under(
    list: &[(UnitId, i32)],
    limit: i32,
    not: Option<UnitId>,
) -> Option<&(UnitId, i32)> {
    let mut best: Option<&(UnitId, i32)> = None;
    for c in list {
        if not == Some(c.0) {
            continue;
        }
        if c.1 > best.map_or(0, |b| b.1) && c.1 < limit {
            best = Some(c);
        }
    }
    best
}

/// Choose the guards a budget buys: a first kind from a random share of the budget, then
/// a second kind from what is left, nine units at most. Returns the guards and their value.
fn pick_guards(
    rng: &mut Pcg64Mcg,
    rules: &LairRules,
    style: LairStyle,
    list: &[(UnitId, i32)],
    mut budget: i32,
) -> (Vec<(UnitId, u32)>, i32, i32) {
    // The patch's divisors and number of the first kind (1.50, note 015).
    let (max_first, first_spread, second_spread) = if style.stronger {
        let p = &rules.patch_150;
        (p.max_first_guards, p.first_spread, p.second_spread)
    } else {
        (
            rules.max_first_guards,
            rules.first_spread,
            rules.second_spread,
        )
    };
    let mut guards: Vec<(UnitId, u32)> = Vec::new();
    let draw = |rng: &mut Pcg64Mcg, budget: i32, spread: i32, not: Option<UnitId>| {
        for _ in 0..200 {
            let limit = budget / random(rng, spread);
            if let Some(c) = dearest_under(list, limit, not) {
                return Some(*c);
            }
        }
        None
    };
    let mut first_value = 0;
    let mut second_value = 0;
    let mut first_count = 0;
    if let Some((id, cost)) = draw(rng, budget, first_spread, None) {
        let mut count = budget / cost;
        if count > 1 && random(rng, 2) == 1 {
            count -= 1;
        }
        count = count.min(max_first);
        budget -= cost * count;
        first_count = count;
        first_value = cost * count;
        guards.push((id, count as u32));
    }
    if first_count > 0 {
        let first_id = guards[0].0;
        if let Some((id, cost)) = draw(rng, budget, second_spread - first_count, Some(first_id)) {
            let count = (budget / cost).min(rules.max_guards as i32 - first_count);
            if count > 0 {
                second_value = cost * count;
                guards.push((id, count as u32));
            }
        }
    }
    (guards, first_value, second_value)
}

/// Fill a lair's hoard from its guards' value (`Create_Lair` after the guards are chosen).
#[allow(clippy::too_many_arguments)]
fn fill_hoard(
    rng: &mut Pcg64Mcg,
    rules: &LairRules,
    style: LairStyle,
    lair: &mut Lair,
    site: SiteType,
    plane: Plane,
    guard_value: i32,
    second_value: i32,
    treasure_percent: i32,
) {
    if style.treasure {
        fill_hoard_150(
            rng,
            rules,
            lair,
            site,
            plane,
            guard_value + second_value * rules.patch_150.second_share / 100,
        );
        return;
    }
    let mut loot = guard_value + second_value * rules.second_share / 100;
    loot = loot * treasure_percent / 100;
    let plane_loot = rules.plane_loot[plane.index()];
    loot = loot * (plane_loot.base + random(rng, plane_loot.rolls)) / 100;
    loot = loot.max(rules.min_loot);
    let mut special = 0i32;
    if site == SiteType::Tower {
        // A tower always holds a spell, and pays for it out of the rest.
        let rarity = random(rng, rules.spell_rarities);
        loot -= rarity * rarity * rules.spell_cost - rules.tower_spell_refund;
        special = rarity;
    }
    let total: i32 = rules.hoard.iter().map(|(_, w)| *w as i32).sum();
    let coin = |rng: &mut Pcg64Mcg, loot: i32| {
        (random(rng, rules.coin_rolls) * rules.coin_step).min(loot) / 10 * 10
    };
    let mut guard = 0;
    while loot >= rules.min_loot && guard < 2000 {
        guard += 1;
        let mut slot = random(rng, total);
        let kind = rules
            .hoard
            .iter()
            .find(|(_, w)| {
                slot -= *w as i32;
                slot <= 0
            })
            .map(|(k, _)| *k);
        match kind {
            Some(HoardKind::Gold) => {
                let price = coin(rng, loot);
                loot -= rules.coin_cost;
                lair.gold += price as u32;
            }
            Some(HoardKind::Mana) => {
                let price = coin(rng, loot);
                loot -= rules.coin_cost;
                lair.mana += price as u32;
            }
            Some(HoardKind::Item) => {
                if lair.items.len() < rules.max_items && loot >= rules.item_min {
                    let price = (rules.item_base + random(rng, rules.item_rolls) * rules.item_step)
                        .min(loot)
                        / 10
                        * 10;
                    loot -= price;
                    lair.items.push(price as u32);
                }
            }
            Some(HoardKind::Prisoner) => {
                if loot >= rules.prisoner_min && !lair.prisoner {
                    lair.prisoner = true;
                    loot -= rules.prisoner_cost;
                }
            }
            Some(HoardKind::Spell) => {
                let rarity = random(rng, rules.spell_rarities);
                if rarity * rarity * rules.spell_cost <= loot && special < rules.spell_total {
                    loot -= rarity * rarity * rules.spell_cost;
                    special = (special + rarity).min(rules.spell_total);
                }
            }
            Some(HoardKind::Special) => {
                // A spell book or retort: two picks' worth when the budget allows.
                if random(rng, rules.double_chance) == 1 && loot >= rules.special_double_min
                    || loot >= rules.special_double_sure
                {
                    special = 6;
                    loot -= rules.special_cost;
                } else if loot >= rules.special_min {
                    special = 5;
                    loot -= rules.special_cost;
                }
            }
            None => {}
        }
    }
    if special > rules.spell_total {
        lair.gold = 0;
        lair.mana = 0;
        lair.items.clear();
        lair.prisoner = false;
    }
    lair.special = special as u8;
}

/// The hoard of patch 1.50 (notes 016 to 025): a budget with nothing random in it, a table with
/// more gold, mana and spells and fewer books and retorts, and a price for each kind that is the
/// amount itself. `value` is the guards' value, the second kind at its full cost.
fn fill_hoard_150(
    rng: &mut Pcg64Mcg,
    rules: &LairRules,
    lair: &mut Lair,
    site: SiteType,
    plane: Plane,
    value: i32,
) {
    let p = &rules.patch_150;
    let mut loot = value * p.plane_loot_percent[plane.index()] / 100 + p.bonus;
    // A spell (rarity 1 to 4) or a book or retort (5 and 6) shares one place; the book or retort
    // replaces the spell and no longer clears the rest.
    let mut special = 0i32;
    let spell_price = |rarity: i32| rarity * rarity * p.spell_cost;
    if site == SiteType::Tower {
        // A tower tries for a spell first, with no discount, and must qualify for it.
        let rarity = random(rng, rules.spell_rarities);
        if spell_price(rarity) <= loot {
            loot -= spell_price(rarity);
            special = rarity;
        }
    }
    let total: i32 = p.hoard.iter().map(|(_, w)| *w as i32).sum();
    // An amount of `step * Random(coin_rolls)`, at most what is left, whole tens.
    let coin = |rng: &mut Pcg64Mcg, step: i32, loot: i32| {
        (random(rng, p.coin_rolls) * step).min(loot) / 10 * 10
    };
    let mut guard = 0;
    while loot >= rules.min_loot && guard < 2000 {
        guard += 1;
        let mut slot = random(rng, total);
        let kind = p
            .hoard
            .iter()
            .find(|(_, w)| {
                slot -= *w as i32;
                slot <= 0
            })
            .map(|(k, _)| *k);
        match kind {
            Some(HoardKind::Gold) => {
                let amount = coin(rng, p.gold_step, loot);
                loot -= amount;
                lair.gold += amount as u32;
            }
            Some(HoardKind::Mana) => {
                let amount = coin(rng, p.mana_step, loot);
                loot -= amount;
                lair.mana += amount as u32;
            }
            Some(HoardKind::Item) => {
                if lair.items.len() < rules.max_items && loot >= rules.item_min {
                    let price =
                        (p.item_base + random(rng, p.item_rolls) * p.item_step).min(loot) / 10 * 10;
                    loot -= price;
                    lair.items.push((price * p.item_max_percent / 100) as u32);
                }
            }
            Some(HoardKind::Prisoner) => {
                if loot >= rules.prisoner_min && !lair.prisoner {
                    lair.prisoner = true;
                    loot -= p.prisoner_cost;
                }
            }
            Some(HoardKind::Spell) => {
                // One spell a hoard, and none beside a book or retort; the rolls do not add up.
                let rarity = random(rng, rules.spell_rarities);
                if special == 0 && spell_price(rarity) <= loot {
                    loot -= spell_price(rarity);
                    special = rarity;
                }
            }
            Some(HoardKind::Special) => {
                // Most are thrown back and the table is rolled again.
                if random(rng, 100) <= p.special_discard_percent {
                    continue;
                }
                if special <= rules.spell_total && loot >= p.special_cost {
                    special = if loot >= p.special_double_sure { 6 } else { 5 };
                    loot -= p.special_cost;
                }
            }
            None => {}
        }
    }
    lair.special = special as u8;
}

/// Make one lair from its budget (the original's `Create_Lair`): the guards and the hoard.
/// `guard_percent` and `treasure_percent` are the game's difficulty settings.
#[allow(clippy::too_many_arguments)]
pub fn create(
    rng: &mut Pcg64Mcg,
    content: &Content,
    plane: Plane,
    site: SiteType,
    budget: i32,
    guard_percent: i32,
    treasure_percent: i32,
    style: LairStyle,
) -> Lair {
    let rules = &content.rules.lairs;
    let budget = budget * guard_percent / 100;
    let mut list = guard_realm(rng, site, content)
        .map_or_else(Vec::new, |realm| super::candidates(content, realm));
    if style.stronger {
        for (id, cost) in &mut list {
            if let Some(&(_, new)) = rules.patch_150.costs.iter().find(|(n, _)| *n == id.num()) {
                *cost = new;
            }
        }
    }
    let (guards, first, second) = pick_guards(rng, rules, style, &list, budget);
    let mut lair = Lair {
        intact: true,
        guards,
        gold: 0,
        mana: 0,
        items: Vec::new(),
        prisoner: false,
        special: 0,
        fought_by: Vec::new(),
        hurt: Vec::new(),
        apparent: None,
    };
    fill_hoard(
        rng,
        rules,
        style,
        &mut lair,
        site,
        plane,
        first,
        second,
        treasure_percent,
    );
    lair
}

/// The budget of a tower: the one lair the tower has on both planes.
pub fn tower_budget(rng: &mut Pcg64Mcg, content: &Content, style: LairStyle) -> i32 {
    let rules = &content.rules.lairs;
    let b = if style.stronger {
        &rules.patch_150.tower_budget
    } else {
        &rules.tower_budget
    };
    b.base + b.step * random(rng, b.rolls)
}

/// The budget of a magic node of this power; `magic` is the world's magic strength (0 to 2).
pub fn node_budget(rng: &mut Pcg64Mcg, content: &Content, power: i32, magic: i32) -> i32 {
    let rules = &content.rules.lairs;
    (power * power * (rules.node_base + random(rng, rules.node_rolls)) * (magic + 1) / 2)
        .min(rules.node_cap)
}

/// The budget of an encounter site on `plane`, a strong one or a weak one.
pub fn site_budget(
    rng: &mut Pcg64Mcg,
    content: &Content,
    plane: Plane,
    strong: bool,
    style: LairStyle,
) -> i32 {
    let rules = &content.rules.lairs;
    let (strong_rolls, weak_rolls) = if style.stronger {
        (&rules.patch_150.strong_budget, &rules.patch_150.weak_budget)
    } else {
        (&rules.strong_budget, &rules.weak_budget)
    };
    let roll: Roll = if strong {
        strong_rolls[plane.index()]
    } else {
        weak_rolls[plane.index()]
    };
    roll.base + random(rng, roll.rolls) * roll.step
}
