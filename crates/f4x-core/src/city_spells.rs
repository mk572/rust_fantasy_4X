//! City enchantments whose effects are more than stat changes (task 25). Most are read by name
//! from the city's enchantment list where they matter (`economy.rs` for unrest, `battle/` for
//! sieges, `turn.rs` for what happens each turn); this file holds the shared lookups and what
//! casting does.
//!
//! - **Wall of Stone** builds City Walls in a friendly city that has none (the original's
//!   `Cast_Wall_Of_Stone`); it is not an enchantment, so nothing can dispel it.
//! - **Evil Presence**: the magic power of the city's religious buildings (shrine, temple,
//!   parthenon, cathedral) is destroyed, unless the city's owner can use death magic.
//! - **Cloud of Shadow**, **Wall of Fire**, **Wall of Darkness**, **Flying Fortress**: read when
//!   a battle starts at the city (`battle/game.rs`).
//! - **Chaos Rift**: see [`Game::process_chaos_rifts`].

use crate::city::PlayerId;
use crate::economy::{Buildings, Enchantments};
use crate::production::ProductionItem;
use crate::{Event, Game};

/// The hook of the city enchantments the engine reads by name: casting only enchants the city.
pub const CITY_FLAG: &str = "city_flag";
/// The hook of Wall of Stone.
pub const WALL_OF_STONE: &str = "wall_of_stone";
/// The hook of Spell Ward: the realm is picked when casting.
pub const SPELL_WARD: &str = "spell_ward";

/// Chaos Rift's bolts on each unit in the city every turn, and their strength.
const CHAOS_RIFT_BOLTS: u32 = 5;
const CHAOS_RIFT_STRENGTH: i32 = 8;
/// The chance in a hundred that a building falls each turn.
const CHAOS_RIFT_BUILDING_PERCENT: u32 = 5;

/// The realms a Spell Ward can be set against, in the original's order.
pub const WARD_REALMS: [&str; 5] = ["nature", "sorcery", "chaos", "life", "death"];

/// The realms a city has Spell Wards against (the ward spell itself, with its caster and its
/// upkeep, is in the city's enchantments; this says which realms it was set to). Read through
/// [`Game::city_wards`], which ignores it once the spell is gone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpellWards(pub Vec<String>);

/// The buildings whose magic power Evil Presence destroys.
pub(crate) const RELIGIOUS: [&str; 4] = ["shrine", "temple", "parthenon", "cathedral"];

impl Game {
    /// Whether the city carries the named lasting spell, whoever cast it.
    pub fn city_has_enchantment(&self, city: hecs::Entity, name: &str) -> bool {
        self.content.rules.spells.id_of(name).is_some_and(|id| {
            self.world
                .get::<&Enchantments>(city)
                .is_ok_and(|e| e.0.iter().any(|(s, _)| *s == id))
        })
    }

    /// Whether the city may be given walls by Wall of Stone: it has none (a city with walls
    /// built or replaced is refused).
    pub fn can_raise_wall_of_stone(&self, city: hecs::Entity) -> bool {
        self.content.rules.buildings.id_of("city_walls").is_some()
            && !self.city_has_building(city, "city_walls")
    }

    /// Wall of Stone: the city gets City Walls.
    pub(crate) fn wall_of_stone(&mut self, city: hecs::Entity) -> Vec<Event> {
        let Some(walls) = self.content.rules.buildings.id_of("city_walls") else {
            return Vec::new();
        };
        if !self.can_raise_wall_of_stone(city) {
            return Vec::new();
        }
        if let Ok(mut b) = self.world.get::<&mut Buildings>(city) {
            b.0.push(walls);
        }
        vec![Event::ProductionCompleted {
            city,
            item: ProductionItem::Building(walls),
        }]
    }

    /// Chaos Rift: every turn each unit inside the city sustains five Lightning Bolts of strength
    /// 8 (armour piercing, stopped by magic immunity), and there is a 5 percent chance that a
    /// building falls (one that no other building needs). The original's code for it is a stub, so
    /// this is the help text's rule.
    pub(crate) fn process_chaos_rifts(&mut self) -> Vec<Event> {
        use crate::battle::immunity::{AttackNature, Immunities, Realm};
        use crate::rules::effects::DamageTraits;
        use rand::SeedableRng;
        let mut events = Vec::new();
        let rifts: Vec<hecs::Entity> = self
            .cities()
            .into_iter()
            .map(|(city, _, _)| city)
            .filter(|&c| self.city_has_enchantment(c, "chaos_rift"))
            .collect();
        for city in rifts {
            let Ok(at) = self.world.get::<&crate::city::Position>(city).map(|p| *p) else {
                continue;
            };
            let traits = DamageTraits {
                armor_piercing: true,
                ..Default::default()
            };
            let nature = AttackNature {
                stopped_by: Immunities {
                    magic: true,
                    ..Default::default()
                },
                realm: Some(Realm::Chaos),
                armor_piercing: false,
            };
            let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(self.next_u64() ^ 0xC4A0);
            let mut units_lost = 0;
            for unit in self.units_at(at) {
                for _ in 0..CHAOS_RIFT_BOLTS {
                    if self.spell_hit_overland_unit(
                        unit,
                        CHAOS_RIFT_STRENGTH,
                        &traits,
                        nature,
                        &mut rng,
                    ) {
                        units_lost += 1;
                        break;
                    }
                }
            }
            let mut building = None;
            if self.roll(100) <= CHAOS_RIFT_BUILDING_PERCENT {
                let candidates: Vec<crate::rules::BuildingId> = self
                    .world
                    .get::<&Buildings>(city)
                    .map(|b| b.0.clone())
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|&b| !self.has_dependants(city, b))
                    .collect();
                if !candidates.is_empty() {
                    let pick = candidates[self.roll(candidates.len() as u32) as usize - 1];
                    if self.destroy_building(city, pick) {
                        building = Some(pick);
                    }
                }
            }
            events.push(Event::ChaosRiftStrikes {
                city,
                units_lost,
                building,
            });
        }
        events
    }

    /// The realms the city is warded against.
    pub fn city_wards(&self, city: hecs::Entity) -> Vec<String> {
        if !self.city_has_enchantment(city, "spell_ward") {
            return Vec::new();
        }
        self.world
            .get::<&SpellWards>(city)
            .map(|w| w.0.clone())
            .unwrap_or_default()
    }

    /// Whether the city is warded against spells of `realm` (a realm's id).
    pub fn city_warded_against(&self, city: hecs::Entity, realm: &str) -> bool {
        self.city_wards(city).iter().any(|w| w == realm)
    }

    /// Whether a Spell Ward against `realm` can be put on the city: it is one of the five realms
    /// and not warded yet.
    pub fn can_ward(&self, city: hecs::Entity, realm: crate::rules::RealmId) -> bool {
        let id = &self.content.rules.realms.get(realm).id;
        WARD_REALMS.contains(&id.as_str()) && !self.city_warded_against(city, id)
    }

    /// Whether a Spell Ward on the city keeps the unit out: it is a fantastic creature (or a
    /// summoned one) of a realm the city is warded against. The help says "any": the owner's own
    /// creatures too. (Not in the original's code, which never reads the wards for movement.)
    pub fn ward_bars_creature(&self, city: hecs::Entity, unit: hecs::Entity) -> bool {
        use crate::rules::UnitKind;
        let Some(def) = self.unit_def(unit) else {
            return false;
        };
        matches!(def.kind, UnitKind::Fantastic | UnitKind::Summoned)
            && def
                .realm
                .as_deref()
                .is_some_and(|r| self.city_warded_against(city, r))
    }

    /// Whether every realm a Spell Ward can name is warded already.
    pub fn fully_warded(&self, city: hecs::Entity) -> bool {
        WARD_REALMS
            .iter()
            .all(|r| self.city_warded_against(city, r))
    }

    /// Set a Spell Ward against `realm` on the city. The first ward of a city starts its list
    /// afresh, so wards left over from a spell that was dispelled or lost with the city are gone.
    pub(crate) fn add_spell_ward(&mut self, city: hecs::Entity, realm: crate::rules::RealmId) {
        let id = self.content.rules.realms.get(realm).id.clone();
        let mut wards = self.city_wards(city);
        if !wards.contains(&id) {
            wards.push(id);
        }
        let _ = self.world.insert_one(city, SpellWards(wards));
    }

    /// The city whose Spell Ward turns `spell` away when it is cast at `target`: a spell aimed
    /// at an enemy city, or at an enemy unit standing in one, of a realm the city is warded
    /// against (the original's `Apply_Automatic_Spell_Counters`).
    pub(crate) fn ward_turns_away(
        &self,
        player: PlayerId,
        spell: crate::rules::SpellId,
        target: Option<crate::rules::exec::Target>,
    ) -> Option<hecs::Entity> {
        use crate::rules::exec::Target;
        use crate::rules::SpellTarget;
        let def = self.content.rules.spells.get(spell);
        let city = match (def.target, target?) {
            (SpellTarget::EnemyCity, Target::City(c)) if self.city_owner(c) != Some(player) => c,
            (SpellTarget::EnemyUnit, Target::Unit(u)) => {
                let at = self.unit_position(u)?;
                self.city_at(at)
                    .filter(|&c| self.city_owner(c) != Some(player))?
            }
            _ => return None,
        };
        self.city_warded_against(city, &def.realm).then_some(city)
    }

    /// How many times the city's enchantment `spell` is paid for each turn: a Spell Ward costs
    /// its upkeep for every realm it guards.
    pub(crate) fn upkeep_count(&self, city: hecs::Entity, spell: crate::rules::SpellId) -> i32 {
        if self.content.rules.spells.get(spell).id == "spell_ward" {
            (self.city_wards(city).len() as i32).max(1)
        } else {
            1
        }
    }

    /// Evil Presence does nothing to a city whose owner can use death magic, and the original
    /// refuses to cast it there.
    pub fn evil_presence_is_wasted_on(&self, city: hecs::Entity) -> bool {
        self.city_owner(city)
            .and_then(|o| self.magic(o))
            .is_some_and(|m| m.books_in("death") > 0)
    }

    /// Whether Evil Presence is destroying the city's religious buildings' power: the spell is
    /// on the city and its owner cannot use death magic.
    pub(crate) fn evil_presence_active(&self, city: hecs::Entity) -> bool {
        self.city_has_enchantment(city, "evil_presence") && !self.evil_presence_is_wasted_on(city)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::Position;
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::rules::effects::{Effect, EffectRule};
    use crate::rules::exec::Target;
    use crate::rules::{BuildingDef, Duration, Registry, SpellDef, SpellTarget};
    use crate::testutil::content_with_magic;
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

    /// Two wizards on grassland, with the spells and buildings these tests need added to the
    /// test content.
    fn world() -> Game {
        let mut c = content_with_magic();
        let mut spells = c.rules.spells.defs().to_vec();
        let template = spells
            .iter()
            .find(|s| s.id == "gaias_blessing")
            .unwrap()
            .clone();
        let own = SpellTarget::OwnCity;
        let enemy = SpellTarget::EnemyCity;
        let (instant, lasting) = (Duration::Instant, Duration::Permanent);
        for (id, hook, target, duration, realm) in [
            ("wall_of_stone", WALL_OF_STONE, own, instant, "nature"),
            ("evil_presence", CITY_FLAG, enemy, lasting, "death"),
            ("chaos_rift", CITY_FLAG, enemy, lasting, "chaos"),
            ("wall_of_fire", CITY_FLAG, own, lasting, "chaos"),
            ("wall_of_darkness", CITY_FLAG, own, lasting, "death"),
            ("flying_fortress", CITY_FLAG, own, lasting, "sorcery"),
            ("cloud_of_shadow", CITY_FLAG, own, lasting, "death"),
            ("spell_ward", SPELL_WARD, own, lasting, "sorcery"),
        ] {
            spells.push(SpellDef {
                id: id.into(),
                name: id.into(),
                cost: 10,
                upkeep: if id == "spell_ward" { 5 } else { 0 },
                realm: realm.into(),
                target,
                duration,
                effects: vec![EffectRule {
                    effect: Effect::Custom(hook.into()),
                    ..template
                        .effects
                        .first()
                        .cloned()
                        .unwrap_or_else(|| EffectRule {
                            when: crate::rules::effects::Trigger::OnCast,
                            condition: crate::rules::effects::Condition::Always,
                            target: crate::rules::effects::TargetSel::Chosen,
                            effect: Effect::Custom(hook.into()),
                        })
                }],
                ..template.clone()
            });
        }
        c.rules.spells = Registry::new(spells);
        let mut realms = c.rules.realms.defs().to_vec();
        for r in ["sorcery", "death", "arcane"] {
            realms.push(crate::rules::RealmDef {
                id: r.into(),
                name: r.into(),
                ..realms[0].clone()
            });
        }
        c.rules.realms = Registry::new(realms);
        let mut buildings = c.rules.buildings.defs().to_vec();
        let temple = buildings.iter().find(|b| b.id == "temple").unwrap().clone();
        for id in ["city_walls", "shrine", "parthenon"] {
            buildings.push(BuildingDef {
                id: id.into(),
                name: id.into(),
                effects: if id == "city_walls" {
                    vec![]
                } else {
                    temple.effects.clone()
                },
                requires: vec![],
                replaces: None,
                ..temple.clone()
            });
        }
        c.rules.buildings = Registry::new(buildings);
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(3);
        g.content = c;
        g.content.rules.config.tax_unrest_percent = vec![0, 0, 0, 30, 45, 60, 75];
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        g.fog = crate::fog::Fog::new(24, 14);
        for id in [P0, P1] {
            let mut p = Player::new(id, "W");
            p.mana = 1000;
            p.tax_rate = 4;
            g.players.push(p);
        }
        g.init_wizard(P0, &[("life", 2)], &[]).unwrap();
        g.init_wizard(P1, &[("chaos", 2)], &[]).unwrap();
        g
    }

    fn found(g: &mut Game, owner: PlayerId, pos: Position, pop: u32) -> hecs::Entity {
        let race = g.content.rules.races.id_of("elf");
        g.apply(Command::FoundCity {
            owner,
            pos,
            name: "Town".into(),
            race,
            population: pop,
        });
        g.city_at(pos).unwrap()
    }

    fn add_building(g: &mut Game, city: hecs::Entity, id: &str) {
        let b = g.content.rules.buildings.id_of(id).unwrap();
        g.world.get::<&mut Buildings>(city).unwrap().0.push(b);
    }

    fn enchant(g: &mut Game, city: hecs::Entity, spell: &str, caster: PlayerId) {
        let s = g.content.rules.spells.id_of(spell).unwrap();
        g.world
            .get::<&mut Enchantments>(city)
            .unwrap()
            .0
            .push((s, caster));
    }

    #[test]
    fn wall_of_stone_builds_walls_once() {
        let mut g = world();
        let city = found(&mut g, P0, at(10, 6), 4);
        let spell = g.content.rules.spells.id_of("wall_of_stone").unwrap();
        g.magic_mut_for_tests(P0).known.insert(spell);
        g.magic_mut_for_tests(P0).skill_left = 500;
        assert!(!g.city_has_building(city, "city_walls"));
        let cast = |g: &mut Game| {
            g.apply(Command::CastSpell {
                player: P0,
                spell,
                target: Some(Target::City(city)),
            })
        };
        let events = cast(&mut g);
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::ProductionCompleted { .. })));
        assert!(g.city_has_building(city, "city_walls"));
        assert_eq!(
            g.check_target(P0, spell, Some(Target::City(city))),
            Err(crate::magic::MagicError::NoSuchTarget),
            "a walled city is refused"
        );
        // Nor does it work on somebody else's city.
        let other = found(&mut g, P1, at(18, 6), 4);
        assert_eq!(
            g.check_target(P0, spell, Some(Target::City(other))),
            Err(crate::magic::MagicError::NoSuchTarget)
        );
    }

    #[test]
    fn evil_presence_destroys_the_power_of_shrines_unless_the_owner_has_death_magic() {
        let mut g = world();
        let city = found(&mut g, P0, at(10, 6), 10);
        let mana = |g: &Game| g.city_economy(city).unwrap().mana;
        let rebels = |g: &Game| g.city_economy(city).unwrap().rebels;
        let bare = mana(&g);
        add_building(&mut g, city, "shrine");
        add_building(&mut g, city, "temple");
        let blessed = mana(&g);
        assert!(blessed > bare, "the buildings make power");
        let calm = rebels(&g);
        enchant(&mut g, city, "evil_presence", P1);
        assert_eq!(mana(&g), bare, "Evil Presence destroys it");
        assert_eq!(rebels(&g), calm, "but they still pacify");
        g.magic_mut_for_tests(P0).books.insert("death".into(), 1);
        assert_eq!(mana(&g), blessed, "a wizard with death magic is unharmed");
    }

    #[test]
    fn the_caster_pays_upkeep_for_enchantments_on_enemy_cities() {
        let mut g = world();
        let city = found(&mut g, P0, at(10, 6), 4);
        let mut defs = g.content.rules.spells.defs().to_vec();
        defs.iter_mut()
            .find(|d| d.id == "chaos_rift")
            .unwrap()
            .upkeep = 10;
        g.content.rules.spells = Registry::new(defs);
        enchant(&mut g, city, "chaos_rift", P1);
        let cost = |g: &Game, p| g.upkeep_items(p).iter().map(|i| i.mana).sum::<i32>();
        assert_eq!(cost(&g, P1), 10, "the wizard who cast it pays");
        assert_eq!(cost(&g, P0), 0, "the city's owner does not");
    }

    #[test]
    fn a_chaos_rift_bolts_the_units_inside_and_may_bring_a_building_down() {
        let mut g = world();
        let city = found(&mut g, P0, at(10, 6), 4);
        add_building(&mut g, city, "shrine");
        enchant(&mut g, city, "chaos_rift", P1);
        let spear = g.content.rules.units.id_of("spear").unwrap();
        let victim = g.spawn_unit(P0, at(10, 6), spear);
        let elsewhere = g.spawn_unit(P0, at(14, 3), spear);
        let hurt = |g: &Game, u| g.world.get::<&crate::units::Hurt>(u).map_or(0, |h| h.0);
        let mut fell = false;
        for _ in 0..200 {
            let events = g.process_chaos_rifts();
            assert!(events
                .iter()
                .all(|e| matches!(e, Event::ChaosRiftStrikes { .. })));
            fell |= events.iter().any(|e| {
                matches!(
                    e,
                    Event::ChaosRiftStrikes {
                        building: Some(_),
                        ..
                    }
                )
            });
            if g.world.get::<&crate::units::Unit>(victim).is_err() {
                break;
            }
        }
        assert!(
            g.world.get::<&crate::units::Unit>(victim).is_err() || hurt(&g, victim) > 0,
            "five strength 8 bolts a turn do not leave a spearman unharmed"
        );
        assert_eq!(hurt(&g, elsewhere), 0, "units outside the city are safe");
        // The building fell sooner or later (5 percent a turn); run on until it does.
        let mut turns = 0;
        while !fell && turns < 2000 {
            turns += 1;
            fell |= g.process_chaos_rifts().iter().any(|e| {
                matches!(
                    e,
                    Event::ChaosRiftStrikes {
                        building: Some(_),
                        ..
                    }
                )
            });
        }
        assert!(fell, "a building fell");
        assert!(!g.city_has_building(city, "shrine"));
    }

    #[test]
    fn a_battle_at_an_enchanted_city_starts_with_its_walls_and_shadow() {
        use crate::battle::field::Field;
        use crate::battle::Side;
        let mut g = world();
        let city = found(&mut g, P0, at(10, 6), 4);
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let defender = g.spawn_unit(P0, at(10, 6), walker);
        let attacker = g.spawn_unit(P1, at(11, 6), walker);
        g.start_battle(&[attacker], &[defender]).unwrap();
        let plain = g.battle().unwrap();
        assert!(!plain.wall_of_fire && !plain.wall_of_darkness);
        assert!(!plain.map.is_flying_fortress());
        assert!(plain.fields[Side::Defender.index()].is_empty());
        g.battle = None;
        for s in [
            "wall_of_fire",
            "wall_of_darkness",
            "flying_fortress",
            "cloud_of_shadow",
        ] {
            enchant(&mut g, city, s, P0);
        }
        g.start_battle(&[attacker], &[defender]).unwrap();
        let b = g.battle().unwrap();
        assert!(b.wall_of_fire && b.wall_of_darkness);
        assert!(b.map.is_flying_fortress());
        assert!(b.fields[Side::Defender.index()]
            .iter()
            .any(|&(f, _)| f == Field::Darkness));
        assert!(b.fields[Side::Attacker.index()].is_empty());
    }

    fn realm(g: &Game, id: &str) -> crate::rules::RealmId {
        g.content.rules.realms.id_of(id).unwrap()
    }

    fn cast_ward(g: &mut Game, city: hecs::Entity, id: &str) -> Vec<Event> {
        let spell = g.content.rules.spells.id_of("spell_ward").unwrap();
        g.magic_mut_for_tests(P0).known.insert(spell);
        g.magic_mut_for_tests(P0).skill_left = 500;
        let realm = realm(g, id);
        g.apply(Command::CastSpell {
            player: P0,
            spell,
            target: Some(Target::CityRealm(city, realm)),
        })
    }

    #[test]
    fn a_spell_ward_guards_one_realm_each_time_and_costs_upkeep_for_each() {
        let mut g = world();
        let city = found(&mut g, P0, at(10, 6), 4);
        let other = found(&mut g, P1, at(18, 6), 4);
        let ward = g.content.rules.spells.id_of("spell_ward").unwrap();
        // Only your own city, and one of the five realms.
        for (c, r) in [(other, "death"), (city, "arcane")] {
            let realm = realm(&g, r);
            assert_eq!(
                g.check_target(P0, ward, Some(Target::CityRealm(c, realm))),
                Err(crate::magic::MagicError::NoSuchTarget)
            );
        }
        assert_eq!(
            g.check_target(P0, ward, Some(Target::City(city))),
            Err(crate::magic::MagicError::NoSuchTarget),
            "a city alone is not enough"
        );
        cast_ward(&mut g, city, "death");
        assert_eq!(g.city_wards(city), vec!["death".to_string()]);
        let upkeep = |g: &Game| {
            g.upkeep_items(P0)
                .iter()
                .filter(|i| matches!(i.kind, crate::cast::UpkeepKind::City(c, _) if c == city))
                .map(|i| i.mana)
                .sum::<i32>()
        };
        assert_eq!(upkeep(&g), 5);
        let death = realm(&g, "death");
        assert_eq!(
            g.check_target(P0, ward, Some(Target::CityRealm(city, death))),
            Err(crate::magic::MagicError::NoSuchTarget),
            "already warded against death"
        );
        cast_ward(&mut g, city, "chaos");
        assert_eq!(
            g.city_wards(city),
            vec!["death".to_string(), "chaos".into()]
        );
        assert_eq!(upkeep(&g), 10, "five for each ward");
        // Lose the spell and the wards go with it; a new one starts afresh.
        g.world.get::<&mut Enchantments>(city).unwrap().0.clear();
        assert!(g.city_wards(city).is_empty());
        cast_ward(&mut g, city, "life");
        assert_eq!(g.city_wards(city), vec!["life".to_string()]);
    }

    #[test]
    fn a_spell_ward_turns_away_spells_of_its_realm_and_creatures_of_it() {
        let mut g = world();
        let city = found(&mut g, P0, at(10, 6), 4);
        cast_ward(&mut g, city, "death");
        // P1's Evil Presence is a death spell; Chaos Rift is chaos and gets through.
        for (name, fizzles) in [("evil_presence", true), ("chaos_rift", false)] {
            let spell = g.content.rules.spells.id_of(name).unwrap();
            g.magic_mut_for_tests(P1).known.insert(spell);
            g.magic_mut_for_tests(P1).skill_left = 500;
            let events = g.apply(Command::CastSpell {
                player: P1,
                spell,
                target: Some(Target::City(city)),
            });
            assert_eq!(
                events
                    .iter()
                    .any(|e| matches!(e, Event::SpellFizzled { .. })),
                fizzles,
                "{name}"
            );
            assert_eq!(g.city_has_enchantment(city, name), !fizzles, "{name}");
        }
        // A death creature cannot step into the city; a chaos one can.
        let mut units = g.content.rules.units.defs().to_vec();
        for u in units.iter_mut() {
            match u.id.as_str() {
                "walker" => u.realm = Some("death".into()),
                "slow" => u.realm = Some("chaos".into()),
                _ => {}
            }
        }
        g.content.rules.units = Registry::new(units);
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let slow = g.content.rules.units.id_of("slow").unwrap();
        let ghoul = g.spawn_unit(P1, at(11, 6), walker);
        let imp = g.spawn_unit(P1, at(11, 7), slow);
        assert_eq!(
            g.move_stack(&[ghoul], at(10, 6)),
            Err(crate::movement::MoveError::Warded)
        );
        assert!(g.move_stack(&[imp], at(10, 6)).is_ok());
        // The battle at the city knows the wards: its attackers cannot cast death spells.
        let defender = g.spawn_unit(P0, at(10, 6), walker);
        g.start_battle(&[ghoul], &[defender]).unwrap();
        let b = g.battle().unwrap();
        assert_eq!(b.warded_realms, vec!["death".to_string()]);
        assert!(b.ward_stops(crate::battle::Side::Attacker, "death"));
        assert!(!b.ward_stops(crate::battle::Side::Defender, "death"));
        assert!(!b.ward_stops(crate::battle::Side::Attacker, "chaos"));
    }

    #[test]
    fn wards_survive_a_save() {
        let mut g = world();
        let city = found(&mut g, P0, at(10, 6), 4);
        cast_ward(&mut g, city, "death");
        cast_ward(&mut g, city, "life");
        let content = g.content.clone();
        let back = g.snapshot().restore(content).unwrap();
        let again = back.city_at(at(10, 6)).unwrap();
        assert_eq!(
            back.city_wards(again),
            vec!["death".to_string(), "life".to_string()]
        );
    }

    #[test]
    fn evil_presence_is_refused_where_its_owner_can_use_death_magic() {
        let mut g = world();
        let city = found(&mut g, P0, at(10, 6), 4);
        let spell = g.content.rules.spells.id_of("evil_presence").unwrap();
        assert_eq!(g.check_target(P1, spell, Some(Target::City(city))), Ok(()));
        g.magic_mut_for_tests(P0).books.insert("death".into(), 1);
        assert_eq!(
            g.check_target(P1, spell, Some(Target::City(city))),
            Err(crate::magic::MagicError::NoSuchTarget)
        );
    }
}
