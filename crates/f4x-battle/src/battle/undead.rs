//! Who comes back as undead once a battle is over: the winner's Zombie Mastery turns the normal
//! units that fell on either side (and were not destroyed) into zombies, and a unit drained to
//! death by Life Steal or a creator of undead joins the winner. The world makes the units undead
//! and moves them; what it must tell the battle is the kind of the overland unit, whether the
//! winner holds Zombie Mastery, and what the winner's units cost.
//!
//! Tests: in f4x-core's battle/tests.rs, fought on real battles.

use f4x_def::rules::damage_type::DamageType;
use f4x_def::rules::realm::Realm;
use f4x_def::rules::{Rules, UnitKind};

use super::Battle;
use crate::unit::{BattleUnit, Fate};

impl Battle {
    /// Whether the winner's Zombie Mastery (`zombie_mastery`) turns this dead unit, of overland
    /// kind `kind`, into a zombie: any normal unit that fell on either side but was not
    /// destroyed and is not immune to Death.
    pub fn is_zombified(
        &self,
        u: &BattleUnit,
        kind: Option<UnitKind>,
        zombie_mastery: bool,
        rules: &Rules,
    ) -> bool {
        self.winner.is_some()
            && u.fate() != Fate::Gone
            && !u.is_immune_to(&[DamageType::Death], rules)
            && kind == Some(UnitKind::Normal)
            && zombie_mastery
    }

    /// Whether the winner takes this dead unit as an undead one: the loser's, drained to death
    /// (life steal, a creator of undead), not a creature of the Death realm and not a hero; or
    /// a zombie ([`Self::is_zombified`]).
    pub fn is_raised_by(
        &self,
        u: &BattleUnit,
        kind: Option<UnitKind>,
        zombie_mastery: bool,
        rules: &Rules,
    ) -> bool {
        let Some(winner) = self.winner else {
            return false;
        };
        self.is_zombified(u, kind, zombie_mastery, rules)
            || (winner != u.side
                && u.fate() == Fate::Drained
                && u.realm != Some(Realm::Death)
                && !u.is_immune_to(&[DamageType::Death], rules)
                && kind.is_some_and(|k| k != UnitKind::Hero))
    }

    /// The winner's unit that made the undead: one with Life Steal or Create Undead, the costliest
    /// of them. `cost_of` gives the cost of the overland unit a battle unit was copied from, by
    /// the index in `BattleUnit::source`; `None` leaves the unit out. Returns that index.
    pub fn undead_creator<C: Ord>(&self, cost_of: impl Fn(usize) -> Option<C>) -> Option<usize> {
        let winner = self.winner?;
        self.units
            .iter()
            .filter(|u| u.side == winner)
            .filter(|u| {
                let special = u.special_attacks();
                special.life_steal || special.create_undead
            })
            .filter_map(|u| u.source)
            .filter_map(|i| Some((i, cost_of(i)?)))
            .max_by(|a, b| a.1.cmp(&b.1))
            .map(|(i, _)| i)
    }
}
