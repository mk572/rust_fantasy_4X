//! The question before an attack (task 133): moving into an enemy stack or city asks whether to
//! fight it, leave it to the computer (Auto-resolve) or call the attack off, as a box over the
//! map. Plain box for now; the original's picture is task 143.

use crate::input::Input;
use f4x_core::game::Command;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::fighter_pick::PickStep;
use crate::modal::{keep, kind, Modal};
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

/// What the mana box decided.
enum ManaStep {
    Asking(ManaAsk),
    /// Back to the question before.
    Back,
    /// The fight has been run.
    Fought,
}

/// The box over the map for an attack: the question about a treaty, then the choice to fight, to
/// leave it to the computer or to call it off, then for an automatic fight the mana limit.
pub(crate) struct AttackAsk {
    pub(crate) attack: PendingAttack,
    mana: Option<ManaAsk>,
    /// The pick of who fights, when more than nine of the human's units could.
    pick: Option<crate::fighter_pick::FighterPick>,
}

impl AttackAsk {
    pub(crate) fn new(attack: PendingAttack) -> Self {
        Self {
            attack,
            mana: None,
            pick: None,
        }
    }
}

impl Modal for AttackAsk {
    fn kind(&self) -> &'static str {
        kind::ATTACK
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.attack_prompt_frame(*self, input))
    }
}

impl App {
    /// Leave the pending attack to the battle AI: ask how much mana to spend if the wizard could
    /// cast anything (the box that does is returned), else fight at once.
    fn start_auto_battle(&mut self, attack: PendingAttack) -> Option<AttackAsk> {
        let reserve = self.game.player(HUMAN).map_or(0, |p| p.treasury().mana);
        match self.game.strategic_mana_default(HUMAN) {
            Some(amount) => Some(AttackAsk {
                attack,
                mana: Some(ManaAsk {
                    amount,
                    reserve: reserve.max(0) as u32,
                    typed: false,
                }),
                pick: None,
            }),
            None => {
                self.run_auto_battle(attack, None);
                None
            }
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
        let ask = if question_only {
            Some(AttackAsk::new(attack))
        } else {
            self.start_auto_battle(attack)
        };
        if let Some(ask) = ask {
            self.modals.open(ask);
        }
    }

    /// For the screenshot check (`--pick-box`): the human's first stack grows to fourteen units
    /// and attacks a monster, so the pick of who fights opens.
    pub fn open_test_pick(&mut self) {
        let Some(pos) = self
            .game
            .units()
            .player_units(HUMAN)
            .first()
            .and_then(|&u| self.game.units().unit_position(u))
        else {
            return;
        };
        let kinds: Vec<_> = {
            let rules = &self.game.content().rules;
            rules
                .units
                .defs()
                .iter()
                .filter(|d| d.kind == f4x_core::rules::UnitKind::Normal && d.race.is_some())
                .filter_map(|d| rules.units.id_of(&d.id))
                .collect()
        };
        for i in 0..14usize.saturating_sub(self.own_units_at(pos).len()) {
            self.game.spawn_unit(HUMAN, pos, kinds[i % kinds.len()]);
        }
        let attackers = self.own_units_at(pos);
        let defender = self
            .game
            .spawn_unit(f4x_core::player::MONSTERS, pos, kinds[0]);
        let pick = self.new_fighter_pick(&attackers, true, false);
        let attack = PendingAttack {
            attackers,
            defenders: vec![defender],
            defending: false,
            treaty_asked: true,
        };
        self.modals.open(AttackAsk {
            attack,
            mana: None,
            pick: Some(pick),
        });
    }

    fn run_auto_battle(&mut self, attack: PendingAttack, mana: Option<u32>) {
        self.auto_battle(Command::AutoResolve {
            attackers: attack.attackers,
            defenders: attack.defenders,
            mana_budget: mana.map(|m| (HUMAN, m)),
        });
    }

    /// The box that asks for the mana limit.
    fn mana_ask_frame(
        &mut self,
        attack: &PendingAttack,
        mut ask: ManaAsk,
        input: &Input,
    ) -> ManaStep {
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
            input,
        );
        let more = button(
            Rect::new(x + bw - 220.0, y + 100.0, 50.0, 38.0),
            &self.text("attack.mana_more"),
            ask.amount < ask.reserve,
            input,
        );
        crate::frame::text_centered(&ask.amount.to_string(), w / 2.0, y + 128.0, 32.0, GOLD);
        let ok = button(
            Rect::new(x + 60.0, y + bh - 56.0, 200.0, 38.0),
            &self.text("attack.mana_ok"),
            true,
            input,
        );
        let back = button(
            Rect::new(x + bw - 260.0, y + bh - 56.0, 200.0, 38.0),
            &self.text("attack.mana_back"),
            true,
            input,
        );
        let step = |ask: &mut ManaAsk, up: bool, by: u32| {
            ask.typed = true;
            ask.amount = if up {
                (ask.amount + by).min(ask.reserve)
            } else {
                ask.amount.saturating_sub(by)
            };
        };
        if less || input.key_pressed(KeyCode::Down) {
            step(&mut ask, false, 1);
        }
        if more || input.key_pressed(KeyCode::Up) {
            step(&mut ask, true, 1);
        }
        if input.key_pressed(KeyCode::PageDown) {
            step(&mut ask, false, 10);
        }
        if input.key_pressed(KeyCode::PageUp) {
            step(&mut ask, true, 10);
        }
        for c in input.typed() {
            if let Some(d) = c.to_digit(10) {
                let base = if ask.typed { ask.amount } else { 0 };
                ask.typed = true;
                ask.amount = (base * 10 + d).min(ask.reserve);
            }
        }
        if input.key_pressed(KeyCode::Backspace) {
            ask.typed = true;
            ask.amount /= 10;
        }
        if back || input.key_pressed(KeyCode::Escape) {
            ManaStep::Back
        } else if ok || input.key_pressed(KeyCode::Enter) {
            self.run_auto_battle(attack.clone(), Some(ask.amount));
            ManaStep::Fought
        } else {
            ManaStep::Asking(ask)
        }
    }

    /// Show the pending attack over the map until it is answered; what is left of the box.
    pub(crate) fn attack_prompt_frame(
        &mut self,
        AttackAsk {
            mut attack,
            mana,
            pick,
        }: AttackAsk,
        input: &Input,
    ) -> Option<AttackAsk> {
        // A unit that is gone (the game was loaded, a console command) cancels the question.
        let alive = |u: &hecs::Entity| self.game.units().unit_owner(*u).is_some();
        if !attack.attackers.iter().all(alive) || !attack.defenders.iter().all(alive) {
            return None;
        }
        if let Some(ask) = mana {
            return match self.mana_ask_frame(&attack, ask, input) {
                ManaStep::Asking(ask) => Some(AttackAsk {
                    attack,
                    mana: Some(ask),
                    pick: None,
                }),
                ManaStep::Back => Some(AttackAsk::new(attack)),
                ManaStep::Fought => None,
            };
        }
        if let Some(pick) = pick {
            let auto = pick.auto;
            return match self.fighter_pick_frame(pick, input) {
                PickStep::Picking(pick) => Some(AttackAsk {
                    attack,
                    mana: None,
                    pick: Some(pick),
                }),
                PickStep::Back => None,
                PickStep::Done(chosen) => {
                    // The units not chosen are left out: attackers stay put with their moves,
                    // defenders sit the fight out.
                    if attack.defending {
                        attack.defenders = chosen;
                    } else {
                        attack.attackers = chosen;
                    }
                    self.fight_or_auto(attack, auto)
                }
            };
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
                None => attack.treaty_asked = true,
                Some(p) => {
                    let lay = crate::frame::Layout::new();
                    let text = self.text_with(
                        "diplomacy.attack_partner",
                        &Args::new().text("wizard", self.diplo_name(p)),
                    );
                    match self.confirmation_box(&lay, &text, input) {
                        Some(true) => attack.treaty_asked = true,
                        Some(false) => return None,
                        None => {}
                    }
                }
            }
            return Some(AttackAsk::new(attack));
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
        let (x, y) = (
            68.0,
            ((crate::resolution::DESIGN_H - (paragraph + 34.0)) / 2.0).floor(),
        );
        // The question box's lower part has wells for two buttons; with three red ones only its
        // bottom edge is used.
        let edge = self
            .art
            .picture("resource", 1, 0)
            .map_or(24.0, crate::resolution::logical_h);
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
        let fight = self.red_button(&lay, &fight_label, x + 13.0, at, input);
        let auto = self.red_button(&lay, &auto_label, x + 67.0, at, input);
        let cancel =
            !attack.defending && self.red_button(&lay, &cancel_label, x + 121.0, at, input);
        let fight = fight || input.key_pressed(KeyCode::Enter) || input.key_pressed(KeyCode::Y);
        let auto = auto || input.key_pressed(KeyCode::A);
        let cancel = cancel
            || (!attack.defending
                && (input.key_pressed(KeyCode::Escape) || input.key_pressed(KeyCode::N)));
        if cancel {
            None
        } else if fight || auto {
            let auto = auto
                || self
                    .prefs
                    .get(crate::preferences::Pref::StrategicCombatOnly);
            // More than nine of the human's units could fight: who does is asked first.
            let mine = if attack.defending {
                &attack.defenders
            } else {
                &attack.attackers
            };
            if mine.len() > f4x_core::unit::MAX_STACK {
                let pick = self.new_fighter_pick(mine, !attack.defending, auto);
                return Some(AttackAsk {
                    attack,
                    mana: None,
                    pick: Some(pick),
                });
            }
            self.fight_or_auto(attack, auto)
        } else {
            Some(AttackAsk::new(attack))
        }
    }

    /// Begin the fight in person, or leave it to the computer.
    fn fight_or_auto(&mut self, attack: PendingAttack, auto: bool) -> Option<AttackAsk> {
        if auto {
            self.start_auto_battle(attack)
        } else {
            let (attackers, defenders) = (attack.attackers, attack.defenders);
            self.begin_battle(Command::BeginBattle {
                attackers,
                defenders,
            });
            None
        }
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
