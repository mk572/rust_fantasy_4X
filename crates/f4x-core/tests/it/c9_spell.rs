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
