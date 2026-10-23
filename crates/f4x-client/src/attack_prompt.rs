//! The question before an attack (task 133): moving into an enemy stack or city asks whether to
//! fight it, leave it to the computer (Auto-resolve) or call the attack off, as a box over the
//! map. Plain box for now; the original's picture is task 143.

use f4x_core::game::Command;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::ui::{button, GOLD};
use crate::view::App;

/// An attack waiting for the player's answer.
#[derive(Debug, Clone)]
pub struct PendingAttack {
    pub attackers: Vec<hecs::Entity>,
    pub defenders: Vec<hecs::Entity>,
    /// A rival wizard attacks the human: defend in person or leave it to the computer, no
    /// calling it off.
    pub defending: bool,
}

impl App {
    /// Show the pending attack over the map until it is answered. Returns whether the box took
    /// the frame.
    pub(crate) fn attack_prompt_frame(&mut self) -> bool {
        let Some(attack) = self.attack_prompt.clone() else {
            return false;
        };
        // A unit that is gone (the game was loaded, a console command) cancels the question.
        let alive = |u: &hecs::Entity| self.game.units().unit_owner(*u).is_some();
        if !attack.attackers.iter().all(alive) || !attack.defenders.iter().all(alive) {
            self.attack_prompt = None;
            return false;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();

        let (w, h) = (screen_width(), screen_height());
        let (bw, bh) = (560.0, 190.0);
        let (x, y) = ((w - bw) / 2.0, (h - bh) / 2.0);
        draw_rectangle(x, y, bw, bh, Color::new(0.08, 0.06, 0.04, 0.96));
        draw_rectangle_lines(x, y, bw, bh, 3.0, GOLD);
        let title = self.text(if attack.defending {
            "attack.attacked"
        } else {
            "attack.attack"
        });
        crate::frame::text_centered(&title, w / 2.0, y + 36.0, 26.0, GOLD);
        crate::frame::text_centered(&self.attack_line(&attack), w / 2.0, y + 72.0, 20.0, WHITE);
        let fight = button(
            Rect::new(x + 20.0, y + bh - 56.0, 160.0, 38.0),
            &self.text("attack.fight"),
            true,
        );
        let auto = button(
            Rect::new(x + 200.0, y + bh - 56.0, 160.0, 38.0),
            &self.text("attack.auto"),
            true,
        );
        let cancel = !attack.defending
            && button(
                Rect::new(x + 380.0, y + bh - 56.0, 160.0, 38.0),
                &self.text("attack.cancel"),
                true,
            );
        let fight = fight || is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Y);
        let auto = auto || is_key_pressed(KeyCode::A);
        let cancel = cancel
            || (!attack.defending
                && (is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::N)));
        if cancel {
            self.attack_prompt = None;
        } else if fight || auto {
            self.attack_prompt = None;
            let (attackers, defenders) = (attack.attackers, attack.defenders);
            if auto
                || self
                    .prefs
                    .get(crate::preferences::Pref::StrategicCombatOnly)
            {
                self.auto_battle(Command::AutoResolve {
                    attackers,
                    defenders,
                });
            } else {
                self.begin_battle(Command::BeginBattle {
                    attackers,
                    defenders,
                });
            }
        }
        true
    }

    /// "Your 3 units attack 2 Spearmen and a Hero" in a few words.
    fn attack_line(&self, attack: &PendingAttack) -> String {
        let owner = attack
            .defenders
            .first()
            .and_then(|&u| self.game.units().unit_owner(u));
        let counts = |args: Args| {
            args.count("attackers", attack.attackers.len())
                .count("defenders", attack.defenders.len())
        };
        if attack.defending {
            let by = attack
                .attackers
                .first()
                .and_then(|&u| self.game.units().unit_owner(u))
                .unwrap_or(f4x_core::player::MONSTERS);
            let args = counts(self.wizard_arg(Args::new(), "by", by));
            return self.text_with("attack.line_defending", &args);
        }
        match owner {
            Some(p) if p != f4x_core::player::MONSTERS => {
                let args = counts(self.wizard_arg(Args::new(), "owner", p));
                self.text_with("attack.line_wizard", &args)
            }
            _ => self.text_with("attack.line_monster", &counts(Args::new())),
        }
    }
}
