//! The battle screen's mouse pointer, drawn by the game as the original does
//! (`Assign_Mouse_Images`, `Combat_Spell_Target_Screen_Draw`): a finger by default, a winged boot
//! on a square the unit can walk to, crossed swords on an enemy it can strike, a bow on one it can
//! shoot, a wand on a square a spell can be aimed at, and a red cross where nothing can be done.
//!
//! Tests: below. Which pointer a hovered square gets is `pointer_image`; showing it needs a window.

use super::*;
use crate::input::Input;

use crate::pointer::{BOOT, BOW, FINGER, RED_CROSS, SWORDS, WAND};

impl App {
    /// What the pointer should look like over the battle.
    fn pointer_image(
        &self,
        ui: &mut BattleUi,
        battle: &Battle,
        my_turn: bool,
        hovered: Option<Pos>,
        input: &Input,
    ) -> usize {
        if !self.on_map(ui, input.mouse_position().into()) {
            return FINGER;
        }
        if let Some(aim) = &ui.aiming {
            let aimable = hovered.is_some_and(|cell| self.spell_aim_ok(battle, aim, cell));
            return if aimable { WAND } else { RED_CROSS };
        }
        if !my_turn || ui.spell_list {
            return FINGER;
        }
        let Some(cell) = hovered else {
            return RED_CROSS;
        };
        let sel = ui
            .selected
            .and_then(|i| battle.unit(i))
            .filter(|u| unfinished(u));
        let strike = |u: &BattleUnit| {
            if u.attacked {
                RED_CROSS
            } else if distance(u.cell, cell) == 1 {
                SWORDS
            } else if can_shoot(u) {
                BOW
            } else {
                RED_CROSS
            }
        };
        match (Self::seen_unit_at(battle, cell), sel) {
            (Some(target), _) if target.side == battle.active => FINGER,
            (Some(_), Some(u)) => strike(u),
            (Some(_), None) => RED_CROSS,
            (None, Some(u))
                if battle.map.has_wall(cell)
                    && u.has("wall_crusher")
                    && u.side == Side::Attacker =>
            {
                strike(u)
            }
            (None, Some(u)) if battle.reachable(u.id).contains_key(&cell) => BOOT,
            _ => RED_CROSS,
        }
    }

    /// Draw the pointer on top of everything, hiding the system's.
    pub(super) fn draw_battle_pointer(
        &mut self,
        ui: &mut BattleUi,
        battle: &Battle,
        my_turn: bool,
        hovered: Option<Pos>,
        input: &Input,
    ) {
        let image = self.pointer_image(ui, battle, my_turn, hovered, input);
        self.draw_pointer_image(image, input);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::HUMAN;
    use f4x_core::battle::RangedKind;
    use f4x_core::testdir::TestResult;

    /// A real battle between the human's first unit and a monster, the units placed by hand on a
    /// row: ours at (5, 5), theirs at (5, 6) next to it and one more of theirs at (5, 9).
    struct Fight {
        app: App,
        battle: Battle,
        ours: usize,
    }

    fn fight() -> Result<Fight, Box<dyn std::error::Error>> {
        let mut app = App::for_test();
        let attackers = app.game.units().player_units(HUMAN);
        let pos = attackers
            .first()
            .and_then(|&u| app.game.units().unit_position(u))
            .ok_or("the human has no unit")?;
        let kind = app
            .game
            .content()
            .rules
            .units
            .id_of("barbarian_spearmen")
            .ok_or("barbarian_spearmen is not in the content")?;
        let defenders = vec![
            app.game.spawn_unit(f4x_core::player::MONSTERS, pos, kind),
            app.game.spawn_unit(f4x_core::player::MONSTERS, pos, kind),
        ];
        app.game.apply(Command::BeginBattle {
            attackers,
            defenders,
        });
        let mut battle = app.game.battle().ok_or("no battle began")?.clone();
        let mine = |u: &BattleUnit| u.side == Side::Attacker;
        let ours = battle
            .units
            .iter()
            .position(mine)
            .ok_or("no unit of ours in the battle")?;
        let mut theirs = battle.units.iter().enumerate().filter(|(_, u)| !mine(u));
        let (near, far) = (
            theirs.next().ok_or("no enemy")?.0,
            theirs.next().ok_or("no second enemy")?.0,
        );
        for (i, cell) in [(ours, (5, 5)), (near, (5, 6)), (far, (5, 9))] {
            battle.units[i].cell = cell;
        }
        Ok(Fight { app, battle, ours })
    }

    /// The mouse over the battle map, away from the bar.
    fn over_the_map() -> Input {
        let spot = crate::frame::Layout::new().rect((100.0, 50.0, 1.0, 1.0));
        Input {
            mouse: (spot.x, spot.y),
            ..Input::default()
        }
    }

    fn image(f: &Fight, ui: &mut BattleUi, my_turn: bool, at: Option<Pos>) -> usize {
        f.app
            .pointer_image(ui, &f.battle, my_turn, at, &over_the_map())
    }

    #[test]
    fn off_the_map_or_when_it_is_not_our_turn_the_pointer_is_the_finger() -> TestResult {
        let f = fight()?;
        let mut ui = BattleUi::default();
        let bar = Input {
            mouse: (0.0, 0.0),
            ..Input::default()
        };
        assert_eq!(
            f.app
                .pointer_image(&mut ui, &f.battle, true, Some((5, 6)), &bar),
            FINGER
        );
        assert_eq!(image(&f, &mut ui, false, Some((5, 6))), FINGER);
        ui.spell_list = true;
        assert_eq!(image(&f, &mut ui, true, Some((5, 6))), FINGER);
        Ok(())
    }

    #[test]
    fn no_square_or_an_enemy_with_no_one_selected_gets_the_cross() -> TestResult {
        let f = fight()?;
        let mut ui = BattleUi::default();
        assert_eq!(image(&f, &mut ui, true, None), RED_CROSS);
        assert_eq!(image(&f, &mut ui, true, Some((5, 6))), RED_CROSS);
        Ok(())
    }

    #[test]
    fn the_selected_unit_strikes_an_adjacent_enemy_with_swords_and_a_spent_one_cannot() -> TestResult
    {
        let mut f = fight()?;
        let mut ui = BattleUi {
            selected: Some(f.ours),
            ..BattleUi::default()
        };
        assert_eq!(image(&f, &mut ui, true, Some((5, 6))), SWORDS);
        f.battle.units[f.ours].attacked = true;
        assert_eq!(image(&f, &mut ui, true, Some((5, 6))), RED_CROSS);
        Ok(())
    }

    #[test]
    fn a_far_enemy_gets_the_bow_only_from_a_unit_that_can_shoot() -> TestResult {
        let mut f = fight()?;
        let mut ui = BattleUi {
            selected: Some(f.ours),
            ..BattleUi::default()
        };
        assert_eq!(image(&f, &mut ui, true, Some((5, 9))), RED_CROSS);
        let me = &mut f.battle.units[f.ours];
        me.ranged_kind = RangedKind::Missile;
        me.ranged = 3;
        me.ammo = 4;
        assert_eq!(image(&f, &mut ui, true, Some((5, 9))), BOW);
        f.battle.units[f.ours].ammo = 0;
        assert_eq!(
            image(&f, &mut ui, true, Some((5, 9))),
            RED_CROSS,
            "no ammunition"
        );
        Ok(())
    }

    #[test]
    fn our_own_unit_gets_the_finger_and_a_free_square_the_boot_when_it_is_reachable() -> TestResult
    {
        let f = fight()?;
        let mut ui = BattleUi {
            selected: Some(f.ours),
            ..BattleUi::default()
        };
        assert_eq!(image(&f, &mut ui, true, Some((5, 5))), FINGER);
        let reach = f.battle.reachable(f.ours);
        let free = reach
            .keys()
            .copied()
            .find(|&c| f.battle.unit_at(c).is_none())
            .ok_or("the unit can reach no free square")?;
        assert_eq!(image(&f, &mut ui, true, Some(free)), BOOT);
        let unreachable = (0..GRID_W)
            .flat_map(|x| (0..GRID_H).map(move |y| (x, y)))
            .find(|&c| f.battle.unit_at(c).is_none() && !reach.contains_key(&c))
            .ok_or("every square is reachable")?;
        assert_eq!(image(&f, &mut ui, true, Some(unreachable)), RED_CROSS);
        Ok(())
    }
}
