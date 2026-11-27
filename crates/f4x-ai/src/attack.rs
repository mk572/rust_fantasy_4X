//! The attack job (task 836, `docs/ai-design.md` "Attacks"): the one place an AI player decides to
//! attack on the map, and carries the attack out. The other jobs gather their units and walk them to
//! a square beside the target; here a stack standing beside something its roles let it attack does
//! so when it is at least `attack_force` times as strong as the defence. (The roaming monsters have
//! their own, `monster.rs`.)
//!
//! It runs twice a turn (`dispatch::run`): first, while the units have all their moves, and after
//! the last job, for the units that arrived with moves left.
//!
//! Who may attack what ([`Claim`]):
//!
//! - a lair or tower: the force sent against that one (a raid party, a landing);
//! - an enemy at war and a roaming band of monsters: the free soldiers, those gathering or marching
//!   for the campaign, the perimeter's, those sent to a city and the garrisons;
//! - a city of a player not at war: the campaign force sent against it, which declares war first;
//! - units of a player not at war near an own city: the perimeter, only when the AI is open to war
//!   on that player, which it declares first.
//!
//! No war is declared on a partner of a pact or an alliance, nor in a peace period.
//!
//! The attack changes almost nothing in the registry: the units keep their roles and targets. A unit
//! that walks into an empty city leaves the ship that was to carry it (the pickup plan counts only
//! the units that stand on its boarding tile or sit aboard). A city's soldier that went out is sent
//! back at once.
//!
//! Tests: below, on shipped units.

use std::cell::OnceCell;

use f4x_core::diplomacy::{DiplomacyCommand, Treaty};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::world::serial::Serial;
use f4x_core::world::{Command, Event};

use crate::assault::assault;
use crate::lever::Levered;
use crate::military::{engage, neighbours, stacks};
use crate::roles::{MusterKind, Registry, Role};
use crate::site::{expected_vs, lost_before, unarmed_share};
use crate::statecraft::Scene;
use crate::view::{fighting_strength, party, stack_strength};
use crate::Ai;
use f4x_master::Game;

/// What lets a unit attack a square.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Claim {
    /// A lair or tower: the force sent against it.
    Site,
    /// An enemy at war or a roaming band: any soldier on war duty.
    Open,
    /// A city of a player not at war: the campaign force sent against it, war first.
    Campaign,
    /// Units of a player not at war near an own city: the perimeter, war first.
    Perimeter,
}

/// What stands on a square, as an attack sees it.
enum Stand {
    /// A lair or tower: what its guards are worth depends on who attacks.
    Site,
    /// A city with no one in it: it is walked into.
    EmptyCity,
    /// Units and what they are worth in defence, the city counted in.
    Defended(Vec<hecs::Entity>, f32),
}

/// Something beside a stack that some of its units may attack.
struct Target {
    square: Position,
    owner: PlayerId,
    claim: Claim,
    stand: Stand,
}

/// Is the owner one to be attacked with no more said: the monsters, or a player at war?
fn at_war(view: &PlayerView, owner: PlayerId) -> bool {
    owner == MONSTERS || view.treaty_with(owner) == Treaty::War
}

/// Whether the AI may declare war on `owner` to attack it: a player of diplomacy with no treaty,
/// and no peace period running.
pub(crate) fn may_declare(view: &PlayerView, owner: PlayerId) -> bool {
    view.is_diplomat(owner)
        && view.treaty_with(owner) == Treaty::None
        && !view.peace_period_with(owner)
}

/// Whether a campaign may aim at a city of `owner`: it is at war, or war may be declared on it.
pub(crate) fn may_target(view: &PlayerView, owner: PlayerId) -> bool {
    at_war(view, owner) || may_declare(view, owner)
}

/// Whether units of `owner` standing in the open are to be dealt with: the monsters and the players
/// at war always; another player's only near an own city, and only when the AI is open to war on it.
fn claim_on_units(
    ai: &Ai,
    game: &Game,
    view: &PlayerView,
    owner: PlayerId,
    near_city: impl FnOnce() -> bool,
) -> Option<Claim> {
    if at_war(view, owner) {
        return Some(Claim::Open);
    }
    if !may_declare(view, owner) || !near_city() {
        return None;
    }
    let scene = Scene::new(game, ai.player);
    crate::diplo_b_stance::open_to_war(ai, game, &scene, owner, true).then_some(Claim::Perimeter)
}

/// The units on a square in the open near an own city that the perimeter is to go for
/// (`perimeter.rs`).
pub(crate) fn threat_at(
    ai: &Ai,
    game: &Game,
    view: &PlayerView,
    square: Position,
) -> Option<Vec<hecs::Entity>> {
    if view.city_at(square).is_some() || crate::site::live(view, square) {
        return None;
    }
    let units = view.hostile_at(square).filter(|u| !u.is_empty())?;
    let owner = game.units().unit_owner(*units.first()?)?;
    claim_on_units(ai, game, view, owner, || true).map(|_| units)
}

/// What stands on `square` that somebody may attack. `near_city` says whether the square is in the
/// zone of an own city.
fn target_at(
    ai: &Ai,
    game: &Game,
    view: &PlayerView,
    square: Position,
    near_city: impl FnOnce() -> bool,
) -> Option<Target> {
    if let Some(city) = view.city_at(square) {
        let owner = view.city_owner(city).filter(|&o| o != ai.player)?;
        let claim = if at_war(view, owner) {
            Claim::Open
        } else if may_declare(view, owner) {
            Claim::Campaign
        } else {
            return None;
        };
        let defenders = view.hostile_at(square)?;
        let stand = if defenders.is_empty() {
            Stand::EmptyCity
        } else {
            // A square we see is a square we know the defence of.
            let theirs =
                fighting_strength(view, &defenders) * view.levers().shared.city_defence_factor;
            Stand::Defended(defenders, theirs)
        };
        return Some(Target {
            square,
            owner,
            claim,
            stand,
        });
    }
    if crate::site::live(view, square) {
        return Some(Target {
            square,
            owner: MONSTERS,
            claim: Claim::Site,
            stand: Stand::Site,
        });
    }
    let defenders = view.hostile_at(square).filter(|u| !u.is_empty())?;
    let owner = game.units().unit_owner(*defenders.first()?)?;
    let claim = claim_on_units(ai, game, view, owner, near_city)?;
    let theirs = fighting_strength(view, &defenders);
    Some(Target {
        square,
        owner,
        claim,
        stand: Stand::Defended(defenders, theirs),
    })
}

/// Is the unit part of the campaign force sent against `square`?
fn campaigns_against(roles: &Registry, unit: Serial, square: Position) -> bool {
    roles.target_of(unit, Role::March) == Some(square)
        || roles
            .muster_of(unit)
            .is_some_and(|(_, m)| m.kind == MusterKind::Campaign && m.objective == Some(square))
}

/// Was the unit sent against what stands on `square`, with the campaign or a raid?
fn sent_against(roles: &Registry, unit: Serial, square: Position) -> bool {
    roles.target_of(unit, Role::Raid) == Some(square) || campaigns_against(roles, unit, square)
}

/// Do the unit's roles let it attack the target?
fn may_attack(roles: &Registry, unit: Serial, target: &Target) -> bool {
    match target.claim {
        Claim::Site => sent_against(roles, unit, target.square),
        Claim::Campaign => campaigns_against(roles, unit, target.square),
        Claim::Open => {
            roles.is_free(unit)
                || roles.in_muster(unit, MusterKind::Campaign)
                || [
                    Role::March,
                    Role::Perimeter,
                    Role::Reinforce,
                    Role::Garrison,
                ]
                .into_iter()
                .any(|r| roles.is(unit, r))
        }
        Claim::Perimeter => roles.is(unit, Role::Perimeter),
    }
}

/// Whether the garrison of the city at `pos` may go out against the stack on `target`: no other
/// enemy is near the city that the soldiers who stay could not meet.
fn city_safe_without(view: &PlayerView, pos: Position, target: Position) -> bool {
    view.enemies_near(pos, view.levers().shared.near_city_radius)
        .into_iter()
        .all(|(square, _)| square == target)
}

/// One round of attacks: every stack with something beside it that it is strong enough for.
pub(crate) fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let own_cities: Vec<Position> = PlayerView::new(game, me)
        .my_cities()
        .into_iter()
        .map(|(_, p)| p)
        .collect();
    // The squares near the own cities, worked out only if a player not at war stands by.
    let zone: OnceCell<Vec<Position>> = OnceCell::new();
    let fighters = crate::dispatch::soldiers(ai, game, |roles, s| {
        roles.is_free(s)
            || roles.in_muster(s, MusterKind::Campaign)
            || [
                Role::Raid,
                Role::March,
                Role::Perimeter,
                Role::Reinforce,
                Role::Garrison,
            ]
            .into_iter()
            .any(|r| roles.is(s, r))
    });
    let stack_list = stacks(&PlayerView::new(game, me), fighters);
    for (pos, units) in stack_list {
        if game.game_over().is_some() {
            return;
        }
        // The targets beside the stack with the units that may attack each, those the units were
        // sent against first.
        let mut plans: Vec<(bool, Target, Vec<hecs::Entity>)> = {
            let view = PlayerView::new(game, me);
            neighbours(&view, pos)
                .into_iter()
                .filter_map(|square| {
                    let near_city = || {
                        zone.get_or_init(|| {
                            own_cities
                                .iter()
                                .flat_map(|&c| crate::perimeter::zone(&view, c))
                                .collect()
                        })
                        .contains(&square)
                    };
                    let target = target_at(ai, game, &view, square, near_city)?;
                    let allowed: Vec<hecs::Entity> = units
                        .iter()
                        .copied()
                        .filter(|&u| may_attack(&ai.roles, view.serial_of(u), &target))
                        .collect();
                    if allowed.is_empty() {
                        return None;
                    }
                    let passing = !allowed
                        .iter()
                        .any(|&u| sent_against(&ai.roles, view.serial_of(u), square));
                    Some((passing, target, allowed))
                })
                .collect()
        };
        plans.sort_by_key(|&(passing, ..)| passing);
        let at_city = own_cities.contains(&pos);
        for (_, target, allowed) in plans {
            let done = match &target.stand {
                Stand::Site => assault_site(ai, game, &allowed, target.square, events),
                Stand::EmptyCity => walk_in(ai, game, &allowed, &target, events),
                Stand::Defended(defenders, theirs) => {
                    let force = Force {
                        from: pos,
                        at_city,
                        units: &allowed,
                    };
                    fight(ai, game, force, &target, (defenders, *theirs), events)
                }
            };
            // A stack makes one attack in a round.
            if done {
                break;
            }
        }
    }
}

/// Declare war on `owner` where that is still to be done. Whether the two are at war now.
fn go_to_war(ai: &mut Ai, game: &mut Game, owner: PlayerId, events: &mut Vec<Event>) -> bool {
    let me = ai.player;
    if owner == MONSTERS || game.treaty(me, owner) == Treaty::War {
        return true;
    }
    let declare = Command::Diplomacy(DiplomacyCommand::DeclareWar {
        player: me,
        on: owner,
    });
    if ai.send(game, declare, events).is_ok() {
        ai.memory.put().enemy = Some((owner, game.round()));
    }
    game.treaty(me, owner) == Treaty::War
}

/// The force sent against a lair or tower goes in, when it is enough: the best nine of it, with the
/// wizard's spells. A force with a unit that has no moves left waits for it.
fn assault_site(
    ai: &mut Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    site: Position,
    events: &mut Vec<Event>,
) -> bool {
    let me = ai.player;
    let (band, serials, strength) = {
        let view = PlayerView::new(game, me);
        if units.iter().any(|&u| view.unit_moves_left(u) <= 0) {
            return false;
        }
        let band = party(&view, units);
        let strength = stack_strength(&view, &band) + crate::spellhelper::spell_power(&view);
        let needed = expected_vs(&view, site, unarmed_share(&view, &band))
            * view.levers().attack.attack_force;
        let retry = view.levers().sites.retry_edge;
        if strength < needed || lost_before(&ai.site_losses, site, strength, retry) {
            return false;
        }
        let serials: Vec<Serial> = units.iter().map(|&u| view.serial_of(u)).collect();
        (band, serials, strength)
    };
    let before = events.len();
    assault(ai, game, &band, site, events);
    let lost = events[before..]
        .iter()
        .any(|e| matches!(e, Event::BattleEnded { loser, .. } if *loser == me));
    let cleared = events[before..]
        .iter()
        .any(|e| matches!(e, Event::SiteCleared { player, .. } if *player == me));
    // A force that lost the assault (no site cleared, the battle won by the guards) does not go
    // again until it is much stronger.
    if lost && !cleared {
        ai.site_losses.retain(|&(p, _)| p != site);
        ai.site_losses.push((site, strength));
    }
    // A cleared site is the party's work done: it has not stalled, so its time limit starts again
    // (`plan_clock.rs`).
    if cleared {
        for serial in serials {
            ai.roles.restart_plan(serial);
        }
    }
    true
}

/// Walk into a city with nobody in it, with the units that can still move. A garrison stays home.
fn walk_in(
    ai: &mut Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    target: &Target,
    events: &mut Vec<Event>,
) -> bool {
    let me = ai.player;
    let walkers: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        units
            .iter()
            .copied()
            .filter(|&u| {
                view.unit_moves_left(u) > 0 && !ai.roles.is(view.serial_of(u), Role::Garrison)
            })
            .collect()
    };
    if walkers.is_empty() || !go_to_war(ai, game, target.owner, events) {
        return false;
    }
    let serials: Vec<Serial> = {
        let view = PlayerView::new(game, me);
        walkers.iter().map(|&u| view.serial_of(u)).collect()
    };
    let walk = Command::MoveStack {
        units: walkers,
        to: target.square,
    };
    if ai.send(game, walk, events).is_err() {
        return false;
    }
    // The units are off the ship, or off its boarding tile: its pickup plan must not count them
    // any more.
    for serial in serials {
        ai.roles.unboard(serial);
    }
    true
}

/// The units of one square that may attack a target.
struct Force<'a> {
    from: Position,
    /// Whether they stand in an own city.
    at_city: bool,
    units: &'a [hecs::Entity],
}

/// Fight the defenders, when the best nine of the force are enough.
fn fight(
    ai: &mut Ai,
    game: &mut Game,
    force: Force,
    target: &Target,
    (defenders, theirs): (&[hecs::Entity], f32),
    events: &mut Vec<Event>,
) -> bool {
    let me = ai.player;
    // A city's garrison goes out only when the city is safe without it.
    let (band, lent) = {
        let view = PlayerView::new(game, me);
        let lend = force.at_city && city_safe_without(&view, force.from, target.square);
        let fit: Vec<hecs::Entity> = force
            .units
            .iter()
            .copied()
            .filter(|&u| {
                view.unit_moves_left(u) > 0
                    && (lend || !ai.roles.is(view.serial_of(u), Role::Garrison))
            })
            .collect();
        let band = party(&view, &fit);
        let needed = theirs * view.levers().attack.attack_force;
        if band.is_empty() || stack_strength(&view, &band) < needed {
            return false;
        }
        let lent: Vec<hecs::Entity> = band
            .iter()
            .copied()
            .filter(|&u| ai.roles.is(view.serial_of(u), Role::Garrison))
            .collect();
        (band, lent)
    };
    if !go_to_war(ai, game, target.owner, events) {
        return false;
    }
    engage(ai, game, band, defenders.to_vec(), events);
    // The winners stand on the enemy's square: a city's soldier goes home.
    let out: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        lent.into_iter()
            .filter(|&u| view.unit_position(u).is_some_and(|p| p != force.from))
            .collect()
    };
    if !out.is_empty() {
        let home = Command::GoTo {
            units: out,
            to: force.from,
        };
        ai.order(game, home, events);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, spawn_site, COAST_CITY, ENEMY_CITY, P0, P1};
    use f4x_core::diplomacy::DiplomacyEvent;
    use f4x_core::map::Plane;
    use f4x_core::player::race_player::race_player_id;
    use f4x_def::half_int::HalfInt;
    use f4x_def::rules::UnitId;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The independent player whose city these tests found.
    fn independent() -> PlayerId {
        race_player_id(f4x_def::rules::RaceId::new(8 /* high_men */))
    }

    fn zombies(n: u32) -> Vec<(UnitId, u32)> {
        vec![(UnitId::new(174) /* zombies */, n)]
    }

    fn strike(w: &mut Game, ai: &mut Ai) -> Vec<Event> {
        let mut events = Vec::new();
        run(ai, w, &mut events);
        events
    }

    /// Give the units a role with a target.
    fn tag(ai: &mut Ai, w: &Game, units: &[hecs::Entity], role: Role, to: Position) {
        let view = PlayerView::new(w, ai.player);
        for &u in units {
            assert!(ai
                .roles
                .claim(view.serial_of(u), role, w.round(), Some(to))
                .is_ok());
        }
    }

    fn whole(w: &Game, units: &[hecs::Entity]) -> bool {
        units.iter().all(|&u| w.units().unit_owner(u).is_some())
    }

    /// West's six swordsmen stand next to one of East's spearmen, away from every city.
    fn field(war: bool) -> (Game, Vec<hecs::Entity>, Vec<hecs::Entity>) {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let ours = spawn(&mut w, P0, at(6, 3), 105 /* high_men_swordsmen */, 6);
        let theirs = spawn(&mut w, P1, at(7, 3), 104 /* high_men_spearmen */, 1);
        w.make_contact(P0, P1);
        if war {
            w.declare_war(P0, P1);
        }
        (w, ours, theirs)
    }

    /// A stack of a square's size attacks with the nine a battle holds and the rest stay on the
    /// square, available.
    #[test]
    fn a_stack_attacks_with_a_battle_party_and_leaves_the_rest_on_its_square() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let (side, tile) = (w.max_battle_side(), w.max_tile());
        let stack = spawn(
            &mut w,
            P0,
            at(6, 3),
            105, /* high_men_swordsmen */
            tile,
        );
        spawn(&mut w, P1, at(7, 3), 104 /* high_men_spearmen */, 1);
        w.make_contact(P0, P1);
        w.declare_war(P0, P1);
        // The attack waits for the human, so that it can be looked at.
        w.set_human(Some(P1));
        strike(&mut w, &mut Ai::new(P0));
        let attack = w.waiting_attack().ok_or("an attack")?;
        assert_eq!(attack.attackers.len(), side);
        assert!(side < stack.len());
        let view = PlayerView::new(&w, P0);
        let left: Vec<_> = stack
            .iter()
            .filter(|u| !attack.attackers.contains(u))
            .collect();
        assert_eq!(left.len(), tile - side);
        assert!(left
            .iter()
            .all(|&&u| view.unit_position(u) == Some(at(6, 3))));
        Ok(())
    }

    #[test]
    fn an_attack_on_the_human_waits_for_its_answer() -> TestResult {
        let (mut w, ours, theirs) = field(true);
        w.set_human(Some(P1));
        let events = strike(&mut w, &mut Ai::new(P0));
        let attack = w.waiting_attack().ok_or("an attack")?;
        assert_eq!(attack.defenders, theirs, "{events:?}");
        assert_eq!(attack.attackers.len(), ours.len());
        assert!(whole(&w, &theirs), "nothing was fought");
        Ok(())
    }

    #[test]
    fn an_attack_on_a_computer_player_is_fought_at_once() {
        let (mut w, _, theirs) = field(true);
        w.set_human(None);
        let events = strike(&mut w, &mut Ai::new(P0));
        assert_eq!(w.waiting_attack(), None);
        assert!(!whole(&w, &theirs), "the spearman fell: {events:?}");
    }

    /// The same stack beside a wizard it is at peace with, away from every own city, leaves it
    /// alone.
    #[test]
    fn units_of_a_wizard_not_at_war_are_left_alone_in_the_open() {
        let (mut w, _, theirs) = field(false);
        strike(&mut w, &mut Ai::new(P0));
        assert_eq!(w.waiting_attack(), None);
        assert!(whole(&w, &theirs));
        assert_eq!(w.treaty(P0, P1), Treaty::None);
    }

    /// The force marching on a city attacks it from beside it: the war target is the army's.
    #[test]
    fn a_march_beside_its_target_attacks_it() {
        let (mut w, ours, theirs) = field(true);
        let mut ai = Ai::new(P0);
        let target = at(7, 3);
        ai.campaign = Some(target);
        tag(&mut ai, &w, &ours, Role::March, target);
        strike(&mut w, &mut ai);
        assert!(!whole(&w, &theirs));
    }

    /// A force that is not `attack_force` times as strong as the defence waits beside it.
    #[test]
    fn a_force_that_is_too_weak_waits() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let ours = spawn(&mut w, P0, at(6, 3), 104 /* high_men_spearmen */, 1);
        let theirs = spawn(&mut w, P1, at(7, 3), 104 /* high_men_spearmen */, 1);
        w.make_contact(P0, P1);
        w.declare_war(P0, P1);
        assert!(
            w.levers().attack.attack_force > 1.0,
            "an even match is no attack"
        );
        strike(&mut w, &mut Ai::new(P0));
        assert!(whole(&w, &ours) && whole(&w, &theirs));
    }

    /// A roaming band of monsters is struck; the guards of a lair are not struck in passing.
    #[test]
    fn a_roaming_band_is_struck_and_a_lair_is_not_attacked_in_passing() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, at(6, 3), 105 /* high_men_swordsmen */, 6);
        let band = spawn(
            &mut w,
            MONSTERS,
            at(7, 3),
            104, /* high_men_spearmen */
            1,
        );
        let passing = spawn(&mut w, P0, at(6, 8), 105 /* high_men_swordsmen */, 6);
        spawn_site(&mut w, at(7, 8), zombies(1), Vec::new())?;
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        // A raid party sent against another site passes this one.
        tag(&mut ai, &w, &passing, Role::Raid, at(3, 3));
        let events = strike(&mut w, &mut ai);
        assert!(!whole(&w, &band), "the band fell: {events:?}");
        assert!(w.economy().lair_at(at(7, 8)).is_some(), "the lair stands");
        Ok(())
    }

    /// The raid party sent against a site clears it from beside it.
    #[test]
    fn the_party_sent_against_a_site_clears_it_and_gets_a_fresh_time() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let party = spawn(&mut w, P0, at(6, 3), 113 /* high_men_paladins */, 5);
        let site = at(7, 3);
        spawn_site(&mut w, site, zombies(1), Vec::new())?;
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        tag(&mut ai, &w, &party, Role::Raid, site);
        // The party has been out for 90 turns when it clears the site.
        ai.drop_stale_plans(&w.levers().planning, w.round());
        w.set_round(w.round() + 90);
        ai.drop_stale_plans(&w.levers().planning, w.round());
        let events = strike(&mut w, &mut ai);
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::SiteCleared { player, .. } if *player == P0)));
        // Twenty turns later it would be 110 turns out, but the clear started the time again.
        w.set_round(w.round() + 20);
        ai.drop_stale_plans(&w.levers().planning, w.round());
        w.set_round(w.round() + 20);
        ai.drop_stale_plans(&w.levers().planning, w.round());
        assert!(ai.roles.any(Role::Raid), "not dropped");
        assert_eq!(ai.plans_dropped().iter().map(|(_, n)| n).sum::<u32>(), 0);
        Ok(())
    }

    /// A raid party with a unit that has no moves left waits, and goes in the next turn with its
    /// moves whole.
    #[test]
    fn a_party_with_a_tired_unit_waits_before_its_site() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let party = spawn(&mut w, P0, at(6, 3), 113 /* high_men_paladins */, 3);
        let site = at(7, 3);
        spawn_site(&mut w, site, zombies(1), Vec::new())?;
        w.reveal_map(P0);
        w.scenario_set_moves_left(party[0], HalfInt::ZERO);
        let mut ai = Ai::new(P0);
        ai.record_refusals(true);
        tag(&mut ai, &w, &party, Role::Raid, site);
        strike(&mut w, &mut ai);
        assert!(w.economy().lair_at(site).is_some(), "it waits");
        assert!(ai.take_refusals().is_empty(), "no refused order");
        let moves = w.units().unit_max_moves(party[0]);
        w.scenario_set_moves_left(party[0], moves);
        strike(&mut w, &mut ai);
        assert!(w.economy().lair_at(site).is_none(), "cleared");
        Ok(())
    }

    /// A party that loses its assault is remembered with its strength, and does not go again.
    #[test]
    fn a_lost_assault_is_remembered_and_not_repeated() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let party = spawn(&mut w, P0, at(6, 3), 105 /* high_men_swordsmen */, 9);
        let site = at(7, 3);
        // Guards it has never seen, far stronger than what it guesses.
        spawn_site(
            &mut w,
            site,
            vec![(UnitId::new(164) /* great_drake */, 3)],
            Vec::new(),
        )?;
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        tag(&mut ai, &w, &party, Role::Raid, site);
        strike(&mut w, &mut ai);
        assert_eq!(ai.site_losses.len(), 1, "{:?}", ai.site_losses);
        assert_eq!(ai.site_losses[0].0, site);
        Ok(())
    }

    /// A game with an empty independent city at `pos`, its owner met by P0.
    fn independent_city(pos: Position) -> Game {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.add_race_players();
        let owner = independent();
        w.command(Command::FoundCity {
            owner,
            pos,
            name: "Greyhold".into(),
            race: w.content().rules.races.id_numbered(8),
            population: 8,
        });
        w.make_contact(P0, owner);
        w.explore(P0, pos, HalfInt::from_raw(3));
        w
    }

    /// No raid party and no passing stack attacks the city of a player not at war.
    #[test]
    fn a_city_of_a_player_not_at_war_is_not_attacked_by_a_raid_or_in_passing() {
        let town = Position {
            plane: Plane::Arcanus,
            x: 33,
            y: 6,
        };
        let mut w = independent_city(town);
        let raiders = spawn(&mut w, P0, at(32, 6), 113 /* high_men_paladins */, 3);
        spawn(&mut w, P0, at(32, 7), 113 /* high_men_paladins */, 3);
        let mut ai = Ai::new(P0);
        tag(&mut ai, &w, &raiders, Role::Raid, town);
        let events = strike(&mut w, &mut ai);
        assert_eq!(w.treaty(P0, independent()), Treaty::None, "{events:?}");
        let view = PlayerView::new(&w, P0);
        assert!(view.my_cities().into_iter().all(|(_, p)| p != town));
    }

    /// The campaign force declares war on its target's owner before it takes the city.
    #[test]
    fn the_campaign_declares_war_before_it_attacks_an_independent_city() {
        let town = at(33, 6);
        let mut w = independent_city(town);
        let army = spawn(&mut w, P0, at(32, 6), 113 /* high_men_paladins */, 3);
        let mut ai = Ai::new(P0);
        tag(&mut ai, &w, &army, Role::March, town);
        let events = strike(&mut w, &mut ai);
        // The independent loses its last city, and with it the war: the events tell both.
        let position = |wanted: fn(&Event) -> bool| events.iter().position(wanted);
        let declared = position(|e| {
            matches!(
                e,
                Event::Diplomacy(DiplomacyEvent::WarDeclared { attacker, .. }) if *attacker == P0
            )
        });
        let taken = position(|e| matches!(e, Event::CityCaptured { to, .. } if *to == P0));
        assert!(
            declared.is_some() && taken.is_some() && declared < taken,
            "{events:?}"
        );
    }

    /// No war for a campaign against a partner, or in a peace period: the force waits.
    #[test]
    fn a_treaty_or_a_peace_period_stops_the_campaigns_attack() {
        for treaty in [Some(Treaty::Pact), Some(Treaty::Alliance), None] {
            let mut w = game(COAST_CITY, 8 /* high_men */);
            let town = at(ENEMY_CITY.0, ENEMY_CITY.1);
            let beside = at(ENEMY_CITY.0 - 1, ENEMY_CITY.1);
            let army = spawn(&mut w, P0, beside, 113 /* high_men_paladins */, 3);
            w.make_contact(P0, P1);
            match treaty {
                Some(t) => w.start_treaty(P0, P1, t),
                None => {
                    w.declare_war(P0, P1);
                    w.declare_peace(P0, P1);
                    assert!(PlayerView::new(&w, P0).peace_period_with(P1));
                }
            }
            let before = w.treaty(P0, P1);
            let mut ai = Ai::new(P0);
            tag(&mut ai, &w, &army, Role::March, town);
            let events = strike(&mut w, &mut ai);
            assert!(events.is_empty(), "{treaty:?}: {events:?}");
            assert_eq!(w.treaty(P0, P1), before, "{treaty:?}");
            assert!(!may_target(&PlayerView::new(&w, P0), P1), "{treaty:?}");
        }
    }

    /// West holds a large army and East one spearman that stands in the zone of West's city, with
    /// West's perimeter beside it; no treaty.
    fn intruder(turn: u32) -> (Game, Ai, Vec<hecs::Entity>) {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, at(5, 8), 113 /* high_men_paladins */, 9);
        let guard = spawn(&mut w, P0, at(7, 6), 113 /* high_men_paladins */, 3);
        let theirs = spawn(&mut w, P1, at(8, 6), 104 /* high_men_spearmen */, 1);
        w.make_contact(P0, P1);
        w.set_round(turn);
        let mut ai = Ai::new(P0);
        tag(&mut ai, &w, &guard, Role::Perimeter, at(8, 6));
        (w, ai, theirs)
    }

    /// Units that come near a city are struck, after a declaration of war, only when the AI is
    /// open to war on their owner: not in the opening turns, and not in a peace period.
    #[test]
    fn an_intruder_not_at_war_is_struck_only_when_the_ai_is_open_to_war_on_its_owner() {
        let late = 100;
        let (mut w, mut ai, theirs) = intruder(late);
        let scene = Scene::new(&w, P0);
        assert!(
            crate::diplo_b_stance::open_to_war(&ai, &w, &scene, P1, true),
            "a weak neighbour is prey"
        );
        let events = strike(&mut w, &mut ai);
        assert_eq!(w.treaty(P0, P1), Treaty::War, "{events:?}");
        assert!(!whole(&w, &theirs), "{events:?}");

        let early = w.levers().diplomacy_b.from_turn - 1;
        let (mut w, mut ai, theirs) = intruder(early);
        strike(&mut w, &mut ai);
        assert!(whole(&w, &theirs) && w.treaty(P0, P1) == Treaty::None);

        let (mut w, mut ai, theirs) = intruder(late);
        w.declare_war(P0, P1);
        w.declare_peace(P0, P1);
        strike(&mut w, &mut ai);
        assert!(whole(&w, &theirs) && w.treaty(P0, P1) != Treaty::War);
    }

    /// A unit booked on a ship that walks into an empty enemy city is off the ship's passenger
    /// list: the pickup plan does not wait for it or walk it back.
    #[test]
    fn a_unit_that_walks_into_an_empty_city_leaves_the_ships_passenger_list() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let beside = at(ENEMY_CITY.0 - 1, ENEMY_CITY.1);
        let soldier = spawn(&mut w, P0, beside, 105 /* high_men_swordsmen */, 1);
        w.make_contact(P0, P1);
        w.declare_war(P0, P1);
        let mut ai = Ai::new(P0);
        tag(&mut ai, &w, &soldier, Role::Reinforce, beside);
        let serial = PlayerView::new(&w, P0).serial_of(soldier[0]);
        ai.roles.board_at(serial, serial, beside);
        assert!(ai.roles.boarding_of(serial).is_some());
        strike(&mut w, &mut ai);
        assert_eq!(
            w.units().unit_position(soldier[0]),
            Some(at(ENEMY_CITY.0, ENEMY_CITY.1)),
            "it walked into the city"
        );
        assert_eq!(ai.roles.boarding_of(serial), None, "off the passenger list");
    }
}
