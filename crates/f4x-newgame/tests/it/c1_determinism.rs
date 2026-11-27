//! Class 1 (`docs/f4x-newgame-test-design.md`): the same inputs give the same world, whatever else
//! the process has done, in whatever order and on whatever thread.

use crate::support::*;

/// The whole new game as text: how many numbers the setup drew, then the world as a save writes
/// it. Equal text is an equal game.
fn text(case: WorldCase) -> String {
    let game = fresh_game(case);
    format!("draws {}\n{}", game.draws(), save(&game.world))
}

/// The world alone, which does not name the seed.
fn world_text(case: WorldCase) -> String {
    save(&fresh_game(case).world)
}

fn save(world: &f4x_core::world::World) -> String {
    world.snapshot().to_text().expect("the game saves")
}

/// A cut of the worlds: the standard one, each variant, the corner sizes, both kinds of game.
fn cut() -> Vec<WorldCase> {
    let std = standard();
    let mut v = vec![
        std,
        WorldCase {
            all_ai: false,
            ..std
        },
    ];
    for &variant in VARIANTS {
        v.push(WorldCase {
            variant,
            seed: 21,
            ..std
        });
    }
    for (width, height) in sizes() {
        v.push(WorldCase {
            width,
            height,
            seed: 22,
            ..std
        });
    }
    v.push(WorldCase {
        seed: u64::MAX,
        ..std
    });
    v.sort_by_key(|case| format!("{case:?}"));
    v.dedup();
    v
}

#[test]
fn the_same_call_makes_the_same_game_every_time() {
    for case in cut() {
        assert_eq!(text(case), text(case), "{case:?}");
    }
}

#[test]
fn other_seeds_make_other_worlds() {
    // Without this the test above could pass on a generator that ignores its seed.
    let std = standard();
    let a = world_text(WorldCase { seed: 1, ..std });
    for seed in [2, 3, u64::MAX] {
        assert_ne!(a, world_text(WorldCase { seed, ..std }), "seed {seed}");
    }
}

#[test]
fn games_made_in_another_order_are_the_same_games() {
    let worlds = cut();
    let forward: Vec<String> = worlds.iter().map(|&case| text(case)).collect();
    let backward: Vec<String> = worlds.iter().rev().map(|&case| text(case)).collect();
    let backward: Vec<String> = backward.into_iter().rev().collect();
    assert_eq!(forward, backward);
}

#[test]
fn games_made_at_the_same_time_on_other_threads_are_the_same_games() {
    let worlds = cut();
    let alone: Vec<String> = worlds.iter().map(|&case| text(case)).collect();
    let handles: Vec<_> = worlds
        .iter()
        .map(|&case| std::thread::spawn(move || text(case)))
        .collect();
    let together: Vec<String> = handles
        .into_iter()
        .map(|h| h.join().expect("a thread"))
        .collect();
    assert_eq!(alone, together);
}

#[test]
fn the_generator_alone_is_the_same_map_every_time() {
    for case in cut() {
        let a = generated(case);
        let b = f4x_newgame::mapgen::generate(case.seed, &params(case), content()).expect("a map");
        assert_eq!(*a, b, "{case:?}");
    }
}

#[test]
fn names_do_not_move_the_map_or_the_sites() {
    // The names are for the cities and the wizards; the world's squares come from the seed alone.
    let std = standard();
    let plain = fresh_game(std);
    let mut spec = std.spec();
    spec.names.rivals = vec![vec![("Ana".into(), Default::default())]; usize::from(most_wizards())];
    spec.names.cities.insert(0, vec!["A".into(), "B".into()]);
    let (named, _) = f4x_newgame::setup::new_game(content(), &spec).expect("a game");
    let (pm, nm) = (plain.map().expect("a map"), named.map().expect("a map"));
    for plane in f4x_core::map::Plane::all() {
        assert_eq!(
            pm.plane(plane).tiles(),
            nm.plane(plane).tiles(),
            "{plane:?}"
        );
    }
    let mut a = plain.economy().site_squares();
    let mut b = named.economy().site_squares();
    a.sort_by_key(|p| (p.plane.index(), p.y, p.x));
    b.sort_by_key(|p| (p.plane.index(), p.y, p.x));
    assert_eq!(a, b);
}

#[test]
fn the_number_of_wizards_does_not_move_the_towers_nodes_and_sites() {
    // The map is made before the wizards: asking for more of them must not change it.
    let std = standard();
    let sites = |wizards| {
        let w = fresh_game(WorldCase { wizards, ..std });
        let mut v = w.economy().site_squares();
        v.sort_by_key(|p| (p.plane.index(), p.y, p.x));
        v
    };
    let few = sites(2);
    assert!(!few.is_empty());
    assert_eq!(few, sites(5));
    assert_eq!(few, sites(most_wizards()));
}
