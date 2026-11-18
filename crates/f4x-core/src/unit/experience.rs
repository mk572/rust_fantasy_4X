//! Experience at the turn change.
//!
//! The level itself is worked out by [`Game::unit_level`]: experience gives up to Elite (a
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

use crate::game::Game;
use crate::rules::UnitKind;
use crate::unit::{Experience, UnitView, HEROISM};
use hecs::World;

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
        let xp = self.world.get::<&Experience>(unit).map_or(0, |x| x.0);
        (xp < cap).then(|| (xp + amount).min(cap))
    }
}

/// `amount` xp for a living normal unit or hero, up to what experience can give: the turn's
/// one point, or a won battle's share (`battle/spoils.rs`).
pub(crate) fn gain_xp(world: &mut World, content: &Content, unit: hecs::Entity, amount: u32) {
    if let Some(xp) = UnitView::new(world, content).xp_after(unit, amount) {
        crate::game::put_in(world, unit, Experience(xp));
    }
}

/// The turn change for experience: a point for everyone, Heroism ending where it has done its
/// work, and the Armsmasters training their squares.
pub(crate) fn process(world: &mut World, content: &Content, only: Option<PlayerId>) {
    let mut units = UnitView::new(world, content).all_units();
    // While a wizard's Time Stop lasts, only their units gain experience.
    if let Some(keeper) = only {
        let view = UnitView::new(world, content);
        units.retain(|&u| view.unit_owner(u) == Some(keeper));
    }
    for &unit in &units {
        gain_xp(
            world,
            content,
            unit,
            content.rules.config.renown.xp_per_turn,
        );
    }
    for &unit in &units {
        let done = {
            let view = UnitView::new(world, content);
            view.unit_has_heroism(unit)
                && view.unit_xp_level(unit) >= content.rules.config.renown.heroism_level
        };
        if done {
            crate::spell::instance::remove_on(
                world,
                crate::spell::instance::SpellTarget::Unit(unit),
                |s, _| content.rules.spells.get(s).has_hook(HEROISM),
            );
        }
    }
    let view = UnitView::new(world, content);
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
                let xp = view.world.get::<&Experience>(unit).map_or(0, |x| x.0);
                trained.push((unit, xp + gain));
            }
        }
    }
    // Several squares are worked out before any is paid, so the same unit is never trained
    // twice from a stale reading: each unit stands on one square and belongs to one owner.
    for (unit, xp) in trained {
        crate::game::put_in(world, unit, Experience(xp));
    }
}

impl Game {
    /// `amount` xp for a living normal unit or hero (see [`gain_xp`]).
    pub(crate) fn gain_xp(&mut self, unit: hecs::Entity, amount: u32) {
        gain_xp(&mut self.world, &self.content, unit, amount);
    }

    pub(crate) fn process_experience(&mut self, player: crate::player::PlayerId) {
        // While a wizard's Time Stop lasts, only their units gain experience.
        if self.time_stopper().is_some_and(|keeper| keeper != player) {
            return;
        }
        process(&mut self.world, &self.content, Some(player));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Plane;
    use crate::rules::RealmId;
    use crate::rules::Stat;
    use crate::wizard::magic::Retort;
    use f4x_def::testdir::TestResult;

    const P0: PlayerId = PlayerId(0);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    /// The hero world (the game's real units, level paths and heroes) with a life wizard.
    fn world() -> Game {
        let mut g = crate::hero::game::tests::world();
        g.treasury_mut(P0).expect("treasury_mut succeeds").mana =
            f4x_def::half_int::HalfInt::from_int(5000);
        g.init_wizard(P0, &[(RealmId::new(3 /* life */), 4)], &[], None)
            .expect("init_wizard succeeds");
        g
    }

    /// Hire the hero of this unit.
    fn hire(g: &mut Game, unit: u16, pos: Position) -> hecs::Entity {
        let kind = crate::hero::game::tests::kind_of(g, unit);
        g.spawn_hero(P0, pos, kind).expect("spawn_hero succeeds")
    }

    fn spawn(g: &mut Game, id: u16, pos: Position) -> hecs::Entity {
        let def = g
            .content
            .rules
            .units
            .id_numbered(id)
            .expect("id_of succeeds");
        g.spawn_unit(P0, pos, def)
    }

    fn set_xp(g: &mut Game, unit: hecs::Entity, xp: u32) {
        g.world
            .insert_one(unit, Experience(xp))
            .expect("insert_one succeeds");
    }

    fn enchant(g: &mut Game, spell: u16, who: crate::rules::exec::Target) {
        let s = g
            .content
            .rules
            .spells
            .id_numbered(spell)
            .expect("id_of succeeds");
        g.attach_enchantment(who, s, P0);
    }

    /// The experience that gives `level` (from 0) on the unit's level path.
    fn xp_of_level(g: &Game, unit: hecs::Entity, level: usize) -> u32 {
        let path = g
            .units()
            .unit_def(unit)
            .and_then(|d| d.level_path)
            .map(|p| g.content.rules.level_paths.get(p))
            .expect("a level path");
        path.levels[level.min(path.levels.len() - 1)].xp
    }

    fn xp(g: &Game, unit: hecs::Entity) -> u32 {
        g.world.get::<&Experience>(unit).map_or(0, |x| x.0)
    }

    #[test]
    fn a_levels_attack_plus_only_counts_for_a_unit_that_has_the_attack() {
        let mut g = world();
        // The Steam Cannon has no melee attack.
        let unit = spawn(&mut g, 80 /* dwarven_steam_cannon */, at(3, 3));
        set_xp(&mut g, unit, 120);
        assert_eq!(g.units().unit_level(unit), 3);
        assert_eq!(g.units().unit_stat(unit, Stat::Melee), 0);
    }

    #[test]
    fn warlord_and_crusade_each_add_a_level_and_only_they_reach_the_top() -> TestResult {
        let mut g = world();
        let unit = spawn(&mut g, 107 /* high_men_cavalry */, at(3, 3));
        // The real path adds a point of melee at Regular and at Elite, and at Champion.
        let base = g.units().unit_stat(unit, Stat::Melee);
        set_xp(&mut g, unit, 120);
        assert_eq!(g.units().unit_level(unit), 3, "experience stops at elite");
        assert_eq!(g.units().unit_stat(unit, Stat::Melee), base + 2);
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .get_mut::<crate::wizard::sheet::Retorts>()
            .0
            .insert(Retort::new(1) /* warlord */);
        assert_eq!(g.units().unit_level(unit), 4, "warlord: ultra-elite");
        assert_eq!(g.units().unit_stat(unit, Stat::Melee), base + 2);
        enchant(
            &mut g,
            158, /* crusade */
            crate::rules::exec::Target::Player(P0),
        );
        assert_eq!(g.units().unit_level(unit), 5, "both: champion");
        assert_eq!(g.units().unit_stat(unit, Stat::Melee), base + 3);
        // A recruit gets the two levels too, and nothing lifts anyone past champion.
        let recruit = spawn(&mut g, 107 /* high_men_cavalry */, at(4, 3));
        assert_eq!(g.units().unit_level(recruit), 2);
        set_xp(&mut g, recruit, 1000);
        assert_eq!(g.units().unit_level(recruit), 5);
        Ok(())
    }

    #[test]
    fn crusade_leaves_the_undead_and_creatures_alone_but_warlord_does_not() -> TestResult {
        let mut g = world();
        let normal = spawn(&mut g, 107 /* high_men_cavalry */, at(3, 3));
        let creature = spawn(&mut g, 154 /* magic_spirit */, at(4, 3));
        let undead = spawn(&mut g, 107 /* high_men_cavalry */, at(5, 3));
        g.make_undead(undead);
        enchant(
            &mut g,
            158, /* crusade */
            crate::rules::exec::Target::Player(P0),
        );
        assert_eq!(g.units().unit_level(normal), 1);
        assert_eq!(g.units().unit_level(undead), 0, "not for the undead");
        assert_eq!(g.units().unit_level(creature), 0, "nor for creatures");
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .get_mut::<crate::wizard::sheet::Retorts>()
            .0
            .insert(Retort::new(1) /* warlord */);
        assert_eq!(g.units().unit_level(undead), 1, "warlord is for every unit");
        Ok(())
    }

    #[test]
    fn heroism_makes_a_unit_elite_and_ends_when_it_gets_there_by_itself() -> TestResult {
        let mut g = world();
        let lifted = g.content.rules.config.renown.heroism_level;
        let unit = spawn(&mut g, 107 /* high_men_cavalry */, at(3, 3));
        // A twin that has the experience of that level, to compare with.
        let twin = spawn(&mut g, 107 /* high_men_cavalry */, at(4, 3));
        let there = xp_of_level(&g, unit, lifted);
        set_xp(&mut g, twin, there);
        enchant(
            &mut g,
            130, /* heroism */
            crate::rules::exec::Target::Unit(unit),
        );
        assert!(g.units().unit_has_heroism(unit));
        assert_eq!(g.units().unit_level(unit), lifted);
        assert_eq!(
            g.units().unit_stat(unit, Stat::Melee),
            g.units().unit_stat(twin, Stat::Melee)
        );
        // Warlord goes on top of the floor.
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .get_mut::<crate::wizard::sheet::Retorts>()
            .0
            .insert(Retort::new(1) /* warlord */);
        assert_eq!(g.units().unit_level(unit), lifted + 1);
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .get_mut::<crate::wizard::sheet::Retorts>()
            .0
            .clear();
        // Not yet the experience of that level: the spell stays.
        let below = xp_of_level(&g, unit, lifted - 1);
        set_xp(&mut g, unit, below);
        g.process_experience_all();
        assert!(g.units().unit_has_heroism(unit));
        // One that reached it by experience no longer needs the spell.
        set_xp(&mut g, unit, there);
        g.process_experience_all();
        assert!(!g.units().unit_has_heroism(unit));
        assert_eq!(g.units().unit_level(unit), lifted);
        Ok(())
    }

    #[test]
    fn an_armsmaster_trains_the_units_on_its_square() {
        let mut g = world();
        let hero = hire(&mut g, 26 /* swordsman */, at(3, 3));
        let near = spawn(&mut g, 107 /* high_men_cavalry */, at(3, 3));
        let far = spawn(&mut g, 107 /* high_men_cavalry */, at(4, 3));
        // The real swordsman is an armsmaster; a hero's tier decides the xp per level.
        let abilities = g.content.rules.config.abilities.clone();
        let tier = if g
            .units()
            .unit_has_ability(hero, Ability::new(abilities.super_armsmaster))
        {
            f4x_def::hero::Tier::Super
        } else {
            f4x_def::hero::Tier::Normal
        };
        assert!(
            g.units()
                .unit_has_ability(hero, Ability::new(abilities.armsmaster))
                || tier == f4x_def::hero::Tier::Super
        );
        let level = g.units().unit_level(hero);
        let first = g.units().armsmaster_xp(hero);
        assert_eq!(
            first,
            g.content.rules.hero_value("armsmaster", tier, level) as u32
        );
        let cap = xp_of_level(&g, hero, usize::MAX);
        set_xp(&mut g, hero, cap);
        let top = g.units().armsmaster_xp(hero);
        assert!(top > first, "a higher level trains more");
        // Each turn a unit also gains its own xp.
        let turn = g.content.rules.config.renown.xp_per_turn;
        g.process_experience_all();
        assert_eq!(xp(&g, near), turn + top);
        assert_eq!(xp(&g, far), turn);
        assert_eq!(
            xp(&g, hero),
            cap,
            "the hero trains no one but itself, and has the most experience there is"
        );
        // Another hero on the same square: the best trainer counts, not the sum.
        let _other = hire(&mut g, 0 /* dwarf */, at(3, 3));
        let before = xp(&g, near);
        g.process_experience_all();
        assert_eq!(
            xp(&g, near),
            before + turn + top.max(g.units().armsmaster_xp(_other))
        );
    }

    #[test]
    fn living_normal_units_and_heroes_gain_xp_each_turn_up_to_their_cap() {
        let mut g = world();
        let turn = g.content.rules.config.renown.xp_per_turn;
        let normal = spawn(&mut g, 107 /* high_men_cavalry */, at(3, 3));
        let creature = spawn(&mut g, 154 /* magic_spirit */, at(4, 3));
        let undead = spawn(&mut g, 107 /* high_men_cavalry */, at(5, 3));
        g.make_undead(undead);
        let hero = hire(&mut g, 26 /* swordsman */, at(6, 3));
        g.process_experience_all();
        assert_eq!(xp(&g, normal), turn);
        assert_eq!(xp(&g, hero), turn);
        assert_eq!(xp(&g, creature), 0, "creatures gain none");
        assert_eq!(xp(&g, undead), 0, "nor the undead");
        let cap = g
            .content
            .rules
            .level_paths
            .get(
                g.units()
                    .unit_def(normal)
                    .and_then(|d| d.level_path)
                    .expect("a level path"),
            )
            .levels
            .last()
            .map(|l| l.xp)
            .expect("levels");
        set_xp(&mut g, normal, cap - 1);
        g.process_experience_all();
        g.process_experience_all();
        assert_eq!(
            xp(&g, normal),
            cap,
            "the top level is where experience stops"
        );
    }
}
