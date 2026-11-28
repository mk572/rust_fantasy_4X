//! Trade and research pacts: who may agree one, when it ends and what it brings.

use super::{DiplomacyCommand as D, ProposalKind, Treaty};
use crate::map::{Plane, Position};
use crate::player::PlayerId;
use crate::rules::RaceId;
use crate::wizard::fortress::Fall;
use crate::wizard::sheet::Books;
use crate::world::testutil::{fresh, Table};
use crate::world::Command;
use f4x_def::rules::effects::Effect;
use f4x_def::rules::stats::{ModOp, Stat};
use f4x_def::rules::RealmId;

const PACTS: [ProposalKind; 2] = [ProposalKind::TradePact, ProposalKind::ResearchPact];

/// A new game, two of its wizards bound by a wizard pact under the expanded diplomacy options.
fn partners() -> Result<(Table, PlayerId, PlayerId), String> {
    let mut w = fresh(2, 7)?;
    let ids = w.wizards();
    let &[a, b, ..] = ids.as_slice() else {
        return Err("two wizards".into());
    };
    w.scenario_change_settings(|s| s.rules.expanded_diplomacy = true);
    w.make_contact(a, b);
    w.start_treaty(a, b, Treaty::Pact);
    Ok((w, a, b))
}

/// A neutral race player with a city, that `a` has a wizard pact with.
fn independent(w: &mut Table, a: PlayerId) -> Result<PlayerId, String> {
    let race = w
        .player_ids()
        .into_iter()
        .find(|&p| w.race_of_player(p).is_some() && !w.is_dormant(p))
        .ok_or("a race with a city")?;
    w.make_contact(a, race);
    w.start_treaty(a, race, Treaty::Pact);
    Ok(race)
}

fn agree(w: &mut Table, from: PlayerId, to: PlayerId, kind: ProposalKind) {
    w.apply(Command::Diplomacy(D::Propose { from, to, kind }));
    w.apply(Command::Diplomacy(D::Answer {
        player: to,
        from,
        accept: true,
    }));
}

fn held(w: &Table, a: PlayerId, b: PlayerId) -> [bool; 2] {
    [
        w.diplomacy.has_trade_pact(a, b),
        w.diplomacy.has_research_pact(a, b),
    ]
}

#[test]
fn a_pact_is_offered_only_to_a_treaty_partner_under_the_expanded_options() -> Result<(), String> {
    let (mut w, a, b) = partners()?;
    for kind in PACTS {
        for (treaty, allowed) in [
            (Treaty::Pact, true),
            (Treaty::Alliance, true),
            (Treaty::None, false),
            (Treaty::War, false),
        ] {
            w.start_treaty(a, b, treaty);
            assert_eq!(
                w.check_proposal(a, b, kind).is_ok(),
                allowed,
                "{kind:?} {treaty:?}"
            );
        }
        w.start_treaty(a, b, Treaty::Pact);
        w.scenario_change_settings(|s| s.rules.expanded_diplomacy = false);
        assert!(
            w.check_proposal(a, b, kind).is_err(),
            "{kind:?}, setting off"
        );
        w.scenario_change_settings(|s| s.rules.expanded_diplomacy = true);
    }
    Ok(())
}

#[test]
fn an_agreed_pact_holds_both_ways_and_is_not_offered_again() -> Result<(), String> {
    let (mut w, a, b) = partners()?;
    agree(&mut w, a, b, ProposalKind::TradePact);
    assert_eq!(held(&w, b, a), [true, false]);
    agree(&mut w, b, a, ProposalKind::ResearchPact);
    assert_eq!(held(&w, a, b), [true, true]);
    for kind in PACTS {
        assert!(w.check_proposal(a, b, kind).is_err(), "{kind:?} twice");
        assert!(w.check_proposal(b, a, kind).is_err(), "{kind:?} twice");
    }
    Ok(())
}

#[test]
fn an_independent_race_may_trade_but_does_no_research() -> Result<(), String> {
    let (mut w, a, _) = partners()?;
    let race = independent(&mut w, a)?;
    assert!(w
        .check_proposal(a, race, ProposalKind::ResearchPact)
        .is_err());
    agree(&mut w, a, race, ProposalKind::TradePact);
    assert_eq!(held(&w, a, race), [true, false]);
    Ok(())
}

#[test]
fn a_pact_lasts_as_long_as_the_wizard_pact_or_alliance() -> Result<(), String> {
    let both = || -> Result<_, String> {
        let (mut w, a, b) = partners()?;
        for kind in PACTS {
            agree(&mut w, a, b, kind);
        }
        Ok((w, a, b))
    };
    let (mut w, a, b) = both()?;
    w.start_treaty(a, b, Treaty::Alliance);
    assert_eq!(held(&w, a, b), [true, true], "allied");
    w.apply(Command::Diplomacy(D::BreakTreaty { player: b, with: a }));
    assert_eq!(held(&w, a, b), [false, false], "broken");
    w.start_treaty(a, b, Treaty::Pact);
    assert_eq!(held(&w, a, b), [false, false], "a new wizard pact");

    let (mut w, a, b) = both()?;
    w.declare_war(a, b);
    assert_eq!(held(&w, a, b), [false, false], "war");

    let (mut w, a, b) = both()?;
    let (world, rng) = w.world_and_rng();
    world.defeat(b, a, Fall::Resigned, rng);
    assert_eq!(held(&w, a, b), [false, false], "gone");
    Ok(())
}

/// `count` free squares in a row on Arcanus, after every road of the game is taken away.
fn free_row(w: &mut Table, count: i32) -> Result<Vec<Position>, String> {
    let map = w.map().ok_or("a map")?;
    let (width, height) = (map.width() as i32, map.height() as i32);
    for plane in Plane::all() {
        for y in 0..height {
            for x in 0..width {
                let at = Position {
                    plane,
                    x: x as _,
                    y: y as _,
                };
                w.scenario_set_road(at, false);
            }
        }
    }
    (0..height)
        .flat_map(|y| (0..=width - count).map(move |x| (x, y)))
        .map(|(x, y)| {
            (x..x + count)
                .map(|x| Position {
                    plane: Plane::Arcanus,
                    x: x as _,
                    y: y as _,
                })
                .collect::<Vec<_>>()
        })
        .find(|row| row.iter().all(|&p| w.economy().city_at(p).is_none()))
        .ok_or_else(|| "a free row".into())
}

/// Two races that get no trade of their own.
fn plain_races(w: &Table) -> Result<[RaceId; 2], String> {
    let mut races = w
        .content()
        .rules
        .races
        .iter()
        .filter(|(_, r)| r.trade_percent == 0)
        .map(|(id, _)| id);
    Ok([
        races.next().ok_or("a race")?,
        races.next().ok_or("a second race")?,
    ])
}

/// A town of `owner`, with what it takes to ask for its trade.
struct Town {
    city: hecs::Entity,
    at: Position,
    race: RaceId,
    figures: u32,
}

impl Town {
    fn found(w: &mut Table, owner: PlayerId, at: Position, race: RaceId, figures: u32) -> Self {
        let city = w.scenario_found_city(owner, at, "Town", Some(race), figures);
        Town {
            city,
            at,
            race,
            figures,
        }
    }

    fn trade(&self, w: &Table) -> i32 {
        w.economy()
            .site_trade_percent(self.city, self.at, Some(self.race), self.figures)
    }
}

#[test]
fn a_trade_pact_counts_each_citizen_of_a_connected_partner_town_at_the_pact_rate(
) -> Result<(), String> {
    // The same race and another one count alike under a pact.
    for same_race in [false, true] {
        let (mut w, a, b) = partners()?;
        let [first, second] = plain_races(&w)?;
        let row = free_row(&mut w, 5)?;
        let (large, small) = (20, 5);
        let mine = Town::found(&mut w, a, row[0], first, large);
        let race = if same_race { first } else { second };
        let theirs = Town::found(&mut w, b, row[4], race, small);
        let alone = [mine.trade(&w), theirs.trade(&w)];
        for &square in &row {
            w.scenario_set_road(square, true);
        }
        let road = [mine.trade(&w), theirs.trade(&w)];
        agree(&mut w, a, b, ProposalKind::TradePact);
        let rate = w.content().rules.config.diplomacy.trade_pact_percent;
        assert_eq!(mine.trade(&w) - alone[0], rate * small as i32);
        assert!(mine.trade(&w) > road[0], "more than the road alone");
        assert!(theirs.trade(&w) >= road[1], "the partner loses nothing");

        w.scenario_change_settings(|s| s.rules.expanded_diplomacy = false);
        assert_eq!([mine.trade(&w), theirs.trade(&w)], road, "setting off");
    }
    Ok(())
}

#[test]
fn each_trade_pact_raises_what_trade_may_bring_a_town() -> Result<(), String> {
    let (mut w, a, b) = partners()?;
    let other = independent(&mut w, a)?;
    let [first, second] = plain_races(&w)?;
    let row = free_row(&mut w, 3)?;
    let small = 2;
    let mine = Town::found(&mut w, a, row[0], first, small);
    Town::found(&mut w, b, row[2], second, 20);
    for &square in &row {
        w.scenario_set_road(square, true);
    }
    let rules = w.content().rules.config.diplomacy.clone();
    let cap = |pacts: i32| {
        (rules.trade_cap_percent + rules.trade_pact_cap_percent * pacts) * small as i32
    };
    assert_eq!(mine.trade(&w), cap(0));
    agree(&mut w, a, b, ProposalKind::TradePact);
    assert_eq!(mine.trade(&w), cap(1));
    // A pact counts for the cap whether or not a road leads to the partner.
    agree(&mut w, a, other, ProposalKind::TradePact);
    assert_eq!(mine.trade(&w), cap(2));
    Ok(())
}

/// Give `owner` towns with every building that makes research, so that it researches a lot.
fn study(w: &mut Table, owner: PlayerId, towns: &[Position]) -> Result<(), String> {
    let [race, _] = plain_races(w)?;
    let schools: Vec<_> = w
        .content()
        .rules
        .buildings
        .iter()
        .filter(|(_, b)| {
            b.effects.iter().any(|r| {
                matches!(
                    r.effect,
                    Effect::Modify {
                        stat: Stat::CityResearch,
                        op: ModOp::Add,
                        ..
                    }
                )
            })
        })
        .map(|(id, _)| id)
        .collect();
    for &at in towns {
        let town = Town::found(w, owner, at, race, 5);
        for &school in &schools {
            w.scenario_set_building(town.city, school, true);
        }
    }
    Ok(())
}

#[test]
fn a_research_pact_gives_both_a_share_of_what_the_two_research_more_for_shared_books(
) -> Result<(), String> {
    let (mut w, a, b) = partners()?;
    let [life, death, chaos] = [3, 4, 2].map(RealmId::new);
    for (who, books) in [
        (a, vec![(life, 4), (chaos, 2)]),
        (b, vec![(life, 2), (death, 3), (chaos, 5)]),
    ] {
        w.wizard_mut(who)
            .ok_or("a wizard")?
            .put(Some(Books(books.into_iter().collect())));
    }
    // Two books of life and two of chaos are held by both.
    let shared = 4;
    let row = free_row(&mut w, 15)?;
    let towns: Vec<Position> = row.iter().copied().step_by(3).collect();
    study(&mut w, a, &towns[..3])?;
    study(&mut w, b, &towns[3..])?;
    let alone = [w.income(a).research, w.income(b).research];

    agree(&mut w, a, b, ProposalKind::ResearchPact);
    let rules = w.content().rules.config.diplomacy.clone();
    let percent = rules.research_pact_percent + rules.research_pact_book_percent * shared;
    let gain = (alone[0] + alone[1]) * percent / 100;
    assert!(
        gain > (alone[0] + alone[1]) * rules.research_pact_percent / 100,
        "the books are worth something"
    );
    assert_eq!(
        [w.income(a).research, w.income(b).research],
        [alone[0] + gain, alone[1] + gain]
    );

    w.scenario_change_settings(|s| s.rules.expanded_diplomacy = false);
    assert_eq!([w.income(a).research, w.income(b).research], alone);
    Ok(())
}
