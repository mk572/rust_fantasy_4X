//! The battle screen: the combat grid in the original's isometric projection, the units on
//! it, and the orders a player can give. The fight itself is the core's (`f4x_core::battle`).
//!
//! The grid uses the original's 32 by 16 pixel cells and its step of (16, 8) per square
//! (`Combat_Grid_Screen_Coordinates`), scaled to fit the window. The original shows only a
//! band of its 21 by 22 squares; the core plays on all of them, so all are drawn.

use f4x_core::battle::ai::next_command;
use f4x_core::battle::map::{distance, Cell, Pos, GRID_H, GRID_W};
use f4x_core::battle::{Battle, BattleCommand, BattleEvent, BattleUnit, RangedKind, Side};
use f4x_core::content::TerrainClass;
use f4x_core::map::Plane;
use f4x_core::{Command, Event};
use macroquad::prelude::*;

use crate::frame::Align;
use crate::ui::{bar, button, GOLD, PANEL};
use crate::view::{App, Screen, HUMAN};

/// Native size of a cell and the step between neighbouring squares.
const CELL_W: f32 = 32.0;
const CELL_H: f32 = 16.0;
const STEP_X: f32 = 16.0;
const STEP_Y: f32 = 8.0;
/// Room above the top row for tall figures, in native pixels.
const HEADROOM: f32 = 26.0;
const PANEL_W: f32 = 290.0;
/// Seconds between the opponent's orders.
const AI_DELAY: f32 = 0.3;

/// What the screen remembers between frames.
#[derive(Default)]
pub(crate) struct BattleUi {
    selected: Option<usize>,
    ai_timer: f32,
    log: Vec<String>,
    /// The fleeing button was pressed once and waits for a second press.
    confirm_flee: bool,
    /// Unit names by id, kept while the battle exists so the last events can be told after
    /// the game has dropped the battle.
    names: Vec<String>,
    /// The square the last blow landed on and how long it still shows.
    flash: Option<(Pos, f32)>,
}

/// Top-left of a square's cell, relative to the grid's origin, in native pixels.
pub(crate) fn cell_origin(pos: Pos) -> (f32, f32) {
    (
        (pos.0 - pos.1) as f32 * STEP_X,
        (pos.0 + pos.1) as f32 * STEP_Y,
    )
}

/// The square under a native point (relative to the grid's origin), if any. Where cells of
/// tall scenery would overlap the one nearer the viewer wins.
pub(crate) fn cell_at(x: f32, y: f32) -> Option<Pos> {
    let mut found = None;
    for gy in 0..GRID_H {
        for gx in 0..GRID_W {
            let (ox, oy) = cell_origin((gx, gy));
            let dx = (x - (ox + CELL_W / 2.0)).abs() / (CELL_W / 2.0);
            let dy = (y - (oy + CELL_H / 2.0)).abs() / (CELL_H / 2.0);
            if dx + dy <= 1.0 {
                found = Some((gx, gy));
            }
        }
    }
    found
}

/// Native width and height of the grid, with headroom.
fn grid_extent() -> (f32, f32) {
    let width = (GRID_W + GRID_H - 2) as f32 * STEP_X + CELL_W;
    let height = (GRID_W + GRID_H - 2) as f32 * STEP_Y + CELL_H + HEADROOM;
    (width, height)
}

/// The original's picture file for the ground of a fight on this terrain.
pub(crate) fn terrain_set(class: Option<TerrainClass>, plane: Plane) -> &'static str {
    let (arcanus, myrror) = match class {
        Some(TerrainClass::Desert) => ("cmbdesrt", "cmbdesrc"),
        Some(TerrainClass::Mountains) => ("cmbmount", "cmbmounc"),
        Some(TerrainClass::Tundra) => ("cmbtundr", "cmbtundc"),
        _ => ("cmbgrass", "cmbgrasc"),
    };
    match plane {
        Plane::Arcanus => arcanus,
        Plane::Myrror => myrror,
    }
}

/// One of the four plain ground pictures, varied by square.
fn ground_variant(pos: Pos) -> usize {
    ((pos.0 * 7 + pos.1 * 13) as usize) % 4
}

/// Tell what a battle event did, in one line.
pub(crate) fn describe(names: &[String], event: &BattleEvent) -> Option<String> {
    let name = |i: usize| names.get(i).map_or("?", String::as_str);
    Some(match event {
        BattleEvent::Moved { .. } => return None,
        BattleEvent::Attacked {
            attacker,
            defender,
            dealt,
            dealt_kills,
            taken,
            ..
        } => format!(
            "{} hits {}: {dealt} damage{}{}",
            name(*attacker),
            name(*defender),
            kills(*dealt_kills),
            if *taken > 0 {
                format!(", takes {taken}")
            } else {
                String::new()
            }
        ),
        BattleEvent::Shot {
            attacker,
            defender,
            dealt,
            kills: k,
        } => format!(
            "{} shoots {}: {dealt} damage{}",
            name(*attacker),
            name(*defender),
            kills(*k)
        ),
        BattleEvent::UnitDied { unit } => format!("{} is destroyed", name(*unit)),
        BattleEvent::TurnStarted { turn, .. } => format!("Turn {turn}"),
        BattleEvent::Fled { .. } => "The army flees".to_string(),
        BattleEvent::Ended { .. } => return None,
    })
}

fn kills(count: u32) -> String {
    match count {
        0 => String::new(),
        1 => ", 1 figure lost".to_string(),
        n => format!(", {n} figures lost"),
    }
}

/// Can the unit still do something this turn?
fn unfinished(u: &BattleUnit) -> bool {
    u.is_active() && !(u.attacked && u.moves_left == 0)
}

fn can_shoot(u: &BattleUnit) -> bool {
    matches!(u.ranged_kind, RangedKind::Missile | RangedKind::Magic) && u.ranged > 0 && u.ammo > 0
}

impl App {
    /// Which side the human commands in the current battle, if any.
    fn human_side(battle: &Battle) -> Option<Side> {
        [Side::Attacker, Side::Defender]
            .into_iter()
            .find(|&s| battle.player_of(s) == HUMAN)
    }

    /// Start a battle for the stack the human just moved, or for a site's guards.
    pub(crate) fn begin_battle(&mut self, command: Command) {
        self.battle_ui = BattleUi::default();
        let events = self.game.apply(command);
        self.note(events);
        if let Some(battle) = self.game.battle() {
            self.battle_ui.names = battle.units.iter().map(|u| u.name.clone()).collect();
            let first = battle
                .active_units(battle.active)
                .find(|u| unfinished(u))
                .map(|u| u.id);
            self.battle_ui.selected = first;
            self.screen = Screen::Battle;
        }
    }

    /// Pit the human's units against monsters on the same square (for screenshots and tests).
    pub fn open_test_battle(&mut self) {
        let mine = self.game.player_units(HUMAN);
        let Some(pos) = mine.first().and_then(|&u| self.game.unit_position(u)) else {
            return;
        };
        let rules = &self.game.content.rules;
        let ids: Vec<_> = rules
            .units
            .defs()
            .iter()
            .filter(|d| d.kind == f4x_core::rules::UnitKind::Normal && d.race.is_some())
            .take(6)
            .filter_map(|d| rules.units.id_of(&d.id))
            .collect();
        let defenders: Vec<_> = ids
            .into_iter()
            .map(|id| self.game.spawn_unit(f4x_core::units::NEUTRAL, pos, id))
            .collect();
        self.begin_battle(Command::BeginBattle {
            attackers: mine
                .into_iter()
                .filter(|&u| self.game.unit_position(u) == Some(pos))
                .collect(),
            defenders,
        });
    }

    /// Send one order to the battle and tell what came of it.
    fn battle_order(&mut self, order: BattleCommand) {
        let was_attack = match &order {
            BattleCommand::Attack { target, .. } | BattleCommand::Shoot { target, .. } => self
                .game
                .battle()
                .and_then(|b| b.unit(*target))
                .map(|u| u.cell),
            _ => None,
        };
        let events = self.game.apply(Command::Battle(order));
        let names = self.battle_ui.names.clone();
        for event in &events {
            match event {
                Event::Battle(e) => {
                    if let Some(line) = describe(&names, e) {
                        self.battle_ui.log.push(line);
                    }
                }
                Event::BattleEnded { winner, .. } => {
                    let line = if *winner == HUMAN {
                        "Victory!"
                    } else {
                        "Defeat"
                    };
                    self.battle_ui.log.push(line.to_string());
                    self.log.push(line.to_string());
                }
                Event::Rejected(why) => self.battle_ui.log.push(format!("Cannot: {why:?}")),
                _ => {}
            }
        }
        if let Some(cell) = was_attack {
            self.battle_ui.flash = Some((cell, 0.4));
        }
        // The overland log gets the rest (a cleared site's treasure and so on).
        let rest: Vec<Event> = events
            .into_iter()
            .filter(|e| !matches!(e, Event::Battle(_) | Event::BattleEnded { .. }))
            .collect();
        self.note(rest);
        let n = self.battle_ui.log.len();
        if n > 30 {
            self.battle_ui.log.drain(..n - 30);
        }
    }

    /// Select the next unit of the human's side that can still act.
    fn select_next(&mut self, after: Option<usize>) {
        let Some(battle) = self.game.battle() else {
            return;
        };
        let ids: Vec<usize> = battle
            .active_units(battle.active)
            .filter(|u| unfinished(u))
            .map(|u| u.id)
            .collect();
        let start = after.unwrap_or(0);
        let next = ids
            .iter()
            .copied()
            .find(|&i| i > start)
            .or_else(|| ids.first().copied());
        self.battle_ui.selected = next;
    }

    pub(crate) fn battle_screen(&mut self) {
        let Some(battle) = self.game.battle().cloned() else {
            self.screen = Screen::Map;
            return;
        };
        if self.battle_ui.names.len() != battle.units.len() {
            self.battle_ui.names = battle.units.iter().map(|u| u.name.clone()).collect();
        }
        let dt = get_frame_time();
        self.battle_ui.ai_timer -= dt;
        if let Some((_, t)) = self.battle_ui.flash.as_mut() {
            *t -= dt;
        }
        if self.battle_ui.flash.is_some_and(|(_, t)| t <= 0.0) {
            self.battle_ui.flash = None;
        }

        let mine = Self::human_side(&battle);
        let my_turn = mine == Some(battle.active);
        // The other side plays by itself.
        if !my_turn && self.battle_ui.ai_timer <= 0.0 {
            self.battle_ui.ai_timer = AI_DELAY;
            let order = next_command(&battle);
            self.battle_order(order);
            if !self.game.in_battle() {
                self.screen = Screen::Map;
                return;
            }
            self.select_next(None);
            return self.draw_battle(&battle, my_turn, None);
        }

        // Where the pointer is.
        let (w, h) = (screen_width(), screen_height());
        let (scale, ox, oy) = grid_view(w - PANEL_W, h - 30.0);
        let mouse: Vec2 = mouse_position().into();
        let hovered = cell_at((mouse.x - ox) / scale, (mouse.y - oy) / scale);

        if my_turn {
            self.battle_input(&battle, hovered);
        }
        if !self.game.in_battle() {
            self.screen = Screen::Map;
            return;
        }
        let battle = self.game.battle().cloned().unwrap_or(battle);
        self.draw_battle(&battle, my_turn, hovered);
    }

    fn battle_input(&mut self, battle: &Battle, hovered: Option<Pos>) {
        let selected = self.battle_ui.selected;
        let sel_unit = selected
            .and_then(|i| battle.unit(i))
            .filter(|u| unfinished(u));

        if is_key_pressed(KeyCode::Tab) {
            self.select_next(selected);
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::E) {
            self.battle_order(BattleCommand::EndTurn);
            self.battle_ui.selected = None;
            return;
        }
        if is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::D) {
            if let Some(u) = sel_unit {
                self.battle_order(BattleCommand::Done { unit: u.id });
                self.select_next(Some(u.id));
            }
            return;
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            let (w, h) = (screen_width(), screen_height());
            // Clicks on the side panel are the buttons' business.
            if mouse_position().0 < w - PANEL_W && mouse_position().1 < h - 30.0 {
                if let Some(cell) = hovered {
                    self.battle_click(battle, cell);
                }
            }
        }
    }

    fn battle_click(&mut self, battle: &Battle, cell: Pos) {
        let selected = self.battle_ui.selected;
        let target = battle.unit_at(cell).map(|u| (u.id, u.side));
        let sel = selected
            .and_then(|i| battle.unit(i))
            .filter(|u| unfinished(u));
        match (target, sel) {
            // Pick one of my own units.
            (Some((id, side)), _) if side == battle.active => {
                self.battle_ui.selected = Some(id);
            }
            // Hit an enemy: in melee when next to it, otherwise shoot.
            (Some((id, _)), Some(u)) if !u.attacked => {
                let order = if distance(u.cell, cell) == 1 {
                    BattleCommand::Attack {
                        unit: u.id,
                        target: id,
                    }
                } else if can_shoot(u) {
                    BattleCommand::Shoot {
                        unit: u.id,
                        target: id,
                    }
                } else {
                    return;
                };
                self.battle_order(order);
                self.after_order(u.id);
            }
            (None, Some(u)) if battle.reachable(u.id).contains_key(&cell) => {
                self.battle_order(BattleCommand::Move {
                    unit: u.id,
                    to: cell,
                });
                self.after_order(u.id);
            }
            _ => {}
        }
    }

    /// Keep the selection on a unit that can still act.
    fn after_order(&mut self, unit: usize) {
        let still = self
            .game
            .battle()
            .and_then(|b| b.unit(unit))
            .is_some_and(unfinished);
        if !still {
            self.select_next(Some(unit));
        }
    }

    fn draw_battle(&mut self, battle: &Battle, my_turn: bool, hovered: Option<Pos>) {
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let (w, h) = (screen_width(), screen_height());
        let (scale, ox, oy) = grid_view(w - PANEL_W, h - 30.0);
        let at = |pos: Pos| -> (f32, f32) {
            let (x, y) = cell_origin(pos);
            (ox + x * scale, oy + y * scale)
        };
        let (plane, class) = match battle.site {
            Some(site) => (site.plane, self.game.terrain_class_at(site)),
            None => (Plane::Arcanus, None),
        };
        let set = terrain_set(class, plane);

        let selected = self.battle_ui.selected.and_then(|i| battle.unit(i));
        let reach = match (selected, my_turn) {
            (Some(u), true) if unfinished(u) => battle.reachable(u.id),
            _ => Default::default(),
        };

        // Ground, then reach markers.
        for gy in 0..GRID_H {
            for gx in 0..GRID_W {
                let pos = (gx, gy);
                let (sx, sy) = at(pos);
                let cell = battle.map.get(pos);
                let tint = match cell {
                    Some(Cell::Rough) => Color::new(0.75, 0.62, 0.4, 1.0),
                    _ => WHITE,
                };
                match self.art.picture(set, ground_variant(pos), 0) {
                    Some(tex) => draw_texture_ex(
                        tex,
                        sx,
                        sy,
                        tint,
                        DrawTextureParams {
                            dest_size: Some(vec2(CELL_W * scale, CELL_H * scale)),
                            ..Default::default()
                        },
                    ),
                    None => diamond(sx, sy, scale, Color::new(0.2, 0.35, 0.15, 1.0), true),
                }
                if reach.contains_key(&pos) {
                    diamond(sx, sy, scale, Color::new(0.4, 0.7, 1.0, 0.28), true);
                }
            }
        }
        // Grid lines help to judge distance.
        for gy in 0..GRID_H {
            for gx in 0..GRID_W {
                let (sx, sy) = at((gx, gy));
                diamond(sx, sy, scale, Color::new(0.0, 0.0, 0.0, 0.18), false);
            }
        }
        if let Some(pos) = hovered {
            let (sx, sy) = at(pos);
            diamond(sx, sy, scale, Color::new(1.0, 1.0, 1.0, 0.8), false);
        }

        // Scenery and units, far rows first so near ones cover them.
        let mut items: Vec<(i32, bool, Pos, usize)> = Vec::new();
        for gy in 0..GRID_H {
            for gx in 0..GRID_W {
                if battle.map.get((gx, gy)) == Some(Cell::Blocked) {
                    items.push((gx + gy, false, (gx, gy), 0));
                }
            }
        }
        for u in battle.units.iter().filter(|u| u.is_active()) {
            items.push((u.cell.0 + u.cell.1, true, u.cell, u.id));
        }
        items.sort_by_key(|&(depth, is_unit, pos, _)| (depth, is_unit, pos));
        for (_, is_unit, pos, id) in items {
            if is_unit {
                self.draw_unit(battle, id, ox, oy, scale, my_turn);
            } else {
                self.draw_scenery(set, class, pos, at(pos), scale);
            }
        }

        // Markers over everything: the selected unit and the blow just struck.
        if let Some(u) = selected {
            let (sx, sy) = at(u.cell);
            let blink = (get_time() * 3.0) as i64 % 2 == 0;
            diamond(sx, sy, scale, if blink { GOLD } else { WHITE }, false);
            if my_turn {
                for e in battle.active_units(u.side.other()) {
                    let hit = if distance(u.cell, e.cell) == 1 {
                        !u.attacked
                    } else {
                        can_shoot(u) && !u.attacked
                    };
                    if hit {
                        let (ex, ey) = at(e.cell);
                        diamond(ex, ey, scale, Color::new(1.0, 0.2, 0.2, 0.9), false);
                    }
                }
            }
        }
        if let Some((cell, t)) = self.battle_ui.flash {
            let (sx, sy) = at(cell);
            diamond(
                sx,
                sy,
                scale,
                Color::new(1.0, 0.3, 0.1, (t / 0.4).clamp(0.0, 1.0)),
                true,
            );
        }

        self.draw_battle_panel(battle, my_turn, hovered);
    }

    fn draw_scenery(
        &mut self,
        set: &str,
        class: Option<TerrainClass>,
        pos: Pos,
        (sx, sy): (f32, f32),
        scale: f32,
    ) {
        let trees = matches!(
            class,
            Some(TerrainClass::Forest) | Some(TerrainClass::Grassland) | Some(TerrainClass::Swamp)
        ) || (pos.0 + pos.1) % 3 == 0;
        let entry = if trees {
            48 + ((pos.0 * 5 + pos.1 * 3) as usize) % 5
        } else {
            53 + ((pos.0 * 5 + pos.1 * 3) as usize) % 5
        };
        if let Some(tex) = self.art.picture(set, entry, 0) {
            let size = tex.size() * scale;
            // Stand the picture's base on the middle of the cell.
            draw_texture_ex(
                tex,
                sx + (CELL_W * scale - size.x) / 2.0,
                sy + CELL_H * scale * 0.65 - size.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(size),
                    ..Default::default()
                },
            );
        } else {
            diamond(sx, sy, scale, Color::new(0.0, 0.25, 0.0, 0.9), true);
        }
    }

    fn draw_unit(
        &mut self,
        battle: &Battle,
        id: usize,
        ox: f32,
        oy: f32,
        scale: f32,
        my_turn: bool,
    ) {
        let Some(u) = battle.unit(id) else {
            return;
        };
        let (x, y) = cell_origin(u.cell);
        let (sx, sy) = (ox + x * scale, oy + y * scale);
        let banner = self.banner_color(battle.player_of(u.side).0);
        // A coloured base in the owner's banner.
        diamond(sx, sy, scale, Color { a: 0.55, ..banner }, true);
        diamond(sx, sy, scale, banner, false);
        let art = self
            .game
            .content
            .rules
            .units
            .defs()
            .iter()
            .find(|d| d.id == u.unit_id)
            .and_then(|d| d.art.clone());
        let icon = art.and_then(|(lbx, entry)| self.art.picture(&lbx, entry as usize, 0));
        let cx = sx + CELL_W * scale / 2.0;
        let base = sy + CELL_H * scale * 0.7;
        match icon {
            Some(tex) => {
                let size = tex.size() * scale * 1.3;
                let tint = if my_turn && u.side == battle.active && !unfinished(u) {
                    Color::new(0.6, 0.6, 0.6, 1.0)
                } else {
                    WHITE
                };
                draw_texture_ex(
                    tex,
                    cx - size.x / 2.0,
                    base - size.y,
                    tint,
                    DrawTextureParams {
                        dest_size: Some(size),
                        ..Default::default()
                    },
                );
            }
            None => draw_circle(cx, base - 6.0 * scale, 6.0 * scale, banner),
        }
        // Health bar and figure count.
        let max = u.max_hit_points().max(1) as f32;
        let (bw, bh) = (CELL_W * scale * 0.6, (2.0 * scale).max(2.0));
        let bar_rect = Rect::new(cx - bw / 2.0, base + bh, bw, bh);
        let frac = u.hit_points() as f32 / max;
        let color = if frac > 0.5 {
            Color::new(0.2, 0.8, 0.2, 1.0)
        } else if frac > 0.25 {
            Color::new(0.9, 0.8, 0.2, 1.0)
        } else {
            Color::new(0.9, 0.2, 0.2, 1.0)
        };
        bar(bar_rect, frac, color);
        let ts = scale.floor().max(1.0);
        self.bm_text_outlined(
            (0.0, 0.0, ts),
            1,
            &u.figures.to_string(),
            (bar_rect.x + bar_rect.w + 2.0) / ts,
            (bar_rect.y - 3.0 * scale) / ts,
            Align::Left,
        );
    }

    fn draw_battle_panel(&mut self, battle: &Battle, my_turn: bool, hovered: Option<Pos>) {
        let (w, h) = (screen_width(), screen_height());
        let x = w - PANEL_W;
        draw_rectangle(x, 0.0, PANEL_W, h, PANEL);
        let mut y = 28.0;
        let attacker = self.player_label(battle.attacker);
        let defender = self.player_label(battle.defender);
        draw_text(
            &format!("{attacker}  vs  {defender}"),
            x + 12.0,
            y,
            22.0,
            GOLD,
        );
        y += 24.0;
        let who = if my_turn {
            "Your turn".to_string()
        } else {
            format!(
                "{} acts",
                self.player_label(battle.player_of(battle.active))
            )
        };
        draw_text(
            &format!("Turn {}   {who}", battle.turn),
            x + 12.0,
            y,
            20.0,
            WHITE,
        );
        y += 30.0;

        // The unit under the pointer, else the selected one.
        let shown = hovered
            .and_then(|p| battle.unit_at(p))
            .or_else(|| self.battle_ui.selected.and_then(|i| battle.unit(i)));
        if let Some(u) = shown {
            draw_text(&u.name, x + 12.0, y, 22.0, WHITE);
            y += 22.0;
            let side = self.player_label(battle.player_of(u.side));
            draw_text(&side, x + 12.0, y, 18.0, LIGHTGRAY);
            y += 22.0;
            let ammo = if u.ranged_kind == RangedKind::None {
                String::new()
            } else {
                format!("  ammo {}", u.ammo)
            };
            for line in [
                format!("Melee {}   Ranged {}{ammo}", u.melee, u.ranged),
                format!("Defense {}   Resist {}", u.defense, u.resistance),
                format!(
                    "Figures {}/{}   Hits {}/{}",
                    u.figures,
                    u.figures_start,
                    u.hit_points(),
                    u.max_hit_points()
                ),
                format!(
                    "Moves {:.1}/{:.1}",
                    u.moves_left as f32 / 2.0,
                    u.moves as f32 / 2.0
                ),
            ] {
                draw_text(&line, x + 12.0, y, 18.0, WHITE);
                y += 20.0;
            }
            if !u.abilities.is_empty() {
                let text = u.abilities.join(", ").replace('_', " ");
                for line in crate::ui::wrap(&text, 16.0, PANEL_W - 24.0).iter().take(3) {
                    draw_text(line, x + 12.0, y, 16.0, LIGHTGRAY);
                    y += 18.0;
                }
            }
        }

        // Buttons.
        let by = h - 190.0;
        let bw = (PANEL_W - 36.0) / 2.0;
        let sel = self.battle_ui.selected;
        let mut order = None;
        if button(
            Rect::new(x + 12.0, by, bw, 32.0),
            "Done (Space)",
            my_turn && sel.is_some(),
        ) {
            order = sel.map(|u| BattleCommand::Done { unit: u });
        }
        if button(
            Rect::new(x + 24.0 + bw, by, bw, 32.0),
            "Next (Tab)",
            my_turn,
        ) {
            self.select_next(sel);
        }
        if button(
            Rect::new(x + 12.0, by + 40.0, bw, 32.0),
            "End turn",
            my_turn,
        ) {
            order = Some(BattleCommand::EndTurn);
        }
        let flee_label = if self.battle_ui.confirm_flee {
            "Sure?"
        } else {
            "Flee"
        };
        if button(
            Rect::new(x + 24.0 + bw, by + 40.0, bw, 32.0),
            flee_label,
            my_turn,
        ) {
            if self.battle_ui.confirm_flee {
                order = Some(BattleCommand::Flee);
            } else {
                self.battle_ui.confirm_flee = true;
            }
        } else if is_mouse_button_pressed(MouseButton::Left) {
            self.battle_ui.confirm_flee = false;
        }
        if let Some(order) = order {
            let done = match order {
                BattleCommand::Done { unit } => Some(unit),
                _ => None,
            };
            self.battle_order(order);
            match done {
                Some(unit) => self.select_next(Some(unit)),
                None => self.battle_ui.selected = None,
            }
            self.battle_ui.confirm_flee = false;
        }

        // The last lines of the fight.
        let mut ly = by + 92.0;
        for line in self.battle_ui.log.iter().rev().take(4).rev() {
            for l in crate::ui::wrap(line, 15.0, PANEL_W - 24.0).iter().take(1) {
                draw_text(l, x + 12.0, ly, 15.0, LIGHTGRAY);
                ly += 16.0;
            }
        }
        draw_text(
            "Click: select / move / attack     Tab: next unit     Enter: end turn",
            8.0,
            h - 10.0,
            16.0,
            LIGHTGRAY,
        );
    }

    fn player_label(&self, player: f4x_core::city::PlayerId) -> String {
        if player == f4x_core::units::NEUTRAL {
            return "Monsters".to_string();
        }
        self.game
            .player(player)
            .map_or_else(|| "?".to_string(), |p| p.name.clone())
    }
}

/// Scale and origin (screen position of the native origin) for a grid fitted into
/// `width` by `height`.
pub(crate) fn grid_view(width: f32, height: f32) -> (f32, f32, f32) {
    let (gw, gh) = grid_extent();
    let scale = (width / gw).min(height / gh).max(0.5);
    let ox = (width - gw * scale) / 2.0 + (GRID_H - 1) as f32 * STEP_X * scale;
    let oy = (height - gh * scale) / 2.0 + HEADROOM * scale;
    (scale, ox, oy)
}

/// A cell-shaped diamond at a cell's top-left.
fn diamond(sx: f32, sy: f32, scale: f32, color: Color, fill: bool) {
    let (w, h) = (CELL_W * scale, CELL_H * scale);
    let top = vec2(sx + w / 2.0, sy);
    let right = vec2(sx + w, sy + h / 2.0);
    let bottom = vec2(sx + w / 2.0, sy + h);
    let left = vec2(sx, sy + h / 2.0);
    if fill {
        draw_triangle(top, right, bottom, color);
        draw_triangle(top, bottom, left, color);
    } else {
        let t = (scale * 0.8).max(1.0);
        draw_line(top.x, top.y, right.x, right.y, t, color);
        draw_line(right.x, right.y, bottom.x, bottom.y, t, color);
        draw_line(bottom.x, bottom.y, left.x, left.y, t, color);
        draw_line(left.x, left.y, top.x, top.y, t, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_is_found_by_its_centre() {
        for pos in [(0, 0), (10, 11), (20, 21), (3, 18)] {
            let (x, y) = cell_origin(pos);
            assert_eq!(cell_at(x + CELL_W / 2.0, y + CELL_H / 2.0), Some(pos));
        }
    }

    #[test]
    fn nothing_is_found_off_the_grid() {
        assert_eq!(cell_at(-1000.0, 10.0), None);
        assert_eq!(cell_at(0.0, -50.0), None);
    }

    #[test]
    fn neighbours_step_by_the_original_amounts() {
        let (x0, y0) = cell_origin((5, 5));
        assert_eq!(cell_origin((6, 5)), (x0 + 16.0, y0 + 8.0));
        assert_eq!(cell_origin((5, 6)), (x0 - 16.0, y0 + 8.0));
    }

    #[test]
    fn the_ground_set_follows_terrain_and_plane() {
        assert_eq!(
            terrain_set(Some(TerrainClass::Desert), Plane::Arcanus),
            "cmbdesrt"
        );
        assert_eq!(
            terrain_set(Some(TerrainClass::Desert), Plane::Myrror),
            "cmbdesrc"
        );
        assert_eq!(
            terrain_set(Some(TerrainClass::Forest), Plane::Arcanus),
            "cmbgrass"
        );
        assert_eq!(terrain_set(None, Plane::Myrror), "cmbgrasc");
    }

    #[test]
    fn events_are_told_with_unit_names() {
        let names = vec!["Spearmen".to_string(), "Orc".to_string()];
        let line = describe(
            &names,
            &BattleEvent::Attacked {
                attacker: 0,
                defender: 1,
                dealt: 4,
                dealt_kills: 1,
                taken: 0,
                taken_kills: 0,
            },
        )
        .unwrap();
        assert_eq!(line, "Spearmen hits Orc: 4 damage, 1 figure lost");
        assert!(describe(
            &names,
            &BattleEvent::Moved {
                unit: 0,
                path: vec![]
            }
        )
        .is_none());
    }
}
