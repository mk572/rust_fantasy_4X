//! Class 1 (`docs/f4x-newgame-test-design.md`): the same inputs give the same world, whatever else
//! the process has done, in whatever order and on whatever thread.

use crate::support::*;

/// The whole game as text: a save. Equal text is an equal game.
fn text(world: World) -> String {
    fresh_game(world)
        .snapshot()
        .to_text()
        .expect("the game saves")
}

/// A cut of the worlds: the standard one, each variant, the corner sizes, both kinds of game.
fn cut() -> Vec<World> {
    let std = standard();
    let mut v = vec![
        std,
        World {
            all_ai: false,
            ..std
        },
    ];
    for &variant in VARIANTS {
        v.push(World {
            variant,
            seed: 21,
            ..std
        });
    }
    for (width, height) in sizes() {
        v.push(World {
            width,
            height,
            seed: 22,
            ..std
        });
    }
    v.push(World {
        seed: u64::MAX,
        ..std
    });
    v.sort_by_key(|w| format!("{w:?}"));
    v.dedup();
    v
}

#[test]
fn the_same_call_makes_the_same_game_every_time() {
    for world in cut() {
        assert_eq!(text(world), text(world), "{world:?}");
    }
}

#[test]
fn other_seeds_make_other_worlds() {
    // Without this the test above could pass on a generator that ignores its seed.
    let std = standard();
    let a = text(World { seed: 1, ..std });
    for seed in [2, 3, u64::MAX] {
        assert_ne!(a, text(World { seed, ..std }), "seed {seed}");
    }
}

#[test]
fn games_made_in_another_order_are_the_same_games() {
    let worlds = cut();
    let forward: Vec<String> = worlds.iter().map(|&w| text(w)).collect();
    let backward: Vec<String> = worlds.iter().rev().map(|&w| text(w)).collect();
    let backward: Vec<String> = backward.into_iter().rev().collect();
    assert_eq!(forward, backward);
}

#[test]
fn games_made_at_the_same_time_on_other_threads_are_the_same_games() {
    let worlds = cut();
    let alone: Vec<String> = worlds.iter().map(|&w| text(w)).collect();
    let handles: Vec<_> = worlds
        .iter()
        .map(|&w| std::thread::spawn(move || text(w)))
        .collect();
    let together: Vec<String> = handles
        .into_iter()
        .map(|h| h.join().expect("a thread"))
        .collect();
    assert_eq!(alone, together);
}

#[test]
fn the_generator_alone_is_the_same_map_every_time() {
    for world in cut() {
        let a = generated(world);
        let b =
            f4x_newgame::mapgen::generate(world.seed, &params(world), content()).expect("a map");
        assert_eq!(*a, b, "{world:?}");
    }
}

#[test]
fn names_do_not_move_the_map_or_the_sites() {
    // The names are for the cities and the wizards; the world's squares come from the seed alone.
    let std = standard();
    let plain = fresh_game(std);
    let mut spec = std.spec();
    spec.names.rivals = vec![vec![("Ana".into(), Default::default())]; 12];
    spec.names.cities.insert(0, vec!["A".into(), "B".into()]);
    let named = f4x_newgame::setup::new_game(content(), &spec).expect("a game");
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
        let g = fresh_game(World { wizards, ..std });
        let mut v = g.economy().site_squares();
        v.sort_by_key(|p| (p.plane.index(), p.y, p.x));
        v
    };
    let few = sites(2);
    assert!(!few.is_empty());
    assert_eq!(few, sites(5));
    assert_eq!(few, sites(most_wizards()));
}
