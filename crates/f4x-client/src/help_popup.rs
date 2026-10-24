//! Help pop-ups: a right-click on a button, icon or figure opens the help for it, as
//! in the original. Each screen has a list of areas in the original's 320 by 200 pixels
//! (`content/help_screens.ron`, from `HLPENTRY.LBX`) that name the topic they open; the topic's
//! text is the game's own (`content/help.ron`), or the local copy of the original's where ours is
//! not written yet. A topic with no text opens nothing. A click or a key closes the pop-up.
//!
//! The screen under it is frozen: the frame it opened on is kept as a picture, so a click that
//! closes the pop-up cannot reach a button below. With Expanding Help on, the box grows out of the
//! area that was asked about, with lines from the area's corners to the box's.

use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::preferences::Pref;
use crate::view::{App, Screen};

/// The box's text width and the margin round it, in the original's pixels.
const TEXT_WIDTH: f32 = 180.0;

/// A text of more lines than this is set in a wider box.
const LONG_TEXT: usize = 14;
const WIDE_TEXT_WIDTH: f32 = 280.0;

/// The tallest box, in the original's pixels.
const MAX_HEIGHT: f32 = 192.0;
const MARGIN: f32 = 10.0;

/// Seconds the box takes to grow out of the area.
const GROW_SECONDS: f64 = 0.25;

/// What was asked, to be turned into a pop-up once the frame is on the screen.
#[derive(Debug, Clone)]
pub(crate) struct HelpRequest {
    topic: u16,
    from: (f32, f32, f32, f32),
}

/// The pop-up on show.
pub(crate) struct HelpPopup {
    title: String,
    lines: Vec<String>,
    /// The text's width in the original's pixels.
    width: f32,
    /// The first line shown, for a text taller than the screen.
    scroll: usize,
    /// The area asked about, as left, top, width, height.
    from: (f32, f32, f32, f32),
    opened: f64,
    /// The frame that was on the screen when it opened.
    frozen: Texture2D,
}

/// Both are empty without help.
#[derive(Default)]
pub(crate) struct HelpUi {
    pub request: Option<HelpRequest>,
    pub popup: Option<HelpPopup>,
    /// For the screenshot check (`--help X,Y`): ask about this point at once, and skip the
    /// growing.
    pub probe: Option<(i32, i32)>,
}

impl App {
    /// The help screen of `HLPENTRY.LBX` that describes the screen on show, if there is one.
    fn help_screen_id(&self) -> Option<u16> {
        Some(match self.screen {
            Screen::Spellbook => 0,
            Screen::Map => 1,
            Screen::ArmyList => 2,
            Screen::Items => 3,
            Screen::CityList => 4,
            Screen::Magic => 5,
            Screen::City(_) => 6,
            Screen::Wizard => 7,
            Screen::Cartographer => 8,
            Screen::History => 9,
            Screen::Astrologer => 10,
            Screen::EnemyCity(_) => 14,
            Screen::Outpost(_) => 17,
            Screen::UnitSheet => 13,
            Screen::MainMenu => 25,
            Screen::Slots => 26,
            Screen::Options => 29,
            Screen::Settings => 34,
            _ => return None,
        })
    }

    /// Look for a right-click on a help area. Called once a frame, before the screen reacts.
    pub(crate) fn watch_for_help(&mut self) {
        let probe = self.help_ui.probe;
        if !(is_mouse_button_pressed(MouseButton::Right) || probe.is_some())
            || self.help.topics.is_empty()
        {
            return;
        }
        let Some(id) = self.help_screen_id() else {
            return;
        };
        let lay = Layout::new();
        let (mx, my) = mouse_position();
        let (x, y) = probe.unwrap_or((
            ((mx - lay.origin.x) / lay.scale).floor() as i32,
            ((my - lay.origin.y) / lay.scale).floor() as i32,
        ));
        // On the main screen Next Turn replaces Done, Patrol, Wait and Build once every unit has
        // moved, and is not there before.
        let next_turn_shown = matches!(self.screen, Screen::Map) && self.all_units_moved();
        let allowed = |topic: i32| {
            if !matches!(self.screen, Screen::Map) {
                true
            } else if next_turn_shown {
                !(223..=228).contains(&topic)
            } else {
                topic != 229
            }
        };
        if let Some((topic, rect)) = self.help.at_where(id, x, y, allowed) {
            self.help_ui.request = Some(HelpRequest {
                topic: topic.index,
                from: (
                    rect.0 as f32,
                    rect.1 as f32,
                    (rect.2 - rect.0 + 1) as f32,
                    (rect.3 - rect.1 + 1) as f32,
                ),
            });
        }
    }

    /// After the frame is drawn: keep it as a picture and open the pop-up that was asked for.
    pub(crate) fn open_help_after_frame(&mut self) {
        let Some(request) = self.help_ui.request.take() else {
            return;
        };
        let Some(topic) = self.help.topic(request.topic) else {
            return;
        };
        let mut parts = vec![topic.clone()];
        if let Some(more) = topic.include.and_then(|i| self.help.topic(i)) {
            parts.push(more.clone());
        }
        let wrap = |app: &Self, width: f32| {
            let mut lines = Vec::new();
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    lines.push(String::new());
                    lines.push(format!("\u{1}{}", part.title));
                }
                let text = crate::hotkeys::fill_keys(&part.text, &app.hotkeys);
                for paragraph in text.lines() {
                    if paragraph.trim().is_empty() {
                        lines.push(String::new());
                    } else {
                        lines.extend(app.wrap_text(1, paragraph, width));
                    }
                }
            }
            lines
        };
        // A long text gets a wider box so that it fits the screen.
        let mut width = TEXT_WIDTH;
        let mut lines = wrap(self, width);
        if lines.len() > LONG_TEXT {
            width = WIDE_TEXT_WIDTH;
            lines = wrap(self, width);
        }
        let frozen = Texture2D::from_image(&get_screen_data());
        frozen.set_filter(FilterMode::Nearest);
        self.help_ui.popup = Some(HelpPopup {
            title: parts[0].title.clone(),
            lines,
            width,
            scroll: 0,
            from: request.from,
            opened: get_time(),
            frozen,
        });
    }

    /// The pop-up over the frozen frame. Returns whether it took the frame.
    pub(crate) fn help_popup_frame(&mut self) -> bool {
        let Some(popup) = self.help_ui.popup.as_ref() else {
            return false;
        };
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        clear_background(BLACK);
        draw_texture_ex(
            &popup.frozen,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(screen_width(), screen_height())),
                flip_y: true,
                ..Default::default()
            },
        );
        // Where the box ends up: round the area, kept on the screen.
        let line = self.art.text_height(1) + 1.0;
        let title = self.art.text_height(4) + 5.0;
        // The box is at most as tall as the screen; a longer text scrolls.
        let room = ((MAX_HEIGHT - MARGIN * 2.0 - title) / line)
            .floor()
            .max(1.0) as usize;
        let shown = popup.lines.len().min(room);
        let top_line = popup.scroll.min(popup.lines.len() - shown);
        let height = MARGIN * 2.0 + title + line * shown as f32;
        let width = popup.width + 2.0 * MARGIN;
        let (fx, fy, fw, fh) = popup.from;
        let left = (fx + fw / 2.0 - width / 2.0).clamp(4.0, (316.0 - width).max(4.0));
        let top = (fy + fh / 2.0 - height / 2.0).clamp(4.0, (196.0 - height).max(4.0));

        let grown = if self.prefs.get(Pref::ExpandingHelp) && self.help_ui.probe.is_none() {
            (((get_time() - popup.opened) / GROW_SECONDS) as f32).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let ease = 1.0 - (1.0 - grown) * (1.0 - grown);
        let mix = |a: f32, b: f32| a + (b - a) * ease;
        let (x, y, w, h) = (mix(fx, left), mix(fy, top), mix(fw, width), mix(fh, height));
        let r = lay.rect((x, y, w, h));
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.86, 0.76, 0.55, 1.0));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, Color::new(0.35, 0.22, 0.1, 1.0));
        if grown < 1.0 {
            // The growth lines: from each corner of the area to the same corner of the box.
            let from = lay.rect((fx, fy, fw, fh));
            let rim = Color::new(0.95, 0.85, 0.45, 1.0);
            for (a, b) in [
                (vec2(from.x, from.y), vec2(r.x, r.y)),
                (vec2(from.right(), from.y), vec2(r.right(), r.y)),
                (vec2(from.x, from.bottom()), vec2(r.x, r.bottom())),
                (
                    vec2(from.right(), from.bottom()),
                    vec2(r.right(), r.bottom()),
                ),
            ] {
                draw_line(a.x, a.y, b.x, b.y, 1.0, rim);
            }
        }
        if grown >= 1.0 {
            let ink = Color::new(0.2, 0.1, 0.04, 1.0);
            let red = Color::new(0.4, 0.05, 0.02, 1.0);
            let mut y = top + MARGIN;
            let heading = popup.title.clone();
            let body: Vec<String> = popup.lines[top_line..top_line + shown].to_vec();
            let more = top_line + shown < popup.lines.len();
            // Small arrows at the corner show that the text goes on above or below.
            let mark = Color::new(0.4, 0.05, 0.02, 1.0);
            let (cx, s) = (left + width - 9.0, 3.0);
            if more {
                let (a, b, c) = (
                    lay.point(cx - s, top + height - 8.0),
                    lay.point(cx + s, top + height - 8.0),
                    lay.point(cx, top + height - 4.0),
                );
                draw_triangle(a, b, c, mark);
            }
            if top_line > 0 {
                let (a, b, c) = (
                    lay.point(cx - s, top + 8.0),
                    lay.point(cx + s, top + 8.0),
                    lay.point(cx, top + 4.0),
                );
                draw_triangle(a, b, c, mark);
            }
            self.bm_text(view, 4, red, &heading, left + MARGIN, y, Align::Left);
            y += title;
            for text in body {
                if let Some(heading) = text.strip_prefix('\u{1}') {
                    self.bm_text(view, 4, red, heading, left + MARGIN, y, Align::Left);
                    y += title;
                } else {
                    self.bm_text(view, 1, ink, &text, left + MARGIN, y, Align::Left);
                    y += line;
                }
            }
        }
        // The wheel and the arrow keys scroll a long text; any other key or a click closes it.
        let wheel = mouse_wheel().1;
        let scrolled =
            is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::PageDown) || wheel < 0.0;
        let back = is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::PageUp) || wheel > 0.0;
        let paged = is_key_pressed(KeyCode::PageDown) || is_key_pressed(KeyCode::PageUp);
        let leave = (get_last_key_pressed().is_some() && !(scrolled || back))
            || is_mouse_button_pressed(MouseButton::Left)
            || is_mouse_button_pressed(MouseButton::Right);
        if let Some(popup) = self.help_ui.popup.as_mut() {
            let step = if paged { room } else { 3 };
            if scrolled {
                popup.scroll = (popup.scroll + step).min(popup.lines.len().saturating_sub(shown));
            } else if back {
                popup.scroll = popup.scroll.saturating_sub(step);
            }
        }
        if leave {
            self.help_ui.popup = None;
        }
        true
    }
}
