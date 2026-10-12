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
//! read-only [`WorldView`], the random numbers and a list of changes it asks the game to make
//! ([`Outcome`]: gold moved, a spell learned). `Game` only lends it out for a call
//! (`with_diplomacy`) and keeps thin methods with the old names for the other modules and the
//! crates around. Tests of the rules run on a [`FakeView`](crate::testutil::FakeView), without a
//! world.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::city::{PlayerId, Position};
use crate::magic::Retort;
use crate::rng::GameRng;
use crate::rules::SpellId;
use crate::view::WorldView;
use crate::{Event, Game};

/// What a wizard is bound to with another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Treaty {
    #[default]
    None,
    Pact,
    Alliance,
    War,
}

/// A wizard's temper (task 64 assigns them; until then everyone is `Chaotic`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Personality {
    Maniacal,
    Ruthless,
    Aggressive,
    #[default]
    Chaotic,
    Lawful,
    Peaceful,
}

impl Personality {
    /// The original's `TBL_AI_PRS_IDK_Mod`: how well the wizard takes proposals.
    pub fn modifier(self) -> i32 {
        self as i32 * 10
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
    personalities: BTreeMap<u8, Personality>,
    proposals: Vec<Proposal>,
    /// Events that hooks (battles, spells, the turn) raise; the next command returns them.
    queue: Vec<DiplomacyEvent>,
}

fn key(a: PlayerId, b: PlayerId) -> (u8, u8) {
    (a.0.min(b.0), a.0.max(b.0))
}

/// The words of the relation scale, `Hate` to `Harmony`.
pub const RELATION_WORDS: [&str; 11] = [
    "Hate", "Troubled", "Tense", "Restless", "Unease", "Neutral", "Relaxed", "Calm", "Peaceful",
    "Friendly", "Harmony",
];

/// Where a relation sits on the scale: 0 (hate) to 10 (harmony).
pub fn relation_level(relation: i32) -> usize {
    ((100 + relation.clamp(-100, 100)) / 20) as usize
}

pub fn relation_word(relation: i32) -> &'static str {
    RELATION_WORDS[relation_level(relation)]
}

/// A pact needs a relation above this, an alliance above `ALLIANCE_RELATION`.
pub const PACT_RELATION: i32 = 10;
pub const ALLIANCE_RELATION: i32 = 50;
/// The land size the "too many cities" reaction assumes (0 small, 1 medium, 2 large) until
/// the map size is kept in a game.
const LAND_SIZE: i32 = 1;

impl Diplomacy {
    pub fn personality(&self, wizard: PlayerId) -> Personality {
        self.personalities
            .get(&wizard.0)
            .copied()
            .unwrap_or_default()
    }

    pub fn set_personality(&mut self, wizard: PlayerId, personality: Personality) {
        self.personalities.insert(wizard.0, personality);
    }

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
pub fn met_near(
    view: &dyn WorldView,
    player: PlayerId,
    at: Position,
    radius: u32,
) -> Vec<PlayerId> {
    owners(view.foreign_sightings_near(player, at, radius))
}

/// The wizards `player` meets by what it sees now: the owners of the cities and of the units it
/// can make out on the squares in its sight, in player order.
pub fn met_in_sight(view: &dyn WorldView, player: PlayerId) -> Vec<PlayerId> {
    owners(view.foreign_sightings(player))
}

fn owners(sightings: Vec<crate::view::Sighting>) -> Vec<PlayerId> {
    let mut met: Vec<PlayerId> = sightings.into_iter().map(|s| s.owner).collect();
    met.sort();
    met.dedup();
    met
}

/// The total cost of a wizard's units, the original's army strength.
pub fn army_strength(view: &dyn WorldView, player: PlayerId) -> i32 {
    view.units_of(player).iter().map(|u| u.cost as i32).sum()
}

/// What the rules of diplomacy need from the rest of the game, for one call: the read-only
/// view of the world, the random numbers, and the difficulty. What they must change outside
/// diplomacy (gold, a spell learned) they leave in `out` for the game to apply.
pub struct DiplomacyCtx<'a> {
    pub view: &'a dyn WorldView,
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
pub fn wizards(view: &dyn WorldView) -> Vec<PlayerId> {
    let mut out = view.living_wizards();
    out.sort();
    out
}

/// The relation two wizards start with, from their books (`Init_Diplomatic_Relations`).
fn start_relation(view: &dyn WorldView, a: PlayerId, b: PlayerId) -> i32 {
    let books = |p: PlayerId, realm: &str| -> i32 {
        view.magic(p)
            .and_then(|m| m.books.get(realm))
            .map_or(0, |&n| i32::from(n))
    };
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

fn charismatic(view: &dyn WorldView, wizard: PlayerId) -> bool {
    view.magic(wizard)
        .is_some_and(|m| m.retorts.contains(&Retort::Charismatic))
}

/// How a threat weighs: the asker's army against the judge's, as the original rates it.
fn threat_modifier(view: &dyn WorldView, asker: PlayerId, judge: PlayerId) -> i32 {
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
fn enchantment_reaction(
    view: &dyn WorldView,
    spell: &str,
    other: PlayerId,
) -> Option<(i32, Action)> {
    let books = |realm: &str| {
        view.magic(other)
            .and_then(|m| m.books.get(realm))
            .copied()
            .unwrap_or(0)
    };
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
    fn pair_mut(&mut self, view: &dyn WorldView, a: PlayerId, b: PlayerId) -> &mut Pair {
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
    pub fn relation(&self, view: &dyn WorldView, a: PlayerId, b: PlayerId) -> i32 {
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
        at: crate::city::Position,
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
    /// Nature's Awareness meets everyone.
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
        match self.personality(defender) {
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
        if self.personality(breaker) == Personality::Lawful {
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
        let relations =
            stance.treaty_patience + stance.hidden + relation + self.personality(judge).modifier();
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
        view: &dyn WorldView,
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
                    && view.magic(from).is_some_and(|m| m.known.contains(&give))
                    && view.magic(to).is_some_and(|m| m.known.contains(&get))
                    && view.magic(to).is_some_and(|m| !m.known.contains(&give))
                    && view.magic(from).is_some_and(|m| !m.known.contains(&get))
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
        if amount <= 0 || ctx.view.player(from).is_none_or(|p| p.gold < amount) {
            return Err(DiplomacyError::NotEnoughGold);
        }
        // The original offers a quarter, half, three quarters or all of the receiver's gold: the
        // larger the share, the warmer the thanks.
        let reserve = ctx.view.player(to).map_or(0, |p| p.gold).max(1);
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
        let has = |p: PlayerId| ctx.view.magic(p).is_some_and(|m| m.known.contains(&spell));
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

    fn check_gift(
        &self,
        view: &dyn WorldView,
        from: PlayerId,
        to: PlayerId,
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
        Ok(())
    }

    /// Carry out a diplomacy command.
    pub(crate) fn command(
        &mut self,
        ctx: &mut DiplomacyCtx<'_>,
        command: DiplomacyCommand,
    ) -> Vec<Event> {
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
            events.push(Event::Rejected(crate::Rejection::Diplomacy(e)));
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
        if id != crate::endgame::SPELL_OF_MASTERY {
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
        if ctx.view.content().rules.spells.get(spell).id == crate::endgame::SPELL_OF_MASTERY {
            self.enchantment_relations(ctx, player, crate::endgame::SPELL_OF_MASTERY, 1);
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
                .magic(wizard)
                .map(|m| {
                    m.global
                        .iter()
                        .map(|&s| (ctx.view.content().rules.spells.get(s).id.clone(), 10))
                        .collect()
                })
                .unwrap_or_default();
            // The Spell of Mastery is felt while it is being cast.
            let casting_mastery = ctx
                .view
                .magic(wizard)
                .and_then(|m| m.casting)
                .is_some_and(|c| {
                    ctx.view.content().rules.spells.get(c.spell).id
                        == crate::endgame::SPELL_OF_MASTERY
                });
            if casting_mastery {
                active.push((crate::endgame::SPELL_OF_MASTERY.to_string(), 2));
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
            if ctx.view.turn().is_multiple_of(2) {
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
                let turn = ctx.view.turn();
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
        let mut rng = std::mem::replace(&mut self.rng, GameRng::new(0));
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
                    if let Some(p) = self.player_mut(from) {
                        p.gold -= amount;
                    }
                    if let Some(p) = self.player_mut(to) {
                        p.gold += amount;
                    }
                }
                Outcome::Spell { player, spell } => self.learn_spell(player, spell),
            }
        }
        result
    }

    /// A player that is a wizard: neither the monsters nor an independent city.
    pub fn is_wizard(&self, id: PlayerId) -> bool {
        WorldView::is_wizard(self, id)
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

    /// The temper of a wizard.
    pub fn personality(&self, wizard: PlayerId) -> Personality {
        self.diplomacy.personality(wizard)
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
    pub personalities: Vec<(u8, Personality)>,
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
            personalities: self.personalities.iter().map(|(&k, &v)| (k, v)).collect(),
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
        self.personalities = save.personalities.into_iter().collect();
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
        self.diplomacy.to_save(&|s| spells.get(s).id.clone())
    }

    pub(crate) fn diplomacy_from_save(&mut self, save: DiplomacySave) {
        let spells = &self.content.rules.spells;
        self.diplomacy.restore(save, &|name| spells.id_of(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::Position;
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::save::Snapshot;
    use crate::testutil::{content_with_magic, FakeView};
    use crate::{Command, Rejection};

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);
    const P2: PlayerId = PlayerId(2);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    /// Three wizards, the first of them the human, nobody met yet.
    fn world() -> Game {
        let c = content_with_magic();
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(11);
        g.content = c;
        g.map = Some(Map::new(Grid::new(30, 20, grass), Grid::new(30, 20, grass)));
        for id in [P0, P1, P2] {
            g.players.push(Player::new(id, "W"));
        }
        g.human = Some(P0);
        for id in [P0, P1, P2] {
            g.init_wizard(id, &[("sorcery", 2)], &[]).unwrap();
        }
        g
    }

    /// Diplomacy on its own: its state, a fake view of the world and a generator, with the same
    /// calls as the game's interface. Most of the rules need nothing more than this.
    struct Rig {
        diplomacy: Diplomacy,
        view: FakeView,
        rng: GameRng,
    }

    impl std::ops::Deref for Rig {
        type Target = FakeView;
        fn deref(&self) -> &FakeView {
            &self.view
        }
    }

    impl std::ops::DerefMut for Rig {
        fn deref_mut(&mut self) -> &mut FakeView {
            &mut self.view
        }
    }

    // It mirrors the game's whole interface; a test uses the part it needs.
    #[allow(dead_code)]
    impl Rig {
        /// Three wizards, the first of them the human, nobody met yet.
        fn new() -> Rig {
            let mut view = FakeView::new(&[P0, P1, P2]).with_content(content_with_magic());
            view.human = Some(P0);
            for p in &mut view.players {
                p.magic.books.insert("sorcery".into(), 2);
            }
            Rig {
                diplomacy: Diplomacy::default(),
                view,
                rng: GameRng::new(11),
            }
        }

        fn lend<R>(&mut self, f: impl FnOnce(&mut Diplomacy, &mut DiplomacyCtx<'_>) -> R) -> R {
            let (result, out) = {
                let mut ctx = DiplomacyCtx {
                    view: &self.view,
                    rng: &mut self.rng,
                    out: Vec::new(),
                };
                let result = f(&mut self.diplomacy, &mut ctx);
                (result, ctx.out)
            };
            for outcome in out {
                match outcome {
                    Outcome::Gold { from, to, amount } => {
                        self.view
                            .players
                            .iter_mut()
                            .find(|p| p.id == from)
                            .unwrap()
                            .gold -= amount;
                        self.view
                            .players
                            .iter_mut()
                            .find(|p| p.id == to)
                            .unwrap()
                            .gold += amount;
                    }
                    Outcome::Spell { player, spell } => {
                        self.magic_mut_for_tests(player).known.insert(spell);
                    }
                }
            }
            result
        }

        fn magic_mut_for_tests(&mut self, player: PlayerId) -> &mut crate::magic::Magic {
            &mut self
                .view
                .players
                .iter_mut()
                .find(|p| p.id == player)
                .unwrap()
                .magic
        }

        fn wizards(&self) -> Vec<PlayerId> {
            wizards(&self.view)
        }

        fn player_mut(&mut self, id: PlayerId) -> Option<&mut crate::player::Player> {
            self.view.players.iter_mut().find(|p| p.id == id)
        }

        /// The turn: what diplomacy does at its end, and the events it raised.
        fn end_turn(&mut self) -> Vec<Event> {
            self.lend(|d, ctx| d.process_diplomacy(ctx));
            self.view.turn += 1;
            self.drain_diplomacy_events()
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
            self.diplomacy.relation(&self.view, a, b)
        }

        /// The temper of a wizard.
        pub fn personality(&self, wizard: PlayerId) -> Personality {
            self.diplomacy.personality(wizard)
        }

        /// What the hooks raised since the last command, as events.
        pub fn drain_diplomacy_events(&mut self) -> Vec<Event> {
            self.diplomacy.drain_events()
        }

        /// The two wizards meet. Returns whether it was the first time.
        pub fn make_contact(&mut self, a: PlayerId, b: PlayerId) -> bool {
            self.lend(|d, ctx| d.make_contact(ctx, a, b))
        }

        /// `player` has just looked at the squares within `radius` of `at`: it meets the wizards
        /// whose cities or visible units stand there.
        pub fn contact_near(&mut self, player: PlayerId, at: Position, radius: u32) {
            self.lend(|d, ctx| d.contact_near(ctx, player, at, radius));
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
            self.lend(|d, ctx| d.change_relations(ctx, value, attacker, defender, action, spell));
        }

        /// A pact or an alliance between two wizards (`Start_Treaty`).
        pub fn start_treaty(&mut self, a: PlayerId, b: PlayerId, treaty: Treaty) {
            self.lend(|d, ctx| d.start_treaty(ctx, a, b, treaty));
        }

        /// `breaker` ends the pact or alliance with `victim` (`Break_Treaties`).
        pub fn break_treaties(&mut self, breaker: PlayerId, victim: PlayerId) {
            self.lend(|d, ctx| d.break_treaties(ctx, breaker, victim));
        }

        /// `attacker` declares war on `defender` (`Declare_War`).
        pub fn declare_war(&mut self, attacker: PlayerId, defender: PlayerId) {
            self.lend(|d, ctx| d.declare_war(ctx, attacker, defender));
        }

        /// The end of a war (`Declare_Peace`).
        pub fn declare_peace(&mut self, a: PlayerId, b: PlayerId) {
            self.lend(|d, ctx| d.declare_peace(ctx, a, b));
        }

        /// The total cost of a wizard's units, the original's army strength.
        pub fn army_strength(&self, player: PlayerId) -> i32 {
            army_strength(&self.view, player)
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
            self.lend(|d, ctx| d.rate_proposal(ctx, asker, judge, kind))
        }

        /// Whether `from` may put `kind` to `to` now: the entries the original greys out.
        pub fn check_proposal(
            &self,
            from: PlayerId,
            to: PlayerId,
            kind: ProposalKind,
        ) -> Result<(), DiplomacyError> {
            self.diplomacy.check_proposal(&self.view, from, to, kind)
        }

        /// Carry out a diplomacy command.
        pub fn diplomacy_command(&mut self, command: DiplomacyCommand) -> Vec<Event> {
            self.lend(|d, ctx| d.command(ctx, command))
        }

        pub fn enchantments_in_force(&mut self) {
            self.lend(|d, ctx| d.enchantments_in_force(ctx));
        }

        pub fn enchantment_relations(&mut self, caster: PlayerId, spell: &str, divisor: i32) {
            self.lend(|d, ctx| d.enchantment_relations(ctx, caster, spell, divisor));
        }

        /// A spell was cast: the wizards it offends take note. `target_owner` is whose city it fell on.
        pub fn cast_relations(
            &mut self,
            caster: PlayerId,
            spell: SpellId,
            target_owner: Option<PlayerId>,
        ) {
            self.lend(|d, ctx| d.cast_relations(ctx, caster, spell, target_owner));
        }

        /// A wizard begins the Spell of Mastery: the others take note at once.
        pub fn begin_cast_relations(&mut self, player: PlayerId, spell: SpellId) {
            self.lend(|d, ctx| d.begin_cast_relations(ctx, player, spell));
        }

        /// Attacking a wizard one has a treaty with breaks it first.
        pub fn attack_relations(&mut self, attacker: PlayerId, defender: PlayerId) {
            self.lend(|d, ctx| d.attack_relations(ctx, attacker, defender));
        }

        /// `winner` took a city of `loser` that had people in it.
        pub fn city_taken_relations(&mut self, winner: PlayerId, loser: PlayerId) {
            self.lend(|d, ctx| d.city_taken_relations(ctx, winner, loser));
        }

        /// An attack on a city failed.
        pub fn city_held_relations(
            &mut self,
            attacker: PlayerId,
            defender: PlayerId,
            fortress_city: bool,
        ) {
            self.lend(|d, ctx| d.city_held_relations(ctx, attacker, defender, fortress_city));
        }

        /// A wizard's fortress fell or the wizard was destroyed.
        pub fn conquest_relations(&mut self, winner: PlayerId, loser: PlayerId, destroyed: bool) {
            self.lend(|d, ctx| d.conquest_relations(ctx, winner, loser, destroyed));
        }

        /// The part of the turn that belongs to diplomacy.
        pub fn process_diplomacy(&mut self) {
            self.lend(|d, ctx| d.process_diplomacy(ctx));
        }
    }

    /// What the tests do to either, the whole game or diplomacy alone.
    trait Harness {
        fn met(&mut self, a: PlayerId, b: PlayerId);
        fn set_pair(&mut self, a: PlayerId, b: PlayerId, f: impl FnOnce(&mut Pair));
        fn command(&mut self, c: DiplomacyCommand) -> Vec<Event>;
    }

    impl Harness for Game {
        fn met(&mut self, a: PlayerId, b: PlayerId) {
            self.make_contact(a, b);
            self.drain_diplomacy_events();
        }
        fn set_pair(&mut self, a: PlayerId, b: PlayerId, f: impl FnOnce(&mut Pair)) {
            self.with_diplomacy(|d, ctx| f(d.pair_mut(ctx.view, a, b)));
        }
        fn command(&mut self, c: DiplomacyCommand) -> Vec<Event> {
            self.apply(Command::Diplomacy(c))
        }
    }

    impl Harness for Rig {
        fn met(&mut self, a: PlayerId, b: PlayerId) {
            self.make_contact(a, b);
            self.drain_diplomacy_events();
        }
        fn set_pair(&mut self, a: PlayerId, b: PlayerId, f: impl FnOnce(&mut Pair)) {
            self.lend(|d, ctx| f(d.pair_mut(ctx.view, a, b)));
        }
        fn command(&mut self, c: DiplomacyCommand) -> Vec<Event> {
            let mut events = self.diplomacy_command(c);
            events.extend(self.drain_diplomacy_events());
            events
        }
    }

    fn met(g: &mut impl Harness, a: PlayerId, b: PlayerId) {
        g.met(a, b);
    }

    fn set_pair(g: &mut impl Harness, a: PlayerId, b: PlayerId, f: impl FnOnce(&mut Pair)) {
        g.set_pair(a, b, f);
    }

    fn set_rel(g: &mut impl Harness, a: PlayerId, b: PlayerId, v: i32) {
        g.set_pair(a, b, |p| p.visible = v);
    }

    fn command(g: &mut impl Harness, c: DiplomacyCommand) -> Vec<Event> {
        g.command(c)
    }

    fn city(g: &mut Game, owner: PlayerId, pos: Position) -> hecs::Entity {
        let race = g.content.rules.races.id_of("elf");
        g.apply(Command::FoundCity {
            owner,
            pos,
            name: "Town".into(),
            race,
            population: 3,
        });
        g.city_at(pos).unwrap()
    }

    fn rejected(events: &[Event]) -> Option<DiplomacyError> {
        events.iter().find_map(|e| match e {
            Event::Rejected(Rejection::Diplomacy(e)) => Some(*e),
            _ => None,
        })
    }

    #[test]
    fn the_scale_runs_from_hate_to_harmony() {
        assert_eq!(relation_word(-100), "Hate");
        assert_eq!(relation_word(-81), "Hate");
        assert_eq!(relation_word(-80), "Troubled");
        assert_eq!(relation_word(-60), "Tense");
        assert_eq!(relation_word(0), "Neutral");
        assert_eq!(relation_word(99), "Friendly");
        assert_eq!(relation_word(100), "Harmony");
        assert_eq!(relation_level(0), 5);
    }

    #[test]
    fn start_relations_come_from_the_books() {
        let mut g = world();
        // Life against death: -(life + death) * 5, no shared realm.
        g.init_wizard(P0, &[("life", 3)], &[]).unwrap();
        g.init_wizard(P1, &[("death", 2)], &[]).unwrap();
        assert_eq!(g.relation(P0, P1), -25);
        // Shared realms add two for each book of the smaller; chaos costs two for the larger.
        g.init_wizard(P0, &[("nature", 3), ("chaos", 1)], &[])
            .unwrap();
        g.init_wizard(P1, &[("nature", 2), ("chaos", 2)], &[])
            .unwrap();
        assert_eq!(g.relation(P0, P1), 2);
        // Never below -90.
        g.init_wizard(P0, &[("life", 5)], &[]).unwrap();
        g.init_wizard(P1, &[("death", 5)], &[]).unwrap();
        assert_eq!(g.relation(P0, P1), -50);
        g.init_wizard(P0, &[("life", 11)], &[]).unwrap();
        g.init_wizard(P1, &[("death", 11)], &[]).unwrap();
        assert_eq!(g.relation(P0, P1), -90);
    }

    #[test]
    fn a_wizard_whose_city_is_seen_is_met() {
        let mut g = world();
        city(&mut g, P1, at(10, 10));
        assert!(!g.have_met(P0, P1));
        g.explore(P0, at(6, 10), 2);
        assert!(!g.have_met(P0, P1), "out of sight");
        g.explore(P0, at(8, 10), 2);
        assert!(g.have_met(P0, P1));
        assert!(g.have_met(P1, P0));
        assert_eq!(g.diplomacy.stance(P0, P1).unwrap().stage, Stage::Greeted);
        assert!(!g.have_met(P0, P2));
    }

    #[test]
    fn the_event_of_a_meeting_comes_back_with_the_command() {
        let mut g = world();
        city(&mut g, P1, at(10, 10));
        let events = g.apply(Command::CreateUnit {
            owner: P0,
            pos: at(9, 10),
            unit: g.content.rules.units.id_of("walker").unwrap(),
        });
        // The unit looks round where it appears.
        assert!(
            events.contains(&Event::Diplomacy(DiplomacyEvent::ContactMade {
                a: P0,
                b: P1
            }))
        );
        assert!(
            g.drain_diplomacy_events().is_empty(),
            "returned with the command, not kept"
        );
    }

    #[test]
    fn the_turn_meets_those_whose_units_one_can_see() {
        let mut g = world();
        city(&mut g, P0, at(10, 10));
        let walker = g.content.rules.units.id_of("walker").unwrap();
        g.spawn_unit(P1, at(12, 10), walker);
        // Forget that they have been shown to each other already.
        set_pair(&mut g, P0, P1, |p| p.contacted = false);
        assert!(!g.have_met(P0, P1));
        let events = g.apply(Command::EndTurn);
        assert!(g.have_met(P0, P1), "{events:?}");
        assert!(
            events.contains(&Event::Diplomacy(DiplomacyEvent::ContactMade {
                a: P0,
                b: P1
            }))
        );
    }

    #[test]
    fn nature_awareness_meets_everyone() {
        let mut g = world();
        city(&mut g, P0, at(2, 2));
        city(&mut g, P1, at(25, 15));
        city(&mut g, P2, at(15, 8));
        let spell = g.content.rules.spells.id_of("nature_awareness");
        if let Some(spell) = spell {
            g.magic_mut(P0).unwrap().global.push(spell);
            g.apply(Command::EndTurn);
            assert!(g.have_met(P0, P1) && g.have_met(P0, P2));
        }
    }

    #[test]
    fn good_deeds_count_less_the_better_the_relation() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        met(&mut g, P2, P1);
        g.human = Some(P0);
        // P1 is the defender (the human never is): attacker P2.
        set_rel(&mut g, P1, P2, 0);
        g.change_relations(8, P2, P1, Action::Friendship, None);
        assert_eq!(g.relation(P1, P2), 8);
        // At +50 a gain is halved... divided by 50 / 25 + 1 = 3.
        set_rel(&mut g, P1, P2, 50);
        g.change_relations(9, P2, P1, Action::Friendship, None);
        assert_eq!(g.relation(P1, P2), 53);
        // From a negative relation it is doubled, but not past +10.
        set_rel(&mut g, P1, P2, -30);
        g.change_relations(8, P2, P1, Action::Friendship, None);
        assert_eq!(g.relation(P1, P2), -14);
        set_rel(&mut g, P1, P2, -3);
        g.change_relations(20, P2, P1, Action::Friendship, None);
        assert_eq!(g.relation(P1, P2), 10);
    }

    #[test]
    fn bad_deeds_count_double_to_a_friend_and_less_to_an_enemy() {
        let mut g = Rig::new();
        met(&mut g, P2, P1);
        set_rel(&mut g, P1, P2, 20);
        g.change_relations(-10, P2, P1, Action::AttackedCity, None);
        assert_eq!(g.relation(P1, P2), 0);
        set_rel(&mut g, P1, P2, -50);
        g.change_relations(-10, P2, P1, Action::AttackedCity, None);
        // Divided by 50 / 25 + 1 = 3.
        assert_eq!(g.relation(P1, P2), -53);
        set_rel(&mut g, P1, P2, -100);
        g.change_relations(-50, P2, P1, Action::AttackedCity, None);
        assert_eq!(g.relation(P1, P2), -100);
    }

    #[test]
    fn temper_and_charm_change_what_a_deed_is_worth() {
        let mut g = Rig::new();
        met(&mut g, P2, P1);
        set_rel(&mut g, P1, P2, 0);
        g.diplomacy.set_personality(P1, Personality::Lawful);
        g.change_relations(-5, P2, P1, Action::AttackedCity, None);
        assert_eq!(g.relation(P1, P2), -10);
        set_rel(&mut g, P1, P2, 0);
        g.diplomacy.set_personality(P1, Personality::Maniacal);
        g.change_relations(-4, P2, P1, Action::AttackedCity, None);
        assert_eq!(g.relation(P1, P2), -6);
        g.change_relations(4, P2, P1, Action::Friendship, None);
        // -6 is negative, so the gain doubles to 8, then Maniacal takes three quarters: 6.
        assert_eq!(g.relation(P1, P2), 0);
        // A charismatic attacker halves its penalties and doubles its gains.
        g.diplomacy.set_personality(P1, Personality::Chaotic);
        g.magic_mut_for_tests(P2)
            .retorts
            .insert(Retort::Charismatic);
        set_rel(&mut g, P1, P2, 0);
        g.change_relations(-10, P2, P1, Action::AttackedCity, None);
        assert_eq!(g.relation(P1, P2), -5);
        set_rel(&mut g, P1, P2, 0);
        g.change_relations(4, P2, P1, Action::Friendship, None);
        assert_eq!(g.relation(P1, P2), 8);
    }

    #[test]
    fn the_human_is_never_the_one_who_changes() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        let before = g.relation(P0, P1);
        g.change_relations(-40, P1, P0, Action::AttackedCity, None);
        assert_eq!(g.relation(P0, P1), before);
        g.change_relations(-40, P0, P1, Action::AttackedCity, None);
        assert!(g.relation(P0, P1) < before);
    }

    #[test]
    fn strangers_do_not_react_and_neither_does_a_monster() {
        let mut g = Rig::new();
        let before = g.relation(P1, P2);
        g.change_relations(-40, P2, P1, Action::AttackedCity, None);
        assert_eq!(g.relation(P1, P2), before);
        met(&mut g, P2, P1);
        g.change_relations(-40, crate::units::NEUTRAL, P1, Action::AttackedCity, None);
        assert_eq!(g.relation(P1, P2), before);
    }

    #[test]
    fn a_relation_stays_below_65_without_an_alliance_and_below_minus_25_at_war() {
        let mut g = Rig::new();
        met(&mut g, P2, P1);
        set_rel(&mut g, P1, P2, 60);
        g.change_relations(100, P2, P1, Action::Friendship, None);
        assert_eq!(g.relation(P1, P2), 65);
        g.start_treaty(P1, P2, Treaty::Alliance);
        g.change_relations(100, P2, P1, Action::Friendship, None);
        assert!(g.relation(P1, P2) > 65);
        g.declare_war(P1, P2);
        set_pair(&mut g, P1, P2, |p| p.visible = 40);
        g.change_relations(5, P2, P1, Action::Friendship, None);
        assert!(g.relation(P1, P2) <= -25);
    }

    #[test]
    fn what_a_human_does_in_a_war_does_not_worsen_it_but_wears_down_peace() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        g.declare_war(P1, P0);
        let (rel, patience) = (
            g.relation(P0, P1),
            g.diplomacy.stance(P1, P0).unwrap().peace_patience,
        );
        g.change_relations(-40, P0, P1, Action::AttackedCity, None);
        assert_eq!(g.relation(P0, P1), rel);
        // The war is no worse, but peace is harder: a quarter of the deed (after the
        // diminishing returns of an already bad relation) wears on the patience.
        let wear = -40 / (-rel / 25 + 1) / 4;
        assert!(wear < 0);
        assert_eq!(
            g.diplomacy.stance(P1, P0).unwrap().peace_patience,
            patience + wear
        );
        // Other things (a talk-type action) still count.
        g.change_relations(-10, P0, P1, Action::Trespass, None);
        assert!(g.relation(P0, P1) < rel || rel == -100);
    }

    #[test]
    fn the_last_strongest_act_of_the_human_is_remembered() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        set_rel(&mut g, P0, P1, 0);
        g.change_relations(-5, P0, P1, Action::BadCitySpell, Some("famine"));
        g.change_relations(-20, P0, P1, Action::AttackedCity, None);
        g.change_relations(-3, P0, P1, Action::BadCitySpell, Some("famine"));
        let last = g
            .diplomacy
            .stance(P1, P0)
            .unwrap()
            .last_action
            .clone()
            .unwrap();
        assert_eq!(last.action, Action::AttackedCity);
        assert!(last.strength <= -20);
    }

    #[test]
    fn declaring_war_sets_the_state_the_original_sets() {
        let mut g = Rig::new();
        met(&mut g, P1, P2);
        let default = g.diplomacy.pair(P1, P2).unwrap().default;
        g.declare_war(P1, P2);
        assert_eq!(g.treaty(P1, P2), Treaty::War);
        assert_eq!(g.treaty(P2, P1), Treaty::War);
        let rel = g.relation(P2, P1);
        assert!((-100..=-76).contains(&rel), "{rel}");
        assert_eq!(g.diplomacy.pair(P1, P2).unwrap().default, default - 5);
        for (a, b) in [(P1, P2), (P2, P1)] {
            let s = g.diplomacy.stance(a, b).unwrap();
            assert_eq!(
                (s.treaty_patience, s.exchange_patience, s.peace_patience),
                (-200, -200, -130)
            );
            assert_eq!(s.hostility, 3);
        }
        // Once only.
        let events = g.drain_diplomacy_events();
        let wars = events
            .iter()
            .filter(|e| matches!(e, Event::Diplomacy(DiplomacyEvent::WarDeclared { .. })))
            .count();
        assert_eq!(wars, 1);
        g.declare_war(P1, P2);
        assert!(g.drain_diplomacy_events().is_empty());
    }

    #[test]
    fn breaking_a_treaty_costs_trust_with_everyone() {
        let mut g = Rig::new();
        met(&mut g, P1, P2);
        met(&mut g, P0, P1);
        met(&mut g, P0, P2);
        g.start_treaty(P1, P2, Treaty::Alliance);
        let default = g.diplomacy.pair(P1, P2).unwrap().default;
        g.break_treaties(P1, P2);
        assert_eq!(g.treaty(P1, P2), Treaty::None);
        // The oath breaker's own core reaction falls (the original raises it by mistake).
        assert_eq!(g.diplomacy.stance(P1, P2).unwrap().hidden, -20);
        // Everyone else thinks less of the victim.
        assert_eq!(g.diplomacy.stance(P0, P2).unwrap().hidden, -5);
        assert_eq!(g.diplomacy.pair(P1, P2).unwrap().default, default - 20);
        assert_eq!(
            g.diplomacy.stance(P2, P1).unwrap().broken,
            Some(Treaty::Alliance)
        );
        assert_eq!(g.diplomacy.stance(P1, P2).unwrap().treaty_patience, -200);
        assert_eq!(g.diplomacy.stance(P1, P2).unwrap().peace_patience, -200);
        // A pact costs half as much, a Lawful breaker twice.
        g.diplomacy.set_personality(P1, Personality::Lawful);
        g.start_treaty(P1, P2, Treaty::Pact);
        g.break_treaties(P1, P2);
        assert_eq!(g.diplomacy.stance(P1, P2).unwrap().hidden, -40);
    }

    #[test]
    fn ending_no_treaty_costs_the_breaker_nothing_of_its_own() {
        let mut g = Rig::new();
        met(&mut g, P1, P2);
        g.break_treaties(P1, P2);
        assert_eq!(g.diplomacy.stance(P1, P2).map_or(0, |s| s.hidden), 0);
        assert_eq!(g.diplomacy.stance(P2, P1).and_then(|s| s.broken), None);
        // The others' record of the victim still falls, as in the original.
        assert_eq!(g.diplomacy.stance(P0, P2).map_or(0, |s| s.hidden), -5);
    }

    #[test]
    fn a_pact_is_proposed_and_accepted() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        let events = command(
            &mut g,
            DiplomacyCommand::Propose {
                from: P0,
                to: P1,
                kind: ProposalKind::Pact,
            },
        );
        assert!(rejected(&events).is_none(), "{events:?}");
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Diplomacy(DiplomacyEvent::ProposalMade(p)) if p.kind == ProposalKind::Pact
        )));
        assert_eq!(g.diplomacy.proposals().len(), 1);
        // One at a time.
        let again = command(
            &mut g,
            DiplomacyCommand::Propose {
                from: P0,
                to: P1,
                kind: ProposalKind::Alliance,
            },
        );
        assert_eq!(rejected(&again), Some(DiplomacyError::AlreadyPending));
        let events = command(
            &mut g,
            DiplomacyCommand::Answer {
                player: P1,
                from: P0,
                accept: true,
            },
        );
        assert!(rejected(&events).is_none(), "{events:?}");
        assert_eq!(g.treaty(P0, P1), Treaty::Pact);
        assert!(g.diplomacy.proposals().is_empty());
    }

    #[test]
    fn a_refused_proposal_changes_nothing() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        command(
            &mut g,
            DiplomacyCommand::Propose {
                from: P0,
                to: P1,
                kind: ProposalKind::Alliance,
            },
        );
        let events = command(
            &mut g,
            DiplomacyCommand::Answer {
                player: P1,
                from: P0,
                accept: false,
            },
        );
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Diplomacy(DiplomacyEvent::ProposalAnswered {
                accepted: false,
                lapsed: false,
                ..
            })
        )));
        assert_eq!(g.treaty(P0, P1), Treaty::None);
        let none = command(
            &mut g,
            DiplomacyCommand::Answer {
                player: P1,
                from: P0,
                accept: true,
            },
        );
        assert_eq!(rejected(&none), Some(DiplomacyError::NothingPending));
    }

    #[test]
    fn what_the_treaty_allows_decides_what_can_be_proposed() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        met(&mut g, P0, P2);
        met(&mut g, P1, P2);
        let ok = |g: &Rig, from, to, kind| g.check_proposal(from, to, kind).is_ok();
        assert!(ok(&g, P0, P1, ProposalKind::Pact));
        assert!(ok(&g, P0, P1, ProposalKind::Alliance));
        assert!(!ok(&g, P0, P1, ProposalKind::Peace));
        assert!(ok(&g, P0, P1, ProposalKind::WarOn(P2)));
        assert!(!ok(&g, P0, P1, ProposalKind::WarOn(P0)));
        assert!(
            !ok(&g, P0, P1, ProposalKind::BreakAllianceWith(P2)),
            "no alliance to break"
        );
        g.start_treaty(P1, P2, Treaty::Alliance);
        assert!(ok(&g, P0, P1, ProposalKind::BreakAllianceWith(P2)));
        assert!(
            ok(&g, P0, P1, ProposalKind::WarOn(P2)),
            "allies may still be sent to war"
        );
        g.declare_war(P1, P2);
        assert!(!ok(&g, P0, P1, ProposalKind::WarOn(P2)), "already at war");
    }

    #[test]
    fn a_pact_cannot_be_offered_twice_nor_in_a_war() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        g.start_treaty(P0, P1, Treaty::Pact);
        assert!(g.check_proposal(P0, P1, ProposalKind::Pact).is_err());
        assert!(g.check_proposal(P0, P1, ProposalKind::Alliance).is_ok());
        g.declare_war(P1, P0);
        assert!(g.check_proposal(P0, P1, ProposalKind::Pact).is_err());
        assert!(g.check_proposal(P0, P1, ProposalKind::Alliance).is_err());
        assert!(g.check_proposal(P0, P1, ProposalKind::Peace).is_ok());
        assert_eq!(
            g.check_proposal(P0, P2, ProposalKind::Pact),
            Err(DiplomacyError::NotContacted)
        );
        assert_eq!(
            g.check_proposal(P0, P0, ProposalKind::Pact),
            Err(DiplomacyError::SameWizard)
        );
    }

    #[test]
    fn peace_ends_a_war_and_the_human_is_left_alone_a_while() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        g.declare_war(P1, P0);
        g.drain_diplomacy_events();
        let before = g.relation(P0, P1);
        command(
            &mut g,
            DiplomacyCommand::Propose {
                from: P0,
                to: P1,
                kind: ProposalKind::Peace,
            },
        );
        command(
            &mut g,
            DiplomacyCommand::Answer {
                player: P1,
                from: P0,
                accept: true,
            },
        );
        assert_eq!(g.treaty(P0, P1), Treaty::None);
        assert_eq!(g.relation(P0, P1), (before + 40).min(100));
        let peace = g.diplomacy.stance(P1, P0).unwrap().peace_duration;
        assert!((8..=13).contains(&peace), "{peace}");
        assert_eq!(g.diplomacy.stance(P0, P1).unwrap().peace_duration, 0);
    }

    #[test]
    fn a_war_on_a_third_wizard_is_declared_by_the_one_asked() {
        let mut g = Rig::new();
        for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
            met(&mut g, a, b);
        }
        command(
            &mut g,
            DiplomacyCommand::Propose {
                from: P0,
                to: P1,
                kind: ProposalKind::WarOn(P2),
            },
        );
        command(
            &mut g,
            DiplomacyCommand::Answer {
                player: P1,
                from: P0,
                accept: true,
            },
        );
        assert_eq!(g.treaty(P1, P2), Treaty::War);
        assert_eq!(g.treaty(P0, P2), Treaty::None);
    }

    #[test]
    fn asking_an_ally_to_break_with_another_makes_it_do_so() {
        let mut g = Rig::new();
        for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
            met(&mut g, a, b);
        }
        g.start_treaty(P1, P2, Treaty::Alliance);
        command(
            &mut g,
            DiplomacyCommand::Propose {
                from: P0,
                to: P1,
                kind: ProposalKind::BreakAllianceWith(P2),
            },
        );
        command(
            &mut g,
            DiplomacyCommand::Answer {
                player: P1,
                from: P0,
                accept: true,
            },
        );
        assert_eq!(g.treaty(P1, P2), Treaty::None);
        assert_eq!(g.diplomacy.stance(P1, P2).unwrap().hidden, -20);
    }

    #[test]
    fn an_unanswered_proposal_lapses_at_the_end_of_the_turn() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        command(
            &mut g,
            DiplomacyCommand::Propose {
                from: P0,
                to: P1,
                kind: ProposalKind::Pact,
            },
        );
        let events = g.end_turn();
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Diplomacy(DiplomacyEvent::ProposalAnswered {
                accepted: false,
                lapsed: true,
                ..
            })
        )));
        assert!(g.diplomacy.proposals().is_empty());
        assert_eq!(g.treaty(P0, P1), Treaty::None);
    }

    #[test]
    fn a_pact_that_stopped_being_possible_is_not_signed() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        command(
            &mut g,
            DiplomacyCommand::Propose {
                from: P0,
                to: P1,
                kind: ProposalKind::Pact,
            },
        );
        g.declare_war(P1, P0);
        command(
            &mut g,
            DiplomacyCommand::Answer {
                player: P1,
                from: P0,
                accept: true,
            },
        );
        assert_eq!(g.treaty(P0, P1), Treaty::War);
    }

    #[test]
    fn breaking_a_pact_by_command_needs_one() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        let none = command(
            &mut g,
            DiplomacyCommand::BreakTreaty {
                player: P0,
                with: P1,
            },
        );
        assert_eq!(rejected(&none), Some(DiplomacyError::NotAllowed));
        g.start_treaty(P0, P1, Treaty::Pact);
        command(
            &mut g,
            DiplomacyCommand::BreakTreaty {
                player: P0,
                with: P1,
            },
        );
        assert_eq!(g.treaty(P0, P1), Treaty::None);
        assert_eq!(g.diplomacy.stance(P0, P1).unwrap().hidden, -10);
    }

    #[test]
    fn a_wizard_can_declare_war_only_on_one_it_has_met() {
        let mut g = Rig::new();
        let events = command(&mut g, DiplomacyCommand::DeclareWar { player: P1, on: P2 });
        assert_eq!(rejected(&events), Some(DiplomacyError::NotContacted));
        met(&mut g, P1, P2);
        command(&mut g, DiplomacyCommand::DeclareWar { player: P1, on: P2 });
        assert_eq!(g.treaty(P1, P2), Treaty::War);
    }

    #[test]
    fn gold_is_a_gift_that_warms() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        set_rel(&mut g, P0, P1, 0);
        g.player_mut(P0).unwrap().gold = 300;
        g.player_mut(P1).unwrap().gold = 100;
        let events = command(
            &mut g,
            DiplomacyCommand::GiveGold {
                from: P0,
                to: P1,
                amount: 100,
            },
        );
        assert!(rejected(&events).is_none(), "{events:?}");
        assert_eq!(g.player(P0).unwrap().gold, 200);
        assert_eq!(g.player(P1).unwrap().gold, 200);
        assert!(g.relation(P0, P1) >= 0);
        let broke = command(
            &mut g,
            DiplomacyCommand::GiveGold {
                from: P0,
                to: P1,
                amount: 999,
            },
        );
        assert_eq!(rejected(&broke), Some(DiplomacyError::NotEnoughGold));
        let nobody = command(
            &mut g,
            DiplomacyCommand::GiveGold {
                from: P0,
                to: P2,
                amount: 10,
            },
        );
        assert_eq!(rejected(&nobody), Some(DiplomacyError::NotContacted));
    }

    #[test]
    fn a_spell_given_is_known_and_wins_trust() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        let famine = g.content.rules.spells.id_of("famine").unwrap();
        g.magic_mut_for_tests(P0).known.insert(famine);
        g.magic_mut_for_tests(P1).known.remove(&famine);
        set_rel(&mut g, P0, P1, 0);
        let events = command(
            &mut g,
            DiplomacyCommand::GiveSpell {
                from: P0,
                to: P1,
                spell: famine,
            },
        );
        assert!(rejected(&events).is_none(), "{events:?}");
        assert!(g.magic(P1).unwrap().known.contains(&famine));
        assert!(g.relation(P0, P1) > 0);
        let hidden = g.diplomacy.stance(P1, P0).unwrap().hidden;
        assert!((2..=9).contains(&hidden), "{hidden}");
        // It is not worth giving twice.
        let again = command(
            &mut g,
            DiplomacyCommand::GiveSpell {
                from: P0,
                to: P1,
                spell: famine,
            },
        );
        assert_eq!(rejected(&again), Some(DiplomacyError::SpellUnavailable));
    }

    #[test]
    fn spells_are_exchanged_one_for_one() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        let famine = g.content.rules.spells.id_of("famine").unwrap();
        let gaia = g.content.rules.spells.id_of("gaias_blessing").unwrap();
        for s in [famine, gaia] {
            for p in [P0, P1] {
                g.magic_mut_for_tests(p).known.remove(&s);
            }
        }
        g.magic_mut_for_tests(P0).known.insert(famine);
        g.magic_mut_for_tests(P1).known.insert(gaia);
        let kind = ProposalKind::Exchange {
            give: famine,
            get: gaia,
        };
        assert!(g.check_proposal(P0, P1, kind).is_ok());
        command(
            &mut g,
            DiplomacyCommand::Propose {
                from: P0,
                to: P1,
                kind,
            },
        );
        command(
            &mut g,
            DiplomacyCommand::Answer {
                player: P1,
                from: P0,
                accept: true,
            },
        );
        assert!(g.magic(P0).unwrap().known.contains(&gaia));
        assert!(g.magic(P1).unwrap().known.contains(&famine));
        // Nothing left to swap.
        assert!(g.check_proposal(P0, P1, kind).is_err());
    }

    #[test]
    fn the_rating_of_a_proposal_is_the_originals() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        // A pact below +10 is refused out of hand, and costs no patience.
        set_rel(&mut g, P0, P1, 10);
        assert_eq!(
            g.rate_proposal(P0, P1, ProposalKind::Pact),
            Verdict::Refused
        );
        assert_eq!(
            g.diplomacy.stance(P0, P1).map_or(0, |s| s.treaty_patience),
            0
        );
        // A great friend with great patience accepts a pact; patience then falls.
        set_rel(&mut g, P0, P1, 90);
        g.diplomacy.stance_mut(P0, P1).hidden = 30;
        g.diplomacy.stance_mut(P0, P1).treaty_patience = 100;
        g.diplomacy.set_personality(P1, Personality::Peaceful);
        assert_eq!(
            g.rate_proposal(P0, P1, ProposalKind::Pact),
            Verdict::Accepted
        );
        let s = g.diplomacy.stance(P0, P1).unwrap();
        assert!(
            (100 - 10 - 49..=100 - 10 - 21).contains(&s.treaty_patience),
            "{}",
            s.treaty_patience
        );
        assert_eq!((s.exchange_patience, s.peace_patience), (-10, -10));
        // An alliance needs above +50.
        set_rel(&mut g, P0, P1, 50);
        assert_eq!(
            g.rate_proposal(P0, P1, ProposalKind::Alliance),
            Verdict::Refused
        );
    }

    #[test]
    fn a_bigger_army_makes_a_wizard_more_pliant() {
        let mut g = world();
        met(&mut g, P0, P1);
        let spear = g.content.rules.units.id_of("walker").unwrap();
        let cost = g.content.rules.units.get(spear).cost as i32;
        // Neither has an army: the asker is 'weaker', no threat to speak of.
        assert_eq!(threat_modifier(&g, P0, P1), -50);
        for _ in 0..4 {
            g.spawn_unit(P0, at(1, 1), spear);
        }
        assert_eq!(threat_modifier(&g, P0, P1), 50, "the other has no army");
        g.spawn_unit(P1, at(5, 5), spear);
        assert_eq!(threat_modifier(&g, P0, P1), -50 + 4 * cost * 100 / cost);
    }

    #[test]
    fn peace_runs_down_and_patience_comes_back() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        g.diplomacy.stance_mut(P1, P0).peace_duration = 3;
        g.diplomacy.stance_mut(P1, P0).treaty_patience = -100;
        g.diplomacy.stance_mut(P1, P0).peace_patience = 100;
        g.end_turn();
        let s = g.diplomacy.stance(P1, P0).unwrap();
        assert_eq!(s.peace_duration, 2);
        // +10 at the end of the turn, then up to twice 1..5 while below zero (the first)
        // and below fifty (the second).
        assert!(
            (-100 + 10 + 2..=-100 + 10 + 10).contains(&s.treaty_patience),
            "{}",
            s.treaty_patience
        );
        assert_eq!(s.peace_patience, 100, "above 100 nothing is added");
    }

    #[test]
    fn a_pact_makes_friends_over_time() {
        let mut g = Rig::new();
        met(&mut g, P1, P2);
        set_rel(&mut g, P1, P2, 0);
        set_pair(&mut g, P1, P2, |p| p.default = 0);
        g.start_treaty(P1, P2, Treaty::Alliance);
        for _ in 0..40 {
            g.end_turn();
        }
        assert!(g.relation(P1, P2) > 10, "{}", g.relation(P1, P2));
        assert!(g.relation(P1, P2) <= 100);
    }

    #[test]
    fn without_cause_relations_drift_back_to_the_start() {
        let mut g = Rig::new();
        met(&mut g, P1, P2);
        let default = g.diplomacy.pair(P1, P2).unwrap().default;
        set_rel(&mut g, P1, P2, default - 30);
        for _ in 0..60 {
            g.end_turn();
        }
        assert_eq!(g.relation(P1, P2), default);
        // From above it comes down every tenth turn only.
        set_rel(&mut g, P1, P2, default + 20);
        for _ in 0..200 {
            g.end_turn();
        }
        assert_eq!(g.relation(P1, P2), default);
    }

    #[test]
    fn a_recent_deed_holds_the_drift_for_a_turn() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        let default = g.diplomacy.pair(P0, P1).unwrap().default;
        set_rel(&mut g, P0, P1, default + 30);
        g.diplomacy.stance_mut(P1, P0).last_action = Some(LastAction {
            action: Action::Friendship,
            strength: 5,
            spell: None,
        });
        g.end_turn();
        assert_eq!(g.relation(P0, P1), default + 30);
        assert!(g.diplomacy.stance(P1, P0).unwrap().last_action.is_none());
    }

    #[test]
    fn enchantments_offend_the_wizards_they_offend() {
        let mut g = world();
        for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
            met(&mut g, a, b);
        }
        g.init_wizard(P1, &[("life", 2)], &[]).unwrap();
        g.init_wizard(P2, &[("chaos", 2)], &[]).unwrap();
        for (a, b) in [(P1, P0), (P2, P0)] {
            set_rel(&mut g, a, b, 0);
        }
        // Eternal Night offends both; Evil Omens only a Life or Nature wizard.
        g.enchantment_relations(P0, "eternal_night", 1);
        assert_eq!(g.relation(P1, P0), -12);
        assert_eq!(g.relation(P2, P0), -12);
        set_rel(&mut g, P1, P0, 0);
        set_rel(&mut g, P2, P0, 0);
        g.enchantment_relations(P0, "evil_omens", 1);
        assert_eq!(g.relation(P1, P0), -20);
        assert_eq!(g.relation(P2, P0), 0, "a chaos wizard does not mind");
        // The per-turn share is a tenth, rounded to zero.
        set_rel(&mut g, P1, P0, 0);
        g.enchantment_relations(P0, "eternal_night", 10);
        assert_eq!(g.relation(P1, P0), -1);
        // Nothing for a spell nobody minds.
        set_rel(&mut g, P1, P0, 0);
        g.enchantment_relations(P0, "water_walking", 1);
        assert_eq!(g.relation(P1, P0), 0);
    }

    #[test]
    fn a_curse_on_a_city_is_noticed_by_its_owner() {
        let mut g = Rig::new();
        met(&mut g, P0, P1);
        set_rel(&mut g, P0, P1, 0);
        let famine = g.content.rules.spells.id_of("famine").unwrap();
        g.cast_relations(P0, famine, Some(P1));
        assert_eq!(g.relation(P0, P1), -5);
        // On one's own city it is nobody's business.
        set_rel(&mut g, P0, P1, 0);
        g.cast_relations(P0, famine, None);
        assert_eq!(g.relation(P0, P1), 0);
    }

    #[test]
    fn casting_the_spell_of_mastery_turns_the_others_against_the_caster() {
        let mut g = Rig::new();
        for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
            met(&mut g, a, b);
        }
        for b in [P1, P2] {
            set_rel(&mut g, P0, b, 0);
        }
        let mastery = g.content.rules.spells.id_of("spell_of_mastery").unwrap();
        g.begin_cast_relations(P0, mastery);
        // One time in nine the original has the offended wizard declare war as well.
        if g.treaty(P0, P1) == Treaty::War {
            assert!(g.relation(P0, P1) <= -75);
        } else {
            assert_eq!(g.relation(P0, P1), -50);
        }
        // Begun, the Spell of Mastery keeps bothering them a little each turn (half of fifty).
        g.magic_mut_for_tests(P1).casting = Some(crate::magic::Casting {
            spell: mastery,
            remaining: 100,
        });
        set_rel(&mut g, P1, P2, 0);
        g.enchantments_in_force();
        // Half of fifty is twenty-five.
        assert_eq!(g.relation(P1, P2), -25);
    }

    #[test]
    fn attacking_a_partner_breaks_the_treaty_and_costs_relations() {
        let mut g = Rig::new();
        met(&mut g, P1, P2);
        g.start_treaty(P1, P2, Treaty::Pact);
        set_rel(&mut g, P1, P2, 30);
        g.attack_relations(P1, P2);
        assert_eq!(g.treaty(P1, P2), Treaty::None);
        assert!(g.relation(P1, P2) < 30 - 40 + 1, "{}", g.relation(P1, P2));
        assert_eq!(g.diplomacy.stance(P1, P2).unwrap().hidden, -10);
        // With no treaty there is nothing to break and nothing to lose.
        let before = g.relation(P1, P2);
        g.attack_relations(P1, P2);
        assert_eq!(g.relation(P1, P2), before);
    }

    #[test]
    fn a_battle_against_a_partner_breaks_the_pact() {
        let mut g = world();
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let a = g.spawn_unit(P1, at(5, 5), walker);
        let d = g.spawn_unit(P2, at(6, 5), walker);
        met(&mut g, P1, P2);
        g.start_treaty(P1, P2, Treaty::Pact);
        g.drain_diplomacy_events();
        let events = g.apply(Command::BeginBattle {
            attackers: vec![a],
            defenders: vec![d],
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::BattleStarted { .. })),
            "{events:?}"
        );
        assert_eq!(g.treaty(P1, P2), Treaty::None);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Diplomacy(DiplomacyEvent::TreatyBroken { breaker, victim, treaty: Treaty::Pact })
                if *breaker == P1 && *victim == P2
        )));
    }

    #[test]
    fn a_battle_against_a_stranger_makes_them_known_to_each_other() {
        let mut g = world();
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let a = g.spawn_unit(P1, at(5, 5), walker);
        let d = g.spawn_unit(P2, at(6, 5), walker);
        set_pair(&mut g, P1, P2, |p| p.contacted = false);
        g.apply(Command::BeginBattle {
            attackers: vec![a],
            defenders: vec![d],
        });
        assert!(g.have_met(P1, P2));
        assert_eq!(g.treaty(P1, P2), Treaty::None);
    }

    #[test]
    fn attacking_a_wizard_one_has_not_met_meets_it() {
        let mut g = Rig::new();
        g.attack_relations(P1, P2);
        assert!(g.have_met(P1, P2));
    }

    #[test]
    fn taking_a_city_is_a_war() {
        let mut g = world();
        let target = city(&mut g, P1, at(10, 10));
        city(&mut g, P1, at(20, 5));
        let events = g.conquer_city(target, P2, 0);
        assert!(!events.is_empty());
        assert_eq!(g.treaty(P1, P2), Treaty::War);
        assert!(g.relation(P1, P2) <= -76);
        let events = g.drain_diplomacy_events();
        assert!(
            events.contains(&Event::Diplomacy(DiplomacyEvent::WarDeclared {
                attacker: P2,
                defender: P1,
            }))
        );
    }

    #[test]
    fn taking_an_independent_city_is_no_diplomacy() {
        let mut g = world();
        let mut rebels = Player::new_neutral(PlayerId(9), "Free");
        rebels.gold = 0;
        g.players.push(rebels);
        let target = city(&mut g, PlayerId(9), at(10, 10));
        g.conquer_city(target, P1, 0);
        assert!(g.drain_diplomacy_events().is_empty());
        assert!(!g.have_met(P1, PlayerId(9)));
    }

    #[test]
    fn a_city_that_holds_costs_the_attacker_twenty_or_sixty() {
        let mut g = Rig::new();
        met(&mut g, P1, P2);
        set_rel(&mut g, P1, P2, 0);
        g.city_held_relations(P2, P1, false);
        assert_eq!(g.relation(P1, P2), -20);
        set_rel(&mut g, P1, P2, 0);
        g.city_held_relations(P2, P1, true);
        assert_eq!(g.relation(P1, P2), -60);
    }

    #[test]
    fn a_fallen_fortress_is_held_against_the_winner_by_everyone_else() {
        let mut g = Rig::new();
        for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
            met(&mut g, a, b);
        }
        set_rel(&mut g, P1, P2, 0);
        g.conquest_relations(P2, P0, false);
        assert!(g.relation(P1, P2) < 0);
        assert!(g.have_met(P0, P1), "a banished wizard is still known");
        // A wizard destroyed is forgotten, and costs more.
        let banished = g.relation(P1, P2);
        set_rel(&mut g, P1, P2, 0);
        g.conquest_relations(P2, P0, true);
        assert!(g.relation(P1, P2) < banished.min(0));
        assert!(!g.have_met(P0, P1));
        assert!(!g.have_met(P0, P2));
        assert_eq!(g.treaty(P0, P1), Treaty::None);
    }

    #[test]
    fn diplomacy_survives_a_save() {
        let mut g = world();
        for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
            met(&mut g, a, b);
        }
        g.start_treaty(P1, P2, Treaty::Alliance);
        g.declare_war(P0, P1);
        g.diplomacy.set_personality(P2, Personality::Lawful);
        g.diplomacy.stance_mut(P2, P0).last_action = Some(LastAction {
            action: Action::AttackedCity,
            strength: -20,
            spell: Some("famine".into()),
        });
        g.diplomacy.proposals.push(Proposal {
            from: P0,
            to: P2,
            kind: ProposalKind::WarOn(P1),
        });
        let famine = g.content.rules.spells.id_of("famine").unwrap();
        let gaia = g.content.rules.spells.id_of("gaias_blessing").unwrap();
        g.diplomacy.proposals.push(Proposal {
            from: P1,
            to: P2,
            kind: ProposalKind::Exchange {
                give: famine,
                get: gaia,
            },
        });
        g.drain_diplomacy_events();
        let snap: Snapshot = g.snapshot();
        let text = ron::to_string(&snap).unwrap();
        let back: Snapshot = ron::from_str(&text).unwrap();
        assert_eq!(back, snap);
        let h = back.restore(content_with_magic()).unwrap();
        assert_eq!(h.diplomacy, g.diplomacy);
        assert_eq!(h.treaty(P1, P2), Treaty::Alliance);
        assert_eq!(h.treaty(P0, P1), Treaty::War);
        assert_eq!(h.personality(P2), Personality::Lawful);
    }

    #[test]
    fn an_old_save_has_no_diplomacy() {
        let g = world();
        let mut snap = g.snapshot();
        snap.diplomacy = DiplomacySave::default();
        let h = snap.restore(content_with_magic()).unwrap();
        assert!(!h.have_met(P0, P1));
        assert_eq!(h.relation(P0, P1), g.relation(P0, P1));
    }

    #[test]
    fn the_same_game_replays_the_same_diplomacy() {
        let play = || {
            let mut g = world();
            city(&mut g, P0, at(10, 10));
            city(&mut g, P1, at(14, 10));
            met(&mut g, P1, P2);
            g.declare_war(P1, P2);
            for _ in 0..30 {
                g.apply(Command::EndTurn);
            }
            g.diplomacy.clone()
        };
        assert_eq!(play(), play());
    }
}
