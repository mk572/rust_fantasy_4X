//! The Chancellor's scroll (the original's `Chancellor_Screen__WIP(0)` in
//! `REPORT.c`): after a turn is processed a scroll unrolls over the map with the news in
//! sections: units disbanded, city growth and deaths, buildings lost, spells that dissipated.
//! Escape, Enter or a click rolls it up; U, D, the arrows or the wheel scroll a long one.
//! The "End of Turn Summary" switch of the Settings page turns it off.

use crate::font::Style;
use crate::input::Input;
use f4x_core::game::{Event, LossCause};
use f4x_core::rules::SpellTarget;
use f4x_core::text::Args;
use f4x_core::world_event::EventKind;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::modal::{keep, kind, Modal};
use crate::preferences::Pref;
use crate::view::{App, HUMAN};

/// One heading and its lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Section {
    pub title: String,
    pub lines: Vec<String>,
}

/// A scroll being shown.
#[derive(Debug, Clone)]
pub(crate) struct Chancellor {
    pub sections: Vec<Section>,
    /// When it began to unroll, in seconds of the clock.
    pub opened: f64,
    /// How far the text is scrolled up, in the original's pixels.
    pub scrolled: f32,
}

impl Modal for Chancellor {
    fn kind(&self) -> &'static str {
        kind::CHANCELLOR
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.chancellor_frame(*self, input))
    }
}

/// The paper's place and size, and the text's start inside it (the original's pixels).
const PAPER: (f32, f32, f32, f32) = (62.0, 29.0, 196.0, 148.0);
/// The window the text is read through when the scroll is fully open.
const WINDOW_HEIGHT: f32 = 127.0;
/// Pixels from a heading to its first line, from a line to the next, after a section.
const HEADING_STEP: f32 = 11.0;
const LINE_STEP: f32 = 7.0;
const SECTION_GAP: f32 = 2.0;
/// Frames of the unrolling rods (`scroll` entry 2) and seconds each lasts.
const ROD_FRAMES: usize = 17;
const FRAME_SECONDS: f64 = 0.035;

impl Chancellor {
    /// Height of all the text.
    fn text_height(&self) -> f32 {
        if self.sections.is_empty() {
            return HEADING_STEP + SECTION_GAP;
        }
        self.sections
            .iter()
            .map(|s| HEADING_STEP + LINE_STEP * s.lines.len() as f32 + SECTION_GAP)
            .sum()
    }

    /// The furthest the text can be scrolled.
    fn max_scroll(&self) -> f32 {
        (self.text_height() - WINDOW_HEIGHT).max(0.0)
    }
}

impl App {
    /// What the player should be told about a turn change, from its events.
    pub(crate) fn collect_report(&self, events: &[Event]) -> Vec<Section> {
        let rules = &self.game.content().rules;
        let mine = |city: hecs::Entity| self.game.economy().city_owner(city) == Some(HUMAN);
        let mut disbanded = Vec::new();
        let mut growth = Vec::new();
        let mut deaths = Vec::new();
        let mut buildings = Vec::new();
        let mut rivals = Vec::new();
        let mut ships = Vec::new();
        // Wizards we have met are known by name; their shortages are news.
        let rival_name = |player: &f4x_core::player::PlayerId| {
            (*player != HUMAN && self.game.have_met(HUMAN, *player))
                .then(|| self.game.wizard(*player).map(|w| w.name()))
                .flatten()
        };
        let say = |key: &str, args: Args| self.text_with(key, &args);
        let (mut unit_spells, mut city_spells, mut global_spells) =
            (Vec::new(), Vec::new(), Vec::new());
        let mut strikes = Vec::new();
        for event in events {
            match event {
                Event::UnitLost {
                    player,
                    unit,
                    cause,
                } if *player == HUMAN => {
                    let args = Args::new().text("unit", &rules.units.get(*unit).name);
                    disbanded.push(say(
                        match cause {
                            LossCause::Food => "chancellor.lost_food",
                            LossCause::Gold => "chancellor.lost_gold",
                            LossCause::Mana => "chancellor.lost_mana",
                        },
                        args,
                    ));
                }
                Event::Shortage { player, what } if rival_name(player).is_some() => {
                    let what = self.text(match what {
                        f4x_core::game::Shortage::Mana => "chancellor.mana",
                        f4x_core::game::Shortage::Gold => "chancellor.gold",
                        f4x_core::game::Shortage::Food => "chancellor.food",
                    });
                    let args = self
                        .wizard_arg(Args::new(), "wizard", *player)
                        .text("what", what);
                    rivals.push(say("chancellor.rival_short", args));
                }
                Event::HeroLeft { player, hero } if rival_name(player).is_some() => {
                    let args = self
                        .wizard_arg(Args::new(), "wizard", *player)
                        .text("hero", &rules.units.get(*hero).name);
                    rivals.push(say("chancellor.rival_hero_left", args));
                }
                Event::ShipLanded { player, units, at } if *player == HUMAN => ships.push(say(
                    "chancellor.ship_landed",
                    Args::new()
                        .count("units", *units)
                        .count("x", at.x)
                        .count("y", at.y),
                )),
                Event::UnitsDrowned { player, units } if *player == HUMAN => {
                    disbanded.push(say("chancellor.drowned", Args::new().count("n", *units)))
                }
                Event::CityGrew { city, figures } if mine(*city) => growth.push(say(
                    "chancellor.grew",
                    Args::new()
                        .text("city", self.city_name(*city))
                        .count("figures", *figures),
                )),
                Event::CityShrank { city, figures } if mine(*city) => deaths.push(say(
                    "chancellor.shrank",
                    Args::new()
                        .text("city", self.city_name(*city))
                        .count("figures", *figures),
                )),
                Event::ChaosRiftStrikes {
                    city,
                    building: Some(b),
                    ..
                } if mine(*city) => buildings.push(say(
                    "chancellor.lost_building",
                    Args::new()
                        .text("city", self.city_name(*city))
                        .text("building", &rules.buildings.get(*b).name),
                )),
                Event::BuildingsCollapsed { city, count } if mine(*city) => buildings.push(say(
                    "chancellor.lost_buildings",
                    Args::new()
                        .text("city", self.city_name(*city))
                        .count("n", *count),
                )),
                Event::WorldSpellStruck {
                    spell,
                    units_lost,
                    buildings_lost,
                    population_lost,
                    ..
                } => {
                    let args = self
                        .spell_arg(Args::new(), *spell)
                        .count("units", *units_lost)
                        .count("citizens", *population_lost)
                        .count("buildings", *buildings_lost);
                    strikes.push(say("log.world_spell_struck", args));
                }
                Event::EnchantmentLapsed { player, spell } if *player == HUMAN => {
                    let def = rules.spells.get(*spell);
                    let list = match def.target {
                        SpellTarget::OwnUnit | SpellTarget::EnemyUnit | SpellTarget::AnyUnit => {
                            &mut unit_spells
                        }
                        SpellTarget::OwnCity | SpellTarget::EnemyCity => &mut city_spells,
                        _ => &mut global_spells,
                    };
                    list.push(def.name.clone());
                }
                _ => {}
            }
        }
        let running = self.running_events();
        [
            ("chancellor.disbanded", disbanded),
            ("chancellor.growth", growth),
            ("chancellor.deaths", deaths),
            ("chancellor.buildings", buildings),
            ("chancellor.strikes", strikes),
            ("chancellor.wizards", rivals),
            ("chancellor.ships", ships),
            ("chancellor.running_events", running),
            ("chancellor.unit_spells", unit_spells),
            ("chancellor.city_spells", city_spells),
            ("chancellor.global_spells", global_spells),
        ]
        .into_iter()
        .filter(|(_, lines)| !lines.is_empty())
        .map(|(title, lines)| Section {
            title: self.text(title),
            lines,
        })
        .collect()
    }

    /// The lines of the events that are going on: the world event, and the plague and the
    /// population boom in the human's cities (`REPORT.c`).
    fn running_events(&self) -> Vec<String> {
        let events = self.game.events();
        let mut lines = Vec::new();
        if let Some(kind) = events.world_event() {
            lines.push(self.text(match kind {
                EventKind::ConjunctionSorcery
                | EventKind::ConjunctionChaos
                | EventKind::ConjunctionNature => "event.conjunction",
                EventKind::BadMoon => "event.bad_moon",
                EventKind::GoodMoon => "event.good_moon",
                _ => "event.mana_short",
            }));
        }
        for lasting in events.ongoing() {
            let key = match lasting.kind {
                EventKind::Plague => "chancellor.plague",
                EventKind::PopulationBoom => "chancellor.boom",
                _ => continue,
            };
            let Some(city) = lasting
                .city
                .and_then(|s| self.game.units().entity_of_serial(s))
            else {
                continue;
            };
            if self.game.economy().city_owner(city) == Some(HUMAN) {
                lines.push(self.text_with(key, &Args::new().text("city", self.city_name(city))));
            }
        }
        lines
    }

    /// Show the scroll for a turn's news, when the switch is on and there is any.
    pub(crate) fn open_chancellor(&mut self, sections: Vec<Section>) {
        if self.prefs.get(Pref::EndOfTurnSummary) && !sections.is_empty() {
            self.modals.enqueue(Chancellor {
                sections,
                opened: get_time(),
                scrolled: 0.0,
            });
        }
    }

    /// The Chancellor from the advisor list (F6): the last turn's news again, or a line saying
    /// there was none.
    pub(crate) fn open_chancellor_advisor(&mut self) {
        let mut sections = self.last_news.clone();
        if let Some(warning) = self.shortfall_forecast() {
            sections.insert(0, warning);
        }
        self.modals.open(Chancellor {
            sections,
            opened: get_time(),
            scrolled: 0.0,
        });
    }

    /// What the next turn change cannot pay for, if anything.
    fn shortfall_forecast(&self) -> Option<Section> {
        let mut lines = Vec::new();
        if let Some(m) = self.game.upkeep_shortfall(HUMAN) {
            let args = Args::new()
                .count("needed", m.owed)
                .count("reserve", m.reserve);
            lines.push(self.text_with("chancellor.forecast_mana", &args));
        }
        if let Some(t) = self.game.treasury_shortfall(HUMAN) {
            let (food, gold) = t.missing(&[]);
            if food > 0 {
                lines.push(
                    self.text_with("chancellor.forecast_food", &Args::new().count("n", food)),
                );
            }
            if gold > 0 {
                lines.push(
                    self.text_with("chancellor.forecast_gold", &Args::new().count("n", gold)),
                );
            }
            lines.push(self.text("chancellor.forecast_advice"));
        }
        (!lines.is_empty()).then_some(Section {
            title: self.text("chancellor.next_turn"),
            lines,
        })
    }

    /// A made-up report, for `--shot ... --scroll`.
    pub(crate) fn open_test_chancellor(&mut self, long: bool) {
        let grew = |city: &str, figures: u32| {
            self.text_with(
                "chancellor.grew",
                &Args::new().text("city", city).count("figures", figures),
            )
        };
        let mut growth: Vec<String> = vec![grew("Waldheim", 6)];
        if long {
            growth.extend((0..14).map(|i| grew(&format!("Test city {i}"), 4)));
        }
        let lost = self.text_with(
            "chancellor.lost_food",
            &Args::new().text("unit", "Spearmen"),
        );
        let sections = vec![
            Section {
                title: self.text("chancellor.disbanded"),
                lines: vec![lost],
            },
            Section {
                title: self.text("chancellor.growth"),
                lines: growth,
            },
        ];
        // Already unrolled, so a screenshot shows the text.
        self.modals.open(Chancellor {
            sections,
            opened: get_time() - 5.0,
            scrolled: 0.0,
        });
    }

    /// Draw the scroll over the screen and handle its keys; it stays while it is returned.
    pub(crate) fn chancellor_frame(
        &mut self,
        mut scroll: Chancellor,
        input: &Input,
    ) -> Option<Chancellor> {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let frame = (((get_time() - scroll.opened) / FRAME_SECONDS) as usize).min(ROD_FRAMES - 1);
        // The paper is uncovered from the middle outwards as the rods part.
        let half = (10.0 + 4.0 * frame as f32).min(PAPER.3 / 2.0);
        let window = (88.0 + 10.0 - half, half * 2.0);
        let (px, py, pw, ph) = PAPER;
        let (top, height) = (window.0.max(py), window.1.min(ph));
        self.clipped((px, top, pw, height), |app| {
            app.draw_pic(&lay, "scroll", 5, 0, px, py);
            let start =
                2.0 + 99.0 - scroll.text_height().max(WINDOW_HEIGHT) / 2.0 - scroll.scrolled;
            if frame + 1 >= ROD_FRAMES {
                let mut y = start;
                if scroll.sections.is_empty() {
                    app.draw_styled(
                        view,
                        &Style::nearest(4, Color::new(0.4, 0.05, 0.02, 1.0)).center(),
                        &app.text("chancellor.nothing"),
                        158.0,
                        y,
                    );
                }
                for s in &scroll.sections {
                    app.draw_styled(
                        view,
                        &Style::nearest(4, Color::new(0.4, 0.05, 0.02, 1.0)).center(),
                        &s.title,
                        165.0,
                        y,
                    );
                    y += HEADING_STEP;
                    for line in &s.lines {
                        app.draw_styled(
                            view,
                            &Style::nearest(1, Color::new(0.2, 0.1, 0.04, 1.0)),
                            line,
                            70.0,
                            y,
                        );
                        y += LINE_STEP;
                    }
                    y += SECTION_GAP;
                }
            }
        });
        self.draw_pic(&lay, "scroll", 2, frame, 11.0, 0.0);
        if scroll.scrolled > 0.0 {
            self.draw_pic(&lay, "scroll", 0, 0, 156.0, 13.0);
        }
        if scroll.scrolled < scroll.max_scroll() {
            self.draw_pic(&lay, "scroll", 1, 0, 156.0, 172.0);
        }

        let max = scroll.max_scroll();
        let down = input.key_down(KeyCode::D) || input.key_down(KeyCode::Down);
        let up = input.key_down(KeyCode::U) || input.key_down(KeyCode::Up);
        let wheel = match input.mouse_wheel().1 {
            0.0 => 0.0,
            w => w.signum(),
        };
        scroll.scrolled = (scroll.scrolled
            + (f32::from(down) - f32::from(up)) * 60.0 * get_frame_time()
            - wheel * 8.0)
            .clamp(0.0, max);
        let leave = input.key_pressed(KeyCode::Escape)
            || input.key_pressed(KeyCode::Enter)
            || input.key_pressed(KeyCode::KpEnter)
            || input.mouse_pressed(MouseButton::Left)
            || input.mouse_pressed(MouseButton::Right);
        (!leave).then_some(scroll)
    }

    /// Run `draw` with everything outside `r` (screen pixels) cut away.
    pub(crate) fn clipped(&mut self, orig: (f32, f32, f32, f32), draw: impl FnOnce(&mut Self)) {
        let lay = Layout::new();
        let r = lay.rect(orig);
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        let dpi = screen_dpi_scale();
        gl.quad_gl.scissor(Some((
            (r.x * dpi) as i32,
            (r.y * dpi) as i32,
            (r.w * dpi) as i32,
            (r.h * dpi) as i32,
        )));
        draw(self);
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        gl.quad_gl.scissor(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_report_scrolls_and_a_short_one_does_not() {
        let section = |n: usize| Section {
            title: "X".to_string(),
            lines: vec!["line".into(); n],
        };
        let short = Chancellor {
            sections: vec![section(3)],
            opened: 0.0,
            scrolled: 0.0,
        };
        assert_eq!(short.max_scroll(), 0.0);
        let long = Chancellor {
            sections: vec![section(20)],
            opened: 0.0,
            scrolled: 0.0,
        };
        assert!(long.max_scroll() > 0.0);
    }

    #[test]
    fn the_report_names_the_running_world_event() -> f4x_core::testdir::TestResult {
        let mut app = App::for_test();
        assert!(app.running_events().is_empty());
        app.game.cheat_event(EventKind::GoodMoon, HUMAN)?;
        assert_eq!(app.running_events(), vec![app.text("event.good_moon")]);
        Ok(())
    }
}
