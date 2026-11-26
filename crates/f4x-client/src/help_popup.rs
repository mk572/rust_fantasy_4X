//! Help pop-ups: a right-click on a button, icon or figure opens the help for it, as
//! in the original. Each screen has a list of areas in the original's 320 by 200 pixels
//! (`crates/f4x-client/content/help_screens.ron`, from `HLPENTRY.LBX`) that name the topic they open; the topic's
//! text is the game's own (`crates/f4x-lang/content/text/en/help/help.ron`; the original's is never read). A topic with no text opens nothing. A click or a key closes the pop-up.
//!
//! The screen under it is frozen: the frame it opened on is kept as a picture, so a click that
//! closes the pop-up cannot reach a button below. With Expanding Help on, the box grows out of the
//! area that was asked about, with lines from the area's corners to the box's.

use crate::font::{Ink, Style};
use crate::font_ramp;
use crate::input::Input;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::modal::{keep, kind, Modal};
use crate::preferences::Pref;
use crate::view::{App, Screen};
use f4x_core::world_event::EventKind;

/// The scroll the help is drawn on (`Draw_Help_Entry`): its left edge, the text column and the
/// title's indent from that edge, the height of its top and bottom roll, the height a title
/// takes (the font's 9 and 2), the room after an entry, and the lines under the last.
const SCROLL_LEFT: f32 = 50.0;
const TEXT_WIDTH: f32 = 180.0;
const TEXT_LEFT: f32 = 20.0;
const TITLE_LEFT: f32 = 18.0;
const ROLL: f32 = 24.0;
const TITLE_HEIGHT: f32 = 11.0;
const ENTRY_GAP: f32 = 6.0;
/// The room above the first title and below the last entry.
const SPACE_ABOVE: f32 = 2.0;
const SPACE_BELOW: f32 = 5.0;
/// The scroll's paper, from `help.lbx` entry 0, is this wide.
const PAPER_WIDTH: f32 = 217.0;
/// The tallest the text on the scroll may be before it scrolls.
const MAX_TEXT: f32 = 200.0 - 2.0 * ROLL - SPACE_ABOVE - TITLE_HEIGHT - ENTRY_GAP - SPACE_BELOW;

/// Seconds the box takes to grow out of the area.
const GROW_SECONDS: f64 = 0.25;

/// What was asked, to be turned into a pop-up once the frame is on the screen.
#[derive(Debug, Clone)]
pub(crate) struct HelpRequest {
    topic: u16,
    /// A second topic shown under the first (the weapon of the melee row).
    extra: Option<u16>,
    from: (f32, f32, f32, f32),
}

/// The pop-up on show.
pub(crate) struct HelpPopup {
    title: String,
    lines: Vec<String>,
    /// The first line shown, for a text taller than the screen.
    scroll: usize,
    /// The area asked about, as left, top, width, height.
    from: (f32, f32, f32, f32),
    opened: f64,
    /// The frame that was on the screen when it opened.
    frozen: Texture2D,
}

impl Modal for HelpPopup {
    fn kind(&self) -> &'static str {
        kind::HELP
    }
    fn covers_screen(&self) -> bool {
        true
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.help_popup_frame(*self, input))
    }
}

/// What is asked and not yet opened; empty without help.
#[derive(Default)]
pub(crate) struct HelpUi {
    pub request: Option<HelpRequest>,
    /// For the screenshot check (`--help X,Y`): ask about this point at once, and skip the
    /// growing.
    pub probe: Option<(i32, i32)>,
    /// The name and description of the building under the mouse on the city screen, set while
    /// that screen is drawn.
    pub building: Option<u16>,
    /// For the screenshot check (`--help-building ID`): open the description of this building
    /// on the city screen at once.
    pub probe_building: Option<String>,
}

/// The topic of the melee row on the unit sheet.
const MELEE_TOPIC: u16 = 383;

/// The help topic of the weapon a melee icon shows (`unitview` stat icon 0 sword, 3 magic,
/// 2 mithril, 4 adamantium): Sword, Magic Weapon, Mithril Weapon, Adamantium Weapon.
fn weapon_topic(icon: usize) -> Option<u16> {
    match icon {
        0 => Some(384),
        3 => Some(385),
        2 => Some(386),
        4 => Some(762),
        _ => None,
    }
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
            Screen::DiplomacyList => 10,
            Screen::EnemyCity => 14,
            Screen::Outpost => 17,
            Screen::UnitSheet => 13,
            Screen::MainMenu => 25,
            Screen::Slots => 26,
            Screen::Options => 29,
            Screen::Settings => 34,
            _ => return None,
        })
    }

    /// Ask for the pop-up of a spell's own topic, out of the area `from`. False without a topic.
    pub(crate) fn ask_spell_help(
        &mut self,
        spell: f4x_def::rules::SpellId,
        from: (f32, f32, f32, f32),
    ) -> bool {
        let help = self.game.content().rules.spells.get(spell).help;
        let Some(topic) = self.help.topic_of(help) else {
            return false;
        };
        self.help_ui.request = Some(HelpRequest {
            topic: topic.index,
            extra: None,
            from,
        });
        true
    }

    /// The help of a random event under the point (320x200 coordinates): the world event's name
    /// under the minimap, and the Plague or Pop. Boom mark after the race on the city screen.
    fn event_help_at(&self, x: f32, y: f32) -> Option<(u16, (f32, f32, f32, f32))> {
        let events = self.game.events();
        match self.screen {
            Screen::Map => {
                let kind = events.world_event()?;
                let from = (240.0, 152.0, 80.0, 12.0);
                let inside =
                    x >= from.0 && x < from.0 + from.2 && y >= from.1 && y < from.1 + from.3;
                inside.then(|| (event_topic(kind), from))
            }
            Screen::City(city) => {
                let serial = self.game.units().serial_of(city);
                let from = (0.0, 19.0, 211.0, 8.0);
                let inside =
                    x >= from.0 && x < from.0 + from.2 && y >= from.1 && y < from.1 + from.3;
                if !inside {
                    return None;
                }
                [EventKind::Plague, EventKind::PopulationBoom]
                    .into_iter()
                    .find(|k| events.on_city(*k, serial))
                    .map(|k| (event_topic(k), from))
            }
            _ => None,
        }
    }

    /// Look for a right-click on a help area. Called once a frame, before the screen reacts.
    pub(crate) fn watch_for_help(&mut self, input: &Input) {
        let probe = self.help_ui.probe;
        if matches!(self.screen, Screen::City(_)) {
            if let Some(name) = self.help_ui.probe_building.take() {
                let content = self.game.content();
                if let Some(topic) = crate::alias::id_of(&content.rules.buildings, &name)
                    .and_then(|b| content.rules.buildings.get(b).help)
                {
                    self.help_ui.request = Some(HelpRequest {
                        topic,
                        extra: None,
                        from: (158.0, 98.0, 4.0, 4.0),
                    });
                }
                return;
            }
        }
        if input.mouse_pressed(MouseButton::Right) && matches!(self.screen, Screen::City(_)) {
            if let Some(topic) = self.help_ui.building.take() {
                let lay = Layout::new();
                let (mx, my) = input.mouse_position();
                let (x, y) = (
                    (mx - lay.origin.x) / lay.scale,
                    (my - lay.origin.y) / lay.scale,
                );
                self.help_ui.request = Some(HelpRequest {
                    topic,
                    extra: None,
                    from: (x - 2.0, y - 2.0, 4.0, 4.0),
                });
                return;
            }
        }
        if input.mouse_pressed(MouseButton::Right) {
            let lay = Layout::new();
            let (mx, my) = input.mouse_position();
            let at = (
                (mx - lay.origin.x) / lay.scale,
                (my - lay.origin.y) / lay.scale,
            );
            let settings = (matches!(self.screen, Screen::Settings))
                .then(|| crate::settings_screen::row_at(at.0, at.1))
                .flatten()
                .map(|(pref, from)| (crate::settings_screen::help_topic(pref), from));
            if let Some((topic, from)) = settings.or_else(|| self.event_help_at(at.0, at.1)) {
                self.help_ui.request = Some(HelpRequest {
                    topic,
                    extra: None,
                    from,
                });
                return;
            }
        }
        if !(input.mouse_pressed(MouseButton::Right) || probe.is_some())
            || self.help.topics.is_empty()
        {
            return;
        }
        let Some(id) = self.help_screen_id() else {
            return;
        };
        let lay = Layout::new();
        let (mx, my) = input.mouse_position();
        let (x, y) = probe.unwrap_or((
            ((mx - lay.origin.x) / lay.scale).floor() as i32,
            ((my - lay.origin.y) / lay.scale).floor() as i32,
        ));
        // A spell in the spell book opens its own description.
        if matches!(self.screen, Screen::Spellbook) {
            let hit = self
                .pages
                .get(&self.screen)
                .and_then(|page| page.spell_at(x as f32, y as f32));
            if let Some((spell, from)) = hit {
                if self.ask_spell_help(spell, from) {
                    return;
                }
            }
        }
        if matches!(self.screen, Screen::UnitSheet) {
            let hit = self
                .pages
                .get(&self.screen)
                .and_then(|page| page.help_at(self, x, y));
            if let Some((topic, from)) = hit {
                self.help_ui.request = Some(HelpRequest {
                    topic,
                    extra: None,
                    from,
                });
                return;
            }
        }
        // On the main screen Next Turn replaces Done, Patrol, Wait and Build once every unit has
        // moved, and is not there before.
        let next_turn_shown = matches!(self.screen, Screen::Map) && self.all_units_moved();
        // The income, food and mana panels (233 to 235) are the summary column's, drawn while no
        // stack is selected; the moves line (236) belongs to the selected stack's window.
        let selected = !self.selection.is_empty();
        let allowed = |topic: i32| {
            if !matches!(self.screen, Screen::Map) {
                true
            } else if selected && (233..=235).contains(&topic) || !selected && topic == 236 {
                false
            } else if next_turn_shown {
                !(223..=228).contains(&topic)
            } else {
                topic != 229
            }
        };
        if let Some((topic, rect)) = self.help.at_where(id, x, y, allowed) {
            let extra = (topic.index == MELEE_TOPIC && matches!(self.screen, Screen::UnitSheet))
                .then(|| self.pages.get(&self.screen)?.melee_icon())
                .flatten()
                .and_then(weapon_topic);
            self.help_ui.request = Some(HelpRequest {
                topic: topic.index,
                extra,
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
        let mut parts = Vec::new();
        let Some(topic) = self.help.topic(request.topic) else {
            return;
        };
        parts.push(topic.clone());
        if let Some(more) = topic.include.and_then(|i| self.help.topic(i)) {
            parts.push(more.clone());
        }
        if let Some(weapon) = request.extra.and_then(|i| self.help.topic(i)) {
            parts.push(weapon.clone());
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
                        lines.extend(app.bm_wrap(1, paragraph, width));
                    }
                }
            }
            lines
        };
        let lines = wrap(self, TEXT_WIDTH);
        let frozen = Texture2D::from_image(&get_screen_data());
        frozen.set_filter(FilterMode::Nearest);
        self.modals.open(HelpPopup {
            title: parts[0].title.clone(),
            lines,
            scroll: 0,
            from: request.from,
            opened: get_time(),
            frozen,
        });
    }

    /// The pop-up over the frozen frame; what is left of it until it is closed.
    pub(crate) fn help_popup_frame(
        &mut self,
        mut popup: HelpPopup,
        input: &Input,
    ) -> Option<HelpPopup> {
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
        // The scroll's height follows the text, as the original grows it (`Draw_Help_Entry`):
        // a title and its text for each entry, between the rolls, centred on the screen.
        let line = self.art.text_height(1) + 1.0;
        let rows = row_heights(&popup.lines, line);
        // A text taller than the scroll scrolls: the lines from the first one shown that fit.
        let top_line = popup.scroll.min(popup.lines.len().saturating_sub(1));
        let shown = lines_that_fit(&rows, top_line, MAX_TEXT);
        let shown = shown.max(1);
        let room = ((MAX_TEXT / line).floor() as usize).max(1);
        let body_height: f32 = rows[top_line..(top_line + shown).min(rows.len())]
            .iter()
            .sum();
        let text_height = SPACE_ABOVE + TITLE_HEIGHT + ENTRY_GAP + body_height + SPACE_BELOW;
        let scroll_top = (100.0 - (text_height + 2.0 * ROLL) / 2.0).floor().max(0.0);
        let (left, width) = (SCROLL_LEFT, PAPER_WIDTH);
        let (top, height) = (scroll_top, text_height + 2.0 * ROLL);
        let (fx, fy, fw, fh) = popup.from;

        let grown = if self.prefs.get(Pref::ExpandingHelp) && self.help_ui.probe.is_none() {
            (((get_time() - popup.opened) / GROW_SECONDS) as f32).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let ease = 1.0 - (1.0 - grown) * (1.0 - grown);
        let mix = |a: f32, b: f32| a + (b - a) * ease;
        let (x, y, w, h) = (mix(fx, left), mix(fy, top), mix(fw, width), mix(fh, height));
        // The scroll: the paper cut to the text's height, its bottom roll under it. While it
        // grows out of the area only that much of the paper shows. Without the picture a flat
        // rectangle stands in.
        let paper_height = ROLL + text_height;
        let mut drawn = false;
        let clip_h = (top + paper_height - y).min(h).max(0.0);
        self.clipped((x, y, w, clip_h), |app| {
            drawn = app.draw_pic(&lay, "help", 0, 0, left, top).is_some();
        });
        // The paper is cut where the text ends; the picture is taller than any text.
        if drawn {
            if grown >= 1.0 {
                self.draw_pic(&lay, "help", 1, 0, left, top + paper_height);
            }
        } else {
            let r = lay.rect((x, y, w, h));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.86, 0.76, 0.55, 1.0));
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, Color::new(0.35, 0.22, 0.1, 1.0));
        }
        if grown < 1.0 {
            // The growth lines: from each corner of the area to the same corner of the box.
            let from = lay.rect((fx, fy, fw, fh));
            let r = lay.rect((x, y, w, h));
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
            let heading = popup.title.clone();
            let body: Vec<String> = popup.lines[top_line..top_line + shown].to_vec();
            let more = top_line + shown < popup.lines.len();
            // Small arrows at the right edge show that the text goes on above or below.
            let mark = Color::new(0.4, 0.05, 0.02, 1.0);
            let (cx, s) = (left + width - 30.0, 3.0);
            if more {
                let by = top + paper_height - 6.0;
                let (a, b, c) = (
                    lay.point(cx - s, by),
                    lay.point(cx + s, by),
                    lay.point(cx, by + 4.0),
                );
                draw_triangle(a, b, c, mark);
            }
            if top_line > 0 {
                let by = top + ROLL + 4.0;
                let (a, b, c) = (
                    lay.point(cx - s, by + 4.0),
                    lay.point(cx + s, by + 4.0),
                    lay.point(cx, by),
                );
                draw_triangle(a, b, c, mark);
            }
            // The title at its indent, then the text in its column of 180.
            let mut y = top + ROLL + SPACE_ABOVE;
            self.draw_styled(
                view,
                &Style::inked(4, Ink::Set(font_ramp::flat(47))),
                &heading,
                left + TITLE_LEFT,
                y,
            );
            y += TITLE_HEIGHT;
            for (i, text) in body.iter().enumerate() {
                if let Some(heading) = text.strip_prefix('\u{1}') {
                    self.draw_styled(
                        view,
                        &Style::inked(4, Ink::Set(font_ramp::flat(47))),
                        heading,
                        left + TITLE_LEFT,
                        y,
                    );
                } else {
                    self.draw_styled(
                        view,
                        &Style::inked(1, Ink::Set(font_ramp::flat(55))),
                        text,
                        left + TEXT_LEFT,
                        y,
                    );
                }
                y += rows[top_line + i];
            }
        }
        // The wheel and the arrow keys scroll a long text; any other key or a click closes it.
        let wheel = input.mouse_wheel().1;
        let scrolled =
            input.key_pressed(KeyCode::Down) || input.key_pressed(KeyCode::PageDown) || wheel < 0.0;
        let back =
            input.key_pressed(KeyCode::Up) || input.key_pressed(KeyCode::PageUp) || wheel > 0.0;
        let paged = input.key_pressed(KeyCode::PageDown) || input.key_pressed(KeyCode::PageUp);
        let leave = (input.any_key_pressed() && !(scrolled || back))
            || input.mouse_pressed(MouseButton::Left)
            || input.mouse_pressed(MouseButton::Right);
        let step = if paged { room } else { 3 };
        if scrolled {
            popup.scroll = (popup.scroll + step).min(popup.lines.len().saturating_sub(shown));
        } else if back {
            popup.scroll = popup.scroll.saturating_sub(step);
        }
        (!leave).then_some(popup)
    }
}

/// The height of each line of a help text: a heading takes a title's, the blank line before one
/// the room between entries, and any other line `line`.
fn row_heights(lines: &[String], line: f32) -> Vec<f32> {
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if l.starts_with('\u{1}') {
                TITLE_HEIGHT
            } else if l.is_empty() && lines.get(i + 1).is_some_and(|n| n.starts_with('\u{1}')) {
                ENTRY_GAP
            } else {
                line
            }
        })
        .collect()
}

/// How many of the lines from `first` fit in `room` pixels (at least one).
fn lines_that_fit(rows: &[f32], first: usize, room: f32) -> usize {
    let mut used = 0.0;
    let mut n = 0;
    while first + n < rows.len() && used + rows[first + n] <= room {
        used += rows[first + n];
        n += 1;
    }
    n.max(1)
}

/// The help topic of a random event (`HLP_PLAGUE` and the ones after it, 768 to 775).
fn event_topic(kind: EventKind) -> u16 {
    match kind {
        EventKind::Plague => 768,
        EventKind::PopulationBoom => 769,
        EventKind::ConjunctionSorcery => 770,
        EventKind::ConjunctionChaos => 771,
        EventKind::ConjunctionNature => 772,
        EventKind::BadMoon => 773,
        EventKind::GoodMoon => 774,
        _ => 775,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_scroll_holds_what_fits_between_its_rolls() {
        use super::*;
        let lines: Vec<String> = ["a", "b", "", "\u{1}Head", "c"]
            .into_iter()
            .map(String::from)
            .collect();
        let rows = row_heights(&lines, 8.0);
        assert_eq!(rows, [8.0, 8.0, ENTRY_GAP, TITLE_HEIGHT, 8.0]);
        assert_eq!(lines_that_fit(&rows, 0, 17.0), 2);
        assert_eq!(lines_that_fit(&rows, 0, 1000.0), 5);
        assert_eq!(lines_that_fit(&rows, 3, 5.0), 1, "one line at least");
        assert_eq!(lines_that_fit(&rows, 4, 8.0), 1);
    }

    #[test]
    fn every_event_has_a_topic_from_768_to_775() {
        use super::*;
        let all = [
            EventKind::Plague,
            EventKind::PopulationBoom,
            EventKind::ConjunctionSorcery,
            EventKind::ConjunctionChaos,
            EventKind::ConjunctionNature,
            EventKind::BadMoon,
            EventKind::GoodMoon,
            EventKind::ManaShort,
        ];
        let topics: Vec<u16> = all.into_iter().map(event_topic).collect();
        assert_eq!(topics, (768..=775).collect::<Vec<_>>());
    }

    use super::*;
    use crate::unit_sheet::melee_icon;

    #[test]
    fn each_weapon_quality_opens_its_own_topic() {
        let topics: Vec<_> = (0..4).map(|q| weapon_topic(melee_icon(q))).collect();
        assert_eq!(topics, [Some(384), Some(385), Some(386), Some(762)]);
    }
}
