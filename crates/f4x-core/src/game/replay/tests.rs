use super::*;
use crate::game::testutil::content_with_units;
use crate::map::{Grid, Map, Plane, Position};
use crate::player::PlayerId;
use crate::testdir::TestResult;

const P0: PlayerId = PlayerId(0);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

/// A small explored world with one walker at (5, 5).
fn world() -> (Game, Serial) {
    let c = content_with_units();
    let grass = c.terrains.id_of("grassland").expect("id_of succeeds");
    let mut g = Game::new(3);
    g.content = c;
    g.map = Some(Map::new(Grid::new(20, 10, grass), Grid::new(20, 10, grass)));
    g.fog = crate::map::fog::Fog::new(20, 10);
    g.add_player(P0, "W");
    let walker = g
        .content
        .rules
        .units
        .id_of("dwarf")
        .expect("id_of succeeds");
    g.treasury_mut(P0).expect("treasury_mut succeeds").gold = 1000;
    let u = g.spawn_unit(P0, at(5, 5), walker);
    g.fog.explore(P0, at(10, 5), 25, false);
    let serial = g.serial_of(u);
    (g, serial)
}

fn walk(g: &mut Game, s: Serial, to: Position) {
    let u = g.units().entity_of_serial(s).expect("the walker is there");
    g.apply(Command::MoveStack { units: vec![u], to });
}

fn state(g: &Game) -> String {
    g.snapshot().to_text().expect("to_text succeeds")
}

/// Play some turns, with an undo and a refused command among them.
fn play_some(g: &mut Game, s: Serial) {
    walk(g, s, at(6, 5));
    walk(g, s, at(7, 5));
    g.undo().expect("undo succeeds");
    walk(g, s, at(6, 6));
    walk(g, s, at(18, 9)); // refused: too far
    g.apply(Command::SetTaxRate {
        player: P0,
        rate: 3,
    });
    g.apply(Command::EndTurn);
    walk(g, s, at(7, 7));
}

#[test]
fn a_recorded_game_plays_again_to_the_same_state() -> TestResult {
    let (mut g, s) = world();
    g.set_undo(true);
    g.start_recording().map_err(|e| format!("{e:?}"))?;
    play_some(&mut g, s);
    let replay = g.stop_recording().ok_or("stop_recording is missing")?;
    assert_eq!(
        replay.commands.len(),
        5,
        "the refused move and the undone one are not in the record: {:?}",
        replay.commands
    );
    let (again, events) = replay
        .play(g.content.clone(), usize::MAX)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(events.len(), 5);
    assert_eq!(state(&again), state(&g));
    Ok(())
}

#[test]
fn a_replay_survives_being_written_and_read() -> TestResult {
    let (mut g, s) = world();
    g.set_undo(true);
    g.start_recording().map_err(|e| format!("{e:?}"))?;
    play_some(&mut g, s);
    let replay = g.stop_recording().ok_or("stop_recording is missing")?;
    let text = replay.to_text()?;
    let back = Replay::from_text(&text).map_err(|e| format!("{e:?}"))?;
    let (again, _) = back
        .play(g.content.clone(), usize::MAX)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(state(&again), state(&g));
    Ok(())
}

#[test]
fn commands_find_their_units_after_a_load_changed_the_entities() -> TestResult {
    let (mut g, s) = world();
    // Make some entities come and go so that a rebuilt world numbers them differently.
    let walker = g
        .content
        .rules
        .units
        .id_of("dwarf")
        .ok_or("dwarf is not in the content")?;
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold = 1000;
    let spare = g.spawn_unit(P0, at(1, 1), walker);
    let _ = g.world.despawn(spare);
    g.set_undo(true);
    g.start_recording().map_err(|e| format!("{e:?}"))?;
    play_some(&mut g, s);
    let replay = g.replay_so_far().map_err(|e| format!("{e:?}"))?;
    let (again, _) = replay
        .play(g.content.clone(), usize::MAX)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(state(&again), state(&g));
    assert!(again.is_recording(), "the replayed game goes on recording");
    assert_eq!(
        again
            .replay_so_far()
            .map_err(|e| format!("{e:?}"))?
            .commands,
        replay.commands
    );
    Ok(())
}

#[test]
fn a_replay_can_stop_part_way() -> TestResult {
    let (mut g, s) = world();
    g.start_recording().map_err(|e| format!("{e:?}"))?;
    walk(&mut g, s, at(6, 5));
    let one_step = state(&g);
    walk(&mut g, s, at(7, 5));
    let replay = g.stop_recording().ok_or("stop_recording is missing")?;
    let (half, _) = replay
        .play(g.content.clone(), 1)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(state(&half), one_step);
    Ok(())
}

#[test]
fn recording_stops_when_asked() -> TestResult {
    let (mut g, _) = world();
    assert_eq!(g.replay_so_far().unwrap_err(), ReplayError::NotRecording);
    assert!(g.stop_recording().is_none());
    g.start_recording().map_err(|e| format!("{e:?}"))?;
    assert!(g.is_recording());
    assert!(g.stop_recording().is_some());
    assert!(!g.is_recording());
    Ok(())
}

#[test]
fn a_command_naming_nothing_is_reported() -> TestResult {
    let (mut g, s) = world();
    g.start_recording().map_err(|e| format!("{e:?}"))?;
    walk(&mut g, s, at(6, 5));
    let mut replay = g.stop_recording().ok_or("stop_recording is missing")?;
    replay.commands[0] = replay.commands[0].replace(&format!("[{}]", s.0), "[999]");
    assert_eq!(
        replay.play(g.content.clone(), usize::MAX).err(),
        Some(ReplayError::Unresolved(0))
    );
    Ok(())
}

#[test]
fn a_spell_in_a_command_is_written_by_its_id_and_read_back() -> TestResult {
    let (mut g, _) = world();
    g.content.rules.spells = crate::rules::Registry::new(crate::game::testutil::real_spells());
    let spells = &g.content.rules.spells;
    let first = spells.defs().first().expect("the test content has spells");
    let spell = spells.id_of(&first.id).ok_or("id_of is missing")?;
    let id = spells.get(spell).id.clone();
    let cast = Command::Battle(crate::battle::BattleCommand::Cast {
        spell,
        aim: crate::battle::SpellAim::None,
        mana: 0,
    });
    g.load_context();
    let text = ron::ser::to_string(&cast)?;
    assert!(text.contains(&format!("\"{id}\"")), "{text}");
    assert_eq!(ron::from_str::<Command>(&text)?, cast);
    Ok(())
}
