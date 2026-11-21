use super::*;

#[test]
fn end_turn_advances_turn() {
    let mut g = Game::new(1);
    assert_eq!(
        g.apply(Command::EndTurn),
        vec![Event::TurnStarted { turn: 2 }]
    );
}

#[test]
fn same_seed_same_rng_stream() {
    let (mut a, mut b) = (Game::new(42), Game::new(42));
    assert_eq!((a.next_u64(), a.next_u64()), (b.next_u64(), b.next_u64()));
}

#[derive(Debug, PartialEq)]
struct Mark(u32);

#[test]
fn put_gives_and_replaces_a_component_and_strip_takes_it_off() {
    let mut g = Game::new(1);
    let e = g.world.spawn(());
    g.put(e, Mark(1));
    g.put(e, Mark(2));
    assert_eq!(g.world.get::<&Mark>(e).map(|m| m.0), Ok(2));
    g.strip::<Mark>(e);
    assert!(g.world.get::<&Mark>(e).is_err());
    // A component that is not there is fine.
    g.strip::<Mark>(e);
    g.put_bundle(e, (Mark(3), 7u8));
    assert_eq!(g.world.get::<&u8>(e).map(|m| *m), Ok(7));
}

// The helpers exist so that an entity that is gone is noticed; a build without debug assertions
// goes on, as the ignored result did.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "no such entity to give a component to")]
fn put_on_an_entity_that_is_gone_stops_a_debug_build() {
    let mut g = Game::new(1);
    let e = g.world.spawn(());
    g.world.despawn(e).ok();
    g.put(e, Mark(1));
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "no such entity to take a component off")]
fn strip_on_an_entity_that_is_gone_stops_a_debug_build() {
    let mut g = Game::new(1);
    let e = g.world.spawn((Mark(1),));
    g.world.despawn(e).ok();
    g.strip::<Mark>(e);
}

/// The setting `cavalry_halberdier_abilities`, on the shipped units: on, every cavalry unit has
/// First Strike and every halberdier unit Negate First Strike, the units' other abilities stay
/// and no other unit changes; off, the rows are as shipped.
#[test]
fn the_cavalry_halberdier_setting_gives_first_strike_and_its_negation_by_type(
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::rules::unit_flag::UnitFlag;
    use crate::rules::Ability;
    let mut g = Game::new(1);
    g.set_content(crate::game::testutil::shipped());
    let targets = g.content.rules.config.targets.clone();
    let rows = |g: &Game| -> Vec<(u16, String, Vec<Ability>)> {
        g.content
            .rules
            .units
            .iter()
            .map(|(_, d)| (d.num, d.id.clone(), d.abilities.clone()))
            .collect()
    };
    let shipped_rows = rows(&g);
    // The lists name the units the ids call cavalry and halberdiers, all of them.
    for (num, id, _) in &shipped_rows {
        assert_eq!(
            targets.cavalry_units.contains(num),
            id.ends_with("_cavalry"),
            "{id}"
        );
        assert_eq!(
            targets.halberdier_units.contains(num),
            id.ends_with("_halberdiers"),
            "{id}"
        );
    }
    // Off: nothing changes.
    assert!(!g.settings.rules.cavalry_halberdier_abilities);
    g.apply_content_settings();
    assert_eq!(rows(&g), shipped_rows);
    // On.
    g.settings.rules.cavalry_halberdier_abilities = true;
    g.apply_content_settings();
    let strike = Ability::new(targets.first_strike);
    let negate = Ability::new(targets.negate_first_strike);
    for ((num, id, before), (_, _, after)) in shipped_rows.iter().zip(rows(&g)) {
        let (cavalry, halberdier) = (
            targets.cavalry_units.contains(num),
            targets.halberdier_units.contains(num),
        );
        assert_eq!(
            after.contains(&strike),
            cavalry || before.contains(&strike),
            "{id}"
        );
        assert_eq!(
            after.contains(&negate),
            halberdier || before.contains(&negate),
            "{id}"
        );
        assert!(
            before.iter().all(|a| after.contains(a)),
            "{id} lost an ability"
        );
        let added = after.len() - before.len();
        let wanted = usize::from(cavalry && !before.contains(&strike))
            + usize::from(halberdier && !before.contains(&negate));
        assert_eq!(added, wanted, "{id}");
    }
    // Where the units' abilities are read: the flags the battle uses.
    let orc_cavalry = g
        .content
        .rules
        .units
        .by_num(141 /* orc_cavalry */)
        .ok_or("no orc cavalry")?;
    assert!(g
        .units()
        .flags_of(&orc_cavalry.abilities)
        .contains(UnitFlag::FirstStrike));
    let orc_halberdiers = g
        .content
        .rules
        .units
        .by_num(139 /* orc_halberdiers */)
        .ok_or("no orc halberdiers")?;
    assert!(g
        .units()
        .flags_of(&orc_halberdiers.abilities)
        .contains(UnitFlag::NegatesFirstStrike));
    // Off again (a loaded game applies its own choice on the shipped rows): they are as shipped.
    g.set_content(crate::game::testutil::shipped());
    g.settings.rules.cavalry_halberdier_abilities = false;
    g.apply_content_settings();
    assert_eq!(rows(&g), shipped_rows);
    Ok(())
}
