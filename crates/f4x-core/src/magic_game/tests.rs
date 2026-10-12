use super::*;
use crate::city::Position;
use crate::magic::*;
use crate::map::{Grid, Map, Plane};
use crate::player::Player;
use crate::rules::exec::Target;
use crate::rules::Stat;
use crate::save::Snapshot;
use crate::testutil::content_with_magic;
use crate::Command;
use std::collections::BTreeSet;

const P0: PlayerId = PlayerId(0);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

fn spell(g: &Game, id: &str) -> SpellId {
    g.content.rules.spells.id_of(id).unwrap()
}

/// 24x14 grassland, one wizard with 2 life books and 100 mana.
fn world() -> Game {
    let c = content_with_magic();
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut game = Game::new(7);
    game.content = c;
    game.content.rules.config.tax_unrest_percent = vec![0, 0, 0, 30, 45, 60, 75];
    game.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    let mut p = Player::new(P0, "Merlin");
    p.gold = 500;
    p.tax_rate = 2;
    p.mana = 100;
    game.players.push(p);
    game.init_wizard(P0, &[("life", 2)], &[]).unwrap();
    game
}

/// The first city is the wizard's fortress, later ones are ordinary cities.
fn found(g: &mut Game, pos: Position, pop: u32) -> hecs::Entity {
    let race = g.content.rules.races.id_of("elf");
    let name = "Town".into();
    if g.player(P0).unwrap().fortress.is_none() {
        g.apply(Command::PlaceFortress {
            player: P0,
            pos,
            name,
            race,
            population: pop,
        });
    } else {
        g.apply(Command::FoundCity {
            owner: P0,
            pos,
            name,
            race,
            population: pop,
        });
    }
    g.city_at(pos).unwrap()
}

fn add_building(g: &mut Game, city: hecs::Entity, id: &str) {
    let b = g.content.rules.buildings.id_of(id).unwrap();
    g.world
        .get::<&mut crate::economy::Buildings>(city)
        .unwrap()
        .0
        .push(b);
}

#[test]
fn casting_skill_follows_triangular_numbers() {
    // Skill k needs more than (k-1)*k points: 2, 6, 12, 20...
    assert_eq!(base_casting_skill(0, false), 0);
    assert_eq!(base_casting_skill(1, false), 1);
    assert_eq!(base_casting_skill(2, false), 1);
    assert_eq!(base_casting_skill(3, false), 2);
    assert_eq!(base_casting_skill(56, false), 7);
    assert_eq!(base_casting_skill(57, false), 8);
    assert_eq!(base_casting_skill(0, true), 10, "archmage adds ten");
    assert_eq!(skill_points_for(8, false), 57);
    assert_eq!(base_casting_skill(skill_points_for(14, true), true), 14);
}

#[test]
fn a_new_wizard_starts_with_books_spells_and_skill() {
    let mut g = world();
    g.init_wizard(P0, &[("life", 3), ("chaos", 2)], &[Retort::Archmage])
        .unwrap();
    let m = g.magic(P0).unwrap();
    // Three books and two books: 2 + 1 common spells known.
    assert_eq!(m.known.len(), 3);
    assert!(m.known.is_disjoint(&m.researchable));
    let in_realm = |set: &BTreeSet<SpellId>, realm: &str| {
        set.iter()
            .filter(|&&s| g.content.rules.spells.get(s).realm == realm)
            .count()
    };
    // Life, 3 books: 6 common + 3 uncommon + 2 rare + 1 very rare, of which 2 known.
    assert_eq!(in_realm(&m.known, "life"), 2);
    assert_eq!(in_realm(&m.researchable, "life"), 12 - 2);
    // Chaos, 2 books: 5 + 2 + 1 + 0, of which 1 known.
    assert_eq!(in_realm(&m.researchable, "chaos"), 8 - 1);
    // Arcane is open to everyone, except the Spell of Mastery.
    assert!(m.researchable.contains(&spell(&g, "magic_spirit")));
    assert!(!m.researchable.contains(&spell(&g, "spell_of_mastery")));
    // 5 books + 5 for the archmage, doubled.
    assert_eq!(g.magic(P0).unwrap().casting_skill(), 20);
}

#[test]
fn the_same_seed_gives_the_same_spell_library() {
    let (a, b) = (world(), world());
    assert_eq!(a.magic(P0), b.magic(P0));
}

#[test]
fn power_comes_from_cities_and_is_split_by_percentages() {
    let mut g = world();
    assert_eq!(g.income(P0).split, PowerSplit::default());
    let city = found(&mut g, at(10, 6), 4);
    add_building(&mut g, city, "temple");
    add_building(&mut g, city, "library");
    // Four elves make two mana, the temple two; the library makes two research.
    // Plus two books' worth from the fortress city.
    assert_eq!(g.income(P0).power_base, 4 + 2);
    assert_eq!(g.income(P0).city_research, 2);
    assert_eq!(
        g.income(P0).split,
        PowerSplit {
            mana: 2,
            skill: 2,
            research: 2 + 2
        }
    );
    g.magic_mut(P0).unwrap().set_split(100, 0, 0).unwrap();
    assert_eq!(
        g.income(P0).split,
        PowerSplit {
            mana: 6,
            skill: 0,
            research: 2
        }
    );
    assert_eq!(
        g.magic_mut(P0).unwrap().set_split(50, 40, 20),
        Err(MagicError::BadSplit)
    );
    g.magic_mut(P0)
        .unwrap()
        .retorts
        .insert(Retort::ManaFocusing);
    assert_eq!(g.income(P0).split.mana, 7, "a quarter more mana");
}

#[test]
fn retorts_and_extra_books_change_research_and_cost() {
    let mut g = world();
    g.init_wizard(P0, &[("life", 9)], &[Retort::SageMaster])
        .unwrap();
    let mend = spell(&g, "mend");
    // Sage master 25 plus two books over seven, 10 each.
    assert_eq!(
        g.magic(P0)
            .unwrap()
            .research_bonus(g.content().rules.spells.get(mend)),
        45
    );
    // Two books over seven take a fifth off the cost.
    assert_eq!(g.casting_cost(P0, mend, false), 8);
    let runes = spell(&g, "magic_spirit");
    assert_eq!(
        g.magic(P0)
            .unwrap()
            .research_bonus(g.content().rules.spells.get(runes)),
        25,
        "arcane gets no book bonus"
    );
    assert_eq!(g.casting_cost(P0, runes, false), 30);
    // A spell that can also be cast in battle costs five times as much overland.
    let strength = spell(&g, "giant_strength");
    assert_eq!(g.casting_cost(P0, strength, true), 8);
    assert_eq!(g.casting_cost(P0, strength, false), 40);
}

#[test]
fn research_takes_turns_and_ends_with_a_new_spell() {
    let mut g = world();
    let target = spell(&g, "magic_spirit");
    let other = spell(&g, "dispel_magic");
    found(&mut g, at(10, 6), 4);
    g.magic_mut(P0).unwrap().set_split(0, 100, 0).unwrap();
    assert_eq!(
        g.set_research(P0, spell(&g, "spell_of_mastery")),
        Err(MagicError::NotAvailable)
    );
    g.set_research(P0, target).unwrap();
    assert_eq!(g.magic(P0).unwrap().research_remaining, 20);
    assert_eq!(
        g.set_research(P0, other),
        Err(MagicError::AlreadyResearching)
    );
    // A 4-citizen city makes 2 mana (elves) and the fortress 2: all of it research.
    let mut learned = None;
    for turn in 1..=30 {
        let events = g.apply(Command::EndTurn);
        if events
            .iter()
            .any(|e| matches!(e, Event::SpellLearned { spell, .. } if *spell == target))
        {
            learned = Some(turn);
            break;
        }
    }
    let turns = learned.expect("the spell is learned");
    assert!(
        (3..=8).contains(&turns),
        "20 points at about 6 a turn: {turns}"
    );
    let m = g.magic(P0).unwrap();
    assert!(m.known.contains(&target));
    assert!(!m.researchable.contains(&target));
    assert_eq!(m.researching, None);
    g.set_research(P0, other).unwrap();
}

#[test]
fn mana_income_fills_the_reserve() {
    let mut g = world();
    let city = found(&mut g, at(10, 6), 4);
    add_building(&mut g, city, "temple");
    g.magic_mut(P0).unwrap().set_split(100, 0, 0).unwrap();
    let before = g.player(P0).unwrap().mana;
    g.apply(Command::EndTurn);
    assert_eq!(g.player(P0).unwrap().mana, before + 6);
}

#[test]
fn an_affordable_spell_takes_effect_at_once() {
    let mut g = world();
    let spear = g.content.rules.units.id_of("spear").unwrap();
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    g.world.insert_one(unit, crate::units::Hurt(3)).unwrap();
    let mend = spell(&g, "mend");
    g.magic_mut(P0).unwrap().known.insert(mend);
    g.magic_mut(P0).unwrap().skill_left = 50;
    let mana = g.player(P0).unwrap().mana;
    // A spell with a target needs one.
    assert_eq!(g.start_cast(P0, mend, None), Err(MagicError::NeedsTarget));
    let events = g.apply(Command::CastSpell {
        player: P0,
        spell: mend,
        target: Some(Target::Unit(unit)),
    });
    assert!(events.contains(&Event::SpellCast {
        player: P0,
        spell: mend
    }));
    assert_eq!(g.unit_hits(unit).0, g.unit_hits(unit).1, "healed");
    assert_eq!(g.player(P0).unwrap().mana, mana - 10);
    assert_eq!(g.magic(P0).unwrap().skill_left, 40);
    assert_eq!(g.magic(P0).unwrap().casting, None);
}

#[test]
fn spells_need_a_known_spell_and_a_valid_target() {
    let mut g = world();
    let spear = g.content.rules.units.id_of("spear").unwrap();
    let mine = g.spawn_unit(P0, at(3, 3), spear);
    let theirs = g.spawn_unit(PlayerId(1), at(4, 3), spear);
    let (mend, zap) = (spell(&g, "mend"), spell(&g, "zap"));
    g.magic_mut(P0).unwrap().skill_left = 50;
    assert_eq!(
        g.start_cast(P0, mend, Some(Target::Unit(mine))),
        Err(MagicError::NotAvailable),
        "not known yet"
    );
    g.magic_mut(P0).unwrap().known.extend([mend, zap]);
    assert_eq!(
        g.check_target(P0, mend, Some(Target::Unit(theirs))),
        Err(MagicError::NoSuchTarget),
        "healing is for your own units"
    );
    assert_eq!(g.check_target(P0, zap, Some(Target::Unit(theirs))), Ok(()));
    assert_eq!(
        g.check_target(P0, zap, Some(Target::Unit(mine))),
        Err(MagicError::NoSuchTarget)
    );
    // Three damage kills a spear-sized unit with that few hit points or hurts it.
    g.start_cast(P0, zap, Some(Target::Unit(theirs))).unwrap();
    let (left, max) = g.unit_hits(theirs);
    assert!(left < max || !g.world.contains(theirs));
}

#[test]
fn a_channeler_pays_half_the_spell_maintenance() {
    let mut g = world();
    found(&mut g, at(10, 6), 4);
    let mut defs = g.content.rules.spells.defs().to_vec();
    defs[0].upkeep = 6;
    g.content.rules.spells = crate::rules::Registry::new(defs);
    let dear = (
        g.content
            .rules
            .spells
            .id_of(&g.content.rules.spells.defs()[0].id)
            .unwrap(),
        6,
    );
    g.magic_mut(P0).unwrap().global.push(dear.0);
    assert_eq!(g.income(P0).mana_upkeep, dear.1);
    g.magic_mut(P0).unwrap().retorts.insert(Retort::Channeler);
    assert_eq!(g.income(P0).mana_upkeep, dear.1 / 2);
    // The reserve pays the half, and the spell stays.
    g.player_mut(P0).unwrap().mana = dear.1 / 2;
    g.apply_mana_upkeep(P0);
    assert_eq!(g.player(P0).unwrap().mana, 0);
    assert!(g.magic(P0).unwrap().global.contains(&dear.0));
}

#[test]
fn a_famous_wizard_starts_with_ten_fame() {
    let mut g = world();
    g.init_wizard(P0, &[("life", 3)], &[Retort::Famous])
        .unwrap();
    assert_eq!(g.player(P0).unwrap().fame, 10);
    g.init_wizard(P0, &[("life", 3)], &[]).unwrap();
    assert_eq!(g.player(P0).unwrap().fame, 0);
}

#[test]
fn a_dear_spell_is_paid_over_turns_then_aimed_and_costs_upkeep() {
    let mut g = world();
    // A galley eats no food and costs no gold, so it outlives the long wait.
    let spear = g.content.rules.units.id_of("galley").unwrap();
    found(&mut g, at(10, 6), 8);
    // All power to research, so the mana reserve only moves by what is spent.
    g.magic_mut(P0).unwrap().set_split(0, 100, 0).unwrap();
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    g.balance_farmers(P0);
    let strength = spell(&g, "giant_strength");
    g.magic_mut(P0).unwrap().known.insert(strength);
    let melee = g.unit_stat(unit, Stat::Melee);
    // 40 mana overland; the wizard's skill is 4, so it takes many turns.
    assert_eq!(g.magic(P0).unwrap().casting_skill(), 4);
    assert_eq!(g.start_cast(P0, strength, None), Ok(vec![]));
    assert_eq!(g.magic(P0).unwrap().casting.unwrap().remaining, 40);
    assert_eq!(
        g.start_cast(P0, strength, None),
        Err(MagicError::AlreadyCasting)
    );
    let mut ready_after = None;
    for turn in 1..=20 {
        let events = g.apply(Command::EndTurn);
        if events.contains(&Event::SpellReady {
            player: P0,
            spell: strength,
        }) {
            ready_after = Some(turn);
            break;
        }
    }
    // Forty mana at four a turn is ten payments, but the last is borrowed from the next
    // turn's skill, as in the original.
    assert_eq!(ready_after, Some(9));
    assert_eq!(g.player(P0).unwrap().mana, 60, "the 40 mana was paid");
    let events = g.apply(Command::ResolveCast {
        player: P0,
        target: Target::Unit(unit),
    });
    assert!(events.contains(&Event::SpellCast {
        player: P0,
        spell: strength
    }));
    assert_eq!(g.unit_stat(unit, Stat::Melee), melee + 1);
    assert_eq!(g.income(P0).mana_upkeep, 2);
    let mana = g.player(P0).unwrap().mana;
    g.apply(Command::EndTurn);
    assert_eq!(g.player(P0).unwrap().mana, mana - 2);

    // With no mana left the enchantment lapses.
    g.player_mut(P0).unwrap().mana = 1;
    let events = g.apply(Command::EndTurn);
    assert!(events.contains(&Event::EnchantmentLapsed {
        player: P0,
        spell: strength
    }));
    assert_eq!(g.unit_stat(unit, Stat::Melee), melee);
    assert_eq!(g.income(P0).mana_upkeep, 0);
}

#[test]
fn cancelling_loses_what_was_paid() {
    let mut g = world();
    let strength = spell(&g, "giant_strength");
    g.magic_mut(P0).unwrap().known.insert(strength);
    g.start_cast(P0, strength, None).unwrap();
    g.apply(Command::EndTurn);
    let mana = g.player(P0).unwrap().mana;
    assert!(mana < 100);
    g.apply(Command::CancelCast { player: P0 });
    assert_eq!(g.magic(P0).unwrap().casting, None);
    assert_eq!(g.player(P0).unwrap().mana, mana, "no refund");
}

#[test]
fn summoning_creates_units_at_the_chosen_tile() {
    let mut g = world();
    let call = spell(&g, "call_spears");
    g.magic_mut(P0).unwrap().known.insert(call);
    g.magic_mut(P0).unwrap().skill_left = 99;
    g.apply(Command::CastSpell {
        player: P0,
        spell: call,
        target: Some(Target::Tile(at(5, 5))),
    });
    assert_eq!(g.units_at(at(5, 5)).len(), 2);
}

#[test]
fn a_city_enchantment_is_cast_and_paid_for() {
    let mut g = world();
    let city = found(&mut g, at(10, 6), 4);
    let famine = spell(&g, "famine");
    g.magic_mut(P0).unwrap().known.insert(famine);
    g.magic_mut(P0).unwrap().skill_left = 99;
    let food = g.city_economy(city).unwrap().food;
    g.apply(Command::CastSpell {
        player: P0,
        spell: famine,
        target: Some(Target::City(city)),
    });
    assert!(g.city_economy(city).unwrap().food < food);
    assert_eq!(g.income(P0).mana_upkeep, 1);
}

#[test]
fn dispelling_removes_enchantments_by_strength() {
    let mut g = world();
    let spear = g.content.rules.units.id_of("spear").unwrap();
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let strength = spell(&g, "giant_strength");
    let melee = g.unit_stat(unit, Stat::Melee);
    g.attach_enchantment(Target::Unit(unit), strength, P0);
    assert_eq!(g.unit_stat(unit, Stat::Melee), melee + 1);
    {
        use crate::rules::exec::EffectHost;
        let mut host = crate::cast::tests_support::host(&mut g);
        // An overwhelming dispel by another wizard always works.
        assert_eq!(host.dispel(Target::Unit(unit), 100_000, PlayerId(1)), 1);
    }
    assert_eq!(g.unit_stat(unit, Stat::Melee), melee);
}

#[test]
fn saves_keep_the_wizard_the_casting_and_unit_spells() {
    let mut g = world();
    let spear = g.content.rules.units.id_of("spear").unwrap();
    found(&mut g, at(10, 6), 8);
    g.balance_farmers(P0);
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let strength = spell(&g, "giant_strength");
    g.attach_enchantment(Target::Unit(unit), strength, P0);
    g.set_research(P0, spell(&g, "magic_spirit")).unwrap();
    g.magic_mut(P0).unwrap().set_split(20, 70, 10).unwrap();
    g.magic_mut(P0).unwrap().known.insert(strength);
    g.start_cast(P0, strength, None).unwrap();
    g.apply(Command::EndTurn);
    let text = g.snapshot().to_text().unwrap();
    let back = Snapshot::from_text(&text).unwrap();
    let loaded = back.restore(content_with_magic()).unwrap();
    assert_eq!(loaded.magic(P0), g.magic(P0));
    assert_eq!(loaded.snapshot(), g.snapshot());
    let unit = loaded.all_units()[0];
    assert_eq!(
        loaded.unit_stat(unit, Stat::Melee),
        g.unit_stat(g.all_units()[0], Stat::Melee)
    );
}

#[test]
fn old_saves_without_wizards_still_load() {
    let mut g = world();
    g.players[0].magic = Magic::default();
    let text = g.snapshot().to_text().unwrap();
    // Take the wizard out of the text the way an old save would lack it.
    let start = text.find("magic:").unwrap();
    let end = text[start..].find("skill_left").unwrap();
    let close = text[start + end..].find("),").unwrap();
    let old = format!("{}{}", &text[..start], &text[start + end + close + 2..]);
    let back = Snapshot::from_text(&old).unwrap();
    let loaded = back.restore(content_with_magic()).unwrap();
    assert_eq!(loaded.magic(P0), Some(&Magic::default()));
}
