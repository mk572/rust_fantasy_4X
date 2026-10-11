//! Tactical battles.
//!
//! A [`Battle`] is a self-contained record, not part of the overland world: it is built from
//! the units that meet (their statistics are copied at the start, with every modifier
//! applied), played through [`Battle::apply`] with [`BattleCommand`]s, and when it ends the
//! game takes the results back (`game.rs`). Because it needs nothing from the world, battles
//! can be tested, replayed and driven by an AI on their own.
//!
//! Sides alternate: the attacker plays all its units, then the defender, and a new turn
//! starts when the defender is done. A side loses when it has no active unit left.

pub mod ai;
pub mod attack;
pub mod game;
pub mod immunity;
pub mod map;
pub mod special;

use rand::{RngCore, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::city::{PlayerId, Position};

use attack::{Combatant, DefenderState, Fighter, Mode};
use immunity::{AttackNature, Immunities, Realm};
use map::{distance, BattleMap, Pos, OPEN_COST};
use special::{Gaze, SpecialAttacks};

/// A side never fights on past this turn: the defender holds.
pub const TURN_LIMIT: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Attacker,
    Defender,
}

impl Side {
    pub fn index(self) -> usize {
        match self {
            Side::Attacker => 0,
            Side::Defender => 1,
        }
    }

    pub fn other(self) -> Side {
        match self {
            Side::Attacker => Side::Defender,
            Side::Defender => Side::Attacker,
        }
    }
}

/// How a unit's ranged attack works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangedKind {
    None,
    /// Arrows, slings and boulders: fired at a distance with a penalty, using ammunition.
    Missile,
    /// Spell shots: no range penalty, using ammunition (mana).
    Magic,
    /// Thrown weapons and breath: used just before melee, no ammunition.
    Short,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Active,
    Dead,
    Fled,
}

/// One unit in a battle, with the statistics it fights with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BattleUnit {
    pub id: usize,
    pub side: Side,
    /// The overland unit this was copied from (a list index, not an entity: see `game.rs`).
    pub source: Option<usize>,
    /// The unit definition's id and display name.
    pub unit_id: String,
    pub name: String,
    pub cell: Pos,
    pub melee: i32,
    pub ranged: i32,
    pub defense: i32,
    pub resistance: i32,
    /// Pluses to hit (each ten percentage points).
    pub to_hit: i32,
    pub to_block: i32,
    /// Hit points of one figure.
    pub hits: i32,
    pub figures_start: u32,
    pub figures: u32,
    /// Damage on the front figure.
    pub front_damage: i32,
    /// Half-moves per turn and left this turn.
    pub moves: u32,
    pub moves_left: u32,
    pub ammo: i32,
    pub ranged_kind: RangedKind,
    pub flies: bool,
    pub first_strike: bool,
    pub negate_first_strike: bool,
    pub long_range: bool,
    /// Its weapons are magical, so weapon immunity does not stop its normal attacks: heroes,
    /// fantastic and summoned creatures, units with a flaming or blessed blade.
    pub magical_weapon: bool,
    /// `Stat::SpecialLevel`: poison strength, or the penalty of its gaze and touches.
    pub special_level: i32,
    /// The ids of its lasting spells, which add to resistance against some realms.
    pub enchantments: Vec<String>,
    /// The creature's realm (chaos and death creatures fear dispel evil), and whether it is
    /// undead.
    pub realm: Option<Realm>,
    pub undead: bool,
    pub abilities: Vec<String>,
    pub status: Status,
    /// Has attacked or shot this turn.
    pub attacked: bool,
}

impl BattleUnit {
    /// A plain unit for tests and simple scenarios: one dice per melee point, no specials.
    pub fn simple(
        side: Side,
        name: &str,
        figures: u32,
        hits: i32,
        melee: i32,
        defense: i32,
    ) -> Self {
        Self {
            id: 0,
            side,
            source: None,
            unit_id: name.to_string(),
            name: name.to_string(),
            cell: (0, 0),
            melee,
            ranged: 0,
            defense,
            resistance: 4,
            to_hit: 0,
            to_block: 0,
            hits,
            figures_start: figures,
            figures,
            front_damage: 0,
            moves: 2 * OPEN_COST,
            moves_left: 2 * OPEN_COST,
            ammo: 0,
            ranged_kind: RangedKind::None,
            flies: false,
            first_strike: false,
            negate_first_strike: false,
            long_range: false,
            magical_weapon: false,
            special_level: 0,
            enchantments: Vec::new(),
            realm: None,
            undead: false,
            abilities: Vec::new(),
            status: Status::Active,
            attacked: false,
        }
    }

    pub fn at(mut self, cell: Pos) -> Self {
        self.cell = cell;
        self
    }

    /// Teleporting and merging units go to any free square for one move.
    pub fn moves_instantly(&self) -> bool {
        self.abilities
            .iter()
            .any(|a| a == "teleport" || a == "merging")
    }

    pub fn is_active(&self) -> bool {
        self.status == Status::Active
    }

    /// Hit points left over all figures.
    pub fn hit_points(&self) -> i32 {
        self.defender_state().hit_points()
    }

    pub fn max_hit_points(&self) -> i32 {
        self.figures_start as i32 * self.hits
    }

    fn defender_state(&self) -> DefenderState {
        DefenderState {
            hits: self.hits,
            defense: self.defense,
            immunities: Immunities::of(&self.abilities),
            resistance: self.resistance,
            enchantments: self.enchantments.clone(),
            realm: self.realm,
            undead: self.undead,
            figures_max: self.figures_start,
            to_block: self.to_block,
            figures: self.figures,
            front_damage: self.front_damage,
        }
    }

    fn set_from(&mut self, state: &DefenderState) {
        self.figures = state.figures;
        self.front_damage = state.front_damage;
        if self.figures == 0 {
            self.status = Status::Dead;
        }
    }

    fn has(&self, ability: &str) -> bool {
        self.abilities.iter().any(|a| a == ability)
    }

    fn special_attacks(&self) -> SpecialAttacks {
        SpecialAttacks::of_unit(&self.abilities, self.special_level)
    }

    /// A combatant for one of the unit's attacks.
    fn combatant(&self, mode: Mode, strength: i32, to_hit: i32) -> Combatant {
        let special = self.special_attacks();
        Combatant {
            strength,
            to_hit,
            // Chaos damage, and the damage of a multiple gaze, ignores every defence.
            ignores_defense: self.has("doom_damage")
                || (mode == Mode::ThrownOrBreath && special.gaze == Gaze::Multi),
            mode,
            nature: AttackNature::of_unit_attack(mode, &self.abilities, self.magical_weapon),
            special,
        }
    }

    fn melee_combatant(&self) -> Combatant {
        self.combatant(Mode::Melee, self.melee, self.to_hit)
    }

    fn thrown_combatant(&self) -> Option<Combatant> {
        (self.ranged_kind == RangedKind::Short && self.ranged > 0)
            .then(|| self.combatant(Mode::ThrownOrBreath, self.ranged, self.to_hit))
    }
}

/// Everything a player or AI can ask a unit to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BattleCommand {
    /// Walk to a square within reach.
    Move { unit: usize, to: Pos },
    /// Attack an adjacent enemy in melee (thrown weapons and breath come first).
    Attack { unit: usize, target: usize },
    /// Fire at an enemy at a distance.
    Shoot { unit: usize, target: usize },
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BattleEvent {
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
    UnitDied {
        unit: usize,
    },
    TurnStarted {
        side: Side,
        turn: u32,
    },
    Fled {
        side: Side,
    },
    Ended {
        winner: Side,
    },
}

/// The state of one battle.
#[derive(Debug, Clone)]
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
    rng: Pcg64Mcg,
}

impl Battle {
    /// Counter Magic: put `strength` (the mana paid for the spell) into `side`'s reserve.
    pub fn cast_counter_magic(&mut self, side: Side, strength: u32) {
        self.counter_magic[side.index()] = strength;
    }

    /// Whether `side`'s enemy's Counter Magic stops a spell of `difficulty` that `side` casts: a
    /// dispel of the reserve's strength must beat the spell. Every attempt drains the reserve
    /// by [`crate::dispel::COUNTER_MAGIC_DRAIN`], whether or not it stopped the spell.
    pub fn counter_magic_stops(&mut self, side: Side, difficulty: u32) -> bool {
        let reserve = &mut self.counter_magic[side.other().index()];
        let strength = *reserve;
        *reserve = reserve.saturating_sub(crate::dispel::COUNTER_MAGIC_DRAIN);
        crate::dispel::dispel_roll(strength, difficulty, &mut self.rng)
    }

    /// Start a battle. Unit ids are assigned in list order; the attacker moves first.
    pub fn new(
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
            rng: Pcg64Mcg::seed_from_u64(seed),
        };
        b.begin_side_turn();
        b.check_winner();
        b
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

    /// Squares the unit could walk to now, with the half-moves each costs.
    pub fn reachable(&self, unit: usize) -> std::collections::HashMap<Pos, u32> {
        let Some(u) = self.units.get(unit).filter(|u| u.is_active()) else {
            return Default::default();
        };
        if u.moves_left > 0 && u.moves_instantly() {
            return self.instant_squares(unit);
        }
        let occupied = |p: Pos| self.unit_at(p).is_some();
        self.map
            .reachable(u.cell, u.moves_left, u.flies, &occupied)
            .into_iter()
            .filter(|&(p, _)| p != u.cell)
            .map(|(p, (c, _))| (p, c))
            .collect()
    }

    /// Where a teleporting or merging unit can appear: every free square that is not blocked,
    /// each for one move (`Move_Battle_Unit`: no path is walked; the original takes two half-moves,
    /// or whatever is left if that is less).
    fn instant_squares(&self, unit: usize) -> std::collections::HashMap<Pos, u32> {
        let cell = self.units[unit].cell;
        let mut squares = std::collections::HashMap::new();
        for y in 0..crate::battle::map::GRID_H {
            for x in 0..crate::battle::map::GRID_W {
                let p = (x, y);
                let open = self.map.enter_cost(p, false).is_some();
                if p != cell && open && self.unit_at(p).is_none() {
                    squares.insert(p, crate::battle::map::OPEN_COST);
                }
            }
        }
        squares
    }

    fn begin_side_turn(&mut self) {
        let side = self.active;
        for u in self
            .units
            .iter_mut()
            .filter(|u| u.side == side && u.is_active())
        {
            u.moves_left = u.moves;
            u.attacked = false;
        }
    }

    fn check_winner(&mut self) -> Option<BattleEvent> {
        if self.winner.is_some() {
            return None;
        }
        let alive = |s| self.active_units(s).count();
        let winner = if alive(Side::Attacker) == 0 {
            Some(Side::Defender)
        } else if alive(Side::Defender) == 0 {
            Some(Side::Attacker)
        } else if self.turn > TURN_LIMIT {
            Some(Side::Defender)
        } else {
            None
        };
        self.winner = winner;
        winner.map(|winner| BattleEvent::Ended { winner })
    }

    fn check_unit(&self, unit: usize) -> Result<&BattleUnit, BattleError> {
        if self.winner.is_some() {
            return Err(BattleError::Over);
        }
        let u = self.units.get(unit).ok_or(BattleError::NoSuchUnit)?;
        if u.side != self.active || !u.is_active() {
            return Err(BattleError::NotYours);
        }
        Ok(u)
    }

    fn check_target(&self, unit: usize, target: usize) -> Result<&BattleUnit, BattleError> {
        let t = self.units.get(target).ok_or(BattleError::NoSuchUnit)?;
        if !t.is_active() || t.side == self.units[unit].side {
            return Err(BattleError::BadTarget);
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
                if u.moves_instantly() {
                    let cost = *self
                        .instant_squares(unit)
                        .get(&to)
                        .ok_or(BattleError::Unreachable)?;
                    let left = u.moves_left.saturating_sub(cost);
                    let u = &mut self.units[unit];
                    u.cell = to;
                    u.moves_left = left;
                    events.push(BattleEvent::Moved {
                        unit,
                        path: vec![to],
                    });
                } else {
                    let occupied = |p: Pos| self.unit_at(p).is_some();
                    let reach = self.map.reachable(u.cell, u.moves_left, u.flies, &occupied);
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
                    let u = &mut self.units[unit];
                    u.cell = to;
                    u.moves_left = left;
                    events.push(BattleEvent::Moved { unit, path });
                }
            }
            BattleCommand::Attack { unit, target } => {
                let u = self.check_unit(unit)?;
                if u.attacked {
                    return Err(BattleError::AlreadyAttacked);
                }
                let t = self.check_target(unit, target)?;
                if distance(u.cell, t.cell) != 1 {
                    return Err(BattleError::NotAdjacent);
                }
                events.extend(self.melee(unit, target));
            }
            BattleCommand::Shoot { unit, target } => {
                let u = self.check_unit(unit)?;
                if u.attacked {
                    return Err(BattleError::AlreadyAttacked);
                }
                if !matches!(u.ranged_kind, RangedKind::Missile | RangedKind::Magic)
                    || u.ranged <= 0
                {
                    return Err(BattleError::NoRangedAttack);
                }
                if u.ammo <= 0 {
                    return Err(BattleError::OutOfAmmo);
                }
                self.check_target(unit, target)?;
                events.extend(self.shoot(unit, target));
            }
            BattleCommand::Done { unit } => {
                self.check_unit(unit)?;
                self.units[unit].moves_left = 0;
                self.units[unit].attacked = true;
            }
            BattleCommand::EndTurn => {
                self.active = self.active.other();
                if self.active == Side::Attacker {
                    self.turn += 1;
                }
                self.begin_side_turn();
                events.push(BattleEvent::TurnStarted {
                    side: self.active,
                    turn: self.turn,
                });
            }
            BattleCommand::Flee => {
                let side = self.active;
                for u in self
                    .units
                    .iter_mut()
                    .filter(|u| u.side == side && u.is_active())
                {
                    u.status = Status::Fled;
                }
                events.push(BattleEvent::Fled { side });
            }
        }
        if let Some(end) = self.check_winner() {
            events.push(end);
        }
        Ok(events)
    }

    fn fighter(&self, id: usize, vs_flier_blocked: bool) -> Fighter {
        let u = &self.units[id];
        let mut melee = u.melee_combatant();
        if vs_flier_blocked {
            // A walker's blows cannot reach a flier; only thrown weapons and breath can.
            melee.strength = 0;
        }
        Fighter {
            melee,
            thrown: u.thrown_combatant(),
            first_strike: u.first_strike,
            negate_first_strike: u.negate_first_strike,
            state: u.defender_state(),
        }
    }

    fn melee(&mut self, att: usize, def: usize) -> Vec<BattleEvent> {
        let reach_blocked = self.units[def].flies && !self.units[att].flies;
        let mut a = self.fighter(att, reach_blocked);
        let mut d = self.fighter(def, false);
        let rng: &mut dyn RngCore = &mut self.rng;
        let out = attack::melee_exchange(&mut a, &mut d, rng);
        let mut events = vec![BattleEvent::Attacked {
            attacker: att,
            defender: def,
            dealt: out.dealt.points,
            dealt_kills: out.dealt.kills,
            taken: out.taken.points,
            taken_kills: out.taken.kills,
        }];
        self.units[att].set_from(&a.state);
        self.units[def].set_from(&d.state);
        self.units[att].attacked = true;
        self.units[att].moves_left = 0;
        for id in [att, def] {
            if !self.units[id].is_active() {
                events.push(BattleEvent::UnitDied { unit: id });
            }
        }
        events
    }

    fn shoot(&mut self, att: usize, def: usize) -> Vec<BattleEvent> {
        let penalty = {
            let a = &self.units[att];
            if a.ranged_kind == RangedKind::Missile {
                attack::range_penalty(distance(a.cell, self.units[def].cell), a.long_range)
            } else {
                0
            }
        };
        let a = &self.units[att];
        let shooter = a.combatant(Mode::Ranged, a.ranged, a.to_hit - penalty);
        let figures = a.figures;
        let mut state = self.units[def].defender_state();
        let rng: &mut dyn RngCore = &mut self.rng;
        let dmg = attack::ranged_attack(&shooter, figures, &mut state, rng);
        self.units[def].set_from(&state);
        let a = &mut self.units[att];
        if dmg.healed > 0 {
            let mut mine = a.defender_state();
            mine.heal(dmg.healed);
            a.set_from(&mine);
        }
        a.ammo -= 1;
        a.attacked = true;
        let mut events = vec![BattleEvent::Shot {
            attacker: att,
            defender: def,
            dealt: dmg.points,
            kills: dmg.kills,
        }];
        if !self.units[def].is_active() {
            events.push(BattleEvent::UnitDied { unit: def });
        }
        events
    }
}

#[cfg(test)]
mod tests;
