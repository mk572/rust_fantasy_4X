//! One unit in a battle: its numbers, status and what it can do, and the rules that change them.
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use f4x_def::half_int::HalfInt;
use f4x_def::id::Serial;
use f4x_def::rules::attack_nature::AttackNature;
use f4x_def::rules::damage_type::{DamageType, DamageTypes, RangedKind};
use f4x_def::rules::immunity::Immunities;
use f4x_def::rules::realm::Realm;
use f4x_def::rules::unit_effect::{UnitEffect, UnitEffects};
use f4x_def::rules::unit_flag::{UnitFlag, UnitFlags};
use f4x_def::rules::{Ability, Rules, SpellId, UnitId};

use crate::attack::{self, Combatant, DefenderState, Mode};
use crate::map::{Mover, Pos, OPEN_COST};
use crate::side::Side;
use crate::special::SpecialAttacks;

pub mod field_bonus;
pub mod lasting;

pub use field_bonus::FieldBonus;
pub use lasting::{BaseStats, Lasting};
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Status {
    Active,
    Dead,
    Fled,
    /// A hero Recall Hero took off the field to the summoning circle: unlike a fled unit it
    /// risks nothing.
    Recalled,
    /// A unit with Merging that sank into the ground: off the field, out of every attack and
    /// spell, until its side's next turn, when it comes up on a square of its choice
    /// (`BattleCommand::Surface`). A side with nothing but merged units has lost, and they get
    /// away whole (divergence 44).
    Merged,
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

/// The three results of Warp Creature, one of which it picks at random.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WarpKind {
    /// Half the melee and ranged attack.
    Attack,
    /// Half the defence.
    Defense,
    /// No resistance.
    Resistance,
}

/// One unit in a battle, with the statistics it fights with.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BattleUnit {
    pub id: usize,
    pub side: Side,
    /// The overland unit this was copied from (a list index, not an entity: see `game.rs`).
    pub source: Option<usize>,
    /// The unit definition (`None` for a plain unit made for a test); the client names it.
    pub unit_id: Option<UnitId>,
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
    /// Of the dice of the melee and of the ranged attack, the ones made of damage types (Flame
    /// Blade, Flaming): a defender immune to one of the types loses them.
    pub melee_parts: Vec<(i32, DamageTypes)>,
    pub ranged_parts: Vec<(i32, DamageTypes)>,
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
    /// Moves per turn and left this turn. Saved as half-moves, the form older saves have.
    #[serde(with = "f4x_def::half_int::raw")]
    pub moves: HalfInt,
    #[serde(with = "f4x_def::half_int::raw")]
    pub moves_left: HalfInt,
    /// Shots left. A spell caster with a magical ranged attack shoots with its mana instead
    /// (`rules.shot_mana` a shot): its shots are what its mana pays for.
    pub ammo: i32,
    /// The mana of a spell caster unit (20, or 40 for a major one) or of a hero with casting
    /// skill (`Game::hero_mana`).
    pub mana: i32,
    /// A hero with casting skill: its spell shots are paid for with its mana too.
    pub hero_caster: bool,
    /// A hero's own name, looked up when the battle starts and kept (empty for any other unit).
    #[serde(default)]
    pub hero_name: String,
    /// What the unit can cast on its own (a hero's spells and items, a caster's abilities).
    pub casting: UnitCasting,
    pub ranged_kind: RangedKind,
    /// What its abilities and other sources set on it for the battle: flight, first strike, a
    /// magical weapon (heroes, fantastic and summoned creatures, units with a flaming or blessed
    /// blade), ... (`UnitFlag`).
    pub flags: UnitFlags,
    /// A ship: it sails water and cannot leave it.
    pub sails: bool,
    /// A swimmer: it moves on land and water alike.
    pub swims: bool,
    /// The quality of its weapons (`unit::WeaponQuality`): above normal, its attacks have a
    /// plus to hit (its thrown and missile attacks too, but not breath, gazes or spell shots).
    pub weapon_quality: u8,
    /// `Stat::SpecialLevel`: poison strength, or the penalty of its gaze and touches.
    pub special_level: i32,
    /// A hero's Leadership and Prayermaster in this battle (`hero/mod.rs`), 0 for anything else: the
    /// best one on a side counts for all its units.
    pub leadership: i32,
    pub prayermaster: i32,
    /// The ids of its lasting spells, which add to resistance against some realms.
    pub enchantments: Vec<SpellId>,
    /// Held in Stasis when the battle began: the unit acts as if asleep and cannot get away.
    #[serde(default)]
    pub in_stasis: bool,
    /// Which of Warp Creature's three curses the unit has.
    #[serde(default)]
    pub warp: Option<WarpKind>,
    /// The creature's realm (chaos and death creatures fear dispel evil), and whether it is
    /// undead.
    pub realm: Option<Realm>,
    pub undead: bool,
    pub abilities: Vec<Ability>,
    /// Of those, the ones an item or a lasting spell gives: their touches are weaker and their
    /// doom damage is halved (`SpecialAttacks::of_unit`).
    #[serde(default)]
    pub granted: Vec<Ability>,
    pub status: Status,
    /// The damage it has taken, by kind: ordinary, drained, and the kind that destroys.
    pub damage_kinds: [i32; 3],
    /// Has attacked or shot this turn.
    pub attacked: bool,
    /// An invisible unit that gave itself away by attacking or casting: the other side sees it
    /// until its own next turn begins.
    pub revealed: bool,
    /// Merged on this turn of its side: it cannot come up before the next one.
    #[serde(default)]
    pub merged_this_turn: bool,
    /// Came up from the ground on this turn: it cannot merge again until the next one.
    #[serde(default)]
    pub surfaced: bool,
    /// Got away from a lost battle under the ground: it flees without the risk of a flight.
    #[serde(default)]
    pub slipped: bool,
    /// Whether it was invisible when the field was last refreshed, to tell when invisibility
    /// begins (it hides the unit at once).
    pub was_invisible: bool,
    /// Lasting spells cast on it in this battle, in the order cast, with who cast them and what
    /// they do to its numbers. They last until the battle ends or a dispel takes them off.
    pub lasting: Vec<Lasting>,
    /// What it started the battle with, kept from the first time something changes its numbers
    /// (a lasting spell, or a request of the battlefield spells): its numbers are those, the
    /// effect of the list and the field's request (`BattleUnit::recompute`).
    pub base: Option<BaseStats>,
    /// What the battlefield spells ask of its numbers (`field.rs`).
    pub field: FieldBonus,
    /// Confusion has put it under the enemy's orders for their next turn.
    pub seized: bool,
    /// While it is under the enemy's orders: the side it really belongs to.
    pub turned_from: Option<Side>,
    /// Creature Binding or Possession has put it on the other side for the rest of the battle:
    /// the side it really belongs to.
    pub bound_from: Option<Side>,
    /// The hit points the web that holds the unit has left (`rules.web_hits` when it is cast); at 0 the
    /// unit is free to act again, though the web's curse (no flight) stays until it is dispelled.
    #[serde(default)]
    pub web_hits: i32,
}

/// What a unit casts with its own mana rather than the wizard's skill.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UnitCasting {
    /// A hero's innate spells: they cost the unit's mana.
    pub innate: Vec<SpellId>,
    /// The spells of a hero's items, with the item (the entity it was copied from) and the
    /// charges left: they cost no mana.
    pub item_charges: Vec<ItemCharge>,
    /// The spells its abilities cast (`casts_healing`, `casts_fireball`, ...): once a battle
    /// each, for no mana.
    pub abilities: Vec<SpellId>,
    /// The penalty to the resistance of what its spells hit (its items' spell save).
    pub spell_save: i32,
    /// Demons it can still summon this battle (`summon_demon`: three for a Demon Lord).
    pub demons: u32,
}

/// A spell of a hero's item and the charges it has left in this battle.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ItemCharge {
    /// The item entity the charges are written back to when the battle ends. A save keeps the
    /// item's creation number only, and a load puts the entity back (`Battle::bind_items`).
    #[serde(skip, default = "dangling")]
    pub item: hecs::Entity,
    pub serial: Serial,
    pub spell: SpellId,
    pub charges: u32,
}

fn dangling() -> hecs::Entity {
    hecs::Entity::DANGLING
}

/// The moves a shot costs (see `BattleUnit::spend_ranged_moves`).
pub const RANGED_ATTACK_COST: HalfInt = HalfInt::from_int(10);

#[cfg(any(test, feature = "testutil"))]
impl BattleUnit {
    /// For tests: put a lasting spell on the unit, with what the shipped rules say it does.
    pub fn put_enchantment(&mut self, spell: SpellId) {
        let rules = &f4x_def::testdir::shipped().rules;
        self.enchantments.push(spell);
        let effects = f4x_def::rules::unit_effect::UnitEffects::of(rules.spells.get(spell));
        if let Some(base) = &mut self.base {
            base.enchantments.push(spell);
        }
        if effects.contains(f4x_def::rules::unit_effect::UnitEffect::Web) {
            self.web_closes(rules.battle.web_hits);
        }
    }
}

impl BattleUnit {
    /// A plain unit for tests and simple scenarios: one dice per melee point, no specials.
    pub fn simple(side: Side, figures: u32, hits: i32, melee: i32, defense: i32) -> Self {
        Self {
            id: 0,
            side,
            source: None,
            unit_id: None,
            cell: (0, 0),
            melee,
            ranged: 0,
            defense,
            resistance: 4,
            to_hit: 0,
            melee_plus: 0,
            ranged_plus: 0,
            melee_parts: Vec::new(),
            ranged_parts: Vec::new(),
            to_block: 0,
            hits,
            extra_hits: 0,
            figures_start: figures,
            figures,
            front_damage: 0,
            moves: OPEN_COST * 2,
            moves_left: OPEN_COST * 2,
            ammo: 0,
            mana: 0,
            hero_caster: false,
            hero_name: String::new(),
            casting: UnitCasting::default(),
            ranged_kind: RangedKind::None,
            flags: UnitFlags::NONE,
            sails: false,
            swims: false,
            weapon_quality: 0,
            special_level: 0,
            leadership: 0,
            prayermaster: 0,
            enchantments: Vec::new(),
            in_stasis: false,
            warp: None,
            realm: None,
            undead: false,
            abilities: Vec::new(),
            granted: Vec::new(),
            status: Status::Active,
            damage_kinds: [0; 3],
            attacked: false,
            revealed: false,
            merged_this_turn: false,
            surfaced: false,
            slipped: false,
            was_invisible: false,
            lasting: Vec::new(),
            base: None,
            field: FieldBonus::default(),
            seized: false,
            turned_from: None,
            bound_from: None,
            web_hits: 0,
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
    pub fn heal_kinds(&mut self, amount: i32) {
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
        let mut state = self.body_state();
        if overheal {
            state.heal_overflow(amount);
        } else {
            state.heal(amount);
        }
        self.set_from(&state);
        self.heal_kinds(amount);
        self.hit_points() - before
    }

    /// How the unit died, by the kind of damage that did most of it (ties go to the kind that
    /// destroys, then to drain). Meaningful once the unit is dead.
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
            flying: self.flag(UnitFlag::Flies),
            sails: self.sails,
            swims: self.swims,
            through_walls: self.flag(UnitFlag::Flies)
                || self.moves_instantly()
                || self.flag(UnitFlag::NonCorporeal),
            ignores_mud: self.flag(UnitFlag::NonCorporeal),
        }
    }

    /// Teleporting units go to any free square for one move. (Merging units do not: they sink
    /// into the ground and come up a turn later, `BattleCommand::Merge`.)
    pub fn moves_instantly(&self) -> bool {
        self.flag(UnitFlag::Teleports)
    }

    /// Whether the unit can sink into the ground now: it has Merging, half a move left, and did
    /// not come up this turn.
    pub fn can_merge(&self) -> bool {
        self.is_active()
            && self.has(Ability::new(43 /* merging */))
            && self.moves_left > 0
            && !self.surfaced
    }

    /// Whether the unit may attack now: it has movement left and has not used its turn up (a
    /// shot, a spell). Blows cost half its allowance and leave the rest to move or strike again
    /// (see ReMoM `Combat.c`, `Battle_Unit_Attack`; wiki article "Melee Attack").
    pub fn can_attack(&self) -> bool {
        self.is_active() && !self.attacked && self.moves_left > 0
    }

    /// A shot costs ten moves, or all that is left (ReMoM `Combat.c`, `Battle_Unit_Attack`: 20
    /// half-moves; wiki article "Ranged Attack"). Only a unit with a great allowance, a Hasted
    /// one or a hero with items, has moves left after it.
    pub fn spend_ranged_moves(&mut self) {
        self.moves_left = self.moves_left.saturating_sub(RANGED_ATTACK_COST);
    }

    /// The blows of a melee attack use up half the allowance (rounded up to a half), or all that
    /// is left when that is less.
    pub fn spend_melee_moves(&mut self) {
        self.moves_left = self.moves_left.saturating_sub(self.moves.half());
    }

    /// Whether Haste is on the unit: its attack is doubled (a second swing in the exchange, a
    /// second volley of a shot).
    pub fn is_hasted(&self) -> bool {
        self.has(Ability::new(26 /* hasted */))
    }

    pub fn is_active(&self) -> bool {
        self.status == Status::Active
    }

    /// Hit points left over all figures.
    pub fn hit_points(&self) -> i32 {
        self.body_state().hit_points()
    }

    pub fn max_hit_points(&self) -> i32 {
        self.figures_start as i32 * (self.hits - self.extra_hits)
    }

    /// The unit as a defender, with the defence rules of its abilities.
    pub fn defender_state(&self, rules: &Rules) -> DefenderState {
        DefenderState {
            immunities: Immunities::of(&self.abilities, &self.enchantments, rules),
            effects: self.effects(rules),
            ..self.body_state()
        }
    }

    /// The unit's body without its defence rules: all that hit points and healing need.
    pub fn body_state(&self) -> DefenderState {
        DefenderState {
            hits: self.hits,
            extra_hits: self.extra_hits,
            defense: self.defense,
            immunities: Immunities::default(),
            resistance: self.resistance,
            enchantments: self.enchantments.clone(),
            effects: Default::default(),
            realm: self.realm,
            undead: self.undead,
            figures_max: self.figures_start,
            to_block: self.to_block,
            figures: self.figures,
            front_damage: self.front_damage,
            lucky: self.has(Ability::new(39 /* lucky */)),
        }
    }

    pub fn set_from(&mut self, state: &DefenderState) {
        self.hits = state.hits;
        self.extra_hits = state.extra_hits;
        self.figures = state.figures;
        self.front_damage = state.front_damage;
        if self.figures == 0 {
            self.status = Status::Dead;
        }
    }

    /// Whether the unit has the battle flag (`UnitFlag`).
    pub fn flag(&self, flag: UnitFlag) -> bool {
        self.flags.contains(flag)
    }

    pub fn has(&self, ability: Ability) -> bool {
        self.abilities.contains(&ability)
    }

    /// Whether the unit can found a city.
    pub fn is_settler(&self) -> bool {
        self.has(Ability::new(12 /* create_outpost */))
    }

    /// What its lasting spells do (the `unit_effect` hook), and Stasis.
    pub fn effects(&self, rules: &Rules) -> f4x_def::rules::unit_effect::UnitEffects {
        let mut set = UnitEffects::NONE;
        for &spell in &self.enchantments {
            set.union(UnitEffects::of(rules.spells.get(spell)));
        }
        if self.in_stasis {
            set.insert(UnitEffect::BlackSleep);
            set.insert(UnitEffect::Holds);
        }
        set
    }

    /// Whether a web still holds the unit: it cannot move or attack until the web is broken.
    pub fn held_by_web(&self, rules: &Rules) -> bool {
        self.web_hits > 0
            && self
                .effects(rules)
                .contains(f4x_def::rules::unit_effect::UnitEffect::Web)
    }

    /// The hit points of the web a unit takes off it in a turn (no attack roll, full damage; the
    /// web covers each figure, so one figure's strength counts). Its melee strength, and:
    /// - a bow, a sling or a magical ranged attack with a shot left: the better of that and
    ///   melee (boulders do not count, they are the same bodily strength as melee);
    /// - a thrown weapon, a breath or a gaze: its strength on top of melee;
    /// - Immolation: its fire on top.
    pub fn web_strength(&self, rules: &Rules) -> i32 {
        let mut strength = self.melee.max(0);
        let shoots = self.ammo > 0
            && (self.ranged_kind == RangedKind::Magic
                || self
                    .weapon_attack(rules)
                    .is_some_and(|a| a.kind == RangedKind::Missile && !a.bodily));
        if shoots {
            strength = strength.max(self.ranged);
        } else if self.ranged_kind == RangedKind::Short {
            strength += self.ranged.max(0);
        }
        if self.has(Ability::new(32 /* immolation */)) {
            strength += rules.battle.immolation_strength;
        }
        strength
    }

    /// Whether the unit regenerates: the ability, or the Regeneration spell, which grants it.
    pub fn regenerates(&self) -> bool {
        self.has(Ability::new(55 /* regeneration */))
    }

    /// Whether the unit's spell shots are paid for with its mana: a spell caster with a magical
    /// ranged attack.
    pub fn shoots_with_mana(&self, rules: &Rules) -> bool {
        self.ranged_kind == RangedKind::Magic && (self.hero_caster || self.is_caster(rules))
    }

    /// The kind of the unit's weapon shot, if it has one (`crate::weapon::weapon_shot_of`).
    pub fn weapon_shot(&self, rules: &Rules) -> Option<RangedKind> {
        crate::weapon::weapon_shot_of(&self.abilities, rules)
    }

    /// The attack of the unit's weapon shot, if it has one (`crate::weapon::weapon_attack_of`).
    pub fn weapon_attack<'a>(
        &self,
        rules: &'a Rules,
    ) -> Option<&'a f4x_def::rules::damage_type::AbilityAttack> {
        crate::weapon::weapon_attack_of(&self.abilities, rules)
    }

    /// Whether an ability makes the unit a spell caster (not a hero with casting skill).
    pub fn is_caster(&self, rules: &Rules) -> bool {
        crate::weapon::caster_mana_of(&self.abilities, rules) > 0
    }

    /// Use up one shot (and the mana it costs a caster).
    pub fn spend_shot(&mut self, shot_mana: i32, rules: &Rules) {
        self.ammo -= 1;
        if self.shoots_with_mana(rules) {
            self.mana = (self.mana - shot_mana).max(0);
            self.ammo = self.ammo.min(self.mana / shot_mana);
        }
    }

    /// Power Drain: the unit loses up to `amount` mana (a spell shooter's shots follow its mana).
    /// Returns what it lost.
    pub fn drain_power(&mut self, amount: i32, shot_mana: i32, rules: &Rules) -> i32 {
        let lost = amount.min(self.mana).max(0);
        self.mana -= lost;
        if self.shoots_with_mana(rules) {
            self.ammo = self.ammo.min(self.mana / shot_mana);
        }
        lost
    }

    /// Mana Leak: a caster loses `drain` mana; any unit with spell shots loses
    /// one (a caster's shots follow its mana).
    pub fn leak_mana(&mut self, drain: i32, shot_mana: i32, rules: &Rules) {
        self.mana = (self.mana - drain).max(0);
        if self.ranged_kind == RangedKind::Magic {
            if self.shoots_with_mana(rules) {
                self.ammo = self.ammo.min(self.mana / shot_mana);
            } else {
                self.ammo = (self.ammo - 1).max(0);
            }
        }
    }

    pub fn special_attacks(&self) -> SpecialAttacks {
        SpecialAttacks::of_unit(&self.abilities, &self.granted, self.special_level)
    }

    /// A combatant for one of the unit's attacks.
    pub fn combatant(
        &self,
        mode: Mode,
        strength: i32,
        to_hit: i32,
        rules: &Rules,
        magic_exceptions: bool,
    ) -> Combatant {
        let special = self.special_attacks();
        // Doom damage that an item or a spell gives is half as strong.
        let strength = if self.granted.contains(&Ability::new(101 /* doom_damage */)) {
            strength / 2
        } else {
            strength
        };
        Combatant {
            strength,
            to_hit,
            mode,
            parts: if mode == Mode::Melee {
                self.melee_parts.clone()
            } else {
                self.ranged_parts.clone()
            },
            nature: AttackNature::of_unit_attack(
                mode,
                &self.abilities,
                self.flag(UnitFlag::MagicalWeapon),
                magic_exceptions,
                rules,
            ),
            special,
            eldritch: self.has(Ability::new(19 /* eldritch_weapon */))
                && (mode == Mode::Melee || self.weapon_shot(rules).is_some()),
            immolation: self.has(Ability::new(32 /* immolation */)),
            fear: self.has(Ability::new(8 /* cause_fear */)),
        }
    }

    /// The one of these damage types the unit has a defence rule against, for the log.
    pub fn immune_type(&self, types: &[DamageType], rules: &Rules) -> Option<DamageType> {
        Immunities::of(&self.abilities, &self.enchantments, rules).met_by(DamageTypes::of(types))
    }

    /// Whether the unit has a defence rule against one of these damage types: what a spell that
    /// names types in its data is stopped by (illusion immunity too, which sees through an
    /// illusion attack and refuses a spell of illusions).
    pub fn is_immune_to(&self, types: &[DamageType], rules: &Rules) -> bool {
        Immunities::of(&self.abilities, &self.enchantments, rules).meets(DamageTypes::of(types))
    }

    /// Whether the unit has a defence rule against one of these damage types (a set, as a spell's
    /// `harm_types` holds it).
    pub fn is_immune_to_set(&self, types: DamageTypes, rules: &Rules) -> bool {
        Immunities::of(&self.abilities, &self.enchantments, rules).meets(types)
    }

    pub fn melee_combatant(&self, rules: &Rules, magic_exceptions: bool) -> Combatant {
        self.combatant(
            Mode::Melee,
            self.melee,
            self.to_hit + self.weapon_plus() + self.melee_plus,
            rules,
            magic_exceptions,
        )
    }

    /// The plus to hit the unit's weapons give.
    pub fn weapon_plus(&self) -> i32 {
        i32::from(self.weapon_quality > 0 || self.has(Ability::new(19 /* eldritch_weapon */)))
    }

    pub fn thrown_combatant(&self, rules: &Rules, magic_exceptions: bool) -> Option<Combatant> {
        // A gaze needs no attack strength (1.50 note 153): a unit weakened to none still gazes.
        let gazes = self.special_attacks().has_gaze();
        (self.ranged_kind == RangedKind::Short && (self.ranged > 0 || gazes)).then(|| {
            // Only a thrown weapon gains the plus, not breath or a gaze.
            let plus = if self.weapon_shot(rules) == Some(RangedKind::Short) {
                self.weapon_plus()
            } else {
                0
            };
            self.combatant(
                Mode::ThrownOrBreath,
                self.ranged,
                self.to_hit + plus + self.ranged_plus,
                rules,
                magic_exceptions,
            )
        })
    }
}
