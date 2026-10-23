use super::*;
use crate::game::testutil::content_with_magic;
use crate::game::Command;
use crate::map::{Grid, Map, Plane, Position};
use crate::player::PlayerId;
use crate::rules::Stat;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
const AT: Position = Position {
    plane: Plane::Arcanus,
    x: 5,
    y: 5,
};

fn world() -> Game {
    let c = content_with_magic();
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut g = Game::new(3);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    g.fog = crate::map::fog::Fog::new(24, 14);
    g.add_player(P0, "W");
    g.add_player(P1, "W");
    g
}

fn unit(g: &mut Game, id: &str) -> hecs::Entity {
    let id = g.content.rules.units.id_of(id).unwrap();
    g.spawn_unit(P0, AT, id)
}

fn spell(g: &Game, id: &str) -> SpellId {
    g.content.rules.spells.id_of(id).unwrap()
}

fn enchant(g: &mut Game, u: hecs::Entity, id: &str) {
    let s = spell(g, id);
    g.attach_enchantment(Target::Unit(u), s, P0);
}

fn aim(g: &Game, u: hecs::Entity, id: &str) -> Result<(), MagicError> {
    g.check_target(P0, spell(g, id), Some(Target::Unit(u)))
}

#[test]
fn a_unit_takes_a_lasting_spell_once() {
    let mut g = world();
    let (a, b) = (unit(&mut g, "walker"), unit(&mut g, "walker"));
    assert_eq!(aim(&g, a, "giant_strength"), Ok(()));
    enchant(&mut g, a, "giant_strength");
    assert_eq!(
        aim(&g, a, "giant_strength"),
        Err(MagicError::AlreadyEnchanted)
    );
    assert_eq!(aim(&g, b, "giant_strength"), Ok(()));
    assert_eq!(aim(&g, a, "flight"), Ok(()));
}

#[test]
fn a_spell_that_gives_way_to_a_stronger_one_is_refused_beside_it_but_not_the_other_way() {
    let mut g = world();
    let u = unit(&mut g, "walker");
    let base = g.units().unit_stat(u, Stat::Defense);
    enchant(&mut g, u, "stone_skin");
    assert_eq!(g.units().unit_stat(u, Stat::Defense), base + 1);
    // Iron Skin goes on top and replaces it, not adds to it.
    assert_eq!(aim(&g, u, "iron_skin"), Ok(()));
    enchant(&mut g, u, "iron_skin");
    assert_eq!(g.units().unit_stat(u, Stat::Defense), base + 5);
    assert_eq!(aim(&g, u, "stone_skin"), Err(MagicError::AlreadyEnchanted));
    // Without the Stone Skin on it, a unit with Iron Skin is refused Stone Skin all the same.
    let v = unit(&mut g, "walker");
    enchant(&mut g, v, "iron_skin");
    assert_eq!(aim(&g, v, "stone_skin"), Err(MagicError::NotCumulative));
    assert_eq!(g.units().unit_stat(v, Stat::Defense), base + 5);
}

#[test]
fn resist_elements_gives_way_to_elemental_armor_and_resist_magic_to_magic_immunity() {
    let mut g = world();
    let u = unit(&mut g, "walker");
    enchant(&mut g, u, "elemental_armor");
    assert_eq!(
        aim(&g, u, "resist_elements"),
        Err(MagicError::NotCumulative)
    );
    let v = unit(&mut g, "walker");
    enchant(&mut g, v, "magic_immunity");
    assert_eq!(aim(&g, v, "resist_magic"), Err(MagicError::NotCumulative));
    // Magic Immunity may go on a unit with Resist Magic, which then adds nothing.
    let w = unit(&mut g, "walker");
    let base = g
        .units()
        .unit_resistance_against(w, Some(crate::battle::immunity::Realm::Life));
    enchant(&mut g, w, "resist_magic");
    assert_eq!(
        g.units()
            .unit_resistance_against(w, Some(crate::battle::immunity::Realm::Life)),
        base + 5
    );
    assert_eq!(aim(&g, w, "magic_immunity"), Ok(()));
    enchant(&mut g, w, "magic_immunity");
    assert_eq!(
        g.units()
            .unit_resistance_against(w, Some(crate::battle::immunity::Realm::Life)),
        base + 30
    );
}

#[test]
fn a_unit_cannot_be_given_what_it_is_by_nature() {
    let mut g = world();
    let spirit = unit(&mut g, "spirit");
    let mage = unit(&mut g, "mage");
    let traveller = unit(&mut g, "traveller");
    let walker = unit(&mut g, "walker");
    assert_eq!(aim(&g, spirit, "wraith_form"), Err(MagicError::AlreadyAble));
    assert_eq!(aim(&g, mage, "wind_walking"), Err(MagicError::AlreadyAble));
    assert_eq!(
        aim(&g, traveller, "planar_travel"),
        Err(MagicError::AlreadyAble)
    );
    assert_eq!(aim(&g, walker, "wraith_form"), Ok(()));
    assert_eq!(aim(&g, walker, "wind_walking"), Ok(()));
    // A spell grants the ability to others, which is not the same as having it by nature.
    assert_eq!(aim(&g, spirit, "flight"), Ok(()));
}

#[test]
fn holy_weapon_is_refused_while_the_wizard_keeps_holy_arms() {
    let mut g = world();
    let u = unit(&mut g, "walker");
    assert_eq!(aim(&g, u, "holy_weapon"), Ok(()));
    let arms = spell(&g, "holy_arms");
    g.add_global(P0, arms);
    assert_eq!(aim(&g, u, "holy_weapon"), Err(MagicError::NotCumulative));
    // Another wizard's Holy Arms does not matter.
    let other = g.spawn_unit(P1, AT, g.content.rules.units.id_of("walker").unwrap());
    assert_eq!(
        g.check_target(P1, spell(&g, "holy_weapon"), Some(Target::Unit(other))),
        Ok(())
    );
}

#[test]
fn a_city_takes_a_lasting_spell_once() {
    let mut g = world();
    let race = g.content.rules.races.id_of("elf");
    g.apply(Command::FoundCity {
        owner: P0,
        pos: AT,
        name: "Town".into(),
        race,
        population: 4,
    });
    let city = g.economy().city_at(AT).unwrap();
    let fire = spell(&g, "wall_of_fire");
    assert_eq!(g.check_target(P0, fire, Some(Target::City(city))), Ok(()));
    g.add_city_enchantment(city, fire, P0);
    assert_eq!(
        g.check_target(P0, fire, Some(Target::City(city))),
        Err(MagicError::AlreadyEnchanted)
    );
    let dark = spell(&g, "wall_of_darkness");
    assert_eq!(g.check_target(P0, dark, Some(Target::City(city))), Ok(()));
}

#[test]
fn an_instant_spell_is_never_refused_for_stacking() {
    let mut g = world();
    let u = unit(&mut g, "walker");
    assert_eq!(aim(&g, u, "healing"), Ok(()));
}
