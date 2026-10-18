//! The big spell book (`Spellbook.c`'s "BigBook"): the Apprentice's library of every spell the
//! wizard can learn, the dialog that offers the spells to research next, and the notice of a
//! spell just learned. Four spells to a page, each with its cost, how long it takes and what it
//! does, on the original's open-book picture (`scroll.lbx`).

use std::collections::VecDeque;

use f4x_core::rules::SpellId;
use f4x_core::Command;
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

/// What a spell does, from the help text: the topic named for it, less its heading lines
/// (effects, target, cost, upkeep). Empty without the help.
pub(crate) fn describe_spell(help: &f4x_core::help::Help, name: &str) -> String {
    let Some(topic) = help
        .topics
        .iter()
        .find(|t| t.title.eq_ignore_ascii_case(name) && t.text.contains("Casting Cost"))
    else {
        return String::new();
    };
    let mut lines = topic.text.lines();
    for line in lines.by_ref() {
        if line.starts_with("Upkeep") {
            break;
        }
    }
    lines.collect::<Vec<_>>().join(" ")
}

impl App {
    /// The paragraph that tells what a spell does.
    fn spell_text(&self, spell: SpellId) -> String {
        describe_spell(
            &self.help,
            &self.game.content().rules.spells.get(spell).name,
        )
    }

    /// Break `text` into lines no wider than `width` of the original's pixels.
    fn wrap_text(&self, style: usize, text: &str, width: f32) -> Vec<String> {
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
        let Some(m) = self.game.magic(HUMAN).cloned() else {
            return false;
        };
        let all: Vec<SpellId> = m
            .known
            .iter()
            .chain(m.researchable.iter())
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
            .magic(HUMAN)
            .map(|m| m.candidates.clone())
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
        vec![page(0, "Research"), page(1, "Spells")]
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
            .magic(HUMAN)
            .is_some_and(|m| m.researching.is_none() && !m.candidates.is_empty());
        let alive = self.game.player(HUMAN).is_some_and(|p| !p.defeated);
        if idle && alive && self.game.game_over.is_none() {
            self.open_research_choice();
        }
    }

    pub(crate) fn apprentice_screen(&mut self) {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let pages = self.big_ui.pages.len();
        let left = self.big_ui.left;
        let clicked = is_mouse_button_pressed(MouseButton::Left);
        let in_rect = |x: f32, y: f32, w: f32, h: f32| {
            lay.rect((x, y, w, h)).contains(mouse_position().into())
        };
        let (_, wheel) = mouse_wheel();
        let forward = (clicked && in_rect(290.0, 7.0, 16.0, 16.0))
            || is_key_pressed(KeyCode::Right)
            || wheel < 0.0;
        let back = (clicked && in_rect(15.0, 7.0, 16.0, 16.0))
            || is_key_pressed(KeyCode::Left)
            || wheel > 0.0;
        if forward && left + 2 < pages {
            self.big_ui.left += 2;
        } else if back && left >= 2 {
            self.big_ui.left -= 2;
        } else if is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::F3)
            || is_mouse_button_pressed(MouseButton::Right)
            || (clicked && in_rect(0.0, 25.0, 320.0, 175.0))
        {
            self.screen = Screen::Map;
            return;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_big_book(&lay, view, None);
    }

    pub(crate) fn research_screen(&mut self) {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let clicked = is_mouse_button_pressed(MouseButton::Left);
        if clicked {
            for slot in 0..8usize {
                let (x, y) = (
                    25.0 + 147.0 * (slot / 4) as f32,
                    25.0 + 37.0 * (slot % 4) as f32,
                );
                if lay
                    .rect((x, y, 110.0, 33.0))
                    .contains(mouse_position().into())
                {
                    let Some(spell) = self
                        .game
                        .magic(HUMAN)
                        .and_then(|m| m.candidates.get(slot).copied())
                    else {
                        continue;
                    };
                    let events = self.game.apply(Command::SetResearch {
                        player: HUMAN,
                        spell,
                    });
                    let ok = !events
                        .iter()
                        .any(|e| matches!(e, f4x_core::Event::Rejected(_)));
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
            "Choose a new spell to research",
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
        let (pages, left) = (self.big_ui.pages.len(), self.big_ui.left);
        if self.big_ui.mode == BigMode::Apprentice {
            if left >= 1 {
                self.draw_pic(lay, "scroll", 7, 0, 15.0, 9.0);
            }
            if left + 2 < pages {
                self.draw_pic(lay, "scroll", 8, 0, 289.0, 9.0);
            }
        }
        for (side, page_i) in [(0usize, left), (1, left + 1)] {
            let Some(page) = self.big_ui.pages.get(page_i).cloned() else {
                continue;
            };
            let x0 = if side == 0 { 25.0 } else { 173.0 };
            let title_x = x0 + if side == 0 { 66.0 } else { 58.0 };
            let starts_realm = page_i == self.big_ui.first_research
                || page_i == 0
                || self
                    .big_ui
                    .pages
                    .get(page_i - 1)
                    .is_none_or(|p| p.title != page.title);
            if side == 0 || starts_realm {
                self.bm_text(
                    view,
                    5,
                    Color::new(0.45, 0.2, 0.1, 1.0),
                    &page.title,
                    title_x,
                    10.0,
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
                    28.0 + 37.0 * i as f32,
                    research_page,
                    None,
                );
            }
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
        let magic = self.game.magic(HUMAN).cloned().unwrap_or_default();
        let known = magic.known.contains(&spell);
        let current = magic.researching == Some(spell);
        let ink = Color::new(0.2, 0.12, 0.05, 1.0);
        let faint = Color::new(0.5, 0.4, 0.3, 1.0);
        let hot = Color::new(0.7, 0.1, 0.05, 1.0);
        let color = ink_override.unwrap_or(if current {
            hot
        } else if known || research_page {
            ink
        } else {
            faint
        });
        self.bm_text(view, 4, color, &def.name, x, y, Align::Left);
        let line = if research_page {
            let bonus = self.game.magic(HUMAN).map_or(0, |m| {
                m.research_bonus(self.game.content().rules.spells.get(spell))
            });
            let cost = if current {
                magic.research_remaining
            } else {
                def.research_cost as i32 * 100 / (100 + bonus)
            };
            let income = self.game.income(HUMAN).research.max(0);
            let turns = if income == 0 {
                999
            } else {
                (cost + income - 1) / income
            };
            format!(
                "Research Cost: {cost} ({turns} {})",
                if turns == 1 { "turn" } else { "turns" }
            )
        } else {
            let cost = self.game.casting_cost(HUMAN, spell, false);
            let limit = self.game.magic(HUMAN).map_or(0, |m| m.casting_skill()).min(
                self.game.player(HUMAN).map_or(0, |p| p.mana) + self.game.income(HUMAN).split.mana,
            );
            let turns = if limit <= 0 {
                999
            } else {
                (cost + limit - 1) / limit
            };
            format!(
                "Casting Cost: {cost} ({turns} {})",
                if turns == 1 { "turn" } else { "turns" }
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

impl App {
    /// "<wizard> has started casting the Spell of Mastery" over a black screen, as the original
    /// does (`Cast_Spell_Of_Mastery`; its swirling animation is local art not drawn here yet,
    /// task 287); any key or click goes on.
    pub(crate) fn mastery_notice_frame(&mut self) -> bool {
        let Some(&caster) = self.mastery_notice.front() else {
            return false;
        };
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        clear_background(Color::new(0.02, 0.0, 0.06, 1.0));
        let name = self
            .game
            .player(caster)
            .map_or("A rival wizard".to_string(), |p| p.name.clone());
        let ink = Color::new(0.95, 0.85, 0.6, 1.0);
        self.bm_text(
            view,
            5,
            ink,
            &format!("{name} has started casting the"),
            160.0,
            90.0,
            Align::Center,
        );
        self.bm_text(
            view,
            5,
            ink,
            "Spell of Mastery",
            160.0,
            110.0,
            Align::Center,
        );
        if is_mouse_button_pressed(MouseButton::Left)
            || is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::Space)
        {
            self.mastery_notice.pop_front();
        }
        true
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
    fn a_spells_description_is_the_help_text_after_its_heading() {
        let help = Help {
            topics: vec![HelpTopic {
                index: 29,
                title: "GAIA'S BLESSING".into(),
                text: "Effects: City Enchantment\nTarget: city\nCasting Cost: 300 mana\nUpkeep: 3 mana/turn\nA city's food grows."
                    .into(),
                picture: None,
                include: None,
            }],
            screens: Vec::new(),
        };
        assert_eq!(
            describe_spell(&help, "Gaia's Blessing"),
            "A city's food grows."
        );
        assert_eq!(describe_spell(&help, "Nothing"), "");
    }
}
