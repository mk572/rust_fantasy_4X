//! The original's full-screen scenes of conquest (`CONQUEST.c`): the victory and the defeat that
//! come before the score, the raiders destroying a wizard, and a banished wizard beginning the
//! Spell of Return (task 360).
//!
//! Each scene counts stages: one stage is a few ticks of the original's clock (18 a second), the
//! scene ends after its last stage, and a click or Esc ends it at once. The pictures are the
//! original's (`win`, `lose`, `conquest`, `wizlab`, `vortex`, `cmbtfx`); without them a scene shows
//! its caption on black and runs as long.

use crate::input::Input;
use f4x_core::player::PlayerId;
use f4x_core::text::{Args, Gender};
use macroquad::prelude::*;

use crate::audio::Track;
use crate::frame::{Align, Layout};
use crate::view::{App, HUMAN};

/// Ticks of the original's clock in a second.
const TICKS_PER_SECOND: f64 = 18.2;

/// What is shown.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Kind {
    /// The human wizard rules both planes (`Win_Animation`).
    Win { wizard: usize },
    /// The human wizard is gone (`Lose_Animation`).
    Lose { wizard: usize },
    /// The raiders (`winner` is `None`) or a rival destroy a wizard (`Conquest_Animation`).
    Conquest {
        loser: usize,
        winner: Option<usize>,
        caption: String,
        /// Whether the loser is the human wizard (the music of defeat) and a woman (the scream).
        human: bool,
        feminine: bool,
    },
    /// A banished rival begins casting the Spell of Return (`Return_Animation`).
    Return { caption: String },
    /// The human wizard has finished the Spell of Mastery and each rival's portrait passes
    /// by (`Spell_Of_Mastery`).
    Parade { wizard: usize, rivals: Vec<usize> },
}

/// A scene on its way: when it began.
#[derive(Debug, Clone)]
pub(crate) struct Scene {
    pub kind: Kind,
    pub started: Option<f64>,
}

impl Scene {
    pub(crate) fn new(kind: Kind) -> Self {
        Self {
            kind,
            started: None,
        }
    }
}

/// The ticks one stage lasts, and the stage after which the scene ends.
fn timing(kind: &Kind) -> (u32, u32) {
    match kind {
        Kind::Win { .. } => (4, 90),
        Kind::Lose { .. } => (4, 45),
        Kind::Conquest { .. } => (5, 30),
        Kind::Return { .. } => (5, 40),
        Kind::Parade { rivals, .. } => (3, parade_stages(rivals.len()) - 1),
    }
}

/// How many stages the parade of `rivals` portraits lasts: twelve and sixty for each.
pub(crate) fn parade_stages(rivals: usize) -> u32 {
    12 + 60 * rivals as u32
}

/// Which `splmastr.lbx` column picture the parade shows at a stage of `total`, and the stage it
/// started at: the one that starts the spell, the one that holds it and the one that lets it go.
pub(crate) fn parade_column(stage: u32, total: u32) -> (usize, u32) {
    if stage + 6 >= total {
        (31, total - 6)
    } else if stage >= 7 {
        (30, 7)
    } else {
        (29, 0)
    }
}

/// The frame of the caster's figure at a stage of the parade (`Spell_Of_Mastery_Draw`): still
/// for the first seven, then it raises its arms and lowers them again in every minute of
/// sixty stages.
pub(crate) fn parade_figure(stage: u32) -> usize {
    if stage <= 6 {
        return 0;
    }
    let m = (stage - 6) % 60;
    if m < 10 {
        (m / 2) as usize
    } else if m > 50 {
        ((60 - m) / 2) as usize
    } else {
        4
    }
}

/// The hands the victorious wizard holds up (`win.lbx` 17 to 22): the shaman's, the priestess's,
/// Oberic's, the draconian's, a woman's, or a man's. The original picks them by the wizard's
/// number through a table whose five targets come in this order.
pub(crate) fn hands_picture(wizard: usize) -> usize {
    match wizard {
        1 => 20,
        2 => 19,
        5 => 21,
        7 => 22,
        9 | 11 | 13 => 18,
        _ => 17,
    }
}

/// One step of the defeated wizard's fall into the limbo (`TBL_DefeatAnimSteps`): where the
/// picture's top-left corner is, the angle it is turned by (degrees) and its size in percent.
pub(crate) const LIMBO_FALL: [(f32, f32, f32, f32); 27] = [
    (0.0, 90.0, 0.0, 100.0),
    (12.0, 73.0, 10.0, 100.0),
    (25.0, 57.0, 20.0, 100.0),
    (32.0, 45.0, 30.0, 100.0),
    (40.0, 29.0, 40.0, 100.0),
    (42.0, 15.0, 50.0, 100.0),
    (55.0, 3.0, 60.0, 100.0),
    (77.0, -4.0, 70.0, 100.0),
    (119.0, -10.0, 80.0, 95.0),
    (147.0, -8.0, 90.0, 90.0),
    (170.0, -5.0, 100.0, 85.0),
    (190.0, 7.0, 110.0, 80.0),
    (212.0, 19.0, 120.0, 75.0),
    (218.0, 40.0, 130.0, 70.0),
    (227.0, 59.0, 140.0, 65.0),
    (217.0, 79.0, 150.0, 60.0),
    (212.0, 97.0, 160.0, 55.0),
    (196.0, 106.0, 170.0, 50.0),
    (179.0, 113.0, 180.0, 45.0),
    (160.0, 109.0, 190.0, 40.0),
    (145.0, 105.0, 200.0, 35.0),
    (135.0, 100.0, 210.0, 30.0),
    (130.0, 94.0, 220.0, 25.0),
    (135.0, 87.0, 230.0, 20.0),
    (147.0, 87.0, 240.0, 15.0),
    (152.0, 89.0, 250.0, 10.0),
    (159.0, 98.0, 260.0, 5.0),
];

/// The keys of the four lines of the victory speech.
const WIN_LINES: [&str; 4] = ["end.win_1", "end.win_2", "end.win_3", "end.win_4"];

/// The line of the victory speech shown at a stage (`Win_Animation_Draw`): a new one every
/// fifteen stages, the last one staying.
pub(crate) fn win_line(stage: u32) -> usize {
    (stage / 15).min(3) as usize
}

/// Where the three figures of the conquest scene stand while they walk up (stages 1 to 6),
/// as `(x, y)` of the axeman, the conquering wizard and the conehead (`Conquerors_Approach`).
pub(crate) fn approach(stage: u32) -> [(f32, f32); 3] {
    let s = stage.saturating_sub(1) as f32;
    [
        (66.0 - s * 11.0 + 252.0, -(stage as f32 + 5.0) + 88.0),
        (90.0 - s * 15.0 + 208.0, 78.0 - s * 13.0 + 60.0),
        (30.0 - s * 5.0 + 100.0, 60.0 - s * 10.0 + 120.0),
    ]
}

/// The frame of the growing zap that the conquest scene shows at a stage: it grows through four
/// frames while the wizard is seized (stages 7 to 10) and shrinks again afterwards (19 to 22,
/// in the order 0, 3, 2, 1).
pub(crate) fn zap_growth(stage: u32) -> Option<usize> {
    match stage {
        7..=10 => Some((stage - 7) as usize),
        19..=22 => Some([0, 3, 2, 1][(stage - 19) as usize]),
        _ => None,
    }
}

impl App {
    /// Queue the scene of the game's end: the human's victory or defeat, before the score.
    pub(crate) fn queue_game_over_scene(&mut self) {
        let Some(over) = self.game.game_over else {
            return;
        };
        let wizard = self
            .game
            .wizard(HUMAN)
            .map_or(0, |w| usize::from(w.portrait()));
        if over.winner == HUMAN && over.kind == f4x_core::game::endgame::EndKind::SpellOfMastery {
            // Every rival who is not banished passes by.
            let rivals = self
                .game
                .player_ids()
                .into_iter()
                .filter(|&p| p != HUMAN && !self.game.is_neutral_player(p))
                .filter_map(|p| self.game.wizard(p))
                .filter(|w| !w.is_banished())
                .map(|w| usize::from(w.portrait()))
                .collect();
            self.end_scenes
                .push_back(Scene::new(Kind::Parade { wizard, rivals }));
        }
        self.end_scenes
            .push_back(Scene::new(if over.winner == HUMAN {
                Kind::Win { wizard }
            } else {
                Kind::Lose { wizard }
            }));
    }

    /// Queue what an event of the game shows: raiders destroying a wizard (their fortress, or
    /// all they had), and a banished rival beginning the Spell of Return.
    pub(crate) fn queue_conquest_scene(
        &mut self,
        player: PlayerId,
        by: Option<PlayerId>,
        banished: bool,
    ) {
        let Some(loser) = self.game.wizard(player) else {
            return;
        };
        let (portrait, feminine) = (
            usize::from(loser.portrait()),
            loser.gender() == Gender::Feminine,
        );
        // The original shows the zap only when the raiders are on one side of it.
        if by.is_none_or(|b| self.game.is_neutral_player(b)) {
            let key = if banished {
                "end.raiders_fortress"
            } else {
                "end.raiders"
            };
            let caption = self.text_with(key, &self.wizard_arg(Args::new(), "loser", player));
            self.end_scenes.push_back(Scene::new(Kind::Conquest {
                loser: portrait,
                winner: None,
                caption,
                human: player == HUMAN,
                feminine,
            }));
        }
        if banished && player != HUMAN {
            let caption = self.text_with(
                "end.returns",
                &self.wizard_arg(Args::new(), "wizard", player),
            );
            self.end_scenes
                .push_back(Scene::new(Kind::Return { caption }));
        }
    }

    /// For screenshots and checks: queue a scene (`win`, `lose`, `conquest`, `return`).
    pub fn open_test_scene(&mut self, name: &str) {
        let wizard = self
            .game
            .wizard(HUMAN)
            .map_or(0, |w| usize::from(w.portrait()));
        let rival = self
            .game
            .player_ids()
            .into_iter()
            .find(|&p| p != HUMAN && !self.game.is_neutral_player(p))
            .unwrap_or(HUMAN);
        match name {
            "win" => self.end_scenes.push_back(Scene::new(Kind::Win { wizard })),
            "lose" => self.end_scenes.push_back(Scene::new(Kind::Lose { wizard })),
            "parade" => {
                let rivals = vec![usize::from(
                    self.game.wizard(rival).map_or(1, |w| w.portrait()),
                )];
                self.end_scenes
                    .push_back(Scene::new(Kind::Parade { wizard, rivals }));
            }
            "conquest" => self.queue_conquest_scene(rival, None, true),
            "return" => self.queue_conquest_scene(rival, Some(HUMAN), true),
            other => eprintln!("unknown scene {other}"),
        }
    }

    /// The music of the scene on top.
    pub(crate) fn end_scene_track(&self) -> Option<Track> {
        use crate::audio::music;
        let scene = self.end_scenes.front()?;
        Some(Track::piece(match scene.kind {
            Kind::Win { .. } => music::WIN_MILITARY,
            Kind::Lose { .. } => music::LOSE_MILITARY,
            Kind::Conquest { human, .. } => {
                if human {
                    music::LOSE_MILITARY
                } else {
                    music::WIN_MILITARY
                }
            }
            Kind::Return { .. } => return None,
            Kind::Parade { .. } => music::SOM_STARTED,
        }))
    }

    /// Draw the scene on top, if any. Returns whether it took the frame.
    pub(crate) fn end_scene_frame(&mut self, input: &Input) -> bool {
        if self.game.in_battle() {
            return false;
        }
        let Some(scene) = self.end_scenes.front_mut() else {
            return false;
        };
        let now = get_time();
        let started = *scene.started.get_or_insert(now);
        let (per_stage, last) = timing(&scene.kind);
        let ticks = ((now - started) * TICKS_PER_SECOND) as u32;
        let stage = ticks / per_stage;
        let kind = scene.kind.clone();
        let skipped = input.key_pressed(KeyCode::Escape) || input.mouse_pressed(MouseButton::Left);
        // The scene's first frame is drawn before it can be skipped, so a held key from the
        // screen before does not flash it past.
        if stage > last || (skipped && ticks > 2) {
            self.end_scenes.pop_front();
            return true;
        }
        clear_background(BLACK);
        // The Spell of Return's box stands over the main screen.
        if matches!(kind, Kind::Return { .. }) {
            self.draw_map();
            self.draw_bars(input);
        }
        let lay = Layout::new();
        clip_to_screen(&lay, true);
        match kind {
            Kind::Win { wizard } => self.draw_win(&lay, wizard, stage),
            Kind::Lose { wizard } => self.draw_lose(&lay, wizard, stage),
            Kind::Conquest {
                loser,
                winner,
                caption,
                feminine,
                ..
            } => self.draw_conquest(&lay, loser, winner, &caption, feminine, stage),
            Kind::Return { caption } => self.draw_return(&lay, &caption, stage),
            Kind::Parade { wizard, rivals } => self.draw_parade(&lay, wizard, &rivals, stage),
        }
        clip_to_screen(&lay, false);
        true
    }

    /// A picture's frame at `(x, y)`; whether there was one.
    fn scene_pic(&mut self, lay: &Layout, lbx: &str, entry: usize, frame: usize, at: (f32, f32)) {
        self.draw_pic(lay, lbx, entry, frame, at.0, at.1);
    }

    /// The frame an animation of `frames` pictures shows at `stage`.
    fn scene_frame(&mut self, lbx: &str, entry: usize, stage: u32) -> usize {
        let frames = self.art.frames(lbx, entry);
        if frames == 0 {
            0
        } else {
            stage as usize % frames
        }
    }

    fn scene_caption(&mut self, lay: &Layout, text: &str, y: f32) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        self.bm_text_outlined(view, 5, text, 160.0, y, Align::Center);
    }

    fn draw_win(&mut self, lay: &Layout, wizard: usize, stage: u32) {
        self.scene_pic(lay, "win", 0, 0, (0.0, 0.0));
        self.scene_pic(lay, "win", hands_picture(wizard), 0, (22.0, 143.0));
        // The wizard's arms rise: each draw adds the next frame over the earlier ones.
        let entry = 3 + wizard;
        let frame = self.scene_frame("win", entry, stage);
        for f in 0..=frame {
            self.scene_pic(lay, "win", entry, f, (93.0, 2.0));
        }
        let sparkle = self.scene_frame("win", 1, stage);
        self.scene_pic(lay, "win", 1, sparkle, (0.0, 0.0));
        let planets = self.scene_frame("win", 2, stage);
        self.scene_pic(lay, "win", 2, planets, (0.0, 0.0));
        let text = self.text(WIN_LINES[win_line(stage)]);
        self.scene_caption(lay, &text, 180.0);
    }

    fn draw_lose(&mut self, lay: &Layout, wizard: usize, stage: u32) {
        let vortex = self.scene_frame("lose", 0, stage);
        self.scene_pic(lay, "lose", 0, vortex, (0.0, 0.0));
        let Some(&(x, y, angle, size)) = LIMBO_FALL.get(stage as usize) else {
            return;
        };
        let Some(tex) = self.art.picture("lose", 1 + wizard, 0).cloned() else {
            return;
        };
        // Shrunk and turned about its top-left corner.
        let at = lay.point(x, y);
        draw_texture_ex(
            &tex,
            at.x,
            at.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(tex.size() * lay.scale * size / 100.0),
                rotation: angle.to_radians(),
                pivot: Some(at),
                ..Default::default()
            },
        );
    }

    fn draw_conquest(
        &mut self,
        lay: &Layout,
        loser: usize,
        winner: Option<usize>,
        caption: &str,
        feminine: bool,
        stage: u32,
    ) {
        self.scene_pic(lay, "wizlab", 19, 0, (0.0, 0.0));
        let at = (69.0, 75.0);
        if stage < 7 {
            self.scene_pic(lay, "wizlab", loser, 0, at);
        }
        if (1..7).contains(&stage) {
            let [axe, wizard, cone] = approach(stage);
            self.scene_pic(lay, "conquest", 15, 0, axe);
            if let Some(w) = winner {
                self.scene_pic(lay, "conquest", w, 0, wizard);
            }
            self.scene_pic(lay, "conquest", 14, 0, cone);
        }
        match winner {
            None if stage > 6 => {
                // The raiders stand in front of the ruin.
                self.scene_pic(lay, "wizlab", loser, 0, at);
                self.scene_pic(lay, "conquest", 15, 0, (252.0, 88.0));
                self.scene_pic(lay, "conquest", 14, 0, (100.0, 120.0));
            }
            Some(w) if (7..23).contains(&stage) => self.draw_zap(lay, loser, w, feminine, stage),
            _ => {}
        }
        self.scene_caption(lay, caption, 6.0);
    }

    /// Stages 7 to 22 with a conquering wizard: he seizes the loser (the zap grows), strikes
    /// him (the lightning plays), and what is left of him dissolves.
    fn draw_zap(&mut self, lay: &Layout, loser: usize, winner: usize, feminine: bool, stage: u32) {
        let loser_at = (69.0, 75.0);
        if stage == 11 {
            self.audio
                .effect(Some(crate::audio::scream_sound(feminine)));
        }
        if stage < 19 {
            self.scene_pic(lay, "wizlab", loser, 0, loser_at);
        }
        if let Some(f) = zap_growth(stage) {
            self.scene_pic(lay, "conquest", 16, f, (177.0, 62.0));
        }
        if (11..19).contains(&stage) {
            let f = self.scene_frame("conquest", 17, stage - 11);
            self.scene_pic(lay, "conquest", 17, f, (0.0, 0.0));
        }
        if stage >= 19 {
            // The loser's own pieces come back over each other, one more each stage.
            for f in 0..=(stage - 19) as usize {
                self.scene_pic(lay, "conquest", 18 + loser, f, (66.0, 16.0));
            }
        }
        self.scene_pic(lay, "conquest", 33 + winner, 0, (208.0, 60.0));
        self.scene_pic(lay, "conquest", 32, 0, (100.0, 120.0));
    }

    fn draw_parade(&mut self, lay: &Layout, wizard: usize, rivals: &[usize], stage: u32) {
        let total = parade_stages(rivals.len());
        self.scene_pic(lay, "wizlab", 19, 0, (0.0, 0.0));
        let (column, since) = parade_column(stage, total);
        let frame = self.scene_frame("splmastr", column, stage - since);
        self.scene_pic(lay, "splmastr", column, frame, (90.0, 0.0));
        let figure = parade_figure(stage);
        self.scene_pic(lay, "splmastr", 14 + wizard, figure, (69.0, 75.0));
        if stage > 6 && 6 + 60 * (rivals.len() as u32) > stage {
            // One rival's portrait a minute, and a twinkle as it arrives.
            let at = (stage - 6) % 60;
            if let Some(&rival) = rivals.get(((stage - 6) / 60) as usize) {
                let frame = (at as usize).min(self.art.frames("splmastr", rival).max(1) - 1);
                self.scene_pic(lay, "splmastr", rival, frame, (0.0, 0.0));
            }
            if at < 9 {
                let frame = self.scene_frame("splmastr", 28, at);
                self.scene_pic(lay, "splmastr", 28, frame, (95.0, 55.0));
            }
        }
    }

    fn draw_return(&mut self, lay: &Layout, caption: &str, stage: u32) {
        let (x, y) = (51.0, 28.0);
        self.scene_pic(lay, "cmbtfx", 27, 0, (x, y));
        let f = self.scene_frame("vortex", 0, stage);
        self.scene_pic(lay, "vortex", 0, f, (x + 8.0, y + 8.0));
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        for (i, line) in self.bm_wrap(4, caption, 165.0).iter().enumerate() {
            self.bm_text_outlined(
                view,
                4,
                line,
                x + 7.0,
                y + 114.0 + 11.0 * i as f32,
                Align::Left,
            );
        }
    }
}

/// Keep the drawing inside the original's 320 by 200 screen, as its pictures are clipped.
fn clip_to_screen(lay: &Layout, on: bool) {
    let mut gl = unsafe { get_internal_gl() };
    gl.flush();
    if on {
        let r = lay.rect((0.0, 0.0, 320.0, 200.0));
        let dpi = screen_dpi_scale();
        gl.quad_gl.scissor(Some((
            (r.x * dpi) as i32,
            ((screen_height() - r.y - r.h) * dpi) as i32,
            (r.w * dpi) as i32,
            (r.h * dpi) as i32,
        )));
    } else {
        gl.quad_gl.scissor(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_victory_speech_changes_every_fifteen_stages_and_the_last_line_stays() {
        assert_eq!(win_line(0), 0);
        assert_eq!(win_line(14), 0);
        assert_eq!(win_line(15), 1);
        assert_eq!(win_line(45), 3);
        assert_eq!(win_line(90), 3);
    }

    #[test]
    fn the_conquerors_walk_in_from_the_right_and_below() {
        let first = approach(1);
        let last = approach(6);
        assert_eq!(first[0], (318.0, 82.0));
        assert_eq!(first[1], (298.0, 138.0));
        assert_eq!(first[2], (130.0, 180.0));
        // Each figure ends up where the zap scene finds it.
        assert_eq!(last[0], (263.0, 77.0));
        assert!(last[1].0 < first[1].0 && last[2].1 < first[2].1);
    }

    #[test]
    fn the_parade_lasts_a_minute_a_rival_and_the_column_changes_twice() {
        assert_eq!(parade_stages(0), 12);
        assert_eq!(parade_stages(3), 192);
        assert_eq!(parade_column(0, 72), (29, 0));
        assert_eq!(parade_column(7, 72), (30, 7));
        assert_eq!(parade_column(65, 72), (30, 7));
        assert_eq!(parade_column(66, 72), (31, 66));
        // The figure rests, raises its arms over ten stages, holds them and lowers them.
        assert_eq!(parade_figure(6), 0);
        assert_eq!(parade_figure(6 + 8), 4);
        assert_eq!(parade_figure(6 + 30), 4);
        assert_eq!(parade_figure(6 + 56), 2);
    }

    #[test]
    fn the_zap_grows_then_shrinks() {
        let growth: Vec<_> = (7..=10).filter_map(zap_growth).collect();
        assert_eq!(growth, [0, 1, 2, 3]);
        let shrink: Vec<_> = (19..=22).filter_map(zap_growth).collect();
        assert_eq!(shrink, [0, 3, 2, 1]);
        assert_eq!(zap_growth(15), None);
    }

    #[test]
    fn the_limbo_fall_ends_small_and_the_hands_follow_the_wizard() {
        assert_eq!(LIMBO_FALL[0].3, 100.0);
        assert_eq!(LIMBO_FALL[26].3, 5.0);
        assert_eq!(hands_picture(0), 17);
        assert_eq!(hands_picture(1), 20);
        assert_eq!(hands_picture(2), 19);
        assert_eq!(hands_picture(13), 18);
    }
}
