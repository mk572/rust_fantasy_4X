//! The battle screen: the combat grid in the original's isometric projection, the units on
//! it, and the orders a player can give. The fight itself is the core's (`f4x_core::battle`).
//!
//! The grid uses the original's 32 by 16 pixel cells and its step of (16, 8) per square
//! (`Combat_Grid_Screen_Coordinates`), scaled to fit the window. The core's squares are the
//! original's mirrored (`x = 20 - cgx`), so a square is drawn where the original draws the
//! mirrored one, and the picture comes out as the original's. Like the original this draws only
//! the band of 200 squares it plays on (the field generator shuts the rest).

use std::collections::{HashMap, VecDeque};

use crate::font::Style;
use crate::input::Input;
use crate::page::Page;
use crate::spellbook::BookUi;
use f4x_core::battle::ai::next_command;
use f4x_core::battle::map::{distance, Pos, Wall, GRID_H, GRID_W};
use f4x_core::battle::{
    Battle, BattleCommand, BattleEvent, BattleUnit, Confusion, Fate, RangedKind, Side, SpellAim,
};
use f4x_core::game::{Command, Event};
use f4x_core::map::Plane;
use f4x_core::rules::defs::SpellTarget;
use f4x_core::rules::SpellId;
use f4x_core::text::{Args, Texts};
use macroquad::prelude::*;

use crate::art::FigureEffect;
use crate::battle_anim::{self, Anim, Visual};
use crate::battle_ground::Piece;
use crate::frame::{Align, Layout};
use crate::ui::{self, bar, button, GOLD, PANEL};
use crate::view::{App, Screen, HUMAN};

/// Native size of a cell and the step between neighbouring squares.
const CELL_W: f32 = 32.0;
const CELL_H: f32 = 16.0;
const STEP_X: f32 = 16.0;
const STEP_Y: f32 = 8.0;
/// Seconds between the opponent's orders.
const AI_DELAY: f32 = 0.3;

/// What the screen remembers between frames.
#[derive(Default)]
pub(crate) struct BattleUi {
    /// The combat spell book: its pages are laid out again every frame, the spread and the page
    /// turn stay.
    book: BookUi,
    selected: Option<usize>,
    ai_timer: f32,
    /// Where the box of the selected unit's spells and abilities was drawn.
    box_rect: Option<Rect>,
    /// The square the spell just ordered is aimed at, for its animation.
    cast_target: Option<Pos>,
    /// What Call Chaos did to each unit in the order just given: the unit and the outcome.
    chaos: Vec<(usize, u8)>,
    log: Vec<String>,
    /// The fleeing button was pressed once and waits for a second press.
    confirm_flee: bool,
    /// Unit names by id, kept while the battle exists so the last events can be told after
    /// the game has dropped the battle.
    names: Vec<String>,
    /// The battle as it was before the orders now being played; shown while they animate.
    shown: Option<Battle>,
    /// The battle as it was last drawn: the picture under the result scroll once the game has
    /// dropped the battle.
    last_drawn: Option<Battle>,
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
    /// The magic wall being raised by the spell that is playing: its kind (0 fire, 1 darkness)
    /// and the frame it is at.
    pub(crate) fn rising_wall(&self) -> Option<(u8, usize)> {
        match &self.playing {
            Some((
                Anim::WallRise {
                    wall: Some(dark), ..
                },
                t,
            )) => Some((u8::from(*dark), (*t / battle_anim::SPELL_FRAME) as usize)),
            _ => None,
        }
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

/// Tell what a battle event did, in one line.
pub(crate) fn describe(
    texts: &Texts,
    names: &[String],
    spell: &dyn Fn(SpellId) -> String,
    event: &BattleEvent,
) -> Option<String> {
    let name = |i: usize| names.get(i).map_or("?", String::as_str).to_string();
    let say = |key: &str, args: Args| Some(texts.get(key, &args));
    let unit = |i: usize| Args::new().text("unit", name(i));
    let spell_of = |s: SpellId| Args::new().text("spell", spell(s));
    match event {
        BattleEvent::Moved { .. } | BattleEvent::ChaosCalled { .. } => None,
        BattleEvent::Attacked {
            attacker,
            defender,
            dealt,
            dealt_kills,
            taken,
            ..
        } => {
            let taken = if *taken > 0 {
                texts.get("battle.takes_back", &Args::new().count("n", *taken))
            } else {
                String::new()
            };
            let args = Args::new()
                .text("attacker", name(*attacker))
                .text("defender", name(*defender))
                .count("dealt", *dealt)
                .text("kills", kills(texts, *dealt_kills))
                .text("taken", taken);
            say("battle.hits", args)
        }
        BattleEvent::Shot {
            attacker,
            defender,
            dealt,
            kills: k,
        } => {
            let args = Args::new()
                .text("attacker", name(*attacker))
                .text("defender", name(*defender))
                .count("dealt", *dealt)
                .text("kills", kills(texts, *k));
            say("battle.shoots", args)
        }
        BattleEvent::WallAttacked {
            unit: u, destroyed, ..
        } => say(
            if *destroyed {
                "battle.strikes_wall_falls"
            } else {
                "battle.strikes_wall"
            },
            unit(*u),
        ),
        BattleEvent::UnitDied { unit: u } => say("battle.unit_died", unit(*u)),
        BattleEvent::SpellCast { spell: s, .. } => say("battle.spell_cast", spell_of(*s)),
        BattleEvent::UnitSpellCast {
            unit: u, spell: s, ..
        } => {
            let args = unit(*u).text("spell", spell(*s));
            say("battle.unit_casts", args)
        }
        BattleEvent::WallDestroyed { .. } => say("battle.wall_destroyed", Args::new()),
        BattleEvent::MudMade { .. } => say("battle.mud", Args::new()),
        BattleEvent::FissureOpened { .. } => say("battle.fissure", Args::new()),
        BattleEvent::VortexMade { .. } => say("battle.vortex", Args::new()),
        BattleEvent::VortexMoved { .. } => None,
        BattleEvent::LifeDrained { unit: u, points } => {
            say("battle.life_drained", unit(*u).count("points", *points))
        }
        BattleEvent::PowerDrained { unit: u, amount } => {
            say("battle.power_drained", unit(*u).count("amount", *amount))
        }
        BattleEvent::SpellCountered { spell: s, .. } => say("battle.countered", spell_of(*s)),
        BattleEvent::SpellDamaged {
            unit: u,
            dealt,
            kills: k,
        }
        | BattleEvent::FieldStruck {
            unit: u,
            dealt,
            kills: k,
        } => {
            let args = unit(*u)
                .count("dealt", *dealt)
                .text("kills", kills(texts, *k));
            say("battle.takes_damage", args)
        }
        BattleEvent::SpellImmune { unit: u } => say("battle.unharmed", unit(*u)),
        BattleEvent::SpellHealed { unit: u, healed } => {
            say("battle.healed", unit(*u).count("healed", *healed))
        }
        BattleEvent::SpellEnchanted { unit: u, spell: s } => {
            say("battle.enchanted", unit(*u).text("spell", spell(*s)))
        }
        BattleEvent::SpellDispelled {
            unit: u, removed, ..
        } => say("battle.dispelled", unit(*u).count("removed", *removed)),
        BattleEvent::Summoned { unit: u } => say("battle.summoned", unit(*u)),
        BattleEvent::Raised { unit: u } => say("battle.raised", unit(*u)),
        BattleEvent::FieldDispelled { spell: s, .. } => say("battle.field_dispelled", spell_of(*s)),
        BattleEvent::FieldRaised { spell: s, .. } => say("battle.field_raised", spell_of(*s)),
        BattleEvent::WallRaised { spell: s, .. } => say("battle.wall_raised", spell_of(*s)),
        BattleEvent::SpellResisted { unit: u } => say("battle.resists", unit(*u)),
        BattleEvent::SpellSlew { unit: u, kills: k } => {
            say("battle.spell_slew", unit(*u).count("kills", *k))
        }
        BattleEvent::Terrified { unit: u } => say("battle.terrified", unit(*u)),
        BattleEvent::Confused { unit: u, act } => say(
            match act {
                Confusion::Idle => "battle.confused_idle",
                Confusion::Wandered => "battle.confused_wandered",
                Confusion::Seized => "battle.confused_seized",
            },
            unit(*u),
        ),
        BattleEvent::ManaLeaked { .. } => say("battle.mana_leaked", Args::new()),
        BattleEvent::CounterMagicRaised { strength, .. } => say(
            "battle.counter_magic",
            Args::new().count("strength", *strength),
        ),
        BattleEvent::TurnStarted { turn, .. } => {
            say("battle.turn", Args::new().count("turn", *turn))
        }
        BattleEvent::Fled { .. } => say("battle.fled", Args::new()),
        BattleEvent::HeroRecalled { unit: u } => say("battle.recalled", unit(*u)),
        BattleEvent::Ended { .. } | BattleEvent::Exhausted => None,
    }
}

/// ", 2 figures lost", or nothing when none was.
fn kills(texts: &Texts, count: u32) -> String {
    if count == 0 {
        return String::new();
    }
    texts.get("battle.figures_lost", &Args::new().count("n", count))
}

/// Can the unit still do something this turn?
/// The missile picture set (`cmbmagic` entries in eights) a unit's ranged attack throws
/// (`msl_*`: 0 lightning, 1 arrow, 2 fireball, 3 illusion, 4 rocks, 5 sling, 6 deathbolt,
/// 7 icebolt, 9 priest, 10 drow, 11 shimmer).
pub(crate) fn missile_type(u: &BattleUnit) -> usize {
    let has = |a: &str| u.abilities.iter().any(|x| x == a);
    [
        ("rock_throwing", 4),
        ("sling", 5),
        ("lightning_bolt", 0),
        ("fireball_attack", 2),
        ("illusion_attack", 3),
        ("sorcery_bolt", 3),
        ("death_bolt", 6),
        ("ice_bolt", 7),
        ("shaman_bolt", 9),
        ("drow_bolt", 10),
        ("sprite_bolt", 11),
        ("nature_bolt", 11),
    ]
    .into_iter()
    .find_map(|(ability, kind)| has(ability).then_some(kind))
    .unwrap_or(if u.ranged_kind == RangedKind::Magic {
        2
    } else {
        1
    })
}

/// The realm of the glow round an enchanted unit (`Combat_Unit_Enchantment_Outline_Set`): the
/// first of its lasting spells in the original's order (0 chaos, 1 death, 2 life, 3 nature,
/// 4 sorcery). Invisible units have none.
fn enchantment_realm(u: &BattleUnit, ids: &[String]) -> Option<usize> {
    if u.is_invisible() {
        return None;
    }
    realm_of_spells(ids)
}

/// The realm of the first of these lasting spells in the original's order (0 chaos, 1 death,
/// 2 life, 3 nature, 4 sorcery): the colour of the glow round the unit.
pub(crate) fn realm_of_spells(ids: &[String]) -> Option<usize> {
    const ORDER: [(&str, usize); 32] = [
        ("regeneration", 3),
        ("resist_magic", 4),
        ("righteousness", 2),
        ("planar_travel", 2),
        ("haste", 4),
        ("black_channels", 1),
        ("lionheart", 2),
        ("iron_skin", 3),
        ("magic_immunity", 4),
        ("wind_walking", 4),
        ("elemental_armor", 3),
        ("true_sight", 2),
        ("invulnerability", 2),
        ("path_finding", 3),
        ("flight", 4),
        ("eldritch_weapon", 0),
        ("wraith_form", 1),
        ("giant_strength", 3),
        ("immolation", 0),
        ("guardian_wind", 4),
        ("spell_lock", 4),
        ("heroism", 2),
        ("berserk", 1),
        ("holy_armor", 2),
        ("water_walking", 3),
        ("endurance", 2),
        ("cloak_of_fear", 1),
        ("stone_skin", 3),
        ("flame_blade", 0),
        ("bless", 2),
        ("holy_weapon", 2),
        ("resist_elements", 3),
    ];
    ORDER
        .iter()
        .find(|(id, _)| ids.iter().any(|e| e == id))
        .map(|&(_, realm)| realm)
}

/// The curse picture on a unit (`chriver` 16 on, `Battle_Unit_Curse_Effects`): vertigo,
/// confusion, whirlwind, mind storm, shatter, weakness, mind twist, web.
fn curse_icon(ids: &[String]) -> Option<usize> {
    const CURSES: [&str; 8] = [
        "vertigo",
        "confusion",
        "whirlwind",
        "mind_storm",
        "shatter",
        "weakness",
        "mind_twist",
        "web",
    ];
    CURSES
        .iter()
        .position(|c| ids.iter().any(|id| id == c))
        .map(|i| 16 + i)
}

fn unfinished(u: &BattleUnit) -> bool {
    u.is_active() && !(u.attacked && u.moves_left == 0)
}

fn can_shoot(u: &BattleUnit) -> bool {
    matches!(u.ranged_kind, RangedKind::Missile | RangedKind::Magic) && u.ranged > 0 && u.ammo > 0
}

mod bar;
mod pointer;
mod specials;
mod spell_anim;

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
        let events = self.game.apply(command);
        self.note(events);
        if self.game.in_battle() {
            self.open_battle_screen();
        }
    }

    /// Fight a battle to the end without the battle screen (strategic combat only, or an
    /// attack the player left to the computer).
    pub(crate) fn auto_battle(&mut self, command: Command) {
        let events = self.game.apply(command);
        self.record_combat_results(&events);
        let won = events.iter().find_map(|e| match e {
            Event::BattleEnded { winner, .. } => Some(*winner == HUMAN),
            _ => None,
        });
        if let Some(won) = won {
            let line = self.text(if won {
                "battle.victory"
            } else {
                "battle.defeat"
            });
            self.log.push(line);
        }
        let rest: Vec<Event> = events
            .into_iter()
            .filter(|e| !matches!(e, Event::Battle(_) | Event::BattleEnded { .. }))
            .collect();
        self.note(rest);
    }

    /// The page for the battle in progress (it may have been started by the console).
    fn new_battle_page(&self) -> BattleUi {
        let mut ui = BattleUi::default();
        if let Some(battle) = self.game.battle() {
            ui.names = battle.units.iter().map(|u| u.name.clone()).collect();
            ui.selected = battle
                .active_units(battle.active)
                .find(|u| unfinished(u))
                .map(|u| u.id);
            // The attackers (at the original's high x) look towards low x, the defenders the other way.
            for u in &battle.units {
                let look = if u.side == Side::Attacker { 7 } else { 3 };
                ui.facing.insert(u.id, look);
            }
        }
        ui
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
        let to = f4x_core::map::Position {
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
        let mut to = f4x_core::map::Position {
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
                let at = f4x_core::map::Position {
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
        if kind.starts_with("fx-") || matches!(kind, "spell-book" | "shot") {
            self.with_page(|page, app| page.test_effect(app, kind));
            return;
        }
        use f4x_core::battle::field::Field;
        let spells = &self.game.content().rules.spells;
        let raised: Vec<_> = [
            (Side::Attacker, "prayer"),
            (Side::Attacker, "blur"),
            (Side::Defender, "wrack"),
            (Side::Defender, "terror"),
            (Side::Defender, "entangle"),
        ]
        .into_iter()
        .filter_map(|(side, id)| Some((side, Field::of(id)?, spells.id_of(id)?)))
        .collect();
        let curses: Vec<_> = [
            "confusion",
            "black_sleep",
            "warp_creature",
            "web",
            "vertigo",
        ]
        .into_iter()
        .filter_map(|id| spells.id_of(id))
        .collect();
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
            }
            "enchants" => {
                for (side, field, spell) in raised {
                    battle.raise_field(side, field, spell);
                }
                battle.cast_counter_magic(Side::Attacker, 20);
            }
            "glow" => {
                for (u, id) in battle.units.iter_mut().zip([
                    "bless",
                    "flame_blade",
                    "regeneration",
                    "wraith_form",
                    "flight",
                ]) {
                    u.enchantments.push(id.into());
                }
                for (u, spell) in battle.units.iter_mut().skip(5).zip(curses) {
                    u.combat_spells.push((spell, f4x_core::player::PlayerId(0)));
                }
            }
            "vortex" => {
                if let Some(cell) = (9..13)
                    .flat_map(|y| (8..12).map(move |x| (x, y)))
                    .find(|&cell| !battle.is_occupied(cell))
                {
                    battle.vortices.push(f4x_core::battle::vortex::Vortex {
                        cell,
                        owner: Side::Attacker,
                        steered: false,
                    });
                }
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
        if kind == "info" {
            self.with_page(|page, app| page.test_effect(app, kind));
        }
    }

    /// For screenshots: the animation of a spell (`--battle-field fx-fireball`) cast on the first
    /// defender, a moment in.
    fn test_spell_fx(&mut self, ui: &mut BattleUi, id: &str) {
        let Some(battle) = self.game.battle().cloned() else {
            return;
        };
        let Some(spell) = self.game.content().rules.spells.id_of(id) else {
            return;
        };
        ui.cast_target = battle
            .units
            .iter()
            .find(|u| u.side == Side::Defender)
            .map(|u| u.cell);
        // Call Chaos: a different outcome for each of the defenders.
        ui.chaos = battle
            .units
            .iter()
            .filter(|u| u.side == Side::Defender)
            .enumerate()
            .map(|(i, u)| (u.id, (i + 4) as u8 % 8))
            .collect();
        if let Some(anim) = self.spell_animation(ui, &battle, spell, Ok(Side::Attacker)) {
            ui.shown = Some(battle);
            ui.playing = Some((anim, 0.0));
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
            .map(|id| self.game.spawn_unit(f4x_core::player::MONSTERS, pos, id))
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
    fn battle_order(&mut self, ui: &mut BattleUi, order: BattleCommand) {
        let before = self.game.battle().cloned();
        ui.cast_target = match (&order, &before) {
            (BattleCommand::Cast { aim, .. } | BattleCommand::UnitCast { aim, .. }, Some(b)) => {
                match *aim {
                    SpellAim::Unit(id) => b.unit(id).map(|u| u.cell),
                    SpellAim::Tile(cell) => Some(cell),
                    SpellAim::None => None,
                }
            }
            _ => None,
        };
        let events = self.game.apply(Command::Battle(order));
        self.record_combat_results(&events);
        // A summoned creature has a name too.
        if let Some(b) = self.game.battle() {
            if b.units.len() != ui.names.len() {
                ui.names = b.units.iter().map(|u| u.name.clone()).collect();
            }
        }
        ui.chaos = events
            .iter()
            .filter_map(|e| match e {
                Event::Battle(BattleEvent::ChaosCalled { unit, outcome }) => {
                    Some((*unit, *outcome))
                }
                _ => None,
            })
            .collect();
        let names = ui.names.clone();
        let mut anims = Vec::new();
        for event in &events {
            match event {
                Event::Battle(e) => {
                    let spell_name =
                        |id: SpellId| self.game.content().rules.spells.get(id).name.clone();
                    if let Some(line) = describe(&self.game.content().texts, &names, &spell_name, e)
                    {
                        ui.log.push(line);
                    }
                    if let Some(b) = &before {
                        self.animate(ui, b, e, &mut anims);
                    }
                }
                Event::BattleEnded { winner, .. } => {
                    let line = self.text(if *winner == HUMAN {
                        "battle.victory"
                    } else {
                        "battle.defeat"
                    });
                    ui.log.push(line.clone());
                    self.log.push(line);
                }
                Event::Rejected(why) => {
                    let line = self.text(why.key());
                    ui.log.push(line);
                }
                _ => {}
            }
        }
        if !anims.is_empty() {
            if ui.shown.is_none() {
                ui.shown = before;
            }
            ui.queue.extend(anims);
        }
        // The overland log gets the rest (a cleared site's treasure and so on).
        let rest: Vec<Event> = events
            .into_iter()
            .filter(|e| !matches!(e, Event::Battle(_) | Event::BattleEnded { .. }))
            .collect();
        self.note(rest);
        let n = ui.log.len();
        if n > 30 {
            ui.log.drain(..n - 30);
        }
    }

    /// Turn one battle event into animations and remember which way units now look.
    /// `before` is the battle as it stood when the order was given.
    fn animate(
        &mut self,
        ui: &mut BattleUi,
        before: &Battle,
        event: &BattleEvent,
        out: &mut Vec<Anim>,
    ) {
        let cell_of = |id: usize| before.unit(id).map(|u| u.cell);
        match event {
            BattleEvent::Moved { unit, path } => {
                if let (Some(start), Some(&last)) = (cell_of(*unit), path.last()) {
                    let prev = path
                        .len()
                        .checked_sub(2)
                        .and_then(|i| path.get(i).copied())
                        .unwrap_or(start);
                    ui.facing.insert(*unit, battle_anim::facing(prev, last));
                }
                // A teleporter or a merging unit crosses the field in one jump, with its
                // own sound and picture (`Battle_Unit_Teleport`, `Battle_Unit_Tunnel`).
                let instant = before.unit(*unit).and_then(|u| {
                    if u.has("teleport") {
                        Some(false)
                    } else if u.has("merging") {
                        Some(true)
                    } else {
                        None
                    }
                });
                match (instant, path.last()) {
                    (Some(tunnel), Some(&to)) => {
                        out.push(Anim::Sound {
                            cast: if tunnel { 12 } else { 11 },
                        });
                        out.push(if tunnel {
                            Anim::Tunnel { unit: *unit, to }
                        } else {
                            Anim::Teleport { unit: *unit, to }
                        });
                    }
                    _ => out.push(Anim::Move {
                        unit: *unit,
                        path: path.clone(),
                    }),
                }
            }
            BattleEvent::Attacked {
                attacker, defender, ..
            }
            | BattleEvent::Shot {
                attacker, defender, ..
            } => {
                let (dealt, taken) = match event {
                    BattleEvent::Attacked { dealt, taken, .. } => (*dealt, *taken),
                    BattleEvent::Shot { dealt, .. } => (*dealt, 0),
                    _ => (0, 0),
                };
                if let (Some(a), Some(d)) = (cell_of(*attacker), cell_of(*defender)) {
                    ui.facing.insert(*attacker, battle_anim::facing(a, d));
                    ui.facing.insert(*defender, battle_anim::facing(d, a));
                }
                out.push(if matches!(event, BattleEvent::Shot { .. }) {
                    Anim::Shot {
                        attacker: *attacker,
                        defender: *defender,
                        dealt,
                    }
                } else {
                    Anim::Attack {
                        attacker: *attacker,
                        defender: *defender,
                        dealt,
                        taken,
                    }
                });
            }
            BattleEvent::UnitDied { unit } => out.push(Anim::Death { unit: *unit }),
            // The vortex's round sounds like a small flyer (`Vortex_Combat_Round`).
            BattleEvent::VortexMoved { .. } => out.push(Anim::Sound { cast: 7 }),
            BattleEvent::SpellCast { side, spell, .. } => {
                out.extend(self.spell_cast_sound(*spell));
                out.extend(self.spell_animation(ui, before, *spell, Ok(*side)));
            }
            BattleEvent::UnitSpellCast { unit, spell, .. } => {
                out.extend(self.spell_cast_sound(*spell));
                out.extend(self.spell_animation(ui, before, *spell, Err(*unit)));
            }
            _ => {}
        }
    }

    /// Run the animation clock; when the last one has played, show the real battle again.
    fn advance_animation(&mut self, ui: &mut BattleUi, dt: f32) {
        let mut started = None;
        let mut stepped = None;
        if let Some((anim, t)) = ui.playing.as_mut() {
            let before = *t;
            *t += dt;
            if let Anim::Move { unit, .. } = anim {
                // A sound for each square after the first, which sounded as the walk began.
                if (*t / battle_anim::STEP_TIME) as usize
                    > (before / battle_anim::STEP_TIME) as usize
                {
                    stepped = Some(*unit);
                }
            }
            if *t >= anim.duration() {
                ui.playing = None;
            }
        }
        if ui.playing.is_none() {
            ui.playing = ui.queue.pop_front().map(|a| (a, 0.0));
            started = ui.playing.as_ref().map(|(a, _)| a.clone());
        }
        if ui.playing.is_none() && ui.queue.is_empty() {
            ui.shown = None;
        }
        if let Some(anim) = started {
            self.hear_animation(ui, &anim);
        }
        if let Some(unit) = stepped {
            self.hear_step(ui, unit);
        }
    }

    /// The unit of the battle as it is shown now.
    fn shown_unit<'a>(&'a self, ui: &'a BattleUi, id: usize) -> Option<&'a BattleUnit> {
        ui.shown
            .as_ref()
            .or_else(|| self.game.battle())
            .and_then(|b| b.unit(id))
    }

    /// What the original's table says the unit sounds like.
    fn unit_sound(&self, ui: &mut BattleUi, id: usize) -> Option<f4x_data::sound::UnitSound> {
        let unit = self.shown_unit(ui, id)?;
        self.audio.table.units.get(&unit.unit_id).copied()
    }

    /// The sound that goes with an animation as it starts.
    fn hear_animation(&mut self, ui: &mut BattleUi, anim: &Anim) {
        let key = match anim {
            Anim::Move { unit, .. } => self
                .unit_sound(ui, *unit)
                .and_then(|s| crate::audio::step_sound(s.r#move)),
            Anim::Attack { attacker, .. } => self
                .unit_sound(ui, *attacker)
                .and_then(|s| crate::audio::melee_sound(s.melee)),
            Anim::Shot { attacker, .. } => self
                .shown_unit(ui, *attacker)
                .and_then(|u| crate::audio::shot_sound(missile_type(u))),
            Anim::Sound { cast } => crate::audio::spell_sound(*cast),
            _ => None,
        };
        self.audio.effect(key);
    }

    fn hear_step(&mut self, ui: &mut BattleUi, unit: usize) {
        let key = self
            .unit_sound(ui, unit)
            .and_then(|s| crate::audio::step_sound(s.r#move));
        self.audio.effect(key);
    }

    /// The animation that plays the cast sound of a spell, if the table has one.
    fn spell_cast_sound(&self, spell: f4x_core::rules::SpellId) -> Option<Anim> {
        let id = &self.game.content().rules.spells.get(spell).id;
        self.audio
            .table
            .spells
            .get(id)
            .map(|&cast| Anim::Sound { cast })
    }

    /// How a unit of the battle being drawn looks right now.
    /// The ids of the lasting spells on a unit: those it came with and those cast in the battle.
    fn unit_spell_ids(&self, u: &BattleUnit) -> Vec<String> {
        let spells = &self.game.content().rules.spells;
        u.enchantments
            .iter()
            .cloned()
            .chain(
                u.combat_spells
                    .iter()
                    .map(|(s, _)| spells.get(*s).id.clone()),
            )
            .collect()
    }

    /// How the unit bleeds from the blow or shot being played, if it is a party to it.
    fn blood_of(&self, ui: &mut BattleUi, u: &BattleUnit) -> Option<battle_anim::Blood> {
        let (anim, t) = ui.playing.as_ref()?;
        let (damage, landed) = match *anim {
            Anim::Attack {
                attacker,
                defender,
                dealt,
                taken,
            } => {
                let damage = if u.id == defender {
                    dealt
                } else if u.id == attacker {
                    taken
                } else {
                    0
                };
                (damage, *t - anim.duration() * 0.4)
            }
            Anim::Shot {
                defender, dealt, ..
            } if u.id == defender => (dealt, *t - anim.duration() * battle_anim::SHOT_FLIGHT),
            _ => (0, 0.0),
        };
        battle_anim::blood(u.figures, u.hits, damage, landed)
    }

    fn visual_of(&self, ui: &mut BattleUi, battle: &Battle, id: usize) -> Visual {
        let Some(u) = battle.unit(id) else {
            return Visual::still((0, 0), 2);
        };
        let base = ui.facing.get(&id).copied().unwrap_or(2);
        if let Some((anim, t)) = &ui.playing {
            if let Some(v) = battle_anim::visual_in(anim, *t, id, u.cell, base) {
                return v;
            }
            let other = match anim {
                Anim::Attack {
                    attacker, defender, ..
                }
                | Anim::Shot {
                    attacker, defender, ..
                } => {
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
    fn select_next(&mut self, ui: &mut BattleUi, after: Option<usize>) {
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
        ui.selected = next;
    }

    pub(crate) fn open_battle_screen(&mut self) {
        let page = self.new_battle_page();
        self.open_page(Screen::Battle, page);
    }

    pub(crate) fn battle_screen(&mut self, ui: &mut BattleUi, input: &Input) {
        let dt = get_frame_time();
        self.advance_animation(ui, dt);
        // While orders animate, the battle as it was is drawn; afterwards the real one.
        let animating = ui.animating();
        let over = self.modals.has(crate::modal::kind::COMBAT_RESULT);
        let Some(battle) = ui
            .shown
            .clone()
            .or_else(|| self.game.battle().cloned())
            .or_else(|| over.then(|| ui.last_drawn.clone()).flatten())
        else {
            ui.last_drawn = None;
            self.screen = Screen::Map;
            return;
        };
        if !over {
            ui.last_drawn = Some(battle.clone());
        }
        if ui.names.len() != battle.units.len() {
            ui.names = battle.units.iter().map(|u| u.name.clone()).collect();
        }
        // A box over the fight (a dispel's notices) stops it until it is clicked away.
        if self.under_modal {
            self.draw_battle(ui, &battle, false, None, input);
            return;
        }
        ui.ai_timer -= dt;

        let mine = Self::human_side(&battle);
        let auto = ui.auto;
        let my_turn = mine == Some(battle.active) && !animating && !auto;
        // The other side plays by itself, once nothing is moving; so does yours under Auto.
        if !animating
            && (auto || mine != Some(battle.active))
            && ui.ai_timer <= 0.0
            && self.game.in_battle()
        {
            ui.ai_timer = AI_DELAY;
            let order = self
                .game
                .next_battle_command()
                .unwrap_or_else(|| next_command(&battle));
            self.battle_order(ui, order);
            self.select_next(ui, None);
            return self.draw_battle(ui, &battle, my_turn, None, input);
        }

        // Where the pointer is.
        let (scale, ox, oy) = Self::battle_view();
        let mouse: Vec2 = input.mouse_position().into();
        let hovered = cell_at((mouse.x - ox) / scale, (mouse.y - oy) / scale)
            .filter(|&p| battle.map.is_playable(p));

        if my_turn && !ui.info_open {
            self.battle_input(ui, &battle, hovered, input);
        }
        let battle = ui
            .shown
            .clone()
            .or_else(|| self.game.battle().cloned())
            .unwrap_or(battle);
        self.draw_battle(ui, &battle, my_turn, hovered, input);
        self.draw_battle_pointer(ui, &battle, my_turn && !ui.info_open, hovered, input);
        if ui.info_open {
            if let Some(info) = self.game.battle_info(HUMAN) {
                if self.draw_battle_info(&info, input) {
                    ui.info_open = false;
                }
            } else {
                ui.info_open = false;
            }
        }
    }

    fn battle_input(
        &mut self,
        ui: &mut BattleUi,
        battle: &Battle,
        hovered: Option<Pos>,
        input: &Input,
    ) {
        if ui.aiming.is_some() || ui.spell_list || ui.fallen_list.is_some() {
            return self.spell_input(ui, battle, hovered, input);
        }
        let selected = ui.selected;
        let sel_unit = selected
            .and_then(|i| battle.unit(i))
            .filter(|u| unfinished(u));

        if input.key_pressed(KeyCode::Tab) {
            self.select_next(ui, selected);
        }
        if input.key_pressed(KeyCode::Enter) || input.key_pressed(KeyCode::E) {
            self.battle_order(ui, BattleCommand::EndTurn);
            ui.selected = None;
            return;
        }
        if input.key_pressed(KeyCode::Space) || input.key_pressed(KeyCode::D) {
            if let Some(u) = sel_unit {
                self.battle_order(ui, BattleCommand::Done { unit: u.id });
                self.select_next(ui, Some(u.id));
            }
            return;
        }
        if input.mouse_pressed(MouseButton::Right) {
            // A right-click on a unit opens its statistics sheet.
            if let Some(u) = hovered.and_then(|cell| Self::seen_unit_at(battle, cell)) {
                let id = u.id;
                self.open_battle_sheet(battle, id);
                return;
            }
        }
        if input.mouse_pressed(MouseButton::Left) {
            // Clicks on the panel are the buttons' business.
            if self.on_map(ui, input.mouse_position().into()) {
                if let Some(cell) = hovered {
                    self.battle_click(ui, battle, cell);
                }
            }
        }
    }

    fn battle_click(&mut self, ui: &mut BattleUi, battle: &Battle, cell: Pos) {
        let selected = ui.selected;
        // A vortex of mine is picked, then steered one square.
        if let Some(i) = battle.vortex_at(cell) {
            if battle.vortices[i].owner == battle.active && !battle.vortices[i].steered {
                ui.steering = Some(i);
                return;
            }
        }
        if let Some(i) = ui.steering.take() {
            let v = battle.vortices.get(i);
            if v.is_some_and(|v| distance(v.cell, cell) == 1) {
                self.battle_order(ui, BattleCommand::MoveVortex { index: i, to: cell });
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
                ui.selected = Some(id);
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
                self.battle_order(ui, order);
                self.after_order(ui, u.id);
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
                self.battle_order(ui, order);
                self.after_order(ui, u.id);
            }
            (None, Some(u)) if battle.reachable(u.id).contains_key(&cell) => {
                self.battle_order(
                    ui,
                    BattleCommand::Move {
                        unit: u.id,
                        to: cell,
                    },
                );
                self.after_order(ui, u.id);
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
    fn pick_spell(&mut self, ui: &mut BattleUi, spell: &f4x_core::battle::spell::BattleSpell) {
        ui.spell_list = false;
        let cost = self.game.content().rules.spells.get(spell.spell).cost as i32;
        let unit = ui.spell_unit.take();
        let aimless = matches!(spell.target, SpellTarget::None | SpellTarget::Battlefield);
        if aimless && !spell.takes_extra_mana {
            self.battle_order(ui, cast_order(unit, spell.spell, SpellAim::None, 0));
            ui.selected = None;
            return;
        }
        // Animate Dead and Raise Dead choose among the fallen, who are not on the field to click.
        if [
            f4x_core::battle::spell::ANIMATE_DEAD,
            f4x_core::battle::spell::RAISE_DEAD,
        ]
        .into_iter()
        .any(|h| self.game.spell_has_hook(spell.spell, h))
        {
            // Only the wizard raises the fallen.
            if unit.is_none() {
                ui.fallen_list = Some(spell.spell);
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
        ui.aiming = Some(Aiming {
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
    fn spell_input(
        &mut self,
        ui: &mut BattleUi,
        battle: &Battle,
        hovered: Option<Pos>,
        input: &Input,
    ) {
        if input.key_pressed(KeyCode::Escape) {
            ui.aiming = None;
            ui.spell_list = false;
            ui.spell_unit = None;
            ui.fallen_list = None;
            return;
        }
        let Some(mut aim) = ui.aiming.clone() else {
            return;
        };
        // More or less mana: Up/Down by one, Page Up/Down by five.
        let cost = self.game.content().rules.spells.get(aim.spell).cost as i32;
        let step = |up: KeyCode, down: KeyCode, by: i32| {
            i32::from(input.key_pressed(up)) * by - i32::from(input.key_pressed(down)) * by
        };
        if aim.takes_extra_mana {
            let change = step(KeyCode::Up, KeyCode::Down, 1)
                + step(KeyCode::PageUp, KeyCode::PageDown, 5)
                + step(KeyCode::Equal, KeyCode::Minus, 1);
            aim.mana = (aim.mana + change).clamp(cost, aim.most);
            ui.aiming = Some(aim.clone());
        }
        let aimless = matches!(aim.target, SpellTarget::None | SpellTarget::Battlefield);
        let target = if aimless {
            input.key_pressed(KeyCode::Enter).then_some(SpellAim::None)
        } else if input.mouse_pressed(MouseButton::Left) {
            let on_grid = self.on_map(ui, input.mouse_position().into());
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
            ui.aiming = None;
            self.battle_order(
                ui,
                cast_order(
                    aim.unit,
                    aim.spell,
                    target,
                    if aim.takes_extra_mana {
                        aim.mana as u32
                    } else {
                        0
                    },
                ),
            );
            ui.selected = None;
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

    /// The spells the wizard (or the unit whose list is open) can cast, in the small spell book
    /// over the field (`Combat_Spellbook_Screen`): each spell with what it costs now and a
    /// realm symbol for every time it could be paid for this turn; one that cannot be cast is
    /// greyed. A click on a spell picks it, the ribbon or Escape closes the book.
    fn draw_spell_list(&mut self, ui: &mut BattleUi, battle: &Battle, input: &Input) {
        let mut book = std::mem::take(&mut ui.book);
        self.spell_list_in(ui, &mut book, battle, input);
        ui.book = book;
    }

    /// The combat book of `draw_spell_list`, with the book state taken out of the battle's.
    fn spell_list_in(
        &mut self,
        ui: &mut BattleUi,
        book: &mut BookUi,
        battle: &Battle,
        input: &Input,
    ) {
        let unit = ui.spell_unit;
        let spells: Vec<f4x_core::battle::spell::BattleSpell> = match unit {
            Some(u) => self
                .game
                .battle_unit_spells(u)
                .into_iter()
                .map(|s| f4x_core::battle::spell::BattleSpell {
                    spell: s.spell,
                    cost: s.cost,
                    takes_extra_mana: s.takes_extra_mana,
                    target: s.target,
                    castable: s.castable,
                })
                .collect(),
            None => self.game.battle_spells(HUMAN),
        };
        // How many times the caster could pay: the skill left and the mana, or the unit's mana.
        let caster = Self::my_caster(battle);
        let limit = match unit.and_then(|u| battle.unit(u)) {
            Some(u) => u.mana,
            None => caster.map_or(0, |c| c.skill_left.min(c.mana)),
        };
        let entries: HashMap<SpellId, crate::spellbook::CombatEntry> = spells
            .iter()
            .map(|s| {
                let icons = if !s.castable {
                    0
                } else if s.cost <= 0 {
                    40
                } else {
                    ((limit / s.cost).max(1) as usize).min(40)
                };
                (
                    s.spell,
                    crate::spellbook::CombatEntry {
                        cost: s.cost,
                        extra: s.takes_extra_mana,
                        icons,
                    },
                )
            })
            .collect();
        let ids: Vec<SpellId> = spells.iter().map(|s| s.spell).collect();
        book.pages = self.book_pages(&ids, crate::spellbook::SMALL_PAGE);
        book.combat = Some(entries);
        let (pages, last) = (book.pages.len(), book.pages.len().max(2) - 2);
        book.left = book.left.min(last) & !1;
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        if book.turn.is_some() && !self.book_turn_frame(book, input) {
            return;
        }
        let left = book.left;
        let (_, wheel) = input.mouse_wheel();
        let forward =
            input.key_pressed(KeyCode::Right) || input.key_pressed(KeyCode::F) || wheel < 0.0;
        let back = input.key_pressed(KeyCode::Left) || input.key_pressed(KeyCode::B) || wheel > 0.0;
        let click_in =
            |x: f32, y: f32, w: f32, h: f32| ui::hit(lay.rect((x, y, w, h)), true, input).clicked();
        if (forward || click_in(274.0, 14.0, 14.0, 13.0)) && left + 2 < pages {
            self.turn_book_page(book, true, input);
            return;
        }
        if (back || click_in(29.0, 14.0, 14.0, 13.0)) && left >= 2 {
            self.turn_book_page(book, false, input);
            return;
        }
        if click_in(175.0, 166.0, 19.0, 30.0) {
            ui.spell_list = false;
            ui.spell_unit = None;
            return;
        }
        book.draw_spellbook(self, &lay, view, input);
        if spells.is_empty() {
            self.draw_styled(
                view,
                &Style::nearest(1, Color::new(0.2, 0.12, 0.05, 1.0)),
                &self.text("battle.no_spell"),
                60.0,
                80.0,
            );
        }
        // Clicking a spell.
        for (side, page_i) in [(0usize, left), (1, left + 1)] {
            let Some(page) = book.pages.get(page_i) else {
                continue;
            };
            let x0 = if side == 0 { 32.0 } else { 164.0 };
            for (i, &id) in page.spells.iter().enumerate() {
                if click_in(x0, 29.0 + 22.0 * i as f32, 121.0, 17.0) {
                    if let Some(spell) = spells.iter().find(|s| s.spell == id && s.castable) {
                        self.pick_spell(ui, spell);
                        return;
                    }
                }
            }
        }
        // A unit with Creature Summoning also calls demons.
        let demons = unit
            .and_then(|u| battle.unit(u))
            .map_or(0, |u| u.casting.demons);
        if let (Some(u), true) = (unit, demons > 0) {
            let r = lay.rect((100.0, 170.0, 70.0, 16.0));
            let label = self.text_with("battle.summon_demon", &Args::new().count("n", demons));
            if button(r, &label, self.game.can_summon_demon(u), input) {
                ui.spell_list = false;
                ui.spell_unit = None;
                self.battle_order(ui, BattleCommand::SummonDemon { unit: u });
                ui.selected = None;
            }
        }
    }

    /// The fallen of the wizard's army that the spell could bring back, to pick one from.
    fn draw_fallen_list(
        &mut self,
        ui: &mut BattleUi,
        battle: &Battle,
        spell: SpellId,
        input: &Input,
    ) {
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
            self.text_with("battle.raise_which", &Args::new().text("spell", &name)),
            x + 10.0,
            y + 22.0,
            18.0,
            WHITE,
        );
        if fallen.is_empty() {
            draw_text(
                self.text("battle.no_fallen"),
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
            let how = self.text(match u.fate() {
                Fate::Drained => "battle.fate_drained",
                _ => "battle.fate_slain",
            });
            let args = Args::new()
                .text("unit", &u.name)
                .count("figures", u.figures_start)
                .text("how", how);
            let label = self.text_with("battle.fallen_row", &args);
            let rect = Rect::new(
                x + 8.0,
                y + 30.0 + i as f32 * row_h,
                width - 16.0,
                row_h - 2.0,
            );
            if button(rect, &label, true, input) {
                ui.fallen_list = None;
                self.battle_order(
                    ui,
                    BattleCommand::Cast {
                        spell,
                        aim: SpellAim::Unit(id),
                        mana: 0,
                    },
                );
                ui.selected = None;
            }
        }
    }

    /// The prompt for the spell waiting for its target, and the squares it can be aimed at.
    fn draw_aiming(
        &mut self,
        ui: &mut BattleUi,
        battle: &Battle,
        hovered: Option<Pos>,
        origin: (f32, f32, f32),
    ) {
        let Some(aim) = ui.aiming.clone() else {
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
        let mut text = self.text_with(
            if aimless {
                "battle.aim_enter"
            } else {
                "battle.aim_click"
            },
            &Args::new().text("spell", &name),
        );
        if aim.takes_extra_mana {
            let args = Args::new().count("mana", aim.mana).count("most", aim.most);
            text += &self.text_with("battle.aim_mana", &args);
        }
        text += &self.text("battle.aim_cancel");
        draw_rectangle(8.0, 6.0, text.len() as f32 * 8.5 + 16.0, 26.0, PANEL);
        draw_text(&text, 16.0, 25.0, 20.0, GOLD);
    }

    /// Keep the selection on a unit that can still act.
    fn after_order(&mut self, ui: &mut BattleUi, unit: usize) {
        let still = self
            .game
            .battle()
            .and_then(|b| b.unit(unit))
            .is_some_and(unfinished);
        if !still {
            self.select_next(ui, Some(unit));
        }
    }

    fn draw_battle(
        &mut self,
        ui: &mut BattleUi,
        battle: &Battle,
        my_turn: bool,
        hovered: Option<Pos>,
        input: &Input,
    ) {
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let (scale, ox, oy) = Self::battle_view();
        // The map window is clipped, as the original's is: figures stand taller than a cell.
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        let dpi = screen_dpi_scale();
        let clip = Layout::new().rect((0.0, 0.0, 320.0, 164.0));
        gl.quad_gl.scissor(Some((
            (clip.x * dpi) as i32,
            (clip.y * dpi) as i32,
            (clip.w * dpi) as i32,
            (clip.h * dpi) as i32,
        )));
        let at = |pos: Pos| -> (f32, f32) {
            let (x, y) = cell_origin(pos);
            (ox + x * scale, oy + y * scale)
        };
        let plane = battle.site.map_or(Plane::Arcanus, |site| site.plane);

        let selected = ui.selected.and_then(|i| battle.unit(i));
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
        let mut items = self.scenery_pieces(battle, ui.rising_wall());
        let mut looks: HashMap<usize, Visual> = HashMap::new();
        let viewer = Self::human_side(battle);
        for u in battle
            .units
            .iter()
            .filter(|u| u.is_active() && viewer.is_none_or(|v| battle.seen_by(v, u.id)))
        {
            let vis = self.visual_of(ui, battle, u.id);
            let depth = ((GRID_W - 1) as f32 - vis.at.0 + vis.at.1) * 100.0;
            items.push((depth.round() as i32, 2, u.cell, Piece::Unit(u.id)));
            looks.insert(u.id, vis);
        }
        items.sort_by_key(|&(depth, rank, pos, _)| (depth, rank, pos));
        for (_, _, pos, piece) in items {
            match piece {
                Piece::Unit(id) => {
                    let idle = ui.selected == Some(id) && my_turn;
                    let look = looks[&id];
                    self.draw_unit(ui, battle, id, look, idle, (ox, oy, scale), my_turn);
                    // A teleporter is on two squares while it fades out and in.
                    if let Some((at, alpha)) = look.twin {
                        let twin = Visual {
                            at,
                            alpha,
                            twin: None,
                            ..look
                        };
                        self.draw_unit(ui, battle, id, twin, idle, (ox, oy, scale), my_turn);
                    }
                }
                Piece::Wall(_) if self.wall_picture_missing(battle, plane, pos) => {
                    draw_wall(battle.map.wall(pos), at(pos), scale);
                }
                piece => self.draw_piece(battle, plane, piece, (ox, oy, scale)),
            }
        }
        self.draw_missile(ui, battle, (ox, oy, scale));
        self.draw_spell_fx(ui, (ox, oy, scale));
        // Magic vortices: the original's whirlwind (without the art, a swirl of rings), the
        // steered one marked.
        let vortex_art = self
            .art
            .spell_picture("magic_vortex", &f4x_data::spell_art::Role::Vortex);
        for (i, v) in battle.vortices.iter().enumerate() {
            let (sx, sy) = at(v.cell);
            let (cx, cy) = (sx + CELL_W * scale / 2.0, sy + CELL_H * scale / 2.0);
            let spin = get_time() as f32 * 3.0;
            let figure = vortex_art.as_ref().and_then(|(lbx, entry)| {
                let frames = self.art.frames(lbx, *entry).max(1);
                let frame = (get_time() / 0.1) as usize % frames;
                self.art.picture(lbx, *entry, frame)
            });
            if let Some(tex) = figure {
                // Where a small spell picture stands on its square (`spell_point`).
                draw_texture_ex(
                    tex,
                    sx,
                    sy - 17.0 * scale,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(tex.size() * scale),
                        ..Default::default()
                    },
                );
            }
            for k in 0..if figure.is_some() { 0 } else { 4 } {
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
            if ui.steering == Some(i) {
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
        self.draw_aiming(ui, battle, hovered, (ox, oy, scale));
        gl.flush();
        gl.quad_gl.scissor(None);
        self.draw_battle_panel(ui, battle, my_turn, hovered, input);
        self.draw_spell_overlay(ui);
        if my_turn && ui.spell_list {
            self.draw_spell_list(ui, battle, input);
        }
        if let Some(spell) = ui.fallen_list.filter(|_| my_turn) {
            self.draw_fallen_list(ui, battle, spell, input);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_unit(
        &mut self,
        ui: &mut BattleUi,
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
        let ids = self.unit_spell_ids(u);
        let mut drew = false;
        if let Some(ty) = unit_type {
            // Every figure the unit still has, the back ones first.
            let count = u.figures_start as usize;
            let mut spots: Vec<(usize, (i32, i32))> = (0..u.figures as usize)
                .map(|i| (i, battle_anim::figure_offset(count, i)))
                .collect();
            spots.sort_by_key(|&(_, (_, fy))| fy);
            let blood = self.blood_of(ui, u);
            for (index, (fx, fy)) in spots {
                let bleeding = blood
                    .filter(|b| index < b.lost)
                    .and_then(|b| battle_anim::blood_frame(u.id, index, b).map(|f| (b.gibs, f)));
                // The fourth frame of the blood: the figure is gone.
                if bleeding.is_some_and(|(_, f)| f == 3) {
                    continue;
                }
                let glow = enchantment_realm(u, &ids);
                let step = (get_time() * 8.0) as usize;
                let figure = (ty, vis.facing, frame, banner_index);
                // Black Sleep greys a figure, Warp Creature turns it red and inside out.
                let effect = if u.is_invisible() {
                    Some(FigureEffect::Revealed)
                } else if ids.iter().any(|i| i == "warp_creature") {
                    Some(FigureEffect::Warped)
                } else if ids.iter().any(|i| i == "black_sleep") {
                    Some(FigureEffect::Asleep)
                } else {
                    None
                };
                let tex = match (effect, glow) {
                    (Some(effect), _) => self.art.affected_figure(figure, effect),
                    (None, Some(realm)) => self.art.glowing_figure(figure, realm, step),
                    (None, None) => self.art.figure(ty, vis.facing, frame, banner_index),
                };
                if let Some(tex) = tex {
                    let native = tex.size();
                    // A unit sinking into the ground is drawn lower and cut off at the line
                    // its feet stood on.
                    let sunk = vis.sink.clamp(0.0, native.y);
                    let size = vec2(native.x, native.y - sunk) * scale;
                    // Standing on the bottom corner of the square, the figure rising from
                    // there; the formation's offsets only move the figures about it (a lone
                    // figure's offset is (1, 8)).
                    draw_texture_ex(
                        tex,
                        sx + (CELL_W / 2.0 - native.x / 2.0 + (fx - 1) as f32) * scale,
                        sy + (CELL_H - native.y + sunk + (fy - 8) as f32) * scale,
                        tint,
                        DrawTextureParams {
                            dest_size: Some(size),
                            source: Some(Rect::new(0.0, 0.0, native.x, native.y - sunk)),
                            ..Default::default()
                        },
                    );
                    drew = true;
                    if let Some((gibs, f)) = bleeding {
                        let top = (
                            sx + (CELL_W / 2.0 - native.x / 2.0 + (fx - 1) as f32 + 10.0) * scale,
                            sy + (CELL_H - native.y + (fy - 8) as f32 + 12.0) * scale,
                        );
                        if let Some(blood) = self.art.picture("cmbtcity", 23 + gibs, f) {
                            draw_texture_ex(
                                blood,
                                top.0,
                                top.1,
                                WHITE,
                                DrawTextureParams {
                                    dest_size: Some(blood.size() * scale),
                                    ..Default::default()
                                },
                            );
                        }
                    }
                }
            }
        }
        if let Some(entry) = curse_icon(&ids) {
            // Over the square, as the original's curse entity: 13 across and 15 down from the
            // cell's point.
            let frame = (get_time() * 8.0) as usize % 8;
            if let Some(tex) = self.art.picture("chriver", entry, frame) {
                draw_texture_ex(
                    tex,
                    sx + (14.0 - 13.0) * scale,
                    sy - 15.0 * scale,
                    Color::new(1.0, 1.0, 1.0, vis.alpha),
                    DrawTextureParams {
                        dest_size: Some(tex.size() * scale),
                        ..Default::default()
                    },
                );
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
        self.draw_styled(
            (0.0, 0.0, ts),
            &Style::outlined(1),
            &u.figures.to_string(),
            (bar_rect.x + bar_rect.w + 2.0) / ts,
            (bar_rect.y - 3.0 * scale) / ts,
        );
    }

    /// The arrow or bolt in flight during a shot.
    fn draw_missile(
        &mut self,
        ui: &mut BattleUi,
        battle: &Battle,
        (ox, oy, scale): (f32, f32, f32),
    ) {
        let Some((
            Anim::Shot {
                attacker, defender, ..
            },
            t,
        )) = ui.playing.clone()
        else {
            return;
        };
        let Some(progress) = battle_anim::missile_progress(t) else {
            return;
        };
        let (Some(a), Some(d)) = (battle.unit(attacker), battle.unit(defender)) else {
            return;
        };
        // One missile for each figure of the shooter (`Make_Missiles`), each from its figure to
        // one of the target's, travelling from 10 to 100 percent of the way.
        let kind = missile_type(a);
        let count = a.figures.max(1) as usize;
        let targets = d.figures.max(1) as usize;
        let percent = 10.0 + 90.0 * progress;
        let frame = (get_time() * 12.0) as usize % 3;
        let spot = |cell: Pos, (fx, fy): (i32, i32)| {
            let (x, y) = cell_origin(cell);
            (x + 14.0 + fx as f32, y + fy as f32)
        };
        for i in 0..count {
            let (sfx, sfy) = battle_anim::figure_offset(count, i);
            let (dfx, dfy) = battle_anim::figure_offset(count, i % targets);
            let from = spot(a.cell, (sfx, sfy - 8));
            let to = spot(d.cell, (dfx, dfy - 8));
            // The direction is aimed from the shooter's cell, 8 pixels down.
            let aim = spot(a.cell, (0, 8));
            let angle = (to.1 - aim.1)
                .atan2(to.0 - aim.0)
                .to_degrees()
                .rem_euclid(360.0);
            let direction = match angle {
                a if !(22.0..=337.0).contains(&a) => 2,
                a if a < 69.0 => 3,
                a if a < 112.0 => 4,
                a if a < 158.0 => 5,
                a if a < 202.0 => 6,
                a if a < 248.0 => 7,
                a if a < 292.0 => 0,
                _ => 1,
            };
            let x = from.0 + (to.0 - from.0) * percent / 100.0;
            let mut y = from.1 + (to.1 - from.1) * percent / 100.0;
            // Arrows and thrown rocks fly in an arc.
            if matches!(kind, 1 | 4 | 5) {
                y += ((percent - 50.0) / 4.0).abs();
            }
            match self.art.picture("cmbmagic", kind * 8 + direction, frame) {
                Some(tex) => draw_texture_ex(
                    tex,
                    ox + (x - 14.0) * scale,
                    oy + (y - 22.0) * scale,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(tex.size() * scale),
                        ..Default::default()
                    },
                ),
                None => draw_circle(
                    ox + x * scale,
                    oy + y * scale,
                    2.0 * scale,
                    Color::new(1.0, 0.9, 0.5, 1.0),
                ),
            }
        }
    }

    fn player_label(&self, player: f4x_core::player::PlayerId) -> String {
        if player == f4x_core::player::MONSTERS {
            return self.text("battle.monsters");
        }
        self.game
            .wizard(player)
            .map_or_else(|| "?".to_string(), |w| w.name())
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

pub(crate) fn diamond(sx: f32, sy: f32, scale: f32, color: Color, fill: bool) {
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

/// The battle is its own page, over whatever the screen was.
impl Page for BattleUi {
    fn animating(&self) -> bool {
        self.shown.is_some()
    }

    fn test_effect(&mut self, app: &mut App, kind: &str) {
        if let Some(id) = kind.strip_prefix("fx-") {
            app.test_spell_fx(self, id);
            return;
        }
        match kind {
            "spell-book" => self.spell_list = true,
            "info" => self.info_open = true,
            "shot" => {
                let Some(battle) = app.game.battle() else {
                    return;
                };
                let sides = |side| battle.units.iter().find(|u| u.side == side).map(|u| u.id);
                if let (Some(attacker), Some(defender)) =
                    (sides(Side::Attacker), sides(Side::Defender))
                {
                    self.shown = Some(battle.clone());
                    self.playing = Some((
                        Anim::Shot {
                            attacker,
                            defender,
                            dealt: 6,
                        },
                        0.7,
                    ));
                }
            }
            _ => {}
        }
    }

    fn frame(&mut self, app: &mut App, input: &Input) {
        app.battle_screen(self, input);
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
        use f4x_core::map::terrain::TerrainClass;
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
        let texts = f4x_data::load_texts(&[std::path::Path::new("../../content")], "en").unwrap();
        let line = describe(
            &texts,
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
            &texts,
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
