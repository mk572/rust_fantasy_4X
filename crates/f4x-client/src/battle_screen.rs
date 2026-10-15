//! The battle screen: the combat grid in the original's isometric projection, the units on
//! it, and the orders a player can give. The fight itself is the core's (`f4x_core::battle`).
//!
//! The grid uses the original's 32 by 16 pixel cells and its step of (16, 8) per square
//! (`Combat_Grid_Screen_Coordinates`), scaled to fit the window. The original shows only a
//! band of its 21 by 22 squares; the core plays on all of them, so all are drawn.

use std::collections::{HashMap, VecDeque};

use f4x_core::battle::ai::next_command;
use f4x_core::battle::map::{distance, Cell, Pos, Wall, GRID_H, GRID_W};
use f4x_core::battle::{
    Battle, BattleCommand, BattleEvent, BattleUnit, Confusion, RangedKind, Side, SpellAim,
};
use f4x_core::content::TerrainClass;
use f4x_core::map::Plane;
use f4x_core::rules::defs::SpellTarget;
use f4x_core::rules::SpellId;
use f4x_core::{Command, Event};
use macroquad::prelude::*;

use crate::battle_anim::{self, Anim, Visual};
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
    /// The screen's state has been set up for the battle being fought.
    pub(crate) ready: bool,
    selected: Option<usize>,
    ai_timer: f32,
    log: Vec<String>,
    /// The fleeing button was pressed once and waits for a second press.
    confirm_flee: bool,
    /// Unit names by id, kept while the battle exists so the last events can be told after
    /// the game has dropped the battle.
    names: Vec<String>,
    /// The battle as it was before the orders now being played; shown while they animate.
    shown: Option<Battle>,
    queue: VecDeque<Anim>,
    /// The animation under way and how many seconds it has run.
    playing: Option<(Anim, f32)>,
    /// Which way each unit looks (0 north, clockwise), by unit id.
    facing: HashMap<usize, u8>,
    /// The wizard's spell list is open.
    spell_list: bool,
    /// A spell chosen and waiting for its target (and the mana put into it).
    aiming: Option<Aiming>,
}

/// A spell the player has picked.
#[derive(Debug, Clone, Copy)]
struct Aiming {
    spell: SpellId,
    /// Mana put into it, between its cost and the most the wizard can afford.
    mana: i32,
    most: i32,
    target: SpellTarget,
    takes_extra_mana: bool,
}

impl BattleUi {
    fn animating(&self) -> bool {
        self.shown.is_some()
    }
}

/// Top-left of a square's cell, relative to the grid's origin, in native pixels.
pub(crate) fn cell_origin(pos: Pos) -> (f32, f32) {
    cell_origin_f((pos.0 as f32, pos.1 as f32))
}

/// The same for a position between squares.
pub(crate) fn cell_origin_f(pos: (f32, f32)) -> (f32, f32) {
    ((pos.0 - pos.1) * STEP_X, (pos.0 + pos.1) * STEP_Y)
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
pub(crate) fn describe(
    names: &[String],
    spell: &dyn Fn(SpellId) -> String,
    event: &BattleEvent,
) -> Option<String> {
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
        BattleEvent::WallAttacked {
            unit, destroyed, ..
        } => format!(
            "{} strikes the wall{}",
            name(*unit),
            if *destroyed { " and it falls" } else { "" }
        ),
        BattleEvent::UnitDied { unit } => format!("{} is destroyed", name(*unit)),
        BattleEvent::SpellCast { spell: s, .. } => format!("{} is cast", spell(*s)),
        BattleEvent::SpellCountered { spell: s, .. } => {
            format!("Counter Magic stops {}", spell(*s))
        }
        BattleEvent::SpellDamaged { unit, dealt, kills } => {
            format!(
                "{} takes {dealt} damage{}",
                name(*unit),
                self::kills(*kills)
            )
        }
        BattleEvent::SpellImmune { unit } => format!("{} is unharmed", name(*unit)),
        BattleEvent::SpellHealed { unit, healed } => {
            format!("{} heals {healed}", name(*unit))
        }
        BattleEvent::SpellEnchanted { unit, spell: s } => {
            format!("{} is under {}", name(*unit), spell(*s))
        }
        BattleEvent::SpellDispelled { unit, removed } => {
            format!("Dispel on {}: {removed} removed", name(*unit))
        }
        BattleEvent::Summoned { unit } => format!("{} appears", name(*unit)),
        BattleEvent::Raised { unit } => format!("{} rises again as undead", name(*unit)),
        BattleEvent::FieldRaised { spell: s, .. } => format!("{} covers the field", spell(*s)),
        BattleEvent::SpellResisted { unit } => format!("{} resists", name(*unit)),
        BattleEvent::SpellSlew { unit, kills } => {
            format!("{} loses {kills} figures to the spell", name(*unit))
        }
        BattleEvent::FieldStruck { unit, dealt, kills } => {
            format!(
                "{} takes {dealt} damage{}",
                name(*unit),
                self::kills(*kills)
            )
        }
        BattleEvent::Terrified { unit } => format!("{} cowers in terror", name(*unit)),
        BattleEvent::Confused { unit, act } => match act {
            Confusion::Idle => format!("{} stands about, confused", name(*unit)),
            Confusion::Wandered => format!("{} wanders off, confused", name(*unit)),
            Confusion::Seized => format!("{} falls under the enemy's orders", name(*unit)),
        },
        BattleEvent::ManaLeaked { .. } => "Mana drains away".to_string(),
        BattleEvent::CounterMagicRaised { strength, .. } => {
            format!("Counter Magic {strength} stands against spells")
        }
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
        self.init_battle_ui();
        if self.game.in_battle() {
            self.screen = Screen::Battle;
        }
    }

    /// Set the screen up for the battle in progress (it may have been started by the console).
    pub(crate) fn init_battle_ui(&mut self) {
        self.battle_ui = BattleUi::default();
        if let Some(battle) = self.game.battle() {
            self.battle_ui.ready = true;
            self.battle_ui.names = battle.units.iter().map(|u| u.name.clone()).collect();
            let first = battle
                .active_units(battle.active)
                .find(|u| unfinished(u))
                .map(|u| u.id);
            self.battle_ui.selected = first;
            // The attackers look east across the field, the defenders west.
            for u in &battle.units {
                let look = if u.side == Side::Attacker { 3 } else { 7 };
                self.battle_ui.facing.insert(u.id, look);
            }
        }
    }

    /// Pit the human's units against monsters on the same square (for screenshots and tests).
    pub fn open_test_battle(&mut self) {
        let mine = self.game.units().player_units(HUMAN);
        let Some(pos) = mine
            .first()
            .and_then(|&u| self.game.units().unit_position(u))
        else {
            return;
        };
        let rules = &self.game.content().rules;
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
                .filter(|&u| self.game.units().unit_position(u) == Some(pos))
                .collect(),
            defenders,
        });
    }

    /// Send one order to the battle and tell what came of it.
    fn battle_order(&mut self, order: BattleCommand) {
        let before = self.game.battle().cloned();
        let events = self.game.apply(Command::Battle(order));
        // A summoned creature has a name too.
        if let Some(b) = self.game.battle() {
            if b.units.len() != self.battle_ui.names.len() {
                self.battle_ui.names = b.units.iter().map(|u| u.name.clone()).collect();
            }
        }
        let names = self.battle_ui.names.clone();
        let mut anims = Vec::new();
        for event in &events {
            match event {
                Event::Battle(e) => {
                    let spell_name =
                        |id: SpellId| self.game.content().rules.spells.get(id).name.clone();
                    if let Some(line) = describe(&names, &spell_name, e) {
                        self.battle_ui.log.push(line);
                    }
                    if let Some(b) = &before {
                        self.animate(b, e, &mut anims);
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
        if !anims.is_empty() {
            if self.battle_ui.shown.is_none() {
                self.battle_ui.shown = before;
            }
            self.battle_ui.queue.extend(anims);
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

    /// Turn one battle event into animations and remember which way units now look.
    /// `before` is the battle as it stood when the order was given.
    fn animate(&mut self, before: &Battle, event: &BattleEvent, out: &mut Vec<Anim>) {
        let cell_of = |id: usize| before.unit(id).map(|u| u.cell);
        match event {
            BattleEvent::Moved { unit, path } => {
                if let (Some(start), Some(&last)) = (cell_of(*unit), path.last()) {
                    let prev = path
                        .len()
                        .checked_sub(2)
                        .and_then(|i| path.get(i).copied())
                        .unwrap_or(start);
                    self.battle_ui
                        .facing
                        .insert(*unit, battle_anim::facing(prev, last));
                }
                out.push(Anim::Move {
                    unit: *unit,
                    path: path.clone(),
                });
            }
            BattleEvent::Attacked {
                attacker, defender, ..
            }
            | BattleEvent::Shot {
                attacker, defender, ..
            } => {
                if let (Some(a), Some(d)) = (cell_of(*attacker), cell_of(*defender)) {
                    self.battle_ui
                        .facing
                        .insert(*attacker, battle_anim::facing(a, d));
                    self.battle_ui
                        .facing
                        .insert(*defender, battle_anim::facing(d, a));
                }
                out.push(if matches!(event, BattleEvent::Shot { .. }) {
                    Anim::Shot {
                        attacker: *attacker,
                        defender: *defender,
                    }
                } else {
                    Anim::Attack {
                        attacker: *attacker,
                        defender: *defender,
                    }
                });
            }
            BattleEvent::UnitDied { unit } => out.push(Anim::Death { unit: *unit }),
            _ => {}
        }
    }

    /// Run the animation clock; when the last one has played, show the real battle again.
    fn advance_animation(&mut self, dt: f32) {
        let ui = &mut self.battle_ui;
        if let Some((anim, t)) = ui.playing.as_mut() {
            *t += dt;
            if *t >= anim.duration() {
                ui.playing = None;
            }
        }
        if ui.playing.is_none() {
            ui.playing = ui.queue.pop_front().map(|a| (a, 0.0));
        }
        if ui.playing.is_none() && ui.queue.is_empty() {
            ui.shown = None;
        }
    }

    /// How a unit of the battle being drawn looks right now.
    fn visual_of(&self, battle: &Battle, id: usize) -> Visual {
        let Some(u) = battle.unit(id) else {
            return Visual::still((0, 0), 2);
        };
        let base = self.battle_ui.facing.get(&id).copied().unwrap_or(2);
        if let Some((anim, t)) = &self.battle_ui.playing {
            if let Some(v) = battle_anim::visual_in(anim, *t, id, u.cell, base) {
                return v;
            }
            let other = match anim {
                Anim::Attack { attacker, defender } | Anim::Shot { attacker, defender } => {
                    let o = if *attacker == id { defender } else { attacker };
                    battle.unit(*o).map(|o| o.cell)
                }
                _ => None,
            };
            if let Some(other) = other {
                if let Some(v) = battle_anim::visual_between(anim, *t, id, u.cell, other, base) {
                    return v;
                }
            }
        }
        Visual::still(u.cell, base)
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
        let dt = get_frame_time();
        self.advance_animation(dt);
        // While orders animate, the battle as it was is drawn; afterwards the real one.
        let animating = self.battle_ui.animating();
        let Some(battle) = self
            .battle_ui
            .shown
            .clone()
            .or_else(|| self.game.battle().cloned())
        else {
            self.screen = Screen::Map;
            return;
        };
        if self.battle_ui.names.len() != battle.units.len() {
            self.battle_ui.names = battle.units.iter().map(|u| u.name.clone()).collect();
        }
        self.battle_ui.ai_timer -= dt;

        let mine = Self::human_side(&battle);
        let my_turn = mine == Some(battle.active) && !animating;
        // The other side plays by itself, once nothing is moving.
        if !animating
            && mine != Some(battle.active)
            && self.battle_ui.ai_timer <= 0.0
            && self.game.in_battle()
        {
            self.battle_ui.ai_timer = AI_DELAY;
            let order = self
                .game
                .next_battle_command()
                .unwrap_or_else(|| next_command(&battle));
            self.battle_order(order);
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
        let battle = self
            .battle_ui
            .shown
            .clone()
            .or_else(|| self.game.battle().cloned())
            .unwrap_or(battle);
        self.draw_battle(&battle, my_turn, hovered);
    }

    fn battle_input(&mut self, battle: &Battle, hovered: Option<Pos>) {
        if self.battle_ui.aiming.is_some() || self.battle_ui.spell_list {
            return self.spell_input(battle, hovered);
        }
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
            // A wall crusher strikes a wall section: with its blows next to it, or by shooting.
            (None, Some(u))
                if battle.map.has_wall(cell)
                    && u.has("wall_crusher")
                    && u.side == Side::Attacker
                    && !u.attacked =>
            {
                let order = if distance(u.cell, cell) == 1 {
                    BattleCommand::AttackWall { unit: u.id, cell }
                } else if can_shoot(u) {
                    BattleCommand::ShootWall { unit: u.id, cell }
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

    /// The human wizard's casting in the battle.
    fn my_caster(battle: &Battle) -> Option<f4x_core::battle::BattleCaster> {
        battle.caster(Self::human_side(battle)?).copied()
    }

    /// Choose a spell from the list: cast it at once if it needs no target and no extra mana,
    /// otherwise wait for the target (and the amount of mana).
    fn pick_spell(&mut self, spell: &f4x_core::battle::spells::BattleSpell) {
        self.battle_ui.spell_list = false;
        let cost = self.game.content().rules.spells.get(spell.spell).cost as i32;
        let aimless = matches!(spell.target, SpellTarget::None | SpellTarget::Battlefield);
        if aimless && !spell.takes_extra_mana {
            self.battle_order(BattleCommand::Cast {
                spell: spell.spell,
                aim: SpellAim::None,
                mana: 0,
            });
            self.battle_ui.selected = None;
            return;
        }
        // Animate Dead and Raise Dead take the strongest of the fallen (the fallen are not on the field to
        // click; a list to choose from is task 175).
        if [
            f4x_core::battle::spells::ANIMATE_DEAD,
            f4x_core::battle::spells::RAISE_DEAD,
        ]
        .into_iter()
        .any(|h| self.game.spell_has_hook(spell.spell, h))
        {
            let fallen = self
                .game
                .legal_battle_casts()
                .into_iter()
                .find(|c| c.spell == spell.spell)
                .and_then(|c| {
                    let battle = self.game.battle()?;
                    c.aims
                        .into_iter()
                        .filter_map(|a| match a {
                            SpellAim::Unit(id) => Some(id),
                            _ => None,
                        })
                        .max_by_key(|&id| battle.unit(id).map_or(0, |u| u.max_hit_points()))
                });
            if let Some(id) = fallen {
                self.battle_order(BattleCommand::Cast {
                    spell: spell.spell,
                    aim: SpellAim::Unit(id),
                    mana: 0,
                });
                self.battle_ui.selected = None;
            }
            return;
        }
        let most = self
            .game
            .battle_spell_most_mana(HUMAN, spell.spell)
            .max(cost);
        self.battle_ui.aiming = Some(Aiming {
            spell: spell.spell,
            mana: cost,
            most,
            target: spell.target,
            takes_extra_mana: spell.takes_extra_mana,
        });
    }

    /// Keys and clicks while the spell list is open or a spell waits for its target.
    fn spell_input(&mut self, battle: &Battle, hovered: Option<Pos>) {
        if is_key_pressed(KeyCode::Escape) {
            self.battle_ui.aiming = None;
            self.battle_ui.spell_list = false;
            return;
        }
        let Some(mut aim) = self.battle_ui.aiming else {
            return;
        };
        // More or less mana: Up/Down by one, Page Up/Down by five.
        let cost = self.game.content().rules.spells.get(aim.spell).cost as i32;
        let step = |up: KeyCode, down: KeyCode, by: i32| {
            i32::from(is_key_pressed(up)) * by - i32::from(is_key_pressed(down)) * by
        };
        if aim.takes_extra_mana {
            let change = step(KeyCode::Up, KeyCode::Down, 1)
                + step(KeyCode::PageUp, KeyCode::PageDown, 5)
                + step(KeyCode::Equal, KeyCode::Minus, 1);
            aim.mana = (aim.mana + change).clamp(cost, aim.most);
            self.battle_ui.aiming = Some(aim);
        }
        let aimless = matches!(aim.target, SpellTarget::None | SpellTarget::Battlefield);
        let target = if aimless {
            is_key_pressed(KeyCode::Enter).then_some(SpellAim::None)
        } else if is_mouse_button_pressed(MouseButton::Left) {
            let (w, h) = (screen_width(), screen_height());
            let on_grid = mouse_position().0 < w - PANEL_W && mouse_position().1 < h - 30.0;
            hovered
                .filter(|_| on_grid)
                .and_then(|cell| match aim.target {
                    SpellTarget::Tile => Some(SpellAim::Tile(cell)),
                    _ => battle.unit_at(cell).map(|u| SpellAim::Unit(u.id)),
                })
        } else {
            None
        };
        if let Some(target) = target {
            self.battle_ui.aiming = None;
            self.battle_order(BattleCommand::Cast {
                spell: aim.spell,
                aim: target,
                mana: if aim.takes_extra_mana {
                    aim.mana as u32
                } else {
                    0
                },
            });
            self.battle_ui.selected = None;
        }
    }

    /// Whether the spell may be aimed at the unit or square (for the markers).
    fn spell_aim_ok(&self, battle: &Battle, aim: &Aiming, cell: Pos) -> bool {
        let Some(side) = Self::human_side(battle) else {
            return false;
        };
        match aim.target {
            SpellTarget::Tile => battle.is_summon_square(side, cell),
            target => battle.unit_at(cell).is_some_and(|u| match target {
                SpellTarget::OwnUnit => u.side == side,
                SpellTarget::EnemyUnit => u.side != side,
                _ => true,
            }),
        }
    }

    /// The list of spells the wizard can cast, drawn over the field.
    fn draw_spell_list(&mut self, battle: &Battle) {
        let spells = self.game.battle_spells(HUMAN);
        let (row_h, width) = (26.0, 360.0);
        let height = (spells.len().max(1) as f32 * row_h + 34.0).min(screen_height() - 80.0);
        let (x, y) = (20.0, 20.0);
        draw_rectangle(x, y, width, height, PANEL);
        draw_rectangle_lines(x, y, width, height, 2.0, GOLD);
        let caster = Self::my_caster(battle);
        let head = caster.map_or("No wizard".to_string(), |c| {
            format!("Skill {}   Mana {}   Esc closes", c.skill_left, c.mana)
        });
        draw_text(&head, x + 10.0, y + 22.0, 18.0, WHITE);
        if spells.is_empty() {
            draw_text("No spell to cast here", x + 10.0, y + 50.0, 18.0, LIGHTGRAY);
        }
        let room = ((height - 34.0) / row_h) as usize;
        for (i, spell) in spells.iter().take(room).enumerate() {
            let def = self.game.content().rules.spells.get(spell.spell);
            let label = format!(
                "{}   {}{}",
                def.name,
                spell.cost,
                if spell.takes_extra_mana { "+" } else { "" }
            );
            let rect = Rect::new(
                x + 8.0,
                y + 30.0 + i as f32 * row_h,
                width - 16.0,
                row_h - 2.0,
            );
            if button(rect, &label, spell.castable) {
                self.pick_spell(spell);
            }
        }
    }

    /// The prompt for the spell waiting for its target, and the squares it can be aimed at.
    fn draw_aiming(&mut self, battle: &Battle, hovered: Option<Pos>, origin: (f32, f32, f32)) {
        let Some(aim) = self.battle_ui.aiming else {
            return;
        };
        let (ox, oy, scale) = origin;
        for gy in 0..GRID_H {
            for gx in 0..GRID_W {
                let cell = (gx, gy);
                if self.spell_aim_ok(battle, &aim, cell) {
                    let (x, y) = cell_origin(cell);
                    let hot = hovered == Some(cell);
                    let color = if hot {
                        Color::new(0.9, 0.4, 1.0, 1.0)
                    } else {
                        Color::new(0.7, 0.3, 0.9, 0.6)
                    };
                    diamond(ox + x * scale, oy + y * scale, scale, color, hot);
                }
            }
        }
        let name = self.game.content().rules.spells.get(aim.spell).name.clone();
        let aimless = matches!(aim.target, SpellTarget::None | SpellTarget::Battlefield);
        let mut text = format!(
            "{name}: {}",
            if aimless {
                "Enter casts"
            } else {
                "click a target"
            }
        );
        if aim.takes_extra_mana {
            text += &format!("   mana {} (Up/Down, most {})", aim.mana, aim.most);
        }
        text += "   Esc cancels";
        draw_rectangle(8.0, 6.0, text.len() as f32 * 8.5 + 16.0, 26.0, PANEL);
        draw_text(&text, 16.0, 25.0, 20.0, GOLD);
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
            Some(site) => (site.plane, self.game.economy().terrain_class_at(site)),
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
        for gy in 0..GRID_H {
            for gx in 0..GRID_W {
                if battle.map.wall((gx, gy)) != Wall::None {
                    items.push((gx + gy, false, (gx, gy), 1));
                }
            }
        }
        let mut looks: HashMap<usize, Visual> = HashMap::new();
        for u in battle.units.iter().filter(|u| u.is_active()) {
            let vis = self.visual_of(battle, u.id);
            items.push(((vis.at.0 + vis.at.1).round() as i32, true, u.cell, u.id));
            looks.insert(u.id, vis);
        }
        items.sort_by_key(|&(depth, is_unit, pos, _)| (depth, is_unit, pos));
        for (_, is_unit, pos, id) in items {
            if is_unit {
                let idle = self.battle_ui.selected == Some(id) && my_turn;
                self.draw_unit(battle, id, looks[&id], idle, (ox, oy, scale), my_turn);
            } else if id == 1 {
                draw_wall(battle.map.wall(pos), at(pos), scale);
            } else {
                self.draw_scenery(set, class, pos, at(pos), scale);
            }
        }
        self.draw_missile(battle, (ox, oy, scale));

        // Markers over everything: the selected unit and the blow just struck.
        if let Some(u) = selected {
            let (sx, sy) = at(u.cell);
            let blink = (get_time() * 3.0) as i64 % 2 == 0;
            diamond(sx, sy, scale, if blink { GOLD } else { WHITE }, false);
            if my_turn {
                if u.has("wall_crusher") && u.side == Side::Attacker && !u.attacked {
                    for wall in battle.map.standing_walls() {
                        if distance(u.cell, wall) == 1 || can_shoot(u) {
                            let (wx, wy) = at(wall);
                            diamond(wx, wy, scale, Color::new(1.0, 0.6, 0.1, 0.9), false);
                        }
                    }
                }
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
        self.draw_aiming(battle, hovered, (ox, oy, scale));
        self.draw_battle_panel(battle, my_turn, hovered);
        if my_turn && self.battle_ui.spell_list {
            self.draw_spell_list(battle);
        }
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

    #[allow(clippy::too_many_arguments)]
    fn draw_unit(
        &mut self,
        battle: &Battle,
        id: usize,
        vis: Visual,
        idle: bool,
        (ox, oy, scale): (f32, f32, f32),
        my_turn: bool,
    ) {
        let Some(u) = battle.unit(id) else {
            return;
        };
        let (x, y) = cell_origin_f(vis.at);
        let (sx, sy) = (ox + x * scale, oy + y * scale);
        let owner = battle.player_of(u.side).0;
        let banner = self.banner_color(owner);
        let banner_index = self.banner_index(owner);
        let fade = vis.alpha;
        // A coloured base in the owner's banner.
        diamond(
            sx,
            sy,
            scale,
            Color {
                a: 0.55 * fade,
                ..banner
            },
            true,
        );
        diamond(sx, sy, scale, Color { a: fade, ..banner }, false);
        let art = self
            .game
            .content()
            .rules
            .units
            .defs()
            .iter()
            .find(|d| d.id == u.unit_id)
            .and_then(|d| d.art.clone());
        let unit_type = art
            .as_ref()
            .and_then(|(lbx, entry)| crate::art::unit_type_number(lbx, *entry as usize));
        let frame = if idle && vis.frame == 1 {
            (get_time() * 3.0) as u8 % 3
        } else {
            vis.frame
        };
        let spent = my_turn && u.side == battle.active && !unfinished(u);
        let shade = if spent { 0.65 } else { 1.0 };
        let tint = Color::new(
            shade,
            shade * (1.0 - 0.6 * vis.hurt),
            shade * (1.0 - 0.6 * vis.hurt),
            fade,
        );
        let cx = sx + CELL_W * scale / 2.0;
        let mut drew = false;
        if let Some(ty) = unit_type {
            // Every figure the unit still has, the back ones first.
            let count = u.figures_start as usize;
            let mut spots: Vec<(i32, i32)> = (0..u.figures as usize)
                .map(|i| battle_anim::figure_offset(count, i))
                .collect();
            spots.sort_by_key(|&(_, fy)| fy);
            for (fx, fy) in spots {
                if let Some(tex) = self.art.figure(ty, vis.facing, frame, banner_index) {
                    let size = tex.size() * scale;
                    draw_texture_ex(
                        tex,
                        sx + (CELL_W / 2.0 - 14.0 + fx as f32) * scale,
                        sy + (CELL_H / 2.0 - 23.0 + fy as f32) * scale,
                        tint,
                        DrawTextureParams {
                            dest_size: Some(size),
                            ..Default::default()
                        },
                    );
                    drew = true;
                }
            }
        }
        if !drew {
            // No figures extracted: the overland picture stands in.
            let icon = art.and_then(|(lbx, entry)| self.art.picture(&lbx, entry as usize, 0));
            let base = sy + CELL_H * scale * 0.7;
            match icon {
                Some(tex) => {
                    let size = tex.size() * scale * 1.3;
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
        }
        if fade < 1.0 {
            return;
        }
        // Health bar and figure count under the feet.
        let max = u.max_hit_points().max(1) as f32;
        let (bw, bh) = (CELL_W * scale * 0.6, (2.0 * scale).max(2.0));
        let bar_rect = Rect::new(cx - bw / 2.0, sy + (CELL_H + 1.0) * scale, bw, bh);
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

    /// A button drawn with one of the original's `compix.lbx` pictures (frame 1 is pressed),
    /// or a plain one without the art. Returns true on the frame it is clicked.
    fn compix_button(
        &mut self,
        entry: usize,
        label: &str,
        (x, y): (f32, f32),
        enabled: bool,
    ) -> bool {
        const SCALE: f32 = 3.0;
        let Some(first) = self.art.picture("compix", entry, 0).cloned() else {
            return button(Rect::new(x, y, 78.0, 30.0), label, enabled);
        };
        let rect = Rect::new(x, y, first.width() * SCALE, first.height() * SCALE);
        let hover = rect.contains(mouse_position().into());
        let down = enabled && hover && is_mouse_button_down(MouseButton::Left);
        let tex = self
            .art
            .picture("compix", entry, usize::from(down))
            .cloned()
            .unwrap_or(first);
        draw_texture_ex(
            &tex,
            rect.x,
            rect.y,
            if enabled {
                WHITE
            } else {
                Color::new(1.0, 1.0, 1.0, 0.4)
            },
            DrawTextureParams {
                dest_size: Some(rect.size()),
                ..Default::default()
            },
        );
        enabled && hover && is_mouse_button_pressed(MouseButton::Left)
    }

    /// The arrow or bolt in flight during a shot.
    fn draw_missile(&mut self, battle: &Battle, (ox, oy, scale): (f32, f32, f32)) {
        let Some((Anim::Shot { attacker, defender }, t)) = self.battle_ui.playing.clone() else {
            return;
        };
        let Some(progress) = battle_anim::missile_progress(t) else {
            return;
        };
        let (Some(a), Some(d)) = (battle.unit(attacker), battle.unit(defender)) else {
            return;
        };
        let centre = |cell: Pos| {
            let (x, y) = cell_origin(cell);
            (
                ox + (x + CELL_W / 2.0) * scale,
                oy + (y + CELL_H / 2.0 - 8.0) * scale,
            )
        };
        let (from, to) = (centre(a.cell), centre(d.cell));
        let arc = (progress * std::f32::consts::PI).sin() * 10.0 * scale;
        let at = (
            from.0 + (to.0 - from.0) * progress,
            from.1 + (to.1 - from.1) * progress - arc,
        );
        if a.ranged_kind == RangedKind::Magic {
            draw_circle(at.0, at.1, 4.0 * scale, Color::new(1.0, 0.9, 0.5, 0.35));
            draw_circle(at.0, at.1, 2.0 * scale, Color::new(1.0, 1.0, 0.9, 1.0));
        } else {
            let (dx, dy) = (to.0 - from.0, to.1 - from.1 - 2.0 * arc);
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let (ux, uy) = (dx / len * 4.0 * scale, dy / len * 4.0 * scale);
            draw_line(
                at.0 - ux,
                at.1 - uy,
                at.0 + ux,
                at.1 + uy,
                (scale * 0.8).max(1.5),
                Color::new(0.85, 0.7, 0.4, 1.0),
            );
        }
    }

    fn draw_battle_panel(&mut self, battle: &Battle, my_turn: bool, hovered: Option<Pos>) {
        let (w, h) = (screen_width(), screen_height());
        let x = w - PANEL_W;
        draw_rectangle(x, 0.0, PANEL_W, h, PANEL);
        let mut y = 28.0;
        let attacker = self.player_label(battle.attacker);
        let defender = self.player_label(battle.defender);
        draw_text(
            format!("{attacker}  vs  {defender}"),
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
            format!("Turn {}   {who}", battle.turn),
            x + 12.0,
            y,
            20.0,
            WHITE,
        );
        y += 22.0;
        if let Some(c) = Self::my_caster(battle) {
            draw_text(
                format!(
                    "Skill {}   Mana {}   Range x{:.1}{}",
                    c.skill_left,
                    c.mana,
                    c.range as f32 / 10.0,
                    if c.cast_this_turn { "   (cast)" } else { "" }
                ),
                x + 12.0,
                y,
                18.0,
                Color::new(0.7, 0.8, 1.0, 1.0),
            );
        }
        y += 22.0;
        // The enchantments over the field, by who cast them.
        let mine = Self::human_side(battle);
        let over: Vec<String> = battle
            .fields()
            .map(|(side, _, spell)| {
                let name = &self.game.content().rules.spells.get(spell).name;
                if Some(side) == mine {
                    name.clone()
                } else {
                    format!("{name} (enemy)")
                }
            })
            .collect();
        if !over.is_empty() {
            for line in crate::ui::wrap(&over.join(", "), 16.0, PANEL_W - 24.0)
                .iter()
                .take(2)
            {
                draw_text(
                    line,
                    x + 12.0,
                    y - 4.0,
                    16.0,
                    Color::new(0.8, 0.7, 1.0, 1.0),
                );
                y += 17.0;
            }
        }

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

        // The original's picture buttons (`compix.lbx`): Spell, Wait, Done / Auto, Info, Flee.
        let by = h - 190.0;
        let sel = self.battle_ui.selected;
        let mut order = None;
        let col = |i: f32| x + 12.0 + i * 92.0;
        // Auto-play and the information window are not built yet (tasks 133, 132).
        let can_cast = my_turn
            && self.battle_ui.aiming.is_none()
            && Self::my_caster(battle).is_some_and(|c| !c.cast_this_turn);
        let spell = self.compix_button(1, "Spell", (col(0.0), by), can_cast);
        if spell || (can_cast && is_key_pressed(KeyCode::S)) {
            self.battle_ui.spell_list = !self.battle_ui.spell_list;
        }
        let wait = self.compix_button(2, "Wait", (col(1.0), by), my_turn);
        if wait || (my_turn && is_key_pressed(KeyCode::W)) {
            self.select_next(sel);
        }
        if self.compix_button(3, "Done", (col(2.0), by), my_turn && sel.is_some()) {
            order = sel.map(|u| BattleCommand::Done { unit: u });
        }
        self.compix_button(4, "Auto", (col(0.0), by + 40.0), false);
        self.compix_button(20, "Info", (col(1.0), by + 40.0), false);
        let sure = self.battle_ui.confirm_flee;
        let flee = self.compix_button(21, "Flee", (col(2.0), by + 40.0), my_turn);
        if flee || (my_turn && is_key_pressed(KeyCode::F)) {
            if sure {
                order = Some(BattleCommand::Flee);
            } else {
                self.battle_ui.confirm_flee = true;
            }
        } else if is_mouse_button_pressed(MouseButton::Left) {
            self.battle_ui.confirm_flee = false;
        }
        if sure {
            draw_text("Flee? Click again", col(2.0) - 40.0, by + 84.0, 16.0, GOLD);
        }
        // The original has no end-of-turn button (it ends when every unit is done).
        if button(
            Rect::new(x + 12.0, by + 92.0, PANEL_W - 24.0, 26.0),
            "End turn (Enter)",
            my_turn,
        ) {
            order = Some(BattleCommand::EndTurn);
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
        let mut ly = by + 138.0;
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
/// A section of the city's wall: a block of grey stone, or a heap of rubble where it fell.
fn draw_wall(wall: Wall, (sx, sy): (f32, f32), scale: f32) {
    let (w, h) = (CELL_W * scale, CELL_H * scale);
    let (top, left, right) = match wall {
        Wall::Standing => (
            Color::new(0.62, 0.6, 0.56, 1.0),
            Color::new(0.4, 0.38, 0.35, 1.0),
            Color::new(0.5, 0.48, 0.44, 1.0),
        ),
        _ => (
            Color::new(0.35, 0.3, 0.26, 0.9),
            Color::new(0.25, 0.22, 0.19, 0.9),
            Color::new(0.3, 0.26, 0.22, 0.9),
        ),
    };
    let rise = if wall == Wall::Standing {
        h * 1.1
    } else {
        h * 0.25
    };
    let (cx, mid_y) = (sx + w / 2.0, sy + h / 2.0);
    let (bottom, west, east, north) = (
        vec2(cx, sy + h),
        vec2(sx, mid_y),
        vec2(sx + w, mid_y),
        vec2(cx, sy),
    );
    let up = vec2(0.0, -rise);
    draw_triangle(west, bottom, west + up, left);
    draw_triangle(bottom, bottom + up, west + up, left);
    draw_triangle(bottom, east, east + up, right);
    draw_triangle(bottom, bottom + up, east + up, right);
    draw_triangle(north + up, east + up, west + up, top);
    draw_triangle(bottom + up, east + up, west + up, top);
}

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
        let spell = |_: SpellId| "Fire Bolt".to_string();
        let line = describe(
            &names,
            &spell,
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
            &spell,
            &BattleEvent::Moved {
                unit: 0,
                path: vec![]
            }
        )
        .is_none());
    }
}
