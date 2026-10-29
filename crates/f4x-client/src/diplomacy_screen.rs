//! The diplomacy screen, laid out like the original's (`DIPLOMAC.c`) on its 320 by 200 pixels:
//! the wizard in the mirror between two gargoyles whose eyes show how the wizard feels about
//! you (green is warm, red is angry), what the wizard says below, and the menu of what you may
//! do. The wizard moves its lips while it speaks and then shows the face of its mood.
//!
//! The menu is the original's: propose a treaty (pact, alliance, peace, war on another wizard,
//! breaking an alliance with one), threaten or break a treaty, offer tribute, exchange spells.
//! A proposal is put to the core and the wizard answers at once (`f4x_ai::answer_proposals`),
//! as it does on this screen in the original.

use crate::input::Input;
use crate::ui;
use f4x_core::diplomacy::{relation_level, ProposalKind, Stage, Treaty};
use f4x_core::game::{Command, Event};
use f4x_core::player::PlayerId;
use f4x_core::rules::SpellId;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::diplomacy_text as say;
use crate::frame::{Align, Layout};
use crate::view::{App, Screen, HUMAN};

/// Where the pictures go (`Diplomacy_Screen_Draw`, the portrait animation and the eyes).
const PORTRAIT_AT: (f32, f32) = (107.0, 13.0);
const MIRROR_AT: (f32, f32) = (95.0, 1.0);
const LEFT_EYES_AT: (f32, f32) = (64.0, 58.0);
const RIGHT_EYES_AT: (f32, f32) = (233.0, 58.0);
/// The text and the menu: left edge, width, and the top of the first line of text.
const TEXT_X: f32 = 38.0;
const TEXT_W: f32 = 245.0;
const TEXT_Y: f32 = 138.0;
const MENU_Y: f32 = 142.0;
/// Seconds a frame of the speaking animation lasts, and how many frames it has.
const TALK_FRAME: f64 = 0.055;
const TALK_FRAMES: usize = 25;
/// Font styles and colour sets: the message is style 4, the menu style 1.
const MESSAGE_STYLE: usize = 4;
const MENU_STYLE: usize = 1;
const MESSAGE_SET: usize = 4;
const MENU_SET: usize = 4;
/// The tint of a menu row that cannot be chosen: the enabled colours, greyed and dimmed.
const MENU_OFF_TINT: Color = Color::new(0.55, 0.55, 0.58, 0.9);

/// How the wizard's face looks when it is not speaking (`moodwiz.lbx` frames).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mood {
    Good,
    Bad,
    Neutral,
}

impl Mood {
    fn frame(self) -> usize {
        match self {
            Mood::Good => 0,
            Mood::Bad => 1,
            Mood::Neutral => 2,
        }
    }
}

/// Which list the menu shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Menu {
    /// The wizard has spoken; a click goes on to the main menu.
    Greeting,
    Main,
    Propose,
    /// Choose a third wizard: for a war (true) or for breaking an alliance (false).
    Third(bool),
    Threaten,
    Tribute,
    TributeSpell,
    /// The realm of the spell wanted.
    ExchangeRealm,
    /// The spell wanted, of this realm (an index into the realms).
    ExchangeGet(usize),
    /// What is offered for the spell wanted.
    ExchangeGive(SpellId),
    /// The wizard has answered; a click goes back to the main menu (or leaves).
    Reply,
}

/// What a row of the menu does.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Act {
    Go(Menu),
    Propose(ProposalKind),
    Break,
    Gold(i32),
    GiveSpell(SpellId),
    Swap {
        give: SpellId,
        get: SpellId,
    },
    Back,
    Leave,
    /// Not available: the row is shown dimmed.
    Nothing,
}

#[derive(Debug, Clone)]
struct Row {
    label: String,
    enabled: bool,
    act: Act,
}

fn row(label: impl Into<String>, enabled: bool, act: Act) -> Row {
    Row {
        label: format!("[ {}", label.into()),
        enabled,
        act,
    }
}

/// The diplomacy screen's state.
#[derive(Debug, Clone)]
pub(crate) struct DiploUi {
    pub with: PlayerId,
    pub menu: Menu,
    /// What the wizard last said, or the question over the menu.
    pub message: String,
    pub mood: Mood,
    /// When the wizard began to speak (seconds), for the lips.
    pub talk_since: f64,
    /// The talk is over once the reply has been read.
    pub leave_after: bool,
    /// When the room was entered and, once the player has left, when: the room fades in and out.
    pub opened: f64,
    pub leaving: Option<f64>,
}

/// Seconds the room takes to fade in and out.
const FADE: f64 = 0.35;

impl DiploUi {
    pub(crate) fn mood_is_bad(&self) -> bool {
        self.mood == Mood::Bad
    }
}

impl Default for DiploUi {
    fn default() -> Self {
        Self {
            with: PlayerId(1),
            menu: Menu::Main,
            message: String::new(),
            mood: Mood::Neutral,
            talk_since: 0.0,
            leave_after: false,
            opened: 0.0,
            leaving: None,
        }
    }
}

impl App {
    /// Talk to a wizard met. Does nothing for one not met.
    pub fn open_diplomacy(&mut self, with: PlayerId) {
        if !self.game.have_met(HUMAN, with) {
            return;
        }
        if self.game.wizard(HUMAN).is_some_and(|w| w.is_banished()) {
            self.log.push(self.text("log.banished_diplomacy"));
            return;
        }
        let level = relation_level(self.game.relation(HUMAN, with));
        let first = self
            .game
            .diplomacy
            .stance(HUMAN, with)
            .is_some_and(|s| s.stage == Stage::Greeted);
        let salt = (self.game.turn as usize).wrapping_add(usize::from(with.0));
        let personality = self
            .game
            .wizard(with)
            .map(|w| w.personality())
            .unwrap_or_default();
        let mut ui = DiploUi {
            with,
            ..Default::default()
        };
        if first {
            ui.menu = Menu::Greeting;
            ui.message = self
                .original_say(say::greeting_situation(personality), with, salt, &[])
                .unwrap_or_else(|| say::greeting(self.texts(), level, personality, salt));
            ui.mood = if level >= 7 {
                Mood::Good
            } else if level <= 3 {
                Mood::Bad
            } else {
                Mood::Neutral
            };
            self.game.apply(Command::Diplomacy(
                f4x_core::diplomacy::DiplomacyCommand::Greeted {
                    player: HUMAN,
                    with,
                },
            ));
        } else if self.audience_refused(with) {
            ui.menu = Menu::Reply;
            ui.message = self
                .original_say(71, with, salt, &[])
                .unwrap_or_else(|| say::cold_shoulder(self.texts(), salt));
            ui.mood = Mood::Bad;
            ui.leave_after = true;
        } else {
            ui.message = say::menu_prompt(self.texts());
        }
        ui.talk_since = get_time();
        ui.opened = get_time();
        self.diplo_ui = ui;
        self.screen = Screen::Diplomacy;
    }

    /// A wizard that has been pushed too far will not see you: the worst patience is far gone.
    fn audience_refused(&self, with: PlayerId) -> bool {
        self.game.diplomacy.stance(HUMAN, with).is_some_and(|s| {
            s.treaty_patience
                .min(s.exchange_patience)
                .min(s.peace_patience)
                < -100
        })
    }

    pub(crate) fn diplo_name(&self, p: PlayerId) -> String {
        self.game
            .wizard(p)
            .map_or_else(|| "?".to_string(), |w| w.name())
    }

    /// The wizards another proposal can be about: met by both of us.
    fn third_wizards(&self, with: PlayerId, war: bool) -> Vec<PlayerId> {
        self.game
            .wizards()
            .into_iter()
            .filter(|&t| t != HUMAN && t != with)
            .filter(|&t| {
                let kind = if war {
                    ProposalKind::WarOn(t)
                } else {
                    ProposalKind::BreakAllianceWith(t)
                };
                self.game.check_proposal(HUMAN, with, kind).is_ok()
            })
            .collect()
    }

    /// Spells `a` knows and `b` lacks, the more valuable first.
    fn spells_to_give(&self, a: PlayerId, b: PlayerId) -> Vec<SpellId> {
        let (Some(ma), Some(mb)) = (self.game.wizard(a), self.game.wizard(b)) else {
            return Vec::new();
        };
        let spells = &self.game.content().rules.spells;
        let mut out: Vec<SpellId> = ma
            .known()
            .iter()
            .copied()
            .filter(|s| !mb.knows(*s))
            .collect();
        out.sort_by_key(|&s| (std::cmp::Reverse(spells.get(s).research_cost), s));
        out
    }

    /// The realms (indexes into the rules' realm list) with a spell `with` could give.
    fn exchange_realms(&self, with: PlayerId) -> Vec<usize> {
        let rules = &self.game.content().rules;
        let spells = self.spells_to_give(with, HUMAN);
        rules
            .realms
            .defs()
            .iter()
            .enumerate()
            .filter(|(_, r)| spells.iter().any(|&s| rules.spells.get(s).realm == r.id))
            .map(|(i, _)| i)
            .collect()
    }

    /// The rows of the menu now, and the prompt over them.
    fn diplo_rows(&self) -> (String, Vec<Row>) {
        let with = self.diplo_ui.with;
        let status = self.game.treaty(HUMAN, with);
        let ok = |kind| self.game.check_proposal(HUMAN, with, kind).is_ok();
        let forget = || row(self.text("diplomacy.forget_it"), true, Act::Back);
        match self.diplo_ui.menu {
            Menu::Main => {
                let war = status == Treaty::War;
                let gold = self.game.player(HUMAN).map_or(0, |p| p.treasury().gold);
                let rows = vec![
                    row(
                        self.text("diplomacy.propose_treaty"),
                        true,
                        Act::Go(Menu::Propose),
                    ),
                    row(
                        self.text("diplomacy.threaten_break"),
                        !war,
                        Act::Go(Menu::Threaten),
                    ),
                    row(
                        self.text("diplomacy.offer_tribute"),
                        gold >= 25,
                        Act::Go(Menu::Tribute),
                    ),
                    row(
                        self.text("diplomacy.exchange_spells"),
                        !war,
                        Act::Go(Menu::ExchangeRealm),
                    ),
                    row(self.text("diplomacy.good_bye"), true, Act::Leave),
                ];
                (say::menu_prompt(self.texts()), rows)
            }
            Menu::Propose => {
                let rows = vec![
                    row(
                        self.text("diplomacy.wizard_pact"),
                        ok(ProposalKind::Pact),
                        Act::Propose(ProposalKind::Pact),
                    ),
                    row(
                        self.text("diplomacy.alliance"),
                        ok(ProposalKind::Alliance),
                        Act::Propose(ProposalKind::Alliance),
                    ),
                    row(
                        self.text("diplomacy.peace_treaty"),
                        ok(ProposalKind::Peace),
                        Act::Propose(ProposalKind::Peace),
                    ),
                    row(
                        self.text("diplomacy.declare_war"),
                        !self.third_wizards(with, true).is_empty(),
                        Act::Go(Menu::Third(true)),
                    ),
                    row(
                        self.text("diplomacy.break_alliance_with"),
                        !self.third_wizards(with, false).is_empty(),
                        Act::Go(Menu::Third(false)),
                    ),
                    forget(),
                ];
                (say::propose_prompt(self.texts()), rows)
            }
            Menu::Third(war) => {
                let mut rows: Vec<Row> = self
                    .third_wizards(with, war)
                    .into_iter()
                    .map(|t| {
                        let kind = if war {
                            ProposalKind::WarOn(t)
                        } else {
                            ProposalKind::BreakAllianceWith(t)
                        };
                        row(self.diplo_name(t), true, Act::Propose(kind))
                    })
                    .collect();
                rows.push(forget());
                (say::third_prompt(self.texts(), war), rows)
            }
            Menu::Threaten => {
                let rows = vec![
                    row(
                        self.text("diplomacy.break_pact"),
                        status == Treaty::Pact,
                        if status == Treaty::Pact {
                            Act::Break
                        } else {
                            Act::Nothing
                        },
                    ),
                    row(
                        self.text("diplomacy.break_alliance"),
                        status == Treaty::Alliance,
                        if status == Treaty::Alliance {
                            Act::Break
                        } else {
                            Act::Nothing
                        },
                    ),
                    row(
                        self.text("diplomacy.threaten_attack"),
                        ok(ProposalKind::Threat),
                        Act::Propose(ProposalKind::Threat),
                    ),
                    forget(),
                ];
                (say::threaten_prompt(self.texts()), rows)
            }
            Menu::Tribute => {
                let gold = self.game.player(HUMAN).map_or(0, |p| p.treasury().gold);
                let cap = gold / 25 * 25;
                let amounts: Vec<i32> = if cap < 100 {
                    (1..=cap / 25).map(|i| i * 25).collect()
                } else {
                    [cap / 4, cap / 2, cap * 3 / 4, cap]
                        .iter()
                        .map(|a| a / 25 * 25)
                        .collect()
                };
                let mut rows: Vec<Row> = amounts
                    .into_iter()
                    .map(|a| {
                        row(
                            self.text_with("diplomacy.gold_amount", &Args::new().count("n", a)),
                            true,
                            Act::Gold(a),
                        )
                    })
                    .collect();
                rows.push(row(
                    self.text("diplomacy.spells"),
                    !self.spells_to_give(HUMAN, with).is_empty(),
                    Act::Go(Menu::TributeSpell),
                ));
                rows.push(forget());
                (say::tribute_prompt(self.texts()), rows)
            }
            Menu::TributeSpell => {
                let mut rows: Vec<Row> = self
                    .spells_to_give(HUMAN, with)
                    .into_iter()
                    .take(5)
                    .map(|s| {
                        let name = self.game.content().rules.spells.get(s).name.clone();
                        row(name, true, Act::GiveSpell(s))
                    })
                    .collect();
                rows.push(forget());
                (say::tribute_prompt(self.texts()), rows)
            }
            Menu::ExchangeRealm => {
                let realms = self.exchange_realms(with);
                if realms.is_empty() || self.spells_to_give(HUMAN, with).is_empty() {
                    return (say::nothing_to_trade(self.texts()), vec![forget()]);
                }
                let mut rows: Vec<Row> = realms
                    .into_iter()
                    .map(|i| {
                        let name = self.game.content().rules.realms.defs()[i].name.clone();
                        row(name, true, Act::Go(Menu::ExchangeGet(i)))
                    })
                    .collect();
                rows.push(forget());
                (say::exchange_prompt(self.texts()), rows)
            }
            Menu::ExchangeGet(realm) => {
                let rules = &self.game.content().rules;
                let id = rules.realms.defs()[realm].id.clone();
                let mut rows: Vec<Row> = self
                    .spells_to_give(with, HUMAN)
                    .into_iter()
                    .filter(|&s| rules.spells.get(s).realm == id)
                    .take(5)
                    .map(|s| {
                        row(
                            rules.spells.get(s).name.clone(),
                            true,
                            Act::Go(Menu::ExchangeGive(s)),
                        )
                    })
                    .collect();
                rows.push(forget());
                (say::exchange_prompt(self.texts()), rows)
            }
            Menu::ExchangeGive(get) => {
                let rules = &self.game.content().rules;
                let mut rows: Vec<Row> = self
                    .spells_to_give(HUMAN, with)
                    .into_iter()
                    .take(5)
                    .map(|s| {
                        row(
                            rules.spells.get(s).name.clone(),
                            true,
                            Act::Swap { give: s, get },
                        )
                    })
                    .collect();
                rows.push(forget());
                (say::exchange_give_prompt(self.texts()), rows)
            }
            Menu::Greeting | Menu::Reply => (self.diplo_ui.message.clone(), Vec::new()),
        }
    }

    /// The wizard speaks: the message is shown, the lips move, and the face takes the mood.
    fn diplo_say(&mut self, text: impl Into<String>, mood: Mood, leave_after: bool) {
        self.diplo_ui.message = text.into();
        self.diplo_ui.mood = mood;
        self.diplo_ui.talk_since = get_time();
        self.diplo_ui.menu = Menu::Reply;
        self.diplo_ui.leave_after = leave_after;
    }

    fn wizard_name(&self, p: PlayerId) -> String {
        self.game
            .wizard(p)
            .map_or_else(|| self.text("log.a_rival"), |w| w.name().to_string())
    }

    /// The original's line for `situation` when its text is on this machine, with the human's
    /// and the wizard's names, the year and `extra` filled in.
    fn original_say(
        &self,
        situation: u16,
        with: PlayerId,
        salt: usize,
        extra: &[(&str, String)],
    ) -> Option<String> {
        let name = |p: PlayerId| {
            self.game
                .wizard(p)
                .map_or_else(|| self.text("log.a_rival"), |w| w.name().to_string())
        };
        let mut values = vec![
            ("player", name(HUMAN)),
            ("wizard", name(with)),
            ("year", (1400 + self.game.turn / 12).to_string()),
        ];
        values.extend(extra.iter().map(|(k, v)| (*k, v.clone())));
        say::original_line(
            &self.diplomsg,
            situation,
            salt,
            self.game
                .player_ids()
                .into_iter()
                .filter(|&p| self.game.wizard(p).is_some())
                .count()
                == 2,
            &values,
        )
    }

    /// Put a proposal and show the wizard's answer.
    fn diplo_propose(&mut self, kind: ProposalKind) {
        let with = self.diplo_ui.with;
        let salt = self.game.turn as usize + usize::from(with.0);
        let level = relation_level(self.game.relation(HUMAN, with));
        let mut events = self.game.apply(Command::Diplomacy(
            f4x_core::diplomacy::DiplomacyCommand::Propose {
                from: HUMAN,
                to: with,
                kind,
            },
        ));
        if events.iter().any(|e| matches!(e, Event::Rejected(_))) {
            self.note(events);
            let said = self
                .original_say(57, with, salt, &[])
                .unwrap_or_else(|| say::refused(self.texts(), kind, level, salt));
            self.diplo_say(said, Mood::Neutral, false);
            return;
        }
        events.extend(f4x_ai::answer_proposals(&mut self.game, with));
        let answer = events.iter().find_map(|e| match e {
            Event::Diplomacy(f4x_core::diplomacy::DiplomacyEvent::ProposalAnswered {
                accepted,
                lapsed,
                ..
            }) => Some((*accepted, *lapsed)),
            _ => None,
        });
        self.note(events);
        match answer {
            Some((true, _)) => {
                let target = match kind {
                    ProposalKind::WarOn(t) | ProposalKind::BreakAllianceWith(t) => {
                        Some(self.wizard_name(t))
                    }
                    _ => None,
                };
                let extra: Vec<(&str, String)> = target
                    .into_iter()
                    .flat_map(|t| [("target", t.clone()), ("third_other", t)])
                    .collect();
                let said = say::accepted_situation(kind)
                    .and_then(|n| self.original_say(n, with, salt, &extra))
                    .unwrap_or_else(|| say::accepted(self.texts(), kind, salt));
                self.diplo_say(said, Mood::Good, false)
            }
            Some((false, false)) if kind == ProposalKind::Threat => {
                let at_war = self.game.treaty(HUMAN, with) == Treaty::War;
                let key = if at_war {
                    "diplomacy.threat_war"
                } else {
                    "diplomacy.refused_threat"
                };
                let said = self.text(key);
                self.diplo_say(said, Mood::Bad, false);
            }
            Some((false, false)) => {
                let level = relation_level(self.game.relation(HUMAN, with));
                let said = self
                    .original_say(57, with, salt, &[])
                    .unwrap_or_else(|| say::refused(self.texts(), kind, level, salt));
                self.diplo_say(said, Mood::Bad, false);
            }
            _ => self.diplo_say(say::lapsed(self.texts()), Mood::Neutral, false),
        }
    }

    fn diplo_act(&mut self, act: Act) {
        let with = self.diplo_ui.with;
        let salt = self.game.turn as usize + usize::from(with.0);
        match act {
            Act::Nothing => {}
            Act::Go(menu) => {
                self.diplo_ui.menu = menu;
                self.diplo_ui.message = self.diplo_rows().0;
            }
            Act::Back => {
                self.diplo_ui.menu = Menu::Main;
                self.diplo_ui.message = say::menu_prompt(self.texts());
            }
            Act::Leave => {
                if self.diplo_ui.leaving.is_none() {
                    self.diplo_ui.leaving = Some(get_time());
                }
            }
            Act::Propose(kind) => self.diplo_propose(kind),
            Act::Break => {
                let events = self.game.apply(Command::Diplomacy(
                    f4x_core::diplomacy::DiplomacyCommand::BreakTreaty {
                        player: HUMAN,
                        with,
                    },
                ));
                self.note(events);
                self.diplo_say(say::treaty_broken(self.texts()), Mood::Bad, false);
            }
            Act::Gold(amount) => {
                let events = self.game.apply(Command::Diplomacy(
                    f4x_core::diplomacy::DiplomacyCommand::GiveGold {
                        from: HUMAN,
                        to: with,
                        amount,
                    },
                ));
                let failed = events.iter().any(|e| matches!(e, Event::Rejected(_)));
                self.note(events);
                if !failed {
                    let said = self
                        .original_say(1, with, salt, &[])
                        .unwrap_or_else(|| say::gold_thanks(self.texts(), salt));
                    self.diplo_say(said, Mood::Good, false);
                }
            }
            Act::GiveSpell(spell) => {
                let events = self.game.apply(Command::Diplomacy(
                    f4x_core::diplomacy::DiplomacyCommand::GiveSpell {
                        from: HUMAN,
                        to: with,
                        spell,
                    },
                ));
                let failed = events.iter().any(|e| matches!(e, Event::Rejected(_)));
                self.note(events);
                if !failed {
                    let gift = self.game.content().rules.spells.get(spell).name.clone();
                    let said = self
                        .original_say(54, with, salt, &[("gift", gift)])
                        .unwrap_or_else(|| say::spell_thanks(self.texts(), salt));
                    self.diplo_say(said, Mood::Good, false);
                }
            }
            Act::Swap { give, get } => self.diplo_propose(ProposalKind::Exchange { give, get }),
        }
    }

    /// Break a long message into lines that fit `width` in a font style.
    fn diplo_wrap(&self, style: usize, text: &str, width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in text.split_whitespace() {
            let trial = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if self.art.text_width(style, &trial) > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = trial;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }

    /// The log line for something that happened between wizards, and the greeting a wizard just
    /// met brings: it comes on the turn after the meeting.
    pub(crate) fn diplomacy_line(
        &mut self,
        d: f4x_core::diplomacy::DiplomacyEvent,
    ) -> Option<String> {
        use f4x_core::diplomacy::DiplomacyEvent as D;
        match d {
            D::ContactMade { a, b } if a == HUMAN || b == HUMAN => {
                let other = if a == HUMAN { b } else { a };
                self.greetings_due.push(other);
                Some(self.text_with(
                    "diplomacy.met",
                    &Args::new().text("wizard", self.diplo_name(other)),
                ))
            }
            D::WarDeclared { attacker, defender } if attacker == HUMAN || defender == HUMAN => {
                let other = if attacker == HUMAN {
                    defender
                } else {
                    attacker
                };
                Some(self.text_with(
                    "diplomacy.at_war_with_you",
                    &Args::new().text("wizard", self.diplo_name(other)),
                ))
            }
            D::WarDeclared { attacker, defender } => {
                let args = Args::new()
                    .text("attacker", self.diplo_name(attacker))
                    .text("defender", self.diplo_name(defender));
                Some(self.text_with("diplomacy.declares_war", &args))
            }
            D::TreatyBroken {
                breaker, victim, ..
            } => {
                let args = Args::new()
                    .text("breaker", self.diplo_name(breaker))
                    .text("victim", self.diplo_name(victim));
                Some(self.text_with("diplomacy.breaks_treaty", &args))
            }
            D::Warned { from, to } if to == HUMAN => Some(self.text_with(
                "diplomacy.warns_you",
                &Args::new().text("wizard", self.diplo_name(from)),
            )),
            D::Trespassing { owner, intruder } if intruder == HUMAN => Some(self.text_with(
                "diplomacy.trespass",
                &Args::new().text("wizard", self.diplo_name(owner)),
            )),
            D::GoldGiven { from, to, gold } if to == HUMAN && from != HUMAN => Some(
                self.text_with(
                    "diplomacy.gives_gold",
                    &Args::new()
                        .text("wizard", self.diplo_name(from))
                        .count("gold", gold),
                ),
            ),
            D::SpellGiven { from, to, spell } if to == HUMAN && from != HUMAN => {
                let name = self.game.content().rules.spells.get(spell).name.clone();
                Some(
                    self.text_with(
                        "diplomacy.gives_spell",
                        &Args::new()
                            .text("wizard", self.diplo_name(from))
                            .text("spell", name),
                    ),
                )
            }
            D::TreatyChanged { a, b, treaty } if a == HUMAN || b == HUMAN => {
                let other = if a == HUMAN { b } else { a };
                let key = match treaty {
                    Treaty::None | Treaty::War => return None,
                    Treaty::Pact => "diplomacy.have_pact",
                    Treaty::Alliance => "diplomacy.have_alliance",
                };
                Some(self.text_with(key, &Args::new().text("wizard", self.diplo_name(other))))
            }
            _ => None,
        }
    }

    /// The screen as a screenshot shows it (`--talk MENU`): a wizard met, a relation, a menu.
    pub fn open_test_talk(&mut self, menu: &str, relation: i32) {
        let Some(with) = self.game.wizards().into_iter().find(|&w| w != HUMAN) else {
            return;
        };
        self.game.make_contact(HUMAN, with);
        let step = if relation >= 0 { 8 } else { -8 };
        for _ in 0..(relation.abs() / 8 + 12) {
            let now = self.game.relation(HUMAN, with);
            if (relation - now).abs() < 8 {
                break;
            }
            if step > 0 {
                self.game.change_relations(
                    8,
                    HUMAN,
                    with,
                    f4x_core::diplomacy::Action::Friendship,
                    None,
                );
            } else {
                self.game.change_relations(
                    -8,
                    HUMAN,
                    with,
                    f4x_core::diplomacy::Action::AttackedCity,
                    None,
                );
            }
        }
        self.game.drain_diplomacy_events();
        self.open_diplomacy(with);
        let menu = match menu {
            "propose" => Menu::Propose,
            "threaten" => Menu::Threaten,
            "tribute" => Menu::Tribute,
            "exchange" => Menu::ExchangeRealm,
            "greeting" => return,
            "gems" => {
                self.open_magic();
                return;
            }
            _ => Menu::Main,
        };
        if self.diplo_ui.menu == Menu::Greeting {
            self.diplo_ui.menu = Menu::Main;
        }
        self.diplo_act(Act::Go(menu));
        self.diplo_ui.talk_since = get_time() - 10.0;
        self.diplo_ui.opened = get_time() - 10.0;
    }

    pub(crate) fn diplomacy_screen(&mut self, input: &Input) {
        let with = self.diplo_ui.with;
        if !self.game.have_met(HUMAN, with) {
            self.screen = Screen::Magic;
            return;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let level = relation_level(self.game.relation(HUMAN, with));
        let portrait = self
            .game
            .wizard(with)
            .map_or(0, |w| usize::from(w.portrait()));

        // The room, the wizard, the mirror's frame over him, and the gargoyles' eyes.
        self.draw_pic(&lay, "diplomac", 0, 0, 0.0, 0.0);
        let talking = get_time() - self.diplo_ui.talk_since;
        let frame = (talking / TALK_FRAME) as usize;
        if frame < TALK_FRAMES && self.diplo_ui.menu != Menu::Main {
            self.draw_pic(
                &lay,
                "diplomac",
                24 + portrait,
                frame,
                PORTRAIT_AT.0,
                PORTRAIT_AT.1,
            );
        } else {
            let mood = self.diplo_ui.mood.frame();
            self.draw_pic(
                &lay,
                "moodwiz",
                portrait,
                mood,
                PORTRAIT_AT.0,
                PORTRAIT_AT.1,
            );
        }
        self.draw_pic(&lay, "backgrnd", 18, 0, MIRROR_AT.0, MIRROR_AT.1);
        self.draw_pic(
            &lay,
            "diplomac",
            2 + level,
            0,
            LEFT_EYES_AT.0,
            LEFT_EYES_AT.1,
        );
        self.draw_pic(
            &lay,
            "diplomac",
            13 + level,
            0,
            RIGHT_EYES_AT.0,
            RIGHT_EYES_AT.1,
        );

        // What is said, and what may be answered.
        let (prompt, rows) = self.diplo_rows();
        let text = if rows.is_empty() {
            self.diplo_ui.message.clone()
        } else {
            prompt
        };
        let line_height = self.art.text_height(MESSAGE_STYLE) + 1.0;
        let mut y = TEXT_Y;
        for line in self.diplo_wrap(MESSAGE_STYLE, &text, TEXT_W) {
            self.bm_text_set(
                view,
                MESSAGE_STYLE,
                MESSAGE_SET,
                &line,
                TEXT_X,
                y,
                Align::Left,
            );
            y += line_height;
        }

        let click = input.mouse_pressed(MouseButton::Left);
        let mut chosen: Option<Act> = None;
        if rows.is_empty() {
            // A message waits to be read.
            if click || input.key_pressed(KeyCode::Space) || input.key_pressed(KeyCode::Enter) {
                if self.diplo_ui.menu == Menu::Greeting {
                    if self.audience_refused(with) {
                        let salt = self.game.turn as usize;
                        let said = self
                            .original_say(71, with, salt, &[])
                            .unwrap_or_else(|| say::cold_shoulder(self.texts(), salt));
                        self.diplo_say(said, Mood::Bad, true);
                    } else {
                        chosen = Some(Act::Back);
                    }
                } else if self.diplo_ui.leave_after {
                    chosen = Some(Act::Leave);
                } else {
                    chosen = Some(Act::Back);
                }
            }
        } else {
            let spacing = if rows.len() <= 5 {
                9.0
            } else {
                (42.0 / (rows.len() - 1) as f32).floor().min(9.0)
            };
            for (i, r) in rows.iter().enumerate() {
                let ry = MENU_Y + spacing * (i + 1) as f32;
                let rect = lay.rect((TEXT_X, ry - 1.0, TEXT_W, 8.0));
                let over = ui::hit(rect, true, input).hovered();
                if over && r.enabled {
                    draw_rectangle(
                        rect.x,
                        rect.y,
                        rect.w,
                        rect.h,
                        Color::new(0.85, 0.7, 0.3, 0.28),
                    );
                }
                if r.enabled {
                    self.bm_text_set(
                        view,
                        MENU_STYLE,
                        MENU_SET,
                        &r.label,
                        TEXT_X,
                        ry,
                        Align::Left,
                    );
                } else {
                    // The set the original dims them with is too dark to read on this background:
                    // the enabled colours, greyed.
                    self.art.draw_bitmap_text_tinted(
                        MENU_STYLE,
                        MENU_SET,
                        &r.label,
                        view.0 + TEXT_X * view.2,
                        view.1 + ry * view.2,
                        view.2,
                        MENU_OFF_TINT,
                    );
                }
                if over && r.enabled && click {
                    chosen = Some(r.act.clone());
                }
            }
        }
        if input.key_pressed(KeyCode::Escape) {
            chosen = Some(match self.diplo_ui.menu {
                Menu::Main | Menu::Greeting => Act::Leave,
                Menu::Reply if self.diplo_ui.leave_after => Act::Leave,
                _ => Act::Back,
            });
        }
        if self.diplo_ui.leaving.is_none() {
            if let Some(act) = chosen {
                self.diplo_act(act);
            }
        }
        // The room fades in when it is entered and out when it is left.
        let now = get_time();
        let dark = match self.diplo_ui.leaving {
            Some(since) => ((now - since) / FADE).min(1.0),
            None => 1.0 - ((now - self.diplo_ui.opened) / FADE).min(1.0),
        };
        if dark > 0.0 {
            draw_rectangle(
                0.0,
                0.0,
                screen_width(),
                screen_height(),
                Color::new(0.0, 0.0, 0.0, dark as f32),
            );
        }
        if dark >= 1.0 && self.diplo_ui.leaving.is_some() {
            self.screen = Screen::Magic;
        }
    }
}
