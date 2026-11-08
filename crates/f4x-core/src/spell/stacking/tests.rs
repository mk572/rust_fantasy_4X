use super::*;
use crate::game::testutil::{num, shipped};
use crate::game::Command;
use crate::map::{Grid, Map, Plane, Position};
use crate::player::PlayerId;
use crate::rules::Stat;
use f4x_def::testdir::TestResult;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
const AT: Position = Position {
    plane: Plane::Arcanus,
    x: 5,
    y: 5,
};

fn world() -> Game {
    let c = shipped();
    let grass = c
        .terrains
        .id_numbered(num::terrain::GRASSLAND)
        .expect("id_of succeeds");
    let mut g = Game::new(3);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    g.fog = crate::map::fog::Fog::new(24, 14);
    g.add_player(P0, "W");
    g.add_player(P1, "W");
    g
}

fn unit(g: &mut Game, id: u16) -> hecs::Entity {
    let id = g
        .content
        .rules
        .units
        .id_numbered(id)
        .expect("id_of succeeds");
    g.spawn_unit(P0, AT, id)
}

fn spell(g: &Game, id: u16) -> SpellId {
    g.content
        .rules
        .spells
        .id_numbered(id)
        .expect("id_of succeeds")
}

fn enchant(g: &mut Game, u: hecs::Entity, id: u16) {
    let s = spell(g, id);
    g.attach_enchantment(Target::Unit(u), s, P0);
}

fn aim(g: &Game, u: hecs::Entity, id: u16) -> Result<(), MagicError> {
    g.check_target(P0, spell(g, id), Some(Target::Unit(u)))
}

#[test]
fn a_unit_takes_a_lasting_spell_once() {
    let mut g = world();
    let (a, b) = (
        unit(&mut g, 107 /* high_men_cavalry */),
        unit(&mut g, 107 /* high_men_cavalry */),
    );
    assert_eq!(aim(&g, a, 4 /* giant_strength */), Ok(()));
    enchant(&mut g, a, 4 /* giant_strength */);
    assert_eq!(
        aim(&g, a, 4 /* giant_strength */),
        Err(MagicError::AlreadyEnchanted)
    );
    assert_eq!(aim(&g, b, 4 /* giant_strength */), Ok(()));
    assert_eq!(aim(&g, a, 56 /* flight */), Ok(()));
}

#[test]
fn a_spell_that_gives_way_to_a_stronger_one_is_refused_beside_it_but_not_the_other_way() {
    let mut g = world();
    let u = unit(&mut g, 107 /* high_men_cavalry */);
    let base = g.units().unit_stat(u, Stat::Defense);
    enchant(&mut g, u, 7 /* stone_skin */);
    assert_eq!(g.units().unit_stat(u, Stat::Defense), base + 1);
    // Iron Skin goes on top and replaces it, not adds to it.
    assert_eq!(aim(&g, u, 24 /* iron_skin */), Ok(()));
    enchant(&mut g, u, 24 /* iron_skin */);
    assert_eq!(g.units().unit_stat(u, Stat::Defense), base + 5);
    assert_eq!(
        aim(&g, u, 7 /* stone_skin */),
        Err(MagicError::AlreadyEnchanted)
    );
    // Without the Stone Skin on it, a unit with Iron Skin is refused Stone Skin all the same.
    let v = unit(&mut g, 107 /* high_men_cavalry */);
    enchant(&mut g, v, 24 /* iron_skin */);
    assert_eq!(
        aim(&g, v, 7 /* stone_skin */),
        Err(MagicError::NotCumulative)
    );
    assert_eq!(g.units().unit_stat(v, Stat::Defense), base + 5);
}

#[test]
fn resist_elements_gives_way_to_elemental_armor_and_resist_magic_to_magic_immunity() {
    let mut g = world();
    let u = unit(&mut g, 107 /* high_men_cavalry */);
    enchant(&mut g, u, 21 /* elemental_armor */);
    assert_eq!(
        aim(&g, u, 2 /* resist_elements */),
        Err(MagicError::NotCumulative)
    );
    let v = unit(&mut g, 107 /* high_men_cavalry */);
    enchant(&mut g, v, 69 /* magic_immunity */);
    assert_eq!(
        aim(&g, v, 41 /* resist_magic */),
        Err(MagicError::NotCumulative)
    );
    // Magic Immunity may go on a unit with Resist Magic, which then adds nothing.
    let w = unit(&mut g, 107 /* high_men_cavalry */);
    let base = g
        .units()
        .unit_resistance_against(w, Some(crate::battle::immunity::Realm::Life));
    enchant(&mut g, w, 41 /* resist_magic */);
    assert_eq!(
        g.units()
            .unit_resistance_against(w, Some(crate::battle::immunity::Realm::Life)),
        base + 5
    );
    assert_eq!(aim(&g, w, 69 /* magic_immunity */), Ok(()));
    enchant(&mut g, w, 69 /* magic_immunity */);
    assert_eq!(
        g.units()
            .unit_resistance_against(w, Some(crate::battle::immunity::Realm::Life)),
        base + 30
    );
}

#[test]
fn a_unit_cannot_be_given_what_it_is_by_nature() {
    let mut g = world();
    let spirit = unit(&mut g, 154 /* magic_spirit */);
    let mage = unit(&mut g, 15 /* wind_mage */);
    let traveller = unit(&mut g, 171 /* shadow_demons */);
    let walker = unit(&mut g, 107 /* high_men_cavalry */);
    assert_eq!(
        aim(&g, spirit, 181 /* wraith_form */),
        Err(MagicError::AlreadyAble)
    );
    assert_eq!(
        aim(&g, mage, 63 /* wind_walking */),
        Err(MagicError::AlreadyAble)
    );
    assert_eq!(
        aim(&g, traveller, 138 /* planar_travel */),
        Err(MagicError::AlreadyAble)
    );
    assert_eq!(aim(&g, walker, 181 /* wraith_form */), Ok(()));
    assert_eq!(aim(&g, walker, 63 /* wind_walking */), Ok(()));
    // A spell grants the ability to others, which is not the same as having it by nature.
    assert_eq!(aim(&g, spirit, 56 /* flight */), Ok(()));
}

#[test]
fn holy_weapon_is_refused_while_the_wizard_keeps_holy_arms() -> TestResult {
    let mut g = world();
    let u = unit(&mut g, 107 /* high_men_cavalry */);
    assert_eq!(aim(&g, u, 124 /* holy_weapon */), Ok(()));
    let arms = spell(&g, 154 /* holy_arms */);
    g.add_global(P0, arms);
    assert_eq!(
        aim(&g, u, 124 /* holy_weapon */),
        Err(MagicError::NotCumulative)
    );
    // Another wizard's Holy Arms does not matter.
    let other = g.spawn_unit(
        P1,
        AT,
        g.content
            .rules
            .units
            .id_numbered(107)
            .ok_or("high_men_cavalry is not in the content")?,
    );
    assert_eq!(
        g.check_target(
            P1,
            spell(&g, 124 /* holy_weapon */),
            Some(Target::Unit(other))
        ),
        Ok(())
    );
    Ok(())
}

#[test]
fn a_city_takes_a_lasting_spell_once() -> TestResult {
    let mut g = world();
    let race = g.content.rules.races.id_numbered(7);
    g.apply(Command::FoundCity {
        owner: P0,
        pos: AT,
        name: "Town".into(),
        race,
        population: 4,
    });
    let city = g.economy().city_at(AT).ok_or("city_at is missing")?;
    let fire = spell(&g, 87 /* wall_of_fire */);
    assert_eq!(g.check_target(P0, fire, Some(Target::City(city))), Ok(()));
    g.add_city_enchantment(city, fire, P0);
    assert_eq!(
        g.check_target(P0, fire, Some(Target::City(city))),
        Err(MagicError::AlreadyEnchanted)
    );
    let dark = spell(&g, 178 /* wall_of_darkness */);
    assert_eq!(g.check_target(P0, dark, Some(Target::City(city))), Ok(()));
    Ok(())
}

#[test]
fn an_instant_spell_is_never_refused_for_stacking() {
    let mut g = world();
    let u = unit(&mut g, 107 /* high_men_cavalry */);
    assert_eq!(aim(&g, u, 125 /* healing */), Ok(()));
}

#[test]
fn a_spell_whose_effects_the_unit_has_already_is_pointless_but_may_be_confirmed() {
    let mut g = world();
    let u = unit(&mut g, 107 /* high_men_cavalry */);
    enchant(&mut g, u, 165 /* black_sleep: holds the unit */);
    // Confusion also holds the unit, so it would add nothing.
    let by_enemy =
        |g: &Game| g.check_target(P1, spell(g, 46 /* confusion */), Some(Target::Unit(u)));
    assert_eq!(by_enemy(&g), Err(MagicError::AlreadyEnchanted));
    assert!(MagicError::AlreadyEnchanted.is_pointless());
    g.confirmed = true;
    assert_eq!(by_enemy(&g), Ok(()));
}

#[test]
fn a_confirmation_does_not_lift_a_refusal_that_is_more_than_pointless() {
    let mut g = world();
    let u = unit(&mut g, 107 /* high_men_cavalry */);
    enchant(&mut g, u, 24 /* iron_skin */);
    g.confirmed = true;
    assert_eq!(
        aim(&g, u, 7 /* stone_skin */),
        Err(MagicError::NotCumulative)
    );
}

#[test]
fn only_the_confirmed_command_goes_through() {
    let mut g = world();
    let u = unit(&mut g, 107 /* high_men_cavalry */);
    enchant(&mut g, u, 4 /* giant_strength */);
    let cast = Command::CastSpell {
        player: P0,
        spell: spell(&g, 4),
        target: Some(Target::Unit(u)),
        extra: 0,
    };
    // The check the command runs first, with and without the confirmation.
    assert_eq!(
        g.check_target(P0, spell(&g, 4), Some(Target::Unit(u)))
            .err()
            .filter(|e| e.is_pointless()),
        Some(MagicError::AlreadyEnchanted)
    );
    let events = g.apply(Command::Anyway(Box::new(cast)));
    assert!(!g.confirmed, "the confirmation lasts for one command");
    // Whatever else it meets (no such spell known), it is not the pointless-cast refusal.
    assert!(!events.iter().any(|e| matches!(
        e,
        crate::game::Event::Rejected(crate::game::Rejection::Magic(MagicError::AlreadyEnchanted))
    )));
}

#[test]
fn the_normal_unit_spells_take_a_normal_unit_or_a_hero_and_not_a_creature() {
    let mut g = world();
    let normal = unit(&mut g, 107 /* high_men_cavalry */);
    let hero = unit(&mut g, 0 /* dwarf */);
    let bears = unit(&mut g, 179 /* war_bears */);
    for spell in [
        86,  /* eldritch_weapon */
        94,  /* flame_blade */
        124, /* holy_weapon */
        126, /* holy_armor */
        130, /* heroism */
    ] {
        assert_eq!(
            aim(&g, normal, spell),
            Ok(()),
            "spell {spell} on a normal unit"
        );
        assert_eq!(aim(&g, hero, spell), Ok(()), "spell {spell} on a hero");
        assert_eq!(
            aim(&g, bears, spell),
            Err(MagicError::Target(TargetError::NotNormalOrHero)),
            "spell {spell} on a creature"
        );
    }
    // The others still take a creature.
    assert_eq!(aim(&g, bears, 4 /* giant_strength */), Ok(()));
}

#[test]
fn torin_is_a_hero_and_a_life_creature_and_takes_the_spell_when_either_passes() {
    let mut g = world();
    let torin = unit(&mut g, 34 /* chosen */);
    let def = g.units().unit_def(torin).expect("Torin has a definition");
    assert!(def.realm.is_some(), "Torin is a creature of a realm");
    assert_eq!(def.kind, crate::rules::UnitKind::Hero);
    assert_eq!(aim(&g, torin, 126 /* holy_armor */), Ok(()));
    assert_eq!(aim(&g, torin, 130 /* heroism */), Ok(()));
}

#[test]
fn undead_are_not_normal_units() {
    assert_eq!(
        unit_only_refusal(UnitOnly::NormalOrHero, UnitKind::Normal, true),
        Some(TargetError::NotNormalOrHero)
    );
    assert_eq!(
        unit_only_refusal(UnitOnly::Any, UnitKind::Fantastic, true),
        None
    );
}

#[test]
fn chaos_channels_takes_a_hero_but_not_a_creature() {
    let mut g = world();
    let hero = unit(&mut g, 0 /* dwarf */);
    let bears = unit(&mut g, 179 /* war_bears */);
    assert_eq!(aim(&g, hero, 93 /* chaos_channels */), Ok(()));
    assert_eq!(
        aim(&g, bears, 93 /* chaos_channels */),
        Err(MagicError::Target(TargetError::NotNormalUnit))
    );
}

#[test]
fn resist_elements_gives_way_to_wraith_form() {
    let mut g = world();
    let u = unit(&mut g, 107 /* high_men_cavalry */);
    enchant(&mut g, u, 181 /* wraith_form */);
    assert_eq!(
        aim(&g, u, 2 /* resist_elements */),
        Err(MagicError::NotCumulative)
    );
}
