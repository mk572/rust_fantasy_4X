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
