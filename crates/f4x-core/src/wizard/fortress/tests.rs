use super::*;
use crate::game::save::{LoadError, Snapshot};
use crate::game::testutil::content_with_magic;
use crate::game::{Command, Rejection};
use crate::map::{Grid, Map, Plane};
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
        g.add_player(id, "W");
        g.treasury_mut(id).unwrap().mana = 100;
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
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(s);
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::Skill>()
        .left = 500;
    g.treasury_mut(P0).unwrap().mana = 500;
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
    let p = g.wizard(P0).unwrap();
    assert_eq!(p.fortress(), Some(at(5, 5)));
    assert_eq!(p.summon_at(), Some(at(5, 5)));
    assert_eq!(
        g.wizard(P0).unwrap().capital_race(),
        g.content.rules.races.id_of("elf")
    );
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
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(call);
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::Skill>()
        .left = 99;
    g.treasury_mut(P0).unwrap().mana = 500;
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
        g.wizard(P0).unwrap().fortress(),
        Some(at(5, 5)),
        "fortress stays"
    );
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::Skill>()
        .left = 99;
    g.treasury_mut(P0).unwrap().mana = 500;
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
    assert_eq!(
        g.player(P0).unwrap().treasury().mana,
        500,
        "nothing was paid"
    );
    // A legal move works, and Myrror adds five to the fortress's power.
    let events = cast(&mut g, "move_fortress", town);
    assert!(events.contains(&Event::FortressMoved {
        player: P0,
        pos: at(10, 9)
    }));
    assert_eq!(g.economy().fortress_city(P0), Some(town));
    assert_eq!(g.wizard(P0).unwrap().summon_at(), Some(at(10, 9)));
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
    assert!(!g.wizard(P0).unwrap().is_banished());
}

#[test]
fn losing_the_fortress_banishes_and_the_spell_of_return_brings_the_wizard_back() {
    let mut g = world();
    g.set_human(Some(P0));
    set_skill(&mut g, P0, 40);
    let capital = place(&mut g, P0, at(5, 5));
    let town = city(&mut g, P0, at(15, 5));
    let w_fame = g.wizard(P1).unwrap().fame();
    let w_mana = g.player(P1).unwrap().treasury().mana;
    let events = g.capture_city(capital, P1);
    assert!(
        events.contains(&Event::WizardBanished { player: P0, by: P1 }),
        "{events:?}"
    );
    let p = g.wizard(P0).unwrap();
    assert!(p.is_banished());
    assert_eq!((p.fortress(), p.summon_at()), (None, None));
    assert_eq!(
        g.player(P0).unwrap().treasury().mana,
        50,
        "half the mana goes to the captor"
    );
    assert_eq!(g.player(P1).unwrap().treasury().mana, w_mana + 50);
    assert_eq!(g.wizard(P1).unwrap().fame(), w_fame + 5);
    // Casting the Spell of Return has begun, with no income of any kind meanwhile.
    let ret = spell(&g, "spell_of_return");
    assert_eq!(g.wizard(P0).unwrap().casting().unwrap().spell, ret);
    assert_eq!(
        g.income(P0).split,
        crate::wizard::magic::PowerShares::default()
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
    g.treasury_mut(P0).unwrap().mana = 5000;
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
    let p = g.wizard(P0).unwrap();
    assert!(!p.is_banished());
    assert_eq!(p.fortress(), Some(at(15, 5)));
    assert_eq!(p.summon_at(), Some(at(15, 5)));
    assert_eq!(g.wizard(P0).unwrap().casting(), None);
}

#[test]
fn losing_the_last_city_defeats_the_wizard() {
    let mut g = world();
    let capital = place(&mut g, P0, at(5, 5));
    let events = g.capture_city(capital, P1);
    assert!(events.contains(&Event::WizardDefeated { player: P0 }));
    let p = g.wizard(P0).unwrap();
    assert!(g.player(P0).is_some_and(|p| p.is_defeated()) && !p.is_banished());
    assert_eq!(g.wizard(P0).unwrap().casting(), None);
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
    assert_eq!(g.wizard(P0).unwrap().summon_at(), Some(at(5, 5)));
}

#[test]
fn saves_keep_the_fortress_and_reject_a_misplaced_one() {
    let mut g = world();
    g.set_human(Some(P0));
    set_skill(&mut g, P0, 40);
    let capital = place(&mut g, P0, at(5, 5));
    city(&mut g, P0, at(15, 5));
    g.capture_city(capital, P1);
    let snap = g.snapshot();
    let loaded = Snapshot::from_text(&snap.to_text().unwrap())
        .unwrap()
        .restore(content_with_magic())
        .unwrap();
    assert_eq!(loaded.snapshot(), g.snapshot());
    assert!(loaded.wizard(P0).unwrap().is_banished());
    // A fortress where the wizard has no city is refused.
    let mut bad = g.snapshot();
    bad.wizards[1].fortress = Some(at(9, 9));
    let err = bad.restore(content_with_magic()).err().unwrap();
    assert!(matches!(err, LoadError::Inconsistent(_)), "{err:?}");
}

#[test]
fn taking_the_last_rivals_last_city_wins_by_conquest() {
    let mut g = world();
    let capital = place(&mut g, P0, at(5, 5));
    g.treasury_mut(P0).unwrap().gold = 90;
    let events = g.capture_city(capital, P1);
    assert!(events.contains(&Event::GameOver {
        winner: P1,
        kind: crate::game::endgame::EndKind::Conquest,
        turn: 1
    }));
    assert_eq!(g.player(P0).and_then(|p| p.defeated_by()), Some(P1));
    // Half the loser's gold goes to the winner.
    assert_eq!(g.player(P0).unwrap().treasury().gold, 45);
    assert_eq!(g.player(P1).unwrap().treasury().gold, 145);
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
    g.add_player(PlayerId(2), "Third");
    let capital = place(&mut g, P1, at(5, 5));
    let events = g.capture_city(capital, P0);
    assert!(events.contains(&Event::WizardDefeated { player: P1 }));
    assert!(g.game_over.is_none(), "wizards 0 and 2 still stand");

    let mut g = world();
    g.add_player(PlayerId(2), "Third");
    g.set_human(Some(P0));
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
    g.set_fame(P0, 10);
    let s = spell(&g, "spell_of_mastery");
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(s);
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::Skill>()
        .left = 9000;
    g.treasury_mut(P0).unwrap().mana = 9000;
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
    assert_eq!(
        score.spells, 4,
        "two known, and the two every wizard starts with"
    );
    assert_eq!(score.total(), 2 + 4 + 50 + 20 + 1998 + 250);
    // The rival gets no time bonus: the mastery winner is someone else.
    assert_eq!(g.score(P1).time, 0);
}

#[test]
fn starting_the_spell_of_mastery_is_announced_and_visible_to_all() {
    let mut g = world();
    place(&mut g, P0, at(5, 5));
    let s = spell(&g, "spell_of_mastery");
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(s);
    g.treasury_mut(P0).unwrap().mana = 500;
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
    g.add_city_enchantment(town, ward, P0);
    g.add_global(P0, ward);
    g.capture_city(capital, P1);
    assert!(g.city_enchantments(town).is_empty());
    assert!(g.wizard(P0).unwrap().global().is_empty());
}

#[test]
fn the_end_of_the_game_is_saved() {
    let mut g = world();
    g.set_human(Some(P0));
    let capital = place(&mut g, P0, at(5, 5));
    g.capture_city(capital, P1);
    let loaded = Snapshot::from_text(&g.snapshot().to_text().unwrap())
        .unwrap()
        .restore(content_with_magic())
        .unwrap();
    assert_eq!(loaded.game_over, g.game_over);
    assert_eq!(loaded.human(), Some(P0));
    assert_eq!(loaded.player(P0).and_then(|p| p.defeated_by()), Some(P1));
}

// ---- the rules over a wizard, with no map ---------------------------------------------------

fn wizard_game() -> Game {
    let mut g = Game::new(3);
    g.add_player(P0, "W");
    g
}

fn square(x: u32) -> Position {
    Position {
        plane: crate::map::Plane::Arcanus,
        x,
        y: 3,
    }
}

/// Where the fortress and the circle of the wizard stand.
fn sites(w: &WizardMut) -> (Option<Position>, Option<Position>) {
    let v = w.view();
    (v.fortress(), v.summon_at())
}

#[test]
fn a_wizard_may_found_a_fortress_once_and_never_banished_or_defeated() {
    let mut g = wizard_game();
    let mut w = g.wizard_mut(P0).unwrap();
    assert_eq!(can_place(&w.view(), false), Ok(()));
    place_at(&mut w, square(1));
    assert_eq!(sites(&w), (Some(square(1)), Some(square(1))));
    assert_eq!(can_place(&w.view(), false), Err(FortressError::NotAllowed));
    let mut g = wizard_game();
    let mut q = g.wizard_mut(P0).unwrap();
    banish_player(&mut q, P1);
    assert_eq!(can_place(&q.view(), false), Err(FortressError::NotAllowed));
    let mut g = wizard_game();
    let r = g.wizard_mut(P0).unwrap();
    assert_eq!(can_place(&r.view(), true), Err(FortressError::NotAllowed));
}

#[test]
fn moving_the_fortress_needs_one_and_takes_the_circle_with_it() {
    let mut g = wizard_game();
    let mut w = g.wizard_mut(P0).unwrap();
    assert_eq!(
        move_fortress_to(&mut w, square(2)),
        Err(FortressError::WrongState)
    );
    place_at(&mut w, square(1));
    w.put(Some(SummonAt(square(5))));
    move_fortress_to(&mut w, square(2)).unwrap();
    assert_eq!(sites(&w), (Some(square(2)), Some(square(2))));
}

#[test]
fn a_banished_wizard_returns_and_takes_the_cities_race() {
    let mut g = wizard_game();
    let mut w = g.wizard_mut(P0).unwrap();
    place_at(&mut w, square(1));
    assert_eq!(return_to(&mut w, square(4)), Err(FortressError::WrongState));
    fortress_falls(&mut w);
    assert_eq!(sites(&w), (None, None));
    let mut t = crate::player::Treasury { gold: 0, mana: 11 };
    assert_eq!(t.take_half_mana(), 5);
    assert_eq!(t.mana, 6, "the odd point stays");
    banish_player(&mut w, P1);
    assert!(w.view().is_banished());
    assert_eq!(w.view().banished_by(), Some(P1));
    return_to(&mut w, square(4)).unwrap();
    assert!(!w.view().is_banished());
    assert_eq!(sites(&w), (Some(square(4)), Some(square(4))));
}

#[test]
fn the_circle_falls_back_to_the_fortress_when_its_city_is_lost() {
    let mut g = wizard_game();
    let mut w = g.wizard_mut(P0).unwrap();
    place_at(&mut w, square(1));
    w.put(Some(SummonAt(square(7))));
    circle_lost_to_capture(&mut w, square(7));
    assert_eq!(w.view().summon_at(), Some(square(1)));
    // Losing the fortress city itself leaves the circle nowhere when it stood there.
    circle_lost_to_capture(&mut w, square(1));
    assert_eq!(w.view().summon_at(), None);
    w.put(Some(SummonAt(square(7))));
    circle_lost_to_destruction(&mut w, square(7));
    assert_eq!(w.view().summon_at(), Some(square(1)));
}

#[test]
fn a_defeated_wizard_has_nothing_left_and_the_prize_is_fame_and_half_the_mana() {
    let mut g = wizard_game();
    let mut w = g.wizard_mut(P0).unwrap();
    place_at(&mut w, square(1));
    banish_player(&mut w, P1);
    defeat_player(&mut w);
    assert!(!w.view().is_banished());
    assert_eq!(sites(&w), (None, None));
    assert_eq!(FORTRESS_PRIZE_FAME, 5);
    assert_eq!(w.view().summon_point(), None);
}

/// A world where P0 holds a fortress city and a town, with a spearman in each and one in the
/// field, and P1 is the conqueror. P0 is a computer wizard of the given casting skill.
fn set_skill(g: &mut Game, player: PlayerId, skill: i32) {
    g.wizard_mut(player)
        .unwrap()
        .get_mut::<crate::wizard::sheet::Skill>()
        .points = crate::wizard::magic::skill_points_for(skill, false);
}

fn banish_world(skill: i32) -> (Game, hecs::Entity, hecs::Entity) {
    let mut g = world();
    set_skill(&mut g, P0, skill);
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
    let p = g.wizard(P0).unwrap();
    assert!(g.player(P0).is_some_and(|p| p.is_defeated()) && !p.is_banished());
    assert_eq!(g.player(P0).and_then(|p| p.defeated_by()), Some(P1));
    // The town is still there, owned by a neutral city player; its garrison of normal units
    // went with it, the fantastic creature and the unit in the field are gone.
    let owner = g.economy().city_owner(town).unwrap();
    assert!(g.is_neutral_player(owner) && owner != crate::player::MONSTERS);
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
    g.treasury_mut(P0).unwrap().mana = 0;
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
    let (mut g, capital, town) = banish_world(40);
    g.set_human(Some(P0));
    // Not while the fortress stands.
    assert_eq!(
        g.apply(Command::Resign { player: P0 }),
        vec![Event::Rejected(Rejection::Fortress(
            FortressError::WrongState
        ))]
    );
    g.capture_city(capital, P1);
    assert!(g.wizard(P0).unwrap().is_banished());
    let events = g.apply(Command::Resign { player: P0 });
    assert!(
        events.contains(&Event::WizardDefeated { player: P0 }),
        "{events:?}"
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::GameOver { winner, .. } if *winner == P1)));
    assert!(g.is_neutral_player(g.economy().city_owner(town).unwrap()));
    assert_eq!(g.player(P0).and_then(|p| p.defeated_by()), Some(P1));
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
    assert_eq!(loaded.wizard(P0).unwrap().banished_by(), Some(P1));
}

// ---- banishment and defeat as in the original (task 341) -------------------------------------

/// P0 and P1 are wizards, P0 with a fortress and a town; P0 knows exactly `leave` spells that P1
/// does not (P1 is taught the rest). Returns those spells.
fn spell_world(leave: usize) -> (Game, hecs::Entity, Vec<crate::rules::SpellId>) {
    let mut g = world();
    g.init_wizard(P1, &[("life", 2)], &[], None).unwrap();
    set_skill(&mut g, P0, 40);
    let capital = place(&mut g, P0, at(5, 5));
    city(&mut g, P0, at(15, 5));
    let known = |g: &Game, p| g.wizard(p).unwrap().known();
    let mut pool: Vec<_> = known(&g, P0).difference(&known(&g, P1)).copied().collect();
    // The test content offers the wizard more spells to learn than it starts with.
    let more: Vec<_> = g
        .content
        .rules
        .spells
        .defs()
        .iter()
        .filter_map(|d| g.content.rules.spells.id_of(&d.id))
        .filter(|s| !known(&g, P0).contains(s) && !known(&g, P1).contains(s))
        .collect();
    while pool.len() < leave {
        let s = more[pool.len()];
        g.learn_spell(P0, s);
        pool.push(s);
    }
    for s in pool.split_off(leave) {
        g.learn_spell(P1, s);
    }
    assert_eq!(
        known(&g, P0).difference(&known(&g, P1)).count(),
        leave,
        "the setup leaves exactly {leave} to take"
    );
    (g, capital, pool)
}

fn spells_taken(events: &[Event]) -> Option<Vec<crate::rules::SpellId>> {
    events.iter().find_map(|e| match e {
        Event::ConquestSpells { spells, .. } => Some(spells.clone()),
        _ => None,
    })
}

#[test]
fn a_human_who_cannot_pay_for_the_return_is_defeated_at_once() {
    let (mut g, capital, _) = banish_world(10);
    g.set_human(Some(P0));
    let events = g.capture_city(capital, P1);
    assert!(
        events.contains(&Event::WizardDefeated { player: P0 }),
        "{events:?}"
    );
    assert!(!events.contains(&Event::WizardBanished { player: P0, by: P1 }));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::GameOver { winner, .. } if *winner == P1)));
}

#[test]
fn the_last_city_taken_without_the_fortress_still_pays_the_prize() {
    let (mut g, capital, town) = banish_world(40);
    g.init_wizard(P1, &[("life", 2)], &[], None).unwrap();
    g.capture_city(capital, P1);
    assert!(g.wizard(P0).unwrap().is_banished());
    let fame = g.wizard(P1).unwrap().fame();
    let mana = g.player(P1).unwrap().treasury().mana;
    let theirs = g.player(P0).unwrap().treasury().mana;
    let events = g.capture_city(town, P1);
    assert!(
        events.contains(&Event::WizardDefeated { player: P0 }),
        "{events:?}"
    );
    assert_eq!(g.wizard(P1).unwrap().fame(), fame + FORTRESS_PRIZE_FAME);
    assert_eq!(g.player(P1).unwrap().treasury().mana, mana + theirs / 2);
}

#[test]
fn a_defeat_gives_the_winner_two_of_the_spells_the_loser_knew() {
    let (mut g, capital, taught) = spell_world(3);
    g.capture_city(g.economy().city_at(at(15, 5)).unwrap(), P1);
    // With the town gone the fortress is the last city: losing it defeats the wizard.
    let events = g.capture_city(capital, P1);
    let taken = spells_taken(&events).expect("a conquest event");
    assert_eq!(taken.len(), 2, "{events:?}");
    assert!(taken.iter().all(|s| taught.contains(s)));
    for s in &taken {
        assert!(g.wizard(P1).unwrap().knows(*s));
    }
}

#[test]
fn a_defeat_with_nothing_new_to_take_still_reports_it_for_the_box() {
    let (mut g, capital, taught) = spell_world(0);
    assert!(taught.is_empty());
    g.capture_city(g.economy().city_at(at(15, 5)).unwrap(), P1);
    let events = g.capture_city(capital, P1);
    assert_eq!(spells_taken(&events), Some(Vec::new()), "{events:?}");
}

#[test]
fn one_new_spell_is_all_that_there_is_to_take() {
    let (mut g, capital, taught) = spell_world(1);
    g.capture_city(g.economy().city_at(at(15, 5)).unwrap(), P1);
    let events = g.capture_city(capital, P1);
    assert_eq!(spells_taken(&events), Some(taught), "{events:?}");
}

#[test]
fn a_banished_computer_wizard_loses_spells_but_a_banished_human_keeps_them() {
    let (mut g, capital, _) = spell_world(2);
    let events = g.capture_city(capital, P1);
    assert!(events.contains(&Event::WizardBanished { player: P0, by: P1 }));
    assert_eq!(spells_taken(&events).map(|s| s.len()), Some(2));

    let (mut g, capital, _) = spell_world(2);
    g.set_human(Some(P0));
    let events = g.capture_city(capital, P1);
    assert!(events.contains(&Event::WizardBanished { player: P0, by: P1 }));
    assert_eq!(spells_taken(&events), None);
}

#[test]
fn an_independent_city_taken_gives_no_spells() {
    let mut g = world();
    g.init_wizard(P1, &[("life", 2)], &[], None).unwrap();
    let town = city(&mut g, P0, at(15, 5));
    let neutral = g.new_city_player();
    g.set_owner(town, neutral);
    let events = g.capture_city(town, P1);
    assert_eq!(spells_taken(&events), None, "{events:?}");
}

#[test]
fn a_city_that_changes_hands_loses_the_old_owners_enchantments() {
    let mut g = world();
    g.init_wizard(P1, &[("life", 2)], &[], None).unwrap();
    place(&mut g, P0, at(5, 5));
    let town = city(&mut g, P0, at(15, 5));
    let ward = spell(&g, "move_fortress");
    g.add_city_enchantment(town, ward, P0);
    g.add_city_enchantment(town, ward, P1);
    g.capture_city(town, P1);
    let casters: Vec<_> = g
        .spells()
        .city_enchantments(town)
        .into_iter()
        .map(|(_, c)| c)
        .collect();
    assert_eq!(casters, vec![P1]);
}

#[test]
fn a_banished_wizard_may_throw_no_spell_and_start_no_talk() {
    let mut g = world();
    g.init_wizard(P1, &[("life", 2)], &[], None).unwrap();
    set_skill(&mut g, P0, 40);
    let capital = place(&mut g, P0, at(5, 5));
    city(&mut g, P0, at(15, 5));
    g.make_contact(P0, P1);
    g.capture_city(capital, P1);
    let first = *g.wizard(P0).unwrap().known().iter().next().unwrap();
    assert_eq!(
        g.wizard(P0).unwrap().check_can_cast(first),
        Err(crate::wizard::magic::MagicError::Banished)
    );
    let events = g.apply(Command::Diplomacy(
        crate::diplomacy::DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: crate::diplomacy::ProposalKind::Pact,
        },
    ));
    assert!(
        events.contains(&Event::Rejected(Rejection::Diplomacy(
            crate::diplomacy::DiplomacyError::Banished
        ))),
        "{events:?}"
    );
}
