use super::*;
use crate::battle::{BattleCommand, Side};
use crate::game::testutil::{num, realm, shipped};
use crate::map::{FeatureId, Grid, Map, Plane};
use crate::player::PlayerId;
use crate::rules::RealmId;
use f4x_def::testdir::TestResult;
use rand::SeedableRng;

use crate::game::Command;

const P0: PlayerId = PlayerId(0);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

fn world(seed: u64) -> Game {
    let c = shipped();
    let grass = c
        .terrains
        .id_numbered(num::terrain::GRASSLAND)
        .expect("id_of succeeds");
    let mut g = Game::new(seed);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    g.fog = crate::map::fog::Fog::new(24, 14);
    for id in [P0, PlayerId(1)] {
        g.add_player(id, "W");
        g.treasury_mut(id).expect("treasury_mut succeeds").gold =
            f4x_def::half_int::HalfInt::from_int(100);
        g.treasury_mut(id).expect("treasury_mut succeeds").mana =
            f4x_def::half_int::HalfInt::from_int(100);
    }
    g.init_wizard(P0, &[(RealmId::new(3 /* life */), 2)], &[], None)
        .expect("init_wizard succeeds");
    g
}

/// A guarded site, the feature numbered `site`, on Arcanus.
fn put_lair(g: &mut Game, x: u32, y: u32, site: u16, guards: Vec<(u16, u32)>) -> hecs::Entity {
    g.put_lair(at(x, y), site, lair(guards))
}

/// What the rules read of the site with the feature number `site`, with nothing in it; `reward`
/// is the realm of its book.
fn info(site: u16, reward: Option<u16>) -> LairInfo {
    LairInfo {
        entity: hecs::Entity::DANGLING,
        pos: at(1, 1),
        site: FeatureId(site),
        reward: reward.map(RealmId::new),
        intact: true,
        guards: Vec::new(),
        gold: 0,
        mana: 0,
        items: Vec::new(),
        prisoner: false,
        special: 0,
        fought_by: Vec::new(),
    }
}

fn lair(guards: Vec<(u16, u32)>) -> Lair {
    Lair {
        intact: true,
        guards: guards
            .into_iter()
            .map(|(g, n)| (UnitId::new(g), n))
            .collect(),
        gold: 120,
        mana: 30,
        items: vec![400],
        prisoner: true,
        special: 0,
        fought_by: Vec::new(),
    }
}

#[test]
fn a_stack_is_stopped_by_a_guarded_site_and_nothing_moves() -> TestResult {
    let mut g = world(2);
    let site = put_lair(
        &mut g,
        5,
        5,
        num::feature::MONSTER_LAIR,
        vec![(165 /* skeletons */, 3)],
    );
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let w = g.spawn_unit(P0, at(4, 5), walker);
    let events = g.apply(Command::MoveStack {
        units: vec![w],
        to: at(5, 5),
    });
    assert_eq!(
        events,
        vec![Event::SiteEncounter {
            units: vec![w],
            from: at(4, 5),
            to: at(5, 5),
            lair: site
        }]
    );
    assert_eq!(g.units().unit_position(w), Some(at(4, 5)));
    assert_eq!(g.units().unit_moves_left(w), 2, "no moves spent");
    Ok(())
}

#[test]
fn walking_into_an_unguarded_site_takes_the_treasure_and_clears_it() -> TestResult {
    let mut g = world(3);
    let site = put_lair(&mut g, 5, 5, num::feature::MONSTER_LAIR, vec![]);
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let w = g.spawn_unit(P0, at(4, 5), walker);
    let events = g.apply(Command::MoveStack {
        units: vec![w],
        to: at(5, 5),
    });
    assert!(matches!(events[0], Event::UnitsMoved { .. }));
    let Some(Event::SiteCleared { player, loot, .. }) = events.get(1) else {
        return Err(format!("expected the site to be cleared: {events:?}").into());
    };
    assert_eq!(*player, P0);
    assert_eq!((loot.gold, loot.mana), (120, 30));
    // The 400 hoard is made into an item, and the prisoner, who cannot be freed (the test
    // rules have no heroes), is rerolled into a second.
    assert!(!loot.prisoner);
    assert_eq!(loot.items.len(), 2);
    assert_eq!(g.items().armory(P0).items().count(), 2);
    assert_eq!(
        (
            g.player(P0)
                .ok_or("player is missing")?
                .treasury()
                .gold
                .floor(),
            g.player(P0)
                .ok_or("player is missing")?
                .treasury()
                .mana
                .floor()
        ),
        (220, 130)
    );
    assert!(g.economy().lair_at(at(5, 5)).is_none());
    assert_eq!(
        g.economy().feature_id_at(at(5, 5)),
        None,
        "the site is gone"
    );
    // Nothing to take the second time.
    let again = g.claim_lair(site, P0);
    assert!(again.is_empty());
    Ok(())
}

#[test]
fn the_hoard_can_hold_a_spell_book_a_retort_or_a_spell() -> TestResult {
    let mut g = world(4);
    let mut l = lair(vec![]);
    l.gold = 0;
    l.mana = 0;
    l.items.clear();
    l.prisoner = false;
    // A spell: from a realm the wizard has books in (or arcane), newly known.
    l.special = 2;
    let site = g.put_lair(at(5, 5), num::feature::MONSTER_LAIR, l);
    let known_before = g.wizard(P0).ok_or("wizard is missing")?.known().len();
    let events = g.claim_lair(site, P0);
    let Some(Event::SiteCleared { loot, .. }) = events.first() else {
        return Err(format!("{events:?}").into());
    };
    let spell = loot.spell.expect("a spell");
    assert!(g.wizard(P0).ok_or("wizard is missing")?.knows(spell));
    assert_eq!(
        g.wizard(P0).ok_or("wizard is missing")?.known().len(),
        known_before + 1
    );
    assert!(
        matches!(
            g.content.rules.spells.get(spell).realm.num(),
            3 /* life */ | 5 /* arcane */
        ),
        "a realm with books, or arcane"
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::SpellLearned { .. })));
    // A book of a dungeon's realm (death): refused while the wizard has life books.
    let l = info(7 /* dungeon */, Some(4 /* death */));
    let mut loot = Loot::default();
    let e = crate::wizard::entity::of_player(
        &g.world,
        g.player_entity(P0).ok_or("player_entity is missing")?,
    )
    .ok_or("of_player is missing")?;
    let mut w = crate::wizard::sheet::WizardMut::new(&mut g.world, e);
    award_special(&g.content.rules, &mut w, &mut g.rng, &l, 1, &mut loot);
    assert!(
        !loot.books.contains(&realm(4 /* death */)),
        "life and death exclude each other"
    );
    // A temple gives a life book, up to the limit (a wizard with twelve may still find a
    // thirteenth, as in the original).
    let l = info(3 /* ancient_temple */, Some(3 /* life */));
    let mut books = 0;
    for _ in 0..40 {
        let mut loot = Loot::default();
        let e = crate::wizard::entity::of_player(
            &g.world,
            g.player_entity(P0).ok_or("player_entity is missing")?,
        )
        .ok_or("of_player is missing")?;
        let mut w = crate::wizard::sheet::WizardMut::new(&mut g.world, e);
        award_special(&g.content.rules, &mut w, &mut g.rng, &l, 1, &mut loot);
        books += loot.books.len();
        assert!(loot.books.iter().all(|&b| b == realm(3 /* life */)));
    }
    assert!(books > 0);
    assert!(g.wizard(P0).ok_or("wizard is missing")?.total_books() <= 13);
    Ok(())
}

fn fight_to_the_end(g: &mut Game) {
    use crate::battle::map::distance;
    for _ in 0..300 {
        if !g.in_battle() {
            return;
        }
        let (side, ids): (Side, Vec<usize>) = {
            let b = g.battle().expect("battle succeeds");
            (b.active, b.active_units(b.active).map(|u| u.id).collect())
        };
        for id in ids {
            if !g.in_battle() {
                return;
            }
            let target = {
                let b = g.battle().expect("battle succeeds");
                b.active_units(side.other()).next().map(|t| (t.id, t.cell))
            };
            let Some((t, cell)) = target else { break };
            let step = {
                let b = g.battle().expect("battle succeeds");
                b.reachable(id)
                    .into_iter()
                    .min_by_key(|(p, _)| (distance(*p, cell), *p))
                    .map(|(p, _)| p)
            };
            if let Some(to) = step {
                g.apply(Command::Battle(BattleCommand::Move { unit: id, to }));
            }
            g.apply(Command::Battle(BattleCommand::Attack {
                unit: id,
                target: t,
            }));
        }
        if g.in_battle() {
            g.apply(Command::Battle(BattleCommand::EndTurn));
        }
    }
    panic!("the battle did not finish");
}

#[test]
fn beating_the_guards_clears_the_site_pays_out_and_moves_in() -> TestResult {
    let mut g = world(5);
    put_lair(
        &mut g,
        5,
        5,
        num::feature::MONSTER_LAIR,
        vec![(165, 1)], /* skeletons */
    );
    // A full stack of strong units against one skeleton.
    let walker = g
        .content
        .rules
        .units
        .id_numbered(113)
        .ok_or("high_men_paladins is not in the content")?;
    let stack: Vec<_> = (0..9).map(|_| g.spawn_unit(P0, at(4, 5), walker)).collect();
    let w = stack[0];
    let events = g.apply(Command::EnterSite {
        units: stack,
        to: at(5, 5),
    });
    assert!(matches!(events[0], Event::BattleStarted { defender, .. } if defender == MONSTERS));
    assert!(g.in_battle());
    assert_eq!(
        g.units().units_at(at(5, 5)).len(),
        1,
        "one guard stands on the site"
    );
    fight_to_the_end(&mut g);
    assert!(g.world.contains(w));
    assert!(g.economy().lair_at(at(5, 5)).is_none(), "cleared");
    assert_eq!(g.economy().feature_id_at(at(5, 5)), None);
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().gold,
        220
    );
    assert_eq!(
        g.units().unit_position(w),
        Some(at(5, 5)),
        "the victors moved in"
    );
    assert!(
        g.units()
            .units_at(at(5, 5))
            .iter()
            .all(|&u| g.units().unit_owner(u) == Some(P0)),
        "no guard is left"
    );
    Ok(())
}

#[test]
fn the_building_of_a_site_stands_in_the_middle_of_the_battlefield() -> TestResult {
    use crate::battle::map::layout::Structure;
    let mut g = world(5);
    put_lair(
        &mut g,
        5,
        5,
        num::feature::TOWER,
        vec![(165, 1)], /* skeletons */
    );
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let w = g.spawn_unit(P0, at(4, 5), walker);
    g.apply(Command::EnterSite {
        units: vec![w],
        to: at(5, 5),
    });
    let active = g.battle.as_ref().expect("a battle is on");
    let look = active.battle.map.look().expect("the field has a look");
    assert_eq!(look.structure, Structure::Tower);
    assert!(!active.battle.map.is_playable(crate::battle::map::STRUCTURE));
    Ok(())
}

#[test]
fn a_guard_drained_to_death_joins_the_victors_as_undead() -> TestResult {
    use crate::battle::Status;
    let mut g = world(5);
    put_lair(
        &mut g,
        5,
        5,
        num::feature::MONSTER_LAIR,
        vec![(165 /* skeletons */, 2)],
    );
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let w = g.spawn_unit(P0, at(4, 5), walker);
    g.apply(Command::EnterSite {
        units: vec![w],
        to: at(5, 5),
    });
    let (drained, slain) = {
        let a = g.battle.as_ref().ok_or("as_ref is missing")?;
        let guards: Vec<usize> = a
            .battle
            .units
            .iter()
            .filter(|u| u.side == Side::Defender)
            .map(|u| u.id)
            .collect();
        assert_eq!(guards.len(), 2);
        (
            (
                guards[0],
                a.sources[a.battle.units[guards[0]]
                    .source
                    .ok_or("source is missing")?],
            ),
            (
                guards[1],
                a.sources[a.battle.units[guards[1]]
                    .source
                    .ok_or("source is missing")?],
            ),
        )
    };
    {
        let b = &mut g.battle.as_mut().ok_or("as_mut is missing")?.battle;
        for (id, kinds) in [(drained.0, [1, 5, 0]), (slain.0, [4, 0, 0])] {
            b.units[id].status = Status::Dead;
            b.units[id].figures = 0;
            b.units[id].damage_kinds = kinds;
        }
        // The test guards are skeletons, creatures of the Death realm, which are not raised.
        b.units[drained.0].realm = None;
    }
    let events = g.apply(Command::Battle(BattleCommand::EndTurn));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::UndeadRaised { player, units: 1, .. } if *player == P0)));
    assert_eq!(g.units().unit_owner(drained.1), Some(P0));
    assert!(g.units().unit_is_undead(drained.1));
    assert_eq!(g.units().unit_position(drained.1), Some(at(5, 5)));
    assert!(!g.world.contains(slain.1), "the other guard is gone");
    assert_eq!(
        g.units().unit_position(w),
        Some(at(5, 5)),
        "the victors moved in"
    );
    Ok(())
}

#[test]
fn zombie_mastery_takes_any_normal_dead_but_not_the_destroyed() -> TestResult {
    use crate::battle::Status;
    let mut g = world(7);
    put_lair(
        &mut g,
        5,
        5,
        num::feature::MONSTER_LAIR,
        vec![(104 /* high_men_spearmen */, 2)],
    );
    // Give the winner Zombie Mastery.
    let zm = g
        .content
        .rules
        .spells
        .id_numbered(188)
        .ok_or("zombie_mastery is not in the content")?;
    g.add_global(P0, zm);
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let w = g.spawn_unit(P0, at(4, 5), walker);
    g.apply(Command::EnterSite {
        units: vec![w],
        to: at(5, 5),
    });
    let guards: Vec<(usize, hecs::Entity)> = {
        let a = g.battle.as_ref().ok_or("as_ref is missing")?;
        a.battle
            .units
            .iter()
            .filter(|u| u.side == Side::Defender)
            .map(|u| (u.id, a.sources[u.source.expect("source succeeds")]))
            .collect()
    };
    {
        let b = &mut g.battle.as_mut().ok_or("as_mut is missing")?.battle;
        // One struck down, one destroyed.
        for (i, (id, _)) in guards.iter().enumerate() {
            b.units[*id].status = Status::Dead;
            b.units[*id].figures = 0;
            b.units[*id].damage_kinds = if i == 0 { [4, 0, 0] } else { [0, 0, 1] };
            b.units[*id].realm = None;
        }
    }
    g.apply(Command::Battle(BattleCommand::EndTurn));
    assert_eq!(g.units().unit_owner(guards[0].1), Some(P0));
    assert!(g.units().unit_is_undead(guards[0].1));
    assert!(!g.world.contains(guards[1].1), "the destroyed one is gone");
    Ok(())
}

#[test]
fn an_attacker_drained_to_death_by_the_guards_stays_as_a_neutral_undead_unit() -> TestResult {
    use crate::battle::Status;
    let mut g = world(5);
    put_lair(
        &mut g,
        5,
        5,
        num::feature::MONSTER_LAIR,
        vec![(165, 1)], /* skeletons */
    );
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let (w, bystander) = (
        g.spawn_unit(P0, at(4, 5), walker),
        g.spawn_unit(P0, at(4, 5), walker),
    );
    g.apply(Command::EnterSite {
        units: vec![w, bystander],
        to: at(5, 5),
    });
    {
        let b = &mut g.battle.as_mut().ok_or("as_mut is missing")?.battle;
        for u in b.units.iter_mut().filter(|u| u.side == Side::Attacker) {
            u.status = Status::Dead;
            u.figures = 0;
            u.damage_kinds = if u.source == Some(0) {
                [1, 5, 0]
            } else {
                [4, 0, 0]
            };
            u.realm = None;
        }
    }
    let events = g.apply(Command::Battle(BattleCommand::EndTurn));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::UndeadRaised { player, units: 1, .. } if *player == MONSTERS)));
    // The drained unit stays on the site for the monsters; the guard made for the fight and
    // the other attacker are gone.
    assert!(g.world.contains(w) && !g.world.contains(bystander));
    assert_eq!(g.units().unit_owner(w), Some(MONSTERS));
    assert!(g.units().unit_is_undead(w));
    assert_eq!(g.units().units_at(at(5, 5)), vec![w]);
    assert!(
        g.economy().lair_at(at(5, 5)).is_some(),
        "the lair is not cleared"
    );
    // It lasts, and it is an enemy to anyone who walks in.
    g.apply(Command::EndTurn);
    assert_eq!(g.units().unit_owner(w), Some(MONSTERS));
    assert!(g.economy().hostile_at(at(5, 5), P0).is_some());
    Ok(())
}

#[test]
fn losing_or_fleeing_leaves_the_site_intact_and_resets_the_guards() -> TestResult {
    let mut g = world(6);
    put_lair(
        &mut g,
        5,
        5,
        num::feature::MONSTER_LAIR,
        vec![(165 /* skeletons */, 3)],
    );
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let w = g.spawn_unit(P0, at(4, 5), walker);
    g.apply(Command::EnterSite {
        units: vec![w],
        to: at(5, 5),
    });
    assert_eq!(g.units().units_at(at(5, 5)).len(), 3);
    g.apply(Command::Battle(BattleCommand::Flee));
    assert!(!g.in_battle());
    assert!(g.economy().lair_at(at(5, 5)).is_some());
    assert!(
        g.units().units_at(at(5, 5)).is_empty(),
        "the guards are made anew on each visit"
    );
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().gold,
        100,
        "no treasure"
    );
    assert_eq!(g.units().unit_position(w), Some(at(4, 5)));
    Ok(())
}

#[test]
fn only_a_guarded_site_next_to_the_stack_can_be_entered() -> TestResult {
    let mut g = world(7);
    put_lair(
        &mut g,
        5,
        5,
        num::feature::MONSTER_LAIR,
        vec![(165, 1)], /* skeletons */
    );
    put_lair(&mut g, 9, 9, num::feature::MONSTER_LAIR, vec![]);
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let w = g.spawn_unit(P0, at(2, 2), walker);
    let reject = |g: &mut Game, to| g.apply(Command::EnterSite { units: vec![w], to });
    assert_eq!(
        reject(&mut g, at(5, 5)),
        vec![Event::Rejected(crate::game::Rejection::Site(
            SiteError::Move(MoveError::NotAdjacent)
        ))]
    );
    assert_eq!(
        reject(&mut g, at(3, 3)),
        vec![Event::Rejected(crate::game::Rejection::Site(
            SiteError::NoSite
        ))]
    );
    assert!(!g.in_battle());
    Ok(())
}

#[test]
fn a_tower_is_cleared_on_both_planes_and_stays() {
    let mut g = world(8);
    let tower = put_lair(&mut g, 5, 5, num::feature::TOWER, vec![]);
    let other_plane = at(5, 5).on_other_plane();
    assert_eq!(g.economy().feature_entity_at(other_plane), Some(tower));
    g.claim_lair(tower, P0);
    assert!(g.economy().lair_at(at(5, 5)).is_none());
    assert!(g.economy().lair_at(other_plane).is_none());
    assert!(
        g.economy().feature_id_at(at(5, 5)).is_some(),
        "the tower itself stays"
    );
}

#[test]
fn lairs_survive_a_save() -> TestResult {
    let mut g = world(9);
    put_lair(
        &mut g,
        5,
        5,
        num::feature::MONSTER_LAIR,
        vec![(165 /* skeletons */, 2), (166 /* ghouls */, 1)],
    );
    let snap = g.snapshot();
    let back = snap.restore(g.content.clone())?;
    assert_eq!(back.economy().lairs(), g.economy().lairs());
    Ok(())
}

#[test]
fn clearing_a_site_takes_its_treasure_once() {
    let mut g = world(10);
    let site = put_lair(
        &mut g,
        5,
        5,
        num::feature::MONSTER_LAIR,
        vec![(107 /* high_men_cavalry */, 1)],
    );
    let cleared = g.clear_site(site).expect("an intact lair");
    assert_eq!(
        (cleared.gold, cleared.mana, cleared.items),
        (120, 30, vec![400])
    );
    assert!(g.economy().lair_at(at(5, 5)).is_none());
    assert!(g.clear_site(site).is_none(), "nothing the second time");
}

#[test]
fn clearing_a_tower_leaves_another_tower_alone() -> TestResult {
    let mut g = world(11);
    let a = put_lair(&mut g, 3, 3, num::feature::TOWER, vec![]);
    put_lair(&mut g, 8, 8, num::feature::TOWER, vec![]);
    g.clear_site(a).ok_or("clear_site is missing")?;
    assert!(g.economy().lair_at(at(3, 3)).is_none());
    assert!(
        g.economy().lair_at(at(8, 8)).is_some(),
        "another tower stays"
    );
    Ok(())
}

/// A wizard's sheet with no game: a world holding one wizard.
fn sheet(
    books: &[(RealmId, u8)],
    retorts: &[crate::wizard::magic::Retort],
) -> (hecs::World, hecs::Entity) {
    let mut world = hecs::World::new();
    let e = world.spawn(crate::wizard::sheet::bundle());
    crate::wizard::sheet::WizardMut::new(&mut world, e).found(&shipped().rules, books, retorts);
    (world, e)
}

#[test]
fn a_hoard_of_books_obeys_the_realm_and_the_limit_without_a_game() {
    let rules = shipped().rules;
    let (mut world, e) = sheet(&[(RealmId::new(3 /* life */), 11)], &[]);
    let mut magic = crate::wizard::sheet::WizardMut::new(&mut world, e);
    let mut rng = crate::game::rng::GameRng::new(5);
    let mut l = info(18 /* ruins */, Some(4 /* death */));
    for _ in 0..30 {
        let mut loot = Loot::default();
        award_special(&rules, &mut magic, &mut rng, &l, 1, &mut loot);
        assert!(loot.books.is_empty(), "death clashes with life");
    }
    l.site = FeatureId(3 /* ancient_temple */);
    l.reward = Some(RealmId::new(3 /* life */));
    for _ in 0..60 {
        award_special(&rules, &mut magic, &mut rng, &l, 2, &mut Loot::default());
    }
    assert_eq!(magic.view().total_books(), 13);
}

#[test]
fn the_limits_of_six_retorts_and_thirteen_books_decide_between_a_book_and_a_retort() {
    let rules = shipped().rules;
    use crate::wizard::magic::Retort;
    let l = info(15 /* nature_node */, Some(0 /* nature */));
    // Six retorts: only books can come.
    let (mut world, e) = sheet(
        &[(RealmId::new(0 /* nature */), 3)],
        &[
            Retort::new(0),  /* alchemy */
            Retort::new(1),  /* warlord */
            Retort::new(16), /* charismatic */
            Retort::new(13), /* famous */
            Retort::new(17), /* artificer */
            Retort::new(15), /* conjurer */
        ],
    );
    let mut magic = crate::wizard::sheet::WizardMut::new(&mut world, e);
    let mut rng = crate::game::rng::GameRng::new(9);
    for _ in 0..40 {
        let mut loot = Loot::default();
        award_special(&rules, &mut magic, &mut rng, &l, 1, &mut loot);
        assert!(loot.retorts.is_empty());
    }
    assert_eq!(magic.view().retorts().len(), 6);
    // Twelve books: one more can come (the original refuses a book only above twelve).
    let (mut world, e) = sheet(&[(RealmId::new(0 /* nature */), 12)], &[]);
    let mut magic = crate::wizard::sheet::WizardMut::new(&mut world, e);
    let mut found = false;
    for _ in 0..40 {
        let mut loot = Loot::default();
        award_special(&rules, &mut magic, &mut rng, &l, 1, &mut loot);
        found |= !loot.books.is_empty();
    }
    assert!(found, "a wizard with twelve books finds a thirteenth");
    assert_eq!(magic.view().total_books(), 13);
    // Thirteen books: only retorts can come.
    let (mut world, e) = sheet(
        &[
            (RealmId::new(0 /* nature */), 7),
            (RealmId::new(1 /* sorcery */), 6),
        ],
        &[],
    );
    let mut magic = crate::wizard::sheet::WizardMut::new(&mut world, e);
    for _ in 0..40 {
        let mut loot = Loot::default();
        award_special(&rules, &mut magic, &mut rng, &l, 1, &mut loot);
        assert!(loot.books.is_empty());
    }
    assert_eq!(magic.view().total_books(), 13);
}

#[test]
fn a_spell_award_teaches_a_spell_the_wizard_can_use() {
    let c = shipped();
    let (mut world, e) = sheet(&[(RealmId::new(3 /* life */), 2)], &[]);
    let mut magic = crate::wizard::sheet::WizardMut::new(&mut world, e);
    let mut rng = crate::game::rng::GameRng::new(9);
    let spell = award_spell(&mut magic, &c, &mut rng, 1).expect("a spell");
    assert!(magic.view().knows(spell));
    let def = c.rules.spells.get(spell);
    assert!(
        matches!(def.realm.num(), 3 /* life */ | 5 /* arcane */),
        "{:?}",
        def.realm
    );
    // One book gives up to uncommon spells, arcane up to rare, even from the dearest hoard.
    for seed in 0..30 {
        let (mut world, e) = sheet(&[(RealmId::new(3 /* life */), 1)], &[]);
        let mut magic = crate::wizard::sheet::WizardMut::new(&mut world, e);
        let mut rng = crate::game::rng::GameRng::new(seed);
        let spell = award_spell(&mut magic, &c, &mut rng, 4).expect("a spell");
        let def = c.rules.spells.get(spell);
        let cap = if def.realm.num() == 5
        /* arcane */
        {
            Rarity::Rare
        } else {
            Rarity::Uncommon
        };
        assert!(def.rarity <= cap, "{} {:?}", def.id, def.rarity);
    }
}

/// A hoard with a spell, a book and an item, and a prisoner, on Arcanus.
fn rich_lair() -> Lair {
    Lair {
        intact: true,
        guards: Vec::new(),
        gold: 100,
        mana: 50,
        items: vec![600],
        prisoner: true,
        special: 2,
        fought_by: Vec::new(),
    }
}

/// The lair world with a hero roster and an independent city that has a home at (2, 2).
fn heroic_world() -> (Game, PlayerId) {
    let mut g = crate::hero::game::tests::world();
    let lair_feature = world(1).content.features;
    g.content.features = lair_feature;
    let town = PlayerId(9);
    g.spawn_city(town, at(2, 2), "Free Town".into(), None, 4);
    (g, town)
}

#[test]
fn a_neutral_winner_gets_no_spell_and_the_value_comes_back_as_gold_or_mana() -> TestResult {
    let (mut g, town) = heroic_world();
    let mut l = rich_lair();
    l.prisoner = false;
    l.items.clear();
    l.special = 4;
    let site = g.put_lair(at(5, 5), num::feature::MONSTER_LAIR, l);
    let before = g.player(town).ok_or("player is missing")?.treasury();
    let events = g.claim_lair(site, town);
    let Some(Event::SiteCleared { loot, .. }) = events.first() else {
        return Err(format!("{events:?}").into());
    };
    assert!(loot.spell.is_none() && loot.books.is_empty() && loot.retorts.is_empty());
    assert!(
        loot.gold + loot.mana > 150,
        "the 800-point spell comes back as gold and mana: {loot:?}"
    );
    assert!(loot.items.is_empty(), "no hero, no item");
    let t = g.player(town).ok_or("player is missing")?.treasury();
    assert_eq!(
        (
            (t.gold - before.gold).floor() as u32,
            (t.mana - before.mana).floor() as u32
        ),
        (loot.gold, loot.mana)
    );
    Ok(())
}

#[test]
fn a_neutral_winner_without_a_hero_gets_no_item_and_a_prisoner_joins_it_at_home() -> TestResult {
    let (mut g, town) = heroic_world();
    let mut l = rich_lair();
    l.special = 0;
    let site = g.put_lair(at(5, 5), num::feature::MONSTER_LAIR, l);
    let events = g.claim_lair(site, town);
    let Some(Event::SiteCleared { loot, .. }) = events.first() else {
        return Err(format!("{events:?}").into());
    };
    assert!(loot.prisoner, "the hero serves the city");
    let heroes = g.units().hero_units(town);
    assert_eq!(heroes.len(), 1);
    assert_eq!(g.units().unit_position(heroes[0]), Some(at(2, 2)));
    assert_eq!(g.units().unit_owner(heroes[0]), Some(town));
    // The hero came with the same claim, so the item is kept.
    assert_eq!(loot.items.len(), 1);
    assert!(g
        .player_component::<crate::player::Offers>(town)
        .ok_or("value is missing")?
        .0
        .is_empty());
    Ok(())
}

#[test]
fn a_prisoner_with_no_free_hero_left_is_rerolled_for_a_wizard_too() -> TestResult {
    let mut g = crate::hero::game::tests::world();
    g.content.features = world(1).content.features;
    for kind in 0..g.content.rules.hero_templates.len() {
        g.spawn_hero(P0, at(1, 1), kind)
            .map_err(|e| format!("{e:?}"))?;
    }
    assert!(!g.units().hero_summonable(P0, crate::hero::kind::Class::Any));
    let mut l = rich_lair();
    l.special = 0;
    l.gold = 0;
    l.mana = 0;
    l.items.clear();
    let site = g.put_lair(at(5, 5), num::feature::MONSTER_LAIR, l);
    let events = g.claim_lair(site, P0);
    let Some(Event::SiteCleared { loot, .. }) = events.first() else {
        return Err(format!("{events:?}").into());
    };
    assert!(!loot.prisoner);
    assert!(g
        .player_component::<crate::player::Offers>(P0)
        .ok_or("value is missing")?
        .0
        .is_empty());
    assert!(
        loot.gold + loot.mana > 0 || !loot.items.is_empty(),
        "{loot:?}"
    );
    Ok(())
}

#[test]
fn rerolled_value_never_exceeds_the_value() {
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(9);
    for value in [50, 300, 1000, 3000] {
        for _ in 0..50 {
            let (gold, mana, items) = reroll_hoard(&mut rng, value, 2);
            assert!(items.len() <= 2);
            assert!(gold + mana + items.iter().sum::<u32>() <= value as u32 + 200);
            assert!(gold + mana + items.iter().sum::<u32>() > 0);
        }
    }
}

#[test]
fn whoever_fought_for_a_site_once_knows_its_guards_even_after_fleeing() -> TestResult {
    let mut g = world(6);
    put_lair(
        &mut g,
        5,
        5,
        num::feature::MONSTER_LAIR,
        vec![(165 /* skeletons */, 3)],
    );
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let w = g.spawn_unit(P0, at(4, 5), walker);
    assert_eq!(g.economy().lair_guards_known_to(at(5, 5), P0), None);
    g.apply(Command::EnterSite {
        units: vec![w],
        to: at(5, 5),
    });
    g.apply(Command::Battle(BattleCommand::Flee));
    assert_eq!(
        g.economy().lair_guards_known_to(at(5, 5), P0),
        Some(vec![(UnitId::new(165) /* skeletons */, 3)])
    );
    // Only the one who fought knows.
    assert_eq!(
        g.economy().lair_guards_known_to(at(5, 5), PlayerId(1)),
        None
    );
    // And it is remembered in a save.
    let back = g.snapshot().restore(g.content.clone())?;
    assert!(back.economy().lair_guards_known_to(at(5, 5), P0).is_some());
    Ok(())
}

#[test]
fn a_game_saved_in_a_lair_fight_goes_on_to_clear_the_site() -> TestResult {
    let mut g = world(5);
    let site = put_lair(
        &mut g,
        5,
        5,
        num::feature::MONSTER_LAIR,
        vec![(165 /* skeletons */, 2)],
    );
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let w = g.spawn_unit(P0, at(4, 5), walker);
    g.apply(Command::EnterSite {
        units: vec![w],
        to: at(5, 5),
    });
    assert!(g.in_battle());
    let text = g.snapshot().to_text()?;
    let mut loaded = crate::game::save::Snapshot::from_text(&text)?.restore(g.content.clone())?;
    assert!(loaded.in_battle());
    assert_eq!(
        loaded
            .battle
            .as_ref()
            .and_then(|a| a.lair)
            .map(|l| loaded.serial_of(l)),
        Some(g.serial_of(site)),
        "the fight is still the assault on the same lair"
    );
    fight_to_the_end(&mut g);
    fight_to_the_end(&mut loaded);
    assert_eq!(g.snapshot().to_text()?, loaded.snapshot().to_text()?);
    Ok(())
}
