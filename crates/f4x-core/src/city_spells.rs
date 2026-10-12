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
use crate::economy::{Buildings, Economy, Enchantments};
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
        self.economy().city_has_enchantment(city, name)
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
        self.economy().evil_presence_is_wasted_on(city)
    }
}

impl Economy<'_> {
    /// Whether the city carries the named lasting spell, whoever cast it.
    pub fn city_has_enchantment(&self, city: hecs::Entity, name: &str) -> bool {
        self.content.rules.spells.id_of(name).is_some_and(|id| {
            self.world
                .get::<&Enchantments>(city)
                .is_ok_and(|e| e.0.iter().any(|(s, _)| *s == id))
        })
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
mod tests;
