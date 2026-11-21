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
use f4x_core::game::{Event, Game};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;

use crate::memory::Memory;
use crate::statecraft::{propose, remark, Scene};
use crate::Ai;

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
    pub ruth_half: u32,
    pub ruth_cap: i32,
    /// The wizard casting the Spell of Mastery is not made peace with (a minus), and, with a leader in
    /// the game, peace with the others and war on the leader (a plus and a minus of this).
    pub mastery: i32,
    pub leader: i32,
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
            ruth_half: k("RUTH_HALF", 150) as u32,
            ruth_cap: k("RUTH_CAP", 60),
            mastery: k("MASTERY", 60),
            leader: k("LEADER", 15),
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
pub(crate) fn weariness(game: &Game, me: PlayerId, other: PlayerId, memory: &Memory) -> i32 {
    let k = Knobs::current();
    let Some(age) = game.war_age(me, other) else {
        return 0;
    };
    let since = game.turn.saturating_sub(age);
    let d = memory.with(other).filter(|d| d.war_since == since);
    let (events, clash) = d.map_or((0, 0), |d| (d.weary, d.last_clash));
    let quiet = game.turn.saturating_sub(clash.max(since));
    events + (quiet as i32).saturating_mul(k.drift_per_ten) / 10
}

/// How `me` regards `other`.
pub(crate) fn regard(
    game: &Game,
    me: PlayerId,
    other: PlayerId,
    scene: &Scene,
    memory: &Memory,
) -> Regard {
    let k = Knobs::current();
    let sk = crate::statecraft::Knobs::current();
    let stance = game
        .diplomacy
        .stance(other, me)
        .cloned()
        .unwrap_or_default();
    let friendship = stance.hidden + game.relation(me, other);
    let (mine, theirs) = (scene.power(game, me), scene.power(game, other));
    let (m, t) = (mine.score(&sk).max(1), theirs.score(&sk));
    let distance = nearest(game, scene, me, other);
    let near = distance.map_or(0, |d| (100 - k.near_per_square * d as i32).clamp(0, 100));
    let might = (t * 100 / m - 50).clamp(0, 100);
    // Ruthlessness is felt in proportion to how near the ruthless one is.
    let ruth = memory
        .ruthlessness(other, game.turn, k.ruth_half)
        .min(k.ruth_cap);
    let fear = (near * might / 100 + near * ruth / 100).clamp(0, 100);
    let mut strategic = 0;
    // The wizard that is about to win by the Spell of Mastery must be stopped; and with a leader
    // in the game the weaker make peace among themselves and keep the war on the leader.
    if PlayerView::new(game, me)
        .rival_mastery_casters()
        .iter()
        .any(|&(p, _)| p == other)
    {
        strategic -= k.mastery;
    }
    match crate::statecraft::leader_of(game, me) {
        Some(leader) if leader == other => strategic -= k.leader,
        Some(leader) if game.treaty(me, leader) == Treaty::War => strategic += k.leader,
        _ => {}
    }
    if distance.is_some_and(|d| d <= k.prey_distance) && m * 100 >= k.prey_pct * t.max(1) {
        strategic -= k.prey;
    }
    let wars: Vec<PlayerId> = scene
        .others
        .iter()
        .copied()
        .filter(|&p| game.treaty(me, p) == Treaty::War)
        .collect();
    if wars.len() >= 2 {
        let danger = wars
            .iter()
            .copied()
            .max_by_key(|&w| (scene.power(game, w).score(&sk), std::cmp::Reverse(w.0)));
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
fn nearest(game: &Game, scene: &Scene, me: PlayerId, other: PlayerId) -> Option<u32> {
    let view = PlayerView::new(game, me);
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
pub(crate) fn peace_wish(game: &Game, me: PlayerId, other: PlayerId, memory: &Memory) -> i32 {
    let k = Knobs::current();
    let sk = crate::statecraft::Knobs::current();
    let scene = Scene::new(game, me);
    let r = regard(game, me, other, &scene, memory);
    let (m, t) = (
        scene.power(game, me).score(&sk).max(1),
        scene.power(game, other).score(&sk),
    );
    // Losing is a wish for peace, far ahead is not.
    let gap = (t - m) * k.gap / t.max(m);
    // The outclassed begs; the stronger finishes the other off.
    let fear = if t >= m {
        r.fear * k.fear_pct / 100
    } else {
        -r.fear * k.fear_pct / 200
    };
    let mood = crate::objective::temper(game.personality(me), game.difficulty()).modifier();
    let young = match game.war_age(me, other) {
        Some(age) if age < k.young_age => k.young,
        _ => 0,
    };
    weariness(game, me, other, memory)
        + gap
        + fear
        + r.friendship * k.friend_pct / 100
        + r.strategic
        + mood / k.mood_div
        - young
}

/// Whether a wizard of system B says yes to a peace `asker` offers (`diplomacy::rate`).
pub(crate) fn accepts(game: &Game, judge: PlayerId, asker: PlayerId, memory: &Memory) -> bool {
    game.treaty(judge, asker) != Treaty::War
        || peace_wish(game, judge, asker, memory) >= Knobs::current().accept
}

/// Whether `ai` asks `other` for peace now: at war, the wish is at the offer's mark and it has not
/// asked lately.
pub(crate) fn offer_due(ai: &Ai, game: &Game, other: PlayerId) -> bool {
    let k = Knobs::current();
    let memory = ai.memory.get();
    game.treaty(ai.player, other) == Treaty::War
        && memory
            .with(other)
            .is_none_or(|d| d.offered == 0 || game.turn >= d.offered + k.cool)
        && peace_wish(game, ai.player, other, &memory) >= k.offer
}

/// The wars it fights with the other wizards: peace is offered where the wish says so.
pub(crate) fn wars(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>, scene: &Scene) {
    for &w in &scene.others {
        // The human is asked by `approach_human`.
        if game.human() == Some(w) || !offer_due(ai, game, w) {
            continue;
        }
        if propose(ai, game, events, w, ProposalKind::Peace) {
            ai.memory.put().offered_peace(w, game.turn);
            remark(ai, game, events, w, Remark::WarWeary);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0, P1};
    use f4x_core::diplomacy::DiplomacyCommand;
    use f4x_core::game::Command;
    use f4x_core::rules::RealmId;
    use f4x_def::half_int::HalfInt;

    /// Where the east wizard has a city on the west island.
    const RIVAL: (u32, u32) = (3, 8);

    fn found(g: &mut Game, owner: PlayerId, x: u32, y: u32) {
        let race = g.content().rules.races.id_numbered(8);
        g.apply(Command::FoundCity {
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        found(&mut g, P0, 10, 10);
        found(&mut g, P1, RIVAL.0, RIVAL.1);
        for p in [P0, P1] {
            g.init_wizard(p, &[(RealmId::new(3 /* life */), 3)], &[], None)
                .expect("a wizard");
        }
        spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            95, /* high_elf_spearmen */
            army0,
        );
        spawn(&mut g, P1, at(RIVAL.0, RIVAL.1), 95, army1);
        g.make_contact(P0, P1);
        g.explore(P0, at(RIVAL.0, RIVAL.1), HalfInt::from_raw(3));
        g.explore(P1, at(COAST_CITY.0, COAST_CITY.1), HalfInt::from_raw(3));
        g.turn = 100;
        let events = g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        assert!(!events.iter().any(|e| matches!(e, Event::Rejected(_))));
        g.turn = turn;
        g
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

    fn told(g: &Game, events: &[Event]) -> Memory {
        let mut m = Memory::default();
        m.notice(P0, g, events);
        m
    }

    #[test]
    fn weariness_rises_with_losses_falls_with_wins_and_is_never_below_zero() {
        let k = Knobs::current();
        let g = war(8, 8, 110);
        let m = told(&g, &[battle_lost(), battle_lost(), city_lost()]);
        let lost = weariness(&g, P0, P1, &m);
        assert_eq!(lost, 2 * k.battle_lost + k.city_lost);
        let m = told(
            &g,
            &[battle_lost(), battle_won(), battle_won(), city_taken()],
        );
        assert_eq!(
            weariness(&g, P0, P1, &m),
            0,
            "wins bring it down to 0 and no lower"
        );
        let m = told(&g, &[city_lost(), city_taken(), battle_lost()]);
        assert_eq!(weariness(&g, P0, P1, &m), k.battle_lost);
    }

    #[test]
    fn a_quiet_war_wears_down_and_a_clash_starts_the_count_again() {
        let k = Knobs::current();
        let mut g = war(8, 8, 110);
        let empty = Memory::default();
        let w10 = weariness(&g, P0, P1, &empty);
        g.turn = 160;
        let w60 = weariness(&g, P0, P1, &empty);
        assert!(w60 > w10, "{w10} then {w60}");
        assert_eq!(w60, 60 * k.drift_per_ten / 10);
        // A battle at turn 160 puts the quiet back to zero; only what it moved remains.
        let m = told(&g, &[battle_lost()]);
        assert_eq!(weariness(&g, P0, P1, &m), k.battle_lost);
        // A memory of an earlier war does not count in a new one.
        let mut peace = told(&g, &[city_lost()]);
        g.declare_peace(P0, P1);
        assert_eq!(weariness(&g, P0, P1, &peace), 0, "no war, no weariness");
        g.turn = 170;
        g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        g.turn = 175;
        assert_eq!(
            weariness(&g, P0, P1, &peace),
            5 * k.drift_per_ten / 10,
            "the new war starts at nothing"
        );
        peace.notice(P0, &g, &[battle_lost()]);
        assert_eq!(weariness(&g, P0, P1, &peace), k.battle_lost);
    }

    #[test]
    fn the_wizard_that_is_losing_wants_peace_more_than_the_one_that_is_winning() {
        let g = war(1, 30, 140);
        let m0 = told(&g, &[battle_lost(), city_lost()]);
        let m1 = Memory::default();
        let loser = peace_wish(&g, P0, P1, &m0);
        let winner = peace_wish(&g, P1, P0, &m1);
        assert!(loser > winner, "loser {loser}, winner {winner}");
        assert!(winner < 0, "the winner does not want it: {winner}");
    }

    #[test]
    fn fear_is_for_the_one_outclassed_and_makes_the_stronger_side_hold_out() {
        let g = war(1, 30, 140);
        let scene0 = Scene::new(&g, P0);
        let scene1 = Scene::new(&g, P1);
        let weak = regard(&g, P0, P1, &scene0, &Memory::default());
        let strong = regard(&g, P1, P0, &scene1, &Memory::default());
        assert!(weak.fear > 0, "near and strong: {weak:?}");
        assert_eq!(strong.fear, 0, "a weak neighbour is no fear: {strong:?}");
        // The same wizard with nothing to fear wants the peace less.
        let even = war(30, 30, 140);
        let calm = peace_wish(&even, P0, P1, &Memory::default());
        let afraid = peace_wish(&g, P0, P1, &Memory::default());
        assert!(afraid > calm, "afraid {afraid}, calm {calm}");
    }

    #[test]
    fn a_young_war_is_not_ended() {
        let young = war(1, 30, 103);
        let old = war(1, 30, 150);
        let m = Memory::default();
        assert!(peace_wish(&young, P0, P1, &m) < peace_wish(&old, P0, P1, &m));
    }

    #[test]
    fn a_stalled_war_between_equals_drifts_to_an_offer_and_the_cool_down_holds() {
        let k = Knobs::current();
        let mut g = war(10, 10, 110);
        let mut ai = Ai::new(P0);
        ai.set_system(crate::System::B);
        let mut turn = 110;
        // Until the wish reaches the mark nothing is offered.
        while peace_wish(&g, P0, P1, &ai.memory.get()) < k.offer {
            assert!(!offer_due(&ai, &g, P1));
            turn += 5;
            g.turn = turn;
            assert!(turn < 2000, "a quiet war reaches the mark in the end");
        }
        assert!(offer_due(&ai, &g, P1));
        let scene = Scene::new(&g, P0);
        let mut events = Vec::new();
        wars(&mut ai, &mut g, &mut events, &scene);
        assert_eq!(ai.memory.get().with(P1).map(|d| d.offered), Some(turn));
        // It does not ask again at once, and asks again when the cool-down is over.
        g.turn = turn + 1;
        assert!(!offer_due(&ai, &g, P1), "cool-down");
        g.turn = turn + k.cool;
        assert!(offer_due(&ai, &g, P1) || g.treaty(P0, P1) != Treaty::War);
    }

    #[test]
    fn a_war_that_is_going_well_is_not_offered_peace_and_a_losing_one_is_accepted() {
        let g = war(1, 30, 160);
        let m = told(
            &g,
            &[battle_lost(), city_lost(), city_lost(), battle_lost()],
        );
        assert!(accepts(&g, P0, P1, &m), "the loser says yes");
        let winner = Memory::default();
        assert!(!accepts(&g, P1, P0, &winner), "the winner says no");
        // And at peace everyone accepts what is not a war's peace.
        let mut at_peace = war(1, 30, 160);
        at_peace.declare_peace(P0, P1);
        assert!(accepts(&at_peace, P1, P0, &winner));
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
        let g = war(8, 8, 110);
        let mut m = Memory::default();
        m.notice(
            P0,
            &g,
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
        let all = k.ruth_raze + k.ruth_war + k.ruth_broken;
        assert_eq!(m.ruthlessness(P1, 110, k.ruth_half), all);
        assert_eq!(m.ruthlessness(P1, 110 + k.ruth_half, k.ruth_half), all / 2);
        assert!(m.ruthlessness(P1, 110 + 40 * k.ruth_half, k.ruth_half) <= 1);
        // The acts of a wizard that has not been met are not its business.
        let stranger = game(COAST_CITY, 8 /* high_men */);
        let mut m = Memory::default();
        m.notice(P0, &stranger, &[razed()]);
        assert_eq!(m.ruthlessness(P1, stranger.turn, k.ruth_half), 0);
    }

    #[test]
    fn a_ruthless_neighbour_is_feared_more() {
        let g = war(10, 10, 140);
        let scene = Scene::new(&g, P0);
        let calm = regard(&g, P0, P1, &scene, &Memory::default());
        let mut m = Memory::default();
        m.notice(P0, &g, &[razed(), razed()]);
        let ruthless = regard(&g, P0, P1, &scene, &m);
        assert!(ruthless.fear > calm.fear, "{ruthless:?} against {calm:?}");
    }

    use crate::{answer_proposals, Diplomats};

    /// `asker` proposes peace to `judge`, who answers through the game's table; whether the war is over.
    fn asked_for_peace(g: &mut Game, asker: PlayerId, judge: PlayerId, table: &Diplomats) -> bool {
        g.apply(Command::Diplomacy(DiplomacyCommand::Propose {
            from: asker,
            to: judge,
            kind: ProposalKind::Peace,
        }));
        answer_proposals(g, judge, table);
        g.treaty(asker, judge) != Treaty::War
    }

    #[test]
    fn a_peace_is_answered_by_the_judges_system_and_memory_through_the_table() {
        // P0 is far behind, P1 far ahead.
        // A judge on system B that is winning answers by the score and says no.
        let mut b = war(1, 30, 160);
        let table = Diplomats::new();
        let mut p1 = Ai::new(P1);
        p1.join(&table);
        p1.set_system(crate::System::B);
        assert!(!asked_for_peace(&mut b, P0, P1, &table), "B says no");
        // And P1 asking P0, which has lost a lot and is on B, is taken.
        let mut c = war(1, 30, 160);
        let table = Diplomats::new();
        let mut p0 = Ai::new(P0);
        p0.join(&table);
        p0.set_system(crate::System::B);
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
        let mut g = war(1, 30, 200);
        let mut ai = Ai::new(P0);
        ai.set_system(crate::System::B);
        ai.notice(
            &g,
            &[battle_lost(), city_lost(), city_lost(), battle_lost()],
        );
        assert!(peace_wish(&g, P0, P1, &ai.memory.get()) >= k.offer);
        let mut events = Vec::new();
        crate::statecraft::run(&mut ai, &mut g, &mut events);
        assert_eq!(
            ai.memory.get().with(P1).map(|d| d.offered),
            Some(200),
            "B asked"
        );
        assert!(
            ai.statecraft.peace_offered.is_empty(),
            "A's list is not used"
        );
    }
}
