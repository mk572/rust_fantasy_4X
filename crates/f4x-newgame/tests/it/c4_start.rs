//! Class 4 (`docs/f4x-newgame-test-design.md`): the start is playable.
//!
//! `start_problems` reads a new game through the public interface of `f4x-core` and the content
//! and returns every rule a wizard's or a neutral city's start breaks, as `rule: what`. It calls
//! nothing of the setup code. The tests run it on every world of the matrix, show with a broken
//! game that each rule can fire, and play a few turns on a cut of the worlds.

use crate::support::*;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Plane, Position};
use f4x_core::player::PlayerId;
use f4x_core::wizard::retort_available;
use f4x_core::world::{Event, World};
use f4x_def::half_int::HalfInt;
use f4x_master::Game;
use f4x_newgame::neutral::{FORTRESS_CLEARANCE, SITE_CLEARANCE};
use f4x_newgame::setup::{parse_retort, MIN_START_SIZE};

fn class_at(w: &World, c: &f4x_def::content::Content, p: Position) -> Option<TerrainClass> {
    let id = w.map()?.plane(p.plane).get(p.x as i32, p.y as i32)?;
    Some(c.terrains.get(id)?.class)
}

/// Every rule the started game breaks, as `rule: what`.
pub fn start_problems(w: &World, case: WorldCase) -> Vec<String> {
    let c = w.content();
    let rules = &c.rules;
    let s = w.settings();
    let mut bad = Vec::new();
    let mut say = |rule: &str, what: String| bad.push(format!("{rule}: {what}"));
    let Some(map) = w.map() else {
        return vec!["map: the game has no map".into()];
    };
    let width = map.width();
    let myrran = parse_retort(rules, "myrran").expect("the content has the Myrran retort");

    let ids = w.player_ids();
    let wizards: Vec<PlayerId> = ids
        .iter()
        .copied()
        .filter(|&p| !w.is_neutral_player(p) && w.wizard(p).is_some())
        .collect();
    if wizards.len() != usize::from(case.wizards) {
        say(
            "wizards",
            format!("{} wizards for {}", wizards.len(), case.wizards),
        );
    }
    if w.human().is_some() == case.all_ai {
        say(
            "human",
            format!("human is {:?}, all_ai is {}", w.human(), case.all_ai),
        );
    }

    // Each wizard: the fortress, the plane, the books and retorts, the purse, the army.
    let mut fortresses: Vec<(PlayerId, Position)> = Vec::new();
    let mut myrrans = 0;
    for &id in &wizards {
        let wizard = w.wizard(id).expect("a wizard");
        let books = wizard.books();
        let retorts = wizard.retorts();
        let is_myrran = wizard.has(myrran);
        myrrans += usize::from(is_myrran);
        let Some(at) = wizard.fortress() else {
            say("fortress", format!("{id:?} has no fortress"));
            continue;
        };
        fortresses.push((id, at));
        if class_at(w, c, at) == Some(TerrainClass::Ocean) || class_at(w, c, at).is_none() {
            say("fortress-land", format!("{id:?} at {at:?} is in the sea"));
        }
        if (at.plane == Plane::Myrror) != is_myrran {
            say(
                "plane",
                format!(
                    "{id:?}: Myrran is {is_myrran} but the fortress is on {:?}",
                    at.plane
                ),
            );
        }
        match w.economy().city_at(at) {
            Some(city) => {
                if w.economy()
                    .cities()
                    .iter()
                    .find(|(e, _, _)| *e == city)
                    .map(|(_, i, _)| i.owner)
                    != Some(id)
                {
                    say(
                        "fortress-city",
                        format!("{id:?}: the city at {at:?} is another's"),
                    );
                }
                // A start on land that feeds less than the least is the last resort, and right only
                // when no square of the plane that can take a city feeds that many.
                if w.economy().site_max_size(at) < MIN_START_SIZE {
                    if let Some(better) = (0..map.height())
                        .flat_map(|y| (0..width).map(move |x| (x, y)))
                        .map(|(x, y)| Position { x, y, ..at })
                        .find(|&p| {
                            w.can_found_city(p).is_ok()
                                && w.economy().site_max_size(p) >= MIN_START_SIZE
                        })
                    {
                        say(
                            "start-size",
                            format!(
                                "{id:?}: the land holds {}, and {better:?} would feed more",
                                w.economy().site_max_size(at)
                            ),
                        );
                    }
                }
                let army = w
                    .city_race(city)
                    .map(|r| rules.races.get(r).start_army.len())
                    .unwrap_or(usize::MAX);
                let here = w
                    .units()
                    .units_at(at)
                    .into_iter()
                    .filter(|&u| w.units().unit_owner(u) == Some(id))
                    .count();
                if here < army {
                    say(
                        "army",
                        format!("{id:?}: {here} units for an army of {army}"),
                    );
                }
            }
            None => say("fortress-city", format!("{id:?}: no city at {at:?}")),
        }
        for (&realm, &n) in &books {
            if n > s.wizards.max_books_per_realm {
                say("books", format!("{id:?}: {n} books in {realm:?}"));
            }
        }
        if books.values().map(|&n| u32::from(n)).sum::<u32>() > u32::from(s.wizards.max_books_total)
        {
            say(
                "books",
                format!("{id:?}: {} books in all", books.values().sum::<u8>()),
            );
        }
        if retorts.len() > rules.config.wizard.max_retorts {
            say("retorts", format!("{id:?}: {} retorts", retorts.len()));
        }
        for &r in &retorts {
            if !retort_available(rules, r, &books) {
                say("retorts", format!("{id:?}: {r:?} without the books for it"));
            }
        }
        if s.rules.ban_life_and_death {
            let has = |name: &str| {
                rules
                    .realms
                    .defs()
                    .iter()
                    .find(|d| d.id == name)
                    .map(|d| {
                        books
                            .get(&f4x_core::rules::RealmId::new(d.num))
                            .copied()
                            .unwrap_or(0)
                    })
                    .unwrap_or(0)
                    > 0
            };
            if has("life") && has("death") {
                say("life-and-death", format!("{id:?} has both"));
            }
        }
        let human = w.human() == Some(id);
        let purse = w.player(id).map(|p| p.treasury());
        let (gold, mana) = (
            HalfInt::from_int(s.wizards.starting_gold.of(human)),
            HalfInt::from_int(s.wizards.starting_mana.of(human)),
        );
        if purse.map(|t| (t.gold, t.mana)) != Some((gold, mana)) {
            say(
                "purse",
                format!("{id:?}: {:?}", purse.map(|t| (t.gold, t.mana))),
            );
        }
    }

    // The Myrran retort is shared out as evenly as it allows.
    let n = wizards.len();
    let (least, most) = (n.saturating_sub(2).div_ceil(2), n.div_ceil(2));
    if !(least..=most).contains(&myrrans) {
        say(
            "myrran-share",
            format!("{myrrans} of {n}, allowed {least} to {most}"),
        );
    }
    // Fortresses are different squares, and keep the distance between cities.
    let min_distance = u32::from(s.rules.min_city_distance);
    for (i, &(a, pa)) in fortresses.iter().enumerate() {
        for &(b, pb) in &fortresses[i + 1..] {
            if pa.plane == pb.plane && chebyshev(width, (pa.x, pa.y), (pb.x, pb.y)) < min_distance {
                say(
                    "fortress-spacing",
                    format!("{a:?} at {pa:?} and {b:?} at {pb:?}"),
                );
            }
        }
    }
    // A banner and a portrait for each, different while there are enough.
    let mut banners = std::collections::BTreeSet::new();
    let mut portraits = std::collections::BTreeSet::new();
    for &id in &wizards {
        if let Some(p) = w.player(id) {
            banners.insert(p.banner());
            portraits.insert(p.portrait());
        }
    }
    if banners.len() < n.min(usize::from(f4x_core::player::BANNER_CHOICES)) {
        say(
            "banner",
            format!("{} banners for {n} wizards", banners.len()),
        );
    }
    if portraits.len() < n.min(usize::from(f4x_core::player::PORTRAITS)) {
        say(
            "portrait",
            format!("{} portraits for {n} wizards", portraits.len()),
        );
    }
    // The races: no two wizards on a plane have the same one while the plane has a free one.
    for plane in Plane::all() {
        let mut taken = Vec::new();
        let mut here = 0;
        for &(id, at) in &fortresses {
            if at.plane != plane {
                continue;
            }
            here += 1;
            if let Some(r) = w.economy().city_at(at).and_then(|e| w.city_race(e)) {
                if taken.contains(&r) {
                    let on_plane = rules
                        .races
                        .defs()
                        .iter()
                        .filter(|d| d.myrror == (plane == Plane::Myrror))
                        .count();
                    if here <= on_plane && !s.rules.limit_ai_races {
                        say(
                            "race",
                            format!("{id:?} has a race another {plane:?} wizard has"),
                        );
                    }
                }
                taken.push(r);
            }
        }
    }

    // The independent cities.
    let sites = w.economy().site_squares();
    let target = s.basic.neutral_city_count() as usize;
    let mut neutral: Vec<Position> = Vec::new();
    for (e, info, at) in w.economy().cities() {
        if !w.is_neutral_player(info.owner) {
            continue;
        }
        neutral.push(at);
        if class_at(w, c, at) == Some(TerrainClass::Ocean) || class_at(w, c, at).is_none() {
            say("neutral-land", format!("the city at {at:?} is in the sea"));
        }
        for &(id, f) in &fortresses {
            if f.plane == at.plane && range(width, (f.x, f.y), (at.x, at.y)) < FORTRESS_CLEARANCE {
                say(
                    "neutral-fortress",
                    format!("{at:?} is near the fortress of {id:?}"),
                );
            }
        }
        for sq in &sites {
            if sq.plane == at.plane && range(width, (sq.x, sq.y), (at.x, at.y)) < SITE_CLEARANCE {
                say("neutral-site", format!("{at:?} is near the site {sq:?}"));
            }
        }
        if let Some(r) = w.city_race(e) {
            let def = rules.races.get(r);
            let ok = if s.rules.mix_neutral_races {
                match at.plane {
                    Plane::Arcanus => def.neutral_weight.arcanus > 0,
                    Plane::Myrror => def.neutral_weight.myrror > 0,
                }
            } else {
                def.myrror == (at.plane == Plane::Myrror)
            };
            if !ok {
                say(
                    "neutral-race",
                    format!("{:?} at {at:?} is of the wrong plane", def.id),
                );
            }
        } else {
            say("neutral-race", format!("the city at {at:?} has no race"));
        }
    }
    for (i, &a) in neutral.iter().enumerate() {
        for &b in &neutral[i + 1..] {
            if a.plane == b.plane && chebyshev(width, (a.x, a.y), (b.x, b.y)) < min_distance {
                say("neutral-spacing", format!("{a:?} and {b:?}"));
            }
        }
    }
    for plane in Plane::all() {
        let n = neutral.iter().filter(|p| p.plane == plane).count();
        if n > target {
            say("neutral-count", format!("{n} on {plane:?} for {target}"));
        }
    }
    // Every site square is on land.
    for sq in &sites {
        if class_at(w, c, *sq) == Some(TerrainClass::Ocean) {
            say("site-land", format!("{sq:?} is in the sea"));
        }
    }
    // One of each hero.
    let heroes = w.units().all_heroes().len();
    if heroes != rules.hero_templates.len() {
        say(
            "heroes",
            format!("{heroes} heroes for {} types", rules.hero_templates.len()),
        );
    }
    bad
}

#[test]
fn every_world_of_the_matrix_starts_playable() {
    let mut failures = Vec::new();
    for case in worlds() {
        let w = game(case);
        let problems = start_problems(&w, case);
        if !problems.is_empty() {
            failures.push(format!("{case:?}\n  {}", problems.join("\n  ")));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_standard_world_has_no_problem_to_start_from() {
    let case = standard();
    assert_eq!(start_problems(&game(case), case), Vec::<String>::new());
}

fn has_rule(problems: &[String], rule: &str) -> bool {
    problems.iter().any(|p| p.starts_with(&format!("{rule}:")))
}

#[test]
fn a_wizard_with_a_book_too_many_is_found() {
    let case = standard();
    let mut w = fresh_game(case);
    // The limit comes down under what a wizard holds: the same as a wizard over the limit.
    let most = w
        .wizard(PlayerId(0))
        .expect("a wizard")
        .books()
        .values()
        .copied()
        .max()
        .expect("a book");
    w.scenario_change_settings(|s| s.wizards.max_books_per_realm = most - 1);
    assert!(has_rule(&start_problems(&w, case), "books"));
}

#[test]
fn a_fortress_in_the_sea_is_found() {
    let case = standard();
    let mut w = fresh_game(case);
    let at = w
        .wizard(PlayerId(0))
        .and_then(|wizard| wizard.fortress())
        .expect("a fortress");
    let sea = w
        .content()
        .terrains
        .first_of_class(TerrainClass::Ocean, at.plane)
        .expect("a sea");
    w.scenario_set_terrain(at, sea);
    assert!(has_rule(&start_problems(&w, case), "fortress-land"));
}

#[test]
fn a_neutral_city_beside_a_fortress_is_found() {
    let case = standard();
    let mut w = fresh_game(case);
    let at = w
        .wizard(PlayerId(0))
        .and_then(|wizard| wizard.fortress())
        .expect("a fortress");
    let race = w
        .content()
        .rules
        .races
        .id_numbered(w.content().rules.races.defs()[0].num)
        .expect("a race");
    // The nearest square a city may be founded on that is closer than the clearance asks.
    let mut founded = false;
    'find: for d in 1..5i32 {
        for dy in -d..=d {
            for dx in -d..=d {
                let pos = Position {
                    x: (at.x as i32 + dx).rem_euclid(w.map().expect("a map").width() as i32) as u32,
                    y: (at.y as i32 + dy).clamp(0, w.map().expect("a map").height() as i32 - 1)
                        as u32,
                    ..at
                };
                let near = range(
                    w.map().expect("a map").width(),
                    (at.x, at.y),
                    (pos.x, pos.y),
                ) < FORTRESS_CLEARANCE;
                if near && w.can_found_city(pos).is_ok() {
                    w.found_neutral_city(pos, race);
                    founded = true;
                    break 'find;
                }
            }
        }
    }
    assert!(founded, "a square beside the fortress to found a city on");
    let problems = start_problems(&w, case);
    assert!(has_rule(&problems, "neutral-fortress"), "{problems:?}");
}

#[test]
fn a_wizard_without_gold_is_found() {
    let case = standard();
    let mut w = fresh_game(case);
    w.treasury_mut(PlayerId(0)).expect("a purse").gold = HalfInt::from_int(-1);
    assert!(has_rule(&start_problems(&w, case), "purse"));
}

/// Ten turns of the computer's play on a game, with every refusal listed.
fn played_refusals(case: WorldCase, turns: usize) -> Vec<String> {
    let mut w = Game::new(fresh_game(case));
    let mut refused = Vec::new();
    for _ in 0..turns {
        for e in w.play_round() {
            if matches!(e, Event::Rejected(_)) {
                refused.push(format!("{e:?}"));
            }
        }
    }
    refused
}

#[test]
fn turns_pass_on_a_cut_of_the_worlds_without_a_refusal_or_a_panic() {
    let std = standard();
    let mut worlds = vec![std];
    for &variant in VARIANTS {
        worlds.push(WorldCase {
            variant,
            seed: 3,
            ..std
        });
    }
    for (width, height) in sizes() {
        worlds.push(WorldCase {
            width,
            height,
            seed: 6,
            ..std
        });
    }
    for w in [2, most_wizards()] {
        worlds.push(WorldCase {
            wizards: w,
            seed: 8,
            ..std
        });
    }
    for case in worlds {
        let refused = played_refusals(case, 10);
        assert!(refused.is_empty(), "{case:?}: {refused:?}");
    }
}

#[test]
fn a_new_game_saves_and_loads_into_the_same_game() {
    for case in [
        standard(),
        WorldCase {
            all_ai: false,
            ..standard()
        },
        WorldCase {
            variant: Variant::OriginalGenerator,
            ..standard()
        },
    ] {
        let w = fresh_game(case);
        let text = w.snapshot().to_text().expect("the game saves");
        let loaded = f4x_core::world::save::Snapshot::from_text(&text)
            .expect("the text reads")
            .restore(content().clone())
            .expect("the game loads");
        assert_eq!(
            loaded.snapshot().to_text().expect("saves again"),
            text,
            "{case:?}"
        );
    }
}
