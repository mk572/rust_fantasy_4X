//! The battle screen's panel along the bottom, laid out like the original's
//! (`Combat_Screen_Draw`, `Draw_Active_Unit_Window`, `Tactical_Combat_Draw_Buttons`,
//! `Draw_Combat_Unit_Display` in `Combat.c`): the two wizards' names, the casting skill, mana and
//! range, the selected unit with its attacks, moves and damage bar, the six buttons, and a small
//! window with the stats of the unit under the pointer.

use super::*;
use crate::font::Style;
use crate::frame::Layout;
use crate::input::Input;

/// Top of the panel (`combat_background_bottom`), and of the buttons and text in it.
const BAR_Y: f32 = 164.0;
const BUTTONS_Y: f32 = 168.0;
const LEFT_BUTTONS_X: f32 = 144.0;
const RIGHT_BUTTONS_X: f32 = 170.0;
/// The window of the unit under the pointer (`x1, y1`).
const SCAN: (f32, f32, f32, f32) = (156.0, 4.0, 68.0, 39.0);

/// The icon (entry of `compix` 29 on, 61 on in the small window) for a unit's melee attack
/// (`Battle_Unit_Melee_Attack_Icon`): the weapon quality picks the sword.
fn melee_icon(u: &BattleUnit) -> Option<usize> {
    (u.melee > 0).then_some(match u.weapon_quality {
        2 => 3,
        3 => 2,
        _ => 0,
    })
}

/// The icon for its ranged attack (`Battle_Unit_Ranged_Attack_Icon`): a bow for missiles, the
/// magic bolt for spell shots; thrown weapons and breath get none.
fn ranged_icon(u: &BattleUnit) -> Option<usize> {
    match u.ranged_kind {
        RangedKind::Missile => Some(5),
        RangedKind::Magic => Some(1),
        RangedKind::None | RangedKind::Short => None,
    }
}

/// 0 walking, 1 flying, 2 sailing or swimming.
fn move_mode(u: &BattleUnit) -> usize {
    if u.flies {
        1
    } else if u.sails || u.swims {
        2
    } else {
        0
    }
}

/// The length of a unit's damage bar (`Draw_Active_Unit_Damage_Bar`), none when it is dead.
fn damage_bar(u: &BattleUnit) -> Option<(i32, &'static str)> {
    let max = u.hits * u.figures_start as i32;
    let lost = (u.figures_start - u.figures) as i32 * u.hits + u.front_damage.max(0);
    let current = max - lost;
    if current <= 0 || max <= 0 {
        return None;
    }
    let length = current * 20 / max - 1;
    let colour = match length {
        13.. => "bar_green",
        6.. => "bar_yellow",
        _ => "bar_red",
    };
    Some((length, colour))
}

/// The enchantments on one side's strip, in the original's order, as `compix` entries
/// (`combat_enchantment_icon_segs`): the battlefield spells, and Counter Magic's 15.
fn enchantment_icons(battle: &Battle, side: Side) -> Vec<usize> {
    use f4x_core::battle::field::Field;
    // (order in the original's table, picture)
    let mut icons: Vec<(usize, usize)> = battle
        .fields()
        .filter(|&(s, _, _)| s == side)
        .map(|(_, field, _)| match field {
            Field::TrueLight => (0, 5),
            Field::Darkness => (1, 6),
            Field::WarpReality => (2, 7),
            Field::BlackPrayer => (3, 8),
            Field::Wrack => (4, 9),
            Field::MetalFires => (5, 10),
            Field::Prayer => (6, 11),
            Field::HighPrayer => (7, 12),
            Field::Terror => (8, 13),
            Field::CallLightning => (9, 14),
            Field::MassInvisibility => (11, 41),
            Field::Entangle => (12, 60),
            Field::ManaLeak => (13, 79),
            Field::Blur => (14, 80),
        })
        .collect();
    if battle.counter_magic[side.index()] > 0 {
        icons.push((10, 15));
    }
    icons.sort();
    icons.into_iter().map(|(_, entry)| entry).collect()
}

/// "0.5x", "1x", "1.5x": the range multiplier of casting (`Draw_Spell_Information_Window`).
fn range_text(tenths: i32) -> String {
    match tenths {
        ..=9 => format!("0.{tenths}x"),
        _ if tenths % 10 == 0 => format!("{}x", tenths / 10),
        _ => format!("{}.{}x", tenths / 10, tenths % 10),
    }
}

/// Moves in whole and half steps.
fn moves_text(half_moves: u32) -> String {
    if half_moves % 2 == 1 {
        format!("{}.5", half_moves / 2)
    } else {
        (half_moves / 2).to_string()
    }
}

impl App {
    /// The window pixels per original pixel and the screen position of the grid's native origin,
    /// with the original's map window at the top left of the 320 by 200 screen.
    pub(super) fn battle_view() -> (f32, f32, f32) {
        let lay = Layout::new();
        // The window's first row shows the cells whose origins are 144 pixels left of the grid's
        // origin and 72 below it, 8 pixels above the window (`Combat_Screen_Map_Draw`).
        (
            lay.scale,
            lay.origin.x + 144.0 * lay.scale,
            lay.origin.y - 80.0 * lay.scale,
        )
    }

    /// Whether a window point is on the map (the original's map window), not on the panel or
    /// the box of the selected unit's spells.
    pub(super) fn on_map(&self, ui: &mut BattleUi, point: Vec2) -> bool {
        Layout::new()
            .rect((0.0, 0.0, crate::resolution::DESIGN_W, BAR_Y))
            .contains(point)
            && !ui.box_rect.is_some_and(|r| r.contains(point))
    }

    /// Text in the panel's font colours: light, with a dark shadow down and to the right.
    fn bar_text(
        &mut self,
        view: (f32, f32, f32),
        style: usize,
        text: &str,
        (x, y): (f32, f32),
        align: Align,
    ) {
        let ink = self.art.minimap().named("combat_text", WHITE);
        let shadow = self.art.minimap().named("combat_outline", BLACK);
        self.flat_text(view, style, text, (x, y), align, ink, shadow);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn flat_text(
        &mut self,
        (ox, oy, s): (f32, f32, f32),
        style: usize,
        text: &str,
        (x, y): (f32, f32),
        align: Align,
        ink: Color,
        shadow: Color,
    ) {
        let width = self.art.text_width(style, text);
        let left = match align {
            Align::Left => x,
            Align::Center => x - width / 2.0,
            Align::Right => x - width,
        };
        let at = |d: f32| (ox + (left + d) * s, oy + (y + d) * s);
        self.art.draw_flat_text(style, text, at(1.0), s, shadow);
        self.art.draw_flat_text(style, text, at(0.0), s, ink);
    }

    /// One of the six picture buttons, the locked picture when it cannot be pressed; true on
    /// the frame it is clicked.
    fn bar_button(
        &mut self,
        lay: &Layout,
        (entry, locked): (usize, usize),
        at: (f32, f32),
        enabled: bool,
        input: &Input,
    ) -> bool {
        if !enabled {
            self.draw_pic(lay, "compix", locked, 0, at.0, at.1);
            return false;
        }
        self.pic_button_on(lay, ("compix", entry), at, input)
    }

    pub(super) fn draw_battle_panel(
        &mut self,
        ui: &mut BattleUi,
        battle: &Battle,
        my_turn: bool,
        hovered: Option<Pos>,
        input: &Input,
    ) {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        self.draw_pic(&lay, "backgrnd", 3, 0, 0.0, BAR_Y);

        // The two names: the opponent's on the left, yours on the right.
        let mine = Self::human_side(battle).unwrap_or(Side::Attacker);
        let ours = battle.player_of(mine);
        let theirs = battle.player_of(mine.other());
        for (player, x) in [(theirs, 40.0), (ours, 278.0)] {
            let name = self.player_label(player);
            let colour = self.banner_color(player.0);
            self.flat_text(
                view,
                4,
                &name,
                (x, 167.0),
                Align::Center,
                colour,
                Color::new(0.0, 0.0, 0.0, 1.0),
            );
        }

        // Skill, mana and range (`Draw_Spell_Information_Window`).
        let (skill, mana, range) =
            Self::my_caster(battle).map_or((0, 0, 10), |c| (c.skill_left, c.mana, c.range));
        for (i, (label, value)) in [
            (self.text("battle.skill"), skill.to_string()),
            (self.text("battle.mana"), mana.to_string()),
            (self.text("battle.range"), range_text(range)),
        ]
        .into_iter()
        .enumerate()
        {
            let y = 172.0 + 9.0 * i as f32;
            self.bar_text(view, 0, &label, (200.0, y), Align::Left);
            self.bar_text(view, 0, &value, (236.0, y), Align::Right);
        }

        // The enchantments in force, four to a strip: the opponent's at the left, yours at the
        // right.
        for (side, left) in [(mine.other(), 8.0), (mine, 247.0)] {
            for (i, entry) in enchantment_icons(battle, side)
                .into_iter()
                .take(4)
                .enumerate()
            {
                self.draw_pic(&lay, "compix", entry, 0, left + 17.0 * i as f32, 179.0);
            }
        }
        self.draw_active_unit(ui, battle);
        let sel = ui.selected;
        let mut order = None;
        // The selected unit casts its own spells when it has any it can cast now (a hero, a
        // creature caster); otherwise the Spell button is the wizard's.
        let unit_caster = sel.filter(|&u| {
            my_turn
                && (self.game.battle_unit_spells(u).iter().any(|s| s.castable)
                    || self.game.can_summon_demon(u))
        });
        let can_cast = my_turn
            && ui.aiming.is_none()
            && ui.fallen_list.is_none()
            && (unit_caster.is_some()
                || Self::my_caster(battle).is_some_and(|c| !c.cast_this_turn));
        let (lx, rx) = (LEFT_BUTTONS_X, RIGHT_BUTTONS_X);
        let row = |i: f32| BUTTONS_Y + 10.0 * i;
        if self.bar_button(&lay, (1, 23), (lx, row(0.0)), can_cast, input)
            || (can_cast && input.key_pressed(KeyCode::S))
        {
            ui.spell_list = !ui.spell_list;
            ui.spell_unit = unit_caster.filter(|_| ui.spell_list);
        }
        if self.bar_button(&lay, (2, 24), (rx, row(0.0)), my_turn, input)
            || (my_turn && input.key_pressed(KeyCode::W))
        {
            self.select_next(ui, sel);
        }
        // Done ends the selected unit's turn; with no unit left that can act it ends the side's.
        if self.bar_button(&lay, (3, 28), (rx, row(2.0)), my_turn, input) {
            let active = sel.filter(|&u| battle.unit(u).is_some_and(super::unfinished));
            order = Some(match active {
                Some(unit) => BattleCommand::Done { unit },
                None => BattleCommand::EndTurn,
            });
        }
        // Auto hands your side to the computer; pressed again, it takes it back.
        let auto = ui.auto;
        if self.bar_button(&lay, (4, 26), (rx, row(1.0)), my_turn || auto, input)
            || ((my_turn || auto) && input.key_pressed(KeyCode::A))
        {
            ui.auto = !auto;
            ui.aiming = None;
            ui.spell_list = false;
            ui.spell_unit = None;
            ui.fallen_list = None;
        }
        // Info lists the effects in force; the original locks it when there are none.
        let has_info = self
            .game
            .battle_info(HUMAN)
            .is_some_and(|info| !info.is_empty());
        if (self.bar_button(&lay, (20, 25), (lx, row(1.0)), has_info, input)
            || (has_info && input.key_pressed(KeyCode::U)))
            && !ui.info_open
        {
            ui.info_open = true;
            return;
        }
        if self.bar_button(&lay, (21, 27), (lx, row(2.0)), my_turn, input)
            || (my_turn && input.key_pressed(KeyCode::F))
        {
            ui.confirm_flee = true;
        }
        if ui.confirm_flee {
            let question = self.text("battle.flee_question");
            match self.confirmation_box(&lay, &question, input) {
                Some(true) => order = Some(BattleCommand::Flee),
                Some(false) => ui.confirm_flee = false,
                None => {}
            }
        }
        if let Some(order) = order {
            let done = match order {
                BattleCommand::Done { unit } => Some(unit),
                _ => None,
            };
            self.battle_order(ui, order);
            match done {
                Some(unit) => self.select_next(ui, Some(unit)),
                None => ui.selected = None,
            }
            ui.confirm_flee = false;
        }

        // The last lines of the fight, at the top of the map.
        let line = self.art.text_height(1) + 2.0;
        let lines: Vec<String> = ui.log.iter().rev().take(3).rev().cloned().collect();
        for (i, text) in lines.iter().enumerate() {
            let at = (4.0, 3.0 + line * i as f32);
            self.draw_styled(view, &Style::flat(1, LIGHTGRAY).shadow(), text, at.0, at.1);
        }
        self.draw_scanned_unit(battle, hovered, view);
        self.draw_unit_specials(ui, battle, my_turn, input);
    }

    /// The selected unit in the middle of the panel (`Draw_Active_Unit_Window`).
    /// The vortex picked for steering stands where a unit would: its picture, its name and what to
    /// do with it.
    fn draw_active_vortex(&mut self, ui: &BattleUi, battle: &Battle, lay: &Layout) {
        if !ui
            .steering
            .and_then(|i| battle.vortices.get(i))
            .is_some_and(|v| v.owner == battle.active && !v.steered)
        {
            return;
        }
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let art = self
            .art
            .spell_picture("magic_vortex", &f4x_data::spell_art::Role::Vortex);
        let figure = art.and_then(|(lbx, entry)| {
            let frames = self.art.frames(&lbx, entry).max(1);
            self.art
                .picture(&lbx, entry, (get_time() / 0.1) as usize % frames)
        });
        if let Some(tex) = figure {
            let at = lay.point(
                84.0 + (32.0 - crate::resolution::logical_w(tex)) / 2.0,
                173.0 + (25.0 - crate::resolution::logical_h(tex)) / 2.0,
            );
            draw_texture_ex(
                tex,
                at.x,
                at.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(crate::resolution::logical_size(tex) * lay.scale),
                    ..Default::default()
                },
            );
        }
        let (name, hint) = (
            self.text("battle.vortex_name"),
            self.text("battle.vortex_hint"),
        );
        self.bar_text(view, 0, &name, (112.0, 167.0), Align::Center);
        self.bar_text(view, 0, &hint, (120.0, 176.0), Align::Left);
    }

    fn draw_active_unit(&mut self, ui: &mut BattleUi, battle: &Battle) {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let Some(u) = ui.selected.and_then(|i| battle.unit(i)) else {
            self.draw_active_vortex(ui, battle, &lay);
            return;
        };
        let owner = battle.player_of(u.side).0;
        let banner = self.banner_index(owner);
        let art = self
            .game
            .content()
            .rules
            .units
            .defs()
            .iter()
            .find(|d| d.id == u.unit_id)
            .and_then(|d| d.art.clone());
        let unit_type =
            art.and_then(|(lbx, entry)| crate::art::unit_type_number(&lbx, entry as usize));
        if let Some(tex) = unit_type.and_then(|t| self.art.figure(t, 2, 0, banner).cloned()) {
            // Centred in the 32 by 25 box at (84, 173).
            let at = lay.point(
                84.0 + (32.0 - crate::resolution::logical_w(&tex)) / 2.0,
                173.0 + (25.0 - crate::resolution::logical_h(&tex)) / 2.0,
            );
            draw_texture_ex(
                &tex,
                at.x,
                at.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(crate::resolution::logical_size(&tex) * lay.scale),
                    ..Default::default()
                },
            );
        }
        self.bar_text(view, 0, &u.name, (112.0, 167.0), Align::Center);
        if let Some(icon) = melee_icon(u) {
            self.bar_text(view, 0, &u.melee.to_string(), (126.0, 174.0), Align::Right);
            self.draw_pic(&lay, "compix", 29 + icon, 0, 128.0, 172.0);
        }
        if let Some(icon) = ranged_icon(u) {
            self.bar_text(view, 0, &u.ranged.to_string(), (126.0, 181.0), Align::Right);
            self.draw_pic(&lay, "compix", 29 + icon, 0, 128.0, 179.0);
        }
        self.bar_text(
            view,
            0,
            &moves_text(u.moves_left),
            (126.0, 188.0),
            Align::Right,
        );
        self.draw_pic(&lay, "compix", 38 + move_mode(u), 0, 128.0, 186.0);
        self.draw_pic(&lay, "compix", 18, 0, 117.0, 194.0);
        self.draw_damage_bar(&lay, u, (118.0, 195.0));
    }

    /// A one pixel tall bar for what is left of a unit's hit points.
    fn draw_damage_bar(&mut self, lay: &Layout, u: &BattleUnit, (x, y): (f32, f32)) {
        let Some((length, colour)) = damage_bar(u) else {
            return;
        };
        let colour = self.art.minimap().named(colour, GREEN);
        let r = lay.rect((x, y, (length + 1) as f32, 1.0));
        draw_rectangle(r.x, r.y, r.w, r.h, colour);
    }

    /// The dark box with a light edge above and left and a dark one below and right, as the
    /// original draws its small windows over the map; returns the dark edge's colour.
    pub(super) fn draw_hud_box(lay: &Layout, rect: (f32, f32, f32, f32)) -> Color {
        let r = lay.rect(rect);
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.1, 0.09, 0.12, 0.92));
        let (light, dark) = (
            Color::new(0.5, 0.45, 0.4, 1.0),
            Color::new(0.1, 0.08, 0.06, 1.0),
        );
        let px = lay.scale;
        draw_rectangle(r.x, r.y, px, r.h, light);
        draw_rectangle(r.x, r.y, r.w, px, light);
        draw_rectangle(r.x + r.w - px, r.y, px, r.h, dark);
        draw_rectangle(r.x, r.y + r.h - px, r.w, px, dark);
        dark
    }

    /// The little window with the stats of the unit under the pointer
    /// (`Draw_Combat_Unit_Display`), at the top of the map, left of the box of spells and abilities.
    fn draw_scanned_unit(&mut self, battle: &Battle, hovered: Option<Pos>, view: (f32, f32, f32)) {
        if !self
            .prefs
            .get(crate::preferences::Pref::AutoUnitInformation)
        {
            return;
        }
        let Some(u) = hovered.and_then(|p| Self::seen_unit_at(battle, p)) else {
            return;
        };
        let lay = Layout::new();
        let (x, y, w, h) = SCAN;
        let dark = Self::draw_hud_box(&lay, (x, y, w, h));
        let ink = self.art.minimap().named("scan_text", WHITE);
        let black = Color::new(0.0, 0.0, 0.0, 1.0);
        let text = |app: &mut App, s: &str, at: (f32, f32), align: Align| {
            app.flat_text(view, 0, s, at, align, ink, black);
        };
        text(self, &u.name, (x + w / 2.0, y + 2.0), Align::Center);
        if let Some(icon) = melee_icon(u) {
            text(
                self,
                &u.melee.to_string(),
                (x + 9.0, y + 10.0),
                Align::Right,
            );
            self.draw_pic(&lay, "compix", 61 + icon, 0, x + 11.0, y + 8.0);
        }
        if let Some(icon) = ranged_icon(u) {
            text(
                self,
                &u.ranged.to_string(),
                (x + 9.0, y + 17.0),
                Align::Right,
            );
            self.draw_pic(&lay, "compix", 61 + icon, 0, x + 11.0, y + 15.0);
        }
        text(
            self,
            &moves_text(u.moves_left),
            (x + 9.0, y + 24.0),
            Align::Right,
        );
        self.draw_pic(&lay, "compix", 72 + move_mode(u), 0, x + 11.0, y + 22.0);
        let hits = self.text("battle.hits_label");
        text(self, &hits, (x + 3.0, y + 32.0), Align::Left);
        let back = lay.rect((x + 19.0, y + 34.0, 20.0, 2.0));
        draw_rectangle(back.x, back.y, back.w, back.h, dark);
        self.draw_damage_bar(&lay, u, (x + 19.0, y + 34.0));
        text(
            self,
            &u.defense.to_string(),
            (x + 48.0, y + 10.0),
            Align::Right,
        );
        self.draw_pic(&lay, "compix", 70, 0, x + 50.0, y + 8.0);
        text(
            self,
            &u.resistance.to_string(),
            (x + 48.0, y + 17.0),
            Align::Right,
        );
        self.draw_pic(&lay, "compix", 75, 0, x + 50.0, y + 15.0);
        if u.mana > 0 {
            text(
                self,
                &u.mana.to_string(),
                (x + 48.0, y + 24.0),
                Align::Right,
            );
            text(self, "MP", (x + 51.0, y + 24.0), Align::Left);
        } else if u.ammo > 0 {
            text(
                self,
                &u.ammo.to_string(),
                (x + 48.0, y + 24.0),
                Align::Right,
            );
            let ammo = self.text("battle.ammo_label");
            text(self, &ammo, (x + 51.0, y + 24.0), Align::Left);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_range_of_casting_reads_as_a_multiplier() {
        assert_eq!(range_text(5), "0.5x");
        assert_eq!(range_text(10), "1x");
        assert_eq!(range_text(15), "1.5x");
        assert_eq!(range_text(20), "2x");
    }

    #[test]
    fn moves_are_written_in_whole_and_half_steps() {
        assert_eq!(moves_text(0), "0");
        assert_eq!(moves_text(1), "0.5");
        assert_eq!(moves_text(4), "2");
        assert_eq!(moves_text(5), "2.5");
    }
}
