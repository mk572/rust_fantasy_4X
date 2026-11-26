//! The AI's statecraft (task 510): it weighs what it sees of the other players and plans its
//! diplomacy, for the wizards and the neutral races alike (`PlayerView::is_diplomat`).
//!
//! Six parts, each a function of what a human could see (`docs/design.md` section 8: the army
//! scores of the Astrologer, the cities seen, the treaties and the proposals):
//!
//! 1. [`dominant_war`]: war on a neighbour it is far ahead of, before the relation sours.
//! 2. [`appease`]: a pact, a gift or a spell for a much stronger neighbour near it, and no war on
//!    it until it has grown.
//! 3. [`coalition`]: pacts and alliances with the other wizards against the one that leads.
//! 4. [`wars`]: peace for a war that is lost, and for one of two fronts; [`peace_ok`] is the
//!    other side of it, a peace not given to a wizard that is losing a war of its own making
//!    before the gain is spent.
//! 5. Trades are by worth (`crate::trade`); [`arms_enemy`]
//!    refuses the ones that arm an enemy.
//! 6. Remarks (`DiplomacyCommand::Remark`: the human is told why) and memory ([`affinity`]
//!    counts a broken treaty against the wizard who broke it).
//!
//! **Factors and self-interest.** [`affinity`] is the modest bonus (fame, a Charismatic retort,
//! the same race, shared books) that eases a pact, an alliance or a peace; it is balanced against
//! the AI's own interest ([`self_interest`]: a friendly neighbour it is far ahead of is still a
//! target, the leader is still left to the others). Every number is in [`Knobs`] and can be set
//! from the environment for a soak (`AIK_DIPLO_<NAME>`, see `crate::knob`).

use f4x_core::diplomacy::{army_strength, DiplomacyCommand, ProposalKind, Remark, Treaty};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::rules::RaceId;
use f4x_core::wizard::magic::Retort;
use f4x_core::world::{Command, Event, World};

use crate::memory::Diplomats;
use crate::Ai;
use f4x_master::Game;

/// Every number of the statecraft in one place.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Knobs {
    /// Switches (1 on, 0 off), to measure a part alone in a soak (`AIK_DIPLO_<NAME>`): the factors and
    /// self-interest of the willingness, and the arms-enemy refusal.
    pub on_factors: bool,
    pub on_arms: bool,
    /// A city's worth in army points, to weigh cities against armies.
    pub city_power: i32,
    /// Self-interest: a neighbour with this percent of the AI's army and of its cities or less, and at least
    /// this many cities of its own, is one the AI means to attack.
    pub dominant_army: i32,
    pub dominant_cities: i32,
    pub dominant_min_cities: usize,
    /// The leader holds this percent of the next best's power; two that fear the same leader get on better
    /// by `coalition_bonus`.
    pub leader_margin: i32,
    pub coalition_bonus: i32,
    /// The factors, each added to the willingness for a pact, an alliance or a peace.
    pub fame_per_point: i32,
    pub fame_cap: i32,
    pub charismatic: i32,
    pub same_race: i32,
    pub shared_realm: i32,
    pub shared_cap: i32,
    pub opposed_realm: i32,
    pub broken_memory: i32,
    /// Self-interest, taken off the willingness: toward a neighbour it means to attack, and
    /// toward the leader.
    pub interest_target: i32,
    pub interest_leader: i32,
}

impl Knobs {
    pub(crate) fn current() -> Self {
        let k =
            |name: &str, default: i32| crate::knob(&format!("DIPLO_{name}"), default as f32) as i32;
        let on = |name: &str| k(name, 1) != 0;
        Self {
            on_factors: on("ON_FACTORS"),
            on_arms: on("ON_ARMS"),
            city_power: k("CITY_POWER", 100),
            dominant_army: k("DOMINANT_ARMY", 160),
            dominant_cities: k("DOMINANT_CITIES", 150),
            dominant_min_cities: k("DOMINANT_MIN_CITIES", 3) as usize,
            leader_margin: k("LEADER_MARGIN", 130),
            coalition_bonus: k("COALITION_BONUS", 30),
            fame_per_point: k("FAME_PER_POINT", 25).max(1),
            fame_cap: k("FAME_CAP", 8),
            charismatic: k("CHARISMATIC", 6),
            same_race: k("SAME_RACE", 8),
            shared_realm: k("SHARED_REALM", 3),
            shared_cap: k("SHARED_CAP", 6),
            opposed_realm: k("OPPOSED_REALM", 4),
            broken_memory: k("BROKEN_MEMORY", 15),
            interest_target: k("INTEREST_TARGET", 25),
            interest_leader: k("INTEREST_LEADER", 15),
        }
    }
}

/// Army and cities of one player, as the observer sees them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Power {
    pub army: i32,
    pub cities: i32,
}

impl Power {
    pub(crate) fn score(self, k: &Knobs) -> i32 {
        self.army + k.city_power * self.cities
    }
}

/// What the observer sees of the players: its own cities and the cities of the others it has
/// seen, and every army (the Astrologer shows the scores).
pub(crate) struct Scene {
    pub(crate) me: PlayerId,
    known: Vec<(PlayerId, f4x_core::map::Position)>,
    pub(crate) mine: Vec<f4x_core::map::Position>,
    /// The living diplomats the observer has met.
    pub(crate) others: Vec<PlayerId>,
}

impl Scene {
    pub(crate) fn new(world: &World, me: PlayerId) -> Self {
        let view = PlayerView::new(world, me);
        let others = world
            .player_ids()
            .into_iter()
            .filter(|&p| p != me && view.is_diplomat(p))
            .filter(|&p| world.have_met(me, p))
            .filter(|&p| world.player(p).is_some_and(|q| !q.is_defeated()))
            .collect();
        Self {
            me,
            known: view
                .known_cities()
                .into_iter()
                .map(|(_, owner, pos)| (owner, pos))
                .collect(),
            mine: view.my_cities().into_iter().map(|(_, pos)| pos).collect(),
            others,
        }
    }

    pub(crate) fn power(&self, world: &World, who: PlayerId) -> Power {
        let cities = if who == self.me {
            self.mine.len()
        } else {
            self.known.iter().filter(|(o, _)| *o == who).count()
        };
        Power {
            army: army_strength(world, who),
            cities: cities as i32,
        }
    }

    pub(crate) fn cities_of(&self, who: PlayerId) -> Vec<f4x_core::map::Position> {
        self.known
            .iter()
            .filter(|(o, _)| *o == who)
            .map(|&(_, p)| p)
            .collect()
    }
}

/// The wizard that leads the game as `observer` sees it: casting the Spell of Mastery, or far
/// ahead of the next best in army and cities. `None` when the observer leads or nobody does.
pub(crate) fn leader_of(world: &World, observer: PlayerId) -> Option<PlayerId> {
    let k = Knobs::current();
    let view = PlayerView::new(world, observer);
    if let Some((caster, _)) = view
        .rival_mastery_casters()
        .into_iter()
        .find(|&(p, _)| world.have_met(observer, p))
    {
        return Some(caster);
    }
    let scene = Scene::new(world, observer);
    let mut scores: Vec<(i32, PlayerId)> = scene
        .others
        .iter()
        .copied()
        .chain(std::iter::once(observer))
        .map(|p| (scene.power(world, p).score(&k), p))
        .collect();
    scores.sort_by_key(|&(s, p)| (std::cmp::Reverse(s), p.0));
    let (best, leader) = *scores.first()?;
    let second = scores.get(1).map_or(0, |&(s, _)| s);
    (leader != observer && best * 100 >= k.leader_margin * second.max(1)).then_some(leader)
}

/// How far ahead of the best rival the observer stands, in the power the statecraft scores (army
/// and cities, as [`leader_of`] reads them): the observer's score over the best other's, 1 when
/// level or when it has met nobody. Above 1 the observer leads (task 697: a lead lowers the edge an
/// army wants before it marches).
pub(crate) fn lead_of(world: &World, observer: PlayerId) -> f32 {
    let k = Knobs::current();
    let scene = Scene::new(world, observer);
    let score = |p: PlayerId| scene.power(world, p).score(&k).max(1) as f32;
    scene
        .others
        .iter()
        .map(|&p| score(observer) / score(p))
        .fold(None, |best: Option<f32>, x| {
            Some(best.map_or(x, |b| b.min(x)))
        })
        .unwrap_or(1.0)
}

/// The race of a player: a neutral player's own, a wizard's the race of its first city.
fn race_of(world: &World, who: PlayerId) -> Option<RaceId> {
    world.race_of_player(who).or_else(|| {
        world
            .economy()
            .cities()
            .into_iter()
            .find(|(_, c, _)| c.owner == who)
            .and_then(|(e, _, _)| world.city_race(e))
    })
}

/// The modest bonus `me` gives `other`'s standing, to its willingness for a pact, an alliance, a
/// trade or a peace: the other's fame, a Charismatic retort, the same race, books in the same
/// realms, less a treaty the other broke with it. None of these decides alone.
pub(crate) fn affinity(world: &World, minds: &Diplomats, me: PlayerId, other: PlayerId) -> i32 {
    let k = Knobs::current();
    let mut a = 0;
    if let Some(w) = world.wizard(other) {
        a += (w.fame() / k.fame_per_point).clamp(0, k.fame_cap);
        if w.retorts().contains(&Retort::new(16) /* charismatic */) {
            a += k.charismatic;
        }
    }
    if let (Some(x), Some(y)) = (race_of(world, me), race_of(world, other)) {
        if x == y {
            a += k.same_race;
        }
    }
    if let (Some(x), Some(y)) = (world.wizard(me), world.wizard(other)) {
        let books = |w: &f4x_core::wizard::entity::Wizard<'_>| w.books();
        let (mine, theirs) = (books(&x), books(&y));
        let shared = mine
            .iter()
            .filter(|(r, n)| **n >= 2 && theirs.get(r).is_some_and(|m| *m >= 2))
            .count() as i32;
        a += (shared * k.shared_realm).min(k.shared_cap);
        // Life and Death do not get on.
        let opposed = |a: u16, b: u16| {
            let has = |m: &std::collections::BTreeMap<f4x_core::rules::RealmId, u8>, r: u16| {
                m.get(&f4x_core::rules::RealmId::new(r))
                    .is_some_and(|n| *n >= 2)
            };
            (has(&mine, a) && has(&theirs, b)) || (has(&mine, b) && has(&theirs, a))
        };
        if opposed(3 /* life */, 4 /* death */) {
            a -= k.opposed_realm;
        }
    }
    // The memory of a broken treaty.
    if minds.opinion(world, me, other).broken {
        a -= k.broken_memory;
    }
    a
}

/// What stands against a friendly answer to `asker` for `judge`: the asker is a neighbour the
/// judge is far ahead of and means to attack, or leads the game.
pub(crate) fn self_interest(world: &World, judge: PlayerId, asker: PlayerId) -> i32 {
    let k = Knobs::current();
    let mut against = 0;
    if dominates(world, judge, asker, &k) {
        against += k.interest_target;
    }
    if leader_of(world, judge) == Some(asker) {
        against += k.interest_leader;
    }
    against
}

/// The sum of the factors and the self-interest, in the willingness of `judge` toward `asker`.
pub(crate) fn goodwill(
    world: &World,
    minds: &Diplomats,
    judge: PlayerId,
    asker: PlayerId,
    kind: ProposalKind,
) -> i32 {
    let k = Knobs::current();
    if !k.on_factors {
        return 0;
    }
    let mut g = affinity(world, minds, judge, asker) - self_interest(world, judge, asker);
    if matches!(kind, ProposalKind::Pact | ProposalKind::Alliance) {
        // Two wizards that fear the same leader get on better.
        if let Some(leader) = leader_of(world, judge) {
            if leader != asker && leader_of(world, asker) == Some(leader) {
                g += k.coalition_bonus;
            }
        }
    }
    g
}

/// Whether `me` is far ahead of `other` in army and cities.
fn dominates(world: &World, me: PlayerId, other: PlayerId, k: &Knobs) -> bool {
    let scene = Scene::new(world, me);
    let (mine, theirs) = (scene.power(world, me), scene.power(world, other));
    theirs.cities > 0
        && mine.cities as usize >= k.dominant_min_cities
        && mine.army * 100 >= k.dominant_army * theirs.army.max(1)
        && mine.cities * 100 >= k.dominant_cities * theirs.cities
}

/// Whether a trade with `asker` would arm an enemy of `judge`: the asker is at war with it or
/// with one of its allies, or leads the game.
pub(crate) fn arms_enemy(world: &World, judge: PlayerId, asker: PlayerId) -> bool {
    Knobs::current().on_arms
        && (world.treaty(judge, asker) == Treaty::War
            || world.player_ids().into_iter().any(|p| {
                p != judge
                    && p != asker
                    && world.treaty(judge, p) == Treaty::Alliance
                    && world.treaty(asker, p) == Treaty::War
            })
            || leader_of(world, judge) == Some(asker))
}

/// A proposal `me` makes to `to`, which `to` answers at once when it is an AI. Whether it was made.
pub(crate) fn propose(
    ai: &Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    to: PlayerId,
    kind: ProposalKind,
) -> bool {
    let me = ai.player;
    if game
        .diplomacy
        .proposals()
        .iter()
        .any(|p| p.from == me && p.to == to)
    {
        return false;
    }
    let made = ai
        .send(
            game,
            Command::Diplomacy(DiplomacyCommand::Propose { from: me, to, kind }),
            events,
        )
        .is_ok();
    if made && game.human() != Some(to) {
        events.extend(crate::settle(game, me, to, &ai.diplomats));
    }
    made
}

pub(crate) fn remark(ai: &Ai, game: &mut Game, events: &mut Vec<Event>, to: PlayerId, why: Remark) {
    ai.order(
        game,
        Command::Diplomacy(DiplomacyCommand::Remark {
            from: ai.player,
            to,
            why,
        }),
        events,
    );
}

/// Once a turn, after the talk with the other wizards.
pub(crate) fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    {
        let view = PlayerView::new(game, me);
        if view.is_defeated() || view.wizard().is_some_and(|w| w.is_banished()) {
            return;
        }
    }
    let scene = Scene::new(game, me);
    // The peace score offers peace where it says so (`diplo_b.rs`); the enemy and the friends come from the
    // three values (`diplo_b_stance.rs`).
    crate::diplo_b::wars(ai, game, events, &scene);
    crate::diplo_b_stance::run(ai, game, events, &scene);
}

/// A gift of gold to `to`, once in `gift_cool` turns, from a treasury that has the gold to spare.
pub(crate) fn gift_gold(
    ai: &Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    to: PlayerId,
    why: Remark,
) -> bool {
    let me = ai.player;
    let k = crate::diplo_b::Knobs::current();
    let gold = game.player(me).map_or(0, |p| p.treasury().gold.floor());
    if !ai
        .memory
        .get()
        .due(to, crate::memory::Did::Gift, game.turn, k.gift_cool)
        || gold < 100
    {
        return false;
    }
    let given = ai
        .send(
            game,
            Command::Diplomacy(DiplomacyCommand::GiveGold {
                from: me,
                to,
                amount: 50,
            }),
            events,
        )
        .is_ok();
    if given {
        ai.memory.put().did(to, crate::memory::Did::Gift, game.turn);
        remark(ai, game, events, to, why);
    }
    given
}

/// Whether `me` could get an army to a city of `target`: one a fighter can walk to, or one on another
/// land mass that a ship could land beside, from a port of its own.
pub(crate) fn can_attack(world: &World, me: PlayerId, scene: &Scene, target: PlayerId) -> bool {
    // A city to march on that a unit can reach.
    let reachable = {
        let view = PlayerView::new(world, me);
        let fighter = view
            .my_units()
            .into_iter()
            .find(|&u| crate::view::is_fighter(&view, u));
        fighter.is_some_and(|u| {
            scene
                .cities_of(target)
                .into_iter()
                .any(|c| view.can_reach(&[u], c))
        })
    };
    // Or a city on another land mass that a ship could land beside, from a port of its own.
    let by_sea = !reachable && {
        let view = PlayerView::new(world, me);
        let port = view
            .my_cities()
            .into_iter()
            .any(|(_, p)| crate::view::is_coastal(&view, p));
        port && scene
            .cities_of(target)
            .into_iter()
            .any(|c| crate::naval::landing(&view, c).is_some())
    };
    reachable || by_sea
}

#[cfg(test)]
mod tests;
