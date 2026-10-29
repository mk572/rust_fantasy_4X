//! The big spell book (`Spellbook.c`'s "BigBook"): the Apprentice's library of every spell the
//! wizard can learn, the dialog that offers the spells to research next, and the notice of a
//! spell just learned. Four spells to a page, each with its cost, how long it takes and what it
//! does, on the original's open-book picture (`scroll.lbx`).

use std::collections::VecDeque;

use crate::input::Input;
use crate::modal::{keep, kind, priority, Modal};
use f4x_core::game::Command;
use f4x_core::rules::SpellId;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::spellbook::BookPage;
use crate::view::{App, Screen, HUMAN};

/// Spells to a page of the big book.
const BIG_PAGE: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BigMode {
    /// Browsing the library: page turns, nothing to choose.
    #[default]
    Apprentice,
    /// Choosing the next research from the eight on offer.
    Research,
}

/// The big book's state.
#[derive(Debug, Clone, Default)]
pub(crate) struct BigBookUi {
    pub mode: BigMode,
    pub pages: Vec<BookPage>,
    /// The first of the two research pages.
    pub first_research: usize,
    pub left: usize,
    /// Spells learned and not yet announced.
    pub learned: VecDeque<SpellId>,
    /// The learn-spell animation of the first of them, once it has started (`learn_spell.rs`).
    pub learn: Option<crate::learn_spell::LearnAnim>,
}

/// What a spell does: the text of its help topic (`spell:<id>`). Empty without one.
pub(crate) fn describe_spell(help: &f4x_core::help::Help, id: &str) -> String {
    help.topic_for(&f4x_core::help::spell_subject(id))
        .map(|t| t.text.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

impl App {
    /// The paragraph that tells what a spell does.
    fn spell_text(&self, spell: SpellId) -> String {
        describe_spell(&self.help, &self.game.content().rules.spells.get(spell).id)
    }

    /// Break `text` into lines no wider than `width` of the original's pixels.
    pub(crate) fn wrap_text(&self, style: usize, text: &str, width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in text.split_whitespace() {
            let try_line = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && self.art.text_width(style, &try_line) > width {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = try_line;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }

    /// Open the Apprentice's library: every spell the books allow, known or not, and the two
    /// pages of spells on offer for research at the end.
    pub fn open_apprentice(&mut self) {
        if self.build_library_pages() {
            self.screen = Screen::Apprentice;
        }
    }

    /// Lay out the library (every spell known or researchable, then the research pages) in
    /// `big_ui`, open at the research pages. False without a wizard.
    pub(crate) fn build_library_pages(&mut self) -> bool {
        let Some(m) = self.game.wizard(HUMAN) else {
            return false;
        };
        let all: Vec<SpellId> = m
            .known()
            .iter()
            .chain(m.researchable().iter())
            .copied()
            .collect();
        let mut pages = self.book_pages(&all, BIG_PAGE);
        let first = pages.len();
        pages.extend(self.research_pages());
        self.big_ui.pages = pages;
        self.big_ui.first_research = first;
        self.big_ui.left = first;
        self.big_ui.mode = BigMode::Apprentice;
        true
    }

    /// The two pages that list the spells on offer for research.
    fn research_pages(&self) -> Vec<BookPage> {
        let offer = self
            .game
            .wizard(HUMAN)
            .map(|m| m.candidates().clone())
            .unwrap_or_default();
        let page = |i: usize, title: &str| BookPage {
            title: title.to_string(),
            spells: offer
                .iter()
                .copied()
                .skip(i * BIG_PAGE)
                .take(BIG_PAGE)
                .collect(),
        };
        vec![
            page(0, &self.text("bigbook.research")),
            page(1, &self.text("bigbook.spells")),
        ]
    }

    /// The dialog that asks what to research next.
    pub fn open_research_choice(&mut self) {
        self.big_ui.pages = self.research_pages();
        self.big_ui.first_research = 0;
        self.big_ui.left = 0;
        self.big_ui.mode = BigMode::Research;
        self.screen = Screen::Research;
    }

    /// At the start of a turn with nothing being researched and spells on offer (and the game
    /// past its first turn, as the original has it), ask what to research.
    pub(crate) fn maybe_ask_for_research(&mut self) {
        if !matches!(self.screen, Screen::Map) || self.game.in_battle() || self.game.turn <= 1 {
            return;
        }
        let idle = self
            .game
            .wizard(HUMAN)
            .is_some_and(|m| m.researching().is_none() && !m.candidates().is_empty());
        // A spell the player asked to learn next goes first, if it is on offer now.
        if let Some(spell) = self.queued_research.filter(|_| idle) {
            self.queued_research = None;
            if self
                .game
                .wizard(HUMAN)
                .is_some_and(|m| m.candidates().contains(&spell))
            {
                let events = self.game.apply(Command::SetResearch {
                    player: HUMAN,
                    spell,
                });
                self.note(events);
                return;
            }
        }
        let alive = self.game.has_player(HUMAN)
            && !self.game.player(HUMAN).is_some_and(|p| p.is_defeated());
        if idle && alive && self.game.game_over.is_none() {
            self.open_research_choice();
        }
    }

    pub(crate) fn apprentice_screen(&mut self, input: &Input) {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        if self.book_turn.is_some() && !self.book_turn_frame(input) {
            return;
        }
        let pages = self.big_ui.pages.len();
        let left = self.big_ui.left;
        let clicked = input.mouse_pressed(MouseButton::Left);
        let in_rect = |x: f32, y: f32, w: f32, h: f32| {
            lay.rect((x, y, w, h))
                .contains(input.mouse_position().into())
        };
        let (_, wheel) = input.mouse_wheel();
        let forward = (clicked && in_rect(290.0, 7.0, 16.0, 16.0))
            || input.key_pressed(KeyCode::Right)
            || wheel < 0.0;
        let back = (clicked && in_rect(15.0, 7.0, 16.0, 16.0))
            || input.key_pressed(KeyCode::Left)
            || wheel > 0.0;
        if forward && left + 2 < pages {
            self.turn_book_page(true, true, input);
            return;
        } else if back && left >= 2 {
            self.turn_book_page(true, false, input);
            return;
        } else if input.key_pressed(KeyCode::Escape)
            || input.key_pressed(KeyCode::F3)
            || input.mouse_pressed(MouseButton::Right)
            || (clicked && in_rect(0.0, 25.0, 320.0, 175.0))
        {
            self.screen = Screen::Map;
            return;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_big_book(&lay, view, None);
    }

    pub(crate) fn research_screen(&mut self, input: &Input) {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let clicked = input.mouse_pressed(MouseButton::Left);
        if clicked {
            for slot in 0..8usize {
                let (x, y) = (
                    25.0 + 147.0 * (slot / 4) as f32,
                    25.0 + 37.0 * (slot % 4) as f32,
                );
                if lay
                    .rect((x, y, 110.0, 33.0))
                    .contains(input.mouse_position().into())
                {
                    let Some(spell) = self
                        .game
                        .wizard(HUMAN)
                        .and_then(|m| m.candidates().get(slot).copied())
                    else {
                        continue;
                    };
                    let events = self.game.apply(Command::SetResearch {
                        player: HUMAN,
                        spell,
                    });
                    let ok = !events
                        .iter()
                        .any(|e| matches!(e, f4x_core::game::Event::Rejected(_)));
                    self.note(events);
                    if ok {
                        self.screen = Screen::Map;
                        return;
                    }
                }
            }
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_big_book(&lay, view, None);
        let prompt = Color::new(1.0, 0.95, 0.8, 1.0);
        self.bm_text_outlined(
            view,
            5,
            &self.text("bigbook.choose_research"),
            43.0,
            183.0,
            Align::Left,
        );
        let _ = prompt;
    }

    /// The open book: the dog-ears (browsing only), and the two pages.
    pub(crate) fn draw_big_book(
        &mut self,
        lay: &Layout,
        view: (f32, f32, f32),
        hide: Option<(usize, usize)>,
    ) {
        let (pages, left) = (self.big_ui.pages.len(), self.big_ui.left);
        let browsing = self.big_ui.mode == BigMode::Apprentice;
        let corners = (browsing && left >= 1, browsing && left + 2 < pages);
        self.draw_big_spread(lay, view, left, left + 1, corners, hide);
    }

    /// The book picture with the pages `left_i` and `right_i` and the dog-ears `corners` asked
    /// for (a page turn shows other pages than the current spread).
    pub(crate) fn draw_big_spread(
        &mut self,
        lay: &Layout,
        view: (f32, f32, f32),
        left_i: usize,
        right_i: usize,
        corners: (bool, bool),
        hide: Option<(usize, usize)>,
    ) {
        let full = lay.rect((0.0, 0.0, 320.0, 200.0));
        if self.draw_pic(lay, "scroll", 6, 0, 0.0, 0.0).is_none() {
            draw_rectangle(
                full.x,
                full.y,
                full.w,
                full.h,
                Color::new(0.55, 0.4, 0.2, 1.0),
            );
        }
        if corners.0 {
            self.draw_pic(lay, "scroll", 7, 0, 15.0, 9.0);
        }
        if corners.1 {
            self.draw_pic(lay, "scroll", 8, 0, 289.0, 9.0);
        }
        for (side, page_i) in [(0usize, left_i), (1, right_i)] {
            let x0 = if side == 0 { 25.0 } else { 173.0 };
            let title = (x0 + if side == 0 { 66.0 } else { 58.0 }, 10.0);
            self.draw_big_page(lay, view, page_i, side == 1, x0, title, 0.0, hide);
        }
    }

    /// A page of the big book: its title (on the left page and on a right page that starts a
    /// realm) centred at `title`, and its spells with the page's left edge at `x0` and `dy`
    /// added to their y.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_big_page(
        &mut self,
        lay: &Layout,
        view: (f32, f32, f32),
        page_i: usize,
        right: bool,
        x0: f32,
        title: (f32, f32),
        dy: f32,
        hide: Option<(usize, usize)>,
    ) {
        let Some(page) = self.big_ui.pages.get(page_i).cloned() else {
            return;
        };
        let starts_realm = page_i == self.big_ui.first_research
            || page_i == 0
            || self
                .big_ui
                .pages
                .get(page_i - 1)
                .is_none_or(|p| p.title != page.title);
        if !right || starts_realm {
            self.bm_text(
                view,
                5,
                Color::new(0.45, 0.2, 0.1, 1.0),
                &page.title,
                title.0,
                title.1,
                Align::Center,
            );
        }
        let research_page = page_i >= self.big_ui.first_research
            && self.big_ui.mode == BigMode::Apprentice
            || self.big_ui.mode == BigMode::Research;
        for (i, &spell) in page.spells.iter().enumerate() {
            if hide == Some((page_i, i)) {
                continue;
            }
            self.draw_big_entry(
                lay,
                view,
                spell,
                x0,
                28.0 + 37.0 * i as f32 + dy,
                research_page,
                None,
            );
        }
    }

    /// One spell on a big page: its name, cost and time, and a paragraph on what it does.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_big_entry(
        &mut self,
        _lay: &Layout,
        view: (f32, f32, f32),
        spell: SpellId,
        x: f32,
        y: f32,
        research_page: bool,
        ink_override: Option<Color>,
    ) {
        let def = self.game.content().rules.spells.get(spell).clone();
        let magic = self.game.wizard(HUMAN);
        let known = magic.is_some_and(|m| m.knows(spell));
        let current = magic.is_some_and(|m| m.researching() == Some(spell));
        let research_left = magic.map_or(0, |m| m.research_remaining());
        let ink = Color::new(0.2, 0.12, 0.05, 1.0);
        let faint = Color::new(0.5, 0.4, 0.3, 1.0);
        // The spell being researched glows (palette index 198 cycling, `BigBook_Draw`).
        let hot = crate::cycle::BIG_BOOK.colour(get_time());
        let color = ink_override.unwrap_or(if current {
            hot
        } else if known || research_page {
            ink
        } else {
            faint
        });
        self.bm_text(view, 4, color, &def.name, x, y, Align::Left);
        let line = if research_page {
            let bonus = self.game.wizard(HUMAN).map_or(0, |m| {
                m.research_bonus(self.game.content().rules.spells.get(spell))
            });
            let cost = if current {
                research_left
            } else {
                def.research_cost as i32 * 100 / (100 + bonus)
            };
            let income = self.game.income(HUMAN).research.max(0);
            let turns = if income == 0 {
                999
            } else {
                (cost + income - 1) / income
            };
            self.text_with(
                "bigbook.research_cost",
                &Args::new().count("cost", cost).count("turns", turns),
            )
        } else {
            let cost = self.game.casting_cost(HUMAN, spell, false);
            let limit = self
                .game
                .wizard(HUMAN)
                .map_or(0, |m| m.casting_skill())
                .min(
                    self.game.player(HUMAN).map_or(0, |p| p.treasury().mana)
                        + self.game.income(HUMAN).split.mana,
                );
            let turns = if limit <= 0 {
                999
            } else {
                (cost + limit - 1) / limit
            };
            self.text_with(
                "bigbook.casting_cost",
                &Args::new().count("cost", cost).count("turns", turns),
            )
        };
        self.bm_text(view, 0, color, &line, x, y + 11.0, Align::Left);
        let text = self.spell_text(spell);
        let line_h = self.art.text_height(0) + 1.0;
        for (n, l) in self
            .wrap_text(0, &text, 128.0)
            .into_iter()
            .take(3)
            .enumerate()
        {
            self.bm_text(
                view,
                0,
                color,
                &l,
                x,
                y + 17.0 + line_h * n as f32,
                Align::Left,
            );
        }
    }
}

/// A wizard has started casting the Spell of Mastery.
pub(crate) struct MasteryNotice(pub(crate) f4x_core::player::PlayerId);

impl Modal for MasteryNotice {
    fn kind(&self) -> &'static str {
        kind::MASTERY
    }
    fn priority(&self) -> u8 {
        priority::MASTERY
    }
    fn music(&self) -> Option<crate::audio::Track> {
        Some(crate::audio::Track::piece(crate::audio::music::SOM_STARTED))
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.mastery_notice_frame(*self, input))
    }
}

impl App {
    /// "<wizard> has started casting the Spell of Mastery" over a black screen, as the original
    /// does (`Cast_Spell_Of_Mastery`; its swirling animation is local art not drawn here yet,
    /// task 287); any key or click goes on.
    pub(crate) fn mastery_notice_frame(
        &mut self,
        MasteryNotice(caster): MasteryNotice,
        input: &Input,
    ) -> Option<MasteryNotice> {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        clear_background(Color::new(0.02, 0.0, 0.06, 1.0));
        let args = self.wizard_arg(Args::new(), "wizard", caster);
        let ink = Color::new(0.95, 0.85, 0.6, 1.0);
        self.bm_text(
            view,
            5,
            ink,
            &self.text_with("bigbook.mastery_started", &args),
            160.0,
            90.0,
            Align::Center,
        );
        self.bm_text(
            view,
            5,
            ink,
            &self.text("bigbook.mastery_name"),
            160.0,
            110.0,
            Align::Center,
        );
        if input.mouse_pressed(MouseButton::Left)
            || input.key_pressed(KeyCode::Enter)
            || input.key_pressed(KeyCode::Escape)
            || input.key_pressed(KeyCode::Space)
        {
            return None;
        }
        Some(MasteryNotice(caster))
    }
}

pub(crate) fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::help::{Help, HelpTopic};

    #[test]
    fn a_spells_description_is_the_text_of_its_topic() {
        let help = Help {
            topics: vec![HelpTopic {
                index: 2000,
                title: "Gaia's Blessing".into(),
                text: "A city's food\ngrows.".into(),
                picture: None,
                include: None,
                subject: Some("spell:gaias_blessing".into()),
            }],
            screens: Vec::new(),
        };
        assert_eq!(
            describe_spell(&help, "gaias_blessing"),
            "A city's food grows."
        );
        assert_eq!(describe_spell(&help, "nothing"), "");
    }
}
