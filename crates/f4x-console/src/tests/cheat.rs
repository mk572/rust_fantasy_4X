//! The override (cheat) commands: they need the override mode, each use is logged, the log is
//! saved, and a game that used the mode is `cheated` (no Hall of Fame).

use super::*;
use f4x_battle::battle::Side;
use f4x_core::feature::def::FeatureKind;
use f4x_core::map::{Plane, Position};
use f4x_def::map::TerrainClass;
use f4x_def::testdir::TestDir;

fn first_unit_of(w: &World, owner: PlayerId) -> hecs::Entity {
    w.units()
        .all_units()
        .into_iter()
        .find(|&u| w.units().unit_owner(u) == Some(owner))
        .expect("the player has a unit")
}

#[test]
fn override_commands_need_the_override_mode() {
    let (mut w, mut c) = (game(), console());
    let refused = c.execute(&mut w, "give gold 500");
    assert!(!refused.ok);
    assert!(
        matches!(err(&refused), ConsoleError::NotAllowed(_)),
        "{refused:?}"
    );
    let gold = w.player(HUMAN).expect("a player").treasury().gold;

    assert!(c.execute(&mut w, "mode override").ok);
    assert_eq!(c.session.mode, Mode::Override);
    assert!(c.execute(&mut w, "give gold 500").ok);
    assert_eq!(
        w.player(HUMAN).expect("a player").treasury().gold,
        gold + 500
    );

    assert!(c.execute(&mut w, "mode legal").ok);
    assert!(!c.execute(&mut w, "give gold 500").ok);
    assert_eq!(
        w.player(HUMAN).expect("a player").treasury().gold,
        gold + 500
    );

    // Switching the mode on and each cheat are in the game's saved log.
    let log = w.override_log().to_vec();
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(log[0].contains("mode override") && log[1].contains("give gold 500"));
    let restored = w
        .snapshot()
        .restore(w.content().clone())
        .expect("the snapshot restores");
    assert_eq!(restored.override_log(), log.as_slice());
    assert!(c.execute(&mut w, "history").text().contains("give gold"));
}

#[test]
fn a_game_that_entered_the_cheat_mode_is_marked_for_good_and_stays_so_in_a_save() {
    let (mut w, mut c) = (game(), console());
    assert!(!w.cheated());
    // Looking is free: a refused cheat and the queries do not mark the game.
    assert!(!c.execute(&mut w, "give gold 1").ok);
    assert!(c.execute(&mut w, "status").ok);
    assert!(!w.cheated());
    // Entering the mode is enough, before any cheat is used.
    let on = c.execute(&mut w, "mode override");
    assert!(on.text().contains("Hall of Fame"), "{}", on.text());
    assert!(w.cheated());
    assert!(c.execute(&mut w, "mode legal").ok);
    assert!(w.cheated(), "going back does not clear it");
    let restored = w
        .snapshot()
        .restore(w.content().clone())
        .expect("the snapshot restores");
    assert!(restored.cheated());
    let text = w.snapshot().to_text().expect("the snapshot is written");
    let reread = f4x_core::world::save::Snapshot::from_text(&text).expect("it reads back");
    assert!(reread
        .restore(w.content().clone())
        .expect("it restores")
        .cheated());
}

#[test]
fn a_player_console_cannot_enter_the_override_mode() {
    let (mut w, _) = (game(), ());
    let mut player = console_for(Access::Play);
    assert!(!player.execute(&mut w, "mode override").ok);
    assert!(!player.execute(&mut w, "give gold 1").ok);
    assert!(!w.cheated());
    assert!(!player.complete("sp").contains(&"spawn"));
    let mut full = console();
    full.execute(&mut w, "mode override");
    assert!(full.complete("sp").contains(&"spawn"));
}

#[test]
fn eliminating_the_rival_wins_by_conquest() {
    let (mut w, mut c) = (game(), console());
    c.execute(&mut w, "mode override");
    let r = c.execute(&mut w, "eliminate 1");
    assert!(r.ok, "{}", r.text());
    assert!(r.text().contains("GAME OVER"), "{}", r.text());
    assert_eq!(w.game_over().expect("the game is over").winner, HUMAN);
}

#[test]
fn undead_enchant_restore_and_kill_change_units() {
    let (mut w, mut c) = (game(), console());
    let unit = first_unit_of(&w, HUMAN);
    let serial = w.units().serial_of(unit).0;
    assert!(!c.execute(&mut w, &format!("undead {serial}")).ok);
    assert!(!w.units().unit_is_undead(unit));
    assert!(c.execute(&mut w, "mode override").ok);
    let r = c.execute(&mut w, &format!("undead {serial}"));
    assert!(r.ok, "{}", r.text());
    assert!(w.units().unit_is_undead(unit));
    let r = c.execute(&mut w, &format!("enchant {serial} haste"));
    assert!(r.ok, "{}", r.text());
    assert!(
        w.units()
            .unit_enchantments(unit)
            .iter()
            .any(|(s, _)| { w.content().rules.spells.get(*s).id == "haste" }),
        "the unit has Haste"
    );
    assert!(c.execute(&mut w, &format!("restore {serial}")).ok);
    assert!(c.execute(&mut w, &format!("kill {serial}")).ok);
    assert!(!w.units().all_units().contains(&unit));
}

#[test]
fn citizens_spawn_and_reveal_work_in_the_override_mode() {
    let (mut w, mut c) = (game(), console());
    c.execute(&mut w, "mode override");
    let n = first_city(&w);
    let r = c.execute(&mut w, &format!("citizens {n} 9"));
    assert!(r.ok, "{}", r.text());
    assert!(c.execute(&mut w, &format!("city {n}")).ok);
    let before = w.units().all_units().len();
    let r = c.execute(&mut w, "spawn barbarian_spearmen 3,3 1");
    assert!(r.ok, "{}", r.text());
    assert_eq!(w.units().all_units().len(), before + 1);
    let before = w.explored_squares(HUMAN);
    assert!(c.execute(&mut w, "reveal").ok);
    let after = w.explored_squares(HUMAN);
    assert!(before < after);
    // Both planes, every square: the map is the same size on each.
    assert_eq!(after % 2, 0, "{after}");
    let rival = w.explored_squares(PlayerId(1));
    assert!(rival < after, "only the asked wizard sees it all");
    assert!(c.execute(&mut w, &format!("build {n} barracks")).ok);
}

#[test]
fn contact_meets_the_other_wizards_in_the_override_mode_only() {
    let (mut w, mut c) = (game(), console());
    assert!(
        !c.execute(&mut w, "contact").ok,
        "a cheat needs the override mode"
    );
    c.execute(&mut w, "mode override");
    let rivals: Vec<PlayerId> = w.wizards().into_iter().filter(|&id| id != HUMAN).collect();
    assert!(!rivals.is_empty());
    let r = c.execute(&mut w, &format!("contact {}", rivals[0].0));
    assert!(r.ok, "{}", r.text());
    assert!(w.have_met(HUMAN, rivals[0]));
    let r = c.execute(&mut w, "contact");
    assert!(r.ok, "{}", r.text());
    for id in rivals {
        assert!(w.have_met(HUMAN, id));
    }
    assert!(!c.execute(&mut w, "contact 99").ok);
}

/// Plays until it is told otherwise, which is never.
struct Sitting;

impl f4x_master::Controller for Sitting {
    fn play(
        &mut self,
        _: &mut f4x_master::Game,
        _: PlayerId,
        _: &mut Vec<f4x_core::world::Event>,
    ) -> f4x_master::Status {
        f4x_master::Status::Waiting
    }
}

#[test]
fn the_override_mode_orders_units_of_a_player_whose_turn_it_is_not() {
    let (mut w, mut c) = (game(), console());
    let seat = f4x_master::Seat::new(HUMAN, Box::new(Sitting));
    f4x_master::Master::new(vec![seat]).run(&mut w, f4x_master::Until::Waiting);
    assert_eq!(w.turn(), Some(HUMAN));
    let rival = w.wizards().into_iter().find(|&id| id != HUMAN);
    let unit = first_unit_of(&w, rival.expect("a rival"));
    let order = format!("stop {}", w.units().serial_of(unit).0);
    assert!(!c.execute(&mut w, &order).ok, "not in the legal mode");
    c.execute(&mut w, "mode override");
    let r = c.execute(&mut w, &order);
    assert!(r.ok, "{}", r.text());
}

#[test]
fn an_attack_on_the_human_by_the_wizard_the_console_plays_is_fought_at_once() {
    let (mut w, mut c) = (game(), console());
    w.world_mut().set_human(Some(HUMAN));
    c.execute(&mut w, "mode override");
    let rival = w.wizards().into_iter().find(|&id| id != HUMAN);
    let rival = rival.expect("a rival");
    let r = c.execute(&mut w, &format!("as {}", rival.0));
    assert!(r.ok, "{}", r.text());
    let mine = first_unit_of(&w, HUMAN);
    let at = w.units().unit_position(mine).expect("a square");
    let theirs = first_unit_of(&w, rival);
    let beside = Position { x: at.x + 1, ..at };
    w.world_mut().cheat_move_unit(theirs, beside);
    let serial = w.units().serial_of(theirs).0;
    let order = format!("auto {serial} {},{}", at.x, at.y);
    let r = c.execute(&mut w, &order);
    assert!(r.ok, "{}", r.text());
    assert_eq!(w.waiting_attack(), None);
    assert!(!w.in_battle());
    let events = &r.events;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, f4x_core::world::Event::BattleStarted { .. })),
        "{}",
        r.text()
    );
}

/// The id of the first feature of a kind in the shipped content.
fn feature_of(w: &World, kind: f4x_core::feature::def::FeatureKind) -> String {
    w.content()
        .features
        .defs()
        .iter()
        .find(|d| d.kind == kind)
        .map(|d| d.id.clone())
        .expect("the content has a feature of the kind")
}

/// The square the site tests build on, on a plane.
fn square(plane: Plane, x: u32) -> Position {
    Position { plane, x, y: 2 }
}

/// Give a square the first terrain of the content that its plane has and `wanted` takes.
fn set_terrain(w: &mut Game, c: &mut Console, at: Position, wanted: impl Fn(TerrainClass) -> bool) {
    let terrain = w
        .content()
        .terrains
        .defs()
        .iter()
        .find(|d| d.planes.contains(&at.plane) && wanted(d.class))
        .map(|d| d.id.clone())
        .expect("the content has such a terrain");
    let r = c.execute(w, &format!("terrain {} {terrain}", words(at)));
    assert!(r.ok, "{}", r.text());
}

/// A square as a command takes it.
fn words(at: Position) -> String {
    let plane = match at.plane {
        Plane::Arcanus => "a",
        Plane::Myrror => "m",
    };
    format!("{},{},{plane}", at.x, at.y)
}

#[test]
fn the_site_cheats_need_the_override_mode() {
    let (mut w, mut c) = (game(), console());
    let site = feature_of(&w, FeatureKind::Encounter);
    for line in [
        format!("lair {site} 2,2"),
        "unlair 2,2".to_string(),
        format!("feature 2,2 {site}"),
    ] {
        let r = c.execute(&mut w, &line);
        assert!(
            matches!(err(&r), ConsoleError::NotAllowed(m) if m.contains("mode override")),
            "{line}: {}",
            r.text()
        );
    }
}

#[test]
fn a_lair_is_put_on_a_square_with_guards_and_taken_off_again() {
    let (mut w, mut c) = (game(), console());
    let at = square(Plane::Arcanus, 2);
    c.execute(&mut w, "mode override");
    set_terrain(&mut w, &mut c, at, |t| t == TerrainClass::Grassland);
    assert!(!c.execute(&mut w, "unlair 2,2").ok, "there is no lair yet");
    let site = feature_of(&w, FeatureKind::Encounter);
    let r = c.execute(&mut w, &format!("lair {site} 2,2 strong"));
    assert!(r.ok, "{}", r.text());
    let lair = w.economy().lair_at(at).expect("a lair stands there");
    assert!(!lair.guards.is_empty(), "the lair has guards");
    let r = c.execute(&mut w, "unlair 2,2");
    assert!(r.ok, "{}", r.text());
    assert!(w.economy().lair_at(at).is_none());
    assert!(!c.execute(&mut w, "lair nonsense 2,2").ok);
    let mineral = format!("lair {} 2,2", feature_of(&w, FeatureKind::Mineral));
    assert!(!c.execute(&mut w, &mineral).ok, "a mineral has no lair");
}

#[test]
fn a_feature_is_put_on_a_square_and_taken_off() {
    let (mut w, mut c) = (game(), console());
    c.execute(&mut w, "mode override");
    let at = square(Plane::Arcanus, 2);
    set_terrain(&mut w, &mut c, at, |t| t == TerrainClass::Grassland);
    let site = feature_of(&w, FeatureKind::Encounter);
    let r = c.execute(&mut w, &format!("feature 2,2 {site}"));
    assert!(r.ok, "{}", r.text());
    assert!(w.economy().feature_entity_at(at).is_some());
    let r = c.execute(&mut w, "feature 2,2 none");
    assert!(r.ok, "{}", r.text());
    assert!(w.economy().feature_entity_at(at).is_none());
}

/// Every feature on every terrain: taken where the content lets it stand (its spawn rules, a
/// node's own terrain, any land for a tower), refused elsewhere with nothing changed.
#[test]
fn a_feature_or_lair_is_refused_on_a_terrain_that_does_not_allow_it() {
    let (mut w, mut c) = (game(), console());
    c.execute(&mut w, "mode override");
    let at = square(Plane::Arcanus, 2);
    let features = w.content().features.defs().to_vec();
    let terrains = w.content().terrains.defs().to_vec();
    let (mut taken, mut refused) = (0, 0);
    for terrain in &terrains {
        for &plane in &terrain.planes {
            let here = Position { plane, ..at };
            // A tower needs land on the other plane too.
            set_terrain(&mut w, &mut c, here.on_other_plane(), |t| {
                t == TerrainClass::Grassland
            });
            let r = c.execute(&mut w, &format!("terrain {} {}", words(here), terrain.id));
            assert!(r.ok, "{}", r.text());
            for f in &features {
                let allowed = terrain.class != TerrainClass::Ocean
                    && (f.kind == FeatureKind::Tower
                        || f.realm.and_then(TerrainClass::of_node_realm) == Some(terrain.class)
                        || f.spawn.iter().any(|s| {
                            s.terrain.contains(&terrain.class) && s.planes.contains(&plane)
                        }));
                let lair = matches!(f.kind, FeatureKind::Encounter | FeatureKind::Tower);
                let mut lines = vec![format!("feature {} {}", words(here), f.id)];
                if lair {
                    lines.push(format!("lair {} {}", f.id, words(here)));
                }
                for line in lines {
                    assert!(
                        c.execute(&mut w, &format!("feature {} none", words(here)))
                            .ok
                    );
                    let r = c.execute(&mut w, &line);
                    let name = format!("{line} on {}: {}", terrain.id, r.text());
                    assert_eq!(r.ok, allowed, "{name}");
                    let found = w.economy().feature_at(here).map(|d| d.num);
                    assert_eq!(found, allowed.then_some(f.num), "{name}");
                    if allowed {
                        taken += 1;
                    } else {
                        assert!(
                            matches!(err(&r), ConsoleError::Failed(m) if m.contains(&terrain.id)),
                            "{name}"
                        );
                        refused += 1;
                    }
                }
            }
        }
    }
    assert!(taken > 0 && refused > 0, "{taken} taken, {refused} refused");
}

#[test]
fn a_refused_feature_leaves_what_stands_on_the_square() {
    let (mut w, mut c) = (game(), console());
    c.execute(&mut w, "mode override");
    let at = square(Plane::Arcanus, 2);
    set_terrain(&mut w, &mut c, at, |t| t == TerrainClass::Grassland);
    set_terrain(&mut w, &mut c, at.on_other_plane(), |t| {
        t == TerrainClass::Ocean
    });
    let site = feature_of(&w, FeatureKind::Encounter);
    assert!(c.execute(&mut w, &format!("lair {site} 2,2")).ok);
    let before = w.economy().lair_at(at).expect("a lair stands there");
    let tower = feature_of(&w, FeatureKind::Tower);
    for line in [format!("feature 2,2 {tower}"), format!("lair {tower} 2,2")] {
        let r = c.execute(&mut w, &line);
        assert!(!r.ok, "{line}: the other plane is sea");
        assert_eq!(w.economy().lair_at(at).as_ref(), Some(&before), "{line}");
    }
    let off = format!("feature {},2 {site}", w.map().expect("a map").width());
    assert!(!c.execute(&mut w, &off).ok, "off the map");
}

#[test]
fn a_tower_replaces_what_stands_on_both_planes() {
    let (mut w, mut c) = (game(), console());
    c.execute(&mut w, "mode override");
    let at = square(Plane::Arcanus, 2);
    let across = at.on_other_plane();
    for p in [at, across] {
        set_terrain(&mut w, &mut c, p, |t| t == TerrainClass::Grassland);
        assert!(c.execute(&mut w, &format!("feature {} none", words(p))).ok);
    }
    let lairs = w.economy().lairs().len();
    let site = feature_of(&w, FeatureKind::Encounter);
    assert!(
        c.execute(&mut w, &format!("lair {site} {}", words(across)))
            .ok
    );
    let tower = feature_of(&w, FeatureKind::Tower);
    let r = c.execute(&mut w, &format!("lair {tower} {}", words(at)));
    assert!(r.ok, "{}", r.text());
    assert_eq!(
        w.economy().lairs().len(),
        lairs + 1,
        "the site across is gone"
    );
    assert_eq!(
        w.economy().feature_entity_at(at),
        w.economy().feature_entity_at(across)
    );
    assert!(c.execute(&mut w, "unlair 2,2").ok);
    assert!(w.economy().feature_entity_at(across).is_none());
    assert_eq!(w.economy().lairs().len(), lairs);
}

/// A tower with guards at 2,2 and a stack of the console's player beside it; the stack's order.
fn tower_and_stack(w: &mut Game, c: &mut Console) -> String {
    c.execute(w, "mode override");
    let (at, beside) = (square(Plane::Arcanus, 2), square(Plane::Arcanus, 3));
    // A tower needs land on both planes.
    for p in [at, beside, at.on_other_plane()] {
        set_terrain(w, c, p, |t| t == TerrainClass::Grassland);
        assert!(c.execute(w, &format!("feature {} none", words(p))).ok);
    }
    let tower = feature_of(w, FeatureKind::Tower);
    let r = c.execute(w, &format!("lair {tower} 2,2"));
    assert!(r.ok, "{}", r.text());
    let units = &w.content().rules.units;
    let spearmen = units
        .id_numbered(104 /* high_men_spearmen */)
        .expect("the shipped content has it");
    let line = format!("spawn {} 3,2", units.get(spearmen).id);
    let r = c.execute(w, &line);
    assert!(r.ok, "{}", r.text());
    "enter @3,2 2,2".to_string()
}

#[test]
fn a_lair_made_by_the_console_is_entered_and_cleared_like_a_generated_one() {
    let (mut w, mut c) = (game(), console());
    let order = tower_and_stack(&mut w, &mut c);
    let at = square(Plane::Arcanus, 2);
    let lair = w.economy().lair_at(at).expect("a lair stands there");
    let gold = w.player(HUMAN).expect("a player").treasury().gold;
    c.execute(&mut w, "mode legal");
    let r = c.execute(&mut w, &order);
    assert!(r.ok, "{}", r.text());
    assert!(
        r.events
            .iter()
            .any(|e| matches!(e, f4x_core::world::Event::BattleStarted { .. })),
        "{}",
        r.text()
    );
    w.world_mut().scenario_edit_battle(|b| {
        for u in b.units.iter_mut().filter(|u| u.side == Side::Defender) {
            u.kill();
            u.status = f4x_battle::unit::Status::Dead;
        }
    });
    let r = c.execute(&mut w, "bend");
    assert!(r.ok, "{}", r.text());
    assert!(
        r.events
            .iter()
            .any(|e| matches!(e, f4x_core::world::Event::SiteCleared { .. })),
        "{}",
        r.text()
    );
    assert!(w.economy().lair_at(at).is_none(), "the tower is cleared");
    assert!(w.player(HUMAN).expect("a player").treasury().gold >= gold + lair.gold as i32);
    let r = c.execute(&mut w, "lairinfo 2,2");
    assert!(r.ok && r.text().contains("cleared"), "{}", r.text());
}

#[test]
fn lairinfo_shows_the_monsters_and_the_treasure_in_the_override_mode() {
    let (mut w, mut c) = (game(), console());
    tower_and_stack(&mut w, &mut c);
    let at = square(Plane::Arcanus, 2);
    let lair = w.economy().lair_at(at).expect("a lair stands there");
    assert!(!lair.guards.is_empty(), "the tower has guards");
    let r = c.execute(&mut w, "lairinfo 2,2");
    assert!(r.ok, "{}", r.text());
    for &(unit, count) in &lair.guards {
        let name = &w.content().rules.units.get(unit).id;
        assert!(
            r.text().contains(&format!("{count} x {name}")),
            "{}",
            r.text()
        );
    }
    assert!(r.text().contains("treasure:"), "{}", r.text());
    for (held, word) in [(lair.gold, "gold"), (lair.mana, "mana")] {
        assert_eq!(
            r.text().contains(&format!("{held} {word}")),
            held > 0,
            "{}",
            r.text()
        );
    }
    let r = c.execute(&mut w, "lairinfo 3,2");
    assert!(matches!(err(&r), ConsoleError::Failed(_)), "{}", r.text());
}

#[test]
fn lairinfo_tells_a_player_only_the_monsters_they_know_and_no_treasure() {
    let (mut w, mut c) = (game(), console());
    let order = tower_and_stack(&mut w, &mut c);
    let at = square(Plane::Arcanus, 2);
    let lair = w.economy().lair_at(at).expect("a lair stands there");
    let names: Vec<String> = lair
        .guards
        .iter()
        .map(|&(unit, _)| w.content().rules.units.get(unit).id.clone())
        .collect();
    c.execute(&mut w, "mode legal");
    // A square no one of the player's has seen.
    let far = w.map().expect("a map");
    let hidden = (0..far.width())
        .map(|x| Position {
            plane: Plane::Myrror,
            x,
            y: far.height() / 2,
        })
        .find(|&p| !w.is_explored(HUMAN, p))
        .expect("an unexplored square");
    let r = c.execute(&mut w, &format!("lairinfo {}", words(hidden)));
    assert!(r.ok && r.text().contains("unexplored"), "{}", r.text());
    // Seen (the stack stands beside it) but never fought for.
    let r = c.execute(&mut w, "lairinfo 2,2");
    assert!(r.ok && r.text().contains("not known"), "{}", r.text());
    assert!(!names.iter().any(|n| r.text().contains(n)), "{}", r.text());
    // Fought for, and left: the monsters are known, the treasure is not told.
    assert!(c.execute(&mut w, &order).ok);
    assert!(c.execute(&mut w, "bflee").ok);
    let r = c.execute(&mut w, "lairinfo 2,2");
    assert!(r.ok, "{}", r.text());
    assert!(names.iter().all(|n| r.text().contains(n)), "{}", r.text());
    assert!(!r.text().contains("treasure"), "{}", r.text());
}

#[test]
fn a_save_and_a_load_keep_what_the_site_cheats_made() -> TestResult {
    let folder = TestDir::new("console-sites");
    let (mut w, mut c) = (game(), console());
    c.session.saves_dir = folder.to_path_buf();
    c.execute(&mut w, "mode override");
    let [site_at, mineral_at, removed_at] = [2, 4, 6].map(|x| square(Plane::Arcanus, x));
    let mineral = w
        .content()
        .features
        .defs()
        .iter()
        .find(|d| {
            d.kind == FeatureKind::Mineral
                && d.spawn.iter().any(|s| s.planes.contains(&Plane::Arcanus))
        })
        .cloned()
        .ok_or("the content has a mineral of Arcanus")?;
    for at in [site_at, removed_at] {
        set_terrain(&mut w, &mut c, at, |t| t == TerrainClass::Grassland);
    }
    set_terrain(&mut w, &mut c, mineral_at, |t| {
        mineral
            .spawn
            .iter()
            .any(|s| s.terrain.contains(&t) && s.planes.contains(&Plane::Arcanus))
    });
    let site = feature_of(&w, FeatureKind::Encounter);
    for line in [
        format!("lair {site} 2,2 strong"),
        format!("feature 4,2 {}", mineral.id),
        format!("lair {site} 6,2"),
        "unlair 6,2".to_string(),
        "save sites".to_string(),
    ] {
        let r = c.execute(&mut w, &line);
        assert!(r.ok, "{line}: {}", r.text());
    }
    let made = w.economy().lair_at(site_at).ok_or("the lair made")?;
    let mut other = game();
    let r = c.execute(&mut other, "load sites");
    assert!(r.ok, "{}", r.text());
    let kept = other
        .economy()
        .lair_at(site_at)
        .ok_or("the lair after a load")?;
    // The entity is the loaded world's own.
    assert_eq!(
        f4x_core::feature::entity::LairInfo {
            entity: made.entity,
            ..kept
        },
        made
    );
    let found = other.economy().feature_at(mineral_at).map(|d| d.num);
    assert_eq!(found, Some(mineral.num));
    assert!(other.economy().feature_entity_at(removed_at).is_none());
    Ok(())
}

#[test]
fn a_hero_is_hired_rerolled_and_dismissed_and_the_treasury_is_read() {
    use f4x_core::hero::HeroState;
    let (mut w, mut c) = (game(), console());
    assert!(
        !c.execute(&mut w, "hire healer").ok,
        "not in the legal mode"
    );
    c.execute(&mut w, "mode override");
    let kind = 7; // healer
    let hero = |w: &World| w.units().hero_entity(kind).expect("the hero exists");
    assert_eq!(w.units().hero_state(hero(&w)), Some(HeroState::Free));
    let r = c.execute(&mut w, "hire healer");
    assert!(r.ok, "{}", r.text());
    assert_eq!(w.units().hero_state(hero(&w)), Some(HeroState::Serving));
    assert!(!c.execute(&mut w, "hire healer").ok, "already in service");
    let r = c.execute(&mut w, "reroll healer");
    assert!(r.ok, "{}", r.text());
    assert_eq!(w.units().hero_state(hero(&w)), Some(HeroState::Serving));
    assert!(
        !c.execute(&mut w, "reroll barbarian_spearmen").ok,
        "not a hero"
    );
    let r = c.execute(&mut w, "dismiss healer");
    assert!(r.ok, "{}", r.text());
    assert_eq!(w.units().hero_state(hero(&w)), Some(HeroState::Free));
    assert!(!c.execute(&mut w, "dismiss healer").ok, "nobody's now");
    let gold = w.player(HUMAN).expect("a player").treasury().gold;
    let r = c.execute(&mut w, "treasury");
    assert!(r.ok && r.text().contains(&gold.to_string()), "{}", r.text());
    assert!(c.execute(&mut w, "treasury 1").ok);
}

#[test]
fn a_hero_is_hired_on_any_square_given_and_else_on_the_fortress() {
    let (mut w, mut c) = (game(), console());
    c.execute(&mut w, "mode override");
    let fortress = w
        .wizard(HUMAN)
        .and_then(|wizard| wizard.fortress())
        .expect("a fortress");
    let kind = 7; // healer
    let at = |w: &World| {
        let hero = w.units().hero_entity(kind).expect("the hero exists");
        w.units().unit_position(hero)
    };
    // Off the map: refused, and the hero stays free.
    assert!(!c.execute(&mut w, "hire healer 9999,9999").ok);
    assert_eq!(at(&w), None);
    // Any square of the map, with or without the player.
    let square = f4x_core::map::Position {
        x: 1,
        y: 1,
        plane: fortress.plane,
    };
    let r = c.execute(
        &mut w,
        &format!(
            "hire healer {} 1,1,{}",
            HUMAN.0,
            if fortress.plane == f4x_core::map::Plane::Arcanus {
                "a"
            } else {
                "m"
            }
        ),
    );
    assert!(r.ok, "{}", r.text());
    assert_eq!(at(&w), Some(square));
    c.execute(&mut w, "dismiss healer");
    let r = c.execute(&mut w, "hire healer");
    assert!(r.ok, "{}", r.text());
    assert_eq!(at(&w), Some(fortress), "no square: the fortress");
}

fn spell_text(w: &World, num: u16) -> (f4x_core::rules::SpellId, String) {
    let rules = &w.content().rules;
    let id = rules.spells.id_numbered(num).expect("the content has it");
    (id, rules.spells.get(id).id.clone())
}

#[test]
fn set_books_changes_the_books_and_the_casting_skill() {
    use f4x_core::rules::RealmId;
    let (mut w, mut c) = (game(), console());
    assert!(
        !c.execute(&mut w, "set books nature 4").ok,
        "needs override"
    );
    assert!(c.execute(&mut w, "mode override").ok);
    let skill = |w: &World| w.wizard(HUMAN).expect("a wizard").skill_points();
    let before = skill(&w);
    assert!(c.execute(&mut w, "set books nature 4").ok);
    assert_eq!(
        w.wizard(HUMAN)
            .expect("a wizard")
            .books_in(RealmId::new(0 /* nature */)),
        4
    );
    assert!(skill(&w) > before, "{} after {before}", skill(&w));
    // Another wizard, by number; none takes the books away.
    assert!(c.execute(&mut w, "set books nature 3 1").ok);
    assert_eq!(
        w.wizard(PlayerId(1))
            .expect("a wizard")
            .books_in(RealmId::new(0 /* nature */)),
        3
    );
    assert_eq!(
        w.wizard(HUMAN)
            .expect("a wizard")
            .books_in(RealmId::new(0 /* nature */)),
        4
    );
    assert!(c.execute(&mut w, "set books nature 0").ok);
    assert_eq!(
        w.wizard(HUMAN)
            .expect("a wizard")
            .books_in(RealmId::new(0 /* nature */)),
        0
    );
    assert_eq!(skill(&w), before);
    // More than the game allows is cut to the most.
    assert!(c.execute(&mut w, "set books nature 200").ok);
    assert_eq!(
        w.wizard(HUMAN)
            .expect("a wizard")
            .books_in(RealmId::new(0 /* nature */)),
        i32::from(w.settings().wizards.max_books_per_realm)
    );
    for bad in [
        "set books",
        "set books nowhere 3",
        "set books nature x",
        "set tomes nature 3",
    ] {
        assert!(!c.execute(&mut w, bad).ok, "{bad}");
    }
}

#[test]
fn a_retort_is_given_and_taken_away_and_the_skill_follows() {
    use f4x_core::wizard::magic::Retort;
    let (mut w, mut c) = (game(), console());
    assert!(
        !c.execute(&mut w, "give retort channeler").ok,
        "needs override"
    );
    assert!(c.execute(&mut w, "mode override").ok);
    assert!(!w
        .wizard(HUMAN)
        .expect("a wizard")
        .has(Retort::new(8 /* channeler */)));
    assert!(c.execute(&mut w, "give retort channeler").ok);
    assert!(w
        .wizard(HUMAN)
        .expect("a wizard")
        .has(Retort::new(8 /* channeler */)));
    // Giving it twice, or taking away one that is not there, says so.
    assert!(!c.execute(&mut w, "give retort channeler").ok);
    assert!(c.execute(&mut w, "remove retort channeler").ok);
    assert!(!w
        .wizard(HUMAN)
        .expect("a wizard")
        .has(Retort::new(8 /* channeler */)));
    assert!(!c.execute(&mut w, "remove retort channeler").ok);
    // A name with two words, and another wizard.
    assert!(c.execute(&mut w, "give retort chaos mastery 1").ok);
    assert!(w
        .wizard(PlayerId(1))
        .expect("a wizard")
        .has(Retort::new(2 /* chaos_mastery */)));
    for bad in [
        "give retort",
        "give retort nothing_like_it",
        "remove retort",
        "remove tome x",
    ] {
        assert!(!c.execute(&mut w, bad).ok, "{bad}");
    }
}

#[test]
fn a_spell_is_given_and_taken_away() {
    let (mut w, mut c) = (game(), console());
    let (spell, name) = spell_text(&w, 12 /* nature_s_eye */);
    assert!(
        !c.execute(&mut w, &format!("give spell {name}")).ok,
        "needs override"
    );
    assert!(c.execute(&mut w, "mode override").ok);
    let knows = |w: &World, p| w.wizard(p).expect("a wizard").known().contains(&spell);
    assert!(!knows(&w, HUMAN));
    assert!(c.execute(&mut w, &format!("give spell {name}")).ok);
    assert!(knows(&w, HUMAN));
    assert!(c.execute(&mut w, &format!("remove spell {name}")).ok);
    assert!(!knows(&w, HUMAN));
    // Taking away one that is not known says so; another wizard, by number.
    assert!(!c.execute(&mut w, &format!("remove spell {name}")).ok);
    assert!(c.execute(&mut w, &format!("give spell {name} 1")).ok);
    assert!(knows(&w, PlayerId(1)) && !knows(&w, HUMAN));
    assert!(!c.execute(&mut w, "give spell no_such_spell").ok);
    // Each use is in the log.
    let log = w.override_log().join("\n");
    assert!(
        log.contains("give spell") && log.contains("remove spell"),
        "{log}"
    );
}
