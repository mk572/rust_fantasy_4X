//! Class 9, the wizard's fall: a city taken, the fortress lost, banishment, defeat and resigning, what the
//! winner takes, and the end of the game by conquest.
//!
//! Each world has the first wizard take cities of the second, who has the fortress city and a second city.

use crate::support::worlds;
use f4x_core::game::endgame::EndKind;
use f4x_core::game::{Command, Event, Game, Rejection};
use f4x_core::player::PlayerId;
use f4x_core::rules::SpellId;
use f4x_core::wizard::fortress::FortressError;
use f4x_def::half_int::HalfInt;

struct Fall {
    name: String,
    g: Game,
    /// The one who takes, and the one who loses.
    a: PlayerId,
    b: PlayerId,
    /// The loser's fortress city and its second city.
    capital: hecs::Entity,
    town: hecs::Entity,
}

fn falls() -> Vec<Fall> {
    let mut out = Vec::new();
    for (name, mut g) in worlds() {
        let w = g.wizards();
        let (a, b) = (w[0], w[1]);
        let Some(capital) = g.economy().fortress_city(b) else {
            continue;
        };
        let Some(town) = crate::support::another_city(&mut g, b, 4) else {
            continue;
        };
        // A wizard that can pay for the Spell of Return: skill and mana to cast it.
        let need = g.content().rules.config.wizard.resign_below_skill;
        g.scenario_set_casting_skill(b, need);
        g.treasury_mut(b).expect("a treasury").mana = HalfInt::from_int(1000);
        g.treasury_mut(b).expect("a treasury").gold = HalfInt::from_int(1000);
        out.push(Fall {
            name,
            g,
            a,
            b,
            capital,
            town,
        });
    }
    assert!(!out.is_empty(), "no world had room for a second city");
    out
}

fn told(events: &[Event], wanted: impl Fn(&Event) -> bool) -> bool {
    events.iter().any(wanted)
}

fn mana(g: &Game, p: PlayerId) -> HalfInt {
    g.player(p).expect("a player").treasury().mana
}

fn gold(g: &Game, p: PlayerId) -> HalfInt {
    g.player(p).expect("a player").treasury().gold
}

/// Whether the content's rule would let the wizard pay for the Spell of Return after the loss.
fn can_pay_for_return(g: &Game, p: PlayerId) -> bool {
    let skill = g
        .wizard(p)
        .expect("a wizard")
        .casting_skill(&g.content().rules);
    skill >= g.content().rules.config.wizard.resign_below_skill
        && mana(g, p) >= HalfInt::from_int(1)
}

#[test]
fn taking_an_ordinary_city_changes_its_owner_and_ends_the_losers_spells_on_it() {
    use f4x_core::spell::instance::SpellTarget as On;
    for mut f in falls() {
        let spell_of = |g: &Game, n: usize| {
            g.content()
                .rules
                .spells
                .defs()
                .iter()
                .filter(|d| {
                    d.target == f4x_def::rules::SpellTarget::OwnCity
                        && d.duration == f4x_def::rules::Duration::Permanent
                })
                .map(|d| SpellId::new(d.num))
                .nth(n)
                .expect("a city spell")
        };
        let (theirs, mine) = (spell_of(&f.g, 0), spell_of(&f.g, 1));
        f.g.scenario_enchant_city(f.town, theirs, f.b);
        f.g.scenario_enchant_city(f.town, mine, f.a);
        let events = f.g.capture_city(f.town, f.a);
        assert_eq!(f.g.economy().city_owner(f.town), Some(f.a), "{}", f.name);
        assert!(
            told(
                &events,
                |e| matches!(e, Event::CityCaptured { from, to, .. }
                if *from == f.b && *to == f.a)
            ),
            "{}: {events:?}",
            f.name
        );
        assert!(
            !told(&events, |e| matches!(
                e,
                Event::WizardBanished { .. } | Event::WizardDefeated { .. }
            )),
            "{}: an ordinary city's loss felled the wizard: {events:?}",
            f.name
        );
        assert!(
            !f.g.spells().has(On::City(f.town), theirs),
            "{}: the old owner's spell stayed",
            f.name
        );
        assert!(
            f.g.spells().has(On::City(f.town), mine),
            "{}: the taker's own spell went",
            f.name
        );
        // The wizard still has its fortress.
        assert!(f.g.economy().fortress_city(f.b).is_some(), "{}", f.name);
        assert!(
            !f.g.wizard(f.b).expect("a wizard").is_banished(),
            "{}",
            f.name
        );
    }
}

#[test]
fn losing_the_fortress_banishes_a_wizard_who_can_pay_for_the_return_and_the_winner_takes_the_spoils(
) {
    for mut f in falls() {
        assert!(can_pay_for_return(&f.g, f.b), "{}: the setup", f.name);
        let prize = f.g.content().rules.config.renown.fortress_prize_fame;
        let (fame, mana_a, gold_a, mana_b, gold_b) = (
            f.g.fame(f.a),
            mana(&f.g, f.a),
            gold(&f.g, f.a),
            mana(&f.g, f.b),
            gold(&f.g, f.b),
        );
        let cities_b = f.g.economy().cities_of(f.b).len();
        let events = f.g.capture_city(f.capital, f.a);
        assert!(
            told(
                &events,
                |e| matches!(e, Event::WizardBanished { player, by }
                if *player == f.b && *by == f.a)
            ),
            "{}: {events:?}",
            f.name
        );
        let w = f.g.wizard(f.b).expect("a wizard");
        assert!(w.is_banished(), "{}", f.name);
        assert_eq!(w.banished_by(), Some(f.a), "{}", f.name);
        assert!(
            w.casting().is_some(),
            "{}: no Spell of Return begun",
            f.name
        );
        assert!(f.g.economy().fortress_city(f.b).is_none(), "{}", f.name);
        assert_eq!(f.g.fame(f.a), fame + prize, "{}: the prize in fame", f.name);
        // Half the mana, no gold, from a wizard who is banished and not defeated.
        assert_eq!(
            mana(&f.g, f.a),
            mana_a + mana_b.half(),
            "{}: half the mana",
            f.name
        );
        assert_eq!(
            gold(&f.g, f.a),
            gold_a,
            "{}: gold from a banished wizard",
            f.name
        );
        assert_eq!(gold(&f.g, f.b), gold_b, "{}", f.name);
        // The rest of the wizard's cities stay.
        assert_eq!(
            f.g.economy().cities_of(f.b).len(),
            cities_b - 1,
            "{}",
            f.name
        );
        assert_eq!(f.g.economy().city_owner(f.town), Some(f.b), "{}", f.name);
    }
}

#[test]
fn a_wizard_who_cannot_pay_for_the_return_is_defeated_at_once_and_the_cities_go_neutral() {
    use f4x_core::player::MONSTERS;
    for (weak_skill, no_mana) in [(true, false), (false, true)] {
        let mut tried = 0;
        for mut f in falls() {
            if weak_skill {
                f.g.scenario_set_casting_skill(f.b, 1);
                if can_pay_for_return(&f.g, f.b) {
                    // A retort that adds skill: the wizard is strong whatever the points are.
                    continue;
                }
            }
            if no_mana {
                f.g.treasury_mut(f.b).expect("a treasury").mana = HalfInt::ZERO;
            }
            tried += 1;
            let units_b = f.g.units().player_units(f.b);
            let events = f.g.capture_city(f.capital, f.a);
            assert!(
                told(
                    &events,
                    |e| matches!(e, Event::WizardDefeated { player } if *player == f.b)
                ),
                "{}: {events:?}",
                f.name
            );
            assert!(
                !told(&events, |e| matches!(e, Event::WizardBanished { .. })),
                "{}",
                f.name
            );
            let loser = f.g.player(f.b).expect("a player");
            assert!(loser.is_defeated(), "{}", f.name);
            assert_eq!(loser.defeated_by(), Some(f.a), "{}", f.name);
            assert!(
                !f.g.wizard(f.b).expect("a wizard").is_banished(),
                "{}",
                f.name
            );
            // The second city lives on as a neutral city; nothing of the wizard's own is left.
            let owner = f.g.economy().city_owner(f.town).expect("an owner");
            assert!(
                f.g.is_neutral_player(owner) && owner != MONSTERS && owner != f.b,
                "{}: the city went to {owner:?}",
                f.name
            );
            assert!(f.g.economy().cities_of(f.b).is_empty(), "{}", f.name);
            for u in units_b {
                assert_ne!(
                    f.g.units().unit_owner(u),
                    Some(f.b),
                    "{}: a defeated wizard keeps a unit",
                    f.name
                );
            }
        }
        assert!(
            tried > 0,
            "weak {weak_skill}, no mana {no_mana}: no world tried"
        );
    }
}

#[test]
fn taking_the_last_city_of_a_wizard_without_a_fortress_defeats_it() {
    for mut f in falls() {
        // The wizard's capital goes first (banished), then its remaining city falls.
        f.g.capture_city(f.capital, f.a);
        assert!(
            f.g.wizard(f.b).expect("a wizard").is_banished(),
            "{}",
            f.name
        );
        let events = f.g.capture_city(f.town, f.a);
        assert!(
            told(
                &events,
                |e| matches!(e, Event::WizardDefeated { player } if *player == f.b)
            ),
            "{}: {events:?}",
            f.name
        );
        assert!(
            f.g.player(f.b).expect("a player").is_defeated(),
            "{}",
            f.name
        );
    }
}

#[test]
fn a_fortress_city_cannot_be_destroyed_and_keeps_a_citizen() {
    for mut f in falls() {
        f.g.scenario_set_citizens(f.capital, 0);
        let why = f.g.destroy_city(f.capital).expect_err("destroyed");
        assert_eq!(why, FortressError::CannotDestroyFortress, "{}", f.name);
        assert!(f.g.economy().city_owner(f.capital).is_some(), "{}", f.name);
        assert!(f.g.economy().citizens(f.capital) >= 1, "{}", f.name);
        // An ordinary city can.
        let events = f.g.destroy_city(f.town).expect("destroyed");
        assert!(
            told(&events, |e| matches!(e, Event::CityDestroyed { .. })),
            "{}",
            f.name
        );
        assert!(f.g.economy().city_owner(f.town).is_none(), "{}", f.name);
    }
}

#[test]
fn the_loot_setting_decides_whether_the_winner_takes_a_defeated_wizards_mana_and_gold() {
    for loot in [true, false] {
        let mut tried = 0;
        for mut f in falls() {
            f.g.scenario_change_settings(|s| s.rules.defeat_loot = loot);
            f.g.scenario_set_casting_skill(f.b, 1);
            if can_pay_for_return(&f.g, f.b) {
                continue;
            }
            tried += 1;
            let (mana_a, gold_a) = (mana(&f.g, f.a), gold(&f.g, f.a));
            f.g.capture_city(f.capital, f.a);
            assert!(
                f.g.player(f.b).expect("a player").is_defeated(),
                "{}",
                f.name
            );
            if loot {
                assert!(mana(&f.g, f.a) > mana_a, "{}: no mana taken", f.name);
                assert!(gold(&f.g, f.a) > gold_a, "{}: no gold taken", f.name);
            } else {
                assert_eq!(mana(&f.g, f.a), mana_a, "{}", f.name);
                assert_eq!(gold(&f.g, f.a), gold_a, "{}", f.name);
            }
        }
        assert!(tried > 0, "loot {loot}: no world tried");
    }
}

#[test]
fn a_defeat_gives_the_winner_up_to_two_spells_the_loser_knew_and_it_did_not() {
    let mut taken_any = false;
    for mut f in falls() {
        f.g.scenario_set_casting_skill(f.b, 1);
        if can_pay_for_return(&f.g, f.b) {
            continue;
        }
        let before = f.g.wizard(f.a).expect("a wizard").known();
        let loser_knew = f.g.wizard(f.b).expect("a wizard").known();
        let events = f.g.capture_city(f.capital, f.a);
        let spells = events
            .iter()
            .find_map(|e| match e {
                Event::ConquestSpells {
                    winner,
                    loser,
                    spells,
                } if *winner == f.a && *loser == f.b => Some(spells.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{}: no spells told: {events:?}", f.name));
        assert!(spells.len() <= 2, "{}: {spells:?}", f.name);
        let after = f.g.wizard(f.a).expect("a wizard").known();
        for s in &spells {
            assert!(
                loser_knew.contains(s) && !before.contains(s),
                "{}: {s:?}",
                f.name
            );
            assert!(after.contains(s), "{}: {s:?} not learned", f.name);
        }
        assert_eq!(after.len(), before.len() + spells.len(), "{}", f.name);
        taken_any |= !spells.is_empty();
    }
    assert!(taken_any, "no winner took a spell in any world");
}

#[test]
fn a_banished_wizard_may_resign_and_is_then_defeated_by_the_banisher_without_a_second_prize() {
    for mut f in falls() {
        assert_eq!(
            f.g.resign(f.b).expect_err("not banished"),
            FortressError::WrongState,
            "{}: a wizard who is not banished resigned",
            f.name
        );
        f.g.capture_city(f.capital, f.a);
        assert!(
            f.g.wizard(f.b).expect("a wizard").is_banished(),
            "{}",
            f.name
        );
        let fame = f.g.fame(f.a);
        let events = f.g.resign(f.b).expect("resigns");
        assert!(
            told(
                &events,
                |e| matches!(e, Event::WizardDefeated { player } if *player == f.b)
            ),
            "{}: {events:?}",
            f.name
        );
        let loser = f.g.player(f.b).expect("a player");
        assert!(
            loser.is_defeated() && loser.defeated_by() == Some(f.a),
            "{}",
            f.name
        );
        assert_eq!(f.g.fame(f.a), fame, "{}: the prize was paid twice", f.name);
        assert_eq!(
            f.g.resign(f.b).expect_err("twice"),
            FortressError::WrongState,
            "{}",
            f.name
        );
    }
}

#[test]
fn defeating_the_last_rival_ends_the_game_by_conquest() {
    let mut ended = 0;
    for mut f in falls() {
        // Every wizard but the two is defeated first; the game goes on while two stand.
        let others: Vec<_> =
            f.g.wizards()
                .into_iter()
                .filter(|&w| w != f.a && w != f.b)
                .collect();
        for o in others {
            let cities: Vec<_> =
                f.g.economy()
                    .cities_of(o)
                    .into_iter()
                    .map(|(c, _)| c)
                    .collect();
            for c in cities {
                f.g.capture_city(c, f.a);
            }
        }
        assert!(
            f.g.game_over.is_none(),
            "{}: over while two wizards stand",
            f.name
        );
        f.g.scenario_set_casting_skill(f.b, 1);
        if can_pay_for_return(&f.g, f.b) {
            continue;
        }
        let events = f.g.capture_city(f.capital, f.a);
        assert!(
            told(
                &events,
                |e| matches!(e, Event::GameOver { winner, kind, .. }
                if *winner == f.a && *kind == EndKind::Conquest)
            ),
            "{}: {events:?}",
            f.name
        );
        assert!(f.g.game_over.is_some(), "{}", f.name);
        ended += 1;
    }
    assert!(ended > 0, "no world ended by conquest");
}

/// The spell that ends the game when its wizard finishes casting it (`spell_of_mastery`).
const MASTERY: u16 = 213;

#[test]
fn starting_the_spell_of_mastery_is_told_to_all_and_listed_with_what_is_still_owed() {
    for mut f in falls() {
        let spell = SpellId::new(MASTERY);
        f.g.cheat_learn_spell(f.a, spell);
        f.g.treasury_mut(f.a).expect("a treasury").mana = HalfInt::from_int(100_000);
        let events = f.g.apply(Command::CastSpell {
            player: f.a,
            spell,
            target: None,
            extra: 0,
        });
        assert!(
            events.iter().all(|e| !matches!(e, Event::Rejected(_))),
            "{}: {events:?}",
            f.name
        );
        assert!(
            told(
                &events,
                |e| matches!(e, Event::MasteryStarted { caster } if *caster == f.a)
            ),
            "{}: {events:?}",
            f.name
        );
        let casters = f.g.mastery_casters();
        assert!(
            casters.iter().any(|&(p, owed)| p == f.a && owed > 0),
            "{}: {casters:?}",
            f.name
        );
        assert!(
            f.g.game_over.is_none(),
            "{}: won before it was paid",
            f.name
        );
    }
}

#[test]
fn finishing_the_spell_of_mastery_wins_at_once_and_scores_it_for_the_caster_only() {
    for mut f in falls() {
        let spell = SpellId::new(MASTERY);
        f.g.cheat_learn_spell(f.a, spell);
        f.g.scenario_casting(f.a, spell, 0);
        let events = f.g.end_player_turn(f.a);
        assert!(
            told(
                &events,
                |e| matches!(e, Event::GameOver { winner, kind, .. }
                if *winner == f.a && *kind == EndKind::SpellOfMastery)
            ),
            "{}: {events:?}",
            f.name
        );
        let over = f.g.game_over.expect("the game is over");
        assert_eq!(
            (over.winner, over.kind),
            (f.a, EndKind::SpellOfMastery),
            "{}",
            f.name
        );
        let (won, lost) = (f.g.score(f.a), f.g.score(f.b));
        assert!(
            won.mastery > 0 && lost.mastery == 0,
            "{}: {won:?} {lost:?}",
            f.name
        );
        assert_eq!(lost.time, 0, "{}: the loser got a time bonus", f.name);
        assert_eq!(
            won.total(),
            (won.spells + won.townsfolk + won.conquests + won.fame + won.time + won.mastery)
                * won.percent
                / 100,
            "{}: the total is not the sum",
            f.name
        );
        // A game that is over takes no more commands.
        let later = f.g.apply(Command::EndTurn);
        assert!(
            told(&later, |e| matches!(
                e,
                Event::Rejected(Rejection::GameOver)
            )),
            "{}: {later:?}",
            f.name
        );
    }
}
