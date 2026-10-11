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
//! nightshade (`nightshade.rs`). Spell Blast cancels the spell another wizard is casting.

use rand::RngCore;

use crate::city::{PlayerId, Position};
use crate::economy::Enchantments;
use crate::magic::Retort;
use crate::rules::exec::Target;
use crate::rules::SpellId;
use crate::units::UnitEnchantments;
use crate::{Event, Game};

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
    /// and how many were removed. A unit under another wizard's Spell Lock can only have the
    /// lock attacked, at difficulty 150.
    pub(crate) fn dispel_list(
        &self,
        list: Vec<(SpellId, PlayerId)>,
        strength: u32,
        by: PlayerId,
        guarded_by_lock: bool,
        rng: &mut dyn RngCore,
    ) -> (Vec<(SpellId, PlayerId)>, u32) {
        let lock = self.content.rules.spells.id_of("spell_lock");
        if guarded_by_lock {
            if let Some(&(_, caster)) = list.iter().find(|&&(s, c)| Some(s) == lock && c != by) {
                let difficulty =
                    SPELL_LOCK_DIFFICULTY * self.dispel_multiplier("sorcery", Some(caster));
                if dispel_roll(strength, difficulty, rng) {
                    let kept = list.into_iter().filter(|&(s, _)| Some(s) != lock).collect();
                    return (kept, 1);
                }
                return (list, 0);
            }
        }
        let mut kept = Vec::new();
        let mut removed = 0;
        for (spell, caster) in list {
            let difficulty = self.dispel_difficulty(spell, Some(caster));
            if caster != by && dispel_roll(strength, difficulty, rng) {
                removed += 1;
            } else {
                kept.push((spell, caster));
            }
        }
        (kept, removed)
    }

    /// A dispel of `strength` by wizard `by` on a unit, a city or everything on a map square
    /// (the city, the units, a warped node). Only enchantments cast by other wizards are
    /// attacked. Returns how many were removed.
    pub(crate) fn dispel_target(
        &mut self,
        who: Target,
        strength: u32,
        by: PlayerId,
        rng: &mut dyn RngCore,
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
                removed
            }
            Target::City(c) => {
                let list = self
                    .world
                    .get::<&Enchantments>(c)
                    .map(|e| e.0.clone())
                    .unwrap_or_default();
                let (kept, removed) = self.dispel_list(list, strength, by, false, rng);
                let _ = self.world.insert_one(c, Enchantments(kept));
                removed
            }
            Target::Tile(at) => {
                let mut removed = self.dispel_node(at, strength, rng);
                if let Some(c) = self.city_at(at) {
                    removed += self.dispel_target(Target::City(c), strength, by, rng);
                }
                for u in self.units_at(at) {
                    removed += self.dispel_target(Target::Unit(u), strength, by, rng);
                }
                removed
            }
            _ => 0,
        }
    }

    /// Dispel a warped node on the square, against the cost of Warp Node.
    fn dispel_node(&mut self, at: Position, strength: u32, rng: &mut dyn RngCore) -> u32 {
        let warp = self
            .content
            .rules
            .spells
            .id_of(crate::corruption::WARP_NODE)
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
        if self.player(by)?.mana < spent {
            return None;
        }
        let spell = self.magic(victim)?.casting?.spell;
        self.player_mut(by)?.mana -= spent;
        self.magic_mut(victim)?.casting = None;
        Some((spell, spent))
    }

    /// Suppress Magic: another wizard's standing dispel of strength 500 against every overland
    /// spell cast. Returns the wizard whose Suppress Magic stopped `spell`, if any.
    pub(crate) fn suppressed_by(&mut self, caster: PlayerId, spell: SpellId) -> Option<PlayerId> {
        let suppress = self.content.rules.spells.id_of("suppress_magic")?;
        let holders: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|p| p.id != caster && p.magic.global.contains(&suppress))
            .map(|p| p.id)
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
mod tests {
    use super::*;
    use crate::city::Position;
    use crate::magic::{Casting, Retort};
    use crate::map::{Grid, Map, Node, Plane};
    use crate::player::Player;
    use crate::rules::effects::{EffectRule, Trigger};
    use crate::rules::{CastContext, Duration, Rarity, Registry, SpellDef, SpellTarget};
    use crate::testutil::content_with_magic;
    use crate::Command;
    use rand::SeedableRng;

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    fn rng() -> rand_pcg::Pcg64Mcg {
        rand_pcg::Pcg64Mcg::seed_from_u64(5)
    }

    /// A spell: lasting ones have `dispel_difficulty` equal to their cost.
    fn spell(
        id: &str,
        realm: &str,
        cost: u32,
        target: SpellTarget,
        lasting: bool,
        effect: Option<crate::rules::effects::Effect>,
    ) -> SpellDef {
        SpellDef {
            id: id.into(),
            name: id.into(),
            realm: realm.into(),
            rarity: Rarity::Common,
            cost,
            research_cost: 10,
            contexts: vec![CastContext::Overland],
            target,
            duration: if lasting {
                Duration::Permanent
            } else {
                Duration::Instant
            },
            upkeep: 0,
            dispel_difficulty: if lasting { cost } else { 0 },
            save: None,
            effects: effect
                .map(|e| {
                    vec![EffectRule {
                        when: Trigger::OnCast,
                        target: crate::rules::effects::TargetSel::Chosen,
                        condition: crate::rules::effects::Condition::Always,
                        effect: e,
                    }]
                })
                .unwrap_or_default(),
        }
    }

    /// Two wizards (lots of mana and skill), grassland, and the spells the tests use.
    fn world() -> Game {
        use crate::rules::effects::Effect;
        let mut c = content_with_magic();
        let mut defs = c.rules.spells.defs().to_vec();
        defs.extend([
            spell(
                "spell_lock",
                "sorcery",
                20,
                SpellTarget::OwnUnit,
                true,
                None,
            ),
            spell("aura", "sorcery", 100, SpellTarget::None, true, None),
            spell(
                "suppress_magic",
                "sorcery",
                1250,
                SpellTarget::None,
                true,
                None,
            ),
            spell(
                "disjunction",
                "arcane",
                5000,
                SpellTarget::GlobalEnchantment,
                false,
                Some(Effect::Custom(DISJUNCTION.into())),
            ),
            spell(
                "spell_binding",
                "sorcery",
                5000,
                SpellTarget::GlobalEnchantment,
                false,
                Some(Effect::Custom(SPELL_BINDING.into())),
            ),
            spell(
                "spell_blast",
                "sorcery",
                50,
                SpellTarget::EnemyWizard,
                false,
                Some(Effect::Custom(SPELL_BLAST.into())),
            ),
            spell(
                "disenchant_area",
                "arcane",
                5000,
                SpellTarget::Tile,
                false,
                Some(Effect::Dispel { strength: 5000 }),
            ),
        ]);
        c.rules.spells = Registry::new(defs);
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(11);
        g.content = c;
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        for (id, name) in [(P0, "Merlin"), (P1, "Rjak")] {
            let mut p = Player::new(id, name);
            p.mana = 20_000;
            g.players.push(p);
            g.init_wizard(id, &[("life", 2)], &[]).unwrap();
            g.magic_mut(id).unwrap().skill_left = 99_999;
        }
        g
    }

    fn sp(g: &Game, id: &str) -> SpellId {
        g.content.rules.spells.id_of(id).unwrap()
    }

    fn cast(g: &mut Game, spell: &str, target: Target) -> Vec<Event> {
        let s = sp(g, spell);
        g.magic_mut(P0).unwrap().known.insert(s);
        g.apply(Command::CastSpell {
            player: P0,
            spell: s,
            target: Some(target),
        })
    }

    #[test]
    fn the_chance_is_strength_over_strength_plus_difficulty() {
        let mut r = rng();
        let mut hits = |strength, difficulty| {
            (0..4000)
                .filter(|_| dispel_roll(strength, difficulty, &mut r))
                .count()
        };
        assert_eq!(hits(0, 10), 0);
        assert!((900..1100).contains(&hits(10, 30)), "a quarter");
        assert!((1900..2100).contains(&hits(30, 30)), "a half");
        // The original's roll of 250 never beats 249: even overwhelming strength fails 1 in 250.
        assert!(hits(100_000, 1) > 3900);
    }

    #[test]
    fn masteries_and_archmage_multiply_a_spells_difficulty() {
        let mut g = world();
        let aura = sp(&g, "aura"); // sorcery, 100
        assert_eq!(g.dispel_difficulty(aura, Some(P0)), 100);
        g.magic_mut(P0)
            .unwrap()
            .retorts
            .insert(Retort::SorceryMastery);
        assert_eq!(
            g.dispel_difficulty(aura, Some(P0)),
            200,
            "its realm's mastery"
        );
        g.magic_mut(P0)
            .unwrap()
            .retorts
            .insert(Retort::NatureMastery);
        assert_eq!(
            g.dispel_difficulty(aura, Some(P0)),
            200,
            "another realm's does not count"
        );
        g.magic_mut(P0).unwrap().retorts.insert(Retort::Archmage);
        assert_eq!(g.dispel_difficulty(aura, Some(P0)), 300);
        assert_eq!(g.dispel_difficulty(aura, Some(P1)), 100);
        assert_eq!(g.dispel_difficulty(aura, None), 100);
    }

    #[test]
    fn only_other_wizards_enchantments_are_dispelled() {
        let mut g = world();
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let unit = g.spawn_unit(P0, at(3, 3), spear);
        let strength = sp(&g, "giant_strength");
        g.attach_enchantment(Target::Unit(unit), strength, P0);
        let mut r = rng();
        assert_eq!(
            g.dispel_target(Target::Unit(unit), 1_000_000, P0, &mut r),
            0
        );
        assert_eq!(
            g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut r),
            1
        );
    }

    #[test]
    fn spell_lock_must_be_dispelled_before_anything_else_on_the_unit() {
        let mut g = world();
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let unit = g.spawn_unit(P0, at(3, 3), spear);
        let (lock, strength) = (sp(&g, "spell_lock"), sp(&g, "giant_strength"));
        g.attach_enchantment(Target::Unit(unit), strength, P0);
        g.attach_enchantment(Target::Unit(unit), lock, P0);
        let mut r = rng();
        assert_eq!(
            g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut r),
            1,
            "the first dispel only takes the lock"
        );
        assert_eq!(
            g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut r),
            1
        );
        assert_eq!(
            g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut r),
            0
        );
        // A weak dispel rarely beats the lock's 150, and a lock never shields from its owner.
        g.attach_enchantment(Target::Unit(unit), lock, P0);
        g.attach_enchantment(Target::Unit(unit), strength, P1);
        assert_eq!(
            g.dispel_target(Target::Unit(unit), 1_000_000, P0, &mut r),
            1
        );
    }

    #[test]
    fn disenchant_area_clears_a_city_its_units_and_a_warped_node() {
        let mut g = world();
        let race = g.content.rules.races.id_of("elf");
        g.apply(Command::FoundCity {
            owner: P0,
            pos: at(6, 6),
            name: "Town".into(),
            race,
            population: 3,
        });
        let city = g.city_at(at(6, 6)).unwrap();
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let unit = g.spawn_unit(P0, at(6, 6), spear);
        let elsewhere = g.spawn_unit(P0, at(9, 9), spear);
        let (strength, aura) = (sp(&g, "giant_strength"), sp(&g, "aura"));
        g.attach_enchantment(Target::City(city), aura, P0);
        g.attach_enchantment(Target::Unit(unit), strength, P0);
        g.attach_enchantment(Target::Unit(elsewhere), strength, P0);
        g.map.as_mut().unwrap().set_nodes(vec![Node {
            plane: Plane::Arcanus,
            x: 6,
            y: 6,
            realm: "chaos".into(),
            power: 8,
            aura: vec![(6, 6)],
            owner: Some(0),
            guardian: false,
            warped: true,
        }]);
        let events = cast(&mut g, "disenchant_area", Target::Tile(at(6, 6)));
        assert!(
            events.contains(&Event::Dispelled {
                player: P0,
                removed: 1
            }),
            "only the warp on the caster's own node goes, their own enchantments stay: {events:?}"
        );
        g.map.as_mut().unwrap().node_mut(0).unwrap().warped = true;
        // The same cast by the other wizard takes it all.
        let da = sp(&g, "disenchant_area");
        g.magic_mut(P1).unwrap().known.insert(da);
        let events = g.apply(Command::CastSpell {
            player: P1,
            spell: sp(&g, "disenchant_area"),
            target: Some(Target::Tile(at(6, 6))),
        });
        assert!(
            events.contains(&Event::Dispelled {
                player: P1,
                removed: 3
            }),
            "{events:?}"
        );
        assert!(!g.node_at(at(6, 6)).unwrap().warped);
        let left = g.world.get::<&UnitEnchantments>(elsewhere).unwrap().0.len();
        assert_eq!(left, 1, "another square is left alone");
    }

    #[test]
    fn disjunction_ends_and_spell_binding_steals_an_enemy_global() {
        let mut g = world();
        let aura = sp(&g, "aura");
        g.attach_enchantment(Target::Player(P1), aura, P1);
        let events = cast(&mut g, "spell_binding", Target::Global(P1, aura));
        assert!(
            events.contains(&Event::GlobalSpellTaken {
                player: P0,
                victim: P1,
                spell: aura
            }),
            "{events:?}"
        );
        assert!(g.magic(P0).unwrap().global.contains(&aura));
        assert!(!g.magic(P1).unwrap().global.contains(&aura));
        // Now P1 ends it with Disjunction.
        let dj = sp(&g, "disjunction");
        g.magic_mut(P1).unwrap().known.insert(dj);
        let events = g.apply(Command::CastSpell {
            player: P1,
            spell: sp(&g, "disjunction"),
            target: Some(Target::Global(P0, aura)),
        });
        assert!(
            events.contains(&Event::GlobalSpellRemoved {
                player: P1,
                victim: P0,
                spell: aura
            }),
            "{events:?}"
        );
        assert!(g.magic(P0).unwrap().global.is_empty());
        // Nothing to take: refused.
        let events = cast(&mut g, "disjunction", Target::Global(P1, aura));
        assert!(
            matches!(events.as_slice(), [Event::Rejected(_)]),
            "{events:?}"
        );
        // You cannot disjoin your own.
        g.attach_enchantment(Target::Player(P0), aura, P0);
        let events = cast(&mut g, "disjunction", Target::Global(P0, aura));
        assert!(matches!(events.as_slice(), [Event::Rejected(_)]));
    }

    #[test]
    fn a_weak_disjunction_often_fails() {
        let mut g = world();
        let aura = sp(&g, "aura"); // difficulty 100
        g.attach_enchantment(Target::Player(P1), aura, P1);
        let mut r = rng();
        let wins = (0..2000)
            .filter(|_| {
                let won = g.attack_global(P0, P1, aura, 100, false, &mut r) == Some(true);
                g.attach_enchantment(Target::Player(P1), aura, P1);
                won
            })
            .count();
        assert!((900..1100).contains(&wins), "half of 2000: {wins}");
    }

    #[test]
    fn spell_blast_costs_what_the_victim_has_spent_and_ends_their_spell() {
        let mut g = world();
        let aura = sp(&g, "aura");
        let cost = g.casting_cost(P1, aura, false);
        g.magic_mut(P1).unwrap().casting = Some(Casting {
            spell: aura,
            remaining: cost - 60,
        });
        assert_eq!(g.mana_spent_casting(P1), Some(60));
        let mana = g.player(P0).unwrap().mana;
        let blast = sp(&g, "spell_blast");
        let paid = g.casting_cost(P0, blast, false);
        let events = cast(&mut g, "spell_blast", Target::Player(P1));
        assert!(
            events.contains(&Event::SpellBlasted {
                player: P0,
                victim: P1,
                spell: aura,
                mana: 60
            }),
            "{events:?}"
        );
        assert_eq!(g.player(P0).unwrap().mana, mana - paid - 60);
        assert_eq!(g.magic(P1).unwrap().casting, None);
        // Nothing being cast: not a target.
        let events = cast(&mut g, "spell_blast", Target::Player(P1));
        assert!(
            matches!(events.as_slice(), [Event::Rejected(_)]),
            "{events:?}"
        );
    }

    #[test]
    fn spell_blast_fails_when_the_caster_cannot_match_the_mana() {
        let mut g = world();
        let aura = sp(&g, "aura");
        let cost = g.casting_cost(P1, aura, false);
        g.magic_mut(P1).unwrap().casting = Some(Casting {
            spell: aura,
            remaining: cost - 90,
        });
        g.player_mut(P0).unwrap().mana = g.casting_cost(P0, sp(&g, "spell_blast"), false) + 50;
        let events = cast(&mut g, "spell_blast", Target::Player(P1));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::SpellBlasted { .. })),
            "{events:?}"
        );
        assert!(g.magic(P1).unwrap().casting.is_some());
    }

    #[test]
    fn suppress_magic_stops_enemy_spells_with_its_strength_over_the_spells_cost() {
        let mut g = world();
        let suppress = sp(&g, "suppress_magic");
        g.attach_enchantment(Target::Player(P1), suppress, P1);
        // Strength 500 against difficulty 500: half.
        let big = sp(&g, "spell_binding"); // cost 5000 ...
        let mid = {
            let mut defs = g.content.rules.spells.defs().to_vec();
            defs.push(spell("mid", "life", 500, SpellTarget::None, false, None));
            g.content.rules.spells = Registry::new(defs);
            sp(&g, "mid")
        };
        let stopped = (0..2000)
            .filter(|_| g.suppressed_by(P0, mid).is_some())
            .count();
        assert!((900..1100).contains(&stopped), "half of 2000: {stopped}");
        assert!(
            g.suppressed_by(P1, mid).is_none(),
            "never against its own wizard"
        );
        let stopped_big = (0..2000)
            .filter(|_| g.suppressed_by(P0, big).is_some())
            .count();
        assert!(
            stopped_big < 400,
            "a costly spell mostly gets through: {stopped_big}"
        );
    }

    #[test]
    fn a_stopped_spell_is_lost_and_reported() {
        let mut g = world();
        let suppress = sp(&g, "suppress_magic");
        g.attach_enchantment(Target::Player(P1), suppress, P1);
        let mut stopped = 0;
        for _ in 0..30 {
            let events = cast(&mut g, "aura", Target::Player(P0));
            if events
                .iter()
                .any(|e| matches!(e, Event::SpellSuppressed { by, .. } if *by == P1))
            {
                stopped += 1;
                assert!(!g.magic(P0).unwrap().global.contains(&sp(&g, "aura")));
                break;
            }
            g.magic_mut(P0).unwrap().global.clear();
        }
        assert_eq!(stopped, 1, "aura costs 100 against 500: about 83% stopped");
    }
}
