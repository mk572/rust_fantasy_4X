//! The realms an attack or spell can belong to, and their conversions.
//!
//! Tests: in `f4x-core`'s `battle/immunity/tests.rs`, on the shipped content.

use super::damage_type::DamageType;

/// The realms an attack or spell can belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Realm {
    Nature,
    Sorcery,
    Chaos,
    Life,
    Death,
    Arcane,
}

impl Realm {
    pub fn from_num(num: u16) -> Option<Realm> {
        Some(match num {
            0 => Realm::Nature,
            1 => Realm::Sorcery,
            2 => Realm::Chaos,
            3 => Realm::Life,
            4 => Realm::Death,
            5 => Realm::Arcane,
            _ => return None,
        })
    }

    /// The realm of a definition's [`RealmId`].
    pub fn of(id: super::RealmId) -> Option<Realm> {
        Realm::from_num(id.num())
    }

    /// The realm's number in `realms.ron`.
    pub fn num(self) -> u16 {
        self as u16
    }

    /// The realm as a damage type.
    pub fn damage_type(self) -> DamageType {
        match self {
            Realm::Nature => DamageType::NatureRealm,
            Realm::Sorcery => DamageType::SorceryRealm,
            Realm::Chaos => DamageType::ChaosRealm,
            Realm::Life => DamageType::LifeRealm,
            Realm::Death => DamageType::DeathRealm,
            Realm::Arcane => DamageType::ArcaneRealm,
        }
    }

    /// The realm's id.
    pub fn id(self) -> super::RealmId {
        super::RealmId::new(self.num())
    }
}
