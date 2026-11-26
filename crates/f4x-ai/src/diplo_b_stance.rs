//! Diplomacy system B, part 2 (task 763, `docs/ai-diplomacy-b.md`): who to fight and who to befriend,
//! from the three values of `diplo_b::Regard`. Every few turns each other wizard gets an **enemy value** and
//! a **friend value**; at most one is the **main enemy** (kept for a while, so the choice does not flip each
//! turn) and the only target of a declaration of war; the rest are courted by their friend value: a pact,
//! an alliance, a gift. It takes the place, for a wizard of B, of the dominant war, the appeasing, the
//! coalition and the war by temper of system A.
//!
//! * **Enemy value**: a weak, reachable neighbour is prey (the more so the weaker); hostility (a negative
//!   friendship); the leader and the Mastery caster; the one already fought; and, when it fears the other
//!   and is not the weaker, the wish to strike first. A wizard much stronger is not a target, and a treaty
//!   partner is spared (a pact more, an alliance most).
//! * **Friend value**: the friendship; a shared enemy; the fear of a stronger neighbour (to grovel to it);
//!   the weaker wizards leaguing against the leader.
//!
//! Tests: below.

use f4x_core::diplomacy::{DiplomacyCommand, ProposalKind, Remark, Treaty};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::world::{Command, Event, World};

use crate::diplo_b::{regard, Knobs};
use crate::memory::Memory;
use crate::statecraft::{can_attack, leader_of, propose, remark, Power, Scene};
use crate::Ai;
use f4x_master::Game;

/// What `me` makes of `other` (task 763).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Values {
    pub enemy: i32,
    pub friend: i32,
}

/// The enemy and the friend value of `other` for `me`. `reachable` is whether an army can get there.
pub(crate) fn values(
    world: &World,
    me: PlayerId,
    other: PlayerId,
    scene: &Scene,
    memory: &Memory,
    reachable: bool,
    round: u32,
) -> Values {
    let k = Knobs::current();
    let sk = crate::statecraft::Knobs::current();
    let r = regard(world, me, other, scene, memory, round);
    let (mine, theirs): (Power, Power) = (scene.power(world, me), scene.power(world, other));
    let (m, t) = (mine.score(&sk).max(1), theirs.score(&sk));
    let leader = leader_of(world, me);
    let caster = PlayerView::new(world, me)
        .rival_mastery_casters()
        .iter()
        .any(|&(p, _)| p == other);
    let treaty = world.treaty(me, other);

    let mut enemy = 0;
    // Prey: weaker than we are and within reach; the weaker, the better.
    if reachable && t < m {
        enemy += (m - t) * k.prey_value / m;
    }
    enemy += (-r.friendship).max(0) * k.hostile_pct / 100;
    if leader == Some(other) {
        enemy += k.leader_enemy;
    }
    if caster {
        enemy += k.mastery_enemy;
    }
    if treaty == Treaty::War {
        enemy += k.at_war;
    }
    // An ally of mine fights it: a friend's war is joined.
    if world.player_ids().into_iter().any(|a| {
        a != me
            && a != other
            && world.treaty(me, a) == Treaty::Alliance
            && world.treaty(a, other) == Treaty::War
    }) {
        enemy += k.ally_war;
    }
    // Afraid of an equal or a weaker one: strike before it does.
    if t * 100 <= m * 110 && r.fear >= k.strike_fear {
        enemy += r.fear / 2;
    }
    // Not the one that is much stronger.
    if t * 100 > m * k.too_strong_pct {
        enemy -= (t - m) * 100 / t;
    }
    enemy -= match treaty {
        Treaty::Pact => 40,
        Treaty::Alliance => 80,
        _ => 0,
    };
    if !reachable {
        enemy = enemy.min(k.enemy_min - 1);
    }

    let mut friend = r.friendship * k.stance_friend_pct / 100;
    // A shared enemy: it fights someone I fight (or my chosen enemy).
    let wars: Vec<PlayerId> = scene
        .others
        .iter()
        .copied()
        .filter(|&p| p != other)
        .filter(|&p| {
            world.treaty(me, p) == Treaty::War || memory.enemy.is_some_and(|(e, _)| e == p)
        })
        .collect();
    if wars.iter().any(|&p| world.treaty(other, p) == Treaty::War) {
        friend += k.common_enemy;
    }
    // Grovelling: the stronger neighbour that is near is made friends with.
    if t > m {
        friend += r.fear * k.grovel_pct / 100;
    }
    // The league against the leader.
    if leader.is_some_and(|l| l != other && l != me) {
        friend += k.league;
    }
    // The leader is not courted by those it threatens, and a chosen enemy is not courted.
    if leader == Some(other) || memory.enemy.is_some_and(|(e, _)| e == other) {
        friend -= k.league;
    }
    Values { enemy, friend }
}

/// The wizard `me` keeps or chooses as its enemy: kept while it lives, is still worth fighting and has been
/// held for `hold` turns; otherwise the best of those worth `enemy_min`.
fn choose_enemy(
    world: &World,
    me: PlayerId,
    scene: &Scene,
    memory: &Memory,
    reach: &dyn Fn(PlayerId) -> bool,
    round: u32,
) -> Option<(PlayerId, u32)> {
    let k = Knobs::current();
    let value = |p| values(world, me, p, scene, memory, reach(p), round).enemy;
    // A pact or alliance partner is no enemy: the AI breaks no treaty to attack (the game refuses it), and
    // a treaty is ended by the talk (`diplo_b_talk`) before a war is chosen (task 776).
    let free = |p: PlayerId| !world.bound_by_treaty(me, p);
    if let Some((e, since)) = memory.enemy {
        let alive = scene.others.contains(&e) && free(e);
        if alive && (round < since + k.hold || value(e) >= k.keep) {
            return Some((e, since));
        }
    }
    scene
        .others
        .iter()
        .copied()
        .filter(|&p| free(p))
        .map(|p| (value(p), p))
        .filter(|&(v, _)| v >= k.enemy_min)
        .max_by_key(|&(v, p)| (v, std::cmp::Reverse(p.0)))
        .map(|(_, p)| (p, round))
}

/// Once a turn, in place of A's dominant war, appeasing and coalition (`statecraft::run`): look at the
/// others now and then, choose the enemy, declare the war, court the friends.
pub(crate) fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>, scene: &Scene) {
    let me = ai.player;
    let k = Knobs::current();
    if game.round() < ai.memory.get().reviewed + k.review {
        return;
    }
    ai.memory.put().reviewed = game.round();
    // Which of the others an army could reach, asked once.
    let reach: Vec<(PlayerId, bool)> = scene
        .others
        .iter()
        .map(|&p| (p, can_attack(game, me, scene, p)))
        .collect();
    let reaches = |p: PlayerId| reach.iter().any(|&(q, r)| q == p && r);
    let chosen = {
        let memory = ai.memory.get();
        choose_enemy(game, me, scene, &memory, &reaches, game.round())
    };
    ai.memory.put().enemy = chosen;
    if let Some((enemy, _)) = chosen {
        declare(ai, game, events, scene, enemy, reaches(enemy));
    }
    for &w in &scene.others {
        if Some(w) == chosen.map(|(e, _)| e) || game.treaty(me, w) == Treaty::War {
            continue;
        }
        if game.human() != Some(w) {
            court(ai, game, events, w, reaches(w));
        }
    }
}

/// War on `enemy` now, without the choice of `run` (the friendship has fallen too far).
pub(crate) fn declare_on(ai: &Ai, game: &mut Game, events: &mut Vec<Event>, enemy: PlayerId) {
    let me = ai.player;
    if ai
        .send(
            game,
            Command::Diplomacy(DiplomacyCommand::DeclareWar {
                player: me,
                on: enemy,
            }),
            events,
        )
        .is_ok()
    {
        remark(ai, game, events, enemy, Remark::DominantWar);
    }
}

/// War on the chosen enemy, when it is time: the game is past the opening, there is no treaty, no war
/// has just ended between the two, the value is high enough and no war is on that the army could not
/// spare (a front at a time, but for a straggler).
fn declare(
    ai: &mut Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    scene: &Scene,
    enemy: PlayerId,
    reachable: bool,
) {
    let me = ai.player;
    let k = Knobs::current();
    if game.round() < k.from_turn
        || game.treaty(me, enemy) != Treaty::None
        || game
            .diplomacy
            .stance(me, enemy)
            .is_some_and(|s| s.peace_duration > 0)
    {
        return;
    }
    let sk = crate::statecraft::Knobs::current();
    let busy = scene.others.iter().any(|&p| {
        game.treaty(me, p) == Treaty::War
            && scene.power(game, me).army < k.busy_army_x * scene.power(game, p).army.max(1)
    });
    let wars = scene
        .others
        .iter()
        .filter(|&&p| game.treaty(me, p) == Treaty::War)
        .count() as i32;
    let v = values(
        game,
        me,
        enemy,
        scene,
        &ai.memory.get(),
        reachable,
        game.round(),
    )
    .enemy
        - wars * k.war_damp;
    let _ = sk;
    if busy || !reachable || v < k.declare {
        return;
    }
    if ai
        .send(
            game,
            Command::Diplomacy(DiplomacyCommand::DeclareWar {
                player: me,
                on: enemy,
            }),
            events,
        )
        .is_ok()
    {
        remark(ai, game, events, enemy, Remark::DominantWar);
    }
}

/// Court a wizard that is no enemy by its friend value: a pact, then an alliance, or a gift where the
/// relation is too low for a pact; once in `court_cool` turns.
pub(crate) fn court(
    ai: &Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    other: PlayerId,
    reachable: bool,
) {
    let me = ai.player;
    let k = Knobs::current();
    let (friend, due) = {
        let memory = ai.memory.get();
        let scene = Scene::new(game, me);
        let due = memory
            .with(other)
            .is_none_or(|d| d.courted == 0 || game.round() >= d.courted + k.court_cool);
        (
            values(game, me, other, &scene, &memory, reachable, game.round()).friend,
            due,
        )
    };
    if !due || friend < k.gift_min {
        return;
    }
    let relation = ai.memory.get().friendship(game, me, other);
    let rules = &game.content().rules.config.diplomacy;
    let (pact_floor, ally_floor) = (rules.pact_relation, rules.alliance_relation);
    let kind = match game.treaty(me, other) {
        Treaty::None if friend >= k.pact_min && relation > pact_floor => Some(ProposalKind::Pact),
        Treaty::Pact if friend >= k.ally_min && relation > ally_floor => {
            Some(ProposalKind::Alliance)
        }
        _ => None,
    };
    let done = match kind {
        Some(kind) => propose(ai, game, events, other, kind),
        // Too low a relation for a pact: a gift wins it over.
        None if game.treaty(me, other) == Treaty::None && relation <= pact_floor => {
            crate::statecraft::gift_gold(ai, game, events, other, Remark::Appeasing)
        }
        None => false,
    };
    if done {
        ai.memory.put().courted(other, game.round());
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0, P1};
    use f4x_core::rules::RealmId;
    use f4x_def::half_int::HalfInt;
    use f4x_master::Game;

    pub(crate) const RIVAL: (u32, u32) = (3, 8);

    fn found(game: &mut Game, owner: PlayerId, x: u32, y: u32) {
        let race = game.content().rules.races.id_numbered(8);
        game.apply(Command::FoundCity {
            owner,
            pos: at(x, y),
            name: format!("City{x}x{y}"),
            race,
            population: 5,
        });
    }

    /// P0 and P1 have met, no treaty, armies of these sizes, a city each on the west island (P1's
    /// at `RIVAL`) and one more for P0; turn `turn`.
    pub(crate) fn pair(army0: usize, army1: usize, turn: u32) -> Game {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        found(&mut w, P0, 10, 10);
        found(&mut w, P1, RIVAL.0, RIVAL.1);
        for p in [P0, P1] {
            w.init_wizard(p, &[(RealmId::new(3 /* life */), 3)], &[], None)
                .expect("a wizard");
        }
        spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            95, /* high_elf_spearmen */
            army0,
        );
        spawn(&mut w, P1, at(RIVAL.0, RIVAL.1), 95, army1);
        w.make_contact(P0, P1);
        w.explore(P0, at(RIVAL.0, RIVAL.1), HalfInt::from_raw(3));
        w.explore(P1, at(COAST_CITY.0, COAST_CITY.1), HalfInt::from_raw(3));
        w.turn = turn;
        w
    }

    fn value(w: &World, me: PlayerId, other: PlayerId, reach: bool) -> Values {
        let scene = Scene::new(w, me);
        values(w, me, other, &scene, &Memory::default(), reach, w.turn)
    }

    #[test]
    fn a_pact_or_alliance_partner_is_never_chosen_as_the_enemy() {
        let mut w = pair(30, 1, 100);
        let enemy = |w: &World, memory: &Memory| {
            let scene = Scene::new(w, P0);
            choose_enemy(w, P0, &scene, memory, &|_| true, w.turn).map(|(p, _)| p)
        };
        assert_eq!(
            enemy(&w, &Memory::default()),
            Some(P1),
            "prey with no treaty"
        );
        for treaty in [Treaty::Pact, Treaty::Alliance] {
            w.start_treaty(P0, P1, treaty);
            assert_eq!(enemy(&w, &Memory::default()), None, "{treaty:?}");
            let mut kept = Memory::default();
            kept.enemy = Some((P1, w.turn));
            assert_eq!(
                enemy(&w, &kept),
                None,
                "an enemy of before, now a {treaty:?} partner"
            );
        }
    }

    #[test]
    fn a_weak_neighbour_in_reach_is_prey_and_a_strong_one_is_not_a_target() {
        let w = pair(30, 1, 100);
        let prey = value(&w, P0, P1, true);
        let strong = value(&pair(1, 30, 100), P0, P1, true);
        assert!(prey.enemy > strong.enemy, "{prey:?} against {strong:?}");
        assert!(prey.enemy >= Knobs::current().enemy_min, "{prey:?}");
        assert!(strong.enemy < Knobs::current().enemy_min, "{strong:?}");
        // Out of reach nobody is a target, however weak.
        assert!(value(&w, P0, P1, false).enemy < Knobs::current().enemy_min);
    }

    #[test]
    fn a_stronger_neighbour_that_is_near_is_courted_and_a_weak_one_is_not_feared() {
        let weak = value(&pair(30, 1, 100), P0, P1, true);
        let strong = value(&pair(1, 30, 100), P0, P1, true);
        assert!(strong.friend > weak.friend, "{strong:?} against {weak:?}");
    }

    #[test]
    fn a_treaty_spares_a_partner() {
        use f4x_core::diplomacy::DiplomacyCommand::Propose;
        let mut w = pair(30, 1, 100);
        let free = value(&w, P0, P1, true).enemy;
        w.apply(Command::Diplomacy(Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Pact,
        }));
        w.apply(Command::Diplomacy(DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        }));
        if w.treaty(P0, P1) == Treaty::Pact {
            assert!(value(&w, P0, P1, true).enemy < free, "a pact spares it");
        }
    }

    #[test]
    fn a_wizard_on_b_declares_war_on_the_prey_it_can_reach() {
        let mut w = pair(30, 1, 100);
        let mut ai = Ai::new(P0);
        let scene = Scene::new(&w, P0);
        run(&mut ai, &mut w, &mut Vec::new(), &scene);
        assert_eq!(w.treaty(P0, P1), Treaty::War, "the prey is declared war on");
        assert_eq!(ai.memory.get().enemy.map(|e| e.0), Some(P1));
    }
}
