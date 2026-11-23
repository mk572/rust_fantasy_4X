//! What the AI makes of the overland spells beyond summons and plain buffs (task 509). Each is
//! read from the effect the rules give the spell (the typed hooks of `f4x-core`: `GlobalEffect`,
//! `TerrainEffect`, `WorldKill`, the gate hook, the damage a spell does to a square), never from
//! its number, so a spell that carries an effect is cast for it. The match over a hook's enum
//! has no catch-all: a new effect has to be given a use here before the code builds.
//!
//! The intents are: the lasting spells on the whole world (a boon for the caster, a boon for a
//! wizard whose books lean to the realm, a strike at the wizards at war, a strike at the wizards
//! at war that have books in a realm, or none), a storm on an enemy stack at a city, a curse on
//! an enemy's land or node, a volcano on the wizard's own land, and a gate for the army to cross
//! the planes by. The targets are picked here too; `wizardry.rs` decides when each is cast.
//!
//! Tests: at the end of this file, on the shipped rules.

use f4x_core::city::gate::{ASTRAL_GATE, EARTH_GATE, GATE};
use f4x_core::diplomacy::Treaty;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::rules::exec::Target;
use f4x_core::spell::global_enchantment::GlobalEffect;
use f4x_core::spell::terraform::TerrainEffect;
use f4x_core::spell::unit_effect::UnitEffect;
use f4x_core::spell::wizard::WorldKill;
use f4x_def::rules::effects::{Effect, TargetSel};
use f4x_def::rules::{RealmId, SpellDef, SpellId, SpellTarget, Stat, UnitKind};

use crate::view::{fighting_strength, is_fighter};

/// What a lasting spell on the whole world is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalUse {
    /// It helps its caster whatever the books.
    Boon,
    /// It helps a caster whose books lean to the spell's realm: half or more of them.
    Realm,
    /// It strikes the other wizards: begun while at war with one.
    War,
    /// It strikes the wizards at war that have a book in one of the realms (the first list); a
    /// caster with a book in one of the second list would be hurt by it too and leaves it.
    Against(&'static [u16], &'static [u16]),
    /// It hurts its caster as much as the others, or traps it: never cast.
    Never,
}

/// The effect of a global spell, if it has one.
pub(crate) fn global_effect(def: &SpellDef) -> Option<GlobalEffect> {
    GlobalEffect::NAMES
        .iter()
        .filter_map(|n| GlobalEffect::named(n))
        .find(|e| e.is_in(def))
}

/// What a global effect is for. No catch-all: a new effect is given its use here.
pub(crate) fn global_use(effect: GlobalEffect) -> GlobalUse {
    use GlobalEffect as E;
    match effect {
        // Seeing the whole map, faster ships, goodwill, fame and fewer rebels, strong armies,
        // the other wizards' spells in view: each helps whoever casts it.
        E::NatureAwareness
        | E::HerbMastery
        | E::WindMastery
        | E::AuraOfMajesty
        | E::JustCause
        | E::HolyArms
        | E::Crusade
        | E::CharmOfLife
        | E::DetectMagic
        | E::Awareness => GlobalUse::Boon,
        // Help the creatures, the zombies and the darkness of one realm: a wizard of that realm.
        E::ChaosSurge | E::DoomMastery | E::ZombieMastery | E::EternalNight => GlobalUse::Realm,
        // Strike at every other wizard while at war.
        E::TimeStop | E::MeteorStorm | E::GreatWasting | E::Armageddon | E::SuppressMagic => {
            GlobalUse::War
        }
        // Strike the spells of the realms they are set against.
        E::NaturesWrath => GlobalUse::Against(&[4 /* death */, 2 /* chaos */], &[]),
        E::LifeForce => GlobalUse::Against(&[4 /* death */], &[]),
        E::Tranquility => GlobalUse::Against(&[2 /* chaos */], &[]),
        // Life and nature spells cost half as much again for every wizard, the caster too.
        E::EvilOmens => {
            GlobalUse::Against(
                &[3 /* life */, 0 /* nature */],
                &[3 /* life */, 0 /* nature */],
            )
        }
        // No unit of the caster can change plane either.
        E::PlanarSeal => GlobalUse::Never,
    }
}

/// What a global spell is for, by its effect (a spell that kills across the map is a strike at
/// the wizards at war, unless it kills the caster's creatures too).
pub(crate) fn global_use_of(def: &SpellDef) -> Option<GlobalUse> {
    if let Some(effect) = global_effect(def) {
        return Some(global_use(effect));
    }
    WorldKill::NAMES
        .iter()
        .filter_map(|n| WorldKill::named(n))
        .find(|k| k.is_in(def))
        .map(|_| GlobalUse::War)
}

/// Whether the wizard's books lean to the realm: half of them or more.
fn leans_to(view: &PlayerView, realm: RealmId) -> bool {
    view.wizard()
        .is_some_and(|w| w.books_in(realm) * 2 >= w.total_books().max(1) && w.books_in(realm) > 0)
}

/// Whether a lasting spell on the whole world has its use now: the wizard it helps, or the
/// wizard at war it strikes.
pub(crate) fn global_wanted(view: &PlayerView, def: &SpellDef) -> bool {
    let Some(kind) = global_use_of(def) else {
        return false;
    };
    match kind {
        GlobalUse::Boon => true,
        GlobalUse::Realm => leans_to(view, def.realm),
        GlobalUse::War => !view.rivals_at_war().is_empty() && !hurts_own_creatures(view, def),
        GlobalUse::Against(victims, spare) => {
            let realms = |n: &[u16]| n.iter().map(|&r| RealmId::new(r)).collect::<Vec<_>>();
            view.rival_at_war_has_books(&realms(victims))
                && !view
                    .wizard()
                    .is_some_and(|w| realms(spare).iter().any(|&r| w.books_in(r) > 0))
        }
        GlobalUse::Never => false,
    }
}

/// Great Unsummoning banishes the caster's own creatures with the others': not cast while the
/// wizard has any.
fn hurts_own_creatures(view: &PlayerView, def: &SpellDef) -> bool {
    WorldKill::GreatUnsummoning.is_in(def)
        && view.my_units().into_iter().any(|u| {
            view.unit_def(u)
                .is_some_and(|d| matches!(d.kind, UnitKind::Summoned | UnitKind::Fantastic))
        })
}

/// Whether a modifier of a city or unit stat that is added in this direction is a gain: more
/// unrest is a loss, less is a gain; for the other stats more is a gain.
pub(crate) fn gain_when_added(stat: Stat, value_is_up: bool) -> bool {
    match stat {
        Stat::CityUnrest | Stat::CityUnrestPercent => !value_is_up,
        _ => value_is_up,
    }
}

// ---- storms on a stack, curses on land, volcanoes, gates ---------------------------------

/// A spell that strikes every unit on a square of the map: a storm, or the Black Wind.
pub(crate) fn is_stack_strike(def: &SpellDef) -> bool {
    def.target == SpellTarget::Tile
        && (def.effects.iter().any(|r| {
            matches!(r.effect, Effect::Damage { .. }) && r.target == TargetSel::UnitsOnChosenSquare
        }) || UnitEffect::of(def) == Some(UnitEffect::BlackWind))
}

/// A spell that makes an enemy's land worth less: Corruption on its fields, Warp Node on its node.
pub(crate) fn is_land_curse(def: &SpellDef) -> bool {
    TerrainEffect::Corruption.is_in(def) || TerrainEffect::WarpNode.is_in(def)
}

/// A spell that raises something of use to the caster on a square of its own land: a volcano.
pub(crate) fn is_land_boost(def: &SpellDef) -> bool {
    TerrainEffect::RaiseVolcano.is_in(def)
}

/// A gate that lets a stack change plane in the city it is in.
pub(crate) fn is_astral_gate(def: &SpellDef) -> bool {
    def.has_hook_arg(GATE, ASTRAL_GATE)
}

/// A gate that teleports a stack to another city with one.
pub(crate) fn is_earth_gate(def: &SpellDef) -> bool {
    def.has_hook_arg(GATE, EARTH_GATE)
}

/// How near an own city an enemy stack must stand to be struck.
const STRIKE_RADIUS: i32 = 3;
/// The share of a city's garrison an enemy stack must match to be worth a storm.
const STRIKE_SHARE: f32 = 0.5;

/// The square of the strongest enemy stack near one of the wizard's cities that is at least half
/// as strong as what holds the city, when the spell can be aimed at it.
pub(crate) fn strike_square(view: &PlayerView, spell: SpellId) -> Option<Target> {
    let mut best: Option<(f32, Position)> = None;
    for (_, city) in view.my_cities() {
        let held: Vec<_> = view
            .garrison(city)
            .into_iter()
            .filter(|&u| is_fighter(view, u))
            .collect();
        let held = fighting_strength(view, &held);
        for (square, units) in view.enemies_near(city, STRIKE_RADIUS) {
            let strength = fighting_strength(view, &units);
            if strength >= held * STRIKE_SHARE
                && strength > 0.0
                && best.is_none_or(|(b, _)| strength > b)
                && view.can_aim(spell, Some(Target::Tile(square)))
            {
                best = Some((strength, square));
            }
        }
    }
    best.map(|(_, square)| Target::Tile(square))
}

/// The squares of the two rings round a city, nearest first, in a fixed order.
fn rings(view: &PlayerView, city: Position) -> Vec<Position> {
    let mut out = Vec::new();
    for radius in 1..=2i32 {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dy.abs()) == radius {
                    out.extend(view.shift(city, dx, dy));
                }
            }
        }
    }
    out
}

/// A square of the land of the most populous city of a wizard at war that the spell can be aimed
/// at, or the magic node such a wizard holds.
pub(crate) fn land_curse_target(view: &PlayerView, spell: SpellId) -> Option<Target> {
    let def = view.spell(spell);
    if TerrainEffect::WarpNode.is_in(def) {
        let at_war = view.rivals_at_war();
        return view
            .nodes()
            .into_iter()
            .filter(|n| n.owner.is_some_and(|o| at_war.contains(&o)))
            .map(|n| n.pos)
            .map(Target::Tile)
            .find(|&t| view.can_aim(spell, Some(t)));
    }
    let mut cities: Vec<(u32, hecs::Entity, Position)> = view
        .known_cities()
        .into_iter()
        .filter(|&(_, owner, _)| {
            view.is_rival_wizard(owner) && view.treaty_with(owner) == Treaty::War
        })
        .filter_map(|(c, _, p)| view.city_report(c).map(|r| (r.figures, c, p)))
        .collect();
    cities.sort_by_key(|&(n, c, _)| (std::cmp::Reverse(n), view.serial_of(c)));
    cities.into_iter().find_map(|(_, _, city)| {
        rings(view, city)
            .into_iter()
            .map(Target::Tile)
            .find(|&t| view.can_aim(spell, Some(t)))
    })
}

/// A free square of the wizard's own land, next to one of its cities, for a volcano; the
/// fortress city first.
pub(crate) fn land_boost_target(view: &PlayerView, spell: SpellId) -> Option<Target> {
    let fortress = view.fortress();
    let mut cities = view.my_cities();
    cities.sort_by_key(|&(c, p)| (Some(p) != fortress, view.serial_of(c)));
    cities.into_iter().find_map(|(_, city)| {
        rings(view, city)
            .into_iter()
            .map(Target::Tile)
            .find(|&t| view.can_aim(spell, Some(t)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{
        at, game, game_rival_across, shipped, spawn, COAST_CITY, ENEMY_CITY, P0, P1,
    };
    use f4x_core::diplomacy::DiplomacyCommand;
    use f4x_core::game::{Command, Game};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn spell(num: u16) -> Result<SpellId, String> {
        shipped()
            .rules
            .spells
            .id_numbered(num)
            .ok_or(format!("spell {num} is shipped"))
    }

    fn war(g: &mut Game) {
        g.make_contact(P0, P1);
        g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
    }

    fn books(g: &mut Game, player: f4x_core::player::PlayerId, books: &[(u16, u8)]) {
        let realms: Vec<_> = books.iter().map(|&(r, n)| (RealmId::new(r), n)).collect();
        if let Some(mut w) = g.wizard_mut(player) {
            w.found(&shipped().rules, &realms, &[]);
        }
    }

    fn wanted(g: &Game, num: u16) -> Result<bool, String> {
        let id = spell(num)?;
        Ok(global_wanted(
            &PlayerView::new(g, P0),
            g.content().rules.spells.get(id),
        ))
    }

    #[test]
    fn every_global_spell_has_a_use_read_from_its_effect() {
        let uses: Vec<(String, GlobalUse)> = shipped()
            .rules
            .spells
            .defs()
            .iter()
            .filter_map(|d| global_use_of(d).map(|u| (d.id.clone(), u)))
            .collect();
        let of = |name: &str| uses.iter().find(|(id, _)| id == name).map(|(_, u)| *u);
        for name in [
            "nature_awareness",
            "just_cause",
            "aura_of_majesty",
            "wind_mastery",
        ] {
            assert_eq!(of(name), Some(GlobalUse::Boon), "{name}");
        }
        for name in [
            "chaos_surge",
            "doom_mastery",
            "zombie_mastery",
            "eternal_night",
        ] {
            assert_eq!(of(name), Some(GlobalUse::Realm), "{name}");
        }
        for name in [
            "time_stop",
            "armageddon",
            "suppress_magic",
            "death_wish",
            "great_unsummoning",
        ] {
            assert_eq!(of(name), Some(GlobalUse::War), "{name}");
        }
        assert!(matches!(of("natures_wrath"), Some(GlobalUse::Against(..))));
        assert!(matches!(of("evil_omens"), Some(GlobalUse::Against(..))));
        assert_eq!(of("planar_seal"), Some(GlobalUse::Never));
    }

    #[test]
    fn a_boon_is_wanted_at_peace_a_realm_spell_by_a_wizard_of_the_realm_a_strike_at_war(
    ) -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.make_contact(P0, P1);
        assert!(wanted(&g, 34 /* nature_awareness */)?, "a boon, at peace");
        assert!(!wanted(&g, 199 /* death_wish */)?, "a strike needs a war");
        books(&mut g, P0, &[(3 /* life */, 5)]);
        assert!(
            !wanted(&g, 116 /* chaos_surge */)?,
            "a life wizard has no use for it"
        );
        books(&mut g, P0, &[(2 /* chaos */, 5)]);
        assert!(wanted(&g, 116 /* chaos_surge */)?, "a chaos wizard does");
        war(&mut g);
        assert!(wanted(&g, 199 /* death_wish */)?, "at war");
        assert!(!wanted(&g, 135 /* planar_seal */)?, "never");
        Ok(())
    }

    #[test]
    fn a_spell_against_a_realm_needs_an_enemy_with_its_books_and_a_caster_it_does_not_hurt(
    ) -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        war(&mut g);
        books(&mut g, P1, &[(3 /* life */, 5)]);
        assert!(
            !wanted(&g, 156 /* life_force */)?,
            "no death books on the other side"
        );
        assert!(
            wanted(&g, 198 /* evil_omens */)?,
            "life books on the other side"
        );
        books(&mut g, P0, &[(0 /* nature */, 3)]);
        assert!(
            !wanted(&g, 198 /* evil_omens */)?,
            "it would cost the caster's own nature spells as well"
        );
        books(&mut g, P1, &[(4 /* death */, 5)]);
        assert!(
            wanted(&g, 156 /* life_force */)?,
            "death books on the other side"
        );
        Ok(())
    }

    #[test]
    fn great_unsummoning_is_not_cast_while_the_wizard_has_creatures_of_its_own() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        war(&mut g);
        assert!(wanted(&g, 75 /* great_unsummoning */)?);
        spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            179, /* war_bears */
            1,
        );
        assert!(
            !wanted(&g, 75 /* great_unsummoning */)?,
            "it would banish the bears too"
        );
        Ok(())
    }

    #[test]
    fn a_storm_goes_to_an_enemy_stack_by_a_city_and_only_at_war() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.make_contact(P0, P1);
        let storm = spell(25 /* ice_storm */)?;
        spawn(
            &mut g,
            P1,
            at(COAST_CITY.0 + 1, COAST_CITY.1),
            104, /* high_men_spearmen */
            4,
        );
        g.reveal_map(P0);
        assert_eq!(
            strike_square(&PlayerView::new(&g, P0), storm),
            None,
            "at peace"
        );
        war(&mut g);
        assert_eq!(
            strike_square(&PlayerView::new(&g, P0), storm),
            Some(Target::Tile(at(COAST_CITY.0 + 1, COAST_CITY.1)))
        );
        Ok(())
    }

    #[test]
    fn corruption_goes_to_the_land_beside_an_enemy_city_at_war() -> TestResult {
        let mut g = game_rival_across(COAST_CITY, 8 /* high_men */);
        g.make_contact(P0, P1);
        g.reveal_map(P0);
        let corruption = spell(85 /* corruption */)?;
        assert_eq!(
            land_curse_target(&PlayerView::new(&g, P0), corruption),
            None,
            "at peace"
        );
        war(&mut g);
        let Some(Target::Tile(square)) = land_curse_target(&PlayerView::new(&g, P0), corruption)
        else {
            return Err("no square for Corruption".into());
        };
        let enemy = f4x_core::map::Position {
            plane: f4x_core::map::Plane::Myrror,
            ..at(ENEMY_CITY.0, ENEMY_CITY.1)
        };
        assert_eq!(square.plane, enemy.plane);
        assert!(
            square != enemy && square.x.abs_diff(enemy.x) <= 2 && square.y.abs_diff(enemy.y) <= 2
        );
        Ok(())
    }

    #[test]
    fn a_volcano_goes_to_a_free_square_beside_an_own_city() -> TestResult {
        let g = game(COAST_CITY, 8 /* high_men */);
        let Some(Target::Tile(square)) =
            land_boost_target(&PlayerView::new(&g, P0), spell(98 /* raise_volcano */)?)
        else {
            return Err("no square for a volcano".into());
        };
        let city = at(COAST_CITY.0, COAST_CITY.1);
        assert!(square != city && square.x.abs_diff(city.x) <= 2 && square.y.abs_diff(city.y) <= 2);
        Ok(())
    }
}
