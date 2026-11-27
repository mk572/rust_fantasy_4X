//! Diplomacy system B, part 1 (task 762, `docs/ai-diplomacy-b.md`): three values for each other wizard
//! ([`Regard`]), the weariness of each war, and **one peace score** ([`peace_wish`]) that decides both
//! when the wizard offers peace and whether it accepts one. Only a wizard of system B uses it
//! (`Ai::system`); system A keeps `statecraft::wars`, `statecraft::peace_ok` and the peace roll of
//! `diplomacy::negotiate`.
//!
//! * **Friendship**: the relation and the hidden relation of the pair (the core's), which makes it overlook
//!   faults.
//! * **Fear**: how near and how strong the other is. It does not push one way: the one that is outclassed
//!   begs, the stronger one finishes the other off.
//! * **Strategic**: a weak, reachable neighbour is prey; the more dangerous of two fronts is worth a peace.
//! * **Weariness** (`Memory`): up for a battle or a city lost, down for one won or taken, up a little every
//!   turn without a clash, never below 0.
//!
//! Every number is in [`Knobs`] (`AIK_DIPLO_B_<NAME>`). Tests: below.

use f4x_core::diplomacy::{ProposalKind, Remark, Treaty};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::world::{Event, World};

use crate::memory::Memory;
use crate::statecraft::{propose, remark, Scene};
use crate::Ai;
use f4x_master::Game;

/// Every number of system B's peace.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Knobs {
    /// The wish at which it offers peace, and the one at which it accepts one (lower).
    pub offer: i32,
    pub accept: i32,
    /// Turns between two offers to the same wizard.
    pub cool: u32,
    /// What a battle or a city lost or won moves the weariness by.
    pub battle_lost: i32,
    pub battle_won: i32,
    pub city_lost: i32,
    pub city_taken: i32,
    /// Weariness gained for every ten turns without a clash.
    pub drift_per_ten: i32,
    /// The most the strength gap moves the wish, either way.
    pub gap: i32,
    /// How much of the fear moves the wish (percent); the stronger side hears half of it.
    pub fear_pct: i32,
    /// Percent of the friendship (relation and hidden relation) that counts.
    pub friend_pct: i32,
    /// A neighbour this many percent weaker and nearer than `prey_distance` is prey, which lowers the wish.
    pub prey_pct: i32,
    pub prey_distance: u32,
    pub prey: i32,
    /// The more dangerous of two fronts is worth this much more.
    pub two_fronts: i32,
    /// A war this young is not ended by either side, and by how much.
    pub young_age: u32,
    pub young: i32,
    /// The personality's mood divided by this is added.
    pub mood_div: i32,
    /// Nearness: 100 minus this for each square of distance between the nearest cities.
    pub near_per_square: i32,
    /// What a city razed, a war declared and a treaty broken add to a wizard's ruthlessness, and the
    /// turns it takes to fade by half. The most of it that counts, in fear points.
    pub ruth_raze: i32,
    pub ruth_war: i32,
    pub ruth_broken: i32,
    /// A fortress taken and a wizard destroyed (task 782): fear in every wizard that has met the winner.
    pub ruth_fortress: i32,
    pub ruth_destroyed: i32,
    pub ruth_half: u32,
    pub ruth_cap: i32,
    /// The wizard casting the Spell of Mastery is not made peace with (a minus), and, with a leader in
    /// the game, peace with the others and war on the leader (a plus and a minus of this).
    pub mastery: i32,
    pub leader: i32,
    // The choice of friends and enemies (`diplo_b_stance.rs`).
    /// Turns between two looks at the others, the turns an enemy is kept at least, and the value below
    /// which it is let go.
    pub review: u32,
    pub hold: u32,
    pub keep: i32,
    /// The enemy value a wizard needs to be chosen, and to be declared war on.
    pub enemy_min: i32,
    pub declare: i32,
    /// A war already fought keeps the wizard from declaring another unless its army is this many times
    /// the size of that enemy's army.
    pub busy_army_x: i32,
    /// What each war the wizard already fights takes off its willingness to declare another (from the enemy
    /// value the declaration needs `declare` of).
    pub war_damp: i32,
    /// The most a weak, reachable neighbour is worth as prey; and what an enemy already fought, the leader
    /// and the Mastery caster add; percent of the hostility (negative friendship) that counts; the fear at
    /// which an equal or weaker wizard strikes first.
    pub prey_value: i32,
    pub at_war: i32,
    pub leader_enemy: i32,
    pub mastery_enemy: i32,
    pub hostile_pct: i32,
    pub strike_fear: i32,
    /// No war before this turn and not on someone this much stronger (percent).
    pub from_turn: u32,
    pub too_strong_pct: i32,
    /// Friend value: percent of friendship, what a shared enemy adds, percent of the fear of a stronger
    /// neighbour (grovelling), what the league against the leader adds.
    pub stance_friend_pct: i32,
    pub common_enemy: i32,
    pub grovel_pct: i32,
    pub league: i32,
    /// The friend value at which it asks for a pact, for an alliance, and gives a gift; turns between two
    /// advances to the same wizard.
    pub pact_min: i32,
    pub ally_min: i32,
    pub gift_min: i32,
    pub court_cool: u32,
    // Answering and talking (`diplo_b_talk.rs`).
    /// The friend value at which a pact, an alliance is accepted; the friendship at which a war is
    /// accepted on a third wizard (with the enemy value of the third); the lead in friendship over the
    /// third that makes it break an alliance for the asker.
    pub pact_accept: i32,
    pub ally_accept: i32,
    pub war_accept: i32,
    pub break_accept: i32,
    /// The friendship at which it warns a treaty partner, breaks the treaty, and declares war on someone it
    /// has no treaty with.
    pub warn_below: i32,
    pub break_below: i32,
    pub war_below: i32,
    /// Turns between two warnings, demands, gifts and trade offers to the same player.
    pub warn_cool: u32,
    pub demand_cool: u32,
    pub gift_cool: u32,
    pub trade_cool: u32,
    /// The friendship from which a treaty partner is given gifts; the fear from which a wizard yields
    /// to a threat or a demand.
    pub gift_friend: i32,
    pub yield_fear: i32,
    /// What an ally at war with a wizard adds to the enemy value of that wizard.
    pub ally_war: i32,
}

impl Knobs {
    pub(crate) fn current() -> Self {
        let k = |name: &str, default: i32| {
            crate::knob(&format!("DIPLO_B_{name}"), default as f32) as i32
        };
        Self {
            offer: k("OFFER", 80),
            accept: k("ACCEPT", 60),
            cool: k("COOL", 15) as u32,
            battle_lost: k("BATTLE_LOST", 5),
            battle_won: k("BATTLE_WON", 4),
            city_lost: k("CITY_LOST", 25),
            city_taken: k("CITY_TAKEN", 25),
            drift_per_ten: k("DRIFT_PER_TEN", 7),
            gap: k("GAP", 50),
            fear_pct: k("FEAR_PCT", 40),
            friend_pct: k("FRIEND_PCT", 10),
            prey_pct: k("PREY_PCT", 150),
            prey_distance: k("PREY_DISTANCE", 30) as u32,
            prey: k("PREY", 30),
            two_fronts: k("TWO_FRONTS", 25),
            young_age: k("YOUNG_AGE", 10) as u32,
            young: k("YOUNG", 40),
            mood_div: k("MOOD_DIV", 3).max(1),
            near_per_square: k("NEAR_PER_SQUARE", 5),
            ruth_raze: k("RUTH_RAZE", 15),
            ruth_war: k("RUTH_WAR", 6),
            ruth_broken: k("RUTH_BROKEN", 10),
            ruth_fortress: k("RUTH_FORTRESS", 10),
            ruth_destroyed: k("RUTH_DESTROYED", 20),
            ruth_half: k("RUTH_HALF", 150) as u32,
            ruth_cap: k("RUTH_CAP", 60),
            mastery: k("MASTERY", 60),
            leader: k("LEADER", 15),
            review: k("REVIEW", 5) as u32,
            hold: k("HOLD", 40) as u32,
            keep: k("KEEP", 20),
            enemy_min: k("ENEMY_MIN", 25),
            declare: k("DECLARE", 30),
            busy_army_x: k("BUSY_ARMY_X", 2),
            war_damp: k("WAR_DAMP", 15),
            prey_value: k("PREY_VALUE", 60),
            at_war: k("AT_WAR", 30),
            leader_enemy: k("LEADER_ENEMY", 25),
            mastery_enemy: k("MASTERY_ENEMY", 60),
            hostile_pct: k("HOSTILE_PCT", 30),
            strike_fear: k("STRIKE_FEAR", 50),
            from_turn: k("FROM_TURN", 5) as u32,
            too_strong_pct: k("TOO_STRONG_PCT", 130),
            stance_friend_pct: k("STANCE_FRIEND_PCT", 30),
            common_enemy: k("COMMON_ENEMY", 30),
            grovel_pct: k("GROVEL_PCT", 40),
            league: k("LEAGUE", 20),
            pact_min: k("PACT_MIN", 30),
            ally_min: k("ALLY_MIN", 55),
            gift_min: k("GIFT_MIN", 25),
            court_cool: k("COURT_COOL", 20) as u32,
            pact_accept: k("PACT_ACCEPT", 20),
            ally_accept: k("ALLY_ACCEPT", 45),
            war_accept: k("WAR_ACCEPT", 60),
            break_accept: k("BREAK_ACCEPT", 40),
            warn_below: k("WARN_BELOW", -30),
            break_below: k("BREAK_BELOW", -50),
            war_below: k("WAR_BELOW", -75),
            warn_cool: k("WARN_COOL", 10) as u32,
            demand_cool: k("DEMAND_COOL", 20) as u32,
            gift_cool: k("GIFT_COOL", 30) as u32,
            trade_cool: k("TRADE_COOL", 15) as u32,
            gift_friend: k("GIFT_FRIEND", 60),
            yield_fear: k("YIELD_FEAR", 50),
            ally_war: k("ALLY_WAR", 30),
        }
    }
}

/// What a wizard feels about another (task 762).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Regard {
    /// The relation and the hidden relation: how likeable the other is.
    pub friendship: i32,
    /// 0 to 100: near, and strong against us. (Its ruthlessness is added when the AI is told of
    /// what a wizard does to the others.)
    pub fear: i32,
    /// The power politics of the pair: prey is a minus, the more dangerous front a plus.
    pub strategic: i32,
}

/// The weariness of the war with `other` now: what the battles and cities moved it by, and a little
/// more for every turn since the last clash or the start of the war. Nothing at peace.
pub(crate) fn weariness(
    world: &World,
    me: PlayerId,
    other: PlayerId,
    memory: &Memory,
    round: u32,
) -> i32 {
    let k = Knobs::current();
    let Some(age) = world.war_age(me, other) else {
        return 0;
    };
    let since = round.saturating_sub(age);
    let d = memory.with(other).filter(|d| d.war_since == since);
    let (events, clash) = d.map_or((0, 0), |d| (d.weary, d.last_clash));
    let quiet = round.saturating_sub(clash.max(since));
    events + (quiet as i32).saturating_mul(k.drift_per_ten) / 10
}

/// How `me` regards `other`.
pub(crate) fn regard(
    world: &World,
    me: PlayerId,
    other: PlayerId,
    scene: &Scene,
    memory: &Memory,
    round: u32,
) -> Regard {
    let k = Knobs::current();
    let sk = crate::statecraft::Knobs::current();
    let opinion = memory.opinion(world, me, other);
    let friendship = opinion.hidden + opinion.friendship;
    let (mine, theirs) = (scene.power(world, me), scene.power(world, other));
    let (m, t) = (mine.score(&sk).max(1), theirs.score(&sk));
    let distance = nearest(world, scene, me, other);
    let near = distance.map_or(0, |d| (100 - k.near_per_square * d as i32).clamp(0, 100));
    let might = (t * 100 / m - 50).clamp(0, 100);
    // Ruthlessness is felt in proportion to how near the ruthless one is.
    let ruth = memory
        .ruthlessness(other, round, k.ruth_half)
        .min(k.ruth_cap);
    let fear = (near * might / 100 + near * ruth / 100).clamp(0, 100);
    let mut strategic = 0;
    // The wizard that is about to win by the Spell of Mastery must be stopped; and with a leader
    // in the game the weaker make peace among themselves and keep the war on the leader.
    if PlayerView::new(world, me)
        .rival_mastery_casters()
        .iter()
        .any(|&(p, _)| p == other)
    {
        strategic -= k.mastery;
    }
    match crate::statecraft::leader_of(world, me) {
        Some(leader) if leader == other => strategic -= k.leader,
        Some(leader) if world.treaty(me, leader) == Treaty::War => strategic += k.leader,
        _ => {}
    }
    if distance.is_some_and(|d| d <= k.prey_distance) && m * 100 >= k.prey_pct * t.max(1) {
        strategic -= k.prey;
    }
    let wars: Vec<PlayerId> = scene
        .others
        .iter()
        .copied()
        .filter(|&p| world.treaty(me, p) == Treaty::War)
        .collect();
    if wars.len() >= 2 {
        let danger = wars
            .iter()
            .copied()
            .max_by_key(|&w| (scene.power(world, w).score(&sk), std::cmp::Reverse(w.0)));
        if danger == Some(other) {
            strategic += k.two_fronts;
        }
    }
    Regard {
        friendship,
        fear,
        strategic,
    }
}

/// The squares between the nearest cities of the two, as `me` knows them.
fn nearest(world: &World, scene: &Scene, me: PlayerId, other: PlayerId) -> Option<u32> {
    let view = PlayerView::new(world, me);
    scene
        .mine
        .iter()
        .flat_map(|&a| scene.cities_of(other).into_iter().map(move |b| (a, b)))
        .filter_map(|(a, b)| view.distance(a, b))
        .min()
}

/// How much `me` wants peace with `other`: the weariness of the war plus the gap in strength, the
/// fear, the friendship, the strategic value, the temper, less a young war. At `offer` it asks, at
/// `accept` it says yes.
pub(crate) fn peace_wish(
    world: &World,
    me: PlayerId,
    other: PlayerId,
    memory: &Memory,
    round: u32,
) -> i32 {
    let k = Knobs::current();
    let sk = crate::statecraft::Knobs::current();
    let scene = Scene::new(world, me);
    let r = regard(world, me, other, &scene, memory, round);
    let (m, t) = (
        scene.power(world, me).score(&sk).max(1),
        scene.power(world, other).score(&sk),
    );
    // Losing is a wish for peace, far ahead is not.
    let gap = (t - m) * k.gap / t.max(m);
    // The outclassed begs; the stronger finishes the other off.
    let fear = if t >= m {
        r.fear * k.fear_pct / 100
    } else {
        -r.fear * k.fear_pct / 200
    };
    let mood = crate::objective::temper(world.personality(me), world.difficulty()).modifier();
    let young = match world.war_age(me, other) {
        Some(age) if age < k.young_age => k.young,
        _ => 0,
    };
    weariness(world, me, other, memory, round)
        + gap
        + fear
        + r.friendship * k.friend_pct / 100
        + r.strategic
        + mood / k.mood_div
        - young
}

/// Whether a wizard of system B says yes to a peace `asker` offers (`diplomacy::rate`).
pub(crate) fn accepts(
    world: &World,
    judge: PlayerId,
    asker: PlayerId,
    memory: &Memory,
    round: u32,
) -> bool {
    world.treaty(judge, asker) != Treaty::War
        || peace_wish(world, judge, asker, memory, round) >= Knobs::current().accept
}

/// Whether `ai` asks `other` for peace now: at war, the wish is at the offer's mark and it has not
/// asked lately.
pub(crate) fn offer_due(ai: &Ai, world: &World, other: PlayerId, round: u32) -> bool {
    let k = Knobs::current();
    let memory = ai.memory.get();
    world.treaty(ai.player, other) == Treaty::War
        && memory
            .with(other)
            .is_none_or(|d| d.offered == 0 || round >= d.offered + k.cool)
        && peace_wish(world, ai.player, other, &memory, round) >= k.offer
}

/// The wars it fights with the other wizards: peace is offered where the wish says so.
pub(crate) fn wars(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>, scene: &Scene) {
    for &w in &scene.others {
        // The human is asked by `approach_human`.
        if game.human() == Some(w) || !offer_due(ai, game, w, game.round()) {
            continue;
        }
        if propose(ai, game, events, w, ProposalKind::Peace) {
            ai.memory.put().offered_peace(w, game.round());
            remark(ai, game, events, w, Remark::WarWeary);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0, P1};
    use f4x_core::diplomacy::DiplomacyCommand;
    use f4x_core::rules::RealmId;
    use f4x_core::world::Command;
    use f4x_def::half_int::HalfInt;
    use f4x_master::Game;

    /// Where the east wizard has a city on the west island.
    const RIVAL: (u32, u32) = (3, 8);

    fn found(w: &mut Game, owner: PlayerId, x: u32, y: u32) {
        let race = w.content().rules.races.id_numbered(8);
        w.apply(Command::FoundCity {
            owner,
            pos: at(x, y),
            name: format!("City{x}x{y}"),
            race,
            population: 5,
        });
    }

    /// P0 and P1 are at war since turn 100, with armies of these sizes, each with two cities (P1's
    /// nearer one on P0's island) and it is turn `turn`.
    fn war(army0: usize, army1: usize, turn: u32) -> Game {
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
        w.set_round(100);
        let events = w.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        assert!(!events.iter().any(|e| matches!(e, Event::Rejected(_))));
        w.set_round(turn);
        w
    }

    fn battle_lost() -> Event {
        Event::BattleEnded {
            winner: P1,
            loser: P0,
        }
    }

    fn battle_won() -> Event {
        Event::BattleEnded {
            winner: P0,
            loser: P1,
        }
    }

    fn city_lost() -> Event {
        Event::CityCaptured {
            city: hecs::Entity::DANGLING,
            from: P0,
            to: P1,
        }
    }

    fn city_taken() -> Event {
        Event::CityCaptured {
            city: hecs::Entity::DANGLING,
            from: P1,
            to: P0,
        }
    }

    fn told(w: &Game, events: &[Event]) -> Memory {
        let mut m = Memory::default();
        m.notice(P0, w, w.round(), events);
        m
    }

    #[test]
    fn weariness_rises_with_losses_falls_with_wins_and_is_never_below_zero() {
        let w = war(8, 8, 110);
        let m = told(&w, &[battle_lost(), battle_lost(), city_lost()]);
        let lost = weariness(&w, P0, P1, &m, w.round());
        assert!(lost > 0, "losses wear it down");
        let m = told(
            &w,
            &[battle_lost(), battle_won(), battle_won(), city_taken()],
        );
        assert_eq!(
            weariness(&w, P0, P1, &m, w.round()),
            0,
            "wins bring it down to 0 and no lower"
        );
        let m = told(&w, &[city_lost(), city_taken(), battle_lost()]);
        let after_wins = weariness(&w, P0, P1, &m, w.round());
        assert!(after_wins < lost, "a city and a battle won ease it");
    }

    #[test]
    fn a_quiet_war_wears_down_and_a_clash_starts_the_count_again() {
        let mut w = war(8, 8, 110);
        let empty = Memory::default();
        let w10 = weariness(&w, P0, P1, &empty, w.round());
        w.set_round(160);
        let w60 = weariness(&w, P0, P1, &empty, w.round());
        assert!(w60 > w10, "{w10} then {w60}");
        // A battle at turn 160 puts the quiet back to zero; only what it moved remains.
        let m = told(&w, &[battle_lost()]);
        assert!(
            weariness(&w, P0, P1, &m, w.round()) < w60,
            "a clash starts the count again"
        );
        // A memory of an earlier war does not count in a new one.
        let peace = told(&w, &[city_lost()]);
        w.declare_peace(P0, P1);
        assert_eq!(
            weariness(&w, P0, P1, &peace, w.round()),
            0,
            "no war, no weariness"
        );
        w.set_round(170);
        w.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        w.set_round(175);
        assert!(
            weariness(&w, P0, P1, &peace, w.round()) < w10,
            "the new war starts at nothing, not at the old city lost"
        );
    }

    #[test]
    fn the_wizard_that_is_losing_wants_peace_more_than_the_one_that_is_winning() {
        let w = war(1, 30, 140);
        let m0 = told(&w, &[battle_lost(), city_lost()]);
        let m1 = Memory::default();
        let loser = peace_wish(&w, P0, P1, &m0, w.round());
        let winner = peace_wish(&w, P1, P0, &m1, w.round());
        assert!(loser > winner, "loser {loser}, winner {winner}");
        assert!(winner < 0, "the winner does not want it: {winner}");
    }

    #[test]
    fn fear_is_for_the_one_outclassed_and_makes_the_stronger_side_hold_out() {
        let w = war(1, 30, 140);
        let scene0 = Scene::new(&w, P0);
        let scene1 = Scene::new(&w, P1);
        let weak = regard(&w, P0, P1, &scene0, &Memory::default(), w.round());
        let strong = regard(&w, P1, P0, &scene1, &Memory::default(), w.round());
        assert!(weak.fear > 0, "near and strong: {weak:?}");
        assert_eq!(strong.fear, 0, "a weak neighbour is no fear: {strong:?}");
        // The same wizard with nothing to fear wants the peace less.
        let even = war(30, 30, 140);
        let calm = peace_wish(&even, P0, P1, &Memory::default(), w.round());
        let afraid = peace_wish(&w, P0, P1, &Memory::default(), w.round());
        assert!(afraid > calm, "afraid {afraid}, calm {calm}");
    }

    #[test]
    fn a_stalled_war_between_equals_drifts_to_an_offer_and_the_cool_down_holds() {
        let k = Knobs::current();
        let mut w = war(10, 10, 110);
        let mut ai = Ai::new(P0);
        let mut turn = 110;
        // Until the wish reaches the mark nothing is offered.
        while peace_wish(&w, P0, P1, &ai.memory.get(), w.round()) < k.offer {
            assert!(!offer_due(&ai, &w, P1, w.round()));
            turn += 5;
            w.set_round(turn);
            assert!(turn < 2000, "a quiet war reaches the mark in the end");
        }
        assert!(offer_due(&ai, &w, P1, w.round()));
        let scene = Scene::new(&w, P0);
        let mut events = Vec::new();
        wars(&mut ai, &mut w, &mut events, &scene);
        assert_eq!(ai.memory.get().with(P1).map(|d| d.offered), Some(turn));
        // It does not ask again at once, and asks again when the cool-down is over.
        w.set_round(turn + 1);
        assert!(!offer_due(&ai, &w, P1, w.round()), "cool-down");
        w.set_round(turn + k.cool);
        assert!(offer_due(&ai, &w, P1, w.round()) || w.treaty(P0, P1) != Treaty::War);
    }

    #[test]
    fn a_war_that_is_going_well_is_not_offered_peace_and_a_losing_one_is_accepted() {
        let w = war(1, 30, 160);
        let m = told(
            &w,
            &[battle_lost(), city_lost(), city_lost(), battle_lost()],
        );
        assert!(accepts(&w, P0, P1, &m, w.round()), "the loser says yes");
        let winner = Memory::default();
        assert!(
            !accepts(&w, P1, P0, &winner, w.round()),
            "the winner says no"
        );
        // And at peace everyone accepts what is not a war's peace.
        let mut at_peace = war(1, 30, 160);
        at_peace.declare_peace(P0, P1);
        assert!(accepts(&at_peace, P1, P0, &winner, w.round()));
    }

    fn razed() -> Event {
        Event::CityRazed {
            player: P1,
            gold: 0,
            fame_lost: 0,
        }
    }

    #[test]
    fn what_a_wizard_met_does_is_noted_as_ruthlessness_and_fades() {
        use f4x_core::diplomacy::DiplomacyEvent;
        let k = Knobs::current();
        let w = war(8, 8, 110);
        let mut m = Memory::default();
        m.notice(
            P0,
            &w,
            w.round(),
            &[
                razed(),
                Event::Diplomacy(DiplomacyEvent::WarDeclared {
                    attacker: P1,
                    defender: f4x_core::player::MONSTERS,
                }),
                Event::Diplomacy(DiplomacyEvent::TreatyBroken {
                    breaker: P1,
                    victim: f4x_core::player::MONSTERS,
                    treaty: Treaty::Pact,
                }),
            ],
        );
        let now = m.ruthlessness(P1, 110, k.ruth_half);
        assert!(now > 0);
        let later = m.ruthlessness(P1, 110 + k.ruth_half, k.ruth_half);
        assert!(0 < later && later < now, "it fades");
        assert!(m.ruthlessness(P1, 110 + 40 * k.ruth_half, k.ruth_half) <= 1);
        // The acts of a wizard that has not been met are not its business.
        let stranger = game(COAST_CITY, 8 /* high_men */);
        let mut m = Memory::default();
        m.notice(P0, &stranger, stranger.round(), &[razed()]);
        assert_eq!(m.ruthlessness(P1, stranger.round(), k.ruth_half), 0);
    }

    #[test]
    fn a_ruthless_neighbour_is_feared_more() {
        let w = war(10, 10, 140);
        let scene = Scene::new(&w, P0);
        let calm = regard(&w, P0, P1, &scene, &Memory::default(), w.round());
        let mut m = Memory::default();
        m.notice(P0, &w, w.round(), &[razed(), razed()]);
        let ruthless = regard(&w, P0, P1, &scene, &m, w.round());
        assert!(ruthless.fear > calm.fear, "{ruthless:?} against {calm:?}");
    }

    use crate::{answer_proposals, Diplomats};

    /// `asker` proposes peace to `judge`, who answers through the game's table; whether the war is over.
    fn asked_for_peace(w: &mut Game, asker: PlayerId, judge: PlayerId, table: &Diplomats) -> bool {
        w.apply(Command::Diplomacy(DiplomacyCommand::Propose {
            from: asker,
            to: judge,
            kind: ProposalKind::Peace,
        }));
        answer_proposals(w, judge, table);
        w.treaty(asker, judge) != Treaty::War
    }

    #[test]
    fn a_peace_is_answered_by_the_judges_system_and_memory_through_the_table() {
        // P0 is far behind, P1 far ahead.
        // A judge on system B that is winning answers by the score and says no.
        let mut b = war(1, 30, 160);
        let table = Diplomats::new();
        let mut p1 = Ai::new(P1);
        p1.join(&table);
        assert!(!asked_for_peace(&mut b, P0, P1, &table), "B says no");
        // And P1 asking P0, which has lost a lot and is on B, is taken.
        let mut c = war(1, 30, 160);
        let table = Diplomats::new();
        let mut p0 = Ai::new(P0);
        p0.join(&table);
        p0.notice(
            &c,
            &[battle_lost(), city_lost(), city_lost(), battle_lost()],
        );
        assert!(
            asked_for_peace(&mut c, P1, P0, &table),
            "the loser on B says yes"
        );
    }

    #[test]
    fn the_statecraft_of_a_wizard_on_b_offers_peace_by_the_score_and_a_on_a_does_not_use_it() {
        let k = Knobs::current();
        let mut w = war(1, 30, 200);
        let mut ai = Ai::new(P0);
        ai.notice(
            &w,
            &[battle_lost(), city_lost(), city_lost(), battle_lost()],
        );
        assert!(peace_wish(&w, P0, P1, &ai.memory.get(), w.round()) >= k.offer);
        let mut events = Vec::new();
        crate::statecraft::run(&mut ai, &mut w, &mut events);
        assert_eq!(
            ai.memory.get().with(P1).map(|d| d.offered),
            Some(200),
            "B asked"
        );
    }

    #[test]
    fn the_detail_note_names_the_system_the_enemy_and_what_was_seen() {
        let w = war(8, 8, 110);
        let mut ai = Ai::new(P0);
        let first = ai.diplomacy_note(&w, w.round());
        assert!(
            first.starts_with("diplomacy B, enemy -, weary [], seen [], regard ["),
            "{first}"
        );
        assert!(
            first.contains("p1 friendship")
                && first.contains("fear")
                && first.contains("strategic"),
            "the regard names the three values: {first}"
        );
        ai.notice(&w, &[razed()]);
        ai.memory.put().enemy = Some((P1, 100));
        let note = ai.diplomacy_note(&w, w.round());
        assert!(
            note.starts_with("diplomacy B, enemy p1 since 100"),
            "{note}"
        );
        assert!(note.contains("p1 razed 1"), "{note}");
    }
}
