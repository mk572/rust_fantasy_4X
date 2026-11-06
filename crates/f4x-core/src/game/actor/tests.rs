use super::*;
use crate::game::testutil::shipped;
use crate::map::{Grid, Map, Plane, Position};
use f4x_def::testdir::TestResult;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

#[test]
fn a_command_names_the_player_it_acts_for() -> TestResult {
    let c = shipped();
    let grass = c
        .terrains
        .id_numbered(crate::game::testutil::num::terrain::GRASSLAND)
        .ok_or("no grass")?;
    let mut g = Game::new(3);
    g.content = c;
    g.map = Some(Map::new(Grid::new(8, 8, grass), Grid::new(8, 8, grass)));
    g.add_player(P0, "A");
    g.add_player(P1, "B");
    let at = Position {
        plane: Plane::Arcanus,
        x: 2,
        y: 2,
    };
    let dwarf = g.content.rules.units.id_numbered(0).ok_or("no dwarf")?;
    let theirs = g.spawn_unit(P1, at, dwarf);
    assert_eq!(
        g.command_actor(&Command::MoveStack {
            units: vec![theirs],
            to: at
        }),
        Some(P1)
    );
    assert_eq!(
        g.command_actor(&Command::SetTaxRate {
            player: P0,
            rate: 1
        }),
        Some(P0)
    );
    assert_eq!(g.command_actor(&Command::EndTurn), None);
    Ok(())
}
