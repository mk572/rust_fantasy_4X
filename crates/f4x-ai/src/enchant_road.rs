//! Enchant Road (task 642): the AI casts it to make its armies and settlers fast along the roads
//! between the places that matter. The rule (what is joined to what, in which order, and where
//! the cast square lies) is in `docs/ai-design.md` section 16. The roads are the ones the
//! engineers have built (`road.rs`); this only enchants what is there.
//!
//! Tests: at the end of this file, on the shipped rules.

use std::collections::{HashMap, HashSet, VecDeque};

use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Plane, Position};
use f4x_core::player::view::PlayerView;
use f4x_core::rules::exec::Target;
use f4x_def::rules::SpellId;

use crate::view::{is_coastal, BORDER_RANGE};

/// Citizens from which a city is large.
const LARGE_CITY: u32 = 8;

const NEIGHBOURS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// What a place is, for the order of the joining.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Harbour,
    Large,
    Border,
    Tower,
}

/// The sort key of a place: another continent last, then the rank, the larger first, the nearer
/// to its root, the square.
type Order = (bool, Rank, std::cmp::Reverse<u32>, u32, u32, u32);

/// The squares of the land the position stands on, as far as it is explored (the continent).
fn continent(view: &PlayerView, from: Position) -> HashSet<Position> {
    let land = |p: Position| {
        view.terrain_class(p)
            .is_some_and(|c| c != TerrainClass::Ocean)
    };
    let mut seen: HashSet<Position> = HashSet::new();
    let mut queue = VecDeque::new();
    if land(from) {
        seen.insert(from);
        queue.push_back(from);
    }
    while let Some(p) = queue.pop_front() {
        for (dx, dy) in NEIGHBOURS {
            if let Some(n) = view.shift(p, dx, dy) {
                if !seen.contains(&n) && land(n) {
                    seen.insert(n);
                    queue.push_back(n);
                }
            }
        }
    }
    seen
}

/// The squares of enchanted road joined to `root` (itself included) by enchanted road.
fn network(view: &PlayerView, root: Position) -> HashSet<Position> {
    let mut seen: HashSet<Position> = HashSet::new();
    let mut queue = VecDeque::new();
    seen.insert(root);
    queue.push_back(root);
    while let Some(p) = queue.pop_front() {
        for (dx, dy) in NEIGHBOURS {
            if let Some(n) = view.shift(p, dx, dy) {
                if !seen.contains(&n) && view.has_enchanted_road(n) {
                    seen.insert(n);
                    queue.push_back(n);
                }
            }
        }
    }
    seen
}

/// The shortest way over the roads from a square of `start` to `to`: the squares in order, from
/// the one in `start` (and `to` itself), `None` when no road joins them. A place stands on a road
/// of its own.
fn road_between(
    view: &PlayerView,
    start: &HashSet<Position>,
    to: Position,
    places: &HashSet<Position>,
) -> Option<Vec<Position>> {
    let on_road = |p: Position| view.has_road(p) || places.contains(&p);
    let mut from: HashMap<Position, Position> = HashMap::new();
    let mut queue: VecDeque<Position> = VecDeque::new();
    let mut origins: Vec<Position> = start.iter().copied().collect();
    origins.sort_by_key(|p| (p.plane.index(), p.y, p.x));
    for p in origins {
        from.insert(p, p);
        queue.push_back(p);
    }
    while let Some(p) = queue.pop_front() {
        if p == to {
            let mut path = vec![p];
            let mut at = p;
            while from[&at] != at {
                at = from[&at];
                path.push(at);
            }
            path.reverse();
            return Some(path);
        }
        for (dx, dy) in NEIGHBOURS {
            if let Some(n) = view.shift(p, dx, dy) {
                if !from.contains_key(&n) && on_road(n) {
                    from.insert(n, p);
                    queue.push_back(n);
                }
            }
        }
    }
    None
}

/// The places to join, with the root of the network each belongs to, in the order of the joining.
fn places(view: &PlayerView, fortress: Position) -> Vec<(Position, Position)> {
    let own: Vec<(hecs::Entity, Position)> = view
        .my_cities()
        .into_iter()
        .filter(|&(_, p)| p.plane == Plane::Arcanus)
        .collect();
    let size = |c: hecs::Entity| view.city_economy(c).map_or(0, |e| e.figures);
    let rivals: Vec<Position> = view
        .known_cities()
        .into_iter()
        .filter(|&(_, owner, _)| view.is_rival_wizard(owner))
        .map(|(_, _, p)| p)
        .collect();
    let mut ranked: Vec<(Rank, u32, Position)> = Vec::new();
    let home = continent(view, fortress);
    let inland = !is_coastal(view, fortress);
    for &(c, p) in &own {
        if p == fortress {
            continue;
        }
        let rank = if inland && is_coastal(view, p) && home.contains(&p) {
            Rank::Harbour
        } else if size(c) >= LARGE_CITY {
            Rank::Large
        } else if rivals
            .iter()
            .any(|&r| view.distance(p, r).is_some_and(|d| d <= BORDER_RANGE))
        {
            Rank::Border
        } else {
            continue;
        };
        ranked.push((rank, size(c), p));
    }
    for (p, intact) in view.towers() {
        if !intact && p.plane == Plane::Arcanus {
            ranked.push((Rank::Tower, 0, p));
        }
    }
    // The fortress's own continent first; within it the order of the ranks, the larger first,
    // the nearer to the fortress next.
    let mut roots: HashMap<Position, Position> = HashMap::new();
    let mut out: Vec<(Order, Position, Position)> = Vec::new();
    for (rank, citizens, p) in ranked {
        let root = if home.contains(&p) {
            fortress
        } else {
            // Another continent: the network is around its largest city.
            *roots.entry(p).or_insert_with(|| {
                let land = continent(view, p);
                own.iter()
                    .filter(|&&(_, q)| land.contains(&q))
                    .max_by_key(|&&(c, q)| (size(c), std::cmp::Reverse((q.y, q.x))))
                    .map_or(p, |&(_, q)| q)
            })
        };
        if root == p {
            continue;
        }
        let near = view.distance(p, root).unwrap_or(u32::MAX);
        out.push((
            (
                root != fortress,
                rank,
                std::cmp::Reverse(citizens),
                near,
                p.y,
                p.x,
            ),
            p,
            root,
        ));
    }
    out.sort_by_key(|&(key, _, _)| key);
    out.into_iter().map(|(_, p, root)| (p, root)).collect()
}

/// The square to cast Enchant Road on: on the road between the first place that is not yet
/// joined to its network and that network, `None` when every place is joined or has no road to
/// the network (the engineers build it first).
pub(crate) fn target(view: &PlayerView, spell: SpellId) -> Option<Target> {
    let fortress = view.fortress().filter(|f| f.plane == Plane::Arcanus)?;
    let list = places(view, fortress);
    let mut seen_places: HashSet<Position> = list.iter().map(|&(p, _)| p).collect();
    seen_places.insert(fortress);
    for &(_, root) in &list {
        seen_places.insert(root);
    }
    let mut nets: HashMap<Position, HashSet<Position>> = HashMap::new();
    for (place, root) in list {
        let net = nets.entry(root).or_insert_with(|| network(view, root));
        if net.contains(&place) {
            continue;
        }
        let Some(path) = road_between(view, net, place, &seen_places) else {
            continue;
        };
        // The first square of the road that is not enchanted; the cast, two squares on from it,
        // enchants five.
        let Some(first) = path.iter().position(|&p| !view.has_enchanted_road(p)) else {
            continue;
        };
        let at = path[(first + 2).min(path.len() - 1)];
        let tile = Target::Tile(at);
        if view.can_aim(spell, Some(tile)) {
            return Some(tile);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn_tower, COAST_CITY, INLAND_CITY, P0};
    use f4x_core::world::{Command, World};
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn spell(w: &World) -> Result<SpellId, Box<dyn std::error::Error>> {
        Ok(w.content()
            .rules
            .spells
            .id_numbered(55 /* enchant_road */)
            .ok_or("no enchant road")?)
    }

    /// A road along row 6 from the inland city to the coast city, as the engineers would leave it.
    fn with_road(w: &mut Game) {
        for x in INLAND_CITY.0..=COAST_CITY.0 {
            w.scenario_set_road(at(x, 6), true);
        }
    }

    /// The wizard's fortress inland at (6, 6), its other city on the coast at (10, 6) and a road
    /// between them along row 6, as the engineers would leave it.
    fn world() -> Result<Game, Box<dyn std::error::Error>> {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let events = w.apply(Command::PlaceFortress {
            player: P0,
            pos: at(INLAND_CITY.0, INLAND_CITY.1),
            name: "Capital".into(),
            race: w.content().rules.races.id_numbered(8),
            population: 4,
        });
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, f4x_core::world::Event::Rejected(_))),
            "{events:?}"
        );
        with_road(&mut w);
        Ok(w)
    }

    #[test]
    fn an_inland_fortress_is_joined_to_the_harbour_by_casts_along_the_road() -> TestResult {
        let mut w = world()?;
        let spell = spell(&w)?;
        let fortress = PlayerView::new(&w, P0).fortress().ok_or("a fortress")?;
        assert_eq!(fortress, at(INLAND_CITY.0, INLAND_CITY.1));
        let mut casts = 0;
        loop {
            let view = PlayerView::new(&w, P0);
            let Some(Target::Tile(t)) = target(&view, spell) else {
                break;
            };
            drop(view);
            assert!(casts < 4, "the road is only five squares long");
            w.scenario_enchant_roads(t);
            casts += 1;
        }
        let view = PlayerView::new(&w, P0);
        for x in INLAND_CITY.0..=COAST_CITY.0 {
            assert!(view.has_enchanted_road(at(x, 6)), "square {x}");
        }
        assert!(casts >= 1);
        Ok(())
    }

    /// The fortress inland at (4, 4), the harbour on the coast at (10, 6) with a road along row
    /// 4 and down column 10, and a second city of the given size inland at (4, 9) with a road
    /// down column 4.
    fn wide_world(population: u32) -> Result<Game, Box<dyn std::error::Error>> {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        w.apply(Command::PlaceFortress {
            player: P0,
            pos: at(4, 4),
            name: "Capital".into(),
            race: w.content().rules.races.id_numbered(8),
            population: 4,
        });
        w.apply(Command::FoundCity {
            owner: P0,
            pos: at(4, 9),
            name: "South".into(),
            race: w.content().rules.races.id_numbered(8),
            population,
        });
        assert!(
            w.economy().city_at(at(4, 9)).is_some(),
            "the city is founded"
        );
        for x in 4..=10 {
            w.scenario_set_road(at(x, 4), true);
        }
        for y in [5, 6] {
            w.scenario_set_road(at(10, y), true);
        }
        for y in 5..=9 {
            w.scenario_set_road(at(4, y), true);
        }
        Ok(w)
    }

    fn next(w: &World) -> Result<Option<Position>, Box<dyn std::error::Error>> {
        let view = PlayerView::new(w, P0);
        Ok(match target(&view, spell(w)?) {
            Some(Target::Tile(t)) => Some(t),
            _ => None,
        })
    }

    #[test]
    fn the_harbour_is_joined_before_a_large_city_and_the_network_grows_from_the_fortress(
    ) -> TestResult {
        let mut w = wide_world(9)?;
        // Both roads are unenchanted: the harbour's comes first, two squares on from the fortress.
        assert_eq!(next(&w)?, Some(at(6, 4)));
        let mut casts = 0;
        let mut order = Vec::new();
        while let Some(t) = next(&w)? {
            order.push(t);
            w.scenario_enchant_roads(t);
            casts += 1;
            assert!(casts <= 5, "{order:?}");
        }
        let view = PlayerView::new(&w, P0);
        for x in 4..=10 {
            assert!(view.has_enchanted_road(at(x, 4)), "row 4 square {x}");
        }
        for y in 4..=9 {
            assert!(view.has_enchanted_road(at(4, y)), "column 4 square {y}");
        }
        // The harbour's road took two casts, the large city's one: the city was joined to the
        // network at the fortress, not by a road of its own all the way.
        assert_eq!(order.len(), 3, "{order:?}");
        assert_eq!(order[2].x, 4, "the last cast is on the large city's road");
        Ok(())
    }

    #[test]
    fn a_small_city_far_from_every_rival_is_not_joined() -> TestResult {
        let mut w = wide_world(3)?;
        while let Some(t) = next(&w)? {
            w.scenario_enchant_roads(t);
        }
        let view = PlayerView::new(&w, P0);
        assert!(view.has_enchanted_road(at(10, 6)), "the harbour is joined");
        assert!(
            !view.has_enchanted_road(at(4, 9)),
            "no rival near, not large: no road for it"
        );
        Ok(())
    }

    #[test]
    fn an_open_tower_is_joined_and_an_intact_one_is_not() -> TestResult {
        let mut w = world()?;
        w.scenario_enchant_roads(at(8, 6));
        spawn_tower(&mut w, at(3, 6), true).map_err(|e| e.to_string())?;
        for x in 3..=6 {
            w.scenario_set_road(at(x, 6), true);
        }
        assert_eq!(next(&w)?, None, "the tower still has its guards");
        let mut w = world()?;
        w.scenario_enchant_roads(at(8, 6));
        spawn_tower(&mut w, at(3, 6), false).map_err(|e| e.to_string())?;
        for x in 3..=6 {
            w.scenario_set_road(at(x, 6), true);
        }
        let t = next(&w)?.ok_or("an open tower is joined")?;
        w.scenario_enchant_roads(t);
        assert_eq!(next(&w)?, None);
        assert!(PlayerView::new(&w, P0).has_enchanted_road(at(3, 6)));
        Ok(())
    }

    #[test]
    fn a_place_already_in_the_network_is_not_joined_again() -> TestResult {
        let mut w = world()?;
        let spell = spell(&w)?;
        // The whole road is enchanted at once (cast in the middle of it).
        w.scenario_enchant_roads(at(8, 6));
        w.scenario_enchant_roads(at(6, 6));
        w.scenario_enchant_roads(at(10, 6));
        let view = PlayerView::new(&w, P0);
        assert_eq!(target(&view, spell), None);
        Ok(())
    }

    #[test]
    fn without_a_road_nothing_is_cast() -> TestResult {
        let mut w = game(INLAND_CITY, 8);
        w.reveal_map(P0);
        let spell = spell(&w)?;
        let view = PlayerView::new(&w, P0);
        assert_eq!(target(&view, spell), None);
        Ok(())
    }
}
