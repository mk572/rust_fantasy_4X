//! The advisors (task 71): the original's `Advisor_Screen` in `AdvsrScr.c`. The Info button
//! (and F1 to F9) opens a list box, "Select An Advisor", that leads to the Surveyor,
//! Cartographer, Apprentice, History, Astrologer, Chancellor, Tax Collector, Grand Vizier and
//! Mirror. The list box is the original's `Selection_Box` (`GENDRAW.c`), also used by the Tax
//! Collector. History and Astrologer are screens of their own here.

use f4x_core::game::Command;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::view::{App, Screen, HUMAN};

/// What a chosen row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// The advisor list; the row is the advisor.
    Advisors,
    /// The tax rates; the row is the rate.
    Tax,
}

/// A row: its text, and a second part drawn `tab` pixels further on (the original's `\x1D`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Row {
    pub left: String,
    pub tab: f32,
    pub right: String,
}

/// A list box over the map: a title and rows, one picked by a click.
#[derive(Debug, Clone)]
pub(crate) struct SelectionBox {
    pub title: String,
    pub rows: Vec<Row>,
    pub purpose: Purpose,
}

/// The pieces of the box in `resource.lbx`: left, top, right, bottom, bottom left, bottom
/// right, then the row buttons (frame 0 plain, 1 lit) and their 2-pixel right ends.
const LEFT: usize = 5;
const BOTTOM_LEFT: usize = 6;
const TOP: usize = 7;
const RIGHT: usize = 8;
const BOTTOM: usize = 9;
const BOTTOM_RIGHT: usize = 10;
const BUTTONS: usize = 12;
const BUTTON_ENDS: usize = 22;
const ROW_HEIGHT: f32 = 14.0;
const TOP_HEIGHT: f32 = 18.0;
const BOTTOM_HEIGHT: f32 = 14.0;
const SIDE_WIDTH: f32 = 16.0;
const RIGHT_WIDTH: f32 = 18.0;

/// The advisors in the original's order, with the F key of each.
const ADVISORS: [&str; 9] = [
    "advisor.surveyor",
    "advisor.cartographer",
    "advisor.apprentice",
    "advisor.history",
    "advisor.astrologer",
    "advisor.chancellor",
    "advisor.tax_collector",
    "advisor.grand_vizier",
    "advisor.mirror",
];

impl App {
    /// The Info button: the list of advisors.
    pub(crate) fn open_advisor_menu(&mut self) {
        let vizier = self.game.has_grand_vizier(HUMAN);
        let rows = ADVISORS
            .iter()
            .enumerate()
            .map(|(i, key)| {
                let name = self.text(key);
                Row {
                    left: if i == 7 && vizier {
                        self.text_with("advisor.starred", &Args::new().text("name", name))
                    } else {
                        name
                    },
                    tab: 81.0,
                    right: self.text_with("advisor.key", &Args::new().count("n", i + 1)),
                }
            })
            .collect();
        self.list_box = Some(SelectionBox {
            title: self.text("advisor.select"),
            rows,
            purpose: Purpose::Advisors,
        });
    }

    /// One advisor, by its place in the list (and its F key, less one).
    pub(crate) fn open_advisor(&mut self, which: usize) {
        match which {
            2 => self.open_apprentice(),
            0 => self.start_surveyor(),
            1 => {
                self.cart_plane = self.plane;
                self.screen = Screen::Cartographer;
            }
            3 => self.screen = Screen::History,
            4 => self.screen = Screen::Astrologer,
            5 => self.open_chancellor_advisor(),
            6 => self.open_tax_collector(),
            7 => self.toggle_grand_vizier(),
            8 => self.open_wizard(HUMAN),
            _ => {
                let args = Args::new().text("advisor", self.text(ADVISORS[which]));
                self.log.push(self.text_with("advisor.not_yet", &args));
            }
        }
    }

    /// The Tax Collector: each rate's gold per citizen and unrest, the current one starred.
    fn open_tax_collector(&mut self) {
        let unrest = self.game.content().rules.config.tax_unrest_percent.clone();
        let current = self
            .game
            .player_component::<f4x_core::player::Tax>(HUMAN)
            .map_or(0, |t| usize::from(t.0));
        let rows = unrest
            .iter()
            .enumerate()
            .map(|(rate, percent)| {
                let gold = match rate {
                    0 => "0".to_string(),
                    r if r % 2 == 1 => format!("{}.5", r / 2).trim_start_matches('0').to_string(),
                    r => (r / 2).to_string(),
                };
                Row {
                    left: self.text_with("advisor.tax_gold", &Args::new().text("gold", gold)),
                    tab: if rate == 0 {
                        59.0
                    } else if rate == 1 {
                        54.0
                    } else {
                        52.0
                    },
                    right: self.text_with(
                        if rate == current {
                            "advisor.tax_unrest_current"
                        } else {
                            "advisor.tax_unrest"
                        },
                        &Args::new().count("n", *percent),
                    ),
                }
            })
            .collect();
        self.list_box = Some(SelectionBox {
            title: self.text("advisor.tax_title"),
            rows,
            purpose: Purpose::Tax,
        });
    }

    /// The Grand Vizier: switched off at once; switching on asks first.
    fn toggle_grand_vizier(&mut self) {
        let on = self.game.has_grand_vizier(HUMAN);
        if on {
            self.game.apply(Command::SetGrandVizier {
                player: HUMAN,
                on: false,
            });
            self.log.push(self.text("advisor.vizier_off"));
        } else {
            self.vizier_ask = true;
        }
    }

    /// The question before the Grand Vizier takes over the cities' building.
    pub(crate) fn vizier_ask_frame(&mut self) -> bool {
        if !self.vizier_ask {
            return false;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(
            w / 2.0 - 330.0,
            h / 2.0 - 60.0,
            660.0,
            120.0,
            Color::new(0.08, 0.06, 0.04, 0.95),
        );
        draw_rectangle_lines(
            w / 2.0 - 330.0,
            h / 2.0 - 60.0,
            660.0,
            120.0,
            3.0,
            crate::ui::GOLD,
        );
        crate::frame::text_centered(
            &self.text("advisor.vizier_question"),
            w / 2.0,
            h / 2.0 - 15.0,
            20.0,
            WHITE,
        );
        crate::frame::text_centered(
            &self.text("advisor.vizier_keys"),
            w / 2.0,
            h / 2.0 + 30.0,
            22.0,
            crate::ui::GOLD,
        );
        if is_key_pressed(KeyCode::Y) || is_key_pressed(KeyCode::Enter) {
            self.vizier_ask = false;
            self.game.apply(Command::SetGrandVizier {
                player: HUMAN,
                on: true,
            });
            self.log.push(self.text("advisor.vizier_on"));
        } else if is_key_pressed(KeyCode::N) || is_key_pressed(KeyCode::Escape) {
            self.vizier_ask = false;
        }
        true
    }

    /// The list box over the map; true while it is up.
    pub(crate) fn selection_frame(&mut self) -> bool {
        let Some(sel) = self.list_box.clone() else {
            return false;
        };
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();

        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        // The box is as wide as its widest row or title, and sits in the middle of the screen.
        let width_of = |app: &Self, r: &Row| {
            app.art.text_width(4, &r.left)
                + r.tab.max(0.0)
                + if r.right.is_empty() {
                    0.0
                } else {
                    app.art.text_width(4, &r.right)
                }
                - if r.tab > 0.0 {
                    app.art.text_width(4, &r.left).min(r.tab)
                } else {
                    0.0
                }
        };
        let widest = sel
            .rows
            .iter()
            .map(|r| width_of(self, r))
            .fold(self.art.text_width(4, &sel.title), f32::max);
        let width = SIDE_WIDTH + widest + 8.0 + RIGHT_WIDTH;
        let height = TOP_HEIGHT + ROW_HEIGHT * sel.rows.len() as f32 + BOTTOM_HEIGHT;
        let (x1, y1) = ((320.0 - width) / 2.0, ((200.0 - height) / 2.0).floor());
        let (x2, y2) = (x1 + width - 1.0, y1 + height - 1.0);

        let mouse: Vec2 = mouse_position().into();
        let hover = (0..sel.rows.len()).find(|&i| {
            lay.rect((
                x1 + SIDE_WIDTH,
                y1 + TOP_HEIGHT + ROW_HEIGHT * i as f32,
                width - SIDE_WIDTH - RIGHT_WIDTH,
                ROW_HEIGHT,
            ))
            .contains(mouse)
        });

        self.clipped((x1, y1, width, height), |app| {
            app.draw_pic(&lay, "resource", LEFT, 0, x1, y1);
            app.draw_pic(&lay, "resource", TOP, 0, x1 + SIDE_WIDTH, y1);
            for i in 0..sel.rows.len() {
                let lit = usize::from(hover == Some(i));
                let y = y1 + TOP_HEIGHT + ROW_HEIGHT * i as f32;
                let which = if i == sel.rows.len() - 1 { 4 } else { i % 5 };
                app.draw_pic(&lay, "resource", BUTTONS + which, lit, x1 + SIDE_WIDTH, y);
                app.draw_pic(
                    &lay,
                    "resource",
                    BUTTON_ENDS + which,
                    lit,
                    x2 - RIGHT_WIDTH + 1.0,
                    y,
                );
            }
            app.draw_pic(
                &lay,
                "resource",
                BOTTOM,
                0,
                x1 + SIDE_WIDTH,
                y2 - BOTTOM_HEIGHT + 1.0,
            );
            app.draw_pic(&lay, "resource", BOTTOM_LEFT, 0, x1, y2 - 28.0);
            app.draw_pic(&lay, "resource", RIGHT, 0, x2 - RIGHT_WIDTH + 1.0, y1);
            app.draw_pic(
                &lay,
                "resource",
                BOTTOM_RIGHT,
                0,
                x2 - RIGHT_WIDTH + 1.0,
                y2 - 28.0,
            );
        });

        let mid = (x1 + x2) / 2.0;
        let shadow = Color::new(0.0, 0.0, 0.0, 1.0);
        self.bm_text(
            view,
            4,
            shadow,
            &sel.title,
            mid + 1.0,
            y1 + 5.0,
            Align::Center,
        );
        self.bm_text(
            view,
            4,
            Color::new(0.95, 0.85, 0.5, 1.0),
            &sel.title,
            mid,
            y1 + 4.0,
            Align::Center,
        );
        for (i, row) in sel.rows.iter().enumerate() {
            let y = y1 + TOP_HEIGHT + ROW_HEIGHT * i as f32 + 2.0;
            let colour = if hover == Some(i) {
                Color::new(1.0, 0.95, 0.75, 1.0)
            } else {
                Color::new(0.8, 0.62, 0.3, 1.0)
            };
            let x = x1 + SIDE_WIDTH + 4.0;
            self.bm_text(view, 4, colour, &row.left, x, y, Align::Left);
            if !row.right.is_empty() {
                self.bm_text(view, 4, colour, &row.right, x + row.tab, y, Align::Left);
            }
        }

        if is_key_pressed(KeyCode::Escape) || is_mouse_button_pressed(MouseButton::Right) {
            self.list_box = None;
        } else if is_mouse_button_pressed(MouseButton::Left) {
            if let Some(i) = hover {
                self.list_box = None;
                self.choose_row(sel.purpose, i);
            } else if !lay.rect((x1, y1, width, height)).contains(mouse) {
                self.list_box = None;
            }
        }
        true
    }

    fn choose_row(&mut self, purpose: Purpose, row: usize) {
        match purpose {
            Purpose::Advisors => self.open_advisor(row),
            Purpose::Tax => {
                self.game.apply(Command::SetTaxRate {
                    player: HUMAN,
                    rate: row as u8,
                });
            }
        }
    }
}
