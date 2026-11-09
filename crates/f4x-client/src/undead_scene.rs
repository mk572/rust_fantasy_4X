//! The scene of undead rising after a battle (`Undead_Animation`, `SPLMASTR.c`): a full-screen
//! graveyard (`cmbtfx` 27) with a mist (29) and two layers of earth (30, 31), the figure of the
//! raised unit climbing out of the ground over 40 stages, and a message under it. It is shown
//! for the human's battles, once for the zombies of Zombie Mastery and once for the units drained
//! to death; Esc or a click leaves it (`Combat.c` near line 14680).

use crate::font::{rgb, Style};
use crate::frame::Layout;
use crate::input::Input;
use crate::modal::{keep, Modal};
use crate::text::Args;
use crate::view::App;
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_def::rules::UnitId;
use macroquad::prelude::*;

/// Where the picture goes (`start_x`, `y_start`).
const FRAME: (f32, f32) = (70.0, 14.0);
/// Two ticks of the original's 55 ms timer a stage.
const STAGE: f64 = 0.11;
/// The figure rises for this many stages; the scene lasts 100.
const RISE: usize = 40;
const STAGES: usize = 100;
/// The width of the message under the picture.
const TEXT_WIDTH: f32 = 165.0;
/// `MONSTER.LBX` entries: the zombies (unit 174 less the first summon, 154), and the blank
/// picture of a creator that has no figure of its own.
const ZOMBIE_FIGURE: usize = 20;
const BLANK_FIGURE: usize = 47;
const FIRST_SUMMON_UNIT: u16 = 154;

/// The raised units to show.
#[derive(Debug, Clone)]
pub(crate) struct UndeadScene {
    /// `MONSTER.LBX` entry of the figure that rises.
    figure: usize,
    label: String,
    started: Option<f64>,
}

impl Modal for UndeadScene {
    fn kind(&self) -> &'static str {
        "undead_scene"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.undead_scene_frame(*self, input))
    }
}

/// The figure of the units a creator made: its own picture if it is one of the summoned
/// creatures, the blank one if not.
fn creator_figure(creator: Option<UnitId>) -> usize {
    creator
        .and_then(|unit| unit.num().checked_sub(FIRST_SUMMON_UNIT))
        .map_or(BLANK_FIGURE, usize::from)
}

impl App {
    /// Show the units raised for `player`: zombies (`zombies`) or undead made by `creator`.
    pub(crate) fn announce_undead(
        &mut self,
        player: PlayerId,
        units: u32,
        zombies: bool,
        creator: Option<UnitId>,
    ) {
        self.modals
            .enqueue(self.undead_scene_for(player, units, zombies, creator, None));
    }

    fn undead_scene_for(
        &self,
        player: PlayerId,
        units: u32,
        zombies: bool,
        creator: Option<UnitId>,
        started: Option<f64>,
    ) -> UndeadScene {
        let args = Args::new().count("n", units);
        let (figure, label) = if zombies {
            (
                ZOMBIE_FIGURE,
                self.text_with("battle.zombies_raised", &args),
            )
        } else {
            let label = if player == crate::view::HUMAN {
                self.text_with("battle.undead_for_you", &args)
            } else if player == MONSTERS {
                self.text_with("battle.undead_for_creator", &args)
            } else {
                let args = self.wizard_arg(args, "wizard", player);
                self.text_with("battle.undead_for_wizard", &args)
            };
            (creator_figure(creator), label)
        };
        UndeadScene {
            figure,
            label,
            started,
        }
    }

    /// For screenshots: the scene of `units` zombies (or the undead of a creator, `figure`
    /// being its `MONSTER.LBX` entry) `seconds` in.
    pub fn open_test_undead(&mut self, zombies: bool, units: u32, seconds: f64) {
        let creator = (!zombies).then(|| UnitId::new(FIRST_SUMMON_UNIT + 9));
        let scene = self.undead_scene_for(
            crate::view::HUMAN,
            units,
            zombies,
            creator,
            Some(get_time() - seconds),
        );
        self.modals.enqueue(scene);
    }

    fn undead_scene_frame(&mut self, mut scene: UndeadScene, input: &Input) -> Option<UndeadScene> {
        let started = *scene.started.get_or_insert_with(get_time);
        let stage = ((get_time() - started) / STAGE) as usize;
        if stage >= STAGES || input.dismisses() {
            return None;
        }
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (x, y) = FRAME;
        let rising = stage < RISE;
        let climbed = stage.min(RISE) as f32;
        let span = RISE as f32;
        self.draw_pic(&lay, "cmbtfx", 27, 0, x, y);
        let figure = scene.figure;
        self.clipped((0.0, 0.0, crate::resolution::DESIGN_W, y + 109.0), |app| {
            if rising {
                app.draw_pic(
                    &lay,
                    "cmbtfx",
                    30,
                    0,
                    x,
                    y + 17.0 - (climbed * 17.0 / span).floor(),
                );
                app.draw_pic(
                    &lay,
                    "cmbtfx",
                    31,
                    0,
                    x,
                    y + 40.0 - (climbed * 40.0 / span).floor(),
                );
                app.draw_pic(&lay, "cmbtfx", 29, 0, x, y);
                app.draw_pic(
                    &lay,
                    "monster",
                    figure,
                    0,
                    x + 78.0,
                    y + 110.0 - (climbed * 80.0 / span).floor(),
                );
            } else {
                app.draw_pic(&lay, "cmbtfx", 30, 0, x, y);
                app.draw_pic(&lay, "cmbtfx", 31, 0, x, y);
                app.draw_pic(&lay, "monster", figure, 0, x + 78.0, y + 30.0);
            }
        });
        let style = Style::set(4, 4)
            .shadow()
            .edge(rgb(0x10, 0x10, 0x10))
            .center();
        for (i, line) in self.bm_wrap(4, &scene.label, TEXT_WIDTH).iter().enumerate() {
            self.draw_styled(
                view,
                &style,
                line,
                x + 7.0 + TEXT_WIDTH / 2.0,
                y + 114.0 + 11.0 * i as f32,
            );
        }
        Some(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::game::Event;

    #[test]
    fn a_creator_without_a_figure_of_its_own_shows_the_blank_picture() {
        assert_eq!(creator_figure(None), BLANK_FIGURE);
        assert_eq!(creator_figure(Some(UnitId::new(10))), BLANK_FIGURE);
        assert_eq!(creator_figure(Some(UnitId::new(174))), ZOMBIE_FIGURE);
    }

    #[test]
    fn the_zombies_figure_is_the_zombies_unit() {
        let app = App::for_test();
        let rules = &app.game.content().rules;
        let zombies = rules
            .units
            .iter()
            .find(|(_, u)| u.id == "zombies")
            .expect("zombies")
            .0;
        assert_eq!(creator_figure(Some(zombies)), ZOMBIE_FIGURE);
    }

    #[test]
    fn the_message_names_who_the_undead_serve() {
        let app = App::for_test();
        let one = |player| app.undead_scene_for(player, 1, false, None, None).label;
        assert_eq!(
            one(crate::view::HUMAN),
            "1 unit rises from the dead to serve you."
        );
        assert_eq!(
            one(MONSTERS),
            "1 unit rises from the dead to serve its creator."
        );
        let many = app.undead_scene_for(MONSTERS, 3, false, None, None).label;
        assert_eq!(many, "3 units rise from the dead to serve their creator.");
        let zombies = app
            .undead_scene_for(crate::view::HUMAN, 2, true, None, None)
            .label;
        assert_eq!(zombies, "2 units have been transformed into zombies.");
    }

    fn raised(player: PlayerId) -> Event {
        Event::UndeadRaised {
            player,
            units: 2,
            zombies: true,
            creator: None,
        }
    }

    #[test]
    fn a_battle_of_the_human_shows_the_undead_before_the_result() {
        let mut app = App::for_test();
        let ended = Event::BattleEnded {
            winner: crate::view::HUMAN,
            loser: PlayerId(1),
        };
        app.record_combat_results(&[ended, raised(crate::view::HUMAN)]);
        assert!(app.modals.has("undead_scene"));
        assert!(app.modals.blocks_turn());
    }

    #[test]
    fn a_battle_of_two_rivals_shows_nothing() {
        let mut app = App::for_test();
        let ended = Event::BattleEnded {
            winner: PlayerId(1),
            loser: PlayerId(2),
        };
        app.record_combat_results(&[ended, raised(PlayerId(1))]);
        assert!(!app.modals.has("undead_scene"));
    }
}
