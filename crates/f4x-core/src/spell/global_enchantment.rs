//! What a wizard's global enchantments do to the rest of the game.
//!
//! The spells sit in `Magic::global`; the code that cares asks here. Plain stat changes come
//! from the spell's own passive rules (with a condition such as "normal units"); the rest is
//! recognised by spell id.
//!
//! Done: Evil Omens (cost), Herb Mastery, Planar Seal, Awareness, Nature Awareness, Detect
//! Magic, Holy Arms (the attack bonus), Chaos Surge, Charm of Life. The others are listed in
//! `docs/TASKS.md` (task 87).

use crate::game::Game;
use crate::player::PlayerId;
use crate::rules::effects::{Effect, Trigger};
use crate::rules::exec::{eval, Facts};
use crate::rules::{Modifier, Source, SpellId, Stat};
use crate::unit::{Hurt, UnitView};

/// Does this wizard keep the global spell with this id going?
pub(crate) fn magic_has_global(
    wizard: &crate::wizard::entity::Wizard<'_>,
    content: &crate::content::Content,
    id: &str,
) -> bool {
    wizard
        .global()
        .iter()
        .any(|&s| content.rules.spells.get(s).id == id)
}

/// The hook global spells carry when the engine reads them from the wizard's list instead of
/// running anything when they are cast.
pub const GLOBAL_EFFECT: &str = "global_effect";

/// The fame Just Cause adds while it lasts.
pub(crate) const JUST_CAUSE_FAME: i32 = 10;

impl Game {
    /// Does `player` keep the global spell with this id going?
    pub fn has_global(&self, player: PlayerId, id: &str) -> bool {
        self.wizard(player)
            .is_some_and(|m| magic_has_global(&m, &self.content, id))
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
        self.player_ids()
            .into_iter()
            .any(|p| self.has_global(p, "evil_omens"))
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
                self.fog
                    .explore(player, at, crate::map::fog::sight_for(w.max(h)));
            }
        } else if self.has_global(player, "awareness") {
            let cities: Vec<_> = self
                .economy()
                .cities()
                .into_iter()
                .map(|(_, _, p)| p)
                .collect();
            for at in cities {
                self.fog.explore(player, at, crate::map::fog::sight_for(1));
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
            self.fog.explore(city.owner, at, sight);
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
        g.fog = crate::map::fog::Fog::new(24, 14);
        for id in [P0, P1] {
            g.add_player(id, "W");
            g.treasury_mut(id).unwrap().mana = 5000;
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
    fn planar_seal_stops_every_plane_shift() {
        let mut g = world();
        let traveller = g.content.rules.units.id_of("traveller").unwrap();
        let u = g.spawn_unit(P0, at(5, 5), traveller);
        assert!(g.economy().can_shift_plane(&[u]).is_ok());
        grant(&mut g, P1, "planar_seal");
        assert!(g.units().planar_seal_active());
        assert_eq!(
            g.economy().can_shift_plane(&[u]),
            Err(crate::unit::movement::MoveError::CannotShiftPlane),
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
        assert_eq!(
            g.units().unit_hits(a).0,
            g.units().unit_hits(a).1,
            "mine is healed"
        );
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
        let melee = g.units().unit_stat(normal, Stat::Melee);
        grant(&mut g, P0, "holy_arms");
        assert_eq!(g.units().unit_stat(normal, Stat::Melee), melee);
        assert!(!g.units().unit_has_magical_weapon(normal), "undead: no");
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
        let melee = |g: &Game, u| g.units().unit_stat(u, Stat::Melee);
        let (m0, m1) = (melee(&g, normal), melee(&g, chaos));
        assert!(!g.units().unit_has_magical_weapon(normal));
        grant(&mut g, P0, "holy_arms");
        assert_eq!(melee(&g, normal), m0 + 1);
        assert!(g.units().unit_has_magical_weapon(normal), "blessed blades");
        assert!(!g.units().unit_has_magical_weapon(rival), "not mine");
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
        let before = g.units().unit_stat(u, Stat::MaxHp);
        grant(&mut g, P0, "charm_of_life");
        assert_eq!(
            g.units().unit_stat(u, Stat::MaxHp),
            before + (before / 4).max(1)
        );
    }

    #[test]
    fn awareness_shows_cities_and_nature_awareness_the_whole_map() {
        let mut g = world();
        let elf = g.content.rules.races.id_of("elf");
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
    fn just_cause_adds_ten_to_fame_while_it_lasts() {
        let mut g = world();
        g.set_fame(P0, 3);
        assert_eq!(g.fame(P0), 3);
        grant(&mut g, P0, "just_cause");
        assert_eq!(g.fame(P0), 13);
        assert_eq!(g.fame(P1), g.wizard(P1).unwrap().fame(), "only the caster");
        assert_eq!(g.score(P0).fame, 26, "and it counts for the score");
    }

    #[test]
    fn detect_magic_shows_what_others_are_casting() {
        let mut g = world();
        let s = spell(&g, "healing");
        g.wizard_mut(P1)
            .unwrap()
            .get_mut::<crate::wizard::magic::Magic>()
            .casting = Some(crate::wizard::magic::Casting {
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
