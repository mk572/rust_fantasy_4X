//! The guards and treasure of the towers, magic nodes and encounter sites of a new world, made
//! from a budget as the original's `Create_Lair` does. What a site does in play (entering it,
//! the fight, the loot) is `f4x_core::feature::lair`.
//!
//! Differences from the original are listed in `docs/lairs.md`.
//!
//! Tests: below.

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::mapgen::GenParams;
use crate::place::Placed;
use f4x_core::feature::def::FeatureKind;
use f4x_core::feature::lair::Lair;
use f4x_core::feature::node::Node;
use f4x_core::map::{Map, Plane, Position};
use f4x_core::rules::lair::{HoardKind, LairRules};
use f4x_core::rules::{RealmId, UnitId};
use f4x_def::content::Content;

/// What kind of site a lair guards, for choosing its guards' realm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SiteType {
    Tower,
    Node(RealmId),
    /// Temples and ruins draw on death and life.
    Holy,
    /// Caves, lairs, dungeons and keeps draw on death, chaos and nature.
    Wild,
}

fn site_type(def: &f4x_core::feature::def::FeatureDef) -> SiteType {
    match def.kind {
        FeatureKind::Tower => SiteType::Tower,
        FeatureKind::Node => def.realm.map_or(SiteType::Wild, SiteType::Node),
        _ if def.holy => SiteType::Holy,
        _ => SiteType::Wild,
    }
}

/// `Random(n)` of the original: `1..=n`.
fn random(rng: &mut Pcg64Mcg, n: i32) -> i32 {
    rng.gen_range(1..=n.max(1))
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
    list: &[(UnitId, i32)],
    mut budget: i32,
) -> (Vec<(UnitId, u32)>, i32, i32) {
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
    if let Some((id, cost)) = draw(rng, budget, rules.first_spread, None) {
        let mut count = budget / cost;
        if count > 1 && random(rng, 2) == 1 {
            count -= 1;
        }
        count = count.min(rules.max_first_guards);
        budget -= cost * count;
        first_count = count;
        first_value = cost * count;
        guards.push((id, count as u32));
    }
    if first_count > 0 {
        let first_id = guards[0].0;
        if let Some((id, cost)) = draw(
            rng,
            budget,
            rules.second_spread - first_count,
            Some(first_id),
        ) {
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
    lair: &mut Lair,
    site: SiteType,
    plane: Plane,
    guard_value: i32,
    second_value: i32,
    treasure_percent: i32,
) {
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

/// Make one lair from its budget (the original's `Create_Lair`).
#[allow(clippy::too_many_arguments)]
fn make_lair(
    rng: &mut Pcg64Mcg,
    content: &Content,
    plane: Plane,
    site: SiteType,
    budget: i32,
    guard_percent: i32,
    treasure_percent: i32,
) -> Lair {
    let rules = &content.rules.lairs;
    let budget = budget * guard_percent / 100;
    let list = guard_realm(rng, site, content).map_or_else(Vec::new, |realm| {
        f4x_core::feature::lair::candidates(content, realm)
    });
    let (guards, first, second) = pick_guards(rng, rules, &list, budget);
    let mut lair = Lair {
        intact: true,
        guards,
        gold: 0,
        mana: 0,
        items: Vec::new(),
        prisoner: false,
        special: 0,
        fought_by: Vec::new(),
    };
    fill_hoard(
        rng,
        rules,
        &mut lair,
        site,
        plane,
        first,
        second,
        treasure_percent,
    );
    lair
}

/// Give every tower, node and encounter site on the map its guards and treasure, each with the
/// square it is on. A tower is one site, on its Arcanus square.
pub fn generate_lairs(
    seed: u64,
    params: &GenParams,
    map: &Map,
    placed: &Placed,
    nodes: &[(Position, Node)],
    content: &Content,
) -> Vec<(Position, Lair)> {
    let mut rng = Pcg64Mcg::seed_from_u64(seed ^ 0x1A12_5EED_600D_5CA1);
    let guards = i32::from(params.lair_guard_percent.min(300));
    let treasure = i32::from(params.lair_treasure_percent.min(500));
    let magic = i32::from(params.magic_strength.min(2));
    let mut lairs: Vec<(Position, Lair)> = Vec::new();
    // Towers stand on both planes and share one set of guards and treasure.
    let mut seen_towers: Vec<(u32, u32)> = Vec::new();
    for plane in Plane::all() {
        for y in 0..map.height() {
            for x in 0..map.width() {
                let Some(def) = placed
                    .get(plane, x as i32, y as i32)
                    .and_then(|f| content.features.get(f))
                else {
                    continue;
                };
                if def.kind != FeatureKind::Tower || seen_towers.contains(&(x, y)) {
                    continue;
                }
                seen_towers.push((x, y));
                let budget = content.rules.lairs.tower_budget.base
                    + content.rules.lairs.tower_budget.step
                        * random(&mut rng, content.rules.lairs.tower_budget.rolls);
                let lair = make_lair(
                    &mut rng,
                    content,
                    Plane::Arcanus,
                    SiteType::Tower,
                    budget,
                    guards,
                    treasure,
                );
                lairs.push((
                    Position {
                        plane: Plane::Arcanus,
                        x,
                        y,
                    },
                    lair,
                ));
            }
        }
    }
    for (pos, node) in nodes {
        let power = i32::from(node.power);
        let rules = &content.rules.lairs;
        let budget =
            (power * power * (rules.node_base + random(&mut rng, rules.node_rolls)) * (magic + 1)
                / 2)
            .min(rules.node_cap);
        let lair = make_lair(
            &mut rng,
            content,
            pos.plane,
            SiteType::Node(node.realm),
            budget,
            guards,
            treasure,
        );
        lairs.push((*pos, lair));
    }
    // The random sites, in map order; exactly `strong_sites` of them (picked at random) are the
    // strong ones, the rest weak.
    let mut sites = Vec::new();
    for plane in Plane::all() {
        for y in 0..map.height() {
            for x in 0..map.width() {
                let Some(def) = placed
                    .get(plane, x as i32, y as i32)
                    .and_then(|f| content.features.get(f))
                else {
                    continue;
                };
                if def.kind == FeatureKind::Encounter {
                    sites.push((plane, x, y, def));
                }
            }
        }
    }
    let mut order: Vec<usize> = (0..sites.len()).collect();
    for i in (1..order.len()).rev() {
        order.swap(i, rng.gen_range(0..=i));
    }
    let strong_count = (params.strong_sites as usize).min(sites.len());
    let strong: Vec<bool> = {
        let mut flags = vec![false; sites.len()];
        for &i in &order[..strong_count] {
            flags[i] = true;
        }
        flags
    };
    for (i, &(plane, x, y, def)) in sites.iter().enumerate() {
        let rules = &content.rules.lairs;
        let roll = if strong[i] {
            rules.strong_budget[plane.index()]
        } else {
            rules.weak_budget[plane.index()]
        };
        let budget = roll.base + random(&mut rng, roll.rolls) * roll.step;
        let lair = make_lair(
            &mut rng,
            content,
            plane,
            site_type(def),
            budget,
            guards,
            treasure,
        );
        lairs.push((Position { plane, x, y }, lair));
    }
    lairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::map::{Grid, Map};
    use f4x_def::testdir::{shipped, TestResult};

    /// The numbers of the shipped content's tower, encounter site and grassland.
    const TOWER: u16 = 21;
    const MONSTER_LAIR: u16 = 13;
    const GRASSLAND: u16 = 4;

    fn realm(n: u16) -> RealmId {
        RealmId::new(n)
    }

    fn grass_map(c: &Content, w: u32, h: u32) -> Map {
        let grass = c
            .terrains
            .id_numbered(GRASSLAND)
            .expect("the shipped content has grassland");
        Map::new(Grid::new(w, h, grass), Grid::new(w, h, grass))
    }

    #[test]
    fn only_summoned_non_transport_units_of_the_realm_can_guard() {
        let c = shipped();
        let death: Vec<&str> = f4x_core::feature::lair::candidates(c, realm(4 /* death */))
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
        assert!(f4x_core::feature::lair::candidates(c, RealmId::new(99)).is_empty());
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
        let list = f4x_core::feature::lair::candidates(shipped(), realm(4 /* death */));
        let rules = LairRules::default();
        let mut rng = Pcg64Mcg::seed_from_u64(1);
        for budget in [0, 20, 60, 200, 700, 1500, 4000] {
            for _ in 0..50 {
                let (guards, first, second) = pick_guards(&mut rng, &rules, &list, budget);
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
        let (guards, ..) = pick_guards(&mut rng, &rules, &list, 20);
        assert!(guards.is_empty());
    }

    #[test]
    fn a_site_type_decides_the_guards_realm() {
        let rules = shipped();
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
    fn every_site_gets_a_lair_and_a_tower_has_one_for_both_planes() -> TestResult {
        let c = shipped();
        let map = grass_map(c, 24, 14);
        let mut placed = Placed::new(24, 14);
        let id = |n: u16| c.features.id_numbered(n).expect("id_numbered succeeds");
        placed.set(Plane::Arcanus, 3, 3, id(TOWER));
        placed.set(Plane::Myrror, 3, 3, id(TOWER));
        placed.set(Plane::Arcanus, 8, 5, id(MONSTER_LAIR));
        placed.set(Plane::Myrror, 9, 5, id(MONSTER_LAIR));
        placed.set(Plane::Myrror, 12, 9, id(MONSTER_LAIR));
        let params = GenParams::default();
        let lairs = generate_lairs(11, &params, &map, &placed, &[], c);
        assert_eq!(lairs.len(), 1 + 3, "one tower, three encounter sites");
        assert!(lairs.iter().all(|(_, l)| l.intact));
        let towers: Vec<_> = lairs.iter().filter(|(p, _)| (p.x, p.y) == (3, 3)).collect();
        assert_eq!(towers.len(), 1);
        assert_eq!(towers[0].0.plane, Plane::Arcanus);
        assert!(towers[0].1.special >= 1, "a tower always holds a spell");
        // The same seed gives the same lairs.
        let again = generate_lairs(11, &params, &map, &placed, &[], c);
        assert_eq!(again, lairs);
        let other = generate_lairs(12, &params, &map, &placed, &[], c);
        assert_ne!(other, lairs);
        Ok(())
    }

    #[test]
    fn hoards_follow_the_original_rules() {
        let c = shipped();
        let mut rng = Pcg64Mcg::seed_from_u64(5);
        let mut saw_items = false;
        let mut saw_big = false;
        for i in 0..3000 {
            let plane = if i % 2 == 0 {
                Plane::Arcanus
            } else {
                Plane::Myrror
            };
            let l = make_lair(
                &mut rng,
                c,
                plane,
                SiteType::Wild,
                200 + (i % 40) * 100,
                75,
                133,
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
                    make_lair(
                        &mut rng,
                        c,
                        Plane::Arcanus,
                        SiteType::Wild,
                        900,
                        guard_percent,
                        100,
                    )
                    .guard_count()
                })
                .sum::<u32>()
        };
        assert!(value(125) > value(25));
    }

    /// Every lair of a busy map, for a few seeds, as one number: the lair tables moved to data
    /// without changing a generated world.
    fn lair_checksum() -> u64 {
        let c = shipped();
        let map = grass_map(c, 24, 14);
        let mut placed = Placed::new(24, 14);
        let id = |n: u16| c.features.id_numbered(n).expect("id_numbered succeeds");
        for (i, plane) in Plane::all().enumerate() {
            for k in 0..10u32 {
                placed.set(plane, 1 + k, 1 + 4 * i as u32, id(MONSTER_LAIR));
            }
        }
        placed.set(Plane::Arcanus, 3, 9, id(TOWER));
        placed.set(Plane::Myrror, 3, 9, id(TOWER));
        placed.set(Plane::Arcanus, 20, 9, id(TOWER));
        placed.set(Plane::Myrror, 20, 9, id(TOWER));
        let nodes: Vec<(Position, Node)> = [
            1, /* sorcery */
            0, /* nature */
            2, /* chaos */
        ]
        .iter()
        .enumerate()
        .map(|(i, realm)| {
            (
                Position {
                    plane: if i % 2 == 0 {
                        Plane::Arcanus
                    } else {
                        Plane::Myrror
                    },
                    x: 10 + i as u32,
                    y: 12,
                },
                Node {
                    realm: self::realm(*realm),
                    power: 5 + i as u8,
                    aura: Vec::new(),
                    guardian: false,
                    warped: false,
                },
            )
        })
        .collect();
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for seed in 1..6 {
            let params = GenParams {
                strong_sites: 8,
                weak_sites: 12,
                ..GenParams::default()
            };
            let lairs = generate_lairs(seed, &params, &map, &placed, &nodes, c);
            for byte in format!("{lairs:?}").bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash
    }

    #[test]
    fn generated_lairs_for_a_seed_stay_the_same() {
        assert_eq!(lair_checksum(), 11_489_091_015_628_393_938);
    }
}
