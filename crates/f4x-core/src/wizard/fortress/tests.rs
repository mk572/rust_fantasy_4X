use super::*;
use crate::game::save::{LoadError, Snapshot};
use crate::game::testutil::content_with_magic;
use crate::game::{Command, Rejection};
use crate::map::{Grid, Map, Plane};
use crate::player::Player;
use crate::rules::exec::Target;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

fn world() -> Game {
    let c = content_with_magic();
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut g = Game::new(3);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    for id in [P0, P1] {
        let mut p = Player::new(id, "W");
        p.mana = 100;
        g.players.push(p);
    }
    g.init_wizard(P0, &[("life", 2)], &[], None).unwrap();
    // The test content has no bears: War Bears brings a copy of the spear.
    let mut units = g.content.rules.units.defs().to_vec();
    let mut bears = units.iter().find(|u| u.id == "spear").unwrap().clone();
    bears.id = "war_bears".into();
    units.push(bears);
    g.content.rules.units = crate::rules::Registry::new(units);
    g
}

fn place(g: &mut Game, player: PlayerId, pos: Position) -> hecs::Entity {
    let race = g.content.rules.races.id_of("elf");
    let events = g.apply(Command::PlaceFortress {
        player,
        pos,
        name: "Capital".into(),
        race,
        population: 4,
    });
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::FortressPlaced { .. })),
        "{events:?}"
    );
    g.economy().city_at(pos).unwrap()
}

fn city(g: &mut Game, player: PlayerId, pos: Position) -> hecs::Entity {
    let race = g.content.rules.races.id_of("elf");
    g.apply(Command::FoundCity {
        owner: player,
        pos,
        name: "Town".into(),
        race,
        population: 3,
    });
    g.economy().city_at(pos).unwrap()
}

fn spell(g: &Game, id: &str) -> crate::rules::SpellId {
    g.content.rules.spells.id_of(id).unwrap()
}

fn cast(g: &mut Game, id: &str, city: hecs::Entity) -> Vec<Event> {
    let s = spell(g, id);
    g.magic_mut(P0).unwrap().known.insert(s);
    g.magic_mut(P0).unwrap().skill_left = 500;
    g.player_mut(P0).unwrap().mana = 500;
    g.apply(Command::CastSpell {
        player: P0,
        spell: s,
        target: Some(Target::City(city)),
    })
}

#[test]
fn placing_the_fortress_founds_the_capital_once() {
    let mut g = world();
    let capital = place(&mut g, P0, at(5, 5));
    let p = g.player(P0).unwrap();
    assert_eq!(p.fortress, Some(at(5, 5)));
    assert_eq!(p.summon_at, Some(at(5, 5)));
    assert_eq!(p.capital_race, g.content.rules.races.id_of("elf"));
    assert_eq!(g.economy().fortress_city(P0), Some(capital));
    assert!(g.economy().is_fortress_city(capital));
    // A second fortress, or one on a bad site, is refused.
    let again = g.apply(Command::PlaceFortress {
        player: P0,
        pos: at(12, 5),
        name: "x".into(),
        race: None,
        population: 1,
    });
    assert_eq!(
        again,
        vec![Event::Rejected(Rejection::Fortress(
            FortressError::NotAllowed
        ))]
    );
    // The fortress feeds the power base: one mana per book, plus the city's own.
    assert_eq!(g.income(P0).power_base, 2 + 2);
}

#[test]
fn summoned_creatures_appear_at_the_circle() {
    let mut g = world();
    place(&mut g, P0, at(5, 5));
    let town = city(&mut g, P0, at(15, 5));
    let call = spell(&g, "war_bears");
    g.magic_mut(P0).unwrap().known.insert(call);
    g.magic_mut(P0).unwrap().skill_left = 99;
    g.player_mut(P0).unwrap().mana = 500;
    // No target is needed: the creatures come to the circle.
    assert!(!g.spell_needs_target(P0, call));
    g.apply(Command::CastSpell {
        player: P0,
        spell: call,
        target: None,
    });
    assert_eq!(g.units().units_at(at(5, 5)).len(), 1);
    // Move the circle with the spell, and they come there instead.
    let events = cast(&mut g, "summoning_circle", town);
    assert!(events.contains(&Event::SummoningCircleMoved {
        player: P0,
        pos: at(15, 5)
    }));
    assert_eq!(
        g.player(P0).unwrap().fortress,
        Some(at(5, 5)),
        "fortress stays"
    );
    g.magic_mut(P0).unwrap().skill_left = 99;
    g.player_mut(P0).unwrap().mana = 500;
    g.apply(Command::CastSpell {
        player: P0,
        spell: call,
        target: None,
    });
    assert_eq!(g.units().units_at(at(15, 5)).len(), 1);
}

#[test]
fn the_fortress_spells_check_their_target_before_charging() {
    let mut g = world();
    let capital = place(&mut g, P0, at(5, 5));
    let foreign = place(&mut g, P1, at(15, 5));
    let town = city(&mut g, P0, at(10, 9));
    for (id, bad) in [
        ("move_fortress", foreign),
        ("move_fortress", capital),
        ("summoning_circle", foreign),
        ("spell_of_return", town),
    ] {
        let events = cast(&mut g, id, bad);
        assert!(
            matches!(events.as_slice(), [Event::Rejected(Rejection::Magic(_))]),
            "{id}: {events:?}"
        );
    }
    assert_eq!(g.player(P0).unwrap().mana, 500, "nothing was paid");
    // A legal move works, and Myrror adds five to the fortress's power.
    let events = cast(&mut g, "move_fortress", town);
    assert!(events.contains(&Event::FortressMoved {
        player: P0,
        pos: at(10, 9)
    }));
    assert_eq!(g.economy().fortress_city(P0), Some(town));
    assert_eq!(g.player(P0).unwrap().summon_at, Some(at(10, 9)));
    let myrror = Position {
        plane: Plane::Myrror,
        x: 3,
        y: 3,
    };
    let far = city(&mut g, P0, myrror);
    let before = g.income(P0).power_base;
    cast(&mut g, "move_fortress", far);
    assert_eq!(g.income(P0).power_base, before + 5, "plus five on Myrror");
}

#[test]
fn taking_an_ordinary_city_changes_its_owner_only() {
    let mut g = world();
    place(&mut g, P0, at(5, 5));
    let town = city(&mut g, P0, at(15, 5));
    let events = g.capture_city(town, P1);
    assert_eq!(
        events,
        vec![Event::CityCaptured {
            city: town,
            from: P0,
            to: P1
        }]
    );
    assert_eq!(g.economy().city_owner(town), Some(P1));
    assert!(!g.player(P0).unwrap().banished);
}

#[test]
fn losing_the_fortress_banishes_and_the_spell_of_return_brings_the_wizard_back() {
    let mut g = world();
    g.human = Some(P0);
    let capital = place(&mut g, P0, at(5, 5));
    let town = city(&mut g, P0, at(15, 5));
    let w = g.player(P1).unwrap().clone();
    let events = g.capture_city(capital, P1);
    assert!(
        events.contains(&Event::WizardBanished { player: P0, by: P1 }),
        "{events:?}"
    );
    let p = g.player(P0).unwrap();
    assert!(p.banished);
    assert_eq!((p.fortress, p.summon_at), (None, None));
    assert_eq!(p.mana, 50, "half the mana goes to the captor");
    assert_eq!(g.player(P1).unwrap().mana, w.mana + 50);
    assert_eq!(g.player(P1).unwrap().fame, w.fame + 5);
    // Casting the Spell of Return has begun, with no income of any kind meanwhile.
    let ret = spell(&g, "spell_of_return");
    assert_eq!(g.magic(P0).unwrap().casting.unwrap().spell, ret);
    assert_eq!(
        g.income(P0).split,
        crate::wizard::magic::PowerSplit::default()
    );
    assert_eq!(
        g.apply(crate::game::Command::CancelCast { player: P0 }),
        vec![crate::game::Event::Rejected(crate::game::Rejection::Magic(
            crate::wizard::magic::MagicError::NotAvailable
        ))]
    );
    assert_eq!(
        g.start_cast(P0, ret, Some(Target::City(town))),
        Err(crate::wizard::magic::MagicError::NotAvailable)
    );
    // It is paid over the following turns (a thousand points at the wizard's skill), then needs a
    // city.
    g.player_mut(P0).unwrap().mana = 5000;
    let mut ready = false;
    for _ in 0..400 {
        let events = g.apply(Command::EndTurn);
        if events.contains(&Event::SpellReady {
            player: P0,
            spell: ret,
        }) {
            ready = true;
            break;
        }
    }
    assert!(ready, "the Spell of Return completes");
    let events = g.apply(Command::ResolveCast {
        player: P0,
        target: Target::City(town),
    });
    assert!(
        events.contains(&Event::WizardReturned {
            player: P0,
            pos: at(15, 5)
        }),
        "{events:?}"
    );
    let p = g.player(P0).unwrap();
    assert!(!p.banished);
    assert_eq!(p.fortress, Some(at(15, 5)));
    assert_eq!(p.summon_at, Some(at(15, 5)));
    assert_eq!(g.magic(P0).unwrap().casting, None);
}

#[test]
fn losing_the_last_city_defeats_the_wizard() {
    let mut g = world();
    let capital = place(&mut g, P0, at(5, 5));
    let events = g.capture_city(capital, P1);
    assert!(events.contains(&Event::WizardDefeated { player: P0 }));
    let p = g.player(P0).unwrap();
    assert!(p.defeated && !p.banished);
    assert_eq!(p.magic.casting, None);
    // A defeated wizard cannot place a new fortress.
    let r = g.place_fortress(P0, at(9, 9), "x".into(), None, 1);
    assert_eq!(r, Err(FortressError::NotAllowed));
}

#[test]
fn a_fortress_city_cannot_be_destroyed() {
    let mut g = world();
    let capital = place(&mut g, P0, at(5, 5));
    let town = city(&mut g, P0, at(15, 5));
    g.world.get::<&mut Population>(capital).unwrap().figures = 0;
    assert_eq!(
        g.destroy_city(capital),
        Err(FortressError::CannotDestroyFortress)
    );
    assert_eq!(g.world.get::<&Population>(capital).unwrap().figures, 1);
    // Destroying the city that held the circle sends it back to the fortress.
    g.move_summoning_circle(P0, town).unwrap();
    let events = g.destroy_city(town).unwrap();
    assert_eq!(events, vec![Event::CityDestroyed { pos: at(15, 5) }]);
    assert!(g.economy().city_at(at(15, 5)).is_none());
    assert_eq!(g.player(P0).unwrap().summon_at, Some(at(5, 5)));
}

#[test]
fn saves_keep_the_fortress_and_reject_a_misplaced_one() {
    let mut g = world();
    g.human = Some(P0);
    let capital = place(&mut g, P0, at(5, 5));
    city(&mut g, P0, at(15, 5));
    g.capture_city(capital, P1);
    let snap = g.snapshot();
    let loaded = Snapshot::from_text(&snap.to_text().unwrap())
        .unwrap()
        .restore(content_with_magic())
        .unwrap();
    assert_eq!(loaded.snapshot(), g.snapshot());
    assert!(loaded.player(P0).unwrap().banished);
    // A fortress where the wizard has no city is refused.
    let mut bad = g.snapshot();
    bad.players[1].fortress = Some(at(9, 9));
    let err = bad.restore(content_with_magic()).err().unwrap();
    assert!(matches!(err, LoadError::Inconsistent(_)), "{err:?}");
}

#[test]
fn taking_the_last_rivals_last_city_wins_by_conquest() {
    let mut g = world();
    let capital = place(&mut g, P0, at(5, 5));
    g.player_mut(P0).unwrap().gold = 90;
    let events = g.capture_city(capital, P1);
    assert!(events.contains(&Event::GameOver {
        winner: P1,
        kind: crate::game::endgame::EndKind::Conquest,
        turn: 1
    }));
    assert_eq!(g.player(P0).unwrap().defeated_by, Some(P1));
    // Half the loser's gold goes to the winner.
    assert_eq!(g.player(P0).unwrap().gold, 45);
    assert_eq!(g.player(P1).unwrap().gold, 145);
    assert_eq!(
        g.apply(Command::EndTurn),
        vec![Event::Rejected(crate::game::Rejection::GameOver)]
    );
    // The winner scores 50 for the conquest.
    assert_eq!(g.score(P1).conquests, 50);
    assert_eq!(g.score(P0).conquests, 0);
}

#[test]
fn the_game_goes_on_while_two_wizards_stand_unless_the_human_falls() {
    let mut g = world();
    g.players.push(Player::new(PlayerId(2), "Third"));
    let capital = place(&mut g, P1, at(5, 5));
    let events = g.capture_city(capital, P0);
    assert!(events.contains(&Event::WizardDefeated { player: P1 }));
    assert!(g.game_over.is_none(), "wizards 0 and 2 still stand");

    let mut g = world();
    g.players.push(Player::new(PlayerId(2), "Third"));
    g.human = Some(P0);
    let capital = place(&mut g, P0, at(5, 5));
    let events = g.capture_city(capital, P1);
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::GameOver { winner, .. } if *winner == P1)));
}

#[test]
fn finishing_the_spell_of_mastery_wins_and_scores() {
    let mut g = world();
    let capital = place(&mut g, P0, at(5, 5));
    g.player_mut(P0).unwrap().fame = 10;
    let s = spell(&g, "spell_of_mastery");
    g.magic_mut(P0).unwrap().known.insert(s);
    g.magic_mut(P0).unwrap().skill_left = 9000;
    g.player_mut(P0).unwrap().mana = 9000;
    let _ = capital;
    let events = g.apply(Command::CastSpell {
        player: P0,
        spell: s,
        target: None,
    });
    assert!(
        events.contains(&Event::GameOver {
            winner: P0,
            kind: crate::game::endgame::EndKind::SpellOfMastery,
            turn: 1
        }),
        "{events:?}"
    );
    let score = g.score(P0);
    assert_eq!(score.mastery, 250);
    assert_eq!(score.fame, 20);
    // The rival still stood, so it counts as beaten.
    assert_eq!(score.conquests, 50);
    assert_eq!(score.time, 2000 - 2);
    assert_eq!(score.townsfolk, 2);
    assert_eq!(score.spells, 2);
    assert_eq!(score.total(), 2 + 2 + 50 + 20 + 1998 + 250);
    // The rival gets no time bonus: the mastery winner is someone else.
    assert_eq!(g.score(P1).time, 0);
}

#[test]
fn starting_the_spell_of_mastery_is_announced_and_visible_to_all() {
    let mut g = world();
    place(&mut g, P0, at(5, 5));
    let s = spell(&g, "spell_of_mastery");
    g.magic_mut(P0).unwrap().known.insert(s);
    g.player_mut(P0).unwrap().mana = 500;
    assert!(g.mastery_casters().is_empty());
    let events = g.apply(Command::CastSpell {
        player: P0,
        spell: s,
        target: None,
    });
    assert!(
        events.contains(&Event::MasteryStarted { caster: P0 }),
        "{events:?}"
    );
    let casters = g.mastery_casters();
    assert_eq!(casters.len(), 1);
    assert_eq!(casters[0].0, P0);
    assert!(casters[0].1 > 0, "still to pay");
}

#[test]
fn renaming_a_city_trims_and_cuts_the_name_and_ignores_an_empty_one() {
    let mut g = world();
    let town = city(&mut g, P0, at(5, 5));
    let name = |g: &Game| g.city_name(town).unwrap();
    let before = name(&g);
    g.apply(Command::RenameCity {
        city: town,
        name: "  ".into(),
    });
    assert_eq!(name(&g), before);
    g.apply(Command::RenameCity {
        city: town,
        name: "  Port Eversleeping  ".into(),
    });
    assert_eq!(name(&g), "Port Eversle");
}

#[test]
fn a_defeat_clears_the_losers_enchantments_and_nodes() {
    let mut g = world();
    let capital = place(&mut g, P0, at(5, 5));
    let town = city(&mut g, P1, at(15, 5));
    let ward = spell(&g, "move_fortress");
    g.world
        .insert_one(town, crate::city::economy::Enchantments(vec![(ward, P0)]))
        .unwrap();
    g.player_mut(P0).unwrap().magic.global.push(ward);
    g.capture_city(capital, P1);
    assert!(g
        .world
        .get::<&crate::city::economy::Enchantments>(town)
        .unwrap()
        .0
        .is_empty());
    assert!(g.player(P0).unwrap().magic.global.is_empty());
}

#[test]
fn the_end_of_the_game_is_saved() {
    let mut g = world();
    g.human = Some(P0);
    let capital = place(&mut g, P0, at(5, 5));
    g.capture_city(capital, P1);
    let loaded = Snapshot::from_text(&g.snapshot().to_text().unwrap())
        .unwrap()
        .restore(content_with_magic())
        .unwrap();
    assert_eq!(loaded.game_over, g.game_over);
    assert_eq!(loaded.human, Some(P0));
    assert_eq!(loaded.player(P0).unwrap().defeated_by, Some(P1));
}

// ---- the rules over a player, with no game ------------------------------------------------

fn wizard() -> Player {
    Player::new(P0, "Merlin")
}

fn square(x: u32) -> Position {
    Position {
        plane: crate::map::Plane::Arcanus,
        x,
        y: 3,
    }
}

#[test]
fn a_wizard_may_found_a_fortress_once_and_never_banished_or_defeated() {
    let mut p = wizard();
    assert_eq!(can_place(&p), Ok(()));
    place_at(&mut p, square(1), None);
    assert_eq!(
        (p.fortress, p.summon_at),
        (Some(square(1)), Some(square(1)))
    );
    assert_eq!(can_place(&p), Err(FortressError::NotAllowed));
    let mut q = wizard();
    q.banished = true;
    assert_eq!(can_place(&q), Err(FortressError::NotAllowed));
    let mut r = wizard();
    r.defeated = true;
    assert_eq!(can_place(&r), Err(FortressError::NotAllowed));
}

#[test]
fn moving_the_fortress_needs_one_and_takes_the_circle_with_it() {
    let mut p = wizard();
    assert_eq!(
        move_fortress_to(&mut p, square(2)),
        Err(FortressError::WrongState)
    );
    place_at(&mut p, square(1), None);
    p.summon_at = Some(square(5));
    move_fortress_to(&mut p, square(2)).unwrap();
    assert_eq!(
        (p.fortress, p.summon_at),
        (Some(square(2)), Some(square(2)))
    );
}

#[test]
fn a_banished_wizard_returns_and_takes_the_cities_race() {
    let mut p = wizard();
    place_at(&mut p, square(1), None);
    assert_eq!(
        return_to(&mut p, square(4), None),
        Err(FortressError::WrongState)
    );
    let half = fortress_falls(&mut p);
    assert_eq!((p.fortress, p.summon_at, half), (None, None, 0));
    p.mana = 11;
    p.fortress = Some(square(1));
    assert_eq!(fortress_falls(&mut p), 5);
    assert_eq!(p.mana, 6, "the odd point stays");
    banish_player(&mut p, None);
    assert!(p.banished && p.magic.casting.is_none());
    return_to(&mut p, square(4), None).unwrap();
    assert!(!p.banished);
    assert_eq!(
        (p.fortress, p.summon_at),
        (Some(square(4)), Some(square(4)))
    );
}

#[test]
fn the_circle_falls_back_to_the_fortress_when_its_city_is_lost() {
    let mut p = wizard();
    place_at(&mut p, square(1), None);
    p.summon_at = Some(square(7));
    circle_lost_to_capture(&mut p, square(7));
    assert_eq!(p.summon_at, Some(square(1)));
    // Losing the fortress city itself leaves the circle nowhere when it stood there.
    circle_lost_to_capture(&mut p, square(1));
    assert_eq!(p.summon_at, None);
    p.summon_at = Some(square(7));
    circle_lost_to_destruction(&mut p, square(7));
    assert_eq!(p.summon_at, Some(square(1)));
}

#[test]
fn a_defeated_wizard_has_nothing_left_and_the_prize_is_fame_and_half_the_mana() {
    let mut p = wizard();
    place_at(&mut p, square(1), None);
    p.banished = true;
    defeat_player(&mut p);
    assert!(p.defeated && !p.banished);
    assert_eq!((p.fortress, p.summon_at), (None, None));
    let mut w = Player::new(P1, "Rival");
    fortress_prize(&mut w, 7);
    assert_eq!((w.fame, w.mana), (5, 7));
    assert_eq!(summon_point(&wizard()), None);
}

/// A world where P0 holds a fortress city and a town, with a spearman in each and one in the
/// field, and P1 is the conqueror. P0 is a computer wizard of the given casting skill.
fn banish_world(skill: i32) -> (Game, hecs::Entity, hecs::Entity) {
    let mut g = world();
    g.magic_mut(P0).unwrap().skill_points = crate::wizard::magic::skill_points_for(skill, false);
    let capital = place(&mut g, P0, at(5, 5));
    let town = city(&mut g, P0, at(15, 5));
    let spear = g.content.rules.units.id_of("spear").unwrap();
    g.spawn_unit(P0, at(15, 5), spear);
    g.spawn_unit(P0, at(9, 9), spear);
    (g, capital, town)
}

#[test]
fn a_computer_wizard_who_cannot_pay_for_the_return_resigns_and_the_cities_go_neutral() {
    let (mut g, capital, town) = banish_world(10);
    let galley = g.content.rules.units.id_of("galley").unwrap();
    let mut defs = g.content.rules.units.defs().to_vec();
    defs.iter_mut().find(|d| d.id == "galley").unwrap().kind = crate::rules::UnitKind::Fantastic;
    g.content.rules.units = crate::rules::Registry::new(defs);
    let beast = g.spawn_unit(P0, at(15, 5), galley);
    let events = g.capture_city(capital, P1);
    assert!(
        events.contains(&Event::WizardDefeated { player: P0 }),
        "{events:?}"
    );
    assert!(!events.contains(&Event::WizardBanished { player: P0, by: P1 }));
    let p = g.player(P0).unwrap();
    assert!(p.defeated && !p.banished);
    assert_eq!(p.defeated_by, Some(P1));
    // The town is still there, owned by a neutral city player; its garrison of normal units
    // went with it, the fantastic creature and the unit in the field are gone.
    let owner = g.economy().city_owner(town).unwrap();
    assert!(g.is_neutral_player(owner) && owner != crate::player::NEUTRAL);
    let held = g.units().units_at(at(15, 5));
    assert_eq!(held.len(), 1);
    assert_eq!(g.units().unit_owner(held[0]), Some(owner));
    assert!(g.units().unit_owner(beast).is_none());
    assert!(g.units().player_units(P0).is_empty());
    assert!(g.units().units_at(at(9, 9)).is_empty());
}

#[test]
fn a_computer_wizard_with_no_mana_resigns_too() {
    let (mut g, capital, _) = banish_world(40);
    g.player_mut(P0).unwrap().mana = 0;
    let events = g.capture_city(capital, P1);
    assert!(
        events.contains(&Event::WizardDefeated { player: P0 }),
        "{events:?}"
    );
}

#[test]
fn a_strong_computer_wizard_is_banished_and_keeps_its_cities() {
    let (mut g, capital, town) = banish_world(40);
    let events = g.capture_city(capital, P1);
    assert!(
        events.contains(&Event::WizardBanished { player: P0, by: P1 }),
        "{events:?}"
    );
    assert_eq!(g.economy().city_owner(town), Some(P0));
    assert_eq!(g.units().player_units(P0).len(), 2);
}

#[test]
fn a_banished_human_may_resign() {
    let (mut g, capital, town) = banish_world(10);
    g.human = Some(P0);
    // Not while the fortress stands.
    assert_eq!(
        g.apply(Command::Resign { player: P0 }),
        vec![Event::Rejected(Rejection::Fortress(
            FortressError::WrongState
        ))]
    );
    g.capture_city(capital, P1);
    assert!(g.player(P0).unwrap().banished);
    let events = g.apply(Command::Resign { player: P0 });
    assert!(
        events.contains(&Event::WizardDefeated { player: P0 }),
        "{events:?}"
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::GameOver { winner, .. } if *winner == P1)));
    assert!(g.is_neutral_player(g.economy().city_owner(town).unwrap()));
    assert_eq!(g.player(P0).unwrap().defeated_by, Some(P1));
}

#[test]
fn the_banisher_is_remembered_across_a_save() {
    let (mut g, capital, _) = banish_world(40);
    g.capture_city(capital, P1);
    let snap = g.snapshot();
    let loaded = Snapshot::from_text(&snap.to_text().unwrap())
        .unwrap()
        .restore(content_with_magic())
        .unwrap();
    assert_eq!(loaded.player(P0).unwrap().banished_by, Some(P1));
}
