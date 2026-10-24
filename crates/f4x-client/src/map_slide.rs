//! A stack that slides from square to square on the map, and the glow of enchanted units
//! (task 298; `Move_Units_Draw`, `Draw_Unit_Enchantment_Outline` in the original).
//!
//! The core moves units at once; the client keeps what it was told (`Event::UnitsMoved`) as
//! slides and draws each moving unit part of the way along its step until the slide is over.
//! The original takes five or six screen flips a step; ours takes `STEP_SECONDS`.

use f4x_core::game::Event;
use f4x_core::map::Position;
use macroquad::prelude::*;

use crate::preferences::Pref;
use crate::view::{App, HUMAN};

/// How long a unit takes to cross one square.
pub(crate) const STEP_SECONDS: f64 = 0.16;

/// Units that moved one square, from `get_time()` `start` on.
#[derive(Debug, Clone)]
pub(crate) struct Slide {
    pub units: Vec<hecs::Entity>,
    pub from: Position,
    pub to: Position,
    pub start: f64,
}

impl Slide {
    fn end(&self) -> f64 {
        self.start + STEP_SECONDS
    }
}

impl App {
    /// Note a move the player should see. Your own units always slide; another wizard's or a
    /// monster's only with Enemy Moves on, on squares you have explored, and when you can see
    /// one of the units.
    pub(crate) fn start_slide(&mut self, event: &Event) {
        let Event::UnitsMoved { units, from, to } = event else {
            return;
        };
        let step = from.x.abs_diff(to.x).max(from.y.abs_diff(to.y));
        if from.plane != to.plane || step != 1 || units.is_empty() {
            return;
        }
        let mine = units
            .iter()
            .any(|&u| self.game.units().unit_owner(u) == Some(HUMAN));
        if !mine {
            let seen = self.game.is_explored(HUMAN, *from) || self.game.is_explored(HUMAN, *to);
            let visible = units
                .iter()
                .any(|&u| self.game.economy().unit_visible_to(u, HUMAN));
            if !(self.prefs.get(Pref::EnemyMoves) && seen && visible) {
                return;
            }
        }
        // Behind an earlier step of the same units.
        let now = get_time();
        let start = self
            .slides
            .iter()
            .filter(|s| s.units.iter().any(|u| units.contains(u)))
            .map(Slide::end)
            .fold(now, f64::max);
        self.slides.push(Slide {
            units: units.clone(),
            from: *from,
            to: *to,
            start,
        });
    }

    /// Forget the slides that are over.
    pub(crate) fn finish_slides(&mut self) {
        let now = get_time();
        self.slides.retain(|s| s.end() > now);
    }

    /// The slides on show now, one for each group of units: where it is on its way, as a square
    /// with a fraction, and the slide.
    pub(crate) fn sliding(&self, plane: f4x_core::map::Plane) -> Vec<(Vec2, &Slide)> {
        let now = get_time();
        let mut shown: Vec<(Vec2, &Slide)> = Vec::new();
        for (i, s) in self.slides.iter().enumerate() {
            // A unit is drawn by the first of its slides that is not over.
            let earlier = self.slides[..i]
                .iter()
                .any(|e| e.end() > now && e.units.iter().any(|u| s.units.contains(u)));
            if earlier || s.end() <= now || s.from.plane != plane {
                continue;
            }
            let f = ((now - s.start) / STEP_SECONDS).clamp(0.0, 1.0) as f32;
            let at = vec2(
                s.from.x as f32 + (s.to.x as f32 - s.from.x as f32) * f,
                s.from.y as f32 + (s.to.y as f32 - s.from.y as f32) * f,
            );
            shown.push((at, s));
        }
        shown
    }

    /// Whether a slide still holds the unit away from its square.
    pub(crate) fn is_sliding(&self, unit: hecs::Entity) -> bool {
        let now = get_time();
        self.slides
            .iter()
            .any(|s| s.end() > now && s.units.contains(&unit))
    }

    /// The realm of the glow round a unit on the map (0 chaos to 4 sorcery), when an
    /// enchantment of its own is on it.
    pub(crate) fn unit_glow_realm(&self, unit: hecs::Entity) -> Option<usize> {
        let spells = &self.game.content().rules.spells;
        let ids: Vec<String> = self
            .game
            .units()
            .unit_enchantments(unit)
            .into_iter()
            .map(|(s, _)| spells.get(s).id.clone())
            .collect();
        crate::battle_screen::realm_of_spells(&ids)
    }
}

/// The step of the glow's colours (the original moves it every other screen flip).
pub(crate) fn glow_step() -> usize {
    (get_time() * 8.0) as usize
}
