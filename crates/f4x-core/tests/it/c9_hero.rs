//! Class 9, the heroes: one individual of each type in the game, what is rolled for it, who may
//! hire it and at what price, what it adds while it serves, and what becomes of it and its items
//! when it leaves the army.

use crate::support::{around, content, text, worlds};
use f4x_battle::battle::{BattleCommand, Side};
use f4x_core::hero::kind::{self, Class, Standing, HEROES};
use f4x_core::hero::{Hero, HeroAbilities, HeroError, HeroState, Tier};
use f4x_core::item::armory::{ItemSpot, Placed};
use f4x_core::item::Item;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::rules::{Ability, PickType, RealmId, UnitId};
use f4x_core::wizard::magic::Retort;
use f4x_core::world::rng::GameRng;
use f4x_core::world::save::Snapshot;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event, World};
use f4x_def::half_int::HalfInt;
use f4x_def::testdir::{deep, sample};
use std::collections::BTreeSet;

fn kinds(w: &World) -> std::ops::Range<usize> {
    0..w.content().rules.hero_templates.len()
}

fn state_of(w: &World, kind: usize) -> HeroState {
    let hero = w.units().hero_entity(kind).expect("the hero");
    w.units().hero_state(hero).expect("a state")
}

fn fortress(w: &World, p: PlayerId) -> Position {
    w.wizard(p)
        .expect("a wizard")
        .fortress()
        .expect("a fortress")
}

fn join(w: &mut Table, p: PlayerId, at: Position, kind: usize) -> hecs::Entity {
    w.spawn_hero(p, at, kind)
        .unwrap_or_else(|e| panic!("hero {kind}: {e:?}"))
}

/// The record rolled for a serving hero.
fn record(w: &World, kind: usize) -> Hero {
    let hero = w.units().hero_entity(kind).expect("the hero");
    let snapshot = w.snapshot();
    let serial = w.units().serial_of(hero);
    snapshot
        .heroes
        .iter()
        .find(|h| h.serial == serial.0)
        .map(|h| h.hero.clone())
        .expect("a record")
}

/// A hero of the kind's roll with some of it changed, in the army of `p`.
fn changed_hero(
    w: &mut Table,
    p: PlayerId,
    at: Position,
    kind: usize,
    change: impl FnOnce(&mut Hero),
) -> hecs::Entity {
    let content = w.content().clone();
    let names = w.names().clone();
    let mut rng = GameRng::new(1);
    let template = &content.rules.hero_templates[kind];
    let mut hero = kind::roll_one(&mut rng, &content.rules, &names, kind, template);
    change(&mut hero);
    w.scenario_spawn_hero_copy(p, at, hero)
        .unwrap_or_else(|e| panic!("hero {kind}: {e:?}"))
}

// ---- what a hero is ----

#[test]
fn a_game_has_one_hero_of_each_type_with_a_name_and_no_wizard_has_one_at_the_start() {
    for (name, w) in worlds() {
        let all: Vec<_> = w
            .units()
            .all_heroes()
            .into_iter()
            .map(|h| w.units().hero_kind(h).expect("a kind"))
            .collect();
        assert_eq!(all, kinds(&w).collect::<Vec<_>>(), "{name}");
        for k in kinds(&w) {
            assert_eq!(state_of(&w, k), HeroState::Free, "{name}: {k}");
            assert!(
                w.units().hero_type_name(k).is_some_and(|n| !n.is_empty()),
                "{name}: {k}"
            );
            let unit = kind::unit_of(&w.content().rules, k).expect("a unit");
            assert_eq!(kind::kind_of(&w.content().rules, unit), Some(k), "{name}");
        }
        for p in w.wizards() {
            assert!(w.units().heroes(p).is_empty(), "{name}");
        }
    }
}

/// The picks a record holds beyond what its template gives, split into the fighter's, the
/// mage's and those of either.
fn picked(abilities: &HeroAbilities, fixed: &HeroAbilities) -> (i32, i32, i32) {
    let tiers = |names: &[&str]| -> i32 {
        names
            .iter()
            .map(|n| {
                abilities.tier(n).expect("a tier") as i32 - fixed.tier(n).expect("a tier") as i32
            })
            .sum()
    };
    let martial = tiers(&[
        "leadership",
        "legendary",
        "blademaster",
        "armsmaster",
        "constitution",
        "might",
        "agility",
    ]);
    let magic = tiers(&["prayermaster", "sage", "arcane_power"]);
    let either = [
        (abilities.lucky, fixed.lucky),
        (abilities.charmed, fixed.charmed),
        (abilities.noble, fixed.noble),
    ]
    .iter()
    .map(|&(a, f)| i32::from(a) - i32::from(f))
    .sum();
    (martial, magic, either)
}

#[test]
fn the_roll_spends_exactly_the_picks_of_the_template_on_what_the_hero_may_have() {
    let content = content();
    let rules = &content.rules;
    let names = f4x_def::names::NameBundle::default();
    let mut casters = 0;
    for seed in 0..sample(1500, 60) {
        let heroes = kind::roll(&mut GameRng::new(seed as u64), rules, &names);
        assert_eq!(heroes.len(), rules.hero_templates.len());
        for (k, (hero, t)) in heroes.iter().zip(&rules.hero_templates).enumerate() {
            let what = format!("seed {seed}, hero {k}");
            assert_eq!(hero.kind, k, "{what}");
            assert_eq!(hero.spells, t.spells, "{what}");
            let fixed = HeroAbilities::from_ids(&t.abilities);
            let (martial, magic, either) = picked(&hero.abilities, &fixed);
            let skill = i32::from(hero.casting_skill) - t.casting_skill as i32;
            assert!(
                martial >= 0 && magic >= 0 && either >= 0 && skill >= 0,
                "{what}"
            );
            assert_eq!(
                martial + magic + either + skill,
                t.picks as i32,
                "{what}: {hero:?}"
            );
            match t.pick_type {
                PickType::Warrior => assert_eq!((magic, skill), (0, 0), "{what}"),
                PickType::Mage => assert_eq!(martial, 0, "{what}"),
                PickType::Any => {}
            }
            let unit = rules.units.get(t.unit);
            if unit
                .abilities
                .contains(&Ability::new(40 /* magic_immunity */))
            {
                assert_eq!(hero.abilities.charmed, fixed.charmed, "{what}");
            }
            if !rules
                .config
                .abilities
                .magic_ranged
                .iter()
                .any(|&n| unit.abilities.contains(&Ability::new(n)))
            {
                assert_eq!(hero.abilities.arcane_power, fixed.arcane_power, "{what}");
            }
            casters += i32::from(skill > 0);
        }
    }
    assert!(casters > 0);
}

#[test]
fn a_seed_rolls_the_same_heroes_and_another_seed_others() {
    let content = content();
    let names = f4x_def::names::NameBundle::default();
    let roll = |seed| kind::roll(&mut GameRng::new(seed), &content.rules, &names);
    assert_eq!(roll(4), roll(4));
    assert_ne!(roll(4), roll(5));
}

#[test]
fn a_cheat_rolls_a_heros_abilities_anew_and_keeps_its_name() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let at = fortress(&w, p);
        let k = w
            .content()
            .rules
            .hero_templates
            .iter()
            .position(|t| t.picks > 0)
            .expect("a hero with picks");
        join(&mut w, p, at, k);
        let before = record(&w, k);
        let mut changed = false;
        for _ in 0..20 {
            let (world, rng) = w.world_and_rng();
            world
                .cheat_reroll_hero(k, rng)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            let now = record(&w, k);
            assert_eq!(now.name, before.name, "{name}");
            changed |= now.abilities != before.abilities;
        }
        assert!(changed, "{name}");
        let (world, rng) = w.world_and_rng();
        assert!(world.cheat_reroll_hero(999, rng).is_err(), "{name}");
    }
}

// ---- who is picked and what it costs ----

#[test]
fn fame_books_and_state_decide_who_can_be_offered() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        w.cheat_set_books(p, RealmId::new(3 /* life */), 0);
        w.cheat_set_books(p, RealmId::new(4 /* death */), 0);
        let picks = |w: &mut Table, fame: i32, free: bool, class: Class| {
            w.set_fame(p, fame);
            let mut seen = BTreeSet::new();
            for _ in 0..sample(3000, 600) {
                let (world, rng) = w.world_and_rng();
                if let Some(k) = world.pick_hero(p, free, class, rng) {
                    seen.insert(k);
                }
            }
            seen
        };
        let unknown = picks(&mut w, 0, false, Class::Any);
        let known = picks(&mut w, 20, false, Class::Any);
        let famous = picks(&mut w, 200, false, Class::Any);
        assert!(
            unknown.is_subset(&known) && known.is_subset(&famous) && unknown.len() < famous.len(),
            "{name}: {unknown:?} {known:?} {famous:?}"
        );
        // The dearer the hero, the more fame it asks: those a lesser name draws are the cheaper.
        let cost = |k: &usize| {
            let unit = kind::unit_of(&w.content().rules, *k).expect("a unit");
            w.content().rules.units.get(unit).cost
        };
        let dearest_known = known.iter().map(cost).max().expect("somebody");
        assert!(
            famous.difference(&known).all(|k| cost(k) > dearest_known),
            "{name}"
        );
        // The holy and the unholy ask for a book of their realm.
        for k in [
            27, /* priestess */
            28, /* paladin */
            29, /* black_knight */
            32, /* necromancer */
        ] {
            assert!(!famous.contains(&k), "{name}: {k}");
        }
        // A summoning asks for no fame.
        assert_eq!(picks(&mut w, 0, true, Class::Any), famous, "{name}");
        w.cheat_set_books(p, RealmId::new(3 /* life */), 1);
        let life = picks(&mut w, 200, true, Class::Any);
        assert!(
            life.contains(&27 /* priestess */) && life.contains(&28 /* paladin */),
            "{name}"
        );
        assert!(!life.contains(&32 /* necromancer */), "{name}");
        w.cheat_set_books(p, RealmId::new(4 /* death */), 1);
        let both = picks(&mut w, 200, true, Class::Any);
        assert!(
            both.contains(&29 /* black_knight */) && both.contains(&32 /* necromancer */),
            "{name}"
        );
        // Heroes and champions are drawn from their own halves.
        let heroes = picks(&mut w, 200, true, Class::Hero);
        let champions = picks(&mut w, 200, true, Class::Champion);
        assert!(heroes.iter().all(|&k| k < HEROES), "{name}: {heroes:?}");
        assert!(
            !champions.is_empty() && champions.iter().all(|&k| k >= HEROES),
            "{name}: {champions:?}"
        );
        // With no champion free, a champion's summoning takes whoever is.
        for &k in &champions {
            let at = fortress(&w, p);
            join(&mut w, p, at, k);
        }
        let rest = picks(&mut w, 200, true, Class::Champion);
        assert!(
            !rest.is_empty() && rest.iter().all(|&k| k < HEROES),
            "{name}: {rest:?}"
        );
        // Nobody who serves, lies dead or is gone is picked.
        let standing = Standing {
            fame: 200,
            life_books: 1,
            death_books: 1,
        };
        let rules = &w.content().rules;
        for state in [HeroState::Serving, HeroState::Dead, HeroState::Gone] {
            let states = vec![state; rules.hero_templates.len()];
            assert!(
                (0..states.len()).all(|k| !kind::can_offer(rules, &states, k, &standing)),
                "{name}: {state:?}"
            );
            let mut rng = GameRng::new(1);
            assert_eq!(
                kind::pick(&mut rng, rules, &states, &standing, true, Class::Champion),
                None,
                "{name}: {state:?}"
            );
        }
    }
}

#[test]
fn a_free_pick_finds_the_last_hero_left() {
    let standing = Standing {
        fame: 0,
        life_books: 1,
        death_books: 1,
    };
    for (name, w) in worlds() {
        let rules = &w.content().rules;
        let mut rng = GameRng::new(1);
        // Every type but the last, who only a spell of its own brings.
        for last in 0..rules.hero_templates.len() - 1 {
            let mut states = vec![HeroState::Gone; rules.hero_templates.len()];
            states[last] = HeroState::Free;
            for class in [Class::Any, Class::Champion] {
                for _ in 0..sample(100, 10) {
                    assert_eq!(
                        kind::pick(&mut rng, rules, &states, &standing, true, class),
                        Some(last),
                        "{name}: {class:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_price_grows_with_the_level_a_hero_kept_and_falls_for_a_charismatic_wizard() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        w.cheat_set_retort(p, Retort::new(16 /* charismatic */), false);
        for k in kinds(&w) {
            let what = format!("{name}: {k}");
            let unit = kind::unit_of(&w.content().rules, k).expect("a unit");
            let cost = w
                .content()
                .rules
                .units
                .get(unit)
                .hero
                .as_ref()
                .expect("a hero")
                .hire_cost as i32;
            assert_eq!(w.units().hero_hire_price(p, k), Some(cost), "{what}");
            assert_eq!(w.units().hero_kept_level(k), 0, "{what}");
            let hero = w.units().hero_entity(k).expect("the hero");
            let mut last = (0, cost);
            for xp in [30, 150, 1000, 100_000] {
                w.scenario_set_experience(hero, xp);
                let level = w.units().hero_kept_level(k);
                assert_eq!(
                    level,
                    kind::level_of(&w.content().rules, unit, xp),
                    "{what}"
                );
                let price = w.units().hero_hire_price(p, k).expect("a price");
                assert!(
                    level >= last.0 && (price > last.1) == (level > last.0),
                    "{what}: {last:?} then level {level} at {price}"
                );
                last = (level, price);
            }
            assert!(last.0 > 0, "{what}: no level gained");
            w.cheat_set_retort(p, Retort::new(16 /* charismatic */), true);
            let cheap = w.units().hero_hire_price(p, k).expect("a price");
            assert!(cheap < last.1, "{what}: {cheap} against {}", last.1);
            w.cheat_set_retort(p, Retort::new(16 /* charismatic */), false);
            w.scenario_set_experience(hero, 0);
        }
        assert_eq!(w.units().hero_hire_price(p, 999), None, "{name}");
    }
}

#[test]
fn hiring_takes_the_price_and_refuses_the_poor_the_unknown_and_the_taken() {
    for (name, mut w) in worlds() {
        let (p, q) = (w.wizards()[0], w.wizards()[1]);
        let price = w.units().hero_hire_price(p, 0).expect("a price");
        w.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(price - 1);
        assert_eq!(w.hire_hero(p, 0), Err(HeroError::NoGold), "{name}");
        assert_eq!(state_of(&w, 0), HeroState::Free, "{name}");
        w.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(price + 7);
        let hero = w
            .hire_hero(p, 0)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(
            w.player(p).expect("a player").treasury().gold,
            HalfInt::from_int(7),
            "{name}"
        );
        assert_eq!(w.units().unit_owner(hero), Some(p), "{name}");
        assert_eq!(state_of(&w, 0), HeroState::Serving, "{name}");
        w.treasury_mut(q).expect("a treasury").gold = HalfInt::from_int(100_000);
        assert_eq!(w.hire_hero(q, 0), Err(HeroError::NotAvailable), "{name}");
        assert_eq!(w.hire_hero(q, 999), Err(HeroError::Unknown), "{name}");
        let there = fortress(&w, q);
        assert_eq!(
            w.spawn_hero(q, there, 999),
            Err(HeroError::Unknown),
            "{name}"
        );
        assert!(w.units().heroes(q).is_empty(), "{name}");
    }
}

// ---- in service ----

#[test]
fn a_hero_that_joins_stands_ready_where_it_was_sent_with_its_record() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let at = fortress(&w, p);
        let taken: Vec<usize> = kinds(&w).step_by(if deep() { 1 } else { 5 }).collect();
        for &k in &taken {
            let what = format!("{name}: {k}");
            let hero = join(&mut w, p, at, k);
            assert_eq!(w.units().hero_entity(k), Some(hero), "{what}");
            assert_eq!(w.units().unit_position(hero), Some(at), "{what}");
            assert_eq!(w.units().unit_owner(hero), Some(p), "{what}");
            assert_eq!(
                w.units().unit_moves_left(hero),
                w.units().unit_max_moves(hero),
                "{what}"
            );
            let rec = record(&w, k);
            assert_eq!(w.units().hero_name(hero), Some(rec.name.clone()), "{what}");
            let abilities = w.units().unit_abilities(hero);
            for a in rec.abilities.ids() {
                assert!(abilities.contains(&a), "{what}: {a:?}");
            }
        }
        assert_eq!(w.units().heroes(p).len(), taken.len(), "{name}");
        assert_eq!(w.units().hero_units(p), w.units().heroes(p), "{name}");
    }
}

#[test]
fn a_noble_hero_costs_nothing_to_keep_and_brings_gold() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let at = fortress(&w, p);
        let before = w.budget(p);
        let plain = changed_hero(&mut w, p, at, 0, |h| h.abilities.noble = false);
        assert!(w.units().unit_gold_upkeep(plain) > 0, "{name}");
        assert_eq!(w.budget(p).noble_gold, before.noble_gold, "{name}");
        let noble = changed_hero(&mut w, p, at, 1, |h| h.abilities.noble = true);
        assert!(
            w.units()
                .unit_has_ability(noble, Ability::new(96 /* noble */)),
            "{name}"
        );
        assert_eq!(w.units().unit_gold_upkeep(noble), 0, "{name}");
        assert!(w.budget(p).noble_gold > before.noble_gold, "{name}");
    }
}

#[test]
fn a_heros_mana_comes_from_skill_level_and_items_and_helps_only_at_the_fortress() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let at = fortress(&w, p);
        assert_eq!(w.hero_casting_skill(p), 0, "{name}");
        let none = changed_hero(&mut w, p, at, 0, |h| h.casting_skill = 0);
        assert_eq!(w.units().hero_mana(none), 0, "{name}");
        let caster = changed_hero(&mut w, p, at, 2 /* sage */, |h| h.casting_skill = 3);
        let young = w.units().hero_mana(caster);
        assert!(young > 0, "{name}");
        w.scenario_set_experience(caster, 100_000);
        let old = w.units().hero_mana(caster);
        assert!(old > young, "{name}: {young} to {old}");
        let helps = w.hero_casting_skill(p);
        assert!(helps > 0 && helps < old, "{name}: {helps} of {old}");
        // An item of spell skill adds to the mana of a hero that has any.
        let (slot, item) = fitting(&w, caster, |i| i.bonus.spell_skill > 0)
            .expect("a staff or wand of spell skill");
        let bonus = item.bonus.spell_skill;
        arm(&mut w, p, caster, slot, item.clone());
        assert_eq!(w.units().hero_mana(caster), old + bonus, "{name}");
        if let Some((slot, item)) = fitting(&w, none, |i| i.bonus.spell_skill > 0) {
            arm(&mut w, p, none, slot, item);
            assert_eq!(w.units().hero_mana(none), 0, "{name}");
        }
        // Away from the fortress it helps nobody.
        let away = around(&w, at)
            .into_iter()
            .find(|&q| q != at && open_land(&w, q))
            .expect("open land");
        w.cheat_move_unit(caster, away);
        assert_eq!(w.hero_casting_skill(p), 0, "{name}");
        let q = w.wizards()[1];
        assert_eq!(w.hero_casting_skill(q), 0, "{name}");
    }
}

#[test]
fn sage_and_legendary_heroes_add_research_and_fame_by_their_level() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let at = fortress(&w, p);
        let (research, fame) = (w.units().sage_research(p), w.fame(p));
        assert_eq!(research, 0, "{name}");
        let hero = changed_hero(&mut w, p, at, 2 /* sage */, |h| {
            h.abilities.sage = Tier::Normal;
            h.abilities.legendary = Tier::Super;
        });
        for xp in [0, 150, 100_000] {
            w.scenario_set_experience(hero, xp);
            let level = w.units().unit_level(hero);
            let value = |a: &str, t| w.content().rules.hero_value(a, t, level);
            assert_eq!(
                w.units().sage_research(p),
                value("sage", Tier::Normal),
                "{name}: {xp}"
            );
            assert_eq!(
                w.fame(p),
                fame + value("legendary", Tier::Super),
                "{name}: {xp}"
            );
        }
        assert!(w.units().sage_research(p) > 0 && w.fame(p) > fame, "{name}");
        w.cheat_remove_unit(hero);
        assert_eq!(
            (w.units().sage_research(p), w.fame(p)),
            (research, fame),
            "{name}"
        );
    }
}

// ---- leaving ----

#[test]
fn a_hero_that_leaves_keeps_its_experience_and_is_dead_or_free_by_the_way_it_left() {
    for (name, mut w) in worlds() {
        let (p, q) = (w.wizards()[0], w.wizards()[1]);
        let at = fortress(&w, p);
        let slain = join(&mut w, p, at, 0);
        let let_go = join(&mut w, p, at, 1);
        w.scenario_set_experience(slain, 40);
        w.scenario_set_experience(let_go, 70);
        w.cheat_slay_unit(slain);
        w.cheat_remove_unit(let_go);
        assert_eq!(state_of(&w, 0), HeroState::Dead, "{name}");
        assert_eq!(state_of(&w, 1), HeroState::Free, "{name}");
        assert_eq!(w.units().hero_xp(slain), 40, "{name}");
        assert_eq!(w.units().hero_xp(let_go), 70, "{name}");
        assert_eq!(w.units().raisable_heroes(), vec![0], "{name}");
        // The body is gone; the hero is the same one, and no one's.
        for h in [slain, let_go] {
            assert_eq!(w.units().unit_owner(h), None, "{name}");
            assert_eq!(w.units().unit_position(h), None, "{name}");
            assert!(w.units().unit_def(h).is_none(), "{name}");
        }
        assert!(w.units().heroes(p).is_empty(), "{name}");
        // Taking a fallen hero out again changes nothing.
        w.cheat_remove_unit(slain);
        assert_eq!(state_of(&w, 0), HeroState::Dead, "{name}");
        // Any wizard may have them again, with what they learned.
        for (kind, xp) in [(0, 40), (1, 70)] {
            let there = fortress(&w, q);
            let again = join(&mut w, q, there, kind);
            assert_eq!(w.units().hero_xp(again), xp, "{name}");
        }
        assert_eq!(w.units().heroes(q).len(), 2, "{name}");
        assert!(w.units().raisable_heroes().is_empty(), "{name}");
    }
}

fn open_land(w: &World, q: Position) -> bool {
    w.economy()
        .terrain_class_at(q)
        .is_some_and(|c| c != TerrainClass::Ocean)
        && w.units().units_at(q).is_empty()
        && w.economy().city_at(q).is_none()
        && w.economy().feature_entity_at(q).is_none()
}

/// The first item of the content that `want` takes and one of the hero's slots fits.
fn fitting(w: &World, hero: hecs::Entity, want: impl Fn(&Item) -> bool) -> Option<(usize, Item)> {
    let slots = w
        .units()
        .unit_def(hero)
        .and_then(|d| d.hero.as_ref())
        .expect("a hero")
        .kind
        .slots();
    w.content()
        .rules
        .items
        .defs()
        .iter()
        .map(Item::from_def)
        .filter(want)
        .find_map(|item| {
            let slot = slots.iter().position(|s| s.fits(item.kind))?;
            Some((slot, item))
        })
}

/// Give the wizard the item and put it in the hero's slot.
fn arm(w: &mut Table, p: PlayerId, hero: hecs::Entity, slot: usize, item: Item) {
    let from = match w.give_item(p, item) {
        Placed::Vault(i) => ItemSpot::Vault(i),
        other => panic!("no room in the vault: {other:?}"),
    };
    let events = w.apply(Command::MoveItem {
        player: p,
        from,
        to: ItemSpot::Hero(hero, slot),
        pay: true,
    });
    assert_eq!(w.items().carried(hero).len(), 1, "{events:?}");
}

/// A hero of `p` at its fortress with an item in a slot.
fn armed(w: &mut Table, p: PlayerId, kind: usize) -> hecs::Entity {
    let at = fortress(w, p);
    let hero = join(w, p, at, kind);
    let (slot, item) = fitting(w, hero, |_| true).expect("an item");
    arm(w, p, hero, slot, item);
    hero
}

fn in_armory(w: &World, p: PlayerId) -> usize {
    let a = w.items().armory(p);
    a.vault.iter().flatten().count() + a.awards.len()
}

#[test]
fn a_slain_or_dismissed_heros_items_go_to_its_wizard_and_are_not_passed_on_twice() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let before = in_armory(&w, p);
        let slain = armed(&mut w, p, 0);
        w.cheat_slay_unit(slain);
        assert_eq!(in_armory(&w, p), before + 1, "{name}");
        w.cheat_remove_unit(slain);
        assert_eq!(in_armory(&w, p), before + 1, "{name}");
        let dismissed = armed(&mut w, p, 1);
        let events = w.apply(Command::DismissUnit { unit: dismissed });
        assert!(
            events.contains(&Event::UnitsDisbanded {
                player: p,
                units: 1
            }),
            "{name}: {events:?}"
        );
        assert_eq!(in_armory(&w, p), before + 2, "{name}");
        assert_eq!(state_of(&w, 1), HeroState::Free, "{name}");
        // A hero removed outright takes its items with it.
        let removed = armed(&mut w, p, 3);
        w.cheat_remove_unit(removed);
        assert_eq!(in_armory(&w, p), before + 2, "{name}");
        assert!(w.items().carried(removed).is_empty(), "{name}");
    }
}

/// A battle in which `attacker` wipes out the hero's stack.
fn lose_to(w: &mut Table, attacker: PlayerId, hero: hecs::Entity, name: &str) -> Vec<Event> {
    let home = w.units().unit_position(hero).expect("a square");
    let (at, from) = around(w, home)
        .into_iter()
        .filter(|&q| open_land(w, q))
        .find_map(|q| {
            let from = around(w, q)
                .into_iter()
                .find(|&r| r != q && open_land(w, r))?;
            Some((q, from))
        })
        .unwrap_or_else(|| panic!("{name}: no room for a battle"));
    w.cheat_move_unit(hero, at);
    let kind = if attacker == MONSTERS {
        169 /* demon */
    } else {
        104 /* high_men_spearmen */
    };
    w.apply(Command::CreateUnit {
        owner: attacker,
        pos: from,
        unit: UnitId::new(kind),
    });
    let attackers = w.units().units_at(from);
    let mut events = w
        .start_battle(&attackers, &[hero])
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    w.scenario_edit_battle(|b| {
        for u in b.units.iter_mut().filter(|u| u.side == Side::Defender) {
            u.kill();
            u.status = f4x_battle::unit::Status::Dead;
        }
    });
    events.extend(w.apply(Command::Battle(BattleCommand::EndTurn)));
    assert!(!w.in_battle(), "{name}: {events:?}");
    events
}

#[test]
fn the_victor_takes_the_items_of_a_fallen_hero_but_the_monsters_destroy_them() {
    for (name, mut w) in worlds() {
        let (p, q) = (w.wizards()[0], w.wizards()[1]);
        let hero = armed(&mut w, p, 0);
        let (mine, theirs) = (in_armory(&w, p), in_armory(&w, q));
        let events = lose_to(&mut w, q, hero, &name);
        assert_eq!(state_of(&w, 0), HeroState::Dead, "{name}");
        assert_eq!(
            (in_armory(&w, p), in_armory(&w, q)),
            (mine, theirs + 1),
            "{name}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ItemsAwarded { player, lost: 0, .. } if *player == q)),
            "{name}: {events:?}"
        );
        let hero = armed(&mut w, p, 1);
        let (mine, theirs) = (in_armory(&w, p), in_armory(&w, q));
        let events = lose_to(&mut w, MONSTERS, hero, &name);
        assert_eq!(state_of(&w, 1), HeroState::Dead, "{name}");
        assert_eq!(
            (in_armory(&w, p), in_armory(&w, q)),
            (mine, theirs),
            "{name}"
        );
        assert!(w.items().carried(hero).is_empty(), "{name}");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::ItemsAwarded { .. })),
            "{name}: {events:?}"
        );
    }
}

#[test]
fn a_defeated_wizards_heroes_are_free_for_the_others_in_the_field_and_in_cities() {
    for (name, mut w) in worlds() {
        let (p, q) = (w.wizards()[0], w.wizards()[1]);
        let home = fortress(&w, q);
        let field = around(&w, home)
            .into_iter()
            .find(|&s| s != home && open_land(&w, s))
            .expect("open land");
        join(&mut w, q, field, 4);
        join(&mut w, q, home, 5);
        let (world, rng) = w.world_and_rng();
        world.cheat_defeat_player(q, p, rng);
        assert_eq!(state_of(&w, 4), HeroState::Free, "{name}");
        assert_eq!(state_of(&w, 5), HeroState::Free, "{name}");
        assert!(w.units().heroes(q).is_empty(), "{name}");
    }
}

#[test]
fn heroes_that_cannot_be_paid_go_home_and_can_be_hired_again() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let at = fortress(&w, p);
        let all: Vec<usize> = kinds(&w).collect();
        for &k in &all {
            join(&mut w, p, at, k);
        }
        w.apply(Command::SetTaxRate { player: p, rate: 0 });
        w.treasury_mut(p).expect("a treasury").gold = HalfInt::ZERO;
        let serving = w.units().heroes(p).len();
        w.end_player_turn(p);
        w.start_player_turn(p, 2);
        assert!(w.units().heroes(p).len() < serving, "{name}: all were paid");
        for k in all {
            let hero = w.units().hero_entity(k).expect("the hero");
            let serves = w.units().unit_owner(hero) == Some(p);
            let expected = if serves {
                HeroState::Serving
            } else {
                HeroState::Free
            };
            assert_eq!(state_of(&w, k), expected, "{name}: {k}");
        }
    }
}

#[test]
fn the_heroes_survive_a_save_in_every_state() -> Result<(), Box<dyn std::error::Error>> {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let at = fortress(&w, p);
        let serving = armed(&mut w, p, 0);
        w.scenario_set_experience(serving, 33);
        let dead = join(&mut w, p, at, 1);
        w.cheat_slay_unit(dead);
        let free = join(&mut w, p, at, 3);
        w.scenario_set_experience(free, 90);
        w.cheat_remove_unit(free);
        let saved = w.snapshot().to_text()?;
        let back = Snapshot::from_text(&saved)?.restore(content())?;
        assert_eq!(text(&back), saved, "{name}");
        assert_eq!(
            back.units().hero_states(),
            w.units().hero_states(),
            "{name}"
        );
        for k in [0, 1, 3] {
            assert_eq!(record(&back, k), record(&w, k), "{name}: {k}");
            let hero = back.units().hero_entity(k).expect("the hero");
            let was = w.units().hero_entity(k).expect("the hero");
            assert_eq!(back.units().hero_xp(hero), w.units().hero_xp(was), "{name}");
        }
        assert_eq!(back.units().heroes(p).len(), 1, "{name}");
        assert_eq!(back.units().raisable_heroes(), vec![1], "{name}");
    }
    Ok(())
}
