//! Experience at the turn change (task 27).
//!
//! The level itself is worked out by [`Game::unit_level`]: experience gives up to Elite (a
//! hero's Demi-god), Heroism lifts a unit to Elite, and the Warlord retort and the Crusade spell
//! add a level each, which is the only way for a normal unit into Ultra-Elite and Champion.
//! Here the turn change does two things:
//! - Heroism ends on a unit that reaches Elite by itself, as the help says;
//! - a hero with the Armsmaster ability trains the other units on its square: each gets 2 xp (3
//!   with Super Armsmaster) per level the hero has, counting from 1. When several heroes with the
//!   ability share a square, the best of them counts. (Heroes' abilities are task 47; until it
//!   hands them out, the ability is only there when a unit's definition lists it.)
//!
//! Every living normal unit and hero also gains 1 xp each turn, up to the last level experience
//! gives (Elite; a hero's Demi-god): `Do_All_Units_XP_Check` in the original. Creatures and the
//! undead gain none. A won battle gives xp too (`battle/spoils.rs`).

use crate::content::Content;
use crate::map::Position;
use crate::player::PlayerId;

use crate::game::Game;
use crate::rules::UnitKind;
use crate::unit::{Experience, UnitView, HEROISM, HEROISM_LEVEL};
use hecs::World;

/// A hero's ability ids (task 47 adds the rest).
pub const ARMSMASTER: &str = "armsmaster";
pub const SUPER_ARMSMASTER: &str = "super_armsmaster";

impl UnitView<'_> {
    /// Experience a hero with Armsmaster gives its stack each turn (0 without the ability).
    pub fn armsmaster_xp(&self, hero: hecs::Entity) -> u32 {
        let per_level = if self.unit_has_ability(hero, SUPER_ARMSMASTER) {
            3
        } else if self.unit_has_ability(hero, ARMSMASTER) {
            2
        } else {
            return 0;
        };
        (self.unit_level(hero) as u32 + 1) * per_level
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
            .as_deref()
            .and_then(|p| self.content.rules.level_paths.by_name(p))
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
        let _ = world.insert_one(unit, Experience(xp));
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
        gain_xp(world, content, unit, 1);
    }
    if let Some(heroism) = content.rules.spells.id_of(HEROISM) {
        for &unit in &units {
            let done = {
                let view = UnitView::new(world, content);
                view.unit_has_heroism(unit) && view.unit_xp_level(unit) >= HEROISM_LEVEL
            };
            if done {
                crate::spell::instance::remove_on(
                    world,
                    crate::spell::instance::SpellTarget::Unit(unit),
                    |s, _| s == heroism,
                );
            }
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
        let _ = world.insert_one(unit, Experience(xp));
    }
}

impl Game {
    /// `amount` xp for a living normal unit or hero (see [`gain_xp`]).
    pub(crate) fn gain_xp(&mut self, unit: hecs::Entity, amount: u32) {
        gain_xp(&mut self.world, &self.content, unit, amount);
    }

    pub(crate) fn process_experience(&mut self) {
        let stopper = self.time_stopper();
        process(&mut self.world, &self.content, stopper);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::content_with_magic;
    use crate::map::{Grid, Map, Plane};
    use crate::rules::effects::{Condition, Effect, EffectRule, TargetSel, Trigger};
    use crate::rules::{BonusLevelDef, LevelDef, LevelPathDef, ModOp, Registry, Stat, UnitKind};
    use crate::wizard::magic::Retort;

    const P0: PlayerId = PlayerId(0);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    /// Every level adds one to the melee attack, so the level shows in the stat.
    fn melee_plus_one() -> Vec<EffectRule> {
        vec![EffectRule {
            when: Trigger::Passive,
            condition: Condition::Always,
            target: TargetSel::This,
            effect: Effect::Modify {
                stat: Stat::Melee,
                op: ModOp::Add,
                value: 1,
            },
        }]
    }

    fn world() -> Game {
        let mut c = content_with_magic();
        let mut units = c.rules.units.defs().to_vec();
        for u in units.iter_mut() {
            match u.id.as_str() {
                "walker" => u.kind = UnitKind::Normal,
                "hardy" => {
                    u.kind = UnitKind::Hero;
                    u.abilities.push(ARMSMASTER.into());
                }
                "scout" => {
                    u.kind = UnitKind::Hero;
                    u.abilities.push(SUPER_ARMSMASTER.into());
                }
                _ => {}
            }
        }
        c.rules.units = Registry::new_sorted(units);
        let level = |name: &str, xp| LevelDef {
            name: name.into(),
            xp,
            effects: melee_plus_one(),
        };
        let bonus = |name: &str| BonusLevelDef {
            name: name.into(),
            effects: melee_plus_one(),
        };
        c.rules.level_paths = Registry::new(vec![LevelPathDef {
            id: "xp".into(),
            levels: vec![
                LevelDef {
                    name: "Recruit".into(),
                    xp: 0,
                    effects: vec![],
                },
                level("Regular", 20),
                level("Veteran", 60),
                level("Elite", 120),
            ],
            bonus: vec![bonus("Ultra-Elite"), bonus("Champion")],
        }]);
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(3);
        g.content = c;
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        g.fog = crate::map::fog::Fog::new(24, 14);
        g.add_player(P0, "W");
        g.treasury_mut(P0).unwrap().mana = 5000;
        g.init_wizard(P0, &[("life", 4)], &[], None).unwrap();
        g
    }

    fn spawn(g: &mut Game, id: &str, pos: Position) -> hecs::Entity {
        let def = g.content.rules.units.id_of(id).unwrap();
        g.spawn_unit(P0, pos, def)
    }

    fn set_xp(g: &mut Game, unit: hecs::Entity, xp: u32) {
        g.world.insert_one(unit, Experience(xp)).unwrap();
    }

    fn enchant(g: &mut Game, spell: &str, who: crate::rules::exec::Target) {
        let s = g.content.rules.spells.id_of(spell).unwrap();
        g.attach_enchantment(who, s, P0);
    }

    fn xp(g: &Game, unit: hecs::Entity) -> u32 {
        g.world.get::<&Experience>(unit).map_or(0, |x| x.0)
    }

    #[test]
    fn a_levels_attack_plus_only_counts_for_a_unit_that_has_the_attack() {
        let mut g = world();
        let mut defs = g.content.rules.units.defs().to_vec();
        for d in defs.iter_mut().filter(|d| d.id == "walker") {
            d.stats.insert(Stat::Melee, 0);
        }
        g.content.rules.units = Registry::new_sorted(defs);
        let unit = spawn(&mut g, "walker", at(3, 3));
        set_xp(&mut g, unit, 120);
        assert_eq!(g.units().unit_level(unit), 3);
        assert_eq!(g.units().unit_stat(unit, Stat::Melee), 0);
    }

    #[test]
    fn warlord_and_crusade_each_add_a_level_and_only_they_reach_the_top() {
        let mut g = world();
        let unit = spawn(&mut g, "walker", at(3, 3));
        set_xp(&mut g, unit, 120);
        assert_eq!(g.units().unit_level(unit), 3, "experience stops at elite");
        assert_eq!(g.units().unit_stat(unit, Stat::Melee), 3 + 3);
        g.wizard_mut(P0)
            .unwrap()
            .get_mut::<crate::wizard::sheet::Retorts>()
            .0
            .insert(Retort::Warlord);
        assert_eq!(g.units().unit_level(unit), 4, "warlord: ultra-elite");
        assert_eq!(g.units().unit_stat(unit, Stat::Melee), 3 + 4);
        enchant(&mut g, "crusade", crate::rules::exec::Target::Player(P0));
        assert_eq!(g.units().unit_level(unit), 5, "both: champion");
        assert_eq!(g.units().unit_stat(unit, Stat::Melee), 3 + 5);
        // A recruit gets the two levels too, and nothing lifts anyone past champion.
        let recruit = spawn(&mut g, "walker", at(4, 3));
        assert_eq!(g.units().unit_level(recruit), 2);
        set_xp(&mut g, recruit, 1000);
        assert_eq!(g.units().unit_level(recruit), 5);
    }

    #[test]
    fn crusade_leaves_the_undead_and_creatures_alone_but_warlord_does_not() {
        let mut g = world();
        let normal = spawn(&mut g, "walker", at(3, 3));
        let creature = spawn(&mut g, "slow", at(4, 3));
        let undead = spawn(&mut g, "walker", at(5, 3));
        g.make_undead(undead);
        enchant(&mut g, "crusade", crate::rules::exec::Target::Player(P0));
        assert_eq!(g.units().unit_level(normal), 1);
        assert_eq!(g.units().unit_level(undead), 0, "not for the undead");
        assert_eq!(g.units().unit_level(creature), 0, "nor for creatures");
        g.wizard_mut(P0)
            .unwrap()
            .get_mut::<crate::wizard::sheet::Retorts>()
            .0
            .insert(Retort::Warlord);
        assert_eq!(g.units().unit_level(undead), 1, "warlord is for every unit");
    }

    #[test]
    fn heroism_makes_a_unit_elite_and_ends_when_it_gets_there_by_itself() {
        let mut g = world();
        let unit = spawn(&mut g, "walker", at(3, 3));
        enchant(&mut g, "heroism", crate::rules::exec::Target::Unit(unit));
        assert!(g.units().unit_has_heroism(unit));
        assert_eq!(g.units().unit_level(unit), 3);
        assert_eq!(g.units().unit_stat(unit, Stat::Melee), 3 + 3);
        // Warlord goes on top of the floor.
        g.wizard_mut(P0)
            .unwrap()
            .get_mut::<crate::wizard::sheet::Retorts>()
            .0
            .insert(Retort::Warlord);
        assert_eq!(g.units().unit_level(unit), 4);
        g.wizard_mut(P0)
            .unwrap()
            .get_mut::<crate::wizard::sheet::Retorts>()
            .0
            .clear();
        // Enough experience for a veteran: the spell stays.
        set_xp(&mut g, unit, 60);
        g.process_experience();
        assert!(g.units().unit_has_heroism(unit));
        // An elite by experience no longer needs it.
        set_xp(&mut g, unit, 120);
        g.process_experience();
        assert!(!g.units().unit_has_heroism(unit));
        assert_eq!(g.units().unit_level(unit), 3);
    }

    #[test]
    fn an_armsmaster_trains_the_units_on_its_square() {
        let mut g = world();
        let hero = spawn(&mut g, "hardy", at(3, 3));
        let near = spawn(&mut g, "walker", at(3, 3));
        let far = spawn(&mut g, "walker", at(4, 3));
        assert_eq!(g.units().armsmaster_xp(hero), 2, "level 1: 2 xp");
        set_xp(&mut g, hero, 20);
        assert_eq!(g.units().armsmaster_xp(hero), 4, "level 2: 4 xp");
        // Each turn a unit also gains its own one xp.
        g.process_experience();
        assert_eq!(xp(&g, near), 1 + 4);
        assert_eq!(xp(&g, far), 1);
        assert_eq!(
            xp(&g, hero),
            21,
            "the hero trains no one but itself, by the turn"
        );
        // Another hero with the better ability on the same square: the best counts, not the sum.
        let _super = spawn(&mut g, "scout", at(3, 3));
        g.process_experience();
        assert_eq!(
            xp(&g, near),
            5 + 1 + 4,
            "super at level 1 gives 3, the first hero's 4 wins"
        );
        set_xp(&mut g, hero, 120);
        g.process_experience();
        assert_eq!(xp(&g, near), 10 + 1 + 8, "level 4: 8 xp");
    }

    #[test]
    fn living_normal_units_and_heroes_gain_one_xp_a_turn_up_to_their_cap() {
        let mut g = world();
        let normal = spawn(&mut g, "walker", at(3, 3));
        let creature = spawn(&mut g, "slow", at(4, 3));
        let undead = spawn(&mut g, "walker", at(5, 3));
        g.make_undead(undead);
        let hero = spawn(&mut g, "hardy", at(6, 3));
        g.process_experience();
        assert_eq!(xp(&g, normal), 1);
        assert_eq!(xp(&g, hero), 1);
        assert_eq!(xp(&g, creature), 0, "creatures gain none");
        assert_eq!(xp(&g, undead), 0, "nor the undead");
        set_xp(&mut g, normal, 119);
        g.process_experience();
        g.process_experience();
        assert_eq!(xp(&g, normal), 120, "elite is where experience stops");
    }
}
