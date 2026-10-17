//! The battle screen: the combat grid in the original's isometric projection, the units on
//! it, and the orders a player can give. The fight itself is the core's (`f4x_core::battle`).
//!
//! The grid uses the original's 32 by 16 pixel cells and its step of (16, 8) per square
//! (`Combat_Grid_Screen_Coordinates`), scaled to fit the window. The core's squares are the
//! original's mirrored (`x = 20 - cgx`), so a square is drawn where the original draws the
//! mirrored one, and the picture comes out as the original's. Like the original this draws only
//! the band of 200 squares it plays on (the field generator shuts the rest).

use std::collections::{HashMap, VecDeque};

use f4x_core::battle::ai::next_command;
use f4x_core::battle::map::{distance, Pos, Wall, GRID_H, GRID_W};
use f4x_core::battle::{
    Battle, BattleCommand, BattleEvent, BattleUnit, Confusion, Fate, RangedKind, Side, SpellAim,
};
use f4x_core::map::Plane;
use f4x_core::rules::defs::SpellTarget;
use f4x_core::rules::SpellId;
use f4x_core::{Command, Event};
use macroquad::prelude::*;

use crate::battle_anim::{self, Anim, Visual};
use crate::battle_ground::Piece;
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
    /// The spell list is open: the wizard's, or the unit's when `spell_unit` is set.
    spell_list: bool,
    /// The unit whose own spells the list shows (a hero, a creature caster).
    spell_unit: Option<usize>,
    /// A spell chosen and waiting for its target (and the mana put into it).
    aiming: Option<Aiming>,
    /// Animate Dead or Raise Dead chosen: the list of the fallen to pick from.
    fallen_list: Option<SpellId>,
    /// The Auto button is on: the computer plays the human's side too, until the battle ends
    /// or the button is pressed again.
    auto: bool,
    /// The Magic Vortex being steered: click it, then a square next to it.
    steering: Option<usize>,
    /// The information window (the Info button) is open.
    info_open: bool,
}

/// A spell the player has picked.
#[derive(Debug, Clone)]
struct Aiming {
    /// The unit casting it, if it is not the wizard.
    unit: Option<usize>,
    spell: SpellId,
    /// Mana put into it, between its cost and the most the wizard can afford.
    mana: i32,
    most: i32,
    target: SpellTarget,
    takes_extra_mana: bool,
    /// For a spell aimed at a square: the squares the game accepts.
    tiles: Vec<Pos>,
}

impl BattleUi {
    fn animating(&self) -> bool {
        self.shown.is_some()
    }
}

/// The order for a spell cast by the wizard (`unit` none) or by a battle unit.
fn cast_order(unit: Option<usize>, spell: SpellId, aim: SpellAim, mana: u32) -> BattleCommand {
    match unit {
        Some(unit) => BattleCommand::UnitCast {
            unit,
            spell,
            aim,
            mana,
        },
        None => BattleCommand::Cast { spell, aim, mana },
    }
}

/// Top-left of a cell of the original's frame (`cgx`, `cgy`), relative to the grid's origin.
fn original_origin(cgx: f32, cgy: f32) -> (f32, f32) {
    ((cgx - cgy) * STEP_X, (cgx + cgy) * STEP_Y)
}

/// Top-left of a square's cell, relative to the grid's origin, in native pixels.
pub(crate) fn cell_origin(pos: Pos) -> (f32, f32) {
    cell_origin_f((pos.0 as f32, pos.1 as f32))
}

/// The same for a position between squares.
pub(crate) fn cell_origin_f(pos: (f32, f32)) -> (f32, f32) {
    original_origin((GRID_W - 1) as f32 - pos.0, pos.1)
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

/// Native extent of the band of squares the original plays on (the cell origins run from
/// -144 to 144 across and 88 to 248 down), with headroom for tall figures.
const BAND_X: (f32, f32) = (-144.0, 144.0 + CELL_W);
const BAND_Y: (f32, f32) = (88.0, 248.0 + CELL_H);

/// Native width and height of the grid, with headroom.
fn grid_extent() -> (f32, f32) {
    (BAND_X.1 - BAND_X.0, BAND_Y.1 - BAND_Y.0 + HEADROOM)
}

/// The original's picture file for the ground of a fight on this terrain.
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
        BattleEvent::UnitSpellCast { unit, spell: s, .. } => {
            format!("{} casts {}", name(*unit), spell(*s))
        }
        BattleEvent::WallDestroyed { .. } => "A section of the wall is destroyed".to_string(),
        BattleEvent::MudMade { .. } => "The ground turns to mud".to_string(),
        BattleEvent::FissureOpened { .. } => "The earth cracks open".to_string(),
        BattleEvent::VortexMade { .. } => "A magic vortex appears".to_string(),
        BattleEvent::VortexMoved { .. } => return None,
        BattleEvent::LifeDrained { unit, points } => {
            format!("{} loses {points} life", name(*unit))
        }
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
        BattleEvent::HeroRecalled { unit } => {
            format!("{} is recalled to the summoning circle", name(*unit))
        }
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

    /// The unit on a square, as far as the human's side can make it out: an invisible enemy
    /// that has not given itself away is not there.
    fn seen_unit_at(battle: &Battle, cell: Pos) -> Option<&BattleUnit> {
        let viewer = Self::human_side(battle);
        battle
            .unit_at(cell)
            .filter(|u| viewer.is_none_or(|v| battle.seen_by(v, u.id)))
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

    /// Fight a battle to the end without the battle screen (strategic combat only, or an
    /// attack the player left to the computer).
    pub(crate) fn auto_battle(&mut self, command: Command) {
        let events = self.game.apply(command);
        let won = events.iter().find_map(|e| match e {
            Event::BattleEnded { winner, .. } => Some(*winner == HUMAN),
            _ => None,
        });
        if let Some(won) = won {
            self.log
                .push(if won { "Victory!" } else { "Defeat" }.to_string());
        }
        let rest: Vec<Event> = events
            .into_iter()
            .filter(|e| !matches!(e, Event::Battle(_) | Event::BattleEnded { .. }))
            .collect();
        self.note(rest);
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
            // The attackers (at the original's high x) look towards low x, the defenders the other way.
            for u in &battle.units {
                let look = if u.side == Side::Attacker { 7 } else { 3 };
                self.battle_ui.facing.insert(u.id, look);
            }
        }
    }

    /// For the screenshot check: move the human's units three squares east and put the feature
    /// `id` (a tower, node or encounter site) there, so the battle has its building.
    pub fn set_test_feature(&mut self, id: &str) {
        let Some(pos) = self
            .game
            .units()
            .player_units(HUMAN)
            .first()
            .and_then(|&u| self.game.units().unit_position(u))
        else {
            return;
        };
        let Some(feature) = self.game.content().features.id_of(id) else {
            return;
        };
        // Out of the city, which would have the field to itself.
        let to = f4x_core::city::Position {
            x: pos.x + 3,
            ..pos
        };
        for unit in self.game.units().player_units(HUMAN) {
            self.game.cheat_move_unit(unit, to);
        }
        self.game.cheat_set_feature(to, Some(feature));
    }

    /// For the screenshot check: the kind of field the test battle is fought on. `river` and
    /// `sea` change the square under it (call this before [`App::open_test_battle`]), `walls` and
    /// `mud` put the city's magic walls and a patch of mud on the field (after it).
    pub fn set_test_field(&mut self, kind: &str) {
        let Some(pos) = self
            .game
            .units()
            .player_units(HUMAN)
            .first()
            .and_then(|&u| self.game.units().unit_position(u))
        else {
            return;
        };
        if !matches!(kind, "river" | "river-myrror" | "sea") {
            return;
        }
        let mut to = f4x_core::city::Position {
            x: pos.x + 3,
            ..pos
        };
        if kind == "river-myrror" {
            to.plane = f4x_core::map::Plane::Myrror;
        }
        for unit in self.game.units().player_units(HUMAN) {
            self.game.cheat_move_unit(unit, to);
        }
        if kind.starts_with("river") {
            let grass = self.game.content().terrains.id_of("grassland");
            if let Some(t) = grass {
                self.game.cheat_set_terrain(to, t);
            }
            for dx in [-1i32, 0, 1] {
                let at = f4x_core::city::Position {
                    x: (to.x as i32 + dx) as u32,
                    ..to
                };
                self.game.cheat_set_river(at, true);
            }
        } else {
            if let Some(t) = self.game.content().terrains.id_of("ocean") {
                self.game.cheat_set_terrain(to, t);
            }
            // The human sails a galley and a warship and brings a Floating Island.
            let rules = &self.game.content().rules;
            let ships: Vec<_> = ["galley", "warship", "floating_island"]
                .iter()
                .filter_map(|id| rules.units.id_of(id))
                .collect();
            for id in ships {
                self.game.spawn_unit(HUMAN, to, id);
            }
            self.test_sea = true;
        }
    }

    /// After the test battle has begun: the walls of fire and darkness round the city, and
    /// mud over the middle of the field.
    pub fn set_test_effects(&mut self, kind: &str) {
        let Some(active) = self.game.battle.as_mut() else {
            return;
        };
        let battle = &mut active.battle;
        match kind {
            "walls" => {
                battle.wall_of_fire = true;
                battle.wall_of_darkness = true;
                let map =
                    std::mem::replace(&mut battle.map, f4x_core::battle::map::BattleMap::open());
                battle.map = map.with_city(true);
            }
            "info" => {
                battle.node_site = Some(f4x_core::battle::immunity::Realm::Chaos);
                battle.node_aura = Some(f4x_core::battle::immunity::Realm::Chaos);
                self.battle_ui.info_open = true;
            }
            "mud" => {
                for y in 8..13 {
                    for x in 8..13 {
                        battle.map.make_mud((x, y));
                    }
                }
            }
            _ => {}
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
        let mut ids = ids;
        // A fight at sea needs ships on both sides.
        if self.test_sea {
            ids.extend(
                ["warship", "trireme"]
                    .iter()
                    .filter_map(|id| rules.units.id_of(id)),
            );
        }
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
        let auto = self.battle_ui.auto;
        let my_turn = mine == Some(battle.active) && !animating && !auto;
        // The other side plays by itself, once nothing is moving; so does yours under Auto.
        if !animating
            && (auto || mine != Some(battle.active))
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
        let hovered = cell_at((mouse.x - ox) / scale, (mouse.y - oy) / scale)
            .filter(|&p| battle.map.is_playable(p));

        if my_turn && !self.battle_ui.info_open {
            self.battle_input(&battle, hovered);
        }
        let battle = self
            .battle_ui
            .shown
            .clone()
            .or_else(|| self.game.battle().cloned())
            .unwrap_or(battle);
        self.draw_battle(&battle, my_turn, hovered);
        if self.battle_ui.info_open {
            if let Some(info) = self.game.battle_info(HUMAN) {
                if self.draw_battle_info(&info) {
                    self.battle_ui.info_open = false;
                }
            } else {
                self.battle_ui.info_open = false;
            }
        }
    }

    fn battle_input(&mut self, battle: &Battle, hovered: Option<Pos>) {
        if self.battle_ui.aiming.is_some()
            || self.battle_ui.spell_list
            || self.battle_ui.fallen_list.is_some()
        {
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
        if is_mouse_button_pressed(MouseButton::Right) {
            // A right-click on a unit opens its statistics sheet.
            if let Some(u) = hovered.and_then(|cell| Self::seen_unit_at(battle, cell)) {
                let id = u.id;
                self.open_battle_sheet(battle, id);
                return;
            }
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
        // A vortex of mine is picked, then steered one square.
        if let Some(i) = battle.vortex_at(cell) {
            if battle.vortices[i].owner == battle.active && !battle.vortices[i].steered {
                self.battle_ui.steering = Some(i);
                return;
            }
        }
        if let Some(i) = self.battle_ui.steering.take() {
            let v = battle.vortices.get(i);
            if v.is_some_and(|v| distance(v.cell, cell) == 1) {
                self.battle_order(BattleCommand::MoveVortex { index: i, to: cell });
                return;
            }
        }
        let target = Self::seen_unit_at(battle, cell).map(|u| (u.id, u.side));
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
        let unit = self.battle_ui.spell_unit.take();
        let aimless = matches!(spell.target, SpellTarget::None | SpellTarget::Battlefield);
        if aimless && !spell.takes_extra_mana {
            self.battle_order(cast_order(unit, spell.spell, SpellAim::None, 0));
            self.battle_ui.selected = None;
            return;
        }
        // Animate Dead and Raise Dead choose among the fallen, who are not on the field to click.
        if [
            f4x_core::battle::spells::ANIMATE_DEAD,
            f4x_core::battle::spells::RAISE_DEAD,
        ]
        .into_iter()
        .any(|h| self.game.spell_has_hook(spell.spell, h))
        {
            // Only the wizard raises the fallen.
            if unit.is_none() {
                self.battle_ui.fallen_list = Some(spell.spell);
            }
            return;
        }
        let most = match unit {
            Some(u) => self
                .game
                .battle()
                .and_then(|b| b.unit(u))
                .map_or(cost, |u| u.mana.max(cost)),
            None => self
                .game
                .battle_spell_most_mana(HUMAN, spell.spell)
                .max(cost),
        };
        let tiles = if spell.target == SpellTarget::Tile {
            let aims: Vec<SpellAim> = match unit {
                Some(u) => self
                    .game
                    .legal_unit_casts()
                    .into_iter()
                    .filter(|(id, c)| *id == u && c.spell == spell.spell)
                    .flat_map(|(_, c)| c.aims)
                    .collect(),
                None => self
                    .game
                    .legal_battle_casts()
                    .into_iter()
                    .filter(|c| c.spell == spell.spell)
                    .flat_map(|c| c.aims)
                    .collect(),
            };
            aims.into_iter()
                .filter_map(|a| match a {
                    SpellAim::Tile(cell) => Some(cell),
                    _ => None,
                })
                .collect()
        } else {
            Vec::new()
        };
        self.battle_ui.aiming = Some(Aiming {
            tiles,
            unit,
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
            self.battle_ui.spell_unit = None;
            self.battle_ui.fallen_list = None;
            return;
        }
        let Some(mut aim) = self.battle_ui.aiming.clone() else {
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
            self.battle_ui.aiming = Some(aim.clone());
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
                    _ => Self::seen_unit_at(battle, cell).map(|u| SpellAim::Unit(u.id)),
                })
        } else {
            None
        };
        if let Some(target) = target {
            self.battle_ui.aiming = None;
            self.battle_order(cast_order(
                aim.unit,
                aim.spell,
                target,
                if aim.takes_extra_mana {
                    aim.mana as u32
                } else {
                    0
                },
            ));
            self.battle_ui.selected = None;
        }
    }

    /// Whether the spell may be aimed at the unit or square (for the markers).
    fn spell_aim_ok(&self, battle: &Battle, aim: &Aiming, cell: Pos) -> bool {
        let Some(side) = Self::human_side(battle) else {
            return false;
        };
        match aim.target {
            SpellTarget::Tile => aim.tiles.contains(&cell),
            target => Self::seen_unit_at(battle, cell).is_some_and(|u| match target {
                SpellTarget::OwnUnit => u.side == side,
                SpellTarget::EnemyUnit => u.side != side,
                _ => true,
            }),
        }
    }

    /// The list of spells the wizard can cast, drawn over the field.
    fn draw_spell_list(&mut self, battle: &Battle) {
        let unit = self.battle_ui.spell_unit;
        let spells = match unit {
            Some(u) => self
                .game
                .battle_unit_spells(u)
                .into_iter()
                .map(|s| f4x_core::battle::spells::BattleSpell {
                    spell: s.spell,
                    cost: s.cost,
                    takes_extra_mana: s.takes_extra_mana,
                    target: s.target,
                    castable: s.castable,
                })
                .collect(),
            None => self.game.battle_spells(HUMAN),
        };
        let (row_h, width) = (26.0, 360.0);
        // A unit with Creature Summoning also calls demons.
        let demons = unit
            .and_then(|u| battle.unit(u))
            .map_or(0, |u| u.casting.demons);
        let rows = spells.len() + usize::from(demons > 0);
        let height = (rows.max(1) as f32 * row_h + 34.0).min(screen_height() - 80.0);
        let (x, y) = (20.0, 20.0);
        draw_rectangle(x, y, width, height, PANEL);
        draw_rectangle_lines(x, y, width, height, 2.0, GOLD);
        let caster = Self::my_caster(battle);
        let head = match unit.and_then(|u| battle.unit(u)) {
            Some(u) => format!("{}   Mana {}   Esc closes", u.name, u.mana),
            None => caster.map_or("No wizard".to_string(), |c| {
                format!("Skill {}   Mana {}   Esc closes", c.skill_left, c.mana)
            }),
        };
        draw_text(&head, x + 10.0, y + 22.0, 18.0, WHITE);
        if spells.is_empty() && demons == 0 {
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
        if let (Some(u), true) = (unit, demons > 0) {
            let rect = Rect::new(
                x + 8.0,
                y + 30.0 + spells.len() as f32 * row_h,
                width - 16.0,
                row_h - 2.0,
            );
            let label = format!("Summon Demon   ({demons} left)");
            if button(rect, &label, self.game.can_summon_demon(u)) {
                self.battle_ui.spell_list = false;
                self.battle_ui.spell_unit = None;
                self.battle_order(BattleCommand::SummonDemon { unit: u });
                self.battle_ui.selected = None;
            }
        }
    }

    /// The fallen of the wizard's army that the spell could bring back, to pick one from.
    fn draw_fallen_list(&mut self, battle: &Battle, spell: SpellId) {
        let fallen: Vec<usize> = self
            .game
            .legal_battle_casts()
            .into_iter()
            .find(|c| c.spell == spell)
            .map(|c| {
                c.aims
                    .into_iter()
                    .filter_map(|a| match a {
                        SpellAim::Unit(id) => Some(id),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let name = self.game.content().rules.spells.get(spell).name.clone();
        let (row_h, width) = (26.0, 360.0);
        let height = (fallen.len().max(1) as f32 * row_h + 34.0).min(screen_height() - 80.0);
        let (x, y) = (20.0, 20.0);
        draw_rectangle(x, y, width, height, PANEL);
        draw_rectangle_lines(x, y, width, height, 2.0, GOLD);
        draw_text(
            format!("{name}: raise which unit?   Esc closes"),
            x + 10.0,
            y + 22.0,
            18.0,
            WHITE,
        );
        if fallen.is_empty() {
            draw_text(
                "No fallen unit to raise",
                x + 10.0,
                y + 50.0,
                18.0,
                LIGHTGRAY,
            );
        }
        let room = ((height - 34.0) / row_h) as usize;
        for (i, &id) in fallen.iter().take(room).enumerate() {
            let Some(u) = battle.unit(id) else {
                continue;
            };
            let how = match u.fate() {
                Fate::Drained => "drained",
                _ => "slain",
            };
            let label = format!("{}   {} figures   {how}", u.name, u.figures_start);
            let rect = Rect::new(
                x + 8.0,
                y + 30.0 + i as f32 * row_h,
                width - 16.0,
                row_h - 2.0,
            );
            if button(rect, &label, true) {
                self.battle_ui.fallen_list = None;
                self.battle_order(BattleCommand::Cast {
                    spell,
                    aim: SpellAim::Unit(id),
                    mana: 0,
                });
                self.battle_ui.selected = None;
            }
        }
    }

    /// The prompt for the spell waiting for its target, and the squares it can be aimed at.
    fn draw_aiming(&mut self, battle: &Battle, hovered: Option<Pos>, origin: (f32, f32, f32)) {
        let Some(aim) = self.battle_ui.aiming.clone() else {
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
        let plane = battle.site.map_or(Plane::Arcanus, |site| site.plane);

        let selected = self.battle_ui.selected.and_then(|i| battle.unit(i));
        let reach = match (selected, my_turn) {
            (Some(u), true) if unfinished(u) => battle.reachable(u.id),
            _ => Default::default(),
        };

        // Ground and roads, then reach markers.
        self.draw_ground(battle, plane, (ox, oy, scale));
        for gy in 0..GRID_H {
            for gx in 0..GRID_W {
                let pos = (gx, gy);
                if reach.contains_key(&pos) {
                    let (sx, sy) = at(pos);
                    diamond(sx, sy, scale, Color::new(0.4, 0.7, 1.0, 0.28), true);
                }
            }
        }
        // Grid lines help to judge distance.
        for gy in 0..GRID_H {
            for gx in 0..GRID_W {
                if battle.map.is_playable((gx, gy)) {
                    let (sx, sy) = at((gx, gy));
                    diamond(sx, sy, scale, Color::new(0.0, 0.0, 0.0, 0.18), false);
                }
            }
        }
        if let Some(pos) = hovered {
            let (sx, sy) = at(pos);
            diamond(sx, sy, scale, Color::new(1.0, 1.0, 1.0, 0.8), false);
        }

        // Scenery, walls and units, far rows first so near ones cover them.
        let mut items = self.scenery_pieces(battle);
        let mut looks: HashMap<usize, Visual> = HashMap::new();
        let viewer = Self::human_side(battle);
        for u in battle
            .units
            .iter()
            .filter(|u| u.is_active() && viewer.is_none_or(|v| battle.seen_by(v, u.id)))
        {
            let vis = self.visual_of(battle, u.id);
            let depth = ((GRID_W - 1) as f32 - vis.at.0 + vis.at.1) * 100.0;
            items.push((depth.round() as i32, 2, u.cell, Piece::Unit(u.id)));
            looks.insert(u.id, vis);
        }
        items.sort_by_key(|&(depth, rank, pos, _)| (depth, rank, pos));
        for (_, _, pos, piece) in items {
            match piece {
                Piece::Unit(id) => {
                    let idle = self.battle_ui.selected == Some(id) && my_turn;
                    self.draw_unit(battle, id, looks[&id], idle, (ox, oy, scale), my_turn);
                }
                Piece::Wall(_) if self.wall_picture_missing(battle, plane, pos) => {
                    draw_wall(battle.map.wall(pos), at(pos), scale);
                }
                piece => self.draw_piece(battle, plane, piece, (ox, oy, scale)),
            }
        }
        self.draw_missile(battle, (ox, oy, scale));
        // Magic vortices: a swirl of rings, the steered one marked.
        for (i, v) in battle.vortices.iter().enumerate() {
            let (sx, sy) = at(v.cell);
            let (cx, cy) = (sx + CELL_W * scale / 2.0, sy + CELL_H * scale / 2.0);
            let spin = get_time() as f32 * 3.0;
            for k in 0..4 {
                let r = (CELL_H * 0.9 - 3.0 * k as f32) * scale / 1.5;
                let a = spin + k as f32 * 1.1;
                draw_circle_lines(
                    cx + a.cos() * 3.0 * scale,
                    cy + a.sin() * 2.0 * scale,
                    r,
                    2.0,
                    Color::new(0.6 - 0.1 * k as f32, 0.3, 0.9, 0.9),
                );
            }
            if self.battle_ui.steering == Some(i) {
                diamond(sx, sy, scale, GOLD, false);
            }
        }

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
                for e in battle
                    .active_units(u.side.other())
                    .filter(|e| battle.seen_by(u.side, e.id))
                {
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
        if let Some(spell) = self.battle_ui.fallen_list.filter(|_| my_turn) {
            self.draw_fallen_list(battle, spell);
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
        let mut tint = Color::new(
            shade,
            shade * (1.0 - 0.6 * vis.hurt),
            shade * (1.0 - 0.6 * vis.hurt),
            fade,
        );
        // An invisible unit that can be seen (the player's own, one next to an enemy, one
        // that has shown itself) is drawn as a pale, half-see-through ghost, as the original
        // draws it.
        if u.is_invisible() {
            tint = Color::new(0.7 * shade, 0.88 * shade, shade, 0.45 * fade);
        }
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
                    let native = tex.size();
                    let size = native * scale;
                    // Standing on the bottom corner of the square, the figure rising from
                    // there; the formation's offsets only move the figures about it (a lone
                    // figure's offset is (1, 8)).
                    draw_texture_ex(
                        tex,
                        sx + (CELL_W / 2.0 - native.x / 2.0 + (fx - 1) as f32) * scale,
                        sy + (CELL_H - native.y + (fy - 8) as f32) * scale,
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
            let base = sy + CELL_H * scale;
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

        // The magic node under the battle: its counter and its aura.
        for line in [
            battle
                .node_site
                .map(|r| format!("{r:?} node: dispels other realms")),
            battle
                .node_aura
                .map(|r| format!("{r:?} aura: +2 to {r:?} creatures")),
        ]
        .into_iter()
        .flatten()
        {
            draw_text(
                &line,
                x + 12.0,
                y - 4.0,
                16.0,
                Color::new(0.8, 0.7, 1.0, 1.0),
            );
            y += 17.0;
        }

        // The unit under the pointer, else the selected one.
        let shown = hovered
            .and_then(|p| Self::seen_unit_at(battle, p))
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
        // The selected unit casts its own spells when it has any it can cast now (a hero, a
        // creature caster); otherwise the Spell button is the wizard's.
        let unit_caster = sel.filter(|&u| {
            my_turn
                && (self.game.battle_unit_spells(u).iter().any(|s| s.castable)
                    || self.game.can_summon_demon(u))
        });
        let can_cast = my_turn
            && self.battle_ui.aiming.is_none()
            && self.battle_ui.fallen_list.is_none()
            && (unit_caster.is_some()
                || Self::my_caster(battle).is_some_and(|c| !c.cast_this_turn));
        let spell = self.compix_button(1, "Spell", (col(0.0), by), can_cast);
        if spell || (can_cast && is_key_pressed(KeyCode::S)) {
            self.battle_ui.spell_list = !self.battle_ui.spell_list;
            self.battle_ui.spell_unit = unit_caster.filter(|_| self.battle_ui.spell_list);
        }
        let wait = self.compix_button(2, "Wait", (col(1.0), by), my_turn);
        if wait || (my_turn && is_key_pressed(KeyCode::W)) {
            self.select_next(sel);
        }
        if self.compix_button(3, "Done", (col(2.0), by), my_turn && sel.is_some()) {
            order = sel.map(|u| BattleCommand::Done { unit: u });
        }
        // Auto hands your side to the computer; pressed again, it takes it back.
        let auto = self.battle_ui.auto;
        let auto_on = self.compix_button(4, "Auto", (col(0.0), by + 40.0), my_turn || auto);
        if auto_on || ((my_turn || auto) && is_key_pressed(KeyCode::A)) {
            self.battle_ui.auto = !auto;
            self.battle_ui.aiming = None;
            self.battle_ui.spell_list = false;
            self.battle_ui.spell_unit = None;
            self.battle_ui.fallen_list = None;
        }
        // Info lists the effects in force; the original locks it when there are none.
        let has_info = self
            .game
            .battle_info(HUMAN)
            .is_some_and(|info| !info.is_empty());
        let info = self.compix_button(20, "Info", (col(1.0), by + 40.0), has_info);
        if (info || (has_info && is_key_pressed(KeyCode::U))) && !self.battle_ui.info_open {
            self.battle_ui.info_open = true;
            return;
        }
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
    let ox = (width - gw * scale) / 2.0 - BAND_X.0 * scale;
    let oy = (height - gh * scale) / 2.0 + (HEADROOM - BAND_Y.0) * scale;
    (scale, ox, oy)
}

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
        // The core's x runs the other way from the original's.
        assert_eq!(cell_origin((6, 5)), (x0 - 16.0, y0 - 8.0));
        assert_eq!(cell_origin((5, 6)), (x0 - 16.0, y0 + 8.0));
        assert_eq!(cell_origin((4, 5)), (x0 + 16.0, y0 + 8.0));
    }

    #[test]
    fn the_ground_set_follows_terrain_and_plane() {
        use crate::battle_ground::ground_file;
        use f4x_core::battle::map::set_of;
        use f4x_core::content::TerrainClass;
        let terrain_set = |c: Option<TerrainClass>, p: Plane| ground_file(set_of(c), p);
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
