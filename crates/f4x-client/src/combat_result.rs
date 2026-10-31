//! The combat results scroll (`Combat_Results_Scroll` in ReMoM `Combat.c`): after a battle the
//! human wizard fought in, a paper scroll over the map says how it went (triumphant, defeated,
//! retreated, the opponent fled, a city conquered) and what it cost or brought: fame, gold
//! looted, inhabitants killed and the buildings destroyed. The paper depends on the outcome
//! (`scroll.lbx` 9 for a city that changed hands, 10 for a win, 11 for a loss). A conquered city
//! may be razed: the "Raze City" setting asks before the scroll; with it off, R on the scroll
//! asks. A click, Enter or Esc rolls it up.

use crate::font::Style;
use crate::input::Input;
use f4x_core::game::{Command, Event};
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::modal::{keep, kind, Modal};
use crate::preferences::Pref;
use crate::sell_prompt::RAZE_OR_SPARE;
use crate::view::{App, HUMAN};

/// The left of the paper, and of the paragraph text on it.
const PAPER_X: f32 = 54.0;
const TEXT_X: f32 = 75.0;
/// Where the text height starts: the heading's room.
const FIRST_LINE: f32 = 92.0;
const HEADING: Color = Color::new(0.4, 0.05, 0.02, 1.0);
const LINES: Color = Color::new(0.2, 0.1, 0.04, 1.0);

/// How the battle ended for the human wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Won,
    Lost,
    /// The human's forces retreated.
    Fled,
    /// The opponent retreated.
    OpponentFled,
    /// The turn limit ran out (`Battle result 4`).
    Exhausted,
    /// A city of the human's fell without a battle.
    CityLost,
}

/// What happened to the city besides the battle (`_active_battle_unit` 666 to 668 of the original).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Aftermath {
    Nothing,
    /// Monsters wrecked the city and went their way.
    Rampage,
    /// Monsters razed the city and settled in the ruins.
    Ruins,
    /// An independent city's raiders took the city.
    Raiders,
}

/// A battle begun, for the paper: who attacked, and whether a city stood there.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BattleSite {
    attacker: PlayerId,
    defender: PlayerId,
    city: bool,
}

/// What the scroll says.
#[derive(Debug, Clone)]
pub(crate) struct CombatResults {
    outcome: Outcome,
    paper: usize,
    /// The city the human took and may still raze.
    city: Option<hecs::Entity>,
    city_name: String,
    fame: i32,
    gold: i32,
    killed: u32,
    buildings: Vec<String>,
    razed: bool,
    /// The raze question is up.
    asking: bool,
    aftermath: Aftermath,
}

impl App {
    /// Remember a battle that begins, for the scroll's paper.
    pub(crate) fn note_battle_start(
        &mut self,
        attacker: PlayerId,
        defender: PlayerId,
        site: Position,
    ) {
        self.battle_site = Some(BattleSite {
            attacker,
            defender,
            city: self.game.economy().city_at(site).is_some(),
        });
    }

    /// Read the events of a command for the end of a battle or the fall of a city the human was
    /// part of, and prepare the scroll.
    pub(crate) fn record_combat_results(&mut self, events: &[Event]) {
        let site = self.battle_site;
        let ended = events.iter().find_map(|e| match e {
            Event::BattleEnded { winner, loser } if *winner == HUMAN || *loser == HUMAN => {
                Some((*winner, *loser))
            }
            _ => None,
        });
        let looted = events.iter().find_map(|e| match e {
            Event::CityLooted {
                city,
                gold,
                citizens_lost,
                buildings_lost,
                fame_lost,
                fame_gained,
            } => Some((
                *city,
                *gold,
                *citizens_lost,
                buildings_lost,
                *fame_lost,
                *fame_gained,
            )),
            _ => None,
        });
        let human_took = looted.is_some_and(|l| self.game.economy().city_owner(l.0) == Some(HUMAN));
        let human_lost_city = looted.is_some() && !human_took;
        let fled_side = events.iter().find_map(|e| match e {
            Event::Battle(f4x_core::battle::BattleEvent::Fled { side }) => Some(*side),
            _ => None,
        });
        let exhausted = events
            .iter()
            .any(|e| matches!(e, Event::Battle(f4x_core::battle::BattleEvent::Exhausted)));
        let rampage = events.iter().find_map(|e| match e {
            Event::CityRampaged { name, ruined, .. } => Some((name.clone(), *ruined)),
            _ => None,
        });
        let outcome = match (ended, looted) {
            (Some(_), _) if exhausted => Outcome::Exhausted,
            (Some(_), _) if fled_side.is_some() => {
                let fled_human = site.is_some_and(|s| {
                    let player = match fled_side {
                        Some(f4x_core::battle::Side::Attacker) => s.attacker,
                        _ => s.defender,
                    };
                    player == HUMAN
                });
                if fled_human {
                    Outcome::Fled
                } else {
                    Outcome::OpponentFled
                }
            }
            (Some((winner, _)), _) if winner == HUMAN => Outcome::Won,
            (Some(_), _) => Outcome::Lost,
            (None, Some(_)) if human_lost_city => Outcome::CityLost,
            (None, Some(_)) => Outcome::Won,
            (None, None) => return,
        };
        let mut fame = 0;
        for e in events {
            if let Event::BattleSpoils {
                player,
                fame: won,
                loser_fame,
                ..
            } = e
            {
                fame += if *player == HUMAN { *won } else { *loser_fame };
            }
        }
        let mut results = CombatResults {
            outcome,
            paper: 11,
            city: None,
            city_name: String::new(),
            fame,
            gold: 0,
            killed: 0,
            buildings: Vec::new(),
            razed: false,
            asking: false,
            aftermath: Aftermath::Nothing,
        };
        if let Some((name, ruined)) = rampage {
            results.city_name = name;
            results.aftermath = if ruined {
                Aftermath::Ruins
            } else {
                Aftermath::Rampage
            };
        }
        if let Some((city, gold, killed, lost, fame_lost, fame_gained)) = looted {
            let rules = &self.game.content().rules;
            results.gold = gold;
            results.killed = killed;
            results.buildings = lost
                .iter()
                .map(|&b| rules.buildings.get(b).name.clone())
                .collect();
            results.city_name = self.city_name(city);
            let taker = self.game.economy().city_owner(city);
            if !human_took
                && taker.is_some_and(|p| {
                    p != f4x_core::player::MONSTERS && self.game.is_neutral_player(p)
                })
            {
                results.aftermath = Aftermath::Raiders;
            }
            if human_took {
                results.fame += fame_gained;
                results.city = Some(city);
                results.asking = self.prefs.get(Pref::RazeCity);
            } else {
                results.fame -= fame_lost;
            }
        }
        // The paper: a city that changed hands, a win, a loss.
        let city_battle = site.is_some_and(|s| s.city) || looted.is_some();
        let won = matches!(outcome, Outcome::Won | Outcome::OpponentFled);
        results.paper = if city_battle {
            // `Combat_Results_Scroll`: 9 where the attacker won the city, else 10.
            let human_attacked = site.is_some_and(|s| s.attacker == HUMAN);
            if won == human_attacked {
                9
            } else {
                10
            }
        } else if won {
            10
        } else {
            11
        };
        self.combat_results = Some(results);
    }

    /// The heading and the lines under it, each with its style, alignment, x and y, and the
    /// height of it all (`Combat_Results_Scroll_Text`).
    fn results_text(&self, r: &CombatResults, top: f32) -> (Vec<Line>, f32) {
        let mut out = Vec::new();
        let mut y = FIRST_LINE;
        let heading = match r.outcome {
            Outcome::Won => self.text("battle.triumphant"),
            Outcome::Lost => self.text("battle.defeated"),
            Outcome::Fled => self.text("battle.retreated"),
            Outcome::OpponentFled => self.text("battle.opponent_fled"),
            Outcome::Exhausted => self.text("battle.exhausted"),
            Outcome::CityLost => {
                self.text_with("combat.conquered", &Args::new().text("city", &r.city_name))
            }
        };
        out.push(Line::centered(4, HEADING, top + 25.0, heading));
        let aftermath = match r.aftermath {
            Aftermath::Nothing => None,
            Aftermath::Rampage => Some("combat.rampage"),
            Aftermath::Ruins => Some("combat.ruins"),
            Aftermath::Raiders => Some("combat.raiders"),
        };
        if let Some(key) = aftermath {
            out.push(Line::centered(
                1,
                LINES,
                top + y,
                self.text_with(key, &Args::new().text("city", &r.city_name)),
            ));
            y += 9.0;
        }
        if r.fame != 0 {
            let key = if r.fame < 0 {
                "combat.fame_lost"
            } else {
                "combat.fame_gained"
            };
            out.push(Line::centered(
                1,
                LINES,
                top + y,
                self.text_with(key, &Args::new().count("n", r.fame.abs())),
            ));
            y += 9.0;
        }
        if r.gold > 0 {
            out.push(Line::centered(
                1,
                LINES,
                top + y,
                self.text_with("combat.looted", &Args::new().count("n", r.gold)),
            ));
            y += 9.0;
        }
        if r.razed {
            out.push(Line::centered(1, LINES, top + y, self.text("combat.razed")));
            y += 9.0;
        } else {
            if r.killed > 0 {
                out.push(Line::centered(
                    1,
                    LINES,
                    top + y,
                    self.text_with("combat.killed", &Args::new().count("n", r.killed)),
                ));
                y += 9.0;
            }
            if !r.buildings.is_empty() {
                out.push(Line::centered(
                    4,
                    HEADING,
                    top + y,
                    self.text("combat.destroyed_buildings"),
                ));
                y += 11.0;
                for (i, name) in r.buildings.iter().enumerate() {
                    out.push(Line {
                        style: 1,
                        colour: LINES,
                        x: TEXT_X + (i % 2) as f32 * 90.0,
                        y: top + y,
                        centred: false,
                        text: name.clone(),
                    });
                    if i % 2 == 1 {
                        y += 7.0;
                    }
                }
                if r.buildings.len() % 2 == 1 {
                    y += 7.0;
                }
                y += 2.0;
            }
        }
        (out, y)
    }

    /// For screenshots: a battle won in a city, with fame, loot, dead and ruins.
    pub fn open_test_results(&mut self, kind: &str) {
        let mut r = CombatResults {
            outcome: Outcome::Won,
            paper: 10,
            city: None,
            city_name: "Norport".into(),
            fame: 2,
            gold: 0,
            killed: 0,
            buildings: Vec::new(),
            razed: false,
            asking: false,
            aftermath: Aftermath::Nothing,
        };
        match kind {
            "lost" => {
                r.outcome = Outcome::Lost;
                r.paper = 11;
                r.fame = -1;
            }
            "exhausted" => {
                r.outcome = Outcome::Exhausted;
                r.paper = 11;
                r.fame = 0;
            }
            "fled" => {
                r.outcome = Outcome::Fled;
                r.paper = 11;
                r.fame = 0;
            }
            "rampage" | "ruins" | "raiders" => {
                r.outcome = Outcome::Lost;
                r.paper = 9;
                r.fame = -1;
                r.aftermath = match kind {
                    "rampage" => Aftermath::Rampage,
                    "ruins" => Aftermath::Ruins,
                    _ => Aftermath::Raiders,
                };
                if kind == "raiders" {
                    r.gold = 80;
                    r.killed = 2;
                }
            }
            "city" | "raze" => {
                r.asking = kind == "raze";
                r.paper = 9;
                r.gold = 120;
                r.killed = 3;
                r.buildings = ["Barracks", "Library", "Marketplace"]
                    .map(String::from)
                    .to_vec();
            }
            _ => {}
        }
        self.combat_results = Some(r);
    }

    /// The scroll over the screen until it is rolled up; what is left of it while it is up.
    pub(crate) fn combat_results_frame(
        &mut self,
        mut r: CombatResults,
        input: &Input,
    ) -> Option<CombatResults> {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (_, height) = self.results_text(&r, 0.0);
        let top = ((crate::resolution::DESIGN_H - (22.0 + height)) / 2.0).floor();
        let (lines, _) = self.results_text(&r, top);
        let (paper, bottom) = (r.paper, top + height);
        self.clipped((0.0, 0.0, crate::resolution::DESIGN_W, bottom), |app| {
            app.draw_pic(&lay, "scroll", paper, 0, PAPER_X, top);
        });
        self.draw_pic(&lay, "help", 1, 0, PAPER_X, bottom);
        for l in &lines {
            let align = if l.centred {
                Align::Center
            } else {
                Align::Left
            };
            let x = if l.centred { 160.0 } else { l.x };
            self.draw_styled(
                view,
                &Style::nearest(l.style, l.colour).align(align),
                &l.text,
                x,
                l.y,
            );
        }
        let waited = r.asking;
        if r.asking {
            let text = self.text("combat.raze_question");
            match self.question_box(&lay, &text, &RAZE_OR_SPARE, input) {
                Some(true) => r.asking = false,
                Some(false) => {
                    r.asking = false;
                    if let Some(city) = r.city {
                        let events = self.game.apply(Command::RazeCity {
                            player: HUMAN,
                            city,
                        });
                        for e in &events {
                            if let Event::CityRazed {
                                gold, fame_lost, ..
                            } = e
                            {
                                r.razed = true;
                                r.gold += gold;
                                r.fame -= fame_lost;
                                r.killed = 0;
                                r.buildings.clear();
                                r.city = None;
                            }
                        }
                        self.note(events);
                    }
                }
                None => {}
            }
            return Some(r);
        }
        if !waited && r.city.is_some() && input.key_pressed(KeyCode::R) {
            r.asking = true;
            return Some(r);
        }
        let leave = input.key_pressed(KeyCode::Escape)
            || input.key_pressed(KeyCode::Enter)
            || input.key_pressed(KeyCode::KpEnter)
            || input.mouse_pressed(MouseButton::Left)
            || input.mouse_pressed(MouseButton::Right);
        (!leave).then_some(r)
    }
}

impl Modal for CombatResults {
    fn kind(&self) -> &'static str {
        kind::COMBAT_RESULT
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.combat_results_frame(*self, input))
    }
}

/// A line of text on the scroll.
struct Line {
    style: usize,
    colour: Color,
    x: f32,
    y: f32,
    centred: bool,
    text: String,
}

impl Line {
    fn centered(style: usize, colour: Color, y: f32, text: String) -> Self {
        Self {
            style,
            colour,
            x: 160.0,
            y,
            centred: true,
            text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::battle::{BattleEvent, Side};
    use f4x_core::testdir::TestResult;

    const RIVAL: PlayerId = PlayerId(1);

    fn ended(winner: PlayerId, loser: PlayerId) -> Event {
        Event::BattleEnded { winner, loser }
    }

    /// One event for the battle, in the winner's name: its fame and the loser's loss.
    fn spoils(winner: PlayerId, fame: i32, loser_fame: i32) -> Event {
        Event::BattleSpoils {
            player: winner,
            experience: 0,
            fame,
            loser_fame,
        }
    }

    fn results(app: &mut App, events: &[Event]) -> Result<CombatResults, &'static str> {
        app.combat_results = None;
        app.record_combat_results(events);
        app.combat_results.clone().ok_or("no scroll was prepared")
    }

    #[test]
    fn a_battle_the_human_was_not_in_leaves_no_scroll() {
        let mut app = App::for_test();
        app.record_combat_results(&[ended(RIVAL, PlayerId(2))]);
        app.record_combat_results(&[]);
        assert!(app.combat_results.is_none());
    }

    #[test]
    fn a_win_and_a_loss_read_the_fame_of_the_human_side_and_pick_their_paper() -> TestResult {
        let mut app = App::for_test();
        let won = results(&mut app, &[spoils(HUMAN, 8, -3), ended(HUMAN, RIVAL)])?;
        assert_eq!((won.outcome, won.fame, won.paper), (Outcome::Won, 8, 10));
        let lost = results(&mut app, &[spoils(RIVAL, 8, -3), ended(RIVAL, HUMAN)])?;
        assert_eq!(
            (lost.outcome, lost.fame, lost.paper),
            (Outcome::Lost, -3, 11)
        );
        Ok(())
    }

    #[test]
    fn a_retreat_is_told_from_the_side_that_ran() -> TestResult {
        let mut app = App::for_test();
        let fled = |side| Event::Battle(BattleEvent::Fled { side });
        app.note_battle_start(
            HUMAN,
            RIVAL,
            Position {
                x: 1,
                y: 1,
                ..start_of(&app)?
            },
        );
        let ours = results(&mut app, &[fled(Side::Attacker), ended(RIVAL, HUMAN)])?;
        assert_eq!(ours.outcome, Outcome::Fled);
        let theirs = results(&mut app, &[fled(Side::Defender), ended(HUMAN, RIVAL)])?;
        assert_eq!(theirs.outcome, Outcome::OpponentFled);
        let slow = results(
            &mut app,
            &[Event::Battle(BattleEvent::Exhausted), ended(HUMAN, RIVAL)],
        )?;
        assert_eq!(slow.outcome, Outcome::Exhausted);
        Ok(())
    }

    fn start_of(app: &App) -> Result<Position, &'static str> {
        app.game
            .units()
            .player_units(HUMAN)
            .first()
            .and_then(|&u| app.game.units().unit_position(u))
            .ok_or("the human has no unit")
    }
}
