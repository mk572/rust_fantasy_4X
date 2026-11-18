//! Where the next cities go (tasks 664 and 665): one ranked list of the places on the explored map
//! where a city could be founded, and the assignment of the settlers to them.
//!
//! The list is built from the wizard's own view (what it has explored, the cities it knows) and
//! kept in [`SiteBook`]: it is rebuilt when a city is founded or lost and every few turns, not in
//! every settler's turn. A site is scored by the land its city would work (food and growth,
//! production, mana, special resources, rivers), less what the land shares with other cities, plus
//! nearness to the wizard's own cities, less nearness to a rival's (more at war), less the crossing
//! when it lies over the sea (a site on a plane the wizard has no city on is not offered). Of two sites too close to both be cities only
//! the better one is kept. A site that has been settled drops out when the list is rebuilt: there is
//! no record of failures.
//!
//! A site is out of the question for the moment only for a reason the view shows: an enemy stands
//! on it or next to it where the wizard can see (`enemy_near`).
//!
//! The weights are the AI's own tuning, named below; the numbers are not game rules.
//!
//! Tests: in this file, on the small test world of `testworld.rs`.

use std::collections::{HashSet, VecDeque};

use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Plane, Position};
use f4x_core::player::view::PlayerView;

/// Sites keep this far apart from each other and from cities.
pub(crate) const SPACING: u32 = 4;
/// The list is rebuilt after this many turns, at once when the number of known cities changes, and
/// when the wizard has explored this many squares more than at the last rebuild (a scout opens a
/// new place for a city within a few turns early in the game).
const REFRESH_TURNS: u32 = 5;
const REFRESH_SQUARES: usize = 3;

/// The most squares of sea and of one landmass a search goes through.
const SEA_LIMIT: usize = 3000;
const LAND_LIMIT: usize = 1500;

/// How a settler of the wizard gets to a site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Reach {
    /// By land: a landmass one of the wizard's cities stands on.
    Land,
    /// Over the sea: another landmass of the plane, that a ship from one of the wizard's coastal
    /// cities can reach over explored water.
    Sea,
}

/// A place for a city and what it is worth.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Site {
    pub at: Position,
    pub score: f32,
    pub reach: Reach,
}

/// The ranked list of sites, with when it was made.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SiteBook {
    built: Option<u32>,
    cities: usize,
    explored: usize,
    list: Vec<Site>,
}

/// How many squares of the map the wizard has explored.
fn explored_squares(view: &PlayerView) -> usize {
    let Some((width, height)) = view.map_size() else {
        return 0;
    };
    [Plane::Arcanus, Plane::Myrror]
        .into_iter()
        .flat_map(|plane| {
            (0..height).flat_map(move |y| (0..width).map(move |x| Position { plane, x, y }))
        })
        .filter(|&p| view.explored(p))
        .count()
}

impl SiteBook {
    /// The list, best first, rebuilt when it is old or a city has been founded, lost or found.
    pub fn refreshed(&mut self, view: &PlayerView) -> &[Site] {
        let cities = view.my_cities().len() + view.known_cities().len();
        let explored = explored_squares(view);
        let stale = match self.built {
            None => true,
            Some(turn) => {
                view.turn().saturating_sub(turn) >= REFRESH_TURNS
                    || cities != self.cities
                    || explored >= self.explored + REFRESH_SQUARES
            }
        };
        if stale {
            self.list = scan(view);
            self.built = Some(view.turn());
            self.cities = cities;
            self.explored = explored;
        }
        &self.list
    }
}

/// How many sites a wizard has work for, as the number of settlers it can use: the sites that no
/// settler has taken yet, with the ones that need a crossing counting for a few.
pub fn free_sites(list: &[Site], taken: &[Position], view: &PlayerView) -> usize {
    let free = |s: &&Site| {
        !taken
            .iter()
            .any(|&t| view.distance(t, s.at).is_some_and(|d| d < SPACING))
    };
    let land = list
        .iter()
        .filter(free)
        .filter(|s| s.reach == Reach::Land)
        .count();
    let over = list
        .iter()
        .filter(free)
        .filter(|s| s.reach != Reach::Land)
        .count();
    land + over.min(view.content().rules.ai_sites.crossing_sites_counted)
}

/// Is there a place for a city on the explored map?
#[cfg(test)]
pub fn has_sites(view: &PlayerView) -> bool {
    !scan(view).is_empty()
}

/// The explored land connected to the squares `starts` (eight directions), at most `LAND_LIMIT`
/// squares.
pub(crate) fn landmass(view: &PlayerView, starts: &[Position]) -> HashSet<Position> {
    let mut seen: HashSet<Position> = starts.iter().copied().collect();
    let mut queue: VecDeque<Position> = starts.iter().copied().collect();
    while let Some(p) = queue.pop_front() {
        if seen.len() > LAND_LIMIT {
            break;
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                let Some(q) = view.shift(p, dx, dy) else {
                    continue;
                };
                let land = matches!(view.terrain_class(q), Some(c) if c != TerrainClass::Ocean);
                if land && seen.insert(q) {
                    queue.push_back(q);
                }
            }
        }
    }
    seen
}

/// The land squares beside the explored water that a ship from one of the wizard's coastal cities
/// can sail over (enemies in sight stop it), at most `SEA_LIMIT` squares of water.
fn shores_by_sea(view: &PlayerView, own: &[Position]) -> HashSet<Position> {
    let mut sea: HashSet<Position> = HashSet::new();
    let mut queue: VecDeque<Position> = VecDeque::new();
    for &city in own {
        for dy in -1..=1 {
            for dx in -1..=1 {
                if let Some(p) = view.shift(city, dx, dy) {
                    if view.terrain_class(p) == Some(TerrainClass::Ocean) && sea.insert(p) {
                        queue.push_back(p);
                    }
                }
            }
        }
    }
    let mut shores: HashSet<Position> = HashSet::new();
    while let Some(water) = queue.pop_front() {
        if sea.len() > SEA_LIMIT {
            break;
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                let Some(next) = view.shift(water, dx, dy) else {
                    continue;
                };
                match view.terrain_class(next) {
                    Some(TerrainClass::Ocean) => {
                        if view.hostile_at(next).is_none() && sea.insert(next) {
                            queue.push_back(next);
                        }
                    }
                    Some(_) => {
                        shores.insert(next);
                    }
                    None => {}
                }
            }
        }
    }
    shores
}

/// Every place where a city could be founded on what the wizard has explored, best first, with
/// only the better of two sites that are too close to each other.
pub fn scan(view: &PlayerView) -> Vec<Site> {
    let Some((width, height)) = view.map_size() else {
        return Vec::new();
    };
    let own: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
    let known = view.known_cities();
    let at_war = view.rivals_at_war();
    let rivals: Vec<(Position, bool)> = known
        .iter()
        .filter(|&&(_, owner, _)| view.is_rival_wizard(owner))
        .map(|&(_, owner, p)| (p, at_war.contains(&owner)))
        .collect();
    let cities: Vec<Position> = own
        .iter()
        .copied()
        .chain(known.iter().map(|&(_, _, p)| p))
        .collect();
    let home = landmass(view, &own);
    let shores = shores_by_sea(view, &own);
    // Landmass already judged from the sea: every square of it, and whether a ship can land on it.
    let mut by_sea: std::collections::HashMap<Position, bool> = std::collections::HashMap::new();
    let mut found: Vec<Site> = Vec::new();
    for plane in [Plane::Arcanus, Plane::Myrror] {
        // Not the first or last row: nothing can be founded there.
        for y in 1..height.saturating_sub(1) {
            for x in 0..width {
                let at = Position { plane, x, y };
                if !view.can_found_city(at)
                    || view.has_lair(at)
                    || view.in_node(at)
                    || cities
                        .iter()
                        .any(|&c| view.distance(c, at).is_some_and(|d| d < SPACING))
                {
                    continue;
                }
                let reach = if home.contains(&at) {
                    Reach::Land
                } else {
                    let landing = match by_sea.get(&at) {
                        Some(&ok) => ok,
                        None => {
                            let mass = landmass(view, &[at]);
                            let ok = mass.iter().any(|p| shores.contains(p));
                            for p in mass {
                                by_sea.insert(p, ok);
                            }
                            ok
                        }
                    };
                    if !landing {
                        continue;
                    }
                    Reach::Sea
                };
                found.push(Site {
                    at,
                    score: score(view, at, reach, &own, &rivals, &cities),
                    reach,
                });
            }
        }
    }
    found.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then((a.at.plane as u8, a.at.y, a.at.x).cmp(&(b.at.plane as u8, b.at.y, b.at.x)))
    });
    let mut kept: Vec<Site> = Vec::new();
    for site in found {
        if kept
            .iter()
            .all(|k| view.distance(k.at, site.at).is_none_or(|d| d >= SPACING))
        {
            kept.push(site);
        }
    }
    kept
}

/// What a city at `at` would be worth.
fn score(
    view: &PlayerView,
    at: Position,
    reach: Reach,
    own: &[Position],
    rivals: &[(Position, bool)],
    cities: &[Position],
) -> f32 {
    let w = &view.content().rules.ai_sites;
    // The land: the squares the city works, those another city works too at a reduced value.
    let mut land = 0.0;
    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            if dx.abs() == 2 && dy.abs() == 2 {
                continue;
            }
            let Some(p) = view.shift(at, dx, dy) else {
                continue;
            };
            let value = w.land_value(view.terrain_class(p));
            let shared = cities
                .iter()
                .any(|&c| view.distance(c, p).is_some_and(|d| d <= 2));
            land += if shared {
                value * w.shared_share
            } else {
                value
            };
        }
    }
    let special = view.site_specials(at);
    let mut score = land
        + w.production * special.terrain_production
        + w.mana * special.mana
        + w.gold * special.gold
        + w.food * special.food
        + w.unit_cost * special.unit_cost_percent
        + w.river * special.rivers as f32;
    if let Some(d) = own.iter().filter_map(|&c| view.distance(c, at)).min() {
        score += (w.near_own - d as f32 * w.near_own_step).max(0.0);
    }
    for &(city, war) in rivals {
        if let Some(d) = view.distance(city, at).filter(|&d| d < w.enemy_range) {
            score -= (w.enemy_range - d) as f32 * if war { w.enemy_war } else { w.enemy_peace };
        }
    }
    score
        - match reach {
            Reach::Land => 0.0,
            Reach::Sea => w.sea_cost,
        }
}

/// Does the wizard see an enemy on the site or next to it? Only what it sees counts: the block
/// ends when the square is seen again without the enemy.
pub fn enemy_near(view: &PlayerView, at: Position) -> bool {
    view.sees(at) && !view.enemies_near(at, 1).is_empty()
}

/// A settler without a site: where it stands and the squares it can walk to.
pub struct Free {
    pub unit: hecs::Entity,
    pub at: Position,
    pub reach: f4x_core::fast::FastSet<Position>,
}

/// The sites for the free settlers: the best site goes to the settler nearest to it (by distance,
/// one that can walk there before one that needs a ship), then the next site to the nearest of those
/// left, down the list. `taken` are the sites of settlers that already have one, `avoid` the sites to
/// leave out for now. Each result says whether the settler needs a ship for the site (a site over the
/// sea that it cannot walk to, and only a walker needs a ship: `PlayerView::is_walker`): the settler is then tagged `Crossing::Sea` and the general rule for
/// units waiting for a ship builds one (`economy.rs`, `pickup.rs`).
pub fn assign(
    view: &PlayerView,
    list: &[Site],
    free: &[Free],
    taken: &[Position],
    avoid: &[Position],
) -> Vec<(hecs::Entity, Position, bool)> {
    let mut left: Vec<&Free> = free.iter().collect();
    let mut used: Vec<Position> = taken.to_vec();
    let mut out = Vec::new();
    for site in list {
        if left.is_empty() {
            break;
        }
        if used
            .iter()
            .chain(avoid)
            .any(|&t| view.distance(t, site.at).is_some_and(|d| d < SPACING))
            || !view.can_found_city(site.at)
            || enemy_near(view, site.at)
        {
            continue;
        }
        let walks = |f: &Free| f.at == site.at || f.reach.contains(&site.at);
        let cost = |f: &Free| (!walks(f), view.distance(f.at, site.at).unwrap_or(u32::MAX));
        // A settler that cannot walk to the site takes it only over the sea: a land site that this
        // settler cannot walk to (it stands on land the site is cut off from) is for another settler.
        let Some(best) = (0..left.len())
            // Only a walker waits for a ship; a swimmer or a flier (by nature, an item or a
            // spell) crosses the water itself or cannot reach the site.
            .filter(|&i| {
                walks(left[i]) || (site.reach == Reach::Sea && view.is_walker(left[i].unit))
            })
            .min_by_key(|&i| cost(left[i]))
        else {
            continue;
        };
        let settler = left.remove(best);
        used.push(site.at);
        out.push((
            settler.unit,
            site.at,
            site.reach == Reach::Sea && !walks(settler),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, game_with_rivers, spawn, COAST_CITY, ENEMY_CITY, P0, P1};
    use f4x_def::half_int::HalfInt;

    /// The west island, explored by its wizard.
    fn explored_island() -> f4x_core::game::Game {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.explore(P0, at(7, 6), HalfInt::from_int(4));
        g
    }

    #[test]
    fn the_list_is_ranked_and_keeps_every_site_apart_from_the_others_and_from_cities() {
        let g = explored_island();
        let view = PlayerView::new(&g, P0);
        let list = scan(&view);
        assert!(!list.is_empty(), "the explored island has room");
        assert!(
            list.windows(2).all(|w| w[0].score >= w[1].score),
            "best first: {list:?}"
        );
        let capital = at(COAST_CITY.0, COAST_CITY.1);
        for (i, a) in list.iter().enumerate() {
            assert!(view.distance(capital, a.at).is_some_and(|d| d >= SPACING));
            for b in &list[i + 1..] {
                assert!(
                    view.distance(a.at, b.at).is_none_or(|d| d >= SPACING),
                    "{a:?} and {b:?} overlap"
                );
            }
        }
        assert!(has_sites(&view));
    }

    #[test]
    fn sites_on_the_home_island_are_reached_by_land() {
        let g = explored_island();
        let list = scan(&PlayerView::new(&g, P0));
        assert!(list.iter().all(|s| s.reach == Reach::Land), "{list:?}");
    }

    #[test]
    fn sites_over_the_sea_rank_below_every_site_on_the_home_island() {
        let mut g = explored_island();
        // Explore the water to the east island, and the island: sites there are over the sea.
        for x in [16, 22, 26, 30] {
            g.explore(P0, at(x, 6), HalfInt::from_int(3));
        }
        g.explore(P0, at(31, 6), HalfInt::from_int(6));
        let view = PlayerView::new(&g, P0);
        let list = scan(&view);
        let over: Vec<&Site> = list.iter().filter(|s| s.reach == Reach::Sea).collect();
        let home: Vec<&Site> = list.iter().filter(|s| s.reach == Reach::Land).collect();
        assert!(!over.is_empty() && !home.is_empty(), "{list:?}");
        let best_home = home[0].score;
        assert!(
            over.iter().all(|s| s.score < best_home),
            "the home land comes first: {list:?}"
        );
    }

    /// A settler cut off from a land site (the walk finds no way) is not sent there: the site is
    /// left for a settler that can walk, instead of an order that fails every turn.
    #[test]
    fn a_land_site_is_not_given_to_a_settler_that_cannot_walk_to_it() {
        let mut g = explored_island();
        let units = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            111, /* high_men_settlers */
            1,
        );
        let view = PlayerView::new(&g, P0);
        let list = scan(&view);
        assert!(list.iter().any(|s| s.reach == Reach::Land), "{list:?}");
        let land: Vec<Site> = list
            .into_iter()
            .filter(|s| s.reach == Reach::Land)
            .collect();
        let cut_off = vec![Free {
            unit: units[0],
            at: at(COAST_CITY.0, COAST_CITY.1),
            reach: f4x_core::fast::FastSet::default(),
        }];
        assert!(assign(&view, &land, &cut_off, &[], &[]).is_empty());
    }

    #[test]
    fn the_nearest_settler_gets_the_best_site_and_the_next_one_the_next_site() {
        let g = explored_island();
        let view = PlayerView::new(&g, P0);
        let list = scan(&view);
        assert!(list.len() >= 2, "{list:?}");
        // Two settlers: one at the best site, one far from it (at the other end of the island).
        let best = list[0].at;
        let far = list
            .iter()
            .map(|s| s.at)
            .max_by_key(|&p| view.distance(best, p).unwrap_or(0))
            .filter(|&p| p != best);
        let Some(far) = far else {
            panic!("a second site: {list:?}");
        };
        let mut g = explored_island();
        let units = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            111, /* high_men_settlers */
            2,
        );
        let view = PlayerView::new(&g, P0);
        let reach_all: f4x_core::fast::FastSet<Position> = list.iter().map(|s| s.at).collect();
        let free = vec![
            Free {
                unit: units[0],
                at: far,
                reach: reach_all.clone(),
            },
            Free {
                unit: units[1],
                at: best,
                reach: reach_all,
            },
        ];
        let got = assign(&view, &list, &free, &[], &[]);
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(
            got[0],
            (units[1], best, false),
            "the settler at the best site takes it"
        );
        assert_eq!(got[1].0, units[0]);
        assert_ne!(got[1].1, best);
    }

    #[test]
    fn a_site_a_settler_already_holds_is_not_given_again() {
        let mut g = explored_island();
        let units = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            111, /* high_men_settlers */
            1,
        );
        let view = PlayerView::new(&g, P0);
        let list = scan(&view);
        let free = vec![Free {
            unit: units[0],
            at: at(COAST_CITY.0, COAST_CITY.1),
            reach: list.iter().map(|s| s.at).collect(),
        }];
        let got = assign(&view, &list, &free, &[list[0].at], &[]);
        assert_eq!(got.len(), 1);
        assert!(view
            .distance(got[0].1, list[0].at)
            .is_some_and(|d| d >= SPACING));
    }

    #[test]
    fn an_enemy_on_the_site_that_the_wizard_sees_blocks_it_until_the_enemy_is_gone() {
        let mut g = explored_island();
        let best = scan(&PlayerView::new(&g, P0))[0].at;
        // A rival's soldier on the best site, seen by the wizard (the squares are in sight of a unit).
        let _watch = spawn(&mut g, P0, best, 111 /* high_men_settlers */, 1);
        let enemy = spawn(&mut g, P1, best, 111 /* high_men_settlers */, 1);
        g.declare_war(P0, P1);
        let view = PlayerView::new(&g, P0);
        assert!(
            enemy_near(&view, best),
            "the enemy beside the wizard's own unit is seen"
        );
        g.cheat_remove_unit(enemy[0]);
        assert!(
            !enemy_near(&PlayerView::new(&g, P0), best),
            "gone, so the site is open again"
        );
    }

    #[test]
    fn a_full_island_offers_only_sites_over_the_sea_and_a_plane_without_a_city_none() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.explore(P0, at(COAST_CITY.0, COAST_CITY.1), HalfInt::from_int(12));
        for x in [16, 22, 26, 30] {
            g.explore(P0, at(x, 6), HalfInt::from_int(3));
        }
        g.explore(P0, at(ENEMY_CITY.0 + 4, ENEMY_CITY.1), HalfInt::from_int(3));
        crate::testworld::fill_west(&mut g);
        let list = scan(&PlayerView::new(&g, P0));
        assert!(!list.is_empty(), "the east island has room");
        assert!(
            list.iter().all(|s| s.reach == Reach::Sea && s.at.x >= 28),
            "{list:?}"
        );
        assert!(list.iter().all(|s| s.at.plane == Plane::Arcanus));
    }

    /// A site over the sea is for a settler that needs a ship (a walker) or one that crosses the
    /// water itself; a settler that swims (a lizardman) or has flight from a spell does not wait
    /// for a ship, and one that is cut off for another reason takes no such site.
    #[test]
    fn only_a_walking_settler_is_given_a_site_over_the_sea_by_ship() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.explore(P0, at(COAST_CITY.0, COAST_CITY.1), HalfInt::from_int(12));
        for x in [16, 22, 26, 30] {
            g.explore(P0, at(x, 6), HalfInt::from_int(3));
        }
        g.explore(P0, at(ENEMY_CITY.0 + 4, ENEMY_CITY.1), HalfInt::from_int(3));
        crate::testworld::fill_west(&mut g);
        let walker = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            111, /* high_men_settlers */
            1,
        )[0];
        let swimmer = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            125, /* lizardman_settlers */
            1,
        )[0];
        let flier = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            111, /* high_men_settlers */
            1,
        )[0];
        g.scenario_enchant_unit(flier, f4x_def::rules::SpellId::new(56 /* flight */), P0);
        let view = PlayerView::new(&g, P0);
        let list = scan(&view);
        assert!(list.iter().all(|s| s.reach == Reach::Sea), "{list:?}");
        let free = |u| {
            vec![Free {
                unit: u,
                at: at(COAST_CITY.0, COAST_CITY.1),
                reach: f4x_core::fast::FastSet::default(),
            }]
        };
        let got = assign(&view, &list, &free(walker), &[], &[]);
        assert_eq!(
            got.first().map(|&(u, _, ship)| (u, ship)),
            Some((walker, true))
        );
        assert!(!view.is_walker(swimmer) && !view.is_walker(flier));
        assert!(assign(&view, &list, &free(swimmer), &[], &[]).is_empty());
        assert!(assign(&view, &list, &free(flier), &[], &[]).is_empty());
    }

    #[test]
    fn land_not_seen_from_any_water_the_wizard_has_explored_is_no_site() {
        // Nothing but the west island explored: the east island has no site on the list.
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.explore(P0, at(7, 6), HalfInt::from_int(4));
        crate::testworld::fill_west(&mut g);
        let list = scan(&PlayerView::new(&g, P0));
        assert!(list.is_empty(), "{list:?}");
    }

    /// The weights the tests read: the shipped ones.
    fn weights(g: &f4x_core::game::Game) -> f4x_def::rules::ai_site::SiteRules {
        g.content().rules.ai_sites.clone()
    }

    const SITE: (u32, u32) = (6, 6);

    #[test]
    fn a_river_on_the_squares_of_a_site_raises_it_by_the_river_weight() {
        let plain = explored_island();
        let mut wet = game_with_rivers(COAST_CITY, 8 /* high_men */, &[SITE]);
        wet.explore(P0, at(7, 6), HalfInt::from_int(4));
        let (a, b) = (PlayerView::new(&plain, P0), PlayerView::new(&wet, P0));
        let here = at(SITE.0, SITE.1);
        let gain = score(&b, here, Reach::Land, &[], &[], &[])
            - score(&a, here, Reach::Land, &[], &[], &[]);
        assert!((gain - weights(&plain).river).abs() < 1e-4, "{gain}");
    }

    #[test]
    fn a_site_next_to_an_own_city_beats_one_further_out() {
        let g = explored_island();
        let (view, w) = (PlayerView::new(&g, P0), weights(&g));
        let here = at(SITE.0, SITE.1);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let alone = score(&view, here, Reach::Land, &[], &[], &[]);
        let near = score(&view, here, Reach::Land, &[city], &[], &[]);
        let d = view.distance(city, here).expect("one plane") as f32;
        assert!(near > alone, "{near} {alone}");
        assert!((near - alone - (w.near_own - d * w.near_own_step).max(0.0)).abs() < 1e-4);
        // One square further from the city gives a step less.
        let farther = at(SITE.0 - 1, SITE.1);
        let gain = |p: Position| {
            score(&view, p, Reach::Land, &[city], &[], &[])
                - score(&view, p, Reach::Land, &[], &[], &[])
        };
        assert!(gain(here) > gain(farther));
    }

    #[test]
    fn squares_another_city_works_too_count_for_their_share() {
        let g = explored_island();
        let (view, w) = (PlayerView::new(&g, P0), weights(&g));
        let here = at(SITE.0, SITE.1);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let free = score(&view, here, Reach::Land, &[], &[], &[]);
        let shared = score(&view, here, Reach::Land, &[], &[], &[city]);
        assert!(shared < free, "{shared} {free}");
        // The squares within two of the city (here the one square the two areas have in common
        // at most) lose the rest of their value: never more than the land's whole value.
        assert!(free - shared <= free * (1.0 - w.shared_share) + 1e-4);
    }

    #[test]
    fn a_site_over_the_sea_costs_the_sea_weight() {
        let g = explored_island();
        let (view, w) = (PlayerView::new(&g, P0), weights(&g));
        let here = at(SITE.0, SITE.1);
        let land = score(&view, here, Reach::Land, &[], &[], &[]);
        let sea = score(&view, here, Reach::Sea, &[], &[], &[]);
        assert!((land - sea - w.sea_cost).abs() < 1e-4, "{land} {sea}");
    }

    #[test]
    fn a_rivals_city_lowers_a_site_and_one_at_war_lowers_it_more() {
        let g = explored_island();
        let (view, w) = (PlayerView::new(&g, P0), weights(&g));
        let here = at(SITE.0, SITE.1);
        let rival = at(SITE.0 + 2, SITE.1);
        let none = score(&view, here, Reach::Land, &[], &[], &[]);
        let peace = score(&view, here, Reach::Land, &[], &[(rival, false)], &[]);
        let war = score(&view, here, Reach::Land, &[], &[(rival, true)], &[]);
        let nearer = (w.enemy_range - 2) as f32;
        assert!((none - peace - nearer * w.enemy_peace).abs() < 1e-4);
        assert!((none - war - nearer * w.enemy_war).abs() < 1e-4);
        assert!(war < peace);
        // Out of range, nothing.
        let far = at(SITE.0 + w.enemy_range, SITE.1);
        assert_eq!(
            score(&view, here, Reach::Land, &[], &[(far, true)], &[]),
            none
        );
    }

    #[test]
    fn the_land_values_follow_the_terrain_of_the_rules() {
        let g = explored_island();
        let w = weights(&g);
        for (class, value) in &w.land_values {
            assert_eq!(w.land_value(Some(*class)), *value);
        }
        assert_eq!(w.land_value(None), 0.0);
        // Grass beats desert: a city wants food.
        assert!(
            w.land_value(Some(TerrainClass::Grassland)) > w.land_value(Some(TerrainClass::Desert))
        );
    }
}
