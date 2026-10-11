//! Nightshade: a plant in a city's area that shields the city from enemy spells. Each plant
//! adds 100 dispel strength against a bad enchantment cast on the city, so a spell aimed at
//! the city can fizzle. Only a city with a building that allows its use counts the plants.
//!
//! In the original the count is a pseudo-enchantment, set each turn for cities with one of
//! the buildings below and used when a spell is cast at the city (`Spells129.c`). Its
//! mid-turn dispel of the city's own bad enchantments (`CTY_NightshadeDispel`) is an empty
//! stub there and is not modelled.

use crate::city::PlayerId;
use crate::economy::Population;
use crate::rules::{SpellId, SpellTarget};
use crate::Game;

/// Buildings that allow the use of nightshade. The original's code lists eight of them, with
/// the Shrine checked twice where the Animist's Guild, which its help names, was evidently
/// meant; we include the Animist's Guild. (The help names only four buildings and is
/// extended in the imported help.)
pub const NIGHTSHADE_BUILDINGS: [&str; 9] = [
    "shrine",
    "animists_guild",
    "temple",
    "parthenon",
    "cathedral",
    "sages_guild",
    "oracle",
    "alchemists_guild",
    "wizards_guild",
];

/// Dispel strength each plant adds.
pub const STRENGTH_PER_PLANT: u32 = 100;

impl Game {
    /// Nightshade plants the city can use: those in its area, if it has a building that allows
    /// them and is not an outpost.
    pub fn nightshade_count(&self, city: hecs::Entity) -> u32 {
        let (Some(at), Some(map)) = (
            self.world
                .get::<&crate::city::Position>(city)
                .ok()
                .map(|p| *p),
            self.map.as_ref(),
        ) else {
            return 0;
        };
        if self.world.get::<&Population>(city).map_or(0, |p| p.figures) == 0 {
            return 0;
        }
        let rules = &self.content.rules;
        let allowed = NIGHTSHADE_BUILDINGS
            .iter()
            .filter_map(|b| rules.buildings.id_of(b))
            .any(|b| self.has_building(city, b));
        let Some(plant) = self.content.features.id_of("nightshade") else {
            return 0;
        };
        if !allowed {
            return 0;
        }
        let grid = map.plane(at.plane);
        self.city_area(at)
            .iter()
            .filter(|(p, _)| grid.feature(p.x as i32, p.y as i32) == Some(plant))
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
        if def.target != SpellTarget::EnemyCity || self.city_owner(city) == Some(player) {
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
    use crate::city::Position;
    use crate::content::{CityBonus, FeatureDef, FeatureKind, Features};
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::rules::{Registry, SpellTarget};
    use crate::testutil::content_with_economy;
    use crate::Command;

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
        let mut c = content_with_economy();
        let mut defs = c.features.defs().to_vec();
        defs.push(FeatureDef {
            id: "nightshade".into(),
            name: "Nightshade".into(),
            kind: FeatureKind::Mineral,
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
        let grass = c.terrains.id_of("grassland").unwrap();
        let plant = c.features.id_of("nightshade").unwrap();
        let mut game = Game::new(3);
        game.content = c;
        let mut map = Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass));
        for &(x, y) in plants {
            map.plane_mut(Plane::Arcanus).set_feature(x, y, Some(plant));
        }
        game.map = Some(map);
        game.players.push(Player::new(P0, "Owner"));
        game.players.push(Player::new(P1, "Caster"));
        let race = game.content.rules.races.id_of("elf");
        game.apply(Command::FoundCity {
            owner: P0,
            pos: at(10, 6),
            name: "Town".into(),
            race,
            population: 4,
        });
        let city = game.city_at(at(10, 6)).unwrap();
        let spell = game.content.rules.spells.id_of("famine").unwrap();
        (game, city, spell)
    }

    fn build(g: &mut Game, city: hecs::Entity, id: &str) {
        let b = g.content.rules.buildings.id_of(id).unwrap();
        g.world
            .get::<&mut crate::economy::Buildings>(city)
            .unwrap()
            .0
            .push(b);
    }

    #[test]
    fn plants_count_only_for_a_city_with_a_building_that_allows_them() {
        let (mut g, city, _) = world(&[(10, 8), (11, 6), (20, 12)], 10);
        assert_eq!(g.nightshade_count(city), 0, "no temple or shrine yet");
        build(&mut g, city, "temple");
        assert_eq!(
            g.nightshade_count(city),
            2,
            "the plant far away is out of reach"
        );
    }

    #[test]
    fn an_outpost_counts_no_plants() {
        let (mut g, city, _) = world(&[(10, 8)], 10);
        build(&mut g, city, "temple");
        g.world
            .insert_one(
                city,
                crate::economy::Population {
                    figures: 0,
                    tens: 0,
                },
            )
            .unwrap();
        assert_eq!(g.nightshade_count(city), 0);
    }

    #[test]
    fn with_no_difficulty_one_plant_always_turns_an_enemy_spell_away() {
        let (mut g, city, spell) = world(&[(10, 8)], 0);
        build(&mut g, city, "temple");
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
            build(&mut g, city, "temple");
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
        build(&mut g, city, "temple");
        let events = g.resolve_cast(P1, spell, Some(crate::rules::exec::Target::City(city)));
        assert!(matches!(
            events.as_slice(),
            [crate::Event::SpellFizzled { .. }]
        ));
        assert!(g
            .world
            .get::<&crate::economy::Enchantments>(city)
            .map_or(true, |e| e.0.is_empty()));
    }
}
