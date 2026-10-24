//! The question before an attack (task 133): moving into an enemy stack or city asks whether to
//! fight it, leave it to the computer (Auto-resolve) or call the attack off, as a box over the
//! map. Plain box for now; the original's picture is task 143.

use f4x_core::game::Command;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::ui::{button, GOLD};
use crate::view::{App, HUMAN};

/// An attack waiting for the player's answer.
#[derive(Debug, Clone)]
pub struct PendingAttack {
    pub attackers: Vec<hecs::Entity>,
    pub defenders: Vec<hecs::Entity>,
    /// A rival wizard attacks the human: defend in person or leave it to the computer, no
    /// calling it off.
    pub defending: bool,
    /// Whether the player has been asked about the treaty with the defenders' owner, if there is
    /// one (`Combat.c`: "You have a treaty with X. Do you still wish to attack?").
    pub treaty_asked: bool,
}

/// The second step of an automatic fight: how much mana the wizard may spend in it.
#[derive(Debug, Clone)]
pub struct ManaAsk {
    pub amount: u32,
    /// The reserve: the most that can be given.
    pub reserve: u32,
    /// Digits typed replace the suggested amount at the first keystroke.
    typed: bool,
}

impl App {
    /// Leave the pending attack to the battle AI: ask how much mana to spend if the wizard could
    /// cast anything, else fight at once.
    fn start_auto_battle(&mut self, attack: PendingAttack) {
        let reserve = self.game.player(HUMAN).map_or(0, |p| p.treasury().mana);
        match self.game.strategic_mana_default(HUMAN) {
            Some(amount) => {
                self.attack_prompt = Some(attack);
                self.mana_ask = Some(ManaAsk {
                    amount,
                    reserve: reserve.max(0) as u32,
                    typed: false,
                });
            }
            None => self.run_auto_battle(attack, None),
        }
    }

    /// For the screenshot check (`--auto-attack`): the human's first stack leaves an attack on a
    /// monster to the computer, which asks for the mana limit; `--attack-box` stops at the
    /// question before.
    pub fn open_test_auto_attack(&mut self, question_only: bool) {
        let attackers = self.game.units().player_units(HUMAN);
        let Some(pos) = attackers
            .first()
            .and_then(|&u| self.game.units().unit_position(u))
        else {
            return;
        };
        let rules = &self.game.content().rules;
        let Some(kind) = rules
            .units
            .defs()
            .iter()
            .find(|d| d.kind == f4x_core::rules::UnitKind::Normal && d.race.is_some())
            .and_then(|d| rules.units.id_of(&d.id))
        else {
            return;
        };
        let defender = self.game.spawn_unit(f4x_core::player::MONSTERS, pos, kind);
        if let Some(t) = self.game.treasury_mut(HUMAN) {
            t.mana = t.mana.max(120);
        }
        let attack = PendingAttack {
            attackers,
            defenders: vec![defender],
            defending: false,
            treaty_asked: true,
        };
        if question_only {
            self.attack_prompt = Some(attack);
        } else {
            self.start_auto_battle(attack);
        }
    }

    fn run_auto_battle(&mut self, attack: PendingAttack, mana: Option<u32>) {
        self.attack_prompt = None;
        self.mana_ask = None;
        self.auto_battle(Command::AutoResolve {
            attackers: attack.attackers,
            defenders: attack.defenders,
            mana_budget: mana.map(|m| (HUMAN, m)),
        });
    }

    /// The box that asks for the mana limit. Returns whether it took the frame.
    fn mana_ask_frame(&mut self, attack: &PendingAttack, mut ask: ManaAsk) {
        let (w, h) = (screen_width(), screen_height());
        let (bw, bh) = (560.0, 220.0);
        let (x, y) = ((w - bw) / 2.0, (h - bh) / 2.0);
        draw_rectangle(x, y, bw, bh, Color::new(0.08, 0.06, 0.04, 0.96));
        draw_rectangle_lines(x, y, bw, bh, 3.0, GOLD);
        crate::frame::text_centered(
            &self.text("attack.mana_title"),
            w / 2.0,
            y + 36.0,
            26.0,
            GOLD,
        );
        let line = self.text_with(
            "attack.mana_line",
            &Args::new().count("reserve", ask.reserve),
        );
        crate::frame::text_centered(&line, w / 2.0, y + 72.0, 18.0, WHITE);
        let less = button(
            Rect::new(x + 170.0, y + 100.0, 50.0, 38.0),
            &self.text("attack.mana_less"),
            ask.amount > 0,
        );
        let more = button(
            Rect::new(x + bw - 220.0, y + 100.0, 50.0, 38.0),
            &self.text("attack.mana_more"),
            ask.amount < ask.reserve,
        );
        crate::frame::text_centered(&ask.amount.to_string(), w / 2.0, y + 128.0, 32.0, GOLD);
        let ok = button(
            Rect::new(x + 60.0, y + bh - 56.0, 200.0, 38.0),
            &self.text("attack.mana_ok"),
            true,
        );
        let back = button(
            Rect::new(x + bw - 260.0, y + bh - 56.0, 200.0, 38.0),
            &self.text("attack.mana_back"),
            true,
        );
        let step = |ask: &mut ManaAsk, up: bool, by: u32| {
            ask.typed = true;
            ask.amount = if up {
                (ask.amount + by).min(ask.reserve)
            } else {
                ask.amount.saturating_sub(by)
            };
        };
        if less || is_key_pressed(KeyCode::Down) {
            step(&mut ask, false, 1);
        }
        if more || is_key_pressed(KeyCode::Up) {
            step(&mut ask, true, 1);
        }
        if is_key_pressed(KeyCode::PageDown) {
            step(&mut ask, false, 10);
        }
        if is_key_pressed(KeyCode::PageUp) {
            step(&mut ask, true, 10);
        }
        while let Some(c) = get_char_pressed() {
            if let Some(d) = c.to_digit(10) {
                let base = if ask.typed { ask.amount } else { 0 };
                ask.typed = true;
                ask.amount = (base * 10 + d).min(ask.reserve);
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            ask.typed = true;
            ask.amount /= 10;
        }
        if back || is_key_pressed(KeyCode::Escape) {
            self.mana_ask = None;
        } else if ok || is_key_pressed(KeyCode::Enter) {
            self.run_auto_battle(attack.clone(), Some(ask.amount));
        } else {
            self.mana_ask = Some(ask);
        }
    }

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
            self.mana_ask = None;
            return false;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();

        if let Some(ask) = self.mana_ask.clone() {
            clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
            self.draw_map();
            self.draw_bars();
            self.mana_ask_frame(&attack, ask);
            return true;
        }
        if !attack.treaty_asked {
            let partner = attack
                .defenders
                .first()
                .and_then(|&u| self.game.units().unit_owner(u))
                .filter(|&p| {
                    matches!(
                        self.game.treaty(HUMAN, p),
                        f4x_core::diplomacy::Treaty::Pact | f4x_core::diplomacy::Treaty::Alliance
                    )
                });
            match partner {
                None => {
                    self.attack_prompt = Some(PendingAttack {
                        treaty_asked: true,
                        ..attack
                    })
                }
                Some(p) => {
                    let lay = crate::frame::Layout::new();
                    let text = self.text_with(
                        "diplomacy.attack_partner",
                        &Args::new().text("wizard", self.diplo_name(p)),
                    );
                    match self.confirmation_box(&lay, &text) {
                        Some(true) => {
                            self.attack_prompt = Some(PendingAttack {
                                treaty_asked: true,
                                ..attack
                            })
                        }
                        Some(false) => self.attack_prompt = None,
                        None => {}
                    }
                }
            }
            return true;
        }
        // The original's box (`resource` 0 and 1, as `Confirmation_Box`) with the question and
        // three of its red buttons; the original asks nothing before a fight, so this is the
        // closest thing it has.
        let lay = crate::frame::Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let title = self.text(if attack.defending {
            "attack.attacked"
        } else {
            "attack.attack"
        });
        let question = format!("{title} {}", self.attack_line(&attack));
        let line = self.art.text_height(4) + 2.0;
        let lines: Vec<(f32, String)> = self
            .bm_wrap(4, &question, 166.0)
            .into_iter()
            .map(|l| (0.0, l))
            .collect();
        let paragraph = line * lines.len() as f32;
        let (x, y) = (68.0, ((200.0 - (paragraph + 34.0)) / 2.0).floor());
        // The question box's lower part has wells for two buttons; with three red ones only its
        // bottom edge is used.
        let edge = self
            .art
            .picture("resource", 1, 0)
            .map_or(24.0, |t| t.height());
        let below = y + paragraph + 10.0;
        self.draw_pic_rows(&lay, "resource", 0, x, y, y, below + edge - 4.0);
        self.draw_pic_rows(
            &lay,
            "resource",
            1,
            x,
            below,
            below + edge - 4.0,
            below + edge,
        );
        self.draw_box_text(view, &lines, x + 9.0, y + 9.0, line);
        let at = y + paragraph + 17.0;
        let (fight_label, auto_label, cancel_label) = (
            self.text("attack.fight"),
            self.text("attack.auto"),
            self.text("attack.cancel"),
        );
        let fight = self.red_button(&lay, &fight_label, x + 13.0, at);
        let auto = self.red_button(&lay, &auto_label, x + 67.0, at);
        let cancel = !attack.defending && self.red_button(&lay, &cancel_label, x + 121.0, at);
        let fight = fight || is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Y);
        let auto = auto || is_key_pressed(KeyCode::A);
        let cancel = cancel
            || (!attack.defending
                && (is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::N)));
        if cancel {
            self.attack_prompt = None;
        } else if fight || auto {
            if auto
                || self
                    .prefs
                    .get(crate::preferences::Pref::StrategicCombatOnly)
            {
                self.start_auto_battle(attack);
            } else {
                self.attack_prompt = None;
                let (attackers, defenders) = (attack.attackers, attack.defenders);
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
