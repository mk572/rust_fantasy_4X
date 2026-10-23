//! Diplomacy between wizards (task 63): contact, relations on the -100..100 scale, treaties,
//! war, proposals and the per-turn drift. The rules are those of the original (`docs/diplomacy.md`).
//!
//! The core holds the state and applies the results; it never decides whether an AI wizard
//! accepts. A proposal waits as a pending proposal until its receiver answers with a
//! [`DiplomacyCommand::Answer`], and an AI's own initiatives (greetings, demands, war) are
//! commands it issues itself. [`Game::rate_proposal`] is the original's rating of a proposal,
//! offered as a helper to such an AI.
//!
//! Structure (task 179, `architecture-plan.md`): the rules are methods of [`Diplomacy`], which
//! owns the state and takes a [`DiplomacyCtx`] with what it needs of the rest of the game: a
//! read-only [`Game`], the random numbers and a list of changes it asks the game to make
//! ([`Outcome`]: gold moved, a spell learned). `Game` only lends it out for a call
//! (`with_diplomacy`) and keeps thin methods with the old names for the other modules and the
//! crates around. Tests of the rules run on a real game.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::game::rng::GameRng;
use crate::game::{Event, Game};
use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::SpellId;
use crate::wizard::magic::Retort;
use crate::wizard::objective::Personality;

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

/// What Subversion takes off every other wizard's relation to its target.
const SUBVERSION_PENALTY: i32 = 25;

/// What moved a relation (the original's reaction types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// A treaty growing, a gift, the Aura of Majesty.
    Friendship,
    /// AI wizards drawing together.
    Gravitation,
    /// Attacking a wizard one has a treaty with.
    TreatyAttack,
    /// A human army grown too strong, a wizard destroyed or a fortress taken.
    Threat,
    /// Losses in a battle.
    BattleLosses,
    /// Attacking or taking a city.
    AttackedCity,
    SpellOfMastery,
    /// An overland enchantment others dislike.
    Enchantment,
    /// A curse on a city.
    BadCitySpell,
    /// Too many cities.
    Overextension,
    AuraOfMajesty,
    /// Units too close to a wizard's city.
    Trespass,
}

impl Action {
    /// The reaction events the original ranks before the talks: a negative one is ignored
    /// between wizards at war.
    fn is_reaction(self) -> bool {
        self != Action::Trespass
    }
}

/// The last strongest thing a wizard did to another, which the other remarks on next turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastAction {
    pub action: Action,
    pub strength: i32,
    pub spell: Option<String>,
}

/// What holds between two wizards, the same both ways.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pair {
    pub contacted: bool,
    pub status: Treaty,
    /// The relation shown to the player, -100..100.
    pub visible: i32,
    /// What `visible` drifts back to.
    pub default: i32,
}

/// What one wizard holds about another (`from` about `to`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Stance {
    /// The permanent core reaction: oath breaking and gifts. Not shown.
    pub hidden: i32,
    /// How receptive `from` is to treaty, spell-exchange and peace proposals of `to`.
    pub treaty_patience: i32,
    pub exchange_patience: i32,
    pub peace_patience: i32,
    /// Turns in which `from` will not attack `to`.
    pub peace_duration: u32,
    /// How hostile `from` feels towards `to`, 0..=3.
    pub hostility: u8,
    pub stage: Stage,
    /// The treaty `to` broke with `from`, for the oath-breaker remarks.
    pub broken: Option<Treaty>,
    pub last_action: Option<LastAction>,
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
    /// Swap `give` (the proposer's spell) for `get` (the receiver's).
    Exchange {
        give: SpellId,
        get: SpellId,
    },
}

/// A proposal waiting for its receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Proposal {
    pub from: PlayerId,
    pub to: PlayerId,
    pub kind: ProposalKind,
}

/// The original's rating of a proposal (`Diplomacy_Test`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Refused,
    /// A counter-offer would be due; the original has none.
    Counter,
    Maybe,
    Accepted,
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
    /// The key of its sentence in `content/text` (task 348).
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
    RelationChanged {
        attacker: PlayerId,
        defender: PlayerId,
        change: i32,
        action: Action,
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

/// The keys of the words of the relation scale, `Hate` to `Harmony`.
pub const RELATION_KEYS: [&str; 11] = [
    "relation.hate",
    "relation.troubled",
    "relation.tense",
    "relation.restless",
    "relation.unease",
    "relation.neutral",
    "relation.relaxed",
    "relation.calm",
    "relation.peaceful",
    "relation.friendly",
    "relation.harmony",
];

/// Where a relation sits on the scale: 0 (hate) to 10 (harmony).
pub fn relation_level(relation: i32) -> usize {
    ((100 + relation.clamp(-100, 100)) / 20) as usize
}

/// The key of the word for a relation.
pub fn relation_key(relation: i32) -> &'static str {
    RELATION_KEYS[relation_level(relation)]
}

/// A pact needs a relation above this, an alliance above `ALLIANCE_RELATION`.
pub const PACT_RELATION: i32 = 10;
pub const ALLIANCE_RELATION: i32 = 50;
/// The land size the "too many cities" reaction assumes (0 small, 1 medium, 2 large) until
/// the map size is kept in a game.
const LAND_SIZE: i32 = 1;

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

/// The wizards `player` meets by looking at the squares within `radius` of `at`: the owners of
/// the cities and of the units it can make out there, in player order.
pub fn met_near(view: &Game, player: PlayerId, at: Position, radius: u32) -> Vec<PlayerId> {
    owners(view.foreign_sightings_near(player, at, radius))
}

/// The wizards `player` meets by what it sees now: the owners of the cities and of the units it
/// can make out on the squares in its sight, in player order.
pub fn met_in_sight(view: &Game, player: PlayerId) -> Vec<PlayerId> {
    owners(view.foreign_sightings(player))
}

fn owners(sightings: Vec<crate::game::view::Sighting>) -> Vec<PlayerId> {
    let mut met: Vec<PlayerId> = sightings.into_iter().map(|s| s.owner).collect();
    met.sort();
    met.dedup();
    met
}

/// The total cost of a wizard's units, the original's army strength.
pub fn army_strength(view: &Game, player: PlayerId) -> i32 {
    view.units_of(player).iter().map(|u| u.cost as i32).sum()
}

/// What the rules of diplomacy need from the rest of the game, for one call: the read-only
/// view of the world, the random numbers, and the difficulty. What they must change outside
/// diplomacy (gold, a spell learned) they leave in `out` for the game to apply.
pub struct DiplomacyCtx<'a> {
    pub view: &'a Game,
    pub rng: &'a mut GameRng,
    pub out: Vec<Outcome>,
}

/// A change diplomacy asks of the rest of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Move gold from one wizard's treasury to another's.
    Gold {
        from: PlayerId,
        to: PlayerId,
        amount: i32,
    },
    /// The wizard learns the spell, as if it had researched it.
    Spell { player: PlayerId, spell: SpellId },
}

/// Every wizard still in the game, in player order.
pub fn wizards(view: &Game) -> Vec<PlayerId> {
    let mut out = view.living_wizards();
    out.sort();
    out
}

/// The relation two wizards start with, from their books (`Init_Diplomatic_Relations`).
fn start_relation(view: &Game, a: PlayerId, b: PlayerId) -> i32 {
    let books =
        |p: PlayerId, realm: &str| -> i32 { view.wizard(p).map_or(0, |m| m.books_in(realm)) };
    let max_death = books(a, "death").max(books(b, "death"));
    let sum_life = books(a, "life") + books(b, "life");
    let max_chaos = books(a, "chaos").max(books(b, "chaos"));
    let mut score = if sum_life > 0 && max_death > 0 {
        -(sum_life + max_death) * 5
    } else {
        sum_life * 2 - max_death * 3
    };
    score -= max_chaos * 2;
    for realm in ["sorcery", "chaos", "nature", "life"] {
        score += books(a, realm).min(books(b, realm)) * 2;
    }
    score.max(-90)
}

fn charismatic(view: &Game, wizard: PlayerId) -> bool {
    view.wizard(wizard)
        .is_some_and(|m| m.has(Retort::Charismatic))
}

/// How a threat weighs: the asker's army against the judge's, as the original rates it.
fn threat_modifier(view: &Game, asker: PlayerId, judge: PlayerId) -> i32 {
    let (mine, theirs) = (army_strength(view, asker), army_strength(view, judge));
    if theirs < mine {
        if theirs > 0 {
            -50 + mine * 100 / theirs
        } else {
            50
        }
    } else if mine > 0 {
        -50 + theirs * 100 / mine
    } else {
        -50
    }
}

/// The enchantments that sour or sweeten relations: the change, whom it touches, and the
/// divisor of the once-a-turn reaction (`Change_Relations_For_Enchantments`, whose cast-time
/// divisor is 1 and, for the Spell of Mastery, 2 while it is being cast).
fn enchantment_reaction(view: &Game, spell: &str, other: PlayerId) -> Option<(i32, Action)> {
    let books = |realm: &str| view.wizard(other).map_or(0, |m| m.books_in(realm));
    let reaction = match spell {
        "spell_of_mastery" => (-50, Action::SpellOfMastery),
        "eternal_night" => (-12, Action::Enchantment),
        "evil_omens" if books("life") > 0 || books("nature") > 0 => (-20, Action::Enchantment),
        "zombie_mastery" => (-14, Action::Enchantment),
        "aura_of_majesty" => (10, Action::Friendship),
        "wind_mastery" | "suppress_magic" => (-4, Action::Enchantment),
        "natures_wrath" if books("chaos") > 0 || books("death") > 0 => (-20, Action::Enchantment),
        "chaos_surge" if books("chaos") == 0 => (-10, Action::Enchantment),
        "doom_mastery" => (-8, Action::Enchantment),
        "great_wasting" => (-20, Action::Enchantment),
        "meteor_storms" => (-15, Action::Enchantment),
        "armageddon" => (-25, Action::Enchantment),
        "tranquility" if books("chaos") > 0 => (-20, Action::Enchantment),
        "life_force" if books("death") > 0 => (-20, Action::Enchantment),
        "crusade" => (-10, Action::Enchantment),
        "holy_arms" => (-5, Action::Enchantment),
        _ => return None,
    };
    Some(reaction)
}

impl Diplomacy {
    fn pair_mut(&mut self, view: &Game, a: PlayerId, b: PlayerId) -> &mut Pair {
        self.pairs.entry(key(a, b)).or_insert_with(|| {
            let start = start_relation(view, a, b);
            Pair {
                contacted: false,
                status: Treaty::None,
                visible: start,
                default: start,
            }
        })
    }

    pub fn have_met(&self, a: PlayerId, b: PlayerId) -> bool {
        self.pair(a, b).is_some_and(|p| p.contacted)
    }

    /// The treaty between two wizards.
    pub fn treaty(&self, a: PlayerId, b: PlayerId) -> Treaty {
        self.pair(a, b).map_or(Treaty::None, |p| p.status)
    }

    /// The relation between two wizards as the player sees it (0 before they have a record).
    pub fn relation(&self, view: &Game, a: PlayerId, b: PlayerId) -> i32 {
        self.pair(a, b)
            .map_or_else(|| start_relation(view, a, b), |p| p.visible)
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
        if a == b || !ctx.view.is_wizard(a) || !ctx.view.is_wizard(b) {
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
        if !ctx.view.is_wizard(player) {
            return;
        }
        for other in met_near(ctx.view, player, at, radius) {
            self.make_contact(ctx, player, other);
        }
    }

    /// Once a turn: whoever sees another wizard's city or visible unit meets it, and
    /// Nature Awareness meets everyone.
    fn contact_sweep(&mut self, ctx: &mut DiplomacyCtx<'_>) {
        let wizards = wizards(ctx.view);
        for &player in &wizards {
            if ctx.view.has_global(player, "nature_awareness") {
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

    /// What `attacker` did moves `defender`'s feeling about it (`Change_Relations`). The human
    /// wizard is never the defender: the original does not change the human's own side.
    pub fn change_relations(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        value: i32,
        attacker: PlayerId,
        defender: PlayerId,
        action: Action,
        spell: Option<&str>,
    ) {
        if value == 0
            || attacker == defender
            || !ctx.view.is_wizard(attacker)
            || !ctx.view.is_wizard(defender)
            || !self.have_met(attacker, defender)
            || ctx.view.human() == Some(defender)
        {
            return;
        }
        let rel = self.relation(ctx.view, attacker, defender);
        let mut v = value;
        if v > 0 {
            if rel < 0 {
                v *= 2;
                if rel + v > 10 {
                    v = 10 - rel;
                }
            } else {
                v /= rel / 25 + 1;
            }
        } else if rel > 0 {
            v *= 2;
        } else {
            v /= -rel / 25 + 1;
        }
        match ctx.view.personality(defender) {
            Personality::Lawful if v < 0 => v *= 2,
            Personality::Maniacal if v < 0 => v = v * 3 / 2,
            Personality::Maniacal if v > 0 => v = v * 3 / 4,
            _ => {}
        }
        let before_charisma = v;
        if charismatic(ctx.view, attacker) {
            if v > 0 {
                v *= 2;
            } else if v < 0 {
                v /= 2;
            }
        }
        let at_war = self.treaty(defender, attacker) == Treaty::War;
        let mut applied = v;
        if at_war && action.is_reaction() && v < 0 {
            // Wars are not made worse by what one does in them; a quarter wears down the
            // peace patience of a human attacker.
            if ctx.view.human() == Some(attacker) {
                self.stance_mut(defender, attacker).peace_patience += v / 4;
                self.stance_mut(attacker, defender).peace_patience -= v / 4;
            }
            applied = 0;
        }
        // After turn 200 the original helps its AIs along: friendships twice as strong, enmities
        // weaker with the difficulty. This game gives its AIs no help (design.md section 8).
        if applied != 0 {
            let pair = self.pair_mut(ctx.view, attacker, defender);
            pair.visible = (pair.visible + applied).clamp(-100, 100);
            // Patience moves for the defender and is copied to the attacker.
            let d = self.stance_mut(defender, attacker);
            d.treaty_patience += applied;
            d.exchange_patience += applied;
            let (t, e) = (d.treaty_patience, d.exchange_patience);
            let a = self.stance_mut(attacker, defender);
            a.treaty_patience = t;
            a.exchange_patience = e;
            if before_charisma < 0 {
                self.stance_mut(attacker, defender).peace_patience -= before_charisma / 4;
                self.stance_mut(defender, attacker).peace_patience += before_charisma / 4;
            } else {
                self.stance_mut(defender, attacker).peace_patience += before_charisma;
            }
            if action == Action::SpellOfMastery && ctx.rng.roll(100) < 11 {
                self.declare_war(ctx, attacker, defender);
            }
            if ctx.view.human() == Some(attacker) && action != Action::AuraOfMajesty {
                let stance = self.stance_mut(defender, attacker);
                if stance
                    .last_action
                    .as_ref()
                    .is_none_or(|l| applied.abs() > l.strength.abs())
                {
                    stance.last_action = Some(LastAction {
                        action,
                        strength: applied,
                        spell: spell.map(str::to_string),
                    });
                }
            }
            self.queue.push(DiplomacyEvent::RelationChanged {
                attacker,
                defender,
                change: applied,
                action,
            });
        }
        let status = self.treaty(defender, attacker);
        let pair = self.pair_mut(ctx.view, attacker, defender);
        if status == Treaty::War {
            pair.visible = pair.visible.min(-25);
        }
        if status != Treaty::Alliance {
            pair.visible = pair.visible.min(65);
        }
        let hostility = match status {
            Treaty::War => Some(3),
            Treaty::Alliance => Some(0),
            _ => None,
        };
        if let Some(h) = hostility {
            self.stance_mut(defender, attacker).hostility = h;
        }
    }

    // ---- treaties and war ------------------------------------------------------

    fn set_status(&mut self, ctx: &mut DiplomacyCtx<'_>, a: PlayerId, b: PlayerId, treaty: Treaty) {
        self.pair_mut(ctx.view, a, b).status = treaty;
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
        self.stance_mut(a, b).hostility = 0;
        self.stance_mut(b, a).hostility = 0;
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
        if !ctx.view.is_wizard(breaker) || !ctx.view.is_wizard(victim) {
            return;
        }
        let treaty = self.treaty(breaker, victim);
        if treaty == Treaty::War {
            return;
        }
        let mut penalty = match treaty {
            Treaty::Pact => 10,
            Treaty::Alliance => 20,
            _ => 0,
        };
        if ctx.view.personality(breaker) == Personality::Lawful {
            penalty *= 2;
        }
        {
            let h = &mut self.stance_mut(breaker, victim).hidden;
            *h = (*h - penalty).max(-100);
        }
        for other in wizards(ctx.view) {
            if other != breaker && other != victim {
                let h = &mut self.stance_mut(other, victim).hidden;
                *h = (*h - 5).max(-100);
            }
        }
        let broke_a_treaty = penalty != 0;
        let drop = if broke_a_treaty {
            ctx.rng.roll(20) as i32
        } else {
            0
        };
        let pair = self.pair_mut(ctx.view, breaker, victim);
        if treaty == Treaty::Alliance {
            pair.default = (pair.default - penalty).max(-100);
        }
        pair.visible = (pair.visible - drop).max(-100);
        if broke_a_treaty {
            self.stance_mut(breaker, victim).broken = Some(treaty);
            self.stance_mut(victim, breaker).broken = Some(treaty);
        }
        self.set_status(ctx, breaker, victim, Treaty::None);
        for (from, to) in [(breaker, victim), (victim, breaker)] {
            let s = self.stance_mut(from, to);
            s.treaty_patience = -200;
            s.exchange_patience = -200;
            s.peace_patience = -200;
        }
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
        if attacker == defender || !ctx.view.is_wizard(attacker) || !ctx.view.is_wizard(defender) {
            return;
        }
        // Wizards at war know each other.
        self.make_contact(ctx, attacker, defender);
        if self.treaty(attacker, defender) == Treaty::War {
            return;
        }
        self.break_treaties(ctx, attacker, defender);
        let relation = -75 - ctx.rng.roll(25) as i32;
        let pair = self.pair_mut(ctx.view, attacker, defender);
        pair.default = (pair.default - 5).max(-100);
        pair.visible = relation;
        self.set_status(ctx, attacker, defender, Treaty::War);
        for (from, to) in [(attacker, defender), (defender, attacker)] {
            let s = self.stance_mut(from, to);
            s.treaty_patience = -200;
            s.exchange_patience = -200;
            s.peace_patience = -130;
            s.hostility = 3;
        }
        self.queue
            .push(DiplomacyEvent::WarDeclared { attacker, defender });
    }

    /// The end of a war (`Declare_Peace`): no treaty, a better relation, and `peace_duration`
    /// turns in which a human is left alone.
    pub fn declare_peace(&mut self, ctx: &mut DiplomacyCtx<'_>, a: PlayerId, b: PlayerId) {
        self.start_treaty(ctx, a, b, Treaty::None);
        let pair = self.pair_mut(ctx.view, a, b);
        pair.visible = (pair.visible + 40).min(100);
        for (human, other) in [(a, b), (b, a)] {
            if ctx.view.human() == Some(human) {
                let turns = 7 + ctx.rng.roll(6);
                self.stance_mut(other, human).peace_duration = turns;
            }
        }
    }

    // ---- the rating of a proposal ----------------------------------------------

    /// The original's `Diplomacy_Test` of a proposal that `asker` makes to `judge`: an AI that
    /// answers proposals can use it as its starting logic. It draws on the game's random
    /// numbers and wears down the asker's patience as the original does.
    pub fn rate_proposal(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        asker: PlayerId,
        judge: PlayerId,
        kind: ProposalKind,
    ) -> Verdict {
        let relation = self.relation(ctx.view, asker, judge);
        let (type_modifier, early) = match kind {
            ProposalKind::Pact => (75, (relation <= PACT_RELATION).then_some(Verdict::Refused)),
            ProposalKind::Alliance => (
                125,
                (relation <= ALLIANCE_RELATION).then_some(Verdict::Refused),
            ),
            ProposalKind::Peace => (60, None),
            ProposalKind::WarOn(third) => {
                let allied_with_both = self.treaty(asker, judge) == Treaty::Alliance
                    && self.treaty(asker, third) == Treaty::Alliance;
                let early = if allied_with_both {
                    Some(if ctx.rng.roll(8) < 5 {
                        Verdict::Accepted
                    } else {
                        Verdict::Refused
                    })
                } else {
                    None
                };
                (150 + self.relation(ctx.view, judge, third), early)
            }
            ProposalKind::BreakAllianceWith(third) => (
                175 + self.relation(ctx.view, judge, third),
                (relation <= 24).then_some(Verdict::Maybe),
            ),
            // The original swaps spells without a test: both must have one the other lacks.
            ProposalKind::Exchange { .. } => (0, Some(Verdict::Accepted)),
        };
        if let Some(v) = early {
            return v;
        }
        let stance = self.stance(asker, judge).cloned().unwrap_or_default();
        let relations = stance.treaty_patience
            + stance.hidden
            + relation
            + ctx.view.personality(judge).modifier();
        let mut final_value = relations + ctx.rng.roll(100) as i32 - type_modifier
            + threat_modifier(ctx.view, asker, judge);
        if self.treaty(asker, judge) == Treaty::Alliance {
            final_value += 40;
        }
        let wear = 20 + ctx.rng.roll(30) as i32;
        let s = self.stance_mut(asker, judge);
        s.treaty_patience -= 10 + wear;
        s.exchange_patience -= 10;
        s.peace_patience -= 10;
        let verdict = match final_value {
            v if v < -75 => Verdict::Refused,
            v if v < -50 => Verdict::Counter,
            v if v < 0 => Verdict::Maybe,
            _ => Verdict::Accepted,
        };
        // The original has no counter-offers: a doubtful answer is a yes half the time.
        match verdict {
            Verdict::Counter | Verdict::Maybe if ctx.rng.roll(2) == 1 => Verdict::Accepted,
            v => v,
        }
    }

    // ---- proposals and the talk ------------------------------------------------

    /// Whether `from` may put `kind` to `to` now: the entries the original greys out.
    pub fn check_proposal(
        &self,
        view: &Game,
        from: PlayerId,
        to: PlayerId,
        kind: ProposalKind,
    ) -> Result<(), DiplomacyError> {
        if !view.is_wizard(from) || !view.is_wizard(to) {
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
                && view.is_wizard(third)
                && self.have_met(from, third)
                && self.have_met(to, third)
        };
        let ok = match kind {
            ProposalKind::Pact => status == Treaty::None,
            ProposalKind::Alliance => matches!(status, Treaty::None | Treaty::Pact),
            ProposalKind::Peace => status == Treaty::War,
            ProposalKind::WarOn(third) => {
                status != Treaty::War && known_third(third) && self.treaty(to, third) != Treaty::War
            }
            ProposalKind::BreakAllianceWith(third) => {
                status != Treaty::War
                    && known_third(third)
                    && self.treaty(to, third) == Treaty::Alliance
            }
            ProposalKind::Exchange { give, get } => {
                status != Treaty::War
                    && view.wizard(from).is_some_and(|m| m.knows(give))
                    && view.wizard(to).is_some_and(|m| m.knows(get))
                    && view.wizard(to).is_some_and(|m| !m.knows(give))
                    && view.wizard(from).is_some_and(|m| !m.knows(get))
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
        let proposal = Proposal { from, to, kind };
        self.proposals.push(proposal);
        self.queue.push(DiplomacyEvent::ProposalMade(proposal));
        Ok(())
    }

    /// The receiver's answer: an accepted proposal takes effect.
    fn answer(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        player: PlayerId,
        from: PlayerId,
        accept: bool,
    ) -> Result<(), DiplomacyError> {
        let at = self
            .proposals
            .iter()
            .position(|p| p.from == from && p.to == player)
            .ok_or(DiplomacyError::NothingPending)?;
        let proposal = self.proposals.remove(at);
        let accepted = accept
            && self
                .check_proposal(ctx.view, from, player, proposal.kind)
                .is_ok();
        if accepted {
            self.carry_out(ctx, proposal);
        }
        self.queue.push(DiplomacyEvent::ProposalAnswered {
            proposal,
            accepted,
            lapsed: false,
        });
        Ok(())
    }

    fn carry_out(&mut self, ctx: &mut DiplomacyCtx<'_>, p: Proposal) {
        match p.kind {
            ProposalKind::Pact => self.start_treaty(ctx, p.from, p.to, Treaty::Pact),
            ProposalKind::Alliance => self.start_treaty(ctx, p.from, p.to, Treaty::Alliance),
            ProposalKind::Peace => {
                self.declare_peace(ctx, p.from, p.to);
                // Asking for peace wears on the asker's patience twice more.
                let s = self.stance_mut(p.from, p.to);
                s.treaty_patience -= 20;
                s.exchange_patience -= 20;
                s.peace_patience -= 20;
            }
            ProposalKind::WarOn(third) => self.declare_war(ctx, p.to, third),
            ProposalKind::BreakAllianceWith(third) => self.break_treaties(ctx, p.to, third),
            ProposalKind::Exchange { give, get } => {
                ctx.out.push(Outcome::Spell {
                    player: p.to,
                    spell: give,
                });
                ctx.out.push(Outcome::Spell {
                    player: p.from,
                    spell: get,
                });
                // Giving the better spell pleases the receiver.
                let cost = |s: SpellId| ctx.view.content().rules.spells.get(s).research_cost;
                if cost(give) > cost(get) {
                    self.change_relations(ctx, 5, p.from, p.to, Action::Friendship, None);
                }
                let s = self.stance_mut(p.from, p.to);
                s.treaty_patience -= 20;
                s.exchange_patience = (s.exchange_patience - 30).min(50);
                s.peace_patience -= 20;
            }
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
        // The original offers a quarter, half, three quarters or all of the receiver's gold: the
        // larger the share, the warmer the thanks.
        let reserve = ctx.view.player(to).map_or(0, |p| p.treasury().gold).max(1);
        let choice = ((amount * 4 + reserve - 1) / reserve).clamp(1, 4);
        let base = if army_strength(ctx.view, to) > 0 {
            choice * 4
        } else {
            8
        };
        let gain = (base * ctx.rng.roll(3) as i32 / 10).min(100);
        ctx.out.push(Outcome::Gold { from, to, amount });
        let pair = self.pair_mut(ctx.view, from, to);
        pair.visible = (pair.visible + gain).min(100);
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
        let relation = self.relation(ctx.view, from, to);
        let base = if relation < 0 {
            20
        } else {
            (100 - relation) / 10
        };
        let gain = base * (ctx.rng.roll(8) as i32 + ctx.rng.roll(8) as i32) / 10;
        let hidden = 2 + ctx.rng.roll(8) as i32;
        let pair = self.pair_mut(ctx.view, from, to);
        pair.visible = (pair.visible + gain).min(100);
        let s = self.stance_mut(to, from);
        s.peace_patience = (s.peace_patience + gain).min(100);
        s.hidden = (s.hidden + hidden).min(30);
        self.queue
            .push(DiplomacyEvent::SpellGiven { from, to, spell });
        Ok(())
    }

    fn check_gift(&self, view: &Game, from: PlayerId, to: PlayerId) -> Result<(), DiplomacyError> {
        if !view.is_wizard(from) || !view.is_wizard(to) {
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
    ) -> Vec<Event> {
        // A banished wizard starts nothing with the others; it may still answer an offer.
        let actor = match command {
            DiplomacyCommand::Greeted { .. } | DiplomacyCommand::Answer { .. } => None,
            DiplomacyCommand::BreakTreaty { player, .. }
            | DiplomacyCommand::DeclareWar { player, .. } => Some(player),
            DiplomacyCommand::Propose { from, .. }
            | DiplomacyCommand::GiveGold { from, .. }
            | DiplomacyCommand::GiveSpell { from, .. } => Some(from),
        };
        if actor.is_some_and(|a| ctx.view.wizard(a).is_some_and(|w| w.is_banished())) {
            return vec![Event::Rejected(crate::game::Rejection::Diplomacy(
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
            } => self.answer(ctx, player, from, accept),
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
        };
        let mut events = Vec::new();
        if let Err(e) = result {
            events.push(Event::Rejected(crate::game::Rejection::Diplomacy(e)));
        }
        events
    }

    // ---- the turn --------------------------------------------------------------

    /// Every other wizard reacts to `caster`'s enchantment (`Change_Relations_For_Enchantments`).
    pub(crate) fn enchantment_relations(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        caster: PlayerId,
        spell: &str,
        divisor: i32,
    ) {
        for other in wizards(ctx.view) {
            if other == caster {
                continue;
            }
            if let Some((value, action)) = enchantment_reaction(ctx.view, spell, other) {
                self.change_relations(ctx, value / divisor, caster, other, action, Some(spell));
            }
        }
    }

    /// Subversion (`Apply_Subversion`): the relation of every other wizard that has met `target`
    /// falls by [`SUBVERSION_PENALTY`], and the peace between them is over. The original's code
    /// then overwrites the new relation with the old one, which leaves only the peace ended; the
    /// help's rule is followed here (`docs/help-vs-implementation-conflicts.md`, B33).
    pub(crate) fn subvert(&mut self, ctx: &mut DiplomacyCtx<'_>, target: PlayerId) {
        let others: Vec<PlayerId> = ctx
            .view
            .player_ids()
            .into_iter()
            .filter(|&w| w != target && ctx.view.is_wizard(w) && ctx.view.human() != Some(w))
            .filter(|&w| self.have_met(w, target))
            .collect();
        for w in others {
            let pair = self.pair_mut(ctx.view, w, target);
            pair.visible = (pair.visible - SUBVERSION_PENALTY).max(-100);
            self.stance_mut(w, target).peace_duration = 0;
            self.stance_mut(target, w).peace_duration = 0;
        }
    }

    /// What a curse on a city costs in relations (`Change_Relations_For_Bad_City_Spell`).
    fn bad_city_spell_value(spell: &str) -> Option<i32> {
        Some(match spell {
            "earthquake" => -9,
            "corruption" => -25,
            "raise_volcano" => -50,
            "chaos_rift" => -5,
            "call_the_void" => -20,
            "evil_presence" => -5,
            "famine" => -5,
            "cursed_lands" => -5,
            "pestilence" => -8,
            _ => return None,
        })
    }

    /// A spell was cast: the wizards it offends take note. `target_owner` is whose city it fell on.
    pub(crate) fn cast_relations(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        caster: PlayerId,
        spell: SpellId,
        target_owner: Option<PlayerId>,
    ) {
        let id = ctx.view.content().rules.spells.get(spell).id.clone();
        if let (Some(value), Some(owner)) = (Self::bad_city_spell_value(&id), target_owner) {
            self.change_relations(ctx, value, caster, owner, Action::BadCitySpell, Some(&id));
        }
        // Lasting enchantments on the wizard are noticed as they are cast (the Spell of Mastery
        // as it is begun, see `start_cast`).
        if id != crate::game::endgame::SPELL_OF_MASTERY {
            self.enchantment_relations(ctx, caster, &id, 1);
        }
    }

    /// A wizard begins the Spell of Mastery: the others take note at once.
    pub(crate) fn begin_cast_relations(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        player: PlayerId,
        spell: SpellId,
    ) {
        if ctx.view.content().rules.spells.get(spell).id == crate::game::endgame::SPELL_OF_MASTERY {
            self.enchantment_relations(ctx, player, crate::game::endgame::SPELL_OF_MASTERY, 1);
        }
    }

    /// Attacking a wizard one has a treaty with breaks it first; the attacker is the breaker
    /// (the original has the victim break it, a mistake of the decompiled code).
    pub(crate) fn attack_relations(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        attacker: PlayerId,
        defender: PlayerId,
    ) {
        if !ctx.view.is_wizard(attacker) || !ctx.view.is_wizard(defender) {
            return;
        }
        self.make_contact(ctx, attacker, defender);
        if matches!(
            self.treaty(attacker, defender),
            Treaty::Pact | Treaty::Alliance
        ) {
            self.change_relations(ctx, -40, attacker, defender, Action::TreatyAttack, None);
            self.break_treaties(ctx, attacker, defender);
        }
    }

    /// `winner` took a city of `loser` that had people in it: -40, and the war the original
    /// declares with it.
    pub(crate) fn city_taken_relations(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        winner: PlayerId,
        loser: PlayerId,
    ) {
        if !ctx.view.is_wizard(winner) || !ctx.view.is_wizard(loser) {
            return;
        }
        self.make_contact(ctx, winner, loser);
        self.change_relations(ctx, -40, winner, loser, Action::AttackedCity, None);
        self.declare_war(ctx, winner, loser);
    }

    /// An attack on a city failed: -60 if it held the wizard's fortress, else -20.
    pub(crate) fn city_held_relations(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        attacker: PlayerId,
        defender: PlayerId,
        fortress_city: bool,
    ) {
        if !ctx.view.is_wizard(attacker) || !ctx.view.is_wizard(defender) {
            return;
        }
        let value = if fortress_city { -60 } else { -20 };
        self.change_relations(ctx, value, attacker, defender, Action::AttackedCity, None);
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
        for other in wizards(ctx.view) {
            if other == loser {
                continue;
            }
            if destroyed {
                let pair = self.pair_mut(ctx.view, other, loser);
                pair.status = Treaty::None;
                pair.contacted = false;
            }
            if other != winner {
                let value = if destroyed { -40 } else { -20 };
                self.change_relations(ctx, value, winner, other, Action::Threat, None);
            }
        }
    }

    /// The part of the turn that belongs to diplomacy (`Next_Turn_Calc` order): proposals that
    /// nobody answered lapse, peace runs down, the enchantments in force sour or sweeten,
    /// relations drift, new wizards are met, and the patience comes back.
    pub(crate) fn process_diplomacy(&mut self, ctx: &mut DiplomacyCtx<'_>) {
        let lapsed = std::mem::take(&mut self.proposals);
        for proposal in lapsed {
            self.queue.push(DiplomacyEvent::ProposalAnswered {
                proposal,
                accepted: false,
                lapsed: true,
            });
        }
        for s in self.stances.values_mut() {
            s.peace_duration = s.peace_duration.saturating_sub(1);
        }
        self.enchantments_in_force(ctx);
        self.diplomacy_growth(ctx);
        self.contact_sweep(ctx);
        // Patience comes back, and heals by itself a little.
        let keys: Vec<(u8, u8)> = self.stances.keys().copied().collect();
        for &(a, b) in &keys {
            let s = self.stance_mut(PlayerId(a), PlayerId(b));
            for v in [
                &mut s.treaty_patience,
                &mut s.exchange_patience,
                &mut s.peace_patience,
            ] {
                if *v < 100 {
                    *v += 10;
                }
            }
        }
        for (a, b) in keys {
            for slot in 0..3 {
                // Each runs twice: once while it is below zero, once while below 50.
                let first = ctx.rng.roll(5) as i32;
                let second = ctx.rng.roll(5) as i32;
                let s = self.stance_mut(PlayerId(a), PlayerId(b));
                let v = match slot {
                    0 => &mut s.treaty_patience,
                    1 => &mut s.exchange_patience,
                    _ => &mut s.peace_patience,
                };
                if *v < 0 {
                    *v += first;
                }
                if *v < 50 {
                    *v += second;
                }
            }
            let s = self.stance_mut(PlayerId(a), PlayerId(b));
            s.peace_patience = s.peace_patience.clamp(-200, 200);
            s.last_action = None;
        }
    }

    /// The enchantments wizards keep going sour or sweeten relations a little each turn
    /// (`Diplomacy_Growth_For_Enchantments`).
    fn enchantments_in_force(&mut self, ctx: &mut DiplomacyCtx<'_>) {
        for wizard in wizards(ctx.view) {
            let mut active: Vec<(String, i32)> = ctx
                .view
                .wizard(wizard)
                .map(|m| {
                    m.global()
                        .iter()
                        .map(|&s| (ctx.view.content().rules.spells.get(s).id.clone(), 10))
                        .collect()
                })
                .unwrap_or_default();
            // The Spell of Mastery is felt while it is being cast.
            let casting_mastery = ctx
                .view
                .wizard(wizard)
                .and_then(|m| m.casting())
                .is_some_and(|c| {
                    ctx.view.content().rules.spells.get(c.spell).id
                        == crate::game::endgame::SPELL_OF_MASTERY
                });
            if casting_mastery {
                active.push((crate::game::endgame::SPELL_OF_MASTERY.to_string(), 2));
            }
            for (spell, divisor) in active {
                self.enchantment_relations(ctx, wizard, &spell, divisor);
            }
        }
    }

    /// Relations grow in treaties, react to a human army or empire grown too large, and drift
    /// back towards where they started (`Diplomacy_Growth`).
    fn diplomacy_growth(&mut self, ctx: &mut DiplomacyCtx<'_>) {
        let wizards = wizards(ctx.view);
        for (i, &a) in wizards.iter().enumerate() {
            for &b in &wizards[i + 1..] {
                if !self.have_met(a, b) || ctx.rng.roll(2) != 1 {
                    continue;
                }
                match self.treaty(a, b) {
                    Treaty::Pact => {
                        let v = ctx.rng.roll(3) as i32;
                        self.change_relations(ctx, v, a, b, Action::Friendship, None);
                    }
                    Treaty::Alliance => {
                        let v = ctx.rng.roll(6) as i32;
                        self.change_relations(ctx, v, a, b, Action::Friendship, None);
                    }
                    _ => {}
                }
            }
        }
        if let Some(human) = ctx.view.human() {
            let strength = army_strength(ctx.view, human);
            if strength > 0 {
                for &ai in &wizards {
                    let theirs = army_strength(ctx.view, ai);
                    if ai != human && theirs > 0 && theirs < strength {
                        let threshold = strength * 50 / theirs;
                        if ctx.rng.roll(100) as i32 >= threshold && ctx.rng.roll(20) == 1 {
                            self.change_relations(ctx, -10, human, ai, Action::Threat, None);
                        }
                    }
                }
            }
            if ctx.view.turn.is_multiple_of(2) {
                let cities = ctx.view.city_count(human) as i32;
                for &ai in &wizards {
                    if ai == human || (LAND_SIZE + 1) * 3 >= cities || ctx.rng.roll(4) != 1 {
                        continue;
                    }
                    let surplus = cities - (LAND_SIZE + 1) * 3;
                    let divisor = (LAND_SIZE + 6) - i32::from(ctx.view.difficulty());
                    let mut value = (-surplus / divisor.max(1)) * ctx.rng.roll(4) as i32 / 3 * 4;
                    value = value.min(-15);
                    if self.treaty(ai, human) != Treaty::Alliance && self.have_met(ai, human) {
                        self.change_relations(ctx, value, human, ai, Action::Overextension, None);
                    }
                }
            }
        }
        // Drift back to the start, unless something happened lately. Between an AI and the
        // human the AI's record rules; between two AIs the lower number's.
        for (i, &a) in wizards.iter().enumerate() {
            for &b in &wizards[i + 1..] {
                if !self.have_met(a, b) {
                    continue;
                }
                let (ruler, other) = if ctx.view.human() == Some(a) {
                    (b, a)
                } else {
                    (a, b)
                };
                if self
                    .stance(ruler, other)
                    .is_some_and(|s| s.last_action.is_some())
                {
                    continue;
                }
                let gravitation = if ctx.rng.roll(105) as i32 > self.relation(ctx.view, a, b).abs()
                {
                    ctx.rng.roll(2) as i32
                } else {
                    0
                };
                let turn = ctx.view.turn;
                let pair = self.pair_mut(ctx.view, a, b);
                if pair.visible < pair.default {
                    pair.visible = (pair.visible + gravitation).min(pair.default);
                } else if turn.is_multiple_of(10) {
                    pair.visible = (pair.visible - gravitation).max(pair.default);
                }
                pair.visible = pair.visible.clamp(-100, 100);
            }
        }
    }
}

impl Game {
    /// Lend diplomacy out for one call: its state and the random numbers are taken out of the
    /// game, so the rules can run over a read-only view of what is left (`design.md` section 3),
    /// and what they ask of the rest of the game is applied afterwards.
    fn with_diplomacy<R>(
        &mut self,
        f: impl FnOnce(&mut Diplomacy, &mut DiplomacyCtx<'_>) -> R,
    ) -> R {
        let mut diplomacy = std::mem::take(&mut self.diplomacy);
        let mut rng = std::mem::replace(&mut self.rng, GameRng::lent());
        let (result, out) = {
            let mut ctx = DiplomacyCtx {
                view: &*self,
                rng: &mut rng,
                out: Vec::new(),
            };
            let result = f(&mut diplomacy, &mut ctx);
            (result, ctx.out)
        };
        self.rng = rng;
        self.diplomacy = diplomacy;
        for outcome in out {
            match outcome {
                Outcome::Gold { from, to, amount } => {
                    if let Some(t) = self.treasury_mut(from) {
                        t.gold -= amount;
                    }
                    if let Some(t) = self.treasury_mut(to) {
                        t.gold += amount;
                    }
                }
                Outcome::Spell { player, spell } => self.learn_spell(player, spell),
            }
        }
        result
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

    /// The relation between two wizards as the player sees it.
    pub fn relation(&self, a: PlayerId, b: PlayerId) -> i32 {
        self.diplomacy.relation(self, a, b)
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

    /// What `attacker` did moves `defender`'s feeling about it (`Change_Relations`).
    pub fn change_relations(
        &mut self,
        value: i32,
        attacker: PlayerId,
        defender: PlayerId,
        action: Action,
        spell: Option<&str>,
    ) {
        self.with_diplomacy(|d, ctx| {
            d.change_relations(ctx, value, attacker, defender, action, spell)
        });
    }

    /// Subversion on `target`: every other wizard who has met them thinks 25 points worse of them
    /// and will not hold to a peace with them. The human's own feelings are left alone, as in the
    /// original (`Apply_Subversion`).
    pub(crate) fn subvert(&mut self, target: PlayerId) {
        self.with_diplomacy(|d, ctx| d.subvert(ctx, target));
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
    pub fn declare_peace(&mut self, a: PlayerId, b: PlayerId) {
        self.with_diplomacy(|d, ctx| d.declare_peace(ctx, a, b));
    }

    /// The total cost of a wizard's units, the original's army strength.
    pub fn army_strength(&self, player: PlayerId) -> i32 {
        army_strength(self, player)
    }

    /// The original's `Diplomacy_Test` of a proposal that `asker` makes to `judge`: an AI that
    /// answers proposals can use it as its starting logic. It draws on the game's random
    /// numbers and wears down the asker's patience as the original does.
    pub fn rate_proposal(
        &mut self,
        asker: PlayerId,
        judge: PlayerId,
        kind: ProposalKind,
    ) -> Verdict {
        self.with_diplomacy(|d, ctx| d.rate_proposal(ctx, asker, judge, kind))
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
    pub(crate) fn diplomacy_command(&mut self, command: DiplomacyCommand) -> Vec<Event> {
        self.with_diplomacy(|d, ctx| d.command(ctx, command))
    }

    #[cfg(test)]
    pub(crate) fn enchantment_relations(&mut self, caster: PlayerId, spell: &str, divisor: i32) {
        self.with_diplomacy(|d, ctx| d.enchantment_relations(ctx, caster, spell, divisor));
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KindSave {
    Pact,
    Alliance,
    Peace,
    WarOn(u8),
    BreakAllianceWith(u8),
    Exchange { give: String, get: String },
}

impl Diplomacy {
    pub(crate) fn to_save(&self, spell_id: &dyn Fn(SpellId) -> String) -> DiplomacySave {
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
                    kind: match p.kind {
                        ProposalKind::Pact => KindSave::Pact,
                        ProposalKind::Alliance => KindSave::Alliance,
                        ProposalKind::Peace => KindSave::Peace,
                        ProposalKind::WarOn(t) => KindSave::WarOn(t.0),
                        ProposalKind::BreakAllianceWith(t) => KindSave::BreakAllianceWith(t.0),
                        ProposalKind::Exchange { give, get } => KindSave::Exchange {
                            give: spell_id(give),
                            get: spell_id(get),
                        },
                    },
                })
                .collect(),
        }
    }

    pub(crate) fn restore(
        &mut self,
        save: DiplomacySave,
        spell_named: &dyn Fn(&str) -> Option<SpellId>,
    ) {
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
            .filter_map(|p| {
                let kind = match p.kind {
                    KindSave::Pact => ProposalKind::Pact,
                    KindSave::Alliance => ProposalKind::Alliance,
                    KindSave::Peace => ProposalKind::Peace,
                    KindSave::WarOn(t) => ProposalKind::WarOn(PlayerId(t)),
                    KindSave::BreakAllianceWith(t) => ProposalKind::BreakAllianceWith(PlayerId(t)),
                    KindSave::Exchange { give, get } => ProposalKind::Exchange {
                        give: spell_named(&give)?,
                        get: spell_named(&get)?,
                    },
                };
                Some(Proposal {
                    from: PlayerId(p.from),
                    to: PlayerId(p.to),
                    kind,
                })
            })
            .collect();
    }
}

impl Game {
    pub(crate) fn diplomacy_to_save(&self) -> DiplomacySave {
        let spells = &self.content.rules.spells;
        let mut save = self.diplomacy.to_save(&|s| spells.get(s).id.clone());
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
        let spells = &self.content.rules.spells;
        self.diplomacy.restore(save, &|name| spells.id_of(name));
    }
}

#[cfg(test)]
mod tests;
