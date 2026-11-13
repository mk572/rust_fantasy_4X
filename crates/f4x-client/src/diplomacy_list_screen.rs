//! The Diplomacy screen (task 489): everyone the human can talk to in one list, a row for each
//! rival wizard and each neutral race met. A row says how the contact feels about you (the
//! relation as a word, coloured red, yellow or green) and the treaty between you (war, no
//! treaty or peace, pact, alliance). The panel at the bottom of the screen shows the contact
//! under the mouse: its picture and the treaties it holds with the other contacts. A left-click
//! on a row opens the talk (`diplomacy_screen.rs`), a right-click on a wizard its sheet.
//!
//! The list is made for up to nine rival wizards and a dozen neutral races: wizards first, a
//! heading, then the neutral races, nine rows to a page on the city list's picture (`reload.lbx`),
//! scrolled like it. Only contacts met are listed, so the screen shows nothing that the map did
//! not already tell; a last line counts the wizards not met yet. A wizard that is gone is listed
//! last, greyed and not to be talked to.
//!
//! Tests: the rows and the colours, in this file.

use crate::font::{rgb, Ink, Style};
use crate::input::Input;
use crate::page::Page;
use crate::panel_button_art::Panel;
use crate::text::Args;
use crate::ui;
use f4x_core::diplomacy::{relation_key, relation_level, Treaty};
use f4x_core::player::PlayerId;
use macroquad::prelude::*;

use crate::city_list_screen::scrolled;
use crate::frame::Layout;
use crate::view::{App, Screen, HUMAN};

/// Rows on the screen (the city list's).
const ROWS: usize = 9;
/// Where the columns start: the banner square, the name, the attitude, the treaty; the centres of
/// the three ranks (army, magic, research, as the Astrologer's three bars).
const BANNER_X: f32 = 31.0;
const NAME_X: f32 = 41.0;
const ATTITUDE_X: f32 = 104.0;
const TREATY_X: f32 = 150.0;
const RANK_X: [f32; 3] = [226.0, 255.0, 284.0];
/// The picture of the contact under the mouse, and the treaties it holds with the others.
const GEM_AT: (f32, f32) = (46.0, 157.0);
const TREATIES_PER_COLUMN: usize = 4;

/// The text colours of the attitude and of the treaty.
const RED: Color = rgb(0xFF, 0x70, 0x70);
const YELLOW: Color = rgb(0xFF, 0xE0, 0x60);
const GREEN: Color = rgb(0x70, 0xF0, 0x70);
const GREY: Color = rgb(0x98, 0x98, 0xA8);

/// What the list remembers while it is open.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DiplomacyListUi {
    /// The first row shown.
    pub first: usize,
    /// The row under the mouse, kept when the mouse is elsewhere.
    pub scanned: usize,
}

/// One contact: a rival wizard or the neutral player of a race.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Contact {
    pub id: PlayerId,
    /// The name in the list: a neutral race is just its people ("Barbarians").
    pub name: String,
    /// The name in the talk: "Neutral Barbarians".
    pub full_name: String,
    /// Place among the wizards known (the player's included) by army, magic and research as the
    /// Astrologer screen counts them, 1 for the highest; `None` for a neutral race, which has
    /// no magic and no research.
    pub ranks: Option<[usize; 3]>,
    /// How the contact feels about you, -100 to 100.
    pub relation: i32,
    pub treaty: Treaty,
    /// No treaty, but the contact has promised not to attack for a while (a peace made).
    pub peace: bool,
    pub gone: bool,
    pub banner: u8,
}

/// One line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Row {
    Heading(String),
    Contact(Contact),
    Note(String),
}

/// The colour of the word for a relation level: red up to restless, yellow up to relaxed,
/// green above (the original's colours of the gems' scale, `docs/diplomacy.md`).
pub(crate) fn attitude_colour(level: usize) -> Color {
    match level {
        0..=3 => RED,
        4..=6 => YELLOW,
        _ => GREEN,
    }
}

/// The text key of the treaty between you and a contact, and its colour (`None`: the list's own).
fn treaty_words(c: &Contact) -> (&'static str, Option<Color>) {
    match c.treaty {
        Treaty::War => ("treaty.war", Some(RED)),
        Treaty::Pact => ("treaty.pact", Some(GREEN)),
        Treaty::Alliance => ("treaty.alliance", Some(GREEN)),
        Treaty::None if c.peace => ("diplomacy.list_peace", None),
        Treaty::None => ("treaty.none", None),
    }
}

fn flat(colour: Color) -> Style {
    Style::inked(1, Ink::Flat(colour))
        .shadow()
        .edge(rgb(0x10, 0x0C, 0x20))
}

impl App {
    pub(crate) fn open_diplomacy_list(&mut self) {
        self.open_page(Screen::DiplomacyList, DiplomacyListUi::default());
    }

    /// The contacts the human has met: rival wizards in the order of their numbers, then the
    /// neutral races; the wizards that are gone after the others of their kind.
    pub(crate) fn contacts(&self) -> Vec<Contact> {
        let ranks = self.wizard_ranks();
        let mut found: Vec<Contact> = self
            .game
            .player_ids()
            .into_iter()
            .filter(|&id| id != HUMAN && self.game.takes_part_in_diplomacy(id))
            .filter(|&id| self.game.have_met(HUMAN, id))
            .map(|id| {
                let relation = self.game.relation(HUMAN, id);
                let treaty = self.game.treaty(HUMAN, id);
                let peace = treaty == Treaty::None
                    && self
                        .game
                        .diplomacy
                        .stance(id, HUMAN)
                        .is_some_and(|s| s.peace_duration > 0);
                Contact {
                    id,
                    name: self.race_name(id).unwrap_or_else(|| self.diplo_name(id)),
                    full_name: self.diplo_name(id),
                    ranks: ranks.get(&id).copied(),
                    relation,
                    treaty,
                    peace,
                    gone: self.game.player(id).is_some_and(|p| p.is_defeated()),
                    banner: crate::map_draw::banner_of(&self.game, id),
                }
            })
            .collect();
        found.sort_by_key(|c| (self.game.race_of_player(c.id).is_some(), c.gone));
        found
    }

    /// A place as a word: "1st", "2nd".
    pub(crate) fn rank_text(&self, place: usize) -> String {
        match place {
            1 => self.text("diplomacy.rank_1"),
            2 => self.text("diplomacy.rank_2"),
            3 => self.text("diplomacy.rank_3"),
            n => self.text_with("diplomacy.rank_other", &Args::new().count("n", n)),
        }
    }

    /// The people of a neutral player's race, "Barbarians".
    fn race_name(&self, id: PlayerId) -> Option<String> {
        let race = self.game.race_of_player(id)?;
        let num = self.game.content().rules.races.get(race).num;
        Some(self.names.race_plural(num).to_string())
    }

    /// Where each wizard known stands by army strength, magic power and spell research, the
    /// scores of the Astrologer screen: 1 for the highest, a tie shares the better place.
    pub(crate) fn wizard_ranks(&self) -> std::collections::HashMap<PlayerId, [usize; 3]> {
        let table = self.game.astrology_table();
        let known = self.known_wizards();
        let scores: Vec<(PlayerId, [i32; 3])> = known
            .iter()
            .filter_map(|&id| {
                let (_, a, _) = table.iter().find(|(p, ..)| *p == id)?;
                Some((id, [a.strength, a.power, a.research]))
            })
            .collect();
        scores
            .iter()
            .map(|&(id, mine)| {
                let place = |k: usize| 1 + scores.iter().filter(|(_, s)| s[k] > mine[k]).count();
                (id, [place(0), place(1), place(2)])
            })
            .collect()
    }

    /// The list as rows: the wizards, the heading of the neutral races and them, and the line
    /// that counts the wizards not met.
    pub(crate) fn diplomacy_rows(&self) -> Vec<Row> {
        let contacts = self.contacts();
        let mut rows = Vec::new();
        let (neutral, wizards): (Vec<Contact>, Vec<Contact>) = contacts
            .into_iter()
            .partition(|c| self.game.race_of_player(c.id).is_some());
        rows.extend(wizards.into_iter().map(Row::Contact));
        if !neutral.is_empty() {
            rows.push(Row::Heading(self.text("diplomacy.list_neutral")));
            rows.extend(neutral.into_iter().map(Row::Contact));
        }
        let unmet = self
            .game
            .wizards()
            .into_iter()
            .filter(|&w| w != HUMAN && !self.game.have_met(HUMAN, w))
            .filter(|&w| !self.game.player(w).is_some_and(|p| p.is_defeated()))
            .count();
        if unmet > 0 {
            rows.push(Row::Note(self.text_with(
                "diplomacy.list_unmet",
                &Args::new().count("n", unmet),
            )));
        }
        rows
    }

    /// The contact's picture: a wizard's gem in its banner colour, a race's own picture.
    fn draw_contact_gem(&mut self, lay: &Layout, c: &Contact) {
        let (x, y) = GEM_AT;
        if let Some(race) = self.game.race_of_player(c.id) {
            self.draw_neutral(lay, race.num(), crate::neutral_art::Kind::Gem, x, y);
        } else {
            let portrait = self.game.player(c.id).map_or(0, |p| p.portrait());
            if let Some(entry) = crate::art::gem_entry(self.art.wizard(portrait).gem, c.banner) {
                self.draw_pic(lay, "lilwiz", entry, 0, x, y);
            }
        }
    }

    /// The treaties `c` holds with the other contacts of the human, as an icon and a name, in
    /// two columns of four (the city list's place for the enchantments).
    fn draw_contact_treaties(
        &mut self,
        lay: &Layout,
        view: (f32, f32, f32),
        c: &Contact,
        others: &[Contact],
    ) {
        let held: Vec<(Treaty, &Contact)> = others
            .iter()
            .filter(|o| o.id != c.id)
            .map(|o| (self.game.treaty(c.id, o.id), o))
            .filter(|(t, _)| *t != Treaty::None)
            .collect();
        let room = TREATIES_PER_COLUMN * 2;
        let shown = if held.len() > room {
            room - 1
        } else {
            held.len()
        };
        for (k, (treaty, o)) in held.iter().take(shown).enumerate() {
            let (x, y) = if k >= TREATIES_PER_COLUMN {
                (163.0, 168.0 + 7.0 * (k - TREATIES_PER_COLUMN) as f32)
            } else {
                (100.0, 168.0 + 7.0 * k as f32)
            };
            let n = match treaty {
                Treaty::None => continue,
                Treaty::Pact => 0,
                Treaty::Alliance => 1,
                Treaty::War => 2,
            };
            // The wizards' banners have the icons of the treaties; a neutral race has none.
            if o.banner < f4x_core::player::NEUTRAL_BANNER {
                let entry = 60 + n + usize::from(o.banner) * 3;
                self.draw_pic(lay, "magic", entry, 0, x, y - 1.0);
            } else {
                let b = lay.rect((x + 2.0, y, 6.0, 6.0));
                draw_rectangle(b.x, b.y, b.w, b.h, self.art.banner_color(o.banner));
            }
            self.draw_styled(view, &Style::city_list_text(0), &o.name, x + 12.0, y);
        }
        if held.len() > room {
            let more = self.text_with(
                "diplomacy.list_more",
                &Args::new().count("n", held.len() - shown),
            );
            let k = room - 1;
            self.draw_styled(
                view,
                &Style::city_list_text(0),
                &more,
                163.0,
                168.0 + 7.0 * (k - TREATIES_PER_COLUMN) as f32,
            );
        }
    }

    /// For screenshots (`--diplomacy`): every rival and every neutral race met, with relations
    /// from hate to harmony and a war, an alliance and a pact among them, and the list opened.
    pub fn open_test_diplomacy_list(&mut self) {
        let others: Vec<PlayerId> = self
            .game
            .player_ids()
            .into_iter()
            .filter(|&id| id != HUMAN && self.game.takes_part_in_diplomacy(id))
            .collect();
        for (i, &id) in others.iter().enumerate() {
            self.game.make_contact(HUMAN, id);
            let change = [-90, -50, -25, -5, 0, 15, 40, 70, 95][i % 9];
            let action = if change >= 0 {
                f4x_core::diplomacy::Action::Friendship
            } else {
                f4x_core::diplomacy::Action::AttackedCity
            };
            self.game.change_relations(change, HUMAN, id, action, None);
        }
        for (i, pair) in others.windows(2).enumerate() {
            let treaty = [Treaty::War, Treaty::Alliance, Treaty::Pact][i % 3];
            self.game.make_contact(pair[0], pair[1]);
            self.game.start_treaty(pair[0], pair[1], treaty);
        }
        if let Some(&first) = others.first() {
            self.game.start_treaty(HUMAN, first, Treaty::War);
        }
        if let Some(&second) = others.get(1) {
            self.game.start_treaty(HUMAN, second, Treaty::Alliance);
        }
        self.game.drain_diplomacy_events();
        self.open_diplomacy_list();
    }

    /// Open the talk with `with` from this list, to come back to it.
    fn open_talk_from_list(&mut self, with: PlayerId) {
        self.try_open_diplomacy_from(with, Screen::DiplomacyList);
    }
}

impl Page for DiplomacyListUi {
    fn frame(&mut self, app: &mut App, input: &Input) {
        let rows = app.diplomacy_rows();
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        if app.draw_pic(&lay, "reload", 21, 0, 0.0, 0.0).is_none() {
            let r = lay.rect(crate::resolution::SCREEN);
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.2, 0.2, 0.3, 1.0));
        }
        let count = rows.len();
        let mut ui = *self;
        ui.first = ui.first.min(count.saturating_sub(ROWS));

        let owner = app
            .game
            .wizard(HUMAN)
            .map_or_else(String::new, |w| w.name());
        let title = app.text_with("diplomacy.list_title", &Args::new().text("owner", owner));
        app.draw_title_stack(
            view,
            4,
            [
                crate::font_ramp::LIST_TITLE_FAR,
                crate::font_ramp::LIST_TITLE_MID,
                crate::font_ramp::LIST_TITLE_NEAR,
                crate::font_ramp::LIST_TITLE,
            ],
            &title,
            (160.0, 3.0),
        );
        for (head, x) in [
            ("diplomacy.list_name", NAME_X),
            ("diplomacy.list_attitude", ATTITUDE_X),
            ("diplomacy.list_treaty", TREATY_X),
        ] {
            let head = app.text(head);
            app.draw_styled(view, &Style::city_list_text(1), &head, x, 17.0);
        }
        for (head, x) in [
            "diplomacy.list_army",
            "diplomacy.list_magic",
            "diplomacy.list_research",
        ]
        .into_iter()
        .zip(RANK_X)
        {
            let head = app.text(head);
            app.draw_styled(view, &Style::city_list_text(1).center(), &head, x, 17.0);
        }

        // Scrolling: the arrows, U and D, the wheel (as the city list does).
        let mut scroll = 0i32;
        if count > ROWS {
            for x in [11.0, 299.0] {
                if app.army_button(&lay, 1, x, 26.0, ui.first > 0, input) {
                    scroll -= 1;
                }
                if app.army_button(&lay, 2, x, 139.0, ui.first + ROWS < count, input) {
                    scroll += 1;
                }
            }
        }
        if input.key_pressed(KeyCode::U) || input.key_pressed(KeyCode::Up) {
            scroll -= 1;
        }
        if input.key_pressed(KeyCode::D) || input.key_pressed(KeyCode::Down) {
            scroll += 1;
        }
        scroll -= input.mouse_wheel().1.signum() as i32;
        ui.first = scrolled(ui.first, scroll.signum(), count);

        // The rows.
        let contacts: Vec<Contact> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Contact(c) => Some(c.clone()),
                _ => None,
            })
            .collect();
        let shown = count.saturating_sub(ui.first).min(ROWS);
        ui.scanned = ui.scanned.min(shown.saturating_sub(1));
        let (mut left, mut right) = (None, None);
        for (i, row) in rows.iter().skip(ui.first).take(ROWS).enumerate() {
            let y = 26.0 + 14.0 * i as f32;
            let ty = y + 1.0;
            match row {
                Row::Heading(text) => {
                    app.draw_styled(view, &Style::city_list_text(1), text, BANNER_X, ty);
                }
                Row::Note(text) => {
                    app.draw_styled(view, &flat(GREY), text, BANNER_X, ty);
                }
                Row::Contact(c) => {
                    let cell = lay.rect((30.0, y, 260.0, 9.0));
                    let state = ui::hit(cell, !c.gone, input);
                    if state.hovered() || ui.scanned == i {
                        if state.hovered() {
                            ui.scanned = i;
                        }
                        if ui.scanned == i {
                            let h = lay.rect((30.0, y + 1.0, 259.0, 9.0));
                            draw_rectangle(h.x, h.y, h.w, h.h, Color::new(1.0, 1.0, 1.0, 0.25));
                        }
                    }
                    if state.hovered() {
                        if state.clicked() {
                            left = Some(c.id);
                        }
                        if input.mouse_pressed(MouseButton::Right) {
                            right = Some(c.id);
                        }
                    }
                    let b = lay.rect((BANNER_X, y + 2.0, 6.0, 6.0));
                    draw_rectangle(b.x, b.y, b.w, b.h, app.art.banner_color(c.banner));
                    if c.gone {
                        let gone = app.text("diplomacy.list_gone");
                        app.draw_styled(view, &flat(GREY), &c.name, NAME_X, ty);
                        app.draw_styled(view, &flat(GREY), &gone, ATTITUDE_X, ty);
                        continue;
                    }
                    app.draw_styled(view, &Style::city_list_text(1), &c.name, NAME_X, ty);
                    let word = app.text(relation_key(c.relation));
                    app.draw_styled(
                        view,
                        &flat(attitude_colour(relation_level(c.relation))),
                        &word,
                        ATTITUDE_X,
                        ty,
                    );
                    let (key, colour) = treaty_words(c);
                    let word = app.text(key);
                    let style = colour.map_or(Style::city_list_text(1), flat);
                    app.draw_styled(view, &style, &word, TREATY_X, ty);
                    for (k, x) in RANK_X.into_iter().enumerate() {
                        let text = c.ranks.map_or_else(
                            || app.text("diplomacy.rank_none"),
                            |r| app.rank_text(r[k]),
                        );
                        app.draw_styled(view, &Style::city_list_text(1).center(), &text, x, ty);
                    }
                }
            }
        }

        // The contact under the mouse: its picture, its name and the treaties it holds.
        let under = rows.get(ui.first + ui.scanned).and_then(|r| match r {
            Row::Contact(c) => Some(c.clone()),
            _ => None,
        });
        if let Some(c) = under {
            app.draw_contact_gem(&lay, &c);
            app.draw_styled(
                view,
                &Style::city_list_text(1),
                &c.full_name.to_uppercase(),
                99.0,
                158.0,
            );
            if !c.gone {
                app.draw_contact_treaties(&lay, view, &c, &contacts);
            }
        }

        // Your reserves, where the city list shows them.
        let (gold, mana) = app.game.player(HUMAN).map_or(
            (
                f4x_def::half_int::HalfInt::ZERO,
                f4x_def::half_int::HalfInt::ZERO,
            ),
            |p| (p.treasury().gold, p.treasury().mana),
        );
        for (key, amount, x) in [
            ("cities.reserve_gold", gold, 242.0),
            ("cities.reserve_mana", mana, 276.0),
        ] {
            let text = app.text_with(key, &Args::new().text("n", app.texts.half_amount(amount)));
            app.draw_styled(view, &Style::city_list_text(1).center(), &text, x, 172.0);
        }

        let leave = app.panel_button(&lay, Panel::Ok, "ui.button_ok_mixed", (239.0, 182.0), input)
            || input.key_pressed(KeyCode::Escape)
            || input.key_pressed(KeyCode::O);
        *self = ui;
        if leave {
            app.screen = Screen::Map;
        } else if let Some(id) = left {
            app.open_talk_from_list(id);
        } else if let Some(id) = right {
            if app.game.is_wizard(id) {
                app.open_wizard(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_attitude_word_is_red_yellow_or_green_by_level() {
        assert_eq!(attitude_colour(0), RED);
        assert_eq!(attitude_colour(3), RED);
        assert_eq!(attitude_colour(4), YELLOW);
        assert_eq!(attitude_colour(6), YELLOW);
        assert_eq!(attitude_colour(7), GREEN);
        assert_eq!(attitude_colour(10), GREEN);
    }

    #[test]
    fn only_the_contacts_met_are_listed_and_the_rest_are_counted() {
        let mut app = App::for_test();
        let rivals: Vec<PlayerId> = app
            .game
            .wizards()
            .into_iter()
            .filter(|&w| w != HUMAN)
            .collect();
        assert!(!rivals.is_empty(), "the test game has a rival");
        let rows = app.diplomacy_rows();
        assert!(
            !rows.iter().any(|r| matches!(r, Row::Contact(_))),
            "nobody is met at the start"
        );
        assert!(matches!(rows.last(), Some(Row::Note(_))), "{rows:?}");
        app.game.make_contact(HUMAN, rivals[0]);
        let rows = app.diplomacy_rows();
        let met: Vec<PlayerId> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Contact(c) => Some(c.id),
                _ => None,
            })
            .collect();
        assert_eq!(met, vec![rivals[0]]);
    }

    #[test]
    fn a_contact_row_shows_the_relation_and_the_treaty() {
        let mut app = App::for_test();
        let rival = app
            .game
            .wizards()
            .into_iter()
            .find(|&w| w != HUMAN)
            .expect("a rival");
        app.game.make_contact(HUMAN, rival);
        app.game.start_treaty(HUMAN, rival, Treaty::War);
        let rows = app.diplomacy_rows();
        let Some(Row::Contact(c)) = rows.first() else {
            panic!("no contact row in {rows:?}");
        };
        assert_eq!(c.treaty, Treaty::War);
        assert_eq!(c.relation, app.game.relation(HUMAN, rival));
        assert_eq!(treaty_words(c).0, "treaty.war");
        assert_eq!(c.name, app.diplo_name(rival));
    }

    #[test]
    fn a_neutral_race_met_comes_after_the_wizards_under_its_own_heading() {
        let mut app = App::for_test();
        let rival = app
            .game
            .wizards()
            .into_iter()
            .find(|&w| w != HUMAN)
            .expect("a rival");
        let neutral = f4x_core::player::race_player::race_player_id(f4x_def::rules::RaceId::new(0));
        app.game.add_neutral_player(neutral);
        app.game.make_contact(HUMAN, neutral);
        app.game.make_contact(HUMAN, rival);
        let rows = app.diplomacy_rows();
        let heading = rows
            .iter()
            .position(|r| matches!(r, Row::Heading(_)))
            .expect("a heading for the neutral races");
        assert!(matches!(&rows[0], Row::Contact(c) if c.id == rival));
        let Row::Contact(n) = &rows[heading + 1] else {
            panic!("no neutral row in {rows:?}");
        };
        assert_eq!(n.id, neutral);
        assert!(!n.name.starts_with("Neutral"), "{}", n.name);
        assert!(n.full_name.starts_with("Neutral"), "{}", n.full_name);
        assert_eq!(n.ranks, None, "a neutral race has no magic or research");
    }

    #[test]
    fn the_ranks_count_the_wizards_known_and_a_tie_shares_the_better_place() {
        let mut app = App::for_test();
        let rival = app
            .game
            .wizards()
            .into_iter()
            .find(|&w| w != HUMAN)
            .expect("a rival");
        assert!(
            !app.wizard_ranks().contains_key(&rival),
            "a wizard not met is not ranked"
        );
        app.game.make_contact(HUMAN, rival);
        let ranks = app.wizard_ranks();
        let (mine, theirs) = (ranks[&HUMAN], ranks[&rival]);
        for k in 0..3 {
            assert!((1..=2).contains(&mine[k]) && (1..=2).contains(&theirs[k]));
            assert!(
                mine[k] == 1 || theirs[k] == 1,
                "somebody is first in column {k}"
            );
        }
    }

    #[test]
    fn a_place_is_a_word_up_to_the_third_and_th_after() {
        let app = App::for_test();
        assert_eq!(app.rank_text(1), "1st");
        assert_eq!(app.rank_text(2), "2nd");
        assert_eq!(app.rank_text(3), "3rd");
        assert_eq!(app.rank_text(4), "4th");
        assert_eq!(app.rank_text(10), "10th");
    }
}
