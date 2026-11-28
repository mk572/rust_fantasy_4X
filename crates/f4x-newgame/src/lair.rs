//! The guards and treasure of the towers, magic nodes and encounter sites of a new world: this
//! walks the map and gives each site its budget; one lair is made in
//! `f4x_core::feature::lair::create` (the original's `Create_Lair`). What a site does in play
//! (entering it, the fight, the loot) is `f4x_core::feature::lair`.
//!
//! Differences from the original are listed in `docs/lairs.md`.
//!
//! Tests: below.

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::mapgen::GenParams;
use crate::place::Placed;
use f4x_core::feature::def::FeatureKind;
use f4x_core::feature::lair::{create, Lair};
use f4x_core::feature::node::Node;
use f4x_core::map::{Map, Plane, Position};
use f4x_def::content::Content;

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
    let style = params.lair_style;
    // Patch 1.50's scale of the guards by difficulty replaces the setting's percent; its treasure
    // has no reverse of the difficulty.
    let guards = if style.stronger {
        content
            .rules
            .lairs
            .patch_150
            .guard_percent
            .get(usize::from(params.difficulty))
            .copied()
            .unwrap_or(100)
    } else {
        i32::from(params.lair_guard_percent.min(300))
    };
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
                let budget = create::tower_budget(&mut rng, content, style);
                let lair = create::create(
                    &mut rng,
                    content,
                    Plane::Arcanus,
                    create::SiteType::Tower,
                    budget,
                    guards,
                    treasure,
                    style,
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
        let budget = create::node_budget(&mut rng, content, i32::from(node.power), magic);
        let lair = create::create(
            &mut rng,
            content,
            pos.plane,
            create::SiteType::Node(node.realm),
            budget,
            guards,
            treasure,
            style,
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
        let budget = create::site_budget(&mut rng, content, plane, strong[i], style);
        let lair = create::create(
            &mut rng,
            content,
            plane,
            create::site_type(def),
            budget,
            guards,
            treasure,
            style,
        );
        lairs.push((Position { plane, x, y }, lair));
    }
    lairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::map::{Grid, Map};
    use f4x_core::rules::RealmId;
    use f4x_def::testdir::{shipped, TestResult};

    fn grass_map(c: &Content, w: u32, h: u32) -> Map {
        let grass = c
            .terrains
            .id_numbered(4 /* grassland */)
            .expect("the shipped content has grassland");
        Map::new(Grid::new(w, h, grass), Grid::new(w, h, grass))
    }

    #[test]
    fn every_site_gets_a_lair_and_a_tower_has_one_for_both_planes() -> TestResult {
        let c = shipped();
        let map = grass_map(c, 24, 14);
        let mut placed = Placed::new(24, 14);
        let id = |n: u16| c.features.id_numbered(n).expect("id_numbered succeeds");
        placed.set(Plane::Arcanus, 3, 3, id(21 /* tower */));
        placed.set(Plane::Myrror, 3, 3, id(21 /* tower */));
        placed.set(Plane::Arcanus, 8, 5, id(13 /* monster_lair */));
        placed.set(Plane::Myrror, 9, 5, id(13 /* monster_lair */));
        placed.set(Plane::Myrror, 12, 9, id(13 /* monster_lair */));
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

    /// Every lair of a busy map, for a few seeds, as one number: the lair tables moved to data
    /// without changing a generated world.
    fn lair_checksum() -> u64 {
        let c = shipped();
        let map = grass_map(c, 24, 14);
        let mut placed = Placed::new(24, 14);
        let id = |n: u16| c.features.id_numbered(n).expect("id_numbered succeeds");
        for (i, plane) in Plane::all().enumerate() {
            for k in 0..10u32 {
                placed.set(plane, 1 + k, 1 + 4 * i as u32, id(13 /* monster_lair */));
            }
        }
        placed.set(Plane::Arcanus, 3, 9, id(21 /* tower */));
        placed.set(Plane::Myrror, 3, 9, id(21 /* tower */));
        placed.set(Plane::Arcanus, 20, 9, id(21 /* tower */));
        placed.set(Plane::Myrror, 20, 9, id(21 /* tower */));
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
                    realm: RealmId::new(*realm),
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
            // `hurt` (the wounds of a lair's guards, empty in a new world) and `apparent` (set when
            // the site is placed) came after the number was taken: leave them out so that the
            // number still says "generation is unchanged".
            let text = format!("{lairs:?}")
                .replace(", hurt: []", "")
                .replace(", apparent: None", "");
            for byte in text.bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash
    }

    #[test]
    fn generated_lairs_for_a_seed_stay_the_same() {
        assert_eq!(lair_checksum(), 11_489_091_015_628_393_938);
    }

    /// The two switches of patch 1.50 on a busy map: the guards are stronger and follow the
    /// patch's scale of difficulty (not `lair_guard_percent`), the treasure ignores
    /// `lair_treasure_percent`, and each switch alone leaves the other's numbers as they were.
    #[test]
    fn the_patch_switches_change_the_guards_and_the_treasure_of_a_new_world() {
        use f4x_core::rules::lair::LairStyle;
        let c = shipped();
        let map = grass_map(c, 24, 14);
        let mut placed = Placed::new(24, 14);
        let id = |n: u16| c.features.id_numbered(n).expect("id_numbered succeeds");
        for (i, plane) in Plane::all().enumerate() {
            for k in 0..10u32 {
                placed.set(plane, 1 + k, 1 + 4 * i as u32, id(13 /* monster_lair */));
            }
        }
        let make = |style: LairStyle, guard_percent: u16, treasure_percent: u16| {
            let params = GenParams {
                strong_sites: 10,
                weak_sites: 10,
                difficulty: 3, // Hard: 100 percent of the guards in both scales
                lair_guard_percent: guard_percent,
                lair_treasure_percent: treasure_percent,
                lair_style: style,
                ..GenParams::default()
            };
            generate_lairs(5, &params, &map, &placed, &[], c)
        };
        let guards =
            |lairs: &[(Position, Lair)]| lairs.iter().map(|(_, l)| l.guard_count()).sum::<u32>();
        let treasure = |lairs: &[(Position, Lair)]| {
            lairs
                .iter()
                .map(|(_, l)| l.gold + l.mana + l.items.iter().sum::<u32>())
                .sum::<u32>()
        };
        let original = make(LairStyle::default(), 100, 100);
        let stronger = make(
            LairStyle {
                stronger: true,
                treasure: false,
            },
            25,
            100,
        );
        assert!(
            guards(&stronger) > guards(&original),
            "stronger budgets, whatever the setting says"
        );
        assert_eq!(
            stronger,
            make(
                LairStyle {
                    stronger: true,
                    treasure: false
                },
                300,
                100
            ),
            "the percent of the setting is not read"
        );
        let patch_treasure = make(
            LairStyle {
                stronger: false,
                treasure: true,
            },
            100,
            100,
        );
        assert_eq!(
            patch_treasure,
            make(
                LairStyle {
                    stronger: false,
                    treasure: true
                },
                100,
                400
            ),
            "the treasure percent is not read"
        );
        assert_ne!(treasure(&patch_treasure), treasure(&original));
        assert_ne!(
            original,
            make(LairStyle::default(), 100, 400),
            "the original reads it"
        );
    }
}
