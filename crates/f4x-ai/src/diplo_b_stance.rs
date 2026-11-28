//! Who to fight and who to befriend (`docs/ai-diplomacy-b.md`), from the three values of
//! `diplo_b::Regard`. Every few turns each other wizard gets an **enemy value** and
//! a **friend value**; at most one is the **main enemy** (kept for a while, so the choice does not flip each
//! turn), which a war is declared on; the rest are courted by their friend value: a pact,
//! an alliance, a gift.
//!
//! * **Enemy value**: a weak, reachable neighbour is prey (the more so the weaker); hostility (a negative
//!   friendship); the leader and the Mastery caster; the one already fought; and, when it fears the other
//!   and is not the weaker, the wish to strike first. A wizard much stronger is not a target, and a treaty
//!   partner is spared (a pact more, an alliance most).
//! * **Friend value**: the friendship; a shared enemy; the fear of a stronger neighbour (to grovel to it);
//!   the weaker wizards leaguing against the leader.
//!
//! Tests: below.

use crate::lever::Levered;
use f4x_core::diplomacy::{DiplomacyCommand, ProposalKind, Remark, Treaty};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::world::{Command, Event, World};

use crate::diplo_b::regard;
use crate::memory::Memory;
use crate::statecraft::{can_attack, leader_of, propose, remark, Power, Scene};
use crate::Ai;
use f4x_master::Game;

/// What `me` makes of `other`.
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
    let k = world.levers().diplomacy_b;
    let sk = world.levers().diplomacy;
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
    if t * 100 <= m * k.equal_pct && r.fear >= k.strike_fear {
        enemy += r.fear / k.strike_fear_divisor;
    }
    // Not the one that is much stronger.
    if t * 100 > m * k.too_strong_pct {
        enemy -= (t - m) * 100 / t;
    }
    enemy -= match treaty {
        Treaty::Pact => k.pact_penalty,
        Treaty::Alliance => k.alliance_penalty,
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
    let k = world.levers().diplomacy_b;
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

/// Once a turn (`statecraft::run`): look at the others now and then, choose the enemy and declare
/// the war. The friends are courted at the end of the turn (`diplo_b_talk::approach`).
pub(crate) fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>, scene: &Scene) {
    let me = ai.player;
    let k = game.levers().diplomacy_b;
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

/// Would the AI declare war on `other` now? The game is past the opening, there is no treaty, no
/// war has just ended between the two, the enemy value is high enough and no war is on that the
/// army could not spare (a front at a time, but for a straggler).
pub(crate) fn open_to_war(
    ai: &Ai,
    game: &Game,
    scene: &Scene,
    other: PlayerId,
    reachable: bool,
) -> bool {
    let me = ai.player;
    let k = game.levers().diplomacy_b;
    if game.round() < k.from_turn || !crate::attack::may_declare(&PlayerView::new(game, me), other)
    {
        return false;
    }
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
        other,
        scene,
        &ai.memory.get(),
        reachable,
        game.round(),
    )
    .enemy
        - wars * k.war_damp;
    !busy && reachable && v >= k.declare
}

/// War on the chosen enemy, when it is time ([`open_to_war`]).
fn declare(
    ai: &mut Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    scene: &Scene,
    enemy: PlayerId,
    reachable: bool,
) {
    let me = ai.player;
    if !open_to_war(ai, game, scene, enemy, reachable) {
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
/// relation is too low for a pact; once in `court_cool` turns. Whether something was done.
pub(crate) fn court(ai: &Ai, game: &mut Game, events: &mut Vec<Event>, other: PlayerId) -> bool {
    let me = ai.player;
    let k = game.levers().diplomacy_b;
    let (friend, due) = {
        let memory = ai.memory.get();
        let scene = Scene::new(game, me);
        let due = memory
            .with(other)
            .is_none_or(|d| d.courted == 0 || game.round() >= d.courted + k.court_cool);
        // Reach counts for the enemy value only.
        (
            values(game, me, other, &scene, &memory, false, game.round()).friend,
            due,
        )
    };
    if !due || friend < k.gift_min {
        return false;
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
    done
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testworld::levers;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0, P1};
    use f4x_core::diplomacy::DiplomacyEvent;
    use f4x_core::rules::RealmId;
    use f4x_def::half_int::HalfInt;
    use f4x_master::Game;

    pub(crate) const RIVAL: (u32, u32) = (3, 8);

    fn found(game: &mut Game, owner: PlayerId, x: u32, y: u32) {
        let race = game.content().rules.races.id_numbered(8);
        game.command(Command::FoundCity {
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
        w.set_round(turn);
        w
    }

    fn value(w: &Game, me: PlayerId, other: PlayerId, reach: bool) -> Values {
        let scene = Scene::new(w, me);
        values(w, me, other, &scene, &Memory::default(), reach, w.round())
    }

    #[test]
    fn a_pact_or_alliance_partner_is_never_chosen_as_the_enemy() {
        let mut w = pair(30, 1, 100);
        let enemy = |w: &Game, memory: &Memory| {
            let scene = Scene::new(w, P0);
            choose_enemy(w, P0, &scene, memory, &|_| true, w.round()).map(|(p, _)| p)
        };
        assert_eq!(
            enemy(&w, &Memory::default()),
            Some(P1),
            "prey with no treaty"
        );
        for treaty in [Treaty::Pact, Treaty::Alliance] {
            w.start_treaty(P0, P1, treaty);
            assert_eq!(enemy(&w, &Memory::default()), None, "{treaty:?}");
            let kept = Memory {
                enemy: Some((P1, w.round())),
                ..Memory::default()
            };
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
        assert!(prey.enemy >= levers().diplomacy_b.enemy_min, "{prey:?}");
        assert!(strong.enemy < levers().diplomacy_b.enemy_min, "{strong:?}");
        // Out of reach nobody is a target, however weak.
        assert!(value(&w, P0, P1, false).enemy < levers().diplomacy_b.enemy_min);
    }

    #[test]
    fn a_stronger_neighbour_that_is_near_is_courted_and_a_weak_one_is_not_feared() {
        let weak = value(&pair(30, 1, 100), P0, P1, true);
        let strong = value(&pair(1, 30, 100), P0, P1, true);
        assert!(strong.friend > weak.friend, "{strong:?} against {weak:?}");
    }

    #[test]
    fn a_treaty_spares_a_partner_and_an_alliance_more_than_a_pact() {
        let mut w = pair(30, 1, 100);
        let memory = feeling(&w, 0);
        let free = seen(&w, P1, &memory).enemy;
        w.start_treaty(P0, P1, Treaty::Pact);
        let pact = seen(&w, P1, &memory).enemy;
        w.start_treaty(P0, P1, Treaty::Alliance);
        let ally = seen(&w, P1, &memory).enemy;
        assert!(pact < free, "{pact} with a pact, {free} without");
        assert!(ally < pact, "{ally} allied, {pact} with a pact");
    }

    /// A third player that `P0` and `P1` have met, with an army of this size and a city on the east
    /// island that `P0` has not seen (so it is not near enough to be feared).
    fn third(w: &mut Game, army: usize) -> PlayerId {
        let race = w
            .content()
            .rules
            .races
            .id_numbered(3)
            .expect("a third race");
        let id = f4x_core::player::race_player::race_player_id(race);
        w.add_neutral_player(id);
        found(w, id, 32, 8);
        w.make_contact(P0, id);
        w.make_contact(P1, id);
        spawn(w, id, at(32, 8), 95 /* high_elf_spearmen */, army);
        id
    }

    /// `P0`'s memory with this feeling about `P1`, so that the ill will stays put while a test
    /// changes the world.
    fn feeling(w: &Game, friendship: i32) -> Memory {
        let mut m = Memory::default();
        m.set_friendship(w, P0, P1, friendship);
        m
    }

    /// What `P0` makes of `other`, which an army can reach.
    fn seen(w: &Game, other: PlayerId, memory: &Memory) -> Values {
        let scene = Scene::new(w, P0);
        values(w, P0, other, &scene, memory, true, w.round())
    }

    fn chosen(w: &Game, memory: &Memory) -> Option<(PlayerId, u32)> {
        let scene = Scene::new(w, P0);
        choose_enemy(w, P0, &scene, memory, &|_| true, w.round())
    }

    /// A feeling of `P0` about `P1`, no better than indifference, at which the enemy value is in
    /// `range`.
    fn ill_will_for(w: &Game, range: std::ops::Range<i32>) -> i32 {
        (-100..=0)
            .rev()
            .find(|&f| range.contains(&seen(w, P1, &feeling(w, f)).enemy))
            .unwrap_or_else(|| panic!("no feeling gives an enemy value in {range:?}"))
    }

    #[test]
    fn ill_will_adds_to_the_enemy_value() {
        let w = pair(8, 8, 100);
        let calm = seen(&w, P1, &feeling(&w, 0)).enemy;
        let hostile = seen(&w, P1, &feeling(&w, -100)).enemy;
        assert_eq!(hostile - calm, levers().diplomacy_b.hostile_pct);
    }

    #[test]
    fn a_hated_foe_out_of_reach_is_not_worth_choosing() {
        let k = &levers().diplomacy_b;
        let mut w = pair(8, 8, 100);
        w.declare_war(P0, P1);
        let memory = feeling(&w, -100);
        let near = seen(&w, P1, &memory).enemy;
        assert!(near >= k.enemy_min, "{near}");
        let scene = Scene::new(&w, P0);
        let far = values(&w, P0, P1, &scene, &memory, false, w.round()).enemy;
        assert!(far < k.enemy_min, "{far}");
    }

    #[test]
    fn a_chosen_enemy_is_held_for_a_while_and_then_let_go_if_not_worth_it() {
        let k = &levers().diplomacy_b;
        let mut w = pair(1, 30, 100);
        let mut memory = feeling(&w, 0);
        assert!(seen(&w, P1, &memory).enemy < k.keep, "not worth keeping");
        let since = w.round();
        memory.enemy = Some((P1, since));
        w.set_round(since + k.hold - 1);
        assert_eq!(chosen(&w, &memory), Some((P1, since)), "still held");
        w.set_round(since + k.hold);
        assert_eq!(chosen(&w, &memory), None, "the hold is over");
    }

    #[test]
    fn an_enemy_still_worth_keeping_is_kept_after_the_hold_though_it_would_not_be_chosen() {
        let k = &levers().diplomacy_b;
        let mut w = pair(8, 8, 100);
        let mut memory = feeling(&w, ill_will_for(&w, k.keep..k.enemy_min));
        assert_eq!(chosen(&w, &memory), None, "too little to be chosen");
        let since = w.round();
        memory.enemy = Some((P1, since));
        w.set_round(since + k.hold);
        assert_eq!(chosen(&w, &memory), Some((P1, since)));
    }

    #[test]
    fn the_one_already_fought_is_the_more_an_enemy() {
        let mut w = pair(8, 8, 100);
        let memory = feeling(&w, 0);
        let before = seen(&w, P1, &memory).enemy;
        w.declare_war(P0, P1);
        assert_eq!(
            seen(&w, P1, &memory).enemy - before,
            levers().diplomacy_b.at_war
        );
    }

    #[test]
    fn the_leader_is_more_of_an_enemy_and_the_others_league_against_it() {
        let k = &levers().diplomacy_b;
        let mut w = pair(8, 60, 100);
        let other = third(&mut w, 1);
        let memory = feeling(&w, 0);
        assert_eq!(leader_of(&w, P0), Some(P1), "the strong one leads");
        let (leading, with_leader) = (seen(&w, P1, &memory), seen(&w, other, &memory));
        // The third catches up, so nobody leads; what stands between `P0` and `P1` is unchanged.
        spawn(
            &mut w,
            other,
            at(32, 8),
            95, /* high_elf_spearmen */
            60,
        );
        assert_eq!(leader_of(&w, P0), None, "nobody leads");
        let (level, without_leader) = (seen(&w, P1, &memory), seen(&w, other, &memory));
        assert_eq!(leading.enemy - level.enemy, k.leader_enemy);
        assert_eq!(level.friend - leading.friend, k.league, "not courted");
        // The third is now strong too, but its cities are unseen: no fear to grovel by.
        assert_eq!(with_leader.friend - without_leader.friend, k.league);
    }

    #[test]
    fn the_caster_of_the_spell_of_mastery_is_the_more_an_enemy() {
        let k = &levers().diplomacy_b;
        let mut w = pair(8, 8, 100);
        let memory = feeling(&w, 0);
        let mastery = w
            .content()
            .rules
            .spells
            .id_numbered(213 /* spell_of_mastery */)
            .expect("the spell is shipped");
        assert_eq!(leader_of(&w, P0), None, "nobody leads yet");
        let before = seen(&w, P1, &memory).enemy;
        w.scenario_casting(P1, mastery, 5000);
        // The caster counts as the leader too.
        assert_eq!(
            seen(&w, P1, &memory).enemy - before,
            k.mastery_enemy + k.leader_enemy
        );
    }

    #[test]
    fn a_shared_enemy_makes_a_friend_whether_at_war_with_it_or_only_chosen() {
        let k = &levers().diplomacy_b;
        let mut w = pair(8, 8, 100);
        let other = third(&mut w, 8);
        let mut memory = feeling(&w, 0);
        let before = seen(&w, other, &memory).friend;
        w.declare_war(other, P1);
        assert_eq!(seen(&w, other, &memory).friend, before, "its war alone");
        memory.enemy = Some((P1, w.round()));
        assert_eq!(seen(&w, other, &memory).friend - before, k.common_enemy);
        memory.enemy = None;
        w.declare_war(P0, P1);
        assert_eq!(seen(&w, other, &memory).friend - before, k.common_enemy);
    }

    #[test]
    fn the_chosen_enemy_is_not_courted() {
        let w = pair(8, 8, 100);
        let mut memory = feeling(&w, 0);
        let before = seen(&w, P1, &memory).friend;
        memory.enemy = Some((P1, w.round()));
        assert_eq!(
            before - seen(&w, P1, &memory).friend,
            levers().diplomacy_b.league
        );
    }

    #[test]
    fn an_allys_war_is_joined() {
        let mut w = pair(8, 8, 100);
        let other = third(&mut w, 8);
        let memory = feeling(&w, 0);
        w.start_treaty(P0, other, Treaty::Alliance);
        let before = seen(&w, P1, &memory).enemy;
        w.declare_war(other, P1);
        assert_eq!(
            seen(&w, P1, &memory).enemy - before,
            levers().diplomacy_b.ally_war
        );
    }

    #[test]
    fn a_feared_neighbour_that_is_no_stronger_is_struck_first() {
        let k = &levers().diplomacy_b;
        let mut w = pair(8, 8, 100);
        // A city of P1 close by, so that what it does is felt.
        found(&mut w, P1, 7, 10);
        w.explore(P0, at(7, 10), HalfInt::from_raw(3));
        let calm = feeling(&w, 0);
        let mut afraid = calm.clone();
        let razed = Event::CityRazed {
            player: P1,
            gold: 0,
            fame_lost: 0,
        };
        let enough = vec![razed; (k.ruth_cap / k.ruth_raze + 1) as usize];
        afraid.notice(P0, &w, w.round(), &enough);
        // Only the memory of what it did differs, not the feeling.
        afraid.opinions = calm.opinions.clone();
        let scene = Scene::new(&w, P0);
        let fear = |m: &Memory| regard(&w, P0, P1, &scene, m, w.round()).fear;
        assert!(fear(&calm) < k.strike_fear, "{}", fear(&calm));
        assert!(fear(&afraid) >= k.strike_fear, "{}", fear(&afraid));
        assert_eq!(
            seen(&w, P1, &afraid).enemy - seen(&w, P1, &calm).enemy,
            fear(&afraid) / 2
        );
    }

    #[test]
    fn a_second_war_asks_for_more_than_a_first() {
        let k = &levers().diplomacy_b;
        let mut w = pair(8, 8, 100);
        // So weak that the army is not busy with it.
        let other = third(&mut w, 1);
        let ai = Ai::new(P0);
        let willing = ill_will_for(&w, k.declare..k.declare + k.war_damp);
        ai.memory.put().set_friendship(&w, P0, P1, willing);
        let open = |w: &Game, ai: &Ai| open_to_war(ai, w, &Scene::new(w, P0), P1, true);
        assert!(open(&w, &ai), "enough for a first war");
        w.declare_war(P0, other);
        assert!(!open(&w, &ai), "not enough for a second");
    }

    #[test]
    fn a_busy_army_takes_on_no_second_war() {
        let k = &levers().diplomacy_b;
        let open = |others_army: usize| {
            let mut w = pair(30, 1, 100);
            let other = third(&mut w, others_army);
            w.declare_war(P0, other);
            let ai = Ai::new(P0);
            ai.memory.put().set_friendship(&w, P0, P1, -100);
            let scene = Scene::new(&w, P0);
            let value = values(&w, P0, P1, &scene, &ai.memory.get(), true, w.round()).enemy;
            assert!(value - k.war_damp >= k.declare, "{value}");
            let busy =
                scene.power(&w, P0).army < k.busy_army_x * scene.power(&w, other).army.max(1);
            (busy, open_to_war(&ai, &w, &scene, P1, true))
        };
        assert_eq!(open(1), (false, true), "a straggler does not hold it back");
        assert_eq!(open(30), (true, false), "an equal army does");
    }

    #[test]
    fn the_others_are_looked_at_only_now_and_then() {
        let k = &levers().diplomacy_b;
        let mut w = pair(30, 1, 100);
        let mut ai = Ai::new(P0);
        let looked = w.round();
        ai.memory.put().reviewed = looked;
        w.set_round(looked + k.review - 1);
        let scene = Scene::new(&w, P0);
        run(&mut ai, &mut w, &mut Vec::new(), &scene);
        assert_eq!(ai.memory.get().enemy, None, "too soon to look again");
        w.set_round(looked + k.review);
        run(&mut ai, &mut w, &mut Vec::new(), &scene);
        assert_eq!(ai.memory.get().enemy.map(|e| e.0), Some(P1));
    }

    /// The feeling that alone is worth this friend value.
    fn friendship_worth(friend: i32) -> i32 {
        let pct = levers().diplomacy_b.stance_friend_pct;
        (friend * 100 + pct - 1) / pct
    }

    /// `P0` courts `P1`, with a full treasury: what was put to `P1` and whether gold was given.
    fn courting(w: &mut Game, ai: &Ai) -> (bool, Vec<ProposalKind>, bool) {
        let rich = w.levers().diplomacy.gift_least_gold;
        w.treasury_mut(P0).expect("a treasury").gold = HalfInt::from_int(2 * rich);
        let mut events = Vec::new();
        let done = court(ai, w, &mut events, P1);
        let asked = events
            .iter()
            .filter_map(|e| match e {
                Event::Diplomacy(DiplomacyEvent::ProposalMade(p)) if p.from == P0 => Some(p.kind),
                _ => None,
            })
            .collect();
        let gave = events.iter().any(|e| {
            matches!(
                e,
                Event::Diplomacy(DiplomacyEvent::GoldGiven { from: P0, .. })
            )
        });
        (done, asked, gave)
    }

    #[test]
    fn a_friend_is_asked_for_a_pact_and_not_before() {
        let k = &levers().diplomacy_b;
        let mut w = pair(8, 8, 100);
        let ai = Ai::new(P0);
        let short = friendship_worth(k.pact_min) - 100 / k.stance_friend_pct - 1;
        ai.memory.put().set_friendship(&w, P0, P1, short);
        let (done, asked, gave) = courting(&mut w, &ai);
        assert_eq!((done, asked, gave), (false, vec![], false), "not yet");
        let enough = friendship_worth(k.pact_min);
        ai.memory.put().set_friendship(&w, P0, P1, enough);
        let (done, asked, gave) = courting(&mut w, &ai);
        assert_eq!((done, asked, gave), (true, vec![ProposalKind::Pact], false));
    }

    #[test]
    fn a_close_enough_partner_is_asked_for_an_alliance() {
        let k = &levers().diplomacy_b;
        let mut w = pair(8, 8, 100);
        let other = third(&mut w, 8);
        w.start_treaty(P0, P1, Treaty::Pact);
        w.declare_war(P0, other);
        w.declare_war(P1, other);
        let ai = Ai::new(P0);
        let enough = friendship_worth(k.ally_min - k.common_enemy);
        ai.memory.put().set_friendship(&w, P0, P1, enough);
        let (done, asked, gave) = courting(&mut w, &ai);
        assert_eq!(
            (done, asked, gave),
            (true, vec![ProposalKind::Alliance], false)
        );
    }

    #[test]
    fn a_wanted_friend_that_is_not_liked_enough_for_a_pact_is_given_gold() {
        let k = &levers().diplomacy_b;
        let mut w = pair(8, 8, 100);
        let other = third(&mut w, 8);
        w.declare_war(P0, other);
        w.declare_war(P1, other);
        let ai = Ai::new(P0);
        let floor = w.content().rules.config.diplomacy.pact_relation;
        ai.memory.put().set_friendship(&w, P0, P1, floor);
        let friend = seen(&w, P1, &ai.memory.get()).friend;
        assert!(friend >= k.gift_min, "{friend}");
        let (done, asked, gave) = courting(&mut w, &ai);
        assert_eq!((done, asked, gave), (true, vec![], true));
    }

    #[test]
    fn a_wizard_is_courted_again_only_after_a_while() {
        let k = &levers().diplomacy_b;
        let mut w = pair(8, 8, 100);
        let ai = Ai::new(P0);
        ai.memory
            .put()
            .set_friendship(&w, P0, P1, friendship_worth(k.pact_min));
        let last = w.round();
        ai.memory.put().courted(P1, last);
        w.set_round(last + k.court_cool - 1);
        assert!(!courting(&mut w, &ai).0, "too soon");
        w.set_round(last + k.court_cool);
        assert!(courting(&mut w, &ai).0, "due again");
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
