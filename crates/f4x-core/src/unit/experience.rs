//! Experience at the turn change.
//!
//! The level itself is worked out by [`World::unit_level`]: experience gives up to Elite (a
//! hero's Demi-god), Heroism lifts a unit to Elite, and the Warlord retort and the Crusade spell
//! add a level each, which is the only way for a normal unit into Ultra-Elite and Champion.
//! Here the turn change does two things:
//! - Heroism ends on a unit that reaches Elite by itself, as the help says;
//! - a hero with the Armsmaster ability trains the other units on its square: each gets 2 xp (3
//!   with Super Armsmaster) per level the hero has, counting from 1. When several heroes with the
//!   ability share a square, the best of them counts. (The ability is there when the unit's
//!   definition lists it.)
//!
//! Every living normal unit and hero also gains 1 xp each turn, up to the last level experience
//! gives (Elite; a hero's Demi-god): `Do_All_Units_XP_Check` in the original. Creatures and the
//! undead gain none. A won battle gives xp too (`battle/spoils.rs`).

use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::Ability;
use f4x_def::content::Content;

use crate::rules::UnitKind;
use crate::unit::{Experience, UnitView, HEROISM};
use crate::world::World;

impl UnitView<'_> {
    /// Experience a hero with Armsmaster gives its stack each turn (0 without the ability).
    pub fn armsmaster_xp(&self, hero: hecs::Entity) -> u32 {
        let abilities = &self.content.rules.config.abilities;
        let tier = if self.unit_has_ability(hero, Ability::new(abilities.super_armsmaster)) {
            f4x_def::hero::Tier::Super
        } else if self.unit_has_ability(hero, Ability::new(abilities.armsmaster)) {
            f4x_def::hero::Tier::Normal
        } else {
            return 0;
        };
        self.content
            .rules
            .hero_value("armsmaster", tier, self.unit_level(hero)) as u32
    }

    /// The experience a unit has after `amount` more: only a living normal unit or hero (not
    /// the undead, not creatures) gains, and only up to what experience can give. `None` when
    /// nothing changes.
    pub(crate) fn xp_after(&self, unit: hecs::Entity, amount: u32) -> Option<u32> {
        let def = self.unit_def(unit)?;
        if !matches!(def.kind, UnitKind::Normal | UnitKind::Hero) || self.unit_is_undead(unit) {
            return None;
        }
        let cap = def
            .level_path
            .map(|p| self.content.rules.level_paths.get(p))
            .and_then(|p| p.levels.last())
            .map(|l| l.xp)?;
        let xp = self.ecs.get::<&Experience>(unit).map_or(0, |x| x.0);
        (xp < cap).then(|| (xp + amount).min(cap))
    }
}

/// `amount` xp for a living normal unit or hero, up to what experience can give: the turn's
/// one point, or a won battle's share (`battle/spoils.rs`).
pub(crate) fn gain_xp(ecs: &mut hecs::World, content: &Content, unit: hecs::Entity, amount: u32) {
    if let Some(xp) = UnitView::new(ecs, content).xp_after(unit, amount) {
        crate::world::put_in(ecs, unit, Experience(xp));
    }
}

/// The turn change for experience: a point for everyone, Heroism ending where it has done its
/// work, and the Armsmasters training their squares.
pub(crate) fn process(ecs: &mut hecs::World, content: &Content, only: Option<PlayerId>) {
    let mut units = UnitView::new(ecs, content).all_units();
    // While a wizard's Time Stop lasts, only their units gain experience.
    if let Some(keeper) = only {
        let view = UnitView::new(ecs, content);
        units.retain(|&u| view.unit_owner(u) == Some(keeper));
    }
    for &unit in &units {
        gain_xp(ecs, content, unit, content.rules.config.renown.xp_per_turn);
    }
    for &unit in &units {
        let done = {
            let view = UnitView::new(ecs, content);
            view.unit_has_heroism(unit)
                && view.unit_xp_level(unit) >= content.rules.config.renown.heroism_level
        };
        if done {
            crate::spell::instance::remove_on(
                ecs,
                crate::spell::instance::SpellTarget::Unit(unit),
                |s, _| content.rules.spells.get(s).has_hook(HEROISM),
            );
        }
    }
    let view = UnitView::new(ecs, content);
    let mut best: Vec<(Position, PlayerId, u32)> = Vec::new();
    for &unit in &units {
        let gain = view.armsmaster_xp(unit);
        let (Some(at), Some(owner)) = (view.unit_position(unit), view.unit_owner(unit)) else {
            continue;
        };
        if gain == 0
            || !view
                .unit_def(unit)
                .is_some_and(|d| d.kind == UnitKind::Hero)
        {
            continue;
        }
        match best.iter_mut().find(|b| b.0 == at && b.1 == owner) {
            Some(b) => b.2 = b.2.max(gain),
            None => best.push((at, owner, gain)),
        }
    }
    let mut trained = Vec::new();
    for (at, owner, gain) in best {
        for unit in view.units_at(at) {
            let trainee = view.unit_owner(unit) == Some(owner)
                && view
                    .unit_def(unit)
                    .is_some_and(|d| d.kind != UnitKind::Hero && d.level_path.is_some());
            if trainee {
                let xp = view.ecs.get::<&Experience>(unit).map_or(0, |x| x.0);
                trained.push((unit, xp + gain));
            }
        }
    }
    // Several squares are worked out before any is paid, so the same unit is never trained
    // twice from a stale reading: each unit stands on one square and belongs to one owner.
    for (unit, xp) in trained {
        crate::world::put_in(ecs, unit, Experience(xp));
    }
}

impl World {
    /// `amount` xp for a living normal unit or hero (see [`gain_xp`]).
    pub(crate) fn gain_xp(&mut self, unit: hecs::Entity, amount: u32) {
        gain_xp(&mut self.ecs, &self.content, unit, amount);
    }

    pub(crate) fn process_experience(&mut self, player: crate::player::PlayerId) {
        // While a wizard's Time Stop lasts, only their units gain experience.
        if self.time_stopper().is_some_and(|keeper| keeper != player) {
            return;
        }
        process(&mut self.ecs, &self.content, Some(player));
    }
}
