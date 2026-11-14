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

use f4x_core::diplomacy::{
    army_strength, DiplomacyCommand, ProposalKind, Remark, Treaty, ALLIANCE_RELATION, PACT_RELATION,
};
use f4x_core::game::{Command, Event, Game};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::rules::RaceId;
use f4x_core::wizard::magic::Retort;

use crate::Ai;

/// What the AI keeps between turns of its statecraft.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct State {
    /// The neighbours it keeps out of the way of, for being much stronger.
    pub appeased: Vec<PlayerId>,
    /// The turn of the last gift to each of them.
    pub gifted: Vec<(PlayerId, u32)>,
    /// What each war it fights stood at when it began.
    pub wars: Vec<WarNote>,
    /// The turn of the last offer of peace to each wizard.
    #[serde(default)]
    pub peace_offered: Vec<(PlayerId, u32)>,
    /// The alliances it asked for against a leader: the ally, the leader and the turn it asked.
    #[serde(default)]
    pub coalitions: Vec<(PlayerId, PlayerId, u32)>,
}

/// The cities each side held when the war was first seen.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub(crate) struct WarNote {
    pub with: PlayerId,
    pub mine: usize,
    pub theirs: usize,
}

/// Every number of the statecraft in one place.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Knobs {
    /// Switches (1 on, 0 off) for each part, to measure it alone in a soak (`AIK_DIPLO_<NAME>`):
    /// the dominant war, appeasing, the coalition, the wars' peace offers, the peace gate in
    /// answering, the factors and self-interest of the willingness, and the arms-enemy refusal.
    pub on_dominant: bool,
    pub on_appease: bool,
    pub on_coalition: bool,
    pub on_wars: bool,
    pub on_peace_gate: bool,
    pub on_factors: bool,
    pub on_arms: bool,
    /// What an alliance made against a leader does when the leader no longer leads (task 597):
    /// 0 it stays and only the asking stops, 1 it is broken (the oath breaker's penalty).
    pub coalition_end: i32,
    /// A city's worth in army points, to weigh cities against armies.
    pub city_power: i32,
    /// Dominant war: percent of the neighbour's army, and of its cities, the AI must hold.
    pub dominant_army: i32,
    pub dominant_cities: i32,
    /// At least this many cities of its own, and no war before this turn.
    pub dominant_min_cities: usize,
    pub dominant_from_turn: u32,
    /// The chance in a hundred, a turn, that it declares (before the factors).
    pub dominant_base: i32,
    /// Added for a fighting temper and objective, and when the target leads the game.
    pub dominant_fighter: i32,
    pub dominant_leader: i32,
    /// Appeasing: a neighbour with this percent of the AI's army or more, within this distance.
    pub appease_army: i32,
    pub appease_release: i32,
    pub appease_distance: u32,
    /// Gold in the treasury before it gives some, the turns between gifts.
    pub appease_gold: i32,
    pub appease_every: u32,
    /// One turn in this many it offers a pact to a neighbour it appeases.
    pub appease_pact_one_in: u32,
    /// A coalition: the leader holds this percent of the next best's power, and one turn in this
    /// many a wizard asks another to join.
    pub leader_margin: i32,
    pub coalition_one_in: u32,
    pub coalition_bonus: i32,
    /// Wars: a war this old, or older, is one whose gain is spent; ahead means this percent.
    pub spent_after: u32,
    pub ahead: i32,
    /// One turn in this many a losing wizard offers peace, and at least this many turns between
    /// two offers to the same wizard.
    pub peace_one_in: u32,
    pub peace_every: u32,
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
            on_dominant: on("ON_DOMINANT"),
            on_appease: on("ON_APPEASE"),
            on_coalition: on("ON_COALITION"),
            on_wars: on("ON_WARS"),
            on_peace_gate: on("ON_PEACE_GATE"),
            on_factors: on("ON_FACTORS"),
            on_arms: on("ON_ARMS"),
            coalition_end: k("COALITION_END", 0),
            city_power: k("CITY_POWER", 100),
            dominant_army: k("DOMINANT_ARMY", 160),
            dominant_cities: k("DOMINANT_CITIES", 150),
            dominant_min_cities: k("DOMINANT_MIN_CITIES", 3) as usize,
            dominant_from_turn: k("DOMINANT_FROM_TURN", 40) as u32,
            dominant_base: k("DOMINANT_BASE", 12),
            dominant_fighter: k("DOMINANT_FIGHTER", 12),
            dominant_leader: k("DOMINANT_LEADER", 10),
            appease_army: k("APPEASE_ARMY", 150),
            appease_release: k("APPEASE_RELEASE", 110),
            appease_distance: k("APPEASE_DISTANCE", 16) as u32,
            appease_gold: k("APPEASE_GOLD", 150),
            appease_every: k("APPEASE_EVERY", 20) as u32,
            appease_pact_one_in: k("APPEASE_PACT_ONE_IN", 4) as u32,
            leader_margin: k("LEADER_MARGIN", 130),
            coalition_one_in: k("COALITION_ONE_IN", 6) as u32,
            coalition_bonus: k("COALITION_BONUS", 30),
            spent_after: k("SPENT_AFTER", 80) as u32,
            ahead: k("AHEAD", 130),
            peace_one_in: k("PEACE_ONE_IN", 3) as u32,
            peace_every: k("PEACE_EVERY", 25) as u32,
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
    fn score(self, k: &Knobs) -> i32 {
        self.army + k.city_power * self.cities
    }
}

/// What the observer sees of the players: its own cities and the cities of the others it has
/// seen, and every army (the Astrologer shows the scores).
struct Scene {
    me: PlayerId,
    known: Vec<(PlayerId, f4x_core::map::Position)>,
    mine: Vec<f4x_core::map::Position>,
    /// The living diplomats the observer has met.
    others: Vec<PlayerId>,
}

impl Scene {
    fn new(game: &Game, me: PlayerId) -> Self {
        let view = PlayerView::new(game, me);
        let others = game
            .player_ids()
            .into_iter()
            .filter(|&p| p != me && view.is_diplomat(p))
            .filter(|&p| game.have_met(me, p))
            .filter(|&p| game.player(p).is_some_and(|q| !q.is_defeated()))
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

    fn power(&self, game: &Game, who: PlayerId) -> Power {
        let cities = if who == self.me {
            self.mine.len()
        } else {
            self.known.iter().filter(|(o, _)| *o == who).count()
        };
        Power {
            army: army_strength(game, who),
            cities: cities as i32,
        }
    }

    fn cities_of(&self, who: PlayerId) -> Vec<f4x_core::map::Position> {
        self.known
            .iter()
            .filter(|(o, _)| *o == who)
            .map(|&(_, p)| p)
            .collect()
    }
}

/// The wizard that leads the game as `observer` sees it: casting the Spell of Mastery, or far
/// ahead of the next best in army and cities. `None` when the observer leads or nobody does.
pub(crate) fn leader_of(game: &Game, observer: PlayerId) -> Option<PlayerId> {
    let k = Knobs::current();
    let view = PlayerView::new(game, observer);
    if let Some((caster, _)) = view
        .rival_mastery_casters()
        .into_iter()
        .find(|&(p, _)| game.have_met(observer, p))
    {
        return Some(caster);
    }
    let scene = Scene::new(game, observer);
    let mut scores: Vec<(i32, PlayerId)> = scene
        .others
        .iter()
        .copied()
        .chain(std::iter::once(observer))
        .map(|p| (scene.power(game, p).score(&k), p))
        .collect();
    scores.sort_by_key(|&(s, p)| (std::cmp::Reverse(s), p.0));
    let (best, leader) = *scores.first()?;
    let second = scores.get(1).map_or(0, |&(s, _)| s);
    (leader != observer && best * 100 >= k.leader_margin * second.max(1)).then_some(leader)
}

/// The race of a player: a neutral player's own, a wizard's the race of its first city.
fn race_of(game: &Game, who: PlayerId) -> Option<RaceId> {
    game.race_of_player(who).or_else(|| {
        game.economy()
            .cities()
            .into_iter()
            .find(|(_, c, _)| c.owner == who)
            .and_then(|(e, _, _)| game.city_race(e))
    })
}

/// The modest bonus `me` gives `other`'s standing, to its willingness for a pact, an alliance, a
/// trade or a peace: the other's fame, a Charismatic retort, the same race, books in the same
/// realms, less a treaty the other broke with it. None of these decides alone.
pub(crate) fn affinity(game: &Game, me: PlayerId, other: PlayerId) -> i32 {
    let k = Knobs::current();
    let mut a = 0;
    if let Some(w) = game.wizard(other) {
        a += (w.fame() / k.fame_per_point).clamp(0, k.fame_cap);
        if w.retorts().contains(&Retort::new(16) /* charismatic */) {
            a += k.charismatic;
        }
    }
    if let (Some(x), Some(y)) = (race_of(game, me), race_of(game, other)) {
        if x == y {
            a += k.same_race;
        }
    }
    if let (Some(x), Some(y)) = (game.wizard(me), game.wizard(other)) {
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
    if game
        .diplomacy
        .stance(me, other)
        .is_some_and(|s| s.broken.is_some())
    {
        a -= k.broken_memory;
    }
    a
}

/// What stands against a friendly answer to `asker` for `judge`: the asker is a neighbour the
/// judge is far ahead of and means to attack, or leads the game.
pub(crate) fn self_interest(game: &Game, judge: PlayerId, asker: PlayerId) -> i32 {
    let k = Knobs::current();
    let mut against = 0;
    if dominates(game, judge, asker, &k) {
        against += k.interest_target;
    }
    if leader_of(game, judge) == Some(asker) {
        against += k.interest_leader;
    }
    against
}

/// The sum of the factors and the self-interest, in the willingness of `judge` toward `asker`.
pub(crate) fn goodwill(game: &Game, judge: PlayerId, asker: PlayerId, kind: ProposalKind) -> i32 {
    let k = Knobs::current();
    if !k.on_factors {
        return 0;
    }
    let mut g = affinity(game, judge, asker) - self_interest(game, judge, asker);
    if matches!(kind, ProposalKind::Pact | ProposalKind::Alliance) {
        // Two wizards that fear the same leader get on better.
        if let Some(leader) = leader_of(game, judge) {
            if leader != asker && leader_of(game, asker) == Some(leader) {
                g += k.coalition_bonus;
            }
        }
    }
    g
}

/// Whether `me` is far ahead of `other` in army and cities.
fn dominates(game: &Game, me: PlayerId, other: PlayerId, k: &Knobs) -> bool {
    let scene = Scene::new(game, me);
    let (mine, theirs) = (scene.power(game, me), scene.power(game, other));
    theirs.cities > 0
        && mine.cities as usize >= k.dominant_min_cities
        && mine.army * 100 >= k.dominant_army * theirs.army.max(1)
        && mine.cities * 100 >= k.dominant_cities * theirs.cities
}

/// Whether `judge` would accept a peace `asker` asks for: not while it is far ahead in a war
/// that has not lasted long enough to spend its gain.
pub(crate) fn peace_ok(game: &Game, judge: PlayerId, asker: PlayerId) -> bool {
    let k = Knobs::current();
    if !k.on_peace_gate || game.treaty(judge, asker) != Treaty::War {
        return true;
    }
    // The gain is spent when the war has taken a city from the other, or has gone on long enough.
    if game
        .war_age(judge, asker)
        .is_none_or(|age| age >= k.spent_after)
        || game.war_gain(judge, asker).is_some_and(|gain| gain >= 1)
    {
        return true;
    }
    let scene = Scene::new(game, judge);
    let (mine, theirs) = (scene.power(game, judge), scene.power(game, asker));
    mine.score(&k) * 100 < k.ahead * theirs.score(&k).max(1)
}

/// Whether a trade with `asker` would arm an enemy of `judge`: the asker is at war with it or
/// with one of its allies, or leads the game.
pub(crate) fn arms_enemy(game: &Game, judge: PlayerId, asker: PlayerId) -> bool {
    Knobs::current().on_arms
        && (game.treaty(judge, asker) == Treaty::War
            || game.player_ids().into_iter().any(|p| {
                p != judge
                    && p != asker
                    && game.treaty(judge, p) == Treaty::Alliance
                    && game.treaty(asker, p) == Treaty::War
            })
            || leader_of(game, judge) == Some(asker))
}

/// A proposal `me` makes to `to`, which `to` answers at once when it is an AI. Whether it was made.
fn propose(
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
        events.extend(crate::answer_proposals(game, to));
    }
    made
}

fn remark(ai: &Ai, game: &mut Game, events: &mut Vec<Event>, to: PlayerId, why: Remark) {
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
    let k = Knobs::current();
    let scene = Scene::new(game, me);
    if k.on_wars {
        wars(ai, game, events, &k, &scene);
    }
    if k.on_appease {
        appease(ai, game, events, &k, &scene);
    }
    if k.on_coalition {
        coalition(ai, game, events, &k, &scene);
        end_coalitions(ai, game, events, &k);
    }
    if k.on_dominant {
        dominant_war(ai, game, events, &k, &scene);
    }
}

/// Part 4: the wars it fights. A war that is lost, or the more dangerous of two fronts, is
/// offered peace now and then.
fn wars(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>, k: &Knobs, scene: &Scene) {
    let me = ai.player;
    let at_war: Vec<PlayerId> = scene
        .others
        .iter()
        .copied()
        .filter(|&p| game.treaty(me, p) == Treaty::War)
        .collect();
    ai.statecraft.wars.retain(|n| at_war.contains(&n.with));
    for &w in &at_war {
        if !ai.statecraft.wars.iter().any(|n| n.with == w) {
            ai.statecraft.wars.push(WarNote {
                with: w,
                mine: scene.mine.len(),
                theirs: scene.cities_of(w).len(),
            });
        }
    }
    let danger = at_war
        .iter()
        .copied()
        .max_by_key(|&w| (scene.power(game, w).score(k), std::cmp::Reverse(w.0)));
    for w in at_war {
        // The human is asked by `approach_human`.
        let last = ai
            .statecraft
            .peace_offered
            .iter()
            .find(|(p, _)| *p == w)
            .map_or(0, |&(_, t)| t + k.peace_every);
        if game.human() == Some(w) || game.turn < last || game.roll(k.peace_one_in) != 1 {
            continue;
        }
        let Some(note) = ai.statecraft.wars.iter().find(|n| n.with == w).copied() else {
            continue;
        };
        let (mine, theirs) = (scene.power(game, me), scene.power(game, w));
        let lost = note.mine.saturating_sub(scene.mine.len());
        let taken = note.theirs.saturating_sub(scene.cities_of(w).len());
        let losing = (lost > taken && mine.army * 100 < 80 * theirs.army)
            || lost >= taken + 2
            || mine.score(k) * 100 < 60 * theirs.score(k);
        let two_fronts = scene
            .others
            .iter()
            .filter(|&&p| game.treaty(me, p) == Treaty::War)
            .count()
            >= 2
            && danger == Some(w);
        if (losing || two_fronts)
            && peace_ok(game, me, w)
            && propose(ai, game, events, w, ProposalKind::Peace)
        {
            ai.statecraft.peace_offered.retain(|(p, _)| *p != w);
            ai.statecraft.peace_offered.push((w, game.turn));
            remark(ai, game, events, w, Remark::WarWeary);
        }
    }
}

/// Part 2: a much stronger neighbour near it gets a pact, a gift or a spell, and no war.
fn appease(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>, k: &Knobs, scene: &Scene) {
    let me = ai.player;
    let mine = scene.power(game, me);
    for &r in &scene.others {
        let theirs = scene.power(game, r);
        let near = scene
            .mine
            .iter()
            .flat_map(|&a| scene.cities_of(r).into_iter().map(move |b| (a, b)))
            .filter_map(|(a, b)| PlayerView::new(game, me).distance(a, b))
            .min()
            .is_some_and(|d| d <= k.appease_distance);
        let stronger = theirs.army > 0 && theirs.army * 100 >= k.appease_army * mine.army.max(1);
        let held = ai.statecraft.appeased.contains(&r);
        if stronger && near && !held {
            ai.statecraft.appeased.push(r);
        } else if held && (theirs.army * 100 < k.appease_release * mine.army.max(1) || !near) {
            ai.statecraft.appeased.retain(|&p| p != r);
        }
    }
    let appeased = ai.statecraft.appeased.clone();
    for r in appeased {
        if !scene.others.contains(&r) || game.human() == Some(r) {
            continue;
        }
        let treaty = game.treaty(me, r);
        if treaty == Treaty::War {
            continue;
        }
        let relation = game.relation(me, r);
        if treaty == Treaty::None
            && relation > PACT_RELATION
            && game.roll(k.appease_pact_one_in) == 1
            && propose(ai, game, events, r, ProposalKind::Pact)
        {
            remark(ai, game, events, r, Remark::Appeasing);
            continue;
        }
        let last = ai
            .statecraft
            .gifted
            .iter()
            .find(|(p, _)| *p == r)
            .map_or(0, |&(_, t)| t);
        if game.turn < last + k.appease_every || relation > 60 {
            continue;
        }
        let gold = game.player(me).map_or(0, |p| p.treasury().gold.floor());
        let given = if gold >= k.appease_gold {
            let amount = 25 * (1 + game.roll(3) as i32);
            ai.send(
                game,
                Command::Diplomacy(DiplomacyCommand::GiveGold {
                    from: me,
                    to: r,
                    amount,
                }),
                events,
            )
            .is_ok()
        } else if let Some(give) = crate::trade::spell_to_give(game, me, r) {
            ai.send(
                game,
                Command::Diplomacy(DiplomacyCommand::GiveSpell {
                    from: me,
                    to: r,
                    spell: give,
                }),
                events,
            )
            .is_ok()
        } else {
            false
        };
        if given {
            ai.statecraft.gifted.retain(|(p, _)| *p != r);
            ai.statecraft.gifted.push((r, game.turn));
            remark(ai, game, events, r, Remark::Appeasing);
        }
    }
}

/// Part 3: with the leader in view, the weaker wizards pact and ally with each other. A wizard
/// it is not on good enough terms with for a pact gets a gift first (the same gold, no oftener
/// than an appeased neighbour's): a hostile wizard that is useful against the leader is still
/// dealt with.
fn coalition(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>, k: &Knobs, scene: &Scene) {
    let me = ai.player;
    let Some(leader) = leader_of(game, me) else {
        return;
    };
    for &w in &scene.others {
        if w == leader
            || game.human() == Some(w)
            || game.roll(k.coalition_one_in) != 1
            || game.treaty(me, w) == Treaty::War
        {
            continue;
        }
        let kind = match game.treaty(me, w) {
            Treaty::None => ProposalKind::Pact,
            Treaty::Pact => ProposalKind::Alliance,
            _ => continue,
        };
        let relation = game.relation(me, w);
        let floor = match kind {
            ProposalKind::Alliance => ALLIANCE_RELATION,
            _ => PACT_RELATION,
        };
        if relation > floor {
            if propose(ai, game, events, w, kind) {
                remark(ai, game, events, w, Remark::Coalition);
                if kind == ProposalKind::Alliance {
                    ai.statecraft.coalitions.retain(|&(a, _, _)| a != w);
                    ai.statecraft.coalitions.push((w, leader, game.turn));
                }
            }
        } else {
            gift_gold(ai, game, events, k, w, Remark::Coalition);
        }
    }
}

/// Part 3, the end: an alliance asked for against a leader is over when the leader no longer
/// leads. With `coalition_end` 1 it is broken (the allies part; the oath breaker's penalty
/// follows); with 0 it stays and nothing more is asked of it. A request that was never
/// accepted is forgotten after a few turns.
fn end_coalitions(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>, k: &Knobs) {
    let me = ai.player;
    let mut over: Vec<PlayerId> = Vec::new();
    ai.statecraft.coalitions.retain(|&(ally, leader, asked)| {
        let allied = game.treaty(me, ally) == Treaty::Alliance;
        if !allied {
            return game.turn <= asked + 3;
        }
        if leader_of(game, me) == Some(leader) {
            return true;
        }
        over.push(ally);
        false
    });
    if k.coalition_end == 1 {
        for ally in over {
            ai.send(
                game,
                Command::Diplomacy(DiplomacyCommand::BreakTreaty {
                    player: me,
                    with: ally,
                }),
                events,
            )
            .ok();
        }
    }
}

/// A gift of gold to win the wizard over, once in `appease_every` turns and only from a full
/// treasury. Whether it was given.
fn gift_gold(
    ai: &mut Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    k: &Knobs,
    to: PlayerId,
    why: Remark,
) -> bool {
    let me = ai.player;
    let last = ai
        .statecraft
        .gifted
        .iter()
        .find(|(p, _)| *p == to)
        .map_or(0, |&(_, t)| t);
    let gold = game.player(me).map_or(0, |p| p.treasury().gold.floor());
    if game.turn < last + k.appease_every || gold < k.appease_gold {
        return false;
    }
    let amount = 25 * (1 + game.roll(3) as i32);
    let given = ai
        .send(
            game,
            Command::Diplomacy(DiplomacyCommand::GiveGold {
                from: me,
                to,
                amount,
            }),
            events,
        )
        .is_ok();
    if given {
        ai.statecraft.gifted.retain(|(p, _)| *p != to);
        ai.statecraft.gifted.push((to, game.turn));
        remark(ai, game, events, to, why);
    }
    given
}

/// Part 1: war on a neighbour far ahead of in army and cities, with no treaty, once it can
/// march on one of its cities.
fn dominant_war(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>, k: &Knobs, scene: &Scene) {
    let me = ai.player;
    if game.turn < k.dominant_from_turn || ai.quarrel.is_some() {
        return;
    }
    // No war on two fronts.
    if scene
        .others
        .iter()
        .any(|&p| game.treaty(me, p) == Treaty::War)
    {
        return;
    }
    let objective = PlayerView::new(game, me)
        .wizard()
        .and_then(|w| w.objective());
    let fights = crate::objective::picks_fights(
        objective,
        crate::objective::temper(game.personality(me), game.difficulty()),
    );
    let leader = leader_of(game, me);
    let mine = scene.power(game, me);
    let target = scene
        .others
        .iter()
        .copied()
        .filter(|p| !ai.statecraft.appeased.contains(p))
        .filter(|&p| game.treaty(me, p) == Treaty::None)
        .filter(|&p| {
            game.diplomacy
                .stance(me, p)
                .is_none_or(|s| s.peace_duration == 0)
        })
        .filter(|&p| dominates(game, me, p, k))
        .min_by_key(|&p| (scene.power(game, p).score(k), p.0));
    let Some(target) = target else {
        return;
    };
    // The willingness: the base, a fighting temper, a target that leads, the margin of the lead;
    // less the factors that make the target a friend.
    let theirs = scene.power(game, target);
    let edge = ((mine.army * 100 / theirs.army.max(1) - k.dominant_army) / 10).clamp(0, 20);
    let willing = k.dominant_base
        + if fights { k.dominant_fighter } else { 0 }
        + if leader == Some(target) {
            k.dominant_leader
        } else {
            0
        }
        + edge
        - affinity(game, me, target);
    if (game.roll(100) as i32) > willing {
        return;
    }
    // A city to march on that a unit can reach.
    let reachable = {
        let view = PlayerView::new(game, me);
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
        let view = PlayerView::new(game, me);
        let port = view
            .my_cities()
            .into_iter()
            .any(|(_, p)| crate::view::is_coastal(&view, p));
        port && scene
            .cities_of(target)
            .into_iter()
            .any(|c| crate::naval::landing(&view, c).is_some())
    };
    if !reachable && !by_sea {
        return;
    }
    if ai
        .send(
            game,
            Command::Diplomacy(DiplomacyCommand::DeclareWar {
                player: me,
                on: target,
            }),
            events,
        )
        .is_ok()
    {
        ai.quarrel = Some(target);
        remark(ai, game, events, target, Remark::DominantWar);
    }
}

#[cfg(test)]
mod tests;
