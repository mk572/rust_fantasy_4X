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
pub mod brain;
pub mod casting;
pub mod field;
pub mod game;
pub mod immunity;
pub mod legal;
pub mod map;
pub mod special;
pub mod spells;
pub mod spoils;
pub mod vortex;

use rand::{RngCore, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::city::{PlayerId, Position};
use crate::rules::SpellId;

use attack::{Combatant, DefenderState, Fighter, Mode};
use field::{Field, FieldBonus};
use immunity::{AttackNature, Immunities, Realm};
use map::{distance, in_city_area, BattleMap, Mover, Pos, OPEN_COST};
use special::{Gaze, SpecialAttacks};

pub use casting::{BattleCaster, SpellAim, SpellError};

/// A side never fights on past this turn: the defender holds.
pub const TURN_LIMIT: u32 = 100;

/// The strength of a Wall of Fire's attack.
pub const WALL_OF_FIRE_STRENGTH: i32 = 5;
/// The strength of the dispel a magic node puts against the spells of other realms.
pub const NODE_COUNTER_STRENGTH: u32 = 50;
/// What a node's aura adds to its realm's creatures: attack, defence and resistance.
pub const NODE_AURA_BONUS: i32 = 2;

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
    /// A hero Recall Hero took off the field to the summoning circle: unlike a fled unit it
    /// risks nothing.
    Recalled,
}

/// How a unit that died was brought down, by the kind of damage that did most of it (the
/// original's `bus_Dead`, `bus_Drained` and `bus_Gone`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    /// Struck down: Animate Dead can raise it.
    Slain,
    /// Drained by life steal or a creator of undead: Animate Dead can raise it, and the
    /// winner takes it as an undead unit after the battle.
    Drained,
    /// Destroyed or severed: nothing brings it back.
    Gone,
}

/// The mana a spell shot costs a caster unit.
pub const SHOT_MANA: i32 = 3;

/// What a confused unit does in a turn: one of four, chosen at random.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confusion {
    /// Stands about doing nothing.
    Idle,
    /// Goes to a random free square and stops.
    Wandered,
    /// Does nothing now, and the enemy commands it on their turn.
    Seized,
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
    /// Pluses to hit that only the melee attack gets (a hero's sword), and only the ranged or
    /// thrown attack (its bow): the plus to hit of its items.
    pub melee_plus: i32,
    pub ranged_plus: i32,
    pub to_block: i32,
    /// Hit points of one figure.
    pub hits: i32,
    /// Of `hits`, the extra ones overhealing (life steal) added in this battle: they are not
    /// part of the unit's real strength, so the damage written back ignores them.
    pub extra_hits: i32,
    pub figures_start: u32,
    pub figures: u32,
    /// Damage on the front figure.
    pub front_damage: i32,
    /// Half-moves per turn and left this turn.
    pub moves: u32,
    pub moves_left: u32,
    /// Shots left. A spell caster with a magical ranged attack shoots with its mana instead
    /// (`SHOT_MANA` a shot): its shots are what its mana pays for.
    pub ammo: i32,
    /// The mana of a spell caster unit (20, or 40 for a major one) or of a hero with casting
    /// skill (`Game::hero_mana`).
    pub mana: i32,
    /// A hero with casting skill: its spell shots are paid for with its mana too.
    pub hero_caster: bool,
    /// What the unit can cast on its own (a hero's spells and items, a caster's abilities).
    pub casting: UnitCasting,
    pub ranged_kind: RangedKind,
    pub flies: bool,
    /// A ship: it sails water and cannot leave it.
    pub sails: bool,
    /// A swimmer: it moves on land and water alike.
    pub swims: bool,
    pub first_strike: bool,
    pub negate_first_strike: bool,
    pub long_range: bool,
    /// Its weapons are magical, so weapon immunity does not stop its normal attacks: heroes,
    /// fantastic and summoned creatures, units with a flaming or blessed blade.
    pub magical_weapon: bool,
    /// The quality of its weapons (`units::WeaponQuality`): above normal, its attacks have a
    /// plus to hit (its thrown and missile attacks too, but not breath, gazes or spell shots).
    pub weapon_quality: u8,
    /// `Stat::SpecialLevel`: poison strength, or the penalty of its gaze and touches.
    pub special_level: i32,
    /// A hero's Leadership and Prayermaster in this battle (`hero.rs`), 0 for anything else: the
    /// best one on a side counts for all its units.
    pub leadership: i32,
    pub prayermaster: i32,
    /// The ids of its lasting spells, which add to resistance against some realms.
    pub enchantments: Vec<String>,
    /// The creature's realm (chaos and death creatures fear dispel evil), and whether it is
    /// undead.
    pub realm: Option<Realm>,
    pub undead: bool,
    pub abilities: Vec<String>,
    pub status: Status,
    /// The damage it has taken, by kind: ordinary, drained, and the kind that destroys.
    pub damage_kinds: [i32; 3],
    /// Has attacked or shot this turn.
    pub attacked: bool,
    /// Lasting spells cast on it in this battle, and who cast them. They last until the battle
    /// ends or a dispel takes them off.
    pub combat_spells: Vec<(SpellId, PlayerId)>,
    /// The unit as it was before the first of them, which a dispel goes back to.
    pub base: Option<Box<BattleUnit>>,
    /// What the battlefield spells have changed (`field.rs`).
    pub field: FieldBonus,
    /// Confusion has put it under the enemy's orders for their next turn.
    pub seized: bool,
    /// While it is under the enemy's orders: the side it really belongs to.
    pub turned_from: Option<Side>,
    /// Creature Binding or Possession has put it on the other side for the rest of the battle:
    /// the side it really belongs to.
    pub bound_from: Option<Side>,
}

/// What a unit casts with its own mana rather than the wizard's skill (task 265).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitCasting {
    /// A hero's innate spells: they cost the unit's mana.
    pub innate: Vec<SpellId>,
    /// The spells of a hero's items, with the slot of the item and the charges left: they cost
    /// no mana.
    pub item_charges: Vec<(usize, SpellId, u32)>,
    /// The spells its abilities cast (`casts_healing`, `casts_fireball`, ...): once a battle
    /// each, for no mana.
    pub abilities: Vec<SpellId>,
    /// The penalty to the resistance of what its spells hit (its items' spell save).
    pub spell_save: i32,
    /// Demons it can still summon this battle (`summon_demon`: three for a Demon Lord).
    pub demons: u32,
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
            melee_plus: 0,
            ranged_plus: 0,
            to_block: 0,
            hits,
            extra_hits: 0,
            figures_start: figures,
            figures,
            front_damage: 0,
            moves: 2 * OPEN_COST,
            moves_left: 2 * OPEN_COST,
            ammo: 0,
            mana: 0,
            hero_caster: false,
            casting: UnitCasting::default(),
            ranged_kind: RangedKind::None,
            flies: false,
            sails: false,
            swims: false,
            first_strike: false,
            negate_first_strike: false,
            long_range: false,
            magical_weapon: false,
            weapon_quality: 0,
            special_level: 0,
            leadership: 0,
            prayermaster: 0,
            enchantments: Vec::new(),
            realm: None,
            undead: false,
            abilities: Vec::new(),
            status: Status::Active,
            damage_kinds: [0; 3],
            attacked: false,
            combat_spells: Vec::new(),
            base: None,
            field: FieldBonus::default(),
            seized: false,
            turned_from: None,
            bound_from: None,
        }
    }

    /// Note the damage the unit took, by its kind.
    pub fn record(&mut self, d: attack::Damage) {
        self.damage_kinds[0] += d.points - d.drained - d.gone;
        self.damage_kinds[1] += d.drained;
        self.damage_kinds[2] += d.gone;
    }

    /// What healing takes off the damage tally: the ordinary damage first, then the drained.
    /// Damage that destroys is never healed.
    pub(super) fn heal_kinds(&mut self, amount: i32) {
        self.damage_kinds[0] -= amount;
        if self.damage_kinds[0] < 0 {
            self.damage_kinds[1] = (self.damage_kinds[1] + self.damage_kinds[0]).max(0);
            self.damage_kinds[0] = 0;
        }
    }

    /// The hit points healing can still bring back: all it has lost but what was destroyed.
    pub fn healable(&self) -> i32 {
        (self.max_hit_points() - self.hit_points() - self.damage_kinds[2]).max(0)
    }

    /// Heal the unit by `amount` hit points (`Battle_Unit_Heal`): the front figure, then dead
    /// figures back. Unless `overheal`, no more than `healable` comes back, since damage that
    /// destroys stays. Returns the hit points the unit gained.
    pub fn heal_points(&mut self, amount: i32, overheal: bool) -> i32 {
        let amount = if overheal {
            amount
        } else {
            amount.min(self.healable())
        };
        if amount <= 0 {
            return 0;
        }
        let before = self.hit_points();
        let mut state = self.defender_state();
        if overheal {
            state.heal_overflow(amount);
        } else {
            state.heal(amount);
        }
        self.set_from(&state);
        self.heal_kinds(amount);
        self.hit_points() - before
    }

    /// How the unit died, by the kind of damage that did most of it (as the original does:
    /// ties go to the kind that destroys, then to drain). Meaningful once the unit is dead.
    pub fn fate(&self) -> Fate {
        let [normal, drained, gone] = self.damage_kinds;
        if gone > 0 && gone >= drained && gone >= normal {
            Fate::Gone
        } else if drained > gone && drained >= normal {
            Fate::Drained
        } else {
            Fate::Slain
        }
    }

    pub fn at(mut self, cell: Pos) -> Self {
        self.cell = cell;
        self
    }

    /// How the unit crosses the field: fliers, teleporters, the merging and the non-corporeal
    /// pass through the walls of a city.
    pub fn mover(&self) -> Mover {
        Mover {
            flying: self.flies,
            sails: self.sails,
            swims: self.swims,
            through_walls: self.flies || self.moves_instantly() || self.has("non_corporeal"),
            ignores_mud: self.has("non_corporeal"),
        }
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
        self.figures_start as i32 * (self.hits - self.extra_hits)
    }

    fn defender_state(&self) -> DefenderState {
        DefenderState {
            hits: self.hits,
            extra_hits: self.extra_hits,
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
            blur: self.field.blur,
            large_shield: self.has("large_shield"),
        }
    }

    fn set_from(&mut self, state: &DefenderState) {
        self.hits = state.hits;
        self.extra_hits = state.extra_hits;
        self.figures = state.figures;
        self.front_damage = state.front_damage;
        if self.figures == 0 {
            self.status = Status::Dead;
        }
    }

    pub fn has(&self, ability: &str) -> bool {
        self.abilities.iter().any(|a| a == ability)
    }

    /// Whether the unit regenerates: the ability, or the Regeneration spell, which grants it.
    pub fn regenerates(&self) -> bool {
        self.has("regeneration")
    }

    /// Whether the unit's spell shots are paid for with its mana: a spell caster with a magical
    /// ranged attack (the original's rule for casters and heroes).
    pub fn shoots_with_mana(&self) -> bool {
        self.ranged_kind == RangedKind::Magic
            && (self.hero_caster || self.has("spellcaster") || self.has("spellcaster_major"))
    }

    /// Use up one shot (and the mana it costs a caster).
    fn spend_shot(&mut self) {
        self.ammo -= 1;
        if self.shoots_with_mana() {
            self.mana = (self.mana - SHOT_MANA).max(0);
            self.ammo = self.ammo.min(self.mana / SHOT_MANA);
        }
    }

    /// Mana Leak: a caster loses `field::MANA_LEAK_DRAIN` mana; any unit with spell shots loses
    /// one (a caster's shots follow its mana).
    pub(super) fn leak_mana(&mut self, drain: i32) {
        self.mana = (self.mana - drain).max(0);
        if self.ranged_kind == RangedKind::Magic {
            if self.shoots_with_mana() {
                self.ammo = self.ammo.min(self.mana / SHOT_MANA);
            } else {
                self.ammo = (self.ammo - 1).max(0);
            }
        }
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
            sees_through: self.has("illusion_immunity"),
            eldritch: self.has("eldritch_weapon")
                && (mode == Mode::Melee
                    || self.has("bow")
                    || self.has("sling")
                    || self.has("rock_throwing")
                    || self.has("thrown")),
            immolation: self.has("immolation"),
            fear: self.has("cause_fear"),
        }
    }

    fn melee_combatant(&self) -> Combatant {
        self.combatant(
            Mode::Melee,
            self.melee,
            self.to_hit + self.weapon_plus() + self.melee_plus,
        )
    }

    /// The plus to hit the unit's weapons give.
    fn weapon_plus(&self) -> i32 {
        i32::from(self.weapon_quality > 0 || self.has("eldritch_weapon"))
    }

    fn thrown_combatant(&self) -> Option<Combatant> {
        (self.ranged_kind == RangedKind::Short && self.ranged > 0).then(|| {
            // Only a thrown weapon gains the plus, not breath or a gaze.
            let plus = if self.has("thrown") {
                self.weapon_plus()
            } else {
                0
            };
            self.combatant(
                Mode::ThrownOrBreath,
                self.ranged,
                self.to_hit + plus + self.ranged_plus,
            )
        })
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
    /// No standing wall there.
    NoWall,
    /// Only the attackers' wall crushers attack walls.
    NotWallCrusher,
    /// The spell cannot be cast now.
    Spell(SpellError),
    /// A Wall of Darkness: no ranged attack on a unit inside the city from outside it, unless
    /// the shooter sees through illusions.
    WallOfDarkness,
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
        mana: i32,
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
    /// A damage spell hurt a unit.
    SpellDamaged {
        unit: usize,
        dealt: i32,
        kills: u32,
    },
    /// The unit took no harm from the spell: immune, or protected by a lasting spell.
    SpellImmune {
        unit: usize,
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
    },
    /// A battlefield spell went up for a side.
    FieldRaised {
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
    /// Confusion decided what the unit does this turn (`Confusion`).
    Confused {
        unit: usize,
        act: Confusion,
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
    /// Recall Hero took the hero (a battle unit id) off the field.
    HeroRecalled {
        unit: usize,
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
    /// The realms (ids) a Spell Ward guards the city against: the attackers cannot cast spells
    /// of them. (The help says "including combat spells"; the original's code does not check.)
    pub warded_realms: Vec<String>,
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
    pub(super) save_penalty: i32,
    /// The battle unit that is casting, if a unit and not the wizard: Life Drain heals it.
    pub(super) spell_caster: Option<usize>,
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

    /// Whether the node under the battle stops a spell of `realm` and `difficulty`: a dispel of
    /// strength [`NODE_COUNTER_STRENGTH`] must beat it, unless the spell is of the node's own
    /// realm. (The caster's Node Mastery is checked by the caller.)
    pub fn node_stops(&mut self, realm: Option<Realm>, difficulty: u32) -> bool {
        let Some(node) = self.node_site else {
            return false;
        };
        if realm == Some(node) {
            return false;
        }
        crate::dispel::dispel_roll(NODE_COUNTER_STRENGTH, difficulty, &mut self.rng)
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
            rng: Pcg64Mcg::seed_from_u64(seed),
        };
        let _ = b.begin_side_turn();
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
        let occupied = |p: Pos| self.is_occupied(p);
        self.map
            .reachable_as(u.cell, u.moves_left, u.mover(), &occupied)
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
                // The towers at the corners of a city's wall are shut to everyone.
                let open = self.map.enter_cost(p, false).is_some()
                    && !(map::is_wall_corner(p) && self.map.has_wall(p));
                if p != cell && open && self.unit_at(p).is_none() {
                    squares.insert(p, crate::battle::map::OPEN_COST);
                }
            }
        }
        squares
    }

    fn begin_side_turn(&mut self) -> Vec<BattleEvent> {
        let side = self.active;
        let mut events = Vec::new();
        // A new turn begins with the attacker: the spells that act every turn strike first.
        if side == Side::Attacker {
            events.extend(self.turn_effects());
        }
        for u in &mut self.units {
            if let Some(owner) = u.turned_from.take() {
                u.side = owner;
            }
        }
        // Holy Bonus and Resistance to All follow whoever is still standing.
        self.refresh_fields();
        for u in self
            .units
            .iter_mut()
            .filter(|u| u.side == side && u.is_active())
        {
            u.moves_left = u.moves;
            u.attacked = false;
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

    fn check_winner(&mut self) -> Option<BattleEvent> {
        if self.winner.is_some() {
            return None;
        }
        // A unit lent to the enemy by confusion still counts for its owner.
        let alive = |s| {
            self.units
                .iter()
                .filter(|u| u.is_active() && u.turned_from.unwrap_or(u.side) == s)
                .count()
        };
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
        if winner.is_some() {
            for u in &mut self.units {
                // A creature still under Creature Binding is destroyed (Possession only lasts).
                if u.is_active()
                    && u.bound_from.is_some()
                    && u.enchantments.iter().any(|e| e == "creature_binding")
                {
                    u.figures = 0;
                    u.status = Status::Dead;
                    u.damage_kinds[2] = u.damage_kinds[2].max(1);
                }
                if let Some(owner) = u.turned_from.take().or_else(|| u.bound_from.take()) {
                    u.side = owner;
                }
            }
        }
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

    /// A unit that may strike the wall section at `cell`: one of the attackers' wall crushers
    /// that has not attacked, with a standing wall there.
    fn check_wall_attacker(&self, unit: usize, cell: Pos) -> Result<&BattleUnit, BattleError> {
        let u = self.check_unit(unit)?;
        if u.attacked {
            return Err(BattleError::AlreadyAttacked);
        }
        if u.side != Side::Attacker || !u.has("wall_crusher") {
            return Err(BattleError::NotWallCrusher);
        }
        if !self.map.has_wall(cell) {
            return Err(BattleError::NoWall);
        }
        Ok(u)
    }

    /// A wall crusher strikes a wall section: it falls half the time to blows from next to it,
    /// a quarter of the time to a ranged attack (which costs a shot).
    fn attack_wall(&mut self, unit: usize, cell: Pos, ranged: bool) -> Vec<BattleEvent> {
        let destroyed = self.roll_wall_falls(ranged);
        if destroyed {
            self.map.break_wall(cell);
        }
        let u = &mut self.units[unit];
        u.attacked = true;
        if ranged {
            u.spend_shot();
        } else {
            u.moves_left = 0;
        }
        vec![BattleEvent::WallAttacked {
            unit,
            cell,
            destroyed,
        }]
    }

    fn roll_wall_falls(&mut self, ranged: bool) -> bool {
        use rand::Rng;
        let percent = if ranged { 25 } else { 50 };
        self.rng.gen_range(1..=100) <= percent
    }

    /// The defence a unit being hit gains from the city's walls: against attacks from outside
    /// the city area, +3 on a standing wall and +1 elsewhere in the city (the original's
    /// `Battle_Unit_Process_Attack`). Only a city with walls gives it.
    fn city_defense_bonus(&self, hit: usize, from: usize) -> i32 {
        let (hit, from) = (&self.units[hit], &self.units[from]);
        if !self.map.is_walled() || !in_city_area(hit.cell) || in_city_area(from.cell) {
            return 0;
        }
        if self.map.has_wall(hit.cell) {
            3
        } else {
            1
        }
    }

    /// After an attack by a wall crusher on a unit standing on a wall section, the wall may
    /// fall too.
    fn crush_wall_under(&mut self, attacker: usize, target: usize) -> Vec<BattleEvent> {
        let cell = self.units[target].cell;
        let a = &self.units[attacker];
        if a.side != Side::Attacker || !a.has("wall_crusher") || !self.map.has_wall(cell) {
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

    /// The penalty to hit that `hit`'s invisibility gives `attacker`: one, unless the attacker is
    /// immune to illusions.
    fn invisibility_penalty(&self, attacker: usize, hit: usize) -> i32 {
        i32::from(self.units[hit].is_invisible() && !self.units[attacker].has("illusion_immunity"))
    }

    /// Whether a Spell Ward keeps `side` from casting spells of `realm`: it guards the city
    /// against that realm and the side is attacking the city.
    pub fn ward_stops(&self, side: Side, realm: &str) -> bool {
        side == Side::Attacker && self.warded_realms.iter().any(|r| r == realm)
    }

    /// Whether a Wall of Fire burns this unit when it crosses: walkers do, fliers and the units
    /// that move instantly do not.
    fn burns_at_the_wall(&self, unit: usize) -> bool {
        let u = &self.units[unit];
        !u.flies && !u.moves_instantly()
    }

    /// A Wall of Fire's attack on a unit: strength 5, area, fire (the original casts a Fireball
    /// of its base strength).
    fn burn(&mut self, unit: usize) -> Vec<BattleEvent> {
        let traits = crate::rules::effects::DamageTraits {
            area: true,
            ..Default::default()
        };
        let nature = AttackNature {
            stopped_by: Immunities {
                fire: true,
                magic: true,
                ..Default::default()
            },
            realm: Some(Realm::Chaos),
            armor_piercing: false,
        };
        let dmg = self.spell_damage(unit, WALL_OF_FIRE_STRENGTH, &traits, nature);
        self.spell_events_for_damage(unit, dmg)
    }

    /// Whether a Wall of Darkness stops `unit` shooting at `target`: the target is in the city
    /// and the shooter outside it, and the shooter does not see through illusions (the
    /// original's code forgets the innate immunity, which its help text allows).
    fn blocked_by_darkness(&self, unit: usize, target: usize) -> bool {
        self.wall_of_darkness
            && in_city_area(self.units[target].cell)
            && !in_city_area(self.units[unit].cell)
            && !self.units[unit].has("illusion_immunity")
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
                    u.moves_left = left;
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
                    u.moves_left = left;
                    if !stopped || !path.is_empty() {
                        events.push(BattleEvent::Moved { unit, path });
                    }
                    events.extend(burned);
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
                    || u.ranged <= 0
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
            BattleCommand::Done { unit } => {
                self.check_unit(unit)?;
                self.units[unit].moves_left = 0;
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
        a.state.defense += self.city_defense_bonus(att, def);
        d.state.defense += self.city_defense_bonus(def, att);
        // Invisible creatures are harder to hit (by anyone not immune to illusions).
        for (fighter, hits, hit) in [(&mut a, att, def), (&mut d, def, att)] {
            let penalty = self.invisibility_penalty(hits, hit);
            fighter.melee.to_hit -= penalty;
            if let Some(t) = &mut fighter.thrown {
                t.to_hit -= penalty;
            }
        }
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
        self.units[def].record(out.dealt);
        self.units[att].record(out.taken);
        self.units[att].heal_kinds(out.dealt.healed);
        self.units[def].heal_kinds(out.taken.healed);
        self.units[att].attacked = true;
        self.units[att].moves_left = 0;
        for id in [att, def] {
            if !self.units[id].is_active() {
                events.push(BattleEvent::UnitDied { unit: id });
            }
        }
        events.extend(self.crush_wall_under(att, def));
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
        let penalty = penalty + self.invisibility_penalty(att, def);
        let plus = if a.ranged_kind == RangedKind::Missile {
            a.weapon_plus()
        } else {
            0
        };
        let shooter = a.combatant(
            Mode::Ranged,
            a.ranged,
            a.to_hit + plus + a.ranged_plus - penalty,
        );
        let figures = a.figures;
        let mut state = self.units[def].defender_state();
        state.defense += self.city_defense_bonus(def, att);
        let rng: &mut dyn RngCore = &mut self.rng;
        let dmg = attack::ranged_attack(&shooter, figures, &mut state, rng);
        self.units[def].set_from(&state);
        self.units[def].record(dmg);
        let a = &mut self.units[att];
        if dmg.healed > 0 {
            a.heal_points(dmg.healed, true);
        }
        a.spend_shot();
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
        events.extend(self.crush_wall_under(att, def));
        events
    }
}

#[cfg(test)]
mod tests;
