//! Class 9, the world's own doors: whose command a command is, and the override mutators (cheats)
//! that the console and the tests use.

use crate::support::{around, commands, content, text, worlds};
use f4x_battle::battle::BattleCommand;
use f4x_core::feature::def::FeatureKind;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Plane, Position};
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::rules::{BuildingId, RealmId, UnitId};
use f4x_core::wizard::magic::Retort;
use f4x_core::world::save::Snapshot;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event, World};
use f4x_def::half_int::HalfInt;

const SPEARMEN: u16 = 104; // high_men_spearmen
const NOBODY: PlayerId = PlayerId(250);

fn create(w: &mut Table, owner: PlayerId, pos: Position) -> hecs::Entity {
    let before = w.units().units_at(pos);
    w.apply(Command::CreateUnit {
        owner,
        pos,
        unit: UnitId::new(SPEARMEN),
    });
    w.units()
        .units_at(pos)
        .into_iter()
        .find(|u| !before.contains(u))
        .expect("a unit was made")
}

fn open_land(w: &World, q: Position) -> bool {
    w.economy()
        .terrain_class_at(q)
        .is_some_and(|c| c != TerrainClass::Ocean)
        && w.units().units_at(q).is_empty()
        && w.economy().city_at(q).is_none()
        && w.economy().feature_entity_at(q).is_none()
}

/// A square of open land beside the first wizard's fortress.
fn beside_home(w: &World) -> Position {
    let p = w.wizards()[0];
    let home = w
        .wizard(p)
        .expect("a wizard")
        .fortress()
        .expect("a fortress");
    around(w, home)
        .into_iter()
        .find(|&q| open_land(w, q))
        .expect("open land")
}

// ---- whose command ----

#[test]
fn a_command_that_acts_for_a_player_is_nobody_elses_to_give() {
    for (name, w) in worlds() {
        let everybody: Vec<PlayerId> = w.player_ids().into_iter().chain([NOBODY]).collect();
        let (mut acting, mut nameless) = (0, 0);
        for command in commands(&w) {
            let what = format!("{name}: {command:?}");
            let actor = w.command_actor(&command);
            for &p in &everybody {
                let may = w.may_command(p, &command);
                match actor {
                    Some(a) if a != p => assert!(!may, "{what}: {p:?} for {a:?}"),
                    Some(_) => acting += 1,
                    // A command that names nobody is left to the world to refuse; it is barred
                    // only to who orders another's units with it.
                    None => nameless += i32::from(may),
                }
            }
            // A command repeated "anyway" is the same player's.
            let anyway = Command::Anyway(Box::new(command.clone()));
            assert_eq!(w.command_actor(&anyway), actor, "{what}");
            for &p in &everybody {
                assert_eq!(
                    w.may_command(p, &anyway),
                    w.may_command(p, &command),
                    "{what}"
                );
            }
        }
        assert!(acting > 0 && nameless > 0, "{name}: {acting}, {nameless}");
    }
}

#[test]
fn a_command_is_for_the_owner_of_what_it_orders_and_a_mixed_stack_is_nobodys() {
    for (name, mut w) in worlds() {
        let (p, q) = (w.wizards()[0], w.wizards()[1]);
        let at = beside_home(&w);
        let mine = create(&mut w, p, at);
        let theirs = create(&mut w, q, at);
        let (city, _) = w.economy().cities_of(q)[0];
        let to = around(&w, at)[0];
        let walk = |units| Command::MoveStack { units, to };
        for (command, owner) in [
            (walk(vec![mine]), p),
            (walk(vec![theirs]), q),
            (Command::DismissUnit { unit: mine }, p),
            (Command::BalanceFarmers { player: q }, q),
            (Command::SetFarmers { city, farmers: 1 }, q),
            (
                Command::BeginBattle {
                    attackers: vec![mine],
                    defenders: vec![theirs],
                },
                p,
            ),
        ] {
            let what = format!("{name}: {command:?}");
            assert_eq!(w.command_actor(&command), Some(owner), "{what}");
            let other = if owner == p { q } else { p };
            assert!(w.may_command(owner, &command), "{what}");
            assert!(!w.may_command(other, &command), "{what}");
        }
        for mixed in [walk(vec![mine, theirs]), walk(vec![theirs, mine])] {
            for player in [p, q] {
                assert!(!w.may_command(player, &mixed), "{name}: {mixed:?}");
            }
        }
        // With the unit gone, the order names nobody and the world refuses it.
        w.cheat_remove_unit(mine);
        assert_eq!(w.command_actor(&walk(vec![mine])), None, "{name}");
        assert!(w.may_command(q, &walk(vec![mine])), "{name}");
        let events = w.apply(walk(vec![mine]));
        assert!(
            events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{name}: {events:?}"
        );
    }
}

#[test]
fn a_battle_order_is_for_the_side_whose_turn_it_is() {
    for (name, mut w) in worlds() {
        let (p, q) = (w.wizards()[0], w.wizards()[1]);
        let order = Command::Battle(BattleCommand::EndTurn);
        assert_eq!(w.command_actor(&order), None, "{name}: no battle");
        let at = beside_home(&w);
        let far = around(&w, at)
            .into_iter()
            .find(|&s| s != at && open_land(&w, s))
            .expect("open land");
        let attacker = create(&mut w, p, at);
        let defender = create(&mut w, q, far);
        w.start_battle(&[attacker], &[defender])
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let mut sides = Vec::new();
        for _ in 0..4 {
            let Some(actor) = w.command_actor(&order) else {
                break;
            };
            let other = if actor == p { q } else { p };
            assert!(w.may_command(actor, &order), "{name}");
            assert!(!w.may_command(other, &order), "{name}");
            sides.push(actor);
            w.apply(order.clone());
        }
        assert!(
            sides.contains(&p) && sides.contains(&q),
            "{name}: {sides:?}"
        );
    }
}

// ---- cheats ----

#[test]
fn an_override_marks_the_game_for_good_and_is_saved() -> Result<(), Box<dyn std::error::Error>> {
    for (name, mut w) in worlds() {
        assert!(!w.cheated() && w.override_log().is_empty(), "{name}");
        w.record_override("first");
        w.record_override("second");
        assert!(w.cheated(), "{name}");
        assert_eq!(w.override_log(), ["first", "second"], "{name}");
        let back = Snapshot::from_text(&text(&w))?.restore(content())?;
        assert!(back.cheated(), "{name}");
        assert_eq!(back.override_log(), w.override_log(), "{name}");
    }
    Ok(())
}

#[test]
fn a_restored_unit_is_whole_and_ready() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let at = beside_home(&w);
        let unit = create(&mut w, p, at);
        let full = w.units().unit_hits(unit);
        w.scenario_set_hurt(unit, 1);
        w.scenario_set_moves_left(unit, HalfInt::ZERO);
        assert!(w.units().unit_hits(unit).0 < full.0, "{name}");
        w.cheat_restore_unit(unit);
        assert_eq!(w.units().unit_hits(unit), full, "{name}");
        assert_eq!(
            w.units().unit_moves_left(unit),
            w.units().unit_max_moves(unit),
            "{name}"
        );
    }
}

#[test]
fn a_unit_is_enchanted_by_cheat_only_while_it_is_there() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let at = beside_home(&w);
        let unit = create(&mut w, p, at);
        let spell = f4x_core::rules::SpellId::new(2); // resist_elements
        assert!(w.cheat_enchant_unit(unit, spell, p), "{name}");
        assert_eq!(
            w.spells().unit_enchantments(unit),
            vec![(spell, p)],
            "{name}"
        );
        w.cheat_remove_unit(unit);
        assert!(!w.cheat_enchant_unit(unit, spell, p), "{name}");
    }
}

#[test]
fn terrain_rivers_and_features_are_set_on_the_map_and_nowhere_else() {
    for (name, mut w) in worlds() {
        let at = beside_home(&w);
        let off = Position {
            x: w.map().expect("a map").width(),
            ..at
        };
        let terrain = |w: &World| {
            w.map()
                .expect("a map")
                .plane(at.plane)
                .get(at.x as i32, at.y as i32)
                .expect("a terrain")
        };
        let was = terrain(&w);
        let other = w
            .content()
            .terrains
            .first_where(|d| w.content().terrains.id_numbered(d.num) != Some(was))
            .expect("another terrain");
        assert!(w.cheat_set_terrain(at, other), "{name}");
        assert_eq!(terrain(&w), other, "{name}");
        assert!(!w.cheat_set_terrain(off, other), "{name}");
        let river = |w: &World| {
            w.map()
                .expect("a map")
                .plane(at.plane)
                .has_river(at.x as i32, at.y as i32)
        };
        for on in [true, false] {
            assert!(w.cheat_set_river(at, on), "{name}");
            assert_eq!(river(&w), on, "{name}");
            assert!(!w.cheat_set_river(off, on), "{name}");
        }
        let features: Vec<_> = [FeatureKind::Mineral, FeatureKind::Encounter]
            .into_iter()
            .map(|kind| {
                w.content()
                    .features
                    .first_where(|d| d.kind == kind)
                    .expect("such a feature")
            })
            .collect();
        for &id in &features {
            assert!(w.cheat_set_feature(at, Some(id)), "{name}");
            let found = w.economy().feature_at(at).map(|d| d.num);
            assert_eq!(found, w.content().features.get(id).map(|d| d.num), "{name}");
            assert!(!w.cheat_set_feature(off, Some(id)), "{name}");
        }
        assert!(w.cheat_set_feature(at, None), "{name}");
        assert!(w.economy().feature_at(at).is_none(), "{name}");
        assert!(w.economy().feature_entity_at(at).is_none(), "{name}");
    }
}

#[test]
fn a_lair_is_put_with_guards_and_taken_away_again() {
    for (name, mut w) in worlds() {
        let at = beside_home(&w);
        let encounter = w
            .content()
            .features
            .first_where(|d| d.kind == FeatureKind::Encounter)
            .expect("an encounter site");
        let mineral = w
            .content()
            .features
            .first_where(|d| d.kind == FeatureKind::Mineral)
            .expect("a mineral");
        let off = Position {
            x: w.map().expect("a map").width(),
            ..at
        };
        let before = text(&w);
        let (world, rng) = w.world_and_rng();
        assert!(
            world.cheat_add_lair(at, mineral, true, rng).is_err(),
            "{name}"
        );
        assert!(
            world.cheat_add_lair(off, encounter, true, rng).is_err(),
            "{name}"
        );
        assert!(!world.cheat_remove_lair(at), "{name}: nothing there");
        assert_eq!(text(world), before, "{name}");
        world
            .cheat_add_lair(at, encounter, true, rng)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            world
                .economy()
                .lair_at(at)
                .is_some_and(|l| l.guard_count() > 0),
            "{name}"
        );
        assert!(world.cheat_remove_lair(at), "{name}");
        assert!(world.economy().lair_at(at).is_none(), "{name}");
        // A mineral is a feature with no lair to remove.
        world.cheat_set_feature(at, Some(mineral));
        assert!(!world.cheat_remove_lair(at), "{name}");
    }
}

#[test]
fn a_building_is_given_once_and_not_to_what_is_gone() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (city, _) = w.economy().cities_of(p)[0];
        let building = w
            .content()
            .rules
            .buildings
            .defs()
            .iter()
            .map(|d| BuildingId::new(d.num))
            .find(|&b| !w.economy().has_building(city, b))
            .expect("a building it lacks");
        assert!(w.cheat_add_building(city, building), "{name}");
        assert!(w.economy().has_building(city, building), "{name}");
        let once = text(&w);
        assert!(w.cheat_add_building(city, building), "{name}");
        assert_eq!(text(&w), once, "{name}");
        let at = beside_home(&w);
        let gone = create(&mut w, p, at);
        w.cheat_remove_unit(gone);
        assert!(!w.cheat_add_building(gone, building), "{name}");
    }
}

#[test]
fn spells_books_and_retorts_are_given_and_taken_from_a_wizard() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let neutral = w
            .player_ids()
            .into_iter()
            .find(|&n| w.is_neutral_player(n) && w.wizard(n).is_none())
            .expect("a neutral player");
        // A spell the wizard could research.
        let spell = w
            .wizard(p)
            .expect("a wizard")
            .researchable()
            .into_iter()
            .next()
            .expect("something to research");
        let knows = |w: &World| w.wizard(p).expect("a wizard").knows(spell);
        let open = |w: &World| {
            w.wizard(p)
                .expect("a wizard")
                .researchable()
                .contains(&spell)
        };
        assert!(!w.cheat_forget_spell(p, spell), "{name}: not known yet");
        assert!(w.cheat_learn_spell(p, spell), "{name}");
        assert!(knows(&w) && !open(&w), "{name}");
        assert!(w.cheat_forget_spell(p, spell), "{name}");
        assert!(!knows(&w) && open(&w), "{name}");
        // Without the books for it, a forgotten spell is not there to research.
        let realm = w.content().rules.spells.get(spell).realm;
        w.cheat_learn_spell(p, spell);
        assert!(w.cheat_set_books(p, realm, 0), "{name}");
        assert!(w.cheat_forget_spell(p, spell), "{name}");
        let arcane = w
            .wizard(p)
            .expect("a wizard")
            .books_allow(w.content().rules.spells.get(spell));
        assert_eq!(open(&w), arcane, "{name}");
        assert!(!w.cheat_learn_spell(neutral, spell), "{name}");
        assert!(!w.cheat_forget_spell(neutral, spell), "{name}");

        let life = RealmId::new(3); // life
        let most = w.settings().wizards.max_books_per_realm;
        assert!(w.cheat_set_books(p, life, u8::MAX), "{name}");
        let books = |w: &World| w.wizard(p).expect("a wizard").books_in(life);
        assert_eq!(books(&w), i32::from(most), "{name}");
        assert!(w.cheat_set_books(p, life, 1), "{name}");
        assert_eq!(books(&w), 1, "{name}");
        assert!(!w.cheat_set_books(neutral, life, 1), "{name}");

        let retort = Retort::new(16); // charismatic
        w.cheat_set_retort(p, retort, false);
        assert!(w.cheat_set_retort(p, retort, true), "{name}");
        assert!(!w.cheat_set_retort(p, retort, true), "{name}: had it");
        assert!(
            w.wizard(p).expect("a wizard").retorts().contains(&retort),
            "{name}"
        );
        assert!(w.cheat_set_retort(p, retort, false), "{name}");
        assert!(!w.cheat_set_retort(p, retort, false), "{name}: lacked it");
        assert!(!w.cheat_set_retort(neutral, retort, true), "{name}");
    }
}

#[test]
fn an_eliminated_wizard_loses_every_city_to_the_monsters_once() {
    for (name, mut w) in worlds() {
        let (p, q) = (w.wizards()[0], w.wizards()[1]);
        let cities: Vec<_> = w.economy().cities_of(q).into_iter().map(|c| c.0).collect();
        let (world, rng) = w.world_and_rng();
        let events = world.cheat_eliminate(q, p, rng);
        assert!(
            events.contains(&Event::WizardDefeated { player: q }),
            "{name}: {events:?}"
        );
        assert!(world.player(q).is_some_and(|x| x.is_defeated()), "{name}");
        assert!(world.economy().cities_of(q).is_empty(), "{name}");
        for city in cities {
            assert_eq!(world.economy().city_owner(city), Some(MONSTERS), "{name}");
        }
        assert!(world.cheat_eliminate(q, p, rng).is_empty(), "{name}: again");
        assert!(
            world.cheat_eliminate(NOBODY, p, rng).is_empty(),
            "{name}: nobody"
        );
    }
}

#[test]
fn revealing_the_map_shows_every_square_of_both_planes_to_one_player() {
    for (name, mut w) in worlds() {
        let (p, q) = (w.wizards()[0], w.wizards()[1]);
        let map = w.map().expect("a map");
        let all = (map.width() * map.height()) as usize * Plane::all().count();
        let theirs = w.explored_squares(q);
        assert!(w.explored_squares(p) < all, "{name}");
        w.reveal_map(p);
        assert_eq!(w.explored_squares(p), all, "{name}");
        assert_eq!(w.explored_squares(q), theirs, "{name}");
    }
}
