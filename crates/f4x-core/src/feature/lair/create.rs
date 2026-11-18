//! The guards and treasure of a tower, a magic node or an encounter site, made from a budget as
//! the original's `Create_Lair` does. A new world calls it once for each site on the map
//! (`f4x_newgame::lair`); a ruin left by a rampage is made in `threat/monster.rs`.
//!
//! Differences from the original are listed in `docs/lairs.md`.

use rand_pcg::Pcg64Mcg;

use super::Lair;
use crate::feature::def::FeatureKind;
use crate::game::rng::random;
use crate::map::Plane;
use crate::rules::lair::{HoardKind, LairRules, LairStyle, Roll};
use crate::rules::{RealmId, UnitId};
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

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_def::testdir::TestResult;
    use rand::SeedableRng;

    fn realm(n: u16) -> RealmId {
        RealmId::new(n)
    }

    #[test]
    fn only_summoned_non_transport_units_of_the_realm_can_guard() {
        let c = crate::game::testutil::shipped();
        let death: Vec<&str> = super::super::candidates(&c, realm(4 /* death */))
            .into_iter()
            .map(|(id, _)| c.rules.units.get(id).id.as_str())
            .collect();
        assert_eq!(
            death,
            [
                "skeletons",
                "ghouls",
                "night_stalker",
                "werewolves",
                "demon",
                "wraiths",
                "shadow_demons",
                "death_knights",
                "demon_lord",
                "zombies"
            ],
            "every summoned creature of death, in the original's order, no ship and no other realm"
        );
        assert!(super::super::candidates(&c, RealmId::new(99)).is_empty());
    }

    #[test]
    fn the_dearest_affordable_unit_is_chosen_and_the_first_of_equals() -> TestResult {
        let list = vec![
            (UnitId::new(1), 25),
            (UnitId::new(2), 80),
            (UnitId::new(3), 80),
            (UnitId::new(4), 500),
        ];
        assert_eq!(
            dearest_under(&list, 501, None)
                .ok_or("dearest_under is missing")?
                .0,
            UnitId::new(4)
        );
        assert_eq!(
            dearest_under(&list, 500, None)
                .ok_or("dearest_under is missing")?
                .0,
            UnitId::new(2),
            "strictly cheaper"
        );
        assert_eq!(
            dearest_under(&list, 500, Some(UnitId::new(2)))
                .ok_or("dearest_under is missing")?
                .0,
            UnitId::new(3)
        );
        assert_eq!(dearest_under(&list, 25, None), None);
        Ok(())
    }

    #[test]
    fn guards_fit_the_budget_and_the_stack() {
        let list =
            super::super::candidates(&crate::game::testutil::shipped(), realm(4 /* death */));
        let content = crate::game::testutil::shipped();
        let rules = &content.rules.lairs;
        let mut rng = Pcg64Mcg::seed_from_u64(1);
        for budget in [0, 20, 60, 200, 700, 1500, 4000] {
            for _ in 0..50 {
                let (guards, first, second) =
                    pick_guards(&mut rng, rules, LairStyle::default(), &list, budget);
                let total: u32 = guards.iter().map(|g| g.1).sum();
                assert!(total <= rules.max_guards, "{total} guards for {budget}");
                assert!(first + second <= budget, "{first}+{second} over {budget}");
                if guards.len() == 2 {
                    assert_ne!(guards[0].0, guards[1].0, "two different kinds");
                    assert!(guards[0].1 <= rules.max_first_guards as u32);
                }
            }
        }
        // A budget below the cheapest unit buys nothing.
        let (guards, ..) = pick_guards(&mut rng, rules, LairStyle::default(), &list, 20);
        assert!(guards.is_empty());
    }

    #[test]
    fn a_site_type_decides_the_guards_realm() {
        let rules = &crate::game::testutil::shipped();
        let mut rng = Pcg64Mcg::seed_from_u64(2);
        for _ in 0..100 {
            assert_eq!(
                guard_realm(&mut rng, SiteType::Node(realm(1 /* sorcery */)), rules),
                Some(realm(1 /* sorcery */))
            );
            assert_eq!(
                guard_realm(&mut rng, SiteType::Node(realm(0 /* nature */)), rules),
                Some(realm(0 /* nature */))
            );
            assert_eq!(
                guard_realm(&mut rng, SiteType::Node(realm(2 /* chaos */)), rules),
                Some(realm(2 /* chaos */))
            );
            let holy = guard_realm(&mut rng, SiteType::Holy, rules).unwrap();
            assert!([realm(4 /* death */), realm(3 /* life */)].contains(&holy));
            let wild = guard_realm(&mut rng, SiteType::Wild, rules).unwrap();
            assert!([
                realm(4 /* death */),
                realm(2 /* chaos */),
                realm(0 /* nature */)
            ]
            .contains(&wild));
        }
        let seen: std::collections::BTreeSet<_> = (0..300)
            .map(|_| guard_realm(&mut rng, SiteType::Tower, rules))
            .collect();
        assert_eq!(seen.len(), 5, "towers draw on every realm");
        let site = |num: u16| {
            let features = &rules.features;
            site_type(
                features
                    .get(features.id_numbered(num).expect("a shipped feature"))
                    .expect("a shipped feature"),
            )
        };
        assert_eq!(site(3 /* ancient_temple */), SiteType::Holy);
        assert_eq!(site(18 /* ruins */), SiteType::Holy);
        assert_eq!(site(7 /* dungeon */), SiteType::Wild);
        assert_eq!(
            site(4 /* chaos_node */),
            SiteType::Node(realm(2 /* chaos */))
        );
    }

    #[test]
    fn hoards_follow_the_original_rules() {
        let c = crate::game::testutil::shipped();
        let mut rng = Pcg64Mcg::seed_from_u64(5);
        let mut saw_items = false;
        let mut saw_big = false;
        for i in 0..3000 {
            let plane = if i % 2 == 0 {
                Plane::Arcanus
            } else {
                Plane::Myrror
            };
            let l = create(
                &mut rng,
                &c,
                plane,
                SiteType::Wild,
                200 + (i % 40) * 100,
                75,
                133,
                LairStyle::default(),
            );
            assert_eq!(l.gold % 10, 0);
            assert_eq!(l.mana % 10, 0);
            assert!(l.items.len() <= 3);
            assert!(l.items.iter().all(|&v| v >= 100 && v % 10 == 0));
            assert!(l.special <= 6);
            if l.special > 4 {
                saw_big = true;
                assert_eq!(
                    (l.gold, l.mana, l.items.len(), l.prisoner),
                    (0, 0, 0, false)
                );
            }
            saw_items |= !l.items.is_empty();
        }
        assert!(saw_items && saw_big);
        // Impossible keeps more of the budget (125%): more guards than on Intro (25%).
        let value = |guard_percent| {
            let mut rng = Pcg64Mcg::seed_from_u64(9);
            (0..200)
                .map(|_| {
                    create(
                        &mut rng,
                        &c,
                        Plane::Arcanus,
                        SiteType::Wild,
                        900,
                        guard_percent,
                        100,
                        LairStyle::default(),
                    )
                    .guard_count()
                })
                .sum::<u32>()
        };
        assert!(value(125) > value(25));
    }

    const STRONGER: LairStyle = LairStyle {
        stronger: true,
        treasure: false,
    };
    const TREASURE_150: LairStyle = LairStyle {
        stronger: false,
        treasure: true,
    };

    /// Patch 1.50's budgets, from the ranges of the wiki's Encounter Zone article.
    #[test]
    fn the_stronger_lairs_have_the_patchs_budget_ranges() {
        let c = crate::game::testutil::shipped();
        let span = |f: &dyn Fn(&mut Pcg64Mcg) -> i32| {
            let mut rng = Pcg64Mcg::seed_from_u64(3);
            let v: Vec<i32> = (0..6000).map(|_| f(&mut rng)).collect();
            (*v.iter().min().unwrap_or(&0), *v.iter().max().unwrap_or(&0))
        };
        assert_eq!(span(&|r| tower_budget(r, &c, STRONGER)), (700, 3050));
        assert_eq!(
            span(&|r| tower_budget(r, &c, LairStyle::default())),
            (700, 1200)
        );
        let site = |plane, strong| span(&|r| site_budget(r, &c, plane, strong, STRONGER));
        assert_eq!(site(Plane::Arcanus, false), (30, 600));
        assert_eq!(site(Plane::Myrror, false), (30, 900));
        assert_eq!(site(Plane::Arcanus, true), (300, 4250));
        assert_eq!(site(Plane::Myrror, true), (300, 4750));
    }

    #[test]
    fn the_stronger_lairs_have_six_of_the_first_kind_and_the_patchs_costs() {
        let c = crate::game::testutil::shipped();
        let rules = &c.rules.lairs;
        let mut rng = Pcg64Mcg::seed_from_u64(4);
        // A death guard list with the patch's costs: no two units of one cost.
        let mut list = super::super::candidates(&c, realm(4 /* death */));
        for (id, cost) in &mut list {
            if let Some(&(_, new)) = rules.patch_150.costs.iter().find(|(n, _)| *n == id.num()) {
                *cost = new;
            }
        }
        let mut costs: Vec<i32> = list.iter().map(|c| c.1).collect();
        costs.sort_unstable();
        assert_eq!(costs, [25, 30, 80, 125, 200, 300, 325, 500, 600, 900]);
        for budget in [200, 700, 1500, 4000, 8000] {
            for _ in 0..80 {
                let (guards, first, second) = pick_guards(&mut rng, rules, STRONGER, &list, budget);
                assert!(first + second <= budget);
                assert!(guards.iter().map(|g| g.1).sum::<u32>() <= rules.max_guards);
                if let Some(g) = guards.first() {
                    assert!(g.1 <= 6, "{} of the first kind", g.1);
                }
            }
        }
        // The costs of every realm are all different, as the patch meant.
        for r in 0..5 {
            let mut list = super::super::candidates(&c, realm(r));
            for (id, cost) in &mut list {
                if let Some(&(_, new)) = rules.patch_150.costs.iter().find(|(n, _)| *n == id.num())
                {
                    *cost = new;
                }
            }
            let mut costs: Vec<i32> = list.iter().map(|c| c.1).collect();
            costs.sort_unstable();
            costs.dedup();
            assert_eq!(costs.len(), list.len(), "realm {r}");
        }
    }

    /// Patch 1.50's treasure: nothing random in the budget, the second kind at its full cost,
    /// 75 more, and a book or retort does not clear the rest.
    #[test]
    fn the_patchs_treasure_has_no_random_budget_and_keeps_the_rest_beside_a_book() {
        let c = crate::game::testutil::shipped();
        let mut rng = Pcg64Mcg::seed_from_u64(8);
        let (mut book_with_more, mut saw_items) = (false, false);
        for i in 0..4000 {
            let plane = if i % 2 == 0 {
                Plane::Arcanus
            } else {
                Plane::Myrror
            };
            let l = create(
                &mut rng,
                &c,
                plane,
                SiteType::Wild,
                300 + (i % 60) * 150,
                100,
                // The setting's treasure percent is not read.
                400,
                TREASURE_150,
            );
            assert_eq!(l.gold % 10, 0);
            assert_eq!(l.mana % 10, 0);
            assert!(l.items.len() <= 3 && l.special <= 6);
            // An item is kept at 120 percent of a price of 200 or more (the article: 240 to 6,120).
            assert!(l.items.iter().all(|&v| (240..=6120).contains(&v)));
            saw_items |= !l.items.is_empty();
            if l.special > 4 && (l.gold + l.mana > 0 || !l.items.is_empty() || l.prisoner) {
                book_with_more = true;
            }
        }
        assert!(saw_items && book_with_more);
        // The value of a hoard follows the guards alone: the same guards on Myrror give a quarter
        // more, whatever the difficulty's percent.
        let total = |plane, percent| {
            let mut rng = Pcg64Mcg::seed_from_u64(21);
            (0..300)
                .map(|_| {
                    let l = create(
                        &mut rng,
                        &c,
                        plane,
                        SiteType::Wild,
                        1500,
                        100,
                        percent,
                        TREASURE_150,
                    );
                    l.gold + l.mana + l.items.iter().sum::<u32>() + l.special as u32 * 1000
                })
                .sum::<u32>()
        };
        assert_eq!(total(Plane::Arcanus, 400), total(Plane::Arcanus, 100));
        assert!(total(Plane::Myrror, 100) > total(Plane::Arcanus, 100));
        // A site that guards nothing still has its 75 for a little gold or mana.
        let mut gifts = 0;
        for _ in 0..300 {
            let l = create(
                &mut rng,
                &c,
                Plane::Arcanus,
                SiteType::Wild,
                0,
                100,
                100,
                TREASURE_150,
            );
            assert!(l.items.is_empty() && !l.prisoner && l.special == 0);
            gifts += l.gold + l.mana;
        }
        assert!(gifts > 0);
    }
}
