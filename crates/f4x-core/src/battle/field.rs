//! Battlefield enchantments: spells that act on the whole field, each cast by one side.
//!
//! They follow the original's `Battle_Unit_Regular_Stats` (the stat changes), `Combat_Next_Turn`
//! (what happens at the start of each turn) and `Battle_Unit_Process_Attack` (Blur).
//!
//! - **Stat changes** are kept as a [`FieldBonus`] on each unit and recomputed whenever a spell
//!   goes up or a unit changes (`Battle::refresh_fields`), so they never pile up: True Light and
//!   Darkness (on both sides' creatures of life and death, and cancelling each other), Warp
//!   Reality (every unit but chaos creatures), Black Prayer (the caster's enemies), Prayer and
//!   High Prayer (the caster's units), Metal Fires (the caster's non-fantastic units without a
//!   flame blade), Mass Invisibility and Blur (the caster's units).
//! - **At the start of each turn** (the attacker's): Mana Leak, then Call Lightning, then Wrack,
//!   for the sides that cast them.
//! - **At the start of each side's turn**: Terror (the caster's enemies must resist or cannot
//!   act) and Entangle (the caster's enemies that walk and have a body lose a move).

use rand::{Rng, RngCore};

use super::attack::{unblockable, Damage};
use super::immunity::{AttackNature, Immunities, Realm};
use super::map::{self, Pos};
use super::{Battle, BattleEvent, BattleUnit, Confusion, RangedKind, Side};
use crate::rules::effects::DamageTraits;
use crate::rules::SpellId;

/// Hooks the spell data names.
pub const BATTLEFIELD: &str = "battlefield";
pub const CALL_CHAOS: &str = "call_chaos";
pub const DISINTEGRATE: &str = "disintegrate";

/// Mana Leak drains this much from each enemy wizard each turn.
pub const MANA_LEAK_DRAIN: i32 = 5;
/// Call Lightning's bolts: strength, and how many (three to five) strike each turn.
pub const CALL_LIGHTNING_STRENGTH: i32 = 8;

/// The battlefield enchantments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    TrueLight,
    Darkness,
    WarpReality,
    BlackPrayer,
    Wrack,
    MetalFires,
    Prayer,
    HighPrayer,
    Terror,
    CallLightning,
    MassInvisibility,
    Entangle,
    ManaLeak,
    Blur,
}

impl Field {
    /// The enchantment a spell (by its id) puts up.
    pub fn of(spell_id: &str) -> Option<Field> {
        Some(match spell_id {
            "true_light" => Field::TrueLight,
            "darkness" => Field::Darkness,
            "warp_reality" => Field::WarpReality,
            "black_prayer" => Field::BlackPrayer,
            "wrack" => Field::Wrack,
            "metal_fires" => Field::MetalFires,
            "prayer" => Field::Prayer,
            "high_prayer" => Field::HighPrayer,
            "terror" => Field::Terror,
            "call_lightning" => Field::CallLightning,
            "mass_invisibility" => Field::MassInvisibility,
            "entangle" => Field::Entangle,
            "mana_leak" => Field::ManaLeak,
            "blur" => Field::Blur,
            _ => return None,
        })
    }
}

/// What the battlefield spells have changed about a unit's numbers; kept so it can be taken off
/// again (`BattleUnit::field`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FieldBonus {
    pub melee: i32,
    pub ranged: i32,
    pub defense: i32,
    pub resistance: i32,
    pub to_hit: i32,
    pub to_block: i32,
    /// Metal Fires made its weapons magical (it had none that were).
    pub magical: bool,
    /// Mass Invisibility makes it invisible.
    pub invisible: bool,
    /// Blur makes it hard to hit.
    pub blur: bool,
}

impl BattleUnit {
    /// Take the battlefield spells' changes off the unit's numbers.
    pub(super) fn take_off_field(&mut self) {
        let old = std::mem::take(&mut self.field);
        self.melee -= old.melee;
        self.ranged -= old.ranged;
        self.defense -= old.defense;
        self.resistance -= old.resistance;
        self.to_hit -= old.to_hit;
        self.to_block -= old.to_block;
        if old.magical {
            self.magical_weapon = false;
        }
    }

    fn put_on_field(&mut self, wanted: FieldBonus) {
        // A stat never goes below nothing, so a loss may be less than asked.
        let melee = wanted.melee.max(-self.melee);
        let ranged = wanted.ranged.max(-self.ranged);
        let defense = wanted.defense.max(-self.defense);
        let resistance = wanted.resistance.max(-self.resistance);
        self.melee += melee;
        self.ranged += ranged;
        self.defense += defense;
        self.resistance += resistance;
        self.to_hit += wanted.to_hit;
        self.to_block += wanted.to_block;
        let magical = wanted.magical && !self.magical_weapon;
        if magical {
            self.magical_weapon = true;
        }
        self.field = FieldBonus {
            melee,
            ranged,
            defense,
            resistance,
            magical,
            ..wanted
        };
    }

    /// Whether the creature belongs to a realm (a summoned or fantastic creature, not one of a
    /// race's units).
    pub fn is_fantastic(&self) -> bool {
        self.realm.is_some()
    }

    /// Invisible: attackers without immunity to illusions have a penalty to hit it.
    pub fn is_invisible(&self) -> bool {
        self.has("invisibility") || self.field.invisible
    }

    pub(super) fn has_enchantment(&self, id: &str) -> bool {
        self.enchantments.iter().any(|e| e == id)
    }
}

impl Battle {
    /// Whether `side` has put up the enchantment.
    pub fn has_field(&self, side: Side, field: Field) -> bool {
        self.fields[side.index()].iter().any(|&(f, _)| f == field)
    }

    /// Whether either side has put it up.
    pub fn has_field_either(&self, field: Field) -> bool {
        self.has_field(Side::Attacker, field) || self.has_field(Side::Defender, field)
    }

    /// The enchantments on the field, with who cast each and the spell that did.
    pub fn fields(&self) -> impl Iterator<Item = (Side, Field, SpellId)> + '_ {
        [Side::Attacker, Side::Defender]
            .into_iter()
            .flat_map(|side| {
                self.fields[side.index()]
                    .iter()
                    .map(move |&(f, s)| (side, f, s))
            })
    }

    /// Put an enchantment up for `side`; false if it already has it.
    pub fn raise_field(&mut self, side: Side, field: Field, spell: SpellId) -> bool {
        if self.has_field(side, field) {
            return false;
        }
        self.fields[side.index()].push((field, spell));
        self.refresh_fields();
        true
    }

    /// What the battlefield spells do to this unit's numbers now.
    fn field_bonus_for(&self, u: &BattleUnit) -> FieldBonus {
        let mine = u.side;
        let own = |f| self.has_field(mine, f);
        let theirs = |f| self.has_field(mine.other(), f);
        let mut b = FieldBonus::default();
        // Light and darkness: life and death creatures gain and lose by turns.
        let light = i32::from(self.has_field_either(Field::TrueLight));
        let dark = i32::from(self.has_field_either(Field::Darkness));
        let swing = match u.realm {
            Some(Realm::Life) => light - dark,
            Some(Realm::Death) => dark - light,
            _ => 0,
        };
        for _ in 0..swing.abs() {
            let step = swing.signum();
            // A gain only helps an attack the creature has; a loss is felt anyway.
            b.melee += if step > 0 && u.melee <= 0 { 0 } else { step };
            b.ranged += if step > 0 && u.ranged <= 0 { 0 } else { step };
            b.defense += step;
            b.resistance += step;
        }
        if self.has_field_either(Field::WarpReality) && u.realm != Some(Realm::Chaos) {
            b.to_hit -= 2;
        }
        if theirs(Field::BlackPrayer) {
            b.melee -= 1;
            b.ranged -= 1;
            b.defense -= 1;
            b.resistance -= 2;
        }
        if own(Field::HighPrayer) {
            b.to_hit += 1;
            b.to_block += 1;
            b.resistance += 3;
            if u.melee > 0 {
                b.melee += 2;
            }
            b.defense += 2;
        } else if own(Field::Prayer) {
            b.to_hit += 1;
            b.to_block += 1;
            b.resistance += 1;
        }
        if own(Field::MetalFires) && !u.is_fantastic() && !u.has_enchantment("flame_blade") {
            if u.melee > 0 {
                b.melee += 1;
            }
            if u.ranged_kind == RangedKind::Missile || u.has("thrown") {
                b.ranged += 1;
            }
            b.magical = true;
        }
        // Holy Bonus and Resistance to All: the highest level among the side's standing units,
        // never added up. Holy Bonus gives attack (melee), defence and resistance, and counts
        // towards resistance to all (`Update_Battlefield_Bonuses`, `COMBINIT.c`).
        let level = |ability: &str| {
            self.units
                .iter()
                .filter(|x| x.side == mine && x.is_active() && x.has(ability))
                .map(|x| x.special_level)
                .max()
                .unwrap_or(0)
        };
        let holy = level("holy_bonus");
        let best = |f: fn(&BattleUnit) -> i32| {
            self.units
                .iter()
                .filter(|x| x.side == mine && x.is_active())
                .map(f)
                .max()
                .unwrap_or(0)
        };
        // A hero's Prayermaster counts as Resistance to All; the better of the two applies.
        let resist_all = level("resist_all").max(best(|x| x.prayermaster));
        if holy > 0 && u.melee > 0 {
            b.melee += holy;
        }
        // Leadership: the best hero's bonus to the melee (and half of it to the ranged, but not
        // magic) attack of every normal unit, not the fantastic ones or the undead.
        let lead = best(|x| x.leadership);
        if lead > 0 && !u.is_fantastic() && !u.undead {
            if u.melee > 0 {
                b.melee += lead;
            }
            if u.ranged > 0 && u.ranged_kind != RangedKind::Magic {
                b.ranged += lead / 2;
            }
        }
        b.defense += holy;
        b.resistance += holy + resist_all;
        b.invisible = own(Field::MassInvisibility);
        b.blur = own(Field::Blur);
        b
    }

    /// Bring every unit's numbers in line with the enchantments on the field.
    pub fn refresh_fields(&mut self) {
        for i in 0..self.units.len() {
            self.units[i].take_off_field();
            let wanted = self.field_bonus_for(&self.units[i]);
            self.units[i].put_on_field(wanted);
        }
    }

    /// What happens when a new turn begins (the attacker's start): Mana Leak, Call Lightning and
    /// Wrack, for each side that has put them up.
    pub(super) fn turn_effects(&mut self) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        for caster in [Side::Attacker, Side::Defender] {
            if self.has_field(caster, Field::ManaLeak) {
                events.extend(self.mana_leak(caster.other()));
            }
        }
        for caster in [Side::Attacker, Side::Defender] {
            if self.has_field(caster, Field::CallLightning) {
                events.extend(self.call_lightning(caster));
            }
        }
        for caster in [Side::Attacker, Side::Defender] {
            if self.has_field(caster, Field::Wrack) {
                events.extend(self.wrack(caster));
            }
        }
        events.extend(self.vortex_round());
        events
    }

    /// The side's wizard loses mana and its units with spell shots lose a shot.
    fn mana_leak(&mut self, victim: Side) -> Vec<BattleEvent> {
        if let Some(c) = self.casters[victim.index()].as_mut() {
            c.mana = (c.mana - MANA_LEAK_DRAIN).max(0);
        }
        for u in self
            .units
            .iter_mut()
            .filter(|u| u.side == victim && u.is_active())
        {
            u.leak_mana(MANA_LEAK_DRAIN);
        }
        vec![BattleEvent::ManaLeaked { side: victim }]
    }

    /// Three to five bolts of lightning strike random enemy units (each try has an even chance
    /// of using the unit it lands on).
    fn call_lightning(&mut self, caster: Side) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        let mut bolts = self.rng.gen_range(3..=5);
        let mut tries = 0;
        let traits = DamageTraits {
            armor_piercing: true,
            ..Default::default()
        };
        let nature = AttackNature {
            stopped_by: Immunities {
                magic: true,
                ..Default::default()
            },
            realm: Some(Realm::Nature),
            armor_piercing: false,
        };
        while bolts > 0 && tries < 30 && !self.units.is_empty() {
            tries += 1;
            let id = self.rng.gen_range(0..self.units.len());
            let u = &self.units[id];
            if u.side == caster
                || !u.is_active()
                || u.has_enchantment("wraith_form")
                || self.rng.gen_range(1..=2) != 1
            {
                continue;
            }
            let dmg = self.spell_damage(id, CALL_LIGHTNING_STRENGTH, &traits, nature);
            events.extend(self.spell_events_for_damage(id, dmg));
            bolts -= 1;
            tries = 0;
        }
        events
    }

    /// Every figure of every enemy unit without death immunity must resist at +1 or lose a hit
    /// point.
    fn wrack(&mut self, caster: Side) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        for id in 0..self.units.len() {
            let u = &self.units[id];
            if u.side == caster
                || !u.is_active()
                || u.has("death_immunity")
                || u.has_enchantment("wraith_form")
            {
                continue;
            }
            let mut state = u.defender_state();
            let rng: &mut dyn RngCore = &mut self.rng;
            let mut lost = Damage::default();
            for _ in 0..state.figures {
                if state.resistance_failure(Some(Realm::Death), 1, rng) > 0 {
                    lost += unblockable(&mut state, 1);
                }
            }
            if lost.points > 0 {
                self.units[id].set_from(&state);
                self.units[id].record(lost);
                events.push(BattleEvent::FieldStruck {
                    unit: id,
                    dealt: lost.points,
                    kills: lost.kills,
                });
                if !self.units[id].is_active() {
                    events.push(BattleEvent::UnitDied { unit: id });
                }
            }
        }
        events
    }

    /// What the side's enchantments from the other side do as its turn begins: Terror and
    /// Entangle.
    pub(super) fn side_turn_effects(&mut self, side: Side) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        let terror = self.has_field(side.other(), Field::Terror);
        let entangle = self.has_field(side.other(), Field::Entangle);
        for id in 0..self.units.len() {
            if self.units[id].side != side || !self.units[id].is_active() {
                continue;
            }
            if terror {
                let state = self.units[id].defender_state();
                let rng: &mut dyn RngCore = &mut self.rng;
                if state.resistance_failure(Some(Realm::Death), 1, rng) > 0 {
                    let u = &mut self.units[id];
                    u.moves_left = 0;
                    u.attacked = true;
                    events.push(BattleEvent::Terrified { unit: id });
                }
            }
            // Entangle holds only walkers with a body: the help says so, and the original's code,
            // which takes the move from every unit, is taken to be a bug.
            if entangle && !self.units[id].flies && !self.units[id].has("non_corporeal") {
                let u = &mut self.units[id];
                u.moves_left = u.moves_left.saturating_sub(2);
            }
            events.extend(self.regenerate(id));
            if self.units[id]
                .enchantments
                .iter()
                .any(|e| e == "black_sleep")
            {
                self.units[id].attacked = true;
            }
            events.extend(self.confusion(id));
        }
        // The enemy's units that confusion has put in this side's hands (they go back as the
        // next turn begins).
        for u in &mut self.units {
            if u.seized && u.side == side.other() && u.is_active() {
                u.seized = false;
                u.turned_from = Some(u.side);
                u.side = side;
                u.moves_left = u.moves;
                u.attacked = false;
            }
        }
        events
    }

    /// A confused unit does one of four things each turn, each as likely: nothing, a move to a
    /// random free square, the enemy's orders, or its own business as usual.
    fn confusion(&mut self, id: usize) -> Option<BattleEvent> {
        if !self.units[id].enchantments.iter().any(|e| e == "confusion") {
            return None;
        }
        let act = match self.rng.gen_range(0..4) {
            0 => Confusion::Idle,
            1 => Confusion::Wandered,
            2 => Confusion::Seized,
            _ => return None,
        };
        if act == Confusion::Wandered {
            let free: Vec<Pos> = (0..map::GRID_H)
                .flat_map(|y| (0..map::GRID_W).map(move |x| (x, y)))
                .filter(|&p| {
                    self.map.enter_cost(p, false).is_some()
                        && !(map::is_wall_corner(p) && self.map.has_wall(p))
                        && self.unit_at(p).is_none()
                })
                .collect();
            if !free.is_empty() {
                let to = free[self.rng.gen_range(0..free.len())];
                self.units[id].cell = to;
            }
        }
        let u = &mut self.units[id];
        u.moves_left = 0;
        u.attacked = true;
        u.seized = act == Confusion::Seized;
        Some(BattleEvent::Confused { unit: id, act })
    }

    /// A regenerating unit (the ability, or the Regeneration spell) heals one hit as its side's
    /// turn begins.
    fn regenerate(&mut self, id: usize) -> Option<BattleEvent> {
        let u = &mut self.units[id];
        if !u.regenerates() {
            return None;
        }
        let healed = u.heal_points(1, false);
        (healed > 0).then_some(BattleEvent::SpellHealed { unit: id, healed })
    }
}
