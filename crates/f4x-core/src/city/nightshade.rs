//! Nightshade: a plant in a city's area that shields the city from enemy spells. Each plant
//! adds 100 dispel strength against a bad enchantment cast on the city, so a spell aimed at
//! the city can fizzle. Only a city with a building that allows its use counts the plants.
//!
//! In the original the count is a pseudo-enchantment, set each turn for cities with one of
//! the buildings below and used when a spell is cast at the city (`Spells129.c`). Its
//! mid-turn dispel of the city's own bad enchantments (`CTY_NightshadeDispel`) is an empty
//! stub there and is not modelled.

use crate::city::economy::Population;
use crate::game::Game;
use crate::player::PlayerId;
use crate::rules::BuildingId;
use crate::rules::{SpellId, SpellTarget};

/// Buildings that allow the use of nightshade. The original's code lists eight of them, with
/// the Shrine checked twice where the Animist's Guild, which its help names, was evidently
/// meant; we include the Animist's Guild. (The help names only four buildings and is
/// extended in the imported help.)
pub const NIGHTSHADE_BUILDINGS: [BuildingId; 9] = [
    BuildingId::new(22), // shrine
    BuildingId::new(10), // animists_guild
    BuildingId::new(23), // temple
    BuildingId::new(24), // parthenon
    BuildingId::new(25), // cathedral
    BuildingId::new(17), // sages_guild
    BuildingId::new(18), // oracle
    BuildingId::new(19), // alchemists_guild
    BuildingId::new(21), // wizards_guild
];

/// Dispel strength each plant adds.
pub const STRENGTH_PER_PLANT: u32 = 100;

impl Game {
    /// Nightshade plants the city can use: those in its area, if it has a building that allows
    /// them and is not an outpost.
    pub fn nightshade_count(&self, city: hecs::Entity) -> u32 {
        let (Some(at), Some(map)) = (
            self.world
                .get::<&crate::map::Position>(city)
                .ok()
                .map(|p| *p),
            self.map.as_ref(),
        ) else {
            return 0;
        };
        if self.world.get::<&Population>(city).map_or(0, |p| p.figures) == 0 {
            return 0;
        }
        let allowed = NIGHTSHADE_BUILDINGS
            .iter()
            .any(|&b| self.economy().has_building(city, b));
        let Some(plant) = self.content.features.id_numbered(16)
        /* nightshade */
        else {
            return 0;
        };
        if !allowed {
            return 0;
        }
        let grid = map.plane(at.plane);
        self.economy()
            .city_area(at)
            .iter()
            .filter(|(p, _)| grid.mineral(p.x as i32, p.y as i32) == Some(plant))
            .count() as u32
    }

    /// Whether the plants turn `spell`, cast by `player` at `city`, away. Only a spell aimed at
    /// an enemy city (a bad city enchantment) is resisted; the chance is the plants' dispel
    /// strength over that strength plus the spell's dispel difficulty.
    pub(crate) fn nightshade_fizzles(
        &mut self,
        player: PlayerId,
        spell: SpellId,
        city: hecs::Entity,
    ) -> bool {
        let def = self.content.rules.spells.get(spell);
        if def.target != SpellTarget::EnemyCity || self.economy().city_owner(city) == Some(player) {
            return false;
        }
        let strength = self.nightshade_count(city) * STRENGTH_PER_PLANT;
        if strength == 0 {
            return false;
        }
        // The caster's masteries make the spell harder to turn away, as for any dispel.
        let difficulty = self.dispel_difficulty(spell, Some(player));
        self.roll(100) as u64 <= u64::from(strength) * 100 / u64::from(strength + difficulty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::def::{CityBonus, FeatureDef, FeatureKind, Features};
    use crate::game::testutil::{num, shipped};
    use crate::game::Command;
    use crate::map::Position;
    use crate::map::{Grid, Map, Plane};
    use crate::rules::{Registry, SpellTarget};
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

    /// P0's city at (10, 6) of 4 people; a spell "famine" that curses enemy cities with the
    /// given dispel difficulty; nightshade on the squares listed.
    fn world(plants: &[(u32, u32)], difficulty: u32) -> (Game, hecs::Entity, SpellId) {
        let mut c = shipped();
        let mut defs = c.features.defs().to_vec();
        defs.push(FeatureDef {
            num: defs.len() as u16 + 1,
            id: "nightshade".into(),
            kind: FeatureKind::Mineral,
            realm: None,
            holy: false,
            city: CityBonus::default(),
            spawn: vec![],
        });
        c.features = Features::new(defs);
        let mut spells = c.rules.spells.defs().to_vec();
        for s in &mut spells {
            if s.id == "famine" {
                s.target = SpellTarget::EnemyCity;
                s.dispel_difficulty = difficulty;
                s.cost = difficulty;
            }
        }
        c.rules.spells = Registry::new(spells);
        let grass = c
            .terrains
            .id_numbered(num::terrain::GRASSLAND)
            .expect("id_of succeeds");
        let plant = c
            .features
            .id_numbered(num::feature::NIGHTSHADE)
            .expect("id_of succeeds");
        let mut game = Game::new(3);
        game.content = c;
        let mut map = Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass));
        for &(x, y) in plants {
            map.plane_mut(Plane::Arcanus).set_mineral(x, y, Some(plant));
        }
        game.map = Some(map);
        game.add_player(P0, "Owner");
        game.add_player(P1, "Caster");
        let race = game.content.rules.races.id_numbered(7);
        game.apply(Command::FoundCity {
            owner: P0,
            pos: at(10, 6),
            name: "Town".into(),
            race,
            population: 4,
        });
        let city = game.economy().city_at(at(10, 6)).expect("city_at succeeds");
        let spell = game
            .content
            .rules
            .spells
            .id_numbered(189)
            .expect("id_of succeeds");
        (game, city, spell)
    }

    fn build(g: &mut Game, city: hecs::Entity, id: u16) {
        let b = g
            .content
            .rules
            .buildings
            .id_numbered(id)
            .expect("id_of succeeds");
        g.world
            .get::<&mut crate::city::economy::Buildings>(city)
            .expect("value succeeds")
            .0
            .push(b);
    }

    #[test]
    fn plants_count_only_for_a_city_with_a_building_that_allows_them() {
        let (mut g, city, _) = world(&[(10, 8), (11, 6), (20, 12)], 10);
        assert_eq!(g.nightshade_count(city), 0, "no temple or shrine yet");
        build(&mut g, city, 23 /* temple */);
        assert_eq!(
            g.nightshade_count(city),
            2,
            "the plant far away is out of reach"
        );
    }

    #[test]
    fn an_outpost_counts_no_plants() -> TestResult {
        let (mut g, city, _) = world(&[(10, 8)], 10);
        build(&mut g, city, 23 /* temple */);
        g.world.insert_one(
            city,
            crate::city::economy::Population {
                figures: 0,
                tens: 0,
            },
        )?;
        assert_eq!(g.nightshade_count(city), 0);
        Ok(())
    }

    #[test]
    fn with_no_difficulty_one_plant_always_turns_an_enemy_spell_away() {
        let (mut g, city, spell) = world(&[(10, 8)], 0);
        build(&mut g, city, 23 /* temple */);
        for _ in 0..20 {
            assert!(g.nightshade_fizzles(P1, spell, city));
        }
        assert!(
            !g.nightshade_fizzles(P0, spell, city),
            "the owner's own spells are not resisted"
        );
    }

    #[test]
    fn more_plants_turn_more_spells_away() {
        // A hundred dispel strength per plant against difficulty 300: 25% for one plant,
        // 40% for two.
        let rate = |plants: &[(u32, u32)]| {
            let (mut g, city, spell) = world(plants, 300);
            build(&mut g, city, 23 /* temple */);
            (0..2000)
                .filter(|_| g.nightshade_fizzles(P1, spell, city))
                .count()
        };
        let (one, two) = (rate(&[(10, 8)]), rate(&[(10, 8), (11, 6)]));
        assert!((400..600).contains(&one), "one plant: {one} of 2000");
        assert!((700..900).contains(&two), "two plants: {two} of 2000");
        assert_eq!(rate(&[]), 0);
    }

    #[test]
    fn a_fizzled_spell_puts_nothing_on_the_city() {
        let (mut g, city, spell) = world(&[(10, 8)], 0);
        build(&mut g, city, 23 /* temple */);
        let events = g.resolve_cast(P1, spell, Some(crate::rules::exec::Target::City(city)));
        assert!(matches!(
            events.as_slice(),
            [crate::game::Event::SpellFizzled { .. }]
        ));
        assert!(g.city_enchantments(city).is_empty());
    }
}
