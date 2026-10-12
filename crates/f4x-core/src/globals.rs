//! What a wizard's global enchantments do to the rest of the game.
//!
//! The spells sit in `Magic::global`; the code that cares asks here. Plain stat changes come
//! from the spell's own passive rules (with a condition such as "normal units"); the rest is
//! recognised by spell id.
//!
//! Done: Evil Omens (cost), Herb Mastery, Planar Seal, Awareness, Nature's Awareness, Detect
//! Magic, Holy Arms (the attack bonus), Chaos Surge, Charm of Life. The others are listed in
//! `docs/TASKS.md` (task 87).

use crate::city::PlayerId;
use crate::rules::effects::{Effect, Trigger};
use crate::rules::exec::{eval, Facts};
use crate::rules::{Modifier, Source, SpellId, Stat};
use crate::units::{Hurt, Units};
use crate::Game;

/// Does this wizard keep the global spell with this id going?
pub(crate) fn magic_has_global(
    magic: &crate::magic::Magic,
    content: &crate::content::Content,
    id: &str,
) -> bool {
    magic
        .global
        .iter()
        .any(|&s| content.rules.spells.get(s).id == id)
}

/// The hook global spells carry when the engine reads them from the wizard's list instead of
/// running anything when they are cast.
pub const GLOBAL_EFFECT: &str = "global_effect";

impl Game {
    /// Does `player` keep the global spell with this id going?
    pub fn has_global(&self, player: PlayerId, id: &str) -> bool {
        self.magic(player)
            .is_some_and(|m| magic_has_global(m, &self.content, id))
    }

    /// Does any wizard keep the global spell with this id going?
    pub fn any_global(&self, id: &str) -> bool {
        self.players.iter().any(|p| self.has_global(p.id, id))
    }

    /// Is another wizard's Evil Omens making `player`'s life and nature spells dearer?
    pub(crate) fn evil_omens_against(&self, player: PlayerId) -> bool {
        self.players
            .iter()
            .any(|p| p.id != player && self.has_global(p.id, "evil_omens"))
    }

    /// The spell each other wizard is casting, as far as `viewer`'s Detect Magic shows it.
    /// Empty without the spell.
    pub fn detect_magic_view(&self, viewer: PlayerId) -> Vec<(PlayerId, SpellId)> {
        if !self.has_global(viewer, "detect_magic") {
            return Vec::new();
        }
        self.players
            .iter()
            .filter(|p| p.id != viewer)
            .filter_map(|p| Some((p.id, self.magic(p.id)?.casting?.spell)))
            .collect()
    }

    /// Explore what Awareness and Nature's Awareness show `player`.
    pub(crate) fn reveal_for_globals(&mut self, player: PlayerId) {
        let Some(map) = self.map.as_ref() else {
            return;
        };
        let (w, h) = (map.width(), map.height());
        if self.has_global(player, "nature_awareness") {
            for plane in crate::map::Plane::ALL {
                // One square large enough to cover the whole plane.
                let at = crate::city::Position {
                    plane,
                    x: w / 2,
                    y: h / 2,
                };
                self.fog.explore(player, at, w.max(h));
            }
        } else if self.has_global(player, "awareness") {
            let cities: Vec<_> = self.cities().into_iter().map(|(_, _, p)| p).collect();
            for at in cities {
                self.fog.explore(player, at, 0);
            }
        }
    }

    /// Once a turn: Herb Mastery heals every unit, and the revealing spells look again.
    pub(crate) fn process_globals(&mut self) {
        let ids: Vec<PlayerId> = self.players.iter().map(|p| p.id).collect();
        for player in ids {
            if self.has_global(player, "herb_mastery") {
                for unit in self.player_units(player) {
                    if !self.unit_is_death(unit) {
                        let _ = self.world.insert_one(unit, Hurt(0));
                    }
                }
            }
            self.reveal_for_globals(player);
        }
        // Cities explore what their sight reaches (an Oracle or Nature's Eye widens it).
        let cities: Vec<_> = self.cities().into_iter().collect();
        for (entity, city, at) in cities {
            let sight = self.economy().city_sight(entity).max(0) as u32;
            self.fog.explore(city.owner, at, sight);
        }
    }
}

impl Units<'_> {
    /// Modifiers a unit gets from its owner's global spells: rules with a condition are
    /// checked against the unit, and Charm of Life adds a quarter of its hit points.
    pub(crate) fn global_unit_modifiers(&self, unit: hecs::Entity) -> Vec<Modifier> {
        let (Some(owner), Some(def)) = (self.unit_owner(unit), self.unit_def(unit)) else {
            return Vec::new();
        };
        let Some(magic) = self.magic(owner) else {
            return Vec::new();
        };
        if magic.global.is_empty() {
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
        for &spell in &magic.global {
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
    use crate::city::Position;
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::rules::{Registry, UnitKind};
    use crate::testutil::content_with_magic;

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
                "walker" => u.kind = UnitKind::Normal,
                "slow" => u.realm = Some("chaos".into()),
                _ => {}
            }
        }
        c.rules.units = Registry::new(units);
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(3);
        g.content = c;
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        g.fog = crate::fog::Fog::new(24, 14);
        for id in [P0, P1] {
            let mut p = Player::new(id, "W");
            p.mana = 5000;
            g.players.push(p);
            g.init_wizard(id, &[("life", 4)], &[], None).unwrap();
        }
        g
    }

    fn spell(g: &Game, id: &str) -> SpellId {
        g.content.rules.spells.id_of(id).unwrap()
    }

    /// Cast a global spell for free (the cost rules are tested elsewhere).
    fn grant(g: &mut Game, player: PlayerId, id: &str) {
        let s = spell(g, id);
        g.attach_enchantment(crate::rules::exec::Target::Player(player), s, player);
    }

    #[test]
    fn evil_omens_makes_other_wizards_life_and_nature_dearer() {
        let mut g = world();
        let life = spell(&g, "life_c0");
        let chaos = spell(&g, "chaos_c0");
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
            before,
            "the caster is spared"
        );
    }

    #[test]
    fn planar_seal_stops_every_plane_shift() {
        let mut g = world();
        let traveller = g.content.rules.units.id_of("traveller").unwrap();
        let u = g.spawn_unit(P0, at(5, 5), traveller);
        assert!(g.can_shift_plane(&[u]).is_ok());
        grant(&mut g, P1, "planar_seal");
        assert!(g.units().planar_seal_active());
        assert_eq!(
            g.can_shift_plane(&[u]),
            Err(crate::movement::MoveError::CannotShiftPlane),
            "even for planar travellers, and even against the caster's rival"
        );
    }

    #[test]
    fn herb_mastery_heals_every_unit_each_turn() {
        let mut g = world();
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let a = g.spawn_unit(P0, at(5, 5), walker);
        let b = g.spawn_unit(P1, at(8, 5), walker);
        g.world.insert_one(a, Hurt(3)).unwrap();
        g.world.insert_one(b, Hurt(3)).unwrap();
        grant(&mut g, P0, "herb_mastery");
        g.process_globals();
        assert_eq!(g.unit_hits(a).0, g.unit_hits(a).1, "mine is healed");
        assert_eq!(g.world.get::<&Hurt>(b).unwrap().0, 3, "theirs is not");
    }

    #[test]
    fn undead_get_no_herb_mastery_no_holy_arms_and_cost_half_as_much_mana_again() {
        let mut g = world();
        let mut units = g.content.rules.units.defs().to_vec();
        for u in units.iter_mut() {
            if u.id == "slow" {
                u.upkeep.mana = 4;
            }
        }
        g.content.rules.units = Registry::new(units);
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let slow = g.content.rules.units.id_of("slow").unwrap();
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
        let melee = g.unit_stat(normal, Stat::Melee);
        grant(&mut g, P0, "holy_arms");
        assert_eq!(g.unit_stat(normal, Stat::Melee), melee);
        // Herb Mastery does not heal it either.
        g.world.insert_one(normal, Hurt(3)).unwrap();
        grant(&mut g, P0, "herb_mastery");
        g.process_globals();
        assert_eq!(g.world.get::<&Hurt>(normal).unwrap().0, 3);
    }

    #[test]
    fn holy_arms_and_chaos_surge_check_the_unit() {
        let mut g = world();
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let slow = g.content.rules.units.id_of("slow").unwrap();
        let normal = g.spawn_unit(P0, at(5, 5), walker);
        let chaos = g.spawn_unit(P0, at(6, 5), slow);
        let rival = g.spawn_unit(P1, at(9, 5), walker);
        let melee = |g: &Game, u| g.unit_stat(u, Stat::Melee);
        let (m0, m1) = (melee(&g, normal), melee(&g, chaos));
        grant(&mut g, P0, "holy_arms");
        assert_eq!(melee(&g, normal), m0 + 1);
        assert_eq!(melee(&g, chaos), m1, "not a normal unit");
        assert_eq!(melee(&g, rival), m0, "not mine");
        grant(&mut g, P0, "chaos_surge");
        assert_eq!(melee(&g, chaos), m1 + 2);
        assert_eq!(
            melee(&g, normal),
            m0 + 1,
            "chaos surge skips non-chaos units"
        );
    }

    #[test]
    fn charm_of_life_adds_a_quarter_of_the_hit_points_at_least_one() {
        let mut g = world();
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let u = g.spawn_unit(P0, at(5, 5), walker);
        let before = g.unit_stat(u, Stat::MaxHp);
        grant(&mut g, P0, "charm_of_life");
        assert_eq!(g.unit_stat(u, Stat::MaxHp), before + (before / 4).max(1));
    }

    #[test]
    fn awareness_shows_cities_and_nature_awareness_the_whole_map() {
        let mut g = world();
        let elf = g.content.rules.races.id_of("elf");
        g.apply(crate::Command::FoundCity {
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
        assert!(
            !g.fog.is_explored(P0, Plane::Arcanus, 19, 10),
            "nothing around it"
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
    fn detect_magic_shows_what_others_are_casting() {
        let mut g = world();
        let s = spell(&g, "life_c5");
        g.magic_mut(P1).unwrap().casting = Some(crate::magic::Casting {
            spell: s,
            remaining: 10,
        });
        assert!(g.detect_magic_view(P0).is_empty());
        grant(&mut g, P0, "detect_magic");
        assert_eq!(g.detect_magic_view(P0), vec![(P1, s)]);
        assert!(
            g.detect_magic_view(P1).is_empty(),
            "only the caster of Detect Magic sees"
        );
    }
}
