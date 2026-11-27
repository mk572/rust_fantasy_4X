//! Diplomacy between wizards: contact, treaties, war, proposals and the events that tell the others what
//! happened (`docs/diplomacy.md`).
//!
//! The core holds the facts and applies the results; it never decides whether an AI wizard
//! accepts, and it keeps no feeling of one wizard for another: each AI keeps its own opinion
//! (`f4x-ai/src/opinion.rs`) and changes it from the events (task 775). A proposal waits as a pending proposal until its receiver answers with a
//! [`DiplomacyCommand::Answer`], and an AI's own initiatives (greetings, demands, war) are
//! commands it issues itself (`f4x-ai/src/diplomacy.rs` holds the rating of a proposal and the
//! war a wizard declares on the caster of the Spell of Mastery).
//!
//! Structure (`docs/archive/architecture-plan.md`): the rules are methods of [`Diplomacy`], which
//! owns the state and takes a [`DiplomacyCtx`] with what it needs of the rest of the game: a
//! read-only [`World`], the random numbers and a list of changes it asks the game to make
//! ([`Outcome`]: gold moved, a spell learned). `World` only lends it out for a call
//! (`with_diplomacy`) and has thin methods for the other modules and the
//! crates around. Tests of the rules run on a real game.
//!
//! Tests: in diplomacy/tests.rs.

pub mod trade;
pub use trade::Goods;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::SpellId;
use crate::wizard::objective::Personality;
use crate::world::rng::GameRng;
use crate::world::{Event, World};

/// What a wizard is bound to with another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Treaty {
    #[default]
    None,
    Pact,
    Alliance,
    War,
}

impl Treaty {
    /// The key of the words of the wizard sheet.
    pub fn key(self) -> &'static str {
        match self {
            Treaty::None => "treaty.none",
            Treaty::Pact => "treaty.pact",
            Treaty::Alliance => "treaty.alliance",
            Treaty::War => "treaty.war",
        }
    }
}

/// How far a first meeting has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Stage {
    #[default]
    Unmet,
    /// Met; the wizard will greet next.
    Greeted,
    Established,
}

/// What holds between two wizards, the same both ways: facts of the game. What each feels about the other
/// is the AI's (`f4x-ai/src/opinion.rs`, `docs/diplomacy.md` 9.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pair {
    pub contacted: bool,
    pub status: Treaty,
    /// The rounds the war has lasted, while the two are at war (the AI weighs a war by its age).
    #[serde(default)]
    pub war_age: Option<u32>,
    /// The cities each held when the war began, the lower player number first.
    #[serde(default)]
    pub war_cities: [u16; 2],
}

/// What a wizard holds about another, kept in the slot of one of them: how far the talk has gone, and the
/// turns in which `from` will not attack `to` (a peace made, a threat yielded to).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Stance {
    /// Turns in which `from` will not attack `to`.
    pub peace_duration: u32,
    pub stage: Stage,
}

/// What a wizard offers another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProposalKind {
    Pact,
    Alliance,
    Peace,
    /// Ask the receiver to declare war on a third wizard.
    WarOn(PlayerId),
    /// Ask the receiver to break its alliance with a third wizard.
    BreakAllianceWith(PlayerId),
    /// A trade (`trade.rs`): the proposer hands `give` to the receiver and gets `get`. A swap of
    /// two spells is one, and so is a spell for gold.
    Trade {
        give: Goods,
        get: Goods,
    },
    /// The proposer threatens to attack the receiver (`Threaten to Attack`): the receiver
    /// yields (accepts) and is left in peace for a few turns, or does not.
    Threat,
    /// The proposer demands `gold` of the receiver as tribute.
    Demand {
        gold: i32,
    },
}

/// A proposal waiting for its receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Proposal {
    pub from: PlayerId,
    pub to: PlayerId,
    pub kind: ProposalKind,
    /// How many times a trade has been countered on the way here (0: the first offer).
    pub round: u8,
}

impl Proposal {
    /// Whether `player` makes it, receives it, or is the one it is about.
    fn concerns(&self, player: PlayerId) -> bool {
        self.from == player
            || self.to == player
            || matches!(self.kind, ProposalKind::WarOn(p) | ProposalKind::BreakAllianceWith(p) if p == player)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiplomacyError {
    /// One of them is not a wizard in the game.
    NotWizards,
    SameWizard,
    NotContacted,
    /// The state of the treaties does not allow it.
    NotAllowed,
    AlreadyPending,
    NothingPending,
    NotEnoughGold,
    /// The spell is not the giver's, or the receiver cannot learn it.
    SpellUnavailable,
    /// A banished wizard may not contact the others.
    Banished,
}

impl DiplomacyError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::NotWizards => "refuse_wizard.diplomacy_error_not_wizards",
            Self::SameWizard => "refuse_wizard.diplomacy_error_same_wizard",
            Self::NotContacted => "refuse_wizard.diplomacy_error_not_contacted",
            Self::NotAllowed => "refuse_wizard.diplomacy_error_not_allowed",
            Self::AlreadyPending => "refuse_wizard.diplomacy_error_already_pending",
            Self::NothingPending => "refuse_wizard.diplomacy_error_nothing_pending",
            Self::NotEnoughGold => "refuse_wizard.diplomacy_error_not_enough_gold",
            Self::SpellUnavailable => "refuse_wizard.diplomacy_error_spell_unavailable",
            Self::Banished => "refuse_wizard.diplomacy_error_banished",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiplomacyEvent {
    ContactMade {
        a: PlayerId,
        b: PlayerId,
    },
    TreatyChanged {
        a: PlayerId,
        b: PlayerId,
        treaty: Treaty,
    },
    WarDeclared {
        attacker: PlayerId,
        defender: PlayerId,
    },
    /// `breaker` ended the treaty it had with `victim`.
    TreatyBroken {
        breaker: PlayerId,
        victim: PlayerId,
        treaty: Treaty,
    },
    ProposalMade(Proposal),
    /// The receiver of `old` answered a trade with another one, `new`, which now waits for the
    /// first proposer.
    ProposalCountered {
        old: Proposal,
        new: Proposal,
    },
    ProposalAnswered {
        proposal: Proposal,
        accepted: bool,
        /// The receiver did not answer in the turn.
        lapsed: bool,
    },
    GoldGiven {
        from: PlayerId,
        to: PlayerId,
        gold: i32,
    },
    SpellGiven {
        from: PlayerId,
        to: PlayerId,
        spell: SpellId,
    },
    /// `from` warns `to` that its patience is running out.
    Warned {
        from: PlayerId,
        to: PlayerId,
    },
    /// `from` tells `to` why it did what it did.
    Remarked {
        from: PlayerId,
        to: PlayerId,
        why: Remark,
    },
    /// `intruder` has units next to a city of `owner`, who gives a warning (the first turn of a
    /// treaty partner's trespass).
    Trespassing {
        owner: PlayerId,
        intruder: PlayerId,
    },
    /// A battle's end: what `defender` lost to `attacker` (task 782). A fact, not a feeling: the
    /// AI weighs it (`docs/diplomacy.md` section 5). Units that were summoned in the fight and
    /// the ones that were never in it do not count; `others` includes the `settlers`.
    BattleLosses {
        attacker: PlayerId,
        defender: PlayerId,
        heroes: u32,
        others: u32,
        settlers: u32,
    },
    /// An attack of `attacker` on a city of `defender` failed; `fortress` is whether it held the
    /// defender's fortress.
    AssaultFailed {
        attacker: PlayerId,
        defender: PlayerId,
        fortress: bool,
    },
    /// `winner` took the fortress of `loser`, who goes on (the Spell of Return).
    FortressTaken {
        winner: PlayerId,
        loser: PlayerId,
    },
    /// `winner` destroyed the wizard `loser`.
    WizardDestroyed {
        winner: PlayerId,
        loser: PlayerId,
    },
    /// The independent race of `player` has lost its last city.
    IndependenceLost {
        player: PlayerId,
    },
    /// `caster` cast a lasting enchantment on itself (the Spell of Mastery as it is begun).
    EnchantmentCast {
        caster: PlayerId,
        spell: SpellId,
    },
    /// `caster` cast a curse on a city of `owner`.
    CitySpellCast {
        caster: PlayerId,
        owner: PlayerId,
        spell: SpellId,
    },
    /// `caster` cast Subversion on `target`.
    Subverted {
        caster: PlayerId,
        target: PlayerId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DiplomacyCommand {
    /// The player has heard the greeting of a wizard it met.
    Greeted {
        player: PlayerId,
        with: PlayerId,
    },
    Propose {
        from: PlayerId,
        to: PlayerId,
        kind: ProposalKind,
    },
    /// The receiver answers the proposal `from` made it.
    Answer {
        player: PlayerId,
        from: PlayerId,
        accept: bool,
    },
    /// The receiver answers the trade `from` made it with a trade of its own: what it would give
    /// and what it would ask. An empty side is an open question (what would you give for this?).
    Counter {
        player: PlayerId,
        from: PlayerId,
        kind: ProposalKind,
    },
    /// End a pact or alliance (the oath breaker's penalties follow).
    BreakTreaty {
        player: PlayerId,
        with: PlayerId,
    },
    DeclareWar {
        player: PlayerId,
        on: PlayerId,
    },
    GiveGold {
        from: PlayerId,
        to: PlayerId,
        amount: i32,
    },
    GiveSpell {
        from: PlayerId,
        to: PlayerId,
        spell: SpellId,
    },
    /// `from` tells `to` that it is running out of patience (the first step of a quarrel).
    Warn {
        from: PlayerId,
        to: PlayerId,
    },
    /// `from` tells `to` why it has done what it did (a war, a gift, a peace offer): the
    /// original's remarks. Nothing changes but what the human is told.
    Remark {
        from: PlayerId,
        to: PlayerId,
        why: Remark,
    },
}

impl DiplomacyCommand {
    /// The player who says it: the command is this player's to send.
    pub fn speaker(&self) -> PlayerId {
        self.parties()[0]
    }

    /// The two players the command is between, the speaker first.
    fn parties(&self) -> [PlayerId; 2] {
        match *self {
            DiplomacyCommand::Greeted { player, with }
            | DiplomacyCommand::BreakTreaty { player, with } => [player, with],
            DiplomacyCommand::DeclareWar { player, on } => [player, on],
            DiplomacyCommand::Answer { player, from, .. }
            | DiplomacyCommand::Counter { player, from, .. } => [player, from],
            DiplomacyCommand::Propose { from, to, .. }
            | DiplomacyCommand::GiveGold { from, to, .. }
            | DiplomacyCommand::GiveSpell { from, to, .. }
            | DiplomacyCommand::Warn { from, to }
            | DiplomacyCommand::Remark { from, to, .. } => [from, to],
        }
    }
}

/// Why an AI wizard did what it did to another (`DiplomacyCommand::Remark`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Remark {
    /// It declared war because the other is much weaker.
    DominantWar,
    /// It gives a gift or offers a treaty because the other is much stronger.
    Appeasing,
    /// It asks for an alliance against the wizard that leads.
    Coalition,
    /// It offers peace because the war goes badly or serves no purpose.
    WarWeary,
}

impl Remark {
    /// The text key of the remark.
    pub fn key(self) -> &'static str {
        match self {
            Remark::DominantWar => "diplomacy.remark_dominant_war",
            Remark::Appeasing => "diplomacy.remark_appeasing",
            Remark::Coalition => "diplomacy.remark_coalition",
            Remark::WarWeary => "diplomacy.remark_war_weary",
        }
    }
}

/// Everything diplomacy keeps in a game.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diplomacy {
    pairs: BTreeMap<(u8, u8), Pair>,
    stances: BTreeMap<(u8, u8), Stance>,
    proposals: Vec<Proposal>,
    /// Events that hooks (battles, spells, the turn) raise; the next command returns them.
    queue: Vec<DiplomacyEvent>,
}

fn key(a: PlayerId, b: PlayerId) -> (u8, u8) {
    (a.0.min(b.0), a.0.max(b.0))
}

/// The squares of a plane of a medium map, which the city allowance below is counted for.
const ORIGINAL_PLANE_SQUARES: i32 = 60 * 40;

/// The land size of the game: 0 small, 1 medium, 2 large.
fn land_size(view: &World) -> i32 {
    let land = view.settings.basic.land_percent;
    if land <= crate::world::setting::LAND_SMALL {
        0
    } else if land <= crate::world::setting::LAND_MEDIUM {
        1
    } else {
        2
    }
}

/// The cities a wizard may hold before the others take it amiss (`(landsize + 1) * 3` on a medium
/// map, more on a larger one, in step with its land).
fn city_allowance(view: &World) -> i32 {
    let basic = &view.settings.basic;
    let squares = i32::from(basic.map_width) * i32::from(basic.map_height);
    ((land_size(view) + 1) * 3 * squares / ORIGINAL_PLANE_SQUARES).max(3)
}

impl Diplomacy {
    pub fn pair(&self, a: PlayerId, b: PlayerId) -> Option<&Pair> {
        self.pairs.get(&key(a, b))
    }

    pub fn stance(&self, from: PlayerId, to: PlayerId) -> Option<&Stance> {
        self.stances.get(&(from.0, to.0))
    }

    fn stance_mut(&mut self, from: PlayerId, to: PlayerId) -> &mut Stance {
        self.stances.entry((from.0, to.0)).or_default()
    }

    pub fn proposals(&self) -> &[Proposal] {
        &self.proposals
    }
}

/// The wizards `player` meets by what it sees now: the owners of the cities and of the units it
/// can make out on the squares in its sight, in player order.
pub fn met_in_sight(view: &World, player: PlayerId) -> Vec<PlayerId> {
    owners(view.foreign_sightings(player))
}

fn owners(sightings: Vec<crate::world::view::Sighting>) -> Vec<PlayerId> {
    let mut met: Vec<PlayerId> = sightings.into_iter().map(|s| s.owner).collect();
    met.sort();
    met.dedup();
    met
}

/// The total cost of a wizard's units.
pub fn army_strength(view: &World, player: PlayerId) -> i32 {
    view.units_of(player).iter().map(|u| u.cost as i32).sum()
}

/// What the rules of diplomacy need from the rest of the game, for one call: the read-only
/// view of the world and the difficulty. What they must change outside
/// diplomacy (gold, a spell learned) they leave in `out` for the game to apply.
pub struct DiplomacyCtx<'a> {
    pub view: &'a World,
    pub out: Vec<Outcome>,
}

/// A change diplomacy asks of the rest of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// An item of `from`'s vault or award list goes to `to`'s.
    Item {
        from: PlayerId,
        to: PlayerId,
        item: crate::world::serial::Serial,
    },
    /// A unit changes owner and appears in `to`'s closest city with room.
    Unit {
        to: PlayerId,
        unit: crate::world::serial::Serial,
    },
    /// A hero changes owner the same way, with its experience and its items.
    Hero {
        to: PlayerId,
        hero: crate::world::serial::Serial,
    },
    /// Move gold from one wizard's treasury to another's.
    Gold {
        from: PlayerId,
        to: PlayerId,
        amount: i32,
    },
    /// The wizard learns the spell, as if it had researched it.
    Spell { player: PlayerId, spell: SpellId },
    /// Peace ends the spells each of the two cast on the other's cities
    /// (`Cancel_Players_City_Enchantments`).
    DropCityCurses { a: PlayerId, b: PlayerId },
}

/// Every wizard still in the game, in player order.
pub fn wizards(view: &World) -> Vec<PlayerId> {
    let mut out = view.living_wizards();
    out.sort();
    out
}

impl Diplomacy {
    fn pair_mut(&mut self, _view: &World, a: PlayerId, b: PlayerId) -> &mut Pair {
        self.pairs.entry(key(a, b)).or_insert_with(|| Pair {
            contacted: false,
            status: Treaty::None,
            war_age: None,
            war_cities: [0, 0],
        })
    }

    pub fn have_met(&self, a: PlayerId, b: PlayerId) -> bool {
        self.pair(a, b).is_some_and(|p| p.contacted)
    }

    /// The treaty between two wizards.
    pub fn treaty(&self, a: PlayerId, b: PlayerId) -> Treaty {
        self.pair(a, b).map_or(Treaty::None, |p| p.status)
    }

    /// How many turns the two have been at war (none when they are not).
    pub fn war_age(&self, a: PlayerId, b: PlayerId) -> Option<u32> {
        self.pair(a, b).and_then(|p| p.war_age)
    }

    /// Every war is `rounds` older (younger, if negative); none is younger than new.
    pub(crate) fn age_wars(&mut self, rounds: i64) {
        for age in self.pairs.values_mut().filter_map(|p| p.war_age.as_mut()) {
            let older = i64::from(*age).saturating_add(rounds);
            *age = older.clamp(0, i64::from(u32::MAX)) as u32;
        }
    }

    /// How many cities `a` has gained since its war with `b` began (negative: lost). None when
    /// they are not at war.
    pub fn war_gain(&self, view: &World, a: PlayerId, b: PlayerId) -> Option<i32> {
        let pair = self.pair(a, b)?;
        pair.war_age?;
        let at_start = pair.war_cities[usize::from(a.0 > b.0)];
        Some(view.economy().city_count(a) as i32 - i32::from(at_start))
    }

    /// What the hooks raised since the last command, as events.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.queue)
            .into_iter()
            .map(Event::Diplomacy)
            .collect()
    }

    // ---- contact ---------------------------------------------------------------

    /// The two wizards meet. Returns whether it was the first time.
    pub fn make_contact(&mut self, ctx: &mut DiplomacyCtx<'_>, a: PlayerId, b: PlayerId) -> bool {
        if a == b || !ctx.view.takes_part_in_diplomacy(a) || !ctx.view.takes_part_in_diplomacy(b) {
            return false;
        }
        let pair = self.pair_mut(ctx.view, a, b);
        if pair.contacted {
            return false;
        }
        pair.contacted = true;
        for (from, to) in [(a, b), (b, a)] {
            let stance = self.stance_mut(from, to);
            if stance.stage == Stage::Unmet {
                stance.stage = Stage::Greeted;
            }
        }
        self.queue.push(DiplomacyEvent::ContactMade { a, b });
        true
    }

    /// `player` has just looked at the squares within `radius` of `at`: it meets the wizards
    /// whose cities or visible units stand there.
    pub(crate) fn contact_near(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        player: PlayerId,
        at: crate::map::Position,
        radius: u32,
    ) {
        if !ctx.view.takes_part_in_diplomacy(player) {
            return;
        }
        for other in ctx.view.foreign_owners_near(player, at, radius) {
            self.make_contact(ctx, player, other);
        }
    }

    /// Once a turn: whoever sees another wizard's city or visible unit meets it, and
    /// Nature Awareness meets everyone.
    fn contact_sweep(&mut self, ctx: &mut DiplomacyCtx<'_>) {
        let wizards = wizards(ctx.view);
        for &player in &wizards {
            if ctx.view.has_global_effect(
                player,
                crate::spell::global_enchantment::GlobalEffect::NatureAwareness,
            ) {
                for &other in &wizards {
                    self.make_contact(ctx, player, other);
                }
                continue;
            }
            for other in met_in_sight(ctx.view, player) {
                self.make_contact(ctx, player, other);
            }
        }
    }

    // ---- changing relations ----------------------------------------------------

    // ---- treaties and war ------------------------------------------------------

    fn set_status(&mut self, ctx: &mut DiplomacyCtx<'_>, a: PlayerId, b: PlayerId, treaty: Treaty) {
        let count =
            |p: PlayerId| ctx.view.economy().city_count(p).min(usize::from(u16::MAX)) as u16;
        let cities = if a.0 <= b.0 {
            [count(a), count(b)]
        } else {
            [count(b), count(a)]
        };
        let pair = self.pair_mut(ctx.view, a, b);
        pair.status = treaty;
        if treaty == Treaty::War && pair.war_age.is_none() {
            pair.war_cities = cities;
        }
        pair.war_age = (treaty == Treaty::War).then(|| pair.war_age.unwrap_or(0));
        self.queue
            .push(DiplomacyEvent::TreatyChanged { a, b, treaty });
    }

    /// A pact or an alliance between two wizards (`Start_Treaty`).
    pub fn start_treaty(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        a: PlayerId,
        b: PlayerId,
        treaty: Treaty,
    ) {
        self.set_status(ctx, a, b, treaty);
    }

    /// `breaker` ends the pact or alliance with `victim` (`Break_Treaties`). Unlike the original
    /// (which subtracts a negative number and so improves the breaker's own record), the
    /// oath-breaker's penalty is a real one.
    pub fn break_treaties(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        breaker: PlayerId,
        victim: PlayerId,
    ) {
        if !ctx.view.takes_part_in_diplomacy(breaker) || !ctx.view.takes_part_in_diplomacy(victim) {
            return;
        }
        let treaty = self.treaty(breaker, victim);
        if treaty == Treaty::War {
            return;
        }
        // What the others feel about it is for each AI to decide, from the event.
        let broke_a_treaty = matches!(treaty, Treaty::Pact | Treaty::Alliance);
        self.set_status(ctx, breaker, victim, Treaty::None);
        if broke_a_treaty {
            self.queue.push(DiplomacyEvent::TreatyBroken {
                breaker,
                victim,
                treaty,
            });
        }
    }

    /// `attacker` declares war on `defender` (`Declare_War`).
    pub fn declare_war(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        attacker: PlayerId,
        defender: PlayerId,
    ) {
        if attacker == defender
            || !ctx.view.takes_part_in_diplomacy(attacker)
            || !ctx.view.takes_part_in_diplomacy(defender)
        {
            return;
        }
        // Wizards at war know each other.
        self.make_contact(ctx, attacker, defender);
        if self.treaty(attacker, defender) == Treaty::War {
            return;
        }
        self.break_treaties(ctx, attacker, defender);
        self.set_status(ctx, attacker, defender, Treaty::War);
        self.queue
            .push(DiplomacyEvent::WarDeclared { attacker, defender });
    }

    /// The end of a war (`Declare_Peace`): no treaty, a better relation, and `peace_duration`
    /// turns in which each of the two is left alone by the other (the original does this for the
    /// human only).
    pub fn declare_peace(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        a: PlayerId,
        b: PlayerId,
        rng: &mut GameRng,
    ) {
        self.start_treaty(ctx, a, b, Treaty::None);
        for (kept, other) in [(a, b), (b, a)] {
            let turns = 7 + rng.roll(6);
            self.stance_mut(other, kept).peace_duration = turns;
        }
        ctx.out.push(Outcome::DropCityCurses { a, b });
    }

    // ---- proposals and the talk ------------------------------------------------

    /// Whether `from` may put `kind` to `to` now: the entries the original greys out.
    pub fn check_proposal(
        &self,
        view: &World,
        from: PlayerId,
        to: PlayerId,
        kind: ProposalKind,
    ) -> Result<(), DiplomacyError> {
        if !view.takes_part_in_diplomacy(from) || !view.takes_part_in_diplomacy(to) {
            return Err(DiplomacyError::NotWizards);
        }
        if from == to {
            return Err(DiplomacyError::SameWizard);
        }
        if !self.have_met(from, to) {
            return Err(DiplomacyError::NotContacted);
        }
        let status = self.treaty(from, to);
        let known_third = |third: PlayerId| {
            third != from
                && third != to
                && view.takes_part_in_diplomacy(third)
                && !view.is_dormant(third)
                && self.have_met(from, third)
                && self.have_met(to, third)
        };
        let ok = match kind {
            ProposalKind::Pact => status == Treaty::None,
            ProposalKind::Alliance => matches!(status, Treaty::None | Treaty::Pact),
            ProposalKind::Peace => status == Treaty::War,
            ProposalKind::Threat => status != Treaty::War,
            ProposalKind::Demand { gold } => status != Treaty::War && gold > 0,
            // A wizard allied with the third one is first asked to break the alliance: Break
            // Alliance replaces Declare War for that wizard (1.50 note 058).
            ProposalKind::WarOn(third) => {
                status != Treaty::War
                    && known_third(third)
                    && !matches!(self.treaty(to, third), Treaty::War | Treaty::Alliance)
            }
            ProposalKind::BreakAllianceWith(third) => {
                status != Treaty::War
                    && known_third(third)
                    && self.treaty(to, third) == Treaty::Alliance
            }
            ProposalKind::Trade { give, get } => {
                // An empty side is an open question to be answered with a counter.
                status != Treaty::War
                    && !(give.is_empty() && get.is_empty())
                    && !(give.is_only_gold() && get.is_only_gold())
                    // Each side can hand over its part (the books for a spell, a vault for an
                    // item, a city for a unit, a treasury for gold).
                    && trade::can_hand_over(view, from, to, give)
                    && trade::can_hand_over(view, to, from, get)
            }
        };
        if ok {
            Ok(())
        } else {
            Err(DiplomacyError::NotAllowed)
        }
    }

    fn propose(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        from: PlayerId,
        to: PlayerId,
        kind: ProposalKind,
    ) -> Result<(), DiplomacyError> {
        self.check_proposal(ctx.view, from, to, kind)?;
        if self.proposals.iter().any(|p| p.from == from && p.to == to) {
            return Err(DiplomacyError::AlreadyPending);
        }
        let proposal = Proposal {
            from,
            to,
            kind,
            round: 0,
        };
        self.proposals.push(proposal);
        self.queue.push(DiplomacyEvent::ProposalMade(proposal));
        Ok(())
    }

    /// The receiver of a trade answers with a trade of its own (a negotiation, `max_rounds` of `rules.ron`).
    fn counter(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        player: PlayerId,
        from: PlayerId,
        kind: ProposalKind,
    ) -> Result<(), DiplomacyError> {
        let at = self
            .proposals
            .iter()
            .position(|p| p.from == from && p.to == player)
            .ok_or(DiplomacyError::NothingPending)?;
        let old = self.proposals[at];
        let both_trades = matches!(old.kind, ProposalKind::Trade { .. })
            && matches!(kind, ProposalKind::Trade { .. });
        if !both_trades || old.round + 1 >= ctx.view.content().rules.config.diplomacy.max_rounds {
            return Err(DiplomacyError::NotAllowed);
        }
        self.check_proposal(ctx.view, player, from, kind)?;
        if self
            .proposals
            .iter()
            .any(|p| p.from == player && p.to == from)
        {
            return Err(DiplomacyError::AlreadyPending);
        }
        self.proposals.remove(at);
        let new = Proposal {
            from: player,
            to: from,
            kind,
            round: old.round + 1,
        };
        self.proposals.push(new);
        self.queue
            .push(DiplomacyEvent::ProposalCountered { old, new });
        Ok(())
    }

    /// The receiver's answer: an accepted proposal takes effect.
    fn answer(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        player: PlayerId,
        from: PlayerId,
        accept: bool,
        rng: &mut GameRng,
    ) -> Result<(), DiplomacyError> {
        let at = self
            .proposals
            .iter()
            .position(|p| p.from == from && p.to == player)
            .ok_or(DiplomacyError::NothingPending)?;
        // An open trade (one side empty) is not a deal: it can only be countered or turned down.
        if accept
            && matches!(self.proposals[at].kind, ProposalKind::Trade { give, get }
                if give.is_empty() || get.is_empty())
        {
            return Err(DiplomacyError::NotAllowed);
        }
        let proposal = self.proposals.remove(at);
        let can_pay = match proposal.kind {
            ProposalKind::Demand { gold } => ctx
                .view
                .player(player)
                .is_some_and(|p| p.treasury().gold >= gold),
            _ => true,
        };
        let accepted = accept
            && can_pay
            && self
                .check_proposal(ctx.view, from, player, proposal.kind)
                .is_ok();
        if accepted {
            self.carry_out(ctx, proposal, rng);
        }
        self.queue.push(DiplomacyEvent::ProposalAnswered {
            proposal,
            accepted,
            lapsed: false,
        });
        Ok(())
    }

    fn carry_out(&mut self, ctx: &mut DiplomacyCtx<'_>, p: Proposal, rng: &mut GameRng) {
        match p.kind {
            ProposalKind::Pact => self.start_treaty(ctx, p.from, p.to, Treaty::Pact),
            ProposalKind::Alliance => self.start_treaty(ctx, p.from, p.to, Treaty::Alliance),
            ProposalKind::Peace => {
                self.declare_peace(ctx, p.from, p.to, rng);
            }
            ProposalKind::Threat => {
                // The yielding wizard leaves the threatener alone for a while.
                let turns = rng.roll(15);
                self.stance_mut(p.to, p.from).peace_duration = turns;
            }
            ProposalKind::Demand { gold } => ctx.out.push(Outcome::Gold {
                from: p.to,
                to: p.from,
                amount: gold,
            }),
            ProposalKind::WarOn(third) => self.declare_war(ctx, p.to, third),
            ProposalKind::BreakAllianceWith(third) => self.break_treaties(ctx, p.to, third),
            ProposalKind::Trade { give, get } => {
                Self::hand_over(ctx, p.from, p.to, give);
                Self::hand_over(ctx, p.to, p.from, get);
            }
        }
    }

    /// What `from` hands to `to` in a trade: the outcomes that move it.
    fn hand_over(ctx: &mut DiplomacyCtx<'_>, from: PlayerId, to: PlayerId, goods: Goods) {
        if goods.gold > 0 {
            ctx.out.push(Outcome::Gold {
                from,
                to,
                amount: goods.gold,
            });
        }
        if let Some(spell) = goods.spell {
            ctx.out.push(Outcome::Spell { player: to, spell });
        }
        if let Some(item) = goods.item {
            ctx.out.push(Outcome::Item { from, to, item });
        }
        if let Some(unit) = goods.unit {
            ctx.out.push(Outcome::Unit { to, unit });
        }
        if let Some(hero) = goods.hero {
            ctx.out.push(Outcome::Hero { to, hero });
        }
    }

    fn give_gold(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        from: PlayerId,
        to: PlayerId,
        amount: i32,
    ) -> Result<(), DiplomacyError> {
        self.check_gift(ctx.view, from, to)?;
        if amount <= 0
            || ctx
                .view
                .player(from)
                .is_none_or(|p| p.treasury().gold < amount)
        {
            return Err(DiplomacyError::NotEnoughGold);
        }
        ctx.out.push(Outcome::Gold { from, to, amount });
        self.queue.push(DiplomacyEvent::GoldGiven {
            from,
            to,
            gold: amount,
        });
        Ok(())
    }

    fn give_spell_as_gift(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        from: PlayerId,
        to: PlayerId,
        spell: SpellId,
    ) -> Result<(), DiplomacyError> {
        self.check_gift(ctx.view, from, to)?;
        let has = |p: PlayerId| ctx.view.wizard(p).is_some_and(|m| m.knows(spell));
        if !has(from) || has(to) {
            return Err(DiplomacyError::SpellUnavailable);
        }
        ctx.out.push(Outcome::Spell { player: to, spell });
        self.queue
            .push(DiplomacyEvent::SpellGiven { from, to, spell });
        Ok(())
    }

    fn check_gift(&self, view: &World, from: PlayerId, to: PlayerId) -> Result<(), DiplomacyError> {
        if !view.takes_part_in_diplomacy(from) || !view.takes_part_in_diplomacy(to) {
            return Err(DiplomacyError::NotWizards);
        }
        if from == to {
            return Err(DiplomacyError::SameWizard);
        }
        if !self.have_met(from, to) {
            return Err(DiplomacyError::NotContacted);
        }
        Ok(())
    }

    /// Carry out a diplomacy command.
    pub(crate) fn command(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        command: DiplomacyCommand,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        // Nobody deals with a dormant player.
        if command.parties().iter().any(|&p| ctx.view.is_dormant(p)) {
            return vec![Event::Rejected(crate::world::Rejection::Diplomacy(
                DiplomacyError::NotWizards,
            ))];
        }
        // A banished wizard starts nothing with the others; it may still answer an offer.
        let actor = match command {
            DiplomacyCommand::Greeted { .. }
            | DiplomacyCommand::Answer { .. }
            | DiplomacyCommand::Counter { .. } => None,
            DiplomacyCommand::BreakTreaty { player, .. }
            | DiplomacyCommand::DeclareWar { player, .. } => Some(player),
            DiplomacyCommand::Propose { from, .. }
            | DiplomacyCommand::GiveGold { from, .. }
            | DiplomacyCommand::GiveSpell { from, .. }
            | DiplomacyCommand::Warn { from, .. }
            | DiplomacyCommand::Remark { from, .. } => Some(from),
        };
        if actor.is_some_and(|a| ctx.view.wizard(a).is_some_and(|w| w.is_banished())) {
            return vec![Event::Rejected(crate::world::Rejection::Diplomacy(
                DiplomacyError::Banished,
            ))];
        }
        let result = match command {
            DiplomacyCommand::Greeted { player, with } => {
                if self.have_met(player, with) {
                    let s = self.stance_mut(player, with);
                    if s.stage == Stage::Greeted {
                        s.stage = Stage::Established;
                    }
                    Ok(())
                } else {
                    Err(DiplomacyError::NotContacted)
                }
            }
            DiplomacyCommand::Propose { from, to, kind } => self.propose(ctx, from, to, kind),
            DiplomacyCommand::Answer {
                player,
                from,
                accept,
            } => self.answer(ctx, player, from, accept, rng),
            DiplomacyCommand::Counter { player, from, kind } => {
                self.counter(ctx, player, from, kind)
            }
            DiplomacyCommand::BreakTreaty { player, with } => {
                if matches!(self.treaty(player, with), Treaty::Pact | Treaty::Alliance) {
                    self.break_treaties(ctx, player, with);
                    Ok(())
                } else {
                    Err(DiplomacyError::NotAllowed)
                }
            }
            DiplomacyCommand::DeclareWar { player, on } => {
                if !self.have_met(player, on) {
                    Err(DiplomacyError::NotContacted)
                } else if self.treaty(player, on) == Treaty::War {
                    Err(DiplomacyError::NotAllowed)
                } else {
                    self.declare_war(ctx, player, on);
                    Ok(())
                }
            }
            DiplomacyCommand::GiveGold { from, to, amount } => {
                self.give_gold(ctx, from, to, amount)
            }
            DiplomacyCommand::GiveSpell { from, to, spell } => {
                self.give_spell_as_gift(ctx, from, to, spell)
            }
            DiplomacyCommand::Remark { from, to, why } => {
                self.check_gift(ctx.view, from, to).map(|()| {
                    self.queue.push(DiplomacyEvent::Remarked { from, to, why });
                })
            }
            DiplomacyCommand::Warn { from, to } => self.check_gift(ctx.view, from, to).map(|()| {
                self.queue.push(DiplomacyEvent::Warned { from, to });
            }),
        };
        let mut events = Vec::new();
        if let Err(e) = result {
            events.push(Event::Rejected(crate::world::Rejection::Diplomacy(e)));
        }
        events
    }

    // ---- the turn --------------------------------------------------------------

    /// Subversion (`Apply_Subversion`): the relation of every other wizard that has met `target`
    /// falls by `subversion_penalty` (`rules.ron`), and the peace between them is over. The original's code
    /// then overwrites the new relation with the old one, which leaves only the peace ended; the
    /// help's rule is followed here (`docs/help-vs-implementation-conflicts.md`, B33).
    pub(crate) fn subvert(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        caster: PlayerId,
        target: PlayerId,
    ) {
        self.queue
            .push(DiplomacyEvent::Subverted { caster, target });
        let others: Vec<PlayerId> = ctx
            .view
            .player_ids()
            .into_iter()
            .filter(|&w| {
                w != target && ctx.view.takes_part_in_diplomacy(w) && ctx.view.human() != Some(w)
            })
            .filter(|&w| self.have_met(w, target))
            .collect();
        for w in others {
            self.stance_mut(w, target).peace_duration = 0;
            self.stance_mut(target, w).peace_duration = 0;
        }
    }

    /// A spell was cast: the wizards it offends take note. `target_owner` is whose city it fell on.
    pub(crate) fn cast_relations(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        caster: PlayerId,
        spell: SpellId,
        target_owner: Option<PlayerId>,
    ) {
        if let Some(owner) = target_owner {
            self.queue.push(DiplomacyEvent::CitySpellCast {
                caster,
                owner,
                spell,
            });
        }
        // Lasting enchantments on the wizard are noticed as they are cast (the Spell of Mastery
        // as it is begun, see `start_cast`).
        if !ctx
            .view
            .spell_has_hook(spell, crate::spell::special::SPELL_OF_MASTERY)
            && ctx.view.content().rules.spells.get(spell).duration
                != crate::rules::Duration::Instant
        {
            self.queue
                .push(DiplomacyEvent::EnchantmentCast { caster, spell });
        }
    }

    /// A wizard begins the Spell of Mastery: the others take note at once.
    pub(crate) fn begin_cast_relations(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        player: PlayerId,
        spell: SpellId,
    ) {
        if ctx
            .view
            .spell_has_hook(spell, crate::spell::special::SPELL_OF_MASTERY)
        {
            self.queue.push(DiplomacyEvent::EnchantmentCast {
                caster: player,
                spell,
            });
        }
    }

    /// A battle between two wizards: they meet. A treaty between them is never broken here: an attack
    /// on a treaty partner is refused until a player has broken the treaty (`World::bound_by_treaty`).
    pub(crate) fn attack_relations(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        attacker: PlayerId,
        defender: PlayerId,
    ) {
        if !ctx.view.takes_part_in_diplomacy(attacker)
            || !ctx.view.takes_part_in_diplomacy(defender)
        {
            return;
        }
        self.make_contact(ctx, attacker, defender);
    }

    /// `winner` took a city of `loser` that had people in it: -40. The original also declares war
    /// here; the game does not: only a player's decision changes a treaty (`docs/design.md` section 8,
    /// `docs/diplomacy.md` 9.2).
    pub(crate) fn city_taken_relations(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        winner: PlayerId,
        loser: PlayerId,
    ) {
        if !ctx.view.takes_part_in_diplomacy(winner) || !ctx.view.takes_part_in_diplomacy(loser) {
            return;
        }
        self.make_contact(ctx, winner, loser);
    }

    /// An attack on a city failed: -60 if it held the wizard's fortress, else -20.
    pub(crate) fn city_held_relations(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        attacker: PlayerId,
        defender: PlayerId,
        fortress_city: bool,
    ) {
        if !ctx.view.takes_part_in_diplomacy(attacker)
            || !ctx.view.takes_part_in_diplomacy(defender)
        {
            return;
        }
        self.queue.push(DiplomacyEvent::AssaultFailed {
            attacker,
            defender,
            fortress: fortress_city,
        });
    }

    /// A wizard's fortress fell (-20 with every other wizard) or the wizard was destroyed (-40,
    /// and they forget each other).
    pub(crate) fn conquest_relations(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        winner: PlayerId,
        loser: PlayerId,
        destroyed: bool,
    ) {
        if winner == loser {
            return;
        }
        self.queue.push(if destroyed {
            DiplomacyEvent::WizardDestroyed { winner, loser }
        } else {
            DiplomacyEvent::FortressTaken { winner, loser }
        });
        for other in wizards(ctx.view) {
            if other == loser {
                continue;
            }
            if destroyed {
                let pair = self.pair_mut(ctx.view, other, loser);
                pair.status = Treaty::None;
                pair.war_age = None;
                pair.contacted = false;
            }
        }
    }

    /// Every treaty, war and promise of peace between `player` and anybody ends, and every
    /// proposal from, to or about it lapses. That they have met stays known.
    pub(crate) fn end_relations(&mut self, ctx: &mut DiplomacyCtx<'_>, player: PlayerId) {
        let treaties: Vec<(PlayerId, PlayerId)> = self
            .pairs
            .iter()
            .filter(|(&(a, b), pair)| {
                (a == player.0 || b == player.0) && pair.status != Treaty::None
            })
            .map(|(&(a, b), _)| (PlayerId(a), PlayerId(b)))
            .collect();
        for (a, b) in treaties {
            self.set_status(ctx, a, b, Treaty::None);
        }
        for (&(from, to), stance) in &mut self.stances {
            if from == player.0 || to == player.0 {
                stance.peace_duration = 0;
            }
        }
        let (lapsed, open): (Vec<Proposal>, Vec<Proposal>) = std::mem::take(&mut self.proposals)
            .into_iter()
            .partition(|p| p.concerns(player));
        self.proposals = open;
        for proposal in lapsed {
            self.queue.push(DiplomacyEvent::ProposalAnswered {
                proposal,
                accepted: false,
                lapsed: true,
            });
        }
    }

    /// The players that have a treaty, a war, a promise of peace or an open proposal.
    fn bound_players(&self) -> BTreeSet<PlayerId> {
        let treaties = self.pairs.iter().filter(|(_, p)| p.status != Treaty::None);
        let promises = self.stances.iter().filter(|(_, s)| s.peace_duration > 0);
        let proposals = self.proposals.iter().map(|p| (p.from.0, p.to.0));
        treaties
            .map(|(&ids, _)| ids)
            .chain(promises.map(|(&ids, _)| ids))
            .chain(proposals)
            .flat_map(|(a, b)| [PlayerId(a), PlayerId(b)])
            .collect()
    }

    /// The proposals nobody answered lapse: a turn is ending.
    pub(crate) fn lapse_proposals(&mut self) {
        let lapsed = std::mem::take(&mut self.proposals);
        for proposal in lapsed {
            self.queue.push(DiplomacyEvent::ProposalAnswered {
                proposal,
                accepted: false,
                lapsed: true,
            });
        }
    }

    /// The part of the round's end that belongs to diplomacy (`Next_Turn_Calc` order): peace
    /// runs down, the enchantments in force sour or sweeten, relations drift, new wizards are
    /// met, and the patience comes back.
    pub(crate) fn process_diplomacy(&mut self, ctx: &mut DiplomacyCtx<'_>) {
        for s in self.stances.values_mut() {
            s.peace_duration = s.peace_duration.saturating_sub(1);
        }
        self.contact_sweep(ctx);
    }
}

impl World {
    /// [`World::run_diplomacy`] for rules that give no spell, with what they ask applied.
    fn with_diplomacy<R>(
        &mut self,
        f: impl FnOnce(&mut Diplomacy, &mut DiplomacyCtx<'_>) -> R,
    ) -> R {
        let (result, out) = self.run_diplomacy(f);
        for outcome in out {
            self.apply_outcome(outcome);
        }
        result
    }

    /// A change diplomacy asked for that draws no random number.
    fn apply_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Gold { from, to, amount } => {
                if let Some(t) = self.treasury_mut(from) {
                    t.gold -= amount;
                }
                if let Some(t) = self.treasury_mut(to) {
                    t.gold += amount;
                }
            }
            // Only a command gives a spell, and it learns it itself, with the generator.
            Outcome::Spell { .. } => debug_assert!(false, "a spell outside a command"),
            Outcome::Item { from, to, item } => self.pass_item(from, to, item),
            Outcome::Unit { to, unit } => self.pass_unit(to, unit),
            // A hero goes like a unit: it keeps its experience and the items it carries.
            Outcome::Hero { to, hero } => self.pass_unit(to, hero),
            Outcome::DropCityCurses { a, b } => self.drop_city_curses(a, b),
        }
    }

    /// Lend diplomacy out for one call: its state is taken out of the world, so the rules can
    /// run over a read-only view of what is left (`design.md` section 3). What they ask of the
    /// rest of the world comes back with their result.
    fn run_diplomacy<R>(
        &mut self,
        f: impl FnOnce(&mut Diplomacy, &mut DiplomacyCtx<'_>) -> R,
    ) -> (R, Vec<Outcome>) {
        let mut diplomacy = std::mem::take(&mut self.diplomacy);
        let (result, out) = {
            let mut ctx = DiplomacyCtx {
                view: &*self,
                out: Vec::new(),
            };
            let result = f(&mut diplomacy, &mut ctx);
            (result, ctx.out)
        };
        self.diplomacy = diplomacy;
        (result, out)
    }

    /// An item of `from` goes to `to` (a trade): out of the armory, into the other's.
    fn pass_item(&mut self, from: PlayerId, to: PlayerId, serial: crate::world::serial::Serial) {
        let Some((entity, item)) = trade::armory_item(self, from, serial) else {
            return;
        };
        self.despawn_item(entity);
        self.give_item(to, item);
    }

    /// A unit of a city goes to `to` (a trade): it changes owner, appears at the delivery square
    /// and drops its orders.
    fn pass_unit(&mut self, to: PlayerId, serial: crate::world::serial::Serial) {
        let Some(unit) = self.units().entity_of_serial(serial) else {
            return;
        };
        let Some(at) = self
            .units()
            .unit_position(unit)
            .and_then(|from| trade::delivery_square(self, to, from))
        else {
            return;
        };
        self.set_owner(unit, to);
        let _ = self.ecs.insert_one(unit, at);
        self.watch_tile(at);
    }

    /// Take off every city the spells that `a` and `b` cast on each other's cities.
    fn drop_city_curses(&mut self, a: PlayerId, b: PlayerId) {
        for (city, info, _) in self.economy().cities() {
            let other = if info.owner == a {
                b
            } else if info.owner == b {
                a
            } else {
                continue;
            };
            let cast: Vec<_> = self
                .spells()
                .city_enchantments(city)
                .into_iter()
                .filter(|&(_, caster)| caster == other)
                .collect();
            for (spell, caster) in cast {
                self.remove_city_enchantment(city, spell, caster);
            }
        }
    }

    /// Every wizard still in the game, in player order.
    pub fn wizards(&self) -> Vec<PlayerId> {
        wizards(self)
    }

    pub fn have_met(&self, a: PlayerId, b: PlayerId) -> bool {
        self.diplomacy.have_met(a, b)
    }

    /// The treaty between two wizards.
    pub fn treaty(&self, a: PlayerId, b: PlayerId) -> Treaty {
        self.diplomacy.treaty(a, b)
    }

    /// Whether `a` and `b` have a pact or an alliance, which bars an attack of one on the other
    /// until a player breaks it (`DiplomacyCommand::BreakTreaty`).
    pub fn bound_by_treaty(&self, a: PlayerId, b: PlayerId) -> bool {
        matches!(self.treaty(a, b), Treaty::Pact | Treaty::Alliance)
    }

    /// How many turns the two have been at war (none when they are not).
    pub fn war_age(&self, a: PlayerId, b: PlayerId) -> Option<u32> {
        self.diplomacy.war_age(a, b)
    }

    /// The cities `a` has gained since its war with `b` began (negative: lost).
    pub fn war_gain(&self, a: PlayerId, b: PlayerId) -> Option<i32> {
        self.diplomacy.war_gain(self, a, b)
    }

    /// What the hooks raised since the last command, as events.
    pub fn drain_diplomacy_events(&mut self) -> Vec<Event> {
        self.diplomacy.drain_events()
    }

    /// The two wizards meet. Returns whether it was the first time.
    pub fn make_contact(&mut self, a: PlayerId, b: PlayerId) -> bool {
        self.with_diplomacy(|d, ctx| d.make_contact(ctx, a, b))
    }

    /// `player` has just looked at the squares within `radius` of `at`: it meets the wizards
    /// whose cities or visible units stand there.
    pub(crate) fn contact_near(&mut self, player: PlayerId, at: Position, radius: u32) {
        self.with_diplomacy(|d, ctx| d.contact_near(ctx, player, at, radius));
    }

    /// Subversion on `target`: every other wizard who has met them thinks 25 points worse of them
    /// and will not hold to a peace with them. The human's own feelings are left alone, as in the
    /// original (`Apply_Subversion`).
    pub(crate) fn subvert(&mut self, caster: PlayerId, target: PlayerId) {
        self.with_diplomacy(|d, ctx| d.subvert(ctx, caster, target));
    }

    /// A fact for the AI's feelings (task 782).
    pub(crate) fn report_diplomacy(&mut self, event: DiplomacyEvent) {
        self.diplomacy.queue.push(event);
    }

    /// A pact or an alliance between two wizards (`Start_Treaty`).
    pub fn start_treaty(&mut self, a: PlayerId, b: PlayerId, treaty: Treaty) {
        self.with_diplomacy(|d, ctx| d.start_treaty(ctx, a, b, treaty));
    }

    /// `breaker` ends the pact or alliance with `victim` (`Break_Treaties`).
    pub fn break_treaties(&mut self, breaker: PlayerId, victim: PlayerId) {
        self.with_diplomacy(|d, ctx| d.break_treaties(ctx, breaker, victim));
    }

    /// `attacker` declares war on `defender` (`Declare_War`).
    pub fn declare_war(&mut self, attacker: PlayerId, defender: PlayerId) {
        self.with_diplomacy(|d, ctx| d.declare_war(ctx, attacker, defender));
    }

    /// The end of a war (`Declare_Peace`).
    pub fn declare_peace(&mut self, a: PlayerId, b: PlayerId, rng: &mut GameRng) {
        self.with_diplomacy(|d, ctx| d.declare_peace(ctx, a, b, rng));
    }

    /// The land size of the game (0 small, 1 medium, 2 large) and the cities a wizard may hold before
    /// the others take it amiss: facts of the game for the AI's look (task 783).
    pub fn land_size(&self) -> i32 {
        land_size(self)
    }

    pub fn city_allowance(&self) -> i32 {
        city_allowance(self)
    }

    /// The total cost of a wizard's units.
    pub fn army_strength(&self, player: PlayerId) -> i32 {
        army_strength(self, player)
    }

    /// Whether `from` may put `kind` to `to` now: the entries the original greys out.
    pub fn check_proposal(
        &self,
        from: PlayerId,
        to: PlayerId,
        kind: ProposalKind,
    ) -> Result<(), DiplomacyError> {
        self.diplomacy.check_proposal(self, from, to, kind)
    }

    /// Carry out a diplomacy command.
    pub(crate) fn diplomacy_command(
        &mut self,
        command: DiplomacyCommand,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        let (events, out) = self.run_diplomacy(|d, ctx| d.command(ctx, command, rng));
        for outcome in out {
            match outcome {
                Outcome::Spell { player, spell } => self.learn_spell(player, spell, rng),
                other => self.apply_outcome(other),
            }
        }
        events
    }

    /// A spell was cast: the wizards it offends take note. `target_owner` is whose city it fell on.
    pub(crate) fn cast_relations(
        &mut self,
        caster: PlayerId,
        spell: SpellId,
        target_owner: Option<PlayerId>,
    ) {
        self.with_diplomacy(|d, ctx| d.cast_relations(ctx, caster, spell, target_owner));
    }

    /// A wizard begins the Spell of Mastery: the others take note at once.
    pub(crate) fn begin_cast_relations(&mut self, player: PlayerId, spell: SpellId) {
        self.with_diplomacy(|d, ctx| d.begin_cast_relations(ctx, player, spell));
    }

    /// `owner` has just lost a city: a race that has none left has lost its independence, and
    /// with it its relations with everybody (while it has units it may make new ones).
    pub(crate) fn city_lost_relations(&mut self, owner: PlayerId) {
        if self.race_of_player(owner).is_some() && self.economy().city_count(owner) == 0 {
            self.with_diplomacy(|d, ctx| {
                d.end_relations(ctx, owner);
                d.queue
                    .push(DiplomacyEvent::IndependenceLost { player: owner });
            });
        }
    }

    /// A dormant player has no relations: one with units and no city may have made some, and has
    /// lost the last of the units.
    pub(crate) fn end_dormant_relations(&mut self) {
        for player in self.diplomacy.bound_players() {
            if self.is_dormant(player) {
                self.with_diplomacy(|d, ctx| d.end_relations(ctx, player));
            }
        }
    }

    /// Attacking a wizard one has a treaty with breaks it first.
    pub(crate) fn attack_relations(&mut self, attacker: PlayerId, defender: PlayerId) {
        self.with_diplomacy(|d, ctx| d.attack_relations(ctx, attacker, defender));
    }

    /// `winner` took a city of `loser` that had people in it.
    pub(crate) fn city_taken_relations(&mut self, winner: PlayerId, loser: PlayerId) {
        self.with_diplomacy(|d, ctx| d.city_taken_relations(ctx, winner, loser));
    }

    /// An attack on a city failed.
    pub(crate) fn city_held_relations(
        &mut self,
        attacker: PlayerId,
        defender: PlayerId,
        fortress_city: bool,
    ) {
        self.with_diplomacy(|d, ctx| d.city_held_relations(ctx, attacker, defender, fortress_city));
    }

    /// A wizard's fortress fell or the wizard was destroyed.
    pub(crate) fn conquest_relations(
        &mut self,
        winner: PlayerId,
        loser: PlayerId,
        destroyed: bool,
    ) {
        self.with_diplomacy(|d, ctx| d.conquest_relations(ctx, winner, loser, destroyed));
    }

    /// The part of the turn that belongs to diplomacy.
    pub(crate) fn process_diplomacy(&mut self) {
        self.with_diplomacy(|d, ctx| d.process_diplomacy(ctx));
    }
}

/// What diplomacy keeps in a save.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiplomacySave {
    pub pairs: Vec<(u8, u8, Pair)>,
    pub stances: Vec<(u8, u8, Stance)>,
    pub personalities: Vec<(u8, crate::wizard::objective::Personality)>,
    #[serde(default)]
    pub objectives: Vec<(u8, crate::wizard::objective::Objective)>,
    pub proposals: Vec<ProposalSave>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalSave {
    pub from: u8,
    pub to: u8,
    pub kind: KindSave,
    #[serde(default)]
    pub round: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KindSave {
    Pact,
    Alliance,
    Peace,
    WarOn(u8),
    BreakAllianceWith(u8),
    /// A swap of two spells, as saved before trades (`Trade`).
    Exchange {
        give: SpellId,
        get: SpellId,
    },
    Trade {
        give: Goods,
        get: Goods,
    },
    Threat,
    Demand {
        gold: i32,
    },
}

impl Diplomacy {
    pub(crate) fn to_save(&self) -> DiplomacySave {
        DiplomacySave {
            pairs: self
                .pairs
                .iter()
                .map(|(k, v)| (k.0, k.1, v.clone()))
                .collect(),
            stances: self
                .stances
                .iter()
                .map(|(k, v)| (k.0, k.1, v.clone()))
                .collect(),
            personalities: Vec::new(),
            objectives: Vec::new(),
            proposals: self
                .proposals
                .iter()
                .map(|p| ProposalSave {
                    from: p.from.0,
                    to: p.to.0,
                    round: p.round,
                    kind: match p.kind {
                        ProposalKind::Pact => KindSave::Pact,
                        ProposalKind::Alliance => KindSave::Alliance,
                        ProposalKind::Peace => KindSave::Peace,
                        ProposalKind::WarOn(t) => KindSave::WarOn(t.0),
                        ProposalKind::BreakAllianceWith(t) => KindSave::BreakAllianceWith(t.0),
                        ProposalKind::Threat => KindSave::Threat,
                        ProposalKind::Demand { gold } => KindSave::Demand { gold },
                        ProposalKind::Trade { give, get } => KindSave::Trade { give, get },
                    },
                })
                .collect(),
        }
    }

    pub(crate) fn restore(&mut self, save: DiplomacySave) {
        self.pairs = save
            .pairs
            .into_iter()
            .map(|(a, b, p)| ((a, b), p))
            .collect();
        self.stances = save
            .stances
            .into_iter()
            .map(|(a, b, s)| ((a, b), s))
            .collect();
        self.proposals = save
            .proposals
            .into_iter()
            .map(|p| {
                let kind = match p.kind {
                    KindSave::Pact => ProposalKind::Pact,
                    KindSave::Alliance => ProposalKind::Alliance,
                    KindSave::Peace => ProposalKind::Peace,
                    KindSave::WarOn(t) => ProposalKind::WarOn(PlayerId(t)),
                    KindSave::BreakAllianceWith(t) => ProposalKind::BreakAllianceWith(PlayerId(t)),
                    KindSave::Threat => ProposalKind::Threat,
                    KindSave::Demand { gold } => ProposalKind::Demand { gold },
                    KindSave::Exchange { give, get } => ProposalKind::Trade {
                        give: Goods::spell(give),
                        get: Goods::spell(get),
                    },
                    KindSave::Trade { give, get } => ProposalKind::Trade { give, get },
                };
                Proposal {
                    from: PlayerId(p.from),
                    to: PlayerId(p.to),
                    kind,
                    round: p.round,
                }
            })
            .collect();
    }
}

impl World {
    pub(crate) fn diplomacy_to_save(&self) -> DiplomacySave {
        let mut save = self.diplomacy.to_save();
        for id in self.player_ids() {
            if let Some(w) = self.wizard(id) {
                if let Some(p) = w.component::<Personality>() {
                    save.personalities.push((id.0, *p));
                }
                save.objectives.extend(w.objective().map(|o| (id.0, o)));
            }
        }
        save
    }

    pub(crate) fn diplomacy_from_save(&mut self, save: DiplomacySave) {
        for &(id, personality) in &save.personalities {
            if let Some(mut w) = self.wizard_mut(PlayerId(id)) {
                w.put(Some(personality));
            }
        }
        for &(id, objective) in &save.objectives {
            if let Some(mut w) = self.wizard_mut(PlayerId(id)) {
                w.put(Some(objective));
            }
        }
        // A proposal that trades a spell the rules lack is dropped.
        let spells = &self.content.rules.spells;
        let mut save = save;
        save.proposals.retain(|p| match p.kind {
            KindSave::Exchange { give, get } => {
                spells.id_numbered(give.num()).is_some() && spells.id_numbered(get.num()).is_some()
            }
            KindSave::Trade { give, get } => [give.spell, get.spell]
                .into_iter()
                .flatten()
                .all(|s| spells.id_numbered(s.num()).is_some()),
            _ => true,
        });
        self.diplomacy.restore(save);
    }
}

#[cfg(test)]
mod fact_tests {
    use super::*;
    use crate::world::testutil::fresh;

    /// The diplomacy events a closure raises.
    fn raised(w: &mut World, f: impl FnOnce(&mut World)) -> Vec<DiplomacyEvent> {
        w.drain_diplomacy_events();
        f(w);
        w.drain_diplomacy_events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Diplomacy(d) => Some(d),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_hooks_report_what_happened_for_the_ais_opinions() {
        let mut w = fresh(3, 3).expect("a game");
        let ids = w.wizards();
        let (a, b, c) = (ids[0], ids[1], ids[2]);
        for &x in &ids {
            for &y in &ids {
                w.make_contact(x, y);
            }
        }
        let spell = SpellId::new(85);
        let events = raised(&mut w, |w| w.city_held_relations(a, b, true));
        assert!(events.contains(&DiplomacyEvent::AssaultFailed {
            attacker: a,
            defender: b,
            fortress: true
        }));
        let events = raised(&mut w, |w| w.conquest_relations(a, b, false));
        assert!(events.contains(&DiplomacyEvent::FortressTaken {
            winner: a,
            loser: b
        }));
        let events = raised(&mut w, |w| w.conquest_relations(a, c, true));
        assert!(events.contains(&DiplomacyEvent::WizardDestroyed {
            winner: a,
            loser: c
        }));
        let events = raised(&mut w, |w| w.cast_relations(a, spell, Some(b)));
        assert!(events.contains(&DiplomacyEvent::CitySpellCast {
            caster: a,
            owner: b,
            spell
        }));
        let events = raised(&mut w, |w| w.subvert(a, b));
        assert!(events.contains(&DiplomacyEvent::Subverted {
            caster: a,
            target: b
        }));
        // A spell nobody was hit by says nothing about a city.
        let events = raised(&mut w, |w| w.cast_relations(a, spell, None));
        assert!(!events
            .iter()
            .any(|e| matches!(e, DiplomacyEvent::CitySpellCast { .. })));
    }
}
