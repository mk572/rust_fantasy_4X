//! Dispelling: the one formula, the spells that use it, and Spell Blast.
//!
//! A dispel of strength `s` against a spell of difficulty `d` works with chance `s / (s + d)`
//! (the original rolls 1 to 250 against `250 s / (s + d)`). A spell's difficulty is its cost,
//! doubled again for each of its caster's masteries that applies: the mastery of the spell's
//! realm (nature, sorcery, chaos) and Archmage each add one more times the cost. A spell's
//! dispel strength is the mana put into it: overland always its base cost, in a battle whatever the
//! wizard puts in (`battle/spells.rs`); the "True" spells count every point as three.
//!
//! Users: Dispel Magic (units), Disenchant Area and True (a map square), Disjunction and True
//! (an enemy wizard's global enchantment), Spell Binding (the same, but the caster takes the
//! spell), Suppress Magic (a standing strength-500 dispel against enemy overland spells),
//! Counter Magic (a battlefield reserve, `battle::Battle::counter_magic_stops`) and
//! nightshade (`city/nightshade.rs`). Spell Blast cancels the spell another wizard is casting.

use rand::RngCore;

use crate::city::economy::Enchantments;
use crate::game::{Event, Game};
use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::exec::Target;
use crate::rules::SpellId;
use crate::unit::UnitEnchantments;
use crate::wizard::magic::Retort;

/// Enchantments with the wizard who cast each.
pub(crate) type Spells = Vec<(SpellId, PlayerId)>;

/// How hard Spell Lock is to dispel, whatever its cost.
pub const SPELL_LOCK_DIFFICULTY: u32 = 150;
/// The strength of Suppress Magic's standing dispel.
pub const SUPPRESS_MAGIC_STRENGTH: u32 = 500;
/// What each enemy spell takes from a Counter Magic reserve.
pub const COUNTER_MAGIC_DRAIN: u32 = 5;
/// Hooks the spell data names.
pub const DISJUNCTION: &str = "disjunction";
pub const DISJUNCTION_TRUE: &str = "disjunction_true";
pub const SPELL_BINDING: &str = "spell_binding";
pub const SPELL_BLAST: &str = "spell_blast";
pub const COUNTER_MAGIC: &str = "counter_magic";

/// Whether a dispel of `strength` beats something of `difficulty`: a roll of 1 to 250 at or
/// under `250 * strength / (strength + difficulty)`.
pub fn dispel_roll(strength: u32, difficulty: u32, rng: &mut dyn RngCore) -> bool {
    if strength == 0 {
        return false;
    }
    let chance = 250 * u64::from(strength) / u64::from(strength + difficulty);
    rng.next_u64() % 250 < chance
}

impl Game {
    /// A wizard turns off one of their own spells on a city (the city screen's list of
    /// enchantments): it ends, and so does its upkeep.
    pub fn turn_off_city_spell(
        &mut self,
        player: PlayerId,
        city: hecs::Entity,
        spell: SpellId,
    ) -> Result<Vec<Event>, crate::wizard::magic::MagicError> {
        use crate::wizard::magic::MagicError::NoSuchTarget;
        let list = self
            .world
            .get::<&Enchantments>(city)
            .map(|e| e.0.clone())
            .map_err(|_| NoSuchTarget)?;
        if !list.contains(&(spell, player)) {
            return Err(NoSuchTarget);
        }
        let kept = list.into_iter().filter(|&e| e != (spell, player)).collect();
        let _ = self.world.insert_one(city, Enchantments(kept));
        if self.content.rules.spells.get(spell).id == "spell_ward" {
            let _ = self
                .world
                .remove_one::<crate::spell::city::SpellWards>(city);
        }
        Ok(vec![Event::EnchantmentLapsed { player, spell }])
    }

    /// How many land units would drown if `spell` were turned off on `unit`: the question the
    /// sheet asks before it warns the wizard. Nothing is changed.
    pub fn drowned_if_off(&mut self, unit: hecs::Entity, spell: SpellId) -> usize {
        let Ok(list) = self
            .world
            .get::<&UnitEnchantments>(unit)
            .map(|e| e.0.clone())
        else {
            return 0;
        };
        let kept = list.iter().copied().filter(|&(s, _)| s != spell).collect();
        let _ = self.world.insert_one(unit, UnitEnchantments(kept));
        let lost = self.economy().stranded().len();
        let _ = self.world.insert_one(unit, UnitEnchantments(list));
        lost
    }

    /// Turn off one of the wizard's own spells on a unit (`UV_Remove_Unit_Enchantment`). The
    /// unit may be left at sea without what kept it afloat; it drowns at once, and with it
    /// any other land unit of the square that no longer has what it needs.
    pub fn turn_off_unit_spell(
        &mut self,
        player: PlayerId,
        unit: hecs::Entity,
        spell: SpellId,
    ) -> Result<Vec<Event>, crate::wizard::magic::MagicError> {
        use crate::wizard::magic::MagicError::NoSuchTarget;
        let list = self
            .world
            .get::<&UnitEnchantments>(unit)
            .map(|e| e.0.clone())
            .map_err(|_| NoSuchTarget)?;
        if !list.contains(&(spell, player)) {
            return Err(NoSuchTarget);
        }
        let kept = list.into_iter().filter(|&e| e != (spell, player)).collect();
        let _ = self.world.insert_one(unit, UnitEnchantments(kept));
        let mut events = vec![Event::EnchantmentLapsed { player, spell }];
        events.extend(self.drown_stranded());
        Ok(events)
    }

    /// How many times its cost a spell cast by `caster` resists: one, plus one for the
    /// caster's mastery of its realm and one for Archmage.
    pub fn dispel_multiplier(&self, realm: &str, caster: Option<PlayerId>) -> u32 {
        let Some(m) = caster.and_then(|c| self.magic(c)) else {
            return 1;
        };
        let mastery = match realm {
            "nature" => m.has(Retort::NatureMastery),
            "sorcery" => m.has(Retort::SorceryMastery),
            "chaos" => m.has(Retort::ChaosMastery),
            _ => false,
        };
        1 + u32::from(mastery) + u32::from(m.has(Retort::Archmage))
    }

    /// The difficulty of dispelling `spell` cast by `caster` (`None` for no caster's retorts).
    pub fn dispel_difficulty(&self, spell: SpellId, caster: Option<PlayerId>) -> u32 {
        let def = self.content.rules.spells.get(spell);
        let base = if def.dispel_difficulty > 0 {
            def.dispel_difficulty
        } else {
            def.cost
        };
        base * self.dispel_multiplier(&def.realm, caster)
    }

    /// Try each enchantment in `list` cast by someone other than `by`; returns what is left
    /// and what was removed. A unit under another wizard's Spell Lock can only have the
    /// lock attacked, at difficulty 150.
    pub(crate) fn dispel_list(
        &self,
        list: Vec<(SpellId, PlayerId)>,
        strength: u32,
        by: PlayerId,
        guarded_by_lock: bool,
        rng: &mut dyn RngCore,
    ) -> (Spells, Spells) {
        let lock = self.content.rules.spells.id_of("spell_lock");
        if guarded_by_lock {
            if let Some(&(_, caster)) = list.iter().find(|&&(s, c)| Some(s) == lock && c != by) {
                let difficulty =
                    SPELL_LOCK_DIFFICULTY * self.dispel_multiplier("sorcery", Some(caster));
                if dispel_roll(strength, difficulty, rng) {
                    let (gone, kept) = list.into_iter().partition(|&(s, _)| Some(s) == lock);
                    return (kept, gone);
                }
                return (list, Vec::new());
            }
        }
        let mut kept = Vec::new();
        let mut removed = Vec::new();
        for (spell, caster) in list {
            let difficulty = self.dispel_difficulty(spell, Some(caster));
            if caster != by && dispel_roll(strength, difficulty, rng) {
                removed.push((spell, caster));
            } else {
                kept.push((spell, caster));
            }
        }
        (kept, removed)
    }

    /// A dispel of `strength` by wizard `by` on a unit, a city or everything on a map square
    /// (the city, the units, a warped node). Only enchantments cast by other wizards are
    /// attacked. Returns how many were removed, and notes each spell that went in `gone`.
    pub(crate) fn dispel_target(
        &mut self,
        who: Target,
        strength: u32,
        by: PlayerId,
        rng: &mut dyn RngCore,
        gone: &mut Vec<Event>,
    ) -> u32 {
        match who {
            Target::Unit(u) => {
                let list = self
                    .world
                    .get::<&UnitEnchantments>(u)
                    .map(|e| e.0.clone())
                    .unwrap_or_default();
                let (kept, removed) = self.dispel_list(list, strength, by, true, rng);
                let _ = self.world.insert_one(u, UnitEnchantments(kept));
                gone.extend(
                    removed
                        .iter()
                        .map(|&(spell, victim)| Event::EnchantmentDispelled {
                            victim,
                            spell,
                            on_unit: true,
                        }),
                );
                removed.len() as u32
            }
            Target::City(c) => {
                let list = self
                    .world
                    .get::<&Enchantments>(c)
                    .map(|e| e.0.clone())
                    .unwrap_or_default();
                let (kept, removed) = self.dispel_list(list, strength, by, false, rng);
                let _ = self.world.insert_one(c, Enchantments(kept));
                gone.extend(
                    removed
                        .iter()
                        .map(|&(spell, victim)| Event::EnchantmentDispelled {
                            victim,
                            spell,
                            on_unit: false,
                        }),
                );
                removed.len() as u32
            }
            Target::Tile(at) => {
                let mut removed = self.dispel_node(at, strength, rng);
                removed += self.dispel_road(at, strength, rng);
                if let Some(c) = self.economy().city_at(at) {
                    removed += self.dispel_target(Target::City(c), strength, by, rng, gone);
                }
                for u in self.units().units_at(at) {
                    removed += self.dispel_target(Target::Unit(u), strength, by, rng, gone);
                }
                removed
            }
            _ => 0,
        }
    }

    /// Dispel an enchanted road on the square, against the difficulty of Enchant Road. The
    /// road keeps no caster, so any dispel may attack it (the original's cannot be dispelled
    /// at all; divergence 27). Myrror's roads are magic by the plane and stay so.
    fn dispel_road(&mut self, at: Position, strength: u32, rng: &mut dyn RngCore) -> u32 {
        let enchanted = self.map.as_ref().is_some_and(|m| {
            m.plane(at.plane)
                .has_enchanted_road(at.x as i32, at.y as i32)
        });
        if !enchanted {
            return 0;
        }
        let difficulty = self
            .content
            .rules
            .spells
            .id_of("enchant_road")
            .map_or(100, |s| self.dispel_difficulty(s, None));
        if !dispel_roll(strength, difficulty, rng) {
            return 0;
        }
        if let Some(m) = self.map.as_mut() {
            m.plane_mut(at.plane).set_enchanted_road(at.x, at.y, false);
        }
        1
    }

    /// Dispel a warped node on the square, against the cost of Warp Node.
    fn dispel_node(&mut self, at: Position, strength: u32, rng: &mut dyn RngCore) -> u32 {
        let warp = self
            .content
            .rules
            .spells
            .id_of(crate::map::corruption::WARP_NODE)
            .map_or(300, |s| self.content.rules.spells.get(s).cost);
        let Some(map) = self.map.as_mut() else {
            return 0;
        };
        let Some(i) = map.node_at(at.plane, at.x, at.y) else {
            return 0;
        };
        match map.node_mut(i) {
            Some(n) if n.warped && dispel_roll(strength, warp, rng) => {
                n.warped = false;
                1
            }
            _ => 0,
        }
    }

    /// Disjunction and Spell Binding: attack `victim`'s global enchantment `spell` with a dispel
    /// of `strength`. When `steal` is set, a success hands the spell to `by` instead of ending
    /// it. Returns whether it worked, or `None` if `victim` has no such spell (or is `by`).
    pub(crate) fn attack_global(
        &mut self,
        by: PlayerId,
        victim: PlayerId,
        spell: SpellId,
        strength: u32,
        steal: bool,
        rng: &mut dyn RngCore,
    ) -> Option<bool> {
        if victim == by || !self.magic(victim)?.global.contains(&spell) {
            return None;
        }
        let difficulty = self.dispel_difficulty(spell, Some(victim));
        if !dispel_roll(strength, difficulty, rng) {
            return Some(false);
        }
        self.magic_mut(victim)?.global.retain(|&s| s != spell);
        if steal {
            let mine = self.magic_mut(by)?;
            if !mine.global.contains(&spell) {
                mine.global.push(spell);
            }
            self.reveal_for_globals(by);
        }
        Some(true)
    }

    /// Spell Blast: the mana `victim` has put into the spell being cast, if any.
    pub fn mana_spent_casting(&self, victim: PlayerId) -> Option<i32> {
        let c = self.magic(victim)?.casting?;
        Some((self.casting_cost(victim, c.spell, false) - c.remaining).max(0))
    }

    /// Spell Blast by `by` on `victim`: pays as much mana as the victim has spent on the spell
    /// being cast and ends it. Returns the spell and the mana paid, or `None` if the victim
    /// is casting nothing or `by` cannot pay.
    pub(crate) fn spell_blast(&mut self, by: PlayerId, victim: PlayerId) -> Option<(SpellId, i32)> {
        if victim == by {
            return None;
        }
        let spent = self.mana_spent_casting(victim)?;
        if self.treasury(by)?.mana < spent {
            return None;
        }
        let spell = self.magic(victim)?.casting?.spell;
        self.treasury_mut(by)?.mana -= spent;
        self.magic_mut(victim)?.casting = None;
        Some((spell, spent))
    }

    /// Another wizard's standing dispel of strength 500 against every overland spell cast:
    /// Suppress Magic against all spells, Life Force against death spells, Tranquility against
    /// chaos spells. Returns the wizard whose enchantment stopped `spell`, if any.
    pub(crate) fn suppressed_by(&mut self, caster: PlayerId, spell: SpellId) -> Option<PlayerId> {
        let realm = self.content.rules.spells.get(spell).realm.clone();
        let guards: Vec<&str> = ["suppress_magic", "life_force", "tranquility"]
            .into_iter()
            .filter(|&g| match g {
                "life_force" => realm == "death",
                "tranquility" => realm == "chaos",
                _ => true,
            })
            .collect();
        let holders: Vec<PlayerId> = self
            .player_ids()
            .into_iter()
            .filter(|&p| p != caster)
            .flat_map(|p| {
                let has = &guards;
                has.iter()
                    .filter(|&&g| self.has_global(p, g))
                    .map(move |_| p)
                    .collect::<Vec<_>>()
            })
            .collect();
        let difficulty = self.dispel_difficulty(spell, Some(caster));
        for holder in holders {
            let chance = 250 * u64::from(SUPPRESS_MAGIC_STRENGTH)
                / u64::from(SUPPRESS_MAGIC_STRENGTH + difficulty);
            if u64::from(self.roll(250)) <= chance {
                return Some(holder);
            }
        }
        None
    }
}

/// What a hook reports about the attack on a global enchantment.
pub(crate) fn global_attack_events(
    by: PlayerId,
    victim: PlayerId,
    spell: SpellId,
    result: Option<bool>,
    steal: bool,
) -> Vec<Event> {
    match result {
        Some(true) if steal => vec![Event::GlobalSpellTaken {
            player: by,
            victim,
            spell,
        }],
        Some(true) => vec![Event::GlobalSpellRemoved {
            player: by,
            victim,
            spell,
        }],
        Some(false) => vec![Event::DispelFailed { player: by, spell }],
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
