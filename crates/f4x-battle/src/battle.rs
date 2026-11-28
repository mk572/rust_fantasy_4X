//! Tactical battles.
//!
//! A [`Battle`] is a self-contained record, not part of the overland world: it is built from
//! the units that meet (their statistics are copied at the start, with every modifier
//! applied), played through [`Battle::apply`] with [`BattleCommand`]s, and when it ends the
//! game takes the results back (`world.rs`). Because it needs nothing from the world, battles
//! can be tested, replayed and driven by an AI on their own.
//!
//! Sides alternate: the attacker plays all its units, then the defender, and a new turn
//! starts when the defender is done. A side loses when it has no active unit left.
//!
//! Tests: in battle/tests.rs.

pub mod content_ref;
pub mod info;
pub mod legal_command;
pub mod load;
pub mod setup;
pub mod spoils;
pub mod undead;

use f4x_def::half_int::HalfInt;
use f4x_def::id::PlayerId;
use f4x_def::map::Position;
use f4x_def::rules::attack_nature::{AttackNature, Mode};
use f4x_def::rules::damage_type::DamageType;
use f4x_def::rules::realm::Realm;
use f4x_def::rules::unit_flag::UnitFlag;
use f4x_def::rules::{Ability, BattleRules, SpellId};
use f4x_def::settings::MeleeVsFliers;
use rand::RngCore;

use crate::attack::{self, Damage, Fighter};
use crate::battlemagic::field::Field;
use crate::battlemagic::vortex;
use crate::map::{distance, in_city_area, BattleMap, Pos};
use crate::power;
use crate::rng::GameRng;

pub use crate::battlemagic::caster::{BattleCaster, SpellAim};
pub use crate::battlemagic::error::SpellError;
pub use crate::side::Side;
pub use crate::unit::{
    BattleUnit, Confusion, Fate, FieldBonus, ItemCharge, Lasting, Status, UnitCasting, WarpKind,
    WizardSheet,
};
pub use f4x_def::rules::damage_type::RangedKind;

/// Everything a player or AI can ask a unit to do.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BattleCommand {
    /// Walk to a square within reach.
    Move { unit: usize, to: Pos },
    /// Attack an adjacent enemy in melee (thrown weapons and breath come first).
    Attack { unit: usize, target: usize },
    /// Fire at an enemy at a distance.
    Shoot { unit: usize, target: usize },
    /// A wall crusher strikes a standing section of the city wall next to it.
    AttackWall { unit: usize, cell: Pos },
    /// A wall crusher fires at a standing section of the city wall.
    ShootWall { unit: usize, cell: Pos },
    /// The wizard casts a spell (once a turn). `mana` is the total put into a spell that takes
    /// extra mana (a bolt, a dispel, Counter Magic); other spells take their cost whatever it is.
    Cast {
        spell: SpellId,
        aim: SpellAim,
        mana: u32,
    },
    /// A unit casts a spell of its own (a hero's, an item's or an ability's) instead of the
    /// wizard. `mana` is the total put into a spell that takes extra mana; it comes from the
    /// unit's mana. The unit ends its turn.
    UnitCast {
        unit: usize,
        spell: SpellId,
        aim: SpellAim,
        mana: u32,
    },
    /// A unit with the Creature Summoning ability (the Demon Lord) calls a demon to its side.
    /// It costs no mana and uses up the unit's turn.
    SummonDemon { unit: usize },
    /// The wizard whose vortex it is moves it one square (once a turn each).
    MoveVortex { index: usize, to: Pos },
    /// A unit with Merging (and half a move left) sinks into the ground: off the field until
    /// its side's next turn.
    Merge { unit: usize },
    /// A merged unit comes up on any free square, at no cost in moves and with its full turn;
    /// it cannot merge again this turn.
    Surface { unit: usize, to: Pos },
    /// Give up the rest of the unit's turn.
    Done { unit: usize },
    /// End the side's turn.
    EndTurn,
    /// Leave the battle: the other side wins.
    Flee,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleError {
    Over,
    NoSuchUnit,
    /// The unit is not the active side's, or is no longer fighting.
    NotYours,
    NoMovesLeft,
    Unreachable,
    NotAdjacent,
    OutOfRange,
    AlreadyAttacked,
    NoRangedAttack,
    OutOfAmmo,
    /// Target on the same side, or dead.
    BadTarget,
    /// An invisible enemy the side cannot see: it cannot be aimed at.
    Unseen,
    /// No standing wall there.
    NoWall,
    /// Only the attackers' wall crushers attack walls.
    NotWallCrusher,
    /// The spell cannot be cast now.
    Spell(SpellError),
    /// A Wall of Darkness: no ranged attack on a unit inside the city from outside it, unless
    /// the shooter sees through illusions.
    WallOfDarkness,
    /// The unit cannot merge now: no Merging, no move left, or it came up this turn.
    CannotMerge,
    /// The unit is not under the ground, or it went down this turn.
    NotMerged,
}

impl BattleError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Over => "refuse_battle.battle_error_over",
            Self::NoSuchUnit => "refuse_battle.battle_error_no_such_unit",
            Self::NotYours => "refuse_battle.battle_error_not_yours",
            Self::NoMovesLeft => "refuse_battle.battle_error_no_moves_left",
            Self::Unreachable => "refuse_battle.battle_error_unreachable",
            Self::NotAdjacent => "refuse_battle.battle_error_not_adjacent",
            Self::OutOfRange => "refuse_battle.battle_error_out_of_range",
            Self::AlreadyAttacked => "refuse_battle.battle_error_already_attacked",
            Self::NoRangedAttack => "refuse_battle.battle_error_no_ranged_attack",
            Self::OutOfAmmo => "refuse_battle.battle_error_out_of_ammo",
            Self::BadTarget => "refuse_battle.battle_error_bad_target",
            Self::Unseen => "refuse_battle.battle_error_unseen",
            Self::NoWall => "refuse_battle.battle_error_no_wall",
            Self::NotWallCrusher => "refuse_battle.battle_error_not_wall_crusher",
            Self::Spell(e) => e.key(),
            Self::WallOfDarkness => "refuse_battle.battle_error_wall_of_darkness",
            Self::CannotMerge => "refuse_battle.battle_error_cannot_merge",
            Self::NotMerged => "refuse_battle.battle_error_not_merged",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BattleEvent {
    /// A dead unit rose again as undead (Animate Dead).
    Raised {
        unit: usize,
    },
    Moved {
        unit: usize,
        path: Vec<Pos>,
    },
    Attacked {
        attacker: usize,
        defender: usize,
        /// Hit points of damage the defender took, and figures it lost.
        dealt: i32,
        dealt_kills: u32,
        /// The same for the attacker from the reply.
        taken: i32,
        taken_kills: u32,
    },
    Shot {
        attacker: usize,
        defender: usize,
        dealt: i32,
        kills: u32,
    },
    /// A wall crusher struck a wall section, and it fell or did not.
    WallAttacked {
        unit: usize,
        cell: Pos,
        destroyed: bool,
    },
    UnitDied {
        unit: usize,
    },
    /// A wizard cast a spell, paying `skill` of its casting skill and `mana` from the reserve.
    SpellCast {
        side: Side,
        spell: SpellId,
        skill: i32,
        mana: HalfInt,
    },
    /// A unit cast a spell of its own, paying `mana` of its mana (none for an item's charge or
    /// an ability).
    UnitSpellCast {
        unit: usize,
        spell: SpellId,
        mana: i32,
    },
    /// The enemy's Counter Magic stopped the spell.
    SpellCountered {
        side: Side,
        spell: SpellId,
    },
    /// Call Chaos dealt a unit one of its eight outcomes (0 nothing, 1 healing, 2 Chaos
    /// Channels, 3 Warp Creature, 4 Fire Bolt, 5 Warp Lightning, 6 Doom Bolt, 7 Disintegrate); the
    /// events that follow tell what it did.
    ChaosCalled {
        unit: usize,
        outcome: u8,
    },
    /// A damage spell hurt a unit.
    SpellDamaged {
        unit: usize,
        dealt: i32,
        kills: u32,
    },
    /// The unit took no harm from the spell: immune, or protected by a lasting spell. `by` is
    /// the damage type it is immune to, when there is one to name.
    SpellImmune {
        unit: usize,
        by: Option<DamageType>,
    },
    /// A blow or a shot did nothing to the defender: it is immune to the damage type `by`.
    Unharmed {
        attacker: usize,
        defender: usize,
        by: DamageType,
    },
    /// A healing spell mended the unit, or it regenerated.
    SpellHealed {
        unit: usize,
        healed: i32,
    },
    /// A lasting spell was put on the unit.
    SpellEnchanted {
        unit: usize,
        spell: SpellId,
    },
    /// A dispel took this many lasting spells off the unit.
    SpellDispelled {
        unit: usize,
        removed: u32,
        /// Which spells went, with the wizards that cast them.
        spells: Vec<(SpellId, PlayerId)>,
    },
    /// A dispel took a battlefield spell down: the side that had put it up.
    FieldDispelled {
        side: Side,
        spell: SpellId,
    },
    /// A battlefield spell went up for a side.
    FieldRaised {
        side: Side,
        spell: SpellId,
    },
    /// The defender raised a wall at its city (Wall of Stone, Fire or Darkness).
    WallRaised {
        side: Side,
        spell: SpellId,
    },
    /// A spell's resistance check: the unit resisted it.
    SpellResisted {
        unit: usize,
    },
    /// Figures were destroyed by a spell that gives each a resistance check.
    SpellSlew {
        unit: usize,
        kills: u32,
    },
    /// Wrack or Call Lightning hurt a unit at the start of a turn.
    FieldStruck {
        unit: usize,
        dealt: i32,
        kills: u32,
    },
    /// Terror holds the unit back this turn.
    Terrified {
        unit: usize,
    },
    /// The unit spent its turn on the web that holds it: `dealt` hit points off it, and whether
    /// that broke it.
    WebStruck {
        unit: usize,
        dealt: i32,
        broken: bool,
    },
    /// Confusion decided what the unit does this turn (`Confusion`).
    Confused {
        unit: usize,
        act: Confusion,
    },
    /// A bolt of Call Lightning fell on a unit; what it did to it follows.
    LightningStruck {
        unit: usize,
    },
    /// Mana Leak drained a side's wizard (and spell shots).
    ManaLeaked {
        side: Side,
    },
    /// A creature was summoned.
    Summoned {
        unit: usize,
    },
    /// A side's Counter Magic reserve was set.
    CounterMagicRaised {
        side: Side,
        strength: u32,
    },
    TurnStarted {
        side: Side,
        turn: u32,
    },
    Fled {
        side: Side,
    },
    /// Disrupt or Cracks Call destroyed a section of the city wall.
    WallDestroyed {
        cell: Pos,
    },
    /// Earth to Mud turned the ground round a square to mud.
    MudMade {
        cell: Pos,
    },
    /// Cracks Call opened a fissure on the square.
    FissureOpened {
        cell: Pos,
    },
    /// A Magic Vortex appeared on the square.
    VortexMade {
        cell: Pos,
    },
    /// A Magic Vortex (by its number) moved to the square.
    VortexMoved {
        index: usize,
        to: Pos,
    },
    /// Life Drain took `points` of life from a unit and gave it to the caster.
    LifeDrained {
        unit: usize,
        points: i32,
    },
    /// A Power Drain took `amount` mana from a unit.
    PowerDrained {
        unit: usize,
        amount: i32,
    },
    /// A unit sank into the ground.
    Merged {
        unit: usize,
    },
    /// A merged unit came up on a square.
    Surfaced {
        unit: usize,
        cell: Pos,
    },
    /// Recall Hero took the hero (a battle unit id) off the field.
    HeroRecalled {
        unit: usize,
    },
    /// The turn limit ran out with both sides still standing: every unit retreats exhausted and
    /// the defender is given the field (ends the battle, right before `Ended`).
    Exhausted,
    Ended {
        winner: Side,
    },
}

fn default_side_limit() -> usize {
    f4x_def::settings::MAX_STACK
}

/// The state of one battle.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Battle {
    pub attacker: PlayerId,
    pub defender: PlayerId,
    /// The overland square it is fought on.
    pub site: Option<Position>,
    pub map: BattleMap,
    pub units: Vec<BattleUnit>,
    pub turn: u32,
    pub active: Side,
    pub winner: Option<Side>,
    /// Each side's Counter Magic reserve, indexed by [`Side::index`]: spells the other side
    /// casts must first beat a dispel of this strength, and each attempt drains it.
    pub counter_magic: [u32; 2],
    /// Each side's wizard, indexed by [`Side::index`]: none for a side with no wizard.
    pub casters: [Option<BattleCaster>; 2],
    /// The battlefield enchantments each side has put up, with the spell that did.
    pub fields: [Vec<(Field, SpellId)>; 2],
    /// Damage to the city from the attackers inside it, one for each of their units there at
    /// the start of each turn (`Add_City_Damage_From_Battle_Units_Within`); it kills citizens
    /// and destroys buildings when the city falls.
    pub city_damage: u32,
    /// The city has a Wall of Fire: a walker coming into the city area from outside, or striking
    /// into it, is burned (strength 5, `burn`).
    pub wall_of_fire: bool,
    /// The city has a Wall of Darkness: see [`BattleError::WallOfDarkness`].
    pub wall_of_darkness: bool,
    /// The realms (ids) a Spell Ward guards the city against: the attackers cannot cast spells of
    /// them.
    pub warded_realms: Vec<f4x_def::rules::RealmId>,
    /// The realm of the magic node whose aura the battle is fought in: its creatures fight with
    /// +2 to attack, defence and resistance (`Battle_Unit_Special_Stats`).
    pub node_aura: Option<Realm>,
    /// The realm of the node the battle is fought on (the centre structure): every spell of
    /// another realm must first beat a dispel of strength 50 (`Combat_Cast_Spell`).
    pub node_site: Option<Realm>,
    /// The Magic Vortices on the field (`vortex.rs`).
    pub vortices: Vec<vortex::Vortex>,
    /// The spell save of the unit that is casting: it makes the resistance checks of its spell
    /// harder. 0 outside a cast.
    pub save_penalty: i32,
    /// The battle unit that is casting, if a unit and not the wizard: Life Drain heals it.
    pub spell_caster: Option<usize>,
    /// How hard a walker hits a flier in melee (`RuleSettings::melee_vs_fliers`).
    #[serde(default)]
    pub melee_vs_fliers: MeleeVsFliers,
    /// Everyone inside a walled city gets the wall's +3 defence (`RuleSettings::walls_cover_city`).
    #[serde(default)]
    pub walls_cover_city: bool,
    /// The exceptions to death immunity (`RuleSettings::death_immunity_exceptions`).
    #[serde(default)]
    pub death_immunity_exceptions: bool,
    /// The exceptions to magic immunity (`RuleSettings::magic_immunity_exceptions`).
    #[serde(default)]
    pub magic_immunity_exceptions: bool,
    /// Most units a side can have in the battle: summoning stops here
    /// (`RuleSettings::max_battle_side`).
    #[serde(default = "default_side_limit")]
    pub max_side: usize,
    /// The definitions of the game, shared with it: not saved, the game that loads the battle
    /// attaches its own.
    #[serde(skip)]
    pub content: content_ref::ContentRef,
    pub rng: GameRng,
    /// The global enchantments and retorts of each side's wizard, by id (`unit/stats.rs`): a unit
    /// that joins the battle takes them.
    #[serde(default)]
    pub wizards: [WizardSheet; 2],
}

impl Battle {
    /// Counter Magic: put `strength` (the mana paid for the spell) into `side`'s reserve.
    pub fn cast_counter_magic(&mut self, side: Side, strength: u32) {
        self.counter_magic[side.index()] = strength;
    }

    /// Whether `side`'s enemy's Counter Magic stops a spell of `difficulty` that `side` casts: a
    /// dispel of the reserve's strength must beat the spell. Every attempt drains the reserve
    /// by `rules.counter_magic_drain`, whether or not it stopped the spell.
    pub fn counter_magic_stops(&mut self, side: Side, difficulty: u32) -> bool {
        let drain = self.rules().counter_magic_drain;
        let reserve = &mut self.counter_magic[side.other().index()];
        let strength = *reserve;
        *reserve = reserve.saturating_sub(drain);
        crate::resistance::dispel_roll(
            &self.content.rules.battle,
            strength,
            difficulty,
            &mut self.rng,
        )
    }

    /// Whether the node under the battle stops a spell of `realm` and `difficulty`: a dispel of
    /// strength `rules.node_counter_strength` must beat it, unless the spell is of the node's own
    /// realm. (The caster's Node Mastery is checked by the caller.)
    pub fn node_stops(&mut self, realm: Option<Realm>, difficulty: u32) -> bool {
        let Some(node) = self.node_site else {
            return false;
        };
        if realm == Some(node) {
            return false;
        }
        crate::resistance::dispel_roll(
            &self.content.rules.battle,
            self.content.rules.battle.node_counter_strength,
            difficulty,
            &mut self.rng,
        )
    }

    /// The numbers of the battle (`rules.ron`).
    pub fn rules(&self) -> &BattleRules {
        &self.content.rules.battle
    }

    /// Start a battle. Unit ids are assigned in list order; the attacker moves first.
    pub fn new(
        content: std::sync::Arc<f4x_def::content::Content>,
        attacker: PlayerId,
        defender: PlayerId,
        map: BattleMap,
        mut units: Vec<BattleUnit>,
        seed: u64,
    ) -> Self {
        for (i, u) in units.iter_mut().enumerate() {
            u.id = i;
        }
        let mut b = Self {
            attacker,
            defender,
            site: None,
            map,
            units,
            turn: 1,
            active: Side::Attacker,
            winner: None,
            counter_magic: [0, 0],
            casters: [None, None],
            fields: [Vec::new(), Vec::new()],
            city_damage: 0,
            wall_of_fire: false,
            wall_of_darkness: false,
            warded_realms: Vec::new(),
            node_aura: None,
            node_site: None,
            vortices: Vec::new(),
            save_penalty: 0,
            spell_caster: None,
            melee_vs_fliers: MeleeVsFliers::default(),
            walls_cover_city: false,
            death_immunity_exceptions: false,
            magic_immunity_exceptions: false,
            max_side: default_side_limit(),
            content: content_ref::ContentRef::new(content),
            rng: GameRng::new(seed),
            wizards: Default::default(),
        };
        b.bind_units();
        let _ = b.begin_side_turn();
        b.check_winner();
        b
    }

    /// Give every unit the content and the wizards, so it can work its numbers out. A battle
    /// that was loaded has to do it too: neither is saved with the units.
    pub fn bind_units(&mut self) {
        let content = self.content.arc();
        for u in &mut self.units {
            u.bind(&content);
            u.set_wizards(self.wizards.clone());
        }
    }

    /// Put a unit in the place of the one numbered `id` (a dead unit raised again).
    pub fn put_unit(&mut self, id: usize, mut unit: BattleUnit) {
        unit.bind(&self.content.arc());
        unit.set_wizards(self.wizards.clone());
        self.units[id] = unit;
    }

    /// The wizard of a side keeps these global enchantments and retorts: the units work their
    /// numbers out with them, those that join the battle too.
    pub fn set_wizard(&mut self, side: Side, wizard: WizardSheet) {
        self.wizards[side.index()] = wizard;
        self.bind_units();
    }

    pub fn player_of(&self, side: Side) -> PlayerId {
        match side {
            Side::Attacker => self.attacker,
            Side::Defender => self.defender,
        }
    }

    pub fn unit(&self, id: usize) -> Option<&BattleUnit> {
        self.units.get(id)
    }

    /// Active units of a side.
    pub fn active_units(&self, side: Side) -> impl Iterator<Item = &BattleUnit> {
        self.units
            .iter()
            .filter(move |u| u.side == side && u.is_active())
    }

    /// The active unit standing on a square.
    pub fn unit_at(&self, cell: Pos) -> Option<&BattleUnit> {
        self.units.iter().find(|u| u.is_active() && u.cell == cell)
    }

    /// Squares the unit could walk to now, with the moves each costs.
    pub fn reachable(&self, unit: usize) -> std::collections::HashMap<Pos, HalfInt> {
        let Some(u) = self.units.get(unit).filter(|u| u.is_active()) else {
            return Default::default();
        };
        if u.moves_left > 0 && u.moves_instantly() {
            return self.instant_squares(unit);
        }
        let occupied = |p: Pos| self.is_occupied(p);
        self.map
            .reachable_as(u.cell, u.moves_left, u.mover(), &occupied)
            .into_iter()
            .filter(|&(p, _)| p != u.cell)
            .map(|(p, (c, _))| (p, c))
            .collect()
    }

    /// Where a teleporting unit can appear: every free square that is not blocked, each at the
    /// teleport cost (`Move_Battle_Unit`: no path is walked, and a unit with less left spends
    /// that).
    pub fn instant_squares(&self, unit: usize) -> std::collections::HashMap<Pos, HalfInt> {
        let cell = self.units[unit].cell;
        self.free_squares()
            .into_iter()
            .filter(|&p| p != cell)
            .map(|p| (p, self.map.movement().teleport))
            .collect()
    }

    /// Every free square a unit may stand on: open ground nobody stands on (the towers at the
    /// corners of a city's wall are shut to everyone).
    pub fn free_squares(&self) -> Vec<Pos> {
        let mut squares = Vec::new();
        for y in 0..crate::map::GRID_H {
            for x in 0..crate::map::GRID_W {
                let p = (x, y);
                let open = self.map.enter_cost(p, false).is_some() && !self.map.is_shut_tower(p);
                if open && self.unit_at(p).is_none() {
                    squares.push(p);
                }
            }
        }
        squares
    }

    pub fn begin_side_turn(&mut self) -> Vec<BattleEvent> {
        let side = self.active;
        let mut events = Vec::new();
        // A new turn begins with the attacker: the spells that act every turn strike first.
        if side == Side::Attacker {
            events.extend(self.turn_effects());
        }
        for u in self.units.iter_mut().filter(|u| u.side == side) {
            u.surfaced = false;
            u.merged_this_turn = false;
        }
        for u in &mut self.units {
            if let Some(owner) = u.turned_from.take() {
                u.side = owner;
            }
        }
        // Holy Bonus and Resistance to All follow whoever is still standing.
        self.note_invisibility();
        for u in self
            .units
            .iter_mut()
            .filter(|u| u.side == side && u.is_active())
        {
            u.refill_moves();
            u.attacked = false;
            u.revealed = false;
        }
        if let Some(caster) = &mut self.casters[side.index()] {
            caster.cast_this_turn = false;
        }
        self.vortex_new_turn(side);
        events.extend(self.side_turn_effects(side));
        // A new turn begins with the attacker: each of its units inside the city hurts it.
        if side == Side::Attacker && self.map.is_city() {
            let inside = self
                .active_units(Side::Attacker)
                .filter(|u| in_city_area(u.cell))
                .count() as u32;
            self.city_damage += inside;
        }
        events
    }

    pub fn check_winner(&mut self) -> Vec<BattleEvent> {
        if self.winner.is_some() {
            return Vec::new();
        }
        // A side whose every unit left is lent to the enemy at once loses on the spot, and those
        // units are destroyed (wiki article "Confusion").
        let all_lent = |s| {
            let mut mine = self
                .units
                .iter()
                .filter(|u| u.is_active() && u.turned_from.unwrap_or(u.side) == s);
            let any = mine.clone().next().is_some();
            any && mine.all(|u| u.turned_from.is_some())
        };
        let lost_to_confusion = [Side::Attacker, Side::Defender]
            .into_iter()
            .find(|&s| all_lent(s));
        if let Some(loser) = lost_to_confusion {
            for u in &mut self.units {
                if u.is_active() && u.turned_from == Some(loser) {
                    u.kill();
                    u.status = Status::Dead;
                }
            }
        }
        // A unit lent to the enemy by confusion still counts for its owner.
        let alive = |s| {
            self.units
                .iter()
                .filter(|u| u.is_active() && u.turned_from.unwrap_or(u.side) == s)
                .count()
        };
        let mut exhausted = false;
        let winner = if alive(Side::Attacker) == 0 {
            Some(Side::Defender)
        } else if alive(Side::Defender) == 0 {
            Some(Side::Attacker)
        } else if self.turn > self.rules().turn_limit {
            exhausted = true;
            Some(Side::Defender)
        } else {
            None
        };
        self.winner = winner;
        if let Some(won) = winner {
            // Units still under the ground when the battle ends: the winner's come up again, the
            // loser's get away whole.
            for u in self.units.iter_mut().filter(|u| u.status == Status::Merged) {
                if u.side == won {
                    u.status = Status::Active;
                } else {
                    u.slipped = true;
                    u.status = Status::Fled;
                }
            }
        }
        if winner.is_some() {
            for u in &mut self.units {
                // A creature still under Creature Binding is destroyed (Possession only lasts).
                if u.is_active()
                    && u.bound_from.is_some()
                    && u.effects(&self.content.rules)
                        .contains(f4x_def::rules::unit_effect::UnitEffect::CreatureBinding)
                {
                    u.kill();
                    u.status = Status::Dead;
                    u.damage_kinds[2] = u.damage_kinds[2].max(1);
                }
                if let Some(owner) = u.turned_from.take().or_else(|| u.bound_from.take()) {
                    u.side = owner;
                }
            }
        }
        let mut events = Vec::new();
        if exhausted {
            events.push(BattleEvent::Exhausted);
        }
        events.extend(winner.map(|winner| BattleEvent::Ended { winner }));
        events
    }

    pub fn check_unit(&self, unit: usize) -> Result<&BattleUnit, BattleError> {
        if self.winner.is_some() {
            return Err(BattleError::Over);
        }
        let u = self.units.get(unit).ok_or(BattleError::NoSuchUnit)?;
        if u.side != self.active || !u.is_active() {
            return Err(BattleError::NotYours);
        }
        Ok(u)
    }

    pub fn check_can_attack(u: &BattleUnit) -> Result<(), BattleError> {
        if u.attacked {
            return Err(BattleError::AlreadyAttacked);
        }
        if u.moves_left == 0 {
            return Err(BattleError::NoMovesLeft);
        }
        Ok(())
    }

    /// A unit that may strike the wall section at `cell`: one of the attackers' wall crushers
    /// that has not attacked, with a standing wall there.
    pub fn check_wall_attacker(&self, unit: usize, cell: Pos) -> Result<&BattleUnit, BattleError> {
        let u = self.check_unit(unit)?;
        Self::check_can_attack(u)?;
        if u.side != Side::Attacker || !u.crushes_walls() {
            return Err(BattleError::NotWallCrusher);
        }
        if !self.map.has_wall(cell) {
            return Err(BattleError::NoWall);
        }
        Ok(u)
    }

    /// A wall crusher strikes a wall section: it falls with the percent of `city_wall` for blows
    /// from next to it, or for a ranged attack (which costs a shot).
    pub fn attack_wall(&mut self, unit: usize, cell: Pos, ranged: bool) -> Vec<BattleEvent> {
        let destroyed = self.roll_wall_falls(ranged);
        if destroyed {
            self.map.break_wall(cell);
        }
        let shot_mana = self.rules().shot_mana;
        let u = &mut self.units[unit];
        u.revealed = true;
        if ranged {
            u.spend_shot(shot_mana, &self.content.rules);
            u.spend_ranged_moves(self.content.rules.battle.shot_moves);
        } else {
            u.spend_melee_moves();
        }
        vec![BattleEvent::WallAttacked {
            unit,
            cell,
            destroyed,
        }]
    }

    pub fn roll_wall_falls(&mut self, ranged: bool) -> bool {
        use rand::Rng;
        let wall = &self.content.rules.battle.city_wall;
        let percent = if ranged {
            wall.crush_ranged_percent
        } else {
            wall.crush_melee_percent
        };
        self.rng.gen_range(1..=100) <= percent
    }

    /// The defence a unit being hit gains from the city's walls: against attacks from outside
    /// the city area (melee or ranged: it is the position that counts, not the attack),
    /// `wall_defence` on a standing wall and `city_defence` elsewhere in the city (the original's
    /// `Battle_Unit_Process_Attack`), or `wall_defence` everywhere inside with `walls_cover_city`.
    /// Only a city with walls gives it.
    pub fn city_defense_bonus(&self, hit: usize, from: usize) -> i32 {
        let (hit, from) = (&self.units[hit], &self.units[from]);
        if !self.map.is_walled() || !in_city_area(hit.cell) || in_city_area(from.cell) {
            return 0;
        }
        let rules = &self.content.rules.battle;
        if self.walls_cover_city || self.map.has_wall(hit.cell) {
            rules.wall_defence
        } else {
            rules.city_defence
        }
    }

    /// After an attack by a wall crusher on a unit standing on a wall section, the wall may
    /// fall too.
    pub fn crush_wall_under(&mut self, attacker: usize, target: usize) -> Vec<BattleEvent> {
        let cell = self.units[target].cell;
        let a = &self.units[attacker];
        if a.side != Side::Attacker || !a.crushes_walls() || !self.map.has_wall(cell) {
            return Vec::new();
        }
        let ranged = distance(a.cell, cell) > 1;
        let destroyed = self.roll_wall_falls(ranged);
        if destroyed {
            self.map.break_wall(cell);
        }
        vec![BattleEvent::WallAttacked {
            unit: attacker,
            cell,
            destroyed,
        }]
    }

    /// The penalty to hit that Blur on `hit` gives `attacker` (`blur_penalty`), none if the
    /// attacker is immune to illusions.
    pub fn blur_penalty(&self, attacker: usize, hit: usize) -> i32 {
        let blurred = self.now(hit).is_some_and(|u| u.blur())
            && !self.units[attacker].has(Ability::new(31 /* illusion_immunity */))
            && !self.field_spares(&self.units[attacker], Field::Blur);
        if blurred {
            self.rules().to_hit.blur_penalty
        } else {
            0
        }
    }

    /// The penalty to hit that `hit`'s invisibility gives `attacker`: so much for each square of
    /// distance (`invisibility_penalty_per_square`), none if the attacker is immune to illusions.
    pub fn invisibility_penalty(&self, attacker: usize, hit: usize) -> i32 {
        if self.now(hit).is_some_and(|u| u.is_invisible())
            && !self.units[attacker].has(Ability::new(31 /* illusion_immunity */))
        {
            distance(self.units[attacker].cell, self.units[hit].cell) as i32
                * self.rules().to_hit.invisibility_penalty_per_square
        } else {
            0
        }
    }

    /// Whether `viewer`'s side can make out unit `id`. Its own units always; an invisible enemy
    /// only when the viewer has a unit immune to illusions, when one of the viewer's units
    /// stands next to it, or while it shows itself by having attacked or cast a spell. What
    /// cannot be seen cannot be aimed at, and its place is not shown (spells that strike a
    /// whole area still reach it).
    pub fn seen_by(&self, viewer: Side, id: usize) -> bool {
        let Some(u) = self.units.get(id) else {
            return false;
        };
        if u.side == viewer || !self.now_of(u).is_invisible() || u.revealed {
            return true;
        }
        self.active_units(viewer).any(|e| {
            e.has(Ability::new(31 /* illusion_immunity */)) || distance(e.cell, u.cell) == 1
        })
    }

    /// Whether a Spell Ward keeps `side` from casting spells of `realm`: it guards the city
    /// against that realm and the side is attacking the city.
    pub fn ward_stops(&self, side: Side, realm: Option<f4x_def::rules::RealmId>) -> bool {
        side == Side::Attacker && realm.is_some_and(|r| self.warded_realms.contains(&r))
    }

    /// Whether a Wall of Fire burns this unit when it crosses: walkers do, fliers and the units
    /// that move instantly do not.
    pub fn burns_at_the_wall(&self, unit: usize) -> bool {
        let u = &self.units[unit];
        !u.flag(UnitFlag::Flies) && !u.moves_instantly()
    }

    /// A Wall of Fire's attack on a unit (`wall_of_fire_strength`): area, fire.
    pub fn burn(&mut self, unit: usize) -> Vec<BattleEvent> {
        let traits = f4x_def::rules::effects::DamageTraits {
            area: true,
            ..Default::default()
        };
        let nature = AttackNature::magical(Realm::Chaos, &[DamageType::Fire]);
        let dmg = self.spell_damage(unit, self.rules().wall_of_fire_strength, &traits, nature);
        self.spell_events_for_damage(unit, dmg, nature.types)
    }

    /// Whether a Wall of Darkness stops `unit` shooting at `target`: the target is in the city
    /// and the shooter outside it, and the shooter does not see through illusions (the
    /// original's code forgets the innate immunity, which its help text allows).
    pub fn blocked_by_darkness(&self, unit: usize, target: usize) -> bool {
        self.wall_of_darkness
            && in_city_area(self.units[target].cell)
            && !in_city_area(self.units[unit].cell)
            && !self.units[unit].has(Ability::new(31 /* illusion_immunity */))
    }

    pub fn check_target(&self, unit: usize, target: usize) -> Result<&BattleUnit, BattleError> {
        let t = self.units.get(target).ok_or(BattleError::NoSuchUnit)?;
        if !t.is_active() || t.side == self.units[unit].side {
            return Err(BattleError::BadTarget);
        }
        if !self.seen_by(self.units[unit].side, target) {
            return Err(BattleError::Unseen);
        }
        Ok(t)
    }

    /// Carry out a command for the side whose turn it is.
    pub fn apply(&mut self, command: BattleCommand) -> Result<Vec<BattleEvent>, BattleError> {
        if self.winner.is_some() {
            return Err(BattleError::Over);
        }
        let mut events = Vec::new();
        match command {
            BattleCommand::Move { unit, to } => {
                let u = self.check_unit(unit)?;
                if u.moves_left == 0 {
                    return Err(BattleError::NoMovesLeft);
                }
                // Staying where it stands is not a move.
                if to == u.cell {
                    return Err(BattleError::Unreachable);
                }
                if u.moves_instantly() {
                    let cost = *self
                        .instant_squares(unit)
                        .get(&to)
                        .ok_or(BattleError::Unreachable)?;
                    let left = u.moves_left.saturating_sub(cost);
                    let u = &mut self.units[unit];
                    u.cell = to;
                    u.set_moves_left(left);
                    events.push(BattleEvent::Moved {
                        unit,
                        path: vec![to],
                    });
                } else {
                    let occupied = |p: Pos| self.is_occupied(p);
                    let reach = self
                        .map
                        .reachable_as(u.cell, u.moves_left, u.mover(), &occupied);
                    let (cost, _) = *reach.get(&to).ok_or(BattleError::Unreachable)?;
                    // Walk the path back from the target.
                    let mut path = vec![to];
                    let mut at = to;
                    while let Some(&(_, from)) = reach.get(&at) {
                        if from == u.cell {
                            break;
                        }
                        path.push(from);
                        at = from;
                    }
                    path.reverse();
                    let left = u.moves_left.saturating_sub(cost);
                    // A Wall of Fire burns the walker that steps into the city from outside.
                    let mut burned = Vec::new();
                    let mut stopped = false;
                    if self.wall_of_fire && self.burns_at_the_wall(unit) {
                        let mut cell = u.cell;
                        for (i, &step) in path.iter().enumerate() {
                            if !in_city_area(cell) && in_city_area(step) {
                                burned = self.burn(unit);
                                if !self.units[unit].is_active() {
                                    path.truncate(i);
                                    stopped = true;
                                    break;
                                }
                            }
                            cell = step;
                        }
                    }
                    let u = &mut self.units[unit];
                    u.cell = path.last().copied().unwrap_or(u.cell);
                    u.set_moves_left(left);
                    if !stopped || !path.is_empty() {
                        events.push(BattleEvent::Moved { unit, path });
                    }
                    events.extend(burned);
                }
            }
            BattleCommand::Attack { unit, target } => {
                let u = self.check_unit(unit)?;
                Self::check_can_attack(u)?;
                let t = self.check_target(unit, target)?;
                if distance(u.cell, t.cell) != 1 {
                    return Err(BattleError::NotAdjacent);
                }
                // Striking into a city behind a Wall of Fire from outside it burns the attacker.
                if self.wall_of_fire
                    && self.burns_at_the_wall(unit)
                    && !in_city_area(u.cell)
                    && in_city_area(t.cell)
                {
                    events.extend(self.burn(unit));
                    if !self.units[unit].is_active() {
                        return Ok(events);
                    }
                }
                events.extend(self.melee(unit, target));
            }
            BattleCommand::Shoot { unit, target } => {
                let u = self.check_unit(unit)?;
                Self::check_can_attack(u)?;
                if !matches!(u.ranged_kind, RangedKind::Missile | RangedKind::Magic)
                    || self.now_of(u).ranged() <= 0
                {
                    return Err(BattleError::NoRangedAttack);
                }
                if u.ammo <= 0 {
                    return Err(BattleError::OutOfAmmo);
                }
                self.check_target(unit, target)?;
                if self.blocked_by_darkness(unit, target) {
                    return Err(BattleError::WallOfDarkness);
                }
                events.extend(self.shoot(unit, target));
            }
            BattleCommand::AttackWall { unit, cell } => {
                let u = self.check_wall_attacker(unit, cell)?;
                if distance(u.cell, cell) != 1 {
                    return Err(BattleError::NotAdjacent);
                }
                events.extend(self.attack_wall(unit, cell, false));
            }
            BattleCommand::ShootWall { unit, cell } => {
                let u = self.check_wall_attacker(unit, cell)?;
                if !matches!(u.ranged_kind, RangedKind::Missile | RangedKind::Magic)
                    || self.now_of(u).ranged() <= 0
                {
                    return Err(BattleError::NoRangedAttack);
                }
                if u.ammo <= 0 {
                    return Err(BattleError::OutOfAmmo);
                }
                events.extend(self.attack_wall(unit, cell, true));
            }
            // Spells need the rules, which only the game has (`spells.rs`).
            BattleCommand::Cast { .. }
            | BattleCommand::UnitCast { .. }
            | BattleCommand::SummonDemon { .. } => {
                return Err(BattleError::Spell(SpellError::NeedsGame))
            }
            BattleCommand::Merge { unit } => {
                let u = self.check_unit(unit)?;
                if !u.can_merge() {
                    return Err(BattleError::CannotMerge);
                }
                let u = &mut self.units[unit];
                u.status = Status::Merged;
                u.merged_this_turn = true;
                u.end_moves();
                events.push(BattleEvent::Merged { unit });
            }
            BattleCommand::Surface { unit, to } => {
                let u = self.units.get(unit).ok_or(BattleError::NoSuchUnit)?;
                if u.side != self.active || u.status != Status::Merged || u.merged_this_turn {
                    return Err(BattleError::NotMerged);
                }
                if !self.free_squares().contains(&to) {
                    return Err(BattleError::Unreachable);
                }
                let u = &mut self.units[unit];
                u.status = Status::Active;
                u.cell = to;
                u.surfaced = true;
                u.refill_moves();
                u.attacked = false;
                events.push(BattleEvent::Surfaced { unit, cell: to });
            }
            BattleCommand::Done { unit } => {
                self.check_unit(unit)?;
                self.units[unit].end_moves();
                self.units[unit].attacked = true;
            }
            BattleCommand::MoveVortex { index, to } => {
                events.extend(self.steer_vortex(index, to)?);
            }
            BattleCommand::EndTurn => {
                self.active = self.active.other();
                if self.active == Side::Attacker {
                    self.turn += 1;
                }
                events.push(BattleEvent::TurnStarted {
                    side: self.active,
                    turn: self.turn,
                });
                events.extend(self.begin_side_turn());
            }
            BattleCommand::Flee => {
                let side = self.active;
                for u in self.units.iter_mut().filter(|u| u.side == side) {
                    if u.status == Status::Merged {
                        u.slipped = true;
                        u.status = Status::Fled;
                    } else if u.is_active() {
                        u.status = Status::Fled;
                    }
                }
                events.push(BattleEvent::Fled { side });
            }
        }
        events.extend(self.check_winner());
        Ok(events)
    }

    pub fn fighter(&self, id: usize, vs_flier_blocked: bool) -> Fighter {
        let u = self.now_of(&self.units[id]);
        let rules = &self.content.rules;
        let mut melee = u.melee_combatant(rules, self.magic_immunity_exceptions);
        if vs_flier_blocked {
            // A walker's blows reach a flier by the setting: not at all (the original, so only
            // thrown weapons and breath could hurt it), half as well, or in full (divergence 26).
            melee.strength = self.melee_vs_fliers.strength(melee.strength);
        }
        Fighter {
            melee,
            thrown: u.thrown_combatant(rules, self.magic_immunity_exceptions),
            first_strike: u.flag(UnitFlag::FirstStrike),
            negate_first_strike: u.flag(UnitFlag::NegatesFirstStrike),
            hasted: u.is_hasted(),
            state: u.defender_state(rules),
        }
    }

    /// Take the mana of a Power Drain off the unit, with the event when it had any to lose.
    pub fn power_drained(&mut self, unit: usize, amount: i32) -> Option<BattleEvent> {
        let lost = self.units[unit].drain_power(amount);
        (lost > 0).then_some(BattleEvent::PowerDrained { unit, amount: lost })
    }

    pub fn melee(&mut self, att: usize, def: usize) -> Vec<BattleEvent> {
        let reach_blocked =
            self.units[def].flag(UnitFlag::Flies) && !self.units[att].flag(UnitFlag::Flies);
        let mut a = self.fighter(att, reach_blocked);
        let mut d = self.fighter(def, false);
        a.state.defense += self.city_defense_bonus(att, def);
        d.state.defense += self.city_defense_bonus(def, att);
        // Invisible creatures are harder to hit (by anyone not immune to illusions).
        for (fighter, hits, hit) in [(&mut a, att, def), (&mut d, def, att)] {
            let penalty = self.invisibility_penalty(hits, hit) + self.blur_penalty(hits, hit);
            fighter.melee.to_hit -= penalty;
            if let Some(t) = &mut fighter.thrown {
                t.to_hit -= penalty;
            }
        }
        // Which immunity, if any, will stop the blows of each side (for the log).
        let stopped_def = d.state.immunities.stopped_by(a.melee.types());
        let stopped_att = a.state.immunities.stopped_by(d.melee.types());
        let rng: &mut dyn RngCore = &mut self.rng;
        let out = attack::melee_exchange(&self.content.rules.battle, &mut a, &mut d, rng);
        let mut events = vec![BattleEvent::Attacked {
            attacker: att,
            defender: def,
            dealt: out.dealt.points,
            dealt_kills: out.dealt.kills,
            taken: out.taken.points,
            taken_kills: out.taken.kills,
        }];
        if let Some(by) = stopped_def.filter(|_| out.dealt.points == 0) {
            events.push(BattleEvent::Unharmed {
                attacker: att,
                defender: def,
                by,
            });
        }
        if let Some(by) = stopped_att.filter(|_| out.taken.points == 0) {
            events.push(BattleEvent::Unharmed {
                attacker: def,
                defender: att,
                by,
            });
        }
        self.units[att].set_from(&a.state);
        self.units[def].set_from(&d.state);
        self.units[def].record(out.dealt);
        self.units[att].record(out.taken);
        self.units[att].heal_kinds(out.dealt.healed);
        self.units[def].heal_kinds(out.taken.healed);
        self.units[att].revealed = true;
        self.units[att].spend_melee_moves();
        for (id, mana_lost) in [(def, out.dealt.mana_lost), (att, out.taken.mana_lost)] {
            events.extend(self.power_drained(id, mana_lost));
        }
        for id in [att, def] {
            if !self.units[id].is_active() {
                events.push(BattleEvent::UnitDied { unit: id });
            }
        }
        events.extend(self.crush_wall_under(att, def));
        events
    }

    pub fn shoot(&mut self, att: usize, def: usize) -> Vec<BattleEvent> {
        let penalty = {
            let a = &self.units[att];
            if a.ranged_kind == RangedKind::Missile {
                self.content.rules.battle.to_hit.range_penalty(
                    distance(a.cell, self.units[def].cell),
                    a.flag(UnitFlag::LongRange),
                )
            } else {
                0
            }
        };
        let a = self.now_of(&self.units[att]);
        let penalty = penalty + self.invisibility_penalty(att, def) + self.blur_penalty(att, def);
        let plus = if a.ranged_kind == RangedKind::Missile {
            a.weapon_plus(&self.content.rules)
        } else {
            0
        };
        let shooter = a.combatant(
            Mode::Ranged,
            a.ranged(),
            a.to_hit() + plus + a.ranged_plus - penalty,
            &self.content.rules,
            self.magic_immunity_exceptions,
            a.bonus(),
        );
        let figures = a.figures;
        let volleys = if a.is_hasted() { 2 } else { 1 };
        let shot_mana = self.rules().shot_mana;
        let mut state = self
            .now_of(&self.units[def])
            .defender_state(&self.content.rules);
        state.defense += self.city_defense_bonus(def, att);
        let stopped = state.immunities.stopped_by(shooter.types());
        let mut dmg = Damage::default();
        // Haste: a second volley while there is ammunition (or mana) for it and the target stands.
        for volley in 0..volleys {
            if volley > 0 && (self.units[att].ammo <= 0 || state.figures == 0) {
                break;
            }
            let rng: &mut dyn RngCore = &mut self.rng;
            let d = attack::ranged_attack(
                &self.content.rules.battle,
                &shooter,
                figures,
                &mut state,
                rng,
            );
            self.units[def].set_from(&state);
            self.units[def].record(d);
            let a = &mut self.units[att];
            if d.healed > 0 {
                a.heal_points(d.healed, true);
            }
            a.spend_shot(shot_mana, &self.content.rules);
            dmg += d;
        }
        let a = &mut self.units[att];
        a.revealed = true;
        a.spend_ranged_moves(self.content.rules.battle.shot_moves);
        let mut events = vec![BattleEvent::Shot {
            attacker: att,
            defender: def,
            dealt: dmg.points,
            kills: dmg.kills,
        }];
        if let Some(by) = stopped.filter(|_| dmg.points == 0) {
            events.push(BattleEvent::Unharmed {
                attacker: att,
                defender: def,
                by,
            });
        }
        events.extend(self.power_drained(def, dmg.mana_lost));
        if !self.units[def].is_active() {
            events.push(BattleEvent::UnitDied { unit: def });
        }
        events.extend(self.crush_wall_under(att, def));
        events
    }
}

/// What an attack value, a defence value and hit points are worth together: the kernel of
/// [`power::unit_power`], for a unit in a fight (the hit points it has left now) where no unit
/// definition is at hand. Everything outside a fight uses `power::unit_power`.
pub fn strength(attack: i32, defense: i32, hit_points: i32) -> f32 {
    power::kernel(attack as f32, defense as f32, hit_points as f32)
}
