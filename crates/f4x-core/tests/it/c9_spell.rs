//! Class 9, the spell: what the content says a spell may be cast on, and what casting costs.
//!
//! The matrix is every spell of the shipped content against every kind of target the world has (an own
//! unit, an enemy unit, an own city, an enemy city, an empty square, the caster, another wizard): a spell
//! is never cast on a kind of target its definition (`target`) does not name, and a spell that names it is
//! not refused for the kind.

use crate::support::{content, text, worlds};
use f4x_core::game::save::Snapshot;
use f4x_core::game::{Command, Event, Game, Rejection};
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_core::rules::exec::Target;
use f4x_core::rules::SpellId;
use f4x_core::spell::target_error::TargetError;
use f4x_core::wizard::magic::MagicError;
use f4x_def::half_int::HalfInt;
use f4x_def::rules::{CastContext, SpellTarget};

/// What a target is, for the table of what each spell takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    OwnUnit,
    EnemyUnit,
    OwnCity,
    EnemyCity,
    Tile,
    SelfWizard,
    OtherWizard,
}

const KINDS: [Kind; 7] = [
    Kind::OwnUnit,
    Kind::EnemyUnit,
    Kind::OwnCity,
    Kind::EnemyCity,
    Kind::Tile,
    Kind::SelfWizard,
    Kind::OtherWizard,
];

/// Whether a spell whose definition names `wanted` may be aimed at a target of `kind`.
fn named(wanted: SpellTarget, kind: Kind) -> bool {
    match wanted {
        SpellTarget::OwnUnit => kind == Kind::OwnUnit,
        SpellTarget::EnemyUnit => kind == Kind::EnemyUnit,
        SpellTarget::AnyUnit => matches!(kind, Kind::OwnUnit | Kind::EnemyUnit),
        SpellTarget::OwnCity => kind == Kind::OwnCity,
        SpellTarget::EnemyCity => kind == Kind::EnemyCity,
        SpellTarget::Tile => kind == Kind::Tile,
        SpellTarget::EnemyWizard => kind == Kind::OtherWizard,
        // The others take no target of these kinds, or none at all.
        _ => false,
    }
}

/// The target of each kind in the game, for the wizard `p` against the wizard `other`.
fn target_of(g: &Game, p: PlayerId, other: PlayerId, kind: Kind) -> Option<Target> {
    let unit_of = |who: PlayerId| g.units().player_units(who).into_iter().next();
    let city_of = |who: PlayerId| g.economy().cities_of(who).into_iter().next().map(|c| c.0);
    match kind {
        Kind::OwnUnit => unit_of(p).map(Target::Unit),
        Kind::EnemyUnit => unit_of(other).map(Target::Unit),
        Kind::OwnCity => city_of(p).map(Target::City),
        Kind::EnemyCity => city_of(other).map(Target::City),
        Kind::SelfWizard => Some(Target::Player(p)),
        Kind::OtherWizard => Some(Target::Player(other)),
        Kind::Tile => {
            // An empty land square next to the caster's first city.
            let at = g.economy().cities_of(p).first()?.1;
            f4x_core_around(g, at)
                .into_iter()
                .find(|&q| g.units().units_at(q).is_empty() && g.economy().city_at(q).is_none())
                .map(Target::Tile)
        }
    }
}

fn f4x_core_around(g: &Game, at: Position) -> Vec<Position> {
    crate::support::around(g, at)
}

/// Whether the refusal is about the kind of target, not about something else the spell asks.
fn about_the_kind(why: MagicError) -> bool {
    matches!(
        why,
        MagicError::NeedsTarget
            | MagicError::Target(
                TargetError::WrongTarget
                    | TargetError::NotYourUnit
                    | TargetError::NotEnemyUnit
                    | TargetError::NotYourCity
                    | TargetError::NotEnemyCity
                    | TargetError::NotEnemyWizard
                    | TargetError::NotGlobalSpell
                    // Evil Presence names an enemy city; on a city of a wizard with death magic it is
                    // refused as wasted before the owner is looked at (`spell/cast.rs`).
                    | TargetError::EvilPresenceWasted
            )
    )
}

#[test]
fn a_spell_is_cast_only_on_the_kinds_of_target_its_definition_names() {
    let (_, base) = worlds().remove(0);
    let snapshot = Snapshot::from_text(&text(&base)).expect("reads");
    let wizards: Vec<PlayerId> = base
        .player_ids()
        .into_iter()
        .filter(|&p| !base.is_neutral_player(p) && base.wizard(p).is_some())
        .collect();
    let (p, other) = (wizards[0], wizards[1]);
    let spells: Vec<SpellId> = base
        .content()
        .rules
        .spells
        .defs()
        .iter()
        .filter(|d| d.contexts.contains(&CastContext::Overland))
        .map(|d| SpellId::new(d.num))
        .collect();
    let stride = if f4x_def::testdir::deep() { 1 } else { 3 };
    let (mut accepted, mut refused_kind, mut tried) = (0, 0, 0);
    for spell in spells.into_iter().step_by(stride) {
        let def = base.content().rules.spells.get(spell).clone();
        // The spells that name no target, or a hero, a fortress or a circle, are not in the table.
        if !matches!(
            def.target,
            SpellTarget::OwnUnit
                | SpellTarget::EnemyUnit
                | SpellTarget::AnyUnit
                | SpellTarget::OwnCity
                | SpellTarget::EnemyCity
                | SpellTarget::Tile
                | SpellTarget::EnemyWizard
        ) {
            continue;
        }
        for kind in KINDS {
            let mut g = snapshot.clone().restore(content()).expect("restores");
            let Some(target) = target_of(&g, p, other, kind) else {
                continue;
            };
            // A summoning spell takes no target while its wizard has a circle to summon at.
            if !g.spell_needs_target(p, spell) {
                continue;
            }
            g.cheat_learn_spell(p, spell);
            g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(100_000);
            // Paid for, so that the target is chosen now.
            g.scenario_casting(p, spell, 0);
            let before = text(&g);
            let events = g.apply(Command::ResolveCast { player: p, target });
            tried += 1;
            let refusal = events.iter().find_map(|e| match e {
                Event::Rejected(Rejection::Magic(why)) => Some(*why),
                _ => None,
            });
            match (named(def.target, kind), refusal) {
                (false, None) => panic!(
                    "{}: cast on {kind:?}, which it does not name ({:?})",
                    def.id, def.target
                ),
                (false, Some(why)) => {
                    assert!(about_the_kind(why), "{}: on {kind:?}: {why:?}", def.id);
                    assert_eq!(
                        text(&g),
                        before,
                        "{}: a refused cast changed the game",
                        def.id
                    );
                    refused_kind += 1;
                }
                (true, None) => accepted += 1,
                // A Spell Ward is aimed at a city and a realm (`Target::CityRealm`), not at a city alone.
                (true, Some(why)) => assert!(
                    def.id == "spell_ward"
                        || !matches!(why, MagicError::Target(TargetError::WrongTarget)),
                    "{}: on {kind:?}, which it names: {why:?}",
                    def.id
                ),
            }
        }
    }
    assert!(
        tried > 100 && accepted > 10 && refused_kind > 100,
        "{tried} {accepted} {refused_kind}"
    );
}

fn refusal(events: &[Event]) -> Option<Rejection> {
    events.iter().find_map(|e| match e {
        Event::Rejected(r) => Some(*r),
        _ => None,
    })
}

/// A wizard with two cities, the circle in the first, and a summoning spell they know.
pub(crate) fn two_cities(g: &mut Game) -> (PlayerId, hecs::Entity, hecs::Entity, SpellId) {
    let p = g
        .player_ids()
        .into_iter()
        .find(|&p| !g.is_neutral_player(p) && g.wizard(p).is_some())
        .expect("a wizard");
    // A second city, a few squares from the first.
    let home = g.economy().cities_of(p)[0].1;
    'found: for dx in -6i32..=6 {
        for dy in -6i32..=6 {
            let (x, y) = (home.x as i32 + dx, home.y as i32 + dy);
            if x < 0 || y < 0 {
                continue;
            }
            let pos = Position {
                x: x as u32,
                y: y as u32,
                plane: home.plane,
            };
            let events = g.apply(Command::FoundCity {
                owner: p,
                pos,
                name: "Second".to_owned(),
                race: None,
                population: 3,
            });
            if refusal(&events).is_none() {
                break 'found;
            }
        }
    }
    let cities = g.economy().cities_of(p);
    assert!(cities.len() >= 2, "no second city could be founded");
    let spell = g
        .content()
        .rules
        .spells
        .defs()
        .iter()
        .map(|d| SpellId::new(d.num))
        .find(|&s| {
            let d = g.content().rules.spells.get(s);
            d.contexts.contains(&CastContext::Overland)
                && g.spell_summons(s)
                && d.target == SpellTarget::Tile
        })
        .expect("a summoning spell");
    g.cheat_learn_spell(p, spell);
    g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(100_000);
    (p, cities[0].0, cities[1].0, spell)
}

fn pos_of(g: &Game, p: PlayerId, city: hecs::Entity) -> Position {
    g.economy()
        .cities_of(p)
        .into_iter()
        .find(|c| c.0 == city)
        .expect("a city")
        .1
}

fn circles(g: &Game, p: PlayerId) -> Vec<Position> {
    g.wizard(p).expect("a wizard").summon_circles()
}

#[test]
fn a_second_summoning_circle_replaces_the_first_unless_the_setting_keeps_both() {
    for keep in [false, true] {
        let (_, mut g) = worlds().remove(0);
        g.scenario_change_settings(|s| s.rules.multiple_summoning_circles = keep);
        let (p, first, second, _) = two_cities(&mut g);
        let (a, b) = (pos_of(&g, p, first), pos_of(&g, p, second));
        assert_eq!(circles(&g, p), [a]);
        g.move_summoning_circle(p, second).expect("moved");
        assert_eq!(circles(&g, p), if keep { vec![a, b] } else { vec![b] });
        // Again in the same city: still one circle there.
        g.move_summoning_circle(p, second).expect("moved");
        assert_eq!(circles(&g, p).iter().filter(|&&c| c == b).count(), 1);
        // The fortress moved to the second city keeps the circles too.
        g.move_fortress(p, second).expect("moved");
        assert_eq!(circles(&g, p).contains(&a), keep);
        // The text of the save keeps them.
        let again = Snapshot::from_text(&text(&g))
            .expect("reads")
            .restore(content())
            .expect("restores");
        assert_eq!(circles(&again, p), circles(&g, p));
    }
}

#[test]
fn with_several_circles_a_summoning_asks_for_the_city_and_the_creature_appears_there() {
    let (_, mut g) = worlds().remove(0);
    g.scenario_change_settings(|s| s.rules.multiple_summoning_circles = true);
    let (p, first, second, spell) = two_cities(&mut g);
    let (a, b) = (pos_of(&g, p, first), pos_of(&g, p, second));
    // The spell is paid for, so that its target is chosen now.
    let cast = |g: &mut Game, target: Option<Target>| match target {
        Some(target) => g.apply(Command::ResolveCast { player: p, target }),
        None => vec![Event::Rejected(Rejection::Magic(
            g.check_target(p, spell, None)
                .expect_err("a target is asked"),
        ))],
    };

    // One circle: no target is asked.
    assert!(!g.spell_needs_target(p, spell));
    // Two: the city is asked for, and only a city with a circle will do.
    g.move_summoning_circle(p, second).expect("moved");
    assert!(g.spell_needs_target(p, spell));
    let third = g
        .economy()
        .cities_of(p)
        .into_iter()
        .map(|c| c.0)
        .find(|&c| c != first && c != second);
    g.scenario_casting(p, spell, 0);
    let before = text(&g);
    assert_eq!(
        refusal(&cast(&mut g, None)),
        Some(Rejection::Magic(MagicError::NeedsTarget))
    );
    if let Some(third) = third {
        assert!(refusal(&cast(&mut g, Some(Target::City(third)))).is_some());
    }
    let rival = g
        .economy()
        .cities()
        .into_iter()
        .find(|c| c.1.owner != p)
        .map(|c| c.0)
        .expect("a rival city");
    assert!(refusal(&cast(&mut g, Some(Target::City(rival)))).is_some());
    assert_eq!(text(&g), before, "a refused cast changed the game");
    for (city, at) in [(second, b), (first, a)] {
        g.scenario_casting(p, spell, 0);
        let here = g.units().units_at(at).len();
        let events = cast(&mut g, Some(Target::City(city)));
        assert_eq!(refusal(&events), None, "{events:?}");
        assert!(
            g.units().units_at(at).len() > here,
            "nothing appeared at {at:?}"
        );
    }
}

#[test]
fn with_several_circles_word_of_recall_asks_for_the_circle_and_sends_the_unit_there() {
    use f4x_core::spell::special::{RECALL, RECALL_ANYONE};
    let (_, mut g) = worlds().remove(0);
    g.scenario_change_settings(|s| s.rules.multiple_summoning_circles = true);
    let (p, _, second, _) = two_cities(&mut g);
    let recall = g
        .content()
        .rules
        .spells
        .defs()
        .iter()
        .filter(|d| d.contexts.contains(&CastContext::Overland))
        .find(|d| d.has_hook_arg(RECALL, RECALL_ANYONE))
        .map(|d| SpellId::new(d.num))
        .expect("a Word of Recall");
    g.cheat_learn_spell(p, recall);
    g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(100_000);
    g.move_summoning_circle(p, second).expect("moved");
    let circles = g.wizard(p).expect("a wizard").summon_circles();
    let (a, b) = (circles[0], circles[1]);
    let unit = g.units().player_units(p)[0];
    g.cheat_move_unit(unit, a);
    assert!(g.check_target(p, recall, Some(Target::Unit(unit))).is_err());
    let elsewhere = Position {
        x: a.x.wrapping_add(1),
        ..a
    };
    assert_eq!(
        g.check_target(p, recall, Some(Target::UnitAt(unit, elsewhere))),
        Err(MagicError::Target(TargetError::WrongCircle))
    );
    g.scenario_casting(p, recall, 0);
    let events = g.apply(Command::ResolveCast {
        player: p,
        target: Target::UnitAt(unit, b),
    });
    assert_eq!(refusal(&events), None, "{events:?}");
    assert_eq!(g.units().unit_position(unit), Some(b));
}

/// A wizard who knows Time Stop (spell 80, `time_stop`, the dearest spell with no target) and has the
/// mana for it: a cast that takes many turns to pay.
fn slow_caster() -> Vec<(String, Game, PlayerId, SpellId)> {
    worlds()
        .into_iter()
        .map(|(name, mut g)| {
            let p = g
                .wizards()
                .into_iter()
                .find(|&p| g.wizard(p).is_some())
                .expect("a wizard");
            let spell = g.content().rules.spells.id_numbered(80).expect("time_stop");
            g.cheat_learn_spell(p, spell);
            g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(100_000);
            (name, g, p, spell)
        })
        .collect()
}

#[test]
fn a_spell_dearer_than_the_skill_is_paid_over_turns_and_takes_effect_when_paid() {
    for (name, mut g, p, spell) in slow_caster() {
        let events = g.apply(Command::CastSpell {
            player: p,
            spell,
            target: None,
            extra: 0,
        });
        assert_eq!(refusal(&events), None, "{name}: {events:?}");
        let mut owed = g
            .wizard(p)
            .expect("a wizard")
            .casting()
            .map(|c| c.remaining);
        assert!(
            owed.is_some_and(|o| o > 0),
            "{name}: a spell dearer than a turn's skill is not begun: {owed:?}"
        );
        let mut turns = 0;
        let mut cast = false;
        while turns < 5_000 && !cast {
            turns += 1;
            let events = g.end_player_turn(p);
            cast = events
                .iter()
                .any(|e| matches!(e, Event::SpellCast { player, spell: s } if *player == p && *s == spell));
            let now = g
                .wizard(p)
                .expect("a wizard")
                .casting()
                .map(|c| c.remaining);
            if !cast {
                assert!(
                    now.zip(owed).is_some_and(|(n, o)| n < o),
                    "{name}: turn {turns}: nothing was paid: {owed:?} then {now:?}"
                );
            }
            owed = now;
        }
        assert!(cast, "{name}: not cast in {turns} turns");
        assert!(turns > 1, "{name}: paid in one turn after all");
        assert!(g.wizard(p).expect("a wizard").casting().is_none(), "{name}");
        assert_eq!(
            g.time_stopper(),
            Some(p),
            "{name}: the spell did not take effect"
        );
    }
}

#[test]
fn cancelling_a_cast_stops_it_and_cancelling_nothing_changes_nothing() {
    for (name, mut g, p, spell) in slow_caster() {
        let before = crate::support::text(&g);
        let events = g.apply(Command::CancelCast { player: p });
        assert_eq!(refusal(&events), None, "{name}: {events:?}");
        assert_eq!(
            crate::support::text(&g),
            before,
            "{name}: cancelling nothing changed the game"
        );
        g.apply(Command::CastSpell {
            player: p,
            spell,
            target: None,
            extra: 0,
        });
        g.end_player_turn(p);
        assert!(g.wizard(p).expect("a wizard").casting().is_some(), "{name}");
        let events = g.apply(Command::CancelCast { player: p });
        assert_eq!(refusal(&events), None, "{name}: {events:?}");
        assert!(g.wizard(p).expect("a wizard").casting().is_none(), "{name}");
        for _ in 0..3 {
            let events = g.end_player_turn(p);
            assert!(
                !events.iter().any(|e| matches!(e, Event::SpellCast { .. })),
                "{name}: a cancelled spell was cast"
            );
        }
        assert_eq!(g.time_stopper(), None, "{name}");
    }
}

#[test]
fn a_wizard_who_cannot_pay_the_upkeep_gives_up_the_dearest_spells_first() {
    let mut tried = 0;
    for (name, mut g) in worlds() {
        let p = g.wizards()[0];
        // Lasting spells of the world, in the order they were cast: those of the content that are
        // global and cost upkeep.
        let held: Vec<SpellId> = g
            .content()
            .rules
            .spells
            .defs()
            .iter()
            .filter(|d| {
                d.contexts.contains(&CastContext::Overland)
                    && d.target == SpellTarget::None
                    && d.upkeep > 0
                    // Its keeper pays for that spell alone.
                    && d.num != 80 /* time_stop */
                    && d.duration == f4x_def::rules::Duration::Permanent
            })
            .map(|d| SpellId::new(d.num))
            .collect();
        for &s in &held {
            g.scenario_add_global(p, s);
        }
        let kept_before: Vec<_> = held
            .iter()
            .copied()
            .filter(|&s| g.has_global(p, s))
            .collect();
        if kept_before.len() < 3 {
            continue;
        }
        tried += 1;
        g.treasury_mut(p).expect("a treasury").mana = HalfInt::ZERO;
        let events = g.end_player_turn(p);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Shortage { player, what: f4x_core::game::Shortage::Mana } if *player == p)),
            "{name}: no shortage told: {events:?}"
        );
        let gone: Vec<usize> = (0..kept_before.len())
            .filter(|&i| !g.has_global(p, kept_before[i]))
            .collect();
        let stayed: Vec<usize> = (0..kept_before.len())
            .filter(|&i| g.has_global(p, kept_before[i]))
            .collect();
        assert!(!gone.is_empty(), "{name}: nothing was given up");
        // The order of giving up: the highest upkeep, then the cheapest to replace, then the newest
        // (`upkeep_release_order`). Whatever went comes before whatever stayed in it.
        let rules = &g.content().rules;
        let key = |i: usize| {
            let d = rules.spells.get(kept_before[i]);
            (std::cmp::Reverse(d.upkeep), d.cost, std::cmp::Reverse(i))
        };
        let last_gone = gone.iter().map(|&i| key(i)).max().expect("one");
        assert!(
            stayed.iter().all(|&i| key(i) > last_gone),
            "{name}: a spell that stayed comes before one that went in the order of giving up"
        );
        let lapsed = events
            .iter()
            .filter(|e| matches!(e, Event::EnchantmentLapsed { player, .. } if *player == p))
            .count();
        assert_eq!(lapsed, gone.len(), "{name}: {events:?}");
    }
    assert!(tried > 0, "no world held enough lasting spells");
}

#[test]
fn a_lasting_spell_takes_hold_on_its_target_once_and_turning_it_off_ends_it() {
    use f4x_core::check::world_problems;
    use f4x_core::spell::instance::SpellTarget as On;
    use f4x_def::rules::Duration;
    let (_, base) = worlds().remove(0);
    let snapshot = Snapshot::from_text(&text(&base)).expect("reads");
    let wizards: Vec<PlayerId> = base
        .player_ids()
        .into_iter()
        .filter(|&p| !base.is_neutral_player(p) && base.wizard(p).is_some())
        .collect();
    let (p, other) = (wizards[0], wizards[1]);
    let stride = if f4x_def::testdir::deep() { 1 } else { 2 };
    let (mut held, mut ended) = (0, 0);
    let spells: Vec<SpellId> = base
        .content()
        .rules
        .spells
        .defs()
        .iter()
        .filter(|d| {
            d.contexts.contains(&CastContext::Overland) && d.duration == Duration::Permanent
        })
        .map(|d| SpellId::new(d.num))
        .collect();
    for spell in spells.into_iter().step_by(stride) {
        let def = base.content().rules.spells.get(spell).clone();
        // The kind of target each names; the spells with no target or another kind are not in this table.
        let kind = match def.target {
            SpellTarget::OwnUnit | SpellTarget::AnyUnit => Kind::OwnUnit,
            SpellTarget::EnemyUnit => Kind::EnemyUnit,
            SpellTarget::OwnCity => Kind::OwnCity,
            SpellTarget::EnemyCity => Kind::EnemyCity,
            _ => continue,
        };
        let mut g = snapshot.clone().restore(content()).expect("restores");
        let Some(target) = target_of(&g, p, other, kind) else {
            continue;
        };
        g.cheat_learn_spell(p, spell);
        g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(100_000);
        g.scenario_casting(p, spell, 0);
        let events = g.apply(Command::ResolveCast { player: p, target });
        if refusal(&events).is_some() {
            // A refusal for what the target is (a unit the spell does not take) is the matrix's.
            continue;
        }
        let on = match target {
            Target::Unit(u) => On::Unit(u),
            Target::City(c) => On::City(c),
            _ => unreachable!("a unit or a city"),
        };
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::SpellCast { player, spell: s } if *player == p && *s == spell)),
            "{}: the cast is not told: {events:?}",
            def.id
        );
        let wizard = g.wizard(p).expect("a wizard").entity();
        let spells = g.spells();
        let mine: Vec<_> = spells
            .of_def(spell)
            .into_iter()
            .filter(|&i| spells.target(i) == Some(on) && spells.caster(i) == Some(wizard))
            .collect();
        assert_eq!(mine.len(), 1, "{}: on {kind:?}: {events:?}", def.id);
        held += 1;
        // A second cast on the same target never doubles it.
        g.scenario_casting(p, spell, 0);
        g.apply(Command::ResolveCast { player: p, target });
        assert_eq!(
            g.spells()
                .of_def(spell)
                .into_iter()
                .filter(|&i| g.spells().target(i) == Some(on))
                .count(),
            1,
            "{}: the spell is on the target twice",
            def.id
        );
        // The wizard turns it off.
        let off = match target {
            Target::Unit(unit) => Command::TurnOffUnitSpell {
                player: p,
                unit,
                spell,
            },
            Target::City(city) => Command::TurnOffCitySpell {
                player: p,
                city,
                spell,
            },
            _ => unreachable!(),
        };
        let events = g.apply(off);
        assert_eq!(refusal(&events), None, "{}: {events:?}", def.id);
        assert!(
            !g.spells().has(on, spell),
            "{}: still on after turning it off",
            def.id
        );
        ended += 1;
        let bad = world_problems(&g);
        assert!(bad.is_empty(), "{}: {bad:#?}", def.id);
    }
    assert!(held > 10 && ended == held, "{held} held, {ended} ended");
}

/// The first lasting spells of the content that name a city of their own caster, for a city to carry.
fn city_spells(g: &Game, n: usize) -> Vec<SpellId> {
    g.content()
        .rules
        .spells
        .defs()
        .iter()
        .filter(|d| {
            d.contexts.contains(&CastContext::Overland)
                && d.target == SpellTarget::OwnCity
                && d.duration == f4x_def::rules::Duration::Permanent
        })
        .map(|d| SpellId::new(d.num))
        .take(n)
        .collect()
}

/// Two wizards who have met, the first with plenty of mana.
fn rivals() -> Vec<(String, Game, PlayerId, PlayerId)> {
    worlds()
        .into_iter()
        .map(|(name, mut g)| {
            let w: Vec<PlayerId> = g
                .player_ids()
                .into_iter()
                .filter(|&p| !g.is_neutral_player(p) && g.wizard(p).is_some())
                .collect();
            g.make_contact(w[0], w[1]);
            g.make_contact(w[1], w[0]);
            g.treasury_mut(w[0]).expect("a treasury").mana = HalfInt::from_int(100_000);
            (name, g, w[0], w[1])
        })
        .collect()
}

/// `player` casts spell `number` on `target` with `extra` mana in it, paid at once.
fn cast_at(g: &mut Game, player: PlayerId, number: u16, target: Target, extra: i32) -> Vec<Event> {
    let spell = SpellId::new(number);
    g.cheat_learn_spell(player, spell);
    g.scenario_casting_with(player, spell, 0, extra);
    g.apply(Command::ResolveCast { player, target })
}

#[test]
fn a_strong_disenchant_area_takes_off_the_other_wizards_spells_and_leaves_the_casters_own() {
    use f4x_core::spell::instance::SpellTarget as On;
    for (name, mut g, p, other) in rivals() {
        let city = g.economy().cities_of(other)[0].0;
        let at = g.economy().cities_of(other)[0].1;
        let spells = city_spells(&g, 2);
        assert_eq!(spells.len(), 2, "{name}");
        g.scenario_enchant_city(city, spells[0], other);
        g.scenario_enchant_city(city, spells[1], p);
        let events = cast_at(
            &mut g,
            p,
            204, /* disenchant_area */
            Target::Tile(at),
            100_000,
        );
        assert_eq!(refusal(&events), None, "{name}: {events:?}");
        assert!(
            !g.spells().has(On::City(city), spells[0]),
            "{name}: the rival's spell stayed under a dispel of any strength"
        );
        assert!(
            g.spells().has(On::City(city), spells[1]),
            "{name}: the caster's own spell was dispelled"
        );
    }
}

#[test]
fn a_dispel_on_a_bare_square_is_refused_as_there_being_nothing_to_dispel() {
    for (name, mut g, p, other) in rivals() {
        let at = g.economy().cities_of(other)[0].1;
        let events = cast_at(
            &mut g,
            p,
            204, /* disenchant_area */
            Target::Tile(at),
            0,
        );
        assert_eq!(
            refusal(&events),
            Some(Rejection::Magic(MagicError::Target(
                TargetError::NothingToDispel
            ))),
            "{name}: {events:?}"
        );
    }
}

#[test]
fn a_strong_disjunction_takes_a_rivals_global_spell_and_is_refused_on_the_casters_own() {
    for (name, mut g, p, other) in rivals() {
        let global = SpellId::new(158 /* crusade */);
        g.scenario_add_global(other, global);
        g.scenario_add_global(p, global);
        let theirs = g.global_instance(other, global).expect("an instance");
        let own = g.global_instance(p, global).expect("an instance");
        let events = cast_at(
            &mut g,
            p,
            210, /* disjunction */
            Target::Spell(own),
            100_000,
        );
        assert_eq!(
            refusal(&events),
            Some(Rejection::Magic(MagicError::Target(
                TargetError::NotGlobalSpell
            ))),
            "{name}: {events:?}"
        );
        assert!(g.has_global(p, global), "{name}");
        let events = cast_at(&mut g, p, 210, Target::Spell(theirs), 100_000);
        assert_eq!(refusal(&events), None, "{name}: {events:?}");
        assert!(!g.has_global(other, global), "{name}: the rival keeps it");
        assert!(g.has_global(p, global), "{name}: the caster lost its own");
        assert!(
            events.iter().any(
                |e| matches!(e, Event::GlobalSpellRemoved { player, victim, spell }
                if *player == p && *victim == other && *spell == global)
            ),
            "{name}: {events:?}"
        );
    }
}

#[test]
fn spell_blast_ends_a_cast_at_the_price_of_what_its_wizard_had_spent() {
    for (name, mut g, p, other) in rivals() {
        let spell = SpellId::new(80 /* time_stop */);
        let cost = i32::try_from(g.content().rules.spells.get(spell).cost).expect("a cost");
        let owed = cost / 3;
        g.cheat_learn_spell(other, spell);
        g.scenario_casting(other, spell, owed);
        let before = g.player(p).expect("a player").treasury().mana;
        let events = cast_at(
            &mut g,
            p,
            58, /* spell_blast */
            Target::Player(other),
            0,
        );
        assert_eq!(refusal(&events), None, "{name}: {events:?}");
        let paid = events
            .iter()
            .find_map(|e| match e {
                Event::SpellBlasted { mana, victim, .. } if *victim == other => Some(*mana),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name}: no blast told: {events:?}"));
        assert_eq!(paid, cost - owed, "{name}: the price is what was spent");
        assert!(
            g.wizard(other).expect("a wizard").casting().is_none(),
            "{name}"
        );
        assert!(
            g.player(p).expect("a player").treasury().mana <= before - HalfInt::from_int(paid),
            "{name}: the blast cost the caster nothing"
        );
    }
}

#[test]
fn spell_blast_is_refused_when_the_caster_cannot_match_the_mana() {
    for (name, mut g, p, other) in rivals() {
        let spell = SpellId::new(80 /* time_stop */);
        g.cheat_learn_spell(other, spell);
        g.scenario_casting(other, spell, 1);
        g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(1);
        let events = cast_at(&mut g, p, 58, Target::Player(other), 0);
        assert!(refusal(&events).is_some(), "{name}: {events:?}");
        assert!(
            g.wizard(other).expect("a wizard").casting().is_some(),
            "{name}: the cast was ended"
        );
    }
}

#[test]
fn a_spell_gives_way_to_the_stronger_ones_it_is_not_cumulative_with_and_not_the_other_way() {
    let (_, base) = worlds().remove(0);
    let snapshot = Snapshot::from_text(&text(&base)).expect("reads");
    let wizards: Vec<PlayerId> = base
        .player_ids()
        .into_iter()
        .filter(|&p| !base.is_neutral_player(p) && base.wizard(p).is_some())
        .collect();
    let (p, other) = (wizards[0], wizards[1]);
    let unit_target = |t: SpellTarget| {
        matches!(
            t,
            SpellTarget::OwnUnit | SpellTarget::AnyUnit | SpellTarget::EnemyUnit
        )
    };
    let mut pairs = Vec::new();
    for def in base.content().rules.spells.defs() {
        if !def.contexts.contains(&CastContext::Overland) || !unit_target(def.target) {
            continue;
        }
        for &with in &def.not_with {
            pairs.push((SpellId::new(def.num), with));
        }
    }
    let (mut refused, mut free) = (0, 0);
    for (newer, held) in pairs {
        let (new_def, held_def) = (
            base.content().rules.spells.get(newer).clone(),
            base.content().rules.spells.get(held).clone(),
        );
        let kind = if new_def.target == SpellTarget::EnemyUnit {
            Kind::EnemyUnit
        } else {
            Kind::OwnUnit
        };
        let mut g = snapshot.clone().restore(content()).expect("restores");
        let Some(target) = target_of(&g, p, other, kind) else {
            continue;
        };
        let Target::Unit(unit) = target else { continue };
        g.cheat_learn_spell(p, newer);
        g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(100_000);
        // What it must give way to: on the unit, or among the caster's global enchantments.
        if unit_target(held_def.target) {
            g.scenario_enchant_unit(unit, held, p);
        } else if held_def.target == SpellTarget::None {
            g.scenario_add_global(p, held);
        } else {
            continue;
        }
        g.scenario_casting(p, newer, 0);
        let events = g.apply(Command::ResolveCast { player: p, target });
        match refusal(&events) {
            Some(Rejection::Magic(MagicError::NotCumulative)) => refused += 1,
            // The unit may not be the kind the spell takes at all: the matrix's business.
            Some(Rejection::Magic(MagicError::Target(_))) => {}
            other => panic!(
                "{} over {}: not refused for being not cumulative: {other:?}",
                new_def.id, held_def.id
            ),
        }
        // The other way round: the stronger spell goes on over the weaker one, unless it too names it.
        if unit_target(held_def.target) && !held_def.not_with.contains(&newer) {
            let mut g = snapshot.clone().restore(content()).expect("restores");
            let Some(Target::Unit(unit)) = target_of(&g, p, other, kind) else {
                continue;
            };
            g.cheat_learn_spell(p, held);
            g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(100_000);
            g.scenario_enchant_unit(unit, newer, p);
            g.scenario_casting(p, held, 0);
            let events = g.apply(Command::ResolveCast {
                player: p,
                target: Target::Unit(unit),
            });
            assert_ne!(
                refusal(&events),
                Some(Rejection::Magic(MagicError::NotCumulative)),
                "{} over {}: refused both ways",
                held_def.id,
                new_def.id
            );
            free += 1;
        }
    }
    assert!(refused > 0 && free > 0, "{refused} refused, {free} free");
}
