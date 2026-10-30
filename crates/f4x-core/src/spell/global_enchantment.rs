//! What a wizard's global enchantments do to the rest of the game.
//!
//! The spells are spell instances with the target `Global` (`spell/instance.rs`); the code that cares asks here. Plain stat changes come
//! from the spell's own passive rules (with a condition such as "normal units"); the rest is
//! recognised by spell id.

use crate::game::Game;
use crate::player::PlayerId;
use crate::rules::effects::{Effect, Trigger};
use crate::rules::exec::{eval, Facts};
use crate::rules::{Modifier, Source, SpellId, Stat};
use crate::spell::instance::SpellTarget;
use crate::unit::{Hurt, UnitView};
use crate::wizard::magic::MagicError;

/// Does this wizard keep the global spell with this id going?
pub(crate) fn magic_has_global(
    wizard: &crate::wizard::entity::Wizard<'_>,
    content: &crate::content::Content,
    id: &str,
) -> bool {
    wizard.any_global(|s| content.rules.spells.get(s).id == id)
}

/// The hook global spells carry when the engine reads them from the wizard's spells instead of
/// running anything when they are cast.
pub const GLOBAL_EFFECT: &str = "global_effect";

/// The fame Just Cause adds while it lasts.
pub(crate) const JUST_CAUSE_FAME: i32 = 10;

impl Game {
    /// A lasting spell starts on the whole world, cast by `player`'s wizard; it is there once.
    pub(crate) fn add_global(&mut self, player: PlayerId, spell: SpellId) {
        self.add_enchantment(SpellTarget::Global, spell, player);
    }

    /// Turn off one of the wizard's own global enchantments.
    pub(crate) fn turn_off(&mut self, player: PlayerId, spell: SpellId) -> Result<(), MagicError> {
        if self.wizard(player).is_none() {
            return Err(MagicError::UnknownPlayer);
        }
        if self.turn_off_enchantment(SpellTarget::Global, player, spell) {
            Ok(())
        } else {
            Err(MagicError::NotAvailable)
        }
    }

    /// Every wizard's lasting spells on the world end (Disjunction).
    pub(crate) fn end_all_globals(&mut self) {
        for instance in self.spells().on(SpellTarget::Global) {
            self.despawn_spell(instance);
        }
    }

    /// A wizard's lasting spells on the world all end, for a test that starts again.
    #[cfg(test)]
    pub(crate) fn clear_globals(&mut self, player: PlayerId) {
        if let Some(caster) = self.wizard(player).map(|w| w.entity()) {
            for instance in self.spells().globals_of(caster) {
                self.despawn_spell(instance);
            }
        }
    }

    /// Does `player` keep the global spell with this id going?
    pub fn has_global(&self, player: PlayerId, id: &str) -> bool {
        self.wizard(player)
            .is_some_and(|m| magic_has_global(&m, &self.content, id))
    }

    /// The instance of `spell` that `player` keeps going on the whole world: what Disjunction
    /// and Spell Binding take as their target.
    pub fn global_instance(&self, player: PlayerId, spell: SpellId) -> Option<hecs::Entity> {
        let wizard = self.wizard(player)?.entity();
        let spells = self.spells();
        let held = spells.globals_of(wizard);
        held.into_iter().find(|&i| spells.def(i) == Some(spell))
    }

    /// The global enchantments of every wizard but `player`, wizard by wizard and oldest first:
    /// the list Disjunction and Spell Binding choose from.
    pub fn rival_globals(&self, player: PlayerId) -> Vec<hecs::Entity> {
        let spells = self.spells();
        self.player_ids()
            .into_iter()
            .filter(|&p| p != player)
            .filter_map(|p| self.wizard(p).map(|w| w.entity()))
            .flat_map(|w| spells.globals_of(w))
            .collect()
    }

    /// The wizard whose Time Stop is stopping everyone else's time, if any.
    pub fn time_stopper(&self) -> Option<PlayerId> {
        self.player_ids()
            .into_iter()
            .find(|&p| self.has_global(p, "time_stop"))
    }

    /// Does any wizard keep the global spell with this id going?
    pub fn any_global(&self, id: &str) -> bool {
        self.player_ids()
            .into_iter()
            .any(|p| self.has_global(p, id))
    }

    /// A wizard's fame as the rules count it: what they have earned, and ten more while Just
    /// Cause is kept going.
    pub fn fame(&self, player: PlayerId) -> i32 {
        self.units().fame(player)
    }

    /// Is anyone's Evil Omens, the caster's own included, making life and nature spells
    /// dearer?
    pub(crate) fn evil_omens_active(&self) -> bool {
        self.units().any_global("evil_omens")
    }

    /// The spell each other wizard is casting, as far as `viewer`'s Detect Magic shows it.
    /// Empty without the spell.
    pub fn detect_magic_view(&self, viewer: PlayerId) -> Vec<(PlayerId, SpellId)> {
        if !self.has_global(viewer, "detect_magic") {
            return Vec::new();
        }
        self.player_ids()
            .into_iter()
            .filter(|&p| p != viewer)
            .filter_map(|p| Some((p, self.wizard(p)?.casting()?.spell)))
            .collect()
    }

    /// Explore what Awareness and Nature Awareness show `player`.
    pub(crate) fn reveal_for_globals(&mut self, player: PlayerId) {
        let Some(map) = self.map.as_ref() else {
            return;
        };
        let (w, h) = (map.width(), map.height());
        if self.has_global(player, "nature_awareness") {
            for plane in crate::map::Plane::ALL {
                // One square large enough to cover the whole plane.
                let at = crate::map::Position {
                    plane,
                    x: w / 2,
                    y: h / 2,
                };
                self.fog.explore(
                    player,
                    at,
                    crate::map::fog::sight_for(w.max(h)),
                    self.settings.basic.diagonal_extra_cost,
                );
            }
        } else if self.has_global(player, "awareness") {
            let cities: Vec<_> = self
                .economy()
                .cities()
                .into_iter()
                .map(|(_, _, p)| p)
                .collect();
            for at in cities {
                self.fog.explore(
                    player,
                    at,
                    crate::map::fog::sight_for(1),
                    self.settings.basic.diagonal_extra_cost,
                );
            }
        }
    }

    /// Once a turn: Herb Mastery heals every unit, and the revealing spells look again.
    pub(crate) fn process_globals(&mut self) {
        let ids: Vec<PlayerId> = crate::player::ids(&self.world);
        for player in ids {
            if self.has_global(player, "herb_mastery") {
                for unit in self.units().player_units(player) {
                    if !self.units().unit_is_death(unit) {
                        let _ = self.world.insert_one(unit, Hurt(0));
                    }
                }
            }
            self.reveal_for_globals(player);
        }
        // Cities explore what their sight reaches (an Oracle or Nature's Eye widens it).
        let cities: Vec<_> = self.economy().cities().into_iter().collect();
        for (entity, city, at) in cities {
            let sight = self.economy().city_sight(entity).max(0) as u32;
            self.fog.explore(
                city.owner,
                at,
                sight,
                self.settings.basic.diagonal_extra_cost,
            );
        }
    }
}

impl UnitView<'_> {
    /// Modifiers a unit gets from its owner's global spells: rules with a condition are
    /// checked against the unit, and Charm of Life adds a quarter of its hit points.
    pub(crate) fn global_unit_modifiers(&self, unit: hecs::Entity) -> Vec<Modifier> {
        let (Some(owner), Some(def)) = (self.unit_owner(unit), self.unit_def(unit)) else {
            return Vec::new();
        };
        let Some(magic) = self.wizard(owner) else {
            return Vec::new();
        };
        let globals = magic.global();
        if globals.is_empty() {
            return Vec::new();
        }
        let facts = Facts {
            abilities: self.unit_abilities(unit),
            race: def.race.clone(),
            realm: self.unit_realm(unit),
            // The undead are not normal units: Holy Arms and Crusade pass them by.
            kind: Some(if self.unit_is_undead(unit) {
                crate::rules::UnitKind::Summoned
            } else {
                def.kind
            }),
            ..Facts::default()
        };
        let mut mods = Vec::new();
        for &spell in &globals {
            let sdef = self.content.rules.spells.get(spell);
            for rule in &sdef.effects {
                if rule.when != Trigger::Passive || !eval(&rule.condition, &facts) {
                    continue;
                }
                if let Effect::Modify { stat, op, value } = rule.effect {
                    mods.push(Modifier::new(stat, op, value, Source::Spell(spell)));
                }
            }
            if sdef.id == "charm_of_life" {
                let hp = def.stats.get(&Stat::MaxHp).copied().unwrap_or(0);
                if hp > 0 {
                    mods.push(Modifier::new(
                        Stat::MaxHp,
                        crate::rules::ModOp::Add,
                        (hp / 4).max(1),
                        Source::Spell(spell),
                    ));
                }
            }
        }
        mods
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::content_with_magic;
    use crate::map::Position;
    use crate::map::{Grid, Map, Plane};
    use crate::rules::{Registry, UnitKind};
    use crate::testdir::TestResult;

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    fn world() -> Game {
        let mut c = content_with_magic();
        let mut units = c.rules.units.defs().to_vec();
        for u in units.iter_mut() {
            match u.id.as_str() {
                "high_men_cavalry" => u.kind = UnitKind::Normal,
                "high_men_spearmen" => u.realm = Some("chaos".into()),
                _ => {}
            }
        }
        c.rules.units = Registry::new_sorted(units);
        let grass = c.terrains.id_of("grassland").expect("id_of succeeds");
        let mut g = Game::new(3);
        g.content = c;
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        g.fog = crate::map::fog::Fog::new(24, 14);
        for id in [P0, P1] {
            g.add_player(id, "W");
            g.treasury_mut(id).expect("treasury_mut succeeds").mana = 5000;
            g.init_wizard(id, &[("life", 4)], &[], None)
                .expect("init_wizard succeeds");
        }
        g
    }

    fn spell(g: &Game, id: &str) -> SpellId {
        g.content.rules.spells.id_of(id).expect("id_of succeeds")
    }

    /// Cast a global spell for free (the cost rules are tested elsewhere).
    fn grant(g: &mut Game, player: PlayerId, id: &str) {
        let s = spell(g, id);
        g.attach_enchantment(crate::rules::exec::Target::Player(player), s, player);
    }

    #[test]
    fn evil_omens_makes_life_and_nature_dearer_for_everyone() {
        let mut g = world();
        let life = spell(&g, "healing");
        let chaos = spell(&g, "fire_bolt");
        let (before, before_chaos) = (
            g.casting_cost(P0, life, false),
            g.casting_cost(P0, chaos, false),
        );
        grant(&mut g, P1, "evil_omens");
        assert_eq!(g.casting_cost(P0, life, false), before + before / 2);
        assert_eq!(
            g.casting_cost(P0, chaos, false),
            before_chaos,
            "chaos is unaffected"
        );
        assert_eq!(
            g.casting_cost(P1, life, false),
            before + before / 2,
            "the caster pays too"
        );
    }

    #[test]
    fn planar_seal_stops_every_plane_shift() -> TestResult {
        let mut g = world();
        let traveller = g
            .content
            .rules
            .units
            .id_of("shadow_demons")
            .ok_or("shadow_demons is not in the content")?;
        let u = g.spawn_unit(P0, at(5, 5), traveller);
        assert!(g.economy().can_shift_plane(&[u]).is_ok());
        grant(&mut g, P1, "planar_seal");
        assert!(g.units().planar_seal_active());
        assert_eq!(
            g.economy().can_shift_plane(&[u]),
            Err(crate::unit::movement::MoveError::CannotShiftPlane),
            "even for planar travellers, and even against the caster's rival"
        );
        Ok(())
    }

    #[test]
    fn herb_mastery_heals_every_unit_each_turn() -> TestResult {
        let mut g = world();
        let walker = g
            .content
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("high_men_cavalry is not in the content")?;
        let a = g.spawn_unit(P0, at(5, 5), walker);
        let b = g.spawn_unit(P1, at(8, 5), walker);
        g.world.insert_one(a, Hurt(3))?;
        g.world.insert_one(b, Hurt(3))?;
        grant(&mut g, P0, "herb_mastery");
        g.process_globals();
        assert_eq!(
            g.units().unit_hits(a).0,
            g.units().unit_hits(a).1,
            "mine is healed"
        );
        assert_eq!(g.world.get::<&Hurt>(b)?.0, 3, "theirs is not");
        Ok(())
    }

    #[test]
    fn undead_get_no_herb_mastery_no_holy_arms_and_cost_half_as_much_mana_again() -> TestResult {
        let mut g = world();
        // The Fire Elemental costs 4 mana a turn to keep.
        let walker = g
            .content
            .rules
            .units
            .id_of("high_men_spearmen")
            .ok_or("high_men_spearmen is not in the content")?;
        let slow = g
            .content
            .rules
            .units
            .id_of("fire_elemental")
            .ok_or("fire_elemental is not in the content")?;
        let normal = g.spawn_unit(P0, at(5, 5), walker);
        let fantastic = g.spawn_unit(P0, at(6, 5), slow);
        assert_eq!(g.income(P0).mana_upkeep, 4);
        g.make_undead(fantastic);
        assert_eq!(g.income(P0).mana_upkeep, 6, "half as much again");
        g.make_undead(normal);
        assert_eq!(
            g.income(P0).mana_upkeep,
            6,
            "a normal unit made undead costs none"
        );
        // Holy Arms passes a unit made undead by.
        let melee = g.units().unit_stat(normal, Stat::Melee);
        grant(&mut g, P0, "holy_arms");
        assert_eq!(g.units().unit_stat(normal, Stat::Melee), melee);
        assert!(!g.units().unit_has_magical_weapon(normal), "undead: no");
        // Herb Mastery does not heal it either.
        g.world.insert_one(normal, Hurt(3))?;
        grant(&mut g, P0, "herb_mastery");
        g.process_globals();
        assert_eq!(g.world.get::<&Hurt>(normal)?.0, 3);
        Ok(())
    }

    #[test]
    fn holy_arms_and_chaos_surge_check_the_unit() -> TestResult {
        let mut g = world();
        let walker = g
            .content
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("high_men_cavalry is not in the content")?;
        let slow = g
            .content
            .rules
            .units
            .id_of("high_men_spearmen")
            .ok_or("high_men_spearmen is not in the content")?;
        let normal = g.spawn_unit(P0, at(5, 5), walker);
        let chaos = g.spawn_unit(P0, at(6, 5), slow);
        let rival = g.spawn_unit(P1, at(9, 5), walker);
        let melee = |g: &Game, u| g.units().unit_stat(u, Stat::Melee);
        let to_hit = |g: &Game, u| g.units().unit_stat(u, Stat::ToHit);
        let (m0, m1) = (melee(&g, normal), melee(&g, chaos));
        let h0 = to_hit(&g, normal);
        assert!(!g.units().unit_has_magical_weapon(normal));
        grant(&mut g, P0, "holy_arms");
        assert_eq!(to_hit(&g, normal), h0 + 1);
        assert_eq!(melee(&g, normal), m0, "to hit, not strength");
        assert!(g.units().unit_has_magical_weapon(normal), "blessed blades");
        assert!(!g.units().unit_has_magical_weapon(rival), "not mine");
        assert_eq!(melee(&g, chaos), m1, "not a normal unit");
        assert_eq!(melee(&g, rival), m0, "not mine");
        grant(&mut g, P0, "chaos_surge");
        assert_eq!(melee(&g, chaos), m1 + 2);
        assert_eq!(melee(&g, normal), m0, "chaos surge skips non-chaos units");
        Ok(())
    }

    #[test]
    fn charm_of_life_adds_a_quarter_of_the_hit_points_at_least_one() -> TestResult {
        let mut g = world();
        let walker = g
            .content
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("high_men_cavalry is not in the content")?;
        let u = g.spawn_unit(P0, at(5, 5), walker);
        let before = g.units().unit_stat(u, Stat::MaxHp);
        grant(&mut g, P0, "charm_of_life");
        assert_eq!(
            g.units().unit_stat(u, Stat::MaxHp),
            before + (before / 4).max(1)
        );
        Ok(())
    }

    #[test]
    fn nature_awareness_and_natures_eye_show_who_guards_a_site() -> TestResult {
        let mut g = world();
        let guards = vec![("high_men_cavalry".to_string(), 2)];
        let lair = crate::feature::lair::Lair {
            intact: true,
            guards: guards.clone(),
            gold: 0,
            mana: 0,
            items: Vec::new(),
            prisoner: false,
            special: 0,
            fought_by: Vec::new(),
        };
        g.put_lair(at(20, 10), "lair", lair);
        let known = |g: &Game| g.economy().lair_guards_known_to(at(20, 10), P0);
        assert_eq!(known(&g), None, "never fought, nothing shown");
        // Nature's Eye on a city nearby: sight 9 reaches four squares and a half.
        let elf = g.content.rules.races.id_of("high_elf");
        g.apply(crate::game::Command::FoundCity {
            owner: P0,
            pos: at(17, 10),
            name: "Near".into(),
            race: elf,
            population: 3,
        });
        let city = g
            .economy()
            .city_at(at(17, 10))
            .ok_or("city_at is missing")?;
        assert_eq!(known(&g), None, "three squares away is outside plain sight");
        let eye = spell(&g, "natures_eye");
        g.attach_enchantment(crate::rules::exec::Target::City(city), eye, P0);
        assert_eq!(known(&g), Some(guards.clone()));
        // Another wizard's eye does not show it to P0.
        assert_eq!(g.economy().lair_guards_known_to(at(20, 10), P1), None);
        // Nature Awareness shows every site.
        g.put_lair(
            at(2, 2),
            "lair",
            crate::feature::lair::Lair {
                intact: true,
                guards: guards.clone(),
                gold: 0,
                mana: 0,
                items: Vec::new(),
                prisoner: false,
                special: 0,
                fought_by: Vec::new(),
            },
        );
        assert_eq!(g.economy().lair_guards_known_to(at(2, 2), P0), None);
        grant(&mut g, P0, "nature_awareness");
        assert_eq!(g.economy().lair_guards_known_to(at(2, 2), P0), Some(guards));
        Ok(())
    }

    #[test]
    fn awareness_shows_cities_and_nature_awareness_the_whole_map() {
        let mut g = world();
        let elf = g.content.rules.races.id_of("high_elf");
        g.apply(crate::game::Command::FoundCity {
            owner: P1,
            pos: at(20, 10),
            name: "Far".into(),
            race: elf,
            population: 3,
        });
        assert!(!g.fog.is_explored(P0, Plane::Arcanus, 20, 10));
        grant(&mut g, P0, "awareness");
        assert!(
            g.fog.is_explored(P0, Plane::Arcanus, 20, 10),
            "the city is revealed"
        );
        assert_eq!(
            g.fog.corners(P0, Plane::Arcanus, 20, 10),
            15,
            "the city square is fully known"
        );
        assert!(
            g.fog.corners(P0, Plane::Arcanus, 19, 10) < 15,
            "its neighbours only at the corners"
        );
        assert!(
            !g.fog.is_explored(P0, Plane::Arcanus, 18, 10),
            "nothing further out"
        );
        grant(&mut g, P0, "nature_awareness");
        assert!(g.fog.is_explored(P0, Plane::Myrror, 3, 3));
        assert!(g
            .economy()
            .visibility(P0, Plane::Arcanus)
            .iter()
            .all(|&v| v));
        assert!(g
            .economy()
            .visibility(P1, Plane::Arcanus)
            .iter()
            .any(|&v| !v));
    }

    #[test]
    fn just_cause_adds_ten_to_fame_while_it_lasts() -> TestResult {
        let mut g = world();
        g.set_fame(P0, 3);
        assert_eq!(g.fame(P0), 3);
        grant(&mut g, P0, "just_cause");
        assert_eq!(g.fame(P0), 13);
        assert_eq!(
            g.fame(P1),
            g.wizard(P1).ok_or("wizard is missing")?.fame(),
            "only the caster"
        );
        assert_eq!(g.score(P0).fame, 26, "and it counts for the score");
        Ok(())
    }

    #[test]
    fn detect_magic_shows_what_others_are_casting() {
        let mut g = world();
        let s = spell(&g, "healing");
        g.scenario_casting(P1, s, 10);
        assert!(g.detect_magic_view(P0).is_empty());
        grant(&mut g, P0, "detect_magic");
        assert_eq!(g.detect_magic_view(P0), vec![(P1, s)]);
        assert!(
            g.detect_magic_view(P1).is_empty(),
            "only the caster of Detect Magic sees"
        );
    }
}
