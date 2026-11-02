//! City enchantments whose effects are more than stat changes. Most are read by name
//! from the city's enchantment list where they matter (`city/economy.rs` for unrest, `battle/` for
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
//!
//! Tests: in spell/city/tests.rs.

use crate::city::economy::{Buildings, Economy};
use crate::city::production::ProductionItem;
use crate::game::{Event, Game};
use crate::player::PlayerId;
use crate::rules::RealmId;
use crate::rules::SpellId;
use crate::spell::target_error::TargetError;

/// The hook of the city enchantments the engine reads by name: casting only enchants the city.
pub const CITY_FLAG: &str = "city_flag";

/// Chaos Rift's bolts on each unit in the city every turn, and their strength.
const CHAOS_RIFT_BOLTS: u32 = 5;
const CHAOS_RIFT_STRENGTH: i32 = 8;
/// The chance in a hundred that a building falls each turn.
const CHAOS_RIFT_BUILDING_PERCENT: u32 = 5;

/// The realms a Spell Ward can be set against, in the original's order.
pub const WARD_REALMS: [RealmId; 5] = crate::rules::BOOK_REALMS;

/// The realms whose spells Consecration keeps off the city and the units in it.
pub const CONSECRATION_REALMS: [RealmId; 2] = [
    RealmId::new(4), // death
    RealmId::new(2), // chaos
];

/// The realms a city has Spell Wards against (the ward spell itself, with its caster and its
/// upkeep, is in the city's enchantments; this says which realms it was set to). Read through
/// [`Game::city_wards`], which ignores it once the spell is gone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpellWards(pub Vec<RealmId>);

/// The buildings whose magic power Evil Presence destroys.
pub(crate) const RELIGIOUS: [&str; 4] = ["shrine", "temple", "parthenon", "cathedral"];

/// City Walls (building 35).
const CITY_WALLS: crate::rules::BuildingId = crate::rules::BuildingId::new(35);

impl Game {
    /// Whether the city may be given walls by Wall of Stone: it has none (a city with walls
    /// built or replaced is refused).
    pub fn check_raise_wall_of_stone(&self, city: hecs::Entity) -> Result<(), TargetError> {
        if self.economy().city_has_building(city, CITY_WALLS) {
            return Err(TargetError::Walled);
        }
        Ok(())
    }

    /// Wall of Stone: the city gets City Walls.
    pub(crate) fn wall_of_stone(&mut self, city: hecs::Entity) -> Vec<Event> {
        let walls = CITY_WALLS;
        if self.check_raise_wall_of_stone(city).is_err() {
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
            .economy()
            .cities()
            .into_iter()
            .map(|(city, _, _)| city)
            .filter(|&c| {
                self.economy()
                    .city_has_enchantment(c, SpellId::new(110) /* chaos_rift */)
            })
            .collect();
        for city in rifts {
            let Ok(at) = self.world.get::<&crate::map::Position>(city).map(|p| *p) else {
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
            for unit in self.units().units_at(at) {
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
                    .filter(|&b| !self.economy().has_dependants(city, b))
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

    /// Whether a Spell Ward against `realm` can be put on the city: it is one of the five realms
    /// and not warded yet.
    pub fn check_ward(
        &self,
        city: hecs::Entity,
        realm: crate::rules::RealmId,
    ) -> Result<(), TargetError> {
        if !WARD_REALMS.contains(&realm) {
            return Err(TargetError::NotWardRealm);
        }
        if self.economy().city_warded_against(city, realm) {
            return Err(TargetError::Warded);
        }
        Ok(())
    }

    /// Whether every realm a Spell Ward can name is warded already.
    pub fn fully_warded(&self, city: hecs::Entity) -> bool {
        WARD_REALMS
            .iter()
            .all(|&r| self.economy().city_warded_against(city, r))
    }

    /// Set a Spell Ward against `realm` on the city. The first ward of a city starts its list
    /// afresh, so wards left over from a spell that was dispelled or lost with the city are gone.
    pub fn add_spell_ward(&mut self, city: hecs::Entity, realm: crate::rules::RealmId) {
        let mut wards = self.economy().city_wards(city);
        if !wards.contains(&realm) {
            wards.push(realm);
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
            (SpellTarget::EnemyCity, Target::City(c))
                if self.economy().city_owner(c) != Some(player) =>
            {
                c
            }
            (SpellTarget::EnemyUnit, Target::Unit(u)) => {
                let at = self.units().unit_position(u)?;
                self.economy()
                    .city_at(at)
                    .filter(|&c| self.economy().city_owner(c) != Some(player))?
            }
            (SpellTarget::Tile, Target::Tile(at)) if self.is_square_spell(spell) => self
                .economy()
                .city_at(at)
                .filter(|&c| self.economy().city_owner(c) != Some(player))?,
            _ => return None,
        };
        self.economy()
            .city_shielded_realms(city)
            .iter()
            .any(|&r| Some(r) == def.realm.id())
            .then_some(city)
    }

    /// How many times the city's enchantment `spell` is paid for each turn: a Spell Ward costs
    /// its upkeep for every realm it guards.
    pub(crate) fn upkeep_count(&self, city: hecs::Entity, spell: crate::rules::SpellId) -> i32 {
        if spell == crate::rules::SpellId::new(72)
        /* spell_ward */
        {
            (self.economy().city_wards(city).len() as i32).max(1)
        } else {
            1
        }
    }
}

impl Economy<'_> {
    /// Whether the city carries the lasting spell, whoever cast it.
    pub fn city_has_enchantment(&self, city: hecs::Entity, spell: SpellId) -> bool {
        crate::spell::instance::Spells::new(self.world)
            .city_enchantments(city)
            .iter()
            .any(|&(s, _)| s == spell)
    }

    /// Evil Presence does nothing to a city whose owner can use death magic, and the original
    /// refuses to cast it there.
    pub fn evil_presence_is_wasted_on(&self, city: hecs::Entity) -> bool {
        self.city_owner(city)
            .and_then(|o| self.wizard(o))
            .is_some_and(|m| m.books_in(RealmId::new(4) /* death */) > 0)
    }

    /// Whether Evil Presence is destroying the city's religious buildings' power: the spell is
    /// on the city and its owner cannot use death magic.
    pub(crate) fn evil_presence_active(&self, city: hecs::Entity) -> bool {
        self.city_has_enchantment(city, SpellId::new(183) /* evil_presence */)
            && !self.evil_presence_is_wasted_on(city)
    }
}

impl Economy<'_> {
    /// The realms the city is warded against.
    pub fn city_wards(&self, city: hecs::Entity) -> Vec<RealmId> {
        if !self.city_has_enchantment(city, SpellId::new(72) /* spell_ward */) {
            return Vec::new();
        }
        self.world
            .get::<&SpellWards>(city)
            .map(|w| w.0.clone())
            .unwrap_or_default()
    }

    /// Whether the city is warded against spells of `realm`.
    pub fn city_warded_against(&self, city: hecs::Entity, realm: RealmId) -> bool {
        self.city_wards(city).contains(&realm)
    }

    /// The realms whose spells cannot be cast on the city or the units in it: the Spell Ward's,
    /// and death and chaos under Consecration.
    pub fn city_shielded_realms(&self, city: hecs::Entity) -> Vec<RealmId> {
        let mut realms = self.city_wards(city);
        if self.city_has_enchantment(city, SpellId::new(155) /* consecration */) {
            for realm in CONSECRATION_REALMS {
                if !realms.contains(&realm) {
                    realms.push(realm);
                }
            }
        }
        realms
    }

    /// Whether a Spell Ward on the city keeps the unit out: it is a fantastic creature (or a
    /// summoned one) of a realm the city is warded against. The help says "any": the owner's own
    /// creatures too. (Not in the original's code, which never reads the wards for movement.)
    pub fn ward_bars_creature(&self, city: hecs::Entity, unit: hecs::Entity) -> bool {
        use crate::rules::UnitKind;
        let Some(def) = self.units().unit_def(unit) else {
            return false;
        };
        matches!(def.kind, UnitKind::Fantastic | UnitKind::Summoned)
            && def
                .realm
                .as_ref()
                .and_then(|r| r.id())
                .is_some_and(|r| self.city_warded_against(city, r))
    }
}

#[cfg(test)]
mod tests;
