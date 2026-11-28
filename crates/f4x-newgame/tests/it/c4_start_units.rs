//! The settings `starting_units` (`docs/game-settings.md` 4.2): the settlers, the Magic Spirit and
//! the Trireme a wizard may start with, off by default.

use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_core::world::setting::StartingUnits;
use f4x_core::world::World;
use f4x_def::testdir::{shipped, TestResult};
use f4x_newgame::setup::{new_game, NewGame};

const WIZARDS: u8 = 3;

fn start(seed: u64, units: StartingUnits) -> Result<World, Box<dyn std::error::Error>> {
    let mut spec = NewGame::with_wizards(WIZARDS);
    spec.seed = seed;
    spec.all_ai = true;
    spec.settings.starting_units = units;
    Ok(new_game(shipped(), &spec)?.0)
}

/// The wizard's units as `(unit number, square)`, sorted.
fn army(w: &World, player: PlayerId) -> Vec<(u16, Position)> {
    let units = w.units();
    let mut out: Vec<_> = units
        .player_units(player)
        .into_iter()
        .filter_map(|u| Some((units.unit_def(u)?.num, units.unit_position(u)?)))
        .collect();
    out.sort_by_key(|&(num, at)| (num, at.y, at.x));
    out
}

#[test]
fn the_setting_is_off_in_a_new_game() {
    assert_eq!(
        NewGame::default().settings.starting_units,
        StartingUnits::default()
    );
}

#[test]
fn each_setting_adds_its_unit_to_every_wizard_and_nothing_else() -> TestResult {
    let rules = &shipped().rules;
    let (spirit, ship) = (
        rules.config.wizard.start_spirit,
        rules.config.wizard.start_ship,
    );
    let all = StartingUnits {
        settler: true,
        magic_spirit: true,
        trireme: true,
    };
    let (mut with_ship, mut without_ship) = (0, 0);
    for seed in 0..12 {
        let plain = start(seed, StartingUnits::default())?;
        let full = start(seed, all)?;
        for p in (0..WIZARDS).map(PlayerId) {
            let at = full
                .wizard(p)
                .and_then(|w| w.fortress())
                .ok_or("no fortress")?;
            let race = full
                .wizard(p)
                .and_then(|w| w.capital_race())
                .ok_or("no race")?;
            let before = army(&plain, p);
            let mut added = army(&full, p);
            for unit in &before {
                let i = added
                    .iter()
                    .position(|u| u == unit)
                    .ok_or_else(|| format!("seed {seed}, {p:?}: {unit:?} is gone"))?;
                added.remove(i);
            }
            let coastal = full
                .economy()
                .city_at(at)
                .is_some_and(|c| full.economy().city_is_coastal(c));
            let (ships, ashore): (Vec<_>, Vec<_>) =
                added.into_iter().partition(|&(num, _)| num == ship);
            assert_eq!(
                ships.len(),
                usize::from(coastal),
                "seed {seed}, {p:?}: coastal {coastal}"
            );
            for (_, sea) in ships {
                let terrain = full
                    .map()
                    .and_then(|m| m.plane(sea.plane).get(sea.x as i32, sea.y as i32))
                    .and_then(|t| full.content().terrains.get(t))
                    .ok_or("no terrain")?;
                assert_eq!(terrain.class, TerrainClass::Ocean, "seed {seed}: {sea:?}");
                assert_eq!(sea.plane, at.plane);
                let width = full.map().ok_or("no map")?.width();
                let across = sea.x.abs_diff(at.x);
                assert!(across.min(width - across) <= 1 && sea.y.abs_diff(at.y) <= 1);
            }
            assert_eq!(ashore.len(), 2, "seed {seed}, {p:?}: {ashore:?}");
            assert!(ashore.iter().all(|&(_, pos)| pos == at));
            assert!(ashore.iter().any(|&(num, _)| num == spirit));
            assert!(ashore.iter().any(|&(num, _)| {
                rules
                    .units
                    .by_num(num)
                    .is_some_and(|d| d.is_settler() && d.race.is_some_and(|r| r == race))
            }));
            if coastal {
                with_ship += 1;
            } else {
                without_ship += 1;
            }
        }
    }
    assert!(
        with_ship > 0 && without_ship > 0,
        "{with_ship} {without_ship}"
    );
    Ok(())
}

#[test]
fn one_setting_alone_adds_one_unit() -> TestResult {
    let none = StartingUnits::default();
    let settler = StartingUnits {
        settler: true,
        ..none
    };
    let spirit = StartingUnits {
        magic_spirit: true,
        ..none
    };
    for p in (0..WIZARDS).map(PlayerId) {
        let plain = army(&start(1, none)?, p).len();
        for units in [settler, spirit] {
            assert_eq!(army(&start(1, units)?, p).len(), plain + 1, "{units:?}");
        }
    }
    Ok(())
}

#[test]
fn every_race_has_settlers_to_start_with() {
    let rules = &shipped().rules;
    for race in rules.races.defs() {
        assert!(
            race.units.iter().any(|&u| rules.units.get(u).is_settler()),
            "{}",
            race.id
        );
    }
}
