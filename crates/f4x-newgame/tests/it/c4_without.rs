//! Class 4 (`docs/f4x-newgame-test-design.md`): whole new games with heroes and items off, and
//! with lairs off, through the public interface only.

use f4x_core::player::PlayerId;
use f4x_core::world::World;
use f4x_def::content::Content;
use f4x_def::feature::FeatureKind;
use f4x_def::testdir::{sample, TestResult};
use f4x_newgame::error::NewGameError;
use f4x_newgame::setup::{new_game, NewGame};

fn shipped() -> &'static Content {
    f4x_def::testdir::shipped()
}

fn game(seed: u64, change: impl FnOnce(&mut NewGame)) -> Result<World, NewGameError> {
    let mut spec = NewGame {
        seed,
        all_ai: true,
        ..NewGame::with_wizards(5)
    };
    change(&mut spec);
    new_game(shipped(), &spec).map(|(world, _)| world)
}

fn wizards(w: &World) -> impl Iterator<Item = PlayerId> + '_ {
    w.player_ids()
        .into_iter()
        .filter(|&p| w.wizard(p).is_some())
}

#[test]
fn without_heroes_no_wizard_has_a_retort_or_a_spell_that_needs_them() -> TestResult {
    for seed in 0..sample(200, 20) as u64 {
        let w = game(seed, |s| s.settings.world.heroes_enabled = false)?;
        let rules = &w.content().rules;
        for p in wizards(&w) {
            let wizard = w.wizard(p).ok_or("a wizard")?;
            for r in wizard.retorts() {
                assert!(!rules.retorts.get(r).needs_heroes, "seed {seed}: {r:?}");
            }
            for (id, def) in rules.spells.iter().filter(|(_, d)| d.needs_heroes) {
                assert!(
                    !wizard.knows(id) && !wizard.can_research(id),
                    "seed {seed}, {p:?}: {}",
                    def.id
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_ready_made_wizard_takes_its_other_loadout_without_heroes() -> TestResult {
    let defs = shipped().rules.wizards.defs();
    assert!(defs.iter().any(|d| d.without_heroes.is_some()));
    for def in defs.iter().filter(|d| d.without_heroes.is_some()) {
        for heroes in [true, false] {
            let w = game(1, |s| {
                s.all_ai = false;
                s.preset = Some(def.num);
                s.settings.world.heroes_enabled = heroes;
            })?;
            let wizard = w.wizard(PlayerId(0)).ok_or("the human wizard")?;
            let (books, retorts) = def.loadout(heroes);
            for &(realm, n) in books {
                assert_eq!(wizard.books_in(realm), i32::from(n), "{}", def.id);
            }
            assert_eq!(
                wizard.retorts(),
                retorts.iter().copied().collect(),
                "{}, heroes {heroes}",
                def.id
            );
        }
    }
    Ok(())
}

#[test]
fn a_custom_wizard_cannot_pick_a_retort_that_needs_heroes_without_them() -> TestResult {
    let rules = &shipped().rules;
    let (retort, def) = rules
        .retorts
        .iter()
        .find(|(_, d)| d.needs_heroes)
        .ok_or("a retort that needs heroes")?;
    let picks = NewGame::default().settings.wizards.picks.human;
    for heroes in [true, false] {
        let made = game(1, |s| {
            s.all_ai = false;
            s.books = [(
                f4x_core::rules::RealmId::new(1 /* sorcery */),
                picks - def.cost,
            )]
            .into();
            s.retorts = [retort].into();
            s.settings.world.heroes_enabled = heroes;
        });
        assert_eq!(made.is_ok(), heroes, "heroes {heroes}");
    }
    Ok(())
}

/// The number of encounter sites and of guarded towers and nodes of a game.
fn sites(w: &World) -> (usize, usize) {
    let economy = w.economy();
    let encounters = economy
        .lairs()
        .iter()
        .filter(|l| {
            economy
                .feature_at(l.pos)
                .is_some_and(|f| f.kind == FeatureKind::Encounter)
        })
        .count();
    (encounters, economy.lairs().len() - encounters)
}

#[test]
fn without_lairs_the_map_has_towers_and_nodes_but_no_encounter_sites() -> TestResult {
    for seed in 0..sample(100, 6) as u64 {
        for lairs in [true, false] {
            let w = game(seed, |s| {
                s.settings.world.lairs_enabled = lairs;
                // The counts asked for are not read without lairs.
                s.settings.world.strong_sites = Some(20);
                s.settings.world.weak_sites = Some(20);
            })?;
            let (encounters, others) = sites(&w);
            assert!(others > 0, "seed {seed}, lairs {lairs}");
            assert_eq!(encounters > 0, lairs, "seed {seed}: {encounters}");
        }
    }
    Ok(())
}
